// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/model/phase_field/phasefield.hh,
// phasefield.cc, phasefield_inline_impl.hh; src/model/common/
// constitutive_laws/constitutive_law_tmpl.hh (name parameter,
// initConstitutiveLaw), random_internal_field_tmpl.hh (`gc` parameter).

//! The phase-field law contract (upstream `PhaseField`, PF-005).
//!
//! A law owns its parameters and the per-quadrature-point internals of
//! upstream ([`PhaseFieldBase`]): `g_c` (random field, parameter `gc`),
//! `damage` (with history), `grad_d`, `phi` (with history), `strain`
//! (`dim²`, written by the solid coupler), `driving_force`,
//! `driving_energy` (`dim`), `damage_energy` (`dim²`),
//! `damage_energy_density`, `dissipated_energy`. The model drives the laws
//! through [`PhaseField`]; every method that needs the model receives a
//! phase-scoped [`LawContext`] (ARCHITECTURE), whose services lend the
//! FE engine `PhaseFieldFEEngine` and the nodal `damage`.
//!
//! # Weak form
//!
//! For the nodal damage `d` the model assembles (upstream statement order,
//! [`crate::model`]):
//!
//! ```text
//! f_int = −Σ_e ( ∫ Nᵀ · driving_force + ∫ Bᵀ · driving_energy )
//! K     =  Σ_e ( ∫ Nᵀ · damage_energy_density · N + ∫ Bᵀ · damage_energy · B )
//! ```
//!
//! with, per quadrature point, `driving_force = f₀(φ) + d·damage_energy_density`
//! and `driving_energy = damage_energy · ∇d` (law specific `f₀`, see
//! [`crate::laws`]). With `irreversibility_type = history` or `tao` (and the
//! corrected UD-377 interpolation) the residual `r = f_ext + f_int` is linear
//! in `d` (`r = −K d + const`), so Newton–Raphson converges in one
//! correction. With `penalization` the residual is only piecewise linear
//! (penalties `γ·min(0, d − d_prev)`, AT1 `ρ_rec·min(0, d)`), and under the
//! UD-377 compat interpolation `K` is not its Jacobian: Newton may need
//! several corrections or diverge (UD-385, UD-388).
//!
//! # Upstream defects (D-003)
//!
//! [`PhaseFieldUpstreamCompat`] collects the switches reproducing upstream
//! defects of this package (`docs/UPSTREAM_DEFECTS.md` UD-377…UD-380,
//! UD-385, UD-387, UD-388); the default is the corrected behaviour. The model sets
//! them on every law ([`crate::PhaseFieldModel::set_upstream_compat`], the
//! only setter: one set of switches per model, D-041 spirit). UD-004 (ghost
//! gradients written into not-ghost storage) is reachable on distributed
//! meshes (switch `ghost_gradient_into_not_ghost`).

use akantu_core::eigen_order::product_coeff;
use akantu_core::{
    Array, Element, ElementType, ElementTypeMapArray, Error, GhostType, Idx, Real, Result,
};
use akantu_mesh::Mesh;
use akantu_model::InternalField;
use akantu_model::constitutive_law::{ConstitutiveLaw, LawContext, LawSpec};
use akantu_model::internal_field::TypeRestriction;
use akantu_parser::ByteString;
use akantu_parser::param::{
    ParamDescriptor, ParamInfo, ParamKind, ParamValue, ParameterAccess, RandomParameter, table_get,
    table_info, table_names, table_set, table_set_from_input,
};

use crate::energy_split::EnergySplit;

/// Switches reproducing upstream defects of the phase-field package
/// (decision D-003, category B: corrected by default). Applied by the
/// model to every law ([`crate::PhaseFieldModel::set_upstream_compat`]).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PhaseFieldUpstreamCompat {
    /// **UD-377**: upstream `computeAllDrivingForces` and
    /// `computeDissipatedEnergyByElements` interpolate the nodal damage on
    /// **every** element of the type (no element filter) into the law's
    /// `damage` internal (resizing it to the mesh block) and then read it
    /// positionally against the law's own points: with several laws (or a
    /// law not covering a whole type) a law uses the damage of the first
    /// mesh elements instead of its own. Default: interpolate on the law's
    /// elements.
    pub unfiltered_damage_interpolation: bool,
    /// **UD-378**: upstream `PhaseField::assembleStiffnessMatrix`
    /// **returns** at the first element type whose filter is empty, so the
    /// following types of that law are never assembled into `K` (mixed
    /// meshes). Default: skip the empty type (`continue`, as every other
    /// loop of the law).
    pub stiffness_stops_at_empty_filter: bool,
    /// **UD-379**: upstream `computeDissipatedEnergyByElement` never
    /// advances its `g_c` iterator: every quadrature point of the element
    /// uses the `g_c` of the element's first point (visible with a random
    /// `gc`). Default: each point's own `g_c`.
    pub element_energy_first_point_gc: bool,
    /// **UD-380**: upstream `PhaseField::getEnergy(id, element)` integrates
    /// with the **law-local** element index used as a mesh element index
    /// (Jacobians of another element when the law's filter is not the
    /// identity). Default: the mesh element of the law-local index.
    pub element_energy_local_index_as_mesh_index: bool,
    /// **UD-385**: upstream `PhaseFieldLinear::initPhaseField` accepts a
    /// non-positive recovery penalty `ρ_rec = g_c/l0·9(diam/l0 − 2)/(64
    /// tol_rec) ≤ 0` (mesh bounding-box diagonal `diam ≤ 2 l0`, or
    /// `g_c ≤ 0`, `tol_rec ≤ 0`) with `irreversibility_type = penalization`:
    /// the AT1 point density `2φ + γ[d < d_prev] + ρ_rec[d < 0]` becomes
    /// negative (anti-penalty) and upstream's Newton–Raphson can diverge
    /// (oracle case `rev_at1_negative_rho_rec`). Default: refused at
    /// `initPhaseField` ([`Error::InvalidArgument`]). Read at
    /// `initPhaseField`.
    pub accept_nonpositive_recovery_penalty: bool,
    /// **UD-388**: upstream AT1 penalisation tangent uses the strict
    /// indicator `γ·[d < d_prev]` (`phasefield_linear.cc:140`). After
    /// `savePreviousState` every step starts at `d = d_prev`, so the
    /// indicator is 0; with `φ = 0` (full unloading) the AT1 density `2φ` is
    /// 0 too and `K = ∫Bᵀ(¾g_c l0)B` is singular without a damage Dirichlet
    /// condition (oracle case `rev_at1_full_unload_quadrangle_4`: Newton
    /// diverges). Default: the active one-sided derivative `γ·[d ≤ d_prev]`
    /// of the continuous residual `γ·min(0, d − d_prev)` (`K` stays positive
    /// definite for `γ > 0`). AT2 is unaffected (its density `2φ + g_c/l0`
    /// is positive) and keeps upstream's indicator.
    pub at1_strict_irreversibility_indicator: bool,
    /// **UD-387** (category B, D-066 (2)): upstream's driving energy of the
    /// `volumetric_deviatoric` split in 1-D/2-D uses the deviator of the
    /// `D × D` block only (`tr ε/3` on the diagonal, the out-of-plane
    /// components dropped), so `φ⁺` is not the potential of the degraded
    /// stress `σ⁺` (`∂φ⁺/∂ε = σ⁺ − (2μ/9)(3 − D) tr ε I`) and `φ⁺ + φ⁻` is not
    /// the unsplit energy (the upstream test value `5/9 μ ε²`). Default: the
    /// consistent `φ⁺ = ½κ⟨tr ε⟩₊² + μ(ε:ε − (tr ε)²/3)`
    /// ([`EnergySplit::phi_consistent`], `2/3 μ ε²` there); 3-D and the
    /// `no_split` option are the same either way.
    pub in_block_deviator_energy: bool,
    /// **UD-004** (category B; reachable on distributed meshes): upstream `PhaseField::computeAllDrivingForces(_ghost)`
    /// writes the gradient of the ghost elements' damage into the
    /// **not-ghost** `grad_d` block (`gradd(type, _not_ghost)` with the
    /// ghost filter, `phasefield.cc:114-117`: the block is resized to the
    /// ghost points and overwritten until the next not-ghost computation),
    /// and the ghost `grad_d` stays at its default. No assembled quantity
    /// reads either in between (`_pfm_damage` overwrites the ghost driving
    /// force and energy before the ghost assembly); a reader of the stored
    /// `grad_d` after `beforeSolveStep` (an element energy, a dump) sees the
    /// ghost values. Default: the ghost gradient goes into the ghost block.
    pub ghost_gradient_into_not_ghost: bool,
    /// **UD-553** (category B, as the solid model, MPI-053):
    /// upstream's `synchronize(tag)` for a tag without a registered
    /// synchronizer is a silent no-op. Default:
    /// [`crate::PhaseFieldModel::synchronize`] refuses such a tag on a
    /// distributed mesh; `true` does nothing, as upstream.
    pub unregistered_synchronization_tag_is_a_noop: bool,
}

impl PhaseFieldUpstreamCompat {
    /// Upstream behaviour for every switch.
    #[must_use]
    pub const fn upstream() -> Self {
        Self {
            unfiltered_damage_interpolation: true,
            stiffness_stops_at_empty_filter: true,
            element_energy_first_point_gc: true,
            element_energy_local_index_as_mesh_index: true,
            accept_nonpositive_recovery_penalty: true,
            at1_strict_irreversibility_indicator: true,
            in_block_deviator_energy: true,
            ghost_gradient_into_not_ghost: true,
            unregistered_synchronization_tag_is_a_noop: true,
        }
    }
}

/// Irreversibility enforcement (upstream parameter `irreversibility_type`,
/// PF-013…018).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Irreversibility {
    /// `history`: `φ ← max(φ, φ_prev)` (default).
    #[default]
    History,
    /// `penalization`: penalty on damage decrease (and, AT1, on negative
    /// damage).
    Penalization,
    /// `tao`: neither; bounds are expected from an external bound-constrained
    /// solver (PETSc TAO, NLS-007).
    Tao,
}

/// Parameters and internals common to every phase-field law (upstream
/// `PhaseField` members).
#[derive(Clone, Debug)]
pub struct PhaseFieldBase {
    /// Law id (`<model id>:<index>:<type>`).
    pub id: String,
    /// User name (decoded `name` parameter).
    pub name: String,
    name_bytes: ByteString,
    /// Registered type (`quadratic`, `linear`).
    pub law_type: String,
    /// Spatial dimension.
    pub spatial_dimension: usize,
    /// Elements of the law, per `(type, ghost)`.
    pub element_filter: ElementTypeMapArray<Idx>,
    /// Length scale `l0`.
    pub l0: Real,
    /// Young's modulus `E`.
    pub e: Real,
    /// Poisson's ratio `nu`.
    pub nu: Real,
    /// `Plane_Stress`.
    pub plane_stress: bool,
    /// Upstream member `finite_deformation` (never registered as a
    /// parameter, never read).
    pub finite_deformation: bool,
    /// Raw `irreversibility_type` bytes.
    pub irreversibility_type: ByteString,
    /// Decoded irreversibility (set by `updateInternalParameters`).
    pub irreversibility: Irreversibility,
    /// Upstream `use_history`.
    pub use_history: bool,
    /// Upstream `use_penalization`.
    pub use_penalization: bool,
    /// First Lamé parameter (`updateInternalParameters`).
    pub lambda: Real,
    /// Shear modulus.
    pub mu: Real,
    /// The `gc` parameter as given (getter).
    gc_param: RandomParameter,
    /// Critical energy release rate per quadrature point (random field).
    pub g_c: InternalField<Real>,
    /// Damage at the quadrature points (with history).
    pub damage: InternalField<Real>,
    /// Damage gradient (`dim`).
    pub grad_d: InternalField<Real>,
    /// Driving energy density `φ` (with history).
    pub phi: InternalField<Real>,
    /// Strain (`dim × dim`, column-major), written by the coupler.
    pub strain: InternalField<Real>,
    /// Scalar driving force.
    pub driving_force: InternalField<Real>,
    /// Vector driving energy (`dim`).
    pub driving_energy: InternalField<Real>,
    /// Gradient coefficient matrix (`dim × dim`).
    pub damage_energy: InternalField<Real>,
    /// Scalar coefficient of the damage term.
    pub damage_energy_density: InternalField<Real>,
    /// Dissipated energy density.
    pub dissipated_energy: InternalField<Real>,
    /// Upstream-defect switches, set by the model only
    /// ([`crate::PhaseFieldModel::set_upstream_compat`]; read with
    /// [`PhaseFieldBase::compat`]).
    pub(crate) compat: PhaseFieldUpstreamCompat,
    is_init: bool,
}

/// Names of the internals in upstream registration order.
pub const INTERNAL_NAMES: [&str; 10] = [
    "g_c",
    "damage",
    "grad_d",
    "phi",
    "strain",
    "driving_force",
    "driving_energy",
    "damage_energy",
    "damage_energy_density",
    "dissipated_energy",
];

impl PhaseFieldBase {
    /// Upstream `PhaseField(model, id)`: internals registered (`phi` and
    /// `damage` with history, default 0), element filter with an empty
    /// block for every regular element type of the law's dimension of
    /// `mesh` (both ghost types).
    pub fn new(spec: &LawSpec, law_type: &str, mesh: &Mesh) -> Result<Self> {
        let d = akantu_core::dim::check_dim(spec.spatial_dimension)?;
        let gc_param = RandomParameter {
            base_value: 0.0,
            distribution: None,
        };
        let mut phi = InternalField::new("phi", 1, 0.0)?;
        let mut damage = InternalField::new("damage", 1, 0.0)?;
        phi.set_default_value(0.0);
        damage.set_default_value(0.0);
        phi.initialize_history();
        damage.initialize_history();
        let mut element_filter = ElementTypeMapArray::new(format!("{}:element_filter", spec.id));
        for ghost in GhostType::ALL {
            let filter = akantu_core::TypeFilter {
                spatial_dimension: Some(d),
                ghost_type: ghost,
                kind: Some(akantu_core::ElementKind::Regular),
            };
            let types: Vec<ElementType> = mesh.element_types(filter).collect();
            for ty in types {
                element_filter.alloc(0, 1, ty, ghost, 0)?;
            }
        }
        let mut base = Self {
            id: spec.id.clone(),
            name: String::new(),
            name_bytes: ByteString::default(),
            law_type: law_type.into(),
            spatial_dimension: d,
            element_filter,
            l0: 0.0,
            e: 0.0,
            nu: 0.0,
            plane_stress: false,
            finite_deformation: false,
            irreversibility_type: ByteString::from("history"),
            irreversibility: Irreversibility::History,
            use_history: true,
            use_penalization: false,
            lambda: 0.0,
            mu: 0.0,
            g_c: InternalField::new_random("g_c", 1, gc_param.to_core())?,
            gc_param,
            damage,
            grad_d: InternalField::new("grad_d", d, 0.0)?,
            phi,
            strain: InternalField::new("strain", d * d, 0.0)?,
            driving_force: InternalField::new("driving_force", 1, 0.0)?,
            driving_energy: InternalField::new("driving_energy", d, 0.0)?,
            damage_energy: InternalField::new("damage_energy", d * d, 0.0)?,
            damage_energy_density: InternalField::new("damage_energy_density", 1, 0.0)?,
            dissipated_energy: InternalField::new("dissipated_energy", 1, 0.0)?,
            compat: PhaseFieldUpstreamCompat::default(),
            is_init: false,
        };
        // upstream `registerInternal`: every internal is an `InternalField`
        // with the law's dimension and `_ek_regular`
        // (`constitutive_law_tmpl.hh:63-72`, `internal_field_tmpl.hh:31-56`),
        // so it only handles the filter types of that dimension and kind
        // (row CL-002)
        for (_, f) in base.internals_mut() {
            f.set_restriction(d, akantu_core::ElementKind::Regular);
        }
        // upstream InternalField::initialize: an (empty) block for every
        // type of the element filter, so the parse-time
        // `updateInternalParameters` loops over existing, empty blocks
        let empty: Vec<(ElementType, GhostType, usize)> = base
            .element_filter
            .as_map()
            .iter_all()
            .map(|(ty, g, _)| (ty, g, 0))
            .collect();
        for (_, f) in base.internals_mut() {
            f.resize_blocks(&empty)?;
        }
        Ok(base)
    }

    /// Element types the internals handle: the law's spatial dimension and
    /// `_ek_regular` (upstream `InternalField::spatial_dimension` /
    /// `element_kind` of `registerInternal`); what the laws return from
    /// [`ConstitutiveLaw::internal_restriction`].
    #[must_use]
    pub fn internal_restriction(&self) -> TypeRestriction {
        TypeRestriction {
            spatial_dimension: Some(self.spatial_dimension),
            element_kind: akantu_core::ElementKind::Regular,
        }
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

    /// The `gc` parameter (base value and distribution as given).
    #[must_use]
    pub fn gc_parameter(&self) -> &RandomParameter {
        &self.gc_param
    }

    /// Upstream `Real(g_c)`: the base value of the `gc` parameter.
    #[must_use]
    pub fn gc(&self) -> Real {
        self.g_c.parameter_value()
    }

    /// The upstream-defect switches of this law (the model's, applied by
    /// [`crate::PhaseFieldModel::set_upstream_compat`] and at
    /// `initConstitutiveLaws`).
    #[must_use]
    pub fn compat(&self) -> PhaseFieldUpstreamCompat {
        self.compat
    }

    /// `true` once `initPhaseField` ran.
    #[must_use]
    pub fn is_init(&self) -> bool {
        self.is_init
    }

    /// Mark initialised.
    pub fn set_init(&mut self) {
        self.is_init = true;
    }

    /// Element types of the filter of the law's dimension for `ghost`
    /// (upstream `getElementFilter().elementTypes(dim, ghost)`).
    #[must_use]
    pub fn filter_types(&self, ghost: GhostType) -> Vec<ElementType> {
        self.element_filter
            .element_types(akantu_core::TypeFilter {
                spatial_dimension: Some(self.spatial_dimension),
                ghost_type: ghost,
                kind: None,
            })
            .collect()
    }

    /// Filter of `(ty, ghost)` (empty when absent).
    #[must_use]
    pub fn filter(&self, ty: ElementType, ghost: GhostType) -> &[Idx] {
        self.element_filter
            .get(ty, ghost)
            .map_or(&[][..], |a| a.as_slice())
    }

    /// `(type, ghost, nb_quadrature_points)` of every filter block of the
    /// types the internals handle (the law's dimension, `_ek_regular`;
    /// upstream `InternalField::filterTypes`, empty filter blocks
    /// included). Elements of other types in the filter get no internal
    /// block (row CL-002).
    #[must_use]
    pub fn blocks(&self, ctx: &LawContext<'_>) -> Vec<(ElementType, GhostType, usize)> {
        let r = self.internal_restriction();
        self.element_filter
            .as_map()
            .iter_all()
            .filter(|&(ty, _, _)| r.handles(ty))
            .map(|(ty, g, f)| (ty, g, f.len() * ctx.services.nb_integration_points(ty, g)))
            .collect()
    }

    /// The internals in registration order (name, field).
    #[must_use]
    pub fn internals(&self) -> [(&'static str, &InternalField<Real>); 10] {
        [
            (INTERNAL_NAMES[0], &self.g_c),
            (INTERNAL_NAMES[1], &self.damage),
            (INTERNAL_NAMES[2], &self.grad_d),
            (INTERNAL_NAMES[3], &self.phi),
            (INTERNAL_NAMES[4], &self.strain),
            (INTERNAL_NAMES[5], &self.driving_force),
            (INTERNAL_NAMES[6], &self.driving_energy),
            (INTERNAL_NAMES[7], &self.damage_energy),
            (INTERNAL_NAMES[8], &self.damage_energy_density),
            (INTERNAL_NAMES[9], &self.dissipated_energy),
        ]
    }

    /// The internals in registration order, mutable.
    pub fn internals_mut(&mut self) -> [(&'static str, &mut InternalField<Real>); 10] {
        [
            (INTERNAL_NAMES[0], &mut self.g_c),
            (INTERNAL_NAMES[1], &mut self.damage),
            (INTERNAL_NAMES[2], &mut self.grad_d),
            (INTERNAL_NAMES[3], &mut self.phi),
            (INTERNAL_NAMES[4], &mut self.strain),
            (INTERNAL_NAMES[5], &mut self.driving_force),
            (INTERNAL_NAMES[6], &mut self.driving_energy),
            (INTERNAL_NAMES[7], &mut self.damage_energy),
            (INTERNAL_NAMES[8], &mut self.damage_energy_density),
            (INTERNAL_NAMES[9], &mut self.dissipated_energy),
        ]
    }

    /// Upstream `PhaseField::updateInternalParameters`: Lamé parameters
    /// (`λ = νE/((1+ν)(1−2ν))`, plane stress `νE/((1+ν)(1−ν))`,
    /// `μ = E/(2(1+ν))`) and the irreversibility flags.
    ///
    /// # Errors
    /// An `irreversibility_type` other than `history`, `penalization` or
    /// `tao` (upstream `AKANTU_EXCEPTION`).
    pub fn update_internal_parameters(&mut self) -> Result<()> {
        let (e, nu) = (self.e, self.nu);
        self.lambda = nu * e / ((1. + nu) * (1. - 2. * nu));
        if self.plane_stress {
            self.lambda = nu * e / ((1. + nu) * (1. - nu));
        }
        self.mu = e / (2. * (1. + nu));
        let (irr, hist, pen) = match self.irreversibility_type.as_bytes() {
            b"history" => (Irreversibility::History, true, false),
            b"penalization" => (Irreversibility::Penalization, false, true),
            b"tao" => (Irreversibility::Tao, false, false),
            _ => {
                return Err(Error::invalid(format!(
                    "Unknown irreversibility type: {}",
                    self.irreversibility_type
                )));
            }
        };
        self.irreversibility = irr;
        self.use_history = hist;
        self.use_penalization = pen;
        Ok(())
    }

    /// Upstream `updateInternalParameters` loop of the AT1/AT2 laws: for the
    /// not-ghost points, `damage_energy = (factor·g_c·l0) I` and
    /// `previous phi = phi_hist(g_c)`.
    pub(crate) fn init_damage_energy_and_history(
        &mut self,
        factor: Option<Real>,
        phi_hist: impl Fn(Real) -> Real,
    ) -> Result<()> {
        let d = self.spatial_dimension;
        let l0 = self.l0;
        for ty in self.filter_types(GhostType::NotGhost) {
            let gc = self.g_c.get(ty, GhostType::NotGhost)?.as_slice().to_vec();
            {
                let mut de = self.damage_energy.get_mut(ty, GhostType::NotGhost)?;
                for (m, &g) in de.as_mut_slice().chunks_exact_mut(d * d).zip(&gc) {
                    // Matrix d(dim, dim); d.eye(value)
                    let value = match factor {
                        Some(f) => f * g * l0,
                        None => g * l0,
                    };
                    // `t * Identity`: off-diagonal `t * 0` (keeps the sign of t)
                    for j in 0..d {
                        for i in 0..d {
                            m[i + j * d] = value * if i == j { 1.0 } else { 0.0 };
                        }
                    }
                }
            }
            let prev = self.phi.previous_mut(ty, GhostType::NotGhost)?;
            for (p, &g) in prev.as_mut_slice().iter_mut().zip(&gc) {
                *p = phi_hist(g);
            }
        }
        Ok(())
    }
}

/// Upstream `std::string name` (constitutive law) and `PhaseField`
/// parameters, in registration order.
pub const BASE_PARAMS: &[ParamDescriptor<PhaseFieldBase>] = &[
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
        "l0",
        ParamKind::Real,
        ParameterAccess(ParameterAccess::PARSABLE.0 | ParameterAccess::READABLE.0),
        "length scale parameter",
        |s| ParamValue::Real(s.l0),
        Some(|s, v| {
            s.l0 = v.as_real()?;
            Ok(())
        }),
    ),
    ParamDescriptor::new(
        "gc",
        ParamKind::Random,
        ParameterAccess(ParameterAccess::PARSABLE.0 | ParameterAccess::READABLE.0),
        "critical local fracture energy density",
        |s| ParamValue::Random(s.gc_param.clone()),
        Some(|s, v| {
            // upstream ParameterTyped<RandomInternalField>::setAuto/set:
            // setRandomDistribution (redraws the existing values)
            let p = match v {
                ParamValue::Random(p) => p,
                other => RandomParameter {
                    base_value: other.as_real()?,
                    distribution: None,
                },
            };
            s.g_c.set_random_distribution(p.to_core());
            s.gc_param = p;
            Ok(())
        }),
    ),
    ParamDescriptor::new(
        "E",
        ParamKind::Real,
        ParameterAccess(ParameterAccess::PARSABLE.0 | ParameterAccess::READABLE.0),
        "Young's modulus",
        |s| ParamValue::Real(s.e),
        Some(|s, v| {
            s.e = v.as_real()?;
            Ok(())
        }),
    ),
    ParamDescriptor::new(
        "nu",
        ParamKind::Real,
        ParameterAccess(ParameterAccess::PARSABLE.0 | ParameterAccess::READABLE.0),
        "Poisson ratio",
        |s| ParamValue::Real(s.nu),
        Some(|s, v| {
            s.nu = v.as_real()?;
            Ok(())
        }),
    ),
    ParamDescriptor::new(
        "irreversibility_type",
        ParamKind::ByteString,
        ParameterAccess(ParameterAccess::PARSABLE.0 | ParameterAccess::READABLE.0),
        "Type of irreversibility",
        |s| ParamValue::ByteString(s.irreversibility_type.clone()),
        Some(|s, v| {
            s.irreversibility_type = v.into_byte_string()?;
            Ok(())
        }),
    ),
    ParamDescriptor::new(
        "Plane_Stress",
        ParamKind::Bool,
        ParameterAccess(ParameterAccess::PARSABLE.0 | ParameterAccess::READABLE.0),
        "Is plane stress",
        |s| ParamValue::Bool(s.plane_stress),
        Some(|s, v| {
            s.plane_stress = v.as_bool()?;
            Ok(())
        }),
    ),
];

/// Built-in laws: a static parameter table on top of [`BASE_PARAMS`].
pub trait PhaseFieldParams: Sized + 'static {
    /// The common part.
    fn params_base(&self) -> &PhaseFieldBase;
    /// The common part, mutable.
    fn params_base_mut(&mut self) -> &mut PhaseFieldBase;
    /// The law's own parameters.
    fn own_params(&self) -> &'static [ParamDescriptor<Self>];
    /// Upstream `updateInternalParameters` of the most derived class.
    fn update_params(&mut self) -> Result<()>;
}

/// `get_param` of a built-in law.
pub fn params_get<S: PhaseFieldParams>(s: &S, name: &str) -> Result<ParamValue> {
    if table_info(BASE_PARAMS, name).is_some() {
        table_get(BASE_PARAMS, s.params_base(), name)
    } else {
        table_get(s.own_params(), s, name)
    }
}

/// `set` of a built-in law (writable check, then
/// `updateInternalParameters`).
pub fn params_set<S: PhaseFieldParams>(s: &mut S, name: &str, value: ParamValue) -> Result<()> {
    if table_info(BASE_PARAMS, name).is_some() {
        table_set(BASE_PARAMS, s.params_base_mut(), name, value)?;
    } else {
        let table = s.own_params();
        table_set(table, s, name, value)?;
    }
    s.update_params()
}

/// `setAuto` (input file) of a built-in law.
pub fn params_set_from_input<S: PhaseFieldParams>(
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
pub fn params_info<S: PhaseFieldParams>(s: &S, name: &str) -> Option<ParamInfo> {
    table_info(BASE_PARAMS, name).or_else(|| table_info(s.own_params(), name))
}

/// Parameter names of a built-in law (base first).
pub fn params_names<S: PhaseFieldParams>(s: &S) -> Vec<&'static str> {
    let mut v = table_names(BASE_PARAMS);
    v.extend(table_names(s.own_params()));
    v
}

/// Implements `ParameterRegistry`, `Parsable` and `ConstitutiveLaw` for a
/// built-in law implementing [`PhaseFieldParams`] whose base field is
/// `base`.
macro_rules! impl_phasefield_law {
    ($ty:ty) => {
        impl ::akantu_parser::ParameterRegistry for $ty {
            fn get_param(&self, name: &str) -> ::akantu_core::Result<::akantu_parser::ParamValue> {
                $crate::phasefield::params_get(self, name)
            }
            fn set_param(
                &mut self,
                name: &str,
                value: ::akantu_parser::ParamValue,
            ) -> ::akantu_core::Result<()> {
                $crate::phasefield::params_set(self, name, value)
            }
            fn param_names(&self) -> Vec<&'static str> {
                $crate::phasefield::params_names(self)
            }
            fn update_internal_parameters(&mut self) -> ::akantu_core::Result<()> {
                $crate::phasefield::PhaseFieldParams::update_params(self)
            }
            fn param_info(&self, name: &str) -> Option<::akantu_parser::param::ParamInfo> {
                $crate::phasefield::params_info(self, name)
            }
            fn set_param_from_input(
                &mut self,
                name: &str,
                value: ::akantu_parser::ParamValue,
            ) -> ::akantu_core::Result<()> {
                $crate::phasefield::params_set_from_input(self, name, value)
            }
        }
        impl ::akantu_parser::Parsable for $ty {
            fn section_type(&self) -> ::akantu_parser::ParserType {
                ::akantu_parser::ParserType::Phasefield
            }
            fn parsable_id(&self) -> &str {
                &self.base.id
            }
        }
        impl ::akantu_model::constitutive_law::ConstitutiveLaw for $ty {
            fn name(&self) -> &str {
                &self.base.name
            }
            fn law_id(&self) -> &str {
                &self.base.id
            }
            fn law_type(&self) -> &str {
                &self.base.law_type
            }
            fn element_filter(&self) -> &::akantu_core::ElementTypeMapArray<::akantu_core::Idx> {
                &self.base.element_filter
            }
            fn element_filter_mut(
                &mut self,
            ) -> &mut ::akantu_core::ElementTypeMapArray<::akantu_core::Idx> {
                &mut self.base.element_filter
            }
            fn internal_restriction(
                &self,
            ) -> Option<::akantu_model::internal_field::TypeRestriction> {
                Some(self.base.internal_restriction())
            }
            fn for_each_internal(
                &self,
                f: &mut dyn FnMut(&str, ::akantu_model::constitutive_law::InternalFieldRef<'_>),
            ) {
                for (n, field) in self.base.internals() {
                    f(
                        n,
                        ::akantu_model::constitutive_law::InternalFieldRef::Real(field),
                    );
                }
            }
            fn for_each_internal_mut(
                &mut self,
                f: &mut dyn FnMut(
                    &str,
                    ::akantu_model::constitutive_law::InternalFieldMut<'_>,
                ) -> ::akantu_core::Result<()>,
            ) -> ::akantu_core::Result<()> {
                for (n, field) in self.base.internals_mut() {
                    f(
                        n,
                        ::akantu_model::constitutive_law::InternalFieldMut::Real(field),
                    )?;
                }
                Ok(())
            }
            fn energy(&self, energy_id: &str) -> ::akantu_core::Result<::akantu_core::Real> {
                // The energies need the model (FE engine, nodal damage): use
                // `PhaseField::energy_of` / `PhaseFieldModel::energy`.
                let _ = energy_id;
                Err(::akantu_core::Error::invalid(
                    "phase-field energies need the model: use PhaseFieldModel::energy",
                ))
            }
        }
    };
}
pub(crate) use impl_phasefield_law;

/// A phase-field law (upstream `PhaseField`).
///
/// Batch methods work on all quadrature points of the law's elements of one
/// `(type, ghost)` block.
pub trait PhaseField: ConstitutiveLaw {
    /// Common state.
    fn base(&self) -> &PhaseFieldBase;

    /// Mutable common state (the coupler writes `strain`).
    fn base_mut(&mut self) -> &mut PhaseFieldBase;

    /// The energy split (`None` before `initPhaseField`).
    fn energy_split(&self) -> Option<&EnergySplit>;

    /// Upstream `initPhaseField` (through `initConstitutiveLaw`):
    /// `resizeInternals`, `updateInternalParameters`, then the law's
    /// derived constants and energy split.
    fn init_phase_field(&mut self, ctx: &LawContext<'_>) -> Result<()>;

    /// Upstream `computeDrivingForce(type, ghost)`: `φ` from the strain
    /// (history or penalty), then `driving_force`, `driving_energy`,
    /// `damage_energy_density` from the current `damage` and `grad_d`.
    fn compute_driving_force(&mut self, ty: ElementType, ghost: GhostType) -> Result<()>;

    /// Upstream `computeDissipatedEnergy(type)`: the dissipated energy
    /// density of the not-ghost points of `ty` into `dissipated_energy`.
    fn compute_dissipated_energy(&mut self, ty: ElementType) -> Result<()>;

    /// Upstream `computeDissipatedEnergyByElement(type, index, edis)`: the
    /// dissipated energy density at the quadrature points of the law-local
    /// element `index` of `ty` (not-ghost), from the stored `damage` and
    /// `grad_d` (not recomputed, as upstream).
    fn dissipated_energy_by_element(
        &self,
        ty: ElementType,
        index: usize,
        out: &mut [Real],
    ) -> Result<()>;

    /// Upstream `beforeSolveStep`: `computeAllDrivingForces` of the not-ghost
    /// then the ghost elements.
    fn before_solve_step(&mut self, ctx: &LawContext<'_>) -> Result<()> {
        self.compute_all_driving_forces(ctx, GhostType::NotGhost)?;
        self.compute_all_driving_forces(ctx, GhostType::Ghost)
    }

    /// Upstream `afterSolveStep` (no-op for the built-in laws; AT1's damage
    /// clamping is commented out upstream).
    fn after_solve_step(&mut self, ctx: &LawContext<'_>) -> Result<()> {
        let _ = ctx;
        Ok(())
    }

    /// Upstream `computeAllDrivingForces(ghost)`: for every type of the
    /// filter with elements, interpolate the nodal damage on the points
    /// (UD-377: law elements; compat: every element of the type), the
    /// damage gradient (law elements), then `computeDriving Force`.
    fn compute_all_driving_forces(&mut self, ctx: &LawContext<'_>, ghost: GhostType) -> Result<()> {
        for ty in self.base().filter_types(ghost) {
            if self.base().filter(ty, ghost).is_empty() {
                continue;
            }
            interpolate_damage(self.base_mut(), ctx, ty, ghost)?;
            self.compute_driving_force(ty, ghost)?;
        }
        Ok(())
    }

    /// Upstream `computeDissipatedEnergyByElements()`: refresh `damage` and
    /// `grad_d` of the not-ghost points (as in
    /// [`PhaseField::compute_all_driving_forces`]) and compute the
    /// dissipated energy density of every not-ghost type.
    fn compute_dissipated_energy_by_elements(&mut self, ctx: &LawContext<'_>) -> Result<()> {
        for ty in self.base().filter_types(GhostType::NotGhost) {
            if self.base().filter(ty, GhostType::NotGhost).is_empty() {
                continue;
            }
            interpolate_damage(self.base_mut(), ctx, ty, GhostType::NotGhost)?;
            self.compute_dissipated_energy(ty)?;
        }
        Ok(())
    }

    /// Upstream `getEnergy(id)`: `"dissipated"` → `0 + Σ_types ∫ e_dis` over
    /// the not-ghost law elements (after
    /// [`PhaseField::compute_dissipated_energy_by_elements`]); anything else
    /// `0`.
    fn energy_of(&mut self, ctx: &LawContext<'_>, energy_id: &str) -> Result<Real> {
        if energy_id != "dissipated" {
            return Ok(0.0);
        }
        self.compute_dissipated_energy_by_elements(ctx)?;
        let mut edis = 0.0;
        for ty in self.base().filter_types(GhostType::NotGhost) {
            let f = self.base().filter(ty, GhostType::NotGhost);
            let e = self.base().dissipated_energy.get(ty, GhostType::NotGhost)?;
            edis += ctx
                .services
                .integrate_scalar(e.as_slice(), ty, GhostType::NotGhost, f)?;
        }
        Ok(edis)
    }

    /// Upstream `getEnergy(id, element)` with a **law-local** element:
    /// `"dissipated"` → the integral of
    /// [`PhaseField::dissipated_energy_by_element`] over the element
    /// (UD-380: the element's mesh index; compat: the law-local index used
    /// as a mesh index); anything else `0`.
    fn energy_of_element(
        &self,
        ctx: &LawContext<'_>,
        energy_id: &str,
        element: Element,
    ) -> Result<Real> {
        if energy_id != "dissipated" {
            return Ok(0.0);
        }
        let nq = ctx
            .services
            .nb_integration_points(element.ty(), element.ghost_type());
        // no allocation per element: the default rules of the
        // regular elements have at most 27 points (hexahedron_20)
        let mut stack = [0.0; ELEMENT_POINTS_ON_STACK];
        let mut heap = Vec::new();
        let edis: &mut [Real] = if nq <= ELEMENT_POINTS_ON_STACK {
            &mut stack[..nq]
        } else {
            heap.resize(nq, 0.0);
            &mut heap
        };
        self.dissipated_energy_by_element(element.ty(), element.idx(), edis)?;
        let mesh_index = if self.base().compat.element_energy_local_index_as_mesh_index {
            element.index()
        } else {
            *self
                .base()
                .filter(element.ty(), element.ghost_type())
                .get(element.idx())
                .ok_or(Error::InvalidIndex {
                    what: "law-local element",
                    value: i64::from(element.index()),
                })?
        };
        ctx.services
            .integrate_scalar(edis, element.ty(), element.ghost_type(), &[mesh_index])
    }
}

/// Quadrature points of one element kept on the stack by
/// [`PhaseField::energy_of_element`] (more points fall back to the heap).
const ELEMENT_POINTS_ON_STACK: usize = 32;

/// `damage(type, ghost)` ← interpolated nodal damage and
/// `grad_d(type, ghost)` ← its gradient on the law's elements (upstream
/// `computeAllDrivingForces` / `computeDissipatedEnergyByElements`).
fn interpolate_damage(
    base: &mut PhaseFieldBase,
    ctx: &LawContext<'_>,
    ty: ElementType,
    ghost: GhostType,
) -> Result<()> {
    let filter = base.filter(ty, ghost).to_vec();
    let fe = ctx.services.fe_engine("")?;
    let mesh = ctx.services.mesh();
    let damage = ctx.services.nodal_field("damage")?;
    let unfiltered = base.compat.unfiltered_damage_interpolation;
    {
        // computed into a scratch array, then copied into the (exportable)
        // block: the FE engine resizes and overwrites its output (same
        // values and length as writing the block directly)
        let mut dq = Array::new(base.damage.nb_component());
        let f = if unfiltered { None } else { Some(&filter[..]) };
        fe.interpolate_on_integration_points(mesh, damage.view(), &mut dq, 1, ty, ghost, f)?;
        base.damage.get_mut(ty, ghost)?.copy_from(&dq)?;
    }
    // Upstream writes the gradient into the not-ghost block also for ghost
    // elements (UD-004, compat `ghost_gradient_into_not_ghost`: the block is
    // resized to the ghost points, as the FE engine's output); the port
    // writes the block of `ghost`.
    let mut gd = Array::new(base.grad_d.nb_component());
    fe.gradient_on_integration_points(mesh, damage.view(), &mut gd, 1, ty, ghost, Some(&filter))?;
    if ghost == GhostType::Ghost && base.compat.ghost_gradient_into_not_ghost {
        let mut b = base.grad_d.get_mut(ty, GhostType::NotGhost)?;
        b.try_resize(gd.len(), 0.0)?;
        b.copy_from(&gd)?;
    } else {
        base.grad_d.get_mut(ty, ghost)?.copy_from(&gd)?;
    }
    Ok(())
}

/// Per-point data of the driving-force kernels.
pub(crate) struct DrivingBlock<'a> {
    pub phi: &'a mut [Real],
    pub phi_prev: &'a [Real],
    pub strain: &'a [Real],
    pub driving_force: &'a mut [Real],
    pub damage_energy_density: &'a mut [Real],
    pub damage: &'a [Real],
    pub damage_prev: &'a [Real],
    pub g_c: &'a [Real],
    pub grad_d: &'a [Real],
    pub driving_energy: &'a mut [Real],
    pub damage_energy: &'a [Real],
}

/// Number of points of the block (upstream: the zip stops with its first
/// range, `phi`) after checking that every other range is at least as
/// long.
fn block_len(b: &DrivingBlock<'_>, d: usize) -> Result<usize> {
    let n = b.phi.len();
    let check = |what: &'static str, len: usize, nbc: usize| -> Result<()> {
        if len < n * nbc {
            return Err(Error::DimensionMismatch {
                context: what,
                expected: n * nbc,
                got: len,
            });
        }
        Ok(())
    };
    check("computeDrivingForce: previous phi", b.phi_prev.len(), 1)?;
    check("computeDrivingForce: strain", b.strain.len(), d * d)?;
    check(
        "computeDrivingForce: driving_force",
        b.driving_force.len(),
        1,
    )?;
    check(
        "computeDrivingForce: damage_energy_density",
        b.damage_energy_density.len(),
        1,
    )?;
    check("computeDrivingForce: damage", b.damage.len(), 1)?;
    check(
        "computeDrivingForce: previous damage",
        b.damage_prev.len(),
        1,
    )?;
    check("computeDrivingForce: g_c", b.g_c.len(), 1)?;
    check("computeDrivingForce: grad_d", b.grad_d.len(), d)?;
    check(
        "computeDrivingForce: driving_energy",
        b.driving_energy.len(),
        d,
    )?;
    check(
        "computeDrivingForce: damage_energy",
        b.damage_energy.len(),
        d * d,
    )?;
    Ok(n)
}

/// Law-specific constants of the driving-force kernel.
pub(crate) struct DrivingLaw<'a> {
    pub split: &'a EnergySplit,
    /// UD-387 compat: upstream's [`EnergySplit::phi`] instead of the
    /// corrected [`EnergySplit::phi_consistent`].
    pub upstream_phi: bool,
    pub use_history: bool,
    pub use_penalization: bool,
    /// `(φ, g_c) → (damage_energy_density, driving_force before the damage
    /// term)`.
    pub densities: &'a dyn Fn(Real, Real) -> (Real, Real),
    /// `(d, d_prev) → (driving-force penalty, density penalty)`.
    pub penalty: &'a dyn Fn(Real, Real, Real) -> (Real, Real),
}

/// Upstream `computeDrivingForce` + `computeResidual` (+ `applyPenalization`)
/// of the AT1/AT2 laws on one block, point by point (every upstream loop is
/// point-local, so fusing them keeps every operation and its order):
///
/// ```text
/// φ = split.phi(ε) (UD-387 corrected: split.phi_consistent(ε));  history: φ = max(φ, φ_prev)
/// (density, f0) = law(φ, g_c);  driving_force = f0;  driving_energy *= 0
/// driving_force += d · density;  driving_energy += damage_energy · ∇d
/// penalization: driving_force += p_f(d, d_prev);  density += p_k(d, d_prev)
/// ```
pub(crate) fn driving_force_kernel<const D: usize>(
    law: &DrivingLaw<'_>,
    b: DrivingBlock<'_>,
) -> Result<()> {
    let n = block_len(&b, D)?;
    for q in 0..n {
        let strain: &akantu_core::tensor::Matrix<D, D> =
            akantu_core::tensor::as_matrix(&b.strain[q * D * D..(q + 1) * D * D])?;
        let mut phi = if law.upstream_phi {
            law.split.phi(strain)
        } else {
            law.split.phi_consistent(strain)
        };
        if law.use_history && phi < b.phi_prev[q] {
            phi = b.phi_prev[q];
        }
        b.phi[q] = phi;
        let (density, f0) = (law.densities)(phi, b.g_c[q]);
        let mut density = density;
        let mut df = f0;
        let de = &mut b.driving_energy[q * D..(q + 1) * D];
        for v in de.iter_mut() {
            *v *= 0.0;
        }
        // computeResidual
        let d = b.damage[q];
        df += d * density;
        let dmat = &b.damage_energy[q * D * D..(q + 1) * D * D];
        let g = &b.grad_d[q * D..(q + 1) * D];
        for (i, v) in de.iter_mut().enumerate() {
            // Map<Matrix<D,D>> * Map<Vector<D>>: coefficient-based lazy
            // product, compile-time depth (halving tree)
            *v += product_coeff(D, true, &|k| dmat[i + k * D] * g[k]);
        }
        if law.use_penalization {
            let (pf, pk) = (law.penalty)(d, b.damage_prev[q], density);
            df += pf;
            density = pk;
        }
        b.driving_force[q] = df;
        b.damage_energy_density[q] = density;
    }
    Ok(())
}

/// Borrow every block of `(ty, ghost)` needed by the driving-force kernel.
pub(crate) fn with_driving_block<R>(
    base: &mut PhaseFieldBase,
    ty: ElementType,
    ghost: GhostType,
    f: impl FnOnce(DrivingBlock<'_>) -> Result<R>,
) -> Result<R> {
    let PhaseFieldBase {
        g_c,
        damage,
        grad_d,
        phi,
        strain,
        driving_force,
        driving_energy,
        damage_energy,
        damage_energy_density,
        ..
    } = base;
    let (mut phi_cur, phi_prev) = phi.current_and_previous_mut(ty, ghost)?;
    let damage_prev = damage.previous(ty, ghost)?;
    // guards bound in the order the fields were evaluated before (same
    // error precedence)
    let strain = strain.get(ty, ghost)?;
    let mut driving_force = driving_force.get_mut(ty, ghost)?;
    let mut damage_energy_density = damage_energy_density.get_mut(ty, ghost)?;
    let damage = damage.get(ty, ghost)?;
    let g_c = g_c.get(ty, ghost)?;
    let grad_d = grad_d.get(ty, ghost)?;
    let mut driving_energy = driving_energy.get_mut(ty, ghost)?;
    let damage_energy = damage_energy.get(ty, ghost)?;
    let block = DrivingBlock {
        phi: phi_cur.as_mut_slice(),
        phi_prev: phi_prev.as_slice(),
        strain: strain.as_slice(),
        driving_force: driving_force.as_mut_slice(),
        damage_energy_density: damage_energy_density.as_mut_slice(),
        damage: damage.as_slice(),
        damage_prev: damage_prev.as_slice(),
        g_c: g_c.as_slice(),
        grad_d: grad_d.as_slice(),
        driving_energy: driving_energy.as_mut_slice(),
        damage_energy: damage_energy.as_slice(),
    };
    f(block)
}

/// Upstream `computeDissipatedEnergy(type)` of the AT1/AT2 laws: over the
/// `phi` range of the not-ghost block, `e = density(d, ∇d, g_c)`.
pub(crate) fn dissipated_energy_kernel<const D: usize>(
    base: &mut PhaseFieldBase,
    ty: ElementType,
    density: &dyn Fn(Real, &[Real], Real) -> Real,
) -> Result<()> {
    let ghost = GhostType::NotGhost;
    let n = base.phi.get(ty, ghost)?.len();
    let PhaseFieldBase {
        g_c,
        damage,
        grad_d,
        dissipated_energy,
        ..
    } = base;
    let (d, g, gc) = (
        damage.get(ty, ghost)?,
        grad_d.get(ty, ghost)?,
        g_c.get(ty, ghost)?,
    );
    let (d, g, gc) = (d.as_slice(), g.as_slice(), gc.as_slice());
    let mut e = dissipated_energy.get_mut(ty, ghost)?;
    let e = e.as_mut_slice();
    if d.len() < n || g.len() < n * D || gc.len() < n || e.len() < n {
        return Err(Error::DimensionMismatch {
            context: "computeDissipatedEnergy: internals",
            expected: n,
            got: d.len().min(gc.len()).min(e.len()),
        });
    }
    for q in 0..n {
        e[q] = density(d[q], &g[q * D..(q + 1) * D], gc[q]);
    }
    Ok(())
}

/// Upstream `computeDissipatedEnergyByElement(type, index, edis)` of the
/// AT1/AT2 laws (UD-379: each point's `g_c`; compat: the first point's).
pub(crate) fn dissipated_energy_by_element_kernel<const D: usize>(
    base: &PhaseFieldBase,
    ty: ElementType,
    index: usize,
    nq: usize,
    out: &mut [Real],
    density: &dyn Fn(Real, &[Real], Real) -> Real,
) -> Result<()> {
    let ghost = GhostType::NotGhost;
    let (d, g, gc) = (
        base.damage.get(ty, ghost)?,
        base.grad_d.get(ty, ghost)?,
        base.g_c.get(ty, ghost)?,
    );
    let (d, g, gc) = (d.as_slice(), g.as_slice(), gc.as_slice());
    let start = index * nq;
    let end = start + nq;
    if end > d.len() || end * D > g.len() || end > gc.len() || out.len() < nq {
        return Err(Error::InvalidIndex {
            what: "computeDissipatedEnergyByElement: law-local element",
            value: i64::try_from(index).unwrap_or(i64::MAX),
        });
    }
    let first_gc = base.compat.element_energy_first_point_gc;
    for q in start..end {
        let gq = if first_gc { gc[start] } else { gc[q] };
        out[q - start] = density(d[q], &g[q * D..(q + 1) * D], gq);
    }
    Ok(())
}
