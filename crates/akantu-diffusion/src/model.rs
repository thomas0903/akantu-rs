// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/model/diffusion_model/diffusion_model.hh,
// diffusion_model.cc, heat_transfer_model.hh; src/model/common/
// constitutive_laws/constitutive_laws_handler{.hh,_tmpl.hh} (initFullImpl,
// initConstitutiveLaws, assignConstitutiveLawToElements, onElementsAdded /
// onElementsRemoved).

//! [`DiffusionModel`] and [`HeatTransferModel`] (DIFF-001…011).
//!
//! # Structure (D-021)
//!
//! `DiffusionModel { physics, solver, token, dumpers }`: the physics part
//! ([`DiffusionPhysics`]: model base, diffusion laws, model-owned nodal
//! arrays, release counter) implements [`SolverCallback`]; the
//! [`ModelSolver`] owns the DOF manager and the time-step solvers. The DOF
//! family `<dof_name>` (`temperature` for a heat-transfer model) — values,
//! blocked flags and, for dynamic solvers, the rate — lives in the DOF
//! manager (upstream registers the model's arrays by reference);
//! `external_flow` and `internal_flow` belong to the physics. Upstream's
//! `increment` array is never allocated (UD-286).
//!
//! # Laws
//!
//! Laws implement [`DiffusionLaw`]; each law method receives a
//! [`DiffusionLawContext`] whose services read the mesh, the FE engines and
//! the nodal fields while the law is mutably borrowed (ARCHITECTURE).
//!
//! # Collective protocol (ARCHITECTURE.md, "Non-linear solvers, time stepping and switches" and "Agreement across ranks")
//!
//! Every model callback runs inside the solvers' collective operation
//! (agreed by `DofManager::step`); the `_diffusion` exchange of a residual
//! assembly goes through the running operation's scope
//! (`DofManager::phases`). Methods that reduce over ranks (energies, stable
//! time step) run as one `DofManager::collective` operation whose local
//! part is agreed before the reduction. The law initialisation of a
//! distributed model is one agreed scope (as SMM's `init_materials`).

mod assembly;
mod energy;
mod io;
mod parallel;
mod services;

use std::ops::{Deref, DerefMut};

use akantu_comm::phase::{Phases, Severity};
use akantu_core::array_shared::{ArrayMut, ArrayRef, SharedArray};
use akantu_core::enums::{
    AnalysisMethod, EventPriority, IntegrationSchemeType, MatrixType, ModelType,
    NonLinearSolverType, TimeStepSolverType,
};
use akantu_core::{
    Array, Element, ElementType, ElementTypeMapArray, Error, GhostType, Id, Idx, Real, Result,
    SharedElementTypeMapArray, TypeFilter,
};
use akantu_mesh::group::ElementGroupRef;
use akantu_mesh::{ConsumerToken, Mesh, MeshEvent, MeshEventHandler, MeshPart, SharedMesh};
use akantu_model::boundary_condition::{DirichletFunctor, apply_dirichlet, group_error};
use akantu_model::constitutive_law::{
    ConstitutiveLawSelector, LawContext, LawPhase, SelectorChain,
};
use akantu_model::model::{ModelBase, alloc_nodal_shared_field, init_new_solver};
use akantu_model::{ConstitutiveLawsHandler, Model, ModelOptions};
use akantu_parser::{Parser, ParserType};
use akantu_solver::{
    DofManager, IntegrationScheme, ModelSolver, ModelSolverOptions, SolutionType, SolverCallback,
};

use crate::law::{
    DiffusionLaw, DiffusionLawContext, DiffusionLawFactory, DiffusionModelKind,
    DiffusionUpstreamCompat, register_builtin_laws,
};

pub use io::DiffusionDumpers;
pub(crate) use services::DiffusionServices;

/// Default id of a generic diffusion model (upstream `"diffusion"`).
pub const DEFAULT_ID: &str = "diffusion";
/// Default DOF name of a generic diffusion model (upstream `"diffusion"`).
pub const DEFAULT_DOF_NAME: &str = "diffusion";
/// Default id of a heat-transfer model (upstream `"heat_transfer"`).
pub const HEAT_TRANSFER_ID: &str = "heat_transfer";
/// DOF name of a heat-transfer model.
pub const TEMPERATURE: &str = "temperature";
/// Id of the time-step solver of
/// [`DiffusionModel::set_consistent_initial_rate`] (created on first use).
pub const CONSISTENT_RATE_SOLVER: &str = "consistent_initial_rate";

/// Shape of a nodal array (plain, exportable or a guard), for
/// [`check_nodal`]: metadata only, no element access (as SMM, D-056).
pub(crate) trait NodalShape {
    fn shape(&self) -> (usize, usize);
}

impl<T> NodalShape for Array<T> {
    fn shape(&self) -> (usize, usize) {
        (self.len(), self.nb_component())
    }
}

impl<T> NodalShape for SharedArray<T> {
    fn shape(&self) -> (usize, usize) {
        (self.len(), self.nb_component())
    }
}

impl<T> NodalShape for ArrayRef<'_, T> {
    fn shape(&self) -> (usize, usize) {
        (self.len(), self.nb_component())
    }
}

impl<T> NodalShape for ArrayMut<'_, T> {
    fn shape(&self) -> (usize, usize) {
        (self.len(), self.nb_component())
    }
}

/// Shape check of a nodal array read by a model loop: the public accessors
/// hand out write guards, so a user can resize any of them; every loop
/// validates `nb_nodes` tuples of `nb_component` values first and returns
/// `DimensionMismatch` (agreed like any local error) instead of panicking.
pub(crate) fn check_nodal(
    context: &'static str,
    a: &impl NodalShape,
    nb_nodes: usize,
    nb_component: usize,
) -> Result<()> {
    let (len, nbc) = a.shape();
    if len != nb_nodes {
        return Err(Error::DimensionMismatch {
            context,
            expected: nb_nodes,
            got: len,
        });
    }
    if nbc != nb_component {
        return Err(Error::DimensionMismatch {
            context,
            expected: nb_component,
            got: nbc,
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

/// Physics part of the diffusion model: everything except the solver.
/// Implements [`SolverCallback`].
#[derive(Debug)]
pub struct DiffusionPhysics {
    /// Model base (mesh handle, id, dimension, method, FE engines, parser).
    pub base: ModelBase,
    /// Diffusion laws (upstream `ConstitutiveLawsHandler<DiffusionLaw>`).
    pub laws: ConstitutiveLawsHandler<dyn DiffusionLaw>,
    /// Law factory of this model (upstream's process-global
    /// `DiffusionFactory`, built-in laws registered).
    pub factory: DiffusionLawFactory,
    /// Upstream-defect switches (default: corrected).
    pub upstream_compat: DiffusionUpstreamCompat,
    kind: DiffusionModelKind,
    dof_name: Id,
    external_flow: Option<SharedArray<Real>>,
    internal_flow: Option<SharedArray<Real>>,
    diffusion_release: i64,
    need_to_reassemble_capacity: bool,
    need_to_reassemble_capacity_lumped: bool,
    time_step: Real,
    /// `Bᵀ D∇u` per point and its integral per element of the last
    /// internal-flow block (kept between steps: no allocation per step).
    flow_scratch: (Array<Real>, Array<Real>),
}

/// Diffusion model (upstream `DiffusionModel`): a scalar diffusion field
/// named `dof_name` driven by diffusion laws.
#[derive(Debug)]
pub struct DiffusionModel {
    /// Physics (callback).
    pub physics: DiffusionPhysics,
    /// Solver.
    pub solver: ModelSolver,
    /// Mesh-consumer registration.
    pub token: ConsumerToken,
    /// Dumpers of the model (upstream: registered on the mesh).
    pub dumpers: DiffusionDumpers,
}

impl DiffusionModel {
    /// Upstream `DiffusionModel(mesh, dim, id, nullptr, dof_name,
    /// model_type)` with an empty parser (see
    /// [`DiffusionModel::with_parser`]). Upstream defaults: id
    /// [`DEFAULT_ID`], DOF [`DEFAULT_DOF_NAME`], model type
    /// `heat_transfer_model`.
    pub fn new(
        mesh: SharedMesh,
        spatial_dimension: Option<usize>,
        id: &str,
        dof_name: &str,
    ) -> Result<Self> {
        Self::with_parser(
            mesh,
            spatial_dimension,
            id,
            dof_name,
            ModelType::HeatTransferModel,
            Parser::default(),
        )
    }

    /// Generic diffusion model with an explicit input-file snapshot.
    pub fn with_parser(
        mesh: SharedMesh,
        spatial_dimension: Option<usize>,
        id: &str,
        dof_name: &str,
        model_type: ModelType,
        parser: Parser,
    ) -> Result<Self> {
        Self::build(
            mesh,
            spatial_dimension,
            id,
            dof_name,
            model_type,
            DiffusionModelKind::Generic,
            parser,
        )
    }

    /// Upstream constructor (DIFF-001): DOF manager
    /// `<id>:dof_manager_default` (solvers and schemes of the model's
    /// section), Lagrange FE engine `<id>:fem` on the elements of the model
    /// dimension, ParaView dumper `<id>`, law registry with
    /// `constitutive_law_index = -1` everywhere.
    ///
    /// # Errors
    /// An unusable mesh domain, a bad dimension, an invalid model section or
    /// a mesh with ghost elements but no element synchronizer.
    fn build(
        mesh: SharedMesh,
        spatial_dimension: Option<usize>,
        id: &str,
        dof_name: &str,
        model_type: ModelType,
        kind: DiffusionModelKind,
        parser: Parser,
    ) -> Result<Self> {
        let mut base = ModelBase::new(mesh.clone(), model_type, spatial_dimension, id)?;
        base.parser = parser;
        let token = mesh.register_multilevel_consumer(
            id,
            &[
                EventPriority::FE_ENGINE,
                EventPriority::DOF_MANAGER,
                EventPriority::MODEL,
            ],
            MeshPart::Main,
        )?;
        let (laws, comm) = {
            let m = mesh.read()?;
            let has_ghosts = m
                .element_types(TypeFilter {
                    spatial_dimension: None,
                    ghost_type: GhostType::Ghost,
                    kind: None,
                })
                .any(|t| m.nb_element(t, GhostType::Ghost) > 0);
            // upstream registers the element synchronizer for _diffusion,
            // _diffusion_gradient and _constitutive_law_id when the mesh is
            // distributed: a mesh with ghosts must be a distributed one
            if has_ghosts && m.synchronizers().element.is_none() {
                return Err(Error::Mesh(
                    "DiffusionModel: the mesh has ghost elements but no element synchronizer \
                     (distribute it with akantu_mesh::distribution)"
                        .into(),
                ));
            }
            (
                ConstitutiveLawsHandler::new(
                    id,
                    &m,
                    base.spatial_dimension,
                    ParserType::ConstitutiveLaw,
                )?,
                m.communicator().clone(),
            )
        };
        let mut factory = DiffusionLawFactory::default();
        register_builtin_laws(&mut factory, kind)?;
        let mut physics = DiffusionPhysics {
            base,
            laws,
            factory,
            upstream_compat: DiffusionUpstreamCompat::default(),
            kind,
            dof_name: dof_name.to_owned(),
            external_flow: None,
            internal_flow: None,
            diffusion_release: -1,
            need_to_reassemble_capacity: true,
            need_to_reassemble_capacity_lumped: true,
            time_step: 0.0,
            flow_scratch: (Array::new(1), Array::new(1)),
        };
        // upstream initDOFManager(dof_manager) (`diffusion_model.cc:65`,
        // `model_solver.cc:78-95`): the section's dof_manager_type through
        // the DOF-manager factory (`default`; `petsc` only in a build with
        // akantu-solver's opt-in `petsc` feature, GPL-3.0-or-later as a
        // whole, D-076) and the section's solvers/schemes
        let section = physics.base.parser_section().cloned();
        let ty = match &section {
            Some(s) => s
                .get_parameter("dof_manager_type", akantu_parser::SearchScope::Current)
                .map(|p| p.to_word().to_string_lossy().into_owned())
                .unwrap_or_else(|_| "default".to_owned()),
            None => "default".to_owned(),
        };
        let dof_manager = akantu_solver::model_solver::new_dof_manager(&ty, id, &comm)?;
        let mut solver = ModelSolver::new(dof_manager);
        if let Some(section) = section {
            // upstream's global `Parser::isPermissive()` for the
            // `non_linear_solver` sub-section
            let permissive = physics.base.parser.is_permissive();
            solver.init_from_section(&section, &mut physics, permissive)?;
        }
        {
            let m = mesh.read()?;
            let dim = physics.base.spatial_dimension;
            physics.base.fems.register(&format!("{id}:fem"), &m, dim)?;
        }
        let dumpers = DiffusionDumpers::new(id, physics.base.spatial_dimension)?;
        physics.time_step = solver.time_step(None).unwrap_or(0.0);
        Ok(Self {
            physics,
            solver,
            token,
            dumpers,
        })
    }

    /// Upstream `initFull(options)` (MODEL-002): method; default solver for
    /// the method if none exists (UD-009 preserved); laws from the parser
    /// (model section, else the root) when a file was parsed, assigned and
    /// initialised. Laws registered with [`DiffusionModel::register_new_law`]
    /// before are kept.
    pub fn init_full(&mut self, options: &ModelOptions) -> Result<()> {
        self.physics.base.method = options.analysis_method;
        if !self.solver.has_default_solver() {
            let id = self.physics.default_solver_id(options.analysis_method);
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
            let instantiated = {
                let mesh_h = self.physics.base.mesh.clone();
                let mesh = mesh_h.read()?;
                let (laws, factory, services) =
                    self.physics.split_factory(&mesh, &self.solver.dof_manager);
                laws.instantiate_laws(factory, &section, &services, permissive)
            };
            self.init_laws_after(instantiated)?;
        }
        self.physics.time_step = self.solver.time_step(None).unwrap_or(0.0);
        Ok(())
    }

    /// Upstream `initFull` with the default options except the method.
    pub fn init_full_with(&mut self, method: AnalysisMethod) -> Result<()> {
        self.init_full(&ModelOptions::new(method))
    }

    /// Upstream `initConstitutiveLaws()`: assign the not-ghost elements to
    /// the laws with the selector (ghost elements receive their owner's law
    /// over `_constitutive_law_id`), then `initConstitutiveLaw` of every law
    /// (`resizeInternals`, `updateInternalParameters`). Upstream's
    /// `_clh_init_cl` synchronisation has no synchronizer registered for the
    /// diffusion model (nothing is exchanged).
    pub fn init_laws(&mut self) -> Result<()> {
        self.init_laws_after(Ok(()))
    }

    /// [`DiffusionModel::init_laws`] after a local step `pre` (the law
    /// instantiation of `init_full`) whose failure must be agreed too. On a
    /// distributed mesh the whole initialisation is one collective scope
    /// with the solid model's agreed structure: a
    /// failed instantiation is agreed fatal, a failed selection recoverable
    /// (nothing committed), the commit, the law-id exchange and the law
    /// initialisation fatal.
    fn init_laws_after(&mut self, pre: Result<()>) -> Result<()> {
        let mesh_h = self.physics.base.mesh.clone();
        let mesh = mesh_h.read()?;
        let Some(es) = &mesh.synchronizers().element else {
            pre?;
            self.check_has_laws()?;
            self.check_regular_kinds(&mesh)?;
            self.physics.laws.assign_laws_to_elements(&mesh, None)?;
            return self.init_each_law(&mesh);
        };
        let mut ph = Phases::new(&es.communicator, "diffusion:init_laws");
        let r = self.init_laws_in(&mut ph, &mesh, es, pre);
        ph.settle(r)
    }

    fn init_laws_in(
        &mut self,
        ph: &mut Phases,
        mesh: &Mesh,
        es: &akantu_comm::Synchronizer<Element>,
        pre: Result<()>,
    ) -> Result<()> {
        ph.agree("instantiate", pre, Severity::Fatal)?;
        let selection = self
            .check_has_laws()
            .and_then(|()| self.check_regular_kinds(mesh))
            .and_then(|()| self.physics.laws.select_laws(mesh, None));
        let selection = ph.agreed("select", selection, Severity::Recoverable)?;
        let committed = self.physics.laws.commit_selection(selection);
        ph.agree("assign", committed, Severity::Fatal)?;
        let mut acc = parallel::LawIdAccessor {
            laws: &mut self.physics.laws,
        };
        es.synchronize_in(
            ph,
            &mut acc,
            akantu_core::enums::SynchronizationTag::ConstitutiveLawId,
        )
        .map_err(|e| Error::fatal(akantu_core::FatalKind::Agreed, e))?;
        let init = self.init_each_law(mesh);
        ph.agree("init_constitutive_law", init, Severity::Fatal)
    }

    /// UD-811: non-regular elements of the model dimension are refused at
    /// law assignment (before anything is committed).
    fn check_regular_kinds(&self, mesh: &Mesh) -> Result<()> {
        check_regular_kinds(
            mesh,
            &self.physics.base.id,
            self.physics.base.spatial_dimension,
        )
    }

    fn check_has_laws(&self) -> Result<()> {
        if self.physics.laws.nb_laws() == 0 {
            return Err(Error::invalid("No constitutive_law to initialize !"));
        }
        Ok(())
    }

    /// `initConstitutiveLaw` of every law, in creation order.
    fn init_each_law(&mut self, mesh: &Mesh) -> Result<()> {
        let p = &mut self.physics;
        let (laws, services) = p.split(mesh, &self.solver.dof_manager);
        for law in laws.laws_mut() {
            law.init_law(&services)?;
        }
        Ok(())
    }

    /// Upstream `registerNewConstitutiveLaw(name, type, option)`.
    pub fn register_new_law(&mut self, name: &str, ty: &str, option: &str) -> Result<usize> {
        let mesh_h = self.physics.base.mesh.clone();
        let mesh = mesh_h.read()?;
        let (laws, factory, services) = self.physics.split_factory(&mesh, &self.solver.dof_manager);
        let i = laws.register_new_law(factory, name, ty, option, &services)?;
        laws.law_mut(i)?.base_mut().set_name(name);
        Ok(i)
    }

    /// Upstream `setConstitutiveLawSelector(selector)`: the selector with
    /// the default chain as fallback.
    pub fn set_law_selector(&mut self, selector: Box<dyn ConstitutiveLawSelector>) {
        self.physics.laws.selector = SelectorChain::with_fallback_to_default(selector);
    }

    /// Replace the whole selector chain.
    pub fn set_law_selector_chain(&mut self, chain: SelectorChain) {
        self.physics.laws.selector = chain;
    }

    /// Upstream `reassignConstitutiveLaw()` (D-078): the selector is
    /// asked again for every element and the elements whose law changes
    /// move to it (`ConstitutiveLawsHandler::reassign_constitutive_law`).
    ///
    /// # Errors
    /// An element without a law, a selector result out of range, errors of
    /// the laws.
    pub fn reassign_law(&mut self) -> Result<()> {
        use akantu_model::constitutive_law::ModelServices;
        let mesh_h = self.physics.base.mesh.clone();
        let mesh = mesh_h.read()?;
        let (laws, services) = self.physics.split(&mesh, &self.solver.dof_manager);
        let nq = |ty: ElementType, g: GhostType| services.nb_integration_points(ty, g);
        laws.reassign_constitutive_law(&mesh, &nq)
    }

    /// Solve one step (upstream `solveStep(solver_id)`).
    pub fn solve_step(&mut self, solver_id: Option<&str>) -> Result<()> {
        self.physics.base.mesh.check_current(&self.token)?;
        self.physics.time_step = self.solver.time_step(solver_id)?;
        self.solver.solve_step(&mut self.physics, solver_id)
    }

    /// Solve one step with an intercepting user callback.
    pub fn solve_step_intercepted(
        &mut self,
        user: &mut dyn akantu_solver::callback::InterceptCallback,
        solver_id: Option<&str>,
    ) -> Result<()> {
        self.physics.base.mesh.check_current(&self.token)?;
        self.physics.time_step = self.solver.time_step(solver_id)?;
        self.solver
            .solve_step_intercepted(&mut self.physics, user, solver_id)
    }

    /// Upstream `setTimeStep(dt, solver_id)` (also the time step of the
    /// model's own dumper, named after the model id).
    pub fn set_time_step(&mut self, time_step: Real, solver_id: Option<&str>) -> Result<()> {
        self.solver.set_time_step(time_step, solver_id)?;
        self.physics.time_step = self.solver.time_step(None).unwrap_or(time_step);
        self.dumpers.set_time_step(time_step)
    }

    /// Upstream `getTimeStep(solver_id)`.
    pub fn time_step(&self, solver_id: Option<&str>) -> Result<Real> {
        self.solver.time_step(solver_id)
    }

    /// Upstream `getNewSolver(id, options)`.
    pub fn get_new_solver(&mut self, solver_id: &str, options: &ModelSolverOptions) -> Result<()> {
        self.solver
            .get_new_solver(solver_id, options, &mut self.physics)
    }

    /// Upstream `getNewSolver(id, time_step_solver_type, non_linear_solver_type)`.
    pub fn get_new_solver_of_type(
        &mut self,
        solver_id: &str,
        tss: TimeStepSolverType,
        nls: Option<NonLinearSolverType>,
    ) -> Result<()> {
        let options = ModelSolverOptions {
            time_step_solver_type: Some(tss),
            non_linear_solver_type: nls,
            ..ModelSolverOptions::default()
        };
        self.get_new_solver(solver_id, &options)
    }

    /// Upstream `setIntegrationScheme(solver_id, dof_id, type, solution_type)`.
    pub fn set_integration_scheme(
        &mut self,
        solver_id: &str,
        dof_id: &str,
        ty: IntegrationSchemeType,
        solution_type: SolutionType,
    ) -> Result<()> {
        self.solver
            .set_integration_scheme(solver_id, dof_id, ty, solution_type, &self.physics)
    }

    /// Upstream `setIntegrationScheme(solver_id, dof_id, scheme, solution_type)`.
    pub fn set_integration_scheme_instance(
        &mut self,
        solver_id: &str,
        dof_id: &str,
        scheme: Box<dyn IntegrationScheme>,
        solution_type: SolutionType,
    ) -> Result<()> {
        self.solver.set_integration_scheme_instance(
            solver_id,
            dof_id,
            scheme,
            solution_type,
            &self.physics,
        )
    }

    /// Upstream `initNewSolver(method)`: the method's default solver
    /// (created with the model's default options if missing, UD-009)
    /// becomes the default solver. The method is set first (UD-212
    /// correction, as SMM): the diffusion options of `_dynamic` depend on
    /// it.
    pub fn init_new_solver(&mut self, method: AnalysisMethod) -> Result<()> {
        self.physics.base.method = method;
        let id = self.physics.default_solver_id(method);
        init_new_solver(
            &mut self.solver,
            &mut self.physics,
            id,
            ModelSolverOptions::default(),
        )?;
        self.physics.base.method = method;
        self.physics.time_step = self.solver.time_step(None).unwrap_or(0.0);
        Ok(())
    }

    /// The DOF manager (upstream `getDOFManager()`).
    #[must_use]
    pub fn dof_manager(&self) -> &DofManager {
        &self.solver.dof_manager
    }

    /// Mutable DOF manager.
    pub fn dof_manager_mut(&mut self) -> &mut DofManager {
        &mut self.solver.dof_manager
    }

    // ---- nodal fields ---------------------------------------------------

    fn family(&self) -> Result<&akantu_solver::dof_manager::DofData> {
        self.solver
            .dof_manager
            .family(&self.physics.dof_name)
            .map_err(|_| not_allocated(&self.physics.dof_name))
    }

    fn family_mut(&mut self) -> Result<&mut akantu_solver::dof_manager::DofData> {
        let name = self.physics.dof_name.clone();
        self.solver
            .dof_manager
            .family_mut(&name)
            .map_err(|_| not_allocated(&name))
    }

    /// Upstream `getDOFName()`.
    #[must_use]
    pub fn dof_name(&self) -> &str {
        &self.physics.dof_name
    }

    /// Upstream `getDiffusion()` (the temperature of a heat-transfer model).
    pub fn diffusion(&self) -> Result<ArrayRef<'_, Real>> {
        self.family()?.dofs.read()
    }

    /// Mutable diffusion field (bumps nothing, as upstream: the gradient
    /// cache keeps its release until the next predictor / corrector).
    pub fn diffusion_mut(&mut self) -> Result<ArrayMut<'_, Real>> {
        self.family_mut()?.dofs.write()
    }

    /// Upstream `getDiffusionRate()` (dynamic solvers only).
    pub fn diffusion_rate(&self) -> Result<ArrayRef<'_, Real>> {
        let name = format!("{}_rate", self.physics.dof_name);
        self.family()?
            .derivatives
            .get(&1)
            .ok_or_else(|| not_allocated(&name))?
            .read()
    }

    /// Mutable diffusion rate (not exposed by upstream; convenience).
    pub fn diffusion_rate_mut(&mut self) -> Result<ArrayMut<'_, Real>> {
        let name = format!("{}_rate", self.physics.dof_name);
        self.family_mut()?
            .derivatives
            .get_mut(&1)
            .ok_or_else(|| not_allocated(&name))?
            .write()
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

    /// Upstream `getExternalFlow()`.
    pub fn external_flow(&self) -> Result<ArrayRef<'_, Real>> {
        self.physics
            .external_flow
            .as_ref()
            .ok_or_else(|| not_allocated("external_flow"))?
            .read()
    }

    /// Mutable external flow.
    pub fn external_flow_mut(&mut self) -> Result<ArrayMut<'_, Real>> {
        self.physics
            .external_flow
            .as_mut()
            .ok_or_else(|| not_allocated("external_flow"))?
            .write()
    }

    /// Upstream `getInternalFlow()` (`−∫ Bᵀ D ∇u`, assembled by the last
    /// residual assembly).
    pub fn internal_flow(&self) -> Result<ArrayRef<'_, Real>> {
        self.physics
            .internal_flow
            .as_ref()
            .ok_or_else(|| not_allocated("internal_flow"))?
            .read()
    }

    /// Mutable internal flow.
    pub fn internal_flow_mut(&mut self) -> Result<ArrayMut<'_, Real>> {
        self.physics
            .internal_flow
            .as_mut()
            .ok_or_else(|| not_allocated("internal_flow"))?
            .write()
    }

    /// Upstream `getDiffusionRelease()`.
    #[must_use]
    pub fn diffusion_release(&self) -> i64 {
        self.physics.diffusion_release
    }

    /// Analysis method.
    #[must_use]
    pub fn analysis_method(&self) -> AnalysisMethod {
        self.physics.base.method
    }

    /// Spatial dimension.
    #[must_use]
    pub fn spatial_dimension(&self) -> usize {
        self.physics.base.spatial_dimension
    }

    /// Model id.
    #[must_use]
    pub fn id(&self) -> &str {
        &self.physics.base.id
    }

    /// Kind (generic or heat transfer).
    #[must_use]
    pub fn kind(&self) -> DiffusionModelKind {
        self.physics.kind
    }

    // ---- laws ------------------------------------------------------------

    /// Upstream `getNbConstitutiveLaws()`.
    #[must_use]
    pub fn nb_laws(&self) -> usize {
        self.physics.laws.nb_laws()
    }

    /// Upstream `getConstitutiveLaw(index)`.
    pub fn law(&self, index: usize) -> Result<&dyn DiffusionLaw> {
        self.physics.laws.law(index)
    }

    /// Mutable law.
    pub fn law_mut(&mut self, index: usize) -> Result<&mut dyn DiffusionLaw> {
        let law: &mut (dyn DiffusionLaw + 'static) = self.physics.laws.law_mut(index)?;
        Ok(law)
    }

    /// Upstream `getConstitutiveLaw(name)`.
    pub fn law_by_name(&self, name: &str) -> Result<&dyn DiffusionLaw> {
        let i = self.physics.laws.law_index_of(name)?;
        self.physics.laws.law(i)
    }

    /// Upstream `getConstitutiveLawIndex(name)`.
    pub fn law_index(&self, name: &str) -> Result<usize> {
        self.physics.laws.law_index_of(name)
    }

    /// Upstream `getConstitutiveLawByElement()` (law index per element,
    /// `-1` = none).
    #[must_use]
    pub fn law_by_element(&self) -> &SharedElementTypeMapArray<Idx> {
        self.physics.laws.law_index_map()
    }

    /// Mutable law index map (filled by users before `init_full` to drive
    /// the default selector).
    pub fn law_by_element_mut(&mut self) -> &mut SharedElementTypeMapArray<Idx> {
        self.physics.laws.law_index_map_mut()
    }

    /// Upstream `getConstitutiveLawLocalNumbering()`.
    #[must_use]
    pub fn law_local_numbering(&self) -> &ElementTypeMapArray<Idx> {
        self.physics.laws.local_numbering_map()
    }

    /// The internal `name` (`∇u`, `D∇u`, `diffusivity`) of law `law` for
    /// `(ty, ghost)` (upstream `getArray<Real>(name, type, ghost)`).
    pub fn law_internal(
        &self,
        law: usize,
        name: &str,
        ty: ElementType,
        ghost: GhostType,
    ) -> Result<ArrayRef<'_, Real>> {
        self.physics
            .laws
            .law(law)?
            .base()
            .internal(name)?
            .get(ty, ghost)
    }

    // ---- boundary conditions (port extension) ---------------------------

    /// Dirichlet functor on an element group's nodes (values and blocked
    /// flags of the diffusion family, one component). **Port extension:**
    /// upstream's `DiffusionModel` has no `applyBC` (only
    /// `SolidMechanicsModel` derives from `BoundaryCondition`); users set
    /// `getBlockedDOFs()` / `getTemperature()` directly, which remains
    /// available.
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
        self.apply_dirichlet_on(functor, &mesh, g)
            .map_err(|e| group_error(group, &e))
    }

    fn apply_dirichlet_on(
        &mut self,
        functor: &dyn DirichletFunctor,
        mesh: &Mesh,
        group: ElementGroupRef<'_>,
    ) -> Result<()> {
        let f = self.family_mut()?;
        let blocked = f
            .blocked
            .as_mut()
            .ok_or_else(|| not_allocated("blocked_dofs"))?;
        apply_dirichlet(functor, mesh, group, &mut f.dofs, blocked)
    }

    // ---- explicit assembly entry points ---------------------------------

    /// Upstream `assembleDiffisivityMatrix()` (outside a solve: one
    /// collective operation; `K` must exist, as upstream asserts — create it
    /// with `dof_manager_mut().new_matrix("K", Symmetric)`).
    pub fn assemble_diffusivity_matrix(&mut self) -> Result<()> {
        let p = &mut self.physics;
        self.solver
            .dof_manager
            .collective("diffusion:assemble_diffusivity", |dm| {
                p.assemble_diffusivity_matrix(dm)?;
                dm.finalize_matrix("K")
            })
    }

    /// Upstream `assembleCapacityMatrix()` (consistent capacity `M`,
    /// created when missing).
    pub fn assemble_capacity_matrix(&mut self) -> Result<()> {
        let p = &mut self.physics;
        self.solver
            .dof_manager
            .collective("diffusion:assemble_capacity", |dm| {
                if !dm.has_matrix("M") {
                    dm.new_matrix("M", MatrixType::Symmetric)?;
                }
                p.assemble_capacity_matrix(dm)?;
                dm.finalize_matrix("M")
            })
    }

    /// Upstream `assembleCapacityMatrixLumped()` (lumped capacity `M`).
    pub fn assemble_capacity_matrix_lumped(&mut self) -> Result<()> {
        let p = &mut self.physics;
        self.solver
            .dof_manager
            .collective("diffusion:assemble_capacity_lumped", |dm| {
                p.assemble_capacity_matrix_lumped(dm)
            })
    }

    /// Upstream `assembleResidual()`: **accumulates** into the DOF
    /// manager's residual (`r += f_ext + f_int`).
    pub fn assemble_residual(&mut self) -> Result<()> {
        let p = &mut self.physics;
        self.solver
            .dof_manager
            .collective("diffusion:assemble_residual", |dm| p.assemble_residual(dm))
    }

    /// Upstream `DiffusionLaw::computeDiffusivityGradU(ghost)` for every
    /// law (creation order): `∇u` and `D∇u` of the law's elements of
    /// `ghost` from the current nodal field, nothing exchanged
    /// (MPI-043: with [`DiffusionModel::synchronize`]`(_diffusion_gradient)`
    /// the oracle's distributed gradient exchange, upstream's only
    /// distributed path without a residual assembly, UD-275). Local; on a
    /// distributed mesh the ghost elements' gradients use the local nodal
    /// values of their nodes.
    ///
    /// # Errors
    /// Mesh lock, missing DOF family, law errors.
    pub fn compute_diffusivity_grad_u(&mut self, ghost: GhostType) -> Result<()> {
        let mesh_h = self.physics.base.mesh.clone();
        let mesh = mesh_h.read()?;
        self.physics.with_laws(
            &mesh,
            &self.solver.dof_manager,
            LawPhase::Stress,
            |laws, ctx| {
                for law in laws.laws_mut() {
                    law.compute_diffusivity_grad_u(ctx, ghost)?;
                }
                Ok(())
            },
        )
    }

    /// The lumped capacity per DOF (upstream
    /// `getDOFManager().getLumpedMatrixPerDOFs(dof, "M", out)`).
    pub fn lumped_capacity(&self) -> Result<Array<Real>> {
        let mut out = Array::new(1);
        self.solver
            .dof_manager
            .lumped_matrix_per_dofs(&self.physics.dof_name, "M", &mut out)?;
        Ok(out)
    }

    /// **Port extension**: set the rate of the free DOFs
    /// to the consistent initial rate `u̇⁰ = M⁻¹(f − K u⁰)` (consistent
    /// capacity, current temperature and external flow; blocked DOFs keep
    /// their rate). The corrected trapezoidal rule 1 (Crank–Nicolson,
    /// UD-287) is second order from this rate and first order from the
    /// default `u̇⁰ = 0`, with which its first step is half a
    /// backward-Euler step; backward Euler is unaffected (its `(1 − α)` is
    /// 0). Upstream has no such operation: every upstream run starts from
    /// `u̇⁰ = 0`, and nothing changes unless this is called.
    ///
    /// Implementation: one step of upstream's explicit-consistent-mass
    /// configuration (`getDefaultSolverOptions(_dynamic)` with
    /// `_explicit_consistent_mass`: Newton–Raphson, forward Euler,
    /// `_temperature_rate`) with `Δt = 0`, on a time-step solver of its own
    /// ([`CONSISTENT_RATE_SOLVER`], created on first use with the default
    /// solver's sparse solver kind; the default solver is unchanged):
    /// `J = M`, residual `f_ext + f_int − M u̇`, corrector `u̇ += δ`, and
    /// `u` untouched (`u += (1 − α)·Δt·u̇` with `Δt = 0`, `e = αΔt = 0`).
    /// Newton–Raphson stops on the residual criterion (`1e-12` relative to
    /// the first residual): one linear solve. Collective.
    ///
    /// # Errors
    /// No rate registered (`init_full` with a dynamic method first), or the
    /// solve's errors.
    pub fn set_consistent_initial_rate(&mut self) -> Result<()> {
        let dof = self.physics.dof_name.clone();
        if !self.solver.dof_manager.has_dofs(&dof)
            || !self.solver.dof_manager.has_dofs_derivative(&dof, 1)?
        {
            return Err(not_allocated(&format!("{dof}_rate")));
        }
        if !self.solver.has_solver(CONSISTENT_RATE_SOLVER) {
            let sparse = self.solver.dof_manager.backend().sparse_solver_kind("J");
            let mut options = ModelSolverOptions {
                time_step_solver_type: Some(TimeStepSolverType::Dynamic),
                non_linear_solver_type: Some(NonLinearSolverType::NewtonRaphson),
                sparse_solver_type: sparse,
                ..ModelSolverOptions::default()
            };
            options
                .integration_scheme_type
                .insert(dof.clone(), IntegrationSchemeType::ForwardEuler);
            options
                .solution_type
                .insert(dof.clone(), SolutionType::TEMPERATURE_RATE);
            self.solver
                .get_new_solver(CONSISTENT_RATE_SOLVER, &options, &mut self.physics)?;
            self.solver.set_integration_scheme(
                CONSISTENT_RATE_SOLVER,
                &dof,
                IntegrationSchemeType::ForwardEuler,
                SolutionType::TEMPERATURE_RATE,
                &self.physics,
            )?;
            let nls = self
                .solver
                .non_linear_solver_mut(Some(CONSISTENT_RATE_SOLVER))?;
            nls.set_param(
                "convergence_type",
                akantu_parser::ParamValue::Enum("residual".into()),
            )?;
            nls.set_param("threshold", akantu_parser::ParamValue::Real(1e-12))?;
            nls.set_param("max_iterations", akantu_parser::ParamValue::Int(10))?;
        }
        self.solver
            .set_time_step(0.0, Some(CONSISTENT_RATE_SOLVER))?;
        let r = self.solve_step(Some(CONSISTENT_RATE_SOLVER));
        // the model's time step is the default solver's again
        self.physics.time_step = self.solver.time_step(None).unwrap_or(0.0);
        r
    }

    /// Upstream `forceReassemble`-like helper (not upstream): mark both
    /// capacities for reassembly.
    pub fn force_capacity_reassembly(&mut self) {
        self.physics.need_to_reassemble_capacity = true;
        self.physics.need_to_reassemble_capacity_lumped = true;
    }
}

impl DiffusionPhysics {
    /// Upstream `getDefaultSolverID(method)` (DIFF-003): explicit lumped →
    /// `explicit_lumped` (`_dynamic_lumped`), static → `static`, implicit
    /// dynamic → `implicit` (`_dynamic`), anything else (including
    /// `_explicit_consistent_mass`) → `unknown` (`_not_defined`).
    #[must_use]
    pub fn default_solver_id(&self, method: AnalysisMethod) -> (Id, TimeStepSolverType) {
        match method {
            AnalysisMethod::ExplicitLumpedMass => {
                ("explicit_lumped".into(), TimeStepSolverType::DynamicLumped)
            }
            AnalysisMethod::Static => ("static".into(), TimeStepSolverType::Static),
            AnalysisMethod::ImplicitDynamic => ("implicit".into(), TimeStepSolverType::Dynamic),
            _ => ("unknown".into(), TimeStepSolverType::NotDefined),
        }
    }

    /// DOF name.
    #[must_use]
    pub fn dof_name(&self) -> &str {
        &self.dof_name
    }

    /// Split into the laws and the services reading everything else.
    pub(crate) fn split<'a>(
        &'a mut self,
        mesh: &'a Mesh,
        dofs: &'a DofManager,
    ) -> (
        &'a mut ConstitutiveLawsHandler<dyn DiffusionLaw>,
        DiffusionServices<'a>,
    ) {
        let (laws, _, services) = self.split_factory(mesh, dofs);
        (laws, services)
    }

    /// [`DiffusionPhysics::split`] with the factory.
    pub(crate) fn split_factory<'a>(
        &'a mut self,
        mesh: &'a Mesh,
        dofs: &'a DofManager,
    ) -> (
        &'a mut ConstitutiveLawsHandler<dyn DiffusionLaw>,
        &'a DiffusionLawFactory,
        DiffusionServices<'a>,
    ) {
        let Self {
            base,
            laws,
            factory,
            dof_name,
            external_flow,
            internal_flow,
            time_step,
            ..
        } = self;
        let (diffusion, diffusion_rate) = services::dof_arrays(dofs, dof_name);
        let services = DiffusionServices {
            mesh,
            fems: &base.fems,
            diffusion,
            diffusion_rate,
            dof_name,
            external_flow: external_flow.as_ref(),
            internal_flow: internal_flow.as_ref(),
            spatial_dimension: base.spatial_dimension,
            time_step: *time_step,
        };
        (laws, factory, services)
    }

    /// Run `f` for every law with its phase context (`u` from the DOF
    /// manager, the model's release).
    pub(crate) fn with_laws<R>(
        &mut self,
        mesh: &Mesh,
        dofs: &DofManager,
        phase: LawPhase,
        f: impl FnOnce(
            &mut ConstitutiveLawsHandler<dyn DiffusionLaw>,
            &DiffusionLawContext<'_>,
        ) -> Result<R>,
    ) -> Result<R> {
        let release = self.diffusion_release;
        let compat = self.upstream_compat;
        let dof_name = self.dof_name.clone();
        // read guard on the DOF values for the duration of `f` (scoped by
        // this function, D-071)
        let u = dofs.dofs(&dof_name).map_err(|_| not_allocated(&dof_name))?;
        let (laws, services) = self.split(mesh, dofs);
        let ctx = DiffusionLawContext {
            law: LawContext {
                phase,
                services: &services,
            },
            diffusion: &u,
            release,
            compat,
        };
        let r = f(laws, &ctx);
        drop(u);
        r
    }

    /// Upstream `getMatrixType(id)`: `K` and `M` symmetric, anything else
    /// not defined.
    #[must_use]
    pub fn model_matrix_type(&self, matrix_id: &str) -> MatrixType {
        if matrix_id == "K" || matrix_id == "M" {
            MatrixType::Symmetric
        } else {
            MatrixType::NotDefined
        }
    }

    /// Model-owned arrays on node additions (not handled upstream, whose
    /// flows would keep their old size; the port resizes them and marks the
    /// capacities for reassembly).
    fn on_nodes_added(&mut self, mesh: &Mesh) -> Result<()> {
        let n = mesh.nb_nodes();
        self.diffusion_release += 1;
        for a in [&mut self.external_flow, &mut self.internal_flow]
            .into_iter()
            .flatten()
        {
            a.write()?.try_resize(n, 0.0)?;
        }
        self.need_to_reassemble_capacity = true;
        self.need_to_reassemble_capacity_lumped = true;
        Ok(())
    }
}

impl SolverCallback for DiffusionPhysics {
    fn matrix_type(&self, matrix_id: &str) -> MatrixType {
        self.model_matrix_type(matrix_id)
    }

    /// Upstream `assembleMatrix(id)`: `K` → conductivity matrix, `M` →
    /// consistent capacity (only while `need_to_reassemble_capacity`).
    fn assemble_matrix(&mut self, dofs: &mut DofManager, matrix_id: &str) -> Result<()> {
        match matrix_id {
            "K" => self.assemble_diffusivity_matrix(dofs),
            "M" if self.need_to_reassemble_capacity => self.assemble_capacity_matrix(dofs),
            _ => Ok(()),
        }
    }

    /// Upstream `assembleLumpedMatrix(id)`: `M` → lumped capacity, gated by
    /// the lumped flag (UD-281; upstream's consistent flag with
    /// `upstream_compat.lumped_capacity_gated_by_consistent_flag`).
    fn assemble_lumped_matrix(&mut self, dofs: &mut DofManager, matrix_id: &str) -> Result<()> {
        if matrix_id != "M" {
            return Ok(());
        }
        let need = if self
            .upstream_compat
            .lumped_capacity_gated_by_consistent_flag
        {
            self.need_to_reassemble_capacity
        } else {
            self.need_to_reassemble_capacity_lumped
        };
        if need {
            self.assemble_capacity_matrix_lumped(dofs)
        } else {
            Ok(())
        }
    }

    fn assemble_residual(&mut self, dofs: &mut DofManager) -> Result<()> {
        DiffusionPhysics::assemble_residual(self, dofs)
    }

    /// Upstream `predictor()`: `++diffusion_release`.
    fn predictor(&mut self, _dofs: &mut DofManager) -> Result<()> {
        self.diffusion_release += 1;
        Ok(())
    }

    /// Upstream `corrector()`: `++diffusion_release`.
    fn corrector(&mut self, _dofs: &mut DofManager) -> Result<()> {
        self.diffusion_release += 1;
        Ok(())
    }

    /// Upstream `initSolver(type)` (DIFF-003): allocate `external_flow` and
    /// `internal_flow`; register the DOF family `<dof_name>` (one
    /// component) and its blocked flags if missing; for dynamic solvers the
    /// rate as derivative 1 if missing (upstream's off-by-one
    /// `hasDOFsDerivatives` makes a second dynamic solver fail, UD-012: the
    /// port proceeds).
    fn init_solver(&mut self, dofs: &mut DofManager, ty: TimeStepSolverType) -> Result<()> {
        let mesh_h = self.base.mesh.clone();
        let mesh = mesh_h.read()?;
        let n = mesh.nb_nodes();
        drop(alloc_nodal_shared_field(&mut self.external_flow, n, 1)?);
        drop(alloc_nodal_shared_field(&mut self.internal_flow, n, 1)?);
        let name = self.dof_name.clone();
        if !dofs.has_dofs(&name) {
            dofs.register_nodal_dofs(&mesh, &name, 1)?;
            dofs.register_blocked_dofs(&name)?;
        }
        if matches!(
            ty,
            TimeStepSolverType::Dynamic | TimeStepSolverType::DynamicLumped
        ) && !dofs.has_dofs_derivative(&name, 1)?
        {
            dofs.register_dofs_derivative(&name, 1)?;
        }
        Ok(())
    }

    /// Upstream `Model::getDefaultSolverType()` (not overridden):
    /// `dynamic_lumped`.
    fn default_solver_type(&self) -> TimeStepSolverType {
        TimeStepSolverType::DynamicLumped
    }

    /// Upstream `getDefaultSolverOptions(type)` (DIFF-003).
    fn default_solver_options(&self, ty: TimeStepSolverType) -> ModelSolverOptions {
        let mut o = ModelSolverOptions::default();
        let (nls, scheme, st) = match ty {
            TimeStepSolverType::DynamicLumped => (
                NonLinearSolverType::Lumped,
                IntegrationSchemeType::ForwardEuler,
                SolutionType::TEMPERATURE_RATE,
            ),
            TimeStepSolverType::Static => (
                NonLinearSolverType::NewtonRaphson,
                IntegrationSchemeType::PseudoTime,
                SolutionType::NotDefined,
            ),
            TimeStepSolverType::Dynamic => {
                if self.base.method == AnalysisMethod::ExplicitConsistentMass {
                    (
                        NonLinearSolverType::NewtonRaphson,
                        IntegrationSchemeType::ForwardEuler,
                        SolutionType::TEMPERATURE_RATE,
                    )
                } else {
                    (
                        NonLinearSolverType::NewtonRaphson,
                        IntegrationSchemeType::BackwardEuler,
                        SolutionType::TEMPERATURE,
                    )
                }
            }
            // upstream: AKANTU_EXCEPTION(type << " is not a valid time step
            // solver type"); the options stay unset and getNewSolver fails
            TimeStepSolverType::NotDefined => return o,
        };
        o.non_linear_solver_type = Some(nls);
        o.integration_scheme_type
            .insert(self.dof_name.clone(), scheme);
        o.solution_type.insert(self.dof_name.clone(), st);
        o
    }
}

impl MeshEventHandler for DiffusionModel {
    fn event_priority(&self) -> EventPriority {
        EventPriority::MODEL
    }

    fn event_priorities(&self) -> Vec<EventPriority> {
        vec![
            EventPriority::FE_ENGINE,
            EventPriority::DOF_MANAGER,
            EventPriority::MODEL,
        ]
    }

    fn on_mesh_event(&mut self, mesh: &Mesh, event: &MeshEvent) -> Result<()> {
        self.on_mesh_event_at(mesh, event, EventPriority::MODEL)
    }

    /// D-068: the refusals of the dispatch, stated before the commit at the
    /// same levels — the FE engines' and the DOF manager's checks
    /// (forwarded), then the model's own (see `diffusion_check_mesh_event`).
    fn check_mesh_event(&self, mesh: &Mesh, event: &MeshEvent) -> Result<()> {
        diffusion_check_mesh_event(&self.physics, &self.solver, mesh, event)
    }

    /// FE engines (9), DOF manager (20), model arrays + laws (94).
    fn on_mesh_event_at(
        &mut self,
        mesh: &Mesh,
        event: &MeshEvent,
        level: EventPriority,
    ) -> Result<()> {
        match level {
            EventPriority::FE_ENGINE => self.fe_engines_on_event(mesh, event),
            EventPriority::DOF_MANAGER => self.solver.dof_manager.on_mesh_event(mesh, event),
            EventPriority::MODEL => self.model_on_event(mesh, event),
            _ => Ok(()),
        }
    }

    fn on_mesh_event_phased(
        &mut self,
        mesh: &Mesh,
        event: &MeshEvent,
        priority: EventPriority,
        phases: &mut Phases,
    ) -> Result<()> {
        match priority {
            EventPriority::DOF_MANAGER => self
                .solver
                .dof_manager
                .on_mesh_event_phased(mesh, event, priority, phases),
            _ => self.on_mesh_event_at(mesh, event, priority),
        }
    }
}

/// **UD-811** (latent B): upstream assigns every element
/// of the model dimension to a law, whatever its kind
/// (`for_each_element`, kind `_ek_not_defined`), and `addElement` throws for
/// a non-regular kind (`No element of type (not_ghost:_cohesive_2d_4) in
/// this ElementTypeMapArray<int> class ("…:element_filter")`): `initFull`
/// and an `ElementsAdded` event refuse cohesive (or structural) elements.
/// The port refuses them likewise (same error class: a missing element
/// type block), at law assignment and in the capability check.
pub(crate) fn non_regular_refused(model_id: &str, ty: ElementType, ghost: GhostType) -> Error {
    Error::not_found(
        "element type array",
        format!(
            "{model_id}: diffusion law element_filter has no block {ghost}:{ty} (the \
             diffusion laws take regular elements only, as upstream's initFull; UD-811)"
        ),
    )
}

/// The first element type of a non-regular kind of the model dimension
/// with elements (both ghost types), refused by law assignment (UD-811).
pub(crate) fn check_regular_kinds(mesh: &Mesh, model_id: &str, dim: usize) -> Result<()> {
    for ghost in GhostType::ALL {
        for ty in mesh.element_types(TypeFilter {
            spatial_dimension: Some(dim),
            ghost_type: ghost,
            kind: None,
        }) {
            if ty.kind() != akantu_core::ElementKind::Regular && mesh.nb_element(ty, ghost) > 0 {
                return Err(non_regular_refused(model_id, ty, ghost));
            }
        }
    }
    Ok(())
}

/// The model's node-removal refusal (as SMM, UD-209).
fn node_removal_refused() -> Error {
    Error::NotImplemented(
        "DiffusionModel: node removal (the DOF equation numbers would go stale, UD-209)",
    )
}

/// [`MeshEventHandler::check_mesh_event`] of a diffusion model (D-068):
/// forwards to the FE engines and the DOF manager (the handlers its
/// dispatch forwards to), then refuses what the model level of the dispatch
/// would refuse: node removal (UD-209) and added elements of a non-regular
/// kind of the model dimension (UD-811, upstream's `onElementsAdded` →
/// `addElement` throws).
fn diffusion_check_mesh_event(
    physics: &DiffusionPhysics,
    solver: &ModelSolver,
    mesh: &Mesh,
    event: &MeshEvent,
) -> Result<()> {
    physics.base.fems.check_mesh_event(mesh, event)?;
    solver.dof_manager.check_mesh_event(mesh, event)?;
    match event {
        MeshEvent::NodesRemoved { .. } => Err(node_removal_refused()),
        MeshEvent::ElementsAdded { elements, .. } => {
            refuse_non_regular_added(elements, &physics.base.id, physics.base.spatial_dimension)
        }
        _ => Ok(()),
    }
}

/// UD-811 for an `ElementsAdded` event: the first added element of a
/// non-regular kind of the model dimension is refused.
fn refuse_non_regular_added(elements: &[Element], model_id: &str, dim: usize) -> Result<()> {
    match elements.iter().find(|e| {
        e.ty().spatial_dimension() == dim && e.ty().kind() != akantu_core::ElementKind::Regular
    }) {
        Some(e) => Err(non_regular_refused(model_id, e.ty(), e.ghost_type())),
        None => Ok(()),
    }
}

impl DiffusionModel {
    fn fe_engines_on_event(&mut self, mesh: &Mesh, event: &MeshEvent) -> Result<()> {
        match event {
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
        }
    }

    fn model_on_event(&mut self, mesh: &Mesh, event: &MeshEvent) -> Result<()> {
        match event {
            MeshEvent::NodesAdded { .. } => self.physics.on_nodes_added(mesh),
            // As SMM (UD-209): the DOF equation numbers would go stale;
            // refused (fatal in the dispatch stage, the mesh domain poisons).
            MeshEvent::NodesRemoved { .. } => Err(node_removal_refused()),
            MeshEvent::ElementsAdded { elements, .. } => {
                let dim = self.physics.base.spatial_dimension;
                // refused by the capability check before the commit
                refuse_non_regular_added(elements, &self.physics.base.id, dim)?;
                let p = &mut self.physics;
                let (laws, services) = p.split(mesh, &self.solver.dof_manager);
                laws_on_elements_added(laws, &services, mesh, elements, dim)
            }
            MeshEvent::ElementsRemoved {
                elements,
                new_numbering,
                ..
            } => {
                let p = &mut self.physics;
                let (laws, services) = p.split(mesh, &self.solver.dof_manager);
                laws_on_elements_removed(laws, &services, mesh, elements, new_numbering)
            }
            _ => Ok(()),
        }
    }
}

/// Resize the handler maps to the mesh's element counts (new entries `-1`,
/// types of the mesh dimension).
fn fit_handler_maps(h: &mut ConstitutiveLawsHandler<dyn DiffusionLaw>, mesh: &Mesh) -> Result<()> {
    for ghost in GhostType::ALL {
        let types: Vec<ElementType> = mesh
            .element_types(TypeFilter {
                spatial_dimension: Some(mesh.spatial_dimension()),
                ghost_type: ghost,
                kind: None,
            })
            .collect();
        for ty in types {
            let n = mesh.nb_element(ty, ghost);
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

/// Upstream `ConstitutiveLawsHandler::onElementsAdded`: maps re-initialised
/// to the mesh, the new elements of the model dimension assigned with the
/// selector, every initialised law's internals resized (release `+1`).
fn laws_on_elements_added(
    laws: &mut ConstitutiveLawsHandler<dyn DiffusionLaw>,
    services: &DiffusionServices<'_>,
    mesh: &Mesh,
    elements: &[Element],
    spatial_dimension: usize,
) -> Result<()> {
    fit_handler_maps(laws, mesh)?;
    let mut filter = ElementTypeMapArray::<Idx>::new("new_element_filter");
    for e in elements {
        if e.ty().spatial_dimension() != spatial_dimension {
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
        if law.base().is_init {
            law.base_mut().resize_internals(services)?;
        }
    }
    Ok(())
}

/// Upstream `ConstitutiveLawsHandler::onElementsRemoved`.
fn laws_on_elements_removed(
    laws: &mut ConstitutiveLawsHandler<dyn DiffusionLaw>,
    services: &DiffusionServices<'_>,
    mesh: &Mesh,
    elements: &[Element],
    new_numbering: &ElementTypeMapArray<Idx>,
) -> Result<()> {
    use akantu_model::constitutive_law::ModelServices;
    let nq = |ty: ElementType, g: GhostType| services.nb_integration_points(ty, g);
    let mut entries_all = Vec::new();
    for i in 0..laws.nb_laws() {
        let entries = {
            let law = laws.law_mut(i)?;
            law.on_elements_removed(elements, new_numbering, &nq)?
        };
        entries_all.push((i, entries));
    }
    for ghost in GhostType::ALL {
        let types: Vec<ElementType> = mesh
            .element_types(TypeFilter {
                spatial_dimension: Some(mesh.spatial_dimension()),
                ghost_type: ghost,
                kind: None,
            })
            .collect();
        for ty in types {
            // only the blocks the event renumbers (upstream resizes the
            // maps of `new_numbering`'s types only); the others keep their
            // entries
            if !new_numbering.exists(ty, ghost) {
                continue;
            }
            let n = mesh.nb_element(ty, ghost);
            if let Some(mut a) =
                akantu_core::error::if_present(laws.law_index_map_mut().get_mut(ty, ghost))?
            {
                a.as_mut_slice().fill(-1);
                a.try_resize(n, -1)?;
            }
            if let Ok(a) = laws.local_numbering_map_mut().get_mut(ty, ghost) {
                a.as_mut_slice().fill(-1);
                a.try_resize(n, -1)?;
            }
        }
    }
    fit_handler_maps(laws, mesh)?;
    for (i, entries) in entries_all {
        laws.record_renumbered(i, &entries)?;
    }
    Ok(())
}

impl Model for DiffusionModel {
    fn model_type(&self) -> ModelType {
        self.physics.base.model_type
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
        DiffusionModel::init_full(self, options)
    }

    fn solve_step(&mut self, solver_id: Option<&str>) -> Result<()> {
        DiffusionModel::solve_step(self, solver_id)
    }

    fn stable_time_step(&mut self) -> Result<Real> {
        DiffusionModel::stable_time_step(self)
    }

    fn dump(&mut self, time: Option<Real>, step: Option<usize>) -> Result<()> {
        DiffusionModel::dump(self, time, step)
    }
}

/// Heat-transfer model (upstream `HeatTransferModel`, DIFF-002): a
/// [`DiffusionModel`] with DOF `temperature`, default id `heat_transfer`,
/// the temperature accessor aliases and the dump aliases
/// `internal_heat_rate → internal_flow`, `external_heat_rate →
/// external_flow`, `conductivity → diffusivity`. Only this model accepts
/// the `heat_diffusion` law. Dereferences to the [`DiffusionModel`].
#[derive(Debug)]
pub struct HeatTransferModel(pub DiffusionModel);

impl HeatTransferModel {
    /// Upstream `HeatTransferModel(mesh, dim, id)` with an empty parser.
    pub fn new(mesh: SharedMesh, spatial_dimension: Option<usize>, id: &str) -> Result<Self> {
        Self::with_parser(mesh, spatial_dimension, id, Parser::default())
    }

    /// With an input-file snapshot (upstream: the static parser).
    pub fn with_parser(
        mesh: SharedMesh,
        spatial_dimension: Option<usize>,
        id: &str,
        parser: Parser,
    ) -> Result<Self> {
        DiffusionModel::build(
            mesh,
            spatial_dimension,
            id,
            TEMPERATURE,
            ModelType::HeatTransferModel,
            DiffusionModelKind::HeatTransfer,
            parser,
        )
        .map(Self)
    }

    /// Upstream `getTemperature()`.
    pub fn temperature(&self) -> Result<ArrayRef<'_, Real>> {
        self.0.diffusion()
    }

    /// Mutable temperature.
    pub fn temperature_mut(&mut self) -> Result<ArrayMut<'_, Real>> {
        self.0.diffusion_mut()
    }

    /// Upstream `getTemperatureRate()`.
    pub fn temperature_rate(&self) -> Result<ArrayRef<'_, Real>> {
        self.0.diffusion_rate()
    }

    /// Upstream `getTemperatureRelease()`.
    #[must_use]
    pub fn temperature_release(&self) -> i64 {
        self.0.diffusion_release()
    }

    /// Upstream `getExternalHeatRate()`.
    pub fn external_heat_rate(&self) -> Result<ArrayRef<'_, Real>> {
        self.0.external_flow()
    }

    /// Mutable external heat rate.
    pub fn external_heat_rate_mut(&mut self) -> Result<ArrayMut<'_, Real>> {
        self.0.external_flow_mut()
    }

    /// Upstream `getInternalHeatRate()`.
    pub fn internal_heat_rate(&self) -> Result<ArrayRef<'_, Real>> {
        self.0.internal_flow()
    }
}

impl Deref for HeatTransferModel {
    type Target = DiffusionModel;
    fn deref(&self) -> &DiffusionModel {
        &self.0
    }
}

impl DerefMut for HeatTransferModel {
    fn deref_mut(&mut self) -> &mut DiffusionModel {
        &mut self.0
    }
}

impl MeshEventHandler for HeatTransferModel {
    fn event_priority(&self) -> EventPriority {
        self.0.event_priority()
    }
    fn event_priorities(&self) -> Vec<EventPriority> {
        self.0.event_priorities()
    }
    fn on_mesh_event(&mut self, mesh: &Mesh, event: &MeshEvent) -> Result<()> {
        self.0.on_mesh_event(mesh, event)
    }
    fn check_mesh_event(&self, mesh: &Mesh, event: &MeshEvent) -> Result<()> {
        self.0.check_mesh_event(mesh, event)
    }
    fn on_mesh_event_at(
        &mut self,
        mesh: &Mesh,
        event: &MeshEvent,
        level: EventPriority,
    ) -> Result<()> {
        self.0.on_mesh_event_at(mesh, event, level)
    }
    fn on_mesh_event_phased(
        &mut self,
        mesh: &Mesh,
        event: &MeshEvent,
        priority: EventPriority,
        phases: &mut Phases,
    ) -> Result<()> {
        self.0.on_mesh_event_phased(mesh, event, priority, phases)
    }
}

impl Model for HeatTransferModel {
    fn model_type(&self) -> ModelType {
        self.0.model_type()
    }
    fn mesh(&self) -> &SharedMesh {
        Model::mesh(&self.0)
    }
    fn mesh_token(&self) -> &ConsumerToken {
        self.0.mesh_token()
    }
    fn model_solver(&self) -> &ModelSolver {
        self.0.model_solver()
    }
    fn model_solver_mut(&mut self) -> &mut ModelSolver {
        self.0.model_solver_mut()
    }
    fn init_full(&mut self, options: &ModelOptions) -> Result<()> {
        self.0.init_full(options)
    }
    fn solve_step(&mut self, solver_id: Option<&str>) -> Result<()> {
        self.0.solve_step(solver_id)
    }
    fn stable_time_step(&mut self) -> Result<Real> {
        self.0.stable_time_step()
    }
    fn dump(&mut self, time: Option<Real>, step: Option<usize>) -> Result<()> {
        self.0.dump(time, step)
    }
}
