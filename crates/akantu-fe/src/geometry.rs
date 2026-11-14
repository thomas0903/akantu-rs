// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/common/aka_math_tmpl.hh
// (Math::normal, triangle_inradius, tetrahedron_volume, tetrahedron_inradius),
// src/fe_engine/element_classes/element_class_*_inline_impl.hh (getInradius),
// src/fe_engine/element_class_tmpl.hh (GeometricalShapeContains).

//! Geometric helpers of the element classes (FE-004, FE-008, FE-010).
//!
//! Node coordinates are column-major `sdim × nb_nodes` slices; upstream
//! passes them as **dynamic** matrices (`Matrix<Real>` in
//! `FEEngine::getElementInradius`, dynamic `Map`s in the probes), so every
//! column difference norm is a run-time-size reduction (sequential from the
//! first term). `std::min(a, b)` is `b < a ? b : a` and
//! `std::min({…})` keeps the first smallest value; both are reproduced
//! ([`cpp_min`], [`cpp_min_list`]) so that ties and signed zeros behave as
//! upstream.

use akantu_core::Real;
use akantu_core::element_type::{GeometricalShapeType, GeometricalType};
use akantu_core::tensor::{Matrix, SquareMatrixExt};

use crate::eigen;

/// `std::min(a, b)` (returns `a` unless `b < a`).
#[inline]
#[must_use]
pub fn cpp_min(a: Real, b: Real) -> Real {
    if b < a { b } else { a }
}

/// `std::min({…})` (first smallest value, `std::min_element` semantics).
#[inline]
#[must_use]
pub fn cpp_min_list(values: &[Real]) -> Real {
    let mut m = values[0];
    for &v in &values[1..] {
        if v < m {
            m = v;
        }
    }
    m
}

/// Column `c` of a column-major `sdim × n` slice.
#[inline]
fn col(x: &[Real], sdim: usize, c: usize) -> &[Real] {
    &x[c * sdim..(c + 1) * sdim]
}

/// `a.distance(b)` = `(a − b).norm()` for run-time-size columns.
#[inline]
#[must_use]
pub fn distance(a: &[Real], b: &[Real]) -> Real {
    eigen::norm(a.len(), false, |i| a[i] - b[i])
}

/// Upstream `Math::triangle_inradius(c1, c2, c3)`.
#[must_use]
pub fn triangle_inradius(c1: &[Real], c2: &[Real], c3: &[Real]) -> Real {
    let a = distance(c1, c2);
    let b = distance(c2, c3);
    let c = distance(c1, c3);
    let s = (a + b + c) / 2.;
    ((s - a) * (s - b) * (s - c) / s).sqrt()
}

/// Upstream `Math::tetrahedron_volume(c1, c2, c3, c4)` (three-column
/// replacement in a fixed 3×3 matrix, Eigen closed-form determinants).
#[must_use]
pub fn tetrahedron_volume(c1: &[Real], c2: &[Real], c3: &[Real], c4: &[Real]) -> Real {
    let set = |m: &mut Matrix<3, 3>, j: usize, v: &[Real]| {
        for (i, mij) in m[j].iter_mut().enumerate() {
            *mij = v.get(i).copied().unwrap_or(0.0);
        }
    };
    let mut xx: Matrix<3, 3> = [[0.0; 3]; 3];
    set(&mut xx, 0, c2);
    set(&mut xx, 1, c3);
    set(&mut xx, 2, c4);
    let mut vol = xx.det();
    set(&mut xx, 0, c1);
    vol -= xx.det();
    set(&mut xx, 1, c2);
    vol += xx.det();
    set(&mut xx, 2, c3);
    vol -= xx.det();
    vol /= 6.;
    vol
}

/// Upstream `Math::tetrahedron_inradius(c1, c2, c3, c4)`.
#[must_use]
pub fn tetrahedron_inradius(c1: &[Real], c2: &[Real], c3: &[Real], c4: &[Real]) -> Real {
    let l12 = distance(c1, c2);
    let l13 = distance(c1, c3);
    let l14 = distance(c1, c4);
    let l23 = distance(c2, c3);
    let l24 = distance(c2, c4);
    let l34 = distance(c3, c4);
    let mut s1 = (l12 + l23 + l13) * 0.5;
    s1 = (s1 * (s1 - l12) * (s1 - l23) * (s1 - l13)).sqrt();
    let mut s2 = (l12 + l24 + l14) * 0.5;
    s2 = (s2 * (s2 - l12) * (s2 - l24) * (s2 - l14)).sqrt();
    let mut s3 = (l23 + l34 + l24) * 0.5;
    s3 = (s3 * (s3 - l23) * (s3 - l34) * (s3 - l24)).sqrt();
    let mut s4 = (l13 + l34 + l14) * 0.5;
    s4 = (s4 * (s4 - l13) * (s4 - l34) * (s4 - l14)).sqrt();
    let volume = tetrahedron_volume(c1, c2, c3, c4);
    3. * volume / (s1 + s2 + s3 + s4)
}

/// Upstream `GeometricalElement<geometrical_type>::getInradius(X)`
/// ("characteristic length", FE-010); `0` for point and cohesive
/// geometries (upstream default). `x`: column-major `sdim × nb_nodes`.
#[must_use]
pub fn inradius(gt: GeometricalType, x: &[Real], nb_nodes: usize) -> Real {
    if nb_nodes == 0 {
        return 0.0;
    }
    let sdim = x.len() / nb_nodes;
    let u = |c: usize| col(x, sdim, c);
    let d = |a: usize, b: usize| distance(u(a), u(b));
    use GeometricalType as G;
    match gt {
        G::Segment2 => distance(u(1), u(0)),
        G::Segment3 => {
            let dist1 = distance(u(1), u(0));
            let dist2 = distance(u(2), u(1));
            cpp_min(dist1, dist2)
        }
        G::Triangle3 => 2. * triangle_inradius(u(0), u(1), u(2)),
        G::Triangle6 => {
            const TRIANGLES: [[usize; 3]; 4] = [[0, 3, 5], [3, 1, 4], [3, 4, 5], [5, 4, 2]];
            let mut r = Real::MAX;
            for t in TRIANGLES {
                let ir = triangle_inradius(u(t[0]), u(t[1]), u(t[2]));
                r = cpp_min(ir, r);
            }
            2. * r
        }
        G::Quadrangle4 => {
            let a = d(0, 1);
            let b = d(1, 2);
            let c = d(2, 3);
            let e = d(3, 0);
            cpp_min_list(&[a, b, c, e])
        }
        G::Quadrangle8 => {
            let mut h = cpp_min(d(0, 4), d(4, 1));
            h = cpp_min(h, cpp_min(d(1, 5), d(5, 2)));
            h = cpp_min(h, cpp_min(d(2, 6), d(6, 3)));
            cpp_min(h, cpp_min(d(3, 7), d(7, 0)))
        }
        G::Tetrahedron4 => 2. * tetrahedron_inradius(u(0), u(1), u(2), u(3)),
        G::Tetrahedron10 => {
            const TETRAHEDRA: [[usize; 4]; 4] =
                [[0, 4, 6, 7], [4, 1, 5, 8], [6, 5, 2, 9], [7, 8, 9, 3]];
            let mut r = Real::MAX;
            for t in TETRAHEDRA {
                let ir = tetrahedron_inradius(u(t[0]), u(t[1]), u(t[2]), u(t[3]));
                r = cpp_min(ir, r);
            }
            2. * r
        }
        G::Pentahedron6 => pentahedron6_inradius(x, sdim),
        G::Pentahedron15 => pentahedron6_inradius(x, sdim) * 0.5,
        G::Hexahedron8 => hexahedron8_inradius(x, sdim),
        G::Hexahedron20 => hexahedron8_inradius(x, sdim) * 0.5,
        _ => 0.0,
    }
}

fn pentahedron6_inradius(x: &[Real], sdim: usize) -> Real {
    let u = |c: usize| col(x, sdim, c);
    let t1 = triangle_inradius(u(0), u(1), u(2));
    let t2 = triangle_inradius(u(3), u(4), u(5));
    let d1 = distance(u(3), u(0)) * 0.5;
    let d2 = distance(u(5), u(2)) * 0.5;
    let d3 = distance(u(4), u(1)) * 0.5;
    2. * cpp_min_list(&[t1, t2, d1, d2, d3])
}

fn hexahedron8_inradius(x: &[Real], sdim: usize) -> Real {
    let d = |a: usize, b: usize| distance(col(x, sdim, a), col(x, sdim, b));
    cpp_min_list(&[
        d(0, 1),
        d(1, 2),
        d(2, 3),
        d(3, 0),
        d(0, 4),
        d(1, 5),
        d(2, 6),
        d(3, 7),
        d(4, 5),
        d(5, 6),
        d(6, 7),
        d(7, 4),
    ])
}

/// Upstream `GeometricalShapeContains<shape>::contains(ξ)` (FE-004) with
/// `Math::tolerance = tolerance` for simplices:
/// * point (also every cohesive geometry, upstream default shape):
///   `ξ₀ < DBL_EPSILON` (an empty `ξ` is treated as the zero vector upstream
///   callers pass);
/// * square: `|ξ_i| ≤ 1 + DBL_EPSILON` for all `i`;
/// * triangle: `ξ_i ∈ [−tol, 1 + tol]` and `Σξ ≤ 1 + tol`;
/// * prism: `ξ₀ ∈ [−1, 1]`, `ξ₁, ξ₂ ∈ [0, 1]`, `ξ₁ + ξ₂ ≤ 1` (no tolerance).
#[must_use]
pub fn contains(shape: GeometricalShapeType, xi: &[Real], tolerance: Real) -> bool {
    let eps = Real::EPSILON;
    match shape {
        GeometricalShapeType::Point => xi.first().copied().unwrap_or(0.0) < eps,
        GeometricalShapeType::Square => {
            let mut inside = true;
            let mut i = 0;
            while i < xi.len() && inside {
                inside &= xi[i] >= -(1. + eps) && xi[i] <= 1. + eps;
                i += 1;
            }
            inside
        }
        GeometricalShapeType::Triangle => {
            let mut inside = true;
            let mut sum = 0.0;
            let mut i = 0;
            while i < xi.len() && inside {
                inside &= xi[i] >= -tolerance && xi[i] <= 1. + tolerance;
                sum += xi[i];
                i += 1;
            }
            if inside {
                return sum <= 1. + tolerance;
            }
            inside
        }
        GeometricalShapeType::Prism => {
            let mut inside = xi[0] >= -1. && xi[0] <= 1.;
            inside &= xi[1] >= 0. && xi[1] <= 1.;
            inside &= xi[2] >= 0. && xi[2] <= 1.;
            let sum = xi[1] + xi[2];
            inside && sum <= 1.
        }
        GeometricalShapeType::NotDefined => false,
    }
}

/// Upstream `Math::normal(vec)` for a 2-vector: `(v₁, −v₀)` normalised as a
/// **dynamic** vector (sequential squared norm, division by the norm).
#[must_use]
pub fn normal_2d(v: &[Real]) -> [Real; 2] {
    let mut n = [v[1], -v[0]];
    eigen::normalize(&mut n, false);
    n
}

/// Upstream `Math::normal(v1, v2)` for 3-vectors: `normalized(v1 × v2)` on a
/// fixed `Vector<Real, 3>` (halving-tree squared norm).
#[must_use]
pub fn normal_3d(v1: &[Real], v2: &[Real]) -> [Real; 3] {
    let a = [v1[0], v1[1], v1[2]];
    let b = [v2[0], v2[1], v2[2]];
    let mut n = eigen::cross(&a, &b);
    eigen::normalize(&mut n, true);
    n
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn min_semantics() {
        assert_eq!(cpp_min(1.0, 2.0), 1.0);
        assert!(cpp_min(0.0, -0.0).is_sign_positive());
        assert!(cpp_min(-0.0, 0.0).is_sign_negative());
        assert!(cpp_min_list(&[0.0, -0.0, 1.0]).is_sign_positive());
    }

    #[test]
    fn simplex_measures() {
        let o = [0.0, 0.0, 0.0];
        let ex = [1.0, 0.0, 0.0];
        let ey = [0.0, 1.0, 0.0];
        let ez = [0.0, 0.0, 1.0];
        assert!((tetrahedron_volume(&o, &ex, &ey, &ez) - 1.0 / 6.0).abs() < 1e-16);
        // right isosceles triangle with legs 1: r = (a + b − c)/2
        let r = triangle_inradius(&o[..2], &ex[..2], &ey[..2]);
        assert!((r - (2.0 - 2f64.sqrt()) / 2.0).abs() < 1e-15);
    }

    #[test]
    fn containment() {
        use GeometricalShapeType as S;
        let tol = akantu_core::math::DEFAULT_TOLERANCE;
        assert!(contains(S::Square, &[1.0, -1.0], tol));
        assert!(!contains(S::Square, &[1.0000001, 0.0], tol));
        assert!(contains(S::Triangle, &[0.5, 0.5], tol));
        assert!(!contains(S::Triangle, &[0.6, 0.5], tol));
        assert!(contains(S::Prism, &[1.0, 0.5, 0.5], tol));
        assert!(!contains(S::Prism, &[1.0, -1e-300, 0.5], tol));
        assert!(contains(S::Point, &[0.0], tol));
        assert!(!contains(S::Point, &[1e-15], tol));
    }
}
