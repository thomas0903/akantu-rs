// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/model/solid_mechanics/
// solid_mechanics_model.hh, solid_mechanics_model.cc,
// solid_mechanics_model_mass.cc, solid_mechanics_model_io.cc,
// solid_mechanics_model_event_handler.hh; src/model/common/
// constitutive_laws/constitutive_laws_handler{.hh,_tmpl.hh} (initFullImpl,
// initConstitutiveLaws, reassignConstitutiveLaw, onElementsAdded/Removed).

//! [`SolidMechanicsModel`] (SMM-001…022, 029).
//!
//! # Structure (D-021)
//!
//! `SolidMechanicsModel { physics, solver, token }`: the physics part
//! ([`SolidMechanicsPhysics`]: model base, materials, model-owned nodal
//! arrays, release counters) implements [`SolverCallback`]; the
//! [`ModelSolver`] owns the DOF manager and the time-step solvers. The DOF
//! family `displacement` — displacement, velocity, acceleration, blocked
//! flags, increment, previous displacement — lives in the DOF manager
//! (upstream registers the model's arrays by reference); `external_force`,
//! `internal_force`, `mass` and `current_position` belong to the physics.
//!
//! # Materials
//!
//! Materials implement [`Material`]; each material method receives a
//! phase-scoped [`LawContext`] whose services read the mesh, the FE
//! engines and the nodal fields while the material is mutably borrowed
//! (ARCHITECTURE). The generic small-strain pipeline (gradient →
//! stress → `Bᵀσ` assembly, tangent → `BᵀDB` assembly) and the
//! total-Lagrangian finite-deformation assembly are model code
//! (`assembly`), in upstream order, through the DOF-manager contracts.
//!
//! # Collective protocol (ARCHITECTURE.md, "Non-linear solvers, time stepping and switches")
//!
//! Every model callback runs inside the solvers' collective operation
//! (agreed by `DofManager::step`). Model methods that reduce over ranks
//! (energies, stable time step, external work) run as one
//! `DofManager::collective` operation whose local part is agreed before the
//! reduction, so a rank-specific failure surfaces as an error on every rank.

mod assembly;
mod energy;
mod io;
mod non_local;
mod parallel;
mod services;

use akantu_comm::phase::{Phases, Severity};
use akantu_core::array_shared::{ArrayMut, ArrayRef, SharedArray};
use akantu_core::enums::{
    AnalysisMethod, EventPriority, IntegrationSchemeType, MatrixType, ModelType,
    NonLinearSolverType, TimeStepSolverType,
};
use akantu_core::error::if_present;
use akantu_core::tensor::MatrixX;
use akantu_core::{
    Array, Element, ElementType, ElementTypeMapArray, Error, GhostType, Id, Idx, Real, Result,
    SharedElementTypeMapArray, TypeFilter,
};
use akantu_mesh::group::ElementGroupRef;
use akantu_mesh::{ConsumerToken, Mesh, MeshEvent, MeshEventHandler, MeshPart, SharedMesh};
use akantu_model::boundary_condition::{
    DirichletFunctor, NeumannFunctor, apply_dirichlet, apply_neumann_recorded, group_error,
};
use akantu_model::constitutive_law::{
    ConstitutiveLawSelector, LawContext, LawPhase, SelectorChain,
};
use akantu_model::model::{ModelBase, alloc_nodal_shared_field, init_new_solver};
use akantu_model::neumann_store::{MoveReport, NeumannStore};
use akantu_model::{ConstitutiveLawsHandler, Model, ModelOptions};
use akantu_parser::{Parser, ParserType};
use akantu_solver::{
    DofManager, IntegrationScheme, ModelSolver, ModelSolverOptions, SolutionType, SolverCallback,
};

use crate::material::{LawOptions, Material, MaterialFactory, register_builtin_materials};

pub use io::{DumpRoute, SmmDumpers};
pub(crate) use services::{DofArrays, OwnArrays, SmmServices};

/// Id of the DOF family of the model.
pub const DOF_ID: &str = "displacement";

/// Default model id (upstream `"solid_mechanics_model"`).
pub const DEFAULT_ID: &str = "solid_mechanics_model";

/// Name of the default FE engine (upstream `"SolidMechanicsFEEngine"`).
pub const FE_ENGINE_NAME: &str = "SolidMechanicsFEEngine";

/// Events sent to [`SolidMechanicsModelEventHandler`]s (upstream
/// `SolidMechanicsModelEvent::*`, SMM-018).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SolidMechanicsModelEvent {
    /// `BeforeSolveStepEvent(method)`.
    BeforeSolveStep(AnalysisMethod),
    /// `AfterSolveStepEvent(method)`.
    AfterSolveStep(AnalysisMethod),
    /// `BeforeDumpEvent`.
    BeforeDump,
    /// `BeginningOfDamageIterationEvent`.
    BeginningOfDamageIteration,
    /// `AfterDamageEvent`.
    AfterDamage,
}

/// Receiver of model events (upstream `SolidMechanicsModelEventHandler`:
/// every callback defaults to doing nothing).
pub trait SolidMechanicsModelEventHandler: std::fmt::Debug + Send {
    /// `onBeginningSolveStep(method)`.
    fn on_beginning_solve_step(&mut self, method: AnalysisMethod) -> Result<()> {
        let _ = method;
        Ok(())
    }
    /// `onEndSolveStep(method)`.
    fn on_end_solve_step(&mut self, method: AnalysisMethod) -> Result<()> {
        let _ = method;
        Ok(())
    }
    /// `onDump()`.
    fn on_dump(&mut self) -> Result<()> {
        Ok(())
    }
    /// `onBeginningOfDamageIteration()`.
    fn on_beginning_of_damage_iteration(&mut self) -> Result<()> {
        Ok(())
    }
    /// `onDamage()`.
    fn on_damage(&mut self) -> Result<()> {
        Ok(())
    }
}

/// Upstream-compatibility switches of the solid mechanics model (D-003
/// category B defects of the solid mechanics model). The default (`false`
/// everywhere) is the corrected behaviour; differential replays against the
/// unpatched oracle set only the switch of the defect a case reproduces.
/// Material-level switches are [`LawOptions`], solver
/// switches `akantu_solver::UpstreamCompat` (per DOF manager, D-041).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SmmUpstreamCompat {
    /// **UD-208**: the small-strain stiffness assembly
    /// overwrites every material's `grad_u` with `∇u` **without**
    /// subtracting `eigen_grad_u` (upstream `Material::assembleStiffnessMatrix`,
    /// `material.cc:262-312`), while `computeAllStresses` stores
    /// `∇u − ε⁰`. Laws whose tangent depends on the strain (phase field,
    /// user-defined laws; J2 and Drucker–Prager use the elastic tangent at
    /// the reference commit) then evaluate it at the wrong strain, and any
    /// read of `grad_u` after a standalone stiffness assembly sees `∇u`.
    /// A step converged by Newton–Raphson normally ends with a residual
    /// assembly that restores `∇u − ε⁰`; the exception
    /// is modified Newton under the `residual` criterion converging at the
    /// initial test with `K` re-assembled just before the early return,
    /// where the next step's `previous_grad_u` is shifted by `ε⁰`.
    /// The finite-deformation `L2` stiffness part (`assembleStiffnessMatrixL2`,
    /// SMM-023) follows the same rule: upstream builds `B_L1(∇u)` while the
    /// residual uses `∇u − ε⁰`, so its tangent is not the derivative of the
    /// residual when `ε⁰ ≠ 0`.
    /// Default: the stiffness assembly stores `∇u − ε⁰`, as
    /// `computeAllStresses`.
    pub stiffness_grad_u_ignores_eigen_grad_u: bool,
    /// **UD-352** (D-064): upstream `initSolver`
    /// (`solid_mechanics_model.cc:212-216`) copies `X` into the
    /// current-position cache on every call without touching its release,
    /// so after a reader refreshed it at the current displacement release,
    /// a later `initNewSolver` leaves `X` (the undeformed mesh) for the
    /// stable time step, the cohesive normals and every other reader until
    /// the next displacement update. Default: `X` is copied only into a
    /// newly allocated cache; a later `init_solver` leaves the cache and its
    /// release untouched.
    pub init_solver_resets_current_position: bool,
    /// **UD-900** (MPI-041): upstream's `initConstitutiveLaws`
    /// ends with `synchronize(_clh_init_cl)`, but no model registers a
    /// synchronizer for that tag (`SynchronizerRegistry::synchronize`
    /// iterates over none), so the laws' initialisation data are never
    /// exchanged: on a distributed mesh the ghost copies of Marigo's random
    /// threshold `Yd` keep the drawing rank's own values instead of their
    /// owner's (the law's `packData` / `unpackData` for the tag are dead
    /// code). Default: exchanged owner → ghost over the element
    /// synchronizer, in the distributed initialisation scope.
    pub clh_init_cl_not_exchanged: bool,
    /// **UD-553** (D-003 B, decision of 2026-11-11): upstream's
    /// `SynchronizerRegistry::synchronize(tag)`
    /// (`synchronizer_registry.cc:51-64`) runs the synchronizers
    /// registered for `tag`, and the model registers only `_smm_mass`,
    /// `_smm_stress`, `_smm_gradu`, `_smm_density`, `_for_dump`
    /// (`solid_mechanics_model.cc:76-83`) and `_constitutive_law_id`
    /// (`constitutive_laws_handler.hh:77-81`); any other tag (a typo'd or
    /// wrong one) is a silent no-op on a distributed mesh. Default:
    /// [`SolidMechanicsModel::synchronize`] refuses a tag it does not
    /// handle (a programming error); `true`: a silent no-op as upstream.
    /// On a serial mesh every tag is a no-op either way.
    pub unregistered_synchronization_tag_is_a_noop: bool,
    /// **UD-1102** (MPI-048): upstream hands the ghost elements a
    /// neighbourhood's grid synchroniser adds their owner's law only when a
    /// *later* neighbourhood's synchroniser is created
    /// (`ConstitutiveLawsHandler::onElementsAdded` →
    /// `non_local_manager->synchronize(_constitutive_law_id)` over the
    /// synchronisers that already exist) or in `initializeNonLocal`, after
    /// the ghost points were inserted: the grid ghosts of the last
    /// neighbourhood (of the only one, usually) are never inserted, the
    /// cleanup then removes them, and the averages near a partition boundary
    /// miss every neighbour beyond the distribution's ghost layer (the
    /// oracle's 2- and 4-rank `test_build_neighborhood_parallel` averages
    /// differ from its serial ones by up to 112 % and 202 %). Default: each
    /// neighbourhood's grid ghosts receive their owner's law right after its
    /// synchroniser is built; `true`: upstream's sequence.
    pub non_local_ghost_laws_after_insertion: bool,
    /// **UD-1101**: upstream's `cleanupExtraGhostElements` erases
    /// every ghost element in no pair (`Mesh::eraseElements`: removed when
    /// its ghost counter reaches 0), including the distribution's own ghost
    /// layer — e.g. the ghosts of a local law next to a non-local one, whose
    /// stresses the residual assembly at the shared nodes needs. Default:
    /// only ghost elements the grid synchronisers added are erased (their
    /// counters brought to 0), once (UD-1100); `true`: upstream's erasure,
    /// once per neighbourhood with the element set of the first one (with
    /// two or more neighbourhoods upstream then erases unrelated or
    /// relevant ghosts: an error where it aborts, UD-1100).
    pub non_local_cleanup_erases_distribution_ghosts: bool,
}

/// A shared point source of another element kind
/// ([`SolidMechanicsPhysics::extra_point_sources`]).
pub type SharedPointSource =
    std::sync::Arc<dyn akantu_model::non_local::NonLocalPointSource + Send + Sync>;

/// Physics part of the solid mechanics model: everything except the solver.
/// Implements [`SolverCallback`].
#[derive(Debug)]
pub struct SolidMechanicsPhysics {
    /// Model base (mesh handle, id, dimension, method, FE engines, parser).
    pub base: ModelBase,
    /// Materials (upstream `ConstitutiveLawsHandler<Material>` mixin).
    pub materials: ConstitutiveLawsHandler<dyn Material>,
    /// Material factory of this model (built-in materials registered;
    /// upstream's process-global `MaterialFactory`).
    pub factory: MaterialFactory,
    /// Upstream-defect switches applied to every material instantiated by
    /// `init_full` before its `initMaterial` (`None`: each law keeps its
    /// default, the corrected behaviour; see [`LawOptions`]).
    pub law_options: Option<LawOptions>,
    /// Model-level upstream-defect switches (default: corrected).
    pub upstream_compat: SmmUpstreamCompat,
    /// Non-local manager (upstream `ConstitutiveLawsHandler::non_local_manager`):
    /// created and initialised with the laws when one of them is non-local.
    pub non_local: Option<akantu_model::non_local::NonLocalManager>,
    /// Options (route, upstream-defect switches UD-037/UD-561) of the
    /// non-local manager created by `init_full`.
    pub non_local_options: akantu_model::non_local::NonLocalOptions,
    /// Integration points of element kinds other than the regular one
    /// (the IGFEM model's, upstream `NonLocalManagerIGFEM`, IGFEM-009): the
    /// non-local manager's layout includes their blocks, and the model
    /// services count their points (`nb_integration_points` of a type the
    /// regular FE engine does not know), so that laws of these kinds
    /// flatten and inflate their internals with the right point counts.
    pub extra_point_sources: Vec<SharedPointSource>,
    /// The grid synchronisers of the non-local neighbourhoods on a
    /// distributed mesh (MPI-048/049; `None` on one rank or
    /// without non-local laws).
    pub non_local_ghosts: Option<akantu_model::non_local::GridGhosts>,
    // Exportable nodal arrays (D-047): element access through guards.
    external_force: Option<SharedArray<Real>>,
    /// D-105 / UD-1280: the per-boundary-element record of the Neumann
    /// loads (`akantu_model::neumann_store`), created by the first Neumann
    /// application: a model that never applies one has none and pays
    /// nothing. Read by the cohesive model's doubled-node hook
    /// ([`SolidMechanicsPhysics::move_doubled_node_external_force`]).
    /// Boxed: its element-type maps are kilobytes inline, and the models
    /// are copied by value in unoptimised constructors (`constructor_stack`).
    neumann_store: Option<Box<NeumannStore>>,
    internal_force: Option<SharedArray<Real>>,
    mass: Option<SharedArray<Real>>,
    current_position: Option<SharedArray<Real>>,
    displacement_release: i64,
    current_position_release: i64,
    need_to_reassemble_lumped_mass: bool,
    need_to_reassemble_mass: bool,
    /// `K` was released (`release_static_solver`): reassemble it at its
    /// next assembly whatever the laws answer.
    pub(crate) need_to_reassemble_stiffness: bool,
    external_work: Real,
    external_work_release: i64,
    /// Upstream `f_m2a` (kept for API parity; unused by upstream code).
    pub f_m2a: Real,
    /// Laws of new **ghost** elements received from their owners for the
    /// next `ElementsAdded` dispatch: `(ghost element, law)`
    /// in unpack order (receive schemes in rank order, scheme order).
    /// Upstream assigns them at the end of `assignConstitutiveLawToElements`
    /// (`synchronize(_constitutive_law_id)`, a ghost without a law takes the
    /// received one and is appended to that law's filter), before the laws
    /// resize their internals; here the exchange runs in the distributed
    /// hook of the insertion mutation (before the commit, D-053) and the
    /// dispatch consumes this list at the same position. Empty outside a
    /// distributed insertion.
    pub staged_ghost_laws: Vec<(Element, usize)>,
    /// User extension tags registered with
    /// [`SolidMechanicsModel::register_synchronization_tag`] (MPI-053).
    pub extension_tags: Vec<akantu_core::enums::SynchronizationTag>,
    time_step: Real,
    /// User event handlers in registration order (SMM-018; see
    /// [`SolidMechanicsModel::send_event`] for the dispatch order).
    handlers: Vec<RegisteredHandler>,
    /// Next [`EventHandlerId`] (registration counter).
    next_handler_id: u64,
    /// Reused buffers of the internal-force assembly.
    assembly_scratch: assembly::AssemblyScratch,
}

/// Handle of a user event handler, returned by
/// [`SolidMechanicsModel::register_event_handler`] /
/// [`SolidMechanicsModel::register_event_handler_with_priority`] and taken
/// by [`SolidMechanicsModel::unregister_event_handler`] (upstream identifies
/// a handler by its address; D-077).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct EventHandlerId(u64);

/// A user event handler, its priority and the number of materials
/// registered before it (upstream keeps materials and user handlers in one
/// list ordered by priority, newest first among equal priorities; every
/// material is at `_ehp_highest`).
#[derive(Debug)]
struct RegisteredHandler {
    id: EventHandlerId,
    priority: EventPriority,
    laws_before: usize,
    handler: Box<dyn SolidMechanicsModelEventHandler>,
}

impl RegisteredHandler {
    /// Upstream `SolidMechanicsModelEventHandler::sendEvent(event)`.
    fn receive(&mut self, event: SolidMechanicsModelEvent) -> Result<()> {
        let h = &mut self.handler;
        match event {
            SolidMechanicsModelEvent::BeforeSolveStep(m) => h.on_beginning_solve_step(m),
            SolidMechanicsModelEvent::AfterSolveStep(m) => h.on_end_solve_step(m),
            SolidMechanicsModelEvent::BeforeDump => h.on_dump(),
            SolidMechanicsModelEvent::BeginningOfDamageIteration => {
                h.on_beginning_of_damage_iteration()
            }
            SolidMechanicsModelEvent::AfterDamage => h.on_damage(),
        }
    }
}

/// Solid mechanics model (upstream `SolidMechanicsModel`).
#[derive(Debug)]
pub struct SolidMechanicsModel {
    /// Physics (callback).
    pub physics: SolidMechanicsPhysics,
    /// Solver.
    pub solver: ModelSolver,
    /// Mesh-consumer registration.
    pub token: ConsumerToken,
    /// Dumpers of the model (upstream: registered on the mesh).
    pub dumpers: SmmDumpers,
}

/// Shape check of a nodal array read by a model loop:
/// the public accessors hand out `&mut Array`, so a user can resize or
/// replace any of them; every loop validates `nb_nodes` tuples of
/// `nb_component` values first and returns `DimensionMismatch` (agreed like
/// any local error inside a collective operation or solve step, ARCHITECTURE.md, "Non-linear solvers, time stepping and switches")
/// instead of panicking on an out-of-range slice.
/// Shape of a nodal array (plain, exportable or a guard), for
/// [`check_nodal`]: metadata only, no element access.
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

/// Shape check of a per-element law map block (`nb_element` tuples of one
/// value; `material_by_element_mut` hands the maps out).
pub(crate) fn check_element_map(
    context: &'static str,
    a: &Array<Idx>,
    nb_element: usize,
) -> Result<()> {
    check_nodal(context, a, nb_element, 1)
}

fn not_allocated(name: &str) -> Error {
    Error::not_found(
        "nodal field",
        format!("{name} (not allocated: call init_full first)"),
    )
}

impl SolidMechanicsModel {
    /// Upstream `SolidMechanicsModel(mesh, dim, id)` with an empty parser
    /// (see [`SolidMechanicsModel::with_parser`]).
    pub fn new(mesh: SharedMesh, spatial_dimension: Option<usize>, id: &str) -> Result<Self> {
        Self::with_parser(mesh, spatial_dimension, id, Parser::default())
    }

    /// Upstream constructor (SMM-001) with an explicit input-file snapshot
    /// (upstream: the static parser at construction time): DOF manager
    /// `<id>:dof_manager_default` (solvers and schemes of the model's
    /// `model solid_mechanics_model [ … ]` section, upstream
    /// `initDOFManager(section)`), Lagrange FE engine `SolidMechanicsFEEngine`
    /// on the elements of the model dimension, default ParaView dumper,
    /// material registry with `constitutive_law_index = -1` everywhere.
    ///
    /// # Errors
    /// An unusable mesh domain, a bad dimension, an invalid model section or
    /// a distributed mesh with ghost elements (the element synchronizer tags
    /// `_smm_*` need the parallel model).
    pub fn with_parser(
        mesh: SharedMesh,
        spatial_dimension: Option<usize>,
        id: &str,
        parser: Parser,
    ) -> Result<Self> {
        Self::with_parser_and_type(
            mesh,
            spatial_dimension,
            id,
            parser,
            ModelType::SolidMechanicsModel,
        )
    }

    /// [`SolidMechanicsModel::with_parser`] for a derived model type
    /// (upstream constructor argument `model_type`, e.g.
    /// `_solid_mechanics_model_cohesive`: the `model <type> [ … ]` section
    /// of that type configures solvers and laws).
    ///
    /// # Errors
    /// See [`SolidMechanicsModel::with_parser`].
    pub fn with_parser_and_type(
        mesh: SharedMesh,
        spatial_dimension: Option<usize>,
        id: &str,
        parser: Parser,
        model_type: ModelType,
    ) -> Result<Self> {
        let mut base = ModelBase::new(mesh.clone(), model_type, spatial_dimension, id)?;
        base.parser = parser;
        let token = mesh.register_multilevel_consumer(
            id,
            &[
                EventPriority::FE_ENGINE,
                EventPriority::DOF_MANAGER,
                EventPriority::MODEL,
                EventPriority::NON_LOCAL_MANAGER,
            ],
            MeshPart::Main,
        )?;
        let (materials, comm) = {
            let m = mesh.read()?;
            let has_ghosts = m
                .element_types(TypeFilter {
                    spatial_dimension: None,
                    ghost_type: GhostType::Ghost,
                    kind: None,
                })
                .any(|t| m.nb_element(t, GhostType::Ghost) > 0);
            // Ghost elements are exchanged through the mesh's element
            // synchronizer (upstream registers it for _smm_mass,
            // _smm_stress, _smm_gradu, _smm_density, _for_dump and
            // _constitutive_law_id when the mesh is distributed; SMM-016,
            // MODEL-011): a mesh with ghosts must be a distributed one.
            if has_ghosts && m.synchronizers().element.is_none() {
                return Err(Error::Mesh(
                    "SolidMechanicsModel: the mesh has ghost elements but no element \
                     synchronizer (distribute it with akantu_mesh::distribution)"
                        .into(),
                ));
            }
            (
                ConstitutiveLawsHandler::new(id, &m, base.spatial_dimension, ParserType::Material)?,
                m.communicator().clone(),
            )
        };
        let mut factory = MaterialFactory::default();
        register_builtin_materials(&mut factory)?;
        let mut physics = SolidMechanicsPhysics {
            base,
            materials,
            factory,
            law_options: None,
            upstream_compat: SmmUpstreamCompat::default(),
            non_local: None,
            non_local_options: akantu_model::non_local::NonLocalOptions::default(),
            extra_point_sources: Vec::new(),
            non_local_ghosts: None,
            external_force: None,
            neumann_store: None,
            internal_force: None,
            mass: None,
            current_position: None,
            displacement_release: 0,
            current_position_release: 0,
            need_to_reassemble_lumped_mass: true,
            need_to_reassemble_mass: true,
            need_to_reassemble_stiffness: false,
            external_work: 0.0,
            external_work_release: -1,
            f_m2a: 1.0,
            staged_ghost_laws: Vec::new(),
            extension_tags: Vec::new(),
            time_step: 0.0,
            handlers: Vec::new(),
            next_handler_id: 0,
            assembly_scratch: assembly::AssemblyScratch::default(),
        };
        // upstream initDOFManager(dof_manager) (`model_solver.cc:78-95`):
        // the section's dof_manager_type (`default`; `petsc` with the opt-in
        // `petsc` feature, see `init_dof_manager`) and the model section's
        // solvers/schemes; the section lookup fails on a model section whose
        // name is not a model type, as upstream's getParserSection
        let section = physics.base.try_parser_section()?.cloned();
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
            let permissive = physics.base.parser.is_permissive();
            solver.init_from_section(&section, &mut physics, permissive)?;
        }
        {
            let m = mesh.read()?;
            let dim = physics.base.spatial_dimension;
            physics.base.fems.register(FE_ENGINE_NAME, &m, dim)?;
        }
        let dumpers = SmmDumpers::new(id, physics.base.spatial_dimension)?;
        physics.time_step = solver.time_step(None).unwrap_or(0.0);
        Ok(Self {
            physics,
            solver,
            token,
            dumpers,
        })
    }

    /// Upstream `ModelSolver::initDOFManager(dof_manager_type)`
    /// (`model_solver.cc:98-121`; `patch_test_linear_fixture.hh:61`): a new
    /// DOF manager of that type, `<id>:dof_manager_<type>`, replaces the
    /// model's. `"petsc"` needs the opt-in `petsc` feature of this crate (a
    /// build with it is distributed under GPL-3.0-or-later as a whole, D-076)
    /// and runs on the mesh's MPI communicator, or — for a serial mesh — on
    /// the MPI world of a one-process job
    /// ([`akantu_solver::model_solver::dof_manager_communicator`]). Call it
    /// right after constructing the model, before
    /// [`SolidMechanicsModel::init_full`].
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

    /// Upstream `initFull(options)` (SMM-002, MODEL-002): method; default
    /// solver for the method if none exists (UD-009 preserved); materials
    /// from the parser (model section, else the root) when a file was
    /// parsed, assigned and initialised. Laws registered with
    /// [`SolidMechanicsModel::register_new_material`] before are kept.
    pub fn init_full(&mut self, options: &ModelOptions) -> Result<()> {
        self.init_method_and_solver(options)?;
        if let Some(instantiated) = self.instantiate_materials()? {
            // a rank-specific instantiation failure is agreed with the law
            // assignment on a distributed mesh
            self.init_materials_after(instantiated)?;
        }
        self.physics.time_step = self.solver.time_step(None).unwrap_or(0.0);
        Ok(())
    }

    /// First part of [`SolidMechanicsModel::init_full`] (also used by model
    /// extensions that override `initConstitutiveLaws`, e.g. the cohesive
    /// model): the analysis method and, if missing, the default solver of
    /// the method (UD-009 preserved).
    ///
    /// # Errors
    /// Solver creation errors.
    pub fn init_method_and_solver(&mut self, options: &ModelOptions) -> Result<()> {
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
        Ok(())
    }

    /// Upstream `instantiateMaterials()` part of `initFull`: the laws of the
    /// parsed input (model section, else the root), in file order. Returns
    /// `None` when no input file was parsed (nothing instantiated; laws
    /// registered with [`SolidMechanicsModel::register_new_material`] are
    /// kept either way), otherwise the **local** instantiation result.
    ///
    /// That result is not propagated here: feed it to
    /// [`SolidMechanicsModel::init_materials_after`], which agrees a
    /// rank-specific failure (fatal) on a distributed
    /// mesh before any communication and returns it
    /// directly on a serial one.
    ///
    /// # Errors
    /// A `model` section whose name is not a model type (upstream's
    /// `getParserSection` throws, `constitutive_laws_handler.hh:102`; the
    /// parser is the same on every rank, so this is not rank-specific) and
    /// the mesh lock (law errors are in the returned result).
    pub fn instantiate_materials(&mut self) -> Result<Option<Result<()>>> {
        let Some(section) = self.physics.base.try_laws_section()?.cloned() else {
            return Ok(None);
        };
        let permissive = self.physics.base.parser.is_permissive();
        let mesh_h = self.physics.base.mesh.clone();
        let mesh = mesh_h.read()?;
        let p = &mut self.physics;
        let services = SmmServices {
            mesh: &mesh,
            fems: &p.base.fems,
            extra_points: &p.extra_point_sources,
            dofs: DofArrays::of(&self.solver.dof_manager),
            own: OwnArrays::default(),
            spatial_dimension: p.base.spatial_dimension,
            time_step: p.time_step,
        };
        Ok(Some(p.materials.instantiate_laws(
            &p.factory, &section, &services, permissive,
        )))
    }

    /// Upstream `initFull` with the default options except the method.
    pub fn init_full_with(&mut self, method: AnalysisMethod) -> Result<()> {
        self.init_full(&ModelOptions::new(method))
    }

    /// Upstream `initConstitutiveLaws()` (`initMaterials`): assign the
    /// not-ghost elements to the laws with the selector, apply the model's
    /// [`SolidMechanicsPhysics::law_options`], then `initMaterial` of every
    /// law.
    pub fn init_materials(&mut self) -> Result<()> {
        self.init_materials_after(Ok(()))
    }

    /// [`SolidMechanicsModel::init_materials`] after a local step `pre`
    /// whose failure must be agreed too: `init_full`'s law instantiation
    /// ([`SolidMechanicsModel::instantiate_materials`]), or a model
    /// extension's own preparation chained to it (the cohesive model's
    /// facet-material assignment, which also changes the laws' state).
    ///
    /// On a distributed mesh (an element synchronizer exists, on every rank
    /// alike) the whole initialisation is one collective scope (ARCHITECTURE.md, "Agreement across ranks").
    /// Everything is agreed **before** any
    /// communication:
    ///
    /// * a failure of `pre` (the law instantiation of `init_full`) is
    ///   agreed **fatal**: the failing rank keeps a
    ///   registered, partially parsed law and the law sets of the ranks
    ///   differ, so the model cannot be initialised again — rebuild it;
    /// * a failure of the parallel-support check, of the non-local route
    ///   check (`KeyOrder` is refused with `Error::Unsupported`, D-097 (1))
    ///   or of the law selection (e.g. a selector
    ///   error on one rank) returns promptly on every rank as a
    ///   **recoverable** error (the failing rank's own error, "failed on
    ///   another rank" elsewhere), the communicator stays usable and the
    ///   model can be initialised again (filters unchanged, law indices
    ///   overwritten by the next selection): for `KeyOrder`, set another
    ///   route and call [`SolidMechanicsModel::init_materials`].
    ///
    /// The commit of the selection, the `_constitutive_law_id` exchange and
    /// every law's `initMaterial` follow in the same scope; a failure there
    /// (including an unpack error of the exchange) is
    /// agreed as fatal (`Error::Fatal { kind: Agreed }` on every rank, or
    /// `Transport` if the communicator broke): the laws already hold their
    /// elements, rebuild the model.
    ///
    /// # Errors
    /// `pre`'s error (agreed fatal on a distributed mesh), selection,
    /// exchange or `initMaterial` errors as described.
    pub fn init_materials_after(&mut self, pre: Result<()>) -> Result<()> {
        let mesh_h = self.physics.base.mesh.clone();
        {
            let mesh = mesh_h.read()?;
            let Some(es) = &mesh.synchronizers().element else {
                pre?;
                self.check_has_laws()?;
                parallel::check_law_data_tags(&self.physics.materials)?;
                self.physics
                    .materials
                    .assign_laws_to_elements(&mesh, None)?;
                self.apply_law_options();
                return self.init_laws(&mesh);
            };
            let mut ph = Phases::new(&es.communicator, "smm:init_materials");
            let r = self.init_materials_in(&mut ph, &mesh, es, pre);
            ph.settle(r)?;
        }
        // upstream `non_local_manager->initialize()` at the end of
        // `initConstitutiveLaws`: collective steps of their own,
        // mesh mutations among them, outside the read of the mesh. A
        // failure is agreed on every rank and fatal (the laws are
        // initialised and the mesh may have changed: rebuild the model).
        if non_local::has_non_local_law(&self.physics.materials) {
            self.init_non_local_distributed().map_err(|e| match e {
                Error::Fatal { .. } => e,
                e => Error::fatal(akantu_core::FatalKind::Agreed, e),
            })?;
        }
        Ok(())
    }

    /// The distributed initialisation of
    /// [`SolidMechanicsModel::init_materials_after`] in `ph`.
    fn init_materials_in(
        &mut self,
        ph: &mut Phases,
        mesh: &Mesh,
        es: &akantu_comm::Synchronizer<Element>,
        pre: Result<()>,
    ) -> Result<()> {
        // A failed instantiation has changed the handler
        // (registered, partially parsed law on the failing rank only).
        ph.agree("instantiate", pre, Severity::Fatal)?;
        let selection = self
            .check_has_laws()
            .and_then(|()| parallel::check_parallel_support(&self.physics.materials))
            .and_then(|()| parallel::check_law_data_tags(&self.physics.materials))
            // D-097 (1): the non-local route `KeyOrder` is refused here,
            // recoverably and before anything is committed:
            // change the route and call `init_materials` again
            .and_then(|()| non_local::check_distributed_route(&self.physics))
            .and_then(|()| self.physics.materials.select_laws(mesh, None));
        let selection = ph.agreed("select", selection, Severity::Recoverable)?;
        let committed = self.physics.materials.commit_selection(selection);
        ph.agree("assign", committed, Severity::Fatal)?;
        // Ghost elements receive the law of their owner (upstream
        // `synchronize(_constitutive_law_id)` at the end of
        // assignConstitutiveLawToElements), appended to the law's ghost
        // filter in unpack order, before the laws size their internals.
        let mut acc = parallel::LawIdAccessor {
            materials: &mut self.physics.materials,
        };
        // The exchange's own agreements are recoverable,
        // but after the commit the laws hold their elements — a retry would
        // add them a second time. Every rank receives an error at the same
        // agreement of the exchange (or the scope broke: `settle` then
        // keeps the more urgent `Transport`), so the local mapping is
        // consistent on every rank.
        es.synchronize_in(
            ph,
            &mut acc,
            akantu_core::enums::SynchronizationTag::ConstitutiveLawId,
        )
        .map_err(|e| Error::fatal(akantu_core::FatalKind::Agreed, e))?;
        self.apply_law_options();
        let init = self.init_each_material(mesh);
        ph.agree("init_material", init, Severity::Fatal)?;
        // upstream `synchronize(_clh_init_cl)` after every law's
        // initMaterial (`constitutive_laws_handler_tmpl.hh:308`): the laws'
        // initialisation data (Marigo's random `Yd`) owner → ghost
        // (MPI-041). Upstream registers no synchronizer for the tag, so it
        // exchanges nothing there (UD-900, compat switch
        // `clh_init_cl_not_exchanged`). The exchange runs on every rank of a
        // distributed mesh (empty messages when no law has data: which laws
        // have data is a local fact, a skipped exchange on one rank would
        // diverge).
        if !self.physics.upstream_compat.clh_init_cl_not_exchanged {
            let fe = self.physics.base.fems.get("")?;
            let nb_quad = assembly::nb_quad_map(fe, mesh, self.physics.base.spatial_dimension)?;
            let mut acc = parallel::LawDataAccessor {
                materials: &mut self.physics.materials,
                nb_quad: &nb_quad,
            };
            es.synchronize_in(
                ph,
                &mut acc,
                akantu_core::enums::SynchronizationTag::ClhInitCl,
            )
            .map_err(|e| Error::fatal(akantu_core::FatalKind::Agreed, e))?;
        }
        // the non-local manager follows in its own collective steps
        // (`init_materials_after`)
        Ok(())
    }

    fn check_has_laws(&self) -> Result<()> {
        if self.physics.materials.nb_laws() == 0 {
            return Err(Error::invalid("No constitutive_law to initialize !"));
        }
        Ok(())
    }

    /// The model's [`SolidMechanicsPhysics::law_options`] on every law.
    fn apply_law_options(&mut self) {
        if let Some(o) = self.physics.law_options {
            for law in self.physics.materials.laws_mut() {
                if let Some(slot) = law.options_mut() {
                    *slot = o;
                }
            }
        }
    }

    /// `initMaterial` of every law, in creation order, then the non-local
    /// manager (serial meshes: upstream's `synchronize(_clh_init_cl)` in
    /// between has no synchronizer).
    fn init_laws(&mut self, mesh: &Mesh) -> Result<()> {
        self.init_each_material(mesh)?;
        self.init_non_local(mesh)
    }

    /// `initMaterial` of every law, in creation order.
    fn init_each_material(&mut self, mesh: &Mesh) -> Result<()> {
        let p = &mut self.physics;
        let (laws, services) = p.split(mesh, &self.solver.dof_manager);
        let ctx = LawContext {
            phase: LawPhase::Init,
            services: &services,
        };
        for law in laws.laws_mut() {
            law.init_material(&ctx)?;
        }
        Ok(())
    }

    /// Upstream `instantiateConstitutiveLaws` (manager creation when a law
    /// is non-local) and the end of `initConstitutiveLaws`
    /// (`non_local_manager->initialize()`): the manager reads the
    /// `non_local` sections of the input file, gets the solid weight
    /// functions (`damage_wf`, `remove_wf`), and builds its neighbourhoods
    /// from the laws' integration points. A model without non-local laws
    /// has no manager. Serial meshes (distributed ones:
    /// `init_non_local_distributed`).
    fn init_non_local(&mut self, mesh: &Mesh) -> Result<()> {
        if !non_local::has_non_local_law(&self.physics.materials) {
            return Ok(());
        }
        let mut manager = self.take_or_new_non_local_manager(mesh)?;
        let p = &mut self.physics;
        let extra = p.extra_point_sources.clone();
        let r = {
            let (laws, services) = p.split(mesh, &self.solver.dof_manager);
            let ctx = LawContext {
                phase: LawPhase::Init,
                services: &services,
            };
            let fe = services.fems.get("")?;
            let regular = akantu_model::non_local::FePointSource(fe);
            let mut sources: Vec<&dyn akantu_model::non_local::NonLocalPointSource> =
                vec![&regular];
            sources.extend(
                extra
                    .iter()
                    .map(|s| s.as_ref() as &dyn akantu_model::non_local::NonLocalPointSource),
            );
            let mut cb = non_local::SmmNonLocalCallback { laws, ctx: &ctx };
            manager.initialize_with_sources(
                mesh,
                &sources,
                &mut cb,
                &mut akantu_model::non_local::NoGhosts,
            )
        };
        p.non_local = Some(manager);
        r
    }

    /// The non-local manager (upstream `getNonLocalManager()`), `None` when
    /// no law is non-local.
    #[must_use]
    pub fn non_local_manager(&self) -> Option<&akantu_model::non_local::NonLocalManager> {
        self.physics.non_local.as_ref()
    }

    /// Mutable [`SolidMechanicsModel::non_local_manager`].
    pub fn non_local_manager_mut(
        &mut self,
    ) -> Option<&mut akantu_model::non_local::NonLocalManager> {
        self.physics.non_local.as_mut()
    }

    /// Upstream `isNonLocal()`.
    #[must_use]
    pub fn is_non_local(&self) -> bool {
        self.physics.non_local.is_some()
    }

    /// Rebuild the non-local neighbourhoods (upstream has no rebuild; the
    /// rebuild contract requires one after a topology change of the
    /// laws' elements before the next averaging). The rebuild applies the
    /// current `non_local_options` of the physics and re-centres the grids on
    /// the current mesh bounding box (as a fresh manager); the parsed
    /// `non_local` sections, weight functions and registered variables are
    /// kept.
    ///
    /// # Errors
    /// No manager, mesh lock, rebuild errors.
    pub fn reinitialize_non_local(&mut self) -> Result<()> {
        if self.physics.non_local.is_none() {
            return Err(Error::invalid("the model has no non-local law"));
        }
        let mesh_h = self.physics.base.mesh.clone();
        let mesh = mesh_h.read()?;
        non_local::check_serial(&mesh)?;
        self.init_non_local(&mesh)
    }

    /// Upstream `registerNewMaterial(name, type, option)` (SMM-020). The
    /// law's `name` parameter is set to `name` (UD-793, category C: upstream
    /// records the name only in the handler's name → index map and leaves
    /// `getName()` empty; lookups by name are the same).
    pub fn register_new_material(&mut self, name: &str, ty: &str, option: &str) -> Result<usize> {
        let mesh_h = self.physics.base.mesh.clone();
        let mesh = mesh_h.read()?;
        let p = &mut self.physics;
        let services = SmmServices {
            mesh: &mesh,
            fems: &p.base.fems,
            extra_points: &p.extra_point_sources,
            dofs: DofArrays::of(&self.solver.dof_manager),
            own: OwnArrays::default(),
            spatial_dimension: p.base.spatial_dimension,
            time_step: p.time_step,
        };
        let i = p
            .materials
            .register_new_law(&p.factory, name, ty, option, &services)?;
        p.materials.law_mut(i)?.base_mut().set_name(name);
        Ok(i)
    }

    /// Upstream `reassignMaterial()` (`reassignConstitutiveLaw`, SMM-020):
    /// ask the selector for every not-ghost element of the mesh (the index
    /// map is not updated during this pass); then, law by law, the elements
    /// leaving the law are removed (internals compacted, law-local numbering
    /// of the kept elements updated) and the arriving ones added (index map
    /// and law-local numbering set, internals resized with their defaults).
    ///
    /// # Errors
    /// Those of the selector; [`Error::Unsupported`] when a law that loses
    /// elements has an internal on its own FE engine or element filter
    /// whose compaction the port does not implement (CL-001,
    /// checked for every law before any law changes, so a refusal
    /// leaves the model as it was).
    pub fn reassign_material(&mut self) -> Result<()> {
        let mesh_h = self.physics.base.mesh.clone();
        let mesh = mesh_h.read()?;
        let p = &mut self.physics;
        let changes = p.materials.reassignment(&mesh)?;
        let nb = p.materials.nb_laws();
        let mut to_add: Vec<Vec<Element>> = vec![Vec::new(); nb];
        let mut to_remove: Vec<Vec<Element>> = vec![Vec::new(); nb];
        for (e, old, new) in changes {
            to_add[new].push(e);
            to_remove[old].push(e);
        }
        let (laws, services) = p.split(&mesh, &self.solver.dof_manager);
        let ctx = LawContext {
            phase: LawPhase::Init,
            services: &services,
        };
        laws.apply_reassignment(&ctx, &to_remove, &to_add)
    }

    /// Upstream `setMaterialSelector(selector)`: the selector with the
    /// default chain as fallback.
    pub fn set_material_selector(&mut self, selector: Box<dyn ConstitutiveLawSelector>) {
        self.physics.materials.selector = SelectorChain::with_fallback_to_default(selector);
    }

    /// Replace the whole selector chain.
    pub fn set_material_selector_chain(&mut self, chain: SelectorChain) {
        self.physics.materials.selector = chain;
    }

    /// Solve one step (upstream `solveStep(solver_id)`): checks that the
    /// model processed every mesh change, then the solver drives the physics.
    pub fn solve_step(&mut self, solver_id: Option<&str>) -> Result<()> {
        self.physics.base.mesh.check_current(&self.token)?;
        self.physics.time_step = self.solver.time_step(solver_id)?;
        self.solver.solve_step(&mut self.physics, solver_id)
    }

    /// Solve one step with a model extension's callback around the physics
    /// (the cohesive model's `CohesiveCallback`): the mesh-revision check
    /// and time step of [`SolidMechanicsModel::solve_step`], then `f`
    /// drives the solver.
    ///
    /// # Errors
    /// Stale mesh revision, unknown solver, `f`'s error.
    pub fn solve_step_with(
        &mut self,
        solver_id: Option<&str>,
        f: impl FnOnce(&mut SolidMechanicsPhysics, &mut ModelSolver, Option<&str>) -> Result<()>,
    ) -> Result<()> {
        self.physics.base.mesh.check_current(&self.token)?;
        self.physics.time_step = self.solver.time_step(solver_id)?;
        f(&mut self.physics, &mut self.solver, solver_id)
    }

    /// Solve one step with an intercepting user callback (upstream
    /// `solveStep(InterceptSolverCallback &, solver_id)`).
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

    /// Upstream `setTimeStep(dt, solver_id)` (also the default dumper's
    /// time step).
    pub fn set_time_step(&mut self, time_step: Real, solver_id: Option<&str>) -> Result<()> {
        self.solver.set_time_step(time_step, solver_id)?;
        self.physics.time_step = self.solver.time_step(None).unwrap_or(time_step);
        self.dumpers.set_time_step(time_step)
    }

    /// Upstream `ModelSolver::setTimeStep(time_step, solver_id)` without
    /// the solid model's override: the solver's time step (and the time
    /// step the laws see), **no** dumper time step. What a coupler built on
    /// this model's solver calls for its own `setTimeStep` (upstream's
    /// couplers do not override it, so the mesh's default dumper gets no
    /// time step and writes no `.pvd`).
    ///
    /// # Errors
    /// Unknown solver.
    pub fn set_solver_time_step(&mut self, time_step: Real, solver_id: Option<&str>) -> Result<()> {
        self.solver.set_time_step(time_step, solver_id)?;
        self.physics.time_step = self.solver.time_step(None).unwrap_or(time_step);
        Ok(())
    }

    /// Upstream `getTimeStep(solver_id)`.
    pub fn time_step(&self, solver_id: Option<&str>) -> Result<Real> {
        self.solver.time_step(solver_id)
    }

    /// The time step the laws see (`ModelServices::time_step`; upstream laws
    /// call `model.getTimeStep()`, i.e. the default solver's): synchronised
    /// by `init_full`, `set_time_step`, `init_new_solver` and every solve
    /// step.
    #[must_use]
    pub fn law_time_step(&self) -> Real {
        self.physics.time_step
    }

    /// Re-read the default solver's time step into
    /// [`SolidMechanicsModel::law_time_step`] (0 without a default solver),
    /// as the end of [`SolidMechanicsModel::init_full`] does; for model
    /// extensions that run their own `initFull` (the cohesive model).
    pub fn sync_law_time_step(&mut self) {
        self.physics.time_step = self.solver.time_step(None).unwrap_or(0.0);
    }

    /// Upstream `getNewSolver(id, options)` (the model's `init_solver` runs).
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

    /// Upstream `initNewSolver(method)` (switch the analysis method): the
    /// method's default solver (created with the model's default options if
    /// missing, UD-009) becomes the default solver.
    ///
    /// **Deviation (UD-212, category B):** upstream
    /// `Model::initNewSolver` sets `this->method` only *after* creating the
    /// solver, while `getDefaultSolverOptions` reads it, so after
    /// `initFull(_static)` a call `initNewSolver(_explicit_consistent_mass)`
    /// creates the `explicit` solver with the implicit options of the
    /// previous method (trapezoidal rule 2, displacement solution). The port
    /// sets the method first: the new solver gets the options of the new
    /// method (central difference, acceleration solution). `init_full` is
    /// unaffected (upstream sets the method before as well).
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

    // ---- nodal fields (SMM-016 accessors) -----------------------------

    fn family(&self) -> Result<&akantu_solver::dof_manager::DofData> {
        self.solver
            .dof_manager
            .family(DOF_ID)
            .map_err(|_| not_allocated("displacement"))
    }

    fn family_mut(&mut self) -> Result<&mut akantu_solver::dof_manager::DofData> {
        self.solver
            .dof_manager
            .family_mut(DOF_ID)
            .map_err(|_| not_allocated("displacement"))
    }

    /// Upstream `getDisplacement()` (a guard of the exportable array).
    pub fn displacement(&self) -> Result<ArrayRef<'_, Real>> {
        self.family()?.dofs.read()
    }

    /// Mutable displacement (upstream non-const `getDisplacement()`; bumps
    /// nothing, as upstream).
    pub fn displacement_mut(&mut self) -> Result<ArrayMut<'_, Real>> {
        self.family_mut()?.dofs.write()
    }

    /// Upstream `getVelocity()` (dynamic analyses only).
    pub fn velocity(&self) -> Result<ArrayRef<'_, Real>> {
        self.family()?
            .derivatives
            .get(&1)
            .ok_or_else(|| not_allocated("velocity"))?
            .read()
    }

    /// Mutable velocity.
    pub fn velocity_mut(&mut self) -> Result<ArrayMut<'_, Real>> {
        self.family_mut()?
            .derivatives
            .get_mut(&1)
            .ok_or_else(|| not_allocated("velocity"))?
            .write()
    }

    /// Upstream `getAcceleration()` (dynamic analyses only).
    pub fn acceleration(&self) -> Result<ArrayRef<'_, Real>> {
        self.family()?
            .derivatives
            .get(&2)
            .ok_or_else(|| not_allocated("acceleration"))?
            .read()
    }

    /// Mutable acceleration.
    pub fn acceleration_mut(&mut self) -> Result<ArrayMut<'_, Real>> {
        self.family_mut()?
            .derivatives
            .get_mut(&2)
            .ok_or_else(|| not_allocated("acceleration"))?
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
        self.blocked_dofs_shared_mut()?.write()
    }

    /// The exportable blocked-flags array (foreign views, ARCHITECTURE.md, "Shared storage and its aliasing rules").
    pub fn blocked_dofs_shared_mut(&mut self) -> Result<&mut SharedArray<bool>> {
        self.family_mut()?
            .blocked
            .as_mut()
            .ok_or_else(|| not_allocated("blocked_dofs"))
    }

    /// Upstream `getIncrement()` (displacement increment of the last step).
    pub fn increment(&self) -> Result<ArrayRef<'_, Real>> {
        self.family()?
            .increment
            .as_ref()
            .ok_or_else(|| not_allocated("displacement_increment"))?
            .read()
    }

    /// Mutable increment.
    pub fn increment_mut(&mut self) -> Result<ArrayMut<'_, Real>> {
        self.family_mut()?
            .increment
            .as_mut()
            .ok_or_else(|| not_allocated("displacement_increment"))?
            .write()
    }

    /// Upstream `getPreviousDisplacement()`.
    pub fn previous_displacement(&self) -> Result<ArrayRef<'_, Real>> {
        self.family()?
            .previous
            .as_ref()
            .ok_or_else(|| not_allocated("previous_displacement"))?
            .read()
    }

    /// Upstream `getExternalForce()`.
    pub fn external_force(&self) -> Result<ArrayRef<'_, Real>> {
        self.physics.external_force()
    }

    /// Mutable external force.
    pub fn external_force_mut(&mut self) -> Result<ArrayMut<'_, Real>> {
        self.physics
            .external_force
            .as_mut()
            .ok_or_else(|| not_allocated("external_force"))?
            .write()
    }

    /// Upstream `getInternalForce()`.
    pub fn internal_force(&self) -> Result<ArrayRef<'_, Real>> {
        self.physics.internal_force()
    }

    /// Mutable internal force.
    pub fn internal_force_mut(&mut self) -> Result<ArrayMut<'_, Real>> {
        self.physics
            .internal_force
            .as_mut()
            .ok_or_else(|| not_allocated("internal_force"))?
            .write()
    }

    /// Upstream `getMass()` (lumped mass, allocated by its first assembly).
    pub fn mass(&self) -> Result<ArrayRef<'_, Real>> {
        self.physics.mass()
    }

    /// Mutable lumped mass. Upstream's C++ `getMass()` is `const`, but its
    /// Python binding returns a writeable NumPy view of the same array
    /// (`python/py_solid_mechanics_model.cc:171` `def_function_nocopy`,
    /// `python/py_aka_array.hh:121-131,211-221`: writeable by default), so
    /// upstream user code can write it in place too. The next lumped-mass
    /// assembly overwrites it (the `_smm_mass` test
    /// plants garbage in the slave masses).
    pub fn mass_mut(&mut self) -> Result<ArrayMut<'_, Real>> {
        self.physics
            .mass
            .as_mut()
            .ok_or_else(|| not_allocated("mass"))?
            .write()
    }

    /// Upstream `getCurrentPosition()` (SMM-013): `X + u`, recomputed when
    /// the displacement release changed.
    pub fn current_position(&mut self) -> Result<ArrayRef<'_, Real>> {
        self.update_current_position()?;
        self.physics
            .current_position
            .as_ref()
            .ok_or_else(|| not_allocated("current_position"))?
            .read()
    }

    /// Upstream `updateCurrentPosition()` (SMM-013): recompute the cached
    /// current position when the displacement release changed.
    pub fn update_current_position(&mut self) -> Result<()> {
        let mesh_h = self.physics.base.mesh.clone();
        let mesh = mesh_h.read()?;
        self.physics
            .update_current_position(&mesh, &self.solver.dof_manager)
    }

    /// The exportable array of a nodal field, by its upstream name
    /// (`displacement`, `velocity`, `acceleration`, `displacement_increment`
    /// / `increment`, `previous_displacement`, `external_force`,
    /// `internal_force`, `mass`, `current_position`), for foreign views
    /// (ARCHITECTURE.md, "Shared storage and its aliasing rules"). No recomputation (call
    /// [`SolidMechanicsModel::update_current_position`] first for the
    /// current position, as upstream's getter does).
    pub fn nodal_field_shared_mut(&mut self, name: &str) -> Result<&mut SharedArray<Real>> {
        let p = &mut self.physics;
        let own = match name {
            "external_force" => Some(&mut p.external_force),
            "internal_force" => Some(&mut p.internal_force),
            "mass" => Some(&mut p.mass),
            "current_position" => Some(&mut p.current_position),
            _ => None,
        };
        if let Some(slot) = own {
            return slot.as_mut().ok_or_else(|| not_allocated(name));
        }
        let f = self
            .solver
            .dof_manager
            .family_mut(DOF_ID)
            .map_err(|_| not_allocated("displacement"))?;
        match name {
            "displacement" => Ok(&mut f.dofs),
            "velocity" => f.derivatives.get_mut(&1).ok_or_else(|| not_allocated(name)),
            "acceleration" => f.derivatives.get_mut(&2).ok_or_else(|| not_allocated(name)),
            "increment" | "displacement_increment" => {
                f.increment.as_mut().ok_or_else(|| not_allocated(name))
            }
            "previous_displacement" => f.previous.as_mut().ok_or_else(|| not_allocated(name)),
            _ => Err(Error::not_found("nodal field", name)),
        }
    }

    /// Upstream `getDisplacementRelease()`.
    #[must_use]
    pub fn displacement_release(&self) -> i64 {
        self.physics.displacement_release()
    }

    /// Upstream `getCurrentPositionRelease()`.
    #[must_use]
    pub fn current_position_release(&self) -> i64 {
        self.physics.current_position_release
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

    // ---- materials (SMM-020) -------------------------------------------

    /// Upstream `getNbMaterials()`.
    #[must_use]
    pub fn nb_materials(&self) -> usize {
        self.physics.materials.nb_laws()
    }

    /// Upstream `getMaterial(index)`.
    pub fn material(&self, index: usize) -> Result<&dyn Material> {
        self.physics.materials.law(index)
    }

    /// Mutable material.
    pub fn material_mut(&mut self, index: usize) -> Result<&mut dyn Material> {
        let law: &mut (dyn Material + 'static) = self.physics.materials.law_mut(index)?;
        Ok(law)
    }

    /// Upstream `getMaterial(name)`.
    pub fn material_by_name(&self, name: &str) -> Result<&dyn Material> {
        let i = self.physics.materials.law_index_of(name)?;
        self.physics.materials.law(i)
    }

    /// Upstream `getMaterialIndex(name)`.
    pub fn material_index(&self, name: &str) -> Result<usize> {
        self.physics.materials.law_index_of(name)
    }

    /// Upstream `getMaterial(element)`.
    pub fn material_of(&self, element: Element) -> Result<&dyn Material> {
        self.physics.materials.law_of(element)
    }

    /// Upstream `getMaterialByElement()` (law index per element, `-1` = none).
    #[must_use]
    pub fn material_by_element(&self) -> &SharedElementTypeMapArray<Idx> {
        self.physics.materials.law_index_map()
    }

    /// Mutable law index map (filled by users before `init_full` to drive
    /// the default selector).
    pub fn material_by_element_mut(&mut self) -> &mut SharedElementTypeMapArray<Idx> {
        self.physics.materials.law_index_map_mut()
    }

    /// Upstream `getMaterialLocalNumbering()`.
    #[must_use]
    pub fn material_local_numbering(&self) -> &ElementTypeMapArray<Idx> {
        self.physics.materials.local_numbering_map()
    }

    /// Upstream `applyEigenGradU(prescribed, material_name, ghost)`: every
    /// material named `material_name` (SMM-019).
    pub fn apply_eigen_grad_u(
        &mut self,
        prescribed: &MatrixX,
        material_name: &str,
        ghost: GhostType,
    ) -> Result<()> {
        let d = self.physics.base.spatial_dimension;
        if prescribed.rows() * prescribed.cols() != d * d {
            return Err(Error::DimensionMismatch {
                context: "applyEigenGradU: The prescribed grad_u is not of the good size",
                expected: d * d,
                got: prescribed.rows() * prescribed.cols(),
            });
        }
        for law in self.physics.materials.laws_mut() {
            if law.base().name == material_name {
                law.base_mut().apply_eigen_grad_u(prescribed, ghost)?;
            }
        }
        Ok(())
    }

    /// Run `f` on a material with its phase context (energies by element,
    /// internal queries that need the model services).
    pub fn with_material<R>(
        &mut self,
        index: usize,
        phase: LawPhase,
        f: impl FnOnce(&mut dyn Material, &LawContext<'_>) -> Result<R>,
    ) -> Result<R> {
        let mesh_h = self.physics.base.mesh.clone();
        let mesh = mesh_h.read()?;
        let (laws, services) = self.physics.split(&mesh, &self.solver.dof_manager);
        let ctx = LawContext {
            phase,
            services: &services,
        };
        let law: &mut (dyn Material + 'static) = laws.law_mut(index)?;
        f(law, &ctx)
    }

    // ---- boundary conditions (SMM-021, BC-001) --------------------------

    /// Upstream `applyBC(DirichletFunctor, group_name)`: on the
    /// `displacement` family (values and blocked flags).
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

    /// Upstream `applyBC(NeumannFunctor, group_name)`: integrated on the
    /// boundary FE engine and **added** to `external_force`.
    ///
    /// The application is also recorded per boundary element (D-105,
    /// UD-1280; [`SolidMechanicsModel::neumann_store`]), so that a cohesive
    /// insertion doubling a loaded node moves each element's load to the
    /// copy the element uses (unless the cohesive model's compat switch
    /// `doubled_node_keeps_external_force` is set). **Forces written into
    /// `external_force` by other paths (D-108):** a node set to zero is a
    /// reset (its recorded contributions are forgotten: zero the array and
    /// apply the loads again to change them); a node changed to any other
    /// value is taken as carrying an **added** force, which stays with the
    /// original copy at a doubling while the recorded Neumann loads still
    /// move. Rescaling or replacing Neumann loads by writing the array (a
    /// load ramp `f ← λ f`, hand-computed values) is therefore not supported
    /// on a model that may insert at loaded faces: zero it and apply the
    /// loads again, or set the switch. A doubling that moves a record from a
    /// node written since it was recorded is counted
    /// (`NeumannStoreStats::moved_from_written_nodes`) and warned about once.
    pub fn apply_neumann_bc(&mut self, functor: &dyn NeumannFunctor, group: &str) -> Result<()> {
        let mesh_h = self.physics.base.mesh.clone();
        let mesh = mesh_h.read()?;
        let g = mesh
            .groups()
            .element_group(group)
            .map_err(|e| group_error(group, &e))?;
        self.apply_neumann_on(functor, &mesh, g)
            .map_err(|e| group_error(group, &e))
    }

    /// Every Neumann entry point of the model (the group and the all-groups
    /// variants, the contact coupler's and Python's `applyBC`) ends here:
    /// the application is recorded per boundary element (D-105, UD-1280,
    /// [`SolidMechanicsModel::neumann_store`]).
    fn apply_neumann_on(
        &mut self,
        functor: &dyn NeumannFunctor,
        mesh: &Mesh,
        group: ElementGroupRef<'_>,
    ) -> Result<()> {
        let dim = self.physics.base.spatial_dimension;
        let SolidMechanicsPhysics {
            base,
            external_force,
            neumann_store,
            ..
        } = &mut self.physics;
        let dual = external_force
            .as_mut()
            .ok_or_else(|| not_allocated("external_force"))?;
        if neumann_store.is_none() {
            let current = dual.read()?;
            *neumann_store = Some(Box::new(NeumannStore::new(
                current.as_slice(),
                current.nb_component(),
            )?));
        }
        let fe = base.fems.boundary("", mesh)?;
        apply_neumann_recorded(
            functor,
            mesh,
            group,
            fe,
            dim,
            dual,
            neumann_store.as_deref_mut(),
        )
    }

    /// The per-boundary-element record of the model's Neumann loads (D-105,
    /// UD-1280): `None` until the first Neumann application. Its rule for
    /// forces written into `external_force` by other paths (D-108) is
    /// stated at [`SolidMechanicsModel::apply_neumann_bc`] and in
    /// `akantu_model::neumann_store`; its statistics count the records moved
    /// from such nodes (`moved_from_written_nodes`).
    #[must_use]
    pub fn neumann_store(&self) -> Option<&NeumannStore> {
        self.physics.neumann_store.as_deref()
    }

    /// Upstream `applyBC(functor)` on every element group (name order; the
    /// upstream template iterating `element_group_end()` does not compile
    /// when instantiated, see report).
    pub fn apply_dirichlet_bc_all(&mut self, functor: &dyn DirichletFunctor) -> Result<()> {
        let mesh_h = self.physics.base.mesh.clone();
        let mesh = mesh_h.read()?;
        for g in mesh.groups().element_groups() {
            self.apply_dirichlet_on(functor, &mesh, g)
                .map_err(|e| group_error(g.name(), &e))?;
        }
        Ok(())
    }

    /// Neumann variant of [`SolidMechanicsModel::apply_dirichlet_bc_all`].
    pub fn apply_neumann_bc_all(&mut self, functor: &dyn NeumannFunctor) -> Result<()> {
        let mesh_h = self.physics.base.mesh.clone();
        let mesh = mesh_h.read()?;
        for g in mesh.groups().element_groups() {
            self.apply_neumann_on(functor, &mesh, g)
                .map_err(|e| group_error(g.name(), &e))?;
        }
        Ok(())
    }

    // ---- events (SMM-018) -------------------------------------------------

    /// Upstream `registerEventHandler(handler)` with the default priority
    /// `_ehp_highest` (the handler becomes the newest entry of the one list
    /// shared with the materials): see
    /// [`SolidMechanicsModel::register_event_handler_with_priority`].
    pub fn register_event_handler(
        &mut self,
        handler: Box<dyn SolidMechanicsModelEventHandler>,
    ) -> EventHandlerId {
        self.physics
            .register_handler(handler, EventPriority::HIGHEST)
    }

    /// Upstream `registerEventHandler(handler, priority)`
    /// (`aka_event_handler_manager.hh:58-73`, D-077): the handler is
    /// inserted at the `lower_bound` of `priority` in the one list of the
    /// materials and user handlers, i.e. after every entry of a lower value
    /// (higher priority) and before the entries of equal or higher value.
    /// Every material is registered at `_ehp_highest`
    /// ([`EventPriority::HIGHEST`], `material.cc:53`), so a handler at any
    /// other priority (e.g. [`EventPriority::LOWEST`]) runs after every
    /// material; among equal priorities the newest runs first. Upstream
    /// refuses a second registration of the same handler object; an owned
    /// box cannot be registered twice, so this never fails.
    ///
    /// # Errors
    /// None today (kept as `Result` for the proposal's API, D-077).
    pub fn register_event_handler_with_priority(
        &mut self,
        handler: Box<dyn SolidMechanicsModelEventHandler>,
        priority: EventPriority,
    ) -> Result<EventHandlerId> {
        Ok(self.physics.register_handler(handler, priority))
    }

    /// Upstream `unregisterEventHandler(handler)`
    /// (`aka_event_handler_manager.hh:77-86`, D-077): removes the handler
    /// from the list and gives it back; the order of the others is kept.
    ///
    /// # Errors
    /// `id` is not registered (any more): upstream throws "This event
    /// handler is not registered".
    pub fn unregister_event_handler(
        &mut self,
        id: EventHandlerId,
    ) -> Result<Box<dyn SolidMechanicsModelEventHandler>> {
        let handlers = &mut self.physics.handlers;
        let pos = handlers
            .iter()
            .position(|h| h.id == id)
            .ok_or_else(|| Error::invalid("This event handler is not registered"))?;
        Ok(handlers.remove(pos).handler)
    }

    /// Upstream `sendEvent(event)` (`EventHandlerManager::sendEvent`,
    /// SMM-018): upstream's model keeps **one** handler list, in which every
    /// material registers itself when it is constructed (`material.cc:53`)
    /// and every user handler is registered, by default at `_ehp_highest`;
    /// `registerEventHandler` inserts at the `lower_bound` of the priority,
    /// i.e. before the handlers of equal priority
    /// (`aka_event_handler_manager.hh:58-73`), so the list is dispatched
    /// **newest first** across materials and user handlers of the default
    /// priority, then the handlers registered with another priority
    /// ([`SolidMechanicsModel::register_event_handler_with_priority`]) in
    /// increasing priority value, newest first among equals. Materials react
    /// as upstream's `Material`: `BeforeDump` → `onDump` (Cauchy stresses in
    /// finite deformation), `BeginningOfDamageIteration` →
    /// `onDamageIteration` (`savePreviousState`), `AfterDamage` →
    /// `onDamageUpdate` (`updateEnergiesAfterDamage`), the solve-step events
    /// → nothing. The first error stops the dispatch (upstream: the
    /// exception propagates).
    pub fn send_event(&mut self, event: SolidMechanicsModelEvent) -> Result<()> {
        let mesh_h = self.physics.base.mesh.clone();
        let mesh = mesh_h.read()?;
        self.physics
            .send_event(&mesh, &self.solver.dof_manager, event)
    }

    /// Upstream `forceReassembleMassLumped()`? Not upstream: the lumped and
    /// consistent mass flags (`need_to_reassemble_*`) set again (upstream
    /// sets them on node additions).
    ///
    /// **Collective use on a distributed mesh**: the
    /// flag decides whether the next [`SolidMechanicsModel::assemble_mass_lumped`]
    /// reassembles, and the reassembly runs the `_smm_density` exchange
    /// (MPI-036). Call it on every rank: on some ranks only, those ranks
    /// enter the exchange alone — detected as a protocol divergence at the
    /// next agreement, or a hang if no collective follows (upstream has the
    /// same structure).
    pub fn force_mass_reassembly(&mut self) {
        self.physics.need_to_reassemble_lumped_mass = true;
        self.physics.need_to_reassemble_mass = true;
    }

    // ---- parallel synchronisation (SMM-016, MODEL-011) -------------------

    /// Upstream `SynchronizerRegistry::synchronize(tag)` for the tags the
    /// model registers on the mesh's element synchronizer (`_smm_mass`,
    /// `_smm_stress`, `_smm_gradu`, `_smm_density`, `_for_dump`,
    /// `_constitutive_law_id`; plus `_smm_boundary`, which upstream's
    /// element accessor supports for the cohesive model): one collective
    /// exchange owner → ghost of the
    /// model's and the laws' data for `tag` (see `model::parallel`). Does
    /// nothing on a mesh without element synchronizer (serial).
    /// `_smm_for_gradu` does nothing, as upstream: the model never registers
    /// a synchronizer for it (`solid_mechanics_model.cc:78-82`), so
    /// `SynchronizerRegistry::synchronize` iterates over none (its pack
    /// case, the displacement of the element nodes, is unreachable and its
    /// unpack only delegates to the laws: UD-233).
    ///
    /// MPI-041, MPI-036, MPI-053: `_smm_density` and
    /// `_clh_init_cl` carry the laws' own data
    /// ([`crate::Material::exchanged_internals`]: phase field with
    /// `degrade_mass`, Marigo's `Yd`; `_clh_init_cl` is not registered with
    /// [`SmmUpstreamCompat::clh_init_cl_not_exchanged`], UD-900); the
    /// extension tags `_test`, `_user_1`, `_user_2` exchange the laws' data
    /// once registered with
    /// [`SolidMechanicsModel::register_synchronization_tag`] (on every rank:
    /// the registration is agreed first, a tag registered on some ranks
    /// only is an error on every rank). Law data
    /// declared for any other tag are refused at `init_materials`.
    /// Another tag
    /// is an error on a distributed mesh (UD-553, category B; upstream: no
    /// synchronizer registered, nothing happens — reproduced by
    /// [`SmmUpstreamCompat::unregistered_synchronization_tag_is_a_noop`]).
    pub fn synchronize(&mut self, tag: akantu_core::enums::SynchronizationTag) -> Result<()> {
        use akantu_core::enums::SynchronizationTag as T;
        let mesh_h = self.physics.base.mesh.clone();
        let mesh = mesh_h.read()?;
        let Some(es) = &mesh.synchronizers().element else {
            return Ok(());
        };
        let dim = self.physics.base.spatial_dimension;
        let compat = self.physics.upstream_compat;
        if matches!(tag, T::Test | T::User1 | T::User2) {
            // The registration is a per-rank call; it is
            // agreed first, so a tag registered on some ranks only is an
            // error on every rank instead of a divergence (upstream: the
            // registered ranks wait in the exchange for ever).
            let registered = self.physics.extension_tags.contains(&tag);
            let mut ph = Phases::new(&es.communicator, "smm:synchronize_extension_tag");
            let count = ph.run_collective("registered", Ok(u64::from(registered)), |c, v| {
                let mut s = [v];
                c.all_reduce(&mut s, akantu_comm::ReduceOp::Sum)?;
                Ok((s[0], c.size() as u64))
            });
            let r = match count {
                Ok((n, size)) if n == size => {
                    let nb_quad = self
                        .physics
                        .base
                        .fems
                        .get("")
                        .and_then(|fe| assembly::nb_quad_map(fe, &mesh, dim));
                    ph.agreed("prepare", nb_quad, Severity::Recoverable)
                        .and_then(|nb_quad| {
                            let mut acc = parallel::LawDataAccessor {
                                materials: &mut self.physics.materials,
                                nb_quad: &nb_quad,
                            };
                            es.synchronize_in(&mut ph, &mut acc, tag)
                        })
                }
                Ok((0, _)) if compat.unregistered_synchronization_tag_is_a_noop => Ok(()),
                Ok((0, _)) => Err(Error::invalid(format!(
                    "SolidMechanicsModel::synchronize: tag {tag} is not registered by the model"
                ))),
                Ok((n, size)) => Err(Error::invalid(format!(
                    "SolidMechanicsModel::synchronize: the extension tag {tag} is registered on \
                     {n} of {size} ranks (register_synchronization_tag must be called on every \
                     rank)"
                ))),
                Err(e) => Err(e),
            };
            return ph.settle(r);
        }
        let law_data = match tag {
            T::SmmDensity => true,
            T::ClhInitCl => !compat.clh_init_cl_not_exchanged,
            _ => false,
        };
        if law_data {
            let fe = self.physics.base.fems.get("")?;
            let nb_quad = assembly::nb_quad_map(fe, &mesh, dim)?;
            let mut acc = parallel::LawDataAccessor {
                materials: &mut self.physics.materials,
                nb_quad: &nb_quad,
            };
            return es.synchronize(&mut acc, tag);
        }
        match tag {
            // never registered upstream: synchronize(_smm_for_gradu) is a
            // no-op there (MPI-031)
            T::SmmForGradu => Ok(()),
            T::SmmStress | T::SmmGradu => {
                parallel::check_no_law_data(&self.physics.materials, tag)?;
                let fe = self.physics.base.fems.get("")?;
                let nb_quad = assembly::nb_quad_map(fe, &mesh, dim)?;
                let mut acc = parallel::StressAccessor {
                    materials: &mut self.physics.materials,
                    nb_quad: &nb_quad,
                    dim,
                };
                es.synchronize(&mut acc, tag)
            }
            T::ConstitutiveLawId => {
                let mut acc = parallel::LawIdAccessor {
                    materials: &mut self.physics.materials,
                };
                es.synchronize(&mut acc, tag)
            }
            T::SmmMass | T::ForDump | T::SmmBoundary => {
                let dm = &self.solver.dof_manager;
                fn get<'x>(
                    a: Option<&'x SharedArray<Real>>,
                    name: &str,
                ) -> Result<ArrayRef<'x, Real>> {
                    a.ok_or_else(|| not_allocated(name))?.read()
                }
                // guards first (they outlive the accessor below)
                let (reals, blocked): (Vec<ArrayRef<'_, Real>>, Option<ArrayRef<'_, bool>>) =
                    if tag == T::SmmMass {
                        (vec![get(self.physics.mass.as_ref(), "mass")?], None)
                    } else if tag == T::SmmBoundary {
                        // upstream counts the blocked flags as Reals in
                        // getNbData but packs bools; the typed wire contract
                        // uses the packed size (one byte per flag)
                        (
                            vec![
                                get(self.physics.external_force.as_ref(), "external_force")?,
                                dm.dofs_derivative(DOF_ID, 1)?,
                            ],
                            Some(dm.blocked_dofs(DOF_ID)?),
                        )
                    } else {
                        (
                            vec![
                                dm.dofs(DOF_ID)?,
                                dm.dofs_derivative(DOF_ID, 1)?,
                                dm.dofs_derivative(DOF_ID, 2)?,
                                get(self.physics.internal_force.as_ref(), "internal_force")?,
                                get(self.physics.external_force.as_ref(), "external_force")?,
                            ],
                            None,
                        )
                    };
                let mut arrays: Vec<parallel::NodalArray<'_>> = reals
                    .iter()
                    .map(|a| parallel::NodalArray::Real(a))
                    .collect();
                if let Some(b) = &blocked {
                    arrays.push(parallel::NodalArray::Bool(b));
                }
                parallel::check_no_law_data(&self.physics.materials, tag)?;
                let mut acc = parallel::ElementNodalAccessor {
                    mesh: &mesh,
                    dim,
                    arrays,
                };
                es.synchronize(&mut acc, tag)
            }
            // UD-553: upstream iterates the (empty) range of synchronizers
            // registered for the tag
            _ if compat.unregistered_synchronization_tag_is_a_noop => Ok(()),
            other => Err(Error::invalid(format!(
                "SolidMechanicsModel::synchronize: tag {other} is not registered by the model"
            ))),
        }
    }

    /// Upstream `registerSynchronizer(mesh.getElementSynchronizer(), tag)`
    /// on the model's registry for a user **extension** tag (`_test`,
    /// `_user_1`, `_user_2`; MPI-053): later
    /// [`SolidMechanicsModel::synchronize`] calls exchange, owner → ghost
    /// over the mesh's element synchronizer, the internals the laws name in
    /// [`crate::Material::exchanged_internals`] for the tag (upstream: the
    /// model's data accessor forwards the tag to every law's `packData`).
    /// Registering twice is a no-op (upstream would run the exchange twice,
    /// with the same result).
    ///
    /// **Register on every rank**: the call itself is
    /// local, and [`SolidMechanicsModel::synchronize`] agrees the
    /// registration before the exchange: a tag registered on some ranks
    /// only is an error on every rank (upstream: the registered ranks wait
    /// in the exchange for ever).
    ///
    /// # Errors
    /// Any other tag: the model registers its own tags itself, and the
    /// reserved tags (`_whatever`, `_update`, `_ask_nodes`, `_size`,
    /// `_htm_phi`, `_htm_gradient_phi`, `_solver_solution`, …) have no
    /// consumer in the model.
    pub fn register_synchronization_tag(
        &mut self,
        tag: akantu_core::enums::SynchronizationTag,
    ) -> Result<()> {
        use akantu_core::enums::SynchronizationTag as T;
        if !matches!(tag, T::Test | T::User1 | T::User2) {
            return Err(Error::invalid(format!(
                "SolidMechanicsModel::register_synchronization_tag: {tag} is not a user \
                 extension tag (_test, _user_1, _user_2)"
            )));
        }
        if !self.physics.extension_tags.contains(&tag) {
            self.physics.extension_tags.push(tag);
        }
        Ok(())
    }

    /// Upstream DOF-entity data accessor of the model (`_smm_uv`: displacement
    /// and velocity; `_smm_res`: internal force; `_smm_mass`: mass;
    /// `_for_dump`: displacement, velocity, acceleration, internal and
    /// external force) driven by the mesh's **node** synchronizer: masters
    /// overwrite their slaves' values (MPI-026…029). Collective; nothing on
    /// a mesh without node synchronizer.
    pub fn synchronize_nodal(&mut self, tag: akantu_core::enums::SynchronizationTag) -> Result<()> {
        use akantu_core::enums::SynchronizationTag as T;
        let mesh_h = self.physics.base.mesh.clone();
        let mesh = mesh_h.read()?;
        let Some(ns) = &mesh.synchronizers().node else {
            return Ok(());
        };
        let dim = self.physics.base.spatial_dimension;
        let p = &mut self.physics;
        let dm = &mut self.solver.dof_manager;
        let mut u = dm.dofs(DOF_ID)?.clone();
        let mut v = dm.dofs_derivative(DOF_ID, 1)?.clone();
        let mut a = dm.dofs_derivative(DOF_ID, 2)?.clone();
        let mut fi = p
            .internal_force
            .as_ref()
            .ok_or_else(|| not_allocated("internal_force"))?
            .read()?
            .clone();
        let mut fe = p
            .external_force
            .as_ref()
            .ok_or_else(|| not_allocated("external_force"))?
            .read()?
            .clone();
        let mut m = match &p.mass {
            Some(m) => Some(m.read()?.clone()),
            None => None,
        };
        {
            let arrays: Vec<&mut Array<Real>> = match tag {
                T::SmmUv => vec![&mut u, &mut v],
                T::SmmRes => vec![&mut fi],
                T::SmmMass => vec![m.as_mut().ok_or_else(|| not_allocated("mass"))?],
                T::ForDump => vec![&mut u, &mut v, &mut a, &mut fi, &mut fe],
                other => {
                    return Err(Error::invalid(format!(
                        "SolidMechanicsModel::synchronize_nodal: unknown ghost synchronization \
                         tag {other}"
                    )));
                }
            };
            let mut acc = parallel::NodalAccessor { dim, arrays };
            ns.synchronize(&mut acc, tag)?;
        }
        dm.dofs_mut(DOF_ID)?.copy_from(&u)?;
        dm.dofs_derivative_mut(DOF_ID, 1)?.copy_from(&v)?;
        dm.dofs_derivative_mut(DOF_ID, 2)?.copy_from(&a)?;
        // in place (equal lengths): foreign views stay attached
        if let Some(a) = &mut p.internal_force {
            a.write()?.copy_from(&fi)?;
        }
        if let Some(a) = &mut p.external_force {
            a.write()?.copy_from(&fe)?;
        }
        if let (Some(a), Some(m)) = (&mut p.mass, &m) {
            a.write()?.copy_from(m)?;
        }
        Ok(())
    }

    // ---- explicit assembly entry points ---------------------------------

    /// Upstream `assembleInternalForces()` (outside a solve: one collective
    /// operation of the DOF manager).
    pub fn assemble_internal_forces(&mut self) -> Result<()> {
        let p = &mut self.physics;
        self.solver
            .dof_manager
            .collective("smm:assemble_internal_forces", |dm| {
                p.assemble_internal_forces(dm)
            })
    }

    /// Release the memory of the static solver once the caller will not
    /// solve again with the current operator (port extension, not called by
    /// the library: upstream keeps the factors and the matrices for the
    /// model's lifetime). Freed: the analysis and factorisation of `J`'s
    /// sparse solver (MUMPS: its instance, an agreed collective), all of
    /// `J` (it is rebuilt from `K`), and `K`'s values and lookup map. Kept:
    /// `K`'s coordinates in their storage order (16 bytes per entry) and the
    /// record of its profile. A later solve assembles `K` again (whatever
    /// the laws report) into the kept profile, so every entry keeps its
    /// position, rebuilds `J` from it and analyses and factorises again:
    /// the results are bitwise those of a run without the release, also
    /// for a profile that grew after the first assembly (contact) or a
    /// matrix promoted to unsymmetric storage. Typical use: after the static
    /// preload of an explicit run. Do not call it between the steps of an
    /// implicit run: each step would pay a new assembly, analysis and
    /// factorisation.
    ///
    /// **Collective** (every rank, same program point, outside any
    /// collective operation). Errors: [`akantu_core::Error::Unsupported`]
    /// for a backend that cannot release its factorisation (PETSc), before
    /// any matrix is released.
    pub fn release_static_solver(&mut self) -> Result<()> {
        let dm = &mut self.solver.dof_manager;
        if dm.has_matrix("J") {
            dm.release_factorization("J")?;
            dm.release_matrix("J")?;
        }
        if dm.has_matrix("K") {
            dm.release_matrix_values("K")?;
            self.physics.need_to_reassemble_stiffness = true;
        }
        Ok(())
    }

    /// Upstream `assembleStiffnessMatrix(need_to_reassemble)`.
    pub fn assemble_stiffness_matrix(&mut self, need_to_reassemble: bool) -> Result<()> {
        let p = &mut self.physics;
        self.solver
            .dof_manager
            .collective("smm:assemble_stiffness", |dm| {
                p.assemble_stiffness_matrix(dm, need_to_reassemble)?;
                dm.finalize_matrix("K")
            })
    }

    /// Upstream `assembleMassLumped()`.
    ///
    /// **Collective** (every rank calls it): on a
    /// distributed mesh a reassembly exchanges `_smm_density` before the
    /// lumping. Whether it reassembles is each rank's reassembly flag (node
    /// events, which reach every rank, and
    /// [`SolidMechanicsModel::force_mass_reassembly`]): keep it equal on
    /// every rank.
    pub fn assemble_mass_lumped(&mut self) -> Result<()> {
        let p = &mut self.physics;
        self.solver
            .dof_manager
            .collective("smm:assemble_mass_lumped", |dm| p.assemble_mass_lumped(dm))
    }

    /// Upstream `assembleMass()` (consistent mass matrix `M`).
    pub fn assemble_mass(&mut self) -> Result<()> {
        let p = &mut self.physics;
        self.solver
            .dof_manager
            .collective("smm:assemble_mass", |dm| {
                p.assemble_mass(dm)?;
                dm.finalize_matrix("M")
            })
    }

    /// Upstream `assembleResidual()` (SMM-004): **accumulates** into the
    /// DOF manager's residual (`r += f_ext + f_int`), as upstream
    /// (the solvers zero the residual before calling the
    /// callback; a user calling this entry point twice gets twice the
    /// forces — zero it first with `dof_manager_mut().zero_residual()`).
    pub fn assemble_residual(&mut self) -> Result<()> {
        let p = &mut self.physics;
        self.solver
            .dof_manager
            .collective("smm:assemble_residual", |dm| p.assemble_residual(dm))
    }
}

impl SolidMechanicsPhysics {
    /// Upstream `getDefaultSolverID(method)` (SMM-002).
    #[must_use]
    pub fn default_solver_id(&self, method: AnalysisMethod) -> (Id, TimeStepSolverType) {
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

    /// Split into the materials and the services reading everything else
    /// (the `(&mut laws, &services)` phase split, ARCHITECTURE).
    pub(crate) fn split<'a>(
        &'a mut self,
        mesh: &'a Mesh,
        dofs: &'a DofManager,
    ) -> (
        &'a mut ConstitutiveLawsHandler<dyn Material>,
        SmmServices<'a>,
    ) {
        self.split_arrays(mesh, DofArrays::of(dofs))
    }

    /// [`SolidMechanicsPhysics::split`] with the DOF arrays given (borrowed
    /// apart from the DOF manager's collective scope).
    pub(crate) fn split_arrays<'a>(
        &'a mut self,
        mesh: &'a Mesh,
        dofs: DofArrays<'a>,
    ) -> (
        &'a mut ConstitutiveLawsHandler<dyn Material>,
        SmmServices<'a>,
    ) {
        let Self {
            base,
            materials,
            external_force,
            internal_force,
            mass,
            current_position,
            time_step,
            extra_point_sources,
            ..
        } = self;
        let services = SmmServices {
            mesh,
            fems: &base.fems,
            extra_points: extra_point_sources,
            dofs,
            own: OwnArrays {
                external_force: external_force.as_ref(),
                internal_force: internal_force.as_ref(),
                mass: mass.as_ref(),
                current_position: current_position.as_ref(),
            },
            spatial_dimension: base.spatial_dimension,
            time_step: *time_step,
        };
        (materials, services)
    }

    fn register_handler(
        &mut self,
        handler: Box<dyn SolidMechanicsModelEventHandler>,
        priority: EventPriority,
    ) -> EventHandlerId {
        let id = EventHandlerId(self.next_handler_id);
        self.next_handler_id += 1;
        self.handlers.push(RegisteredHandler {
            id,
            priority,
            laws_before: self.materials.nb_laws(),
            handler,
        });
        id
    }

    /// [`SolidMechanicsModel::send_event`]: upstream's list, sorted by
    /// priority value, newest first among equal values (law `i` was
    /// registered before every handler with `laws_before > i`; laws are only
    /// ever appended and are all at `_ehp_highest`).
    fn send_event(
        &mut self,
        mesh: &Mesh,
        dofs: &DofManager,
        event: SolidMechanicsModelEvent,
    ) -> Result<()> {
        let mut handlers = std::mem::take(&mut self.handlers);
        let r = self.dispatch_newest_first(mesh, dofs, &mut handlers, event);
        self.handlers = handlers;
        r
    }

    fn dispatch_newest_first(
        &mut self,
        mesh: &Mesh,
        dofs: &DofManager,
        handlers: &mut [RegisteredHandler],
        event: SolidMechanicsModelEvent,
    ) -> Result<()> {
        // `_ehp_highest`: the laws and the user handlers of that priority,
        // newest first; laws [0, pending) have not been dispatched yet
        let mut pending = self.materials.nb_laws();
        for h in handlers
            .iter_mut()
            .rev()
            .filter(|h| h.priority == EventPriority::HIGHEST)
        {
            let start = h.laws_before.min(pending);
            for law in (start..pending).rev() {
                self.law_event(mesh, dofs, law, event)?;
            }
            pending = start;
            h.receive(event)?;
        }
        for law in (0..pending).rev() {
            self.law_event(mesh, dofs, law, event)?;
        }
        // then every other priority value in increasing order, newest first
        let mut done = EventPriority::HIGHEST;
        while let Some(p) = handlers
            .iter()
            .map(|h| h.priority)
            .filter(|&p| p > done)
            .min()
        {
            for h in handlers.iter_mut().rev().filter(|h| h.priority == p) {
                h.receive(event)?;
            }
            done = p;
        }
        Ok(())
    }

    /// Upstream `Material`'s `SolidMechanicsModelEventHandler` overrides.
    fn law_event(
        &mut self,
        mesh: &Mesh,
        dofs: &DofManager,
        index: usize,
        event: SolidMechanicsModelEvent,
    ) -> Result<()> {
        let (laws, services) = self.split(mesh, dofs);
        let law = laws.law_mut(index)?;
        match event {
            SolidMechanicsModelEvent::BeforeSolveStep(_)
            | SolidMechanicsModelEvent::AfterSolveStep(_) => Ok(()),
            SolidMechanicsModelEvent::BeforeDump => law.on_dump(),
            SolidMechanicsModelEvent::BeginningOfDamageIteration => law.on_damage_iteration(),
            SolidMechanicsModelEvent::AfterDamage => {
                let ctx = LawContext {
                    phase: LawPhase::Energy,
                    services: &services,
                };
                law.on_damage_update(&ctx)
            }
        }
    }

    /// Upstream `updateCurrentPosition()`.
    pub(crate) fn update_current_position(&mut self, mesh: &Mesh, dofs: &DofManager) -> Result<()> {
        if self.current_position_release == self.displacement_release {
            return Ok(());
        }
        let u = dofs
            .dofs(DOF_ID)
            .map_err(|_| not_allocated("displacement"))?;
        let mut cp = self
            .current_position
            .as_mut()
            .ok_or_else(|| not_allocated("current_position"))?
            .write()?;
        let d = self.base.spatial_dimension;
        check_nodal(
            "updateCurrentPosition: current_position",
            &cp,
            mesh.nb_nodes(),
            d,
        )?;
        check_nodal(
            "updateCurrentPosition: displacement",
            &u,
            mesh.nb_nodes(),
            d,
        )?;
        let nodes = mesh.nodes();
        check_nodal(
            "updateCurrentPosition: mesh nodes",
            &nodes,
            mesh.nb_nodes(),
            d,
        )?;
        cp.as_mut_slice().copy_from_slice(nodes.as_slice());
        for (c, d) in cp.as_mut_slice().iter_mut().zip(u.as_slice()) {
            *c += d;
        }
        self.current_position_release = self.displacement_release;
        Ok(())
    }

    /// Upstream `getCurrentPosition()` for model extensions (the cohesive
    /// model computes its normals on the current positions): refresh
    /// `current_position = X + u` if the displacement changed since the
    /// last refresh, then return it.
    ///
    /// # Errors
    /// Arrays not allocated (call `init_full` first) or with a wrong shape.
    pub fn refresh_current_position(&mut self, dofs: &DofManager) -> Result<ArrayRef<'_, Real>> {
        let mesh_h = self.base.mesh.clone();
        let mesh = mesh_h.read()?;
        self.update_current_position(&mesh, dofs)?;
        self.current_position
            .as_ref()
            .ok_or_else(|| not_allocated("current_position"))?
            .read()
    }

    /// Run `f` with the materials and the read-only model services of
    /// `phase` (the model's `(&mut laws, &services)` split, ARCHITECTURE, "Laws (D-016)") — for model extensions that drive their own laws inside the
    /// solver callbacks (the cohesive model's tractions).
    ///
    /// # Errors
    /// An unusable mesh domain or `f`'s error.
    pub fn with_laws<R>(
        &mut self,
        dofs: &DofManager,
        phase: LawPhase,
        f: impl FnOnce(&mut ConstitutiveLawsHandler<dyn Material>, &LawContext<'_>) -> Result<R>,
    ) -> Result<R> {
        let mesh_h = self.base.mesh.clone();
        let mesh = mesh_h.read()?;
        let (laws, services) = self.split(&mesh, dofs);
        let ctx = LawContext {
            phase,
            services: &services,
        };
        f(laws, &ctx)
    }

    /// Upstream `getMatrixType(id)` (SMM-006): `C` → not defined; every
    /// other id → symmetric. Upstream computes the maximum of the
    /// materials' `K` types and then discards it (UD candidate, reported):
    /// `K` is always declared symmetric, as the materials' elemental
    /// matrices are assembled `_symmetric`.
    #[must_use]
    pub fn model_matrix_type(&self, matrix_id: &str) -> MatrixType {
        if matrix_id == "C" {
            MatrixType::NotDefined
        } else {
            MatrixType::Symmetric
        }
    }

    /// Upstream `onNodesAdded` (SMM-015) for the model-owned arrays (the DOF
    /// family grows in the DOF manager at level 20).
    fn on_nodes_added(&mut self, mesh: &Mesh) -> Result<()> {
        let n = mesh.nb_nodes();
        self.displacement_release += 1;
        for a in [
            &mut self.mass,
            &mut self.external_force,
            &mut self.internal_force,
            &mut self.current_position,
        ]
        .into_iter()
        .flatten()
        {
            a.write()?.try_resize(n, 0.0)?;
        }
        self.need_to_reassemble_lumped_mass = true;
        self.need_to_reassemble_mass = true;
        Ok(())
    }

    /// Upstream `onNodesRemoved` (SMM-015, `solid_mechanics_model.cc:
    /// 752-783`) for the model-owned arrays (the DOF family was renumbered
    /// and compacted by the DOF manager at level 20): `mass`,
    /// `external_force`, `internal_force` and `current_position` follow the
    /// numbering (upstream `removeNodesFromArray`; upstream leaves
    /// `current_position` uncompacted, UD-209). Unlike upstream, the masses
    /// and the stiffness are flagged for reassembly: the DOF manager reset
    /// every matrix (their profiles name the old equations), and a law that
    /// tracks its stiffness assembly would otherwise skip `K`
    /// (`mark_stiffness_changed`). Every array is validated before any is
    /// changed.
    fn on_nodes_removed(&mut self, new_numbering: &[Idx]) -> Result<()> {
        let mut plan = None;
        for a in [
            &self.mass,
            &self.external_force,
            &self.internal_force,
            &self.current_position,
        ]
        .into_iter()
        .flatten()
        {
            plan = Some(akantu_model::model::node_compaction_plan(
                a.read()?.len(),
                new_numbering,
            )?);
        }
        if let Some(plan) = &plan {
            for a in [
                &mut self.mass,
                &mut self.external_force,
                &mut self.internal_force,
                &mut self.current_position,
            ]
            .into_iter()
            .flatten()
            {
                akantu_model::model::apply_node_compaction_guard(&mut a.write()?, plan);
            }
        }
        self.displacement_release += 1;
        self.need_to_reassemble_lumped_mass = true;
        self.need_to_reassemble_mass = true;
        for law in self.materials.laws_mut() {
            law.mark_stiffness_changed();
        }
        Ok(())
    }
}

/// D-105 / UD-1280: the Neumann load of a doubled node follows its boundary
/// elements (`akantu_model::neumann_store`).
impl SolidMechanicsPhysics {
    /// The per-boundary-element record of the Neumann loads (`None` until
    /// the first Neumann application).
    #[must_use]
    pub fn neumann_store(&self) -> Option<&NeumannStore> {
        self.neumann_store.as_deref()
    }

    /// Move the recorded Neumann contributions of doubled nodes to the
    /// copies their boundary elements now use (`new_nodes[i]` doubles
    /// `old_nodes[i]`, local indices of the rank: local, slave and ghost
    /// copies; [`NeumannStore::move_doubled_nodes`]). Called by the cohesive
    /// model after a doubling unless its compat switch
    /// `doubled_node_keeps_external_force` (upstream) is set. Rank-local: no
    /// communication. Without a store (no Neumann load applied) nothing
    /// moves.
    ///
    /// # Errors
    /// External force not allocated, inconsistent pairs or store.
    pub fn move_doubled_node_external_force(
        &mut self,
        mesh: &Mesh,
        new_nodes: &[Idx],
        old_nodes: &[Idx],
    ) -> Result<MoveReport> {
        let Some(store) = self.neumann_store.as_mut() else {
            return Ok(MoveReport::default());
        };
        let mut f = self
            .external_force
            .as_mut()
            .ok_or_else(|| not_allocated("external_force"))?
            .write()?;
        store.move_doubled_nodes(mesh, f.as_mut_slice(), new_nodes, old_nodes)
    }

    /// The store's part of a mesh event (the model level, after the nodal
    /// arrays followed it).
    fn neumann_store_on_event(&mut self, mesh: &Mesh, event: &MeshEvent) -> Result<()> {
        let Some(store) = self.neumann_store.as_mut() else {
            return Ok(());
        };
        match event {
            // The copies are recorded and every record is brought to
            // its element's current row by node identity (the insertion
            // copied the facet rows into the boundary rows before this event)
            MeshEvent::NodesAdded {
                nodes, old_nodes, ..
            } => {
                store.on_nodes_added(mesh.nb_nodes());
                let f = self
                    .external_force
                    .as_ref()
                    .ok_or_else(|| not_allocated("external_force"))?
                    .read()?;
                let dual = Some(f.as_slice());
                match old_nodes {
                    Some(old) => store.on_nodes_doubled(mesh, dual, nodes, old),
                    None => store.on_nodes_doubled(mesh, dual, &[], &[]),
                }
            }
            MeshEvent::NodesRemoved { new_numbering, .. } => store.on_nodes_removed(new_numbering),
            MeshEvent::ElementsAdded { elements, .. } => store.on_elements_added(elements),
            MeshEvent::ElementsRemoved { new_numbering, .. } => {
                store.on_elements_removed(new_numbering)
            }
            MeshEvent::ElementsChanged {
                old,
                new,
                new_numbering,
                ..
            } => store.on_elements_changed(mesh, old, new, new_numbering),
            _ => Ok(()),
        }
    }
}

impl SolverCallback for SolidMechanicsPhysics {
    fn matrix_type(&self, matrix_id: &str) -> MatrixType {
        self.model_matrix_type(matrix_id)
    }

    /// Upstream `assembleMatrix(id)`: `K` → stiffness, `M` → consistent
    /// mass, anything else ignored.
    fn assemble_matrix(&mut self, dofs: &mut DofManager, matrix_id: &str) -> Result<()> {
        match matrix_id {
            "K" => self.assemble_stiffness_matrix(dofs, false),
            "M" => self.assemble_mass(dofs),
            _ => Ok(()),
        }
    }

    /// Upstream `assembleLumpedMatrix(id)`: `M` → lumped mass.
    fn assemble_lumped_matrix(&mut self, dofs: &mut DofManager, matrix_id: &str) -> Result<()> {
        if matrix_id == "M" {
            self.assemble_mass_lumped(dofs)
        } else {
            Ok(())
        }
    }

    fn assemble_residual(&mut self, dofs: &mut DofManager) -> Result<()> {
        SolidMechanicsPhysics::assemble_residual(self, dofs)
    }

    /// Upstream `assembleResidual(part)`: `external`, `internal`, otherwise
    /// `SolverCallbackResidualPartUnknown`.
    fn assemble_residual_part(&mut self, dofs: &mut DofManager, part: &str) -> Result<()> {
        match part {
            "external" => {
                let f = self
                    .external_force
                    .as_ref()
                    .ok_or_else(|| not_allocated("external_force"))?
                    .read()?;
                dofs.assemble_to_residual(DOF_ID, f.view(), 1.0)
            }
            "internal" => {
                self.assemble_internal_forces(dofs)?;
                let f = self
                    .internal_force
                    .as_ref()
                    .ok_or_else(|| not_allocated("internal_force"))?
                    .read()?;
                dofs.assemble_to_residual(DOF_ID, f.view(), 1.0)
            }
            other => Err(Error::invalid(format!(
                "The residual part {other} is unknown by the solver callback"
            ))),
        }
    }

    fn can_split_residual(&self) -> bool {
        true
    }

    /// Upstream `predictor()`: `++displacement_release`.
    fn predictor(&mut self, _dofs: &mut DofManager) -> Result<()> {
        self.displacement_release += 1;
        Ok(())
    }

    /// Upstream `corrector()`: `++displacement_release`.
    fn corrector(&mut self, _dofs: &mut DofManager) -> Result<()> {
        self.displacement_release += 1;
        Ok(())
    }

    /// Upstream `beforeSolveStep()`: every material's `beforeSolveStep`
    /// (= `savePreviousState`).
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

    /// Upstream `afterSolveStep(converged)`: every material's
    /// `afterSolveStep`.
    fn after_solve_step(&mut self, dofs: &mut DofManager, converged: bool) -> Result<()> {
        let mesh_h = self.base.mesh.clone();
        let mesh = mesh_h.read()?;
        let (laws, services) = self.split(&mesh, dofs);
        let ctx = LawContext {
            phase: LawPhase::AfterSolveStep,
            services: &services,
        };
        for law in laws.laws_mut() {
            law.after_solve_step(&ctx, converged)?;
        }
        Ok(())
    }

    /// Upstream `initSolver(type)` (SMM-003): allocate the model-owned
    /// nodal arrays and `current_position = X`; register the DOF family
    /// `displacement` (values, blocked flags, increment, previous) if
    /// missing, and its two derivatives for dynamic solvers.
    /// `current_position = X` happens only when the cache is allocated here
    /// (UD-352, D-064; upstream on every call:
    /// [`SmmUpstreamCompat::init_solver_resets_current_position`]); the
    /// release is never touched.
    fn init_solver(&mut self, dofs: &mut DofManager, ty: TimeStepSolverType) -> Result<()> {
        let mesh_h = self.base.mesh.clone();
        let mesh = mesh_h.read()?;
        let n = mesh.nb_nodes();
        let d = self.base.spatial_dimension;
        alloc_nodal_shared_field(&mut self.internal_force, n, d)?;
        alloc_nodal_shared_field(&mut self.external_force, n, d)?;
        let x = mesh.nodes();
        if x.nb_component() != d {
            return Err(Error::Unsupported(format!(
                "a {d}-D solid mechanics model on a {}-D mesh",
                x.nb_component()
            )));
        }
        let new_cache = self.current_position.is_none();
        alloc_nodal_shared_field(&mut self.current_position, n, d)?;
        if new_cache || self.upstream_compat.init_solver_resets_current_position {
            let mut cp = self
                .current_position
                .as_mut()
                .ok_or_else(|| not_allocated("current_position"))?
                .write()?;
            cp.copy_from(&x)?;
        }
        if !dofs.has_dofs(DOF_ID) {
            dofs.register_nodal_dofs(&mesh, DOF_ID, d)?;
            dofs.register_blocked_dofs(DOF_ID)?;
            dofs.register_dofs_increment(DOF_ID)?;
            dofs.register_dofs_previous(DOF_ID)?;
        }
        if matches!(
            ty,
            TimeStepSolverType::Dynamic | TimeStepSolverType::DynamicLumped
        ) && !dofs.has_dofs_derivative(DOF_ID, 1)?
        {
            dofs.register_dofs_derivative(DOF_ID, 1)?;
            dofs.register_dofs_derivative(DOF_ID, 2)?;
        }
        Ok(())
    }

    /// Upstream `getDefaultSolverType()`: `dynamic_lumped`.
    fn default_solver_type(&self) -> TimeStepSolverType {
        TimeStepSolverType::DynamicLumped
    }

    /// Upstream `getDefaultSolverOptions(type)` (SMM-002).
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
            TimeStepSolverType::Dynamic => {
                if self.base.method == AnalysisMethod::ExplicitConsistentMass {
                    (
                        NonLinearSolverType::NewtonRaphson,
                        IntegrationSchemeType::CentralDifference,
                        SolutionType::Acceleration,
                    )
                } else {
                    (
                        NonLinearSolverType::NewtonRaphson,
                        IntegrationSchemeType::TrapezoidalRule2,
                        SolutionType::Displacement,
                    )
                }
            }
            // upstream: AKANTU_EXCEPTION(type << " is not a valid time step
            // solver type"); the options stay unset and getNewSolver fails
            TimeStepSolverType::NotDefined => return o,
        };
        o.non_linear_solver_type = Some(nls);
        o.integration_scheme_type.insert(DOF_ID.into(), scheme);
        o.solution_type.insert(DOF_ID.into(), st);
        o
    }

    /// Exposes the physics to intercepting user callbacks
    /// (`Delegate::split_as::<SolidMechanicsPhysics>()`), which upstream
    /// reach through their own reference to the model (e.g. the
    /// traction-at-split-node `NTNContactSolverCallback`).
    fn as_any_mut(&mut self) -> Option<&mut dyn std::any::Any> {
        Some(self)
    }
}

/// Read access to the physics state for intercepting user callbacks that
/// hold the physics through `Delegate::split_as` during a solve step (no
/// upstream counterpart: upstream callbacks call the model's getters).
impl SolidMechanicsPhysics {
    /// Upstream `getMass()` (lumped mass, `nb_nodes × dim`).
    pub fn mass(&self) -> Result<ArrayRef<'_, Real>> {
        self.mass
            .as_ref()
            .ok_or_else(|| not_allocated("mass"))?
            .read()
    }

    /// Upstream `getInternalForce()`.
    pub fn internal_force(&self) -> Result<ArrayRef<'_, Real>> {
        self.internal_force
            .as_ref()
            .ok_or_else(|| not_allocated("internal_force"))?
            .read()
    }

    /// Upstream `getExternalForce()`.
    pub fn external_force(&self) -> Result<ArrayRef<'_, Real>> {
        self.external_force
            .as_ref()
            .ok_or_else(|| not_allocated("external_force"))?
            .read()
    }

    /// Time step of the running solve (upstream `getTimeStep()` of the
    /// default solver, as stored by `solve_step` / `set_time_step`).
    #[must_use]
    pub fn time_step(&self) -> Real {
        self.time_step
    }

    /// Upstream `getDisplacementRelease()`.
    #[must_use]
    pub fn displacement_release(&self) -> i64 {
        self.displacement_release
    }

    /// The current-position cache, writable, its release unchanged
    /// (the split-node contact's `_cf_nodal` exchange writes the
    /// owners' values at the nodes of ghost elements, upstream's
    /// `const_cast` of `getCurrentPosition()` in
    /// `NTNBaseContact::unpackData`; refresh it first with
    /// [`SolidMechanicsPhysics::refresh_current_position`]).
    ///
    /// # Errors
    /// The cache is not allocated (before `init_full`), or it is borrowed.
    pub fn current_position_mut(&mut self) -> Result<ArrayMut<'_, Real>> {
        self.current_position
            .as_mut()
            .ok_or_else(|| not_allocated("current_position"))?
            .write()
    }

    /// The current position as last updated, without refreshing it
    /// (upstream's `current_position` member; [`Self::refresh_current_position`]
    /// refreshes it when the displacement release changed and returns it).
    ///
    /// # Errors
    /// Not allocated (call `init_full` first).
    pub fn cached_current_position(&self) -> Result<ArrayRef<'_, Real>> {
        self.current_position
            .as_ref()
            .ok_or_else(|| not_allocated("current_position"))?
            .read()
    }
}

/// The solid model's capability check before the mesh commit
/// ([`MeshEventHandler::check_mesh_event`]), for every level the model
/// dispatches to: its FE engines (9), DOF manager (20) and non-local manager
/// (100) — forwarded — and its own arrays and laws (94).
/// Node removal is accepted since SMM-015 (refused before,
/// UD-209): the DOF manager renumbers and the
/// model compacts its nodal arrays; the DOF manager's check refuses the node
/// events whose numbering would read stale indices of the final mesh
/// (composition), so such a mutation is rolled back recoverably on every
/// rank. The model's own arrays are validated at dispatch against the
/// numbering, which the mesh transaction validated against the node event
/// stream. An element removal that an internal on its own FE engine or
/// element filter cannot follow is refused here (CL-001,
/// [`akantu_model::constitutive_law::check_elements_removed`]), so that
/// mutation too is rolled back recoverably instead of poisoning the mesh
/// domain at dispatch. The **one** implementation of the check: every handler that
/// forwards the solid model's mesh events — [`SolidMechanicsModel`],
/// [`SmmEventParts`] and the coordinators built on them (the cohesive
/// model's handler) — calls it.
fn smm_check_mesh_event(
    physics: &SolidMechanicsPhysics,
    solver: &ModelSolver,
    mesh: &Mesh,
    event: &MeshEvent,
) -> Result<()> {
    physics.base.fems.check_mesh_event(mesh, event)?;
    solver.dof_manager.check_mesh_event(mesh, event)?;
    // The non-local manager (dispatch level 100): forwarded, per the
    // forwarding obligation (it accepts today).
    if let Some(m) = &physics.non_local {
        m.check_mesh_event(mesh, event)?;
    }
    // CL-001: a law with an internal on its own FE
    // engine or element filter that cannot follow an element removal
    // refuses it here, before the commit (recoverable rollback), not in the
    // dispatch (fatal, a half-updated law)
    // D-105: the Neumann store follows element renumberings and node
    // compactions; an inconsistent one is refused before the commit
    if let Some(store) = &physics.neumann_store {
        match event {
            MeshEvent::ElementsRemoved { new_numbering, .. } => {
                store.check_elements_removed(new_numbering)?;
            }
            MeshEvent::NodesRemoved { new_numbering, .. } => {
                store.check_nodes_removed(new_numbering)?;
            }
            // In the manner of D-111: a boundary row the records
            // cannot follow by node identity (neither a permutation of the
            // nodes holding its loads nor those nodes with copies doubled
            // from them) is refused here, on the staged mesh, before the
            // commit (recoverable, agreed on every rank)
            MeshEvent::NodesAdded {
                nodes, old_nodes, ..
            } => {
                let f = physics.external_force()?;
                let dual = Some(f.as_slice());
                match old_nodes {
                    Some(old) => store.check_nodes_doubled(mesh, dual, nodes, old)?,
                    None => store.check_nodes_doubled(mesh, dual, &[], &[])?,
                }
            }
            // A change's numbering the record cannot
            // follow is refused here (recoverable, agreed on every rank)
            MeshEvent::ElementsChanged {
                old,
                new,
                new_numbering,
                ..
            } => {
                store.check_elements_changed(mesh, old, new, new_numbering)?;
            }
            _ => {}
        }
    }
    if let MeshEvent::ElementsRemoved {
        elements,
        new_numbering,
        ..
    } = event
    {
        for i in 0..physics.materials.nb_laws() {
            akantu_model::constitutive_law::check_elements_removed(
                physics.materials.law(i)?,
                elements,
                new_numbering,
            )?;
        }
    }
    Ok(())
}

/// The parts of a [`SolidMechanicsModel`] that react to mesh events (the
/// physics and the solver), borrowed apart from the model's consumer token
/// so that a coordinator (the cohesive model, a coupler) can pass the token
/// and the handler to `SharedMesh::mutate` together
/// ([`SolidMechanicsModel::split_for_mutation`]).
#[derive(Debug)]
pub struct SmmEventParts<'a> {
    /// Physics.
    pub physics: &'a mut SolidMechanicsPhysics,
    /// Solver (DOF manager).
    pub solver: &'a mut ModelSolver,
}

impl SolidMechanicsModel {
    /// The consumer token and the event-handling parts of the model,
    /// borrowed disjointly (see [`SmmEventParts`]).
    pub fn split_for_mutation(&mut self) -> (&ConsumerToken, SmmEventParts<'_>) {
        (
            &self.token,
            SmmEventParts {
                physics: &mut self.physics,
                solver: &mut self.solver,
            },
        )
    }

    fn parts(&mut self) -> SmmEventParts<'_> {
        SmmEventParts {
            physics: &mut self.physics,
            solver: &mut self.solver,
        }
    }

    /// One staged mutation of the model's mesh with the model as its
    /// consumer (`SharedMesh::mutate` with the model's token and
    /// [`SmmEventParts`]; the model must be the mesh's only consumer —
    /// otherwise use [`SolidMechanicsModel::split_for_mutation`] with the
    /// other consumers). `distributed` is required on a distributed mesh
    /// when events are recorded, e.g.
    /// `akantu_mesh::distributed_removal::DistributedRemovalHook` for
    /// element and node removals.
    ///
    /// Node removal (SMM-015): `MeshTransaction::{remove_nodes, purify}`
    /// renumber the DOFs and compact the model's nodal arrays, as if the
    /// model had been built on the new mesh with its state transferred
    /// (`docs/NUMERICAL_VALIDATION.md`, node removal).
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

impl MeshEventHandler for SolidMechanicsModel {
    fn event_priority(&self) -> EventPriority {
        EventPriority::MODEL
    }

    fn event_priorities(&self) -> Vec<EventPriority> {
        smm_event_priorities()
    }

    fn on_mesh_event(&mut self, mesh: &Mesh, event: &MeshEvent) -> Result<()> {
        self.parts().on_mesh_event(mesh, event)
    }

    /// Forwarded like the dispatch methods: the solid
    /// model's check (`smm_check_mesh_event`).
    fn check_mesh_event(&self, mesh: &Mesh, event: &MeshEvent) -> Result<()> {
        smm_check_mesh_event(&self.physics, &self.solver, mesh, event)
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
        phases: &mut Phases,
    ) -> Result<()> {
        self.parts()
            .on_mesh_event_phased(mesh, event, priority, phases)
    }
}

fn smm_event_priorities() -> Vec<EventPriority> {
    vec![
        EventPriority::FE_ENGINE,
        EventPriority::DOF_MANAGER,
        EventPriority::MODEL,
        EventPriority::NON_LOCAL_MANAGER,
    ]
}

impl MeshEventHandler for SmmEventParts<'_> {
    fn event_priority(&self) -> EventPriority {
        EventPriority::MODEL
    }

    fn event_priorities(&self) -> Vec<EventPriority> {
        smm_event_priorities()
    }

    fn on_mesh_event(&mut self, mesh: &Mesh, event: &MeshEvent) -> Result<()> {
        self.on_mesh_event_at(mesh, event, EventPriority::MODEL)
    }

    /// The solid model's check (`smm_check_mesh_event`).
    fn check_mesh_event(&self, mesh: &Mesh, event: &MeshEvent) -> Result<()> {
        smm_check_mesh_event(self.physics, self.solver, mesh, event)
    }

    /// FE engines (9), DOF manager (20), model arrays + materials (94),
    /// non-local manager (100).
    fn on_mesh_event_at(
        &mut self,
        mesh: &Mesh,
        event: &MeshEvent,
        level: EventPriority,
    ) -> Result<()> {
        match level {
            EventPriority::FE_ENGINE => self.fe_engines_on_event(mesh, event),
            // the DOF manager, and the native solvers' bounds invalidated by
            // a renumbering
            EventPriority::DOF_MANAGER => self.solver.on_dof_mesh_event(mesh, event),
            EventPriority::MODEL => self.model_on_event(mesh, event),
            EventPriority::NON_LOCAL_MANAGER => match &mut self.physics.non_local {
                Some(m) => m.on_mesh_event(mesh, event),
                None => Ok(()),
            },
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
                .on_dof_mesh_event_phased(mesh, event, priority, phases),
            _ => self.on_mesh_event_at(mesh, event, priority),
        }
    }
}

impl SmmEventParts<'_> {
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
            MeshEvent::NodesAdded { .. } => {
                self.physics.on_nodes_added(mesh)?;
                self.physics.neumann_store_on_event(mesh, event)
            }
            // SMM-015 (UD-209): upstream
            // compacts the model's nodal arrays but its DOF manager ignores
            // the removal (`DOFManager::onNodesRemoved` is empty), so the
            // equation numbers go stale. Here the DOF manager renumbered at
            // level 20 (its family arrays — displacement, velocity,
            // acceleration, blocked flags, previous, increment — followed);
            // the model compacts its own arrays, `current_position`
            // included, and reassembles masses and stiffness.
            MeshEvent::NodesRemoved { new_numbering, .. } => {
                self.physics.on_nodes_removed(new_numbering)?;
                self.physics.neumann_store_on_event(mesh, event)
            }
            MeshEvent::ElementsAdded { elements, .. } => {
                self.physics.neumann_store_on_event(mesh, event)?;
                let dim = self.physics.base.spatial_dimension;
                let ghost_laws = std::mem::take(&mut self.physics.staged_ghost_laws);
                let p = &mut self.physics;
                let (laws, services) = p.split(mesh, &self.solver.dof_manager);
                let ctx = LawContext {
                    phase: LawPhase::Init,
                    services: &services,
                };
                laws.on_elements_added(&ctx, mesh, elements, dim, &ghost_laws)
            }
            MeshEvent::ElementsRemoved {
                elements,
                new_numbering,
                ..
            } => {
                self.physics.neumann_store_on_event(mesh, event)?;
                let p = &mut self.physics;
                let (laws, services) = p.split(mesh, &self.solver.dof_manager);
                let ctx = LawContext {
                    phase: LawPhase::Init,
                    services: &services,
                };
                laws.on_elements_removed_all(&ctx, mesh, elements, new_numbering)
            }
            MeshEvent::ElementsChanged { .. } => self.physics.neumann_store_on_event(mesh, event),
            _ => Ok(()),
        }
    }
}

impl Model for SolidMechanicsModel {
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
        SolidMechanicsModel::init_full(self, options)
    }

    fn solve_step(&mut self, solver_id: Option<&str>) -> Result<()> {
        SolidMechanicsModel::solve_step(self, solver_id)
    }

    fn stable_time_step(&mut self) -> Result<Real> {
        SolidMechanicsModel::stable_time_step(self)
    }

    fn dump(&mut self, time: Option<Real>, step: Option<usize>) -> Result<()> {
        SolidMechanicsModel::dump(self, time, step)
    }
}

/// Element-wise material operations of the handler used by the model
/// (upstream `ConstitutiveLawsHandler` members that need the materials'
/// solid interface).
trait LawsHandlerExt {
    /// `(element, old law, new law)` of every not-ghost element whose
    /// selected law differs (upstream `reassignConstitutiveLaw`, first
    /// loop).
    fn reassignment(&self, mesh: &Mesh) -> Result<Vec<(Element, usize, usize)>>;
    /// Second loop of `reassignConstitutiveLaw` (per law:
    /// `removeElements`, then `addElements`).
    fn apply_reassignment(
        &mut self,
        ctx: &LawContext<'_>,
        to_remove: &[Vec<Element>],
        to_add: &[Vec<Element>],
    ) -> Result<()>;
    /// Upstream `onElementsAdded`; `ghost_laws`: the laws of new ghost
    /// elements received from their owners
    /// ([`SolidMechanicsPhysics::staged_ghost_laws`]).
    fn on_elements_added(
        &mut self,
        ctx: &LawContext<'_>,
        mesh: &Mesh,
        elements: &[Element],
        spatial_dimension: usize,
        ghost_laws: &[(Element, usize)],
    ) -> Result<()>;
    /// Upstream `onElementsRemoved`.
    fn on_elements_removed_all(
        &mut self,
        ctx: &LawContext<'_>,
        mesh: &Mesh,
        elements: &[Element],
        new_numbering: &ElementTypeMapArray<Idx>,
    ) -> Result<()>;
}

/// Resize every internal of `law` to its element filter (upstream
/// `resizeInternals`, register-id order). A law with internals registered
/// on another FE engine or element filter (CL-001) is resized with
/// `resize_internals_with` (each internal on its own support); every other
/// law through its `resize_internals`, which laws may override (the IGFEM
/// laws count the points of their sub-elements there).
///
/// The routing exists because `resize_internals_with` does not call
/// `resize_internals`: sending every law through it (the first version of
/// CL-001) bypassed the IGFEM laws' override and sized their sub-element
/// internals with the default engine's point counts (8 IGFEM tests
/// failed). A law that overrides `resize_internals` and holds an internal
/// on its own support must override `resize_internals_with` too (the
/// trait's documentation); no built-in law does.
fn resize_law_internals(law: &mut dyn Material, ctx: &LawContext<'_>) -> Result<()> {
    if akantu_model::constitutive_law::has_internals_on_own_support(law) {
        law.resize_internals_with(&|engine, filter| {
            crate::material::internal_blocks(ctx, engine, filter)
        })
    } else {
        let blocks = law.base().blocks(ctx);
        law.resize_internals(&blocks)
    }
}

/// Resize the handler maps to the mesh's element counts (new entries `-1`;
/// upstream `initialize(mesh, _with_nb_element = true, _default_value = -1)`:
/// the types of the mesh's spatial dimension only).
fn fit_handler_maps(h: &mut ConstitutiveLawsHandler<dyn Material>, mesh: &Mesh) -> Result<()> {
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

impl LawsHandlerExt for ConstitutiveLawsHandler<dyn Material> {
    fn reassignment(&self, mesh: &Mesh) -> Result<Vec<(Element, usize, usize)>> {
        let by_name = |n: &str| self.law_index_of(n).ok();
        let ctx = akantu_model::constitutive_law::SelectorContext {
            mesh,
            law_index: self.law_index_map(),
            law_by_name: &by_name,
        };
        let mut out = Vec::new();
        // upstream `for_each_element(mesh, …)`: elements of the mesh's
        // spatial dimension, both ghost types (none
        // exist: meshes with ghost elements are refused at construction)
        let elements: Vec<Element> = GhostType::ALL
            .into_iter()
            .flat_map(|ghost| {
                mesh.elements(TypeFilter {
                    spatial_dimension: Some(mesh.spatial_dimension()),
                    ghost_type: ghost,
                    kind: None,
                })
            })
            .collect();
        for e in elements {
            let old = self.law_index_map().element_ref(e)?[0];
            let new = self.selector.select(&ctx, e)?;
            if new >= self.nb_laws() {
                return Err(Error::OutOfRange {
                    what: "constitutive law (selector result)",
                    index: new,
                    len: self.nb_laws(),
                });
            }
            match akantu_core::decode_idx(old)? {
                Some(o) if o != new => out.push((e, o, new)),
                Some(_) => {}
                None => {
                    return Err(Error::invalid(format!(
                        "reassignMaterial: {e} has no material (upstream indexes element_to_remove[-1])"
                    )));
                }
            }
        }
        Ok(out)
    }

    fn apply_reassignment(
        &mut self,
        ctx: &LawContext<'_>,
        to_remove: &[Vec<Element>],
        to_add: &[Vec<Element>],
    ) -> Result<()> {
        let nq = |ty: ElementType, g: GhostType| ctx.services.nb_integration_points(ty, g);
        // CL-001: every law that loses elements is
        // checked before any law changes (an internal on its own support
        // that the compaction cannot follow refuses the reassignment, with
        // nothing changed)
        for (i, leaving) in to_remove.iter().enumerate() {
            if !leaving.is_empty() {
                let law = self.law(i)?;
                let plan = plan_removal_from_law(law, leaving)?;
                akantu_model::constitutive_law::check_own_support_compaction(
                    law,
                    &plan.renumbering,
                )?;
            }
        }
        for i in 0..self.nb_laws() {
            if !to_remove[i].is_empty() {
                let kept = {
                    let law = self.law_mut(i)?;
                    remove_elements_from_law(law, &to_remove[i], &nq)?
                };
                self.record_renumbered(i, &kept)?;
            }
            if !to_add[i].is_empty() {
                let mut entries = Vec::with_capacity(to_add[i].len());
                {
                    let law = self.law_mut(i)?;
                    for &e in &to_add[i] {
                        entries.push((e, law.add_element(e)?));
                    }
                    resize_law_internals(law, ctx)?;
                }
                self.record_renumbered(i, &entries)?;
            }
        }
        Ok(())
    }

    fn on_elements_added(
        &mut self,
        ctx: &LawContext<'_>,
        mesh: &Mesh,
        elements: &[Element],
        spatial_dimension: usize,
        ghost_laws: &[(Element, usize)],
    ) -> Result<()> {
        fit_handler_maps(self, mesh)?;
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
        if self.nb_laws() > 0 {
            self.assign_laws_to_elements(mesh, Some(&filter))?;
        }
        // upstream `synchronize(_constitutive_law_id)` at the end of
        // `assignConstitutiveLawToElements`: a new ghost element without a
        // law takes its owner's, appended to the law's ghost filter in
        // unpack order (the list was received before the commit)
        for &(e, law) in ghost_laws {
            if e.ghost_type() != GhostType::Ghost || !filter.exists(e.ty(), e.ghost_type()) {
                return Err(Error::invalid(format!(
                    "staged ghost law for {e}, which is not a new ghost element of the event"
                )));
            }
            if self.law_index_map().element_ref(e)?[0] != -1 {
                continue;
            }
            let local = self.law_mut(law)?.add_element(e)?;
            self.law_index_map_mut()
                .set_element(e, &[akantu_core::try_idx(law)?])?;
            self.local_numbering_map_mut().element_mut(e)?[0] = local;
        }
        for i in 0..self.nb_laws() {
            let law = self.law_mut(i)?;
            if law.base().is_init() {
                resize_law_internals(law, ctx)?;
                // upstream virtual `ConstitutiveLaw::onElementsAdded` past
                // `resizeInternals` (e.g. `cohesive_bilinear`'s strengths)
                law.on_elements_added(ctx, elements)?;
            }
        }
        Ok(())
    }

    fn on_elements_removed_all(
        &mut self,
        ctx: &LawContext<'_>,
        mesh: &Mesh,
        elements: &[Element],
        new_numbering: &ElementTypeMapArray<Idx>,
    ) -> Result<()> {
        let nq = |ty: ElementType, g: GhostType| ctx.services.nb_integration_points(ty, g);
        let mut entries_all = Vec::new();
        for i in 0..self.nb_laws() {
            let entries = {
                let law = self.law_mut(i)?;
                law.on_elements_removed(elements, new_numbering, &nq)?
            };
            entries_all.push((i, entries));
        }
        // the maps of the renumbered blocks follow the mesh's new element
        // counts (upstream keeps the old sizes with stale entries), then the
        // kept elements are written (types of the mesh's spatial dimension).
        // Blocks without a renumbering keep their entries
        // (a removal of ghost elements only, the non-local
        // cleanup, used to reset the owned elements' laws).
        for ghost in GhostType::ALL {
            let types: Vec<ElementType> = mesh
                .element_types(TypeFilter {
                    spatial_dimension: Some(mesh.spatial_dimension()),
                    ghost_type: ghost,
                    kind: None,
                })
                .filter(|&ty| new_numbering.exists(ty, ghost))
                .collect();
            for ty in types {
                // only the blocks the event renumbers (upstream resizes the
                // maps of `new_numbering`'s types only); the others keep
                // their entries
                if !new_numbering.exists(ty, ghost) {
                    continue;
                }
                let n = mesh.nb_element(ty, ghost);
                if let Some(mut a) = if_present(self.law_index_map_mut().get_mut(ty, ghost))? {
                    a.as_mut_slice().fill(-1);
                    a.try_resize(n, -1)?;
                }
                if let Ok(a) = self.local_numbering_map_mut().get_mut(ty, ghost) {
                    a.as_mut_slice().fill(-1);
                    a.try_resize(n, -1)?;
                }
            }
        }
        fit_handler_maps(self, mesh)?;
        for (i, entries) in entries_all {
            self.record_renumbered(i, &entries)?;
        }
        Ok(())
    }
}

/// Upstream `ConstitutiveLaw::removeElements(elements)`: drop the elements
/// from the filter, keep the others in order and compact the internals.
/// Returns `(element, new law-local index)` of every kept element.
///
/// Upstream builds the old-local → new-local renumbering with a counter
/// that only advances on kept elements (`element_ids` is reset whenever no
/// element was kept yet), so a removed element followed by kept ones
/// shifts the renumbering and moves internal values to wrong elements (UD
/// candidate, reported). The port indexes the renumbering by the old
/// law-local position.
fn remove_elements_from_law(
    law: &mut dyn Material,
    elements: &[Element],
    nb_quad: &dyn Fn(ElementType, GhostType) -> usize,
) -> Result<Vec<(Element, Idx)>> {
    // planned and checked before the filter changes
    let plan = plan_removal_from_law(law, elements)?;
    akantu_model::constitutive_law::check_own_support_compaction(law, &plan.renumbering)?;
    for (ty, ghost, f) in plan.new_filters {
        *law.base_mut().element_filter.get_mut(ty, ghost)? = Array::from_vec(f, 1)?;
    }
    law.remove_integration_points(&plan.renumbering, nb_quad)?;
    Ok(plan.kept_entries)
}

/// Pure part of [`remove_elements_from_law`]: the law-local renumbering
/// (old law-local index → new, `-1` leaving), the new filters and the
/// `(element, new law-local index)` of every kept element.
struct LawRemovalPlan {
    renumbering: ElementTypeMapArray<Idx>,
    new_filters: Vec<(ElementType, GhostType, Vec<Idx>)>,
    kept_entries: Vec<(Element, Idx)>,
}

fn plan_removal_from_law(law: &dyn Material, elements: &[Element]) -> Result<LawRemovalPlan> {
    let mut kept_entries = Vec::new();
    // membership in O(1) (upstream's `std::find` per
    // filtered element is quadratic)
    let leaving: std::collections::HashSet<Element> = elements.iter().copied().collect();
    let mut renumbering = ElementTypeMapArray::<Idx>::new("remove constitutive law filter elem");
    let mut new_filters = Vec::new();
    for (ty, ghost, filter) in law.base().element_filter.as_map().iter_all() {
        let mut num = Vec::with_capacity(filter.len());
        let mut new_filter = Vec::with_capacity(filter.len());
        for &g in filter.as_slice() {
            let el = Element::new(ty, g, ghost);
            if leaving.contains(&el) {
                num.push(-1);
                continue;
            }
            let ni = akantu_core::try_idx(new_filter.len())?;
            num.push(ni);
            new_filter.push(g);
            kept_entries.push((el, ni));
        }
        renumbering
            .as_map_mut()
            .insert(ty, ghost, Array::from_vec(num, 1)?)?;
        new_filters.push((ty, ghost, new_filter));
    }
    Ok(LawRemovalPlan {
        renumbering,
        new_filters,
        kept_entries,
    })
}
