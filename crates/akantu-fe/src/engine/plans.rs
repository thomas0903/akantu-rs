// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/fe_engine/fe_engine_template_tmpl.hh
// (gradientOnIntegrationPoints, computeBtD, integrate), the operations
// the solid-mechanics internal-force assembly chains per element type.

//! Prepared kernels for element-block pipelines.
//!
//! A plan checks the operands of one FE operation over the elements of a
//! filter once (every check of the corresponding [`FeEngine`] method, the
//! product routes of its fixed-size kernel, the connectivity) and then runs
//! the kernel on any range of the filter's elements, serially, with the
//! bits of the full call. A caller can so chain several operations over
//! blocks of elements that fit a cache (gradient, constitutive law,
//! `BᵀD` and integration of a solid law) instead of sweeping the whole
//! mesh once per operation; the arrays shared with other code receive the
//! same values. A plan is `None` whenever the full call would not take the
//! fixed-size kernel (another route, a bad size or connectivity): the
//! caller then runs the full calls, which report their errors.

use akantu_core::array::ArrayView;
use akantu_core::exec::{FastToken, SimdToken};
use akantu_core::{ElementKind, ElementType, Error, GhostType, Idx, Real, Result};
use akantu_mesh::Mesh;

use super::{Block, FeEngine, batch_ids, check_input, check_precomputed, fast};
use crate::element_class::{ElementClass, LagrangeElement};
use crate::with_element_class;

/// [`FeEngine::gradient_on_integration_points_view`] of a nodal field over
/// the elements of a filter, prepared for range calls
/// ([`FeEngine::gradient_plan`]).
pub struct GradientPlan<'a> {
    block: Block<'a>,
    u: &'a [Real],
    dshapes: &'a [Real],
    filter: &'a [Idx],
    nq: usize,
    nb_dof: usize,
    nd: usize,
    fast: Option<FastToken>,
    simd: SimdToken,
    run: GradientRun,
}

/// The kernel of a plan's element class: `(plan, ids, minus, out)`.
type GradientRun = fn(&GradientPlan<'_>, &[Idx], &[Real], &mut [Real]);

impl std::fmt::Debug for GradientPlan<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GradientPlan")
            .field("ty", &self.block.ty)
            .field("elements", &self.filter.len())
            .field("nq", &self.nq)
            .field("nb_dof", &self.nb_dof)
            .finish_non_exhaustive()
    }
}

fn gradient_run<E: LagrangeElement>(
    p: &GradientPlan<'_>,
    ids: &[Idx],
    minus: &[Real],
    out: &mut [Real],
) {
    let g = fast::Gradient {
        conn: p.block.conn(),
        u: p.u,
        dshapes: p.dshapes,
        nq: p.nq,
        ids,
        minus,
        out,
        element: std::marker::PhantomData::<E>,
    };
    fast::run_gradient(p.fast, p.simd, p.nb_dof, g);
}

impl GradientPlan<'_> {
    /// Quadrature points per element.
    #[must_use]
    pub fn nb_points_per_element(&self) -> usize {
        self.nq
    }

    /// Values per quadrature point (`nb_dof · nd`).
    #[must_use]
    pub fn tuple_size(&self) -> usize {
        self.nb_dof * self.nd
    }

    /// Number of elements of the filter.
    #[must_use]
    pub fn nb_elements(&self) -> usize {
        self.filter.len()
    }

    /// The gradient of the filter's elements `first .. first + n`, `n =
    /// out.len() / (nq · tuple_size)`, into `out` (their quadrature points
    /// in filter order): the values the full call writes for them. With a
    /// non-empty `minus` (the same length as `out`) each value is
    /// `fl(∇u − minus)`, as the full call followed by `out -= minus`.
    ///
    /// # Errors
    /// `Error::DimensionMismatch` when `out` is not a whole number of
    /// elements or a non-empty `minus` has another length;
    /// `Error::OutOfRange` when the elements run past the filter. Nothing is
    /// written then.
    pub fn run(&self, first: usize, minus: &[Real], out: &mut [Real]) -> Result<()> {
        let per = self.nq * self.tuple_size();
        if per == 0 || !out.len().is_multiple_of(per) {
            return Err(Error::DimensionMismatch {
                context: "GradientPlan::run: output (whole elements)",
                expected: per,
                got: out.len(),
            });
        }
        if !minus.is_empty() && minus.len() != out.len() {
            return Err(Error::DimensionMismatch {
                context: "GradientPlan::run: minus (the output's length)",
                expected: out.len(),
                got: minus.len(),
            });
        }
        let ids = filter_range(self.filter, first, out.len() / per)?;
        (self.run)(self, ids, minus, out);
        Ok(())
    }
}

/// The filter's entries `first .. first + n`, or `Error::OutOfRange`.
fn filter_range(filter: &[Idx], first: usize, n: usize) -> Result<&[Idx]> {
    first
        .checked_add(n)
        .and_then(|end| filter.get(first..end))
        .ok_or(Error::OutOfRange {
            what: "plan: element range of the filter",
            index: first.saturating_add(n),
            len: filter.len(),
        })
}

/// [`FeEngine::compute_btd_integrate`] over the elements of a filter,
/// prepared for range calls ([`FeEngine::btd_integrate_plan`]).
pub struct BtdIntegratePlan<'a> {
    filter: &'a [Idx],
    bs: &'a [Real],
    jac: &'a [Real],
    nq: usize,
    nn: usize,
    r: usize,
    sdim: usize,
    simd: SimdToken,
    fast: Option<FastToken>,
}

impl std::fmt::Debug for BtdIntegratePlan<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BtdIntegratePlan")
            .field("elements", &self.filter.len())
            .field("nq", &self.nq)
            .field("nn", &self.nn)
            .field("r", &self.r)
            .finish_non_exhaustive()
    }
}

impl BtdIntegratePlan<'_> {
    /// Values per element of the output (`r · nn`).
    #[must_use]
    pub fn element_size(&self) -> usize {
        self.r * self.nn
    }

    /// Values of `D` per element (`nq · r · sdim`).
    #[must_use]
    pub fn operand_size(&self) -> usize {
        self.nq * self.r * self.sdim
    }

    /// `∫ D·B` of the filter's elements `first .. first + n`, `n =
    /// out.len() / element_size`, from their `D` values `d` (quadrature
    /// points in filter order): the values the full call writes for them.
    ///
    /// # Errors
    /// `Error::DimensionMismatch` when `out` is not a whole number of
    /// elements or `d` holds another number of elements;
    /// `Error::OutOfRange` when the elements run past the filter. Nothing is
    /// written then.
    pub fn run(&self, first: usize, d: &[Real], out: &mut [Real]) -> Result<()> {
        let es = self.element_size();
        let n = out.len() / es;
        if out.len() != n * es {
            return Err(Error::DimensionMismatch {
                context: "BtdIntegratePlan::run: output (whole elements)",
                expected: es,
                got: out.len(),
            });
        }
        if d.len() != n * self.operand_size() {
            return Err(Error::DimensionMismatch {
                context: "BtdIntegratePlan::run: D (the output's elements)",
                expected: n * self.operand_size(),
                got: d.len(),
            });
        }
        let ids = filter_range(self.filter, first, n)?;
        fast::btd_integrate_serial(
            self.simd, self.fast, self.r, self.sdim, self.nn, ids, self.nq, self.bs, d, self.jac,
            out,
        );
        Ok(())
    }
}

impl FeEngine {
    /// A [`GradientPlan`] of `u` (`nb_dof` components per node) over the
    /// elements `filter` of `(ty, ghost)`, or `None` when
    /// [`FeEngine::gradient_on_integration_points_view`] would report an
    /// error or not take its fixed-size kernel (the caller then makes the
    /// full call). The plan's results are those of the full call for every
    /// range (same kernel, same executor precision and SIMD level).
    #[must_use]
    pub fn gradient_plan<'a>(
        &'a self,
        mesh: &'a Mesh,
        u: ArrayView<'a, Real>,
        nb_dof: usize,
        ty: ElementType,
        ghost: GhostType,
        filter: &'a [Idx],
    ) -> Option<GradientPlan<'a>> {
        if self.kind == ElementKind::Cohesive || self.check_type(ty, "").is_err() {
            return None;
        }
        check_input("", u.len(), u.nb_component(), mesh.nb_nodes(), nb_dof).ok()?;
        let block = Block::new(mesh, ty, ghost).ok()?;
        batch_ids(Some(filter), block.nb_element).ok()?;
        let dshapes = self.shapes_derivatives(ty, ghost).ok()?;
        let nq = self.nb_integration_points(ty, ghost).ok()?;
        check_precomputed("", dshapes.len(), block.nb_element, nq).ok()?;
        let (nd, run): (usize, GradientRun) = with_element_class!(ty, E => {
            (E::NATURAL_DIM, gradient_run::<E> as GradientRun)
        }, else return None);
        let plan = GradientPlan {
            u: u.as_slice(),
            dshapes: dshapes.as_slice(),
            filter,
            nq,
            nb_dof,
            nd,
            fast: self.exec.fast_token(),
            simd: self.exec.simd_token(),
            run,
            block,
        };
        let ok = with_element_class!(ty, E => {
            fast::gradient_supported::<E>(&self.exec, &plan.block, plan.u, nb_dof, plan.dshapes, nq, filter)
        }, else false);
        ok.then_some(plan)
    }

    /// A [`BtdIntegratePlan`] of a `D` field with `nb_component` values per
    /// point over the elements `filter` of `(ty, ghost)`, or `None` when
    /// [`FeEngine::compute_btd_integrate`] would report an error or not take
    /// its fused kernel.
    #[must_use]
    pub fn btd_integrate_plan<'a>(
        &'a self,
        nb_component: usize,
        ty: ElementType,
        ghost: GhostType,
        filter: &'a [Idx],
    ) -> Option<BtdIntegratePlan<'a>> {
        if self.kind == ElementKind::Cohesive || self.check_type(ty, "").is_err() {
            return None;
        }
        let dsh = self.shapes_derivatives(ty, ghost).ok()?;
        let nq = self.nb_integration_points(ty, ghost).ok()?;
        let ne = dsh.len().checked_div(nq)?;
        batch_ids(Some(filter), ne).ok()?;
        let nn = ty.nb_nodes_per_interpolation_element();
        let sdim = dsh.nb_component().checked_div(nn)?;
        if sdim == 0 || !nb_component.is_multiple_of(sdim) {
            return None;
        }
        let r = nb_component / sdim;
        let jac = self.jacobians(ty, ghost).ok()?;
        if jac.len() != dsh.len() {
            return None;
        }
        let bs = dsh.as_slice();
        fast::btd_integrate_supported(r, sdim, nn, filter, nq, bs, jac.as_slice()).then_some(
            BtdIntegratePlan {
                filter,
                bs,
                jac: jac.as_slice(),
                nq,
                nn,
                r,
                sdim,
                simd: self.exec.simd_token(),
                fast: self.exec.fast_token(),
            },
        )
    }
}
