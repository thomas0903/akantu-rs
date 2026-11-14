// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Test helpers of the
// example integration tests (tests/examples_*.rs): scratch directories,
// output trees, a reader of the VTU files written by the ParaView dumper
// (IOHelper layout: inline base64 with a 4-byte self-counting length header,
// or ASCII), and the oracle fixtures of the examples (fixtures/examples/).

#![allow(dead_code)] // each test binary uses a subset of the helpers

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub mod oracle;
pub mod train;

/// `fixtures/examples`.
pub fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/examples")
}

/// A frozen Gmsh mesh of an upstream example (`fixtures/meshes/gmsh-4.13.1/
/// examples/c++/<rel>`).
pub fn frozen_example_mesh(rel: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/meshes/gmsh-4.13.1/examples/c++")
        .join(rel)
}

/// A fresh scratch directory for one test (below `$AKANTU_EXAMPLES_EXPORT`
/// when set and not empty, to keep the outputs; an empty value counts as
/// unset).
pub fn scratch(name: &str) -> PathBuf {
    let d = match std::env::var_os("AKANTU_EXAMPLES_EXPORT") {
        Some(root) if !root.is_empty() => PathBuf::from(root).join(name),
        _ => std::env::temp_dir().join(format!("akantu-examples-{name}-{}", std::process::id())),
    };
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// Every file below `dir`, relative, sorted.
pub fn files_below(dir: &Path) -> Vec<String> {
    fn walk(base: &Path, d: &Path, out: &mut Vec<String>) {
        for e in std::fs::read_dir(d).unwrap() {
            let p = e.unwrap().path();
            if p.is_dir() {
                walk(base, &p, out);
            } else {
                out.push(p.strip_prefix(base).unwrap().to_string_lossy().into_owned());
            }
        }
    }
    let mut v = Vec::new();
    if dir.exists() {
        walk(dir, dir, &mut v);
    }
    v.sort();
    v
}

/// Byte comparison with a readable first difference.
pub fn assert_same_file(got: &Path, want: &Path) {
    let g = std::fs::read(got).unwrap_or_else(|e| panic!("{got:?}: {e}"));
    let w = std::fs::read(want).unwrap_or_else(|e| panic!("{want:?}: {e}"));
    if g != w {
        let i = g
            .iter()
            .zip(&w)
            .position(|(a, b)| a != b)
            .unwrap_or(g.len().min(w.len()));
        let ctx = |b: &[u8]| {
            String::from_utf8_lossy(&b[i.saturating_sub(80)..(i + 80).min(b.len())]).into_owned()
        };
        panic!(
            "{got:?} differs from {want:?} at byte {i} (len {} vs {}):\n--- got\n{}\n--- want\n{}",
            g.len(),
            w.len(),
            ctx(&g),
            ctx(&w)
        );
    }
}

/// SHA-256 of a file (lowercase hex).
pub fn sha256(path: &Path) -> String {
    akantu_validation::checksum::sha256_file(path).unwrap_or_else(|e| panic!("{path:?}: {e}"))
}

/// Values of a VTU data array.
#[derive(Clone, Debug, PartialEq)]
pub enum Values {
    /// `Float64`.
    F64(Vec<f64>),
    /// Every integer type (`Int32`, `UInt32`, `UInt8`, `Int64`).
    Int(Vec<i64>),
}

impl Values {
    /// Real values (panics on integers).
    pub fn reals(&self) -> &[f64] {
        match self {
            Self::F64(v) => v,
            Self::Int(_) => panic!("integer array read as reals"),
        }
    }

    /// Integer values (panics on reals).
    pub fn ints(&self) -> &[i64] {
        match self {
            Self::Int(v) => v,
            Self::F64(_) => panic!("real array read as integers"),
        }
    }

    /// Number of values.
    pub fn len(&self) -> usize {
        match self {
            Self::F64(v) => v.len(),
            Self::Int(v) => v.len(),
        }
    }

    /// `true` without values.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// One `<DataArray>` of a VTU file.
#[derive(Clone, Debug)]
pub struct DataArray {
    /// `Name` (`positions` for the points, `connectivity`, `offsets`,
    /// `types` for the cells).
    pub name: String,
    /// `type`.
    pub ty: String,
    /// `NumberOfComponents` (1 if absent).
    pub nb_component: usize,
    /// Decoded values.
    pub values: Values,
}

/// A VTU file of the ParaView dumper.
#[derive(Clone, Debug)]
pub struct Vtu {
    /// `NumberOfPoints`.
    pub nb_points: usize,
    /// `NumberOfCells`.
    pub nb_cells: usize,
    /// Arrays by name.
    pub arrays: BTreeMap<String, DataArray>,
}

impl Vtu {
    /// Array `name`.
    pub fn array(&self, name: &str) -> &DataArray {
        self.arrays
            .get(name)
            .unwrap_or_else(|| panic!("no array {name} (have {:?})", self.arrays.keys()))
    }
}

fn attr<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
    let key = format!("{name}=");
    let i = tag.find(&key)? + key.len();
    let rest = tag[i..].trim_start();
    let rest = rest.strip_prefix('"')?;
    let j = rest.find('"')?;
    Some(&rest[..j])
}

/// Standard base64 (RFC 4648 alphabet, `=` padding) with whitespace
/// ignored.
pub fn base64_decode(text: &str) -> Result<Vec<u8>, String> {
    let value = |c: u8| -> Result<u32, String> {
        Ok(u32::from(match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => return Err(format!("invalid character {:?}", char::from(c))),
        }))
    };
    let compact: Vec<u8> = text.bytes().filter(|c| !c.is_ascii_whitespace()).collect();
    if !compact.len().is_multiple_of(4) {
        return Err(format!("length {} is not a multiple of 4", compact.len()));
    }
    let mut out = Vec::with_capacity(compact.len() / 4 * 3);
    for quad in compact.as_chunks::<4>().0.iter() {
        let pad = quad.iter().rev().take_while(|&&c| c == b'=').count();
        if pad > 2 {
            return Err("too much padding".into());
        }
        let mut n = 0u32;
        for &c in &quad[..4 - pad] {
            n = (n << 6) | value(c)?;
        }
        n <<= 6 * pad;
        let bytes = n.to_be_bytes();
        out.extend_from_slice(&bytes[1..4 - pad]);
    }
    Ok(out)
}

fn decode(ty: &str, format: &str, text: &str) -> Values {
    let width = match ty {
        "Float64" | "Int64" | "UInt64" => 8,
        "Int32" | "UInt32" | "Float32" => 4,
        "UInt8" | "Int8" => 1,
        _ => panic!("unknown VTU type {ty}"),
    };
    if format == "ascii" {
        let tokens = text.split_whitespace();
        return if ty == "Float64" || ty == "Float32" {
            Values::F64(tokens.map(|t| t.parse().unwrap()).collect())
        } else {
            Values::Int(tokens.map(|t| t.parse().unwrap()).collect())
        };
    }
    let bytes = base64_decode(text).unwrap_or_else(|e| panic!("bad base64 in a {ty} array: {e}"));
    assert!(bytes.len() >= 4, "base64 payload without header");
    let header = u32::from_le_bytes(bytes[0..4].try_into().unwrap()) as usize;
    let data = &bytes[4..];
    assert_eq!(header, data.len() + 4, "self-counting length header");
    assert_eq!(
        data.len() % width,
        0,
        "{ty} payload of {} bytes",
        data.len()
    );
    let chunks = data.chunks_exact(width);
    match ty {
        "Float64" => Values::F64(
            chunks
                .map(|c| f64::from_le_bytes(c.try_into().unwrap()))
                .collect(),
        ),
        "Float32" => Values::F64(
            chunks
                .map(|c| f64::from(f32::from_le_bytes(c.try_into().unwrap())))
                .collect(),
        ),
        "Int64" => Values::Int(
            chunks
                .map(|c| i64::from_le_bytes(c.try_into().unwrap()))
                .collect(),
        ),
        "UInt64" => Values::Int(
            chunks
                .map(|c| i64::try_from(u64::from_le_bytes(c.try_into().unwrap())).unwrap())
                .collect(),
        ),
        "Int32" => Values::Int(
            chunks
                .map(|c| i64::from(i32::from_le_bytes(c.try_into().unwrap())))
                .collect(),
        ),
        "UInt32" => Values::Int(
            chunks
                .map(|c| i64::from(u32::from_le_bytes(c.try_into().unwrap())))
                .collect(),
        ),
        "UInt8" => Values::Int(chunks.map(|c| i64::from(c[0])).collect()),
        "Int8" => Values::Int(chunks.map(|c| i64::from(c[0] as i8)).collect()),
        _ => unreachable!(),
    }
}

/// Read a VTU file written by the ParaView dumper.
pub fn read_vtu(path: &Path) -> Vtu {
    let doc = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{path:?}: {e}"));
    let piece = doc.find("<Piece").expect("no <Piece>");
    let piece_tag = &doc[piece..piece + doc[piece..].find('>').unwrap()];
    let nb_points = attr(piece_tag, "NumberOfPoints").unwrap().parse().unwrap();
    let nb_cells = attr(piece_tag, "NumberOfCells").unwrap().parse().unwrap();
    let mut arrays = BTreeMap::new();
    let mut rest = &doc[piece..];
    while let Some(i) = rest.find("<DataArray") {
        rest = &rest[i..];
        let end_tag = rest.find('>').unwrap();
        let tag = &rest[..end_tag];
        let body_end = rest.find("</DataArray>").unwrap();
        let body = &rest[end_tag + 1..body_end];
        let name = attr(tag, "Name").unwrap_or("").to_owned();
        let ty = attr(tag, "type").unwrap().to_owned();
        let format = attr(tag, "format").unwrap_or("ascii");
        let nb_component = attr(tag, "NumberOfComponents").map_or(1, |c| c.parse().unwrap());
        let values = decode(&ty, format, body);
        arrays.insert(
            name.clone(),
            DataArray {
                name,
                ty,
                nb_component,
                values,
            },
        );
        rest = &rest[body_end..];
    }
    Vtu {
        nb_points,
        nb_cells,
        arrays,
    }
}

/// `Source` attributes of the pieces of a `.pvtu` file.
pub fn pvtu_pieces(path: &Path) -> Vec<String> {
    let doc = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{path:?}: {e}"));
    doc.match_indices("<Piece")
        .map(|(i, _)| {
            let tag = &doc[i..i + doc[i..].find('>').unwrap()];
            attr(tag, "Source").unwrap().to_owned()
        })
        .collect()
}

/// Largest `|a − b|` over the largest `|b|`, for printed diagnostics only
/// (never a pass criterion: use [`assert_close_rel`]). NaN-aware:
/// a NaN or an infinity on either side, or a non-finite
/// scale, gives `f64::INFINITY` (`f64::max` alone would skip NaNs).
pub fn rel_diff(a: &[f64], b: &[f64]) -> f64 {
    assert_eq!(a.len(), b.len(), "lengths differ");
    if a.iter().chain(b).any(|x| !x.is_finite()) {
        return f64::INFINITY;
    }
    let scale = b.iter().fold(0.0_f64, |m, x| m.max(x.abs()));
    let diff = a
        .iter()
        .zip(b)
        .fold(0.0_f64, |m, (x, y)| m.max((x - y).abs()));
    diff / scale.max(f64::MIN_POSITIVE)
}

/// Tolerance rule `|a_i − e_i| ≤ rel · max_j |e_j|` for every entry,
/// evaluated by the project's comparator
/// (`akantu_validation::compare::compare_f64`, scale `MaxAbs`) with NaNs
/// **forbidden**: a NaN on either side, matched or not, and any infinity
/// mismatch fail the rule (D-046). Panics with the
/// comparator's summary; returns the measured [`rel_diff`].
pub fn assert_close_rel(
    what: &str,
    actual: &[f64],
    expected: &[f64],
    rel: f64,
    justification: &str,
) -> f64 {
    use akantu_validation::array::Field;
    use akantu_validation::compare::{CompareOptions, compare_f64};
    use akantu_validation::tolerance::{NanPolicy, ResolvedTolerance, Scale, ToleranceSpec};
    assert_eq!(actual.len(), expected.len(), "{what}: lengths differ");
    let field = Field::new(
        vec![expected.len()],
        expected.to_vec(),
        vec!["entry".to_owned()],
    )
    .unwrap();
    let tol = ResolvedTolerance {
        spec: ToleranceSpec::abs_rel(0.0, rel, justification)
            .with_scale(Scale::MaxAbs)
            .with_nan(NanPolicy::Forbid),
        origin: "code (crates/akantu/tests/examples_common)".to_owned(),
    };
    let report = compare_f64(
        "examples",
        what,
        &field,
        &[actual.len()],
        actual,
        &tol,
        &CompareOptions::default(),
    );
    assert!(report.passed(), "{what}: {}", report.one_line());
    rel_diff(actual, expected)
}
