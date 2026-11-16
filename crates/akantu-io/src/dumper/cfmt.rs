// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): the textual number output of
// third-party/iohelper/src/{paraview_helper.tcc, dumper_text.hh,
// dumper_lammps.{hh,cc}} and paraview_helper.cc (writeTimePVD), i.e.
// `std::ostream << double` in default, `std::scientific` and `std::setw`
// formats (glibc printf `%g` / `%.*e` conversions). Adapted from
// crates/akantu-core/src/random/fmt_c.rs (same project).
// IOHelper (third-party/iohelper, v1.1.1): Copyright (©) 2010-2012, 2014 EPFL
// (Ecole Polytechnique Fédérale de Lausanne), Laboratory (LSMS - Laboratoire
// de Simulation en Mécanique des Solides). IOHelper is free software: you can
// redistribute it and/or modify it under the terms of the GNU Lesser General
// Public License as published by the Free Software Foundation, either version
// 3 of the License, or (at your option) any later version.

//! C/C++ stream formatting of floating-point numbers.
//!
//! Rust's exact float formatting (`{:.Ne}`, `{:.N}`) yields the correctly
//! rounded decimal digits with ties to even, exactly like glibc's `printf`
//! in the default rounding mode; only the layout differs (`1e5` vs
//! `1e+05`, `%g` trailing-zero removal), which is what this module adapts.

use std::fmt::Write as _;

/// Text of non-finite values as printed by glibc.
fn non_finite(x: f64) -> Option<&'static str> {
    if x.is_nan() {
        Some(if x.is_sign_negative() { "-nan" } else { "nan" })
    } else if x.is_infinite() {
        Some(if x < 0.0 { "-inf" } else { "inf" })
    } else {
        None
    }
}

/// Appends Rust's `{:.precision$e}` text of the finite `x` to `out` and
/// returns the position of its `e` and the decimal exponent.
fn push_rust_exp(out: &mut String, x: f64, precision: usize) -> (usize, i32) {
    let start = out.len();
    let _ = write!(out, "{x:.precision$e}");
    let b = &out.as_bytes()[start..];
    // Rust's LowerExp output always has an `e` followed by an optional `-`
    // and the decimal exponent.
    let Some(epos) = b.iter().rposition(|&c| c == b'e') else {
        return (out.len(), 0);
    };
    let (neg, digits) = match b.get(epos + 1) {
        Some(b'-') => (true, &b[epos + 2..]),
        _ => (false, &b[epos + 1..]),
    };
    let mut e: i32 = 0;
    for &d in digits {
        e = e.saturating_mul(10).saturating_add(i32::from(d - b'0'));
    }
    (start + epos, if neg { -e } else { e })
}

/// Append a C exponent suffix (`e+05`, `e-123`; `E` if `upper`).
fn push_c_exponent(out: &mut String, e: i32, upper: bool) {
    out.push(if upper { 'E' } else { 'e' });
    out.push(if e < 0 { '-' } else { '+' });
    let a = e.unsigned_abs();
    if a < 10 {
        out.push('0');
    }
    push_u64(out, u64::from(a));
}

/// Append the decimal digits of `v` (the text of `{v}`).
pub(crate) fn push_u64(out: &mut String, mut v: u64) {
    let mut buf = [0u8; 20];
    let mut i = buf.len();
    loop {
        i -= 1;
        // `v % 10 < 10`: the cast cannot truncate.
        buf[i] = b'0' + (v % 10) as u8;
        v /= 10;
        if v == 0 {
            break;
        }
    }
    // ASCII digits only.
    out.push_str(std::str::from_utf8(&buf[i..]).unwrap_or(""));
}

/// Append the decimal text of `v` (the text of `{v}`).
pub(crate) fn push_i64(out: &mut String, v: i64) {
    if v < 0 {
        out.push('-');
    }
    push_u64(out, v.unsigned_abs());
}

/// Remove trailing zeros of the fractional part of `out[start..]` (and a
/// trailing `.`).
fn strip_fraction_zeros(out: &mut String, start: usize) {
    if out.as_bytes()[start..].contains(&b'.') {
        let b = out.as_bytes();
        let mut end = b.len();
        while end > start && b[end - 1] == b'0' {
            end -= 1;
        }
        if end > start && b[end - 1] == b'.' {
            end -= 1;
        }
        out.truncate(end);
    }
}

/// Appends `printf("%.*e", precision, x)` to `out` (`os << std::scientific
/// << std::setprecision(precision) << x`): the same characters as
/// [`format_e`] without an allocation per number.
pub(crate) fn push_e(out: &mut String, x: f64, precision: usize) {
    if let Some(s) = non_finite(x) {
        out.push_str(s);
        return;
    }
    let (epos, e) = push_rust_exp(out, x, precision);
    out.truncate(epos);
    push_c_exponent(out, e, false);
}

/// Appends `printf("%.*e", precision, x)` right-aligned in a field of
/// `width` characters (`std::setw(width)`, the text of `{:>width$}`).
pub(crate) fn push_e_width(out: &mut String, x: f64, precision: usize, width: usize) {
    let start = out.len();
    push_e(out, x, precision);
    let len = out.len() - start;
    if len < width {
        const SPACES: &str = "                                ";
        let mut pad = width - len;
        while pad > 0 {
            let k = pad.min(SPACES.len());
            out.insert_str(start, &SPACES[..k]);
            pad -= k;
        }
    }
}

/// Largest `%g` precision whose digits are rearranged on the stack (longer
/// ones use a temporary buffer).
const STACK_DIGITS: usize = 40;

/// Appends `printf("%.*g", precision, x)` to `out` (`%.*G` if `upper`):
/// the same characters as [`format_g`] without an allocation per number.
///
/// The exponent X is that of the `%e` conversion with precision P − 1
/// (P = max(precision, 1)). In fixed notation (P > X ≥ −4) the digits are
/// those of that conversion: fixed notation with P − 1 − X decimals rounds
/// at the same decimal position, except when rounding to P digits carried
/// into a new leading digit (X one more than the exponent of `x`); then
/// both give 10^X (fixed notation rounds one place coarser, and `x` is
/// within half a unit of the finer place of 10^X, so it rounds to 10^X
/// there too; a tie at the finer place is above the midpoint of the
/// coarser one).
pub(crate) fn push_g(out: &mut String, x: f64, precision: usize, upper: bool) {
    if let Some(s) = non_finite(x) {
        if upper {
            for c in s.chars() {
                out.push(c.to_ascii_uppercase());
            }
        } else {
            out.push_str(s);
        }
        return;
    }
    let p = precision.max(1);
    if x == 0.0 {
        out.push_str(if x.is_sign_negative() { "-0" } else { "0" });
        return;
    }
    let start = out.len();
    let (epos, e) = push_rust_exp(out, x, p - 1);
    let p_i = i64::try_from(p).unwrap_or(i64::MAX);
    let e64 = i64::from(e);
    if e64 < p_i && e64 >= -4 {
        // Digits of the mantissa `[-]d[.ddd]`: exactly P of them.
        let mut stack = [0u8; STACK_DIGITS];
        let mut heap = Vec::new();
        let negative = out.as_bytes()[start] == b'-';
        let digits: &[u8] = {
            let m = &out.as_bytes()[start..epos];
            if p <= STACK_DIGITS {
                let mut n = 0;
                for &c in m.iter().filter(|c| c.is_ascii_digit()) {
                    stack[n] = c;
                    n += 1;
                }
                &stack[..n]
            } else {
                heap.extend(m.iter().copied().filter(u8::is_ascii_digit));
                &heap
            }
        };
        out.truncate(start);
        if negative {
            out.push('-');
        }
        let frac_start;
        if e >= 0 {
            // `e < p`: the integer part has e + 1 of the P digits.
            let int_len = usize::try_from(e).unwrap_or(0) + 1;
            let (int, frac) = digits.split_at(int_len.min(digits.len()));
            out.push_str(std::str::from_utf8(int).unwrap_or(""));
            frac_start = out.len();
            if !frac.is_empty() {
                out.push('.');
                out.push_str(std::str::from_utf8(frac).unwrap_or(""));
            }
        } else {
            frac_start = out.len();
            out.push_str("0.");
            for _ in 0..(-e - 1) {
                out.push('0');
            }
            out.push_str(std::str::from_utf8(digits).unwrap_or(""));
        }
        strip_fraction_zeros(out, frac_start);
    } else {
        out.truncate(epos);
        strip_fraction_zeros(out, start);
        push_c_exponent(out, e, upper);
    }
}

/// `printf("%.*e", precision, x)` = `os << std::scientific <<
/// std::setprecision(precision) << x`.
#[must_use]
pub fn format_e(x: f64, precision: usize) -> String {
    let mut s = String::new();
    push_e(&mut s, x, precision);
    s
}

/// `printf("%.*g", precision, x)` = `os << std::setprecision(precision) <<
/// x` with default float flags (precision 6 when never set).
#[must_use]
pub fn format_g(x: f64, precision: usize) -> String {
    let mut s = String::new();
    push_g(&mut s, x, precision, false);
    s
}

/// `os << x` for a `double` with default stream flags (`%g`, precision 6).
#[must_use]
pub fn format_default(x: f64) -> String {
    format_g(x, 6)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The previous implementation (one `String` per number), kept as the
    /// reference of the property test below.
    mod reference {
        fn rust_exp(x: f64, precision: usize) -> (String, i32) {
            let s = format!("{x:.precision$e}");
            match s.split_once('e') {
                Some((m, e)) => (m.to_owned(), e.parse().unwrap_or(0)),
                None => (s, 0),
            }
        }
        fn c_exponent(e: i32, upper: bool) -> String {
            let sign = if e < 0 { '-' } else { '+' };
            let c = if upper { 'E' } else { 'e' };
            format!("{c}{sign}{:02}", e.unsigned_abs())
        }
        fn non_finite(x: f64, upper: bool) -> Option<String> {
            let s = if x.is_nan() {
                if x.is_sign_negative() { "-nan" } else { "nan" }
            } else if x.is_infinite() {
                if x < 0.0 { "-inf" } else { "inf" }
            } else {
                return None;
            };
            Some(if upper {
                s.to_uppercase()
            } else {
                s.to_owned()
            })
        }
        fn strip(s: &mut String) {
            if s.contains('.') {
                while s.ends_with('0') {
                    s.pop();
                }
                if s.ends_with('.') {
                    s.pop();
                }
            }
        }
        pub fn e(x: f64, precision: usize) -> String {
            if let Some(s) = non_finite(x, false) {
                return s;
            }
            let (m, e) = rust_exp(x, precision);
            m + &c_exponent(e, false)
        }
        pub fn g(x: f64, precision: usize, upper: bool) -> String {
            if let Some(s) = non_finite(x, upper) {
                return s;
            }
            let p = precision.max(1);
            if x == 0.0 {
                return if x.is_sign_negative() { "-0" } else { "0" }.to_owned();
            }
            let (mut mantissa, e) = rust_exp(x, p - 1);
            let p_i = i32::try_from(p).unwrap_or(i32::MAX);
            if e < p_i && e >= -4 {
                let frac = usize::try_from(p_i - 1 - e).unwrap_or(0);
                let mut s = format!("{x:.frac$}");
                strip(&mut s);
                s
            } else {
                strip(&mut mantissa);
                mantissa + &c_exponent(e, upper)
            }
        }
    }

    /// xorshift64*: deterministic test inputs.
    fn rng(state: &mut u64) -> u64 {
        *state ^= *state >> 12;
        *state ^= *state << 25;
        *state ^= *state >> 27;
        state.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn check_all(x: f64, out: &mut String) {
        for p in (0..=25).chain([30, 40, 41, 60, 120, 800]) {
            assert_eq!(
                format_e(x, p),
                reference::e(x, p),
                "e: x = {x:e} ({:#x}), p = {p}",
                x.to_bits()
            );
            for upper in [false, true] {
                out.clear();
                out.push_str("prefix ");
                push_g(out, x, p, upper);
                assert_eq!(
                    &out[7..],
                    reference::g(x, p, upper),
                    "g: x = {x:e} ({:#x}), p = {p}, upper = {upper}",
                    x.to_bits()
                );
            }
        }
        for w in [0, 1, 22, 30] {
            out.clear();
            push_e_width(out, x, 15, w);
            assert_eq!(*out, format!("{:>w$}", reference::e(x, 15)));
        }
    }

    #[test]
    fn push_functions_match_the_previous_implementation() {
        let mut out = String::new();
        let mut state = 0x9E37_79B9_7F4A_7C15_u64;
        let specials = [
            0.0,
            -0.0,
            f64::NAN,
            -f64::NAN,
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::MIN_POSITIVE,
            -f64::MIN_POSITIVE,
            f64::from_bits(1),
            f64::from_bits(0x000F_FFFF_FFFF_FFFF),
            f64::MAX,
            f64::MIN,
            f64::EPSILON,
            0.5,
            9.5,
            99.5,
            999_999.5,
            0.000_099_999_5,
            0.000_1,
            0.000_099_999_999_999_999_99,
            1e23_f64.next_down(),
        ];
        for &x in &specials {
            check_all(x, &mut out);
        }
        for _ in 0..2000 {
            // random bit patterns (every exponent, subnormals, NaN payloads)
            let x = f64::from_bits(rng(&mut state));
            check_all(x, &mut out);
            // values near powers of ten and short decimals
            let k = i32::try_from(rng(&mut state) % 40).unwrap_or(0) - 20;
            let p10 = 10f64.powi(k);
            for x in [p10, p10 * (1.0 - f64::EPSILON), p10 * (1.0 + f64::EPSILON)] {
                check_all(x, &mut out);
                check_all(-x, &mut out);
            }
            #[allow(clippy::cast_precision_loss)] // small integers
            let short = (rng(&mut state) % 100_000) as f64 / 1000.0;
            check_all(short, &mut out);
            check_all(short * p10, &mut out);
        }
    }

    #[test]
    fn scientific_matches_glibc_layout() {
        assert_eq!(format_e(0.0, 15), "0.000000000000000e+00");
        assert_eq!(format_e(-0.0, 6), "-0.000000e+00");
        assert_eq!(format_e(1.0 / 3.0, 15), "3.333333333333333e-01");
        assert_eq!(format_e(8e-300, 15), "8.000000000000000e-300");
        assert_eq!(format_e(-2e20, 15), "-2.000000000000000e+20");
        assert_eq!(format_e(1.0, 0), "1e+00");
        assert_eq!(format_e(0.1, 16), "1.0000000000000001e-01");
        assert_eq!(format_e(f64::NAN, 6), "nan");
        assert_eq!(format_e(f64::NEG_INFINITY, 6), "-inf");
    }

    #[test]
    fn general_matches_glibc_layout() {
        assert_eq!(format_default(0.0), "0");
        assert_eq!(format_default(-0.0), "-0");
        assert_eq!(format_default(0.1), "0.1");
        assert_eq!(format_default(0.35 + 0.1), "0.45");
        assert_eq!(format_default(1.0 / 3.0), "0.333333");
        assert_eq!(format_default(1e7), "1e+07");
        assert_eq!(format_default(-2e-5), "-2e-05");
        assert_eq!(format_default(123_456.0), "123456");
        assert_eq!(format_default(1_234_567.0), "1.23457e+06");
        assert_eq!(format_default(0.0001), "0.0001");
        assert_eq!(format_default(0.00001), "1e-05");
    }
}
