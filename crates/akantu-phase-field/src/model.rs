// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/model/phase_field/phase_field_model.hh,
// phase_field_model.cc, phase_field_model_inline_impl.hh; src/model/common/
// constitutive_laws/constitutive_laws_handler{.hh,_tmpl.hh} (initFullImpl,
// initConstitutiveLaws); src/model/model.cc (initFullImpl, initNewSolver).

//! [`PhaseFieldModel`] (PF-001…004, PF-011, PF-012).
//!
//! # Structure (D-021)
//!
//! `PhaseFieldModel { physics, solver, token }`: the physics part
//! ([`PhaseFieldPhysics`]: model base, phase-field laws, `external_force`,
//! `internal_force`) implements [`SolverCallback`]; the [`ModelSolver`]
//! owns the DOF manager (`<id>:dof_manager_default`) and the time-step
//! solvers. The DOF family `damage` — nodal damage, blocked flags, previous
//! damage — lives in the DOF manager (upstream registers the model's arrays
//! by reference).
//!
//! # Solve sequence (default solver `static`, Newton–Raphson, pseudo-time)
//!
//! `beforeSolveStep` → every law `computeAllDrivingForces` (not ghost, then
//! ghost); the solver assembles `K` ([`PhaseFieldModel::assemble_stiffness_matrix`])
//! and the residual (`f_ext + f_int`, the driving forces recomputed from
//! the current damage), solves, corrects `d += δ` on free DOFs, iterates;
//! `afterSolveStep(converged)` → every law's `afterSolveStep` (no-op).
//! History (`damage`, `phi`) is saved only by an explicit
//! [`PhaseFieldModel::save_previous_state`] (upstream `savePreviousState`).
//!
//! # Parallel (MPI-044/045)
//!
//! On a distributed mesh (an element synchronizer) the law initialisation
//! is one agreed collective scope (selection, commit, the
//! `_constitutive_law_id` exchange that gives the ghost elements their
//! owner's law, every law's `initPhaseField`); every residual assembly
//! exchanges `_pfm_damage` (the laws' driving force, driving energy and
//! damage of the ghost elements' points, owner → ghost) through the running
//! operation's scope between the not-ghost driving forces and the ghost
//! assembly, as upstream's asynchronous exchange; energies are rank-ordered
//! all-reduces (D-039); [`PhaseFieldModel::synchronize`] /
//! [`PhaseFieldModel::synchronize_nodal`] expose the registered tags. Mesh
//! element events on a distributed mesh are refused before the commit (the
//! law-id exchange of new ghost elements is not ported for this model).
//!
//! # Coupling
//!
//! The staggered solid–phase-field coupler (CPL-005…007) needs: the strain
//! of the solid (flattened over all materials, mesh-indexed) inflated into
//! [`crate::PhaseFieldBase::strain`] of **every** law through the law's own
//! element filter (`InternalField::inflate`,
//! [`PhaseFieldModel::phase_field_mut`]; upstream `inflateInternal`, no name
//! matching), and the nodal damage ([`PhaseFieldModel::damage`])
//! interpolated on the solid's points with [`PhaseFieldModel::fe_engine`]
//! for the material whose name equals the law's (damage path only). See the
//! crate documentation.

mod assembly;
mod io;
mod parallel;
mod services;

use akantu_comm::Communicator;
use akantu_comm::phase::{Phases, Severity};
use akantu_core::array_shared::{ArrayMut, ArrayRef};
use akantu_core::enums::{
    AnalysisMethod, EventPriority, IntegrationSchemeType, MatrixType, ModelType,
    NonLinearSolverType, TimeStepSolverType,
};
use akantu_core::error::if_present;
use akantu_core::{
    Array, Element, ElementTypeMapArray, Error, GhostType, Id, Idx, Real, Result, SharedArray,
    SharedElementTypeMapArray,
};
use akantu_fe::FeEngine;
use akantu_mesh::{ConsumerToken, Mesh, MeshEvent, MeshEventHandler, MeshPart, SharedMesh};
use akantu_model::boundary_condition::{
    DirichletFunctor, NeumannFunctor, apply_dirichlet, apply_neumann, group_error,
};
use akantu_model::constitutive_law::{
    ConstitutiveLawSelector, LawContext, LawPhase, SelectorChain,
};
use akantu_model::model::{
    ModelBase, alloc_nodal_field, alloc_nodal_shared_field, init_new_solver,
};
use akantu_model::{ConstitutiveLawsHandler, Model, ModelOptions};
use akantu_parser::{Parser, ParserType};
use akantu_solver::{DofManager, ModelSolver, ModelSolverOptions, SolutionType, SolverCallback};

use crate::laws::{PhaseFieldFactory, register_builtin_phasefields};
use crate::phasefield::{PhaseField, PhaseFieldUpstreamCompat};

pub use io::{PhaseFieldDumpers, phase_field_field};
pub(crate) use services::PfServices;

/// Id of the DOF family of the model.
pub const DOF_ID: &str = "damage";

/// Default model id (upstream `"phase_field_model"`).
pub const DEFAULT_ID: &str = "phase_field_model";

/// Name of the FE engine (upstream `"PhaseFieldFEEngine"`).
pub const FE_ENGINE_NAME: &str = "PhaseFieldFEEngine";

/// Physics part of the phase-field model: everything except the solver.
#[derive(Debug)]
pub struct PhaseFieldPhysics {
    /// Model base (mesh handle, id, dimension, method, FE engines, parser).
    pub base: ModelBase,
    /// Phase-field laws (upstream `ConstitutiveLawsHandler<PhaseField>`).
    pub phasefields: ConstitutiveLawsHandler<dyn PhaseField>,
    /// Law factory (built-in laws registered; upstream's process-global
    /// `PhaseFieldFactory`).
    pub factory: PhaseFieldFactory,
    /// Upstream-defect switches applied to every law (default: corrected);
    /// set with [`PhaseFieldModel::set_upstream_compat`] only.
    pub(crate) upstream_compat: PhaseFieldUpstreamCompat,
    external_force: Option<Array<Real>>,
    /// Exportable (the Python `getInternalForce()` view, D-078, PY-091).
    internal_force: Option<SharedArray<Real>>,
    time_step: Real,
}

/// Phase-field model (upstream `PhaseFieldModel`).
#[derive(Debug)]
pub struct PhaseFieldModel {
    /// Physics (callback).
    pub physics: PhaseFieldPhysics,
    /// Solver.
    pub solver: ModelSolver,
    /// Mesh-consumer registration.
    pub token: ConsumerToken,
    /// Dumpers of the model (upstream: registered on the mesh, ParaView
    /// dumper `phase_field`).
    pub dumpers: PhaseFieldDumpers,
}

/// Shape check of a nodal array read by a model loop (the accessors hand
/// out `&mut Array`, so users can resize them).
pub(crate) fn check_nodal<T>(
    context: &'static str,
    a: &Array<T>,
    nb_nodes: usize,
    nb_component: usize,
) -> Result<()> {
    if a.len() != nb_nodes {
        return Err(Error::DimensionMismatch {
            context,
            expected: nb_nodes,
            got: a.len(),
        });
    }
    if a.nb_component() != nb_component {
        return Err(Error::DimensionMismatch {
            context,
            expected: nb_component,
            got: a.nb_component(),
        });
    }
    Ok(())
}

pub(crate) fn not_allocated(name: &str) -> Error {
    Error::not_found(
        "nodal field",
        format!("{name} (not allocated: call init_full first)"),
    )
}

impl PhaseFieldModel {
    /// Upstream `PhaseFieldModel(mesh, dim, id)` with an empty parser (see
    /// [`PhaseFieldModel::with_parser`]).
    pub fn new(mesh: SharedMesh, spatial_dimension: Option<usize>, id: &str) -> Result<Self> {
        Self::with_parser(mesh, spatial_dimension, id, Parser::default())
    }

    /// Upstream constructor (PF-001) with an explicit input-file snapshot:
    /// DOF manager `<id>:dof_manager_default` (solvers and schemes of the
    /// model's `model phase_field_model [ … ]` section), Lagrange FE engine
    /// `PhaseFieldFEEngine` on the elements of the model dimension, law
    /// registry (parser type `phasefield`) with `constitutive_law_index =
    /// -1` everywhere.
    ///
    /// # Errors
    /// An unusable mesh domain, a bad dimension, an invalid model section,
    /// or a mesh with ghost elements but no element synchronizer (not a
    /// distributed mesh).
    pub fn with_parser(
        mesh: SharedMesh,
        spatial_dimension: Option<usize>,
        id: &str,
        parser: Parser,
    ) -> Result<Self> {
        let mut base = ModelBase::new(
            mesh.clone(),
            ModelType::PhaseFieldModel,
            spatial_dimension,
            id,
        )?;
        base.parser = parser;
        let (phasefields, comm) = {
            let m = mesh.read()?;
            let has_ghosts = m
                .element_types(akantu_core::TypeFilter {
                    spatial_dimension: None,
                    ghost_type: GhostType::Ghost,
                    kind: None,
                })
                .any(|t| m.nb_element(t, GhostType::Ghost) > 0);
            // distributed: a mesh with ghosts must be a distributed one
            if has_ghosts && m.synchronizers().element.is_none() {
                return Err(Error::Unsupported(
                    "PhaseFieldModel: a mesh with ghost elements but no element synchronizer \
                     (not a distributed mesh)"
                        .into(),
                ));
            }
            (
                ConstitutiveLawsHandler::new(
                    id,
                    &m,
                    base.spatial_dimension,
                    ParserType::Phasefield,
                )?,
                m.communicator().clone(),
            )
        };
        let token = mesh.register_multilevel_consumer(
            id,
            &[
                EventPriority::FE_ENGINE,
                EventPriority::DOF_MANAGER,
                EventPriority::MODEL,
            ],
            MeshPart::Main,
        )?;
        let mut factory = PhaseFieldFactory::default();
        register_builtin_phasefields(&mut factory)?;
        let mut physics = PhaseFieldPhysics {
            base,
            phasefields,
            factory,
            upstream_compat: PhaseFieldUpstreamCompat::default(),
            external_force: None,
            internal_force: None,
            time_step: 0.0,
        };
        let solver = Self::new_solver(&mut physics, id, &comm)?;
        {
            let m = mesh.read()?;
            let dim = physics.base.spatial_dimension;
            physics.base.fems.register(FE_ENGINE_NAME, &m, dim)?;
        }
        physics.time_step = solver.time_step(None).unwrap_or(0.0);
        let dumpers = PhaseFieldDumpers::new(id, physics.base.spatial_dimension)?;
        Ok(Self {
            physics,
            solver,
            token,
            dumpers,
        })
    }

    /// Upstream `initDOFManager(dof_manager)` of the constructor
    /// (`phase_field_model.cc:51`, `model_solver.cc:78-95`): without a model
    /// section the `default` DOF manager; with one, the manager of its
    /// `dof_manager_type` (default `default`; `petsc` with the opt-in
    /// `petsc` feature, see [`PhaseFieldModel::init_dof_manager`]) and the
    /// solvers / schemes of the section.
    fn new_solver(
        physics: &mut PhaseFieldPhysics,
        id: &str,
        comm: &Communicator,
    ) -> Result<ModelSolver> {
        let section = physics.base.parser_section().cloned();
        let ty = match &section {
            Some(s) => s
                .get_parameter("dof_manager_type", akantu_parser::SearchScope::Current)
                .map(|p| p.to_word().to_string_lossy().into_owned())
                .unwrap_or_else(|_| "default".to_owned()),
            None => "default".to_owned(),
        };
        let dof_manager = akantu_solver::model_solver::new_dof_manager(&ty, id, comm)?;
        let mut solver = ModelSolver::new(dof_manager);
        if let Some(section) = section {
            // upstream's global `Parser::isPermissive()` for the
            // `non_linear_solver` sub-section
            let permissive = physics.base.parser.is_permissive();
            solver.init_from_section(&section, physics, permissive)?;
        }
        Ok(solver)
    }

    /// Upstream `ModelSolver::initDOFManager(dof_manager_type)`
    /// (`model_solver.cc:98-121`; `test_phase_field_optimal_linear.cc:58`,
    /// `test_phase_field_tao.cc:53`): a new DOF manager of that type,
    /// `<id>:dof_manager_<type>`, replaces the model's. `"petsc"` needs the
    /// opt-in `petsc` feature of this crate (a build with it is distributed
    /// under GPL-3.0-or-later as a whole, D-076) and, on a serial mesh, a
    /// job of one MPI process (the manager runs on the MPI world, as
    /// upstream's: [`akantu_solver::model_solver::dof_manager_communicator`]).
    /// Call it right after constructing the model, before
    /// [`PhaseFieldModel::init_full`]; the `petsc_tao` solver of the PETSc
    /// manager is then selected with
    /// `ModelSolverOptions { non_linear_solver_type: PetscTao,
    /// sparse_solver_type: Petsc }` and bounded on the damage increment with
    /// `NativeNonLinear::set_bounds` (upstream `setBounds`; the vectors built
    /// with `DofManager::assemble_to_global_array`, as upstream's tests).
    ///
    /// # Errors
    /// An unknown type (upstream's message), the PETSc conditions above, or
    /// a solver / DOF family that already exists (UD-1020, see
    /// [`ModelSolver::init_dof_manager`]).
    pub fn init_dof_manager(&mut self, dof_manager_type: &str) -> Result<()> {
        let comm = self.physics.base.mesh.read()?.communicator().clone();
        let id = self.physics.base.id.clone();
        self.solver.init_dof_manager(dof_manager_type, &id, &comm)
    }

    /// Upstream `initFull(options)` (PF-001/002): method; default solver for
    /// the method if none exists; laws from the parser (model section, else
    /// the root) when a file was parsed, assigned with the selector and
    /// initialised (`initPhaseField`). Laws registered with
    /// [`PhaseFieldModel::register_new_phase_field`] before are kept.
    pub fn init_full(&mut self, options: &ModelOptions) -> Result<()> {
        self.physics.base.method = options.analysis_method;
        if !self.solver.has_default_solver() {
            let id = default_solver_id(options.analysis_method);
            init_new_solver(
                &mut self.solver,
                &mut self.physics,
                id,
                options.solver_options.clone(),
            )?;
            self.physics.base.method = options.analysis_method;
        }
        if let Some(section) = self.physics.base.laws_section().cloned() {
            let permissive = self.physics.base.parser.is_permissive();
            let mesh_h = self.physics.base.mesh.clone();
            let mesh = mesh_h.read()?;
            let instantiated = {
                let p = &mut self.physics;
                let services = PfServices::new(
                    &mesh,
                    &p.base.fems,
                    &self.solver.dof_manager,
                    None,
                    None,
                    p.base.spatial_dimension,
                    p.time_step,
                );
                p.phasefields
                    .instantiate_laws(&p.factory, &section, &services, permissive)
            };
            // on a distributed mesh the instantiation outcome is agreed in
            // the initialisation scope (a rank-specific failure must not
            // leave the others waiting, as in the solid model)
            self.init_phase_fields_after(&mesh, instantiated)?;
        }
        self.physics.time_step = self.solver.time_step(None).unwrap_or(0.0);
        Ok(())
    }

    /// Upstream `initFull` with the default options except the method.
    pub fn init_full_with(&mut self, method: AnalysisMethod) -> Result<()> {
        self.init_full(&ModelOptions::new(method))
    }

    /// Upstream `initConstitutiveLaws()`: assign the not-ghost elements to
    /// the laws with the selector, apply the model's upstream-defect
    /// switches, then `initPhaseField` of every law.
    pub fn init_phase_fields(&mut self) -> Result<()> {
        let mesh_h = self.physics.base.mesh.clone();
        let mesh = mesh_h.read()?;
        self.init_phase_fields_on(&mesh)
    }

    fn init_phase_fields_on(&mut self, mesh: &Mesh) -> Result<()> {
        self.init_phase_fields_after(mesh, Ok(()))
    }

    /// [`PhaseFieldModel::init_phase_fields`] after a local step `pre` (the
    /// law instantiation of `init_full`) whose failure is agreed **fatal** on
    /// a distributed mesh (the failing rank keeps a registered, partially
    /// parsed law: rebuild the model), as the solid model.
    fn init_phase_fields_after(&mut self, mesh: &Mesh, pre: Result<()>) -> Result<()> {
        if let Some(es) = &mesh.synchronizers().element {
            let mut ph = Phases::new(&es.communicator, "pfm:init_phase_fields");
            let r = ph
                .agree("instantiate", pre, Severity::Fatal)
                .and_then(|()| self.init_phase_fields_in(&mut ph, mesh, es));
            return ph.settle(r);
        }
        pre?;
        self.check_has_laws()?;
        self.physics
            .phasefields
            .assign_laws_to_elements(mesh, None)?;
        self.apply_compat();
        self.init_each_law(mesh)
    }

    fn check_has_laws(&self) -> Result<()> {
        if self.physics.phasefields.nb_laws() == 0 {
            return Err(Error::invalid("No constitutive_law to initialize !"));
        }
        Ok(())
    }

    /// The distributed initialisation (as the solid and diffusion models):
    /// a failed selection is agreed
    /// recoverable (nothing committed); the commit, the `_constitutive_law_id`
    /// exchange (ghost elements appended to their owner's law in rank-order
    /// unpack order, UD-232) and every law's `initPhaseField` are agreed
    /// fatal (the laws hold their elements: rebuild the model).
    fn init_phase_fields_in(
        &mut self,
        ph: &mut Phases,
        mesh: &Mesh,
        es: &akantu_comm::Synchronizer<Element>,
    ) -> Result<()> {
        let selection = self
            .check_has_laws()
            .and_then(|()| self.physics.phasefields.select_laws(mesh, None));
        let selection = ph.agreed("select", selection, Severity::Recoverable)?;
        let committed = self.physics.phasefields.commit_selection(selection);
        ph.agree("assign", committed, Severity::Fatal)?;
        let mut acc = akantu_model::constitutive_laws_handler::LawIdAccessor {
            laws: &mut self.physics.phasefields,
        };
        es.synchronize_in(
            ph,
            &mut acc,
            akantu_core::enums::SynchronizationTag::ConstitutiveLawId,
        )
        .map_err(|e| Error::fatal(akantu_core::FatalKind::Agreed, e))?;
        self.apply_compat();
        let init = self.init_each_law(mesh);
        ph.agree("init_phase_field", init, Severity::Fatal)
    }

    /// `initPhaseField` of every law, in creation order.
    fn init_each_law(&mut self, mesh: &Mesh) -> Result<()> {
        let p = &mut self.physics;
        let (laws, services) = p.split(mesh, &self.solver.dof_manager);
        let ctx = LawContext {
            phase: LawPhase::Init,
            services: &services,
        };
        for law in laws.laws_mut() {
            law.init_phase_field(&ctx)?;
        }
        Ok(())
    }

    fn apply_compat(&mut self) {
        let c = self.physics.upstream_compat;
        for law in self.physics.phasefields.laws_mut() {
            law.base_mut().compat = c;
        }
    }

    /// Set the upstream-defect switches of the model and of every law
    /// (decision D-003; [`PhaseFieldUpstreamCompat::upstream`] reproduces the
    /// reference).
    pub fn set_upstream_compat(&mut self, compat: PhaseFieldUpstreamCompat) {
        self.physics.upstream_compat = compat;
        self.apply_compat();
    }

    /// The model's upstream-defect switches.
    #[must_use]
    pub fn upstream_compat(&self) -> PhaseFieldUpstreamCompat {
        self.physics.upstream_compat
    }

    /// Upstream `registerNewConstitutiveLaw(name, type, option)`.
    pub fn register_new_phase_field(
        &mut self,
        name: &str,
        ty: &str,
        option: &str,
    ) -> Result<usize> {
        let mesh_h = self.physics.base.mesh.clone();
        let mesh = mesh_h.read()?;
        let p = &mut self.physics;
        let services = PfServices::new(
            &mesh,
            &p.base.fems,
            &self.solver.dof_manager,
            None,
            None,
            p.base.spatial_dimension,
            p.time_step,
        );
        let i = p
            .phasefields
            .register_new_law(&p.factory, name, ty, option, &services)?;
        p.phasefields.law_mut(i)?.base_mut().set_name(name);
        Ok(i)
    }

    /// Upstream `setPhaseFieldSelector(selector)`: the selector with the
    /// default chain as fallback.
    pub fn set_phase_field_selector(&mut self, selector: Box<dyn ConstitutiveLawSelector>) {
        self.physics.phasefields.selector = SelectorChain::with_fallback_to_default(selector);
    }

    /// Replace the whole selector chain.
    pub fn set_phase_field_selector_chain(&mut self, chain: SelectorChain) {
        self.physics.phasefields.selector = chain;
    }

    /// Upstream `reassignConstitutiveLaw()` (D-078): the selector is
    /// asked again for every element and the elements whose phase field
    /// changes move to it
    /// (`ConstitutiveLawsHandler::reassign_constitutive_law`).
    ///
    /// # Errors
    /// An element without a phase field, a selector result out of range,
    /// errors of the laws.
    pub fn reassign_phase_field(&mut self) -> Result<()> {
        let mesh_h = self.physics.base.mesh.clone();
        let mesh = mesh_h.read()?;
        let (laws, services) = self.physics.split(&mesh, &self.solver.dof_manager);
        let nq = |ty: akantu_core::ElementType, g: GhostType| {
            akantu_model::constitutive_law::ModelServices::nb_integration_points(&services, ty, g)
        };
        laws.reassign_constitutive_law(&mesh, &nq)
    }

    /// Solve one step (upstream `solveStep(solver_id)`).
    pub fn solve_step(&mut self, solver_id: Option<&str>) -> Result<()> {
        self.physics.base.mesh.check_current(&self.token)?;
        self.physics.time_step = self.solver.time_step(solver_id)?;
        self.solver.solve_step(&mut self.physics, solver_id)
    }

    /// Upstream `setTimeStep(dt, solver_id)` (also the time step of the
    /// `phase_field` dumper).
    pub fn set_time_step(&mut self, time_step: Real, solver_id: Option<&str>) -> Result<()> {
        self.solver.set_time_step(time_step, solver_id)?;
        self.physics.time_step = self.solver.time_step(None).unwrap_or(time_step);
        self.set_dumpers_time_step(time_step)
    }

    /// Upstream `getTimeStep(solver_id)`.
    pub fn time_step(&self, solver_id: Option<&str>) -> Result<Real> {
        self.solver.time_step(solver_id)
    }

    /// Upstream `initNewSolver(method)`.
    pub fn init_new_solver(&mut self, method: AnalysisMethod) -> Result<()> {
        self.physics.base.method = method;
        init_new_solver(
            &mut self.solver,
            &mut self.physics,
            default_solver_id(method),
            ModelSolverOptions::default(),
        )?;
        self.physics.base.method = method;
        Ok(())
    }

    /// Upstream `getNewSolver(id, options)`.
    pub fn get_new_solver(&mut self, solver_id: &str, options: &ModelSolverOptions) -> Result<()> {
        self.solver
            .get_new_solver(solver_id, options, &mut self.physics)
    }

    /// Upstream `savePreviousState()`: every law saves the history of its
    /// internals (`damage`, `phi`).
    pub fn save_previous_state(&mut self) -> Result<()> {
        for law in self.physics.phasefields.laws_mut() {
            law.save_previous_state()?;
        }
        Ok(())
    }

    // ---- accessors ----------------------------------------------------

    /// The DOF manager (upstream `getDOFManager()`).
    #[must_use]
    pub fn dof_manager(&self) -> &DofManager {
        &self.solver.dof_manager
    }

    /// Mutable DOF manager.
    pub fn dof_manager_mut(&mut self) -> &mut DofManager {
        &mut self.solver.dof_manager
    }

    fn family(&self) -> Result<&akantu_solver::dof_manager::DofData> {
        self.solver
            .dof_manager
            .family(DOF_ID)
            .map_err(|_| not_allocated("damage"))
    }

    fn family_mut(&mut self) -> Result<&mut akantu_solver::dof_manager::DofData> {
        self.solver
            .dof_manager
            .family_mut(DOF_ID)
            .map_err(|_| not_allocated("damage"))
    }

    /// Upstream `getDamage()`: nodal damage.
    pub fn damage(&self) -> Result<ArrayRef<'_, Real>> {
        self.family()?.dofs.read()
    }

    /// Mutable nodal damage.
    pub fn damage_mut(&mut self) -> Result<ArrayMut<'_, Real>> {
        self.family_mut()?.dofs.write()
    }

    /// Upstream `getPreviousDamage()`.
    pub fn previous_damage(&self) -> Result<ArrayRef<'_, Real>> {
        self.family()?
            .previous
            .as_ref()
            .ok_or_else(|| not_allocated("previous_damage"))?
            .read()
    }

    /// Upstream `getBlockedDOFs()`.
    pub fn blocked_dofs(&self) -> Result<ArrayRef<'_, bool>> {
        self.family()?
            .blocked
            .as_ref()
            .ok_or_else(|| not_allocated("blocked_dofs"))?
            .read()
    }

    /// Mutable blocked flags.
    pub fn blocked_dofs_mut(&mut self) -> Result<ArrayMut<'_, bool>> {
        self.family_mut()?
            .blocked
            .as_mut()
            .ok_or_else(|| not_allocated("blocked_dofs"))?
            .write()
    }

    /// Upstream `getExternalForce()`.
    pub fn external_force(&self) -> Result<&Array<Real>> {
        self.physics
            .external_force
            .as_ref()
            .ok_or_else(|| not_allocated("external_force"))
    }

    /// Mutable external force.
    pub fn external_force_mut(&mut self) -> Result<&mut Array<Real>> {
        self.physics
            .external_force
            .as_mut()
            .ok_or_else(|| not_allocated("external_force"))
    }

    /// Upstream `getInternalForce()` (a guard of the exportable array).
    pub fn internal_force(&self) -> Result<ArrayRef<'_, Real>> {
        self.physics
            .internal_force
            .as_ref()
            .ok_or_else(|| not_allocated("internal_force"))?
            .read()
    }

    /// Mutable internal force (an exclusive guard).
    pub fn internal_force_mut(&mut self) -> Result<ArrayMut<'_, Real>> {
        self.internal_force_shared_mut()?.write()
    }

    /// The internal force's exportable storage (the Python bindings'
    /// zero-copy `getInternalForce()`).
    pub fn internal_force_shared_mut(&mut self) -> Result<&mut SharedArray<Real>> {
        self.physics
            .internal_force
            .as_mut()
            .ok_or_else(|| not_allocated("internal_force"))
    }

    /// The FE engine `PhaseFieldFEEngine` (upstream `getFEEngine()`).
    pub fn fe_engine(&self) -> Result<&FeEngine> {
        self.physics.base.fems.get("")
    }

    /// Mesh handle.
    #[must_use]
    pub fn mesh_handle(&self) -> &SharedMesh {
        &self.physics.base.mesh
    }

    /// Model id.
    #[must_use]
    pub fn id(&self) -> &str {
        &self.physics.base.id
    }

    /// Spatial dimension.
    #[must_use]
    pub fn spatial_dimension(&self) -> usize {
        self.physics.base.spatial_dimension
    }

    /// Analysis method of the last `init_full`.
    #[must_use]
    pub fn analysis_method(&self) -> AnalysisMethod {
        self.physics.base.method
    }

    /// Upstream `getNbPhaseFields()`.
    #[must_use]
    pub fn nb_phase_fields(&self) -> usize {
        self.physics.phasefields.nb_laws()
    }

    /// Upstream `getPhaseField(index)`.
    pub fn phase_field(&self, index: usize) -> Result<&dyn PhaseField> {
        self.physics.phasefields.law(index)
    }

    /// Mutable law (the coupler writes its `strain`).
    pub fn phase_field_mut(&mut self, index: usize) -> Result<&mut (dyn PhaseField + 'static)> {
        self.physics.phasefields.law_mut(index)
    }

    /// Upstream `getPhaseField(name)`.
    pub fn phase_field_by_name(&self, name: &str) -> Result<&dyn PhaseField> {
        let i = self.physics.phasefields.law_index_of(name)?;
        self.physics.phasefields.law(i)
    }

    /// Index of the law named `name` (upstream `getConstitutiveLawIndex`).
    pub fn phase_field_index(&self, name: &str) -> Result<usize> {
        self.physics.phasefields.law_index_of(name)
    }

    /// Per-element law index (upstream `getConstitutiveLawByElement()`).
    #[must_use]
    pub fn phase_field_by_element(&self) -> &SharedElementTypeMapArray<Idx> {
        self.physics.phasefields.law_index_map()
    }

    /// Mutable per-element law index (users fill it before `init_full` to
    /// drive the default selector).
    pub fn phase_field_by_element_mut(&mut self) -> &mut SharedElementTypeMapArray<Idx> {
        self.physics.phasefields.law_index_map_mut()
    }

    /// Per-element law-local numbering (upstream
    /// `getConstitutiveLawLocalNumbering()`).
    #[must_use]
    pub fn phase_field_local_numbering(&self) -> &ElementTypeMapArray<Idx> {
        self.physics.phasefields.local_numbering_map()
    }

    // ---- boundary conditions (BC-001, upstream initBC(*this, "damage",
    // damage, external_force)) -------------------------------------------

    /// Upstream `applyBC(DirichletFunctor, group_name)` on the `damage`
    /// family (values and blocked flags).
    pub fn apply_dirichlet_bc(
        &mut self,
        functor: &dyn DirichletFunctor,
        group: &str,
    ) -> Result<()> {
        let mesh_h = self.physics.base.mesh.clone();
        let mesh = mesh_h.read()?;
        let g = mesh
            .groups()
            .element_group(group)
            .map_err(|e| group_error(group, &e))?;
        let f = self.family_mut().map_err(|e| group_error(group, &e))?;
        let blocked = f
            .blocked
            .as_mut()
            .ok_or_else(|| not_allocated("blocked_dofs"))?;
        apply_dirichlet(functor, &mesh, g, &mut f.dofs, blocked).map_err(|e| group_error(group, &e))
    }

    /// Upstream `applyBC(NeumannFunctor, group_name)`: integrated on the
    /// boundary FE engine and **added** to `external_force`.
    pub fn apply_neumann_bc(&mut self, functor: &dyn NeumannFunctor, group: &str) -> Result<()> {
        let mesh_h = self.physics.base.mesh.clone();
        let mesh = mesh_h.read()?;
        let g = mesh
            .groups()
            .element_group(group)
            .map_err(|e| group_error(group, &e))?;
        let p = &mut self.physics;
        let dual = p
            .external_force
            .as_mut()
            .ok_or_else(|| not_allocated("external_force"))?;
        let fe = p
            .base
            .fems
            .boundary("", &mesh)
            .map_err(|e| group_error(group, &e))?;
        // `external_force` is a plain (non-exportable) array here: lend its
        // storage to the exportable-array interface without copying.
        let mut shared = SharedArray::new(std::mem::take(dual));
        let r = apply_neumann(functor, &mesh, g, fe, 1, &mut shared);
        *dual = shared.into_inner()?;
        r.map_err(|e| group_error(group, &e))
    }

    // ---- energies (PF-012) ----------------------------------------------

    /// Upstream `getEnergy(id)` (default `"dissipated"`): `0 + Σ_laws`
    /// (creation order) of the laws' energies of the not-ghost elements,
    /// then the sum over ranks (upstream `allReduce(_sum)`; here a
    /// rank-ordered reduction, D-039: identical for one and two ranks). One
    /// collective operation: the local part is agreed before the
    /// reduction.
    pub fn energy(&mut self, energy_id: &str) -> Result<Real> {
        let mesh_h = self.physics.base.mesh.clone();
        let p = &mut self.physics;
        self.solver.dof_manager.collective("pfm:energy", |dm| {
            let local = (|| {
                let mesh = mesh_h.read()?;
                let (laws, services) = p.split(&mesh, dm);
                let ctx = LawContext {
                    phase: LawPhase::Energy,
                    services: &services,
                };
                let mut energy = 0.0;
                for law in laws.laws_mut() {
                    energy += law.energy_of(&ctx, energy_id)?;
                }
                Ok(energy)
            })();
            all_reduce_sum_in(dm, local)
        })
    }

    /// Upstream `getEnergy(id, element)` for a mesh element: the law of the
    /// element, at its law-local index (the stored `damage` and `grad_d` of
    /// the law are used, not recomputed, as upstream).
    pub fn energy_of_element(&self, energy_id: &str, element: Element) -> Result<Real> {
        let mesh_h = self.physics.base.mesh.clone();
        let mesh = mesh_h.read()?;
        self.energy_of_element_in(&mesh, energy_id, element)
    }

    fn energy_of_element_in(&self, mesh: &Mesh, energy_id: &str, element: Element) -> Result<Real> {
        let h = &self.physics.phasefields;
        let law_index = h.law_index_map().element_ref(element)?[0];
        let law_index = akantu_core::decode_idx(law_index)?
            .ok_or_else(|| Error::invalid(format!("{element} has no phase field")))?;
        let local = h.local_numbering_map().element(element)?[0];
        let local = Element::new(element.ty(), local, element.ghost_type());
        let law = h.law(law_index)?;
        crate::laws::check_local(law, local)?;
        let p = &self.physics;
        let services = PfServices::new(
            mesh,
            &p.base.fems,
            &self.solver.dof_manager,
            p.external_force.as_ref(),
            p.internal_force.as_ref(),
            p.base.spatial_dimension,
            p.time_step,
        );
        let ctx = LawContext {
            phase: LawPhase::Energy,
            services: &services,
        };
        law.energy_of_element(&ctx, energy_id, local)
    }

    /// Upstream `getEnergy(id, group_id)`: `0 + Σ` over the group's
    /// not-ghost elements (types in order, elements in group order) of
    /// [`PhaseFieldModel::energy_of_element`], then the sum over ranks
    /// (upstream `allReduce(_sum)`, rank-ordered here, D-039). Elements
    /// without a law (e.g. boundary elements of the group) are an error, as
    /// upstream's lookup in the per-element law map fails for them. One
    /// collective operation (the local part is agreed first).
    pub fn energy_of_group(&mut self, energy_id: &str, group: &str) -> Result<Real> {
        let local = self.energy_of_group_local(energy_id, group);
        self.solver
            .dof_manager
            .collective("pfm:energy_of_group", |dm| all_reduce_sum_in(dm, local))
    }

    fn energy_of_group_local(&self, energy_id: &str, group: &str) -> Result<Real> {
        let mesh_h = self.physics.base.mesh.clone();
        let mesh = mesh_h.read()?;
        let g = mesh
            .groups()
            .element_group(group)
            .map_err(|e| group_error(group, &e))?;
        // upstream `group.elementTypes()` / `getElementsIterable(type)`:
        // not-ghost elements, types in enum order
        let grp = g.group();
        let ghost = GhostType::NotGhost;
        let mut energy = 0.0;
        for ty in grp.element_types(ghost) {
            for &e in &grp.elements_of(ty, ghost) {
                energy += self
                    .energy_of_element_in(&mesh, energy_id, Element::new(ty, e, ghost))
                    .map_err(|err| group_error(group, &err))?;
            }
        }
        Ok(energy)
    }

    // ---- explicit assembly entry points -------------------------------

    /// Upstream `assembleStiffnessMatrix()` (outside a solve).
    pub fn assemble_stiffness_matrix(&mut self) -> Result<()> {
        let p = &mut self.physics;
        self.solver
            .dof_manager
            .collective("pfm:assemble_stiffness", |dm| {
                p.assemble_stiffness_matrix(dm)?;
                dm.finalize_matrix("K")
            })
    }

    /// Upstream `assembleInternalForces()` (outside a solve).
    pub fn assemble_internal_forces(&mut self) -> Result<()> {
        let p = &mut self.physics;
        self.solver
            .dof_manager
            .collective("pfm:assemble_internal_forces", |dm| {
                p.assemble_internal_forces(dm)
            })
    }

    /// Upstream `assembleResidual()`: **accumulates** into the DOF manager's
    /// residual (`r += f_ext + f_int`; zero it first with
    /// `dof_manager_mut().zero_residual()`).
    pub fn assemble_residual(&mut self) -> Result<()> {
        let p = &mut self.physics;
        self.solver
            .dof_manager
            .collective("pfm:assemble_residual", |dm| p.assemble_residual(dm))
    }

    /// Upstream `beforeSolveStep()` outside a solve: every law computes its
    /// driving forces (not ghost, then ghost).
    pub fn compute_driving_forces(&mut self) -> Result<()> {
        let p = &mut self.physics;
        self.solver
            .dof_manager
            .collective("pfm:compute_driving_forces", |dm| p.before_solve_step(dm))
    }
}

/// `allReduce(_sum)` of one scalar through the running collective
/// operation of `dm` (the local value, or the local error, is agreed first;
/// rank-ordered, D-039).
fn all_reduce_sum_in(dm: &mut DofManager, local: Result<Real>) -> Result<Real> {
    let mut ph = dm.phases()?;
    ph.run_collective("pfm:all_reduce", local.map(|x| [x]), |c, mut s| {
        c.all_reduce(&mut s, akantu_comm::ReduceOp::Sum)?;
        Ok(s[0])
    })
}

impl PhaseFieldModel {
    /// Upstream `SynchronizerRegistry::synchronize(tag)` for the tags the
    /// model registers on the mesh's element synchronizer (`_pfm_damage`,
    /// `_for_dump`, `_constitutive_law_id`; MPI-044/045): one collective
    /// exchange owner → ghost — `_pfm_damage`: the laws' driving force,
    /// driving energy and damage of the ghost elements' points; `_for_dump`:
    /// the nodal damage of the ghost elements' nodes (upstream unpacks it
    /// into the nodal damage; it registers the tag but never synchronises
    /// it); `_constitutive_law_id`: the law of the ghost elements without
    /// one. Nothing on a mesh without element synchronizer (serial). Another
    /// tag is an error on a distributed mesh (UD-553, category B; upstream:
    /// no synchronizer registered, nothing happens — reproduced by
    /// [`PhaseFieldUpstreamCompat::unregistered_synchronization_tag_is_a_noop`]).
    pub fn synchronize(&mut self, tag: akantu_core::enums::SynchronizationTag) -> Result<()> {
        use akantu_core::enums::SynchronizationTag as T;
        let mesh_h = self.physics.base.mesh.clone();
        let mesh = mesh_h.read()?;
        let Some(es) = &mesh.synchronizers().element else {
            return Ok(());
        };
        match tag {
            T::PfmDamage => {
                let fe = self.physics.base.fems.get("")?;
                let dim = self.physics.base.spatial_dimension;
                let nb_quad = parallel::nb_quad_map(fe, &mesh, dim)?;
                let mut acc = parallel::PfmDamageAccessor {
                    laws: &mut self.physics.phasefields,
                    nb_quad: &nb_quad,
                    dim,
                };
                es.synchronize(&mut acc, tag)
            }
            T::ForDump => {
                let f = self.family_mut()?;
                let mut damage = f.dofs.write()?;
                let mut acc = parallel::ElementNodalDamageAccessor {
                    mesh: &mesh,
                    damage: damage.as_mut_slice(),
                };
                es.synchronize(&mut acc, tag)
            }
            T::ConstitutiveLawId => {
                let mut acc = akantu_model::constitutive_laws_handler::LawIdAccessor {
                    laws: &mut self.physics.phasefields,
                };
                es.synchronize(&mut acc, tag)
            }
            _ if self
                .physics
                .upstream_compat
                .unregistered_synchronization_tag_is_a_noop =>
            {
                Ok(())
            }
            other => Err(Error::invalid(format!(
                "PhaseFieldModel::synchronize: tag {other} is not registered by the model"
            ))),
        }
    }

    /// Upstream DOF-entity data accessor of the model (`_pfm_damage`,
    /// `_for_dump`: the nodal damage) driven by the mesh's **node**
    /// synchronizer: masters overwrite their slaves' values. Collective;
    /// nothing on a mesh without node synchronizer; another tag is
    /// upstream's `Unknown ghost synchronization tag` error.
    pub fn synchronize_nodal(&mut self, tag: akantu_core::enums::SynchronizationTag) -> Result<()> {
        let mesh_h = self.physics.base.mesh.clone();
        let mesh = mesh_h.read()?;
        let Some(ns) = &mesh.synchronizers().node else {
            return Ok(());
        };
        if !matches!(
            tag,
            akantu_core::enums::SynchronizationTag::PfmDamage
                | akantu_core::enums::SynchronizationTag::ForDump
        ) {
            return Err(parallel::unknown_tag(tag));
        }
        let f = self.family_mut()?;
        let mut damage = f.dofs.write()?;
        let mut acc = parallel::NodalDamageAccessor {
            damage: damage.as_mut_slice(),
        };
        ns.synchronize(&mut acc, tag)
    }
}

/// Upstream `getDefaultSolverID(method)` (PF-002).
#[must_use]
pub fn default_solver_id(method: AnalysisMethod) -> (Id, TimeStepSolverType) {
    match method {
        AnalysisMethod::ExplicitLumpedMass => {
            ("explicit_lumped".into(), TimeStepSolverType::DynamicLumped)
        }
        AnalysisMethod::ExplicitConsistentMass => ("explicit".into(), TimeStepSolverType::Dynamic),
        AnalysisMethod::Static => ("static".into(), TimeStepSolverType::Static),
        AnalysisMethod::ImplicitDynamic => ("implicit".into(), TimeStepSolverType::Dynamic),
        _ => ("unknown".into(), TimeStepSolverType::NotDefined),
    }
}

impl PhaseFieldPhysics {
    /// Split into the laws and the services reading everything else.
    pub(crate) fn split<'a>(
        &'a mut self,
        mesh: &'a Mesh,
        dofs: &'a DofManager,
    ) -> (
        &'a mut ConstitutiveLawsHandler<dyn PhaseField>,
        PfServices<'a>,
    ) {
        let Self {
            base,
            phasefields,
            external_force,
            internal_force,
            time_step,
            ..
        } = self;
        let services = PfServices::new(
            mesh,
            &base.fems,
            dofs,
            external_force.as_ref(),
            internal_force.as_ref(),
            base.spatial_dimension,
            *time_step,
        );
        (phasefields, services)
    }

    fn on_nodes_added(&mut self, mesh: &Mesh) -> Result<()> {
        let n = mesh.nb_nodes();
        if let Some(a) = &mut self.external_force {
            a.try_resize(n, 0.0)?;
        }
        if let Some(a) = &mut self.internal_force {
            a.write()?.try_resize(n, 0.0)?;
        }
        Ok(())
    }

    /// Node removal (SMM-015; upstream `PhaseFieldModel`
    /// has no `onNodesRemoved`: the base handler's empty one, so its forces
    /// keep the old node count): the forces follow the numbering
    /// (`removeNodesFromArray`); the damage family was renumbered and
    /// compacted by the DOF manager at level 20, and `K` is reassembled at
    /// every solve. Both arrays are validated before either changes.
    fn on_nodes_removed(&mut self, new_numbering: &[akantu_core::Idx]) -> Result<()> {
        let mut plan = None;
        let lens = [
            self.external_force.as_ref().map(Array::len),
            self.internal_force.as_ref().map(SharedArray::len),
        ];
        for n in lens.into_iter().flatten() {
            plan = Some(akantu_model::model::node_compaction_plan(n, new_numbering)?);
        }
        if let Some(plan) = &plan {
            if let Some(a) = &mut self.external_force {
                akantu_model::model::apply_node_compaction(a, plan);
            }
            // the internal force is exportable (PY-091): through its guard
            if let Some(a) = &mut self.internal_force {
                akantu_model::model::apply_node_compaction_guard(&mut a.write()?, plan);
            }
        }
        Ok(())
    }
}

impl SolverCallback for PhaseFieldPhysics {
    /// Upstream `getMatrixType(id)`: `K` → symmetric, else not defined.
    fn matrix_type(&self, matrix_id: &str) -> MatrixType {
        if matrix_id == "K" {
            MatrixType::Symmetric
        } else {
            MatrixType::NotDefined
        }
    }

    /// Upstream `assembleMatrix(id)`: `K` → stiffness, anything else an
    /// error (`AKANTU_ERROR("Unknown Matrix ID for PhaseFieldModel")`).
    fn assemble_matrix(&mut self, dofs: &mut DofManager, matrix_id: &str) -> Result<()> {
        if matrix_id == "K" {
            self.assemble_stiffness_matrix(dofs)
        } else {
            Err(Error::invalid(format!(
                "Unknown Matrix ID for PhaseFieldModel : {matrix_id}"
            )))
        }
    }

    /// Upstream `assembleLumpedMatrix(id)`: does nothing.
    fn assemble_lumped_matrix(&mut self, _dofs: &mut DofManager, _matrix_id: &str) -> Result<()> {
        Ok(())
    }

    fn assemble_residual(&mut self, dofs: &mut DofManager) -> Result<()> {
        PhaseFieldPhysics::assemble_residual(self, dofs)
    }

    /// Upstream `beforeSolveStep()`: every law's `beforeSolveStep`
    /// (`computeAllDrivingForces` not ghost, then ghost).
    fn before_solve_step(&mut self, dofs: &mut DofManager) -> Result<()> {
        let mesh_h = self.base.mesh.clone();
        let mesh = mesh_h.read()?;
        let (laws, services) = self.split(&mesh, dofs);
        let ctx = LawContext {
            phase: LawPhase::BeforeSolveStep,
            services: &services,
        };
        for law in laws.laws_mut() {
            law.before_solve_step(&ctx)?;
        }
        Ok(())
    }

    /// Upstream `afterSolveStep(converged)`: nothing unless converged, then
    /// every law's `afterSolveStep`.
    fn after_solve_step(&mut self, dofs: &mut DofManager, converged: bool) -> Result<()> {
        if !converged {
            return Ok(());
        }
        let mesh_h = self.base.mesh.clone();
        let mesh = mesh_h.read()?;
        let (laws, services) = self.split(&mesh, dofs);
        let ctx = LawContext {
            phase: LawPhase::AfterSolveStep,
            services: &services,
        };
        for law in laws.laws_mut() {
            law.after_solve_step(&ctx)?;
        }
        Ok(())
    }

    /// Upstream `initSolver(type)`: allocate `external_force` and
    /// `internal_force` (one component), register the DOF family `damage`
    /// (values, blocked flags, previous values) if missing; a `_dynamic`
    /// solver is `AKANTU_TO_IMPLEMENT` upstream.
    fn init_solver(&mut self, dofs: &mut DofManager, ty: TimeStepSolverType) -> Result<()> {
        let mesh_h = self.base.mesh.clone();
        let mesh = mesh_h.read()?;
        let n = mesh.nb_nodes();
        alloc_nodal_field(&mut self.external_force, n, 1)?;
        drop(alloc_nodal_shared_field(&mut self.internal_force, n, 1)?);
        if !dofs.has_dofs(DOF_ID) {
            dofs.register_nodal_dofs(&mesh, DOF_ID, 1)?;
            dofs.register_blocked_dofs(DOF_ID)?;
            dofs.register_dofs_previous(DOF_ID)?;
        }
        if ty == TimeStepSolverType::Dynamic {
            return Err(Error::NotImplemented(
                "PhaseFieldModel::initSolver(_dynamic) (AKANTU_TO_IMPLEMENT upstream)",
            ));
        }
        Ok(())
    }

    /// Upstream `getDefaultSolverType()`: `static`.
    fn default_solver_type(&self) -> TimeStepSolverType {
        TimeStepSolverType::Static
    }

    /// Upstream `getDefaultSolverOptions(type)` (PF-002).
    fn default_solver_options(&self, ty: TimeStepSolverType) -> ModelSolverOptions {
        let mut o = ModelSolverOptions::default();
        let (nls, scheme, st) = match ty {
            TimeStepSolverType::DynamicLumped => (
                NonLinearSolverType::Lumped,
                IntegrationSchemeType::CentralDifference,
                SolutionType::Acceleration,
            ),
            TimeStepSolverType::Static => (
                NonLinearSolverType::NewtonRaphson,
                IntegrationSchemeType::PseudoTime,
                SolutionType::NotDefined,
            ),
            TimeStepSolverType::Dynamic => (
                NonLinearSolverType::NewtonRaphson,
                IntegrationSchemeType::BackwardEuler,
                SolutionType::DAMAGE,
            ),
            // upstream: AKANTU_EXCEPTION(type << " is not a valid time step
            // solver type"); the options stay unset and getNewSolver fails
            TimeStepSolverType::NotDefined => return o,
        };
        o.non_linear_solver_type = Some(nls);
        o.integration_scheme_type.insert(DOF_ID.into(), scheme);
        o.solution_type.insert(DOF_ID.into(), st);
        o
    }
}

/// The parts of a [`PhaseFieldModel`] that react to mesh events (the
/// physics and the solver), borrowed apart from the model's consumer token
/// so that the token and the handler can be passed to `SharedMesh::mutate`
/// together ([`PhaseFieldModel::split_for_mutation`]; as
/// the solid model's `SmmEventParts`).
#[derive(Debug)]
pub struct PhaseFieldEventParts<'a> {
    /// Physics.
    pub physics: &'a mut PhaseFieldPhysics,
    /// Solver (DOF manager).
    pub solver: &'a mut ModelSolver,
}

impl PhaseFieldModel {
    /// The consumer token and the event-handling parts of the model,
    /// borrowed disjointly (see [`PhaseFieldEventParts`]).
    pub fn split_for_mutation(&mut self) -> (&ConsumerToken, PhaseFieldEventParts<'_>) {
        (
            &self.token,
            PhaseFieldEventParts {
                physics: &mut self.physics,
                solver: &mut self.solver,
            },
        )
    }

    fn parts(&mut self) -> PhaseFieldEventParts<'_> {
        PhaseFieldEventParts {
            physics: &mut self.physics,
            solver: &mut self.solver,
        }
    }

    /// One staged mutation of the model's mesh with the model as its
    /// consumer (`SharedMesh::mutate` with the model's token and
    /// [`PhaseFieldEventParts`]; the model must be the mesh's only consumer
    /// — otherwise use [`PhaseFieldModel::split_for_mutation`] with the
    /// other consumers). `distributed` is required on a distributed mesh
    /// when events are recorded (the model refuses element events there).
    ///
    /// Node removal (SMM-015): `MeshTransaction::{remove_nodes, purify}`
    /// renumber the damage DOFs and compact the model's forces (UD-1071).
    ///
    /// # Errors
    /// The mutation's error (rolled back on every rank before the commit,
    /// poisoned after it; `docs/ARCHITECTURE.md`).
    pub fn mutate_mesh(
        &mut self,
        change: impl FnOnce(&mut akantu_mesh::MeshTransaction<'_>) -> Result<()>,
        distributed: Option<&mut dyn akantu_mesh::DistributedHook>,
    ) -> Result<akantu_mesh::CommittedMutation> {
        let mesh = self.physics.base.mesh.clone();
        let (token, mut parts) = self.split_for_mutation();
        mesh.mutate(&mut [(token, &mut parts)], change, distributed)
    }
}

impl MeshEventHandler for PhaseFieldModel {
    fn event_priority(&self) -> EventPriority {
        EventPriority::MODEL
    }

    fn event_priorities(&self) -> Vec<EventPriority> {
        pf_event_priorities()
    }

    fn on_mesh_event(&mut self, mesh: &Mesh, event: &MeshEvent) -> Result<()> {
        self.parts().on_mesh_event(mesh, event)
    }

    /// The phase-field model's check (`pf_check_mesh_event`).
    fn check_mesh_event(&self, mesh: &Mesh, event: &MeshEvent) -> Result<()> {
        pf_check_mesh_event(&self.physics, &self.solver, mesh, event)
    }

    fn on_mesh_event_at(
        &mut self,
        mesh: &Mesh,
        event: &MeshEvent,
        level: EventPriority,
    ) -> Result<()> {
        self.parts().on_mesh_event_at(mesh, event, level)
    }

    fn on_mesh_event_phased(
        &mut self,
        mesh: &Mesh,
        event: &MeshEvent,
        priority: EventPriority,
        phases: &mut akantu_comm::phase::Phases,
    ) -> Result<()> {
        self.parts()
            .on_mesh_event_phased(mesh, event, priority, phases)
    }
}

fn pf_event_priorities() -> Vec<EventPriority> {
    vec![
        EventPriority::FE_ENGINE,
        EventPriority::DOF_MANAGER,
        EventPriority::MODEL,
    ]
}

/// Before the mesh commit, for every level the model dispatches to: the
/// FE engines' and the DOF manager's checks
/// (forwarded), then the model's own refusal of element events on a
/// distributed mesh, so the mutation is rolled back recoverably instead of
/// poisoning the domain at dispatch. Node removal is accepted since SMM-015:
/// the DOF manager renumbers and the model compacts its
/// forces. The **one** implementation of the check: [`PhaseFieldModel`] and
/// [`PhaseFieldEventParts`] call it.
fn pf_check_mesh_event(
    physics: &PhaseFieldPhysics,
    solver: &ModelSolver,
    mesh: &Mesh,
    event: &MeshEvent,
) -> Result<()> {
    physics.base.fems.check_mesh_event(mesh, event)?;
    solver.dof_manager.check_mesh_event(mesh, event)?;
    match event {
        MeshEvent::ElementsAdded { .. } | MeshEvent::ElementsRemoved { .. }
            if mesh.synchronizers().element.is_some() =>
        {
            Err(distributed_element_events_refused())
        }
        _ => Ok(()),
    }
}

impl MeshEventHandler for PhaseFieldEventParts<'_> {
    fn event_priority(&self) -> EventPriority {
        EventPriority::MODEL
    }

    fn event_priorities(&self) -> Vec<EventPriority> {
        pf_event_priorities()
    }

    fn on_mesh_event(&mut self, mesh: &Mesh, event: &MeshEvent) -> Result<()> {
        self.on_mesh_event_at(mesh, event, EventPriority::MODEL)
    }

    /// The phase-field model's check (`pf_check_mesh_event`).
    fn check_mesh_event(&self, mesh: &Mesh, event: &MeshEvent) -> Result<()> {
        pf_check_mesh_event(self.physics, self.solver, mesh, event)
    }

    /// FE engine (9), DOF manager (20), model arrays and laws (94).
    fn on_mesh_event_at(
        &mut self,
        mesh: &Mesh,
        event: &MeshEvent,
        level: EventPriority,
    ) -> Result<()> {
        match level {
            EventPriority::FE_ENGINE => match event {
                MeshEvent::ElementsAdded { elements, .. } => {
                    for (_, fe) in self.physics.base.fems.iter_mut() {
                        fe.on_elements_added(mesh, elements)?;
                    }
                    Ok(())
                }
                MeshEvent::ElementsRemoved { new_numbering, .. } => {
                    for (_, fe) in self.physics.base.fems.iter_mut() {
                        fe.on_elements_removed(new_numbering)?;
                    }
                    Ok(())
                }
                _ => Ok(()),
            },
            EventPriority::DOF_MANAGER => self.solver.on_dof_mesh_event(mesh, event),
            EventPriority::MODEL => match event {
                MeshEvent::NodesAdded { .. } => self.physics.on_nodes_added(mesh),
                MeshEvent::NodesRemoved { new_numbering, .. } => {
                    self.physics.on_nodes_removed(new_numbering)
                }
                MeshEvent::ElementsAdded { .. } | MeshEvent::ElementsRemoved { .. }
                    if mesh.synchronizers().element.is_some() =>
                {
                    Err(distributed_element_events_refused())
                }
                MeshEvent::ElementsAdded { elements, .. } => {
                    self.laws_on_elements_added(mesh, elements)
                }
                MeshEvent::ElementsRemoved {
                    elements,
                    new_numbering,
                    ..
                } => self.laws_on_elements_removed(mesh, elements, new_numbering),
                _ => Ok(()),
            },
            _ => Ok(()),
        }
    }

    fn on_mesh_event_phased(
        &mut self,
        mesh: &Mesh,
        event: &MeshEvent,
        priority: EventPriority,
        phases: &mut akantu_comm::phase::Phases,
    ) -> Result<()> {
        match priority {
            EventPriority::DOF_MANAGER => self
                .solver
                .on_dof_mesh_event_phased(mesh, event, priority, phases),
            _ => self.on_mesh_event_at(mesh, event, priority),
        }
    }
}

/// Element events on a distributed mesh: upstream assigns the new
/// not-ghost elements with the selector and the new ghost elements over
/// `_constitutive_law_id` (`ConstitutiveLawsHandler::onElementsAdded`); the
/// port's phase-field model ports the exchange at initialisation only,
/// so the event is refused before the mesh commit.
fn distributed_element_events_refused() -> Error {
    Error::NotImplemented(
        "PhaseFieldModel: element additions or removals on a distributed mesh (the \
         _constitutive_law_id exchange of the new ghost elements)",
    )
}

/// Resize the handler maps to the mesh's element counts (new entries `-1`,
/// types of the mesh's spatial dimension).
fn fit_law_maps(h: &mut ConstitutiveLawsHandler<dyn PhaseField>, mesh: &Mesh) -> Result<()> {
    for ghost in GhostType::ALL {
        let types: Vec<akantu_core::ElementType> = mesh
            .element_types(akantu_core::TypeFilter {
                spatial_dimension: Some(mesh.spatial_dimension()),
                ghost_type: ghost,
                kind: None,
            })
            .collect();
        for ty in types {
            let n = mesh.nb_element(ty, ghost);
            // the law index map is exportable (guards), the local numbering
            // plain: same operations, law index first
            let map = h.law_index_map_mut();
            if !map.exists(ty, ghost) {
                map.alloc(0, 1, ty, ghost, -1)?;
            }
            map.get_mut(ty, ghost)?.try_resize(n, -1)?;
            let map = h.local_numbering_map_mut();
            if !map.exists(ty, ghost) {
                map.alloc(0, 1, ty, ghost, -1)?;
            }
            map.get_mut(ty, ghost)?.try_resize(n, -1)?;
        }
    }
    Ok(())
}

impl PhaseFieldEventParts<'_> {
    /// Upstream `ConstitutiveLawsHandler::onElementsAdded` (level 94): the
    /// handler maps follow the mesh, the new elements of the model dimension
    /// are assigned with the selector chain (upstream
    /// `assignConstitutiveLawToElements(new elements)`), and the internals of
    /// the initialised laws are resized (new points get the defaults, `g_c`
    /// new draws).
    fn laws_on_elements_added(&mut self, mesh: &Mesh, elements: &[Element]) -> Result<()> {
        let dim = self.physics.base.spatial_dimension;
        let p = &mut *self.physics;
        let (laws, services) = p.split(mesh, &self.solver.dof_manager);
        let ctx = LawContext {
            phase: LawPhase::Init,
            services: &services,
        };
        fit_law_maps(laws, mesh)?;
        let mut filter = ElementTypeMapArray::<Idx>::new("new_element_filter");
        for e in elements {
            if e.ty().spatial_dimension() != dim {
                continue;
            }
            if !filter.exists(e.ty(), e.ghost_type()) {
                filter.alloc(0, 1, e.ty(), e.ghost_type(), 0)?;
            }
            filter
                .get_mut(e.ty(), e.ghost_type())?
                .try_push_tuple(&[e.index()])?;
        }
        if laws.nb_laws() > 0 {
            laws.assign_laws_to_elements(mesh, Some(&filter))?;
        }
        for law in laws.laws_mut() {
            if law.base().is_init() {
                let blocks = law.base().blocks(&ctx);
                law.resize_internals(&blocks)?;
            }
        }
        Ok(())
    }

    /// Upstream `ConstitutiveLaw::onElementsRemoved` for every law, then the
    /// handler maps follow the mesh's new element counts and the kept
    /// elements' law index and law-local number are written.
    fn laws_on_elements_removed(
        &mut self,
        mesh: &Mesh,
        elements: &[Element],
        new_numbering: &ElementTypeMapArray<Idx>,
    ) -> Result<()> {
        let p = &mut *self.physics;
        let (laws, services) = p.split(mesh, &self.solver.dof_manager);
        let nq = |ty: akantu_core::ElementType, g: GhostType| {
            akantu_model::constitutive_law::ModelServices::nb_integration_points(&services, ty, g)
        };
        let mut entries_all = Vec::new();
        for i in 0..laws.nb_laws() {
            let entries = laws
                .law_mut(i)?
                .on_elements_removed(elements, new_numbering, &nq)?;
            entries_all.push((i, entries));
        }
        for ghost in GhostType::ALL {
            let types: Vec<akantu_core::ElementType> = mesh
                .element_types(akantu_core::TypeFilter {
                    spatial_dimension: Some(mesh.spatial_dimension()),
                    ghost_type: ghost,
                    kind: None,
                })
                .collect();
            for ty in types {
                // only the blocks the event renumbers (upstream resizes the
                // maps of `new_numbering`'s types only); the others keep
                // their entries
                if !new_numbering.exists(ty, ghost) {
                    continue;
                }
                let n = mesh.nb_element(ty, ghost);
                if let Some(mut a) = if_present(laws.law_index_map_mut().get_mut(ty, ghost))? {
                    a.as_mut_slice().fill(-1);
                    a.try_resize(n, -1)?;
                }
                if let Ok(a) = laws.local_numbering_map_mut().get_mut(ty, ghost) {
                    a.as_mut_slice().fill(-1);
                    a.try_resize(n, -1)?;
                }
            }
        }
        fit_law_maps(laws, mesh)?;
        for (i, entries) in entries_all {
            laws.record_renumbered(i, &entries)?;
        }
        Ok(())
    }
}

impl Model for PhaseFieldModel {
    fn model_type(&self) -> ModelType {
        ModelType::PhaseFieldModel
    }

    fn mesh(&self) -> &SharedMesh {
        &self.physics.base.mesh
    }

    fn mesh_token(&self) -> &ConsumerToken {
        &self.token
    }

    fn model_solver(&self) -> &ModelSolver {
        &self.solver
    }

    fn model_solver_mut(&mut self) -> &mut ModelSolver {
        &mut self.solver
    }

    fn init_full(&mut self, options: &ModelOptions) -> Result<()> {
        PhaseFieldModel::init_full(self, options)
    }

    fn solve_step(&mut self, solver_id: Option<&str>) -> Result<()> {
        PhaseFieldModel::solve_step(self, solver_id)
    }

    /// Upstream `PhaseFieldModel` has no stable time step.
    fn stable_time_step(&mut self) -> Result<Real> {
        Err(Error::NotImplemented(
            "PhaseFieldModel::getStableTimeStep (not defined upstream)",
        ))
    }

    /// Upstream `dump()` of the `phase_field` dumper.
    fn dump(&mut self, time: Option<Real>, step: Option<usize>) -> Result<()> {
        PhaseFieldModel::dump(self, time, step)
    }
}
