// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/model/solid_mechanics/
// solid_mechanics_model_cohesive/materials/material_cohesive{.hh,.cc,
// _inline_impl.hh}, cohesive_internal_field{.hh,_tmpl.hh}.

//! [`MaterialCohesive`] (COH-010/011): the common part of every cohesive
//! law — cohesive internals (`opening`, `tractions`, `damage`, energies…),
//! the facet strength `sigma_c` (a random field on the facets the law may
//! be inserted on), the residual `−∫ Nᵀ t` and tangent `∫ (NA)ᵀ D (NA)`
//! assemblies, normals and openings, energies — plus the law itself
//! ([`crate::laws::CohesiveLaw`]).
//!
//! It is a `dyn Material` of the solid mechanics model's law handler (one
//! material numbering for bulk and cohesive laws, as upstream): the model
//! skips its generic small-strain pipeline for it through the
//! `Material::{assemble_internal_forces_custom, elemental_stiffness_custom}`
//! overrides, and the cohesive model reaches it through `as_any_mut`.
//!
//! **Internal domains.** Upstream internals carry an element kind and an
//! FE engine: cohesive internals (`CohesiveInternalField`, engine
//! `CohesiveFEEngine`) have one block per cohesive type of the element
//! filter; facet internals (`FacetInternalField`, `FacetsFEEngine`) one
//! block per facet type of the facet filter; the regular base internals of
//! `Material` (`stress`, `grad_u`, …) stay empty. Quadrature-point counts
//! are the engines' (cohesive: default order; facets: the order of the
//! matching cohesive type, `FacetsCohesiveIntegrationOrderFunctor`).

use std::sync::Arc;

use akantu_core::array::ArrayViewMut;
use akantu_core::array_shared::{ArrayMut, ArrayRef, SliceRef};
use akantu_core::enums::MatrixType;
use akantu_core::{
    Array, Element, ElementKind, ElementType, ElementTypeMapArray, Error, GhostType, Idx, Real,
    Result, TypeFilter,
};
use akantu_fe::FeEngine;
use akantu_fe::eigen::{Dims, product_slices};
use akantu_fe::shape::CohesiveReduction;
use akantu_mesh::Mesh;
use akantu_model::InternalField;
use akantu_model::constitutive_law::{
    ConstitutiveLaw, InternalFieldMut, InternalFieldRef, LawContext, LawSpec, ModelServices,
};
use akantu_model::internal_field::AnyInternalField;
use akantu_model::model::FeEngineRegistry;
use akantu_parser::param::{ParamDescriptor, ParamKind, ParamValue, ParameterAccess};
use akantu_solid::material::{Material, MaterialBase, MaterialFactory, MaterialParams};
use akantu_solver::DofManager;

use crate::laws::CohesiveLaw;

/// Name of the cohesive FE engine of the model (upstream
/// `"CohesiveFEEngine"`).
pub const COHESIVE_FE_ENGINE: &str = "CohesiveFEEngine";

/// Upstream-defect switches of the cohesive model and laws (default:
/// corrected behaviour; `docs/UPSTREAM_DEFECTS.md`).
///
/// The model holds one set ([`crate::CohesiveExtension::compat`], set before
/// `init_full`): its model-level switches are read by the model, and the
/// whole set is merged (field-wise OR) into every cohesive law's own
/// [`CohesiveCore::compat`] when the laws are initialised. A law's switches
/// may also be changed afterwards (e.g. `sigma_c_zipped_per_quad`, read at
/// every insertion check).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CohesiveUpstreamCompat {
    /// **UD-040**: the insertion check zips the facet filter with the
    /// per-quadrature-point `sigma_c` array, so with more than one
    /// quadrature point per facet facet `k` is compared with strength entry
    /// `k` (`material_cohesive_linear.cc` `checkInsertion`). Default: facet
    /// `f` uses the mean of its own points' strengths for the threshold and
    /// each point's strength for `δ_c`.
    pub sigma_c_zipped_per_quad: bool,
    /// **UD-028**: `delta max` keeps no history (upstream enables it in
    /// the base constructor before the laws set `use_previous_delta_max`):
    /// a restored (non-converged) step keeps the trial `δ_max`. Default:
    /// history enabled when the law uses the previous `δ_max`.
    pub delta_max_without_history: bool,
    /// **UD-217** (model level): 3-D facet tangents as
    /// upstream `Math::compute_tangents`, whose first tangent applies the
    /// 2-D rotation to a 3-D normal, `t₁ = normalize(n₁, −n₀, n₂)` — not
    /// orthogonal to `n` when `n₂ ≠ 0`, so the effective stress of an
    /// inclined facet picks up normal traction as shear. Default: `t₁ =
    /// normalize(n₁, −n₀, 0)` (or the Gram–Schmidt complement of `e₀` when
    /// `n ∥ e₂`), orthogonal to `n`.
    pub tangent_rotation_2d_in_3d: bool,
    /// **UD-219** (model level): `initConstitutiveLaws`
    /// skips every facet whose selected material index is 0 (`if (not
    /// mat_index) return;`), so a cohesive law listed first in the input
    /// never receives a facet and extrinsic insertion is silently disabled.
    /// Default: a facet is skipped only when the selected law is not a
    /// cohesive law (no cohesive material for it).
    pub skip_material_index_0: bool,
    /// **UD-218**: `cohesive_bilinear::onElementsAdded`
    /// visits every element of the event (also other laws' elements) and
    /// indexes its strength arrays with the **mesh** index (out of range ⇒
    /// error, upstream undefined behaviour). Default: only the law's own
    /// new elements, at their law-local index.
    pub bilinear_mesh_indices: bool,
    /// **UD-220**: the linear-family tangent uses the
    /// law's base `σc` while the traction uses the per-point `σc,eff`
    /// (inconsistent Jacobian). Default: the per-point `σc,eff`.
    pub tangent_base_sigma_c: bool,
    /// **UD-224** (model level): cohesive elements
    /// inserted into a linear-family law without an elastic branch
    /// (`cohesive_linear`, `_fatigue`, `_friction`, `_uncoupled`) outside
    /// the extrinsic insertion check — intrinsic insertion (`init_full(…,
    /// false)`, `insert_intrinsic_elements`) or a manual `insert_elements`
    /// — keep `σc,eff = 0`: the interface has no strength (zero traction
    /// except in contact). Default: such an insertion is refused with an
    /// error naming the law (use `cohesive_bilinear` for intrinsic
    /// insertion, or extrinsic insertion).
    pub intrinsic_linear_zero_strength: bool,
    /// **UD-241**: the linear family raises its
    /// maximal opening at every traction evaluation, `δ_max ← max(δ_max,
    /// δ)` on the current array (`cohesive_linear`, `cohesive_bilinear`,
    /// and `cohesive_linear_uncoupled`'s `δn,max`/`δt,max`), so in an
    /// implicit solve an unconverged Newton iterate that overshoots damages
    /// the point irreversibly and the converged state depends on the Newton
    /// path. Default, when the history exists (UD-028 default): the traction
    /// and the tangent use the converged history, `δ_max = max(δ_max
    /// previous, δ)` (bilinear: at least `δ₀`), as upstream
    /// `cohesive_exponential` does, and the contact branch is decided from
    /// the converged damage. Explicit dynamics evaluates
    /// the traction once per step after the history is saved and is
    /// unaffected.
    pub delta_max_per_iterate: bool,
    /// **UD-242**: the linear-family tangent treats a
    /// point as penetrating when `Δn < 0`, its traction when `Δn/δc < −tol`
    /// (`cohesive_linear`, `_bilinear`, `_friction`; `_fatigue`: `Δn <
    /// −tol`), so within round-off of closure the tangent adds the penalty
    /// stiffness the traction does not apply. Default: the tangent uses its
    /// traction's predicate.
    pub tangent_penetration_sign: bool,
    /// **UD-243**: on the penetration branch the
    /// linear-family tangent keeps the cohesive normal stiffness `n⊗n·t/δ`
    /// (`(n⊗n (1 − β²/κ) + β²/κ I)·t/δ`) although the traction drops the
    /// normal opening (only the penalty acts normally); without tangential
    /// opening the fictitious `δ = δc/1000` makes it ≈ `1000 σc/δc`.
    /// Default: `β²/κ (I − n⊗n)·t/δ`, the derivative of the traction.
    pub tangent_contact_normal_stiffness: bool,
    /// **UD-244**: the `cohesive_linear_uncoupled`
    /// tangent is not the derivative of its traction — the shear term `(I −
    /// n⊗n)·T/δt` lacks the traction's `β²/κ`, and on penetration the
    /// contact opening is added back to an opening the traction left whole
    /// (normal part doubled, the internal modified) and the shear
    /// derivative uses that full opening instead of the tangential one.
    /// Default: the derivative of the traction.
    pub uncoupled_tangent: bool,
    /// **UD-245**: every history is saved at the entry
    /// of a solve step only (`beforeSolveStep`), so the maximal openings a
    /// converged step reached are committed when the next step starts; a
    /// traction evaluated outside a solve step (the public
    /// `assemble_internal_forces`) by a law reading the converged maximum
    /// (`cohesive_exponential`; the UD-241 default of the linear family)
    /// forgets the last converged step, and the next step entry commits
    /// whatever that evaluation left. Default: those maxima are committed at
    /// the end of each converged step and left alone at step entry; an
    /// evaluation outside a step is a trial against the latest committed
    /// state.
    pub maxima_committed_at_step_entry: bool,
    /// **UD-248**: the `cohesive_exponential` tangent
    /// is not the derivative of its traction — it ignores the maximal
    /// opening (`computeCoupledTangent(…, /*delta_max_new*/)`: below
    /// `δ_max` it returns the loading tangent `(A − δe⊗δe/(δ δc))·e^{1−δ/δc}
    /// σc/δc`, `A = β²I + (1−β²) n⊗n`, while the traction is the secant
    /// `A·Δ·e σc e^{−δ_max/δc}/δc`), and on the compression branch its
    /// coupled part keeps `A` although the coupled traction acts on the
    /// tangential opening only (the derivative is `A (I − n⊗n)`: upstream
    /// adds a normal stiffness `n⊗n·e^{1−δ/δc} σc/δc` the traction does not
    /// have). Default: the derivative of the traction on every branch
    /// (loading, unloading below `δ_max`, compression; loading at `δ =
    /// δ_max`; inside the `|δ| < tol` guard the limit of the branch outside
    /// it, as upstream's `δ := tol`). Upstream cannot run the law (UD-028),
    /// so only the port's implicit exponential path is affected.
    pub exponential_tangent: bool,
    /// **UD-808** (model level): with `cohesive_zones`,
    /// upstream `limitCheckFacets` reads `physical_names(element)` for both
    /// neighbours of every facet (`cohesive_element_inserter.cc:166-189`);
    /// a neighbour whose element type has no `physical_names` array — every
    /// cohesive element — throws `No element of type (not_ghost:_cohesive_2d_4)
    /// in this ElementTypeMapArray<…> class ("mesh:mesh_data:physical_names")`,
    /// so `updateAutomaticInsertion` (and a repeated `insertIntrinsicElements`)
    /// fails once a cohesive element exists. Default: such a neighbour is
    /// outside every zone (a facet next to a cohesive element is already
    /// excluded by the cohesive-neighbour rule, so the flags are those
    /// upstream would compute without the exception). The switch
    /// reproduces upstream's error (`Error::Mesh` with upstream's message).
    pub zones_unnamed_neighbour_error: bool,
    /// **UD-249**: `cohesive_linear_friction` updates the
    /// residual sliding with the magnitude of the friction stress, `s =
    /// ‖Δt‖ − |τ|/k_f` (`material_cohesive_linear_friction.cc:93-97`),
    /// although `τ` is negative on reverse sliding: in reverse stick the
    /// spring's rest point jumps to `2‖Δt‖ − s_prev` and the friction force
    /// chatters below `τ_max`. Default: the signed `τ`, `s = ‖Δt‖ − τ/k_f`,
    /// which is the play operator `s = clamp(s_prev, ‖Δt‖ − τ_max/k_f,
    /// ‖Δt‖ + τ_max/k_f)` (the exact discrete solution of rate-independent
    /// penalty friction).
    pub friction_residual_sliding_abs: bool,
    /// **UD-247**: upstream's implicit
    /// `cohesive_linear_friction`: the contact decision of the traction reads
    /// the damage the previous trial left (the tangent the damage of the
    /// present trial), and the tangent is the linear tangent plus
    /// `k_f (I − n⊗n)` while sticking, the stick decision made with `τ_max`
    /// of the previous step's normal opening — not the derivative of the
    /// traction (`τ_max` depends on `Δn`; the direction of the friction force
    /// depends on `Δt` in 3-D). Default (implicit runs with the `δ_max`
    /// history): contact decided from the converged damage (traction and
    /// tangent, trial-idempotent), and the tangent is the derivative of the
    /// traction (stick `k_f t̂⊗t̂ + τ/‖Δt‖·(I − n⊗n − t̂⊗t̂)`, slip
    /// `τ/‖Δt‖·(I − n⊗n − t̂⊗t̂) − sign(τ) μ·penalty·t̂⊗n`).
    pub friction_upstream_implicit: bool,
    /// **UD-1160** (D-099 (1)): below the maximal opening
    /// the linear-family tangent (`cohesive_linear`, `cohesive_bilinear`,
    /// `cohesive_linear_fatigue`, `cohesive_linear_friction`) is the
    /// unloading secant `σc (1 − δ_max/δc)/δ_max·δ` without a clamp
    /// (`material_cohesive_linear_inline_impl.hh:228-231`): once a point has
    /// failed past δc (`δ_max > δc`) it is a negative stiffness, while the
    /// traction is zero (`damage = 1`). Default: the cohesive part of the
    /// tangent is the derivative of that zero traction, `σc·max(1 −
    /// δ_max/δc, 0)/δ_max·δ` (upstream's secant, bit for bit, while `δ_max
    /// ≤ δc`). The switch reproduces upstream's negative secant.
    pub tangent_failed_secant: bool,
    /// **UD-1280** (D-105, model level): when insertion
    /// doubles a node, upstream's `onNodesAdded`
    /// (`solid_mechanics_model_cohesive.cc:504-548`) gives the new copy a
    /// zero external force and the original copy keeps the whole load of
    /// every boundary element it had, although some of them now belong to
    /// the new copy; which copy is the original follows from the doubled
    /// facet's orientation, which on a distributed mesh depends on the
    /// partition, so a distributed run can apply the accumulated load to
    /// another lip of the crack than the serial run, and the serial run's
    /// placement depends on the element numbering. Default: the solid
    /// model's record of its Neumann loads per boundary element
    /// (`akantu_model::neumann_store`) moves each element's contribution to
    /// the copy the element now uses; a nodal force written into the
    /// external force by another path is taken as added and stays with the
    /// original copy, a node set to zero forgets its records (D-108).
    /// Rescaling or replacing Neumann loads by writing the external force
    /// is not supported by the default: zero it and apply the loads again,
    /// or set this switch. The switch keeps upstream's placement (nothing
    /// moves).
    pub doubled_node_keeps_external_force: bool,
}

impl CohesiveUpstreamCompat {
    /// Every upstream behaviour (differential replays of oracle cases).
    #[must_use]
    pub fn upstream() -> Self {
        Self {
            sigma_c_zipped_per_quad: true,
            delta_max_without_history: true,
            tangent_rotation_2d_in_3d: true,
            skip_material_index_0: true,
            bilinear_mesh_indices: true,
            tangent_base_sigma_c: true,
            intrinsic_linear_zero_strength: true,
            delta_max_per_iterate: true,
            tangent_penetration_sign: true,
            tangent_contact_normal_stiffness: true,
            uncoupled_tangent: true,
            maxima_committed_at_step_entry: true,
            exponential_tangent: true,
            zones_unnamed_neighbour_error: true,
            friction_residual_sliding_abs: true,
            friction_upstream_implicit: true,
            tangent_failed_secant: true,
            doubled_node_keeps_external_force: true,
        }
    }

    /// Field-wise OR of two sets.
    #[must_use]
    pub fn merged(self, other: Self) -> Self {
        Self {
            sigma_c_zipped_per_quad: self.sigma_c_zipped_per_quad | other.sigma_c_zipped_per_quad,
            delta_max_without_history: self.delta_max_without_history
                | other.delta_max_without_history,
            tangent_rotation_2d_in_3d: self.tangent_rotation_2d_in_3d
                | other.tangent_rotation_2d_in_3d,
            skip_material_index_0: self.skip_material_index_0 | other.skip_material_index_0,
            bilinear_mesh_indices: self.bilinear_mesh_indices | other.bilinear_mesh_indices,
            tangent_base_sigma_c: self.tangent_base_sigma_c | other.tangent_base_sigma_c,
            intrinsic_linear_zero_strength: self.intrinsic_linear_zero_strength
                | other.intrinsic_linear_zero_strength,
            delta_max_per_iterate: self.delta_max_per_iterate | other.delta_max_per_iterate,
            tangent_penetration_sign: self.tangent_penetration_sign
                | other.tangent_penetration_sign,
            tangent_contact_normal_stiffness: self.tangent_contact_normal_stiffness
                | other.tangent_contact_normal_stiffness,
            uncoupled_tangent: self.uncoupled_tangent | other.uncoupled_tangent,
            maxima_committed_at_step_entry: self.maxima_committed_at_step_entry
                | other.maxima_committed_at_step_entry,
            exponential_tangent: self.exponential_tangent | other.exponential_tangent,
            zones_unnamed_neighbour_error: self.zones_unnamed_neighbour_error
                | other.zones_unnamed_neighbour_error,
            friction_residual_sliding_abs: self.friction_residual_sliding_abs
                | other.friction_residual_sliding_abs,
            friction_upstream_implicit: self.friction_upstream_implicit
                | other.friction_upstream_implicit,
            tangent_failed_secant: self.tangent_failed_secant | other.tangent_failed_secant,
            doubled_node_keeps_external_force: self.doubled_node_keeps_external_force
                | other.doubled_node_keeps_external_force,
        }
    }
}

/// Number of quadrature points of a cohesive type in `CohesiveFEEngine`
/// (default order: the type's polynomial degree).
pub fn nb_quad_cohesive(ty: ElementType) -> Result<usize> {
    akantu_fe::quadrature::nb_quadrature_points(ty, ty.polynomial_degree())
}

/// Number of quadrature points of a facet type in `FacetsFEEngine`
/// (upstream `FacetsCohesiveIntegrationOrderFunctor`: the polynomial degree
/// of the facet's cohesive type, else the facet's own).
pub fn nb_quad_facet(ty: ElementType) -> Result<usize> {
    let order = ty
        .cohesive_type()
        .map_or(ty.polynomial_degree(), ElementType::polynomial_degree);
    akantu_fe::quadrature::nb_quadrature_points(ty, order)
}

/// Element domain of an internal field.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Domain {
    /// Regular kind (the base `Material` internals): always empty here.
    Regular,
    /// Cohesive elements of the element filter.
    Cohesive,
    /// Facets of the facet filter.
    Facet,
}

/// The common state of every cohesive law (upstream `MaterialCohesive`
/// members).
#[derive(Clone, Debug)]
pub struct CohesiveCore {
    /// Spatial dimension.
    pub dim: usize,
    /// Extrinsic model (upstream `model->getIsExtrinsic()`).
    pub is_extrinsic: bool,
    /// Facets the law may be inserted on (extrinsic).
    pub facet_filter: ElementTypeMapArray<Idx>,
    /// Reversible energy density.
    pub reversible_energy: InternalField<Real>,
    /// Total (trapezoidal) work density.
    pub total_energy: InternalField<Real>,
    /// Opening (history).
    pub opening: InternalField<Real>,
    /// Tractions (history).
    pub tractions: InternalField<Real>,
    /// Contact (penalty) tractions.
    pub contact_tractions: InternalField<Real>,
    /// Penetrations.
    pub contact_opening: InternalField<Real>,
    /// Maximal effective opening (upstream internal `"delta max"`).
    pub delta_max: InternalField<Real>,
    /// Damage.
    pub damage: InternalField<Real>,
    /// Facet strength (random facet field, the `sigma_c` parameter).
    pub sigma_c: InternalField<Real>,
    /// Normals at the cohesive quadrature points (current positions).
    pub normals: InternalField<Real>,
    /// `delta_c` parameter.
    pub delta_c: Real,
    /// The parser form of `sigma_c` (for `get`).
    pub sigma_c_param: akantu_parser::param::RandomParameter,
    /// The law reads the previous `δ_max`.
    pub use_previous_delta_max: bool,
    /// The law reads the previous opening.
    pub use_previous_opening: bool,
    /// Upstream-defect switches.
    pub compat: CohesiveUpstreamCompat,
    /// The model's default solver is explicit (upstream
    /// `model->isDefaultSolverExplicit()`, read by the friction law).
    pub default_solver_explicit: bool,
}

impl CohesiveCore {
    pub(crate) fn new(dim: usize) -> Result<Self> {
        let mut opening = InternalField::new("opening", dim, 0.0)?;
        opening.initialize_history();
        let mut tractions = InternalField::new("tractions", dim, 0.0)?;
        tractions.initialize_history();
        let param = akantu_parser::param::RandomParameter {
            base_value: 0.0,
            distribution: None,
        };
        Ok(Self {
            dim,
            is_extrinsic: false,
            facet_filter: ElementTypeMapArray::new("facet_filter"),
            reversible_energy: InternalField::new("reversible_energy", 1, 0.0)?,
            total_energy: InternalField::new("total_energy", 1, 0.0)?,
            opening,
            tractions,
            contact_tractions: InternalField::new("contact_tractions", dim, 0.0)?,
            contact_opening: InternalField::new("contact_opening", dim, 0.0)?,
            delta_max: InternalField::new("delta max", 1, 0.0)?,
            damage: InternalField::new("damage", 1, 0.0)?,
            sigma_c: InternalField::new_random("sigma_c", 1, param.to_core())?,
            normals: InternalField::new("normal", dim, 0.0)?,
            delta_c: 0.0,
            sigma_c_param: param,
            use_previous_delta_max: false,
            use_previous_opening: false,
            compat: CohesiveUpstreamCompat::default(),
            default_solver_explicit: true,
        })
    }

    /// The base value of `sigma_c` (upstream `Real(sigma_c)`).
    #[must_use]
    pub fn sigma_c_value(&self) -> Real {
        self.sigma_c.parameter_value()
    }
}

/// A cohesive material (upstream `MaterialCohesive` and its laws).
#[derive(Clone, Debug)]
pub struct MaterialCohesive {
    base: MaterialBase,
    /// Common cohesive state.
    pub core: CohesiveCore,
    /// The constitutive law.
    pub law: CohesiveLaw,
    /// Runtime parameters of a runtime law (empty for the built-in laws;
    /// [`crate::laws::foreign`]).
    pub runtime_params: akantu_model::params::DynamicParameters,
    /// Runtime internals of a runtime law, on the law's cohesive elements
    /// (empty for the built-in laws).
    pub runtime_internals: akantu_model::internal_field::DynamicInternals,
    /// Work arrays of the per-step kernels (normals, openings, residual),
    /// kept between steps (performance; each is emptied before use, so
    /// the kernels see what a fresh array gives them).
    scratch: ScratchCell,
}

/// The work arrays behind a lock (a law is shared between threads by the
/// runtime-law interface, so `Sync`); a clone starts with empty arrays.
#[derive(Debug, Default)]
struct ScratchCell(std::sync::Mutex<CohesiveScratch>);

impl Clone for ScratchCell {
    fn clone(&self) -> Self {
        Self::default()
    }
}

impl ScratchCell {
    fn get_mut(&mut self) -> &mut CohesiveScratch {
        // a poisoned lock holds work arrays only: they are emptied before use
        self.0
            .get_mut()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, CohesiveScratch> {
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

/// The work arrays of [`MaterialCohesive`]'s per-step kernels.
#[derive(Clone, Debug, Default)]
struct CohesiveScratch {
    /// Normals or openings at the quadrature points of a cohesive type.
    points: Array<Real>,
    /// `(t + t_contact) N` at the quadrature points.
    traction_shapes: Array<Real>,
    /// Its integral per element.
    partial: Array<Real>,
    /// The elemental residual (both sides).
    int_t_n: Array<Real>,
    /// The node-ordered scatter of the residual per cohesive block (the
    /// threaded assembly; checked against the connectivity before use).
    scatters: Vec<(
        ElementType,
        GhostType,
        Option<akantu_core::exec::NodeScatter>,
    )>,
}

/// `a` as `Array::try_zeros(len, nb_component)` would make it, in its
/// allocation.
fn reset_scratch(a: &mut Array<Real>, len: usize, nb_component: usize) -> Result<()> {
    if a.nb_component() != nb_component {
        *a = Array::new(nb_component);
    }
    a.clear();
    a.try_resize(len, 0.0)
}

/// Elements per part of the threaded per-element kernels of the cohesive
/// laws (normals, openings, residual).
const ELEMENTS_PER_PART: usize = 256;

/// Quadrature points per part of the threaded per-point loops.
const POINTS_PER_PART: usize = 512;

/// `kernel(filter, out)` (a per-element FE kernel: `nq` points of `nbc`
/// values per element of `filter`, each element computed from its own
/// data) into `out`, emptied first as a fresh array: on one thread one
/// call, as before; on several, one call per part of whole elements with
/// the part's sub-filter, each writing its own elements' points. The values
/// are the serial call's for every thread count.
fn per_element_parts(
    exec: &akantu_core::exec::Exec,
    filter: &[Idx],
    nq: usize,
    nbc: usize,
    out: &mut Array<Real>,
    kernel: impl Fn(&[Idx], &mut Array<Real>) -> Result<()> + Sync,
) -> Result<()> {
    if out.nb_component() != nbc {
        *out = Array::new(nbc);
    }
    out.clear();
    if exec.nb_parts(filter.len(), ELEMENTS_PER_PART) <= 1 || nq == 0 || nbc == 0 {
        return kernel(filter, out);
    }
    out.try_resize(filter.len() * nq, 0.0)?;
    exec.try_for_each(
        akantu_core::exec::Tuples::new(out.as_mut_slice(), nq * nbc),
        ELEMENTS_PER_PART,
        |first, mut part| {
            let n = part.as_mut_slice().len() / (nq * nbc);
            let mut local = Array::new(nbc);
            kernel(&filter[first..first + n], &mut local)?;
            if local.as_slice().len() != part.as_mut_slice().len() {
                return Err(Error::DimensionMismatch {
                    context: "cohesive per-element kernel: values of a part",
                    expected: part.as_mut_slice().len(),
                    got: local.as_slice().len(),
                });
            }
            part.as_mut_slice().copy_from_slice(local.as_slice());
            Ok(())
        },
    )
}

/// The element filter of `(ty, ghost)`, borrowed (empty if absent).
fn filter_slice(base: &MaterialBase, ty: ElementType, ghost: GhostType) -> &[Idx] {
    base.element_filter
        .get(ty, ghost)
        .map_or(&[][..], |a| a.as_slice())
}

/// Block `(ty, ghost)` of a field, created empty if missing (a guard of
/// the exportable block, D-047).
pub(crate) fn block_mut(
    f: &mut InternalField<Real>,
    ty: ElementType,
    ghost: GhostType,
) -> Result<ArrayMut<'_, Real>> {
    let nbc = f.nb_component();
    let d = *f.default_value();
    let v = f.values_mut();
    if !v.exists(ty, ghost) {
        v.alloc(0, nbc, ty, ghost, d)?;
    }
    v.get_mut(ty, ghost)
}

/// Read-only block, empty slice if missing (a guard, no copy).
///
/// # Panics
/// If the block is exported and no native section is active (protocol
/// violation, [`akantu_core::SharedArray::borrow`]).
#[track_caller]
pub(crate) fn block(
    f: &InternalField<Real>,
    ty: ElementType,
    ghost: GhostType,
) -> SliceRef<'_, Real> {
    f.values()
        .shared(ty, ghost)
        .map_or_else(|_| SliceRef::empty(), |a| a.borrow().into_slice())
}

/// The current block `(ty, ghost)` of a field (created empty if missing)
/// and, when `with_previous`, its previous values — at least `nq` of them
/// (**UD-241**: the converged maximal openings of the linear family).
pub(crate) fn block_and_previous_mut<'a>(
    f: &'a mut InternalField<Real>,
    ty: ElementType,
    ghost: GhostType,
    nq: usize,
    with_previous: bool,
    context: &'static str,
) -> Result<(ArrayMut<'a, Real>, Option<&'a [Real]>)> {
    block_mut(f, ty, ghost)?;
    if !with_previous || (nq == 0 && f.previous(ty, ghost).is_err()) {
        return Ok((block_mut(f, ty, ghost)?, None));
    }
    let (cur, prev) = f.current_and_previous_mut(ty, ghost)?;
    if prev.len() < nq {
        return Err(Error::DimensionMismatch {
            context,
            expected: nq,
            got: prev.len(),
        });
    }
    Ok((cur, Some(prev.as_slice())))
}

impl MaterialCohesive {
    /// New cohesive material of `law_type` (upstream constructors of
    /// `MaterialCohesive` and the law): element filter blocks for the
    /// cohesive types of `mesh`.
    ///
    /// # Errors
    /// Bad dimension, unknown law.
    pub fn new(spec: &LawSpec, law_type: &str, mesh: &Mesh) -> Result<Self> {
        let mut base = MaterialBase::new(spec, law_type)?;
        base.initialize_element_filter(mesh, ElementKind::Cohesive)?;
        let mut core = CohesiveCore::new(spec.spatial_dimension)?;
        let law = CohesiveLaw::new(law_type, &mut core)?;
        Ok(Self {
            base,
            core,
            law,
            runtime_params: akantu_model::params::DynamicParameters::new(),
            runtime_internals: akantu_model::internal_field::DynamicInternals::default(),
            scratch: ScratchCell::default(),
        })
    }

    /// A runtime cohesive law registered as `law_type` with behaviour
    /// `methods` (a Python `MaterialCohesive` subclass,
    /// [`crate::laws::foreign`]): upstream `MaterialCohesive`'s state and
    /// parameters, no law parameter or internal until the behaviour
    /// registers them.
    ///
    /// # Errors
    /// Bad dimension.
    pub fn new_foreign(
        spec: &LawSpec,
        law_type: &str,
        mesh: &Mesh,
        methods: Box<dyn crate::laws::foreign::ForeignCohesiveMethods>,
    ) -> Result<Self> {
        let mut base = MaterialBase::new(spec, law_type)?;
        base.initialize_element_filter(mesh, ElementKind::Cohesive)?;
        let core = CohesiveCore::new(spec.spatial_dimension)?;
        Ok(Self {
            base,
            core,
            law: CohesiveLaw::Foreign(crate::laws::foreign::ForeignCohesive::new(methods)),
            runtime_params: akantu_model::params::DynamicParameters::new(),
            runtime_internals: akantu_model::internal_field::DynamicInternals::default(),
            scratch: ScratchCell::default(),
        })
    }

    /// Upstream `MaterialCohesive::initMaterial` without the law's own part
    /// (`super().initMaterial()` of a runtime law): the history of
    /// `δ_max` and of the regular internals, every internal (built-in and
    /// runtime) resized to its domain, the law marked initialised.
    ///
    /// # Errors
    /// Resize errors.
    pub fn base_init(&mut self, ctx: &LawContext<'_>) -> Result<()> {
        let _ = ctx;
        if self.core.use_previous_delta_max && !self.core.compat.delta_max_without_history {
            self.core.delta_max.initialize_history();
        }
        self.base.init_history()?;
        self.resize_internals(&[])?;
        self.base.set_init();
        Ok(())
    }

    /// Mark the model extrinsic and initialise the facet filter blocks
    /// (upstream constructor, `model->getIsExtrinsic()`).
    ///
    /// # Errors
    /// A mesh without facet mesh.
    pub fn set_extrinsic(&mut self, mesh: &Mesh, extrinsic: bool) -> Result<()> {
        self.core.is_extrinsic = extrinsic;
        if extrinsic {
            let fm = mesh
                .mesh_facets()
                .ok_or_else(|| Error::Mesh("extrinsic cohesive law without facet mesh".into()))?;
            for ghost in GhostType::ALL {
                for ty in fm.element_types(TypeFilter {
                    spatial_dimension: Some(self.core.dim - 1),
                    ghost_type: ghost,
                    kind: Some(ElementKind::Regular),
                }) {
                    if !self.core.facet_filter.exists(ty, ghost) {
                        self.core.facet_filter.alloc(0, 1, ty, ghost, 0)?;
                    }
                }
            }
        }
        Ok(())
    }

    /// Upstream `addFacet(facet)`: append to the facet filter, return the
    /// law-local facet index. Facets are added before the law is
    /// initialised (the facet internals, e.g. `sigma_c`, are sized then).
    ///
    /// # Errors
    /// Index range; a law already initialised (upstream
    /// accepts the facet and its insertion check then reads past the
    /// strengths).
    pub fn add_facet(&mut self, facet: Element) -> Result<Idx> {
        if self.base.is_init() {
            return Err(Error::invalid(format!(
                "addFacet({facet}) on the initialised cohesive material {}: the facet internals \
                 would not cover it (add facets before init_full)",
                self.base.name
            )));
        }
        let f = &mut self.core.facet_filter;
        if !f.exists(facet.ty(), facet.ghost_type()) {
            f.alloc(0, 1, facet.ty(), facet.ghost_type(), 0)?;
        }
        let b = f.get_mut(facet.ty(), facet.ghost_type())?;
        let local = akantu_core::try_idx(b.len())?;
        b.try_push_tuple(&[facet.index()])?;
        Ok(local)
    }

    /// The element domain of an internal of this law.
    pub(crate) fn domain_of(&self, name: &str) -> Domain {
        match name {
            "stress" | "eigen_grad_u" | "grad_u" | "potential_energy" | "piola_kirchhoff_2"
            | "green_strain" => Domain::Regular,
            "sigma_c" => Domain::Facet,
            other => self.law.domain_of(other).unwrap_or(Domain::Cohesive),
        }
    }

    /// `(type, ghost, nb_quadrature_points)` of the cohesive blocks.
    fn cohesive_blocks(&self) -> Result<Vec<(ElementType, GhostType, usize)>> {
        let mut out = Vec::new();
        for (ty, g, f) in self.base.element_filter.as_map().iter_all() {
            if ty.kind() != ElementKind::Cohesive {
                continue;
            }
            out.push((ty, g, f.len() * nb_quad_cohesive(ty)?));
        }
        Ok(out)
    }

    /// `(type, ghost, nb_quadrature_points)` of the facet blocks.
    fn facet_blocks(&self) -> Result<Vec<(ElementType, GhostType, usize)>> {
        let mut out = Vec::new();
        for (ty, g, f) in self.core.facet_filter.as_map().iter_all() {
            out.push((ty, g, f.len() * nb_quad_facet(ty)?));
        }
        Ok(out)
    }

    /// Cohesive types of the filter with elements for `ghost`, enum order
    /// (upstream `getElementFilter().elementTypes(dim, ghost,
    /// _ek_cohesive)`).
    #[must_use]
    pub fn cohesive_types(&self, ghost: GhostType) -> Vec<ElementType> {
        self.base
            .element_filter
            .element_types(TypeFilter {
                spatial_dimension: Some(self.core.dim),
                ghost_type: ghost,
                kind: Some(ElementKind::Cohesive),
            })
            .collect()
    }

    /// Whether the filter holds cohesive elements of `ghost` (upstream:
    /// a type of `getElementFilter().elementTypes(dim, ghost,
    /// _ek_cohesive)` with `nb_element > 0`).
    #[must_use]
    pub fn has_elements(&self, ghost: GhostType) -> bool {
        self.cohesive_types(ghost).into_iter().any(|ty| {
            self.base
                .element_filter
                .get(ty, ghost)
                .is_ok_and(|a| !a.as_slice().is_empty())
        })
    }

    fn filter_of(&self, ty: ElementType, ghost: GhostType) -> Vec<Idx> {
        self.base
            .element_filter
            .get(ty, ghost)
            .map(|a| a.as_slice().to_vec())
            .unwrap_or_default()
    }

    /// Upstream `computeNormal(position, normals(type, ghost), type, ghost)`:
    /// mean-surface normals at the cohesive quadrature points.
    ///
    /// # Errors
    /// FE engine errors.
    pub fn compute_normal(
        &mut self,
        fe: &FeEngine,
        mesh: &Mesh,
        position: &Array<Real>,
        ty: ElementType,
        ghost: GhostType,
    ) -> Result<()> {
        let filter = filter_slice(&self.base, ty, ghost);
        let mut out = block_mut(&mut self.core.normals, ty, ghost)?;
        out.as_mut_slice().fill(0.0);
        let scratch = self.scratch.get_mut();
        let tmp = &mut scratch.points;
        let nq = fe.nb_integration_points(ty, ghost)?;
        per_element_parts(fe.exec(), filter, nq, self.core.dim, tmp, |sub, part| {
            fe.compute_cohesive_normals(
                mesh,
                position.view(),
                part,
                ty,
                ghost,
                CohesiveReduction::Mean,
                Some(sub),
            )
        })?;
        out.copy_from(tmp)
    }

    /// Upstream `computeOpening(displacement, opening(type, ghost), …)`.
    ///
    /// # Errors
    /// FE engine errors.
    pub fn compute_opening(
        &mut self,
        fe: &FeEngine,
        mesh: &Mesh,
        displacement: &Array<Real>,
        ty: ElementType,
        ghost: GhostType,
    ) -> Result<()> {
        let filter = filter_slice(&self.base, ty, ghost);
        let scratch = self.scratch.get_mut();
        let tmp = &mut scratch.points;
        let nq = fe.nb_integration_points(ty, ghost)?;
        per_element_parts(fe.exec(), filter, nq, self.core.dim, tmp, |sub, part| {
            fe.interpolate_cohesive(
                mesh,
                displacement.view(),
                part,
                ty,
                ghost,
                CohesiveReduction::Opening,
                Some(sub),
            )
        })?;
        block_mut(&mut self.core.opening, ty, ghost)?.copy_from(tmp)
    }

    /// Upstream `MaterialCohesive::computeTraction(ghost)`: for every
    /// non-empty cohesive type, normals on `position` (current positions),
    /// openings of `displacement`, then the law's tractions.
    ///
    /// # Errors
    /// FE engine or law errors.
    pub fn compute_traction(
        &mut self,
        fe: &FeEngine,
        mesh: &Mesh,
        position: &Array<Real>,
        displacement: &Array<Real>,
        ghost: GhostType,
    ) -> Result<()> {
        self.compute_traction_in(None, fe, mesh, position, displacement, ghost)
    }

    /// [`MaterialCohesive::compute_traction`] with the law context (needed
    /// by a runtime law, whose behaviour receives it).
    ///
    /// # Errors
    /// FE engine or law errors; a runtime law without `ctx`.
    pub fn compute_traction_in(
        &mut self,
        ctx: Option<&LawContext<'_>>,
        fe: &FeEngine,
        mesh: &Mesh,
        position: &Array<Real>,
        displacement: &Array<Real>,
        ghost: GhostType,
    ) -> Result<()> {
        for ty in self.cohesive_types(ghost) {
            if filter_slice(&self.base, ty, ghost).is_empty() {
                continue;
            }
            self.compute_normal(fe, mesh, position, ty, ghost)?;
            self.compute_opening(fe, mesh, displacement, ty, ghost)?;
            if self.is_foreign() {
                let ctx = ctx.ok_or_else(|| {
                    Error::invalid("the traction of a runtime cohesive law needs the law context")
                })?;
                self.with_foreign(|m, law| m.compute_traction(law, ctx, ty, ghost))?;
            } else {
                let exec: &akantu_core::exec::Exec = match ctx {
                    Some(c) => c.services.exec(),
                    None => akantu_core::exec::Exec::serial_ref(),
                };
                self.law.compute_traction(&mut self.core, ty, ghost, exec)?;
            }
        }
        Ok(())
    }

    /// The normals (on `position`) and openings (of `displacement`) of
    /// every non-empty cohesive type: the first part of
    /// [`MaterialCohesive::compute_traction`], for a runtime law whose
    /// traction callback must run after the caller released its guards on
    /// the model's exported arrays (a foreign callback is refused while
    /// native borrows are alive, D-024).
    ///
    /// # Errors
    /// FE engine errors.
    pub fn compute_kinematics(
        &mut self,
        fe: &FeEngine,
        mesh: &Mesh,
        position: &Array<Real>,
        displacement: &Array<Real>,
        ghost: GhostType,
    ) -> Result<()> {
        for ty in self.cohesive_types(ghost) {
            if filter_slice(&self.base, ty, ghost).is_empty() {
                continue;
            }
            self.compute_normal(fe, mesh, position, ty, ghost)?;
            self.compute_opening(fe, mesh, displacement, ty, ghost)?;
        }
        Ok(())
    }

    /// The second part for a runtime law: its behaviour's
    /// `computeTraction(type, ghost)` for every non-empty cohesive type
    /// (after [`MaterialCohesive::compute_kinematics`]).
    ///
    /// # Errors
    /// Not a runtime law; the behaviour's errors.
    pub fn compute_foreign_tractions(
        &mut self,
        ctx: &LawContext<'_>,
        ghost: GhostType,
    ) -> Result<()> {
        for ty in self.cohesive_types(ghost) {
            if filter_slice(&self.base, ty, ghost).is_empty() {
                continue;
            }
            self.with_foreign(|m, law| m.compute_traction(law, ctx, ty, ghost))?;
        }
        Ok(())
    }

    /// Upstream `assembleInternalForces(ghost)`: per cohesive type,
    /// `(t + t_contact) N` at each quadrature point, integrated, copied to
    /// both sides (`−` on the first), assembled into `f_int`.
    fn assemble_internal_forces_impl(
        &self,
        fems: &FeEngineRegistry,
        mesh: &Mesh,
        ghost: GhostType,
        mut f_int: ArrayViewMut<'_, Real>,
    ) -> Result<()> {
        let dim = self.core.dim;
        let exec = fems.exec();
        let mut scratch = self.scratch.lock();
        let CohesiveScratch {
            traction_shapes: cpy,
            partial,
            int_t_n,
            scatters,
            ..
        } = &mut *scratch;
        for ty in self.cohesive_types(ghost) {
            let filter = filter_slice(&self.base, ty, ghost);
            if filter.is_empty() {
                continue;
            }
            let fe = fems.get(COHESIVE_FE_ENGINE)?;
            let shapes = fe.shapes(ty, ghost)?;
            let nq = fe.nb_integration_points(ty, ghost)?;
            let s = shapes.nb_component();
            let nn = ty.nb_nodes_per_element();
            let tr = block(&self.core.tractions, ty, ghost);
            let ct = block(&self.core.contact_tractions, ty, ghost);
            let nb = filter.len();
            if tr.len() < nb * nq * dim || ct.len() < nb * nq * dim {
                return Err(Error::DimensionMismatch {
                    context: "MaterialCohesive::assembleInternalForces: tractions",
                    expected: nb * nq * dim,
                    got: tr.len().min(ct.len()),
                });
            }
            let sh = shapes.as_slice();
            // every value of the work arrays below is written before it is
            // read (as the fresh zeroed arrays they replace)
            reset_scratch(cpy, nb * nq, dim * s)?;
            let threaded = exec.nb_parts(nb, ELEMENTS_PER_PART) > 1
                && nn * dim == 2 * dim * s
                && filter.iter().all(|&e| {
                    akantu_core::try_to_usize(e).is_ok_and(|e| (e + 1) * nq * s <= sh.len())
                });
            if threaded {
                // per point `(t + t_c) N` on the threads (each point from
                // its own tractions and shapes), then per element the
                // residual of both sides, then the scatter in element order
                // per node (`NodeScatter`): the serial values and sums
                let (trs, cts): (&[Real], &[Real]) = (&tr, &ct);
                exec.for_each(
                    akantu_core::exec::Tuples::new(cpy.as_mut_slice(), dim * s),
                    ELEMENTS_PER_PART * nq,
                    |first, part| {
                        for (j, out) in part.into_slice().chunks_exact_mut(dim * s).enumerate() {
                            let p = first + j;
                            let e = akantu_core::try_to_usize(filter[p / nq]).unwrap_or(0);
                            let q = p % nq;
                            let n = &sh[(e * nq + q) * s..(e * nq + q + 1) * s];
                            let t = &trs[p * dim..(p + 1) * dim];
                            let tc = &cts[p * dim..(p + 1) * dim];
                            for (j, nj) in n.iter().enumerate() {
                                for i in 0..dim {
                                    out[i + dim * j] = (t[i] + tc[i]) * nj;
                                }
                            }
                        }
                    },
                );
                if partial.nb_component() != dim * s {
                    *partial = Array::new(dim * s);
                }
                partial.clear();
                fe.integrate(cpy.view(), partial, dim * s, ty, ghost, Some(filter))?;
                let half = dim * s;
                reset_scratch(int_t_n, nb, 2 * half)?;
                let src = partial.as_slice();
                exec.for_each(
                    akantu_core::exec::Tuples::new(int_t_n.as_mut_slice(), 2 * half),
                    ELEMENTS_PER_PART,
                    |first, part| {
                        for (j, to) in part.into_slice().chunks_exact_mut(2 * half).enumerate() {
                            let el = first + j;
                            let from = &src[el * half..(el + 1) * half];
                            to[..half].copy_from_slice(from);
                            to[half..2 * half].copy_from_slice(from);
                            for v in &mut to[..half] {
                                *v *= -1.;
                            }
                        }
                    },
                );
                let conn = mesh.connectivity(ty, ghost)?;
                let nb_nodes = f_int.len();
                let k = match scatters
                    .iter()
                    .position(|(t, g, _)| *t == ty && *g == ghost)
                {
                    Some(k) => k,
                    None => {
                        scatters.push((ty, ghost, None));
                        scatters.len() - 1
                    }
                };
                let fresh = !scatters[k].2.as_ref().is_some_and(|sc| {
                    sc.matches(exec, conn.as_slice(), nn, Some(filter), nb_nodes)
                });
                if fresh {
                    scatters[k].2 = Some(akantu_core::exec::NodeScatter::new(
                        conn.as_slice(),
                        nn,
                        Some(filter),
                        nb_nodes,
                    )?);
                }
                if let Some(sc) = &scatters[k].2 {
                    sc.scatter_add(exec, int_t_n.as_slice(), dim, 1.0, f_int.as_mut_slice())?;
                }
                continue;
            }
            {
                let c = cpy.as_mut_slice();
                let mut p = 0;
                for &e in filter {
                    let e = akantu_core::try_to_usize(e)?;
                    for q in 0..nq {
                        let n = &sh[(e * nq + q) * s..(e * nq + q + 1) * s];
                        let t = &tr[p * dim..(p + 1) * dim];
                        let tc = &ct[p * dim..(p + 1) * dim];
                        let out = &mut c[p * dim * s..(p + 1) * dim * s];
                        for (j, nj) in n.iter().enumerate() {
                            for i in 0..dim {
                                out[i + dim * j] = (t[i] + tc[i]) * nj;
                            }
                        }
                        p += 1;
                    }
                }
            }
            if partial.nb_component() != dim * s {
                *partial = Array::new(dim * s);
            }
            partial.clear();
            fe.integrate(cpy.view(), partial, dim * s, ty, ghost, Some(filter))?;
            let half = dim * s;
            reset_scratch(int_t_n, nb, 2 * half)?;
            {
                let o = int_t_n.as_mut_slice();
                let src = partial.as_slice();
                let stride = nn * dim;
                for el in 0..nb {
                    let from = &src[el * half..(el + 1) * half];
                    let to = &mut o[el * stride..el * stride + 2 * half];
                    to[..half].copy_from_slice(from);
                    to[half..2 * half].copy_from_slice(from);
                    for v in &mut to[..half] {
                        *v *= -1.;
                    }
                }
            }
            DofManager::assemble_elemental_array_local_view(
                mesh,
                int_t_n.view(),
                f_int.reborrow(),
                ty,
                ghost,
                1.0,
                Some(filter),
            )?;
        }
        Ok(())
    }

    /// Upstream `assembleStiffnessMatrix(ghost)` up to the DOF-manager
    /// assembly: normals on the current positions, the law's tangent `D`,
    /// `(D NA)ᵀ (NA)` with `A = [I, −I]`, integrated; `_unsymmetric` blocks.
    // the return type is the `Material::elemental_stiffness_custom` contract
    // of akantu-solid (spelled out there too)
    #[allow(clippy::type_complexity)]
    fn elemental_stiffness_impl(
        &mut self,
        ctx: &LawContext<'_>,
        ghost: GhostType,
    ) -> Result<Vec<(ElementType, Vec<Idx>, Array<Real>, MatrixType)>> {
        let dim = self.core.dim;
        let mut out = Vec::new();
        let services: &dyn ModelServices = ctx.services;
        let mesh = services.mesh();
        for ty in self.cohesive_types(ghost) {
            let filter = self.filter_of(ty, ghost);
            if filter.is_empty() {
                continue;
            }
            let fe = services.fe_engine(COHESIVE_FE_ENGINE)?;
            let nq = fe.nb_integration_points(ty, ghost)?;
            let nn = ty.nb_nodes_per_element();
            let shapes = fe.shapes(ty, ghost)?;
            let s = shapes.nb_component();
            let nb = filter.len();
            let rows = dim * s;
            let cols = dim * nn;
            // A: (dim·S) × (dim·nn), column-major
            let mut a = vec![0.0; rows * cols];
            for i in 0..rows {
                a[i + i * rows] = 1.;
                a[i + (i + rows) * rows] = -1.;
            }
            {
                // the guard ends before a runtime law's tangent callback
                let position = services.nodal_field("current_position")?;
                self.compute_normal(fe, mesh, &position, ty, ghost)?;
            }
            let mut tangent = vec![0.0; nb * nq * dim * dim];
            if self.is_foreign() {
                self.with_foreign(|m, law| {
                    m.compute_tangent_traction(law, ctx, ty, ghost, &mut tangent)
                })?;
            } else {
                self.law
                    .compute_tangent_traction(&mut self.core, ty, ghost, &mut tangent)?;
            }
            let size = cols * cols;
            let mut at = Array::try_zeros(nb * nq, size)?;
            let sh = shapes.as_slice();
            let mut nmat = vec![0.0; dim * rows];
            let mut na = vec![0.0; dim * cols];
            let mut dna = vec![0.0; dim * cols];
            {
                let o = at.as_mut_slice();
                let mut p = 0;
                for &e in &filter {
                    let e = akantu_core::try_to_usize(e)?;
                    for q in 0..nq {
                        let n = &sh[(e * nq + q) * s..(e * nq + q + 1) * s];
                        nmat.fill(0.0);
                        for i in 0..dim {
                            for (k, nk) in n.iter().enumerate() {
                                nmat[i + dim * (i + dim * k)] = *nk;
                            }
                        }
                        // NA = N * A (dynamic Matrix<Real> product)
                        product_slices(
                            Dims::dynamic(dim, rows),
                            Dims::dynamic(rows, cols),
                            &nmat,
                            &a,
                            &mut na,
                        )?;
                        let d = &tangent[p * dim * dim..(p + 1) * dim * dim];
                        // D * NA, then (D NA)ᵀ * NA
                        product_slices(
                            Dims::dynamic(dim, dim),
                            Dims::dynamic(dim, cols),
                            d,
                            &na,
                            &mut dna,
                        )?;
                        product_slices(
                            Dims::dynamic(dim, cols).t(),
                            Dims::dynamic(dim, cols),
                            &dna,
                            &na,
                            &mut o[p * size..(p + 1) * size],
                        )?;
                        p += 1;
                    }
                }
            }
            let mut k_e = Array::new(size);
            fe.integrate(at.view(), &mut k_e, size, ty, ghost, Some(&filter))?;
            out.push((ty, filter, k_e, MatrixType::Unsymmetric));
        }
        Ok(out)
    }

    /// Upstream `updateEnergies(type)` (cohesive types only): trapezoidal
    /// work increment `½ (δ − δ_old)·(t_old + t)` and reversible energy
    /// `½ t·δ` per quadrature point.
    fn update_energies_impl(
        &mut self,
        ty: ElementType,
        exec: &akantu_core::exec::Exec,
    ) -> Result<()> {
        if ty.kind() != ElementKind::Cohesive {
            return Ok(());
        }
        let dim = self.core.dim;
        let g = GhostType::NotGhost;
        // read through guards (no copies of the four arrays; performance)
        let tr = block(&self.core.tractions, ty, g);
        let tr_old_guard = self.core.tractions.previous(ty, g)?;
        let tr_old = tr_old_guard.as_slice();
        let op = block(&self.core.opening, ty, g);
        let op_old_guard = self.core.opening.previous(ty, g)?;
        let op_old = op_old_guard.as_slice();
        let nq = tr.len() / dim.max(1);
        let mut erev = block_mut(&mut self.core.reversible_energy, ty, g)?;
        if erev.len() < nq
            || tr_old.len() < tr.len()
            || op.len() < tr.len()
            || op_old.len() < tr.len()
        {
            return Err(Error::DimensionMismatch {
                context: "MaterialCohesive::updateEnergies",
                expected: nq,
                got: erev.len(),
            });
        }
        let mut etot = block_mut(&mut self.core.total_energy, ty, g)?;
        let erev = &mut erev.as_mut_slice()[..nq];
        let etot = etot.as_mut_slice();
        if etot.len() >= nq && exec.nb_parts(nq, POINTS_PER_PART) > 1 {
            // per point on the threads (each point from its own values)
            let (tr, op): (&[Real], &[Real]) = (&tr, &op);
            exec.for_each(
                (
                    akantu_core::exec::Tuples::new(erev, 1),
                    akantu_core::exec::Tuples::new(&mut etot[..nq], 1),
                ),
                POINTS_PER_PART,
                |first, (erev, etot)| {
                    let (erev, etot) = (erev.into_slice(), etot.into_slice());
                    let mut b = [0.0; 3];
                    let mut h = [0.0; 3];
                    for j in 0..erev.len() {
                        let q = first + j;
                        for i in 0..dim {
                            b[i] = op[q * dim + i] - op_old[q * dim + i];
                            h[i] = tr_old[q * dim + i] + tr[q * dim + i];
                        }
                        let inc = 0.5 * crate::vecops::dot_seq(&b, &h, dim);
                        erev[j] = 0.5
                            * crate::vecops::dot_seq(
                                &tr[q * dim..(q + 1) * dim],
                                &op[q * dim..(q + 1) * dim],
                                dim,
                            );
                        etot[j] += inc;
                    }
                },
            );
            return Ok(());
        }
        let mut b = [0.0; 3];
        let mut h = [0.0; 3];
        for q in 0..nq {
            for i in 0..dim {
                b[i] = op[q * dim + i] - op_old[q * dim + i];
                h[i] = tr_old[q * dim + i] + tr[q * dim + i];
            }
            // `Vector<Real> b, h` (run-time size): sequential dot
            let inc = 0.5 * crate::vecops::dot_seq(&b, &h, dim);
            // `traction_it->dot(*opening_it)`: run-time views
            erev[q] = 0.5
                * crate::vecops::dot_seq(
                    &tr[q * dim..(q + 1) * dim],
                    &op[q * dim..(q + 1) * dim],
                    dim,
                );
            if let Some(e) = etot.get_mut(q) {
                *e += inc;
            }
        }
        Ok(())
    }

    fn integrate_cohesive(&self, fe: &FeEngine, values: &[Real], ty: ElementType) -> Result<Real> {
        let filter = filter_slice(&self.base, ty, GhostType::NotGhost);
        fe.integrate_scalar(
            akantu_core::array::ArrayView::new(values, 1)?,
            ty,
            GhostType::NotGhost,
            Some(filter),
        )
    }

    /// Upstream `getReversibleEnergy()`.
    ///
    /// # Errors
    /// FE engine errors.
    pub fn reversible_energy(&self, fe: &FeEngine) -> Result<Real> {
        let mut e = 0.0;
        for ty in self.cohesive_types(GhostType::NotGhost) {
            e += self.integrate_cohesive(
                fe,
                &block(&self.core.reversible_energy, ty, GhostType::NotGhost),
                ty,
            )?;
        }
        Ok(e)
    }

    /// Upstream `getDissipatedEnergy()`: `∫ (total − reversible)`.
    ///
    /// # Errors
    /// FE engine errors.
    pub fn dissipated_energy(&self, fe: &FeEngine) -> Result<Real> {
        let mut e = 0.0;
        for ty in self.cohesive_types(GhostType::NotGhost) {
            let tot = block(&self.core.total_energy, ty, GhostType::NotGhost);
            let rev = block(&self.core.reversible_energy, ty, GhostType::NotGhost);
            let d: Vec<Real> = tot.iter().zip(&rev).map(|(t, r)| t - r).collect();
            e += self.integrate_cohesive(fe, &d, ty)?;
        }
        Ok(e)
    }

    /// Upstream `getContactEnergy()`: `∫ ½ t_c·δ_c`.
    ///
    /// # Errors
    /// FE engine errors.
    pub fn contact_energy(&self, fe: &FeEngine) -> Result<Real> {
        let dim = self.core.dim;
        let mut e = 0.0;
        for ty in self.cohesive_types(GhostType::NotGhost) {
            let ct = block(&self.core.contact_tractions, ty, GhostType::NotGhost);
            let co = block(&self.core.contact_opening, ty, GhostType::NotGhost);
            let n = ct.len() / dim.max(1);
            let c: Vec<Real> = (0..n)
                .map(|q| {
                    0.5 * crate::vecops::dot_seq(
                        &ct[q * dim..(q + 1) * dim],
                        &co[q * dim..(q + 1) * dim],
                        dim,
                    )
                })
                .collect();
            e += self.integrate_cohesive(fe, &c, ty)?;
        }
        Ok(e)
    }

    /// Visit every internal (name, field, domain).
    fn fields(&self) -> Vec<(&'static str, &InternalField<Real>)> {
        let c = &self.core;
        let mut v: Vec<(&'static str, &InternalField<Real>)> = self.base.internals();
        v.extend([
            ("reversible_energy", &c.reversible_energy),
            ("total_energy", &c.total_energy),
            ("opening", &c.opening),
            ("tractions", &c.tractions),
            ("contact_tractions", &c.contact_tractions),
            ("contact_opening", &c.contact_opening),
            ("delta max", &c.delta_max),
            ("damage", &c.damage),
            ("sigma_c", &c.sigma_c),
            ("normal", &c.normals),
        ]);
        v.extend(self.law.fields());
        v
    }

    fn fields_mut(&mut self) -> Vec<(&'static str, &mut InternalField<Real>)> {
        let Self {
            base, core: c, law, ..
        } = self;
        let names = ["stress", "eigen_grad_u", "grad_u", "potential_energy"];
        let mut v: Vec<(&'static str, &mut InternalField<Real>)> = Vec::new();
        let mut base_fields = base.internals_mut().into_iter();
        for n in names {
            if let Some(f) = base_fields.next() {
                v.push((n, f));
            }
        }
        for f in base_fields {
            let n = if f.name() == "piola_kirchhoff_2" {
                "piola_kirchhoff_2"
            } else {
                "green_strain"
            };
            v.push((n, f));
        }
        v.extend([
            ("reversible_energy", &mut c.reversible_energy),
            ("total_energy", &mut c.total_energy),
            ("opening", &mut c.opening),
            ("tractions", &mut c.tractions),
            ("contact_tractions", &mut c.contact_tractions),
            ("contact_opening", &mut c.contact_opening),
            ("delta max", &mut c.delta_max),
            ("damage", &mut c.damage),
            ("sigma_c", &mut c.sigma_c),
            ("normal", &mut c.normals),
        ]);
        v.extend(law.fields_mut());
        v
    }

    /// Internal by name (upstream `getInternal<Real>(name)`).
    ///
    /// # Errors
    /// Unknown internal.
    pub fn internal(&self, name: &str) -> Result<&InternalField<Real>> {
        self.fields()
            .into_iter()
            .find(|(n, _)| *n == name)
            .map(|(_, f)| f)
            .ok_or_else(|| Error::not_found("internal", name))
    }

    /// Block of an internal (upstream `getArray<Real>(name, type, ghost)`).
    ///
    /// # Errors
    /// Unknown internal or block.
    pub fn array(
        &self,
        name: &str,
        ty: ElementType,
        ghost: GhostType,
    ) -> Result<ArrayRef<'_, Real>> {
        self.internal(name)?.get(ty, ghost)
    }
}

impl ConstitutiveLaw for MaterialCohesive {
    fn name(&self) -> &str {
        &self.base.name
    }

    fn law_id(&self) -> &str {
        &self.base.id
    }

    fn law_type(&self) -> &str {
        &self.base.law_type
    }

    fn element_filter(&self) -> &ElementTypeMapArray<Idx> {
        &self.base.element_filter
    }

    fn element_filter_mut(&mut self) -> &mut ElementTypeMapArray<Idx> {
        &mut self.base.element_filter
    }

    fn for_each_internal(&self, f: &mut dyn FnMut(&str, InternalFieldRef<'_>)) {
        for (n, field) in self.fields() {
            f(n, InternalFieldRef::Real(field));
        }
        for (n, field) in self.law.bool_fields() {
            f(n, InternalFieldRef::Bool(field));
        }
        for (n, field) in self.law.int_fields() {
            f(n, InternalFieldRef::Int(field));
        }
        for (n, field) in self.runtime_internals.iter() {
            match field {
                AnyInternalField::Real(x) => f(n, InternalFieldRef::Real(x)),
                AnyInternalField::Int(x) => f(n, InternalFieldRef::Int(x)),
                AnyInternalField::UInt(x) => f(n, InternalFieldRef::UInt(x)),
                AnyInternalField::Bool(x) => f(n, InternalFieldRef::Bool(x)),
            }
        }
    }

    fn for_each_internal_mut(
        &mut self,
        f: &mut dyn FnMut(&str, InternalFieldMut<'_>) -> Result<()>,
    ) -> Result<()> {
        for (n, field) in self.fields_mut() {
            f(n, InternalFieldMut::Real(field))?;
        }
        for (n, field) in self.law.bool_fields_mut() {
            f(n, InternalFieldMut::Bool(field))?;
        }
        for (n, field) in self.law.int_fields_mut() {
            f(n, InternalFieldMut::Int(field))?;
        }
        for (n, field) in self.runtime_internals.iter_mut() {
            match field {
                AnyInternalField::Real(x) => f(n, InternalFieldMut::Real(x))?,
                AnyInternalField::Int(x) => f(n, InternalFieldMut::Int(x))?,
                AnyInternalField::UInt(x) => f(n, InternalFieldMut::UInt(x))?,
                AnyInternalField::Bool(x) => f(n, InternalFieldMut::Bool(x))?,
            }
        }
        Ok(())
    }

    /// Upstream `resizeInternals()`: every internal in register-id order
    /// (byte-wise names) to its own domain's blocks — the order in which
    /// the random fields (`sigma_c` on facets, `sigma_c_eff` on cohesive
    /// elements) draw. The model's blocks (regular engine) are ignored.
    fn resize_internals(&mut self, _blocks: &[(ElementType, GhostType, usize)]) -> Result<()> {
        let cohesive = self.cohesive_blocks()?;
        let facets = self.facet_blocks()?;
        let mut names: Vec<&'static str> = self.fields().into_iter().map(|(n, _)| n).collect();
        names.extend(self.law.bool_fields().into_iter().map(|(n, _)| n));
        names.extend(self.law.int_fields().into_iter().map(|(n, _)| n));
        names.sort_unstable();
        let domains: Vec<Domain> = names.iter().map(|n| self.domain_of(n)).collect();
        for (name, dom) in names.into_iter().zip(domains) {
            let blocks: &[(ElementType, GhostType, usize)] = match dom {
                Domain::Regular => continue,
                Domain::Cohesive => &cohesive,
                Domain::Facet => &facets,
            };
            for (n, field) in self.fields_mut() {
                if n == name {
                    field.resize_blocks(blocks)?;
                    break;
                }
            }
            for (n, field) in self.law.bool_fields_mut() {
                if n == name {
                    field.resize_blocks(blocks)?;
                }
            }
            for (n, field) in self.law.int_fields_mut() {
                if n == name {
                    field.resize_blocks(blocks)?;
                }
            }
        }
        // the runtime internals of a runtime law live on its cohesive
        // elements (registered in name order after the built-in ones)
        let mut runtime: Vec<String> = self
            .runtime_internals
            .iter()
            .map(|(n, _)| n.to_owned())
            .collect();
        runtime.sort();
        for name in runtime {
            match self.runtime_internals.get_mut(&name)? {
                AnyInternalField::Real(f) => f.resize_blocks(&cohesive)?,
                AnyInternalField::Int(f) => f.resize_blocks(&cohesive)?,
                AnyInternalField::UInt(f) => f.resize_blocks(&cohesive)?,
                AnyInternalField::Bool(f) => f.resize_blocks(&cohesive)?,
            }
        }
        Ok(())
    }
}

impl akantu_parser::Parsable for MaterialCohesive {
    fn section_type(&self) -> akantu_parser::ParserType {
        akantu_parser::ParserType::Material
    }
    fn parsable_id(&self) -> &str {
        &self.base.id
    }
}

impl akantu_parser::ParameterRegistry for MaterialCohesive {
    // the built-in parameters first, then a runtime law's runtime ones
    fn get_param(&self, name: &str) -> Result<ParamValue> {
        if self.runtime_params.contains(name)
            && akantu_solid::material::params_info(self, name).is_none()
        {
            return self.runtime_params.get(name);
        }
        akantu_solid::material::params_get(self, name)
    }
    fn set_param(&mut self, name: &str, value: ParamValue) -> Result<()> {
        if self.runtime_params.contains(name)
            && akantu_solid::material::params_info(self, name).is_none()
        {
            return self.runtime_params.set(name, value);
        }
        akantu_solid::material::params_set(self, name, value)
    }
    fn param_names(&self) -> Vec<&'static str> {
        akantu_solid::material::params_names(self)
    }
    fn update_internal_parameters(&mut self) -> Result<()> {
        MaterialParams::update_params(self)
    }
    fn param_info(&self, name: &str) -> Option<akantu_parser::param::ParamInfo> {
        akantu_solid::material::params_info(self, name)
            .or_else(|| self.runtime_params.param_info(name))
    }
    fn set_param_from_input(&mut self, name: &str, value: ParamValue) -> Result<()> {
        if self.runtime_params.contains(name)
            && akantu_solid::material::params_info(self, name).is_none()
        {
            return self.runtime_params.set_from_input(name, value);
        }
        akantu_solid::material::params_set_from_input(self, name, value)
    }
}

/// The state of a runtime cohesive law as the bindings reach it inside its
/// callbacks ([`akantu_solid::RuntimeLawState`]): parameters (upstream
/// `MaterialCohesive`'s, then the runtime ones), the cohesive internals
/// (`opening`, `tractions`, `normal`, …) and the runtime internals.
impl akantu_solid::RuntimeLawState for MaterialCohesive {
    fn law_base(&self) -> &MaterialBase {
        &self.base
    }
    fn law_base_mut(&mut self) -> &mut MaterialBase {
        &mut self.base
    }
    fn runtime_internals(&self) -> &akantu_model::internal_field::DynamicInternals {
        &self.runtime_internals
    }
    fn runtime_internals_mut(&mut self) -> &mut akantu_model::internal_field::DynamicInternals {
        &mut self.runtime_internals
    }
    fn own_real_internals(&self) -> Vec<(&'static str, &InternalField<Real>)> {
        let nb_base = self.base.internals().len();
        self.fields().into_iter().skip(nb_base).collect()
    }
    fn own_real_internal_mut(&mut self, name: &str) -> Option<&mut InternalField<Real>> {
        let nb_base = self.base.internals().len();
        self.fields_mut()
            .into_iter()
            .skip(nb_base)
            .find(|(n, _)| *n == name)
            .map(|(_, f)| f)
    }
    fn get_param(&self, name: &str) -> Result<ParamValue> {
        akantu_parser::ParameterRegistry::get_param(self, name)
    }
    fn set_param(&mut self, name: &str, value: ParamValue) -> Result<()> {
        akantu_parser::ParameterRegistry::set_param(self, name, value)
    }
    fn param_info(&self, name: &str) -> Option<akantu_parser::param::ParamInfo> {
        akantu_parser::ParameterRegistry::param_info(self, name)
    }
    fn register_param_real(
        &mut self,
        name: &str,
        access: ParameterAccess,
        description: &str,
        default: Option<Real>,
    ) -> Result<()> {
        if akantu_solid::material::params_info(self, name).is_some() {
            return Err(Error::AlreadyExists {
                kind: "parameter",
                id: name.into(),
            });
        }
        self.runtime_params.register(
            name,
            ParamKind::Real,
            access,
            description,
            default.map(ParamValue::Real),
        )
    }
    fn base_init(&mut self, ctx: &LawContext<'_>) -> Result<()> {
        MaterialCohesive::base_init(self, ctx)
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

pub(crate) const PAR_READ: ParameterAccess =
    ParameterAccess(ParameterAccess::PARSABLE.0 | ParameterAccess::READABLE.0);

fn set_sigma_c(s: &mut MaterialCohesive, v: ParamValue) -> Result<()> {
    let p = match v {
        ParamValue::Random(p) => p,
        other => akantu_parser::param::RandomParameter {
            base_value: other.as_real()?,
            distribution: None,
        },
    };
    s.core.sigma_c.set_random_distribution(p.to_core());
    s.core.sigma_c_param = p;
    Ok(())
}

/// `sigma_c` (upstream `MaterialCohesive` constructor).
pub(crate) const P_SIGMA_C: ParamDescriptor<MaterialCohesive> = ParamDescriptor::new(
    "sigma_c",
    ParamKind::Random,
    PAR_READ,
    "Critical stress",
    |s| ParamValue::Random(s.core.sigma_c_param.clone()),
    Some(set_sigma_c),
);

/// `delta_c` (upstream `MaterialCohesive` constructor).
pub(crate) const P_DELTA_C: ParamDescriptor<MaterialCohesive> = ParamDescriptor::new(
    "delta_c",
    ParamKind::Real,
    PAR_READ,
    "Critical displacement",
    |s| ParamValue::Real(s.core.delta_c),
    Some(|s, v| {
        s.core.delta_c = v.as_real()?;
        Ok(())
    }),
);

impl MaterialParams for MaterialCohesive {
    fn params_base(&self) -> &MaterialBase {
        &self.base
    }
    fn params_base_mut(&mut self) -> &mut MaterialBase {
        &mut self.base
    }
    fn own_params(&self) -> &'static [ParamDescriptor<Self>] {
        self.law.params()
    }
    fn update_params(&mut self) -> Result<()> {
        self.base.update_internal_parameters()?;
        self.law.update_internal_parameters();
        Ok(())
    }
}

impl MaterialCohesive {
    /// Commit the maximal openings the law reads as history
    /// ([`crate::laws::CohesiveLaw::committed_maxima`], **UD-245**): their
    /// current values become the committed (previous) ones. Called at the
    /// end of a converged solve step (`after_solve_step(true)`), and by
    /// [`crate::SolidMechanicsModelCohesive::solve_step`] when a step
    /// converged without that hook (UD-010 legacy hooks).
    /// No-op under `compat.maxima_committed_at_step_entry`.
    ///
    /// # Errors
    /// History not initialised.
    pub fn commit_maxima(&mut self) -> Result<()> {
        let committed = self.law.committed_maxima(&self.core);
        if committed.is_empty() {
            return Ok(());
        }
        for (name, f) in self.fields_mut() {
            if committed.contains(&name) {
                f.save_current_values()?;
            }
        }
        Ok(())
    }

    /// `savePreviousState` of every history except the committed maxima.
    fn save_previous_state_except_committed(&mut self) -> Result<()> {
        let committed = self.law.committed_maxima(&self.core);
        self.for_each_internal_mut(&mut |name, f| {
            if committed.contains(&name) {
                return Ok(());
            }
            match f {
                InternalFieldMut::Real(f) if f.has_history() => f.save_current_values(),
                InternalFieldMut::Int(f) if f.has_history() => f.save_current_values(),
                InternalFieldMut::UInt(f) if f.has_history() => f.save_current_values(),
                InternalFieldMut::Bool(f) if f.has_history() => f.save_current_values(),
                _ => Ok(()),
            }
        })
    }
}

impl Material for MaterialCohesive {
    fn base(&self) -> &MaterialBase {
        &self.base
    }

    fn base_mut(&mut self) -> &mut MaterialBase {
        &mut self.base
    }

    /// Upstream `initMaterial` of the law: `Material::initMaterial`
    /// (history of the regular internals, `resizeInternals`, `is_init`)
    /// wrapped by the law's own initialisation.
    fn init_material(&mut self, ctx: &LawContext<'_>) -> Result<()> {
        if self.is_foreign() {
            // the runtime law's `initMaterial` (default: `base_init`)
            return self.with_foreign(|m, law| m.init_material(law, ctx));
        }
        if self.core.use_previous_delta_max && !self.core.compat.delta_max_without_history {
            self.core.delta_max.initialize_history();
        }
        self.law.before_init(&mut self.core);
        self.base.init_history()?;
        self.resize_internals(&[])?;
        self.base.set_init();
        let Self {
            base, core, law, ..
        } = self;
        law.after_init(core, base, ctx)
    }

    fn compute_stress(&mut self, _: &LawContext<'_>, _: ElementType, _: GhostType) -> Result<()> {
        Ok(())
    }

    fn compute_tangent_moduli(
        &mut self,
        _: &LawContext<'_>,
        _: ElementType,
        _: GhostType,
        _: &mut Array<Real>,
    ) -> Result<()> {
        Err(Error::NotImplemented(
            "computeTangentModuli of a cohesive material (assembled by the cohesive tangent)",
        ))
    }

    fn compute_potential_energy(&mut self, _: &LawContext<'_>, _: ElementType) -> Result<()> {
        Ok(())
    }

    fn push_wave_speed(&self, _: Element) -> Result<Real> {
        Err(Error::NotImplemented(
            "getPushWaveSpeed of a cohesive material",
        ))
    }

    fn shear_wave_speed(&self, _: Element) -> Result<Real> {
        Err(Error::NotImplemented(
            "getShearWaveSpeed of a cohesive material",
        ))
    }

    /// Upstream `computeAllStresses` is final and empty for cohesive laws.
    fn compute_all_stresses(&mut self, _: &LawContext<'_>, _: GhostType) -> Result<()> {
        Ok(())
    }

    fn potential_energy(&mut self, _: &LawContext<'_>) -> Result<Real> {
        Ok(0.0)
    }

    /// Upstream `MaterialCohesive::getEnergy(type)`: `reversible`,
    /// `dissipated`, `cohesive contact`, else 0.
    fn energy_of(&mut self, ctx: &LawContext<'_>, energy_id: &str) -> Result<Real> {
        let fe = ctx.services.fe_engine(COHESIVE_FE_ENGINE)?;
        match energy_id {
            "reversible" => self.reversible_energy(fe),
            "dissipated" => self.dissipated_energy(fe),
            "cohesive contact" => self.contact_energy(fe),
            _ => Ok(0.0),
        }
    }

    fn update_energies(&mut self, ctx: &LawContext<'_>, ty: ElementType) -> Result<()> {
        self.update_energies_impl(ty, ctx.services.exec())
    }

    /// Upstream `beforeSolveStep` (`savePreviousState`), except for the
    /// committed maximal openings (**UD-245**,
    /// [`crate::laws::CohesiveLaw::committed_maxima`]): those are committed
    /// at the end of a converged step, so a trial evaluated between two
    /// steps (outside a step) is never committed by the next step entry.
    fn before_solve_step(&mut self, _: &LawContext<'_>) -> Result<()> {
        self.save_previous_state_except_committed()
    }

    /// Upstream `afterSolveStep`: a non-converged step restores every
    /// history (the committed maxima included: the current values return to
    /// the latest committed state; `cohesive_bilinear` keeps its maximal
    /// opening at least `δ₀`); a converged step updates the energies, then
    /// commits the maximal openings of [`crate::laws::CohesiveLaw::committed_maxima`]
    /// (**UD-245**).
    fn after_solve_step(&mut self, ctx: &LawContext<'_>, converged: bool) -> Result<()> {
        if !converged {
            self.restore_previous_state()?;
            let Self { core, law, .. } = self;
            return law.after_restore(core);
        }
        let types: Vec<ElementType> = self
            .base
            .element_filter
            .as_map()
            .iter(GhostType::NotGhost)
            .map(|(t, _)| t)
            .collect();
        for ty in types {
            self.update_energies(ctx, ty)?;
        }
        self.commit_maxima()
    }

    /// Upstream `onDamageIteration` (`savePreviousState`), except for the
    /// committed maximal openings: a damage iteration
    /// happens inside a solve step, so the maxima it sees are trials, never
    /// committed by it (they are committed at the end of the converged
    /// step).
    fn on_damage_iteration(&mut self) -> Result<()> {
        self.save_previous_state_except_committed()
    }

    fn on_elements_added(&mut self, ctx: &LawContext<'_>, elements: &[Element]) -> Result<()> {
        let Self {
            base, core, law, ..
        } = self;
        law.on_elements_added(core, base, ctx, elements)
    }

    fn as_any(&self) -> Option<&dyn std::any::Any> {
        Some(self)
    }

    fn as_any_mut(&mut self) -> Option<&mut dyn std::any::Any> {
        Some(self)
    }

    fn assemble_internal_forces_custom(
        &self,
        fems: &FeEngineRegistry,
        mesh: &Mesh,
        ghost: GhostType,
        f_int: ArrayViewMut<'_, Real>,
    ) -> Option<Result<()>> {
        Some(self.assemble_internal_forces_impl(fems, mesh, ghost, f_int))
    }

    fn elemental_stiffness_custom(
        &mut self,
        ctx: &LawContext<'_>,
        ghost: GhostType,
    ) -> Option<Result<Vec<(ElementType, Vec<Idx>, Array<Real>, MatrixType)>>> {
        Some(self.elemental_stiffness_impl(ctx, ghost))
    }
}

/// Register the cohesive laws in `factory` under their upstream names.
///
/// # Errors
/// A name already registered.
pub fn register_cohesive_materials(factory: &mut MaterialFactory) -> Result<()> {
    for name in crate::laws::LAW_NAMES {
        factory.register(
            name,
            Arc::new(move |spec: &LawSpec, services: &dyn ModelServices| {
                let m = MaterialCohesive::new(spec, name, services.mesh())?;
                Ok(Box::new(m) as Box<dyn Material>)
            }),
        )?;
    }
    Ok(())
}
