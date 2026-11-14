// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS): small-vector kernels
// with the evaluation order of upstream's Eigen expressions in the cohesive
// laws (src/model/solid_mechanics/solid_mechanics_model_cohesive/materials/*,
// src/common/aka_math.cc `compute_tangents`).

//! Eigen-order helpers for `dim ≤ 3` vectors (D-008, `akantu_core::eigen_order`).
//!
//! Upstream's cohesive laws are templated on the spatial dimension: views
//! `make_view<dim>(…)` are fixed-size, so their reductions (`dot`, `norm`)
//! use Eigen's halving tree (`t0 + (t1 + t2)` in 3-D). Views with a
//! run-time size (`Vector<Real>`, `make_view(array, dim)`, columns of a
//! dynamic `Matrix<Real>`) reduce sequentially from the first term.

use akantu_core::Real;
use akantu_core::eigen_order::{redux_seq, redux_tree};

fn add(a: Real, b: Real) -> Real {
    a + b
}

/// `a.dot(b)` of two fixed-size `dim` vectors (halving tree).
#[inline]
pub fn dot_fixed(a: &[Real], b: &[Real], dim: usize) -> Real {
    redux_tree(0, dim, &|i| a[i] * b[i], add)
}

/// `a.dot(b)` with a run-time size on the left (sequential).
#[inline]
pub fn dot_seq(a: &[Real], b: &[Real], dim: usize) -> Real {
    redux_seq(dim, &|i| a[i] * b[i], add)
}

/// `a.norm()` of a fixed-size vector.
#[inline]
pub fn norm_fixed(a: &[Real], dim: usize) -> Real {
    redux_tree(0, dim, &|i| a[i] * a[i], add).sqrt()
}

/// `a.norm()` of a run-time-size vector.
#[inline]
pub fn norm_seq(a: &[Real], dim: usize) -> Real {
    redux_seq(dim, &|i| a[i] * a[i], add).sqrt()
}

/// `std::max(a, b)`: `(a < b) ? b : a`.
#[inline]
pub fn std_max(a: Real, b: Real) -> Real {
    if a < b { b } else { a }
}

/// `std::min(a, b)`: `(b < a) ? b : a`.
#[inline]
pub fn std_min(a: Real, b: Real) -> Real {
    if b < a { b } else { a }
}

/// Facet tangents of one normal of `dim` components: `out` receives `dim ×
/// (dim − 1)` values, column-major (upstream `Math::compute_tangents`,
/// run-time views; `t1 = n × t0` normalised in 3-D, fixed size 3).
///
/// * `upstream_2d_rotation` (**UD-217** compat): `t0 = normal2d(n)` —
///   `[n1, −n0, n2…]` normalised, or `e0` if `‖n‖ ≈ 0` — exactly as
///   upstream; in 3-D this `t0` is not orthogonal to `n` when `n2 ≠ 0`
///   (`t0·n = n2²/‖·‖`).
/// * default: in 1-D/2-D the same vector (already orthogonal); in 3-D
///   `t0 = (n1, −n0, 0)` normalised (orthogonal to `n`), or `e0` when `n0 =
///   n1 = 0` (`n ∥ e2`, also orthogonal), `e0` if `‖n‖ ≈ 0` as upstream.
pub fn tangents_of(
    n: &[Real],
    dim: usize,
    out: &mut [Real],
    tol: Real,
    upstream_2d_rotation: bool,
) {
    let nb = dim * (dim - 1);
    out[..nb].fill(0.0);
    if nb == 0 {
        return;
    }
    let nn = norm_seq(n, dim);
    if akantu_core::math::are_float_equal(nn, 0.0, tol) {
        out[0] = 1.0;
    } else {
        let mut t = [0.0; 3];
        t[..dim].copy_from_slice(&n[..dim]);
        t[0] = n[1];
        t[1] = -n[0];
        if dim == 3 && !upstream_2d_rotation {
            t[2] = 0.0;
        }
        // `normalize()` of a run-time-size vector
        let z = redux_seq(dim, &|i| t[i] * t[i], add);
        if z > 0.0 {
            let s = z.sqrt();
            for x in t.iter_mut().take(dim) {
                *x /= s;
            }
            out[..dim].copy_from_slice(&t[..dim]);
        } else {
            // n ∥ e2 (corrected 3-D path only: upstream's vector keeps n2)
            out[0] = 1.0;
        }
    }
    if dim == 3 {
        let a = [n[0], n[1], n[2]];
        let b = [out[0], out[1], out[2]];
        let mut c = akantu_fe::eigen::cross(&a, &b);
        // `normalized()` of a fixed-size vector
        let z = redux_tree(0, 3, &|i| c[i] * c[i], add);
        if z > 0.0 {
            let s = z.sqrt();
            for x in &mut c {
                *x /= s;
            }
        }
        out[3..6].copy_from_slice(&c);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tangents(n: &[Real], upstream: bool) -> [Real; 6] {
        let mut out = [0.0; 6];
        tangents_of(n, 3, &mut out, 1e-14, upstream);
        out
    }

    /// `(n, t0, t1)` is orthonormal for every normal,
    /// including `n ∥ e2`; upstream's first tangent is not orthogonal to an
    /// inclined normal.
    #[test]
    fn tangents_are_orthonormal_to_the_normal() {
        let mut normals = vec![
            [0., 0.6, 0.8],
            [0., 0., 1.],
            [0., 0., -1.],
            [1., 0., 0.],
            [0., -1., 0.],
        ];
        for k in 0..40 {
            let a = 0.37 * k as Real;
            let b = 0.11 * k as Real - 2.;
            normals.push([a.cos() * b.cos(), a.sin() * b.cos(), b.sin()]);
        }
        for n in normals {
            let t = tangents(&n, false);
            let (t0, t1) = (&t[0..3], &t[3..6]);
            let dot = |a: &[Real], b: &[Real]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
            assert!(dot(&n, t0).abs() < 1e-15, "{n:?}: n·t0 = {}", dot(&n, t0));
            assert!(dot(&n, t1).abs() < 1e-15, "{n:?}: n·t1");
            assert!(dot(t0, t1).abs() < 1e-15, "{n:?}: t0·t1");
            assert!((dot(t0, t0) - 1.).abs() < 1e-15, "{n:?}: |t0|");
            assert!((dot(t1, t1) - 1.).abs() < 1e-15, "{n:?}: |t1|");
        }
        // example: upstream t0 = (0.6, 0, 0.8), t0·n = 0.64
        let up = tangents(&[0., 0.6, 0.8], true);
        assert_eq!(&up[0..3], &[0.6, 0., 0.8]);
        let ours = tangents(&[0., 0.6, 0.8], false);
        assert_eq!(&ours[0..3], &[1., 0., 0.]);
    }

    /// 1-D/2-D tangents and 3-D normals with `n2 = 0` are unchanged by the
    /// correction (up to the sign of a zero third component).
    #[test]
    fn tangents_unchanged_where_upstream_is_orthogonal() {
        let mut a = [0.0; 2];
        let mut b = [0.0; 2];
        tangents_of(&[0.6, 0.8], 2, &mut a, 1e-14, true);
        tangents_of(&[0.6, 0.8], 2, &mut b, 1e-14, false);
        assert_eq!(a, b);
        let n = [0.28, -0.96, 0.0];
        assert_eq!(tangents(&n, true), tangents(&n, false));
    }
}
