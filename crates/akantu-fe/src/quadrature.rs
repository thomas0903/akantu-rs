// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/fe_engine/gauss_integration.cc,
// gauss_integration_tmpl.hh, element_class.hh (GaussIntegrationElement),
// integrator_gauss.hh.

//! Gauss quadrature rules (QUAD-001…019).
//!
//! The raw tables of `gauss_integration.cc` are transcribed **with their
//! expression form** (`-1./sqrt(3.)`, `(18. - sqrt(30.))/36.`, …) and
//! evaluated with the same IEEE operations (`f64::sqrt` is correctly rounded,
//! like `std::sqrt`), so the values are bit-identical to upstream. The
//! tensor-product (segment / quadrangle / hexahedron) and prism
//! (segment × triangle) rules reproduce upstream's point ordering and weight
//! products (`GaussIntegrationTypeDataHelper`):
//!
//! * tensor rules: first natural coordinate fastest, weights
//!   `((1·w_x)·w_y)·w_z`;
//! * prism rules: outer loop over the segment points (axis `x`), inner loop
//!   over the triangle points, weight `w_seg · w_tri`.
//!
//! The explicit 6/8/21-point pentahedron tables of `gauss_integration.cc`
//! are dead code upstream (never referenced; UD-056) and are not ported.
//!
//! Supported orders (as upstream, where other orders do not compile):
//! point: any; segment family: 1–7 (1–4 points per direction); triangle,
//! tetrahedron, pentahedron: 1–5.

use akantu_core::element_type::GaussIntegrationType;
use akantu_core::tensor::MatrixX;
use akantu_core::{ElementType, Error, Real, Result};

/// A quadrature rule on the reference element.
#[derive(Clone, Debug, PartialEq)]
pub struct QuadratureRule {
    /// Points, column-major `max(1, natural_dim) × nb_points` (upstream
    /// `getQuadraturePoints`; at least one row, as upstream uses
    /// `max(1, dim)` rows, the point rule has one row holding `0`).
    pub points: MatrixX,
    /// Weights, one per point.
    pub weights: Vec<Real>,
}

impl QuadratureRule {
    /// Number of points.
    #[must_use]
    pub fn nb_points(&self) -> usize {
        self.weights.len()
    }

    /// Natural coordinates of point `q` (`natural_dim` values; empty for the
    /// point rule of natural dimension 0).
    #[must_use]
    pub fn point(&self, q: usize, natural_dim: usize) -> &[Real] {
        let rows = self.points.rows();
        &self.points.as_slice()[q * rows..q * rows + natural_dim]
    }
}

/// One-dimensional Gauss–Legendre data (`GaussIntegrationTypeData<_git_segment, n>`).
fn segment_data(nb_points: usize) -> Option<(Vec<Real>, Vec<Real>)> {
    let s = f64::sqrt;
    Some(match nb_points {
        1 => (vec![0.], vec![2.]),
        2 => (vec![-1. / s(3.), 1. / s(3.)], vec![1., 1.]),
        3 => (
            vec![-s(3. / 5.), 0., s(3. / 5.)],
            vec![5. / 9., 8. / 9., 5. / 9.],
        ),
        4 => (
            vec![
                -s((3. + 2. * s(6. / 5.)) / 7.),
                -s((3. - 2. * s(6. / 5.)) / 7.),
                s((3. - 2. * s(6. / 5.)) / 7.),
                s((3. + 2. * s(6. / 5.)) / 7.),
            ],
            vec![
                (18. - s(30.)) / 36.,
                (18. + s(30.)) / 36.,
                (18. + s(30.)) / 36.,
                (18. - s(30.)) / 36.,
            ],
        ),
        _ => return None,
    })
}

/// Triangle data (`GaussIntegrationTypeData<_git_triangle, n>`), positions
/// point by point (`x0, y0, x1, y1, …`).
fn triangle_data(nb_points: usize) -> Option<(Vec<Real>, Vec<Real>)> {
    let s = f64::sqrt;
    Some(match nb_points {
        1 => (vec![1. / 3., 1. / 3.], vec![1. / 2.]),
        3 => (
            vec![1. / 6., 1. / 6., 2. / 3., 1. / 6., 1. / 6., 2. / 3.],
            vec![1. / 6., 1. / 6., 1. / 6.],
        ),
        4 => (
            vec![
                1. / 5.,
                1. / 5.,
                3. / 5.,
                1. / 5.,
                1. / 5.,
                3. / 5.,
                1. / 3.,
                1. / 3.,
            ],
            vec![
                25. / (24. * 4.),
                25. / (24. * 4.),
                25. / (24. * 4.),
                -27. / (24. * 4.),
            ],
        ),
        6 => {
            let a = (8. - s(10.) + s(38. - 44. * s(2. / 5.))) / 18.;
            let b = (8. - s(10.) - s(38. - 44. * s(2. / 5.))) / 18.;
            let w1 = (620. - s(213125. - 53320. * s(10.))) / 7440.;
            let w2 = (620. + s(213125. - 53320. * s(10.))) / 7440.;
            (
                vec![
                    b,
                    b,
                    1. - 2. * b,
                    b,
                    b,
                    1. - 2. * b,
                    a,
                    1. - 2. * a,
                    a,
                    a,
                    1. - 2. * a,
                    a,
                ],
                vec![w1, w1, w1, w2, w2, w2],
            )
        }
        7 => {
            let a = (6. + s(15.)) / 21.;
            let b = (6. - s(15.)) / 21.;
            let w1 = (155. + s(15.)) / 2400.;
            let w2 = (155. - s(15.)) / 2400.;
            (
                vec![
                    1. / 3.,
                    1. / 3.,
                    a,
                    a,
                    1. - 2. * a,
                    a,
                    a,
                    1. - 2. * a,
                    b,
                    b,
                    1. - 2. * b,
                    b,
                    b,
                    1. - 2. * b,
                ],
                vec![9. / 80., w1, w1, w1, w2, w2, w2],
            )
        }
        _ => return None,
    })
}

/// Tetrahedron data (`GaussIntegrationTypeData<_git_tetrahedron, n>`).
fn tetrahedron_data(nb_points: usize) -> Option<(Vec<Real>, Vec<Real>)> {
    let s = f64::sqrt;
    Some(match nb_points {
        1 => (vec![1. / 4., 1. / 4., 1. / 4.], vec![1. / 6.]),
        4 => {
            let a = (5. - s(5.)) / 20.;
            let b = (5. + 3. * s(5.)) / 20.;
            (
                vec![a, a, a, b, a, a, a, b, a, a, a, b],
                vec![1. / 24., 1. / 24., 1. / 24., 1. / 24.],
            )
        }
        5 => (
            vec![
                1. / 4.,
                1. / 4.,
                1. / 4.,
                1. / 6.,
                1. / 6.,
                1. / 6.,
                1. / 6.,
                1. / 6.,
                1. / 2.,
                1. / 6.,
                1. / 2.,
                1. / 6.,
                1. / 2.,
                1. / 6.,
                1. / 6.,
            ],
            vec![-2. / 15., 3. / 40., 3. / 40., 3. / 40., 3. / 40.],
        ),
        15 => {
            let a = (7. + s(15.)) / 34.;
            let b = (13. - 3. * s(15.)) / 34.;
            let c = (7. - s(15.)) / 34.;
            let d = (13. + 3. * s(15.)) / 34.;
            let e = (5. - s(15.)) / 20.;
            let f = (5. + s(15.)) / 20.;
            let w1 = (2665. - 14. * s(15.)) / 226800.;
            let w2 = (2665. + 14. * s(15.)) / 226800.;
            let w3 = 5. / 567.;
            #[rustfmt::skip]
            let p = vec![
                1. / 4., 1. / 4., 1. / 4.,
                a, a, a,
                a, a, b,
                a, b, a,
                b, a, a,
                c, c, c,
                c, c, d,
                c, d, c,
                d, c, c,
                e, e, f,
                e, f, e,
                f, e, e,
                e, f, f,
                f, e, f,
                f, f, e,
            ];
            (
                p,
                vec![
                    8. / 405.,
                    w1,
                    w1,
                    w1,
                    w1,
                    w2,
                    w2,
                    w2,
                    w2,
                    w3,
                    w3,
                    w3,
                    w3,
                    w3,
                    w3,
                ],
            )
        }
        _ => return None,
    })
}

/// Points of a segment rule of order `n` (QUAD-001:
/// `(n+1)/2 + ((n+1)%2 ? 1 : 0)`).
#[must_use]
pub const fn segment_nb_points(order: usize) -> usize {
    // upstream: (n + 1) / 2 + (bool((n + 1) % 2) ? 1 : 0)
    order.div_ceil(2) + if order.is_multiple_of(2) { 1 } else { 0 }
}

/// Points of a triangle rule of order `n` (1→1, 2→3, 3→4, 4→6, 5→7).
const fn triangle_nb_points(order: usize) -> Option<usize> {
    match order {
        1 => Some(1),
        2 => Some(3),
        3 => Some(4),
        4 => Some(6),
        5 => Some(7),
        _ => None,
    }
}

/// Points of a tetrahedron rule of order `n` (1→1, 2→4, 3→5, 4,5→15).
const fn tetrahedron_nb_points(order: usize) -> Option<usize> {
    match order {
        1 => Some(1),
        2 => Some(4),
        3 => Some(5),
        4 | 5 => Some(15),
        _ => None,
    }
}

/// `(x_order, yz_order)` of the pentahedron rule of order `n`
/// (`DECLARE_GAUSS_NB_POINTS_PENT`).
const fn pentahedron_orders(order: usize) -> Option<(usize, usize)> {
    match order {
        1 | 2 => Some((3, 2)),
        3 => Some((3, 3)),
        4 | 5 => Some((5, 5)),
        _ => None,
    }
}

fn unsupported(ty: ElementType, order: usize) -> Error {
    Error::Unsupported(format!(
        "no Gauss rule of order {order} for {} (upstream rules: segment family 1..7, \
         triangle/tetrahedron/pentahedron 1..5)",
        ty.name()
    ))
}

/// Tensor-product rule of `dim` directions (`GaussIntegrationTypeDataHelper<_git_segment>`).
fn tensor_rule(dim: usize, order: usize) -> Option<QuadratureRule> {
    let nquad = segment_nb_points(order);
    let (pos, w) = segment_data(nquad)?;
    let tot = nquad.pow(dim as u32);
    let mut points = vec![0.0; dim * tot];
    let mut weights = vec![1.0; tot];
    let mut offset = 1;
    for d in 0..dim {
        let mut q = 0;
        for n in 0..tot {
            let rq = q % tot + q / tot;
            points[d + rq * dim] = pos[n % nquad];
            weights[rq] *= w[n % nquad];
            q += offset;
        }
        offset *= nquad;
    }
    Some(QuadratureRule {
        points: MatrixX::from_col_major(dim, tot, points).ok()?,
        weights,
    })
}

/// Plain table rule (`GaussIntegrationTypeDataHelper` generic case).
fn table_rule(rows: usize, (pos, w): (Vec<Real>, Vec<Real>)) -> Option<QuadratureRule> {
    let n = w.len();
    Some(QuadratureRule {
        points: MatrixX::from_col_major(rows, n, pos).ok()?,
        weights: w,
    })
}

/// Prism rule (`GaussIntegrationTypeDataHelper<_git_pentahedron>`).
fn pentahedron_rule(order: usize) -> Option<QuadratureRule> {
    let (xo, yzo) = pentahedron_orders(order)?;
    let (pseg, wseg) = segment_data(segment_nb_points(xo))?;
    let (ptri, wtri) = triangle_data(triangle_nb_points(yzo)?)?;
    let (ns, nt) = (wseg.len(), wtri.len());
    let mut points = Vec::with_capacity(3 * ns * nt);
    let mut weights = Vec::with_capacity(ns * nt);
    for s in 0..ns {
        for t in 0..nt {
            points.extend_from_slice(&[pseg[s], ptri[2 * t], ptri[2 * t + 1]]);
            weights.push(wseg[s] * wtri[t]);
        }
    }
    Some(QuadratureRule {
        points: MatrixX::from_col_major(3, ns * nt, points).ok()?,
        weights,
    })
}

/// Rule of `ty` at integration order `order` (upstream
/// `GaussIntegrationElement<type, order>`); `Error::Unsupported` for orders
/// upstream does not provide, `Error::UnsupportedElementType` for types
/// without a Gauss family.
pub fn gauss_rule(ty: ElementType, order: usize) -> Result<QuadratureRule> {
    let dim = ty.natural_space_dimension();
    let rule = match ty.gauss_integration_type() {
        GaussIntegrationType::Point => table_rule(1, (vec![0.], vec![1.])),
        GaussIntegrationType::Segment => {
            if order == 0 {
                None
            } else {
                tensor_rule(dim, order)
            }
        }
        GaussIntegrationType::Triangle => {
            triangle_nb_points(order).and_then(|n| table_rule(2, triangle_data(n)?))
        }
        GaussIntegrationType::Tetrahedron => {
            tetrahedron_nb_points(order).and_then(|n| table_rule(3, tetrahedron_data(n)?))
        }
        GaussIntegrationType::Pentahedron => pentahedron_rule(order),
        GaussIntegrationType::NotDefined => {
            return Err(Error::UnsupportedElementType {
                ty,
                context: "gauss_rule",
            });
        }
    };
    rule.ok_or_else(|| unsupported(ty, order))
}

/// Number of points of the rule of `ty` at `order`.
pub fn nb_quadrature_points(ty: ElementType, order: usize) -> Result<usize> {
    gauss_rule(ty, order).map(|r| r.nb_points())
}

/// Default rule of `ty` (order = polynomial degree, upstream
/// `DefaultIntegrationOrderFunctor`).
pub fn default_rule(ty: ElementType) -> Result<QuadratureRule> {
    gauss_rule(ty, ty.polynomial_degree())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn point_counts_follow_upstream() {
        assert_eq!(
            (1..=7).map(segment_nb_points).collect::<Vec<_>>(),
            [1, 2, 2, 3, 3, 4, 4]
        );
        let n = |t, o| nb_quadrature_points(t, o).unwrap();
        use ElementType as E;
        assert_eq!(n(E::Hexahedron8, 2), 8);
        assert_eq!(n(E::Hexahedron20, 6), 64);
        assert_eq!(n(E::Quadrangle4, 4), 9);
        assert_eq!(n(E::Pentahedron6, 1), 6);
        assert_eq!(n(E::Pentahedron15, 4), 21);
        assert_eq!(n(E::Pentahedron6, 3), 8);
        assert_eq!(n(E::Tetrahedron10, 4), 15);
        assert_eq!(n(E::Point1, 9), 1);
        assert_eq!(n(E::Cohesive1d2, 1), 1);
        assert_eq!(n(E::Cohesive3d12, 3), 4);
        assert!(gauss_rule(E::Triangle3, 6).is_err());
        assert!(gauss_rule(E::Segment2, 8).is_err());
        assert!(gauss_rule(E::BernoulliBeam2, 3).is_ok()); // segment family
    }

    #[test]
    fn hexahedron_order_is_x_fastest() {
        let r = gauss_rule(ElementType::Hexahedron8, 2).unwrap();
        let a = 1. / 3f64.sqrt();
        let sgn = |q: usize, d: usize| if (q >> d) & 1 == 1 { a } else { -a };
        for q in 0..8 {
            for d in 0..3 {
                assert_eq!(r.points.at(d, q), sgn(q, d));
            }
        }
    }
}
