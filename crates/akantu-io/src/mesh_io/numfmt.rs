// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Reproduces the text
// produced by `std::ostream << double` with `std::setprecision(p)` in the
// default float field (C `printf("%.*g")`, `%.*G` with `std::uppercase`),
// as used by src/io/mesh_io/mesh_io_msh.cc (`MeshIOMSH::write`). Written from
// the C standard's description of `%g`; no C library code is used.

//! `%g`-style formatting of `f64`.
//!
//! C11 §7.21.6.1: with precision `P` (0 → 1) and `X` the decimal exponent
//! of the value rounded to `P` significant digits, `%g` uses fixed notation
//! with `P − 1 − X` decimals if `P > X ≥ −4`, exponential notation with
//! `P − 1` decimals otherwise; trailing zeros (and a trailing decimal
//! point) are removed; the exponent has a sign and at least two digits.
//! Rust's `{:.N}`/`{:.Ne}` formatting is exact (correctly rounded,
//! ties-to-even on the exact binary value), like glibc's `printf`.

/// Format `x` like C `printf("%.{precision}g")` (`%G` if `upper`).
#[must_use]
pub(crate) fn format_g(x: f64, precision: usize, upper: bool) -> String {
    let mut s = String::new();
    crate::dumper::cfmt::push_g(&mut s, x, precision, upper);
    s
}

#[cfg(test)]
mod tests {
    use super::format_g;

    #[test]
    fn matches_printf_g() {
        // Expected strings follow C11 §7.21.6.1 (checked against glibc
        // printf for these values).
        let cases: &[(f64, usize, bool, &str)] = &[
            (0.0, 15, true, "0"),
            (-0.0, 17, false, "-0"),
            (1.0, 15, true, "1"),
            (0.5, 15, true, "0.5"),
            (0.1, 15, true, "0.1"),
            (0.1, 17, true, "0.10000000000000001"),
            (1.0 / 3.0, 17, false, "0.33333333333333331"),
            (1.0 / 3.0, 15, false, "0.333333333333333"),
            (1e-5, 15, true, "1E-05"),
            (1.25e-5, 17, false, "1.2500000000000001e-05"),
            (0.0001, 15, true, "0.0001"),
            (123456.0, 6, false, "123456"),
            (1234567.0, 6, false, "1.23457e+06"),
            (1e15, 15, true, "1E+15"),
            (1e14, 15, true, "100000000000000"),
            (-2.5e100, 17, true, "-2.4999999999999999E+100"),
            (1e23, 17, false, "9.9999999999999992e+22"),
            (5e-324, 17, false, "4.9406564584124654e-324"),
            (f64::MAX, 17, false, "1.7976931348623157e+308"),
            (0.999_999_999_999_999_9, 15, false, "1"),
            (999_999.5, 6, false, "1e+06"),
        ];
        for &(x, p, up, want) in cases {
            assert_eq!(format_g(x, p, up), want, "x = {x:e}, p = {p}");
        }
    }

    #[test]
    fn seventeen_digits_round_trip() {
        let mut x = 1.0_f64;
        for i in 0..2000 {
            x = x * 1.000_123_456_789 + f64::from(i) * 1e-7 - 0.5 * x * (i % 3 == 0) as u8 as f64;
            for v in [x, -x, x * 1e-200, x * 1e250, 1.0 / (x + 1.0)] {
                let s = format_g(v, 17, true);
                assert_eq!(s.parse::<f64>().unwrap().to_bits(), v.to_bits(), "{s}");
            }
        }
    }
}
