// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/geometry/mesh_sphere_intersector_tmpl.hh
// (the Eigen `isApprox` proximity tests of the v5 code, UD-411).
// Provenance of the rest:
// * `orient2d` / `orient3d` are the exact signs of the textbook orientation
//   determinants, with the sign convention of CGAL's `orientation`
//   (CGAL 5.6.2 Kernel_23, LGPL-3.0-or-later), evaluated on the `MP_Float`
//   port of `crate::number` (CGAL Number_types, LGPL-3.0-or-later);
// * `is_approx` follows the formula of Eigen 3.4's `isApprox`
//   (`Eigen/src/Core/Fuzzy.h`, MPL-2.0), with the summation order that
//   Eigen uses for run-time sized vectors.
// Nothing in this file derives from CGAL's GPL packages (Circular_kernel_3 /
// the spherical kernel, Algebraic_kernel_for_spheres, AABB_tree): D-042 (3).

//! Exact orientation predicates and Eigen-compatible approximate equality
//! (D-022 as amended by D-042).
//!
//! The intersection algorithms themselves follow CGAL's code
//! ([`crate::intersections`] for the inexact Cartesian kernel, LGPL;
//! `akantu-geometry-cgal` for the AABB tree and the exact spherical kernel,
//! GPL-3.0-or-later, D-042). This module keeps the generic helpers:
//!
//! * [`orient2d`] / [`orient3d`]: **exact** signs of the orientation
//!   determinants of double inputs, evaluated in exact `MP_Float`
//!   arithmetic (every difference and product of doubles is representable),
//!   for callers that need robust topology (unlike the inexact
//!   `Simple_cartesian<double>` predicates of [`crate::kernel`]).
//! * [`is_approx`]: Eigen's `a.isApprox(b, prec)` for run-time sized
//!   vectors (`‖a − b‖² ≤ prec² · min(‖a‖², ‖b‖²)`, sums from the first
//!   term). Eigen's default precision for `double` is `1e-12`
//!   ([`EIGEN_DUMMY_PRECISION`]), which is what upstream's v5 sphere
//!   intersector actually uses (UD-411).

use akantu_core::{Real, Result};

use crate::number::MpFloat;

/// Eigen `NumTraits<double>::dummy_precision()`: the default precision of
/// `isApprox`.
pub const EIGEN_DUMMY_PRECISION: Real = 1e-12;

fn mp(x: Real) -> Result<MpFloat> {
    MpFloat::try_from_f64(x)
}

fn sign_i8(m: &MpFloat) -> i8 {
    match m.sign() {
        std::cmp::Ordering::Less => -1,
        std::cmp::Ordering::Equal => 0,
        std::cmp::Ordering::Greater => 1,
    }
}

/// Exact sign of `det[[bx−ax, by−ay], [cx−ax, cy−ay]]` (`+1`:
/// counter-clockwise).
///
/// # Errors
/// Non-finite input.
pub fn orient2d(a: [Real; 2], b: [Real; 2], c: [Real; 2]) -> Result<i8> {
    let (ax, ay) = (mp(a[0])?, mp(a[1])?);
    let bx = mp(b[0])?.sub(&ax);
    let by = mp(b[1])?.sub(&ay);
    let cx = mp(c[0])?.sub(&ax);
    let cy = mp(c[1])?.sub(&ay);
    let det = bx.mul(&cy)?.sub(&by.mul(&cx)?);
    Ok(sign_i8(&det))
}

/// Exact sign of the 3-D orientation determinant of `(b−a, c−a, d−a)`
/// (`+1` for a positively oriented tetrahedron `abcd`, CGAL's convention).
///
/// # Errors
/// Non-finite input.
pub fn orient3d(a: [Real; 3], b: [Real; 3], c: [Real; 3], d: [Real; 3]) -> Result<i8> {
    let mut m = [
        [MpFloat::zero(), MpFloat::zero(), MpFloat::zero()],
        [MpFloat::zero(), MpFloat::zero(), MpFloat::zero()],
        [MpFloat::zero(), MpFloat::zero(), MpFloat::zero()],
    ];
    for (row, p) in [b, c, d].iter().enumerate() {
        for k in 0..3 {
            m[row][k] = mp(p[k])?.sub(&mp(a[k])?);
        }
    }
    let minor = |i: usize, j: usize, k: usize, l: usize| -> Result<MpFloat> {
        Ok(m[1][i].mul(&m[2][j])?.sub(&m[1][k].mul(&m[2][l])?))
    };
    let det = m[0][0]
        .mul(&minor(1, 2, 2, 1)?)?
        .sub(&m[0][1].mul(&minor(0, 2, 2, 0)?)?)
        .add(&m[0][2].mul(&minor(0, 1, 1, 0)?)?);
    Ok(sign_i8(&det))
}

/// Eigen `a.isApprox(b, prec)` for run-time sized vectors:
/// `Σ(aᵢ−bᵢ)² ≤ prec·prec·min(Σaᵢ², Σbᵢ²)` (sequential sums; only an exact
/// zero is approximately equal to zero).
#[must_use]
pub fn is_approx(a: &[Real], b: &[Real], prec: Real) -> bool {
    let (mut d, mut na, mut nb) = (0.0, 0.0, 0.0);
    for (x, y) in a.iter().zip(b) {
        d += (x - y) * (x - y);
        na += x * x;
        nb += y * y;
    }
    // Eigen `numext::mini(x, y)` = `y < x ? y : x`.
    let m = if nb < na { nb } else { na };
    d <= prec * prec * m
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_approx_is_relative_like_eigen() {
        assert!(is_approx(&[1.0, 0.0], &[1.0 + 5e-11, 0.0], 1e-10));
        assert!(
            !is_approx(&[1e-12, 0.0], &[2e-12, 0.0], 1e-10),
            "relative near the origin"
        );
        assert!(
            !is_approx(&[0.0, 0.0], &[1e-100, 0.0], 1e-10),
            "only exact zero matches zero"
        );
        assert!(!is_approx(
            &[1.0, 0.0],
            &[1.0 + 5e-11, 0.0],
            EIGEN_DUMMY_PRECISION
        ));
    }

    #[test]
    fn exact_orientation() {
        assert_eq!(orient2d([0.0, 0.0], [1.0, 0.0], [0.0, 1.0]).unwrap(), 1);
        assert_eq!(orient2d([0.0, 0.0], [0.0, 1.0], [1.0, 0.0]).unwrap(), -1);
        // Nearly collinear points that the double formula misjudges.
        let a = [0.5, 0.5];
        let b = [12.0, 12.0];
        let c = [24.0, 24.0 + 2.0 * f64::EPSILON * 24.0];
        assert_eq!(orient2d(a, b, c).unwrap(), 1);
        assert_eq!(orient2d(a, b, [24.0, 24.0]).unwrap(), 0);
        let o = [0.0; 3];
        assert_eq!(
            orient3d(o, [1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]).unwrap(),
            1
        );
        assert_eq!(
            orient3d(o, [0.0, 1.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]).unwrap(),
            -1
        );
        assert_eq!(
            orient3d(o, [1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.3, 0.3, 0.0]).unwrap(),
            0
        );
        assert!(orient2d([f64::NAN, 0.0], b, c).is_err());
    }
}
