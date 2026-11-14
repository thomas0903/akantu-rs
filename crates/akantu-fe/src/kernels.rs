// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/fe_engine/element_class_tmpl.hh
// (computeJMat, computeJacobian, computeShapeDerivatives,
// computeNormalsOnNaturalCoordinates, inverseMap),
// interpolation_element_tmpl.hh / element_class.hh
// (interpolate, interpolateOnNaturalCoordinates,
// gradientOnNaturalCoordinates).

//! Element-level kernels (FE-005…009) with upstream's evaluation order.
//!
//! The order of the sums inside these kernels depends on the Eigen operand
//! types at the upstream call site (fixed vs dynamic sizes, see
//! [`crate::eigen`]); every kernel therefore takes the [`Dims`] of its
//! operands. The call sites document which upstream types they mirror:
//!
//! | Upstream call site | `dnds` | node coordinates `X` |
//! |---|---|---|
//! | `IntegratorGauss` Jacobians (`ElementClass::computeJacobian(quads, X, j)`) | fixed `nd × nn` | dynamic view |
//! | `ShapeLagrange` derivatives (`computeShapeDerivativesOnCPointsByElement`) | dynamic `Tensor3` slice | dynamic view |
//! | `FEEngineTemplate::computeNormalsOnIntegrationPoints` | fixed `nd × nn` | `Map<Matrix<Dynamic, nn>>` (fixed columns) |
//! | `ElementClass::inverseMap` (both `ShapeLagrange` and probes) | dynamic | dynamic `Matrix<Real>` |
//!
//! Jacobian determinants of square `J` are always evaluated on a dynamic
//! matrix upstream (`Matrix<Real> J`), i.e. through the partial-pivot LU, as
//! are the inverses in `computeShapeDerivatives` (dynamic `Map`s).

use akantu_core::{Error, Real, Result};

use crate::eigen::{self, Dims, Route};
use crate::element_class::LagrangeElement;
use crate::geometry;

/// Largest number of interpolation nodes of a Lagrange element (`_hexahedron_20`).
pub const MAX_NODES: usize = 20;
/// Largest spatial / natural dimension.
pub const MAX_DIM: usize = 3;

/// `J = dnds · Xᵀ` (upstream `computeJMat`): `dnds` is `nd × nn`, `X` is
/// `sdim × nn`, `J` is `nd × sdim`, all column-major. Returns the Eigen
/// route taken (see [`crate::eigen::product_route`]).
///
/// # Errors
/// `Error::DimensionMismatch` if the dims disagree or a slice is too short.
pub fn jmat(
    dnds: &[Real],
    dnds_dims: Dims,
    x: &[Real],
    x_dims: Dims,
    j: &mut [Real],
) -> Result<Route> {
    eigen::product_slices(dnds_dims, x_dims.t(), dnds, x, j)
}

/// Upstream `ElementClass::computeJacobian(J)` for a **dynamic** `J`
/// (`nd × sdim`, column-major):
/// * square: signed `det J` (partial-pivot LU, `n ≤ 3`);
/// * `nd = 1` embedded: `‖J‖` (run-time size, sequential);
/// * `nd = 2` embedded in 3-D: `‖J₀ × J₁‖` (rows mapped as a fixed `2 × 3`
///   matrix, fixed-size squared norm);
/// * otherwise `0` (e.g. point elements, `nd = 0`).
///
/// # Errors
/// `Error::DimensionMismatch` if `nd` or `sdim` exceeds 3 or `j` holds
/// fewer than `nd · sdim` values.
pub fn jacobian(j: &[Real], nd: usize, sdim: usize) -> Result<Real> {
    if nd > MAX_DIM || sdim > MAX_DIM {
        return Err(Error::DimensionMismatch {
            context: "jacobian: natural and spatial dimensions (at most 3)",
            expected: MAX_DIM,
            got: nd.max(sdim),
        });
    }
    if j.len() < nd * sdim {
        return Err(Error::DimensionMismatch {
            context: "jacobian: J (nd × sdim)",
            expected: nd * sdim,
            got: j.len(),
        });
    }
    Ok(if nd == sdim {
        eigen::lu_det(nd, j)
    } else {
        match nd {
            1 => eigen::norm(sdim, false, |c| j[c]),
            2 => {
                // Map<const Matrix<Real, 2, 3>>(J.data()): row r, column c at r + 2c
                let r0 = [j[0], j[2], j[4]];
                let r1 = [j[1], j[3], j[5]];
                let cr = eigen::cross(&r0, &r1);
                eigen::norm(3, true, |i| cr[i])
            }
            _ => 0.0,
        }
    })
}

/// `dN/dx = J⁻¹ · dnds` (upstream `computeShapeDerivatives`): `J` dynamic
/// `sdim × sdim` (LU inverse into a dynamic temporary), `dnds` with the given
/// dims (`sdim × nn`, at most 3 rows), output `sdim × nn` column-major.
/// Returns the Eigen route of the product.
///
/// # Errors
/// `Error::DimensionMismatch` if `sdim > 3` (checked before any length
/// arithmetic) or a slice is too short.
pub fn shape_derivatives(
    j: &[Real],
    dnds: &[Real],
    dnds_dims: Dims,
    out: &mut [Real],
) -> Result<Route> {
    let sdim = dnds_dims.rows;
    // Bound the dimension first, so the Jacobian-length
    // diagnostic below cannot overflow for an absurd caller dimension.
    if sdim > MAX_DIM {
        return Err(Error::DimensionMismatch {
            context: "shape_derivatives: spatial dimension (at most 3)",
            expected: MAX_DIM,
            got: sdim,
        });
    }
    if j.len() < sdim * sdim {
        return Err(Error::DimensionMismatch {
            context: "shape_derivatives: Jacobian (square, spatial dimension at most 3)",
            expected: sdim * sdim,
            got: j.len(),
        });
    }
    let mut inv = [0.0; MAX_DIM * MAX_DIM];
    // Upstream does not check for singular Jacobians here: inf/NaN propagate.
    let _ = eigen::lu_inverse(sdim, j, &mut inv);
    eigen::product_slices(Dims::dynamic(sdim, sdim), dnds_dims, &inv, dnds, out)
}

/// `X · N` (upstream `InterpolationElement::interpolate(nodal, shapes)`):
/// `X` is `rows × nn`, `N` an `nn`-vector with the given dims; output
/// `rows` values. Returns the Eigen route taken.
///
/// # Errors
/// `Error::DimensionMismatch` if the dims disagree or a slice is too short.
pub fn interpolate(
    x: &[Real],
    x_dims: Dims,
    n: &[Real],
    n_dims: Dims,
    out: &mut [Real],
) -> Result<Route> {
    eigen::product_slices(x_dims, n_dims, x, n, out)
}

/// `X · dndsᵀ` (upstream `gradientOnNaturalCoordinates`): `X` is
/// `sdim × nn`, `dnds` is `nd × nn`; output `sdim × nd` column-major.
/// Returns the Eigen route taken.
///
/// # Errors
/// `Error::DimensionMismatch` if the dims disagree or a slice is too short.
pub fn gradient_natural(
    x: &[Real],
    x_dims: Dims,
    dnds: &[Real],
    dnds_dims: Dims,
    out: &mut [Real],
) -> Result<Route> {
    eigen::product_slices(x_dims, dnds_dims.t(), x, dnds, out)
}

/// Normal of a facet from its tangents `T = ∂x/∂ξ` (`sdim × nd`,
/// column-major), upstream `computeNormalsOnNaturalCoordinates`: 2-D
/// `Math::normal(T)` (dynamic), 3-D `Math::normal(T₀, T₁)` (fixed).
/// Other dimensions leave `n` untouched (as upstream).
///
/// # Errors
/// `Error::DimensionMismatch` if `t` (2-D: 2, 3-D: 6 values) or `n`
/// (`sdim` values) is too short.
pub fn normal_from_tangents(t: &[Real], sdim: usize, n: &mut [Real]) -> Result<()> {
    let need_t = match sdim {
        2 => 2,
        3 => 6,
        _ => return Ok(()),
    };
    if t.len() < need_t || n.len() < sdim {
        return Err(Error::DimensionMismatch {
            context: "normal_from_tangents: tangents (sdim × (sdim − 1)) / normal (sdim)",
            expected: need_t.max(sdim),
            got: if t.len() < need_t { t.len() } else { n.len() },
        });
    }
    match sdim {
        2 => n[..2].copy_from_slice(&geometry::normal_2d(&t[..2])),
        _ => n[..3].copy_from_slice(&geometry::normal_3d(&t[..3], &t[3..6])),
    }
    Ok(())
}

/// Normals at natural points for an element `E` whose natural dimension is
/// `sdim − 1` (upstream `computeNormalsOnNaturalCoordinates(coord, X, n)`):
/// `points` are `nd`-tuples, `x` is the column-major `sdim × nn` node
/// coordinate matrix and `n` receives `sdim` values per point (exactly
/// `nb_points · sdim`); malformed sizes give `Error::DimensionMismatch`.
/// `x_fixed_cols` tells whether the node-coordinate operand has
/// compile-time columns (FE engine: `Map<Matrix<Dynamic, nn>>` → `true`;
/// dynamic `Matrix<Real>` → `false`): it selects the type of the `dnds`
/// temporary (`Matrix<Real, nd, Cols(X)>`) and hence the summation order.
pub fn normals_on_natural<E: LagrangeElement>(
    points: &[Real],
    x: &[Real],
    sdim: usize,
    x_fixed_cols: bool,
    n: &mut [Real],
) -> Result<()> {
    let nd = E::NATURAL_DIM;
    let nn = E::NB_SHAPES;
    if nd == 0 {
        // ElementClass<_point_1>::computeNormalsOnNaturalCoordinates: no-op
        return Ok(());
    }
    // The scratch tangent matrix holds `MAX_DIM × MAX_DIM`
    // values; facet normals exist for spatial dimensions 2 and 3 only.
    if sdim > MAX_DIM || nd + 1 > MAX_DIM {
        return Err(Error::DimensionMismatch {
            context: "normals_on_natural (spatial dimension at most 3: facet elements of \
                      natural dimension 1 or 2)",
            expected: MAX_DIM,
            got: sdim.max(nd + 1),
        });
    }
    if sdim != nd + 1 {
        return Err(Error::DimensionMismatch {
            context: "normals_on_natural (spatial dimension must be natural dimension + 1)",
            expected: nd + 1,
            got: sdim,
        });
    }
    if !points.len().is_multiple_of(nd) {
        return Err(Error::DimensionMismatch {
            context: "normals_on_natural: points (a whole number of natural-coordinate tuples)",
            expected: nd,
            got: points.len(),
        });
    }
    let nb_points = points.len() / nd;
    if x.len() != sdim * nn {
        return Err(Error::DimensionMismatch {
            context: "normals_on_natural: node coordinates",
            expected: sdim * nn,
            got: x.len(),
        });
    }
    if n.len() != nb_points * sdim {
        return Err(Error::DimensionMismatch {
            context: "normals_on_natural: normals",
            expected: nb_points * sdim,
            got: n.len(),
        });
    }
    let x_dims = Dims::new(sdim, nn, false, x_fixed_cols);
    let dnds_dims = Dims::new(nd, nn, true, x_fixed_cols);
    let mut dnds = [0.0; MAX_DIM * MAX_NODES];
    let mut t = [0.0; MAX_DIM * MAX_DIM];
    for p in 0..nb_points {
        E::compute_dnds(&points[p * nd..(p + 1) * nd], &mut dnds[..nd * nn]);
        gradient_natural(x, x_dims, &dnds[..nd * nn], dnds_dims, &mut t[..sdim * nd])?;
        normal_from_tangents(&t, sdim, &mut n[p * sdim..(p + 1) * sdim])?;
    }
    Ok(())
}

/// Upstream `ElementClass<type>::inverseMap(real, X, natural, max_iterations,
/// tolerance)` (FE-009, `element_class_tmpl.hh:425-552`): Gauss–Newton from
/// `ξ = 0`, `dξ = (Jt (JtᵀJt)⁻¹)ᵀ (x − x(ξ))`, stop when
/// `‖x − x(ξ)‖ ≤ tolerance` (absolute) — upstream has **no** closed form for
/// linear simplices. All operands are dynamic as upstream (`Matrix<Real>`
/// node coordinates, dynamic `Jt`, `G`, `F`), except the shape vector of the
/// interpolation (`Matrix<Real, nn, 1>`).
///
/// `real`: `sdim` values; `x`: column-major `sdim × nn`; `natural`:
/// `NATURAL_DIM` values (overwritten). Like upstream, reaching
/// `max_iterations` is an error even if the last iterate converged
/// (`Error::NotConverged`, upstream message "The solver in inverse map did not
/// converge").
pub fn inverse_map<E: LagrangeElement>(
    real: &[Real],
    x: &[Real],
    natural: &mut [Real],
    max_iterations: usize,
    tolerance: Real,
) -> Result<()> {
    let sdim = real.len();
    let nn = E::NB_SHAPES;
    let nd = E::NATURAL_DIM;
    if natural.len() != nd {
        return Err(Error::DimensionMismatch {
            context: "inverse_map natural coordinates",
            expected: nd,
            got: natural.len(),
        });
    }
    if x.len() != sdim * nn || sdim > MAX_DIM || sdim < nd {
        return Err(Error::DimensionMismatch {
            context: "inverse_map node coordinates",
            expected: sdim * nn,
            got: x.len(),
        });
    }
    natural.fill(0.0);
    let x_dims = Dims::dynamic(sdim, nn);
    let mut shapes = [0.0; MAX_NODES];
    let mut guess = [0.0; MAX_DIM];
    let mut f = [0.0; MAX_DIM];
    let mut update_f = |natural: &[Real], f: &mut [Real; MAX_DIM]| -> Result<Real> {
        E::compute_shapes(natural, &mut shapes[..nn]);
        // Matrix<Real, Dynamic, 1> res = X * Matrix<Real, nn, 1>
        interpolate(
            x,
            x_dims,
            &shapes[..nn],
            Dims::fixed(nn, 1),
            &mut guess[..sdim],
        )?;
        for i in 0..sdim {
            f[i] = real[i] - guess[i];
        }
        Ok(eigen::norm(sdim, false, |i| f[i]))
    };
    let mut error = update_f(natural, &mut f)?;
    let mut dnds = [0.0; MAX_DIM * MAX_NODES];
    let mut jt = [0.0; MAX_DIM * MAX_DIM];
    let mut g = [0.0; MAX_DIM * MAX_DIM];
    let mut ginv = [0.0; MAX_DIM * MAX_DIM];
    let mut fm = [0.0; MAX_DIM * MAX_DIM];
    let mut dxi = [0.0; MAX_DIM];
    let mut iterations = 0;
    while tolerance < error && iterations < max_iterations {
        // Jt = X · dndsᵀ, dnds: Matrix<Real, Dynamic, Dynamic>
        E::compute_dnds(natural, &mut dnds[..nd * nn]);
        gradient_natural(x, x_dims, &dnds[..nd * nn], Dims::dynamic(nd, nn), &mut jt)?;
        let jt_dims = Dims::dynamic(sdim, nd);
        // G = Jtᵀ · Jt
        eigen::product_slices(jt_dims.t(), jt_dims, &jt, &jt, &mut g)?;
        // F = Jt · G⁻¹
        let _ = eigen::lu_inverse(nd, &g, &mut ginv);
        eigen::product_slices(jt_dims, Dims::dynamic(nd, nd), &jt, &ginv, &mut fm)?;
        // dxi = Fᵀ · f (dynamic matrix × dynamic vector: GEMV)
        eigen::product_slices(
            Dims::dynamic(sdim, nd).t(),
            Dims::new(sdim, 1, false, true),
            &fm,
            &f,
            &mut dxi,
        )?;
        for (xi, d) in natural.iter_mut().zip(&dxi[..nd]) {
            *xi += d;
        }
        error = update_f(natural, &mut f)?;
        iterations += 1;
    }
    if iterations >= max_iterations {
        return Err(Error::NotConverged {
            threshold: tolerance,
            niter: iterations,
            error,
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::element_class::types::*;

    #[test]
    fn affine_inverse_map_round_trip() {
        // tri3 with an affine map
        let x = [0.1, 0.2, 1.4, 0.3, 0.2, 1.1];
        let mut xi = [0.0; 2];
        let real = [0.1 + 1.3 * 0.25 + 0.1 * 0.5, 0.2 + 0.1 * 0.25 + 0.9 * 0.5];
        inverse_map::<Triangle3>(&real, &x, &mut xi, 100, 1e-10).unwrap();
        assert!((xi[0] - 0.25).abs() < 1e-12 && (xi[1] - 0.5).abs() < 1e-12);
    }

    #[test]
    fn inverse_map_reports_non_convergence() {
        let x = [0.0, 1.0];
        let mut xi = [0.0];
        // a point element cannot move towards the target
        let r = inverse_map::<Point1>(&[1.0], &[0.0], &mut [], 5, 1e-10);
        assert!(matches!(r, Err(Error::NotConverged { niter: 5, .. })));
        inverse_map::<Segment2>(&[0.5], &x, &mut xi, 100, 1e-10).unwrap();
        assert_eq!(xi[0], 0.0);
    }

    #[test]
    fn jacobian_variants() {
        // square 2x2: LU path
        let j = [2.0, 0.0, 0.0, 3.0];
        assert_eq!(jacobian(&j, 2, 2).unwrap(), 6.0);
        // embedded segment in 2-D
        assert_eq!(jacobian(&[3.0, 4.0], 1, 2).unwrap(), 5.0);
        // embedded triangle in 3-D: rows (1,0,0) and (0,1,0) -> |e3| = 1
        let j = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];
        assert_eq!(jacobian(&j, 2, 3).unwrap(), 1.0);
        assert_eq!(jacobian(&[], 0, 2).unwrap(), 0.0);
    }
}
