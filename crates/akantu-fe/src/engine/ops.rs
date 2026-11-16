// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/fe_engine/fe_engine_template_tmpl.hh
// (gradientOnIntegrationPoints, interpolateOnIntegrationPoints, integrate,
// computeBtD/BtDB/Ntb/NtbN, computeIntegrationPointsCoordinates),
// shape_functions_inline_impl.hh, shape_lagrange_inline_impl.hh,
// shape_cohesive_inline_impl.hh, integrator_gauss_inline_impl.hh,
// fe_engine_template_cohesive.cc, aka_voigthelper_tmpl.hh (usage).

//! Batch operators on integration points (FE-019…023, FE-028, FE-033/034).
//!
//! Summation orders (`crate::eigen`), per quadrature point:
//!
//! | Operation | Upstream operands | Order |
//! |---|---|---|
//! | gradient `U·Bᵀ` | `U`: `Matrix<Dynamic, nn>` view, `B`: fixed `nd × nn` | tree (coefficient-based) / GEMM for large `nb_dof + nd + nn` |
//! | interpolation `U·N` | dynamic `nb_dof × nn` and `nn × nq` views | sequential if `nb_dof + nq + nn < 20`, else GEMM |
//! | integration `f·J` | dynamic `nb_dof × nq` view, dynamic vector | GEMV (from `+0`) |
//! | `BᵀD` = `D·B` | dynamic views | sequential / GEMM |
//! | `BᵀDB` | dynamic `B` (Voigt) and `D` | `(Bᵀ·D)·B`, each product sequential / GEMM |
//! | `Nᵀb` = `b·N` | dynamic `nb_dof × 1`, `1 × nn` | single products |
//! | `NᵀbN` | dynamic | single products |
//!
//! "GEMM" marks products Eigen routes to its GEMM implementation (run-time
//! fallbacks to the coefficient-based product, GEMV or the inner product,
//! else the blocked `gebp` kernel). Every product is evaluated by
//! `akantu_core::eigen_gebp` with the operand extents of the upstream call
//! site ([`crate::eigen::product`]), which reproduces all of these routes
//! bit for bit (`docs/NUMERICAL_VALIDATION.md`).

use akantu_core::array::{ArrayView, ArrayViewMut};
use akantu_core::tensor::MatrixX;
use akantu_core::{Array, ElementKind, ElementType, Error, GhostType, Idx, Real, Result};
use akantu_mesh::Mesh;

use super::{
    Block, FeEngine, batch_ids, check_input, check_precomputed, element_ids, id_index, prepare_out,
    resolve_filter,
};
use crate::eigen::{self, Dims};
use crate::element_class::LagrangeElement;
use crate::shape::CohesiveReduction;
use crate::with_element_class;

impl FeEngine {
    /// Gather the element values of a nodal field, either plainly
    /// (regular) or reduced over the two sides of a cohesive element
    /// (upstream `ShapeLagrange<_ek_cohesive>::extractNodalToElementField`
    /// with `CohesiveReduceFunctionMean` = `.5 * (u⁺ + u⁻)` or
    /// `…Opening` = `u⁺ − u⁻`). Output column-major `nb_dof × nn_itp`.
    pub(crate) fn gather_element(
        block: &Block<'_>,
        e: usize,
        field: &[Real],
        nb_dof: usize,
        reduction: Option<CohesiveReduction>,
        scratch: &mut [Real],
        out: &mut [Real],
    ) -> Result<()> {
        match reduction {
            None => block.gather(e, field, nb_dof, out),
            Some(r) => {
                block.gather(e, field, nb_dof, scratch)?;
                let half = block.nnpe / 2;
                for n in 0..half {
                    for d in 0..nb_dof {
                        let p = scratch[d + n * nb_dof];
                        let m = scratch[d + (n + half) * nb_dof];
                        out[d + n * nb_dof] = match r {
                            CohesiveReduction::Mean => 0.5 * (p + m),
                            CohesiveReduction::Opening => p - m,
                        };
                    }
                }
                Ok(())
            }
        }
    }

    /// Interpolate a nodal field at the quadrature points (upstream
    /// `interpolateOnIntegrationPoints`; cohesive kind: mid-surface mean, as
    /// upstream's default `CohesiveReduceFunctionMean`). Output
    /// `nb_element·nq` tuples (`filter.len()·nq` with a filter) of `nb_dof`.
    #[allow(clippy::too_many_arguments)] // mirrors the upstream signature
    pub fn interpolate_on_integration_points(
        &self,
        mesh: &Mesh,
        u: ArrayView<'_, Real>,
        out: &mut Array<Real>,
        nb_dof: usize,
        ty: ElementType,
        ghost: GhostType,
        filter: Option<&[Idx]>,
    ) -> Result<()> {
        let reduction = (self.kind == ElementKind::Cohesive).then_some(CohesiveReduction::Mean);
        self.interpolate_impl(mesh, u, out, nb_dof, ty, ghost, filter, reduction)
    }

    /// Interpolate on the integration points of cohesive elements with the
    /// given side reduction (upstream
    /// `ShapeLagrange<_ek_cohesive>::interpolateOnIntegrationPoints<type,
    /// ReduceFunction>`; e.g. the opening `u⁺ − u⁻`).
    #[allow(clippy::too_many_arguments)] // mirrors upstream + reduction
    pub fn interpolate_cohesive(
        &self,
        mesh: &Mesh,
        nodal: ArrayView<'_, Real>,
        out: &mut Array<Real>,
        ty: ElementType,
        ghost: GhostType,
        reduction: CohesiveReduction,
        filter: Option<&[Idx]>,
    ) -> Result<()> {
        if self.kind != ElementKind::Cohesive {
            return Err(Error::UnsupportedElementType {
                ty,
                context: "interpolate_cohesive (cohesive engines only)",
            });
        }
        let nb_dof = nodal.nb_component();
        self.interpolate_impl(mesh, nodal, out, nb_dof, ty, ghost, filter, Some(reduction))
    }

    #[allow(clippy::too_many_arguments)]
    fn interpolate_impl(
        &self,
        mesh: &Mesh,
        u: ArrayView<'_, Real>,
        out: &mut Array<Real>,
        nb_dof: usize,
        ty: ElementType,
        ghost: GhostType,
        filter: Option<&[Idx]>,
        reduction: Option<CohesiveReduction>,
    ) -> Result<()> {
        self.check_type(ty, "interpolate_on_integration_points")?;
        check_input(
            "interpolate_on_integration_points: nodal field",
            u.len(),
            u.nb_component(),
            mesh.nb_nodes(),
            nb_dof,
        )?;
        let block = Block::new(mesh, ty, ghost)?;
        let filter = resolve_filter(filter, block.nb_element)?;
        let ids = element_ids(filter, block.nb_element);
        let shapes = self.shapes(ty, ghost)?;
        let nq = self.nb_integration_points(ty, ghost)?;
        check_precomputed(
            "interpolate_on_integration_points: precomputed shapes",
            shapes.len(),
            block.nb_element,
            nq,
        )?;
        let nn = shapes.nb_component();
        prepare_out(out, ids.len() * nq, nb_dof)?;
        let mut scratch = vec![0.0; nb_dof * block.nnpe];
        let mut ue = vec![0.0; nb_dof * nn];
        let p_un = eigen::prepare(Dims::dynamic(nb_dof, nn), Dims::dynamic(nn, nq))?;
        let s = shapes.as_slice();
        let o = out.as_mut_slice();
        for (pos, &e) in ids.iter().enumerate() {
            Self::gather_element(
                &block,
                e,
                u.as_slice(),
                nb_dof,
                reduction,
                &mut scratch,
                &mut ue,
            )?;
            let ne = &s[e * nq * nn..(e + 1) * nq * nn];
            p_un.eval(&ue, ne, &mut o[pos * nq * nb_dof..(pos + 1) * nq * nb_dof])?;
        }
        Ok(())
    }

    /// Physical coordinates of the quadrature points (upstream
    /// `computeIntegrationPointsCoordinates`: interpolation of the node
    /// coordinates).
    pub fn integration_points_coordinates(
        &self,
        mesh: &Mesh,
        out: &mut Array<Real>,
        ty: ElementType,
        ghost: GhostType,
        filter: Option<&[Idx]>,
    ) -> Result<()> {
        let sdim = mesh.spatial_dimension();
        self.interpolate_on_integration_points(
            mesh,
            mesh.nodes().view(),
            out,
            sdim,
            ty,
            ghost,
            filter,
        )
    }

    /// `∇u` at quadrature points: output tuples of `nb_dof · nd` values,
    /// column-major `nb_dof × nd` (`∂u_i/∂x_j` at `i + j·nb_dof`; upstream
    /// `gradientOnIntegrationPoints`). Regular kind only (cohesive:
    /// `AKANTU_TO_IMPLEMENT` upstream); requires precomputed physical
    /// derivatives (natural dimension = mesh dimension) — upstream's release
    /// build silently skips other types (UD-054), the port returns an error.
    #[allow(clippy::too_many_arguments)] // mirrors the upstream signature
    pub fn gradient_on_integration_points(
        &self,
        mesh: &Mesh,
        u: ArrayView<'_, Real>,
        out: &mut Array<Real>,
        nb_dof: usize,
        ty: ElementType,
        ghost: GhostType,
        filter: Option<&[Idx]>,
    ) -> Result<()> {
        if self.kind == ElementKind::Cohesive {
            return Err(Error::NotImplemented(
                "gradientOnIntegrationPoints for cohesive elements (AKANTU_TO_IMPLEMENT upstream)",
            ));
        }
        self.variation_impl(mesh, u, out, nb_dof, ty, ghost, filter, None)
    }

    /// [`FeEngine::gradient_on_integration_points`] into a fixed-shape view
    /// (e.g. a block of an exportable internal field reached through one
    /// write guard, D-047/D-056): the same checks in the same order and the
    /// same values, but `out` is never resized — it must already hold one
    /// tuple per quadrature point of the (filtered) elements, `nb_dof · nd`
    /// values each, otherwise `Error::DimensionMismatch` and nothing is
    /// written. On an error during the computation (bad connectivity) the
    /// entries of the elements before the failing one are already written,
    /// as upstream's `gradientOnIntegrationPoints` into its destination.
    #[allow(clippy::too_many_arguments)] // mirrors the upstream signature
    pub fn gradient_on_integration_points_view(
        &self,
        mesh: &Mesh,
        u: ArrayView<'_, Real>,
        out: ArrayViewMut<'_, Real>,
        nb_dof: usize,
        ty: ElementType,
        ghost: GhostType,
        filter: Option<&[Idx]>,
    ) -> Result<()> {
        if self.kind == ElementKind::Cohesive {
            return Err(Error::NotImplemented(
                "gradientOnIntegrationPoints for cohesive elements (AKANTU_TO_IMPLEMENT upstream)",
            ));
        }
        self.variation_into(
            mesh,
            u,
            GradientOut::Fixed(out),
            nb_dof,
            ty,
            ghost,
            filter,
            None,
        )
    }

    /// Gradient of a (reduced) nodal field w.r.t. the shape derivatives
    /// stored for `ty` (physical for regular, natural for cohesive types:
    /// upstream `variationOnIntegrationPoints`).
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn variation_impl(
        &self,
        mesh: &Mesh,
        u: ArrayView<'_, Real>,
        out: &mut Array<Real>,
        nb_dof: usize,
        ty: ElementType,
        ghost: GhostType,
        filter: Option<&[Idx]>,
        reduction: Option<CohesiveReduction>,
    ) -> Result<()> {
        self.variation_into(
            mesh,
            u,
            GradientOut::Resize(out),
            nb_dof,
            ty,
            ghost,
            filter,
            reduction,
        )
    }

    /// [`FeEngine::variation_impl`] into a resizable array or a fixed-shape
    /// view (the destination is prepared after every input check, where the
    /// array version resizes it).
    #[allow(clippy::too_many_arguments)]
    fn variation_into(
        &self,
        mesh: &Mesh,
        u: ArrayView<'_, Real>,
        out: GradientOut<'_>,
        nb_dof: usize,
        ty: ElementType,
        ghost: GhostType,
        filter: Option<&[Idx]>,
        reduction: Option<CohesiveReduction>,
    ) -> Result<()> {
        self.check_type(ty, "gradient_on_integration_points")?;
        check_input(
            "gradient_on_integration_points: nodal field",
            u.len(),
            u.nb_component(),
            mesh.nb_nodes(),
            nb_dof,
        )?;
        let block = Block::new(mesh, ty, ghost)?;
        let ids = batch_ids(filter, block.nb_element)?;
        let dshapes = self.shapes_derivatives(ty, ghost).map_err(|_| {
            Error::invalid(format!(
                "no shape derivatives for {} (natural dimension differs from the mesh dimension, \
                 or init_shape_functions not called)",
                ty.name()
            ))
        })?;
        let nq = self.nb_integration_points(ty, ghost)?;
        check_precomputed(
            "gradient_on_integration_points: precomputed shape derivatives",
            dshapes.len(),
            block.nb_element,
            nq,
        )?;
        with_element_class!(ty, E => {
            gradient_kernel::<E>(&self.exec, &block, u.as_slice(), nb_dof, dshapes.as_slice(), nq, &ids, reduction, out)
        }, else Err(Error::UnsupportedElementType { ty, context: "gradient_on_integration_points" }))
    }

    /// Integrate a quadrature field per element (upstream `integrate` → one
    /// tuple of `nb_dof` per element): `∫ f = Σ_q f_q · (det J · w)_q`,
    /// accumulated from `+0` (GEMV): bit-exact.
    pub fn integrate(
        &self,
        f: ArrayView<'_, Real>,
        out: &mut Array<Real>,
        nb_dof: usize,
        ty: ElementType,
        ghost: GhostType,
        filter: Option<&[Idx]>,
    ) -> Result<()> {
        self.check_type(ty, "integrate")?;
        let jac = self.jacobians(ty, ghost)?;
        let nq = self.nb_integration_points(ty, ghost)?;
        let ne = jac.len().checked_div(nq).unwrap_or(0);
        let ids = batch_ids(filter, ne)?;
        check_input(
            "integrate: quadrature field",
            f.len(),
            f.nb_component(),
            ids.len() * nq,
            nb_dof,
        )?;
        integrate_with_jacobians(
            &self.exec,
            f.as_slice(),
            nb_dof,
            nq,
            |pos| {
                let e = id_index(ids[pos]);
                &jac.as_slice()[e * nq..(e + 1) * nq]
            },
            ids.len(),
            out,
        )
    }

    /// Integrate a scalar quadrature field over all elements of a type
    /// (upstream `Real integrate(f, type, ghost, filter)`): per-element
    /// integrals combined with `akantu_core::math::reduce` (upstream
    /// `Math::reduce`, sorted in-place pairwise sum). An empty `f` gives `0`.
    pub fn integrate_scalar(
        &self,
        f: ArrayView<'_, Real>,
        ty: ElementType,
        ghost: GhostType,
        filter: Option<&[Idx]>,
    ) -> Result<Real> {
        if f.is_empty() {
            return Ok(0.0);
        }
        let mut per_element = Array::new(1);
        self.integrate(f, &mut per_element, 1, ty, ghost, filter)?;
        Ok(akantu_core::math::reduce(per_element.as_mut_slice()))
    }

    /// Integrate a quadrature field evaluated with the rule of order `order`
    /// (upstream `IntegratorGauss::integrate<type, polynomial_degree>`,
    /// FE-014): Jacobians computed on the fly with that rule (no sign
    /// check), multiplied by its weights; `f` has `nb_element·nq(order)`
    /// tuples.
    #[allow(clippy::too_many_arguments)]
    pub fn integrate_with_order(
        &self,
        mesh: &Mesh,
        f: ArrayView<'_, Real>,
        out: &mut Array<Real>,
        nb_dof: usize,
        ty: ElementType,
        ghost: GhostType,
        order: usize,
    ) -> Result<()> {
        self.check_type(ty, "integrate_with_order")?;
        let rule = crate::quadrature::gauss_rule(ty, order)?;
        let block = Block::new(mesh, ty, ghost)?;
        let nq = rule.nb_points();
        let ne = block.nb_element;
        check_input(
            "integrate_with_order: quadrature field",
            f.len(),
            f.nb_component(),
            ne * nq,
            nb_dof,
        )?;
        let ids: Vec<usize> = (0..ne).collect();
        let mut jac = vec![0.0; ne * nq];
        let kind = self.kind;
        with_element_class!(ty, E => {
            super::compute_jacobians::<E>(&block, &rule, kind, &ids, &ids, &mut jac)
        }, else Err(Error::UnsupportedElementType { ty, context: "integrate_with_order" }))?;
        super::multiply_by_weights(&mut jac, &rule.weights, &ids);
        integrate_with_jacobians(
            &self.exec,
            f.as_slice(),
            nb_dof,
            nq,
            |e| &jac[e * nq..(e + 1) * nq],
            ne,
            out,
        )
    }

    /// Scalar version of [`FeEngine::integrate_with_order`] (upstream
    /// `Real integrate<type, polynomial_degree>(f, ghost)`, `Math::reduce`).
    pub fn integrate_scalar_with_order(
        &self,
        mesh: &Mesh,
        f: ArrayView<'_, Real>,
        ty: ElementType,
        ghost: GhostType,
        order: usize,
    ) -> Result<Real> {
        let mut per_element = Array::new(1);
        self.integrate_with_order(mesh, f, &mut per_element, 1, ty, ghost, order)?;
        Ok(akantu_core::math::reduce(per_element.as_mut_slice()))
    }

    /// Filtered shape-derivative tuples of `(ty, ghost)` per element.
    fn derivatives_for(
        &self,
        ty: ElementType,
        ghost: GhostType,
        filter: Option<&[Idx]>,
    ) -> Result<(Vec<usize>, usize, &Array<Real>)> {
        let d = self.shapes_derivatives(ty, ghost)?;
        let nq = self.nb_integration_points(ty, ghost)?;
        let ne = d.len().checked_div(nq).unwrap_or(0);
        let filter = resolve_filter(filter, ne)?;
        Ok((element_ids(filter, ne), nq, d))
    }

    /// `Bᵀ D` per quadrature point (upstream `computeBtD`): `D` has
    /// `r·sdim` components (`r × sdim` column-major, e.g. a stress), the
    /// output `r·nn` (`D·B`, i.e. node-major nodal vectors).
    pub fn compute_btd(
        &self,
        d: ArrayView<'_, Real>,
        out: &mut Array<Real>,
        ty: ElementType,
        ghost: GhostType,
        filter: Option<&[Idx]>,
    ) -> Result<()> {
        self.check_type(ty, "compute_btd")?;
        if self.kind == ElementKind::Cohesive {
            return Err(Error::NotImplemented(
                "computeBtD for cohesive elements (AKANTU_TO_IMPLEMENT upstream)",
            ));
        }
        let (ids, nq, nn, sdim, r) = self.btd_operands(d, ty, ghost, filter)?;
        let dsh = self.shapes_derivatives(ty, ghost)?;
        prepare_out(out, d.len(), r * nn)?;
        let (bs, ds, os) = (dsh.as_slice(), d.as_slice(), out.as_mut_slice());
        // The common route with fixed-size loops (bitwise the same)
        if super::fast::btd(&self.exec, r, sdim, nn, &ids, nq, bs, ds, os).is_some() {
            return Ok(());
        }
        let bsz = sdim * nn;
        let p_db = eigen::prepare(Dims::dynamic(r, sdim), Dims::dynamic(sdim, nn))?;
        for (pos, &e) in ids.iter().enumerate() {
            let e = id_index(e);
            for q in 0..nq {
                let b = &bs[(e * nq + q) * bsz..(e * nq + q + 1) * bsz];
                let t = pos * nq + q;
                p_db.eval(
                    &ds[t * r * sdim..(t + 1) * r * sdim],
                    b,
                    &mut os[t * r * nn..(t + 1) * r * nn],
                )?;
            }
        }
        Ok(())
    }

    /// The checks of [`FeEngine::compute_btd`] (after the kind checks), in
    /// its order: `(ids, nq, nn, sdim, r)`.
    fn btd_operands<'f>(
        &self,
        d: ArrayView<'_, Real>,
        ty: ElementType,
        ghost: GhostType,
        filter: Option<&'f [Idx]>,
    ) -> Result<(std::borrow::Cow<'f, [Idx]>, usize, usize, usize, usize)> {
        let dsh = self.shapes_derivatives(ty, ghost)?;
        let nq = self.nb_integration_points(ty, ghost)?;
        let ne = dsh.len().checked_div(nq).unwrap_or(0);
        let ids = batch_ids(filter, ne)?;
        let nn = ty.nb_nodes_per_interpolation_element();
        let sdim = dsh.nb_component().checked_div(nn).unwrap_or(0);
        if sdim == 0 || d.nb_component() % sdim != 0 {
            return Err(Error::DimensionMismatch {
                context: "compute_btd: components of D",
                expected: sdim,
                got: d.nb_component(),
            });
        }
        let r = d.nb_component() / sdim;
        check_input(
            "compute_btd: D",
            d.len(),
            d.nb_component(),
            ids.len() * nq,
            r * sdim,
        )?;
        Ok((ids, nq, nn, sdim, r))
    }

    /// [`FeEngine::compute_btd`] of `d` into `scratch`, then
    /// [`FeEngine::integrate`] of `scratch` into `out` (`r·nn` components
    /// per element): the internal-force pattern `∫ Bᵀσ`. Same checks, errors
    /// and values as the two calls; on the common routes the point values
    /// are integrated as they are computed (`scratch` is then not written),
    /// which avoids writing and reading back one `r·nn` tuple per
    /// quadrature point. `scratch` is a buffer of the caller (its content
    /// on return is unspecified).
    #[allow(clippy::too_many_arguments)] // the two calls' arguments
    pub fn compute_btd_integrate(
        &self,
        d: ArrayView<'_, Real>,
        out: &mut Array<Real>,
        scratch: &mut Array<Real>,
        ty: ElementType,
        ghost: GhostType,
        filter: Option<&[Idx]>,
    ) -> Result<()> {
        self.check_type(ty, "compute_btd")?;
        if self.kind == ElementKind::Cohesive {
            return Err(Error::NotImplemented(
                "computeBtD for cohesive elements (AKANTU_TO_IMPLEMENT upstream)",
            ));
        }
        let (ids, nq, nn, sdim, r) = self.btd_operands(d, ty, ghost, filter)?;
        // `integrate` would pass its checks on the values of `compute_btd`
        // (`ids.len()·nq` tuples of `r·nn`) when the Jacobians cover the
        // same elements as the shape derivatives; otherwise the two calls
        // report their errors
        let bs = self.shapes_derivatives(ty, ghost)?.as_slice();
        if let Ok(jac) = self.jacobians(ty, ghost)
            && jac.len() * nn * sdim == bs.len()
        {
            prepare_out(out, ids.len(), r * nn)?;
            if super::fast::btd_integrate(
                &self.exec,
                r,
                sdim,
                nn,
                &ids,
                nq,
                bs,
                d.as_slice(),
                jac.as_slice(),
                out.as_mut_slice(),
            )
            .is_some()
            {
                return Ok(());
            }
        }
        drop(ids);
        self.compute_btd(d, scratch, ty, ghost, filter)?;
        self.integrate(scratch.view(), out, r * nn, ty, ghost, filter)
    }

    /// `Bᵀ D B` per quadrature point (upstream `computeBtDB`): `order_d == 4`
    /// (`D` a Voigt tangent, `B` the symmetric Voigt strain matrix of
    /// `VoigtHelper::transferBMatrixToSymVoigtBMatrix`, output
    /// `(dim·nn)²`), `order_d == 2` (`D` is `dim × dim`, output `nn²`).
    /// Upstream silently does nothing for other orders; the port returns
    /// `Error::InvalidArgument`.
    #[allow(clippy::too_many_arguments)] // mirrors the upstream signature
    pub fn compute_btdb(
        &self,
        d: ArrayView<'_, Real>,
        out: &mut Array<Real>,
        order_d: usize,
        ty: ElementType,
        ghost: GhostType,
        filter: Option<&[Idx]>,
    ) -> Result<()> {
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
        let (ids, nq, dsh) = self.derivatives_for(ty, ghost, filter)?;
        let nn = ty.nb_nodes_per_interpolation_element();
        let bsz = dim * nn;
        if dsh.nb_component() != bsz {
            return Err(Error::DimensionMismatch {
                context: "compute_btdb: shape derivatives (element dimension must equal the mesh dimension)",
                expected: bsz,
                got: dsh.nb_component(),
            });
        }
        let bs = dsh.as_slice();
        match order_d {
            4 => {
                let ts = akantu_core::voigt::voigt_table(dim)?.size;
                let n = dim * nn;
                check_input(
                    "compute_btdb: D",
                    d.len(),
                    d.nb_component(),
                    ids.len() * nq,
                    ts * ts,
                )?;
                prepare_out(out, d.len(), n * n)?;
                let mut bfull = MatrixX::zeros(dim, nn);
                let mut bv = MatrixX::zeros(ts, n);
                let mut btd = vec![0.0; n * ts];
                // (Bᵀ · D) into a dynamic temporary, then · B
                let p_btd = eigen::prepare(Dims::dynamic(ts, n).t(), Dims::dynamic(ts, ts))?;
                let p_btdb = eigen::prepare(Dims::dynamic(n, ts), Dims::dynamic(ts, n))?;
                let (ds, os) = (d.as_slice(), out.as_mut_slice());
                for (pos, &e) in ids.iter().enumerate() {
                    for q in 0..nq {
                        bfull
                            .as_mut_slice()
                            .copy_from_slice(&bs[(e * nq + q) * bsz..(e * nq + q + 1) * bsz]);
                        akantu_core::voigt::b_matrix_to_sym_voigt(dim, &bfull, &mut bv, nn)?;
                        let t = pos * nq + q;
                        let dq = &ds[t * ts * ts..(t + 1) * ts * ts];
                        let b = bv.as_slice();
                        p_btd.eval(b, dq, &mut btd)?;
                        p_btdb.eval(&btd, b, &mut os[t * n * n..(t + 1) * n * n])?;
                    }
                }
                Ok(())
            }
            2 => {
                check_input(
                    "compute_btdb: D",
                    d.len(),
                    d.nb_component(),
                    ids.len() * nq,
                    dim * dim,
                )?;
                prepare_out(out, d.len(), nn * nn)?;
                let mut btd = vec![0.0; nn * dim];
                let p_btd = eigen::prepare(Dims::dynamic(dim, nn).t(), Dims::dynamic(dim, dim))?;
                let p_btdb = eigen::prepare(Dims::dynamic(nn, dim), Dims::dynamic(dim, nn))?;
                let (ds, os) = (d.as_slice(), out.as_mut_slice());
                for (pos, &e) in ids.iter().enumerate() {
                    for q in 0..nq {
                        let b = &bs[(e * nq + q) * bsz..(e * nq + q + 1) * bsz];
                        let t = pos * nq + q;
                        let dq = &ds[t * dim * dim..(t + 1) * dim * dim];
                        p_btd.eval(b, dq, &mut btd)?;
                        p_btdb.eval(&btd, b, &mut os[t * nn * nn..(t + 1) * nn * nn])?;
                    }
                }
                Ok(())
            }
            _ => Err(Error::InvalidArgument(format!(
                "compute_btdb: order_d must be 2 or 4, got {order_d} (upstream silently computes nothing)"
            ))),
        }
    }

    /// `Nᵀ b` per quadrature point (upstream `computeNtb`): `b·N`, output
    /// `nb_dof·nn` (node-major).
    pub fn compute_ntb(
        &self,
        b: ArrayView<'_, Real>,
        out: &mut Array<Real>,
        ty: ElementType,
        ghost: GhostType,
        filter: Option<&[Idx]>,
    ) -> Result<()> {
        self.check_type(ty, "compute_ntb")?;
        if self.kind == ElementKind::Cohesive {
            return Err(Error::NotImplemented(
                "computeNtb for cohesive elements (AKANTU_TO_IMPLEMENT upstream)",
            ));
        }
        let (ids, nq, shapes) = self.shapes_for(ty, ghost, filter)?;
        let nn = shapes.nb_component();
        let nb_dof = b.nb_component();
        check_input("compute_ntb: b", b.len(), nb_dof, ids.len() * nq, nb_dof)?;
        prepare_out(out, b.len(), nb_dof * nn)?;
        let (ss, bs, os) = (shapes.as_slice(), b.as_slice(), out.as_mut_slice());
        let p_bn = eigen::prepare(Dims::dynamic(nb_dof, 1), Dims::dynamic(1, nn))?;
        for (pos, &e) in ids.iter().enumerate() {
            for q in 0..nq {
                let t = pos * nq + q;
                let n = &ss[(e * nq + q) * nn..(e * nq + q + 1) * nn];
                p_bn.eval(
                    &bs[t * nb_dof..(t + 1) * nb_dof],
                    n,
                    &mut os[t * nb_dof * nn..(t + 1) * nb_dof * nn],
                )?;
            }
        }
        Ok(())
    }

    /// `Nᵀ b N` per quadrature point (upstream `computeNtbN`), scalar `b`
    /// only (upstream's expression is only well formed for one component);
    /// output `nn²`.
    pub fn compute_ntbn(
        &self,
        b: ArrayView<'_, Real>,
        out: &mut Array<Real>,
        ty: ElementType,
        ghost: GhostType,
        filter: Option<&[Idx]>,
    ) -> Result<()> {
        self.check_type(ty, "compute_ntbn")?;
        if self.kind == ElementKind::Cohesive {
            return Err(Error::NotImplemented(
                "computeNtbN for cohesive elements (AKANTU_TO_IMPLEMENT upstream)",
            ));
        }
        let (ids, nq, shapes) = self.shapes_for(ty, ghost, filter)?;
        let nn = shapes.nb_component();
        check_input(
            "compute_ntbn: b (one component)",
            b.len(),
            b.nb_component(),
            ids.len() * nq,
            1,
        )?;
        prepare_out(out, b.len(), nn * nn)?;
        let mut ntb = vec![0.0; nn];
        let (ss, bs, os) = (shapes.as_slice(), b.as_slice(), out.as_mut_slice());
        // N.transpose() (nn × 1, N a 1 × nn map) · b (1 × 1), then · N
        let p_ntb = eigen::prepare(Dims::dynamic(1, nn).t(), Dims::dynamic(1, 1))?;
        let p_ntbn = eigen::prepare(Dims::dynamic(nn, 1), Dims::dynamic(1, nn))?;
        for (pos, &e) in ids.iter().enumerate() {
            for q in 0..nq {
                let t = pos * nq + q;
                let n = &ss[(e * nq + q) * nn..(e * nq + q + 1) * nn];
                p_ntb.eval(n, &bs[t..t + 1], &mut ntb)?;
                p_ntbn.eval(&ntb, n, &mut os[t * nn * nn..(t + 1) * nn * nn])?;
            }
        }
        Ok(())
    }

    /// Filtered shapes of `(ty, ghost)`.
    fn shapes_for(
        &self,
        ty: ElementType,
        ghost: GhostType,
        filter: Option<&[Idx]>,
    ) -> Result<(Vec<usize>, usize, &Array<Real>)> {
        let s = self.shapes(ty, ghost)?;
        let nq = self.nb_integration_points(ty, ghost)?;
        let ne = s.len().checked_div(nq).unwrap_or(0);
        let filter = resolve_filter(filter, ne)?;
        Ok((element_ids(filter, ne), nq, s))
    }
}

/// Per-element integrals `out_e = Σ_q f_{e,q} J_{e,q}` (GEMV, from `+0`).
pub(crate) fn integrate_with_jacobians<'j>(
    exec: &akantu_core::exec::Exec,
    f: &[Real],
    nb_dof: usize,
    nq: usize,
    jac: impl Fn(usize) -> &'j [Real] + Sync,
    nb_element: usize,
    out: &mut Array<Real>,
) -> Result<()> {
    prepare_out(out, nb_element, nb_dof)?;
    let o = out.as_mut_slice();
    // The common route with plain loops (bitwise the same)
    if super::fast::integrate(exec, f, nb_dof, nq, &jac, nb_element, o).is_some() {
        return Ok(());
    }
    if nb_dof == 1 && super::fast::integrate_scalar(f, nq, &jac, nb_element, o).is_some() {
        return Ok(());
    }
    let p_fj = eigen::prepare(Dims::dynamic(nb_dof, nq), Dims::new(nq, 1, false, true))?;
    for pos in 0..nb_element {
        let j = jac(pos);
        let fe = &f[pos * nq * nb_dof..(pos + 1) * nq * nb_dof];
        p_fj.eval(fe, j, &mut o[pos * nb_dof..(pos + 1) * nb_dof])?;
    }
    Ok(())
}

/// Destination of a gradient: an array resized by the callee (the
/// historical contract of the FE engine) or a fixed-shape view whose shape
/// is checked instead (a block of an exportable field written
/// through one guard, without a temporary and a copy).
pub(crate) enum GradientOut<'o> {
    /// Resized to `len` tuples of `nb_component` values.
    Resize(&'o mut Array<Real>),
    /// Must already have exactly that shape.
    Fixed(ArrayViewMut<'o, Real>),
}

impl GradientOut<'_> {
    /// The destination values, `len` tuples of `nb_component` (resized, or
    /// checked for a view: nothing is written on a mismatch).
    fn prepare(&mut self, len: usize, nb_component: usize) -> Result<&mut [Real]> {
        match self {
            Self::Resize(out) => {
                prepare_out(out, len, nb_component)?;
                Ok(out.as_mut_slice())
            }
            Self::Fixed(view) => {
                if view.nb_component() != nb_component {
                    return Err(Error::DimensionMismatch {
                        context: "gradient_on_integration_points: output view (components)",
                        expected: nb_component,
                        got: view.nb_component(),
                    });
                }
                if view.len() != len {
                    return Err(Error::DimensionMismatch {
                        context: "gradient_on_integration_points: output view (tuples)",
                        expected: len,
                        got: view.len(),
                    });
                }
                Ok(view.as_mut_slice())
            }
        }
    }
}

/// `∇u = U · Bᵀ` per point with upstream's operand types
/// (`U`: `Map<Matrix<Dynamic, nn>>`, `B`: `Map<Matrix<nd, nn>>`).
#[allow(clippy::too_many_arguments)]
fn gradient_kernel<E: LagrangeElement>(
    exec: &akantu_core::exec::Exec,
    block: &Block<'_>,
    u: &[Real],
    nb_dof: usize,
    dshapes: &[Real],
    nq: usize,
    ids: &[Idx],
    reduction: Option<CohesiveReduction>,
    mut out: GradientOut<'_>,
) -> Result<()> {
    let nd = E::NATURAL_DIM;
    let nn = E::NB_SHAPES;
    if nd == 0 {
        // point elements: upstream specialisation leaves zeros
        return out.prepare(ids.len() * nq, 0).map(|_| ());
    }
    let o = out.prepare(ids.len() * nq, nb_dof * nd)?;
    // The common route with fixed-size loops (bitwise the same)
    if reduction.is_none()
        && let Some(r) = super::fast::gradient::<E>(exec, block, u, nb_dof, dshapes, nq, ids, o)
    {
        return r;
    }
    let bsz = nd * nn;
    let mut scratch = vec![0.0; nb_dof * block.nnpe];
    let mut ue = vec![0.0; nb_dof * nn];
    let p_ub = eigen::prepare(Dims::new(nb_dof, nn, false, true), Dims::fixed(nd, nn).t())?;
    for (pos, &e) in ids.iter().enumerate() {
        let e = id_index(e);
        FeEngine::gather_element(block, e, u, nb_dof, reduction, &mut scratch, &mut ue)?;
        for q in 0..nq {
            let b = &dshapes[(e * nq + q) * bsz..(e * nq + q + 1) * bsz];
            let t = pos * nq + q;
            let g = &mut o[t * nb_dof * nd..(t + 1) * nb_dof * nd];
            p_ub.eval(&ue, b, g)?;
        }
    }
    Ok(())
}
