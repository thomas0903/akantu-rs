// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). This file has no
// direct C++ counterpart: it provides the NaN-propagating reductions that
// test pass conditions use.

//! NaN-propagating maxima and minima for pass conditions.
//!
//! [`f64::max`] / [`f64::min`] return the non-NaN operand when one operand
//! is NaN (IEEE 754 `maxNum` / `minNum`), so `values.fold(0.0, f64::max)`
//! **skips** NaN: an
//! all-NaN difference reduces to `0.0` and satisfies `worst < tol`. Such
//! checks existed on oracle and patch data (the FD replay's
//! "at rest" check passed with 21 NaN displacements). A check whose
//! reduction may see a NaN uses these functions instead: a NaN anywhere
//! makes the result NaN, and every comparison `NaN < tol` / `NaN <= tol`
//! (and `NaN > lower`) is `false`, so the check fails.
//!
//! `f64::maximum` / `f64::minimum` (IEEE 754-2019) would do the same but
//! are not stable on the pinned toolchain.

/// Larger of `a` and `b`; NaN if either is NaN (the first NaN operand is
/// returned, so its bits are kept). On equal operands (`+0`/`−0`
/// included) `a` is returned.
#[must_use]
pub fn nan_max(a: f64, b: f64) -> f64 {
    if a.is_nan() {
        a
    } else if b.is_nan() || b > a {
        b
    } else {
        a
    }
}

/// Smaller of `a` and `b`; NaN if either is NaN (the first NaN operand is
/// returned). On equal operands (`+0`/`−0` included) `a` is returned.
#[must_use]
pub fn nan_min(a: f64, b: f64) -> f64 {
    if a.is_nan() {
        a
    } else if b.is_nan() || b < a {
        b
    } else {
        a
    }
}

/// Method forms of [`nan_max`] / [`nan_min`], so that `worst =
/// worst.max(d)` becomes `worst = worst.nan_max(d)` and `.fold(0.0,
/// f64::max)` becomes `.fold(0.0, NanMinMax::nan_max)`.
pub trait NanMinMax {
    /// See [`nan_max`].
    #[must_use]
    fn nan_max(self, other: Self) -> Self;
    /// See [`nan_min`].
    #[must_use]
    fn nan_min(self, other: Self) -> Self;
}

impl NanMinMax for f64 {
    fn nan_max(self, other: f64) -> f64 {
        nan_max(self, other)
    }
    fn nan_min(self, other: f64) -> f64 {
        nan_min(self, other)
    }
}

/// Largest absolute value (`0.0` for an empty input); NaN if any value is
/// NaN.
#[must_use]
pub fn max_abs<I: IntoIterator<Item = f64>>(values: I) -> f64 {
    values.into_iter().fold(0.0, |m, x| nan_max(m, x.abs()))
}

/// Largest `|a[i] − b[i]|` (`0.0` for empty inputs); NaN if any difference
/// is NaN (a NaN on either side, or `∞ − ∞`), and `+∞` if the lengths
/// differ, so that no tolerance accepts a length mismatch.
#[must_use]
pub fn max_abs_diff(a: &[f64], b: &[f64]) -> f64 {
    if a.len() != b.len() {
        return f64::INFINITY;
    }
    max_abs(a.iter().zip(b).map(|(x, y)| x - y))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The defect this module exists for: `f64::max` skips NaN.
    #[test]
    fn std_max_skips_nan_and_nan_max_does_not() {
        let v = [0.0, f64::NAN, 1e-16];
        assert_eq!(v.iter().copied().fold(0.0, f64::max), 1e-16);
        assert!(v.iter().copied().fold(0.0, NanMinMax::nan_max).is_nan());
        // a NaN stays once seen, whatever follows
        let mut worst = 0.0f64;
        for d in [f64::NAN, 1.0, 0.5, f64::INFINITY] {
            worst = worst.nan_max(d);
        }
        assert!(worst.is_nan());
        // and the usual pass condition then fails
        let (passes, passes_inf) = (worst < 1e-10, worst <= f64::INFINITY);
        assert!(!passes && !passes_inf);
    }

    #[test]
    fn nan_max_on_numbers_is_the_maximum() {
        assert_eq!(nan_max(1.0, 2.0), 2.0);
        assert_eq!(nan_max(2.0, 1.0), 2.0);
        assert_eq!(nan_max(-3.0, f64::NEG_INFINITY), -3.0);
        assert_eq!(nan_max(1.0, f64::INFINITY), f64::INFINITY);
        // equal operands: the first is kept (signed zeros)
        assert_eq!(nan_max(-0.0, 0.0).to_bits(), (-0.0f64).to_bits());
        assert_eq!(nan_max(0.0, -0.0).to_bits(), 0.0f64.to_bits());
    }

    #[test]
    fn nan_min_propagates_nan() {
        let v = [1.0, f64::NAN, 0.5];
        assert_eq!(v.iter().copied().fold(f64::INFINITY, f64::min), 0.5);
        assert!(
            v.iter()
                .copied()
                .fold(f64::INFINITY, NanMinMax::nan_min)
                .is_nan()
        );
        assert_eq!(nan_min(1.0, 2.0), 1.0);
        assert_eq!(nan_min(2.0, f64::NEG_INFINITY), f64::NEG_INFINITY);
        assert_eq!(nan_min(0.0, -0.0).to_bits(), 0.0f64.to_bits());
        let lowest = [2.0, f64::NAN].into_iter().fold(f64::INFINITY, nan_min);
        let positive = lowest > 0.0;
        assert!(!positive);
    }

    #[test]
    fn nan_max_keeps_the_first_nan() {
        let p = f64::from_bits(0x7ff8_0000_0000_0001);
        let q = f64::from_bits(0xfff8_0000_0000_0000);
        assert_eq!(nan_max(p, q).to_bits(), p.to_bits());
        assert_eq!(nan_max(1.0, q).to_bits(), q.to_bits());
        assert_eq!(nan_max(q, 1.0).to_bits(), q.to_bits());
    }

    #[test]
    fn max_abs_and_max_abs_diff() {
        assert_eq!(max_abs([]), 0.0);
        assert_eq!(max_abs([1.0, -3.0, 2.0]), 3.0);
        assert!(max_abs([1.0, f64::NAN, 2.0]).is_nan());
        assert_eq!(max_abs_diff(&[1.0, 2.0], &[1.5, 2.0]), 0.5);
        assert!(max_abs_diff(&[1.0, 2.0], &[1.0, f64::NAN]).is_nan());
        assert!(max_abs_diff(&[f64::INFINITY], &[f64::INFINITY]).is_nan());
        assert_eq!(max_abs_diff(&[1.0], &[1.0, 2.0]), f64::INFINITY);
        assert_eq!(max_abs_diff(&[], &[]), 0.0);
    }
}
