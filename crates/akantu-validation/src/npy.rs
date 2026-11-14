// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). This file has no
// direct C++ counterpart: it implements the array container of the golden
// fixture format (docs/NUMERICAL_VALIDATION.md).

//! Minimal, dependency-free reader/writer for NumPy `.npy` files.
//!
//! Only the subset used by the fixture format is supported:
//!
//! * format versions 1.0, 2.0 and 3.0 (the writer emits 1.0);
//! * little-endian (or byte-order-free) element types
//!   `<f8`, `<i8`, `<i4`, `<u8`, `<u4`, `|u1`, `|b1`;
//! * C (row-major) order; Fortran-order files are accepted on read and
//!   converted to C order so callers always see row-major data.
//!
//! Values are decoded bit-exactly (`f64::from_le_bytes`), so a fixture
//! written by the C++ probe writer or by `numpy.save` round-trips without
//! any loss.

use std::fs;
use std::io::Write;
use std::path::Path;

use crate::error::{Error, Result};

const MAGIC: &[u8; 6] = b"\x93NUMPY";

/// Element type of an `.npy` array.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Dtype {
    /// IEEE-754 binary64, `<f8`.
    F64,
    /// Signed 64-bit integer, `<i8`.
    I64,
    /// Signed 32-bit integer, `<i4`.
    I32,
    /// Unsigned 64-bit integer, `<u8`.
    U64,
    /// Unsigned 32-bit integer, `<u4`.
    U32,
    /// Unsigned byte, `|u1`.
    U8,
    /// Boolean stored as one byte, `|b1`.
    Bool,
}

impl Dtype {
    /// The NumPy `descr` string written for this type.
    #[must_use]
    pub fn descr(self) -> &'static str {
        match self {
            Dtype::F64 => "<f8",
            Dtype::I64 => "<i8",
            Dtype::I32 => "<i4",
            Dtype::U64 => "<u8",
            Dtype::U32 => "<u4",
            Dtype::U8 => "|u1",
            Dtype::Bool => "|b1",
        }
    }

    /// Size of one element in bytes.
    #[must_use]
    pub fn size(self) -> usize {
        match self {
            Dtype::F64 | Dtype::I64 | Dtype::U64 => 8,
            Dtype::I32 | Dtype::U32 => 4,
            Dtype::U8 | Dtype::Bool => 1,
        }
    }

    /// Parse a NumPy `descr` string. Big-endian types are rejected: the
    /// fixture format is little-endian only.
    pub fn from_descr(descr: &str) -> Option<Self> {
        Some(match descr {
            "<f8" => Dtype::F64,
            "<i8" => Dtype::I64,
            "<i4" => Dtype::I32,
            "<u8" => Dtype::U64,
            "<u4" => Dtype::U32,
            "|u1" | "<u1" => Dtype::U8,
            "|b1" | "<b1" => Dtype::Bool,
            _ => return None,
        })
    }

    /// Manifest name of the type (`"f64"`, `"i64"`, ...).
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Dtype::F64 => "f64",
            Dtype::I64 => "i64",
            Dtype::I32 => "i32",
            Dtype::U64 => "u64",
            Dtype::U32 => "u32",
            Dtype::U8 => "u8",
            Dtype::Bool => "bool",
        }
    }

    /// Inverse of [`Dtype::name`].
    pub fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "f64" => Dtype::F64,
            "i64" => Dtype::I64,
            "i32" => Dtype::I32,
            "u64" => Dtype::U64,
            "u32" => Dtype::U32,
            "u8" => Dtype::U8,
            "bool" => Dtype::Bool,
            _ => return None,
        })
    }
}

/// Decoded element storage of an `.npy` array.
#[derive(Debug, Clone, PartialEq)]
pub enum NpyData {
    /// `<f8`
    F64(Vec<f64>),
    /// `<i8`
    I64(Vec<i64>),
    /// `<i4`
    I32(Vec<i32>),
    /// `<u8`
    U64(Vec<u64>),
    /// `<u4`
    U32(Vec<u32>),
    /// `|u1`
    U8(Vec<u8>),
    /// `|b1`
    Bool(Vec<bool>),
}

impl NpyData {
    /// Element type.
    #[must_use]
    pub fn dtype(&self) -> Dtype {
        match self {
            NpyData::F64(_) => Dtype::F64,
            NpyData::I64(_) => Dtype::I64,
            NpyData::I32(_) => Dtype::I32,
            NpyData::U64(_) => Dtype::U64,
            NpyData::U32(_) => Dtype::U32,
            NpyData::U8(_) => Dtype::U8,
            NpyData::Bool(_) => Dtype::Bool,
        }
    }

    /// Number of elements.
    #[must_use]
    pub fn len(&self) -> usize {
        match self {
            NpyData::F64(v) => v.len(),
            NpyData::I64(v) => v.len(),
            NpyData::I32(v) => v.len(),
            NpyData::U64(v) => v.len(),
            NpyData::U32(v) => v.len(),
            NpyData::U8(v) => v.len(),
            NpyData::Bool(v) => v.len(),
        }
    }

    /// `true` when there are no elements.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// An n-dimensional array read from (or to be written to) an `.npy` file.
///
/// `data` is always in C (row-major) order and `data.len()` equals the
/// product of `shape` (1 for a 0-d scalar).
#[derive(Debug, Clone, PartialEq)]
pub struct NpyArray {
    /// Array dimensions; empty for a scalar.
    pub shape: Vec<usize>,
    /// Row-major element data.
    pub data: NpyData,
}

/// Largest integer magnitude that converts exactly to `f64`.
const F64_EXACT_INT: u64 = 1 << 53;

impl NpyArray {
    /// Build an array, checking that `data` has `product(shape)` elements.
    pub fn new(shape: Vec<usize>, data: NpyData) -> Result<Self> {
        let expected = shape_len(&shape)?;
        if expected != data.len() {
            return Err(Error::shape(format!(
                "shape {shape:?} needs {expected} elements, data has {}",
                data.len()
            )));
        }
        Ok(Self { shape, data })
    }

    /// Element type.
    #[must_use]
    pub fn dtype(&self) -> Dtype {
        self.data.dtype()
    }

    /// Convert to `f64` values. Integers are converted only when they are
    /// exactly representable (|x| <= 2^53); booleans map to 0/1.
    pub fn to_f64(&self) -> Result<Vec<f64>> {
        fn conv_i(x: i64) -> Result<f64> {
            if x.unsigned_abs() <= F64_EXACT_INT {
                #[allow(clippy::cast_precision_loss)] // exactness checked just above
                Ok(x as f64)
            } else {
                Err(Error::shape(format!(
                    "integer {x} is not exactly representable as f64"
                )))
            }
        }
        Ok(match &self.data {
            NpyData::F64(v) => v.clone(),
            NpyData::I64(v) => v.iter().map(|&x| conv_i(x)).collect::<Result<_>>()?,
            NpyData::I32(v) => v.iter().map(|&x| f64::from(x)).collect(),
            NpyData::U64(v) => v
                .iter()
                .map(|&x| {
                    if x <= F64_EXACT_INT {
                        #[allow(clippy::cast_precision_loss)] // exactness checked just above
                        Ok(x as f64)
                    } else {
                        Err(Error::shape(format!(
                            "integer {x} is not exactly representable as f64"
                        )))
                    }
                })
                .collect::<Result<_>>()?,
            NpyData::U32(v) => v.iter().map(|&x| f64::from(x)).collect(),
            NpyData::U8(v) => v.iter().map(|&x| f64::from(x)).collect(),
            NpyData::Bool(v) => v.iter().map(|&x| f64::from(u8::from(x))).collect(),
        })
    }

    /// Convert to `i64` values. Floating-point data is rejected (integer
    /// quantities must be stored with an integer dtype).
    pub fn to_i64(&self) -> Result<Vec<i64>> {
        Ok(match &self.data {
            NpyData::F64(_) => {
                return Err(Error::shape(
                    "cannot read an f64 array as integers; store integer data with an integer dtype",
                ));
            }
            NpyData::I64(v) => v.clone(),
            NpyData::I32(v) => v.iter().map(|&x| i64::from(x)).collect(),
            NpyData::U64(v) => v
                .iter()
                .map(|&x| {
                    i64::try_from(x)
                        .map_err(|_| Error::shape(format!("u64 value {x} does not fit in i64")))
                })
                .collect::<Result<_>>()?,
            NpyData::U32(v) => v.iter().map(|&x| i64::from(x)).collect(),
            NpyData::U8(v) => v.iter().map(|&x| i64::from(x)).collect(),
            NpyData::Bool(v) => v.iter().map(|&x| i64::from(x)).collect(),
        })
    }
}

/// Number of elements for a shape, with overflow detection.
pub(crate) fn shape_len(shape: &[usize]) -> Result<usize> {
    shape.iter().try_fold(1usize, |acc, &d| {
        acc.checked_mul(d)
            .ok_or_else(|| Error::shape(format!("shape {shape:?} overflows usize")))
    })
}

/// Read an `.npy` file.
pub fn read_npy(path: impl AsRef<Path>) -> Result<NpyArray> {
    let path = path.as_ref();
    let bytes = fs::read(path).map_err(|e| Error::io(path, e))?;
    parse_npy(&bytes).map_err(|e| match e {
        Error::Npy { reason, .. } => Error::npy(path, reason),
        other => other,
    })
}

/// Parse `.npy` content held in memory.
pub fn parse_npy(bytes: &[u8]) -> Result<NpyArray> {
    let err = |reason: String| Error::npy("", reason);
    if bytes.len() < 10 || &bytes[..6] != MAGIC {
        return Err(err("missing \\x93NUMPY magic string".into()));
    }
    let (major, minor) = (bytes[6], bytes[7]);
    if minor != 0 {
        return Err(err(format!(
            "unsupported format version {major}.{minor} (supported: 1.0, 2.0, 3.0)"
        )));
    }
    let (header_len, header_start) = match major {
        1 => (
            usize::from(u16::from_le_bytes([bytes[8], bytes[9]])),
            10usize,
        ),
        2 | 3 => {
            if bytes.len() < 12 {
                return Err(err("truncated header length".into()));
            }
            let len = u32::from_le_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]);
            let len = usize::try_from(len).map_err(|_| err("header length overflow".into()))?;
            (len, 12usize)
        }
        v => return Err(err(format!("unsupported format version {v}.{}", bytes[7]))),
    };
    let header_end = header_start
        .checked_add(header_len)
        .filter(|&e| e <= bytes.len())
        .ok_or_else(|| err("truncated header".into()))?;
    let header = std::str::from_utf8(&bytes[header_start..header_end])
        .map_err(|_| err("header is not valid UTF-8".into()))?;
    let header = parse_header(header).map_err(err)?;
    let dtype = Dtype::from_descr(&header.descr).ok_or_else(|| {
        err(format!(
            "unsupported dtype descr '{}' (supported: <f8 <i8 <i4 <u8 <u4 |u1 |b1; big-endian is not allowed)",
            header.descr
        ))
    })?;
    let n = shape_len(&header.shape)?;
    let payload = &bytes[header_end..];
    let expected = n
        .checked_mul(dtype.size())
        .ok_or_else(|| err("payload size overflow".into()))?;
    if payload.len() != expected {
        return Err(err(format!(
            "payload has {} bytes, shape {:?} of {} needs {expected}",
            payload.len(),
            header.shape,
            dtype.descr()
        )));
    }
    let data = decode(dtype, payload).map_err(err)?;
    let data = if header.fortran_order && header.shape.len() > 1 {
        fortran_to_c(&header.shape, data)
    } else {
        data
    };
    Ok(NpyArray {
        shape: header.shape,
        data,
    })
}

fn decode(dtype: Dtype, payload: &[u8]) -> std::result::Result<NpyData, String> {
    fn chunks<const N: usize>(p: &[u8]) -> impl Iterator<Item = [u8; N]> + '_ {
        p.as_chunks::<N>().0.iter().map(|c| {
            let mut a = [0u8; N];
            a.copy_from_slice(c);
            a
        })
    }
    Ok(match dtype {
        Dtype::F64 => NpyData::F64(chunks::<8>(payload).map(f64::from_le_bytes).collect()),
        Dtype::I64 => NpyData::I64(chunks::<8>(payload).map(i64::from_le_bytes).collect()),
        Dtype::I32 => NpyData::I32(chunks::<4>(payload).map(i32::from_le_bytes).collect()),
        Dtype::U64 => NpyData::U64(chunks::<8>(payload).map(u64::from_le_bytes).collect()),
        Dtype::U32 => NpyData::U32(chunks::<4>(payload).map(u32::from_le_bytes).collect()),
        Dtype::U8 => NpyData::U8(payload.to_vec()),
        Dtype::Bool => NpyData::Bool(
            payload
                .iter()
                .map(|&b| match b {
                    0 => Ok(false),
                    1 => Ok(true),
                    other => Err(format!("invalid boolean byte {other}")),
                })
                .collect::<std::result::Result<_, _>>()?,
        ),
    })
}

/// Reorder column-major data into row-major order.
fn fortran_to_c(shape: &[usize], data: NpyData) -> NpyData {
    fn reorder<T: Copy>(shape: &[usize], src: &[T]) -> Vec<T> {
        let n = src.len();
        let mut out = Vec::with_capacity(n);
        let mut idx = vec![0usize; shape.len()];
        for _ in 0..n {
            // Fortran offset of the current C-order multi-index.
            let mut off = 0usize;
            let mut stride = 1usize;
            for (k, &d) in shape.iter().enumerate() {
                off += idx[k] * stride;
                stride *= d;
            }
            out.push(src[off]);
            // Increment the C-order multi-index (last axis fastest).
            for k in (0..shape.len()).rev() {
                idx[k] += 1;
                if idx[k] < shape[k] {
                    break;
                }
                idx[k] = 0;
            }
        }
        out
    }
    match data {
        NpyData::F64(v) => NpyData::F64(reorder(shape, &v)),
        NpyData::I64(v) => NpyData::I64(reorder(shape, &v)),
        NpyData::I32(v) => NpyData::I32(reorder(shape, &v)),
        NpyData::U64(v) => NpyData::U64(reorder(shape, &v)),
        NpyData::U32(v) => NpyData::U32(reorder(shape, &v)),
        NpyData::U8(v) => NpyData::U8(reorder(shape, &v)),
        NpyData::Bool(v) => NpyData::Bool(reorder(shape, &v)),
    }
}

#[derive(Debug)]
struct Header {
    descr: String,
    fortran_order: bool,
    shape: Vec<usize>,
}

/// Parse the Python dict literal of an `.npy` header, e.g.
/// `{'descr': '<f8', 'fortran_order': False, 'shape': (3, 4), }`.
fn parse_header(text: &str) -> std::result::Result<Header, String> {
    let mut p = Parser {
        s: text.as_bytes(),
        i: 0,
    };
    let mut descr = None;
    let mut fortran = None;
    let mut shape = None;
    p.ws();
    p.expect(b'{')?;
    loop {
        p.ws();
        if p.eat(b'}') {
            break;
        }
        let key = p.string()?;
        p.ws();
        p.expect(b':')?;
        p.ws();
        match key.as_str() {
            "descr" if descr.is_none() => descr = Some(p.string()?),
            "fortran_order" if fortran.is_none() => fortran = Some(p.boolean()?),
            "shape" if shape.is_none() => shape = Some(p.tuple()?),
            "descr" | "fortran_order" | "shape" => {
                return Err(format!("duplicate header key '{key}'"));
            }
            other => return Err(format!("unexpected header key '{other}'")),
        }
        p.ws();
        if p.eat(b',') {
            continue;
        }
        p.ws();
        p.expect(b'}')?;
        break;
    }
    p.ws();
    if p.i != p.s.len() {
        return Err("trailing characters after header dict".into());
    }
    Ok(Header {
        descr: descr.ok_or("header lacks 'descr'")?,
        fortran_order: fortran.ok_or("header lacks 'fortran_order'")?,
        shape: shape.ok_or("header lacks 'shape'")?,
    })
}

struct Parser<'a> {
    s: &'a [u8],
    i: usize,
}

impl Parser<'_> {
    fn ws(&mut self) {
        while self.i < self.s.len() && self.s[self.i].is_ascii_whitespace() {
            self.i += 1;
        }
    }
    fn eat(&mut self, c: u8) -> bool {
        if self.s.get(self.i) == Some(&c) {
            self.i += 1;
            true
        } else {
            false
        }
    }
    fn expect(&mut self, c: u8) -> std::result::Result<(), String> {
        if self.eat(c) {
            Ok(())
        } else {
            Err(format!(
                "expected '{}' at header offset {}",
                char::from(c),
                self.i
            ))
        }
    }
    fn string(&mut self) -> std::result::Result<String, String> {
        let q = *self.s.get(self.i).ok_or("unexpected end of header")?;
        if q != b'\'' && q != b'"' {
            return Err(format!(
                "expected a quoted string at header offset {}",
                self.i
            ));
        }
        self.i += 1;
        let start = self.i;
        while self.i < self.s.len() && self.s[self.i] != q {
            self.i += 1;
        }
        if self.i >= self.s.len() {
            return Err("unterminated string in header".into());
        }
        let out = String::from_utf8_lossy(&self.s[start..self.i]).into_owned();
        self.i += 1;
        Ok(out)
    }
    fn boolean(&mut self) -> std::result::Result<bool, String> {
        if self.s[self.i..].starts_with(b"True") {
            self.i += 4;
            Ok(true)
        } else if self.s[self.i..].starts_with(b"False") {
            self.i += 5;
            Ok(false)
        } else {
            Err(format!("expected True/False at header offset {}", self.i))
        }
    }
    fn tuple(&mut self) -> std::result::Result<Vec<usize>, String> {
        self.expect(b'(')?;
        let mut out = Vec::new();
        loop {
            self.ws();
            if self.eat(b')') {
                return Ok(out);
            }
            let start = self.i;
            while self.i < self.s.len() && self.s[self.i].is_ascii_digit() {
                self.i += 1;
            }
            // Tolerate a Python 2 long suffix.
            let digits = std::str::from_utf8(&self.s[start..self.i]).map_err(|e| e.to_string())?;
            if digits.is_empty() {
                return Err(format!("expected a dimension at header offset {start}"));
            }
            let _ = self.eat(b'L');
            out.push(
                digits
                    .parse::<usize>()
                    .map_err(|e| format!("bad dimension '{digits}': {e}"))?,
            );
            self.ws();
            if self.eat(b',') {
                continue;
            }
            self.ws();
            self.expect(b')')?;
            return Ok(out);
        }
    }
}

/// Serialise an array to `.npy` (format 1.0, header padded to 64 bytes).
pub fn encode_npy(array: &NpyArray) -> Result<Vec<u8>> {
    let n = shape_len(&array.shape)?;
    if n != array.data.len() {
        return Err(Error::shape(format!(
            "shape {:?} needs {n} elements, data has {}",
            array.shape,
            array.data.len()
        )));
    }
    let shape_txt = match array.shape.len() {
        0 => "()".to_string(),
        1 => format!("({},)", array.shape[0]),
        _ => format!(
            "({})",
            array
                .shape
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(", ")
        ),
    };
    let mut header = format!(
        "{{'descr': '{}', 'fortran_order': False, 'shape': {shape_txt}, }}",
        array.dtype().descr()
    );
    // magic(6) + version(2) + len(2) + header + '\n' must be a multiple of 64.
    let unpadded = 10 + header.len() + 1;
    let pad = (64 - unpadded % 64) % 64;
    header.extend(std::iter::repeat_n(' ', pad));
    header.push('\n');
    let hlen = u16::try_from(header.len())
        .map_err(|_| Error::npy("", "header too long for format 1.0"))?;
    let mut out = Vec::with_capacity(10 + header.len() + n * array.dtype().size());
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&[1, 0]);
    out.extend_from_slice(&hlen.to_le_bytes());
    out.extend_from_slice(header.as_bytes());
    match &array.data {
        NpyData::F64(v) => v
            .iter()
            .for_each(|x| out.extend_from_slice(&x.to_le_bytes())),
        NpyData::I64(v) => v
            .iter()
            .for_each(|x| out.extend_from_slice(&x.to_le_bytes())),
        NpyData::I32(v) => v
            .iter()
            .for_each(|x| out.extend_from_slice(&x.to_le_bytes())),
        NpyData::U64(v) => v
            .iter()
            .for_each(|x| out.extend_from_slice(&x.to_le_bytes())),
        NpyData::U32(v) => v
            .iter()
            .for_each(|x| out.extend_from_slice(&x.to_le_bytes())),
        NpyData::U8(v) => out.extend_from_slice(v),
        NpyData::Bool(v) => out.extend(v.iter().map(|&b| u8::from(b))),
    }
    Ok(out)
}

/// Write an array to an `.npy` file, creating parent directories.
pub fn write_npy(path: impl AsRef<Path>, array: &NpyArray) -> Result<()> {
    let path = path.as_ref();
    let bytes = encode_npy(array)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| Error::io(parent, e))?;
    }
    let mut f = fs::File::create(path).map_err(|e| Error::io(path, e))?;
    f.write_all(&bytes).map_err(|e| Error::io(path, e))?;
    Ok(())
}

/// Convenience: write an `f64` array.
pub fn write_npy_f64(path: impl AsRef<Path>, shape: &[usize], data: &[f64]) -> Result<()> {
    write_npy(
        path,
        &NpyArray::new(shape.to_vec(), NpyData::F64(data.to_vec()))?,
    )
}

/// Convenience: write an `i64` array.
pub fn write_npy_i64(path: impl AsRef<Path>, shape: &[usize], data: &[i64]) -> Result<()> {
    write_npy(
        path,
        &NpyArray::new(shape.to_vec(), NpyData::I64(data.to_vec()))?,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_all_dtypes() {
        let cases = vec![
            NpyArray::new(
                vec![2, 3],
                NpyData::F64(vec![
                    0.0,
                    -0.0,
                    1.5,
                    f64::NAN,
                    f64::INFINITY,
                    f64::MIN_POSITIVE,
                ]),
            )
            .unwrap(),
            NpyArray::new(vec![3], NpyData::I64(vec![i64::MIN, 0, i64::MAX])).unwrap(),
            NpyArray::new(vec![1, 2], NpyData::I32(vec![-7, 7])).unwrap(),
            NpyArray::new(vec![2], NpyData::U64(vec![0, u64::MAX])).unwrap(),
            NpyArray::new(vec![2], NpyData::U32(vec![1, u32::MAX])).unwrap(),
            NpyArray::new(vec![2], NpyData::U8(vec![0, 255])).unwrap(),
            NpyArray::new(vec![2, 1], NpyData::Bool(vec![true, false])).unwrap(),
            NpyArray::new(vec![], NpyData::F64(vec![42.0])).unwrap(),
            NpyArray::new(vec![0, 3], NpyData::F64(vec![])).unwrap(),
        ];
        for a in cases {
            let bytes = encode_npy(&a).unwrap();
            assert_eq!(bytes.len() % 64, (a.data.len() * a.dtype().size()) % 64);
            let header_len = usize::from(u16::from_le_bytes([bytes[8], bytes[9]]));
            assert_eq!((10 + header_len) % 64, 0, "header must be 64-byte aligned");
            let b = parse_npy(&bytes).unwrap();
            assert_eq!(a.shape, b.shape);
            match (&a.data, &b.data) {
                (NpyData::F64(x), NpyData::F64(y)) => {
                    let xb: Vec<u64> = x.iter().map(|v| v.to_bits()).collect();
                    let yb: Vec<u64> = y.iter().map(|v| v.to_bits()).collect();
                    assert_eq!(xb, yb, "f64 must round-trip bit-exactly");
                }
                (x, y) => assert_eq!(x, y),
            }
        }
    }

    #[test]
    fn parses_numpy_written_header() {
        // Byte-for-byte what numpy 1.26 writes for np.arange(6.).reshape(2,3).
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"\x93NUMPY\x01\x00");
        let mut header = "{'descr': '<f8', 'fortran_order': False, 'shape': (2, 3), }".to_string();
        while !(10 + header.len() + 1).is_multiple_of(64) {
            header.push(' ');
        }
        header.push('\n');
        bytes.extend_from_slice(&u16::try_from(header.len()).unwrap().to_le_bytes());
        bytes.extend_from_slice(header.as_bytes());
        for i in 0..6 {
            bytes.extend_from_slice(&f64::from(i).to_le_bytes());
        }
        let a = parse_npy(&bytes).unwrap();
        assert_eq!(a.shape, vec![2, 3]);
        assert_eq!(a.to_f64().unwrap(), vec![0.0, 1.0, 2.0, 3.0, 4.0, 5.0]);
    }

    #[test]
    fn fortran_order_is_converted() {
        // Column-major storage of [[0,1,2],[3,4,5]] is 0,3,1,4,2,5.
        let header = "{'descr': '<i8', 'fortran_order': True, 'shape': (2, 3), }\n";
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"\x93NUMPY\x01\x00");
        bytes.extend_from_slice(&u16::try_from(header.len()).unwrap().to_le_bytes());
        bytes.extend_from_slice(header.as_bytes());
        for v in [0i64, 3, 1, 4, 2, 5] {
            bytes.extend_from_slice(&v.to_le_bytes());
        }
        let a = parse_npy(&bytes).unwrap();
        assert_eq!(a.to_i64().unwrap(), vec![0, 1, 2, 3, 4, 5]);
    }

    #[test]
    fn rejects_big_endian_and_bad_payload() {
        let header = "{'descr': '>f8', 'fortran_order': False, 'shape': (1,), }\n";
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"\x93NUMPY\x01\x00");
        bytes.extend_from_slice(&u16::try_from(header.len()).unwrap().to_le_bytes());
        bytes.extend_from_slice(header.as_bytes());
        bytes.extend_from_slice(&1.0f64.to_be_bytes());
        let e = parse_npy(&bytes).unwrap_err();
        assert!(e.to_string().contains("big-endian"), "{e}");

        let mut ok =
            encode_npy(&NpyArray::new(vec![2], NpyData::F64(vec![1.0, 2.0])).unwrap()).unwrap();
        ok.pop();
        let e = parse_npy(&ok).unwrap_err();
        assert!(e.to_string().contains("payload"), "{e}");
        assert!(parse_npy(b"not an npy file").is_err());
    }

    fn with_header(version: [u8; 2], header: &str, payload: &[u8]) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"\x93NUMPY");
        bytes.extend_from_slice(&version);
        if version[0] == 1 {
            bytes.extend_from_slice(&u16::try_from(header.len()).unwrap().to_le_bytes());
        } else {
            bytes.extend_from_slice(&u32::try_from(header.len()).unwrap().to_le_bytes());
        }
        bytes.extend_from_slice(header.as_bytes());
        bytes.extend_from_slice(payload);
        bytes
    }

    /// Parser hardening: duplicate header keys, unsupported
    /// version pairs and truncated payloads are errors, never silent.
    #[test]
    fn malformed_headers_are_rejected() {
        let one = 1.0f64.to_le_bytes();
        let dup = "{'descr': '<f8', 'fortran_order': False, 'shape': (1,), 'descr': '<i8', }\n";
        let e = parse_npy(&with_header([1, 0], dup, &one)).unwrap_err();
        assert!(e.to_string().contains("duplicate header key"), "{e}");
        let dup_shape =
            "{'descr': '<f8', 'fortran_order': False, 'shape': (1,), 'shape': (2,), }\n";
        assert!(parse_npy(&with_header([1, 0], dup_shape, &one)).is_err());
        let ok = "{'descr': '<f8', 'fortran_order': False, 'shape': (1,), }\n";
        assert!(parse_npy(&with_header([1, 0], ok, &one)).is_ok());
        assert!(parse_npy(&with_header([2, 0], ok, &one)).is_ok());
        assert!(parse_npy(&with_header([3, 0], ok, &one)).is_ok());
        for v in [[1u8, 1u8], [2, 1], [4, 0], [0, 0], [3, 2]] {
            let e = parse_npy(&with_header(v, ok, &one)).unwrap_err();
            assert!(
                e.to_string().contains("unsupported format version"),
                "{v:?}: {e}"
            );
        }
        // Truncated payloads and headers.
        assert!(parse_npy(&with_header([1, 0], ok, &one[..7])).is_err());
        assert!(parse_npy(&with_header([1, 0], ok, &[])).is_err());
        let mut truncated_header = with_header([1, 0], ok, &one);
        truncated_header.truncate(20);
        assert!(parse_npy(&truncated_header).is_err());
        let mut truncated_len = with_header([2, 0], ok, &one);
        truncated_len.truncate(11);
        assert!(parse_npy(&truncated_len).is_err());
        // Missing keys.
        let missing = "{'descr': '<f8', 'shape': (1,), }\n";
        assert!(parse_npy(&with_header([1, 0], missing, &one)).is_err());
        // Oversized payload is as wrong as a short one.
        let mut long = one.to_vec();
        long.extend_from_slice(&one);
        assert!(parse_npy(&with_header([1, 0], ok, &long)).is_err());
    }

    /// Exact comparisons distinguish the smallest subnormals from zero
    /// after a round trip through the writer.
    #[test]
    fn subnormals_survive_the_round_trip_and_differ_from_zero() {
        let tiny = f64::from_bits(1);
        let a = NpyArray::new(vec![3], NpyData::F64(vec![tiny, -tiny, 0.0])).unwrap();
        let back = parse_npy(&encode_npy(&a).unwrap()).unwrap();
        let v = back.to_f64().unwrap();
        assert_eq!(v[0].to_bits(), 1);
        assert_eq!(v[1].to_bits(), (-tiny).to_bits());
        assert_ne!(v[0], 0.0);
        assert_ne!(v[1], 0.0);
        assert_eq!(crate::compare::ulp_distance(v[0], 0.0), 1);
    }

    #[test]
    fn conversions() {
        let a = NpyArray::new(vec![2], NpyData::I64(vec![1 << 53, -(1 << 53)])).unwrap();
        assert_eq!(
            a.to_f64().unwrap(),
            vec![9_007_199_254_740_992.0, -9_007_199_254_740_992.0]
        );
        let b = NpyArray::new(vec![1], NpyData::I64(vec![(1 << 53) + 1])).unwrap();
        assert!(b.to_f64().is_err());
        let c = NpyArray::new(vec![1], NpyData::F64(vec![1.0])).unwrap();
        assert!(c.to_i64().is_err());
        let d = NpyArray::new(vec![1], NpyData::U64(vec![u64::MAX])).unwrap();
        assert!(d.to_i64().is_err());
        assert!(NpyArray::new(vec![2, 2], NpyData::F64(vec![1.0])).is_err());
    }
}
