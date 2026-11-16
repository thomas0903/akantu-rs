// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/model/common/dof_manager/
// {dof_manager_default.*, dof_manager_petsc.*}, src/solver/
// {sparse_matrix_aij.*, sparse_matrix_petsc.*, solver_vector_petsc.*,
// sparse_solver_mumps.*, sparse_solver_eigen.*}, src/model/common/
// non_linear_solver/non_linear_solver_petsc.cc:100-200 (SNES FormFunction /
// FormJacobian through a context pointer).

//! Linear-system backends of the DOF manager
//! (D-014).
//!
//! The DOF manager owns DOF arrays, numbering and the residual; **matrix
//! storage, assembly finalisation and the linear solve belong to a
//! backend** behind [`LinearSystemBackend`] (a trait object: the manager is
//! dynamically dispatched over its backend, not generic):
//!
//! | Backend | Matrix storage | Numbering seen by the backend | Solve | Feature |
//! |---|---|---|---|---|
//! | [`AijBackend`] | [`SparseMatrixAij`] (COO, global coordinates) | akantu global equations | a [`SparseDirectSolver`] per matrix (eigen / faer, centralised on rank 0 when multi-rank, D-040; MUMPS) | default |
//! | `PetscBackend` (`crate::petsc_backend`) | rank-partial AIJ assembly store + PETSc `Mat` built at each collective native use | PETSc application ordering (AO) applied to **rows, columns and vectors alike** (UD-006 fix) | KSP, or SNES/TAO through [`NativeNonLinearDriver`] | `petsc` |
//!
//! Contract for every backend:
//! * global coordinates passed in are akantu global equations
//!   ([`GlobalIdx`]); a backend with another numbering maps rows, columns and
//!   vector entries through one consistent map (UD-006);
//! * **collective** operations — [`LinearSystemBackend::finalize_assembly`],
//!   [`LinearSystemBackend::solve`] — receive a [`BackendContext`] and
//!   communicate only through its [`Phases`] scope (prepared payloads
//!   agreed before any message);
//! * effective symmetry (UD-034 default policy): a local promotion during
//!   `add_values` is a proposal; the DOF manager agrees it on all ranks at
//!   `finalize` and calls [`LinearSystemBackend::promote_to_unsymmetric`]
//!   on every rank and on derived matrices; a backend must then
//!   re-analyse (MUMPS `sym = 0`, new PETSc preallocation);
//! * when the numbering changes (registration, node addition, connectivity
//!   change) the manager resets **every** matrix with
//!   [`LinearSystemBackend::reset_profile`] — profile, values and solver
//!   analysis/factorisation caches — like upstream
//!   `DOFManager::resizeGlobalArrays` → `clearProfile` on every matrix;
//! * solve vectors are indexed by **local equations** ([`EquationLayout`]):
//!   the backend reads the **owned** rows of the right-hand side (complete
//!   on their owner) and writes the owned rows of the solution; shared rows
//!   are filled afterwards by the manager (owner → copies over the node
//!   synchronizer) and pure-ghost rows have no global equation and are
//!   never read nor written;
//! * `aij` returns the COO matrix for the Python `AkantuSparseMatrix` API or
//!   `Error::Unsupported` for native backends; no solve path uses it;
//! * native resources are released before MPI finalisation, on MPI's main
//!   thread when the policy requires it
//!   (`akantu_comm::lifecycle::Lifecycle::release_handle`).
//!
//! **Native non-linear solvers** (PETSc SNES/TAO): the backend hands out a
//! [`NativeNonLinearDriver`] owning copies of its native handles, so the
//! backend itself stays reachable through the DOF manager while the native
//! solver runs. The driver calls back through a [`NativeSolveContext`]
//! (`{ &mut DofManager, &mut dyn SolverCallback, Option<Error> }`, an
//! opaque pointer in C) whose trampolines return status codes: the Rust
//! error is stored in the context and re-raised after the native solve
//! returns. `form_function`/`form_jacobian` follow upstream
//! `NonLinearSolverPETSc::assembleResidual/assembleJacobian`: corrector on
//! the trial solution, callback assembly, blocked rows zeroed, `F = −r`.

use std::collections::BTreeMap;

use akantu_comm::phase::Phases;
use akantu_core::enums::{MatrixType, SparseSolverType};
use akantu_core::{Error, GlobalIdx, Id, Real, Result};
use akantu_linalg::centralize;
use akantu_linalg::direct::{
    BackendCapabilities, MatrixDistribution, SparseDirectSolver, SparseSolverCompat,
};
use akantu_linalg::sparse::BlockPositionCache;
use akantu_linalg::{EquationLayout, PartialSumReduction, SparseMatrixAij, SymmetryPolicy};

use crate::callback::SolverCallback;
use crate::dof_manager::DofManager;

/// Backend kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BackendKind {
    /// Upstream `DOFManagerDefault` (+ Eigen/faer or MUMPS solvers).
    Aij,
    /// Upstream `DOFManagerPETSc`.
    Petsc,
    /// Test/mock or third-party backend.
    Other(&'static str),
}

/// Context of a collective backend operation.
#[derive(Debug)]
pub struct BackendContext<'a> {
    /// Local ↔ global equations of this rank (owned / shared / pure ghost).
    pub layout: &'a EquationLayout,
    /// Phase scope of the enclosing collective operation; every
    /// communication of the backend goes through it.
    pub phases: &'a mut Phases,
}

/// Matrix storage, assembly and solve backend of a DOF manager.
pub trait LinearSystemBackend: std::fmt::Debug + Send {
    /// Kind.
    fn kind(&self) -> BackendKind;
    /// Create matrix `id` of declared type `ty` and global order `size`.
    fn new_matrix(&mut self, id: &str, ty: MatrixType, size: GlobalIdx) -> Result<()>;
    /// Whether `id` exists.
    fn has_matrix(&self, id: &str) -> bool;
    /// Ids of every matrix (deterministic order).
    fn matrix_ids(&self) -> Vec<Id>;
    /// Effective symmetry of `id` (after an agreed or proposed promotion).
    fn matrix_type(&self, id: &str) -> Result<MatrixType>;
    /// Remove the profile and values of `id`, set its global order to
    /// `size` and invalidate solver analysis/factorisation (upstream
    /// `clearProfile` on a numbering change).
    fn reset_profile(&mut self, id: &str, size: GlobalIdx) -> Result<()>;
    /// Switch `id` to unsymmetric storage (agreed UD-034 promotion); a
    /// no-op if already unsymmetric. Must not fail half-way.
    fn promote_to_unsymmetric(&mut self, id: &str) -> Result<()>;
    /// Profile an elemental block (global rows × global columns).
    fn add_block_to_profile(
        &mut self,
        id: &str,
        rows: &[GlobalIdx],
        cols: &[GlobalIdx],
    ) -> Result<()>;
    /// Add a column-major elemental block (atomic: no change on error).
    fn add_values(
        &mut self,
        id: &str,
        rows: &[GlobalIdx],
        cols: &[GlobalIdx],
        values: &[Real],
        values_type: MatrixType,
    ) -> Result<()>;
    /// Release the analysis and factorisation of the sparse solver of
    /// matrix `id` on the caller's request (port extension, upstream keeps
    /// them): the next solve of `id` analyses and factorises again.
    /// Collective for a distributed solver (MUMPS). Default: not supported.
    fn release_factorization(&mut self, id: &str) -> Result<()> {
        Err(Error::Unsupported(format!(
            "releasing the factorisation of matrix '{id}' is not supported by the {:?} backend",
            self.kind()
        )))
    }
    /// Release the values and the lookup map of matrix `id`, keeping its
    /// profile (coordinates in storage order): the next assembly finds
    /// every entry at its former position
    /// ([`SparseMatrixAij::release_values`]). Default: not supported.
    fn release_matrix_values(&mut self, id: &str) -> Result<()> {
        Err(Error::Unsupported(format!(
            "releasing the values of matrix '{id}' is not supported by the {:?} backend",
            self.kind()
        )))
    }
    /// Release the storage of matrix `id` (profile, values, lookup map);
    /// the matrix stays declared and the next assembly profiles it again.
    /// `size` is the system order. Default:
    /// [`LinearSystemBackend::reset_profile`].
    fn release_matrix_storage(&mut self, id: &str, size: GlobalIdx) -> Result<()> {
        self.reset_profile(id, size)
    }
    /// `true` if [`LinearSystemBackend::add_block_to_profile_and_values`]
    /// may replace a loop of `add_block_to_profile` over every element
    /// followed by a loop of `add_values` over the same elements in the
    /// same order (the same profile, values and order). Default: `false`.
    fn can_fuse_profile_and_values(&self, _id: &str, _values_type: MatrixType) -> bool {
        false
    }
    /// `add_block_to_profile` then `add_values` of one block (see
    /// [`LinearSystemBackend::can_fuse_profile_and_values`]).
    fn add_block_to_profile_and_values(
        &mut self,
        id: &str,
        rows: &[GlobalIdx],
        cols: &[GlobalIdx],
        values: &[Real],
        values_type: MatrixType,
    ) -> Result<()> {
        self.add_block_to_profile(id, rows, cols)?;
        self.add_values(id, rows, cols, values, values_type)
    }
    /// Start a repeated element loop into `id` with the position cache
    /// `cache` (see [`LinearSystemBackend::add_values_cached`]). Default:
    /// nothing (the cache stays unused).
    fn begin_cached_assembly(&self, _id: &str, _cache: &mut BlockPositionCache) {}
    /// [`LinearSystemBackend::add_values`] for slot `slot` (element) of a
    /// repeated element loop: the same values, order, errors and releases,
    /// with the profile lookups cached in `cache` where the backend has a
    /// cache ([`SparseMatrixAij::add_values_cached`]). Default: `add_values`.
    #[allow(clippy::too_many_arguments)] // `add_values` + the cache and slot
    fn add_values_cached(
        &mut self,
        id: &str,
        _cache: &mut BlockPositionCache,
        _slot: usize,
        rows: &[GlobalIdx],
        cols: &[GlobalIdx],
        values: &[Real],
        values_type: MatrixType,
    ) -> Result<()> {
        self.add_values(id, rows, cols, values, values_type)
    }
    /// The serial loop of [`LinearSystemBackend::add_values_cached`] over a
    /// batch of square blocks (slot `slots[k]`, equations `equations(k, ·)`,
    /// values `values[k·n²..(k+1)·n²]`, `n = nb_eq`) on the threads of
    /// `exec`, with the same results
    /// ([`SparseMatrixAij::add_values_cached_batch`]). `false` when not done
    /// (nothing changed but the cache's configuration): the caller runs the
    /// serial loop. Default: `false`.
    #[allow(clippy::too_many_arguments)] // the serial loop's arguments, batched
    fn add_values_cached_batch(
        &mut self,
        _id: &str,
        _exec: &akantu_core::exec::Exec,
        _cache: &mut BlockPositionCache,
        _slots: &[usize],
        _nb_eq: usize,
        _equations: &(dyn Fn(usize, &mut Vec<GlobalIdx>) -> Result<()> + Sync),
        _values: &[Real],
        _values_type: MatrixType,
    ) -> bool {
        false
    }
    /// Run the whole-matrix operations (zeroing, sums, Dirichlet rows,
    /// products) on the threads of `exec`, with the results of one thread
    /// (D-123). Default: ignored.
    fn set_exec(&mut self, exec: &akantu_core::exec::Exec) {
        let _ = exec;
    }
    /// Add individual terms (pre-assembled matrices), creating entries.
    fn add_terms(&mut self, id: &str, terms: &[(GlobalIdx, GlobalIdx, Real)]) -> Result<()>;
    /// Collective end of an assembly phase of `id` (PETSc
    /// `MatAssemblyBegin/End`; validation only for AIJ).
    fn finalize_assembly(&mut self, id: &str, ctx: &mut BackendContext<'_>) -> Result<()>;
    /// Zero the values of `id`, keeping its profile.
    fn zero(&mut self, id: &str) -> Result<()>;
    /// Dirichlet rows/columns (owner-only diagonal).
    fn apply_boundary(
        &mut self,
        id: &str,
        layout: &EquationLayout,
        blocked: &[bool],
        block_val: Real,
    ) -> Result<()>;
    /// `y = beta·y + alpha·A·x` over local equations.
    #[allow(clippy::too_many_arguments)] // mirrors SparseMatrixAij::mat_vec_mul plus the id
    fn mat_vec(
        &self,
        id: &str,
        layout: &EquationLayout,
        reduction: &dyn PartialSumReduction,
        x: &[Real],
        y: &mut [Real],
        alpha: Real,
        beta: Real,
    ) -> Result<()>;
    /// Solve `A x = b` with matrix `id` (collective). `rhs` and `solution`
    /// are indexed by local equations; only owned rows are read/written.
    fn solve(
        &mut self,
        id: &str,
        ctx: &mut BackendContext<'_>,
        rhs: &[Real],
        solution: &mut [Real],
    ) -> Result<()>;
    /// A native non-linear solver over matrix `id` (PETSc SNES/TAO);
    /// `Error::Unsupported` for backends without one.
    fn native_non_linear(&mut self, id: &str) -> Result<Box<dyn NativeNonLinearDriver>> {
        Err(Error::Unsupported(format!(
            "backend {:?} has no native non-linear solver (matrix '{id}')",
            self.kind()
        )))
    }
    /// COO access (Python `getMatrix`); `Error::Unsupported` for native
    /// backends.
    fn aij(&self, id: &str) -> Result<&SparseMatrixAij>;
    /// Mutable COO access (upstream `getMatrix(id)` used for direct
    /// `add(i, j, v)` / `operator*=` by callbacks, e.g.
    /// `test_dof_manager_default`); `Error::Unsupported` for native
    /// backends.
    fn aij_mut(&mut self, id: &str) -> Result<&mut SparseMatrixAij> {
        Err(Error::Unsupported(format!(
            "backend {:?} has no mutable COO access (matrix '{id}')",
            self.kind()
        )))
    }
    /// Value release of `id` (upstream `SparseMatrix::getRelease`, compared
    /// with `!=` by the integration schemes and `applyBoundary`).
    fn value_release(&self, id: &str) -> Result<u64> {
        Err(Error::Unsupported(format!(
            "backend {:?} does not report releases (matrix '{id}')",
            self.kind()
        )))
    }
    /// `dst.copyProfile(src)` (upstream `SparseMatrixAIJ::copyProfile`:
    /// coordinates and profile release of `src`, values zeroed, value
    /// release of `dst` incremented, `dst`'s type kept).
    fn copy_profile(&mut self, dst: &str, src: &str) -> Result<()> {
        Err(Error::Unsupported(format!(
            "backend {:?} cannot copy the profile of '{src}' into '{dst}'",
            self.kind()
        )))
    }
    /// `dst += alpha · src` (upstream `SparseMatrix::add(B, alpha)` →
    /// `B.addMeTo(*this, alpha)`), entries of `src` in storage order; see
    /// [`MatrixAddPolicy`] for a symmetric `src` added to an unsymmetric
    /// `dst`.
    fn add_matrix(&mut self, dst: &str, src: &str, alpha: Real) -> Result<()> {
        let _ = alpha;
        Err(Error::Unsupported(format!(
            "backend {:?} cannot add '{src}' to '{dst}'",
            self.kind()
        )))
    }
    /// New matrix `id` as a copy of `src` (upstream
    /// `getNewMatrix(id, matrix_to_copy_id)`, the AIJ copy constructor).
    fn copy_matrix(&mut self, id: &str, src: &str) -> Result<()> {
        Err(Error::Unsupported(format!(
            "backend {:?} cannot copy '{src}' into a new matrix '{id}'",
            self.kind()
        )))
    }
    /// Use `solver` for the linear solves with matrix `id` (upstream: the
    /// Newton–Raphson solver allocates `SparseSolverFactory(type, dof_manager,
    /// "J")`). Backends with their own linear solvers (PETSc KSP) return
    /// `Error::Unsupported`.
    fn set_sparse_solver(&mut self, id: &str, solver: Box<dyn SparseDirectSolver>) -> Result<()> {
        let _ = solver;
        Err(Error::Unsupported(format!(
            "backend {:?} does not take a pluggable sparse solver (matrix '{id}')",
            self.kind()
        )))
    }
    /// Release the internal data of every linear solver of the backend
    /// (upstream `SparseSolver::onCommunicatorFinalize` →
    /// `destroyInternalData`, `sparse_solver.cc:70`). [`crate::DofManager`]
    /// calls it when it is dropped, **before** its communicator handle is
    /// released, so a solver's internal data (factorisations, native solver
    /// instances bound to the communicator) never outlives the
    /// communicator's finalisation. A later
    /// solve needs a new analysis. Default: nothing to release.
    fn release_solvers(&mut self) {}
    /// Kind of the sparse direct solver installed for matrix `id`
    /// (`Eigen`, `Faer`, …), if any (D-040: the reported solver kind matches
    /// the resolved choice).
    fn sparse_solver_kind(&self, id: &str) -> Option<SparseSolverType> {
        let _ = id;
        None
    }
    /// Diagnostic statistics of the last solve of the sparse direct solver
    /// installed for matrix `id` (`SparseDirectSolver::statistics`: MUMPS
    /// memory / flop figures); empty if there is none. Read-only.
    fn sparse_solver_statistics(&self, id: &str) -> Vec<(&'static str, Real)> {
        let _ = id;
        Vec::new()
    }
    /// The manager-wide upstream-compatibility switches of the sparse
    /// solvers (D-041 (4); `DofManager` passes them before every solve):
    /// the backend applies them to the solver of every solve
    /// (`SparseDirectSolver::set_upstream_compat`), so a reallocated
    /// solver follows the manager's switch too. Default: no pluggable
    /// solver, nothing to apply.
    fn set_sparse_solver_compat(&mut self, compat: SparseSolverCompat) {
        let _ = compat;
    }
    /// `true` if the backend solves a multi-rank system with a serial
    /// sparse direct solver by centralising it on rank 0 inside the solve's
    /// agreed collective scope (D-040: [`AijBackend`]); the Newton–Raphson
    /// solver then allocates its solver with
    /// `SparseSolverFactory::create_centralized` instead of the direct
    /// `create(type, nb_proc)`, which refuses `eigen`/`faer` on several
    /// processes like upstream.
    fn centralizes_sparse_solves(&self) -> bool {
        false
    }
    /// A native non-linear solver configured by `request` (solver kind,
    /// options, bounds). Default: [`LinearSystemBackend::native_non_linear`].
    fn native_non_linear_with(
        &mut self,
        id: &str,
        request: &NativeRequest,
    ) -> Result<Box<dyn NativeNonLinearDriver>> {
        let _ = request;
        self.native_non_linear(id)
    }
    /// Bring the backend's native copy of matrix `id` up to date
    /// (collective; the PETSc matrix a native solver holds, after
    /// a Jacobian assembly inside its callback). Default: nothing to do.
    fn sync_native(&mut self, id: &str, ctx: &mut BackendContext<'_>) -> Result<()> {
        let _ = (id, ctx);
        Ok(())
    }
    /// Set an option of the linear solver of matrix `id` (the
    /// KSP options of upstream `SparseSolverPETSc::set`, e.g. `pc_type`).
    /// Default: `Error::Unsupported`.
    fn set_solver_option(&mut self, id: &str, name: &str, value: &str) -> Result<()> {
        let _ = (name, value);
        Err(Error::Unsupported(format!(
            "backend {:?} has no linear-solver options (matrix '{id}')",
            self.kind()
        )))
    }
    /// The manager's upstream-compatibility switches changed (D-041: one
    /// switch per DOF manager applies to its backend too). Default: none
    /// apply.
    fn set_upstream_compat(&mut self, compat: &crate::dof_manager::UpstreamCompat) {
        let _ = compat;
    }
    /// Downcast access for backend-specific drivers. Default:
    /// `None`.
    fn as_any_mut(&mut self) -> Option<&mut dyn std::any::Any> {
        None
    }
}

/// Configuration of a native non-linear solve (upstream
/// `NonLinearSolverPETSc` / `NonLinearSolverTAO` state handed to the
/// backend at each solve).
#[derive(Clone, Debug, Default)]
pub struct NativeRequest {
    /// Kind (`PetscSnes` or `PetscTao`).
    pub kind: Option<akantu_core::enums::NonLinearSolverType>,
    /// Identifier of the non-linear solver (object names).
    pub solver_id: Id,
    /// Private options database (`-name value`, upstream `ParsablePETSc`),
    /// in the order they were set.
    pub options: Vec<(String, String)>,
    /// Variable bounds on the increment over local equations (TAO,
    /// upstream `NonLinearSolverTAO::setBounds`).
    pub bounds: Option<(Vec<Real>, Vec<Real>)>,
    /// UD-428 compat: reproduce upstream's aliasing of the solver's iterate
    /// with the DOF manager's solution vector (trial points copied into
    /// it) and of SNES's function vector with the residual.
    pub upstream_aliasing: bool,
}

/// Diagnostics of the last native solve (upstream `NonLinearSolverPETSc`
/// read-only parameters and `SNESNotConvergedException` data).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct NativeReport {
    /// Native converged reason (`>= 0`: converged).
    pub reason: i32,
    /// Reason name (`convergence_reason`).
    pub reason_string: String,
    /// `function_norm`.
    pub function_norm: Real,
    /// `solution_norm`.
    pub solution_norm: Real,
    /// `update_norm`.
    pub update_norm: Real,
    /// Relative tolerance (the threshold of `Error::NotConverged`).
    pub rtol: Real,
    /// Absolute tolerance.
    pub atol: Real,
    /// Step tolerance.
    pub stol: Real,
    /// Divergence tolerance.
    pub divtol: Real,
    /// Maximum iterations.
    pub max_iterations: i64,
    /// Relative norm `norm / petsc_a[0]` (SNES); TAO: the final gradient
    /// norm.
    pub relative_norm: Real,
    /// TAO: the final objective value (`TaoGetSolutionStatus`; 0 for SNES).
    pub objective: Real,
}

impl NativeReport {
    /// Upstream's `TAONotConvergedException` text
    /// (`non_linear_solver_tao.hh:107-115`) for a TAO solve that stopped
    /// after `n_iter` iterations: reals as `std::stringstream` prints them
    /// (6 significant digits), the reason as its integer.
    #[must_use]
    pub fn tao_not_converged_message(&self, n_iter: usize) -> String {
        let g = |x: Real| akantu_linalg::matrix_market::format_g(x, 6);
        format!(
            "The TAO solver did not converge after {n_iter} iterations (reason: {}, \
             max_iterations: {}, atol: {}, rtol: {}, ttol: {})",
            self.reason,
            self.max_iterations,
            g(self.atol),
            g(self.rtol),
            g(self.stol)
        )
    }
}

/// How `dst += alpha · src` treats a symmetric-storage `src` added to an
/// unsymmetric `dst` (candidate defect, `sparse_matrix_aij.cc:252-268`:
/// upstream adds only the stored triangle, so `J = c_K K + c_M M` loses the
/// mirror of `M` once `J` is unsymmetric, e.g. after a UD-034 promotion of
/// `K`). Identical results whenever `dst` is symmetric or `src` unsymmetric.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MatrixAddPolicy {
    /// Default (fix): every off-diagonal entry of a symmetric `src` is also
    /// added at its mirror position ([`SparseMatrixAij::add_matrix`]).
    #[default]
    MirrorSymmetricSource,
    /// Upstream compatibility: entries of `src` as stored
    /// ([`SparseMatrixAij::add_matrix_upstream`]).
    Upstream,
}

/// A non-linear solver running inside a backend (PETSc SNES/TAO). It owns
/// copies of the native handles it needs; it reaches the model and the DOF
/// manager (including the backend's matrices) only through `ctx`.
pub trait NativeNonLinearDriver: std::fmt::Debug {
    /// Run the native solve. Returns `(iterations, final error,
    /// converged)`; callback failures are stored in `ctx` and re-raised by
    /// the caller.
    fn solve(&mut self, ctx: &mut NativeSolveContext<'_>) -> Result<(usize, Real, bool)>;
    /// Diagnostics of the last solve (`None`: not reported).
    fn report(&self) -> Option<NativeReport> {
        None
    }
}

/// Callback context of a native non-linear solve (the opaque `void *ctx`
/// of `SNESSetFunction`/`SNESSetJacobian`), valid for one native solve.
pub struct NativeSolveContext<'a> {
    dofs: &'a mut DofManager,
    callback: &'a mut dyn SolverCallback,
    matrix_id: Id,
    error: Option<Error>,
}

impl std::fmt::Debug for NativeSolveContext<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NativeSolveContext")
            .field("matrix_id", &self.matrix_id)
            .field("error", &self.error)
            .finish_non_exhaustive()
    }
}

/// Run a model callback of a native solve, turning a panic into an **agreed
/// fatal** error of the enclosing step: the panicking
/// rank's step reports `Error::Fatal { kind: Agreed }` to its agreement, so
/// every rank fails at that step at once (before the fix the panic unwound
/// past the agreement and the other ranks waited for the MPI deadline).
/// Fatal, because the callback may have left the model or the DOF manager
/// half-updated: the manager is poisoned on every rank.
fn guarded<T>(what: &str, f: impl FnOnce() -> Result<T>) -> Result<T> {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)) {
        Ok(r) => r,
        Err(payload) => {
            let msg = payload
                .downcast_ref::<&str>()
                .map(|s| (*s).to_owned())
                .or_else(|| payload.downcast_ref::<String>().cloned())
                .unwrap_or_else(|| "non-string panic payload".to_owned());
            Err(Error::fatal(
                akantu_core::FatalKind::Agreed,
                Error::User(format!("native solve callback '{what}' panicked: {msg}")),
            ))
        }
    }
}

impl<'a> NativeSolveContext<'a> {
    /// Context over the running solve.
    pub fn new(
        dofs: &'a mut DofManager,
        callback: &'a mut dyn SolverCallback,
        matrix_id: impl Into<Id>,
    ) -> Self {
        Self {
            dofs,
            callback,
            matrix_id: matrix_id.into(),
            error: None,
        }
    }

    /// Local ↔ global layout of the DOF manager.
    #[must_use]
    pub fn layout(&self) -> &EquationLayout {
        self.dofs.layout()
    }

    /// Number of local equations (length of every vector of the context).
    #[must_use]
    pub fn nb_local(&self) -> usize {
        self.dofs.local_system_size()
    }

    fn status(&mut self, r: Result<()>) -> i32 {
        match r {
            Ok(()) => 0,
            Err(e) => {
                // The first error keeps its diagnostic; a later, more
                // urgent fatal kind is not lost.
                self.error = Some(match self.error.take() {
                    None => e,
                    Some(first) => first.escalated_by(&e),
                });
                1
            }
        }
    }

    /// Trampoline `FormFunction(x, f)`: corrector on the trial solution
    /// `x` (owned rows), callback residual assembly, blocked rows zeroed,
    /// `f = −residual` on owned rows. Returns 0 or a nonzero status (the
    /// error is stored in the context).
    ///
    /// Every trampoline is one [`DofManager::collective`] operation (it
    /// joins the native solve's scope when called from
    /// [`crate::NativeNonLinear`]): vector shapes and each callback outcome
    /// are agreed before any dependent communication, so the returned
    /// status is **identical on every rank**.
    pub fn form_function(&mut self, x: &[Real], f: &mut [Real]) -> i32 {
        let cb = &mut *self.callback;
        let r = self.dofs.collective("native:form_function", |dofs| {
            let n = dofs.local_system_size();
            let shape = check_len("NativeSolveContext::form_function (f)", f.len(), n);
            dofs.step("shape", |_| shape)?;
            dofs.set_solution(x)?;
            // upstream `NonLinearSolverPETSc::corrector(x)`: the trial
            // increment is applied to the last converged step.
            dofs.step("callback:restore_last_converged_step", |d| {
                guarded("restore_last_converged_step", || {
                    cb.restore_last_converged_step(d)
                })
            })?;
            dofs.step("callback:corrector", |d| {
                guarded("corrector", || cb.corrector(d))
            })?;
            dofs.zero_residual();
            dofs.step("callback:assemble_residual", |d| {
                guarded("assemble_residual", || cb.assemble_residual(d))
            })?;
            let blocked = dofs.blocked_snapshot();
            let layout = dofs.layout();
            let res = dofs.residual().values();
            for (l, fl) in f.iter_mut().enumerate() {
                *fl = if layout.is_owned(l) && !blocked[l] {
                    -res[l]
                } else {
                    0.0
                };
            }
            drop(res);
            Ok(())
        });
        self.status(r)
    }

    /// Trampoline `FormJacobian(x)`: corrector on `x`, callback assembly of
    /// the matrix, collective finalisation (symmetry agreement) and
    /// Dirichlet rows; status identical on every rank (see
    /// [`NativeSolveContext::form_function`]).
    pub fn form_jacobian(&mut self, x: &[Real]) -> i32 {
        let id = self.matrix_id.clone();
        let cb = &mut *self.callback;
        let r = self.dofs.collective("native:form_jacobian", |dofs| {
            dofs.set_solution(x)?;
            dofs.step("callback:restore_last_converged_step", |d| {
                guarded("restore_last_converged_step", || {
                    cb.restore_last_converged_step(d)
                })
            })?;
            dofs.step("callback:corrector", |d| {
                guarded("corrector", || cb.corrector(d))
            })?;
            dofs.step("callback:assemble_matrix", |d| {
                guarded("assemble_matrix", || cb.assemble_matrix(d, &id))
            })?;
            dofs.finalize_matrix(&id)?;
            dofs.apply_boundary(&id)
        });
        self.status(r)
    }

    /// Native linear solve with the backend's own matrix `id` (the KSP of
    /// a SNES), local-equation vectors; status identical on every rank.
    pub fn linear_solve(&mut self, rhs: &[Real], x: &mut [Real]) -> i32 {
        let id = self.matrix_id.clone();
        let r = self.dofs.solve_linear_system(&id, rhs, x);
        self.status(r)
    }

    /// Rank-ordered Euclidean norm over owned rows (collective; the
    /// outcome is identical on every rank).
    pub fn owned_norm(&mut self, v: &[Real]) -> std::result::Result<Real, i32> {
        let r = self.dofs.owned_norm(v);
        match r {
            Ok(n) => Ok(n),
            Err(e) => Err(self.status(Err(e))),
        }
    }

    /// The first error stored by a trampoline, carrying the most urgent
    /// fatal kind of every stored error.
    pub fn take_error(&mut self) -> Option<Error> {
        self.error.take()
    }

    /// `true` once a trampoline stored an error (identical on every rank:
    /// trampoline outcomes are agreed).
    #[must_use]
    pub fn has_error(&self) -> bool {
        self.error.is_some()
    }

    /// The DOF manager's communicator (native entries of the driver).
    #[must_use]
    pub fn communicator(&self) -> &akantu_comm::Communicator {
        self.dofs.communicator()
    }

    /// Identifier of the native solver's matrix (`"J"`).
    #[must_use]
    pub fn matrix_id(&self) -> &str {
        &self.matrix_id
    }

    /// Agree a local result of the driver on every rank (a step of the
    /// solve's scope), e.g. the verdict of a native region's admission
    /// before the region is entered.
    pub fn agree(&mut self, name: &str, local: Result<()>) -> Result<()> {
        self.dofs.step(name, |_| local)
    }

    /// One collective operation giving `f` the backend and a
    /// [`BackendContext`] of the solve's scope (the driver's native
    /// preparation, the synchronisation of a native matrix); the outcome is
    /// agreed.
    pub fn with_backend<R>(
        &mut self,
        name: &str,
        f: impl FnOnce(&mut dyn LinearSystemBackend, &mut BackendContext<'_>) -> Result<R>,
    ) -> Result<R> {
        self.dofs.backend_collective(name, f)
    }

    /// Trampoline status of [`NativeSolveContext::with_backend`] for a
    /// native matrix update inside a callback (`sync_native` of matrix
    /// `id`).
    pub fn sync_native(&mut self) -> i32 {
        let id = self.matrix_id.clone();
        let r = self
            .dofs
            .backend_collective(&format!("native:sync:{id}"), |b, ctx| {
                b.sync_native(&id, ctx)
            });
        self.status(r)
    }

    /// Rank-ordered dot product over owned rows (collective; outcome
    /// identical on every rank). TAO objective.
    pub fn owned_dot(&mut self, a: &[Real], b: &[Real]) -> std::result::Result<Real, i32> {
        let r = self.dofs.owned_dot(a, b);
        match r {
            Ok(v) => Ok(v),
            Err(e) => Err(self.status(Err(e))),
        }
    }

    /// End of a native solve (upstream `NonLinearSolverPETSc::solve` after
    /// `SNESSolve`): the final increment `x` (owned rows) set as the
    /// solution, `restore_last_converged_step`, `corrector`. Status
    /// identical on every rank.
    pub fn finish(&mut self, x: &[Real]) -> i32 {
        let cb = &mut *self.callback;
        let r = self.dofs.collective("native:finish", |dofs| {
            dofs.set_solution(x)?;
            dofs.step("callback:restore_last_converged_step", |d| {
                guarded("restore_last_converged_step", || {
                    cb.restore_last_converged_step(d)
                })
            })?;
            dofs.step("callback:corrector", |d| {
                guarded("corrector", || cb.corrector(d))
            })
        });
        self.status(r)
    }
}

/// Capacity limits of `caps` for the whole matrix `a` solved on one
/// process: global equations, entries and — for a
/// centralised right-hand side — the centralised order.
fn serial_capacity_check(caps: &BackendCapabilities, a: &SparseMatrixAij) -> Result<()> {
    let nnz = u64::try_from(a.nnz()).unwrap_or(u64::MAX);
    caps.check_local(a.size(), nnz)?;
    if caps.rhs == akantu_linalg::direct::RhsDistribution::CentralizedOnRoot {
        caps.check_centralized(a.size(), nnz)?;
    }
    Ok(())
}

fn check_len(context: &'static str, got: usize, expected: usize) -> Result<()> {
    if got == expected {
        Ok(())
    } else {
        Err(Error::DimensionMismatch {
            context,
            expected,
            got,
        })
    }
}

/// Default backend: upstream `DOFManagerDefault` storage, with one
/// [`SparseDirectSolver`] per solved matrix.
#[derive(Debug, Default)]
pub struct AijBackend {
    matrices: BTreeMap<Id, SparseMatrixAij>,
    solvers: BTreeMap<Id, Box<dyn SparseDirectSolver>>,
    /// `(profile release, effective type)` analysed and value release
    /// factorised, per solved matrix.
    analysed: BTreeMap<Id, ((u64, MatrixType), u64)>,
    /// Multi-rank centralised solves (rank 0): every rank's `(profile
    /// release, symmetric)` analysed and value releases factorised.
    central: BTreeMap<Id, CentralCache>,
    policy: SymmetryPolicy,
    add_policy: MatrixAddPolicy,
    /// Manager-wide sparse-solver compatibility switches, applied to the
    /// solver of every solve (D-041 (4)).
    sparse_compat: SparseSolverCompat,
    /// Executor of the whole-matrix operations (zeroing, sums, Dirichlet
    /// rows, products): the DOF manager's (D-123).
    exec: akantu_core::exec::Exec,
}

/// Rank-0 change detector of a centralised multi-rank solve.
type CentralCache = (Vec<(u64, bool)>, Vec<u64>);

impl AijBackend {
    /// Backend with the UD-034 `policy` for new matrices.
    #[must_use]
    pub fn with_policy(policy: SymmetryPolicy) -> Self {
        Self {
            policy,
            ..Self::default()
        }
    }

    /// Select the [`MatrixAddPolicy`] of `add_matrix` (default: the mirror
    /// fix; [`MatrixAddPolicy::Upstream`] for differential runs that need
    /// upstream's operator when `J` is unsymmetric).
    pub fn set_matrix_add_policy(&mut self, policy: MatrixAddPolicy) {
        self.add_policy = policy;
    }

    /// Current [`MatrixAddPolicy`].
    #[must_use]
    pub fn matrix_add_policy(&self) -> MatrixAddPolicy {
        self.add_policy
    }

    /// Use `solver` for the linear solves of matrix `id` (upstream: the
    /// non-linear solver creates `SparseSolver(dof_manager, "J")`).
    /// A replaced solver releases its internal data first (upstream: the
    /// replaced `SparseSolver` is destroyed, e.g. `mumpsDataDestroy`).
    pub fn set_solver(&mut self, id: &str, solver: Box<dyn SparseDirectSolver>) {
        self.analysed.remove(id);
        self.central.remove(id);
        if let Some(mut old) = self.solvers.insert(id.into(), solver) {
            old.destroy_internal_data();
        }
    }

    fn get(&self, id: &str) -> Result<&SparseMatrixAij> {
        self.matrices
            .get(id)
            .ok_or_else(|| Error::not_found("matrix", id))
    }

    fn get_mut(&mut self, id: &str) -> Result<&mut SparseMatrixAij> {
        self.matrices
            .get_mut(id)
            .ok_or_else(|| Error::not_found("matrix", id))
    }

    /// Multi-rank solve with a centralised backend (see
    /// [`LinearSystemBackend::solve`]).
    fn solve_centralized(
        &mut self,
        id: &str,
        ctx: &mut BackendContext<'_>,
        rank: akantu_core::Rank,
        rhs: &[Real],
        solution: &mut [Real],
    ) -> Result<()> {
        // Local preflight; the backend's distribution and capacity limits
        // are checked and agreed by `gather_system` before any payload is
        // copied.
        let mut caps = None;
        let check = (|| -> Result<&SparseMatrixAij> {
            let solver = self
                .solvers
                .get(id)
                .ok_or_else(|| Error::not_found("sparse solver for matrix", id))?;
            let c = solver.capabilities();
            if !c.is_fully_centralized() {
                return Err(Error::Unsupported(format!(
                    "AijBackend: the {} solver of matrix '{id}' takes neither a centralised nor a \
                     distributed-assembled system ({:?} matrix, {:?} right-hand side)",
                    solver.name(),
                    c.matrix,
                    c.rhs
                )));
            }
            caps = Some(c);
            if solution.len() != ctx.layout.nb_local() {
                return Err(Error::DimensionMismatch {
                    context: "AijBackend::solve",
                    expected: ctx.layout.nb_local(),
                    got: solution.len(),
                });
            }
            self.get(id)
        })();
        let caps = caps.unwrap_or_else(akantu_linalg::direct::centralized_capabilities);
        let name = format!("aij:solve:{id}");
        let sys = centralize::gather_system(ctx.phases, &name, ctx.layout, check, rhs, &caps)?;
        let x_root = if rank == 0 {
            (|| -> Result<Option<Vec<Real>>> {
                let (Some(a), Some(b)) = (sys.matrix.as_ref(), sys.rhs.as_ref()) else {
                    return Err(Error::invalid("rank 0 has no gathered system"));
                };
                let solver = self
                    .solvers
                    .get_mut(id)
                    .ok_or_else(|| Error::not_found("sparse solver for matrix", id))?;
                let profile: Vec<(u64, bool)> = sys.releases.iter().map(|r| (r.0, r.2)).collect();
                let values: Vec<u64> = sys.releases.iter().map(|r| r.1).collect();
                let cache = self.central.entry(id.into()).or_default();
                if cache.0 != profile {
                    // the gathered matrix is a new object: re-analyse its pattern
                    cache.1.clear();
                    solver.analysis(a)?;
                    cache.0 = profile;
                }
                if cache.1 != values {
                    solver.factorize(a)?;
                    cache.1 = values;
                }
                let mut x = vec![0.0; b.len()];
                solver.solve(b, &mut x)?;
                Ok(Some(x))
            })()
        } else {
            Ok(None)
        };
        if x_root.is_err() {
            self.central.remove(id);
        }
        centralize::scatter_solution(ctx.phases, &name, &sys, x_root, solution)
    }
}

impl LinearSystemBackend for AijBackend {
    fn kind(&self) -> BackendKind {
        BackendKind::Aij
    }
    fn new_matrix(&mut self, id: &str, ty: MatrixType, size: GlobalIdx) -> Result<()> {
        if self.matrices.contains_key(id) {
            return Err(Error::AlreadyExists {
                kind: "matrix",
                id: id.into(),
            });
        }
        let mut m = SparseMatrixAij::new(id, size, ty);
        m.set_symmetry_policy(self.policy);
        self.matrices.insert(id.into(), m);
        Ok(())
    }
    fn has_matrix(&self, id: &str) -> bool {
        self.matrices.contains_key(id)
    }
    fn matrix_ids(&self) -> Vec<Id> {
        self.matrices.keys().cloned().collect()
    }
    fn matrix_type(&self, id: &str) -> Result<MatrixType> {
        Ok(self.get(id)?.matrix_type())
    }
    fn reset_profile(&mut self, id: &str, size: GlobalIdx) -> Result<()> {
        self.get_mut(id)?.reset_profile(size);
        self.analysed.remove(id);
        self.central.remove(id);
        Ok(())
    }
    fn promote_to_unsymmetric(&mut self, id: &str) -> Result<()> {
        self.get_mut(id)?.promote_to_unsymmetric();
        Ok(())
    }
    fn add_block_to_profile(
        &mut self,
        id: &str,
        rows: &[GlobalIdx],
        cols: &[GlobalIdx],
    ) -> Result<()> {
        self.get_mut(id)?.add_block_to_profile(rows, cols)
    }
    fn add_values(
        &mut self,
        id: &str,
        rows: &[GlobalIdx],
        cols: &[GlobalIdx],
        values: &[Real],
        values_type: MatrixType,
    ) -> Result<()> {
        self.get_mut(id)?
            .add_values(rows, cols, values, values_type)
    }
    fn can_fuse_profile_and_values(&self, id: &str, values_type: MatrixType) -> bool {
        self.get(id).is_ok_and(|m| !m.would_promote(values_type))
    }
    fn release_factorization(&mut self, id: &str) -> Result<()> {
        self.get(id)?;
        self.analysed.remove(id);
        self.central.remove(id);
        match self.solvers.get_mut(id) {
            Some(s) => s.release_factorization(),
            None => Ok(()),
        }
    }
    fn release_matrix_values(&mut self, id: &str) -> Result<()> {
        self.get_mut(id)?.release_values();
        self.analysed.remove(id);
        self.central.remove(id);
        Ok(())
    }
    fn release_matrix_storage(&mut self, id: &str, size: GlobalIdx) -> Result<()> {
        let m = self.get_mut(id)?;
        m.release_storage();
        m.set_size(size);
        self.analysed.remove(id);
        self.central.remove(id);
        Ok(())
    }
    fn add_block_to_profile_and_values(
        &mut self,
        id: &str,
        rows: &[GlobalIdx],
        cols: &[GlobalIdx],
        values: &[Real],
        values_type: MatrixType,
    ) -> Result<()> {
        self.get_mut(id)?
            .add_block_to_profile_and_values(rows, cols, values, values_type)
    }
    fn begin_cached_assembly(&self, id: &str, cache: &mut BlockPositionCache) {
        if let Ok(m) = self.get(id) {
            cache.begin(m);
        }
    }
    fn add_values_cached(
        &mut self,
        id: &str,
        cache: &mut BlockPositionCache,
        slot: usize,
        rows: &[GlobalIdx],
        cols: &[GlobalIdx],
        values: &[Real],
        values_type: MatrixType,
    ) -> Result<()> {
        self.get_mut(id)?
            .add_values_cached(cache, slot, rows, cols, values, values_type)
    }
    fn set_exec(&mut self, exec: &akantu_core::exec::Exec) {
        self.exec = exec.clone();
    }
    fn add_values_cached_batch(
        &mut self,
        id: &str,
        exec: &akantu_core::exec::Exec,
        cache: &mut BlockPositionCache,
        slots: &[usize],
        nb_eq: usize,
        equations: &(dyn Fn(usize, &mut Vec<GlobalIdx>) -> Result<()> + Sync),
        values: &[Real],
        values_type: MatrixType,
    ) -> bool {
        self.get_mut(id).is_ok_and(|m| {
            m.add_values_cached_batch(exec, cache, slots, nb_eq, equations, values, values_type)
        })
    }
    fn add_terms(&mut self, id: &str, terms: &[(GlobalIdx, GlobalIdx, Real)]) -> Result<()> {
        let m = self.get_mut(id)?;
        // Validate every coordinate first: no partial insertion on error.
        for &(i, j, _) in terms {
            if i < 0 || j < 0 || i == akantu_core::INVALID_GLOBAL_IDX {
                return Err(Error::InvalidArgument(format!(
                    "invalid global equation ({i}, {j}) in pre-assembled terms"
                )));
            }
        }
        for &(i, j, v) in terms {
            m.add(i, j, v)?;
        }
        Ok(())
    }
    fn finalize_assembly(&mut self, id: &str, _ctx: &mut BackendContext<'_>) -> Result<()> {
        self.get(id).map(|_| ())
    }
    fn zero(&mut self, id: &str) -> Result<()> {
        let exec = self.exec.clone();
        self.get_mut(id)?.zero_with(&exec);
        Ok(())
    }
    fn apply_boundary(
        &mut self,
        id: &str,
        layout: &EquationLayout,
        blocked: &[bool],
        block_val: Real,
    ) -> Result<()> {
        let exec = self.exec.clone();
        self.get_mut(id)?
            .apply_boundary_on(&exec, layout, blocked, block_val)
    }
    fn mat_vec(
        &self,
        id: &str,
        layout: &EquationLayout,
        reduction: &dyn PartialSumReduction,
        x: &[Real],
        y: &mut [Real],
        alpha: Real,
        beta: Real,
    ) -> Result<()> {
        self.get(id)?
            .mat_vec_mul_with(&self.exec, layout, reduction, x, y, alpha, beta)
    }
    /// Solve through the matrix's [`SparseDirectSolver`]: analysis when the
    /// profile release **or the effective symmetry** changed, factorisation
    /// when the values changed. Serial runs solve locally; multi-rank runs
    /// with a centralised backend (Eigen, faer) gather the rank-partial
    /// matrix and the owned right-hand-side rows on rank 0
    /// (`akantu_linalg::centralize`, upstream `SolverVectorDistributed`),
    /// solve there (re-analysing when any rank's profile release or the
    /// symmetry changed, refactorising when any value release changed) and
    /// scatter the owned solution rows back; shared rows are synchronised
    /// by the manager. A distributed-assembled backend (MUMPS
    /// `ICNTL(18) = 3`) takes every rank's partial matrix in place
    /// on any number of processes (upstream `_fully_distributed` in an MPI
    /// build): [`SparseDirectSolver::solve_distributed`] (centralised RHS on
    /// rank 0, owned solution rows scattered back, re-use decisions agreed
    /// by the backend).
    fn solve(
        &mut self,
        id: &str,
        ctx: &mut BackendContext<'_>,
        rhs: &[Real],
        solution: &mut [Real],
    ) -> Result<()> {
        let (rank, size) = ctx.phases.rank_size();
        if let Some(solver) = self.solvers.get_mut(id) {
            // D-041 (4): the manager's switches reach every solver, whenever
            // it was (re)allocated; identical on every rank (and part of a
            // MUMPS solve's agreed options descriptor).
            solver.set_upstream_compat(self.sparse_compat);
        }
        if let Some(solver) = self.solvers.get_mut(id)
            && solver.capabilities().matrix == MatrixDistribution::DistributedAssembled
        {
            let a = self
                .matrices
                .get(id)
                .ok_or_else(|| Error::not_found("matrix", id));
            return solver.solve_distributed(
                ctx.phases,
                &format!("aij:solve:{id}"),
                ctx.layout,
                a,
                rhs,
                solution,
            );
        }
        if size > 1 {
            return self.solve_centralized(id, ctx, rank, rhs, solution);
        }
        let prepared = (|| -> Result<(Vec<Real>, Vec<GlobalIdx>)> {
            let a = self.get(id)?;
            let solver = self
                .solvers
                .get(id)
                .ok_or_else(|| Error::not_found("sparse solver for matrix", id))?;
            serial_capacity_check(&solver.capabilities(), a)?;
            let n = usize::try_from(a.size())
                .map_err(|_| Error::invalid("matrix order exceeds the address space"))?;
            let layout = ctx.layout;
            if rhs.len() != layout.nb_local() || solution.len() != layout.nb_local() {
                return Err(Error::DimensionMismatch {
                    context: "AijBackend::solve",
                    expected: layout.nb_local(),
                    got: rhs.len().min(solution.len()),
                });
            }
            let mut b = vec![0.0; n];
            let mut owned = Vec::with_capacity(layout.nb_owned());
            for (l, &r) in rhs.iter().enumerate() {
                if layout.is_owned(l)
                    && let Some(g) = layout.global(l)
                {
                    let gi = usize::try_from(g)
                        .ok()
                        .filter(|&gi| gi < n)
                        .ok_or_else(|| Error::invalid("owned equation outside the matrix"))?;
                    b[gi] = r;
                    owned.push(g);
                }
            }
            Ok((b, owned))
        })();
        let a = self.matrices.get(id);
        let solver = self.solvers.get_mut(id);
        let cache = self
            .analysed
            .entry(id.into())
            .or_insert(((0, MatrixType::NotDefined), 0));
        let x = ctx
            .phases
            .run(&format!("aij:solve:{id}"), prepared, |_, (b, owned)| {
                // Serial: no communication.
                let (Some(a), Some(solver)) = (a, solver) else {
                    return Ok(Err(Error::not_found("matrix", id)));
                };
                let r = (|| -> Result<Vec<(GlobalIdx, Real)>> {
                    let key = (a.profile_release(), a.matrix_type());
                    if cache.0 != key {
                        solver.analysis(a)?;
                        cache.0 = key;
                        cache.1 = 0;
                    }
                    if cache.1 != a.value_release() {
                        solver.factorize(a)?;
                        cache.1 = a.value_release();
                    }
                    let mut x = vec![0.0; b.len()];
                    solver.solve(&b, &mut x)?;
                    Ok(owned
                        .iter()
                        .map(|&g| (g, x[usize::try_from(g).unwrap_or(0)]))
                        .collect())
                })();
                Ok(r)
            })?;
        for (g, v) in x {
            solution[ctx.layout.local(g)?] = v;
        }
        Ok(())
    }
    fn aij(&self, id: &str) -> Result<&SparseMatrixAij> {
        self.get(id)
    }
    fn aij_mut(&mut self, id: &str) -> Result<&mut SparseMatrixAij> {
        self.get_mut(id)
    }
    fn value_release(&self, id: &str) -> Result<u64> {
        Ok(self.get(id)?.value_release())
    }
    fn copy_profile(&mut self, dst: &str, src: &str) -> Result<()> {
        if dst == src {
            // Upstream copies a matrix's own profile onto itself: values
            // zeroed, value release incremented.
            let m = self.get_mut(dst)?;
            let copy = m.clone();
            m.copy_profile(&copy);
            return Ok(());
        }
        // No copy of the source (it would double the matrix memory for the
        // duration of the call): take the destination out of the map.
        self.get(src)?;
        let mut d = self
            .matrices
            .remove(dst)
            .ok_or_else(|| Error::not_found("matrix", dst))?;
        d.copy_profile_with(&self.exec, &self.matrices[src]);
        self.matrices.insert(dst.into(), d);
        Ok(())
    }
    fn add_matrix(&mut self, dst: &str, src: &str, alpha: Real) -> Result<()> {
        if dst == src {
            // upstream `K.add(K, alpha)`: every entry
            // reads its own value before writing it, i.e. `K ← (1 + α) K`
            // entry by entry; adding a copy gives the same sums
            let copy = self.get(src)?.clone();
            let policy = self.add_policy;
            let d = self.get_mut(dst)?;
            return match policy {
                MatrixAddPolicy::MirrorSymmetricSource => d.add_matrix(&copy, alpha),
                MatrixAddPolicy::Upstream => d.add_matrix_upstream(&copy, alpha),
            };
        }
        self.get(src)?;
        let mut d = self
            .matrices
            .remove(dst)
            .ok_or_else(|| Error::not_found("matrix", dst))?;
        let r = {
            let s = &self.matrices[src];
            match self.add_policy {
                MatrixAddPolicy::MirrorSymmetricSource => d.add_matrix_with(&self.exec, s, alpha),
                MatrixAddPolicy::Upstream => d.add_matrix_upstream_with(&self.exec, s, alpha),
            }
        };
        self.matrices.insert(dst.into(), d);
        r
    }
    fn copy_matrix(&mut self, id: &str, src: &str) -> Result<()> {
        if self.matrices.contains_key(id) {
            return Err(Error::AlreadyExists {
                kind: "matrix",
                id: id.into(),
            });
        }
        let m = SparseMatrixAij::copy_with_id(self.get(src)?, id);
        self.matrices.insert(id.into(), m);
        Ok(())
    }
    fn set_sparse_solver(&mut self, id: &str, solver: Box<dyn SparseDirectSolver>) -> Result<()> {
        self.set_solver(id, solver);
        Ok(())
    }
    fn sparse_solver_kind(&self, id: &str) -> Option<SparseSolverType> {
        self.solvers.get(id).map(|s| s.kind())
    }
    fn sparse_solver_statistics(&self, id: &str) -> Vec<(&'static str, Real)> {
        self.solvers
            .get(id)
            .map(|s| s.statistics())
            .unwrap_or_default()
    }
    fn release_solvers(&mut self) {
        for s in self.solvers.values_mut() {
            s.destroy_internal_data();
        }
        self.analysed.clear();
        self.central.clear();
    }
    fn set_sparse_solver_compat(&mut self, compat: SparseSolverCompat) {
        self.sparse_compat = compat;
    }
    fn centralizes_sparse_solves(&self) -> bool {
        true
    }
}
