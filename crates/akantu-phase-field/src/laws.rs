// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/model/phase_field/phasefields/
// phasefield_quadratic{.hh,.cc,_inline_impl.hh},
// phasefield_linear{.hh,.cc,_inline_impl.hh}; phasefield.hh
// (instantiatePhaseField).

//! The built-in phase-field laws (PF-006/007, PF-013…018), registered as
//! `quadratic` (AT2) and `linear` (AT1), each with the energy splits
//! `no_split` (default) and `volumetric_deviatoric` (allocator option).
//!
//! | | `quadratic` (AT2) | `linear` (AT1) |
//! |---|---|---|
//! | `damage_energy` | `g_c l0 I` | `¾ g_c l0 I` |
//! | `previous phi` (init) | `0` | `3 g_c / (16 l0)` |
//! | `damage_energy_density` | `2φ + g_c/l0` | `2φ` |
//! | `driving_force` | `−2φ + d·(2φ + g_c/l0)` | `−2φ + 3g_c/(8l0) + d·2φ` |
//! | dissipated energy density | `½g_c l0 ∇d·∇d + g_c d²/(2l0)` | `⅜g_c l0 ∇d·∇d + 3g_c d/(8l0)` |
//! | penalty `γ` | `g_c/l0·(1/tol_ir² − 1)` | `g_c/l0·27/(64 tol_ir²)` |
//! | recovery `ρ_rec` | — | `g_c/l0·9(diam/l0 − 2)/(64 tol_rec)` |
//!
//! `g_c` in `γ`, `ρ_rec` is the base value of the `gc` parameter (upstream
//! `Real(g_c)`), `diam` the diagonal of the mesh bounding box. With
//! `irreversibility_type = penalization` the driving force gains
//! `γ·min(0, d − d_prev)` (AT1: `+ ρ_rec·min(0, d)`) and the density
//! `γ·[d < d_prev]` (AT1: `+ ρ_rec·[d < 0]`); with `history` (default)
//! `φ ← max(φ, φ_prev)`; with `tao` neither (bounds come from the solver).
//!
//! Corrected defaults of the AT1 penalisation (D-003 category B, switches in
//! [`crate::PhaseFieldUpstreamCompat`]):
//!
//! * **UD-385**: `ρ_rec ≤ 0` (`diam ≤ 2 l0`) with `penalization` is refused
//!   at `initPhaseField` (upstream: anti-penalty, Newton can diverge);
//! * **UD-388**: the irreversibility tangent is the active one-sided
//!   derivative `γ·[d ≤ d_prev]` (upstream `γ·[d < d_prev]` is 0 at the
//!   first iterate of every step, singular `K` when `φ = 0`). Cost: a
//!   loading step that starts at `d = d_prev` takes one more Newton
//!   correction (the first one is damped by `γ`).

use std::sync::Arc;

use akantu_core::{Element, ElementType, Error, GhostType, Real, Result};
use akantu_model::constitutive_law::{ConstitutiveLaw, LawContext, LawSpec, ModelServices};
use akantu_parser::param::{ParamDescriptor, ParamKind, ParamValue, ParameterAccess};

use crate::energy_split::{EnergySplit, EnergySplitKind};
use crate::phasefield::{
    DrivingLaw, PhaseField, PhaseFieldBase, PhaseFieldParams, dissipated_energy_by_element_kernel,
    dissipated_energy_kernel, driving_force_kernel, impl_phasefield_law, with_driving_block,
};

/// Factory of phase-field laws (upstream `PhaseFieldFactory`).
pub type PhaseFieldFactory = akantu_model::ConstitutiveLawFactory<dyn PhaseField>;

/// `std::min(Real(0.), x)`.
#[inline(always)]
fn min_zero(x: Real) -> Real {
    if x < 0.0 { x } else { 0.0 }
}

/// `Real(bool)`.
#[inline(always)]
fn indicator(b: bool) -> Real {
    if b { 1.0 } else { 0.0 }
}

fn not_initialised(id: &str) -> Error {
    Error::invalid(format!(
        "phase field {id}: initPhaseField has not been called (no energy split)"
    ))
}

const PARSE_READ: ParameterAccess =
    ParameterAccess(ParameterAccess::PARSABLE.0 | ParameterAccess::READABLE.0);

// ------------------------------------------------------------------ AT2

/// `quadratic` law (upstream `PhaseFieldQuadratic<dim, EnergySplit>`, AT2).
#[derive(Clone, Debug)]
pub struct PhaseFieldQuadratic {
    /// Common state.
    pub base: PhaseFieldBase,
    /// Energy split (allocator option).
    pub split_kind: EnergySplitKind,
    /// `irreversibility_tol` (default `1e-2`).
    pub tol_ir: Real,
    /// Penalty `γ` (set by `initPhaseField`).
    pub gamma: Real,
    split: Option<EnergySplit>,
}

const QUADRATIC_PARAMS: &[ParamDescriptor<PhaseFieldQuadratic>] = &[ParamDescriptor::new(
    "irreversibility_tol",
    ParamKind::Real,
    PARSE_READ,
    "Irreversibility tolerance",
    |s| ParamValue::Real(s.tol_ir),
    Some(|s, v| {
        s.tol_ir = v.as_real()?;
        Ok(())
    }),
)];

impl PhaseFieldQuadratic {
    /// Upstream constructor (`registerParam("irreversibility_tol", tol_ir,
    /// 1e-2)`).
    pub fn new(
        spec: &LawSpec,
        split_kind: EnergySplitKind,
        mesh: &akantu_mesh::Mesh,
    ) -> Result<Self> {
        Ok(Self {
            base: PhaseFieldBase::new(spec, "quadratic", mesh)?,
            split_kind,
            tol_ir: 1e-2,
            gamma: 0.0,
            split: None,
        })
    }
}

impl PhaseFieldParams for PhaseFieldQuadratic {
    fn params_base(&self) -> &PhaseFieldBase {
        &self.base
    }
    fn params_base_mut(&mut self) -> &mut PhaseFieldBase {
        &mut self.base
    }
    fn own_params(&self) -> &'static [ParamDescriptor<Self>] {
        QUADRATIC_PARAMS
    }
    /// Upstream `PhaseFieldQuadratic::updateInternalParameters`: the base
    /// update, then for the not-ghost points `damage_energy = g_c l0 I` and
    /// `previous phi = 0`.
    fn update_params(&mut self) -> Result<()> {
        self.base.update_internal_parameters()?;
        self.base.init_damage_energy_and_history(None, |_| 0.0)
    }
}

impl_phasefield_law!(PhaseFieldQuadratic);

impl PhaseField for PhaseFieldQuadratic {
    fn base(&self) -> &PhaseFieldBase {
        &self.base
    }
    fn base_mut(&mut self) -> &mut PhaseFieldBase {
        &mut self.base
    }
    fn energy_split(&self) -> Option<&EnergySplit> {
        self.split.as_ref()
    }

    /// Upstream `initPhaseField`: `initConstitutiveLaw` (resize the
    /// internals, `updateInternalParameters`), then
    /// `γ = g_c/l0·(1/tol_ir² − 1)` and the energy split.
    fn init_phase_field(&mut self, ctx: &LawContext<'_>) -> Result<()> {
        let blocks = self.base.blocks(ctx);
        self.resize_internals(&blocks)?;
        self.update_params()?;
        self.base.set_init();
        self.gamma = self.base.gc() / self.base.l0 * (1. / (self.tol_ir * self.tol_ir) - 1.);
        self.split = Some(EnergySplit::new(
            self.split_kind,
            self.base.e,
            self.base.nu,
            self.base.plane_stress,
        ));
        Ok(())
    }

    fn compute_driving_force(&mut self, ty: ElementType, ghost: GhostType) -> Result<()> {
        let split = self.split.ok_or_else(|| not_initialised(&self.base.id))?;
        let l0 = self.base.l0;
        let gamma = self.gamma;
        let densities = |phi: Real, gc: Real| (2.0 * phi + gc / l0, -2.0 * phi);
        let penalty = |d: Real, dp: Real, density: Real| {
            (
                gamma * min_zero(d - dp),
                density + gamma * indicator(d < dp),
            )
        };
        let law = DrivingLaw {
            split: &split,
            upstream_phi: self.base.compat.in_block_deviator_energy,
            use_history: self.base.use_history,
            use_penalization: self.base.use_penalization,
            densities: &densities,
            penalty: &penalty,
        };
        let dim = self.base.spatial_dimension;
        akantu_core::with_dim!(dim, D => with_driving_block(&mut self.base, ty, ghost, |b| {
            driving_force_kernel::<D>(&law, b)
        }))?
    }

    fn compute_dissipated_energy(&mut self, ty: ElementType) -> Result<()> {
        let l0 = self.base.l0;
        let density = move |d: Real, g: &[Real], gc: Real| quadratic_density(l0, d, g, gc);
        let dim = self.base.spatial_dimension;
        akantu_core::with_dim!(dim, D => dissipated_energy_kernel::<D>(&mut self.base, ty, &density))?
    }

    fn dissipated_energy_by_element(
        &self,
        ty: ElementType,
        index: usize,
        out: &mut [Real],
    ) -> Result<()> {
        let l0 = self.base.l0;
        let density = move |d: Real, g: &[Real], gc: Real| quadratic_density(l0, d, g, gc);
        let dim = self.base.spatial_dimension;
        let nq = out.len();
        akantu_core::with_dim!(dim, D => dissipated_energy_by_element_kernel::<D>(
            &self.base, ty, index, nq, out, &density
        ))?
    }
}

/// Upstream `PhaseFieldQuadratic::computeDissipatedEnergyOnQuad`.
fn quadratic_density(l0: Real, dam: Real, grad_d: &[Real], gc: Real) -> Real {
    let mut edis = 0.;
    for &g in grad_d {
        edis += 0.5 * gc * l0 * g * g;
    }
    edis += gc * dam * dam / (2. * l0);
    edis
}

// ------------------------------------------------------------------ AT1

/// `linear` law (upstream `PhaseFieldLinear<dim, EnergySplit>`, AT1).
#[derive(Clone, Debug)]
pub struct PhaseFieldLinear {
    /// Common state.
    pub base: PhaseFieldBase,
    /// Energy split (allocator option).
    pub split_kind: EnergySplitKind,
    /// `irreversibility_tol` (default `1e-2`).
    pub tol_ir: Real,
    /// `recovery_tol` (default `1e-2`).
    pub tol_rec: Real,
    /// Irreversibility penalty `γ` (`initPhaseField`).
    pub gamma: Real,
    /// Recovery penalty `ρ_rec` (`initPhaseField`).
    pub rho_rec: Real,
    split: Option<EnergySplit>,
}

const LINEAR_PARAMS: &[ParamDescriptor<PhaseFieldLinear>] = &[
    ParamDescriptor::new(
        "irreversibility_tol",
        ParamKind::Real,
        PARSE_READ,
        "Irreversibility tolerance",
        |s| ParamValue::Real(s.tol_ir),
        Some(|s, v| {
            s.tol_ir = v.as_real()?;
            Ok(())
        }),
    ),
    ParamDescriptor::new(
        "recovery_tol",
        ParamKind::Real,
        PARSE_READ,
        "Recovery tolerance",
        |s| ParamValue::Real(s.tol_rec),
        Some(|s, v| {
            s.tol_rec = v.as_real()?;
            Ok(())
        }),
    ),
];

impl PhaseFieldLinear {
    /// Upstream constructor (`irreversibility_tol`, `recovery_tol`, both
    /// `1e-2`).
    pub fn new(
        spec: &LawSpec,
        split_kind: EnergySplitKind,
        mesh: &akantu_mesh::Mesh,
    ) -> Result<Self> {
        Ok(Self {
            base: PhaseFieldBase::new(spec, "linear", mesh)?,
            split_kind,
            tol_ir: 1e-2,
            tol_rec: 1e-2,
            gamma: 0.0,
            rho_rec: 0.0,
            split: None,
        })
    }
}

impl PhaseFieldParams for PhaseFieldLinear {
    fn params_base(&self) -> &PhaseFieldBase {
        &self.base
    }
    fn params_base_mut(&mut self) -> &mut PhaseFieldBase {
        &mut self.base
    }
    fn own_params(&self) -> &'static [ParamDescriptor<Self>] {
        LINEAR_PARAMS
    }
    /// Upstream `PhaseFieldLinear::updateInternalParameters`: the base
    /// update, then for the not-ghost points `damage_energy = ¾ g_c l0 I`
    /// and `previous phi = 3 g_c / (16 l0)`.
    fn update_params(&mut self) -> Result<()> {
        self.base.update_internal_parameters()?;
        let l0 = self.base.l0;
        self.base
            .init_damage_energy_and_history(Some(3. / 4.), |gc| 3. * gc / (16. * l0))
    }
}

impl_phasefield_law!(PhaseFieldLinear);

impl PhaseField for PhaseFieldLinear {
    fn base(&self) -> &PhaseFieldBase {
        &self.base
    }
    fn base_mut(&mut self) -> &mut PhaseFieldBase {
        &mut self.base
    }
    fn energy_split(&self) -> Option<&EnergySplit> {
        self.split.as_ref()
    }

    /// Upstream `initPhaseField`: `initConstitutiveLaw`, then
    /// `γ = g_c/l0·27/(64 tol_ir²)`, `ρ_rec = g_c/l0·9(diam/l0 − 2)/(64
    /// tol_rec)` with `diam` the diagonal of the mesh bounding box (first
    /// `dim` directions), and the energy split.
    ///
    /// # Errors
    /// An empty mesh bounding box; with `irreversibility_type =
    /// penalization`, a recovery penalty `ρ_rec ≤ 0` (UD-385, unless
    /// [`crate::PhaseFieldUpstreamCompat::accept_nonpositive_recovery_penalty`]).
    fn init_phase_field(&mut self, ctx: &LawContext<'_>) -> Result<()> {
        let blocks = self.base.blocks(ctx);
        self.resize_internals(&blocks)?;
        self.update_params()?;
        let gc = self.base.gc();
        let l0 = self.base.l0;
        self.gamma = gc / l0 * 27. / (64. * self.tol_ir * self.tol_ir);
        let bbox = ctx.services.mesh().bbox();
        let mut diam: Real = 0.;
        for i in 0..self.base.spatial_dimension {
            let s = match (bbox.upper.get(i), bbox.lower.get(i)) {
                // an empty box (a programmatic mesh without upstream's
                // `makeReady`/`compute_bounding_box`) would give an
                // infinite ρ_rec and NaN penalties upstream: refused
                (Some(u), Some(l)) if u >= l && (u - l).is_finite() => u - l,
                (Some(_), Some(_)) => {
                    return Err(Error::invalid(
                        "PhaseFieldLinear::initPhaseField: the mesh bounding box is empty \
                         (compute it with Mesh::compute_bounding_box, upstream makeReady)",
                    ));
                }
                _ => {
                    return Err(Error::DimensionMismatch {
                        context: "PhaseFieldLinear::initPhaseField: mesh bounding box",
                        expected: self.base.spatial_dimension,
                        got: bbox.upper.len(),
                    });
                }
            };
            diam += s * s;
        }
        let diam = diam.sqrt();
        self.rho_rec = gc / l0 * 9. * (diam / l0 - 2.) / (64. * self.tol_rec);
        if self.base.use_penalization
            && self.rho_rec <= 0.0
            && !self.base.compat.accept_nonpositive_recovery_penalty
        {
            return Err(Error::invalid(format!(
                "phase field {}: the `linear` (AT1) law with irreversibility_type = \
                 penalization needs a positive recovery penalty, got rho_rec = \
                 gc/l0*9*(diam/l0 - 2)/(64*recovery_tol) = {:e} (gc = {gc:e}, l0 = {l0:e}, \
                 mesh bounding-box diagonal diam = {diam:e}, recovery_tol = {:e}; it needs \
                 diam > 2*l0, gc > 0 and recovery_tol > 0): upstream accepts it and its \
                 Newton-Raphson can diverge (UD-385); \
                 PhaseFieldUpstreamCompat::accept_nonpositive_recovery_penalty restores \
                 upstream's behaviour",
                self.base.id, self.rho_rec, self.tol_rec
            )));
        }
        self.base.set_init();
        self.split = Some(EnergySplit::new(
            self.split_kind,
            self.base.e,
            self.base.nu,
            self.base.plane_stress,
        ));
        Ok(())
    }

    fn compute_driving_force(&mut self, ty: ElementType, ghost: GhostType) -> Result<()> {
        let split = self.split.ok_or_else(|| not_initialised(&self.base.id))?;
        let l0 = self.base.l0;
        let (gamma, rho_rec) = (self.gamma, self.rho_rec);
        // UD-388: corrected tangent [d ≤ d_prev] (compat: upstream's [d < d_prev])
        let strict = self.base.compat.at1_strict_irreversibility_indicator;
        let densities = |phi: Real, gc: Real| (2. * phi, -2. * phi + 3. * gc / (8. * l0));
        let penalty = |d: Real, dp: Real, density: Real| {
            let penalization_ir = gamma * min_zero(d - dp);
            let penalization_rec = rho_rec * min_zero(d);
            let mut k = density;
            k += gamma * indicator(if strict { d < dp } else { d <= dp });
            k += rho_rec * indicator(d < 0.);
            (penalization_ir + penalization_rec, k)
        };
        let law = DrivingLaw {
            split: &split,
            upstream_phi: self.base.compat.in_block_deviator_energy,
            use_history: self.base.use_history,
            use_penalization: self.base.use_penalization,
            densities: &densities,
            penalty: &penalty,
        };
        let dim = self.base.spatial_dimension;
        akantu_core::with_dim!(dim, D => with_driving_block(&mut self.base, ty, ghost, |b| {
            driving_force_kernel::<D>(&law, b)
        }))?
    }

    fn compute_dissipated_energy(&mut self, ty: ElementType) -> Result<()> {
        let l0 = self.base.l0;
        let density = move |d: Real, g: &[Real], gc: Real| linear_density(l0, d, g, gc);
        let dim = self.base.spatial_dimension;
        akantu_core::with_dim!(dim, D => dissipated_energy_kernel::<D>(&mut self.base, ty, &density))?
    }

    fn dissipated_energy_by_element(
        &self,
        ty: ElementType,
        index: usize,
        out: &mut [Real],
    ) -> Result<()> {
        let l0 = self.base.l0;
        let density = move |d: Real, g: &[Real], gc: Real| linear_density(l0, d, g, gc);
        let dim = self.base.spatial_dimension;
        let nq = out.len();
        akantu_core::with_dim!(dim, D => dissipated_energy_by_element_kernel::<D>(
            &self.base, ty, index, nq, out, &density
        ))?
    }
}

/// Upstream `PhaseFieldLinear::computeDissipatedEnergyOnQuad`.
fn linear_density(l0: Real, dam: Real, grad_d: &[Real], gc: Real) -> Real {
    let mut edis = 0.;
    for &g in grad_d {
        edis += 3. / 8. * gc * l0 * g * g;
    }
    edis += 3. * gc * dam / (8. * l0);
    edis
}

/// Register the built-in laws (upstream `instantiatePhaseField<...>`
/// registrations `quadratic` and `linear`). The allocator option selects the
/// energy split ([`EnergySplitKind::from_option`]); the law dimension is the
/// model's.
pub fn register_builtin_phasefields(factory: &mut PhaseFieldFactory) -> Result<()> {
    factory.register(
        "quadratic",
        Arc::new(|spec: &LawSpec, services: &dyn ModelServices| {
            let kind = EnergySplitKind::from_option(&spec.option)?;
            Ok(
                Box::new(PhaseFieldQuadratic::new(spec, kind, services.mesh())?)
                    as Box<dyn PhaseField>,
            )
        }),
    )?;
    factory.register(
        "linear",
        Arc::new(|spec: &LawSpec, services: &dyn ModelServices| {
            let kind = EnergySplitKind::from_option(&spec.option)?;
            Ok(
                Box::new(PhaseFieldLinear::new(spec, kind, services.mesh())?)
                    as Box<dyn PhaseField>,
            )
        }),
    )
}

/// The element `element` (law-local index) is valid for `law`.
pub(crate) fn check_local(law: &dyn PhaseField, element: Element) -> Result<()> {
    let f = law.base().filter(element.ty(), element.ghost_type());
    if element.idx() >= f.len() {
        return Err(Error::InvalidIndex {
            what: "law-local element",
            value: i64::from(element.index()),
        });
    }
    Ok(())
}
