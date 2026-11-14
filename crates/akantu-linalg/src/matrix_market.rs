// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/solver/sparse_matrix_aij.cc
// (saveProfile, saveMatrix), src/solver/solver_vector_default_tmpl.hh
// (saveVector), src/solver/solver_vector_distributed.cc (saveVector).
// The floating-point text reproduces `std::ostream << double` in the
// default float field with a chosen precision (upstream:
// `std::numeric_limits<double>::digits10`; port default 17, UD-061),
// written from the C standard's description of `printf("%.*g")`
// (C11 §7.21.6.1) and checked black-box against the oracle (D-030); no C or
// C++ library code was read or translated.

//! Matrix Market text in upstream's layout (SPM-007), with the value
//! precision chosen by [`MatrixMarketOptions`] (UD-061, category B):
//!
//! * **default: 17 significant digits** (`max_digits10`), so that a
//!   write → read round trip is bit-identical for every finite binary64
//!   value (including subnormals and `DBL_MAX`);
//! * [`MatrixMarketOptions::upstream_precision`]: upstream's `digits10` = 15
//!   digits, **byte-identical** with the files the C++ oracle writes (lossy:
//!   relative read-back change up to 5e-15 for normal values, absolute
//!   quantisation of subnormals, `DBL_MAX` reads back as `inf`). Used for
//!   byte comparisons with upstream fixtures.
//!
//! Layout (normative, upstream):
//!
//! * `saveMatrix`: `%%MatrixMarket matrix coordinate real symmetric|general`,
//!   then `n n nnz`, then one `i j a` line per stored entry in storage order
//!   (1-based coordinates as first inserted, so a symmetric matrix may list
//!   entries of either triangle), `'\n'` line ends, values printed in the
//!   default float field (`%.{p}g`);
//! * `saveProfile`: `%%MatrixMarket matrix coordinate pattern
//!   symmetric|general`, `n n nnz`, then `i j 1` per entry (upstream writes a
//!   value column even in a pattern file), lines ended by `std::endl`;
//! * `saveVector`: `%%MatrixMarket matrix coordinate real general`,
//!   `N 1 count`, then `g+1 1 v` for every owned local equation in local
//!   order.
//!
//! File names: `.mtx` is appended when the name has no extension; with more
//! than one process `_rank-<r>.mtx` replaces the extension
//! (`std::filesystem::path` semantics, reproduced with [`Path`]).

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use akantu_core::{Error, Real, Result};

/// Precision upstream sets on Matrix Market streams
/// (`std::numeric_limits<Real>::digits10`; lossy, UD-061).
pub const UPSTREAM_MATRIX_MARKET_PRECISION: usize = 15;

/// Round-trip precision of binary64 (`std::numeric_limits<Real>::
/// max_digits10`): the port's default (UD-061).
pub const ROUND_TRIP_PRECISION: usize = 17;

/// Output policy of Matrix Market dumps (UD-061, category B: round-trip
/// precision by default, upstream precision explicitly selected).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MatrixMarketOptions {
    /// Significant digits of values (`%.{p}g`; default 17).
    pub significant_digits: usize,
}

impl Default for MatrixMarketOptions {
    fn default() -> Self {
        Self {
            significant_digits: ROUND_TRIP_PRECISION,
        }
    }
}

impl MatrixMarketOptions {
    /// Upstream's precision (15 digits), for byte comparisons with files
    /// written by the C++ oracle (`saveMatrix`/`saveVector`).
    #[must_use]
    pub fn upstream_precision() -> Self {
        Self {
            significant_digits: UPSTREAM_MATRIX_MARKET_PRECISION,
        }
    }

    /// Text of one value under this policy.
    #[must_use]
    pub fn format(&self, x: Real) -> String {
        format_g(x, self.significant_digits)
    }
}

/// Text of `x` as printed by `std::ostream << x` with `precision(p)` and the
/// default float field, i.e. C `printf("%.{p}g", x)` (precision 0 means 1;
/// glibc spellings `inf`, `-inf`, `nan`, `-nan`).
///
/// Rust's exact decimal formatting (`{:.Ne}`) gives correctly rounded digits
/// with ties to even on the exact binary value, as glibc does in the default
/// rounding mode; only the layout is adapted here.
#[must_use]
pub fn format_g(x: f64, precision: usize) -> String {
    if x.is_nan() {
        return if x.is_sign_negative() { "-nan" } else { "nan" }.to_owned();
    }
    if x.is_infinite() {
        return if x < 0.0 { "-inf" } else { "inf" }.to_owned();
    }
    if x == 0.0 {
        return if x.is_sign_negative() { "-0" } else { "0" }.to_owned();
    }
    let p = precision.max(1);
    // X: decimal exponent of the %e conversion with precision P - 1.
    let e_text = format!("{:.*e}", p - 1, x);
    let (mantissa, exp) = match e_text.split_once('e') {
        Some((m, e)) => (m.to_owned(), e.parse::<i64>().unwrap_or(0)),
        None => (e_text.clone(), 0),
    };
    let p_i = i64::try_from(p).unwrap_or(i64::MAX);
    if exp < p_i && exp >= -4 {
        let decimals = usize::try_from(p_i - 1 - exp).unwrap_or(0);
        strip_fraction_zeros(format!("{x:.decimals$}"))
    } else {
        let mut m = strip_fraction_zeros(mantissa);
        let sign = if exp < 0 { '-' } else { '+' };
        let _ = write!(m, "e{sign}{:02}", exp.unsigned_abs());
        m
    }
}

fn strip_fraction_zeros(mut s: String) -> String {
    if s.contains('.') {
        while s.ends_with('0') {
            s.pop();
        }
        if s.ends_with('.') {
            s.pop();
        }
    }
    s
}

/// A Matrix Market value with the default policy (17 significant digits,
/// round-trip exact; UD-061).
#[must_use]
pub fn format_value(x: Real) -> String {
    MatrixMarketOptions::default().format(x)
}

/// `std::ostream << x` as upstream's Matrix Market streams print it
/// (precision 15, byte-identical with the oracle; lossy).
#[must_use]
pub fn format_value_upstream(x: Real) -> String {
    MatrixMarketOptions::upstream_precision().format(x)
}

/// File name used by upstream `saveMatrix(filename)` on rank `rank` of
/// `nb_proc`: `.mtx` appended if `filename` has no extension; with
/// `nb_proc > 1` the extension is replaced by `_rank-<rank>.mtx`.
#[must_use]
pub fn matrix_file_name(filename: &Path, rank: i32, nb_proc: usize) -> PathBuf {
    let mut file = filename.to_path_buf();
    if file.extension().is_none() {
        file.set_extension("mtx");
    }
    if nb_proc > 1 {
        file = rank_suffixed(&file, rank);
    }
    file
}

/// File name used by upstream `saveVector`: the distributed vector first
/// replaces the extension by `_rank-<rank>.mtx` when `nb_proc > 1`, then the
/// default vector appends `.mtx` when no extension is left.
#[must_use]
pub fn vector_file_name(filename: &Path, rank: i32, nb_proc: usize) -> PathBuf {
    let mut file = if nb_proc > 1 {
        rank_suffixed(filename, rank)
    } else {
        filename.to_path_buf()
    };
    if file.extension().is_none() {
        file.set_extension("mtx");
    }
    file
}

/// File name of [`crate::sparse::SparseMatrixAij::save_profile_on`] on rank
/// `rank` of `nb_proc` (UD-180, B): one process, the name as given
/// (upstream `saveProfile`); several, `_rank-<rank>.mtx` replaces the
/// extension, as for `saveMatrix`.
#[must_use]
pub fn profile_file_name(filename: &Path, rank: i32, nb_proc: usize) -> PathBuf {
    if nb_proc > 1 {
        rank_suffixed(filename, rank)
    } else {
        filename.to_path_buf()
    }
}

/// `file.replace_extension(""); file += "_rank-<r>.mtx"`.
fn rank_suffixed(file: &Path, rank: i32) -> PathBuf {
    let stem = if file.extension().is_some() {
        file.with_extension("")
    } else {
        file.to_path_buf()
    };
    let mut s = stem.into_os_string();
    s.push(format!("_rank-{rank}.mtx"));
    PathBuf::from(s)
}

/// Write `text` to `path` (upstream opens with `std::ofstream`, truncating).
pub fn write_text(path: &Path, text: &str) -> Result<()> {
    std::fs::write(path, text).map_err(|source| Error::Io {
        path: path.display().to_string(),
        source,
    })
}

/// Symmetry field of a Matrix Market header.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MmSymmetry {
    /// `general`
    General,
    /// `symmetric`
    Symmetric,
}

/// Field of a Matrix Market coordinate header.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MmField {
    /// `real`
    Real,
    /// `pattern`
    Pattern,
}

/// A parsed Matrix Market coordinate file (1-based coordinates as stored).
#[derive(Clone, Debug, PartialEq)]
pub struct MatrixMarket {
    /// `real` or `pattern`.
    pub field: MmField,
    /// `general` or `symmetric`.
    pub symmetry: MmSymmetry,
    /// Rows.
    pub nrows: usize,
    /// Columns.
    pub ncols: usize,
    /// Row of each entry (1-based).
    pub rows: Vec<i64>,
    /// Column of each entry (1-based).
    pub cols: Vec<i64>,
    /// Value of each entry (the value column of a pattern file if present,
    /// else 1).
    pub values: Vec<Real>,
}

fn parse_err(line: usize, message: impl Into<String>) -> Error {
    Error::Parse {
        file: "<matrix market>".into(),
        line,
        column: 0,
        message: message.into(),
    }
}

/// Parse a Matrix Market coordinate text (the subset upstream writes; also
/// accepts `%` comment lines). Every syntax problem is an [`Error::Parse`].
pub fn parse(text: &str) -> Result<MatrixMarket> {
    let mut lines = text.lines().enumerate();
    let (_, header) = lines.next().ok_or_else(|| parse_err(1, "empty file"))?;
    let words: Vec<&str> = header.split_whitespace().collect();
    if words.len() != 5
        || !words[0].eq_ignore_ascii_case("%%MatrixMarket")
        || !words[1].eq_ignore_ascii_case("matrix")
        || !words[2].eq_ignore_ascii_case("coordinate")
    {
        return Err(parse_err(1, format!("unsupported header '{header}'")));
    }
    let field = match words[3].to_ascii_lowercase().as_str() {
        "real" => MmField::Real,
        "pattern" => MmField::Pattern,
        f => return Err(parse_err(1, format!("unsupported field '{f}'"))),
    };
    let symmetry = match words[4].to_ascii_lowercase().as_str() {
        "general" => MmSymmetry::General,
        "symmetric" => MmSymmetry::Symmetric,
        s => return Err(parse_err(1, format!("unsupported symmetry '{s}'"))),
    };
    let mut body = lines.filter(|(_, l)| !l.trim_start().starts_with('%'));
    let (sl, size) = body
        .next()
        .ok_or_else(|| parse_err(2, "missing size line"))?;
    let dims: Vec<&str> = size.split_whitespace().collect();
    if dims.len() != 3 {
        return Err(parse_err(sl + 1, "size line needs 'rows cols nnz'"));
    }
    let num = |s: &str, l: usize| -> Result<usize> {
        s.parse::<usize>()
            .map_err(|_| parse_err(l + 1, format!("invalid count '{s}'")))
    };
    let (nrows, ncols, nnz) = (num(dims[0], sl)?, num(dims[1], sl)?, num(dims[2], sl)?);
    let mut out = MatrixMarket {
        field,
        symmetry,
        nrows,
        ncols,
        rows: Vec::new(),
        cols: Vec::new(),
        values: Vec::new(),
    };
    for (l, line) in body {
        if line.trim().is_empty() {
            continue;
        }
        let t: Vec<&str> = line.split_whitespace().collect();
        let need = if field == MmField::Real { 3 } else { 2 };
        if t.len() < need || t.len() > 3 {
            return Err(parse_err(l + 1, format!("malformed entry '{line}'")));
        }
        let idx = |s: &str, max: usize| -> Result<i64> {
            let v = s
                .parse::<i64>()
                .map_err(|_| parse_err(l + 1, format!("invalid index '{s}'")))?;
            if v < 1 || usize::try_from(v).map_or(true, |u| u > max) {
                return Err(parse_err(l + 1, format!("index {v} out of 1..={max}")));
            }
            Ok(v)
        };
        out.rows.push(idx(t[0], nrows)?);
        out.cols.push(idx(t[1], ncols)?);
        let v = match t.get(2) {
            Some(s) => s
                .parse::<f64>()
                .map_err(|_| parse_err(l + 1, format!("invalid value '{s}'")))?,
            None => 1.0,
        };
        out.values.push(v);
    }
    if out.rows.len() != nnz {
        return Err(parse_err(
            0,
            format!(
                "header announces {nnz} entries, file has {}",
                out.rows.len()
            ),
        ));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn printf_g_precision_15() {
        // Expected strings follow C11 §7.21.6.1 (confirmed black-box against
        // the oracle's libstdc++ in fixtures/solver/matrix_market_text).
        let cases: &[(f64, &str)] = &[
            (0.0, "0"),
            (-0.0, "-0"),
            (2.0, "2"),
            (10.0, "10"),
            (90.0, "90"),
            (0.1, "0.1"),
            (1.0 / 3.0, "0.333333333333333"),
            (2.0 / 3.0, "0.666666666666667"),
            (1e-5, "1e-05"),
            (1.25e-5, "1.25e-05"),
            (0.0001, "0.0001"),
            (1e15, "1e+15"),
            (1e14, "100000000000000"),
            (123_456_789_012_345.6, "123456789012346"),
            (1_234_567_890_123_455.0, "1.23456789012346e+15"),
            (1_234_567_890_123_445.0, "1.23456789012344e+15"),
            (-2.5e100, "-2.5e+100"),
            (f64::MAX, "1.79769313486232e+308"),
            (5e-324, "4.94065645841247e-324"),
            (0.999_999_999_999_999_9, "1"),
            (f64::INFINITY, "inf"),
            (f64::NEG_INFINITY, "-inf"),
            (f64::NAN, "nan"),
            (-f64::NAN, "-nan"),
        ];
        for &(x, want) in cases {
            assert_eq!(format_value_upstream(x), want, "x = {x:e}");
        }
    }

    /// UD-061: the default precision (17) round-trips
    /// every finite binary64 exactly, where upstream's 15 digits do not.
    #[test]
    fn default_precision_round_trips() {
        let cases = [
            1.000_000_000_000_005,
            f64::MAX,
            f64::MIN_POSITIVE,
            5e-324,
            -1.234_567_890_123_456_7e-300,
            0.1,
            1.0 / 3.0,
            -0.0,
        ];
        for x in cases {
            let t = format_value(x);
            let back: f64 = t.parse().unwrap();
            assert_eq!(back.to_bits(), x.to_bits(), "{x:e} -> {t}");
        }
        assert_eq!(format_value(0.1), "0.10000000000000001");
        assert_eq!(format_value(2.0), "2");
        // upstream precision loses these
        assert_eq!(
            format_value_upstream(1.000_000_000_000_005),
            "1.00000000000001"
        );
        assert_eq!(
            format_value_upstream(f64::MAX).parse::<f64>().unwrap(),
            f64::INFINITY
        );
        // 100 000 pseudo-random bit patterns (finite ones round-trip)
        let mut s: u64 = 18491;
        for _ in 0..100_000 {
            s = s
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            let x = f64::from_bits(s);
            if x.is_finite() {
                let back: f64 = format_value(x).parse().unwrap();
                assert_eq!(back.to_bits(), x.to_bits(), "{x:e}");
            }
        }
    }

    #[test]
    fn file_names_follow_std_filesystem() {
        let p = |s: &str, r, n| matrix_file_name(Path::new(s), r, n);
        assert_eq!(p("matrix", 0, 1), PathBuf::from("matrix.mtx"));
        assert_eq!(p("matrix.mtx", 0, 1), PathBuf::from("matrix.mtx"));
        assert_eq!(p("m.txt", 0, 1), PathBuf::from("m.txt"));
        assert_eq!(p("dir.d/matrix", 0, 1), PathBuf::from("dir.d/matrix.mtx"));
        assert_eq!(p(".hidden", 0, 1), PathBuf::from(".hidden.mtx"));
        assert_eq!(p("matrix", 3, 4), PathBuf::from("matrix_rank-3.mtx"));
        assert_eq!(p("m.txt", 1, 2), PathBuf::from("m_rank-1.mtx"));
        let v = |s: &str, r, n| vector_file_name(Path::new(s), r, n);
        assert_eq!(v("b", 0, 1), PathBuf::from("b.mtx"));
        assert_eq!(v("b.dat", 2, 3), PathBuf::from("b_rank-2.mtx"));
        // UD-180: saveProfile keeps the name on one process
        assert_eq!(
            profile_file_name(Path::new("profile"), 0, 1),
            PathBuf::from("profile")
        );
        assert_eq!(
            profile_file_name(Path::new("profile.mtx"), 1, 2),
            PathBuf::from("profile_rank-1.mtx")
        );
    }

    #[test]
    fn parse_rejects_malformed_input() {
        assert!(parse("").is_err());
        assert!(parse("%%MatrixMarket matrix array real general\n1 1\n1\n").is_err());
        assert!(parse("%%MatrixMarket matrix coordinate real general\n2 2 1\n3 1 1\n").is_err());
        assert!(parse("%%MatrixMarket matrix coordinate real general\n2 2 2\n1 1 1\n").is_err());
        assert!(parse("%%MatrixMarket matrix coordinate real general\n2 2 1\n1 1 x\n").is_err());
        let m = parse("%%MatrixMarket matrix coordinate pattern symmetric\n% c\n2 2 1\n2 1 1\n")
            .unwrap();
        assert_eq!((m.rows[0], m.cols[0], m.values[0]), (2, 1, 1.0));
        assert_eq!(m.symmetry, MmSymmetry::Symmetric);
    }
}
