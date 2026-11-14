// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/model/common/model_solver.hh,
// model_solver.cc.

//! Model solver (upstream `ModelSolver`, MODEL-glue part): the
//! DOF manager plus named time-step solvers.
//!
//! Upstream `ModelSolver` *is* the model's solver callback and owns the DOF
//! manager; here the model keeps its physics (a [`SolverCallback`]) and its
//! `ModelSolver` in separate fields (D-021), so the methods that need the
//! model's virtuals (`initSolver`, `getMatrixType`, default options) take
//! the callback as an argument:
//!
//! * [`ModelSolver::get_new_solver`] (upstream `getNewSolver`): resolves
//!   `auto` (non-linear solver: Newton–Raphson for `static`/`dynamic`,
//!   lumped for `dynamic_lumped`; sparse solver: MUMPS if registered, else
//!   Eigen), calls the model's `init_solver`, allocates the non-linear and
//!   time-step solvers; the first solver becomes the default;
//! * [`ModelSolver::set_integration_scheme`] creates the needed matrices
//!   with the model's types;
//!
//! **Collective protocol (`docs/ARCHITECTURE.md`).** Solver
//! construction is one [`DofManager::collective`] operation: the local
//! validation, the model's `init_solver` callback, the allocation of the
//! non-linear solver (and its sparse solver) and of the time-step solver
//! are agreed steps, so a failure on one rank is an ordinary error on every
//! rank (the failing rank's own error, "failed on another rank"
//! elsewhere) and no rank registers the solver. The default-solver id is
//! set before anything else, as upstream (UD-201), hence identically on
//! every rank also after an agreed failure. `set_integration_scheme`
//! likewise agrees the scheme's matrix types and the creation of the
//! needed matrices before the scheme is registered on any rank.
//! * [`ModelSolver::init_from_section`] (upstream
//!   `initDOFManager(section, type)`) builds solvers and schemes from the
//!   model's `.dat` section.

use std::collections::BTreeMap;

use akantu_comm::Communicator;
use akantu_core::enums::{
    IntegrationSchemeType, NonLinearSolverType, SparseSolverType, TimeStepSolverType,
};
use akantu_core::{Error, Id, Real, Result};
use akantu_linalg::direct::SparseSolverFactory;
use akantu_parser::param::{ParamInfo, ParamValue, ParameterRegistry, Parsable};
use akantu_parser::{ParserSection, ParserType, SearchScope};

use crate::backend::BackendKind;
use crate::callback::SolverCallback;
use crate::dof_manager::DofManager;
use crate::integration_scheme::{IntegrationScheme, SolutionType};
use crate::non_linear::{NonLinearSolver, new_non_linear_solver};
use crate::time_step::TimeStepSolver;

/// Options of a new solver (upstream `ModelSolverOptions`; `None` is
/// upstream's `_not_defined` / `_auto`).
#[derive(Clone, Debug, Default)]
pub struct ModelSolverOptions {
    /// Time-step solver kind (`None` → `not_defined`).
    pub time_step_solver_type: Option<TimeStepSolverType>,
    /// Non-linear solver kind (`None` → `auto`).
    pub non_linear_solver_type: Option<NonLinearSolverType>,
    /// Sparse direct solver (`None` → `auto`; D-040: `Faer` is the port
    /// extension, selected explicitly only).
    pub sparse_solver_type: Option<SparseSolverType>,
    /// Integration scheme per DOF family.
    pub integration_scheme_type: BTreeMap<Id, IntegrationSchemeType>,
    /// Solution type per DOF family.
    pub solution_type: BTreeMap<Id, SolutionType>,
}

impl ModelSolverOptions {
    fn tss(&self) -> TimeStepSolverType {
        self.time_step_solver_type
            .unwrap_or(TimeStepSolverType::NotDefined)
    }

    fn nls(&self) -> NonLinearSolverType {
        self.non_linear_solver_type
            .unwrap_or(NonLinearSolverType::Auto)
    }

    fn sparse(&self) -> SparseSolverType {
        self.sparse_solver_type.unwrap_or(SparseSolverType::Auto)
    }

    /// Upstream `ModelSolverOptions::update`: unset kinds are taken from
    /// `other`; schemes and solution types of `other` override.
    pub fn update(&mut self, other: &ModelSolverOptions) {
        if self.tss() == TimeStepSolverType::NotDefined {
            self.time_step_solver_type = other.time_step_solver_type;
        }
        if self.nls() == NonLinearSolverType::Auto {
            self.non_linear_solver_type = other.non_linear_solver_type;
        }
        if self.sparse() == SparseSolverType::Auto {
            self.sparse_solver_type = other.sparse_solver_type;
        }
        for (k, v) in &other.integration_scheme_type {
            self.integration_scheme_type.insert(k.clone(), *v);
        }
        for (k, v) in &other.solution_type {
            self.solution_type.insert(k.clone(), *v);
        }
    }
}

/// DOF manager + time-step solvers.
#[derive(Debug)]
pub struct ModelSolver {
    /// DOF manager (upstream "the brain").
    pub dof_manager: DofManager,
    solvers: BTreeMap<Id, TimeStepSolver>,
    default_solver: Option<Id>,
}

/// A non-linear solver or scheme as a [`Parsable`] of a given section type
/// (parameters delegated), to parse `.dat` sections into trait objects.
struct ParseAs<'a, T: ?Sized> {
    inner: &'a mut T,
    section: ParserType,
    id: String,
}

impl<T: ParameterRegistry + ?Sized> ParameterRegistry for ParseAs<'_, T> {
    fn get_param(&self, name: &str) -> Result<ParamValue> {
        self.inner.get_param(name)
    }
    fn set_param(&mut self, name: &str, value: ParamValue) -> Result<()> {
        self.inner.set_param(name, value)
    }
    fn param_names(&self) -> Vec<&'static str> {
        self.inner.param_names()
    }
    fn update_internal_parameters(&mut self) -> Result<()> {
        self.inner.update_internal_parameters()
    }
    fn param_info(&self, name: &str) -> Option<ParamInfo> {
        self.inner.param_info(name)
    }
    fn set_param_from_input(&mut self, name: &str, value: ParamValue) -> Result<()> {
        self.inner.set_param_from_input(name, value)
    }
}

impl<T: ParameterRegistry + ?Sized> Parsable for ParseAs<'_, T> {
    fn section_type(&self) -> ParserType {
        self.section
    }
    fn parsable_id(&self) -> &str {
        &self.id
    }
}

fn word(section: &ParserSection, name: &str) -> Result<Option<String>> {
    match section.get_parameter(name, SearchScope::Current) {
        Ok(p) => {
            let w = p.to_word();
            String::from_utf8(w.as_bytes().to_vec())
                .map(Some)
                .map_err(|_| Error::invalid(format!("parameter '{name}' is not valid UTF-8")))
        }
        Err(_) => Ok(None),
    }
}

/// Upstream `ModelSolver::initDOFManager(dof_manager_type)`
/// (`model_solver.cc:98-121`): the DOF manager is allocated by name from
/// the `DOFManagerFactory`, which knows `default` and — in a build with the
/// opt-in `petsc` feature of this crate, as upstream with `AKANTU_USE_PETSC`
/// (`dof_manager_petsc.cc` registers `"petsc"`) — `petsc`. Any other name
/// fails with upstream's message including the factory's text
/// (`aka_factory.hh:63-67`); `petsc` without the feature fails the same way
/// (an upstream build without PETSc) with a hint naming the feature.
///
/// **Licence:** the `petsc` feature links PETSc and its GPL-licensed
/// dependencies; a build with it is distributed under GPL-3.0-or-later as a
/// whole (D-076 option A, `NOTICE.md`).
pub fn check_dof_manager_type(dof_manager_type: &str) -> Result<()> {
    if dof_manager_type == "default" || (cfg!(feature = "petsc") && dof_manager_type == "petsc") {
        return Ok(());
    }
    let hint = if dof_manager_type == "petsc" {
        " (the PETSc DOF manager needs the opt-in `petsc` feature of akantu-solver, or of the \
         model crate; a build with it is distributed under GPL-3.0-or-later as a whole, D-076)"
    } else {
        ""
    };
    Err(Error::invalid(format!(
        "To use the solver {dof_manager_type} you will have to code it. This is an unknown \
         solver type.The id \"{dof_manager_type}\" is not registered in the \
         akantu::DOFManager factory.{hint}"
    )))
}

/// The communicator of a model's DOF manager of type `dof_manager_type`
/// whose mesh lives on `mesh_communicator` (the serial-mesh rule of the
/// PETSc DOF manager).
///
/// Upstream builds every DOF manager on the mesh's communicator, which in an
/// MPI build is the static communicator (`MPI_COMM_WORLD`, MPI initialised by
/// `akantu::initialize`) also for a mesh read on one process, and
/// `DOFManagerPETSc::init` hands its MPI handle to PETSc
/// (`dof_manager_petsc.cc:146-157`; `PETSC_COMM_SELF` only in a build
/// without MPI). The port gives a serial mesh the serial communicator, which
/// has no MPI handle (`PetscBackend::new` refuses it). Rule:
///
/// 1. `petsc` on a mesh whose communicator is MPI keeps that communicator;
///    so does every other type.
/// 2. `petsc` on a serial communicator of one rank runs on the MPI world,
///    and the job must have exactly one process. The number of processes is
///    a local query (`akantu_comm::world_size`; MPI is initialised first
///    if needed, as upstream's `akantu::initialize` does on every process),
///    decided **before any collective call**: on more processes the request
///    is refused with `Error::Unsupported` (every process would own its
///    whole mesh on `MPI_COMM_WORLD`; distribute the mesh instead), an
///    ordinary, recoverable error — nothing was communicated, so it also
///    holds if only some processes make the request. **Thread level**:
///    MPI not yet initialised is initialised here at
///    `MpiOptions::default()`'s level, `MPI_THREAD_FUNNELED` (the level of
///    the facade's `runtime::initialize` too); a later
///    `Communicator::mpi_world` of that process asking for `Serialized` or
///    `Multiple` then fails ("MPI provides Funneled, … required"). An
///    environment already initialised is used at its own level, so a
///    program that needs a higher level initialises MPI with it (e.g.
///    `Communicator::mpi_world` with that `thread_level`) **before** the
///    first PETSc DOF manager of a serial mesh.
/// 3. The world of (2) is **process-wide**: one duplicate of
///    `MPI_COMM_WORLD`, shared by every such DOF manager and kept until
///    [`release_petsc_world`] (called by the facade's `runtime::finalize`)
///    or process exit (the exit hook of `akantu-comm`,
///    `akantu_comm::release_at_exit`), never released by dropping a model —
///    so models created one after another work as upstream's (MPI lives
///    until `akantu::finalize`), and PETSc, whose session is tied to the
///    communicator of its first user, is not finalised and re-initialised
///    between them. After a refusal the process keeps the environment guard
///    of the query instead (no duplicate exists), so MPI stays usable
///    whoever initialised it.
/// 4. An in-process world of several ranks (`Communicator::local_world`, no
///    MPI handle) is refused.
pub fn dof_manager_communicator(
    dof_manager_type: &str,
    mesh_communicator: &Communicator,
) -> Result<Communicator> {
    if dof_manager_type != "petsc" || mesh_communicator.is_mpi() {
        return Ok(mesh_communicator.clone());
    }
    if mesh_communicator.size() != 1 {
        return Err(Error::Unsupported(format!(
            "the PETSc DOF manager needs an MPI communicator; the mesh lives on an in-process \
             world of {} ranks without an MPI handle",
            mesh_communicator.size()
        )));
    }
    serial_mesh_world::world()
}

/// Release the process-wide MPI world of the PETSc DOF managers of serial
/// meshes (rule (3) of [`dof_manager_communicator`]), or the environment
/// guard kept after a refusal: the facade's `runtime::finalize` calls it
/// before completing the MPI lifecycle, and the exit hook registered with
/// the first such world does the same at process exit. Models still holding
/// the world keep MPI alive until they are dropped. Never blocks: `false`
/// if another thread is creating such a world right now (nothing
/// released). Without the `petsc` feature there is nothing to release.
pub fn release_petsc_world() -> bool {
    serial_mesh_world::release()
}

/// Test-only hook: while the returned guard lives,
/// [`release_petsc_world`] returns `false`, as when another thread is
/// creating the process-wide world, so that a caller's handling of that
/// case (the facade's `runtime::finalize`) can be tested without a race.
/// With the `petsc` feature the guard holds the world's own lock (the real
/// condition: `release` only tries it); without it there is no world and
/// the guard counts as a holder. Not part of the API.
#[doc(hidden)]
#[must_use]
pub fn hold_petsc_world_for_test() -> PetscWorldHold {
    serial_mesh_world::hold()
}

pub use serial_mesh_world::PetscWorldHold;

#[cfg(feature = "petsc")]
mod serial_mesh_world {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Mutex, PoisonError, TryLockError};

    use akantu_comm::lifecycle::MpiEnvironment;
    use akantu_comm::{Communicator, MpiOptions, RsmpiRuntime};
    use akantu_core::{Error, Result};

    /// What the process keeps after the first request.
    enum Held {
        /// One process: the MPI world (a duplicate of `MPI_COMM_WORLD`).
        World(Communicator),
        /// Several processes: the refusal's environment guard (MPI stays
        /// initialised; no communicator was created) and the job size.
        Refused {
            _env: MpiEnvironment<'static, RsmpiRuntime>,
            size: usize,
        },
    }

    /// The process-wide state (leaf lock: never held across a wait for
    /// another lock; MPI calls under it are local or, for one process,
    /// trivially collective).
    static HELD: Mutex<Option<Held>> = Mutex::new(None);

    /// [`release`] is registered with the exit hook of `akantu-comm`.
    static EXIT_RELEASE_REGISTERED: AtomicBool = AtomicBool::new(false);

    fn refusal(size: usize) -> Error {
        Error::Unsupported(format!(
            "the PETSc DOF manager of a model on a serial (not distributed) mesh runs on \
             MPI_COMM_WORLD, which has {size} processes here: every process would own its whole \
             mesh; distribute the mesh (akantu_mesh::distribution) or run one process"
        ))
    }

    /// The exit hook's callback: never blocks (exit mode, D-067 (3)).
    fn release_at_exit() -> bool {
        super::release_petsc_world()
    }

    pub(super) fn world() -> Result<Communicator> {
        let mut held = HELD.lock().unwrap_or_else(PoisonError::into_inner);
        match &*held {
            Some(Held::World(w)) => return Ok(w.clone()),
            Some(Held::Refused { size, .. }) => return Err(refusal(*size)),
            None => {}
        }
        // Registered before anything is kept, so that whatever is kept is
        // released at exit (a refused registration is retried next time).
        if !EXIT_RELEASE_REGISTERED.load(Ordering::Acquire) {
            akantu_comm::release_at_exit(release_at_exit)?;
            EXIT_RELEASE_REGISTERED.store(true, Ordering::Release);
        }
        let options = MpiOptions::default();
        // A local query, before any collective call (rule (2)).
        let (env, size) = akantu_comm::world_size(options.thread_level)?;
        if size != 1 {
            *held = Some(Held::Refused { _env: env, size });
            return Err(refusal(size));
        }
        // One process: the duplication involves no other process.
        let world = Communicator::mpi_world(options)?;
        drop(env);
        *held = Some(Held::World(world.clone()));
        Ok(world)
    }

    /// [`super::hold_petsc_world_for_test`]: the world's lock, held (the
    /// guard is never read: holding the lock is its purpose).
    #[doc(hidden)]
    pub struct PetscWorldHold {
        _lock: std::sync::MutexGuard<'static, Option<Held>>,
    }

    impl std::fmt::Debug for PetscWorldHold {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("PetscWorldHold")
        }
    }

    pub(super) fn hold() -> PetscWorldHold {
        PetscWorldHold {
            _lock: HELD.lock().unwrap_or_else(PoisonError::into_inner),
        }
    }

    pub(super) fn release() -> bool {
        let taken = match HELD.try_lock() {
            Ok(mut g) => g.take(),
            Err(TryLockError::Poisoned(p)) => p.into_inner().take(),
            Err(TryLockError::WouldBlock) => return false,
        };
        // Dropped outside the lock: the world's destruction (and PETSc's
        // finalisation, if this was its session's communicator) is queued
        // through the MPI lifecycle.
        drop(taken);
        true
    }
}

#[cfg(not(feature = "petsc"))]
mod serial_mesh_world {
    use akantu_comm::Communicator;
    use akantu_core::{Error, Result};

    pub(super) fn world() -> Result<Communicator> {
        Err(Error::Unsupported(
            "the PETSc DOF manager needs the opt-in `petsc` feature (a build with it is \
             distributed under GPL-3.0-or-later as a whole, D-076)"
                .into(),
        ))
    }

    use std::sync::atomic::{AtomicUsize, Ordering};

    /// Live [`PetscWorldHold`]s (test-only hook; no world exists here).
    static HOLDS: AtomicUsize = AtomicUsize::new(0);

    /// [`super::hold_petsc_world_for_test`] without the `petsc` feature.
    #[doc(hidden)]
    #[derive(Debug)]
    pub struct PetscWorldHold(());

    impl Drop for PetscWorldHold {
        fn drop(&mut self) {
            HOLDS.fetch_sub(1, Ordering::AcqRel);
        }
    }

    pub(super) fn hold() -> PetscWorldHold {
        HOLDS.fetch_add(1, Ordering::AcqRel);
        PetscWorldHold(())
    }

    pub(super) fn release() -> bool {
        HOLDS.load(Ordering::Acquire) == 0
    }
}

/// Upstream `ModelSolver::initDOFManager(dof_manager_type)`
/// (`model_solver.cc:98-121`) for a model `model_id` on a mesh living on
/// `mesh_communicator`: the type checked against the factory
/// ([`check_dof_manager_type`], upstream's message), the communicator
/// chosen by [`dof_manager_communicator`], then
/// [`DofManager::of_type`] with upstream's id
/// `<model_id>:dof_manager_<type>`. `petsc` is collective over the job's
/// processes (PETSc session, `PetscBackend::new`).
pub fn new_dof_manager(
    dof_manager_type: &str,
    model_id: &str,
    mesh_communicator: &Communicator,
) -> Result<DofManager> {
    check_dof_manager_type(dof_manager_type)?;
    let comm = dof_manager_communicator(dof_manager_type, mesh_communicator)?;
    DofManager::of_type(
        dof_manager_type,
        format!("{model_id}:dof_manager_{dof_manager_type}"),
        comm,
    )
}

impl ModelSolver {
    /// New solver around a DOF manager.
    #[must_use]
    pub fn new(dof_manager: DofManager) -> Self {
        Self {
            dof_manager,
            solvers: BTreeMap::new(),
            default_solver: None,
        }
    }

    /// Upstream `ModelSolver::initDOFManager(dof_manager_type)`
    /// (`model_solver.cc:98-121`): a new DOF manager of that type replaces
    /// the current one ([`new_dof_manager`]: id
    /// `<model_id>:dof_manager_<type>`, communicator rule of
    /// [`dof_manager_communicator`]); the upstream-compatibility switches of
    /// the old manager are kept. Upstream's tests call it right after
    /// constructing the model, before `initFull`
    /// (`test_phase_field_tao.cc:53`, `patch_test_linear_fixture.hh:61`).
    ///
    /// # Errors
    /// An unknown type (upstream's message); `petsc` without the feature or
    /// on a communicator the PETSc back-end cannot use; **refused** (UD-1020,
    /// D-003 A) once a time-step solver exists or DOF families are
    /// registered: upstream drops the old manager with its solvers and
    /// registered DOFs but keeps the default solver id, so the model's next
    /// `solveStep` fails on a solver that no longer exists (or the model's
    /// arrays are no longer known to the manager).
    pub fn init_dof_manager(
        &mut self,
        dof_manager_type: &str,
        model_id: &str,
        mesh_communicator: &Communicator,
    ) -> Result<()> {
        if !self.solvers.is_empty() || !self.dof_manager.dof_ids().is_empty() {
            return Err(Error::invalid(format!(
                "init_dof_manager({dof_manager_type}) must be called before any time step solver \
                 or DOF family exists (here: {} solver(s), {} DOF famil(y/ies)); upstream drops \
                 them with the old DOF manager and keeps the default solver id (UD-1020): call it \
                 right after constructing the model, before init_full",
                self.solvers.len(),
                self.dof_manager.dof_ids().len()
            )));
        }
        let mut dm = new_dof_manager(dof_manager_type, model_id, mesh_communicator)?;
        dm.set_upstream_compat(self.dof_manager.upstream_compat());
        self.dof_manager = dm;
        Ok(())
    }

    /// Instantiate a time-step solver without calling the model's
    /// `init_solver` (see [`ModelSolver::get_new_solver`]).
    pub fn new_solver(&mut self, solver_id: &str, options: &ModelSolverOptions) -> Result<()> {
        self.new_solver_impl(solver_id, options.clone(), None)
    }

    /// Upstream `getNewSolver(solver_id, options)`: resolve `auto`, call
    /// the model's `init_solver(type)`, allocate the non-linear solver
    /// (default DOF manager factory) and the time-step solver. The first
    /// solver id becomes the default (set before validation, as upstream).
    pub fn get_new_solver(
        &mut self,
        solver_id: &str,
        options: &ModelSolverOptions,
        model: &mut dyn SolverCallback,
    ) -> Result<()> {
        self.new_solver_impl(solver_id, options.clone(), Some(model))
    }

    fn new_solver_impl(
        &mut self,
        solver_id: &str,
        mut options: ModelSolverOptions,
        model: Option<&mut dyn SolverCallback>,
    ) -> Result<()> {
        // Upstream quirk (UD-201): the id becomes the default before any
        // validation; identical on every rank (nothing rank-specific ran).
        if self.default_solver.is_none() {
            self.default_solver = Some(solver_id.into());
        }
        let validation = (|| -> Result<()> {
            if self.solvers.contains_key(solver_id) {
                return Err(Error::AlreadyExists {
                    kind: "time step solver",
                    id: solver_id.into(),
                });
            }
            Ok(())
        })();
        let tss = options.tss();
        let nls_type = if options.nls() == NonLinearSolverType::Auto {
            match tss {
                TimeStepSolverType::Dynamic | TimeStepSolverType::Static => {
                    Ok(NonLinearSolverType::NewtonRaphson)
                }
                TimeStepSolverType::DynamicLumped => Ok(NonLinearSolverType::Lumped),
                TimeStepSolverType::NotDefined => Err(Error::invalid(format!(
                    "{tss} is not a valid time step solver type"
                ))),
            }
        } else {
            Ok(options.nls())
        };
        if options.sparse() == SparseSolverType::Auto {
            options.sparse_solver_type = Some(match self.dof_manager.backend().kind() {
                // D-040: MUMPS when registered and the manager
                // communicates over MPI, else Eigen.
                BackendKind::Aij => SparseSolverFactory::default().resolve_available(
                    SparseSolverType::Auto,
                    self.dof_manager.communicator().is_mpi(),
                ),
                BackendKind::Petsc => SparseSolverType::Petsc,
                BackendKind::Other(_) => SparseSolverType::Auto,
            });
        }
        let sparse = options.sparse();
        let name = format!("get_new_solver:{solver_id}");
        let tss_solver = self.dof_manager.collective(&name, |dofs| {
            dofs.step("validate", |_| validation)?;
            let nls_type = dofs.step("resolve_non_linear_solver_type", |_| nls_type)?;
            if let Some(model) = model {
                dofs.step("init_solver", |d| model.init_solver(d, tss))?;
            }
            let nls = dofs.step("new_non_linear_solver", |d| {
                new_non_linear_solver(d, solver_id, nls_type, sparse)
            })?;
            dofs.step("new_time_step_solver", |_| {
                TimeStepSolver::new(solver_id, tss, nls)
            })
        })?;
        self.solvers.insert(solver_id.into(), tss_solver);
        Ok(())
    }

    /// Install a fully constructed time-step solver under `solver_id`
    /// (couplers/tests); the first solver becomes the default.
    pub fn insert_solver(&mut self, solver_id: &str, solver: TimeStepSolver) -> Result<()> {
        if self.solvers.contains_key(solver_id) {
            return Err(Error::AlreadyExists {
                kind: "time step solver",
                id: solver_id.into(),
            });
        }
        self.solvers.insert(solver_id.into(), solver);
        if self.default_solver.is_none() {
            self.default_solver = Some(solver_id.into());
        }
        Ok(())
    }

    /// Upstream `setIntegrationScheme(solver_id, dof_id, type,
    /// solution_type)` (the solver id is not defaulted, as upstream).
    pub fn set_integration_scheme(
        &mut self,
        solver_id: &str,
        dof_id: &str,
        ty: IntegrationSchemeType,
        solution_type: SolutionType,
        model: &dyn SolverCallback,
    ) -> Result<()> {
        let tss = self
            .solvers
            .get_mut(solver_id)
            .ok_or_else(|| Error::not_found("time step solver", solver_id))?;
        tss.set_integration_scheme(&mut self.dof_manager, dof_id, ty, solution_type, model)
    }

    /// Upstream `setIntegrationScheme(solver_id, dof_id, scheme,
    /// solution_type)` with an externally built scheme.
    pub fn set_integration_scheme_instance(
        &mut self,
        solver_id: &str,
        dof_id: &str,
        scheme: Box<dyn IntegrationScheme>,
        solution_type: SolutionType,
        model: &dyn SolverCallback,
    ) -> Result<()> {
        let tss = self
            .solvers
            .get_mut(solver_id)
            .ok_or_else(|| Error::not_found("time step solver", solver_id))?;
        tss.set_integration_scheme_instance(
            &mut self.dof_manager,
            dof_id,
            scheme,
            solution_type,
            model,
        )
    }

    /// Upstream `hasIntegrationScheme(solver_id, dof_id)`.
    pub fn has_integration_scheme(&self, solver_id: &str, dof_id: &str) -> Result<bool> {
        Ok(self
            .solvers
            .get(solver_id)
            .ok_or_else(|| Error::not_found("time step solver", solver_id))?
            .has_integration_scheme(dof_id))
    }

    /// Mutable solver by id (`None` = default), e.g. to change its time step.
    pub fn solver_mut(&mut self, solver_id: Option<&str>) -> Result<&mut TimeStepSolver> {
        let id = self.resolve(solver_id)?;
        self.solvers
            .get_mut(&id)
            .ok_or_else(|| Error::not_found("time step solver", id))
    }

    /// Non-linear solver of a time-step solver (upstream
    /// `getNonLinearSolver`).
    pub fn non_linear_solver(&self, solver_id: Option<&str>) -> Result<&dyn NonLinearSolver> {
        Ok(self.solver(solver_id)?.non_linear_solver.as_ref())
    }

    /// Mutable non-linear solver (parameters, UD-010 hook mode).
    pub fn non_linear_solver_mut(
        &mut self,
        solver_id: Option<&str>,
    ) -> Result<&mut dyn NonLinearSolver> {
        Ok(self.solver_mut(solver_id)?.non_linear_solver.as_mut())
    }

    /// The DOF manager's reaction to a committed mesh event (dispatch level
    /// 20), forwarded by the models, plus the solver state named by local
    /// equations: a node removal renumbers the equations (SMM-015), so the
    /// variable bounds of every native (`petsc_tao`) solver are invalidated
    /// ([`crate::NativeNonLinear::invalidate_bounds`]):
    /// the next bounded solve is refused until they are set again.
    ///
    /// # Errors
    /// The DOF manager's.
    pub fn on_dof_mesh_event(
        &mut self,
        mesh: &akantu_mesh::Mesh,
        event: &akantu_mesh::MeshEvent,
    ) -> Result<()> {
        use akantu_mesh::MeshEventHandler;
        let r = self.dof_manager.on_mesh_event(mesh, event);
        self.after_dof_mesh_event(event);
        r
    }

    /// [`ModelSolver::on_dof_mesh_event`] as one step of a mesh mutation's
    /// phase sequence (the DOF manager's `on_mesh_event_phased`).
    ///
    /// # Errors
    /// The DOF manager's.
    pub fn on_dof_mesh_event_phased(
        &mut self,
        mesh: &akantu_mesh::Mesh,
        event: &akantu_mesh::MeshEvent,
        priority: akantu_core::enums::EventPriority,
        phases: &mut akantu_comm::phase::Phases,
    ) -> Result<()> {
        use akantu_mesh::MeshEventHandler;
        let r = self
            .dof_manager
            .on_mesh_event_phased(mesh, event, priority, phases);
        self.after_dof_mesh_event(event);
        r
    }

    /// Bounds over local equations do not survive a renumbering.
    fn after_dof_mesh_event(&mut self, event: &akantu_mesh::MeshEvent) {
        if matches!(event, akantu_mesh::MeshEvent::NodesRemoved { .. }) {
            for s in self.solvers.values_mut() {
                if let Some(n) = s.non_linear_solver.as_native_mut() {
                    n.invalidate_bounds("a node removal renumbered the local equations (SMM-015)");
                }
            }
        }
    }

    /// Solve one step with a user callback intercepting the model callback
    /// (upstream `solveStep(InterceptSolverCallback &, id)`, Python
    /// `model.solveStep(callback)`): the user's methods receive a
    /// [`crate::callback::Delegate`] to the model callback (their
    /// `super()`), valid only during this call.
    pub fn solve_step_intercepted(
        &mut self,
        model_callback: &mut dyn SolverCallback,
        user: &mut dyn crate::callback::InterceptCallback,
        solver_id: Option<&str>,
    ) -> Result<()> {
        let mut cb = crate::callback::Intercept::new(user, model_callback);
        self.solve_step(&mut cb, solver_id)
    }

    /// Whether a solver exists (upstream `hasSolver`; the empty id means
    /// the default solver).
    #[must_use]
    pub fn has_solver(&self, solver_id: &str) -> bool {
        match self.resolve(Some(solver_id)) {
            Ok(id) => self.solvers.contains_key(&id),
            Err(_) => false,
        }
    }

    /// Whether a default solver is set (upstream `hasDefaultSolver`).
    #[must_use]
    pub fn has_default_solver(&self) -> bool {
        self.default_solver.is_some()
    }

    /// Change the default solver (upstream `setDefaultSolver`).
    pub fn set_default_solver(&mut self, solver_id: &str) -> Result<()> {
        if !self.solvers.contains_key(solver_id) {
            return Err(Error::not_found("time step solver", solver_id));
        }
        self.default_solver = Some(solver_id.into());
        Ok(())
    }

    /// Time step of a solver (upstream `getTimeStep`).
    pub fn time_step(&self, solver_id: Option<&str>) -> Result<Real> {
        Ok(self.solver(solver_id)?.time_step)
    }

    /// Set the time step of a solver (upstream `setTimeStep`).
    pub fn set_time_step(&mut self, time_step: Real, solver_id: Option<&str>) -> Result<()> {
        self.solver_mut(solver_id)?.time_step = time_step;
        Ok(())
    }

    /// Solver by id (`None` = default).
    pub fn solver(&self, solver_id: Option<&str>) -> Result<&TimeStepSolver> {
        let id = self.resolve(solver_id)?;
        self.solvers
            .get(&id)
            .ok_or_else(|| Error::not_found("time step solver", id))
    }

    /// Solve one step with `callback` (upstream `solveStep`): a disjoint
    /// borrow of the DOF manager and the time-step solver.
    pub fn solve_step(
        &mut self,
        callback: &mut dyn SolverCallback,
        solver_id: Option<&str>,
    ) -> Result<()> {
        let id = self.resolve(solver_id)?;
        let tss = self
            .solvers
            .get_mut(&id)
            .ok_or_else(|| Error::not_found("time step solver", id.clone()))?;
        tss.solve_step(&mut self.dof_manager, callback)
    }

    /// Upstream `initDOFManager(section, dof_manager_type)` (after the DOF
    /// manager exists): every `time_step_solver` sub-section creates a
    /// solver (`name` defaults to the section's type) from the model's
    /// default options for that type, overridden by `sparse_solver_type`,
    /// at most one `non_linear_solver` sub-section (its name is the type;
    /// its parameters are parsed into the solver) and the
    /// `integration_scheme` sub-sections (name = scheme type, parameter
    /// `name` = DOF id, optional `solution_type`); the model's default
    /// schemes complete the missing families; `default_solver` selects the
    /// default solver.
    ///
    /// `permissive` is upstream's global `Parser::isPermissive()` (the
    /// `permissive_parser` flag of the parsed input files: pass the model's
    /// [`akantu_parser::Parser::is_permissive`]). Upstream's
    /// `Parsable::parseParam` (`parsable.cc:49-62`) consults it for every
    /// parameter of the `non_linear_solver` sub-section, the only solver
    /// sub-section parsed as a `Parsable` (`model_solver.cc:171`): an unknown
    /// name is then a warning (`log::warn!`), not an error.
    pub fn init_from_section(
        &mut self,
        section: &ParserSection,
        model: &mut dyn SolverCallback,
        permissive: bool,
    ) -> Result<()> {
        for ts in section.sub_sections_of(ParserType::TimeStepSolver) {
            let ty_name = ts.name.clone();
            let solver_id = word(ts, "name")?.unwrap_or_else(|| ty_name.clone());
            let tss_type: TimeStepSolverType = ty_name.parse()?;
            let mut options = model.default_solver_options(tss_type);
            if options.tss() == TimeStepSolverType::NotDefined {
                options.time_step_solver_type = Some(tss_type);
            }
            let mut nls_type = options.nls();
            let sparse_default = options.sparse().to_string();
            let ss_word = word(ts, "sparse_solver_type")?.unwrap_or(sparse_default);
            // Upstream values `auto`, `mumps`, `eigen`, `petsc` plus the
            // documented port extension `faer` (D-040), parsed exactly as
            // upstream's `getOptionToType` (`operator>>`): `_mumps` is
            // rejected; the default is `std::to_string` of the
            // model's choice, a bare name.
            let ss_type: SparseSolverType = ss_word.parse()?;
            let nls_sections: Vec<&ParserSection> =
                ts.sub_sections_of(ParserType::NonLinearSolver).collect();
            match nls_sections.len() {
                0 => {}
                1 => nls_type = nls_sections[0].name.parse()?,
                _ => {
                    return Err(Error::invalid(format!(
                        "More than one non linear solver are provided for the time step solver {solver_id}"
                    )));
                }
            }
            options.non_linear_solver_type = Some(nls_type);
            options.sparse_solver_type = Some(ss_type);
            self.get_new_solver(&solver_id, &options, model)?;
            if let Some(nls_section) = nls_sections.first() {
                let nls = self.non_linear_solver_mut(Some(&solver_id))?;
                // PETSc solvers turn every parameter into a PETSc option
                // (upstream `ParsablePETSc::parseSection`).
                match nls.parse_options_section(nls_section) {
                    Some(r) => r?,
                    None => ParseAs {
                        inner: nls,
                        section: ParserType::NonLinearSolver,
                        id: solver_id.clone(),
                    }
                    .parse_section(nls_section, permissive)?,
                }
            }
            for is in ts.sub_sections_of(ParserType::IntegrationScheme) {
                let dof_id = word(is, "name")?.ok_or_else(|| {
                    Error::invalid(format!(
                        "No degree of freedom name specified for the integration scheme of type {}",
                        is.name
                    ))
                })?;
                let it_type: IntegrationSchemeType = is.name.parse()?;
                // Upstream `solver_options.solution_type[dof_id]`:
                // `std::map::operator[]` value-initialises the enumeration
                // to 0 = `_displacement` (oracle-checked).
                let default_st = *options
                    .solution_type
                    .entry(dof_id.clone())
                    .or_insert(SolutionType::Displacement);
                let st = match word(is, "solution_type")? {
                    Some(w) => w.parse()?,
                    None => default_st,
                };
                self.set_integration_scheme(&solver_id, &dof_id, it_type, st, model)?;
            }
            let defaults: Vec<(Id, IntegrationSchemeType)> = options
                .integration_scheme_type
                .iter()
                .map(|(k, v)| (k.clone(), *v))
                .collect();
            for (dof_id, it_type) in defaults {
                if !self.has_integration_scheme(&solver_id, &dof_id)? {
                    let st = *options
                        .solution_type
                        .entry(dof_id.clone())
                        .or_insert(SolutionType::Displacement);
                    self.set_integration_scheme(&solver_id, &dof_id, it_type, st, model)?;
                }
            }
        }
        if let Some(default) = word(section, "default_solver")? {
            if self.solvers.contains_key(&default) {
                self.set_default_solver(&default)?;
            } else {
                return Err(Error::invalid(format!(
                    "The solver \"{default}\" was not created, it cannot be set as default solver"
                )));
            }
        }
        Ok(())
    }

    fn resolve(&self, solver_id: Option<&str>) -> Result<Id> {
        match solver_id {
            Some(s) if !s.is_empty() => Ok(s.to_owned()),
            _ => self
                .default_solver
                .clone()
                .ok_or_else(|| Error::not_found("default time step solver", "")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use akantu_comm::Communicator;

    struct Nothing;
    impl SolverCallback for Nothing {}

    #[test]
    fn missing_solvers_are_errors() {
        let mut ms = ModelSolver::new(DofManager::new("dm", Communicator::serial()));
        assert!(!ms.has_solver("static"));
        assert!(ms.set_default_solver("static").is_err());
        assert!(ms.time_step(None).is_err());
        assert!(ms.solve_step(&mut Nothing, None).is_err());
        assert!(ms.solve_step(&mut Nothing, Some("dynamic")).is_err());
        // Default callback behaviour mirrors upstream AKANTU_TO_IMPLEMENT.
        assert!(Nothing.assemble_residual(&mut ms.dof_manager).is_err());
    }

    #[test]
    fn auto_resolution_follows_the_time_step_solver_type() {
        let mut ms = ModelSolver::new(DofManager::new("dm", Communicator::serial()));
        let opts = |t| ModelSolverOptions {
            time_step_solver_type: Some(t),
            ..ModelSolverOptions::default()
        };
        ms.new_solver("s", &opts(TimeStepSolverType::Static))
            .unwrap();
        ms.new_solver("e", &opts(TimeStepSolverType::DynamicLumped))
            .unwrap();
        assert_eq!(
            ms.non_linear_solver(Some("s")).unwrap().kind(),
            NonLinearSolverType::NewtonRaphson
        );
        assert_eq!(
            ms.non_linear_solver(Some("e")).unwrap().kind(),
            NonLinearSolverType::Lumped
        );
        assert!(
            ms.new_solver("x", &opts(TimeStepSolverType::NotDefined))
                .is_err()
        );
        assert!(
            ms.new_solver("s", &opts(TimeStepSolverType::Static))
                .is_err()
        );
        assert_eq!(ms.solver(None).unwrap().id, "s");
        let petsc = ModelSolverOptions {
            non_linear_solver_type: Some(NonLinearSolverType::PetscSnes),
            ..opts(TimeStepSolverType::Static)
        };
        assert!(ms.new_solver("p", &petsc).is_err());
    }
}
