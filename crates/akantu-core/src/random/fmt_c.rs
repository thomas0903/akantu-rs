// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): the textual output of RandomParameter::printself
// (src/common/aka_random_generator.hh), i.e. libstdc++ `std::ostream << double`
// (glibc printf `%.*g` / `%.*e` conversions).

//! C/C++ floating-point text formatting (`%g`, `%e`) as produced by glibc.
//!
//! Rust's exact float formatting (`{:.N}`, `{:.Ne}`) yields the correctly
//! rounded decimal digits with ties to even, exactly like glibc's `printf` in
//! the default rounding mode; only the layout differs (`1e5` vs `1e+05`,
//! trailing-zero removal of `%g`), which is what this module adapts.

use std::fmt::Write as _;

/// Text for non-finite values as printed by glibc (`inf`, `-inf`, `nan`, `-nan`).
fn non_finite(x: f64) -> Option<&'static str> {
    if x.is_nan() {
        Some(if x.is_sign_negative() { "-nan" } else { "nan" })
    } else if x.is_infinite() {
        Some(if x < 0.0 { "-inf" } else { "inf" })
    } else {
        None
    }
}

/// Split Rust's `{:.Ne}` output into mantissa and exponent.
fn rust_exp(x: f64, precision: usize) -> (String, i32) {
    let s = format!("{x:.precision$e}");
    match s.split_once('e') {
        Some((m, e)) => (m.to_owned(), e.parse().unwrap_or(0)),
        None => (s, 0),
    }
}

/// Append a C exponent suffix (`e+05`, `e-123`).
fn push_c_exponent(out: &mut String, e: i32) {
    let sign = if e < 0 { '-' } else { '+' };
    let _ = write!(out, "e{sign}{:02}", e.unsigned_abs());
}

/// `printf("%.*e", precision, x)`.
pub(super) fn format_e(x: f64, precision: usize) -> String {
    if let Some(s) = non_finite(x) {
        return s.to_owned();
    }
    let (mut m, e) = rust_exp(x, precision);
    push_c_exponent(&mut m, e);
    m
}

/// Remove trailing zeros of the fractional part (and a trailing `.`).
fn strip_fraction_zeros(s: &mut String) {
    if s.contains('.') {
        while s.ends_with('0') {
            s.pop();
        }
        if s.ends_with('.') {
            s.pop();
        }
    }
}

/// `printf("%.*g", precision, x)` (no `#` flag).
pub(crate) fn format_g(x: f64, precision: usize) -> String {
    if let Some(s) = non_finite(x) {
        return s.to_owned();
    }
    let p = precision.max(1);
    if x == 0.0 {
        return if x.is_sign_negative() { "-0" } else { "0" }.to_owned();
    }
    // The exponent X is that of the %e conversion with precision P - 1.
    let (mut mantissa, e) = rust_exp(x, p - 1);
    let p_i = i32::try_from(p).unwrap_or(i32::MAX);
    if e < p_i && e >= -4 {
        let frac = usize::try_from(p_i - 1 - e).unwrap_or(0);
        let mut s = format!("{x:.frac$}");
        strip_fraction_zeros(&mut s);
        s
    } else {
        strip_fraction_zeros(&mut mantissa);
        push_c_exponent(&mut mantissa, e);
        mantissa
    }
}

/// `std::ostream << double` with default flags (`%.6g`).
pub(super) fn ostream_default(x: f64) -> String {
    format_g(x, 6)
}

/// `std::ostream << double` with `scientific` and precision
/// `max_digits10` (17), as set by libstdc++'s distribution `operator<<`.
pub(super) fn ostream_scientific17(x: f64) -> String {
    format_e(x, 17)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn g_format() {
        assert_eq!(format_g(94e3, 6), "94000");
        assert_eq!(format_g(300e6, 6), "3e+08");
        assert_eq!(format_g(1e-7, 6), "1e-07");
        assert_eq!(format_g(0.1, 6), "0.1");
        assert_eq!(format_g(123_456.0, 6), "123456");
        assert_eq!(format_g(1_234_567.0, 6), "1.23457e+06");
        assert_eq!(format_g(0.0001, 6), "0.0001");
        assert_eq!(format_g(0.00001, 6), "1e-05");
        assert_eq!(format_g(999_999.5, 6), "1e+06");
        assert_eq!(format_g(-0.0, 6), "-0");
        assert_eq!(format_g(f64::NEG_INFINITY, 6), "-inf");
    }

    #[test]
    fn e_format() {
        assert_eq!(format_e(9.4e3, 17), "9.40000000000000000e+03");
        assert_eq!(format_e(0.0, 17), "0.00000000000000000e+00");
        assert_eq!(format_e(5e-324, 3), "4.941e-324");
    }
}
