// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs: C++ iostream-compatible conversions used by the Akantu parser.
// The use of the conversions (`parse_int`, the two-step `parse_bool`, the
// string list/set rules) follows Akantu src/io/parser/parser_tmpl.hh:
// Copyright (©) 2013-2023 EPFL (Ecole Polytechnique Fédérale de Lausanne)
// Laboratory (LSMS - Laboratoire de Simulation en Mécanique des Solides)
// (LGPL-3.0-or-later; see COPYING.lesser and NOTICE.md).
//
// CLEAN-ROOM IMPLEMENTATION (D-030). Written without reading
// any C++ standard-library implementation source (libstdc++, libc++) or the
// code this file replaces. Sources: the ISO C++ standard ([istream::sentry],
// [istream.formatted.arithmetic], [facet.num.get.virtuals] stages 1–3 and the
// bool rules, [facet.num.put.virtuals], [ostream.inserters.arithmetic]), the
// ISO C standard (strtoll/strtoull/strtod, printf `%e`/`%g`, "C" locale
// `isspace`), the calling code in upstream Akantu (LGPL:
// src/io/parser/parser_tmpl.hh, src/io/parser/cppargparse/), and black-box
// observations of the oracle toolchain recorded by
// oracle/probes/parser/stream_blackbox.cc in
// tests/fixtures/parser/stream_blackbox.jsonl (checked by
// tests/stream_blackbox.rs). Provenance log: crates/akantu-parser/CLEANROOM_LOG.md.

//! Emulation of the C++ iostream conversions used by upstream Akantu.
//!
//! Upstream converts parameter values with `std::stringstream` extraction
//! (`sstr >> t`, only `badbit` checked) and prints numbers with
//! `std::ostream <<`. Everything here works on **bytes** (upstream values are
//! `std::string`s, i.e. byte strings) in the classic "C" locale, the only
//! locale upstream uses.
//!
//! # Extraction (`operator>>`)
//!
//! 1. The sentry skips "C"-locale white space ([`is_c_space`]); if the input
//!    is exhausted, `failbit | eofbit` is set and **nothing is stored**
//!    (`value: None` in [`IntExtraction`]).
//! 2. Numbers: the characters of the longest prefix that can still start a
//!    number of the requested kind are accumulated (stage 2): for integers
//!    an optional sign then decimal digits; for floating point an optional
//!    sign, digits, an optional `.` and digits, then — only if a digit was
//!    seen — `e`/`E`, an optional sign and digits. Hexadecimal prefixes,
//!    `inf`/`nan` and thousands separators are never accumulated in the "C"
//!    locale with the default `dec` base.
//! 3. The accumulated field is converted (stage 3) as by `strtoll`/`strtoull`
//!    /`strtod`. If it does not convert completely (e.g. `"-"`, `"1e"`), `0`
//!    is stored and `failbit` set. Out-of-range values store the bound of the
//!    type and set `failbit`, with the type-specific rules of [`StreamInt`];
//!    floating-point overflow stores `±DBL_MAX` (not infinity) and sets
//!    `failbit`, underflow stores the correctly rounded (zero or subnormal)
//!    value without `failbit`.
//! 4. `bool` with `boolalpha` matches `true`/`false` character by character
//!    (case-sensitive) and consumes the matched prefix; with `noboolalpha` it
//!    reads a `long`: `0` → `false`, `1` → `true`, any other stored value →
//!    `true` with `failbit`.
//! 5. `signed char`/`unsigned char` (Rust `i8`/`u8`) extract **one character**
//!    (`"5"` → `53`), never a number.
//!
//! # Insertion (`operator<<`)
//!
//! Default floating-point output is C `printf("%.*g", precision)` (precision
//! 6), `std::scientific` is `printf("%.*e", precision)`; infinities and NaNs
//! print as `inf`, `-inf`, `nan`, `-nan` (sign of the NaN bit pattern).

use std::collections::BTreeSet;

/// White space in the "C" locale (`isspace`): space, `\t`, `\n`, `\v`, `\f`,
/// `\r`. Bytes ≥ 0x80 are never white space.
#[must_use]
pub fn is_c_space(c: u8) -> bool {
    matches!(c, b' ' | b'\t' | b'\n' | 0x0b | 0x0c | b'\r')
}

/// Trims "C"-locale white space from both ends.
#[must_use]
pub fn trim_c_space(bytes: &[u8]) -> &[u8] {
    let start = bytes
        .iter()
        .position(|&c| !is_c_space(c))
        .unwrap_or(bytes.len());
    let end = bytes
        .iter()
        .rposition(|&c| !is_c_space(c))
        .map_or(start, |p| p + 1);
    &bytes[start..end]
}

/// Integer types extractable with `operator>>` (C++ `short`, `int`, `long`,
/// their unsigned counterparts, and `signed char`/`unsigned char` for `i8`/
/// `u8`).
///
/// Range rules of stage 3 for an accumulated decimal field with sign `s` and
/// magnitude `m` (observed on the oracle and consistent with the standard):
///
/// * signed types: the exact value `s·m` is stored if representable, else the
///   bound `MIN`/`MAX` on the same side with `failbit` (also for `short`/
///   `int`, which upstream reads through `long` then clamps);
/// * unsigned types: if `m > MAX`, `MAX` with `failbit` (whatever the sign);
///   otherwise `m`, negated modulo 2ⁿ when the sign is `-` (`"-1"` →
///   `MAX`), without `failbit`.
pub trait StreamInt: Copy {
    /// Whether the type is signed.
    const SIGNED: bool;
    /// `MAX` as `u64`.
    const MAX_U64: u64;
    /// Magnitude of `MIN` as `u64` (`0` for unsigned types).
    const NEG_MIN_U64: u64;
    /// Zero.
    const ZERO: Self;
    /// Maximum value.
    const MAX: Self;
    /// Minimum value.
    const MIN: Self;
    /// `true` for the character types (`i8` = `signed char`, `u8` =
    /// `unsigned char`), whose `operator>>` reads a single character.
    const CHARACTER: bool = false;
    /// The value of sign `negative` and magnitude `magnitude`, which must be
    /// in range: `magnitude <= MAX_U64` (or `<= NEG_MIN_U64` for a negative
    /// signed value). Unsigned types negate modulo 2ⁿ.
    fn from_magnitude(magnitude: u64, negative: bool) -> Self;
    /// The value of a character read by a character type (`CHARACTER`).
    fn from_byte(byte: u8) -> Self {
        Self::from_magnitude(u64::from(byte), false)
    }
}

macro_rules! stream_int_signed {
    ($($t:ty),*) => {$(
        impl StreamInt for $t {
            const SIGNED: bool = true;
            const MAX_U64: u64 = <$t>::MAX as u64;
            const NEG_MIN_U64: u64 = (<$t>::MAX as u64) + 1;
            const ZERO: Self = 0;
            const MAX: Self = <$t>::MAX;
            const MIN: Self = <$t>::MIN;
            fn from_magnitude(magnitude: u64, negative: bool) -> Self {
                debug_assert!(magnitude <= if negative { Self::NEG_MIN_U64 } else { Self::MAX_U64 });
                // In range by contract: `-magnitude` fits in i64 (the wrap
                // only maps 2^63 to i64::MIN, which is the exact value).
                let v = magnitude as i64;
                (if negative { v.wrapping_neg() } else { v }) as $t
            }
        }
    )*};
}

macro_rules! stream_int_unsigned {
    ($($t:ty),*) => {$(
        impl StreamInt for $t {
            const SIGNED: bool = false;
            const MAX_U64: u64 = <$t>::MAX as u64;
            const NEG_MIN_U64: u64 = 0;
            const ZERO: Self = 0;
            const MAX: Self = <$t>::MAX;
            const MIN: Self = 0;
            fn from_magnitude(magnitude: u64, negative: bool) -> Self {
                debug_assert!(magnitude <= Self::MAX_U64);
                let v = magnitude as $t;
                if negative { v.wrapping_neg() } else { v }
            }
        }
    )*};
}

stream_int_signed!(i16, i32, i64);
stream_int_unsigned!(u16, u32, u64);

impl StreamInt for i8 {
    const SIGNED: bool = true;
    const MAX_U64: u64 = i8::MAX as u64;
    const NEG_MIN_U64: u64 = 128;
    const ZERO: Self = 0;
    const MAX: Self = i8::MAX;
    const MIN: Self = i8::MIN;
    const CHARACTER: bool = true;
    fn from_magnitude(magnitude: u64, negative: bool) -> Self {
        let v = magnitude as i64;
        (if negative { v.wrapping_neg() } else { v }) as i8
    }
    fn from_byte(byte: u8) -> Self {
        byte as i8
    }
}

impl StreamInt for u8 {
    const SIGNED: bool = false;
    const MAX_U64: u64 = u8::MAX as u64;
    const NEG_MIN_U64: u64 = 0;
    const ZERO: Self = 0;
    const MAX: Self = u8::MAX;
    const MIN: Self = 0;
    const CHARACTER: bool = true;
    fn from_magnitude(magnitude: u64, negative: bool) -> Self {
        let v = magnitude as u8;
        if negative { v.wrapping_neg() } else { v }
    }
    fn from_byte(byte: u8) -> Self {
        byte
    }
}

/// Result of one `operator>>` extraction from a byte string.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IntExtraction<T> {
    /// Stored value; `None` if nothing was stored (the sentry found only
    /// white space, so the C++ target keeps its previous value).
    pub value: Option<T>,
    /// Stream `failbit` after the extraction.
    pub fail: bool,
    /// Stream `eofbit` after the extraction (the end of the input was reached
    /// while looking for another character).
    pub eof: bool,
    /// Number of bytes consumed from `start` (white space included).
    pub consumed: usize,
}

/// The sentry: position of the first non-space byte at or after `start`
/// (`None` if the input is exhausted).
fn sentry(text: &[u8], start: usize) -> Option<usize> {
    let mut pos = start.min(text.len());
    while pos < text.len() && is_c_space(text[pos]) {
        pos += 1;
    }
    (pos < text.len()).then_some(pos)
}

fn not_stored<T>(text: &[u8], start: usize) -> IntExtraction<T> {
    IntExtraction {
        value: None,
        fail: true,
        eof: true,
        consumed: text.len().saturating_sub(start),
    }
}

/// `operator>>` for an integer type, starting at byte `start`.
#[must_use]
pub fn extract_int<T: StreamInt>(text: &[u8], start: usize) -> IntExtraction<T> {
    let start = start.min(text.len());
    let Some(mut pos) = sentry(text, start) else {
        return not_stored(text, start);
    };
    if T::CHARACTER {
        return IntExtraction {
            value: Some(T::from_byte(text[pos])),
            fail: false,
            eof: false,
            consumed: pos + 1 - start,
        };
    }
    let mut negative = false;
    if matches!(text[pos], b'+' | b'-') {
        negative = text[pos] == b'-';
        pos += 1;
    }
    let digits_start = pos;
    let mut magnitude: u64 = 0;
    let mut too_large = false;
    while let Some(d) = text.get(pos).filter(|c| c.is_ascii_digit()) {
        match magnitude
            .checked_mul(10)
            .and_then(|m| m.checked_add(u64::from(d - b'0')))
        {
            Some(m) => magnitude = m,
            None => too_large = true,
        }
        pos += 1;
    }
    let consumed = pos - start;
    let eof = pos == text.len();
    if pos == digits_start {
        // Empty or sign-only field: not converted.
        return IntExtraction {
            value: Some(T::ZERO),
            fail: true,
            eof,
            consumed,
        };
    }
    let (value, fail) = if negative && T::SIGNED {
        if too_large || magnitude > T::NEG_MIN_U64 {
            (T::MIN, true)
        } else {
            (T::from_magnitude(magnitude, true), false)
        }
    } else if too_large || magnitude > T::MAX_U64 {
        (T::MAX, true)
    } else {
        (T::from_magnitude(magnitude, negative), false)
    };
    IntExtraction {
        value: Some(value),
        fail,
        eof,
        consumed,
    }
}

/// `operator>>(double&)`, starting at byte `start`.
#[must_use]
pub fn extract_double(text: &[u8], start: usize) -> IntExtraction<f64> {
    let start = start.min(text.len());
    let Some(field_start) = sentry(text, start) else {
        return not_stored(text, start);
    };
    let digits = |pos: &mut usize| {
        let from = *pos;
        while text.get(*pos).is_some_and(u8::is_ascii_digit) {
            *pos += 1;
        }
        *pos - from
    };
    let mut pos = field_start;
    if matches!(text[pos], b'+' | b'-') {
        pos += 1;
    }
    let mut mantissa_digits = digits(&mut pos);
    if text.get(pos) == Some(&b'.') {
        pos += 1;
        mantissa_digits += digits(&mut pos);
    }
    let mut complete = mantissa_digits > 0;
    if mantissa_digits > 0 && matches!(text.get(pos), Some(b'e' | b'E')) {
        pos += 1;
        if matches!(text.get(pos), Some(b'+' | b'-')) {
            pos += 1;
        }
        complete = digits(&mut pos) > 0;
    }
    let consumed = pos - start;
    // The field is ASCII by construction.
    let parsed = if complete {
        std::str::from_utf8(&text[field_start..pos])
            .ok()
            .and_then(|f| f.parse::<f64>().ok())
    } else {
        None
    };
    let (value, fail) = match parsed {
        None => (0.0, true),
        Some(v) if v.is_infinite() => (f64::MAX.copysign(v), true),
        Some(v) => (v, false),
    };
    IntExtraction {
        value: Some(value),
        fail,
        eof: pos == text.len(),
        consumed,
    }
}

/// `operator>>(bool&)` with `std::boolalpha` (`alpha`) or
/// `std::noboolalpha`, starting at byte `start`.
#[must_use]
pub fn extract_bool(text: &[u8], start: usize, alpha: bool) -> IntExtraction<bool> {
    let start = start.min(text.len());
    if !alpha {
        let l = extract_int::<i64>(text, start);
        // 0 → false, 1 → true, any other stored value → true + failbit.
        let other = l.value.is_some_and(|v| v != 0 && v != 1);
        let mut b = l.map(|v| v != 0);
        b.fail |= other;
        return b;
    }
    let Some(pos) = sentry(text, start) else {
        return not_stored(text, start);
    };
    let (target, value): (&[u8], bool) = match text[pos] {
        b't' => (b"true", true),
        b'f' => (b"false", false),
        _ => {
            return IntExtraction {
                value: Some(false),
                fail: true,
                eof: false,
                consumed: pos - start,
            };
        }
    };
    for (k, &expected) in target.iter().enumerate().skip(1) {
        let c = text.get(pos + k);
        if c != Some(&expected) {
            // Mismatch or end of input: the matched prefix is consumed.
            return IntExtraction {
                value: Some(false),
                fail: true,
                eof: c.is_none(),
                consumed: pos + k - start,
            };
        }
    }
    IntExtraction {
        value: Some(value),
        fail: false,
        eof: false,
        consumed: pos + target.len() - start,
    }
}

impl<T> IntExtraction<T> {
    fn map<U>(self, f: impl FnOnce(T) -> U) -> IntExtraction<U> {
        IntExtraction {
            value: self.value.map(f),
            fail: self.fail,
            eof: self.eof,
            consumed: self.consumed,
        }
    }
}

/// Upstream `sstr >> t` integer conversion (`ParserParameter::operator T()`,
/// only `badbit` is checked, so it never fails). Upstream leaves its
/// uninitialised `T t` untouched for empty or blank text; the port returns
/// `0` there (category A).
#[must_use]
pub fn parse_int<T: StreamInt>(text: impl AsRef<[u8]>) -> T {
    extract_int::<T>(text.as_ref(), 0).value.unwrap_or(T::ZERO)
}

/// Upstream `ParserParameter::operator bool()`: `sstr >> std::boolalpha >> b`
/// and, if that fails, `clear()` and `sstr >> std::noboolalpha >> b` from
/// where the first attempt stopped. So `true`/`false` (lower case), then
/// integers (`0` → `false`, any other number → `true`); `"True"` → `false`.
/// Empty or blank text leaves upstream's `bool b` uninitialised; the port
/// returns `false` (category A).
#[must_use]
pub fn parse_bool(text: impl AsRef<[u8]>) -> bool {
    let text = text.as_ref();
    let first = extract_bool(text, 0, true);
    if !first.fail {
        return first.value.unwrap_or(false);
    }
    let second = extract_bool(text, first.consumed, false);
    second.value.or(first.value).unwrap_or(false)
}

/// `operator>>(std::string&)` from byte `start`: the first
/// white-space-delimited word (`None` if the input holds no word, in which
/// case C++ leaves the string untouched and sets `failbit`).
#[must_use]
pub fn extract_word(text: &[u8], start: usize) -> Option<&[u8]> {
    let from = sentry(text, start)?;
    let len = text[from..]
        .iter()
        .position(|&c| is_c_space(c))
        .unwrap_or(text.len() - from);
    Some(&text[from..from + len])
}

/// `sstr >> std::string` on bytes: the first white-space-delimited word
/// (empty if there is none).
#[must_use]
pub fn word_bytes(text: &[u8]) -> &[u8] {
    extract_word(text, 0).unwrap_or_default()
}

/// `sstr >> std::string` (first white-space-delimited word, empty if none).
/// Splitting valid UTF-8 at ASCII white space keeps it valid.
#[must_use]
pub fn parse_word(text: &str) -> String {
    String::from_utf8_lossy(word_bytes(text.as_bytes())).into_owned()
}

/// Upstream `operator std::vector<std::string>()` on bytes:
/// `regex_replace(value, "[[:space:]]|\[|\]", "")` then every maximal run of
/// non-`,` bytes (`regex_search("[^,]+")`), in order.
#[must_use]
pub fn string_list_bytes(text: &[u8]) -> Vec<Vec<u8>> {
    let cleaned: Vec<u8> = text
        .iter()
        .copied()
        .filter(|&c| !is_c_space(c) && c != b'[' && c != b']')
        .collect();
    cleaned
        .split(|&c| c == b',')
        .filter(|s| !s.is_empty())
        .map(<[u8]>::to_vec)
        .collect()
}

/// Upstream `operator std::vector<std::string>()` (white space and brackets
/// removed, split on commas, empty items dropped).
#[must_use]
pub fn parse_string_list(text: &str) -> Vec<String> {
    string_list_bytes(text.as_bytes())
        .into_iter()
        .map(|b| String::from_utf8_lossy(&b).into_owned())
        .collect()
}

/// Upstream `operator std::set<std::string>()` (as
/// [`parse_string_list`], ordered and deduplicated; `std::set<std::string>`
/// and `String` both order byte-wise).
#[must_use]
pub fn parse_string_set(text: &str) -> BTreeSet<String> {
    parse_string_list(text).into_iter().collect()
}

fn non_finite(x: f64) -> Option<&'static str> {
    if x.is_nan() {
        Some(if x.is_sign_negative() { "-nan" } else { "nan" })
    } else if x.is_infinite() {
        Some(if x < 0.0 { "-inf" } else { "inf" })
    } else {
        None
    }
}

/// Splits Rust's `{:e}` output (`d.ddde-5`) into mantissa and exponent.
fn rust_exp(s: &str) -> (&str, i32) {
    match s.split_once('e') {
        Some((m, e)) => (m, e.parse().unwrap_or(0)),
        None => (s, 0),
    }
}

/// C exponent suffix: `e`, sign, at least two digits.
fn c_exponent(e: i32) -> String {
    format!("e{}{:02}", if e < 0 { '-' } else { '+' }, e.unsigned_abs())
}

/// `std::scientific` with precision `prec` (`os << x`), i.e. C
/// `printf("%.*e", prec, x)`: correctly rounded (ties to even on the exact
/// binary value), at least two exponent digits.
#[must_use]
pub fn format_scientific(x: f64, prec: usize) -> String {
    if let Some(s) = non_finite(x) {
        return s.to_string();
    }
    let s = format!("{x:.prec$e}");
    let (mantissa, e) = rust_exp(&s);
    format!("{mantissa}{}", c_exponent(e))
}

/// Removes trailing zeros of a fractional part, and the point if nothing is
/// left (`%g` without `#`).
fn strip_fraction_zeros(s: &str) -> &str {
    if s.contains('.') {
        s.trim_end_matches('0').trim_end_matches('.')
    } else {
        s
    }
}

/// Default floating-point format with precision `prec` (`os <<
/// std::setprecision(prec) << x`), i.e. C `printf("%.*g", prec, x)`:
/// with `P = max(prec, 1)` and `X` the decimal exponent after rounding to `P`
/// significant digits, fixed notation with `P - 1 - X` decimals if
/// `-4 <= X < P`, else scientific with `P - 1` decimals; trailing zeros
/// (and a trailing point) removed.
#[must_use]
pub fn format_general(x: f64, prec: usize) -> String {
    if let Some(s) = non_finite(x) {
        return s.to_string();
    }
    let p = prec.max(1);
    let sci = format!("{x:.prec$e}", prec = p - 1);
    let (mantissa, e) = rust_exp(&sci);
    let p_i = i64::try_from(p).unwrap_or(i64::MAX);
    let x_exp = i64::from(e);
    if x_exp >= -4 && x_exp < p_i {
        let decimals = usize::try_from(p_i - 1 - x_exp).unwrap_or(0);
        let fixed = format!("{x:.decimals$}");
        strip_fraction_zeros(&fixed).to_string()
    } else {
        format!("{}{}", strip_fraction_zeros(mantissa), c_exponent(e))
    }
}

/// `os << x` with a default stream state (precision 6): `%g`.
#[must_use]
pub fn format_double(x: f64) -> String {
    format_general(x, 6)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spaces() {
        for c in [b' ', b'\t', b'\n', 0x0b, 0x0c, b'\r'] {
            assert!(is_c_space(c));
        }
        assert!(!is_c_space(0xa0) && !is_c_space(0) && !is_c_space(b'x'));
        assert_eq!(trim_c_space(b" \t a b \n"), b"a b");
        assert_eq!(trim_c_space(b"  "), b"");
        assert_eq!(trim_c_space(b""), b"");
    }

    #[test]
    fn integers() {
        assert_eq!(parse_int::<i32>("3.5"), 3);
        assert_eq!(parse_int::<i32>("abc"), 0);
        assert_eq!(parse_int::<i32>("2147483648"), i32::MAX);
        assert_eq!(parse_int::<i32>("-99999999999999999999"), i32::MIN);
        assert_eq!(parse_int::<u32>("-1"), u32::MAX);
        assert_eq!(parse_int::<u32>("-4294967295"), 1);
        assert_eq!(parse_int::<u32>("-4294967296"), u32::MAX);
        assert_eq!(parse_int::<u64>("-18446744073709551615"), 1);
        assert_eq!(parse_int::<i64>("-9223372036854775808"), i64::MIN);
        assert_eq!(parse_int::<i16>("40000"), i16::MAX);
        assert_eq!(parse_int::<u16>("-65535"), 1);
        assert_eq!(parse_int::<i32>("0x1A"), 0);
        assert_eq!(parse_int::<i32>(" \t+12abc"), 12);
        assert_eq!(parse_int::<i32>(""), 0);
        // Character types read one character.
        assert_eq!(parse_int::<i8>(" 5"), 53);
        assert_eq!(parse_int::<u8>(&b"\xff"[..]), 255);
        assert_eq!(parse_int::<i8>(&b"\xff"[..]), -1);
        let e = extract_int::<i64>(b"  -", 0);
        assert_eq!((e.value, e.fail, e.consumed), (Some(0), true, 3));
        let e = extract_int::<i64>(b"a 42 ", 1);
        assert_eq!((e.value, e.fail, e.consumed), (Some(42), false, 3));
        let e = extract_int::<i64>(b"  ", 0);
        assert_eq!((e.value, e.fail, e.consumed), (None, true, 2));
    }

    #[test]
    fn booleans() {
        assert!(parse_bool("true"));
        assert!(!parse_bool("false"));
        assert!(!parse_bool("True"));
        assert!(parse_bool("1"));
        assert!(parse_bool("2"));
        assert!(parse_bool("-1"));
        assert!(!parse_bool("0"));
        assert!(!parse_bool("tru"));
        assert!(!parse_bool(""));
        assert!(parse_bool("truex"));
        assert!(parse_bool("  42"));
        let e = extract_bool(b"trUe", 0, true);
        assert_eq!((e.value, e.fail, e.consumed), (Some(false), true, 2));
        let e = extract_bool(b"2", 0, false);
        assert_eq!((e.value, e.fail), (Some(true), true));
    }

    #[test]
    fn doubles() {
        let d = |s: &str| {
            let e = extract_double(s.as_bytes(), 0);
            (e.value, e.fail, e.consumed)
        };
        assert_eq!(d("1.5e3x"), (Some(1500.0), false, 5));
        assert_eq!(d("1e"), (Some(0.0), true, 2));
        assert_eq!(d("+.e1"), (Some(0.0), true, 2));
        assert_eq!(d("1e400"), (Some(f64::MAX), true, 5));
        assert_eq!(d("-1e400"), (Some(-f64::MAX), true, 6));
        assert_eq!(d("1e-400"), (Some(0.0), false, 6));
        assert_eq!(d("0x10"), (Some(0.0), false, 1));
        assert_eq!(d("1e5.5"), (Some(1e5), false, 3));
        assert_eq!(d(" "), (None, true, 1));
    }

    #[test]
    fn words_and_lists() {
        assert_eq!(parse_word("  steel  rest"), "steel");
        assert_eq!(parse_word(" \t"), "");
        assert_eq!(
            parse_string_list(" [a, b ,c],,d "),
            vec!["a", "b", "c", "d"]
        );
        assert_eq!(
            parse_string_set("b, a, b"),
            ["a", "b"].into_iter().map(String::from).collect()
        );
        assert_eq!(
            string_list_bytes(b"x\xa0y,z"),
            vec![b"x\xa0y".to_vec(), b"z".to_vec()]
        );
    }

    #[test]
    fn formatting() {
        assert_eq!(format_scientific(0.2, 17), "2.00000000000000011e-01");
        assert_eq!(format_scientific(1.5, 0), "2e+00");
        assert_eq!(format_scientific(1e100, 2), "1.00e+100");
        assert_eq!(format_scientific(-0.0, 1), "-0.0e+00");
        assert_eq!(format_scientific(f64::NAN, 3), "nan");
        assert_eq!(format_scientific(-f64::NAN, 3), "-nan");
        assert_eq!(format_double(0.0001), "0.0001");
        assert_eq!(format_double(0.00001), "1e-05");
        assert_eq!(format_double(123456.0), "123456");
        assert_eq!(format_double(1234567.0), "1.23457e+06");
        assert_eq!(format_double(999999.5), "1e+06");
        assert_eq!(format_double(-2.0 * std::f64::consts::PI), "-6.28319");
        assert_eq!(format_double(f64::NEG_INFINITY), "-inf");
        assert_eq!(format_general(2.5, 0), "2");
        assert_eq!(format_general(0.1, 17), "0.10000000000000001");
    }
}
