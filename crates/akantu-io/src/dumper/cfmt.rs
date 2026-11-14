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

/// Split Rust's `{:.Ne}` output into mantissa and decimal exponent.
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

/// `printf("%.*e", precision, x)` = `os << std::scientific <<
/// std::setprecision(precision) << x`.
#[must_use]
pub fn format_e(x: f64, precision: usize) -> String {
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

/// `printf("%.*g", precision, x)` = `os << std::setprecision(precision) <<
/// x` with default float flags (precision 6 when never set).
#[must_use]
pub fn format_g(x: f64, precision: usize) -> String {
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

/// `os << x` for a `double` with default stream flags (`%g`, precision 6).
#[must_use]
pub fn format_default(x: f64) -> String {
    format_g(x, 6)
}

#[cfg(test)]
mod tests {
    use super::*;

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
