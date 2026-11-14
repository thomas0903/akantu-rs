// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/model/model_couplers/coupler_solid_phasefield.hh,
// coupler_solid_phasefield.cc; src/model/common/constitutive_laws/
// constitutive_laws_handler_tmpl.hh and constitutive_law_tmpl.hh
// (flattenInternal / inflateInternal, as used by the coupler).

//! [`CouplerSolidPhaseField`] (CPL-005…007): staggered coupling of a
//! [`SolidMechanicsModel`] and a [`PhaseFieldModel`] on the same mesh.
//!
//! # Structure (ARCHITECTURE, staggered style)
//!
//! Upstream builds the two models **without** a shared DOF manager: each
//! model owns its own `ModelSolver` (DOF manager, named solvers, time
//! steps), and the users initialise and configure each model directly
//! (`coupler.getSolidMechanicsModel().initFull(…)`). The port keeps exactly
//! that: the coupler owns `solid` and `phase` by value (public fields, the
//! borrowed handles of upstream `getSolidMechanicsModel()` /
//! `getPhaseFieldModel()`), and [`CouplerSolidPhaseField::solve`] runs one
//! staggered pass (upstream `solve`, `coupler_solid_phasefield.cc:410-425`):
//!
//! 1. `solid.solve_step(solid_id)`;
//! 2. strain transfer on the not-ghost elements (CPL-006,
//!    [`CouplerSolidPhaseField::compute_strain_on_quad_points`]);
//! 3. `phase.solve_step(phase_id)`;
//! 4. damage transfer, not-ghost then ghost elements (CPL-007,
//!    [`CouplerSolidPhaseField::compute_damage_on_quad_points`]);
//! 5. `solid.assemble_internal_forces()` (stresses with the new damage).
//!
//! There is no fixed-point iteration (upstream neither): converging the
//! alternate minimisation is the caller's loop, e.g. with
//! [`CouplerSolidPhaseField::check_convergence`].
//!
//! # Strain transfer (CPL-006)
//!
//! Upstream flattens the solid's internals `grad_u` and `epsilon_th` of
//! **every** material into mesh-indexed arrays, computes
//! `ε = (∇u + ∇uᵀ)/2 − ε_th·I` per quadrature point and inflates the result
//! into the `strain` internal of **every** phase-field law through that
//! law's own element filter — no name matching, correct even when the two
//! selectors assign elements differently. The port does the same with
//! `InternalField::flatten` / `InternalField::inflate` and the same
//! coefficient-wise expression (division by 2, then `− ε_th·δᵢⱼ`).
//! Elements not covered by any solid material keep a zero strain (upstream:
//! the flattened arrays are `malloc`ed, so such entries are indeterminate;
//! UD-618, category A).
//!
//! # Damage transfer (CPL-007, UD-382)
//!
//! For every material / phase-field pair **with the same name**, the nodal
//! damage of the phase field is interpolated on the quadrature points and
//! written into the material's `damage` internal and into the phase-field
//! law's own `damage` internal (upstream overwrites the latter so that it
//! matches the converged nodal damage before the next `savePreviousState`).
//!
//! * **Default (corrected, UD-382 category B):** each law receives the
//!   interpolation on **its own** elements (its element filter).
//! * **Compat** ([`CouplerSolidPhaseFieldCompat::unfiltered_damage_transfer`]):
//!   upstream interpolates on **every** element of the type (no filter):
//!   the phase-field law's `damage` block is resized to the mesh block, and
//!   the material's block receives the first entries of that mesh-indexed
//!   array (upstream copies the whole array and its loops read it
//!   positionally, so a material that does not start at the first element
//!   of the type reads another element's damage). With one law covering
//!   each type in mesh order (all upstream tests and examples but
//!   `test_multi_material`) both modes are bitwise identical.
//!
//! Upstream asserts equal numbers of materials and phase fields in debug
//! builds only; the port refuses unequal counts by default and skips the
//! check in compat mode (release behaviour, UD-619).
//!
//! # Not ported (unreachable upstream, UD-617)
//!
//! Upstream's coupler also derives from `Model`/`SolverCallback`
//! (`initFull`, `assembleResidual` over the `displacement` and `damage`
//! families of one DOF manager, `assembleMatrix`, …), but it never creates
//! a DOF manager and its `initFullImpl` dereferences null pointers
//! (`initBC(*this, "displacement", *displacement, …)`), so none of these
//! entry points can run. [`CouplerSolidPhaseField::init_full`] does the
//! evident intent (initialise both models with the same method); the
//! default-solver functions are kept as associated functions for API
//! parity. The coupler's dumper API (`setBaseName`, `addDumpField*`,
//! `dump`; CPL-005) acts on the default series upstream's `coupler.dump()`
//! writes (the mesh's last-registered default dumper), see
//! [`crate::dump`].
//!
//! # Parallel (MPI-046 / MPI-047)
//!
//! Upstream transfers the damage for ghost elements too (from the nodal
//! damage, ghost nodes included) and the strain for not-ghost elements
//! only (UD-004's second half: the ghost strain of the phase-field laws
//! stays at its default; their driving forces are overwritten by
//! `_pfm_damage` before the ghost assembly). The port loops over both ghost
//! types the same way. Upstream registers the element synchronizer for
//! `_csp_damage` and `_csp_strain` on distributed meshes but the coupler's
//! data accessor carries no data for them (the default
//! `DataAccessor<Element>`: zero bytes) and nothing synchronises them
//! (UD-901): [`CouplerSolidPhaseField::synchronize`] runs the (empty)
//! exchange for API parity; the transfers between the two models are
//! local to each rank (both models live on the same distributed mesh).

use akantu_core::array_shared::ArrayMut;
use akantu_core::enums::{
    AnalysisMethod, IntegrationSchemeType, NonLinearSolverType, TimeStepSolverType,
};
use akantu_core::{
    Array, ElementKind, ElementType, ElementTypeMapArray, Error, GhostType, Id, Idx, Real, Result,
    TypeFilter,
};
use akantu_mesh::{Mesh, SharedMesh};
use akantu_model::ModelOptions;
use akantu_model::constitutive_law::{ConstitutiveLaw, InternalFieldMut, InternalFieldRef};
use akantu_parser::Parser;
use akantu_phase_field::PhaseFieldModel;
use akantu_solid::SolidMechanicsModel;
use akantu_solver::{ModelSolverOptions, SolutionType};

/// Default coupler id (upstream `"coupler_solid_phasefield"`).
pub const DEFAULT_ID: &str = "coupler_solid_phasefield";

/// Id of the solid component (upstream `"solid_mechanics_model"`).
pub const SOLID_ID: &str = "solid_mechanics_model";

/// Id of the phase-field component (upstream `"phase_field_model"`).
pub const PHASE_ID: &str = "phase_field_model";

/// Convergence tolerance of upstream `checkConvergence` (hard-coded).
pub const CONVERGENCE_TOLERANCE: Real = 1e-8;

/// Switches reproducing upstream defects of the solid–phase-field coupler
/// (decision D-003; default: corrected behaviour).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CouplerSolidPhaseFieldCompat {
    /// **UD-382** (category B): interpolate the nodal damage on every
    /// element of the type (no element filter) and hand the material the
    /// first entries of that mesh-indexed array (see the module docs).
    pub unfiltered_damage_transfer: bool,
    /// **UD-619**: skip the equal-count check of materials and phase fields
    /// (upstream checks it with a debug assertion only).
    pub skip_law_count_check: bool,
    /// **UD-620** (category B): the convergence formula of upstream's
    /// (private, unused) `checkConvergence`, which never advances its damage
    /// iterators and does not square the differences. On a distributed mesh
    /// it is accepted only together with
    /// [`Self::per_rank_convergence_check`]: alone it
    /// would add each rank's own first local damage difference (a
    /// different node on every rank) over the ranks, which is neither
    /// upstream's expression nor the corrected one, so
    /// [`CouplerSolidPhaseField::check_convergence`] refuses it there.
    pub upstream_convergence_formula: bool,
    /// **UD-905** (category B): upstream's
    /// `checkConvergence` (`coupler_solid_phasefield.cc:430-466`) evaluates
    /// its norms over the **local** arrays of each rank (every local node,
    /// ghosts included, no reduction), so on a distributed mesh each rank
    /// gets its own answer. Default: the squared differences of the owned
    /// entries are summed over the ranks (rank-ordered, D-039), giving one
    /// answer on every rank. `true`: each rank evaluates upstream's local
    /// expression; the per-rank answers are then agreed — equal everywhere,
    /// that answer; different, an agreed error on every rank (a loop that
    /// branched on them would diverge; upstream's result is not defined
    /// there).
    pub per_rank_convergence_check: bool,
    /// **UD-553** (category B, as the solid model, MPI-053):
    /// upstream's `synchronize(tag)` for a tag without a registered
    /// synchronizer is a silent no-op. Default:
    /// [`CouplerSolidPhaseField::synchronize`] refuses such a tag on a
    /// distributed mesh; `true` does nothing, as upstream.
    pub unregistered_synchronization_tag_is_a_noop: bool,
}

impl CouplerSolidPhaseFieldCompat {
    /// Upstream behaviour for every switch.
    #[must_use]
    pub const fn upstream() -> Self {
        Self {
            unfiltered_damage_transfer: true,
            skip_law_count_check: true,
            upstream_convergence_formula: true,
            per_rank_convergence_check: true,
            unregistered_synchronization_tag_is_a_noop: true,
        }
    }
}

/// Staggered solid–phase-field coupler (upstream `CouplerSolidPhaseField`).
#[derive(Debug)]
pub struct CouplerSolidPhaseField {
    /// Coupler id (upstream `id`, default [`DEFAULT_ID`]).
    pub id: Id,
    /// The solid mechanics model (id [`SOLID_ID`], own DOF manager).
    pub solid: SolidMechanicsModel,
    /// The phase-field model (id [`PHASE_ID`], own DOF manager).
    pub phase: PhaseFieldModel,
    /// Upstream-defect switches of the coupler.
    pub compat: CouplerSolidPhaseFieldCompat,
    spatial_dimension: usize,
    /// The coupler's own dumper `coupler_solid_phasefield` (`dump.rs`).
    pub(crate) dumpable: akantu_io::dumper::Dumpable,
}

/// Number of quadrature points of `ty` for an FE engine (0 for a type the
/// engine does not know).
fn nb_quad(fe: &akantu_fe::FeEngine, ty: ElementType, ghost: GhostType) -> usize {
    fe.nb_integration_points(ty, ghost).unwrap_or(0)
}

/// Regular element types of dimension `dim` of `mesh` for `ghost`.
fn regular_types(mesh: &Mesh, dim: usize, ghost: GhostType) -> Vec<ElementType> {
    mesh.element_types(TypeFilter {
        spatial_dimension: Some(dim),
        ghost_type: ghost,
        kind: Some(ElementKind::Regular),
    })
    .collect()
}

/// Upstream `flattenInternal(name, _ek_regular, ghost)` over the laws of a
/// handler: scatter every law's internal `name` into mesh-indexed blocks
/// (zero-filled; laws without the internal are skipped).
///
/// The coupler uses the result at the default engine's points (as
/// upstream, which also assumes it); an internal registered on its own FE
/// engine or element filter (CL-001) is refused instead of being read on
/// the wrong support.
fn flatten_solid_internal(
    solid: &SolidMechanicsModel,
    mesh: &Mesh,
    name: &str,
    ghost: GhostType,
) -> Result<ElementTypeMapArray<Real>> {
    let fe = solid.physics.base.fems.get("")?;
    let mut out = ElementTypeMapArray::new(name);
    for law in solid.physics.materials.laws() {
        let filter = law.element_filter();
        let mut err: Option<Error> = None;
        law.for_each_internal(&mut |n, f| {
            if n != name || err.is_some() {
                return;
            }
            if f.has_own_support() {
                err = Some(Error::Unsupported(format!(
                    "coupler: internal '{name}' of law '{}' has its own FE engine or element \
                     filter (CL-001); the coupler reads it at the default engine's points",
                    law.name()
                )));
                return;
            }
            if let InternalFieldRef::Real(field) = f
                && let Err(e) = field.flatten(
                    filter,
                    ghost,
                    &|ty| mesh.nb_element(ty, ghost),
                    &|ty| nb_quad(fe, ty, ghost),
                    0.0,
                    &mut out,
                )
            {
                err = Some(e);
            }
        });
        if let Some(e) = err {
            return Err(e);
        }
    }
    Ok(out)
}

/// Mutable `Real` internal `name` of a law (`None` when the law has none).
fn with_real_internal<R>(
    law: &mut dyn ConstitutiveLaw,
    name: &str,
    f: impl FnOnce(&mut akantu_model::InternalField<Real>) -> Result<R>,
) -> Result<Option<R>> {
    let mut f = Some(f);
    let mut out: Option<R> = None;
    law.for_each_internal_mut(&mut |n, field| {
        if n != name {
            return Ok(());
        }
        if let (InternalFieldMut::Real(field), Some(f)) = (field, f.take()) {
            out = Some(f(field)?);
        }
        Ok(())
    })?;
    Ok(out)
}

impl CouplerSolidPhaseField {
    /// Upstream `CouplerSolidPhaseField(mesh, dim, id)` with an empty parser
    /// (see [`CouplerSolidPhaseField::with_parser`]).
    ///
    /// # Errors
    /// See [`CouplerSolidPhaseField::with_parser`].
    pub fn new(mesh: SharedMesh, spatial_dimension: Option<usize>) -> Result<Self> {
        Self::with_parser(mesh, spatial_dimension, DEFAULT_ID, Parser::default())
    }

    /// Upstream constructor (CPL-005): the solid model (id
    /// `solid_mechanics_model`) and the phase-field model (id
    /// `phase_field_model`) on the same mesh, each with its own DOF manager
    /// and the same input-file snapshot `parser` (each model reads its own
    /// `model <type> [ … ]` section, or the root).
    ///
    /// # Errors
    /// Construction errors of either model (dimension, mesh domain, model
    /// sections).
    pub fn with_parser(
        mesh: SharedMesh,
        spatial_dimension: Option<usize>,
        id: &str,
        parser: Parser,
    ) -> Result<Self> {
        let solid = SolidMechanicsModel::with_parser(
            mesh.clone(),
            spatial_dimension,
            SOLID_ID,
            parser.clone(),
        )?;
        let dim = solid.spatial_dimension();
        let phase = PhaseFieldModel::with_parser(mesh, Some(dim), PHASE_ID, parser)?;
        // upstream registers the coupler's dumper before the parts theirs
        // (`dump.rs`)
        let dumpable = crate::dump::coupler_dumpable(
            crate::dump::SOLID_PHASE_FIELD_DUMPER,
            id,
            dim,
            ElementKind::Regular,
        )?;
        let mut coupler = Self {
            id: id.to_owned(),
            solid,
            phase,
            compat: CouplerSolidPhaseFieldCompat::default(),
            spatial_dimension: dim,
            dumpable,
        };
        coupler.init_default_dump_series()?;
        Ok(coupler)
    }

    /// Spatial dimension of the coupler.
    #[must_use]
    pub fn spatial_dimension(&self) -> usize {
        self.spatial_dimension
    }

    /// Set the coupler's switches and the phase-field model's switches
    /// together: `upstream = true` reproduces the reference everywhere in
    /// the coupled phase-field path (coupler UD-382/619/620 and the
    /// phase-field model's UD-377…380).
    pub fn set_upstream_compat(&mut self, upstream: bool) {
        if upstream {
            self.compat = CouplerSolidPhaseFieldCompat::upstream();
            self.phase
                .set_upstream_compat(akantu_phase_field::PhaseFieldUpstreamCompat::upstream());
        } else {
            self.compat = CouplerSolidPhaseFieldCompat::default();
            self.phase
                .set_upstream_compat(akantu_phase_field::PhaseFieldUpstreamCompat::default());
        }
    }

    /// Upstream `initFull(options)` intent (UD-617): `solid.initFull` and
    /// `phase.initFull` with the same options. Upstream's coupler
    /// `initFullImpl` dereferences null pointers and would create a solver
    /// without a DOF manager; the upstream tests and examples initialise the
    /// two models directly, which remains the recommended use (different
    /// methods per model, e.g. explicit solid and static phase field).
    ///
    /// # Errors
    /// Initialisation errors of either model.
    pub fn init_full(&mut self, options: &ModelOptions) -> Result<()> {
        self.solid.init_full(options)?;
        self.phase.init_full(options)
    }

    /// Upstream `getDefaultSolverID(method)`.
    #[must_use]
    pub fn default_solver_id(method: AnalysisMethod) -> (Id, TimeStepSolverType) {
        match method {
            AnalysisMethod::ExplicitLumpedMass => {
                ("explicit_lumped".into(), TimeStepSolverType::DynamicLumped)
            }
            AnalysisMethod::ExplicitConsistentMass => {
                ("explicit".into(), TimeStepSolverType::Dynamic)
            }
            AnalysisMethod::Static => ("static".into(), TimeStepSolverType::Static),
            AnalysisMethod::ImplicitDynamic => ("implicit".into(), TimeStepSolverType::Dynamic),
            _ => ("unknown".into(), TimeStepSolverType::NotDefined),
        }
    }

    /// Upstream `getDefaultSolverType()`: `dynamic_lumped`.
    #[must_use]
    pub fn default_solver_type() -> TimeStepSolverType {
        TimeStepSolverType::DynamicLumped
    }

    /// Upstream `getDefaultSolverOptions(type)`: `dynamic_lumped` and
    /// `dynamic` → lumped non-linear solver, central difference on the
    /// accelerations of `displacement`; `static` → `linear`, pseudo-time;
    /// other types are an error.
    ///
    /// # Errors
    /// `not_defined` (upstream `AKANTU_EXCEPTION`).
    pub fn default_solver_options(ty: TimeStepSolverType) -> Result<ModelSolverOptions> {
        let mut o = ModelSolverOptions::default();
        let (nls, scheme, st) = match ty {
            TimeStepSolverType::DynamicLumped | TimeStepSolverType::Dynamic => (
                NonLinearSolverType::Lumped,
                IntegrationSchemeType::CentralDifference,
                SolutionType::Acceleration,
            ),
            TimeStepSolverType::Static => (
                NonLinearSolverType::Linear,
                IntegrationSchemeType::PseudoTime,
                SolutionType::NotDefined,
            ),
            TimeStepSolverType::NotDefined => {
                return Err(Error::invalid(format!(
                    "{ty} is not a valid time step solver type"
                )));
            }
        };
        o.non_linear_solver_type = Some(nls);
        o.integration_scheme_type
            .insert(akantu_solid::model::DOF_ID.into(), scheme);
        o.solution_type
            .insert(akantu_solid::model::DOF_ID.into(), st);
        Ok(o)
    }

    /// Upstream `isDefaultSolverExplicit()` of the solid component.
    #[must_use]
    pub fn is_default_solver_explicit(&self) -> bool {
        self.solid.analysis_method() == AnalysisMethod::ExplicitLumpedMass
    }

    /// One staggered pass (upstream `solve(solid_solver_id,
    /// phase_solver_id)`, `None` or `""` = the model's default solver):
    /// solid solve, strain transfer (not ghost), phase-field solve, damage
    /// transfer (not ghost, ghost), solid internal forces.
    ///
    /// # Errors
    /// Solver errors of either model, transfer errors (see
    /// [`CouplerSolidPhaseField::compute_strain_on_quad_points`] and
    /// [`CouplerSolidPhaseField::compute_damage_on_quad_points`]).
    pub fn solve(
        &mut self,
        solid_solver_id: Option<&str>,
        phase_solver_id: Option<&str>,
    ) -> Result<()> {
        self.solid.solve_step(solid_solver_id)?;
        self.compute_strain_on_quad_points(GhostType::NotGhost)?;
        self.phase.solve_step(phase_solver_id)?;
        self.compute_damage_on_quad_points(GhostType::NotGhost)?;
        self.compute_damage_on_quad_points(GhostType::Ghost)?;
        self.solid.assemble_internal_forces()
    }

    /// Upstream `computeStrainOnQuadPoints(ghost)` (CPL-006): flatten the
    /// solid's `grad_u` and `epsilon_th`, `ε = (∇u + ∇uᵀ)/2 − ε_th·I` per
    /// quadrature point, inflate into the `strain` internal of every
    /// phase-field law (through its own filter).
    ///
    /// # Errors
    /// A type with solid elements but no material defining `grad_u` /
    /// `epsilon_th` (upstream throws on the missing block), shape
    /// inconsistencies of user-modified internals.
    pub fn compute_strain_on_quad_points(&mut self, ghost: GhostType) -> Result<()> {
        let dim = self.spatial_dimension;
        let d2 = dim * dim;
        let mesh_h = self.solid.physics.base.mesh.clone();
        let mesh = mesh_h.read()?;
        let grad_u = flatten_solid_internal(&self.solid, &mesh, "grad_u", ghost)?;
        let eps_th = flatten_solid_internal(&self.solid, &mesh, "epsilon_th", ghost)?;
        let fe = self.solid.physics.base.fems.get("")?;
        let mut strain = ElementTypeMapArray::<Real>::new("temporary strain on quads");
        for ty in regular_types(&mesh, dim, ghost) {
            let nq = nb_quad(fe, ty, ghost);
            let n = akantu_core::checked_len(mesh.nb_element(ty, ghost), nq, "strain on quads")?;
            let missing = |what: &str| {
                Error::invalid(format!(
                    "computeStrainOnQuadPoints: no solid material defines '{what}' on {ty} ({ghost})"
                ))
            };
            let (Ok(gu), Ok(et)) = (grad_u.get(ty, ghost), eps_th.get(ty, ghost)) else {
                if n == 0 {
                    continue;
                }
                return Err(missing(if grad_u.exists(ty, ghost) {
                    "epsilon_th"
                } else {
                    "grad_u"
                }));
            };
            if gu.len() != n || gu.nb_component() != d2 || et.len() != n {
                return Err(Error::DimensionMismatch {
                    context: "computeStrainOnQuadPoints: flattened internals",
                    expected: n,
                    got: gu.len().min(et.len()),
                });
            }
            let out = strain.alloc(n, d2, ty, ghost, 0.0)?;
            let (g, e, s) = (gu.as_slice(), et.as_slice(), out.as_mut_slice());
            for q in 0..n {
                let gq = &g[q * d2..(q + 1) * d2];
                let sq = &mut s[q * d2..(q + 1) * d2];
                let th = e[q];
                for j in 0..dim {
                    for i in 0..dim {
                        let delta: Real = if i == j { 1.0 } else { 0.0 };
                        // Eigen: (grad_u + grad_u.transpose()) / 2. − ε_th·I,
                        // coefficient-wise
                        sq[i + j * dim] = (gq[i + j * dim] + gq[j + i * dim]) / 2. - th * delta;
                    }
                }
            }
        }
        drop(mesh);
        // phase->inflateInternal("strain", strain_tmp, ghost, _ek_regular)
        let phys = &mut self.phase.physics;
        let pfe = phys.base.fems.get("")?;
        for law in phys.phasefields.laws_mut() {
            let b = law.base_mut();
            b.strain.inflate(
                &b.element_filter,
                ghost,
                &|ty| nb_quad(pfe, ty, ghost),
                &strain,
            )?;
        }
        Ok(())
    }

    /// Upstream `computeDamageOnQuadPoints(ghost)` (CPL-007, UD-382): for
    /// every material / phase-field pair with the same name, the nodal
    /// damage interpolated on the quadrature points is written into the
    /// material's `damage` and the phase-field law's `damage` internals
    /// (see the module docs for the corrected and compat modes).
    ///
    /// # Errors
    /// Unequal numbers of materials and phase fields (default mode), a
    /// paired material without a `damage` internal (upstream casts it to
    /// `MaterialDamage` unchecked), FE engine errors.
    pub fn compute_damage_on_quad_points(&mut self, ghost: GhostType) -> Result<()> {
        let nb_materials = self.solid.nb_materials();
        let nb_phasefields = self.phase.nb_phase_fields();
        if !self.compat.skip_law_count_check && nb_materials != nb_phasefields {
            return Err(Error::invalid(format!(
                "The number of phasefields and materials should be equal ({nb_phasefields} \
                 phase fields, {nb_materials} materials)"
            )));
        }
        let dim = self.spatial_dimension;
        let unfiltered = self.compat.unfiltered_damage_transfer;
        let mesh_h = self.phase.physics.base.mesh.clone();
        let mesh = mesh_h.read()?;
        let types = regular_types(&mesh, dim, ghost);
        let phase = &mut self.phase;
        let fe = phase.physics.base.fems.get("")?;
        let damage = phase.solver.dof_manager.dofs(akantu_phase_field::DOF_ID)?;
        let mut buffer = Array::<Real>::new(1);
        for mi in 0..nb_materials {
            let material = self.solid.physics.materials.law_mut(mi)?;
            let mat_name = material.name().to_owned();
            for pi in 0..nb_phasefields {
                let law = phase.physics.phasefields.law_mut(pi)?;
                if law.name() != mat_name {
                    continue;
                }
                for &ty in &types {
                    if unfiltered {
                        // upstream: interpolate on every element of the
                        // type into the phase field's damage, then
                        // damage_on_qpoints_vect.copy(phase_damage)
                        let b = law.base_mut();
                        let Ok(mut pd) = b.damage.get_mut(ty, ghost) else {
                            continue;
                        };
                        // the FE engine writes into a plain array (P10)
                        let mut interpolated = Array::<Real>::new(pd.nb_component());
                        fe.interpolate_on_integration_points(
                            &mesh,
                            damage.view(),
                            &mut interpolated,
                            1,
                            ty,
                            ghost,
                            None,
                        )?;
                        pd.copy_from(&interpolated)?;
                        let full = pd.as_slice();
                        write_material_damage(material, ty, ghost, &mat_name, |md| {
                            let n = md.as_slice().len();
                            let src = full.get(..n).ok_or(Error::DimensionMismatch {
                                context: "computeDamageOnQuadPoints: material damage",
                                expected: n,
                                got: full.len(),
                            })?;
                            md.as_mut_slice().copy_from_slice(src);
                            Ok(())
                        })?;
                    } else {
                        // corrected: each law receives the interpolation on
                        // its own elements
                        let b = law.base_mut();
                        if let Ok(mut pd) = b.damage.get_mut(ty, ghost) {
                            let filter = b.element_filter.get(ty, ghost).map(Array::as_slice);
                            let filter: &[Idx] = filter.unwrap_or(&[]);
                            // the FE engine writes into a plain array (P10)
                            let mut interpolated = Array::<Real>::new(pd.nb_component());
                            fe.interpolate_on_integration_points(
                                &mesh,
                                damage.view(),
                                &mut interpolated,
                                1,
                                ty,
                                ghost,
                                Some(filter),
                            )?;
                            pd.copy_from(&interpolated)?;
                        }
                        let mfilter: Vec<Idx> = material
                            .element_filter()
                            .get(ty, ghost)
                            .map(|a| a.as_slice().to_vec())
                            .unwrap_or_default();
                        fe.interpolate_on_integration_points(
                            &mesh,
                            damage.view(),
                            &mut buffer,
                            1,
                            ty,
                            ghost,
                            Some(&mfilter),
                        )?;
                        let src = buffer.as_slice();
                        write_material_damage(material, ty, ghost, &mat_name, |md| {
                            if md.as_slice().len() != src.len() {
                                return Err(Error::DimensionMismatch {
                                    context: "computeDamageOnQuadPoints: material damage",
                                    expected: src.len(),
                                    got: md.as_slice().len(),
                                });
                            }
                            md.as_mut_slice().copy_from_slice(src);
                            Ok(())
                        })?;
                    }
                }
            }
        }
        Ok(())
    }

    /// Upstream `SynchronizerRegistry::synchronize(tag)` of the coupler
    /// (MPI-046 / MPI-047): `_csp_damage` and `_csp_strain` are registered
    /// on the mesh's element synchronizer, and the coupler's data accessor
    /// (upstream's default `DataAccessor<Element>`) carries no data for
    /// them: one collective exchange of empty messages that changes nothing
    /// (UD-901). Nothing on a mesh without element synchronizer (serial).
    /// Another tag is an error on a distributed mesh (UD-553; with
    /// [`CouplerSolidPhaseFieldCompat::unregistered_synchronization_tag_is_a_noop`] nothing
    /// happens, as upstream).
    ///
    /// # Errors
    /// An unregistered tag; an agreed failure of the exchange.
    pub fn synchronize(&mut self, tag: akantu_core::enums::SynchronizationTag) -> Result<()> {
        use akantu_core::enums::SynchronizationTag as T;
        let mesh_h = self.phase.physics.base.mesh.clone();
        let mesh = mesh_h.read()?;
        let Some(es) = &mesh.synchronizers().element else {
            return Ok(());
        };
        match tag {
            T::CspDamage | T::CspStrain => es.synchronize(&mut NoCouplerData, tag),
            _ if self.compat.unregistered_synchronization_tag_is_a_noop => Ok(()),
            other => Err(Error::invalid(format!(
                "CouplerSolidPhaseField::synchronize: tag {other} is not registered by the coupler"
            ))),
        }
    }

    /// Convergence test of a staggered iteration (upstream private
    /// `checkConvergence`, unused upstream): `max(‖u_new − u_old‖₂ over the
    /// solid's free DOFs, ‖d_new − d_old‖₂) < 1e-8`.
    ///
    /// **Collective** (every rank calls it; one operation of the solid
    /// model's DOF manager): on a distributed mesh the sums of squares run
    /// over the entries this rank owns (nodes neither slave nor pure ghost,
    /// so that every shared node counts once) and are summed over the ranks
    /// in rank order (D-039) before the square roots, so every rank gets the
    /// same answer (UD-905): a staggered loop that stops
    /// on it stops on every rank together. On a mesh without a node
    /// synchronizer every entry counts and the two sums are still added over
    /// the communicator's ranks: with one rank (a serial mesh) the value is
    /// unchanged; on a mesh that was never distributed while the
    /// communicator has several ranks (every rank holding the whole mesh,
    /// which no in-tree flow builds) every rank counts all of its entries,
    /// so for equal arrays on `P` ranks the norms are `√P ×` the serial ones
    /// and the answer can differ from upstream's (each upstream process
    /// evaluates its own arrays, no reduction); every rank still gets the
    /// same answer. The comparison with the hard-coded
    /// tolerance is unchanged.
    ///
    /// With [`CouplerSolidPhaseFieldCompat::upstream_convergence_formula`]
    /// the damage term is upstream's: `sqrt(n·(d_new[0] − d_old[0]))`
    /// (iterators never advanced, differences not squared; a negative sum
    /// gives NaN, which `std::max(norm, NaN)` discards) — UD-620. On a
    /// distributed mesh that switch is accepted only together with
    /// [`CouplerSolidPhaseFieldCompat::per_rank_convergence_check`], with
    /// which each rank evaluates upstream's expression on its whole local
    /// arrays and the per-rank answers are agreed (UD-905); alone it is
    /// refused (`d[0]` is a different node on every
    /// rank, so its sum over the ranks has no meaning).
    ///
    /// # Errors
    /// Arrays of different sizes, blocked flags that do not match the
    /// displacement size, a node count that does not match the mesh on a
    /// distributed mesh, `upstream_convergence_formula` without
    /// `per_rank_convergence_check` on a distributed mesh (all agreed: every
    /// rank fails, the others with "failed on another rank"); with the
    /// per-rank switch, ranks whose answers differ (on every rank).
    pub fn check_convergence(
        &mut self,
        u_new: &Array<Real>,
        u_old: &Array<Real>,
        d_new: &Array<Real>,
        d_old: &Array<Real>,
    ) -> Result<bool> {
        let compat = self.compat;
        // Local part; a failure is agreed inside the operation below (never
        // returned before it: the other ranks would wait in the exchange).
        let local = (|| -> Result<Partial> {
            let blocked = self.solid.blocked_dofs()?;
            let (un, uo, bl) = (u_new.as_slice(), u_old.as_slice(), blocked.as_slice());
            if un.len() != uo.len() || un.len() != bl.len() {
                return Err(Error::DimensionMismatch {
                    context: "checkConvergence: displacement",
                    expected: un.len(),
                    got: uo.len().min(bl.len()),
                });
            }
            let (dn, dd) = (d_new.as_slice(), d_old.as_slice());
            if dn.len() != dd.len() {
                return Err(Error::DimensionMismatch {
                    context: "checkConvergence: damage",
                    expected: dn.len(),
                    got: dd.len(),
                });
            }
            if compat.per_rank_convergence_check {
                let error =
                    convergence_error(un, uo, bl, dn, dd, compat.upstream_convergence_formula);
                return Ok(Partial::Answer(error < CONVERGENCE_TOLERANCE));
            }
            let mesh_h = self.solid.physics.base.mesh.clone();
            let mesh = mesh_h.read()?;
            let owned: Option<Vec<bool>> = if mesh.synchronizers().node.is_some() {
                let nb = mesh.nb_nodes();
                if dn.len() != nb || un.len() % nb.max(1) != 0 {
                    return Err(Error::DimensionMismatch {
                        context: "checkConvergence: nodal arrays on a distributed mesh",
                        expected: nb,
                        got: dn.len(),
                    });
                }
                Some(
                    (0..nb)
                        .map(|n| {
                            let f = mesh.node_flag(n as Idx);
                            !(f.is_slave() || f.is_pure_ghost())
                        })
                        .collect(),
                )
            } else {
                None
            };
            if owned.is_some() && compat.upstream_convergence_formula {
                return Err(Error::invalid(
                    "CouplerSolidPhaseField::check_convergence: \
                     upstream_convergence_formula (UD-620) without per_rank_convergence_check \
                     (UD-905) has no meaning on a distributed mesh (each rank's first local \
                     damage entry is a different node); set both switches together",
                ));
            }
            Ok(Partial::Sums(convergence_partial_sums(
                un,
                uo,
                bl,
                dn,
                dd,
                owned.as_deref(),
                compat.upstream_convergence_formula,
            )))
        })();
        self.solid
            .solver
            .dof_manager
            .collective("coupler:check_convergence", |dm| {
                let mut ph = dm.phases()?;
                match local {
                    Ok(Partial::Answer(converged)) => {
                        let (count, size) = ph.run_collective(
                            "coupler:convergence_answers",
                            Ok(u64::from(converged)),
                            |c, v| {
                                let mut s = [v];
                                c.all_reduce(&mut s, akantu_comm::ReduceOp::Sum)?;
                                Ok((s[0], c.size() as u64))
                            },
                        )?;
                        if count == 0 || count == size {
                            Ok(count == size)
                        } else {
                            Err(Error::invalid(format!(
                                "CouplerSolidPhaseField::check_convergence (per-rank upstream \
                                 evaluation, UD-905): {count} of {size} ranks converged; \
                                 upstream's answer differs between the ranks"
                            )))
                        }
                    }
                    other => {
                        let sums = ph.run_collective(
                            "coupler:convergence_sums",
                            other.and_then(|p| match p {
                                Partial::Sums(s) => Ok(s),
                                Partial::Answer(_) => {
                                    Err(Error::invalid("check_convergence: mixed modes"))
                                }
                            }),
                            |c, mut s| {
                                c.all_reduce(&mut s, akantu_comm::ReduceOp::Sum)?;
                                Ok(s)
                            },
                        )?;
                        Ok(convergence_error_of_sums(sums) < CONVERGENCE_TOLERANCE)
                    }
                }
            })
    }
}

/// Local part of [`CouplerSolidPhaseField::check_convergence`].
enum Partial {
    /// Sums `[Σ Δu², Σ Δd²]` (or upstream's damage sum) over the owned
    /// entries, to be summed over the ranks.
    Sums([Real; 2]),
    /// This rank's own answer (per-rank compat, UD-905).
    Answer(bool),
}

/// The coupler's data accessor (upstream's default `DataAccessor<Element>`
/// methods: no data for any tag).
struct NoCouplerData;

impl akantu_comm::DataAccessor<akantu_core::Element> for NoCouplerData {
    fn nb_data(
        &self,
        _elements: &[akantu_core::Element],
        _tag: akantu_core::enums::SynchronizationTag,
    ) -> usize {
        0
    }
    fn pack_data(
        &self,
        _buffer: &mut akantu_comm::buffer::CommunicationBuffer,
        _elements: &[akantu_core::Element],
        _tag: akantu_core::enums::SynchronizationTag,
    ) -> Result<()> {
        Ok(())
    }
    fn unpack_data(
        &mut self,
        _buffer: &mut akantu_comm::buffer::CommunicationBuffer,
        _elements: &[akantu_core::Element],
        _tag: akantu_core::enums::SynchronizationTag,
    ) -> Result<()> {
        Ok(())
    }
}

/// Write the `damage` internal block `(ty, ghost)` of a material through
/// `f`; a material without a `damage` internal is refused (upstream
/// `static_cast<MaterialDamage<dim> &>` of any material).
fn write_material_damage(
    material: &mut dyn akantu_solid::Material,
    ty: ElementType,
    ghost: GhostType,
    name: &str,
    f: impl FnOnce(&mut ArrayMut<'_, Real>) -> Result<()>,
) -> Result<()> {
    let r = with_real_internal(material, "damage", |field| {
        match field.get_mut(ty, ghost) {
            Ok(mut block) => f(&mut block),
            // no block of this type: nothing of the material to write
            Err(_) => Ok(()),
        }
    })?;
    r.ok_or_else(|| {
        Error::invalid(format!(
            "computeDamageOnQuadPoints: material '{name}' has no 'damage' internal (upstream \
             requires a MaterialDamage, e.g. the phasefield material)"
        ))
    })
}

/// The error measure of [`CouplerSolidPhaseField::check_convergence`] on
/// whole local arrays (a serial mesh, or one rank's upstream expression).
fn convergence_error(
    u_new: &[Real],
    u_old: &[Real],
    blocked: &[bool],
    d_new: &[Real],
    d_old: &[Real],
    upstream: bool,
) -> Real {
    convergence_error_of_sums(convergence_partial_sums(
        u_new, u_old, blocked, d_new, d_old, None, upstream,
    ))
}

/// `[Σ (u_new − u_old)² over the free DOFs, Σ (d_new − d_old)²]` over the
/// nodes with `owned[n]` (every node with `None`), or with `upstream` the
/// damage term of upstream's loop (`d_new[0] − d_old[0]` added once per
/// counted entry: its iterators never advance, UD-620; only with `None`:
/// the caller refuses it with `owned`). The displacement
/// arrays hold `u_new.len() / owned.len()` components per node.
fn convergence_partial_sums(
    u_new: &[Real],
    u_old: &[Real],
    blocked: &[bool],
    d_new: &[Real],
    d_old: &[Real],
    owned: Option<&[bool]>,
    upstream: bool,
) -> [Real; 2] {
    let counts = |i: usize, per_node: usize| owned.is_none_or(|o| o[i / per_node.max(1)]);
    let per_node_u = owned.map_or(1, |o| u_new.len() / o.len().max(1));
    let mut norm = 0.0;
    for (i, ((a, b), bl)) in u_new.iter().zip(u_old).zip(blocked).enumerate() {
        if !*bl && counts(i, per_node_u) {
            norm += (a - b) * (a - b);
        }
    }
    let mut norm2 = 0.0;
    if upstream {
        // `for (i < n) norm2 += (*d_n_it - *d_o_it);` without advancing
        if let (Some(a), Some(b)) = (d_new.first(), d_old.first()) {
            for i in 0..d_new.len() {
                if counts(i, 1) {
                    norm2 += a - b;
                }
            }
        }
    } else {
        for (i, (a, b)) in d_new.iter().zip(d_old).enumerate() {
            if counts(i, 1) {
                norm2 += (a - b) * (a - b);
            }
        }
    }
    [norm, norm2]
}

/// `std::max(sqrt(sums[0]), sqrt(sums[1]))` with upstream's `std::max`
/// semantics: returns the first unless it is smaller than the second (a NaN
/// second term is discarded).
fn convergence_error_of_sums(sums: [Real; 2]) -> Real {
    let norm = sums[0].sqrt();
    let norm2 = sums[1].sqrt();
    if norm < norm2 { norm2 } else { norm }
}

#[cfg(test)]
mod tests {
    use super::convergence_error;

    #[test]
    fn convergence_error_corrected_and_upstream() {
        let u_new = [1.0, 2.0, 3.0];
        let u_old = [1.0, 2.0, 2.0];
        let bl = [false, true, false];
        // corrected: ‖Δd‖ = sqrt(0.09 + 0.16) = 0.5 > ‖Δu‖ = 1? no: ‖Δu‖ = 1
        let d_new = [0.3, 0.4];
        let d_old = [0.0, 0.0];
        assert_eq!(
            convergence_error(&u_new, &u_old, &bl, &d_new, &d_old, false),
            1.0
        );
        let d_new = [3.0, 4.0];
        assert_eq!(
            convergence_error(&u_new, &u_old, &bl, &d_new, &d_old, false),
            5.0
        );
        // upstream: sqrt(2·(3 − 0)) = sqrt(6)
        assert_eq!(
            convergence_error(&u_new, &u_old, &bl, &d_new, &d_old, true),
            6.0f64.sqrt()
        );
        // upstream: a negative first difference gives NaN, discarded by
        // std::max(norm, NaN)
        let d_new = [-1.0, 100.0];
        assert_eq!(
            convergence_error(&u_new, &u_old, &bl, &d_new, &d_old, true),
            1.0
        );
        // corrected sees the large second difference
        assert!(convergence_error(&u_new, &u_old, &bl, &d_new, &d_old, false) > 100.0);
    }
}
