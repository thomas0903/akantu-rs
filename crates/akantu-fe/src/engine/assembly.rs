// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/fe_engine/fe_engine_template_tmpl_field.hh
// (fillField, assembleFieldLumped, assembleLumpedRowSum,
// assembleLumpedDiagonalScaling, ShapesForMassHelper, assembleFieldMatrix).

//! Element-level parts of the field assembly helpers (FE-030, FE-031).
//!
//! Upstream assembles the elemental arrays into the global system through
//! the DOF manager (`assembleElementalArrayLocalArray`,
//! `assembleElementalMatricesToMatrix`); in the port the FE engine returns
//! the elemental arrays and the model calls the DOF manager (ARCHITECTURE, "FE engine"). Layouts are upstream's: per element, node-major DOF vectors
//! (`nb_dof · nn` values, DOF `d` of node `n` at `n·nb_dof + d`) and
//! column-major `(nb_dof·nn)²` matrices.
//!
//! Evaluation order: `Ntb` and the consistent-mass products have a single
//! non-zero term per coefficient, the integrals are GEMV sums from `+0`:
//! all bit-exact.

use akantu_core::{Array, Element, ElementKind, ElementType, Error, GhostType, Real, Result};
use akantu_mesh::Mesh;

use super::{Block, FeEngine, compute_jacobians, multiply_by_weights};
use crate::eigen::{self, Dims};
use crate::element_class::{ElementClass, LagrangeElement};
use crate::quadrature::gauss_rule;
use crate::with_element_class;

/// Lumping scheme of an element type (upstream `assembleFieldLumped`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LumpingScheme {
    /// `M̃_i = ∫ ρ N_i` (linear types).
    RowSum,
    /// `M̃_i = c_i ∫ ρ` with fixed nodal factors (quadratic types:
    /// `_triangle_6`, `_quadrangle_8`, `_tetrahedron_10`, `_hexahedron_20`,
    /// `_pentahedron_15`).
    DiagonalScaling,
}

impl LumpingScheme {
    /// Scheme upstream uses for `ty`.
    #[must_use]
    pub fn of(ty: ElementType) -> Self {
        match ty {
            ElementType::Triangle6
            | ElementType::Quadrangle8
            | ElementType::Tetrahedron10
            | ElementType::Hexahedron20
            | ElementType::Pentahedron15 => LumpingScheme::DiagonalScaling,
            _ => LumpingScheme::RowSum,
        }
    }
}

/// Nodal factors of the diagonal-scaling lumping (upstream
/// `assembleLumpedDiagonalScaling`), `None` for other types.
#[must_use]
pub fn diagonal_scaling_factors(ty: ElementType) -> Option<Vec<Real>> {
    let nn = ty.nb_nodes_per_element();
    let np1 = ty.p1_type()?.nb_nodes_per_element();
    let assign = |corner: Real, mid: Real| -> Vec<Real> {
        (0..nn)
            .map(|n| if n < np1 { corner } else { mid })
            .collect()
    };
    Some(match ty {
        ElementType::Triangle6 => assign(1. / 12., 1. / 4.),
        ElementType::Tetrahedron10 => assign(1. / 32., 7. / 48.),
        ElementType::Quadrangle8 => assign(3. / 76., 16. / 76.),
        ElementType::Hexahedron20 => assign(7. / 248., 16. / 248.),
        ElementType::Pentahedron15 => {
            let mut f = vec![51. / 2358.; nn];
            let mid_triangle = 192. / 2358.;
            let mid_quadrangle = 300. / 2358.;
            for n in [6, 7, 8, 12, 13, 14] {
                f[n] = mid_triangle;
            }
            for n in [9, 10, 11] {
                f[n] = mid_quadrangle;
            }
            f
        }
        _ => return None,
    })
}

/// A field functor (upstream `std::function<void(Matrix<Real> &, const
/// Element &)>`): fills the column-major `nb_dof × nq` values of one
/// element (zero-initialised before the call).
pub type FieldFunctor<'a> = dyn FnMut(&mut [Real], Element) -> Result<()> + 'a;

/// Upstream `fe_engine::details::fillField`: `nb_element·nq` tuples of
/// `nb_dof`.
fn fill_field(
    field_funct: &mut FieldFunctor<'_>,
    nb_dof: usize,
    nb_element: usize,
    nq: usize,
    ty: ElementType,
    ghost: GhostType,
) -> Result<Array<Real>> {
    let mut field = Array::try_zeros(nb_element * nq, nb_dof)?;
    let mut mat = vec![0.0; nb_dof * nq];
    for e in 0..nb_element {
        mat.fill(0.0);
        field_funct(&mut mat, Element::try_new(ty, e, ghost)?)?;
        field.as_mut_slice()[e * nb_dof * nq..(e + 1) * nb_dof * nq].copy_from_slice(&mat);
    }
    Ok(field)
}

impl FeEngine {
    /// Element-level part of upstream `assembleFieldLumped(field_funct,
    /// lumped, dof_manager, type, ghost)`: the per-element nodal arrays
    /// (`nb_element` tuples of `nb_dof·nn`, node-major) that upstream hands
    /// to `DOFManager::assembleElementalArrayLocalArray`. Row-sum lumping
    /// (`∫ ρ N_i` via `computeNtb` + `integrate`) or diagonal scaling
    /// (`c_i ∫ ρ`) per [`LumpingScheme::of`].
    pub fn lumped_field_elemental(
        &self,
        mesh: &Mesh,
        field_funct: &mut FieldFunctor<'_>,
        nb_dof: usize,
        ty: ElementType,
        ghost: GhostType,
    ) -> Result<Array<Real>> {
        self.check_type(ty, "lumped_field_elemental")?;
        let nb_element = mesh.nb_element(ty, ghost);
        // upstream: getNbIntegrationPoints(type) (not-ghost rule)
        let nq = self.nb_integration_points(ty, GhostType::NotGhost)?;
        let field = fill_field(field_funct, nb_dof, nb_element, nq, ty, ghost)?;
        let nn = ty.nb_nodes_per_interpolation_element();
        match LumpingScheme::of(ty) {
            LumpingScheme::RowSum => {
                let mut ntb = Array::new(nb_dof * nn);
                self.compute_ntb(field.view(), &mut ntb, ty, ghost, None)?;
                let mut out = Array::new(nb_dof * nn);
                self.integrate(ntb.view(), &mut out, nb_dof * nn, ty, ghost, None)?;
                Ok(out)
            }
            LumpingScheme::DiagonalScaling => {
                let factors =
                    diagonal_scaling_factors(ty).ok_or(Error::UnsupportedElementType {
                        ty,
                        context: "diagonal scaling",
                    })?;
                let mut out = Array::try_zeros(nb_element, nb_dof * nn)?;
                if nb_element == 0 {
                    return Ok(out);
                }
                let mut int_field = Array::new(nb_dof);
                self.integrate(field.view(), &mut int_field, nb_dof, ty, ghost, None)?;
                let o = out.as_mut_slice();
                for e in 0..nb_element {
                    let v = int_field.tuple(e);
                    for (n, f) in factors.iter().enumerate() {
                        for d in 0..nb_dof {
                            o[e * nb_dof * nn + n * nb_dof + d] = v[d] * f;
                        }
                    }
                }
                Ok(out)
            }
        }
    }

    /// Element-level part of upstream `assembleFieldMatrix(field_funct,
    /// matrix_id, dof_id, dof_manager, type, ghost)` (consistent mass and
    /// similar): per element `∫ Nᵀ diag(ρ) N` with the rule of order
    /// `2·polynomial_degree` (or [`crate::shape::IntegrationOrders::mass`]),
    /// `N` the `nb_dof × nb_dof·nn` block shape matrix; Jacobians computed
    /// on the fly with that rule (no sign check, as upstream). Output:
    /// `nb_element` tuples of `(nb_dof·nn)²` (column-major), the elemental
    /// matrices upstream assembles with
    /// `DOFManager::assembleElementalMatricesToMatrix`. Cohesive kind:
    /// `AKANTU_TO_IMPLEMENT` upstream.
    pub fn field_matrix_elemental(
        &self,
        mesh: &Mesh,
        field_funct: &mut FieldFunctor<'_>,
        nb_dof: usize,
        ty: ElementType,
        ghost: GhostType,
    ) -> Result<Array<Real>> {
        self.check_type(ty, "field_matrix_elemental")?;
        if self.kind == ElementKind::Cohesive {
            return Err(Error::NotImplemented(
                "assembleFieldMatrix for cohesive elements (AKANTU_TO_IMPLEMENT upstream)",
            ));
        }
        let order = self.orders.mass_order(ty);
        let rule = gauss_rule(ty, order)?;
        let nq = rule.nb_points();
        let block = Block::new(mesh, ty, ghost)?;
        let ne = block.nb_element;
        let kind = self.kind;
        let field = fill_field(field_funct, nb_dof, ne, nq, ty, ghost)?;
        with_element_class!(ty, E => {
            let nn = E::NB_SHAPES;
            let nd = E::NATURAL_DIM;
            let lmat = nb_dof * nn;
            // shapes at the mass rule points (same for every element)
            let mut shapes = vec![0.0; nq * nn];
            for q in 0..nq {
                E::compute_shapes(rule.point(q, nd), &mut shapes[q * nn..(q + 1) * nn]);
            }
            let l2 = lmat * lmat;
            // Jacobians times weights (independent of the point matrices)
            let ids: Vec<usize> = (0..ne).collect();
            let mut jac = vec![0.0; ne * nq];
            compute_jacobians::<E>(&block, &rule, kind, &ids, &ids, &mut jac)?;
            multiply_by_weights(&mut jac, &rule.weights, &ids);
            // `Nt * N` per point and `∫ = local · J` per element (the
            // operands of `integrate_with_jacobians`), decided once; each
            // element's point matrices are integrated as soon as they are
            // computed (same products, same per-element integration: the
            // elements are independent), so only one element's `nq` point
            // matrices are held at a time
            let point = eigen::prepare(Dims::dynamic(lmat, nb_dof), Dims::dynamic(nb_dof, lmat))?;
            let integral = eigen::prepare(Dims::dynamic(l2, nq), Dims::new(nq, 1, false, true))?;
            let mut out = Array::new(l2);
            super::prepare_out(&mut out, ne, l2)?;
            let mut local = vec![0.0; nq * l2];
            let mut nv = vec![0.0; nb_dof * lmat];
            let mut nt = vec![0.0; lmat * nb_dof];
            let fs = field.as_slice();
            let o = out.as_mut_slice();
            for e in 0..ne {
                for q in 0..nq {
                    let s = &shapes[q * nn..(q + 1) * nn];
                    nv.fill(0.0);
                    for d in 0..nb_dof {
                        for (k, &sk) in s.iter().enumerate() {
                            nv[d + (k * nb_dof + d) * nb_dof] = sk;
                        }
                    }
                    let t = e * nq + q;
                    let rho = &fs[t * nb_dof..(t + 1) * nb_dof];
                    // Nt = N^T; Nt(d) *= rho(d)
                    for i in 0..lmat {
                        for d in 0..nb_dof {
                            nt[i + d * lmat] = nv[d + i * nb_dof] * rho[d];
                        }
                    }
                    point.eval(&nt, &nv, &mut local[q * l2..(q + 1) * l2])?;
                }
                integral.eval(&local, &jac[e * nq..(e + 1) * nq], &mut o[e * l2..(e + 1) * l2])?;
            }
            Ok(out)
        }, else Err(Error::UnsupportedElementType { ty, context: "field_matrix_elemental" }))
    }
}
