// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/model/solid_mechanics/material.hh,
// material.cc, material_inline_impl.hh; src/model/common/constitutive_laws/
// constitutive_law_tmpl.hh (name parameter, initConstitutiveLaw).

//! The solid material contract (upstream `Material`).
//!
//! Every method receives a phase-scoped [`LawContext`] instead of the
//! upstream back-reference `SolidMechanicsModel & model`:
//! the model lends its read-only services (mesh, FE engine, nodal fields,
//! time step) for one phase while the material itself is mutably borrowed.
//!
//! # Parameters
//!
//! [`MaterialBase`] carries upstream's common parameters (`name`, `rho`,
//! `finite_deformation`, `inelastic_deformation`, `eigen_grad_u`) in a
//! static table ([`BASE_PARAMS`]); each built-in law adds its own table and
//! implements [`MaterialParams`], which gives the `ParameterRegistry`
//! behaviour of upstream: `set` (writable) is followed by
//! `updateInternalParameters`, `set_from_input` (parsable) is not — the
//! parser calls it once at the end of the section.
//!
//! # Internal fields
//!
//! The common internals (`stress`, `eigen_grad_u`, `grad_u`,
//! `potential_energy`, and `piola_kirchhoff_2`/`green_strain` with
//! `finite_deformation`) live in [`MaterialBase`]; laws add theirs. Upstream
//! resizes all internals in register-id order (`std::map`), which
//! [`ConstitutiveLaw::resize_internals`] reproduces (draw order of random
//! fields, D-006).
//!
//! Every internal is restricted to the element types of the law's spatial
//! dimension and kind (`_ek_regular`; upstream `InternalField` constructor,
//! row CL-002): [`MaterialBase::blocks`] only reports the filter blocks of
//! those types, the base internals carry the restriction, and the laws give
//! it to their own internals through
//! [`ConstitutiveLaw::internal_restriction`] — an element of another
//! dimension or kind in the filter never gets an internal block, a value or
//! a random draw.

use akantu_core::array::ArrayViewMut;
use akantu_core::enums::MatrixType;
use akantu_core::error::if_present;
use akantu_core::tensor::{Matrix, MatrixX};
use akantu_core::{
    Array, Element, ElementKind, ElementType, ElementTypeMapArray, Error, GhostType, Idx, Real,
    Result, TypeFilter,
};
use akantu_mesh::Mesh;
use akantu_model::InternalField;
use akantu_model::constitutive_law::ConstitutiveLaw;
use akantu_model::constitutive_law::{ConstitutiveLawFactory, LawContext, LawReducer, LawSpec};
use akantu_model::internal_field::TypeRestriction;
use akantu_parser::ByteString;
use akantu_parser::param::{
    ParamDescriptor, ParamInfo, ParamKind, ParamValue, ParameterAccess, fit_matrix, table_get,
    table_info, table_names, table_set, table_set_from_input,
};

use crate::material_tensor;
use crate::materials::anisotropic::CelerityMode;
use crate::materials::anisotropic_damage::AnisotropicDamageModel;
pub use crate::materials::damage::FiniteStrainWork;
use crate::materials::maxwell::{MaxwellThermal, MaxwellUpdate};
use crate::materials::phasefield::PhaseFieldTangent;
use crate::materials::sls_deviatoric::{SlsDegenerateBranch, SlsDissipation, SlsEnergy};
use crate::materials::thermal::{ThermalEnergy, ThermalStrain};

/// Switches between the upstream behaviour and the port's corrections of
/// upstream defects (`docs/UPSTREAM_DEFECTS.md`, decision D-003). Each law
/// reads the switches that concern it; [`LawOptions::default`] is the
/// corrected behaviour, [`LawOptions::upstream`] reproduces the reference
/// everywhere (differential tests against the unpatched oracle).
///
/// # When options take effect
///
/// Switches that shape the stiffness (`material_axes_as_directions`,
/// `orthotropic_parent_update`) are applied by `updateInternalParameters`:
/// set them **before** `init_material` (the material probe replays do), or
/// use [`Material::set_options`], which re-runs the update. The orthotropic
/// parent update also copies the `eigen_grad_u` parameter into the
/// internal; switching it off after an update cannot undo that copy, so the
/// upstream behaviour is only reproduced when the switch is off before
/// `init_material`. The other switches are read when the quantity is
/// computed (`thermal_strain`, `thermal_energy`: next stress/energy
/// evaluation; `celerity`: next query; the inelastic-law switches
/// `damage_thermal_refresh`, `plastic_thermal_increment`,
/// `finite_strain_plasticity`: next stress evaluation; `neohookean`: next
/// stress/tangent/energy evaluation, and by the parameter update, which
/// skips the `eigen_grad_u` copy in upstream mode).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LawOptions {
    /// UD-038 (category B): 3-D thermal eigenstrain of `elastic`; default
    /// corrected (`αΔT`).
    pub thermal_strain: ThermalStrain,
    /// UD-109 (category B): potential energy density of
    /// `elastic` with a temperature change; default the stored elastic
    /// energy, a potential of the computed stress at fixed temperature
    /// ([`ThermalEnergy`]).
    pub thermal_energy: ThermalEnergy,
    /// UD-033 (category B): celerity of `elastic_anisotropic` /
    /// `elastic_orthotropic`; default the Kelvin/Mandel bound.
    pub celerity: CelerityMode,
    /// UD-105 (category B): upstream's
    /// `elastic_orthotropic` skips the parent `updateInternalParameters`
    /// (`eigen_grad_u` never copied into the internal, stiffness not flagged
    /// as changed). `true` (default) runs the parent updates; `false`
    /// reproduces upstream.
    pub orthotropic_parent_update: bool,
    /// UD-104 (category B): upstream's `rotateCprime`
    /// rotates with `Rᵀ` (`rotator(I, J) = R(k, i)·R(l, j)`, `R` =
    /// normalised axes as columns), so the material axis 1 in global
    /// coordinates is the first **row** of `R`, not `n1` (for
    /// `n1 = (0.8, 0.6)`, `n2 = (−0.6, 0.8)` the stiff direction is
    /// `(0.8, −0.6)`; rotations by 0°/90°/180° and symmetric `R` are not
    /// affected). `true` (default) makes `n1..` the material directions
    /// (`C_ijkl = R_ia R_jb R_kc R_ld C'_abcd`); `false` reproduces upstream.
    pub material_axes_as_directions: bool,
    // --- inelastic laws -----------------------------------------------
    /// UD-038 (category B): upstream's `marigo`, `mazars` and
    /// `plastic_mazars` call only the point kernels of their parents and
    /// never refresh the thermal state (`sigma_th` stays at its last value,
    /// 0 from the start). `true` (default) runs the thermal pre-step of
    /// `MaterialThermal::computeStress` first, as every other elastic-family
    /// law; `false` reproduces upstream.
    pub damage_thermal_refresh: bool,
    /// UD-038 (category B): upstream's
    /// `plastic_linear_isotropic_hardening` builds its trial stress with the
    /// thermal increment `σ_th,prev − σ_th` (wrong sign) and both plastic
    /// laws (with `plastic_drucker_prager`) rebuild the final stress
    /// `σ_prev + C:(Δ∇u − Δε_p)` without any thermal increment. `true`
    /// (default) uses `Δσ_th = σ_th − σ_th,prev` in the trial **and** the
    /// final stress; `false` reproduces upstream.
    pub plastic_thermal_increment: bool,
    /// UD-030 (category B): `plastic_linear_isotropic_hardening` and
    /// `plastic_mazars` with `finite_deformation = true`, refused by default.
    pub finite_strain_plasticity: FiniteStrainPlasticity,
    /// UD-026 / UD-057 (category B): upstream's `neohookean` aborts in
    /// `computeStress`/`computeTangentModuli`/`computePotentialEnergy`
    /// (`NotImplemented`); the port implements the documented law
    /// (`crate::materials::neohookean`).
    pub neohookean: NeohookeanMode,
    // --- inelastic laws: return mappings and energies ----
    /// UD-137 (category B): J2 return mapping of
    /// `plastic_linear_isotropic_hardening` / `plastic_mazars` in 1-D and
    /// 2-D. Upstream combines the 3-D plastic multiplier `Δp = f/(3μ + h)`
    /// and a flow truncated to the in-plane block with the reduced elastic
    /// operator, so the returned stress is off the yield surface (1-D:
    /// denominator must be `E + h`; plane strain: `σ33`, `ε_p33` ignored;
    /// plane stress: no `σ33 = 0` constraint). [`J2ReducedReturn`].
    pub j2_reduced_return: J2ReducedReturn,
    /// UD-138 (category B): `true` (default) evaluates
    /// every trial from the state committed at the last `savePreviousState`
    /// (J2 yield predicate with the committed hardening; Mazars and Marigo
    /// damage maxima and Marigo's `damage_in_y` with the committed damage),
    /// so repeated or rejected Newton/line-search trials of one step do not
    /// influence later trials; `false` reproduces upstream, where the
    /// current (trial) hardening/damage of an earlier evaluation enters.
    pub committed_trial_state: bool,
    /// UD-139 (category B): `true` (default) runs the J2
    /// return of `plastic_mazars` on its own undamaged (effective) stress
    /// history (internal `effective_stress`), the stored stress being
    /// `(1 − d)·σ_eff`; `false` reproduces upstream, whose next trial starts
    /// from the **damaged** stress (stress decays at fixed strain).
    pub plastic_mazars_effective_stress: bool,
    /// UD-140 (category B): `true` (default) evaluates the
    /// Mazars equivalent strain and principal weights on the mechanical
    /// strain (total − plastic − thermal `αΔT` on the directions of free
    /// expansion); `false` reproduces upstream (total minus plastic strain:
    /// free heating damages).
    pub mazars_mechanical_strain: bool,
    /// UD-141 (category B): `true` (default) embeds the
    /// 2-D state of Mazars in the 3-D strain it represents (plane stress:
    /// `ε33 = −ν/(1 − ν)·(ε11 + ε22)` of the elastic strain, with the 3-D
    /// `λ` in the principal weights; plane strain with the J2 parent: the
    /// out-of-plane plastic strain `ε_p33 = −tr ε_p`); `false` reproduces
    /// upstream (`ε33 = 0`, plane-stress `λ`).
    pub mazars_out_of_plane_strain: bool,
    /// UD-142 (category B): `marigo` / `mazars` with
    /// `finite_deformation` ([`FiniteStrainDamage`]).
    pub finite_strain_damage: FiniteStrainDamage,
    /// UD-144…146 (category B): the stress return of
    /// `plastic_drucker_prager` ([`DruckerPragerReturn`]).
    pub drucker_prager_return: DruckerPragerReturn,
    // --- viscoelastic, anisotropic damage, phase field --------------------
    /// UD-029 (category B, SLS part): dissipated-energy increment of
    /// `sls_deviatoric`; default the tensor contraction `(ε_d − αq) : q̇ Δt`
    /// (1-D/2-D: with the reduced branch compliance `B⁻¹`),
    /// upstream the sum of the entries of the matrix product.
    pub sls_dissipation: SlsDissipation,
    /// UD-063 (category B): `sls_deviatoric` computes its trial state from
    /// the history committed at the last `savePreviousState` (repeated
    /// evaluations in one step are idempotent, a failed step rolls back);
    /// `false` reproduces upstream, which advances the history at every
    /// `computeStress`.
    pub sls_committed_state: bool,
    /// UD-161 (category B): stored energy of
    /// `sls_deviatoric` ([`SlsEnergy`]): default the two-spring free energy
    /// (branch spring included, `B⁻¹` form in 1-D/2-D; isothermal
    /// small-strain domain enforced), upstream the
    /// parent's `½σ:∇u`.
    pub sls_energy: SlsEnergy,
    /// UD-762 (category B): degenerate viscous-branch
    /// parameters of `sls_deviatoric` ([`SlsDegenerateBranch`]): default the
    /// limits for `Ev = 0` / `Eta = 0` (no NaN dissipation) and negative or
    /// non-finite values refused; upstream the formulas as written.
    pub sls_degenerate_branch: SlsDegenerateBranch,
    /// UD-029 (category B, Maxwell part) and the related branch-strain
    /// defects: `viscoelastic_maxwell` branch stresses
    /// follow the exponential recurrence after a converged step; upstream
    /// never updates them.
    pub maxwell_update: MaxwellUpdate,
    /// UD-157 (category B): `true` (default) flags the
    /// stiffness of `viscoelastic_maxwell` changed after every parameter
    /// update (`Einf`, `Ev`, `Eta`, `E`, `nu` change its tangent at a fixed
    /// time step), cleared by the next successful tangent; `false`
    /// reproduces upstream, whose `hasStiffnessMatrixChanged` only compares
    /// `previous_dt` with the time step.
    pub maxwell_stiffness_invalidation: bool,
    /// UD-327 (category B): thermal loading of
    /// `viscoelastic_maxwell` ([`MaxwellThermal`]): default `alpha ≠ 0` is
    /// [`Error::Unsupported`] (no thermo-viscoelastic formulation has been
    /// specified and checked);
    /// upstream adds the instantaneous-modulus thermal stress to a law whose
    /// branches and stored energy ignore the thermal strain.
    pub maxwell_thermal: MaxwellThermal,
    /// UD-031 / UD-058 (categories A/B, specification not approved):
    /// the `anisotropic_damage` model. Default
    /// [`AnisotropicDamageModel::Unsupported`] (explicit error, UD-030
    /// precedent); `Upstream` the reference code with its uninitialised
    /// entries defined as zero; `DesmoratExperimental` the unapproved
    /// proposal of `materials/anisotropic_damage.rs` (opt-in only).
    pub anisotropic_damage: AnisotropicDamageModel,
    /// Category B defects: `phasefield` tangent of the
    /// volumetric–deviatoric split — 1-D compression term and the sign test
    /// on the thermal-corrected trace; default consistent with the stress.
    pub phasefield_tangent: PhaseFieldTangent,
    /// UD-158 (FD-PREV) + UD-159, category B: the
    /// finite-strain energy update of every `MaterialDamage` law
    /// (`MaterialDamage::updateEnergies`). Upstream's `getArguments` binds
    /// `previous_sigma` to the history of the **Cauchy** `stress` (never
    /// written by these laws) while `sigma` is the second Piola–Kirchhoff
    /// stress, and pairs them with `Δ∇u`, which is not conjugate to `S`.
    /// [`FiniteStrainWork::Conjugate`] (default):
    /// `½(S_{n+1} + S_n):ΔE_Green`; [`FiniteStrainWork::UpstreamMixed`]
    /// reproduces upstream's whole update. Small strain is unaffected.
    pub finite_strain_work: FiniteStrainWork,
    /// UD-160 (category B): `true` (default) adds the
    /// reversible thermal work `∫ ∂ψ/∂θ dθ` of the stored energy to the
    /// accumulated work of every `MaterialDamage` law, so that the
    /// dissipated energy `∫σ:dε + ∫∂ψ/∂θ dθ − ψ` is the damage dissipation
    /// (heating an undamaged body dissipates nothing); active only with
    /// [`ThermalEnergy::StoredElastic`] (the potential being the stored
    /// energy). `false` reproduces upstream (`∫σ:dε − ψ`).
    pub damage_thermal_work: bool,
}

/// J2 return in 1-D and 2-D (UD-137).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum J2ReducedReturn {
    /// The consistent return of the represented 3-D state (default): 1-D
    /// uniaxial stress `Δp = f/(E + h)`; plane strain: the 3-D radial return
    /// with the out-of-plane stress `σ33 = λ tr ε + 2μ ε_p,in-plane trace +
    /// σ_th` of the committed state (isochoric flow, `ε_p33 = −tr ε_p`);
    /// plane stress: the Simo–Taylor plane-stress projection (scalar Newton
    /// on the consistency condition with `σ33 = 0` exactly). The returned
    /// stress lies on the yield surface.
    #[default]
    Consistent,
    /// Upstream's truncated return (3-D multiplier and flow, in-plane block
    /// only), reproduced bit for bit for differential tests (with
    /// [`LawOptions::upstream`] when trials are repeated within a step,
    /// UD-138).
    UpstreamTruncated,
}

/// `marigo` / `mazars` with `finite_deformation` (UD-142).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FiniteStrainDamage {
    /// Default: no objective finite-strain damage formulation has been
    /// specified and checked, so `computeStress` returns
    /// [`Error::Unsupported`] (upstream is not frame
    /// indifferent — a pure rotation creates stress).
    #[default]
    Unsupported,
    /// Upstream: the small-strain elastic kernel on the raw `∇u`, stored as
    /// the second Piola–Kirchhoff stress (differential tests only).
    UpstreamLegacy,
}

/// Stress return of `plastic_drucker_prager` (UD-144…146).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DruckerPragerReturn {
    /// Default: the closed-form closest-point projection of the yield
    /// function `√(3J2) + α tr σ − k` (`D`-dimensional deviator, as
    /// upstream) for the isotropic elastic operator — regular cone return
    /// or apex `σ = k/(Dα)·I` by the exact criterion, `radial_return` at
    /// fixed pressure; degenerate yield surfaces (no admissible stress) and
    /// non-finite results are [`Error::Numerical`].
    #[default]
    ClosedForm,
    /// Upstream's iterations (apex inverse-deviator loop and trace
    /// correction `1/α`, Newton with the `δδᵀ/3` Hessian, exit after 100
    /// iterations), reproduced bit for bit where they produce a finite
    /// result; non-finite iterates and apex loops beyond 1000 iterations are
    /// [`Error::Numerical`] (upstream: NaN or a hang).
    UpstreamIteration,
}

/// `plastic_linear_isotropic_hardening` with `finite_deformation` (UD-030).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FiniteStrainPlasticity {
    /// Default: no finite-strain J2 formulation has been specified and
    /// checked (upstream's semantics are not a correct default), so
    /// `computeStress` returns [`Error::Unsupported`].
    #[default]
    Unsupported,
    /// Upstream's defined result, reproduced bit for bit for differential
    /// tests together with the other switches of [`LawOptions::upstream`]
    /// (in 1-D and 2-D the second pass otherwise runs the consistent
    /// return of UD-137, and the yield test reads the committed hardening
    /// of UD-138): the finite-strain branch (Green strains, Cauchy-based yield
    /// test, Newton loop) **then** the small-strain kernel on the raw
    /// `∇u`, which overwrites stress and plastic strain but sees the
    /// hardening written by the first branch. Not a correct finite-strain
    /// plasticity.
    UpstreamLegacy,
}

/// Behaviour of `neohookean` (UD-026, UD-057).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum NeohookeanMode {
    /// The documented compressible Neo-Hookean law (default), with the
    /// corrections of the energy and the plane-stress condensation (UD-026).
    #[default]
    Documented,
    /// Upstream: `computeStress`, `computeTangentModuli` and
    /// `computePotentialEnergy` fail with `NotImplemented` (the oracle's
    /// recorded behaviour).
    UpstreamNotImplemented,
}

impl Default for LawOptions {
    /// The corrected behaviour for every switch.
    fn default() -> Self {
        Self {
            thermal_strain: ThermalStrain::Corrected,
            thermal_energy: ThermalEnergy::StoredElastic,
            celerity: CelerityMode::KelvinBound,
            orthotropic_parent_update: true,
            material_axes_as_directions: true,
            damage_thermal_refresh: true,
            plastic_thermal_increment: true,
            finite_strain_plasticity: FiniteStrainPlasticity::Unsupported,
            neohookean: NeohookeanMode::Documented,
            j2_reduced_return: J2ReducedReturn::Consistent,
            committed_trial_state: true,
            plastic_mazars_effective_stress: true,
            mazars_mechanical_strain: true,
            mazars_out_of_plane_strain: true,
            finite_strain_damage: FiniteStrainDamage::Unsupported,
            drucker_prager_return: DruckerPragerReturn::ClosedForm,
            sls_dissipation: SlsDissipation::Contraction,
            sls_committed_state: true,
            sls_energy: SlsEnergy::TwoSpring,
            sls_degenerate_branch: SlsDegenerateBranch::Limit,
            maxwell_update: MaxwellUpdate::Recurrence,
            maxwell_stiffness_invalidation: true,
            maxwell_thermal: MaxwellThermal::Unsupported,
            anisotropic_damage: AnisotropicDamageModel::Unsupported,
            phasefield_tangent: PhaseFieldTangent::Consistent,
            finite_strain_work: FiniteStrainWork::Conjugate,
            damage_thermal_work: true,
        }
    }
}

impl LawOptions {
    /// Upstream behaviour for every switch.
    #[must_use]
    pub const fn upstream() -> Self {
        Self {
            thermal_strain: ThermalStrain::Upstream,
            thermal_energy: ThermalEnergy::Upstream,
            celerity: CelerityMode::UpstreamFirstEigenvalue,
            orthotropic_parent_update: false,
            material_axes_as_directions: false,
            damage_thermal_refresh: false,
            plastic_thermal_increment: false,
            finite_strain_plasticity: FiniteStrainPlasticity::UpstreamLegacy,
            neohookean: NeohookeanMode::UpstreamNotImplemented,
            j2_reduced_return: J2ReducedReturn::UpstreamTruncated,
            committed_trial_state: false,
            plastic_mazars_effective_stress: false,
            mazars_mechanical_strain: false,
            mazars_out_of_plane_strain: false,
            finite_strain_damage: FiniteStrainDamage::UpstreamLegacy,
            drucker_prager_return: DruckerPragerReturn::UpstreamIteration,
            sls_dissipation: SlsDissipation::UpstreamMatrixProduct,
            sls_committed_state: false,
            sls_energy: SlsEnergy::UpstreamElastic,
            sls_degenerate_branch: SlsDegenerateBranch::Upstream,
            maxwell_update: MaxwellUpdate::Upstream,
            maxwell_stiffness_invalidation: false,
            maxwell_thermal: MaxwellThermal::Upstream,
            anisotropic_damage: AnisotropicDamageModel::Upstream,
            phasefield_tangent: PhaseFieldTangent::Upstream,
            finite_strain_work: FiniteStrainWork::UpstreamMixed,
            damage_thermal_work: false,
        }
    }
}

/// State common to every solid material (upstream `Material` members
/// `stress`, `eigengradu`, `gradu`, `potential_energy`,
/// `piola_kirchhoff_2`, `green_strain`, `rho`, `finite_deformation`,
/// `inelastic_deformation`, `eigen_grad_u`, and the constitutive-law
/// `name`, `id` and `element_filter`).
#[derive(Clone, Debug)]
pub struct MaterialBase {
    /// Law id (`<model id>:<index>:<type>`, upstream `getID()`).
    pub id: String,
    /// User name (`name = steel`), decoded for display and lookups (the
    /// exact bytes are the `name` parameter, see [`MaterialBase::name_bytes`]).
    pub name: String,
    name_bytes: ByteString,
    /// Registered type (`elastic`, `local_elastic`, …).
    pub law_type: String,
    /// Spatial dimension.
    pub spatial_dimension: usize,
    /// Elements handled by the material (a plain array, not exportable
    /// storage: D-056 item 2).
    pub element_filter: ElementTypeMapArray<Idx>,
    /// Cauchy stress per quadrature point (`dim × dim`, column-major).
    pub stress: InternalField<Real>,
    /// Eigen displacement gradient per quadrature point (upstream internal
    /// `eigen_grad_u`), subtracted from `∇u` before `computeStress`.
    pub eigen_grad_u: InternalField<Real>,
    /// Displacement gradient per quadrature point (`dim × dim`).
    pub grad_u: InternalField<Real>,
    /// Potential energy density per quadrature point.
    pub potential_energy: InternalField<Real>,
    /// Second Piola–Kirchhoff stress (only with `finite_deformation`,
    /// registered by `init`).
    pub piola_kirchhoff_2: Option<InternalField<Real>>,
    /// Green strain (only with `finite_deformation`).
    pub green_strain: Option<InternalField<Real>>,
    /// Density.
    pub rho: Real,
    /// `finite_deformation` parameter.
    pub finite_deformation: bool,
    /// `inelastic_deformation` parameter (internal).
    pub inelastic_deformation: bool,
    /// The `eigen_grad_u` parameter (`dim × dim`), copied into the
    /// not-ghost blocks of the internal by `updateInternalParameters`.
    pub eigen_grad_u_param: MatrixX,
    /// Upstream `is_non_local`.
    pub is_non_local: bool,
    /// Elemental interpolation of the stresses (upstream members
    /// `interpolation_inverse_coordinates` / `interpolation_points_matrices`,
    /// SMM-024; `crate::stress_interpolation`).
    pub stress_interpolation: Option<akantu_fe::engine::ElementalFieldInterpolation>,
    is_init: bool,
    /// Kind of the element types the internals handle (upstream
    /// `InternalField::element_kind`, `_ek_regular` unless a law changes it).
    element_kind: ElementKind,
}

/// A base internal restricted to `(dim, kind)` (upstream `InternalField`
/// constructed with the law's dimension).
fn restricted_internal(
    name: &str,
    nb_component: usize,
    dim: usize,
    kind: ElementKind,
) -> Result<InternalField<Real>> {
    let mut f = InternalField::new(name, nb_component, 0.0)?;
    f.set_restriction(dim, kind);
    Ok(f)
}

impl MaterialBase {
    /// New base for a law created from `spec` (upstream `Material`
    /// constructor; the `name` parameter is empty until parsed). The element
    /// filter is empty; use [`MaterialBase::with_mesh`] to allocate the
    /// upstream (empty) blocks for every element type of the mesh.
    pub fn new(spec: &LawSpec, law_type: &str) -> Result<Self> {
        let d = akantu_core::dim::check_dim(spec.spatial_dimension)?;
        Ok(Self {
            id: spec.id.clone(),
            name: String::new(),
            name_bytes: ByteString::default(),
            law_type: law_type.into(),
            spatial_dimension: d,
            element_filter: ElementTypeMapArray::new(format!("{}:element_filter", spec.id)),
            stress: restricted_internal("stress", d * d, d, ElementKind::Regular)?,
            eigen_grad_u: restricted_internal("eigen_grad_u", d * d, d, ElementKind::Regular)?,
            grad_u: restricted_internal("grad_u", d * d, d, ElementKind::Regular)?,
            potential_energy: restricted_internal("potential_energy", 1, d, ElementKind::Regular)?,
            piola_kirchhoff_2: None,
            green_strain: None,
            rho: 0.0,
            finite_deformation: false,
            inelastic_deformation: false,
            eigen_grad_u_param: MatrixX::zeros(d, d),
            is_non_local: false,
            stress_interpolation: None,
            is_init: false,
            element_kind: ElementKind::Regular,
        })
    }

    /// Element types the internals handle: the law's spatial dimension and
    /// kind (upstream `InternalField::spatial_dimension` / `element_kind`);
    /// what solid laws return from
    /// [`ConstitutiveLaw::internal_restriction`].
    #[must_use]
    pub fn internal_restriction(&self) -> TypeRestriction {
        TypeRestriction {
            spatial_dimension: Some(self.spatial_dimension),
            element_kind: self.element_kind,
        }
    }

    /// Upstream `setElementKind` on every internal of the law (e.g. a
    /// cohesive law's `_ek_cohesive`): the base internals and
    /// [`MaterialBase::blocks`] follow `kind`; the law's own internals
    /// adopt it at their next resize (their restriction must not have been
    /// set explicitly). Call it before the first resize.
    pub fn set_element_kind(&mut self, kind: ElementKind) {
        self.element_kind = kind;
        let d = self.spatial_dimension;
        for f in self.internals_mut() {
            f.set_restriction(d, kind);
        }
    }

    /// [`MaterialBase::new`] plus upstream's element-filter initialisation:
    /// an empty block for every `(type, ghost)` of `mesh` with the law's
    /// spatial dimension and kind `_ek_regular`.
    pub fn with_mesh(spec: &LawSpec, law_type: &str, mesh: &Mesh) -> Result<Self> {
        let mut b = Self::new(spec, law_type)?;
        b.initialize_element_filter(mesh, ElementKind::Regular)?;
        Ok(b)
    }

    /// Allocate empty filter blocks for the mesh's element types of the
    /// law's dimension and `kind` (upstream `element_filter.initialize(mesh,
    /// _spatial_dimension, _element_kind)`).
    pub fn initialize_element_filter(&mut self, mesh: &Mesh, kind: ElementKind) -> Result<()> {
        for ghost in GhostType::ALL {
            let filter = TypeFilter {
                spatial_dimension: Some(self.spatial_dimension),
                ghost_type: ghost,
                kind: Some(kind),
            };
            let types: Vec<ElementType> = mesh.element_types(filter).collect();
            for ty in types {
                if !self.element_filter.exists(ty, ghost) {
                    self.element_filter.alloc(0, 1, ty, ghost, 0)?;
                }
            }
        }
        Ok(())
    }

    /// The exact bytes of the `name` parameter.
    #[must_use]
    pub fn name_bytes(&self) -> &ByteString {
        &self.name_bytes
    }

    /// Set the `name` parameter.
    pub fn set_name(&mut self, name: impl Into<ByteString>) {
        self.name_bytes = name.into();
        self.name = self.name_bytes.to_string_lossy().into_owned();
    }

    /// `true` once `init` ran (upstream `isInit()`).
    #[must_use]
    pub fn is_init(&self) -> bool {
        self.is_init
    }

    /// Number of quadrature points of the material's elements of
    /// `(ty, ghost)`.
    #[must_use]
    pub fn nb_quadrature_points(
        &self,
        ctx: &LawContext<'_>,
        ty: ElementType,
        ghost: GhostType,
    ) -> usize {
        self.element_filter.get(ty, ghost).map_or(0, |f| f.len())
            * ctx.services.nb_integration_points(ty, ghost)
    }

    /// `(type, ghost, nb_quadrature_points)` of every filter block of the
    /// types the internals handle (ghost-major, enum order; upstream
    /// `InternalField::filterTypes`: the law's dimension and kind, empty
    /// filter blocks included). Elements of other types in the filter get
    /// no internal block.
    #[must_use]
    pub fn blocks(&self, ctx: &LawContext<'_>) -> Vec<(ElementType, GhostType, usize)> {
        let r = self.internal_restriction();
        self.element_filter
            .as_map()
            .iter_all()
            .filter(|&(ty, _, _)| r.handles(ty))
            .map(|(ty, g, _)| (ty, g, self.nb_quadrature_points(ctx, ty, g)))
            .collect()
    }

    /// Element types of the filter of the law's spatial dimension for one
    /// ghost type (upstream `getElementFilter().elementTypes(dim, ghost)`).
    #[must_use]
    pub fn filter_types(&self, ghost: GhostType) -> Vec<ElementType> {
        self.element_filter
            .element_types(TypeFilter {
                spatial_dimension: Some(self.spatial_dimension),
                ghost_type: ghost,
                kind: None,
            })
            .collect()
    }

    /// Size the base internals for the element filter (part of upstream
    /// `Material::initMaterial` / `resizeInternals`; laws with own internals
    /// use [`ConstitutiveLaw::resize_internals`], which covers these too).
    pub fn resize_internals(&mut self, ctx: &LawContext<'_>) -> Result<()> {
        let blocks = self.blocks(ctx);
        for f in self.internals_mut() {
            f.resize_blocks(&blocks)?;
        }
        Ok(())
    }

    /// Upstream `Material::initMaterial` before `initConstitutiveLaw`:
    /// register `piola_kirchhoff_2` (with history) and `green_strain` when
    /// `finite_deformation`, and enable the history of `stress` and
    /// `grad_u`.
    pub fn init_history(&mut self) -> Result<()> {
        let (d, kind) = (self.spatial_dimension, self.element_kind);
        if self.finite_deformation {
            if self.piola_kirchhoff_2.is_none() {
                let mut pk2 = restricted_internal("piola_kirchhoff_2", d * d, d, kind)?;
                pk2.initialize_history();
                self.piola_kirchhoff_2 = Some(pk2);
            }
            if self.green_strain.is_none() {
                self.green_strain = Some(restricted_internal("green_strain", d * d, d, kind)?);
            }
        }
        self.stress.initialize_history();
        self.grad_u.initialize_history();
        Ok(())
    }

    /// Mark the law initialised (end of upstream `initConstitutiveLaw`).
    pub fn set_init(&mut self) {
        self.is_init = true;
    }

    /// The base internals in upstream registration order.
    pub fn internals_mut(&mut self) -> Vec<&mut InternalField<Real>> {
        let mut v = vec![
            &mut self.stress,
            &mut self.eigen_grad_u,
            &mut self.grad_u,
            &mut self.potential_energy,
        ];
        if let Some(p) = &mut self.piola_kirchhoff_2 {
            v.push(p);
        }
        if let Some(g) = &mut self.green_strain {
            v.push(g);
        }
        v
    }

    /// The base internals (name, field) in upstream registration order.
    pub fn internals(&self) -> Vec<(&'static str, &InternalField<Real>)> {
        let mut v = vec![
            ("stress", &self.stress),
            ("eigen_grad_u", &self.eigen_grad_u),
            ("grad_u", &self.grad_u),
            ("potential_energy", &self.potential_energy),
        ];
        if let Some(p) = &self.piola_kirchhoff_2 {
            v.push(("piola_kirchhoff_2", p));
        }
        if let Some(g) = &self.green_strain {
            v.push(("green_strain", g));
        }
        v
    }

    /// The internal that receives the stress computed by the law: `stress`,
    /// or `piola_kirchhoff_2` with `finite_deformation` (upstream
    /// `getArguments` `"sigma"`).
    pub fn sigma_field_mut(&mut self) -> Result<&mut InternalField<Real>> {
        if self.finite_deformation {
            self.piola_kirchhoff_2.as_mut().ok_or_else(|| {
                Error::invalid("finite_deformation: piola_kirchhoff_2 is not registered (init the material first)")
            })
        } else {
            Ok(&mut self.stress)
        }
    }

    /// Read-only [`MaterialBase::sigma_field_mut`].
    pub fn sigma_field(&self) -> Result<&InternalField<Real>> {
        if self.finite_deformation {
            self.piola_kirchhoff_2.as_ref().ok_or_else(|| {
                Error::invalid("finite_deformation: piola_kirchhoff_2 is not registered (init the material first)")
            })
        } else {
            Ok(&self.stress)
        }
    }

    /// Upstream `Material::updateInternalParameters`: copy the
    /// `eigen_grad_u` parameter into every quadrature point of the
    /// **not-ghost** blocks of the internal (`elementTypes(_element_kind =
    /// _ek_regular)` with the default ghost type; ghost blocks keep their
    /// values, as upstream).
    pub fn update_internal_parameters(&mut self) -> Result<()> {
        let d = self.spatial_dimension;
        let m = &self.eigen_grad_u_param;
        if m.rows() != d || m.cols() != d {
            return Err(Error::DimensionMismatch {
                context: "eigen_grad_u",
                expected: d * d,
                got: m.rows() * m.cols(),
            });
        }
        // Disjoint field borrows instead of `filter_types` + a copy of the
        // parameter: allocation-free (the broken branch of
        // `anisotropic_damage` calls this per quadrature point, as upstream).
        let src = m.as_slice();
        let filter = TypeFilter {
            spatial_dimension: Some(d),
            ghost_type: GhostType::NotGhost,
            kind: None,
        };
        for ty in self.element_filter.element_types(filter) {
            // D-071: the guard is scoped to this block explicitly.
            if let Some(mut a) = if_present(self.eigen_grad_u.get_mut(ty, GhostType::NotGhost))? {
                for t in a.as_mut_slice().chunks_exact_mut(d * d) {
                    t.copy_from_slice(src);
                }
            }
        }
        Ok(())
    }

    /// Upstream `Material::applyEigenGradU(prescribed, ghost)`: set the
    /// internal `eigen_grad_u` of every non-empty block of the filter of
    /// `ghost` (all types) to `prescribed` (`dim × dim`, column-major).
    pub fn apply_eigen_grad_u(&mut self, prescribed: &MatrixX, ghost: GhostType) -> Result<()> {
        let d = self.spatial_dimension;
        if prescribed.rows() != d || prescribed.cols() != d {
            return Err(Error::DimensionMismatch {
                context: "applyEigenGradU",
                expected: d * d,
                got: prescribed.rows() * prescribed.cols(),
            });
        }
        let types: Vec<ElementType> = self
            .element_filter
            .as_map()
            .iter(GhostType::NotGhost)
            .map(|(t, _)| t)
            .collect();
        for ty in types {
            if self
                .element_filter
                .get(ty, ghost)
                .map_or(true, Array::is_empty)
            {
                continue;
            }
            if let Some(mut a) = if_present(self.eigen_grad_u.get_mut(ty, ghost))? {
                for t in a.as_mut_slice().chunks_exact_mut(d * d) {
                    t.copy_from_slice(prescribed.as_slice());
                }
            }
        }
        Ok(())
    }

    /// Upstream `Material::getPotentialEnergy()` after
    /// `computePotentialEnergyByElements`: `0 + Σ_types ∫ potential_energy`
    /// over the not-ghost filter blocks of the law's dimension
    /// (`fem.integrate(..., filter)`, see
    /// [`akantu_model::constitutive_law::ModelServices::integrate_scalar`]).
    /// Empty blocks contribute nothing (upstream adds `+0`, which never
    /// changes the bits of a sum starting at `+0`).
    pub fn integrate_potential_energy(&self, ctx: &LawContext<'_>) -> Result<Real> {
        let mut epot = 0.0;
        for ty in self.filter_types(GhostType::NotGhost) {
            let f = self.element_filter.get(ty, GhostType::NotGhost)?;
            if f.is_empty() {
                continue;
            }
            let e = self.potential_energy.get(ty, GhostType::NotGhost)?;
            if e.is_empty() {
                continue;
            }
            epot += ctx.services.integrate_scalar(
                e.as_slice(),
                ty,
                GhostType::NotGhost,
                f.as_slice(),
            )?;
        }
        Ok(epot)
    }

    /// Upstream `Material::computeAllCauchyStresses` (finite deformation,
    /// `onDump`): `σ = StoCauchy(F(∇u), S)` per quadrature point of the
    /// filter blocks of the law's dimension.
    pub fn compute_all_cauchy_stresses(&mut self, ghost: GhostType) -> Result<()> {
        if !self.finite_deformation {
            return Err(Error::invalid(
                "The Cauchy stress can only be computed if you are working in finite deformation.",
            ));
        }
        let d = self.spatial_dimension;
        akantu_core::with_dim!(d, D => self.cauchy_impl::<D>(ghost))?
    }

    fn cauchy_impl<const D: usize>(&mut self, ghost: GhostType) -> Result<()> {
        for ty in self.filter_types(ghost) {
            if !self.stress.exists(ty, ghost) {
                continue;
            }
            // Disjoint borrows instead of per-step copies of
            // grad_u and the PK2 array (same reads, same order of checks)
            let MaterialBase {
                grad_u,
                piola_kirchhoff_2,
                stress,
                ..
            } = &mut *self;
            let g = grad_u.get(ty, ghost)?;
            let s = piola_kirchhoff_2
                .as_ref()
                .ok_or_else(|| Error::invalid("piola_kirchhoff_2 is not registered"))?
                .get(ty, ghost)?;
            let mut sigma = stress.get_mut(ty, ghost)?;
            for ((out, gu), pk2) in sigma
                .as_mut_slice()
                .chunks_exact_mut(D * D)
                .zip(g.as_slice().chunks_exact(D * D))
                .zip(s.as_slice().chunks_exact(D * D))
            {
                let gu: &Matrix<D, D> = akantu_core::tensor::as_matrix(gu)?;
                let pk2: &Matrix<D, D> = akantu_core::tensor::as_matrix(pk2)?;
                let f = material_tensor::grad_u_to_f(gu);
                let c = material_tensor::s_to_cauchy(&f, pk2, 1.0);
                out.copy_from_slice(c.as_flattened());
            }
        }
        Ok(())
    }
}

/// Upstream `std::string name` (constitutive law) and `Material`
/// parameters, in registration order.
pub const BASE_PARAMS: &[ParamDescriptor<MaterialBase>] = &[
    ParamDescriptor::new(
        "name",
        ParamKind::ByteString,
        ParameterAccess(ParameterAccess::PARSABLE.0 | ParameterAccess::READABLE.0),
        "Name of the constitutive law",
        |s| ParamValue::ByteString(s.name_bytes.clone()),
        Some(|s, v| {
            s.set_name(v.into_byte_string()?);
            Ok(())
        }),
    ),
    ParamDescriptor::new(
        "rho",
        ParamKind::Real,
        ParameterAccess::PARSMOD,
        "Density",
        |s| ParamValue::Real(s.rho),
        Some(|s, v| {
            s.rho = v.as_real()?;
            Ok(())
        }),
    ),
    ParamDescriptor::new(
        "finite_deformation",
        ParamKind::Bool,
        ParameterAccess(ParameterAccess::PARSABLE.0 | ParameterAccess::READABLE.0),
        "Is finite deformation",
        |s| ParamValue::Bool(s.finite_deformation),
        Some(|s, v| {
            s.finite_deformation = v.as_bool()?;
            Ok(())
        }),
    ),
    ParamDescriptor::new(
        "inelastic_deformation",
        ParamKind::Bool,
        ParameterAccess::INTERNAL,
        "Is inelastic deformation",
        |s| ParamValue::Bool(s.inelastic_deformation),
        Some(|s, v| {
            s.inelastic_deformation = v.as_bool()?;
            Ok(())
        }),
    ),
    ParamDescriptor::new(
        "eigen_grad_u",
        ParamKind::MatrixReal,
        ParameterAccess::PARSABLE,
        "EigenGradU",
        |s| ParamValue::MatrixReal(s.eigen_grad_u_param.clone()),
        Some(|s, v| {
            if let ParamValue::MatrixReal(m) = v {
                let d = s.spatial_dimension;
                s.eigen_grad_u_param = fit_matrix(d, d, m)?;
            }
            Ok(())
        }),
    ),
];

/// `grad_u ← ∇(displacement) − eigen_grad_u` of `(ty, ghost)` computed in
/// the stored gradient (upstream `computeAllStresses` writes `gradu_vect`
/// directly) when the stored gradient and `eigen_grad_u` already hold
/// `filter.len()·nq` tuples of `dim²` values. The values are
/// those of the temporary path of [`Material::compute_all_stresses`] (same
/// gradient, same subtraction `g − e`), without its per-step temporary and
/// copy. `Ok(false)`, with nothing changed, when the shapes differ or a
/// block is missing (the caller then runs the temporary path, which keeps
/// its error behaviour).
fn grad_u_in_place(
    base: &mut MaterialBase,
    ctx: &LawContext<'_>,
    dim: usize,
    ty: ElementType,
    ghost: GhostType,
    filter: &[Idx],
) -> Result<bool> {
    let n = filter.len() * ctx.services.nb_integration_points(ty, ghost);
    let MaterialBase {
        grad_u,
        eigen_grad_u,
        ..
    } = base;
    // one read guard on `eigen_grad_u` and one write guard on `grad_u` for
    // the whole (type, ghost) block (D-071: both end with
    // this function); the gradient is written through a fixed-shape view
    let (Ok(eigen), Ok(mut gu)) = (eigen_grad_u.get(ty, ghost), grad_u.get_mut(ty, ghost)) else {
        return Ok(false);
    };
    if n == 0 || gu.len() != n || gu.nb_component() != dim * dim || eigen.len() != n {
        return Ok(false);
    }
    ctx.services.gradient_on_integration_points_view(
        "displacement",
        gu.view_mut(),
        dim,
        ty,
        ghost,
        filter,
    )?;
    if eigen.len() != gu.len() {
        return Err(Error::DimensionMismatch {
            context: "computeAllStresses: grad_u -= eigen_grad_u",
            expected: eigen.len(),
            got: gu.len(),
        });
    }
    // every value from its own pair: contiguous parts on the model's
    // threads (D-123)
    let n = gu.as_slice().len().min(eigen.as_slice().len());
    let items = (
        akantu_core::exec::Tuples::new(&mut gu.as_mut_slice()[..n], 1),
        akantu_core::exec::TuplesRef::new(&eigen.as_slice()[..n], 1),
    );
    ctx.services
        .exec()
        .for_each(items, 32_768, |_, (mut g, e)| {
            subtract_part(g.as_mut_slice(), e.as_slice());
        });
    Ok(true)
}

/// `g -= e` per value (a part of `grad_u -= eigen_grad_u`; no-alias
/// parameters, not inlined into the part closure).
#[inline(never)]
fn subtract_part(g: &mut [Real], e: &[Real]) {
    for (g, e) in g.iter_mut().zip(e) {
        *g -= *e;
    }
}

/// The executor of a law's per-point loop: the model's when the law
/// declares [`Material::independent_points`], else the serial one (D-123).
pub fn points_exec<'a, M: Material + ?Sized>(
    law: &M,
    ctx: &'a LawContext<'_>,
) -> &'a akantu_core::exec::Exec {
    if law.independent_points() {
        ctx.services.exec()
    } else {
        akantu_core::exec::Exec::serial_ref()
    }
}

/// Built-in laws: a static parameter table on top of [`BASE_PARAMS`]
/// (upstream registers the parameters of every class of the hierarchy in
/// one registry).
pub trait MaterialParams: Sized + 'static {
    /// The common part.
    fn params_base(&self) -> &MaterialBase;
    /// The common part, mutable.
    fn params_base_mut(&mut self) -> &mut MaterialBase;
    /// The law's own parameters (all classes of its hierarchy except
    /// `Material`).
    fn own_params(&self) -> &'static [ParamDescriptor<Self>];
    /// Upstream `updateInternalParameters` of the most derived class.
    fn update_params(&mut self) -> Result<()>;
}

/// [`akantu_parser::ParameterRegistry::get_param`] of a built-in law.
pub fn params_get<S: MaterialParams>(s: &S, name: &str) -> Result<ParamValue> {
    if table_info(BASE_PARAMS, name).is_some() {
        table_get(BASE_PARAMS, s.params_base(), name)
    } else {
        table_get(s.own_params(), s, name)
    }
}

/// `set` of a built-in law: writable check, conversion, then
/// `updateInternalParameters` (upstream `ParameterRegistry::set`).
pub fn params_set<S: MaterialParams>(s: &mut S, name: &str, value: ParamValue) -> Result<()> {
    if table_info(BASE_PARAMS, name).is_some() {
        table_set(BASE_PARAMS, s.params_base_mut(), name, value)?;
    } else {
        let table = s.own_params();
        table_set(table, s, name, value)?;
    }
    s.update_params()
}

/// `setAuto` (input file) of a built-in law: parsable check and conversion
/// only.
pub fn params_set_from_input<S: MaterialParams>(
    s: &mut S,
    name: &str,
    value: ParamValue,
) -> Result<()> {
    if table_info(BASE_PARAMS, name).is_some() {
        table_set_from_input(BASE_PARAMS, s.params_base_mut(), name, value)
    } else {
        let table = s.own_params();
        table_set_from_input(table, s, name, value)
    }
}

/// Kind and access of a parameter of a built-in law.
pub fn params_info<S: MaterialParams>(s: &S, name: &str) -> Option<ParamInfo> {
    table_info(BASE_PARAMS, name).or_else(|| table_info(s.own_params(), name))
}

/// Names of the parameters of a built-in law (base first).
pub fn params_names<S: MaterialParams>(s: &S) -> Vec<&'static str> {
    let mut v = table_names(BASE_PARAMS);
    v.extend(table_names(s.own_params()));
    v
}

/// Implements [`akantu_parser::ParameterRegistry`] and
/// [`akantu_parser::Parsable`] for a built-in law implementing
/// [`MaterialParams`].
#[macro_export]
macro_rules! impl_material_registry {
    ($ty:ty $(, const $d:ident)?) => {
        impl$(<const $d: usize>)? ::akantu_parser::ParameterRegistry for $ty {
            fn get_param(&self, name: &str) -> ::akantu_core::Result<::akantu_parser::ParamValue> {
                $crate::material::params_get(self, name)
            }
            fn set_param(
                &mut self,
                name: &str,
                value: ::akantu_parser::ParamValue,
            ) -> ::akantu_core::Result<()> {
                $crate::material::params_set(self, name, value)
            }
            fn param_names(&self) -> Vec<&'static str> {
                $crate::material::params_names(self)
            }
            fn update_internal_parameters(&mut self) -> ::akantu_core::Result<()> {
                $crate::material::MaterialParams::update_params(self)
            }
            fn param_info(&self, name: &str) -> Option<::akantu_parser::param::ParamInfo> {
                $crate::material::params_info(self, name)
            }
            fn set_param_from_input(
                &mut self,
                name: &str,
                value: ::akantu_parser::ParamValue,
            ) -> ::akantu_core::Result<()> {
                $crate::material::params_set_from_input(self, name, value)
            }
        }
        impl$(<const $d: usize>)? ::akantu_parser::Parsable for $ty {
            fn section_type(&self) -> ::akantu_parser::ParserType {
                ::akantu_parser::ParserType::Material
            }
            fn parsable_id(&self) -> &str {
                &$crate::material::MaterialParams::params_base(self).id
            }
        }
    };
}

/// Built-in laws deriving from another built-in law (upstream class
/// inheritance, e.g. `MaterialStandardLinearSolidDeviatoric : MaterialElastic`):
/// the parent keeps its registry (base and parent parameters, its own
/// update), the derived law adds a static table. `set` of a parent
/// parameter runs the parent's update and then
/// [`DerivedMaterialParams::update_derived`] (upstream calls the most
/// derived `updateInternalParameters`, which calls the parent's first; the
/// parent's update is idempotent, so running it twice is equivalent).
pub trait DerivedMaterialParams: Sized + 'static {
    /// The parent law (registry of every parameter but the derived ones).
    type Parent: akantu_parser::ParameterRegistry;
    /// The parent.
    fn parent(&self) -> &Self::Parent;
    /// The parent, mutable.
    fn parent_mut(&mut self) -> &mut Self::Parent;
    /// Parameters registered by the derived class (after the parent's).
    fn derived_params(&self) -> &'static [ParamDescriptor<Self>];
    /// Upstream `updateInternalParameters` of the derived class (including
    /// the parent's update).
    fn update_derived(&mut self) -> Result<()>;
    /// Law id (`Parsable::parsable_id`).
    fn derived_id(&self) -> &str;
}

/// `get` of a derived law.
pub fn derived_get<S: DerivedMaterialParams>(s: &S, name: &str) -> Result<ParamValue> {
    let own = s.derived_params();
    if table_info(own, name).is_some() {
        table_get(own, s, name)
    } else {
        akantu_parser::ParameterRegistry::get_param(s.parent(), name)
    }
}

/// `set` of a derived law (writable check, conversion, updates).
pub fn derived_set<S: DerivedMaterialParams>(
    s: &mut S,
    name: &str,
    value: ParamValue,
) -> Result<()> {
    let own = s.derived_params();
    if table_info(own, name).is_some() {
        table_set(own, s, name, value)?;
    } else {
        akantu_parser::ParameterRegistry::set_param(s.parent_mut(), name, value)?;
    }
    s.update_derived()
}

/// `setAuto` (input file) of a derived law: parsable check and conversion
/// only.
pub fn derived_set_from_input<S: DerivedMaterialParams>(
    s: &mut S,
    name: &str,
    value: ParamValue,
) -> Result<()> {
    let own = s.derived_params();
    if table_info(own, name).is_some() {
        table_set_from_input(own, s, name, value)
    } else {
        akantu_parser::ParameterRegistry::set_param_from_input(s.parent_mut(), name, value)
    }
}

/// Kind and access of a parameter of a derived law.
pub fn derived_info<S: DerivedMaterialParams>(s: &S, name: &str) -> Option<ParamInfo> {
    akantu_parser::ParameterRegistry::param_info(s.parent(), name)
        .or_else(|| table_info(s.derived_params(), name))
}

/// Names of a derived law (parent first, upstream registration order).
pub fn derived_names<S: DerivedMaterialParams>(s: &S) -> Vec<&'static str> {
    let mut v = akantu_parser::ParameterRegistry::param_names(s.parent());
    v.extend(table_names(s.derived_params()));
    v
}

/// Implements [`akantu_parser::ParameterRegistry`] and
/// [`akantu_parser::Parsable`] for a built-in law implementing
/// [`DerivedMaterialParams`].
#[macro_export]
macro_rules! impl_derived_material_registry {
    ($ty:ty $(, const $d:ident)?) => {
        impl$(<const $d: usize>)? ::akantu_parser::ParameterRegistry for $ty {
            fn get_param(&self, name: &str) -> ::akantu_core::Result<::akantu_parser::ParamValue> {
                $crate::material::derived_get(self, name)
            }
            fn set_param(
                &mut self,
                name: &str,
                value: ::akantu_parser::ParamValue,
            ) -> ::akantu_core::Result<()> {
                $crate::material::derived_set(self, name, value)
            }
            fn param_names(&self) -> Vec<&'static str> {
                $crate::material::derived_names(self)
            }
            fn update_internal_parameters(&mut self) -> ::akantu_core::Result<()> {
                $crate::material::DerivedMaterialParams::update_derived(self)
            }
            fn param_info(&self, name: &str) -> Option<::akantu_parser::param::ParamInfo> {
                $crate::material::derived_info(self, name)
            }
            fn set_param_from_input(
                &mut self,
                name: &str,
                value: ::akantu_parser::ParamValue,
            ) -> ::akantu_core::Result<()> {
                $crate::material::derived_set_from_input(self, name, value)
            }
        }
        impl$(<const $d: usize>)? ::akantu_parser::Parsable for $ty {
            fn section_type(&self) -> ::akantu_parser::ParserType {
                ::akantu_parser::ParserType::Material
            }
            fn parsable_id(&self) -> &str {
                $crate::material::DerivedMaterialParams::derived_id(self)
            }
        }
    };
}

/// A solid constitutive law.
///
/// Batch methods operate on all quadrature points of the material's elements
/// of one `(type, ghost)` block — the virtual call is outside the hot loop,
/// which is monomorphised inside the implementation.
pub trait Material: ConstitutiveLaw {
    /// Common state.
    fn base(&self) -> &MaterialBase;

    /// Mutable common state (the model writes `grad_u`, reads `stress`).
    fn base_mut(&mut self) -> &mut MaterialBase;

    /// Initialise internals after parameters were parsed (upstream
    /// `initMaterial`).
    fn init_material(&mut self, ctx: &LawContext<'_>) -> Result<()>;

    /// Compute stresses from `grad_u` (upstream `computeStress(type, ghost)`).
    fn compute_stress(
        &mut self,
        ctx: &LawContext<'_>,
        ty: ElementType,
        ghost: GhostType,
    ) -> Result<()>;

    /// The law's declaration for threads (D-123): `true` when
    /// [`Material::compute_stress`] updates every quadrature point from that
    /// point's own data (no state shared between points, no non-local term,
    /// no reduction), so its per-point loop may run on the model's threads
    /// ([`points_exec`]). Default `false`: the loop stays serial.
    fn independent_points(&self) -> bool {
        false
    }

    /// Tangent moduli per quadrature point, column-major Voigt matrices
    /// (`tangent` has one tuple of `voigt²` values per quadrature point of
    /// the block; upstream `computeTangentModuli`).
    fn compute_tangent_moduli(
        &mut self,
        ctx: &LawContext<'_>,
        ty: ElementType,
        ghost: GhostType,
        tangent: &mut Array<Real>,
    ) -> Result<()>;

    /// Potential energy density of the not-ghost quadrature points of `ty`
    /// into the `potential_energy` internal (upstream
    /// `computePotentialEnergy`; the base class raises "not implemented").
    fn compute_potential_energy(&mut self, ctx: &LawContext<'_>, ty: ElementType) -> Result<()>;

    /// P-wave speed of `element` (upstream `getPushWaveSpeed`).
    fn push_wave_speed(&self, element: Element) -> Result<Real>;

    /// S-wave speed (upstream `getShearWaveSpeed`).
    fn shear_wave_speed(&self, element: Element) -> Result<Real>;

    /// Wave speed used by the stable time step (upstream `getCelerity`,
    /// default: the P-wave speed).
    fn celerity(&self, element: Element) -> Result<Real> {
        self.push_wave_speed(element)
    }

    /// Density (upstream `rho`).
    fn rho(&self) -> Real {
        self.base().rho
    }

    /// Symmetry of the tangent (upstream `getTangentType`; base:
    /// `_mt_not_defined`).
    fn tangent_type(&self) -> MatrixType {
        MatrixType::NotDefined
    }

    /// Whether the stiffness must be re-assembled (upstream
    /// `hasStiffnessMatrixChanged`, base: `true`).
    fn has_stiffness_matrix_changed(&self) -> bool {
        true
    }

    /// Upstream `hasMassMatrixChanged` (base: `false`).
    fn has_mass_matrix_changed(&self) -> bool {
        false
    }

    /// Upstream `hasMatrixChanged(id)`: `"K"` → stiffness changed or
    /// finite deformation; `"M"` → mass changed; otherwise `true`.
    fn has_matrix_changed(&self, id: &str) -> bool {
        match id {
            "K" => self.has_stiffness_matrix_changed() || self.base().finite_deformation,
            "M" => self.has_mass_matrix_changed(),
            _ => true,
        }
    }

    /// [`Material::has_stiffness_matrix_changed`] for laws whose answer
    /// depends on the model state of the phase (upstream reads the model,
    /// e.g. `viscoelastic_maxwell` compares the current time step with the
    /// one of its last tangent). Default: the state-free answer.
    fn has_stiffness_matrix_changed_in(&self, ctx: &LawContext<'_>) -> bool {
        let _ = ctx;
        self.has_stiffness_matrix_changed()
    }

    /// An error the law could not return from an infallible question (a
    /// runtime law's `hasStiffnessMatrixChanged`, i.e. a Python exception):
    /// the model calls this right after
    /// [`Material::has_matrix_changed_in`] and propagates it (upstream's
    /// exception leaves `hasStiffnessMatrixChanged` there). Default: none.
    fn take_deferred_error(&self) -> Result<()> {
        Ok(())
    }

    /// [`Material::has_matrix_changed`] with the model state of the phase
    /// (see [`Material::has_stiffness_matrix_changed_in`]).
    fn has_matrix_changed_in(&self, ctx: &LawContext<'_>, id: &str) -> bool {
        match id {
            "K" => self.has_stiffness_matrix_changed_in(ctx) || self.base().finite_deformation,
            "M" => self.has_mass_matrix_changed(),
            _ => true,
        }
    }

    /// Upstream `setToSteadyState(type, ghost)` (base: nothing; the
    /// standard linear solid initialises its viscous state from `grad_u`).
    fn set_to_steady_state(
        &mut self,
        ctx: &LawContext<'_>,
        ty: ElementType,
        ghost: GhostType,
    ) -> Result<()> {
        let _ = (ctx, ty, ghost);
        Ok(())
    }

    /// Upstream `Material::setToSteadyState(ghost)`: for every type of the
    /// filter (law dimension), `grad_u ← ∇(displacement)` (model FE engine;
    /// **no** `eigen_grad_u` subtraction, as upstream), then
    /// [`Material::set_to_steady_state`].
    fn set_all_to_steady_state(&mut self, ctx: &LawContext<'_>, ghost: GhostType) -> Result<()> {
        let dim = self.base().spatial_dimension;
        for ty in self.base().filter_types(ghost) {
            let filter = self
                .base()
                .element_filter
                .get(ty, ghost)?
                .as_slice()
                .to_vec();
            let mut gu = Array::new(dim * dim);
            ctx.services.gradient_on_integration_points(
                "displacement",
                &mut gu,
                dim,
                ty,
                ghost,
                &filter,
            )?;
            if !filter.is_empty() {
                self.base_mut().grad_u.get_mut(ty, ghost)?.copy_from(&gu)?;
            }
            self.set_to_steady_state(ctx, ty, ghost)?;
        }
        Ok(())
    }

    /// Upstream `getMatrixType(id)`: `"K"` → tangent type, `"M"` →
    /// symmetric, otherwise not defined.
    fn matrix_type(&self, id: &str) -> MatrixType {
        match id {
            "K" => self.tangent_type(),
            "M" => MatrixType::Symmetric,
            _ => MatrixType::NotDefined,
        }
    }

    /// Upstream `Material::computeAllStresses(ghost)`: for every type of
    /// the filter (law dimension) with elements, `grad_u ← ∇(displacement)`
    /// (model FE engine) `− eigen_grad_u`, then `computeStress`.
    fn compute_all_stresses(&mut self, ctx: &LawContext<'_>, ghost: GhostType) -> Result<()> {
        let dim = self.base().spatial_dimension;
        for ty in self.base().filter_types(ghost) {
            let filter = self
                .base()
                .element_filter
                .get(ty, ghost)?
                .as_slice()
                .to_vec();
            if filter.is_empty() {
                continue;
            }
            // In the stored gradient when its shape allows (same
            // values, no per-step temporary); else the temporary below
            if grad_u_in_place(self.base_mut(), ctx, dim, ty, ghost, &filter)? {
                self.compute_stress(ctx, ty, ghost)?;
                continue;
            }
            let mut gu = Array::new(dim * dim);
            ctx.services.gradient_on_integration_points(
                "displacement",
                &mut gu,
                dim,
                ty,
                ghost,
                &filter,
            )?;
            {
                let base = self.base_mut();
                let eigen = base.eigen_grad_u.get(ty, ghost)?;
                if eigen.len() != gu.len() {
                    return Err(Error::DimensionMismatch {
                        context: "computeAllStresses: grad_u -= eigen_grad_u",
                        expected: eigen.len(),
                        got: gu.len(),
                    });
                }
                for (g, e) in gu.as_mut_slice().iter_mut().zip(eigen.as_slice()) {
                    *g -= *e;
                }
                base.grad_u.get_mut(ty, ghost)?.copy_from(&gu)?;
            }
            self.compute_stress(ctx, ty, ghost)?;
        }
        Ok(())
    }

    /// The rank reductions upstream performs at the end of
    /// `computeAllStresses(ghost)` (`MaterialDamageIterative`,
    /// `MaterialOrthotropicDamageIterative`: `allReduce` of the maximum
    /// normalised equivalent stress), **declared** instead of issued inside
    /// [`Material::compute_all_stresses`]: the model calls this for
    /// every law, in law order, right after the stress loop of the same
    /// phase, with a reducer bound to the running collective operation
    /// ([`LawReducer`]). Default: none.
    ///
    /// # Errors
    /// Reducer errors (the phase failed on some rank, transport failures).
    fn reduce_after_stresses(
        &mut self,
        ghost: GhostType,
        reducer: &mut dyn LawReducer,
    ) -> Result<()> {
        let _ = (ghost, reducer);
        Ok(())
    }

    /// Upstream `getPotentialEnergy()`: `computePotentialEnergy(type)` for
    /// every not-ghost type of the filter, then the integral.
    fn potential_energy(&mut self, ctx: &LawContext<'_>) -> Result<Real> {
        for ty in self.base().filter_types(GhostType::NotGhost) {
            self.compute_potential_energy(ctx, ty)?;
        }
        self.base().integrate_potential_energy(ctx)
    }

    /// Upstream `getEnergy(id)`: `"potential"` → [`Material::potential_energy`],
    /// anything else `0` (laws override for dissipated energies…).
    fn energy_of(&mut self, ctx: &LawContext<'_>, energy_id: &str) -> Result<Real> {
        if energy_id == "potential" {
            self.potential_energy(ctx)
        } else {
            Ok(0.0)
        }
    }

    /// Potential energy density at the quadrature points of one law-local
    /// element (upstream `computePotentialEnergyByElement`).
    fn potential_energy_by_element(
        &self,
        ctx: &LawContext<'_>,
        element: Element,
    ) -> Result<Vec<Real>> {
        let _ = (ctx, element);
        Err(Error::NotImplemented("computePotentialEnergyByElement"))
    }

    /// Per-element potential energy (upstream `getPotentialEnergy(element)`
    /// with a law-local element). Upstream integrates an **uninitialised**
    /// vector (UD-032, category A); the port integrates the energy density
    /// of the element, [`Material::potential_energy_by_element`].
    fn potential_energy_of_element(&self, ctx: &LawContext<'_>, element: Element) -> Result<Real> {
        let density = self.potential_energy_by_element(ctx, element)?;
        let global = self
            .base()
            .element_filter
            .get(element.ty(), element.ghost_type())?
            .as_slice()
            .get(element.idx())
            .copied()
            .ok_or(Error::InvalidIndex {
                what: "law-local element",
                value: i64::from(element.index()),
            })?;
        ctx.services
            .integrate_scalar(&density, element.ty(), GhostType::NotGhost, &[global])
    }

    /// Upstream virtual `getEnergy(energy_id, element)` with a law-local
    /// `element`: `"potential"` → [`Material::potential_energy_of_element`],
    /// anything else `0` (laws with other per-element
    /// energies — the viscoelastic laws' `dissipated`, … — override it; the
    /// model's `energy_of_element` / `energy_of_group` dispatch through it).
    fn energy_of_element(
        &mut self,
        ctx: &LawContext<'_>,
        energy_id: &str,
        element: Element,
    ) -> Result<Real> {
        if energy_id == "potential" {
            self.potential_energy_of_element(ctx, element)
        } else {
            Ok(0.0)
        }
    }

    /// Upstream `updateEnergies(type)` (after a converged step; base: no-op).
    fn update_energies(&mut self, ctx: &LawContext<'_>, ty: ElementType) -> Result<()> {
        let _ = (ctx, ty);
        Ok(())
    }

    /// Upstream `updateEnergiesAfterDamage(type)` (base: no-op).
    fn update_energies_after_damage(
        &mut self,
        ctx: &LawContext<'_>,
        ty: ElementType,
    ) -> Result<()> {
        let _ = (ctx, ty);
        Ok(())
    }

    /// Hook before a solve step (upstream `beforeSolveStep` =
    /// `savePreviousState`).
    fn before_solve_step(&mut self, ctx: &LawContext<'_>) -> Result<()> {
        let _ = ctx;
        self.save_previous_state()
    }

    /// Hook after a solve step (upstream `afterSolveStep`: restore the
    /// previous state if not converged, else `updateEnergies` for every
    /// not-ghost type of the filter, all dimensions and kinds).
    fn after_solve_step(&mut self, ctx: &LawContext<'_>, converged: bool) -> Result<()> {
        if !converged {
            return self.restore_previous_state();
        }
        let types: Vec<ElementType> = self
            .base()
            .element_filter
            .as_map()
            .iter(GhostType::NotGhost)
            .map(|(t, _)| t)
            .collect();
        for ty in types {
            self.update_energies(ctx, ty)?;
        }
        Ok(())
    }

    /// Upstream `onDamageIteration` (`savePreviousState`).
    fn on_damage_iteration(&mut self) -> Result<()> {
        self.save_previous_state()
    }

    /// Upstream `onDamageUpdate` (`updateEnergiesAfterDamage` for every
    /// not-ghost type).
    fn on_damage_update(&mut self, ctx: &LawContext<'_>) -> Result<()> {
        let types: Vec<ElementType> = self
            .base()
            .element_filter
            .as_map()
            .iter(GhostType::NotGhost)
            .map(|(t, _)| t)
            .collect();
        for ty in types {
            self.update_energies_after_damage(ctx, ty)?;
        }
        Ok(())
    }

    /// Upstream `onDump` (Cauchy stresses in finite deformation).
    fn on_dump(&mut self) -> Result<()> {
        if self.base().finite_deformation {
            self.compute_all_cauchy_stresses(GhostType::NotGhost)
        } else {
            Ok(())
        }
    }

    /// Upstream `computeAllCauchyStresses(ghost)` (overridden by the 2-D
    /// plane-stress toolbox).
    fn compute_all_cauchy_stresses(&mut self, ghost: GhostType) -> Result<()> {
        self.base_mut().compute_all_cauchy_stresses(ghost)
    }

    /// The law's upstream-defect switches (`None` for laws without any).
    fn options(&self) -> Option<&LawOptions> {
        None
    }

    /// Mutable [`Material::options`].
    fn options_mut(&mut self) -> Option<&mut LawOptions> {
        None
    }

    /// Replace the law's upstream-defect switches and run
    /// `updateInternalParameters`, so that switches shaping the stiffness
    /// take effect immediately (see [`LawOptions`] for the one effect an
    /// update cannot undo). A law without switches ignores the call.
    ///
    /// # Errors
    /// The law's parameter update error.
    fn set_options(&mut self, options: LawOptions) -> Result<()> {
        // A change of options changes the derived state
        // (stiffness, axes, energy convention) whatever the mode, so the
        // stiffness is always flagged changed afterwards — even in upstream
        // compat mode, whose own update does not reset the flag (UD-105) —
        // and a failing update restores the previous options and their
        // derived state instead of leaving a partial update behind.
        let previous = match self.options_mut() {
            Some(o) => std::mem::replace(o, options),
            None => return Ok(()),
        };
        match self.update_internal_parameters() {
            Ok(()) => {
                self.mark_stiffness_changed();
                Ok(())
            }
            Err(e) => {
                if let Some(o) = self.options_mut() {
                    *o = previous;
                }
                // The previous options were applied successfully before;
                // recompute their derived state, keeping the original error.
                let _restored = self.update_internal_parameters();
                self.mark_stiffness_changed();
                Err(e)
            }
        }
    }

    /// Force the next `has_stiffness_matrix_changed` to report `true`
    /// (laws that track stiffness assembly override it; the base reports
    /// `true` always, so the default does nothing).
    fn mark_stiffness_changed(&mut self) {}

    /// Law-specific part of upstream's virtual
    /// `ConstitutiveLaw::onElementsAdded(element_list, event)`, called by the
    /// model after the law's internals were resized for the new elements
    /// (`elements`: every element of the event; e.g. `cohesive_bilinear`
    /// recomputes the strengths of new cohesive elements). Default: no-op.
    fn on_elements_added(&mut self, ctx: &LawContext<'_>, elements: &[Element]) -> Result<()> {
        let _ = (ctx, elements);
        Ok(())
    }

    /// The non-local part of the law (upstream `dynamic_cast<
    /// MaterialNonLocalInterface *>`), `None` for local laws (default).
    /// The solid model creates its non-local manager when a law returns
    /// `Some` (`crate::materials::non_local`).
    fn as_non_local(&self) -> Option<&dyn crate::materials::non_local::MaterialNonLocal> {
        None
    }

    /// Mutable [`Material::as_non_local`].
    fn as_non_local_mut(
        &mut self,
    ) -> Option<&mut dyn crate::materials::non_local::MaterialNonLocal> {
        None
    }

    /// Downcast support for model extensions: the cohesive model
    /// (`akantu-cohesive`) reaches its cohesive laws through the
    /// model's law handler (upstream `aka::as_type<MaterialCohesive>`).
    /// Laws that are not extended return `None` (default).
    fn as_any(&self) -> Option<&dyn std::any::Any> {
        None
    }

    /// Mutable [`Material::as_any`].
    fn as_any_mut(&mut self) -> Option<&mut dyn std::any::Any> {
        None
    }

    /// Override of upstream's virtual `Material::assembleInternalForces(ghost)`
    /// for laws with their own residual (cohesive laws: `−∫ Nᵀ t` on the two
    /// sides of the cohesive elements, `MaterialCohesive::assembleInternalForces`).
    /// The model calls it **at the law's position** in its material loop
    /// (so that every nodal sum keeps upstream's order) and skips the
    /// generic small-strain / finite-deformation pipeline when it returns
    /// `Some`. `fems` are the model's FE engines, `f_int` a fixed-shape view
    /// of the model's internal forces: the model holds the (exportable,
    /// D-047) array's guard once for the whole loop and lends its values,
    /// so a law neither takes a second guard nor can resize the nodal field
    /// (`docs/ARCHITECTURE.md`). Default: `None` (generic pipeline).
    fn assemble_internal_forces_custom(
        &self,
        fems: &akantu_model::model::FeEngineRegistry,
        mesh: &Mesh,
        ghost: GhostType,
        f_int: ArrayViewMut<'_, Real>,
    ) -> Option<Result<()>> {
        let _ = (fems, mesh, ghost, f_int);
        None
    }

    /// Override of upstream's virtual `Material::assembleStiffnessMatrix(ghost)`
    /// for laws with their own tangent (cohesive laws,
    /// `MaterialCohesive::assembleStiffnessMatrix`): the elemental matrices
    /// `(type, filter, K_e, elemental matrix type)` that the model
    /// assembles into `K` at the law's position in its material loop
    /// (cohesive blocks are `_unsymmetric`, UD-034). Default: `None` (the
    /// generic `BᵀDB` pipeline, `_symmetric` blocks).
    #[allow(clippy::type_complexity)] // one block list, mirrors upstream's per-type assembly
    fn elemental_stiffness_custom(
        &mut self,
        ctx: &LawContext<'_>,
        ghost: GhostType,
    ) -> Option<Result<Vec<(ElementType, Vec<Idx>, Array<Real>, MatrixType)>>> {
        let _ = (ctx, ghost);
        None
    }

    /// The internal fields the law exchanges with the ghost copies of its
    /// elements for a law-specific synchronisation `tag` (upstream
    /// `Material::getNbData` / `packData` / `unpackData` overrides,
    /// MPI-041 / MPI-053): names of `Real` internals, in upstream pack
    /// order; each is packed element by element, every quadrature point and
    /// component of the element (upstream `packInternalFieldHelper`), after
    /// the model's generic data of the tag (`_smm_stress` / `_smm_gradu`:
    /// none of the built-in laws adds data to them). Built-in overrides:
    /// Marigo `_clh_init_cl` → `Yd`; phase field `_smm_density` with
    /// `degrade_mass` → `damage`, `grad_u`. User laws may answer for the
    /// extension tags `_test`, `_user_1`, `_user_2` (exchanged by
    /// `SolidMechanicsModel::synchronize` once the user registered the tag,
    /// `SolidMechanicsModel::register_synchronization_tag`). Default: none.
    fn exchanged_internals(&self, tag: akantu_core::enums::SynchronizationTag) -> Vec<&str> {
        let _ = tag;
        Vec::new()
    }

    /// Density per quadrature point of `element` (upstream `getRho(rhos,
    /// element)`: the constant `rho`).
    fn rho_on_quadrature_points(&self, element: Element, out: &mut [Real]) {
        let _ = element;
        out.fill(self.base().rho);
    }
}

/// Factory of solid materials (upstream `MaterialFactory`). Allocators
/// receive the spec and the model services of the construction phase (a
/// Python constructor `LocalElastic(model, id)` gets a construction-scoped
/// model handle).
pub type MaterialFactory = ConstitutiveLawFactory<dyn Material>;

/// Register every built-in material implemented so far under its upstream
/// name (upstream `instantiateMaterial<Mat>("name")` registrations):
/// `elastic`, `elastic_anisotropic`, `elastic_orthotropic`;
/// `marigo`, `mazars`, `plastic_mazars`,
/// `plastic_linear_isotropic_hardening`, `plastic_drucker_prager`,
/// `neohookean`; `sls_deviatoric`, `viscoelastic_maxwell`,
/// `anisotropic_damage`, `phasefield`. The remaining upstream keywords
/// (`docs/FEATURE_PARITY.md` SMM-032) are registered by the work packages
/// that implement them.
pub fn register_builtin_materials(factory: &mut MaterialFactory) -> Result<()> {
    crate::materials::elastic::register(factory)?;
    crate::materials::anisotropic::register(factory)?;
    crate::materials::orthotropic::register(factory)?;
    // --- local inelastic laws ---
    crate::materials::marigo::register(factory)?;
    crate::materials::mazars::register(factory)?;
    crate::materials::plastic::register(factory)?;
    crate::materials::drucker_prager::register(factory)?;
    crate::materials::neohookean::register(factory)?;
    // --- end of the local inelastic laws ---
    // Viscoelastic, anisotropic damage, phase field.
    crate::materials::sls_deviatoric::register(factory)?;
    crate::materials::maxwell::register(factory)?;
    crate::materials::anisotropic_damage::register(factory)?;
    crate::materials::phasefield::register(factory)?;
    // Non-local damage laws (package damage_non_local)
    #[cfg(feature = "damage-non-local")]
    crate::materials::non_local::register(factory)?;
    Ok(())
}

/// Validate a tangent array against the block of `grad_u` (upstream zips
/// both views; sizes must agree).
pub(crate) fn check_tangent(tangent: &Array<Real>, nb_quad: usize, voigt: usize) -> Result<()> {
    if tangent.nb_component() != voigt * voigt {
        return Err(Error::DimensionMismatch {
            context: "computeTangentModuli: tangent components",
            expected: voigt * voigt,
            got: tangent.nb_component(),
        });
    }
    if tangent.len() != nb_quad {
        return Err(Error::DimensionMismatch {
            context: "computeTangentModuli: tangent tuples",
            expected: nb_quad,
            got: tangent.len(),
        });
    }
    Ok(())
}

/// `(type, ghost, filter length · nb_quad)` of every block of `filter`,
/// `nb_quad` from the model's FE engine `engine` (`None`: its default
/// engine): the blocks source of
/// [`ConstitutiveLaw::resize_internals_with`] for internals registered on
/// another FE engine or element filter than their law's (CL-001, upstream
/// `registerInternal(id, nb_component, fe_engine_id[, element_filter])`;
/// `InternalField::resize` takes `nb_quad` from the field's `fem`).
///
/// # Errors
/// An unknown FE engine, a type the named engine does not know, a size
/// overflow.
pub fn internal_blocks(
    ctx: &LawContext<'_>,
    engine: Option<&str>,
    filter: &akantu_core::ElementTypeMapArray<akantu_core::Idx>,
) -> Result<akantu_model::constitutive_law::InternalBlocks> {
    let fe = engine.map(|id| ctx.services.fe_engine(id)).transpose()?;
    let mut out = Vec::new();
    for (ty, g, f) in filter.as_map().iter_all() {
        let nq = match fe {
            None => ctx.services.nb_integration_points(ty, g),
            Some(fe) => fe.nb_integration_points(ty, g)?,
        };
        let n = f
            .len()
            .checked_mul(nq)
            .ok_or_else(|| Error::invalid("internal field size overflow"))?;
        out.push((ty, g, n));
    }
    Ok(out)
}
