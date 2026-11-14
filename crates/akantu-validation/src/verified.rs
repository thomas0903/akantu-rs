// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). This file has no
// direct C++ counterpart: it reads the upstream `<test>.verified` text
// outputs (compared by upstream `cmake/akantu_test_driver.sh -r`), so that
// they can be reused as low-precision oracles.

//! Parsing of upstream `.verified` files.
//!
//! Upstream tests print values as text and the CTest driver diffs the
//! output against `<test>.verified`. These files are weak oracles: a printed
//! value is a rounding of the true value, and the rounding depends on the
//! *stream format* that produced it, not on the digits that survived in the
//! file (C++ `defaultfloat` removes trailing zeros: `1` stands for any value
//! in `[0.9999995, 1.000005]`, `2` for `[1.999995, 2.000005]`, while `0` is
//! printed only for an exact zero). The caller therefore declares the
//! printing format of every column ([`VerifiedFormat`]); the parser derives,
//! per value, the rounding interval of that format ([`Allowance`]) and
//! rejects tokens the format could not have produced. Nothing is inferred
//! from the token length alone.

use std::collections::BTreeMap;

use crate::array::Field;
use crate::compare::{CompareOptions, compare_f64};
use crate::error::{Error, Result};
use crate::report::QuantityReport;
use crate::tolerance::ResolvedTolerance;

/// How a column of numbers was printed by the upstream program.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NumberFormat {
    /// Integer output (`std::ostream << Int`): compared exactly.
    Integer,
    /// `std::defaultfloat` with `precision` significant digits (C++ default:
    /// 6), i.e. `%g`: trailing zeros removed, scientific notation when the
    /// exponent is `< -4` or `>= precision`. A printed `0` is an exact zero.
    DefaultFloat {
        /// Stream precision (significant digits).
        precision: u32,
    },
    /// `std::fixed` with `precision` decimals (`%.*f`); a printed zero may
    /// stand for any value within half a unit of the last decimal.
    Fixed {
        /// Stream precision (decimals).
        precision: u32,
    },
    /// `std::scientific` with `precision` mantissa decimals (`%.*e`).
    Scientific {
        /// Stream precision (mantissa decimals).
        precision: u32,
    },
}

/// Printing format of a `.verified` table: a default for every column and
/// per-column overrides (0-based column index among the numeric tokens of a
/// line).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedFormat {
    /// Format of columns without an override.
    pub default: NumberFormat,
    /// Per-column overrides.
    pub columns: BTreeMap<usize, NumberFormat>,
}

impl VerifiedFormat {
    /// Every column printed with the C++ stream defaults (`defaultfloat`,
    /// precision 6).
    #[must_use]
    pub fn cpp_default() -> Self {
        Self::uniform(NumberFormat::DefaultFloat { precision: 6 })
    }

    /// Every column printed with `format`.
    #[must_use]
    pub fn uniform(format: NumberFormat) -> Self {
        Self {
            default: format,
            columns: BTreeMap::new(),
        }
    }

    /// Builder: override the format of column `col`.
    #[must_use]
    pub fn with_column(mut self, col: usize, format: NumberFormat) -> Self {
        self.columns.insert(col, format);
        self
    }

    /// Format of column `col`.
    #[must_use]
    pub fn column(&self, col: usize) -> NumberFormat {
        self.columns.get(&col).copied().unwrap_or(self.default)
    }
}

/// One line of a `.verified` file containing numbers.
#[derive(Debug, Clone, PartialEq)]
pub struct VerifiedRow {
    /// 1-based line number in the file.
    pub line: usize,
    /// Non-numeric tokens of the line, in order (labels such as `w1:`).
    pub labels: Vec<String>,
    /// Numeric tokens, in order, as printed (punctuation stripped).
    pub tokens: Vec<String>,
    /// Parsed values of `tokens`.
    pub values: Vec<f64>,
}

/// Parsed `.verified` file.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct VerifiedTable {
    /// Column names, when the first non-empty line is a header without
    /// numbers.
    pub header: Option<Vec<String>>,
    /// Lines containing at least one number.
    pub rows: Vec<VerifiedRow>,
}

/// Decomposition of a decimal token: digits (no leading zeros, trailing
/// zeros kept), number of decimals and exponent.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Decimal {
    /// Digits of the mantissa as printed, without the decimal point and
    /// without leading zeros.
    mantissa_digits: String,
    /// Number of digits after the decimal point as printed.
    decimals: usize,
    /// Explicit exponent (`e±xx`), if present.
    exponent: Option<i32>,
    /// `true` when the exponent is written the way C/C++ streams write it:
    /// an explicit sign and at least two digits (`e+06`, `e-05`).
    exponent_is_c_style: bool,
    /// Decimal exponent of the leading non-zero digit (`None` for zero).
    leading_exponent: Option<i32>,
}

impl Decimal {
    /// `true` when the printed magnitude is an exact power of ten (`1`,
    /// `1e+06`, `0.001`, `1.0000e+02`): the rounding interval of such a
    /// token is asymmetric because smaller magnitudes have one more decimal.
    fn is_power_of_ten(&self) -> bool {
        self.mantissa_digits.trim_end_matches('0') == "1"
    }
}

/// Strip surrounding punctuation `()[]{}:;=` from a token.
fn strip_punctuation(token: &str) -> &str {
    token.trim_matches(|c: char| "()[]{}:;=".contains(c))
}

/// `true` when a token is meant to be a number (starts with a digit, or a
/// sign/point followed by a digit). Such tokens must parse or the file is
/// malformed: silently turning them into labels would lose expected values.
fn looks_numeric(t: &str) -> bool {
    let b = t.as_bytes();
    match b.first() {
        Some(c) if c.is_ascii_digit() => true,
        Some(b'+' | b'-' | b'.') => {
            b.get(1).is_some_and(u8::is_ascii_digit)
                || (matches!(b.first(), Some(b'+' | b'-'))
                    && b.get(1) == Some(&b'.')
                    && b.get(2).is_some_and(u8::is_ascii_digit))
        }
        _ => false,
    }
}

/// Parse a plain decimal token (`[+-]digits[.digits][e[+-]digits]`).
///
/// Exponent arithmetic is checked: an exponent
/// outside `i32`, or a token whose leading-digit exponent is not
/// representable, is malformed. A finite decimal spelling whose magnitude
/// overflows binary64 (`1e+309`) or a non-zero one that underflows to zero
/// (`1e-400`) is rejected too: only the explicit tokens `inf`/`nan` denote
/// non-finite values, and a printed non-zero value never stands for zero.
fn parse_decimal(t: &str) -> std::result::Result<(f64, Decimal), String> {
    let malformed = || "not a plain decimal number".to_string();
    let unsigned = t.trim_start_matches(['+', '-']);
    if t.len() - unsigned.len() > 1 {
        return Err(malformed());
    }
    let (mantissa, exponent, exponent_is_c_style) = match unsigned.find(['e', 'E']) {
        Some(p) => {
            let exp = &unsigned[p + 1..];
            let digits = exp.trim_start_matches(['+', '-']);
            if digits.is_empty()
                || exp.len() - digits.len() > 1
                || !digits.bytes().all(|b| b.is_ascii_digit())
            {
                return Err(malformed());
            }
            let c_style = exp.len() > digits.len() && digits.len() >= 2;
            let value = exp
                .parse::<i32>()
                .map_err(|_| format!("exponent `{exp}` is outside the 32-bit range"))?;
            (&unsigned[..p], Some(value), c_style)
        }
        None => (unsigned, None, false),
    };
    if mantissa.is_empty()
        || !mantissa.bytes().all(|b| b.is_ascii_digit() || b == b'.')
        || mantissa.bytes().filter(|&b| b == b'.').count() > 1
        || !mantissa.bytes().any(|b| b.is_ascii_digit())
    {
        return Err(malformed());
    }
    let value: f64 = t.parse().map_err(|_| malformed())?;
    let (int_part, frac_part) = mantissa.split_once('.').unwrap_or((mantissa, ""));
    let digits: String = format!("{int_part}{frac_part}");
    // Decimal exponent of the leading non-zero digit:
    // int_len - 1 - pos + exponent, in checked 64-bit arithmetic, which must
    // fit an i32.
    let leading_exponent = match digits.bytes().position(|b| b != b'0') {
        None => None,
        Some(pos) => {
            let lead = i64::try_from(int_part.len())
                .ok()
                .zip(i64::try_from(pos).ok())
                .and_then(|(len, pos)| len.checked_sub(1)?.checked_sub(pos))
                .and_then(|l| l.checked_add(i64::from(exponent.unwrap_or(0))))
                .and_then(|l| i32::try_from(l).ok());
            Some(lead.ok_or_else(|| {
                "the decimal exponent of its leading digit is outside the 32-bit range".to_string()
            })?)
        }
    };
    if !value.is_finite() {
        return Err(format!(
            "the finite decimal spelling overflows binary64 (parsed as {value}); only the \
             tokens `inf`/`nan` denote non-finite values"
        ));
    }
    if value == 0.0 && leading_exponent.is_some() {
        return Err("the non-zero decimal value underflows to zero in binary64".into());
    }
    Ok((
        value,
        Decimal {
            mantissa_digits: digits.trim_start_matches('0').to_string(),
            decimals: frac_part.len(),
            exponent,
            exponent_is_c_style,
            leading_exponent,
        },
    ))
}

/// Parse a numeric token (`nan`/`inf` included). Returns `None` for a
/// non-numeric label, `Some(Err)` for a malformed numeric-looking token.
fn parse_token(token: &str) -> Option<Result<(String, f64)>> {
    let t = strip_punctuation(token);
    if t.is_empty() {
        return None;
    }
    let lower = t.to_ascii_lowercase();
    let unsigned = lower.trim_start_matches(['+', '-']);
    if matches!(unsigned, "nan" | "inf" | "infinity") {
        return lower.parse::<f64>().ok().map(|v| Ok((t.to_string(), v)));
    }
    if !looks_numeric(t) {
        return None;
    }
    match parse_decimal(t) {
        Ok((v, _)) => Some(Ok((t.to_string(), v))),
        Err(why) => Some(Err(Error::shape(format!(
            "malformed numeric token `{token}` in .verified text: {why} (only representable \
             plain decimal numbers, nan and inf are supported)"
        )))),
    }
}

/// Parse a decimal number token, returning its value.
///
/// Surrounding punctuation `()[]{}:;=` is stripped. Returns `None` for
/// labels; malformed numeric-looking tokens are an error.
pub fn parse_number(token: &str) -> Result<Option<f64>> {
    parse_token(token).map(|r| r.map(|(_, v)| v)).transpose()
}

/// Parse the text of a `.verified` file. Numeric-looking tokens that are not
/// plain decimal numbers (`1.2.3`, `0x10`, `15.12KiByte`) make the whole
/// parse fail: filter such lines with [`parse_verified_lines`] first.
pub fn parse_verified(text: &str) -> Result<VerifiedTable> {
    parse_verified_lines(text.lines())
}

/// Parse selected lines of a `.verified` file (see [`parse_verified`]).
pub fn parse_verified_lines<'a>(lines: impl IntoIterator<Item = &'a str>) -> Result<VerifiedTable> {
    let mut table = VerifiedTable::default();
    let mut seen_content = false;
    for (n, line) in lines.into_iter().enumerate() {
        let tokens: Vec<&str> = line
            .split(|c: char| c.is_whitespace() || c == ',')
            .filter(|t| !t.is_empty())
            .collect();
        if tokens.is_empty() {
            continue;
        }
        let mut row = VerifiedRow {
            line: n + 1,
            labels: Vec::new(),
            tokens: Vec::new(),
            values: Vec::new(),
        };
        for t in &tokens {
            match parse_token(t) {
                Some(Ok((tok, v))) => {
                    row.tokens.push(tok);
                    row.values.push(v);
                }
                Some(Err(e)) => {
                    return Err(Error::shape(format!("line {}: {e}", n + 1)));
                }
                None => row.labels.push((*t).to_string()),
            }
        }
        if row.values.is_empty() {
            if !seen_content && table.header.is_none() {
                table.header = Some(row.labels);
            }
        } else {
            table.rows.push(row);
        }
        seen_content = true;
    }
    Ok(table)
}

/// `10^e` correctly rounded to binary64 (subnormal results included):
/// `0` below the subnormal range, `inf` above the finite range. The exponent
/// is 64-bit, so sums of `i32` exponents and `u32` precisions cannot
/// overflow; callers still combine them with checked arithmetic.
fn pow10(e: i64) -> f64 {
    if e > 400 {
        return f64::INFINITY;
    }
    if e < -400 {
        return 0.0;
    }
    // Rust's float parser rounds correctly; `powi` underflows to zero
    // early for negative exponents (it computes 1 / 10^|e|).
    format!("1e{e}").parse().unwrap_or(f64::NAN)
}

/// Checked sum of exponent terms.
fn exponent_sum(terms: &[i64]) -> Option<i64> {
    terms.iter().try_fold(0i64, |acc, &t| acc.checked_add(t))
}

/// Rounding interval of a printed token: every true value the format prints
/// as this token lies in `[value - below, value + above]` (plus a 2-ulp
/// slack for the binary rounding of the decimal token itself).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Allowance {
    /// Allowed distance below the printed value (towards `-inf`).
    pub below: f64,
    /// Allowed distance above the printed value (towards `+inf`).
    pub above: f64,
}

impl Allowance {
    const EXACT: Allowance = Allowance {
        below: 0.0,
        above: 0.0,
    };

    /// Interval of a value printed with a rounding unit `half` (half a unit
    /// in the last printed digit) whose magnitude may also come from the
    /// next lower decade (`toward_zero` < `half` for powers of ten).
    fn magnitude(value: f64, half: f64, toward_zero: f64) -> Self {
        let slack = 2.0 * f64::EPSILON * value.abs();
        if value.is_sign_negative() {
            Allowance {
                below: half + slack,
                above: toward_zero + slack,
            }
        } else {
            Allowance {
                below: toward_zero + slack,
                above: half + slack,
            }
        }
    }

    /// The larger of both sides.
    #[must_use]
    pub fn max(self) -> f64 {
        self.below.max(self.above)
    }
}

/// Rounding interval of a printed token under `format`: the largest
/// distances between the printed (parsed) value and a true value that the
/// format prints as this token. Rejects tokens the format cannot produce.
///
/// * `Integer`, and `0` in `defaultfloat` or `scientific`, are exact (`%g`
///   and `%e` print any non-zero value with significant digits: `1e-300`,
///   never `0`);
/// * a `defaultfloat`/`scientific` token whose mantissa is a power of ten
///   (`1`, `1e+06`, `1.0000e+02`) has an asymmetric interval: towards zero
///   the next lower decade prints one more digit (`999999.5` → `1e+06`, but
///   `999999.4` → `999999`);
/// * `fixed` has an absolute unit; a printed zero stands for any value
///   within half a unit.
pub fn allowance(token: &str, format: NumberFormat) -> Result<Allowance> {
    let t = strip_punctuation(token);
    let lower = t.to_ascii_lowercase();
    let bad = |why: &str| {
        Err(Error::shape(format!(
            "token `{token}` is not a {format:?} output: {why}"
        )))
    };
    if matches!(
        lower.trim_start_matches(['+', '-']),
        "nan" | "inf" | "infinity"
    ) {
        return Ok(Allowance::EXACT);
    }
    let (value, d) = match parse_decimal(t) {
        Ok(x) => x,
        Err(why) => return bad(&why),
    };
    let unrepresentable = || bad("the rounding unit of this token is not representable");
    if t.starts_with('+') {
        return bad("C++ streams do not print a `+` sign by default");
    }
    match format {
        NumberFormat::Integer => {
            if d.decimals != 0 || d.exponent.is_some() || t.contains('.') {
                return bad("integers are printed without decimal point or exponent");
            }
            Ok(Allowance::EXACT)
        }
        NumberFormat::DefaultFloat { precision } => {
            let p = i64::from(precision.max(1));
            if d.exponent.is_some() && !d.exponent_is_c_style {
                return bad("the exponent is printed with a sign and at least two digits");
            }
            if t.ends_with('.') {
                return bad("defaultfloat never ends with a decimal point");
            }
            let Some(lead) = d.leading_exponent else {
                // `%g` prints `0` only for an exact zero.
                if d.decimals != 0 || d.exponent.is_some() {
                    return bad("defaultfloat prints an exact zero as `0`");
                }
                return Ok(Allowance::EXACT);
            };
            let significant =
                i64::try_from(d.mantissa_digits.trim_end_matches('0').len()).unwrap_or(i64::MAX);
            if significant > p {
                return bad("more significant digits than the stream precision");
            }
            if d.decimals > 0 && d.mantissa_digits.ends_with('0') {
                return bad("defaultfloat removes trailing zeros");
            }
            let lead = i64::from(lead);
            let scientific = lead < -4 || lead >= p;
            if scientific != d.exponent.is_some() {
                return bad(if scientific {
                    "this magnitude is printed in scientific notation"
                } else {
                    "this magnitude is printed in fixed notation"
                });
            }
            if scientific {
                // d.ddddde±xx: exactly one digit before the point.
                let int_digits = d.mantissa_digits.len().saturating_sub(d.decimals);
                if int_digits != 1 {
                    return bad("scientific notation has one digit before the point");
                }
            }
            let (Some(unit), Some(lower_unit)) =
                (exponent_sum(&[lead, -p, 1]), exponent_sum(&[lead, -p]))
            else {
                return unrepresentable();
            };
            let half = 0.5 * pow10(unit);
            let toward_zero = if d.is_power_of_ten() {
                0.5 * pow10(lower_unit)
            } else {
                half
            };
            finite(Allowance::magnitude(value, half, toward_zero)).map_or_else(unrepresentable, Ok)
        }
        NumberFormat::Fixed { precision } => {
            let p = usize::try_from(precision).unwrap_or(usize::MAX);
            if d.exponent.is_some() || d.decimals != p || (p == 0 && t.contains('.')) {
                return bad("fixed notation prints exactly `precision` decimals, no exponent");
            }
            let half = 0.5 * pow10(-i64::from(precision));
            finite(Allowance::magnitude(value, half, half)).map_or_else(unrepresentable, Ok)
        }
        NumberFormat::Scientific { precision } => {
            let p = usize::try_from(precision).unwrap_or(usize::MAX);
            let Some(exp) = d.exponent else {
                return bad("scientific notation always has an exponent");
            };
            if !d.exponent_is_c_style {
                return bad("the exponent is printed with a sign and at least two digits");
            }
            let int_digits = d.mantissa_digits.len().saturating_sub(d.decimals);
            let int_part_len = t
                .trim_start_matches('-')
                .split(['e', 'E'])
                .next()
                .unwrap_or("")
                .split('.')
                .next()
                .unwrap_or("")
                .len();
            if d.decimals != p || int_part_len != 1 {
                return bad("scientific notation is d.ddd…e±xx with `precision` decimals");
            }
            let Some(lead) = d.leading_exponent else {
                // `%e` prints `0.000e+00` only for an exact zero.
                if exp != 0 {
                    return bad("scientific notation prints zero with exponent +00");
                }
                return Ok(Allowance::EXACT);
            };
            if int_digits != 1 || lead != exp {
                return bad("scientific notation has one non-zero digit before the point");
            }
            let p = i64::from(precision);
            let (Some(unit), Some(lower_unit)) = (
                exponent_sum(&[i64::from(exp), -p]),
                exponent_sum(&[i64::from(exp), -1, -p]),
            ) else {
                return unrepresentable();
            };
            let half = 0.5 * pow10(unit);
            let toward_zero = if d.is_power_of_ten() {
                0.5 * pow10(lower_unit)
            } else {
                half
            };
            finite(Allowance::magnitude(value, half, toward_zero)).map_or_else(unrepresentable, Ok)
        }
    }
}

/// `Some(a)` when both sides of the interval are finite: an infinite
/// allowance would accept any value.
fn finite(a: Allowance) -> Option<Allowance> {
    (a.below.is_finite() && a.above.is_finite()).then_some(a)
}

/// Numeric content of a `.verified` table with the rounding interval of
/// every value under the declared printing format.
#[derive(Debug, Clone, PartialEq)]
pub struct VerifiedField {
    /// Values `[entry, component]`.
    pub field: Field<f64>,
    /// Allowed distance below each value.
    pub below: Vec<f64>,
    /// Allowed distance above each value.
    pub above: Vec<f64>,
}

impl VerifiedTable {
    /// Rectangular view `[entry, component]` of the numeric values, with the
    /// per-entry rounding interval of the declared printing format. Fails if
    /// the table has no numeric row, if rows have different widths, or if a
    /// token is not a possible output of its column's format.
    pub fn to_field(&self, format: &VerifiedFormat) -> Result<VerifiedField> {
        if self.rows.is_empty() {
            return Err(Error::shape(
                "the .verified text contains no numeric row: nothing to compare against",
            ));
        }
        let width = self.rows[0].values.len();
        if let Some(bad) = self.rows.iter().find(|r| r.values.len() != width) {
            return Err(Error::shape(format!(
                "line {} has {} numbers, the first numeric line has {width}",
                bad.line,
                bad.values.len()
            )));
        }
        let n = self.rows.len() * width;
        let (mut data, mut below, mut above) = (
            Vec::with_capacity(n),
            Vec::with_capacity(n),
            Vec::with_capacity(n),
        );
        for r in &self.rows {
            for (col, (tok, v)) in r.tokens.iter().zip(&r.values).enumerate() {
                let a = allowance(tok, format.column(col))
                    .map_err(|e| Error::shape(format!("line {}, column {col}: {e}", r.line)))?;
                data.push(*v);
                below.push(a.below);
                above.push(a.above);
            }
        }
        let field = Field::new(
            vec![self.rows.len(), width],
            data,
            vec!["entry".into(), "component".into()],
        )?;
        Ok(VerifiedField {
            field,
            below,
            above,
        })
    }

    /// Column index by header name.
    #[must_use]
    pub fn column(&self, name: &str) -> Option<usize> {
        self.header.as_ref()?.iter().position(|h| h == name)
    }
}

/// Compare Rust values (row-major `[rows, cols]`) against a `.verified`
/// table printed with `format`. Each entry may deviate within the rounding
/// interval of its printed format in addition to `tol` (which still needs a
/// justification, e.g. "printed precision only"). Nothing is inferred from
/// the printed token length: the format is declared by the caller.
pub fn compare_against_verified(
    case_id: &str,
    quantity: &str,
    table: &VerifiedTable,
    actual: &[f64],
    tol: &ResolvedTolerance,
    format: &VerifiedFormat,
) -> Result<QuantityReport> {
    let vf = table.to_field(format)?;
    let mut report = compare_f64(
        case_id,
        quantity,
        &vf.field,
        &vf.field.shape.clone(),
        actual,
        tol,
        &CompareOptions {
            extra_abs: Some(&vf.above),
            extra_abs_below: Some(&vf.below),
            component_labels: table.header.as_deref(),
            ragged: None,
            value_equality: false,
        },
    );
    report.message = Some(format!(
        "upstream .verified text oracle printed as {:?}: the rounding interval of that format is added to the tolerance{}",
        format.default,
        report
            .message
            .as_ref()
            .map_or_else(String::new, |m| format!("; {m}"))
    ));
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::report::Outcome;
    use crate::tolerance::ToleranceSpec;

    const G6: NumberFormat = NumberFormat::DefaultFloat { precision: 6 };

    /// Allowances carry a 2-ulp slack of the parsed value: compare loosely.
    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() <= 1e-6 * b.abs()
    }

    fn sym(t: &str, f: NumberFormat) -> f64 {
        let a = allowance(t, f).unwrap();
        assert!(close(a.below, a.above), "{t}: asymmetric {a:?}");
        a.above
    }

    fn exact() -> ResolvedTolerance {
        ResolvedTolerance {
            spec: ToleranceSpec::exact("printed precision only"),
            origin: "test".into(),
        }
    }

    #[test]
    fn number_tokens() {
        assert_eq!(parse_number("2.464762e-02").unwrap(), Some(2.464762e-02));
        assert_eq!(parse_number("-1.0").unwrap(), Some(-1.0));
        assert_eq!(parse_number("5").unwrap(), Some(5.0));
        assert_eq!(parse_number("(0").unwrap(), Some(0.0));
        assert_eq!(parse_number("2)").unwrap(), Some(2.0));
        assert_eq!(parse_number("1e3").unwrap(), Some(1000.0));
        assert!(parse_number("-nan").unwrap().unwrap().is_nan());
        assert_eq!(parse_number("inf").unwrap(), Some(f64::INFINITY));
        assert_eq!(parse_number("w1:").unwrap(), None);
        assert_eq!(parse_number("seqaij").unwrap(), None);
        assert_eq!(parse_number(".").unwrap(), None);
        // Malformed numeric-looking tokens are errors, not labels.
        assert!(parse_number("1.2.3").is_err());
        assert!(parse_number("0x10").is_err());
        assert!(parse_number("15.12KiByte").is_err());
        assert!(parse_number("1e").is_err());
        assert!(parse_number("1e+-3").is_err());
    }

    /// C++ defaultfloat allowances come from the format, not from the
    /// surviving digits. `0` is exact; `2` allows half a unit of the 6th
    /// significant digit; powers of ten have an asymmetric interval.
    #[test]
    fn defaultfloat_allowances() {
        let a = |t: &str| allowance(t, G6).unwrap();
        assert_eq!(a("0"), Allowance::EXACT);
        assert_eq!(a("-0"), Allowance::EXACT);
        assert!(close(sym("2", G6), 0.5e-5));
        assert!(close(sym("123456", G6), 0.5));
        assert!(close(sym("0.0246476", G6), 0.5e-7));
        assert!(close(sym("1.5e-05", G6), 0.5e-10));
        // Powers of ten: `1` stands for [0.9999995, 1.000005].
        let one = a("1");
        assert!(
            close(one.above, 0.5e-5) && close(one.below, 0.5e-6),
            "{one:?}"
        );
        let mone = a("-1");
        assert!(
            close(mone.below, 0.5e-5) && close(mone.above, 0.5e-6),
            "{mone:?}"
        );
        let mega = a("1e+06");
        assert!(close(mega.above, 5.0) && close(mega.below, 0.5), "{mega:?}");
        let tiny = a("0.0001");
        assert!(
            close(tiny.above, 0.5e-9) && close(tiny.below, 0.5e-10),
            "{tiny:?}"
        );
        assert!(close(a("100000").below, 0.05) && close(a("100000").above, 0.5));
        assert_eq!(a("nan"), Allowance::EXACT);
        assert_eq!(a("inf"), Allowance::EXACT);
        // %g keeps fixed notation down to exponent -4: `2.46476e-02` is not
        // a defaultfloat(6) output (it would read `0.0246476`).
        assert!(allowance("2.46476e-02", G6).is_err());
        // Tokens defaultfloat(6) cannot have printed.
        assert!(
            allowance("2.464762e-02", G6).is_err(),
            "7 significant digits"
        );
        assert!(allowance("1.50", G6).is_err(), "trailing zero");
        assert!(allowance("1e-05", G6).is_ok());
        assert!(allowance("1e-5", G6).is_err(), "exponent has two digits");
        assert!(allowance("1e05", G6).is_err(), "exponent has a sign");
        assert!(
            allowance("12e+05", G6).is_err(),
            "one digit before the point"
        );
        assert!(allowance("0.00001", G6).is_err(), "exp < -4 is scientific");
        assert!(allowance("1234567", G6).is_err());
        assert!(allowance("0.0", G6).is_err(), "zero prints as `0`");
        assert!(allowance("+1", G6).is_err(), "no `+` sign by default");
        // Precision 15 accepts 7 significant digits in fixed notation.
        let g15 = NumberFormat::DefaultFloat { precision: 15 };
        // At 15 digits the 2-ulp binary-parse slack is visible next to the
        // half unit.
        let a15 = sym("0.02464762", g15);
        assert!((0.5e-16..=0.5e-16 + 2e-17).contains(&a15), "{a15:e}");
        assert!(
            allowance("2.464762e-02", g15).is_err(),
            "still fixed notation"
        );
        let a15 = sym("123456789", g15);
        assert!((0.5e-6..=0.5e-6 + 1e-7).contains(&a15), "{a15:e}");
    }

    #[test]
    fn fixed_scientific_and_integer_allowances() {
        let f3 = NumberFormat::Fixed { precision: 3 };
        // %.3f prints 1e-7 as `0.000`: a fixed zero has a half unit.
        assert!(close(sym("0.000", f3), 0.5e-3));
        assert!(close(sym("-0.000", f3), 0.5e-3));
        assert!(close(sym("12.500", f3), 0.5e-3));
        assert!(
            close(sym("1.000", f3), 0.5e-3),
            "fixed has an absolute unit"
        );
        assert!(allowance("12.5", f3).is_err());
        assert!(allowance("1e+00", f3).is_err());
        let s4 = NumberFormat::Scientific { precision: 4 };
        assert!(close(sym("1.2345e+02", s4), 0.5e-2));
        // %e prints only an exact zero as 0.0000e+00.
        assert_eq!(allowance("0.0000e+00", s4).unwrap(), Allowance::EXACT);
        assert!(allowance("0.0000e+01", s4).is_err());
        let p = allowance("1.0000e+02", s4).unwrap();
        assert!(close(p.above, 0.5e-2) && close(p.below, 0.5e-3), "{p:?}");
        assert!(allowance("1.23e+02", s4).is_err());
        assert!(allowance("123.45", s4).is_err());
        assert!(allowance("12.345e+02", s4).is_err());
        assert!(allowance("1.2345e+2", s4).is_err());
        assert_eq!(
            allowance("42", NumberFormat::Integer).unwrap(),
            Allowance::EXACT
        );
        assert_eq!(
            allowance("-7", NumberFormat::Integer).unwrap(),
            Allowance::EXACT
        );
        assert!(allowance("42.0", NumberFormat::Integer).is_err());
        assert!(allowance("42.", NumberFormat::Integer).is_err());
        assert!(allowance("1e2", NumberFormat::Integer).is_err());
    }

    /// Counterexample: `value 0\nvalue 1\n` must not
    /// accept `[0.49, 1.49]`.
    #[test]
    fn defaultfloat_rejects_half_unit_of_the_integer_digit() {
        let t = parse_verified("value 0\nvalue 1\n").unwrap();
        let cmp = |actual: &[f64], fmt: &VerifiedFormat| {
            compare_against_verified("c", "v", &t, actual, &exact(), fmt).unwrap()
        };
        let g = VerifiedFormat::cpp_default();
        let r = cmp(&[0.49, 1.49], &g);
        assert_eq!(r.outcome, Outcome::Fail, "{r}");
        assert_eq!(r.n_violations, 2);
        // Values within the real rounding interval pass ...
        assert!(cmp(&[0.0, 1.000_004], &g).passed());
        assert!(cmp(&[0.0, 0.999_999_6], &g).passed());
        // ... but `1` cannot stand for 0.999996 (printed `0.999996`).
        assert_eq!(cmp(&[0.0, 0.999_996], &g).n_violations, 1);
        // A printed zero is exact: 1e-7 against `0` fails.
        let r = cmp(&[1e-7, 1.0], &g);
        assert_eq!(r.n_violations, 1);
        assert_eq!(r.violations[0].location.flat, 0);
        // Integer columns compare exactly.
        let r = cmp(
            &[0.0, 1.000_004],
            &VerifiedFormat::uniform(NumberFormat::Integer),
        );
        assert_eq!(r.n_violations, 1);
    }

    #[test]
    fn energy_table_with_header_in_scientific_format() {
        let text = "          time,          wext,          epot\n\
                    0.000000e+00,  0.000000e+00,  1.000000e-02\n\
                    1.000000e-03,  1.325501e-20,  2.464762e-02\n";
        let t = parse_verified(text).unwrap();
        assert_eq!(t.header.as_ref().unwrap(), &["time", "wext", "epot"]);
        assert_eq!(t.column("epot"), Some(2));
        let fmt = VerifiedFormat::uniform(NumberFormat::Scientific { precision: 6 });
        let vf = t.to_field(&fmt).unwrap();
        assert_eq!(vf.field.shape, vec![2, 3]);
        assert!(close(vf.above[5], 0.5e-8) && close(vf.below[5], 0.5e-8));
        assert_eq!(
            (vf.below[0], vf.above[0]),
            (0.0, 0.0),
            "scientific zero is exact"
        );
        // 1.000000e-02 is a power of ten: asymmetric.
        assert!(close(vf.above[2], 0.5e-8) && close(vf.below[2], 0.5e-9));
        // Not a defaultfloat output (7 significant digits, fixed-exponent zeros).
        assert!(t.to_field(&VerifiedFormat::cpp_default()).is_err());

        let actual = [0.0, 0.0, 0.01, 0.001, 1.3255014e-20, 0.024_647_624];
        let r = compare_against_verified("c", "energies", &t, &actual, &exact(), &fmt).unwrap();
        assert!(r.passed(), "{r}");
        assert_eq!(r.per_component[2].label.as_deref(), Some("epot"));
        let wrong = [0.0, 0.0, 0.01, 0.001, 1.3255014e-20, 0.024_647_63];
        let r = compare_against_verified("c", "energies", &t, &wrong, &exact(), &fmt).unwrap();
        assert_eq!(r.n_violations, 1);
        // A non-zero value against a printed scientific zero fails.
        let nonzero = [1e-300, 0.0, 0.01, 0.001, 1.3255014e-20, 0.024_647_624];
        let r = compare_against_verified("c", "energies", &t, &nonzero, &exact(), &fmt).unwrap();
        assert_eq!(r.n_violations, 1);
    }

    /// Extreme exponents are ordinary malformed-text errors, never
    /// panics (`10e2147483647` overflowed the leading-exponent sum) and
    /// never a silent `inf`/zero for a finite non-zero decimal spelling.
    #[test]
    fn extreme_exponents_are_errors_not_panics() {
        for bad in [
            "10e2147483647",      // leading exponent 2^31: overflowed an i32 sum
            "0.0001e-2147483648", // leading exponent below i32::MIN
            "1e99999999999",      // exponent outside i32
            "1e-99999999999",
            "0e99999999999",
            "1e2147483647", // finite spelling, overflows binary64
            "1e+309",
            "-1e+309",
            "1.8e+308",
            "1e-2147483648", // non-zero spelling, underflows to zero
            "1e-400",
            "-2.5e-330",
        ] {
            let e = parse_number(bad).expect_err(bad);
            assert!(
                e.to_string().contains("malformed numeric token"),
                "{bad}: {e}"
            );
            assert!(parse_verified(&format!("x {bad}\n")).is_err(), "{bad}");
            assert!(allowance(bad, G6).is_err(), "{bad}");
            let s6 = NumberFormat::Scientific { precision: 6 };
            assert!(allowance(bad, s6).is_err(), "{bad}");
        }
        let e = parse_verified("a 1\nb 1e+309\n").unwrap_err().to_string();
        assert!(
            e.contains("line 2") && e.contains("overflows binary64"),
            "{e}"
        );
        let e = parse_number("1e-400").unwrap_err().to_string();
        assert!(e.contains("underflows to zero"), "{e}");
        let e = parse_number("10e2147483647").unwrap_err().to_string();
        assert!(e.contains("32-bit range"), "{e}");
        // Explicit non-finite tokens and representable extremes still parse.
        assert_eq!(parse_number("inf").unwrap(), Some(f64::INFINITY));
        assert_eq!(parse_number("-inf").unwrap(), Some(f64::NEG_INFINITY));
        assert_eq!(parse_number("1.79769e+308").unwrap(), Some(1.79769e308));
        assert_eq!(parse_number("2.22507e-308").unwrap(), Some(2.22507e-308));
        let min_sub = parse_number("4.94066e-324").unwrap().unwrap();
        assert_eq!(min_sub.to_bits(), 1, "smallest subnormal");
        // A zero mantissa with a large exponent is zero, not an underflow.
        assert_eq!(parse_number("0e+999999").unwrap(), Some(0.0));
        assert!(allowance("0e+999999", G6).is_err(), "%g prints zero as `0`");
    }

    /// Allowances at the ends of the binary64 range and with extreme
    /// precisions are computed in checked 64-bit exponent arithmetic with a
    /// correctly rounded power of ten (subnormal units are not flushed to
    /// zero), and are always finite.
    #[test]
    fn allowances_at_extreme_exponents_and_precisions() {
        let a = |t: &str, f: NumberFormat| allowance(t, f).unwrap();
        // Largest finite magnitudes: half a unit of the 6th digit.
        let big = a("1.79769e+308", G6);
        assert!(
            close(big.above, 0.5e303) && close(big.below, 0.5e303),
            "{big:?}"
        );
        let p = a("1e+308", G6);
        assert!(close(p.above, 0.5e303) && close(p.below, 0.5e302), "{p:?}");
        let s4 = NumberFormat::Scientific { precision: 4 };
        let p = a("1.0000e+308", s4);
        assert!(close(p.above, 0.5e304) && close(p.below, 0.5e303), "{p:?}");
        // Subnormal range: the unit 0.5e-315 is a (subnormal) double; powi
        // would have flushed it to zero.
        let sub = a("1e-310", G6);
        assert!(
            sub.above > 4.9e-316 && sub.above < 5.1e-316 && sub.below > 4.9e-317,
            "{sub:?}"
        );
        let sub = a("1.00000e-310", NumberFormat::Scientific { precision: 5 });
        assert!(sub.above > 4.9e-316 && sub.above < 5.1e-316, "{sub:?}");
        // Below the subnormal spacing the interval contains a single double:
        // exact.
        assert_eq!(a("4.94066e-324", G6), Allowance::EXACT);
        // Extreme precisions: no overflow of `lead - precision`.
        let huge = NumberFormat::DefaultFloat {
            precision: u32::MAX,
        };
        let h = a("1e-05", huge);
        assert!(h.max() < 1e-20, "{h:?}");
        let h = a("123456789", huge);
        assert!(h.max() < 1e-7, "{h:?}");
        assert!(
            allowance(
                "0.5",
                NumberFormat::Fixed {
                    precision: u32::MAX
                }
            )
            .is_err()
        );
        assert!(
            allowance(
                "1.5e+00",
                NumberFormat::Scientific {
                    precision: u32::MAX
                }
            )
            .is_err()
        );
        // Zero precision is valid for fixed/scientific.
        assert!(close(sym("12", NumberFormat::Fixed { precision: 0 }), 0.5));
        let s0 = a("2e+300", NumberFormat::Scientific { precision: 0 });
        assert!(
            close(s0.above, 0.5e300) && close(s0.below, 0.5e300),
            "{s0:?}"
        );
        for t in ["1.79769e+308", "1e+308", "1e-310", "4.94066e-324"] {
            let x = a(t, G6);
            assert!(x.below.is_finite() && x.above.is_finite(), "{t}");
        }
    }

    #[test]
    fn labelled_lines_ragged_and_empty() {
        let text = "w1: 0.5 w2: 0\nw1: 0.222222 w2: 0.152582\n";
        let t = parse_verified(text).unwrap();
        assert!(t.header.is_none());
        assert_eq!(t.rows[1].labels, vec!["w1:", "w2:"]);
        assert_eq!(t.rows[1].values, vec![0.222222, 0.152582]);
        let ragged = parse_verified("1 2\n3\n").unwrap();
        assert!(ragged.to_field(&VerifiedFormat::cpp_default()).is_err());
        // Missing numeric output is an error, never an empty pass.
        let empty = parse_verified("failed to compute values\n").unwrap();
        assert!(empty.rows.is_empty());
        let e = compare_against_verified(
            "c",
            "v",
            &empty,
            &[],
            &exact(),
            &VerifiedFormat::cpp_default(),
        )
        .unwrap_err();
        assert!(e.to_string().contains("no numeric row"), "{e}");
        assert!(parse_verified("").unwrap().rows.is_empty());
        // Malformed numeric tokens fail the parse; filtering lines works.
        let mixed = " + memory size   : 15.12KiByte\n + values : {{2, 2}, {2, 2}}\n";
        assert!(parse_verified(mixed).is_err());
        let t = parse_verified_lines(mixed.lines().filter(|l| l.contains("values"))).unwrap();
        assert_eq!(t.rows[0].values, vec![2.0, 2.0, 2.0, 2.0]);
        let r = compare_against_verified(
            "c",
            "v",
            &t,
            &[2.0, 2.0, 2.0, 2.000_004],
            &exact(),
            &VerifiedFormat::cpp_default(),
        )
        .unwrap();
        assert!(r.passed(), "{r}");
        // Integer-looking values printed by defaultfloat do not inherit a
        // half-unit-of-the-integer floor: 2.4 against `2` fails.
        let r = compare_against_verified(
            "c",
            "v",
            &t,
            &[2.0, 2.0, 2.0, 2.4],
            &exact(),
            &VerifiedFormat::cpp_default(),
        )
        .unwrap();
        assert_eq!(r.n_violations, 1);
        // Per-column formats: the first column is an integer index.
        let t = parse_verified("0 1.5\n1 2.5\n").unwrap();
        let fmt = VerifiedFormat::cpp_default().with_column(0, NumberFormat::Integer);
        let vf = t.to_field(&fmt).unwrap();
        assert_eq!(vf.above[0], 0.0);
        assert!(vf.above[1] > 0.0);
        // A column declared as integer rejects a decimal token instead of
        // silently giving it an allowance.
        let t = parse_verified("0.5 1.5\n").unwrap();
        assert!(t.to_field(&fmt).is_err());
    }
}
