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

use akantu_core::array::ArrayView;
use akantu_core::exec::{Exec, Tuples, TuplesRef};
use akantu_core::tensor::MatrixX;
use akantu_core::{Array, Element, ElementKind, ElementType, Error, GhostType, Idx, Real, Result};
use akantu_mesh::Mesh;

use super::{
    Block, FeEngine, check_input, compute_jacobians, element_ids, multiply_by_weights,
    resolve_filter,
};
use crate::eigen::{self, Dims, SliceProduct, product_route};
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
    let mut field = Array::new(nb_dof);
    fill_field_into(field_funct, nb_dof, nb_element, nq, ty, ghost, &mut field)?;
    Ok(field)
}

/// [`fill_field`] into `field` (resized; every value is written).
fn fill_field_into(
    field_funct: &mut FieldFunctor<'_>,
    nb_dof: usize,
    nb_element: usize,
    nq: usize,
    ty: ElementType,
    ghost: GhostType,
    field: &mut Array<Real>,
) -> Result<()> {
    super::prepare_out(field, nb_element * nq, nb_dof)?;
    let mut mat = vec![0.0; nb_dof * nq];
    for e in 0..nb_element {
        mat.fill(0.0);
        field_funct(&mut mat, Element::try_new(ty, e, ghost)?)?;
        field.as_mut_slice()[e * nb_dof * nq..(e + 1) * nb_dof * nq].copy_from_slice(&mat);
    }
    Ok(())
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
        let mut out = Array::new(1);
        let mut field = Array::new(nb_dof);
        self.field_matrix_elemental_into(
            mesh,
            field_funct,
            nb_dof,
            ty,
            ghost,
            &mut out,
            &mut field,
        )?;
        Ok(out)
    }

    /// [`FeEngine::field_matrix_elemental`] into `out` (resized to
    /// `nb_element` tuples of `(nb_dof·nn)²`), with `field` as the buffer
    /// of the field functor's values: arrays kept by the caller between
    /// assemblies keep their pages mapped. The same checks, errors and
    /// values.
    #[allow(clippy::too_many_arguments)] // `field_matrix_elemental` + the two buffers
    pub fn field_matrix_elemental_into(
        &self,
        mesh: &Mesh,
        field_funct: &mut FieldFunctor<'_>,
        nb_dof: usize,
        ty: ElementType,
        ghost: GhostType,
        out: &mut Array<Real>,
        field: &mut Array<Real>,
    ) -> Result<()> {
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
        fill_field_into(field_funct, nb_dof, ne, nq, ty, ghost, field)?;
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
            // `Nt * N` per point and `∫ = local · J` per element (the
            // operands of `integrate_with_jacobians`), decided once; each
            // element's point matrices are integrated as soon as they are
            // computed (same products, same per-element integration: the
            // elements are independent), so only one element's `nq` point
            // matrices are held at a time per part
            let point = eigen::prepare(Dims::dynamic(lmat, nb_dof), Dims::dynamic(nb_dof, lmat))?;
            let integral = eigen::prepare(Dims::dynamic(l2, nq), Dims::new(nq, 1, false, true))?;
            super::prepare_out(out, ne, l2)?;
            let fs = field.as_slice();
            let gemv = integrates_by_gemv(l2, nq);
            let mass = MassKernel {
                block: &block,
                rule: &rule,
                kind,
                shapes: &shapes,
                fs,
                nb_dof,
                nn,
                nq,
                point: &point,
                integral: &integral,
                gemv,
            };
            // Elements are independent outputs (D-123): parts of about 64k
            // multiply-adds, each with its own point-matrix reuse.
            let min = (65_536 / (nq * l2).max(1)).max(1);
            self.exec.try_for_each(Tuples::new(out.as_mut_slice(), l2), min, |first, mut o| {
                mass.part::<E>(first, o.as_mut_slice())
            })?;
            Ok(())
        }, else Err(Error::UnsupportedElementType { ty, context: "field_matrix_elemental" }))
    }
}

/// The operands of the consistent-mass loop of
/// [`FeEngine::field_matrix_elemental`], shared by the parts.
struct MassKernel<'a, 'm> {
    block: &'a Block<'m>,
    rule: &'a crate::quadrature::QuadratureRule,
    kind: ElementKind,
    shapes: &'a [Real],
    fs: &'a [Real],
    nb_dof: usize,
    nn: usize,
    nq: usize,
    point: &'a SliceProduct,
    integral: &'a SliceProduct,
    /// `integral` is a column-major GEMV ([`integrates_by_gemv`]).
    gemv: bool,
}

/// `true` when the integral `f · J` of `nq` point values of `nb_dof`
/// components (operands of `integrate_with_jacobians`) takes the
/// column-major GEMV route without blocking, the route whose additions
/// [`gemv_integral`] performs.
fn integrates_by_gemv(nb_dof: usize, nq: usize) -> bool {
    use akantu_core::eigen_gebp::{GEMV_BLOCKING_MIN_DEPTH, GemvMatrix, Order, Route};
    let gemv = Route::Gemv {
        matrix: GemvMatrix::Lhs,
        kernel: Order::ColMajor,
    };
    nb_dof > 0
        && nq > 0
        && nq < GEMV_BLOCKING_MIN_DEPTH
        && product_route(Dims::dynamic(nb_dof, nq), Dims::new(nq, 1, false, true)) == gemv
}

/// `o = f · J` for one element (`f`: `nq` columns of `o.len()` values,
/// column-major), with the additions of the GEMV route of the general
/// product, in its order: every `o(i)` starts at `+0`, adds `f(i, l)·J(l)`
/// for `l = 0, 1, …`, and the result is added to a zeroed destination
/// (the loop of the engine's integration kernel; valid when
/// [`integrates_by_gemv`] holds).
#[inline(never)] // no-alias parameters (docs/benchmarks/perf-campaign/exec)
fn gemv_integral(f: &[Real], j: &[Real], o: &mut [Real]) {
    let n = o.len();
    o.fill(0.0);
    for (col, &jl) in f.chunks_exact(n).zip(j) {
        for (oi, &fl) in o.iter_mut().zip(col) {
            *oi += fl * jl;
        }
    }
    for oi in o.iter_mut() {
        *oi += 0.0;
    }
}

impl MassKernel<'_, '_> {
    /// The elemental matrices of the elements `first..` into `o` (one
    /// `(nb_dof·nn)²` tuple per element): weighted Jacobians, point
    /// matrices `Nᵀ diag(ρ) N` recomputed only when the density bits of the
    /// point change (the same operands give the same bits), integration.
    #[inline(never)] // no-alias parameters (docs/benchmarks/perf-campaign/exec)
    fn part<E: LagrangeElement>(&self, first: usize, o: &mut [Real]) -> Result<()> {
        let (nb_dof, nn, nq) = (self.nb_dof, self.nn, self.nq);
        let lmat = nb_dof * nn;
        let l2 = lmat * lmat;
        let len = o.len() / l2;
        // Jacobians times weights of the part's elements
        let ids: Vec<usize> = (first..first + len).collect();
        let slots: Vec<usize> = (0..len).collect();
        let mut jac = vec![0.0; len * nq];
        compute_jacobians::<E>(self.block, self.rule, self.kind, &ids, &slots, &mut jac)?;
        multiply_by_weights(&mut jac, &self.rule.weights, &slots);
        let mut local = vec![0.0; nq * l2];
        let mut nv = vec![0.0; nb_dof * lmat];
        let mut nt = vec![0.0; lmat * nb_dof];
        let mut rho_of = vec![0u64; nq * nb_dof];
        let mut valid = vec![false; nq];
        for p in 0..len {
            let e = first + p;
            for q in 0..nq {
                let t = e * nq + q;
                let rho = &self.fs[t * nb_dof..(t + 1) * nb_dof];
                let cached = &mut rho_of[q * nb_dof..(q + 1) * nb_dof];
                if valid[q] && same_bits(rho, cached) {
                    continue;
                }
                let s = &self.shapes[q * nn..(q + 1) * nn];
                nv.fill(0.0);
                for d in 0..nb_dof {
                    for (k, &sk) in s.iter().enumerate() {
                        nv[d + (k * nb_dof + d) * nb_dof] = sk;
                    }
                }
                // Nt = N^T; Nt(d) *= rho(d)
                for i in 0..lmat {
                    for d in 0..nb_dof {
                        nt[i + d * lmat] = nv[d + i * nb_dof] * rho[d];
                    }
                }
                self.point
                    .eval(&nt, &nv, &mut local[q * l2..(q + 1) * l2])?;
                for (c, r) in cached.iter_mut().zip(rho) {
                    *c = r.to_bits();
                }
                valid[q] = true;
            }
            let (je, oe) = (&jac[p * nq..(p + 1) * nq], &mut o[p * l2..(p + 1) * l2]);
            if self.gemv {
                gemv_integral(&local, je, oe);
            } else {
                self.integral.eval(&local, je, oe)?;
            }
        }
        Ok(())
    }
}

impl FeEngine {
    /// `∫ Bᵀ D B` per element: the result of
    /// [`FeEngine::compute_btdb`]`(d, tmp, order_d, ty, ghost, filter)`
    /// followed by [`FeEngine::integrate`]`(tmp, out, n², ty, ghost,
    /// filter)` (`n` the size of the elemental matrix), bit for bit, with
    /// the same checks and errors in the same order, without the per-point
    /// array `tmp` (`nb_element · nq · n²` values). Each element's point
    /// matrices are computed with the products of `compute_btdb` and
    /// integrated with those of `integrate`; elements are independent
    /// outputs, so they run on the engine's threads (D-123) with the same
    /// bits for every thread count.
    #[allow(clippy::too_many_arguments)] // `compute_btdb`'s signature
    pub fn compute_btdb_integrated(
        &self,
        d: ArrayView<'_, Real>,
        out: &mut Array<Real>,
        order_d: usize,
        ty: ElementType,
        ghost: GhostType,
        filter: Option<&[Idx]>,
    ) -> Result<()> {
        // the checks of `compute_btdb`, in its order
        self.check_type(ty, "compute_btdb")?;
        if self.kind == ElementKind::Cohesive {
            return Err(Error::NotImplemented(
                "computeBtDB for cohesive elements (AKANTU_TO_IMPLEMENT upstream)",
            ));
        }
        let dim = ty.spatial_dimension();
        if ty.natural_space_dimension() == 0 {
            return Err(Error::UnsupportedElementType {
                ty,
                context: "compute_btdb (point elements)",
            });
        }
        let dsh = self.shapes_derivatives(ty, ghost)?;
        let nq = self.nb_integration_points(ty, ghost)?;
        let ne = dsh.len().checked_div(nq).unwrap_or(0);
        let ids = element_ids(resolve_filter(filter, ne)?, ne);
        let nn = ty.nb_nodes_per_interpolation_element();
        let bsz = dim * nn;
        if dsh.nb_component() != bsz {
            return Err(Error::DimensionMismatch {
                context: "compute_btdb: shape derivatives (element dimension must equal the mesh dimension)",
                expected: bsz,
                got: dsh.nb_component(),
            });
        }
        let (ts, n) = match order_d {
            4 => {
                let ts = akantu_core::voigt::voigt_table(dim)?.size;
                (ts, dim * nn)
            }
            2 => (dim, nn),
            _ => {
                return Err(Error::InvalidArgument(format!(
                    "compute_btdb: order_d must be 2 or 4, got {order_d} (upstream silently computes nothing)"
                )));
            }
        };
        check_input(
            "compute_btdb: D",
            d.len(),
            d.nb_component(),
            ids.len() * nq,
            ts * ts,
        )?;
        // the checks of `integrate` (its input has the shape `compute_btdb`
        // gives)
        self.check_type(ty, "integrate")?;
        let jac = self.jacobians(ty, ghost)?;
        let nq_j = self.nb_integration_points(ty, ghost)?;
        let ne_j = jac.len().checked_div(nq_j).unwrap_or(0);
        let ids_j = element_ids(resolve_filter(filter, ne_j)?, ne_j);
        if nq_j != nq || ids_j != ids {
            // engines whose Jacobians and derivatives disagree: the two
            // calls, with their own checks
            let mut tmp = Array::new(n * n);
            self.compute_btdb(d, &mut tmp, order_d, ty, ghost, filter)?;
            return self.integrate(tmp.view(), out, n * n, ty, ghost, filter);
        }
        super::prepare_out(out, ids.len(), n * n)?;
        let (p_btd, p_btdb) = if order_d == 4 {
            (
                eigen::prepare(Dims::dynamic(ts, n).t(), Dims::dynamic(ts, ts))?,
                eigen::prepare(Dims::dynamic(n, ts), Dims::dynamic(ts, n))?,
            )
        } else {
            (
                eigen::prepare(Dims::dynamic(dim, nn).t(), Dims::dynamic(dim, dim))?,
                eigen::prepare(Dims::dynamic(nn, dim), Dims::dynamic(dim, nn))?,
            )
        };
        let p_fj = eigen::prepare(Dims::dynamic(n * n, nq), Dims::new(nq, 1, false, true))?;
        let kernel = BtdbKernel {
            voigt: order_d == 4,
            dim,
            nn,
            ts,
            n,
            nq,
            bs: dsh.as_slice(),
            ds: d.as_slice(),
            jac: jac.as_slice(),
            p_btd: &p_btd,
            p_btdb: &p_btdb,
            p_fj: &p_fj,
        };
        let min = (65_536 / (nq * n * n).max(1)).max(1);
        self.exec.try_for_each(
            (
                Tuples::new(out.as_mut_slice(), n * n),
                TuplesRef::new(&ids, 1),
            ),
            min,
            |first, (mut o, ids)| kernel.part(first, ids.as_slice(), o.as_mut_slice()),
        )
    }
}

impl FeEngine {
    /// `∫ Nᵀ b N` per element: the result of
    /// [`FeEngine::compute_ntbn`]`(b, tmp, ty, ghost, filter)` followed by
    /// [`FeEngine::integrate`]`(tmp, out, nn², ty, ghost, filter)`, bit for
    /// bit, with the same checks and errors in the same order, without the
    /// per-point array; elements on the engine's threads (D-123).
    pub fn compute_ntbn_integrated(
        &self,
        b: ArrayView<'_, Real>,
        out: &mut Array<Real>,
        ty: ElementType,
        ghost: GhostType,
        filter: Option<&[Idx]>,
    ) -> Result<()> {
        // the checks of `compute_ntbn`, in its order
        self.check_type(ty, "compute_ntbn")?;
        if self.kind == ElementKind::Cohesive {
            return Err(Error::NotImplemented(
                "computeNtbN for cohesive elements (AKANTU_TO_IMPLEMENT upstream)",
            ));
        }
        let shapes = self.shapes(ty, ghost)?;
        let nq = self.nb_integration_points(ty, ghost)?;
        let ne = shapes.len().checked_div(nq).unwrap_or(0);
        let ids = element_ids(resolve_filter(filter, ne)?, ne);
        let nn = shapes.nb_component();
        check_input(
            "compute_ntbn: b (one component)",
            b.len(),
            b.nb_component(),
            ids.len() * nq,
            1,
        )?;
        // the checks of `integrate`
        self.check_type(ty, "integrate")?;
        let jac = self.jacobians(ty, ghost)?;
        let nq_j = self.nb_integration_points(ty, ghost)?;
        let ne_j = jac.len().checked_div(nq_j).unwrap_or(0);
        let ids_j = element_ids(resolve_filter(filter, ne_j)?, ne_j);
        if nq_j != nq || ids_j != ids {
            let mut tmp = Array::new(nn * nn);
            self.compute_ntbn(b, &mut tmp, ty, ghost, filter)?;
            return self.integrate(tmp.view(), out, nn * nn, ty, ghost, filter);
        }
        let n2 = nn * nn;
        super::prepare_out(out, ids.len(), n2)?;
        // N.transpose() (nn × 1, N a 1 × nn map) · b (1 × 1), then · N
        let p_ntb = eigen::prepare(Dims::dynamic(1, nn).t(), Dims::dynamic(1, 1))?;
        let p_ntbn = eigen::prepare(Dims::dynamic(nn, 1), Dims::dynamic(1, nn))?;
        let p_fj = eigen::prepare(Dims::dynamic(n2, nq), Dims::new(nq, 1, false, true))?;
        let kernel = NtbnKernel {
            nn,
            nq,
            ss: shapes.as_slice(),
            bs: b.as_slice(),
            jac: jac.as_slice(),
            p_ntb: &p_ntb,
            p_ntbn: &p_ntbn,
            p_fj: &p_fj,
        };
        let min = (65_536 / (nq * n2).max(1)).max(1);
        self.exec.try_for_each(
            (Tuples::new(out.as_mut_slice(), n2), TuplesRef::new(&ids, 1)),
            min,
            |first, (mut o, ids)| kernel.part(first, ids.as_slice(), o.as_mut_slice()),
        )
    }
}

/// The operands of [`FeEngine::compute_ntbn_integrated`].
struct NtbnKernel<'a> {
    nn: usize,
    nq: usize,
    ss: &'a [Real],
    bs: &'a [Real],
    jac: &'a [Real],
    p_ntb: &'a SliceProduct,
    p_ntbn: &'a SliceProduct,
    p_fj: &'a SliceProduct,
}

impl NtbnKernel<'_> {
    /// The elemental matrices of the elements `ids` (filter positions
    /// `first..`): point matrices with the products of `compute_ntbn`,
    /// integrals with those of `integrate` (its fixed-size loop when it
    /// applies, else the general product), per chunk of elements.
    #[inline(never)] // no-alias parameters (docs/benchmarks/perf-campaign/exec)
    fn part(&self, first: usize, ids: &[usize], o: &mut [Real]) -> Result<()> {
        let (nn, nq) = (self.nn, self.nq);
        let n2 = nn * nn;
        let chunk = (BTDB_CHUNK_VALUES / (nq * n2).max(1)).max(1);
        let mut local = vec![0.0; chunk.min(ids.len()) * nq * n2];
        let mut ntb = vec![0.0; nn];
        for (c, cids) in ids.chunks(chunk).enumerate() {
            let p0 = first + c * chunk;
            for (k, &e) in cids.iter().enumerate() {
                for q in 0..nq {
                    let t = (p0 + k) * nq + q;
                    let n = &self.ss[(e * nq + q) * nn..(e * nq + q + 1) * nn];
                    self.p_ntb.eval(n, &self.bs[t..t + 1], &mut ntb)?;
                    self.p_ntbn.eval(
                        &ntb,
                        n,
                        &mut local[(k * nq + q) * n2..(k * nq + q + 1) * n2],
                    )?;
                }
            }
            let cl = cids.len();
            let f = &local[..cl * nq * n2];
            let oc = &mut o[c * chunk * n2..(c * chunk + cl) * n2];
            let jac = |pos: usize| {
                let e = cids[pos];
                &self.jac[e * nq..(e + 1) * nq]
            };
            if super::fast::integrate(Exec::serial_ref(), f, n2, nq, &jac, cl, oc).is_none() {
                for pos in 0..cl {
                    self.p_fj.eval(
                        &f[pos * nq * n2..(pos + 1) * nq * n2],
                        jac(pos),
                        &mut oc[pos * n2..(pos + 1) * n2],
                    )?;
                }
            }
        }
        Ok(())
    }
}

/// The operands of [`FeEngine::compute_btdb_integrated`], shared by the
/// parts.
struct BtdbKernel<'a> {
    /// `D` is a Voigt tangent (`order_d = 4`), else a `dim × dim` matrix.
    voigt: bool,
    dim: usize,
    nn: usize,
    /// Rows of `D` (Voigt size or `dim`).
    ts: usize,
    /// Size of the elemental matrix.
    n: usize,
    nq: usize,
    bs: &'a [Real],
    ds: &'a [Real],
    jac: &'a [Real],
    p_btd: &'a SliceProduct,
    p_btdb: &'a SliceProduct,
    p_fj: &'a SliceProduct,
}

/// Point matrices of at most this many values are held per part before
/// they are integrated (a few hundred kB).
const BTDB_CHUNK_VALUES: usize = 32_768;

impl BtdbKernel<'_> {
    /// The elemental matrices of the elements `ids` (filter positions
    /// `first..`) into `o`: the point matrices of a chunk of elements with
    /// the products of `compute_btdb`, then their integrals with the
    /// products of `integrate` (its fixed-size loop when it applies, else
    /// the general product).
    #[inline(never)] // no-alias parameters (docs/benchmarks/perf-campaign/exec)
    fn part(&self, first: usize, ids: &[usize], o: &mut [Real]) -> Result<()> {
        let (dim, nn, ts, n, nq) = (self.dim, self.nn, self.ts, self.n, self.nq);
        let n2 = n * n;
        let bsz = dim * nn;
        let chunk = (BTDB_CHUNK_VALUES / (nq * n2).max(1)).max(1);
        let mut local = vec![0.0; chunk.min(ids.len()) * nq * n2];
        let mut bfull = MatrixX::zeros(dim, nn);
        let mut bv = MatrixX::zeros(ts, n);
        let mut btd = vec![0.0; n * ts];
        let dsz = ts * ts;
        for (c, cids) in ids.chunks(chunk).enumerate() {
            let p0 = first + c * chunk;
            for (k, &e) in cids.iter().enumerate() {
                for q in 0..nq {
                    let b_src = &self.bs[(e * nq + q) * bsz..(e * nq + q + 1) * bsz];
                    let t = (p0 + k) * nq + q;
                    let dq = &self.ds[t * dsz..(t + 1) * dsz];
                    let dst = &mut local[(k * nq + q) * n2..(k * nq + q + 1) * n2];
                    if self.voigt {
                        bfull.as_mut_slice().copy_from_slice(b_src);
                        akantu_core::voigt::b_matrix_to_sym_voigt(dim, &bfull, &mut bv, nn)?;
                        let b = bv.as_slice();
                        self.p_btd.eval(b, dq, &mut btd)?;
                        self.p_btdb.eval(&btd, b, dst)?;
                    } else {
                        self.p_btd.eval(b_src, dq, &mut btd)?;
                        self.p_btdb.eval(&btd, b_src, dst)?;
                    }
                }
            }
            let cl = cids.len();
            let f = &local[..cl * nq * n2];
            let oc = &mut o[c * chunk * n2..(c * chunk + cl) * n2];
            let jac = |pos: usize| {
                let e = cids[pos];
                &self.jac[e * nq..(e + 1) * nq]
            };
            if super::fast::integrate(Exec::serial_ref(), f, n2, nq, &jac, cl, oc).is_none() {
                for pos in 0..cl {
                    self.p_fj.eval(
                        &f[pos * nq * n2..(pos + 1) * nq * n2],
                        jac(pos),
                        &mut oc[pos * n2..(pos + 1) * n2],
                    )?;
                }
            }
        }
        Ok(())
    }
}

/// Whether every value of `values` has the bits stored in `bits` (the
/// density a mass point matrix was computed with). By bits, not by value:
/// `+0` and `−0` differ and a NaN matches its own bits.
fn same_bits(values: &[Real], bits: &[u64]) -> bool {
    values.len() == bits.len() && values.iter().zip(bits).all(|(v, b)| v.to_bits() == *b)
}

#[cfg(test)]
mod tests {
    use super::{Dims, Real, eigen, gemv_integral, integrates_by_gemv, same_bits};

    /// The fixed loop of the mass integration gives the bits of the
    /// general product it replaces, on values with signed zeros,
    /// infinities and subnormals, for the shapes of the element types.
    #[test]
    fn gemv_integral_is_the_general_product() {
        let mut state = 0x2545_f491_4f6c_dd1du64;
        let mut value = || {
            state ^= state >> 12;
            state ^= state << 25;
            state ^= state >> 27;
            let r = state.wrapping_mul(0x9e37_79b9_7f4a_7c15);
            match r % 13 {
                0 => 0.0,
                1 => -0.0,
                2 => Real::INFINITY,
                3 => 5e-324,
                4 => -2.2e-308,
                _ => (r >> 11) as Real / (1u64 << 53) as Real * 8.0 - 4.0,
            }
        };
        let mut checked = 0;
        for (n, nq) in [
            (1, 1),
            (4, 1),
            (9, 3),
            (36, 4),
            (144, 4),
            (576, 8),
            (900, 15),
            (3600, 27),
        ] {
            if !integrates_by_gemv(n, nq) {
                continue;
            }
            let p = eigen::prepare(Dims::dynamic(n, nq), Dims::new(nq, 1, false, true)).unwrap();
            for _ in 0..20 {
                let f: Vec<Real> = (0..n * nq).map(|_| value()).collect();
                let j: Vec<Real> = (0..nq).map(|_| value()).collect();
                let (mut a, mut b) = (vec![1.0; n], vec![2.0; n]);
                p.eval(&f, &j, &mut a).unwrap();
                gemv_integral(&f, &j, &mut b);
                let bits = |v: &[Real]| v.iter().map(|x| x.to_bits()).collect::<Vec<_>>();
                // two NaNs may meet only through inf·0 terms: compare NaN as NaN
                for (x, y) in a.iter().zip(&b) {
                    assert!(
                        x.to_bits() == y.to_bits() || (x.is_nan() && y.is_nan()),
                        "{n}×{nq}"
                    );
                }
                assert_eq!(bits(&a).len(), bits(&b).len());
            }
            checked += 1;
        }
        assert!(checked >= 4, "the GEMV route covers the element shapes");
    }

    #[test]
    fn density_reuse_compares_bits() {
        let bits = |v: &[f64]| v.iter().map(|x| x.to_bits()).collect::<Vec<_>>();
        let nan = f64::from_bits(0xfff8_0000_0000_0abc);
        assert!(same_bits(&[2.5, 0.0], &bits(&[2.5, 0.0])));
        assert!(same_bits(&[nan, -0.0], &bits(&[nan, -0.0])));
        // equal by value, different bits
        assert!(!same_bits(&[0.0, 0.0], &bits(&[-0.0, -0.0])));
        assert!(!same_bits(&[-0.0, 1.0], &bits(&[0.0, 1.0])));
        // different NaN payloads or signs
        assert!(!same_bits(
            &[nan],
            &bits(&[f64::from_bits(0xfff8_0000_0000_0abd)])
        ));
        assert!(!same_bits(
            &[nan],
            &bits(&[f64::from_bits(0x7ff8_0000_0000_0abc)])
        ));
        assert!(!same_bits(&[2.5], &bits(&[2.5, 2.5])));
    }
}
