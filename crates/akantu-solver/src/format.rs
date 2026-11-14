// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs helper (no upstream file): formatting of binary64 values as the
// C/C++ standard streams print them (`std::scientific` = `%.Ne`, default
// floating format = `%.Ng`), from the format specifications of ISO C
// (fprintf) — used for upstream verbose output and for comparing test
// outputs with upstream `.verified` files. Rust's `{:.N$e}` gives the
// correctly rounded decimal mantissa; only the exponent spelling differs.

//! C-stream number formatting (`%e`, `%g`).

/// `%.{prec}e`: `d.ddddde±XX` (at least two exponent digits), correctly
/// rounded; `inf`/`nan` spelled as C does (`inf`, `-inf`, `nan`, `-nan`).
#[must_use]
pub fn c_scientific(x: f64, prec: usize) -> String {
    if !x.is_finite() {
        return special(x);
    }
    let s = format!("{x:.prec$e}");
    let (mant, exp) = s.split_once('e').unwrap_or((&s, "0"));
    let e: i32 = exp.parse().unwrap_or(0);
    let sign = if e < 0 { '-' } else { '+' };
    format!("{mant}e{sign}{:02}", e.unsigned_abs())
}

/// `%.{prec}g` (C++ default floating format with `setprecision(prec)`):
/// `P = prec` (0 → 1); with `X` the decimal exponent of the value rounded
/// to `P` significant digits, `%e` with `P − 1` digits if `X < −4` or
/// `X ≥ P`, else `%f` with `P − 1 − X` digits; trailing zeros (and a
/// trailing point) removed.
#[must_use]
pub fn c_general(x: f64, prec: usize) -> String {
    if !x.is_finite() {
        return special(x);
    }
    let p = prec.max(1);
    if x == 0.0 {
        return if x.is_sign_negative() {
            "-0".into()
        } else {
            "0".into()
        };
    }
    let e_form = format!("{x:.*e}", p - 1);
    let (_, exp) = e_form.split_once('e').unwrap_or((&e_form, "0"));
    let xexp: i64 = exp.parse().unwrap_or(0);
    #[allow(clippy::cast_possible_wrap)] // p is a small precision
    let pi = p as i64;
    if xexp < -4 || xexp >= pi {
        let s = c_scientific(x, p - 1);
        let (mant, rest) = s.split_once('e').unwrap_or((&s, ""));
        format!("{}e{rest}", strip_zeros(mant))
    } else {
        #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)] // 0 ≤ P−1−X < P
        let digits = (pi - 1 - xexp) as usize;
        strip_zeros(&format!("{x:.digits$}")).to_owned()
    }
}

fn strip_zeros(s: &str) -> &str {
    if s.contains('.') {
        s.trim_end_matches('0').trim_end_matches('.')
    } else {
        s
    }
}

fn special(x: f64) -> String {
    match (x.is_nan(), x.is_sign_negative()) {
        (true, false) => "nan".into(),
        (true, true) => "-nan".into(),
        (false, false) => "inf".into(),
        (false, true) => "-inf".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scientific_matches_printf() {
        assert_eq!(c_scientific(0.0, 6), "0.000000e+00");
        assert_eq!(c_scientific(1.325501e-20, 6), "1.325501e-20");
        assert_eq!(c_scientific(-1e-2, 6), "-1.000000e-02");
        assert_eq!(c_scientific(1.5e300, 3), "1.500e+300");
        assert_eq!(c_scientific(f64::INFINITY, 6), "inf");
    }

    #[test]
    fn general_matches_printf() {
        assert_eq!(c_general(-0.101_010_101_010_101, 7), "-0.1010101");
        assert_eq!(c_general(10.0, 7), "10");
        assert_eq!(c_general(0.0, 7), "0");
        assert_eq!(c_general(1e-5, 7), "1e-05");
        assert_eq!(c_general(1_234_567.0, 7), "1234567");
        assert_eq!(c_general(12_345_678.0, 7), "1.234568e+07");
        assert_eq!(c_general(-2.0, 7), "-2");
        assert_eq!(c_general(0.000_123_456_78, 7), "0.0001234568");
    }
}
