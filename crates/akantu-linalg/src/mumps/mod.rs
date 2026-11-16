// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/solver/sparse_solver_mumps.hh,
// src/solver/sparse_solver_mumps.cc (initialize, initMumpsData, analysis,
// factorize, solve/solveInternal, printError, mumpsDataDestroy),
// src/solver/sparse_solver.hh (parallel method), and the MUMPS parts of
// src/model/common/dof_manager/dof_manager_default.cc (distributed
// assembled input, centralised right-hand side and solution).

//! MUMPS sparse direct solver (SOLV-002/003; feature `mumps`).
//!
//! Upstream `SparseSolverMumps` in an MPI build (`_fully_distributed`, on
//! any number of processes):
//!
//! * **input**: distributed assembled matrix (`ICNTL(18) = 3`): every rank
//!   passes its rank-partial AIJ entries (`irn_loc`, `jcn_loc`, `a_loc`,
//!   1-based global equations, upstream storage order); MUMPS sums
//!   duplicates, also across ranks;
//! * **right-hand side / solution**: centralised dense on the host
//!   (`ICNTL(20) = ICNTL(21) = 0`): the owned rows of every rank gathered to
//!   rank 0 (`SolverVectorDistributed::getGlobalVector`), the solution
//!   scattered back to the owned rows (`setGlobalVector`); the DOF manager
//!   synchronises shared rows (`akantu_linalg::centralize`);
//! * **controls** as upstream: `par = 1` (host works), `sym = 2` for a
//!   matrix whose **effective** type is symmetric, else `0` (re-initialised
//!   when a UD-034 promotion changes it; upstream never re-initialises),
//!   `ICNTL(5) = 0` (assembled), `ICNTL(8) = 77` (automatic scaling),
//!   `ICNTL(28) = 0` (automatic sequential/parallel analysis), every other
//!   control at the MUMPS default (ordering `ICNTL(7) = 7`, automatic),
//!   output streams off (`ICNTL(1..4) = 0`; upstream's debug builds print);
//!   [`MumpsOptions`] can override any `ICNTL`/`CNTL` (port extension,
//!   recorded in [`MumpsReport`]);
//! * **phases**: analysis (`JOB = 1`) when the matrix's profile identity
//!   ([`SparseMatrixAij::profile_identity`]: process-unique per matrix
//!   object and profile, so another matrix with colliding release counters
//!   is never taken for the analysed one), the
//!   effective symmetry, the order or the entry count changed on **any**
//!   rank; factorisation (`JOB = 2`) when the value release changed on any
//!   rank; solve (`JOB = 3`) every time — the re-use decision is agreed
//!   (upstream compares each rank's releases locally, and binds its solver
//!   to one matrix id);
//! * **errors** (SOLV-003, upstream `printError`): `INFOG(1)` (global, the
//!   same on every rank) after every job; `-10` (numerically singular) and
//!   `-6` (structurally singular) → [`Error::LinearSolver`] with upstream's
//!   "Solver encountered singular matrix"; `-9` (main workarray too small)
//!   → workspace retry; any other negative code → [`Error::Backend`]
//!   (`backend = "mumps"`, `code = INFOG(1)`, with `INFOG(2)`); non-finite
//!   input is refused before MUMPS ([`Error::Numerical`]), a non-finite
//!   solution is an arithmetic range failure ([`Error::Numerical`]). A
//!   failed analysis or factorisation invalidates the factors (the next
//!   solve redoes them); the solution is written only on success. Upstream
//!   checks only the factorisation and the solve (an analysis failure
//!   surfaces as `-3` at the factorisation, **UD-264**, category B,
//!   corrected without a compat switch) and reports `INFO(1)` reduced with
//!   `min`, which equals `INFOG(1)` for errors;
//! * **workspace retry** (`INFOG(1) = -9` at the factorisation or the
//!   solve): `ICNTL(14) += 10` (kept for later solves) and the analysis,
//!   factorisation and solve are redone, until `ICNTL(14)` reaches 90 (then
//!   upstream's "The MUMPS workarray is too small" error). **UD-216 fixed
//!   by default**: every solve starts from a fresh copy of the original
//!   right-hand side. Upstream retries by calling `solve()` recursively from
//!   inside `factorize()`; the outer `solveInternal()` then solves again
//!   with the inner call's *solution* as right-hand side, returning
//!   `A^-(k+1) b` after `k` factorisation retries
//!   (`sparse_solver_mumps.cc:249-347`). [`RetryRhs::UpstreamSolution`]
//!   reproduces that for differential runs; the DOF manager selects it
//!   manager-wide (`akantu_solver::UpstreamCompat`, D-041, through
//!   [`SparseDirectSolver::set_upstream_compat`]). Upstream also stops only
//!   when `ICNTL(14)` hits 90 exactly; the port stops at `>= 90`, so a
//!   user-chosen start that is not `≡ 0 (mod 10)` terminates too
//!   (**UD-263**, category B).
//!
//! **Collective agreement** (precondition of every native job):
//! no rank decides alone to initialise, analyse, factorise, retry or solve
//! again. Before any native job of a solve, one all-gather agrees (a) the
//! re-use wishes and the effective symmetry, (b) the **binding**: every
//! rank's instance is bound to *this* communicator (identity,
//! `Communicator::is_same`: a duplicate, a split, a reordered or a
//! sub-group communicator is another communicator) and is the *same*
//! native instance (an identity token agreed at its initialisation), and
//! (c) a canonical descriptor of every control-flow setting: each
//! `ICNTL`/`CNTL` override, [`RetryRhs`], the retry ceiling and the
//! persisted `ICNTL(14)`. Any difference is refused on every rank with an
//! ordinary error before MUMPS runs, and the solver stays usable. After
//! every native job, `(INFOG(1), INFOG(2), ICNTL(14))` is all-gathered and
//! checked identical (MUMPS makes them global) before it decides anything;
//! a disagreement — a broken library contract — is an agreed
//! [`Error::Fatal`] and the instance is abandoned (never destroyed
//! collectively). Every native call also carries its job identity in the
//! native header (`Communicator::native_collective`).
//!
//! **Buffers handed to MUMPS**: the coordinates
//! (`IRN_loc`/`JCN_loc`) exist only together with the key of the analysis
//! that saw them; coordinates converted for a new analysis stay local to
//! their solve until its analysis job starts, so a refusal between the
//! conversion and the jobs (e.g. of the right-hand-side gather) leaves the
//! stored analysis and its own coordinates intact. Before every native job
//! the buffers are checked against the matrix (`nnz` coordinates and values,
//! the host's `n` right-hand-side entries) as the **local validation of the
//! native header**, so a mismatch on any rank is refused on every rank
//! before MUMPS runs; a solve that re-uses the analysis also checks it in
//! the agreed coordinates step.
//!
//! **MPI** (`docs/ARCHITECTURE.md`): every MUMPS job is a blocking native
//! collective run through `Communicator::native_collective` inside the
//! solve's agreed phase scope (thread policy checked, `Serialized`
//! admission as a split-class operation, no progress deadline). Under
//! `MPI_THREAD_MULTIPLE` native jobs of one process never overlap either:
//! a native job while another thread's is in flight is refused on every
//! rank (ordinary error; retry, or drive native solvers from one thread).
//! The
//! instance is bound to the communicator of its successful initialisation
//! and holds a clone of it (the MPI environment guard), so MPI is
//! finalised only after the instance is destroyed. **Rebinding** is
//! explicit: [`MumpsSolver::destroy`] (collective on the bound
//! communicator), then a solve on the new communicator initialises there;
//! a solve on another communicator without it is refused (above).
//! Destruction (`JOB = -2`) is a blocking collective too (measured: rank 0
//! waits for its peers): [`MumpsSolver::destroy`] and a re-initialisation
//! run it agreed and keep the instance if it is refused; an initialised
//! instance is never freed without it — dropping one queues it through
//! `Communicator::release_native`: drop the solver on MPI's main thread, at
//! the same point of the program on every rank, with no other thread inside
//! MPI (upstream's destructor has the same obligation; under `Funneled` a
//! drop on a worker is deferred to the main thread's next lifecycle drain).
//! **Broken communicator** (a transport failure was
//! observed on this rank, `Communicator::broken`): no `JOB = -2` is
//! attempted — dropping the solver, [`MumpsSolver::destroy`] (which returns
//! `Error::Fatal { kind: Transport }`) and a solve whose jobs step failed
//! by transport abandon the instance (its memory is leaked; the job aborts
//! per `docs/ARCHITECTURE.md`). A failed `JOB = -1` frees the
//! structure without `JOB = -2` (MUMPS may already have duplicated its
//! communicator: a rare leak). The MUMPS backend needs an MPI communicator: on a serial or
//! in-process communicator the solve fails on every rank with
//! [`Error::Unsupported`] (`SparseSolverFactory::resolve_available` keeps
//! `auto` on Eigen there). The sequential MUMPS library (upstream
//! `_not_parallel`, `ICNTL(18) = 0`) is not linked.
//!
//! Bitwise identity with the oracle's MUMPS solves is an *observed* result
//! for a pinned configuration (same platform, same MUMPS/ScaLAPACK/OpenBLAS
//! shared objects, same OpenBLAS core type and thread count, same input
//! order, 1 rank; `docs/NUMERICAL_VALIDATION.md`
//! and "MUMPS fixes 2"; D-050), never a promise; multi-rank
//! MUMPS solves are not run-to-run reproducible.

// The MUMPS FFI edge (opaque C structure behind the shim in `shim.c`):
// every `unsafe` block carries a `// SAFETY:` comment.
#![allow(unsafe_code)]

use std::collections::BTreeMap;
use std::ffi::{CStr, c_char, c_int, c_void};
use std::ptr::NonNull;
use std::sync::atomic::{AtomicU64, Ordering};

use akantu_comm::phase::{Phases, Severity};
use akantu_comm::{Communicator, NativeComm};
use akantu_core::enums::{MatrixType, SparseSolverType};
use akantu_core::{Error, FatalKind, GlobalIdx, Real, Result};

use crate::centralize;
use crate::direct::{
    BackendCapabilities, MatrixDistribution, RhsDistribution, SolverParallelMethod,
    SparseDirectSolver, SparseSolverCompat, arithmetic_range_failure, mumps_symmetry,
    singular_matrix,
};
use crate::layout::EquationLayout;
use crate::sparse::SparseMatrixAij;

// The shim's C signatures (`src/mumps/shim.c`): `akantu_mumps *` is an
// opaque heap object (`*mut c_void` here), `int` is `c_int`, `int64_t` is
// `i64`, `double` is `f64`. Indices are 1-based and validated on the Rust
// side before every call.
unsafe extern "C" {
    fn akantu_mumps_alloc() -> *mut c_void;
    fn akantu_mumps_free(m: *mut c_void);
    fn akantu_mumps_set_init(m: *mut c_void, comm_fortran: c_int, sym: c_int, par: c_int);
    fn akantu_mumps_job(m: *mut c_void, job: c_int) -> c_int;
    fn akantu_mumps_set_icntl(m: *mut c_void, i: c_int, v: c_int);
    fn akantu_mumps_get_icntl(m: *const c_void, i: c_int) -> c_int;
    fn akantu_mumps_set_cntl(m: *mut c_void, i: c_int, v: f64);
    fn akantu_mumps_get_cntl(m: *const c_void, i: c_int) -> f64;
    fn akantu_mumps_infog(m: *const c_void, i: c_int) -> c_int;
    fn akantu_mumps_rinfog(m: *const c_void, i: c_int) -> f64;
    fn akantu_mumps_set_matrix(
        m: *mut c_void,
        n: c_int,
        nnz_loc: i64,
        irn_loc: *const c_int,
        jcn_loc: *const c_int,
        a_loc: *const f64,
    );
    fn akantu_mumps_clear_matrix(m: *mut c_void);
    fn akantu_mumps_set_rhs(m: *mut c_void, rhs: *mut f64, n: c_int);
    fn akantu_mumps_version(m: *const c_void) -> *const c_char;
    fn akantu_mumps_int_bytes() -> c_int;
    fn akantu_mumps_blas_corename() -> *const c_char;
    fn akantu_mumps_blas_threads() -> c_int;
}

/// The BLAS kernels MUMPS runs with, as OpenBLAS reports them (D-050
/// provenance): MUMPS solves are bitwise reproducible across
/// runs and against the oracle only for the same platform, shared objects,
/// OpenBLAS core type and thread count.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BlasInfo {
    /// `openblas_get_corename()` (e.g. `Haswell`), `None` if the BLAS is
    /// not OpenBLAS.
    pub core: Option<String>,
    /// `openblas_get_num_threads()`, `None` if the BLAS is not OpenBLAS.
    pub threads: Option<i32>,
}

/// The [`BlasInfo`] of this process.
#[must_use]
pub fn blas_info() -> BlasInfo {
    // SAFETY: shim functions without preconditions: a dlsym lookup in the
    // global scope and, if found, OpenBLAS's own getters; the core name is
    // a static NUL-terminated string of OpenBLAS (or null), copied out.
    let (core, threads) = unsafe {
        let p = akantu_mumps_blas_corename();
        let core = if p.is_null() {
            None
        } else {
            Some(CStr::from_ptr(p).to_string_lossy().into_owned())
        };
        (core, akantu_mumps_blas_threads())
    };
    BlasInfo {
        core,
        threads: (threads >= 0).then_some(threads),
    }
}

/// MUMPS jobs used by upstream (`SolverMumpsJob`).
const JOB_INIT: c_int = -1;
const JOB_DESTROY: c_int = -2;
const JOB_ANALYSE: c_int = 1;
const JOB_FACTORIZE: c_int = 2;
const JOB_SOLVE: c_int = 3;

/// Array lengths of `DMUMPS_STRUC_C` (MUMPS 5.x): `icntl[60]`, `cntl[15]`,
/// `infog[80]`, `rinfog[40]`.
const NB_ICNTL: usize = 60;
const NB_CNTL: usize = 15;
const NB_INFOG: usize = 80;
const NB_RINFOG: usize = 40;

/// Upstream's workspace-retry ceiling: `ICNTL(14)` is increased by 10 per
/// retry and the retry stops when it reaches 90 (`sparse_solver_mumps.cc:337-351`).
const ICNTL14_STEP: i32 = 10;
const ICNTL14_CEILING: i32 = 90;

/// Right-hand side of the solves that follow a workspace retry (UD-216).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RetryRhs {
    /// Corrected (default): every solve starts from a fresh copy of the
    /// original right-hand side.
    #[default]
    Original,
    /// Upstream compatibility (UD-216): after `k` factorisation retries the
    /// returned vector is `A^-(k+1) b`, as upstream's recursive retry
    /// computes (`sparse_solver_mumps.cc:249-347`). For differential runs
    /// only.
    UpstreamSolution,
}

/// Options of a [`MumpsSolver`] (port extensions; the defaults are
/// upstream's behaviour).
///
/// Setting `ICNTL` / `CNTL` values goes beyond upstream's interface:
/// upstream's `SparseSolverMumps` keeps its control accessor private.
/// Refused before MUMPS runs, with the same error on
/// every rank: indices outside `1..=60` / `1..=15`, a non-finite `CNTL`,
/// `ICNTL(5)`, `(18)`, `(20)`, `(21)` other than upstream's values, the
/// values that make MUMPS read or write user arrays the backend does not
/// provide (`ICNTL(7) = 1`, `(8) = -1`, `(15) > 0`, `(19) ≠ 0`,
/// `(25) < 0`, `(26) ≠ 0`, `(30) ≠ 0`, `(32) ≠ 0`) and a non-zero
/// value of an index MUMPS 5.7.3 reserves (`40–47`, `50–55`, `57`, `59`,
/// `60`). The manual's MUMPS chapter lists them.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MumpsOptions {
    /// `ICNTL(i)` overrides (1-based `i ≤ 60`), applied after upstream's
    /// values before every analysis (and after `JOB = -1`). `ICNTL(14)` set
    /// here is the start of the workspace retry.
    pub icntl: BTreeMap<usize, i32>,
    /// `CNTL(i)` overrides (1-based `i ≤ 15`), applied like `icntl`.
    pub cntl: BTreeMap<usize, f64>,
    /// UD-216 behaviour.
    pub retry_rhs: RetryRhs,
    /// Workspace-retry ceiling: retry while the increased `ICNTL(14)` is
    /// below it (`None`: upstream's 90).
    pub icntl14_ceiling: Option<i32>,
}

/// What the last solve did (for diagnostics, tests and the scaling note).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MumpsReport {
    /// `version_number` of the linked MUMPS.
    pub version: String,
    /// `sym` of the instance (0 or 2).
    pub sym: i32,
    /// Number of processes of the instance's communicator.
    pub nb_proc: usize,
    /// Jobs run by the last solve, in order (`1`, `2`, `3`; `-1`/`-2` for
    /// (re-)initialisation).
    pub jobs: Vec<i32>,
    /// Workspace retries (`INFOG(1) = -9`) of the last solve.
    pub retries: usize,
    /// Factorisation retries among them (UD-216 compat count).
    pub factorization_retries: usize,
    /// `ICNTL(14)` after the last solve.
    pub icntl14: i32,
    /// `ICNTL(i)` values in force at the last analysis (all 60).
    pub icntl: Vec<i32>,
    /// `INFOG(1..=80)` after the last job.
    pub infog: Vec<i32>,
    /// `RINFOG(1..=40)` after the last job.
    pub rinfog: Vec<f64>,
}

impl MumpsReport {
    /// `INFOG(i)` (1-based), 0 if not recorded.
    #[must_use]
    pub fn infog(&self, i: usize) -> i32 {
        i.checked_sub(1)
            .and_then(|k| self.infog.get(k))
            .copied()
            .unwrap_or(0)
    }

    /// `RINFOG(i)` (1-based), 0 if not recorded.
    #[must_use]
    pub fn rinfog(&self, i: usize) -> f64 {
        i.checked_sub(1)
            .and_then(|k| self.rinfog.get(k))
            .copied()
            .unwrap_or(0.0)
    }
}

/// Process-wide counts of MUMPS instance lifecycle events (diagnostics;
/// the regression tests check that every successful
/// native initialisation is matched by one native destruction).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct InstanceCounts {
    /// Shim structures allocated.
    pub allocated: u64,
    /// Shim structures freed.
    pub freed: u64,
    /// Successful `JOB = -1` (native initialisations).
    pub initialized: u64,
    /// `JOB = -2` calls (native destructions).
    pub destroyed: u64,
    /// Every native job (`dmumps_c` call), whatever its outcome.
    pub jobs: u64,
    /// Initialised instances given up without `JOB = -2` (ranks disagreed
    /// on a global outcome, or the bound communicator is broken); their
    /// MUMPS memory is leaked on purpose.
    pub abandoned: u64,
    /// High-water mark of native jobs running **at the same time** in this
    /// process (at most 1 at every thread level).
    pub max_concurrent_jobs: u64,
    /// Native jobs running right now in this process (diagnostics: lets a
    /// test start a contending operation while a job is in flight).
    pub jobs_in_flight: u64,
}

static COUNT_ALLOCATED: AtomicU64 = AtomicU64::new(0);
static COUNT_FREED: AtomicU64 = AtomicU64::new(0);
static COUNT_INITIALIZED: AtomicU64 = AtomicU64::new(0);
static COUNT_DESTROYED: AtomicU64 = AtomicU64::new(0);
static COUNT_JOBS: AtomicU64 = AtomicU64::new(0);
static COUNT_ABANDONED: AtomicU64 = AtomicU64::new(0);
static JOBS_IN_FLIGHT: AtomicU64 = AtomicU64::new(0);
static MAX_JOBS_IN_FLIGHT: AtomicU64 = AtomicU64::new(0);

/// The process-wide [`InstanceCounts`] (queued destructions are counted
/// when they run).
#[must_use]
pub fn instance_counts() -> InstanceCounts {
    InstanceCounts {
        allocated: COUNT_ALLOCATED.load(Ordering::SeqCst),
        freed: COUNT_FREED.load(Ordering::SeqCst),
        initialized: COUNT_INITIALIZED.load(Ordering::SeqCst),
        destroyed: COUNT_DESTROYED.load(Ordering::SeqCst),
        jobs: COUNT_JOBS.load(Ordering::SeqCst),
        abandoned: COUNT_ABANDONED.load(Ordering::SeqCst),
        max_concurrent_jobs: MAX_JOBS_IN_FLIGHT.load(Ordering::SeqCst),
        jobs_in_flight: JOBS_IN_FLIGHT.load(Ordering::SeqCst),
    }
}

/// Run one `dmumps_c` call (`call`) counting it in the concurrency
/// high-water mark ([`InstanceCounts::max_concurrent_jobs`]).
fn in_flight<R>(call: impl FnOnce() -> R) -> R {
    let now = JOBS_IN_FLIGHT.fetch_add(1, Ordering::SeqCst) + 1;
    MAX_JOBS_IN_FLIGHT.fetch_max(now, Ordering::SeqCst);
    let r = call();
    JOBS_IN_FLIGHT.fetch_sub(1, Ordering::SeqCst);
    r
}

/// Count a native job that returned `infog1`.
fn count_job(job: c_int, infog1: i32) {
    COUNT_JOBS.fetch_add(1, Ordering::SeqCst);
    match job {
        JOB_INIT if infog1 >= 0 => {
            COUNT_INITIALIZED.fetch_add(1, Ordering::SeqCst);
        }
        JOB_DESTROY => {
            COUNT_DESTROYED.fetch_add(1, Ordering::SeqCst);
        }
        _ => {}
    }
}

/// An allocated MUMPS instance (the C heap object of the shim) and, once
/// `JOB = -1` succeeded, the communicator it is bound to.
#[derive(Debug)]
struct Instance {
    ptr: NonNull<c_void>,
    /// The communicator of the successful `JOB = -1`: the MPI environment
    /// guard and the identity every later job is checked against.
    /// `Some` exactly while a native `JOB = -2` is owed.
    bound: Option<Communicator>,
    /// Agreed identity of the native instance: identical on every rank of
    /// `bound`, distinct for every initialisation on it (0 while unbound).
    token: u64,
}

// SAFETY: the instance is plain heap memory owned by exactly one
// `MumpsSolver` (no aliasing, no `Clone`); MUMPS itself is not bound to a
// thread — which threads may enter MPI through it is governed by the MPI
// thread level, enforced at every job by `Communicator::native_collective`
// (and by the lifecycle for the destruction). `Communicator` is `Send`.
unsafe impl Send for Instance {}

/// Source of instance tokens (rank 0 of the initialising communicator
/// proposes one; every rank adopts it through the agreed outcome).
static NEXT_TOKEN: AtomicU64 = AtomicU64::new(1);

impl Instance {
    fn alloc() -> Result<Self> {
        // SAFETY: shim allocator (`calloc`), no preconditions; null is
        // checked below.
        let p = unsafe { akantu_mumps_alloc() };
        if !p.is_null() {
            COUNT_ALLOCATED.fetch_add(1, Ordering::SeqCst);
        }
        NonNull::new(p)
            .map(|ptr| Self {
                ptr,
                bound: None,
                token: 0,
            })
            .ok_or(Error::CapacityExceeded {
                what: "MUMPS instance allocation",
                requested: 1,
                limit: 0,
            })
    }

    fn raw(&self) -> *mut c_void {
        self.ptr.as_ptr()
    }

    /// `JOB = -1` succeeded and no `JOB = -2` ran since.
    fn initialized(&self) -> bool {
        self.bound.is_some()
    }

    /// Run `job` (the caller is inside a native collective of the
    /// instance's communicator whose header agreed the job on every rank).
    /// Returns `INFOG(1)`.
    fn job(&mut self, job: c_int) -> i32 {
        // SAFETY: `ptr` is a live shim instance; the fields read by `job`
        // were set by the caller (init fields before JOB = -1; matrix
        // pointers valid for the duration of JOB = 1/2; RHS valid during
        // JOB = 3); every process of the instance's communicator runs the
        // same job: the agreed plan checked that every rank's instance is
        // the same native instance bound to the solve's communicator, every
        // decision derives from agreed values, and the native header
        // compared the job identity.
        let infog1 = in_flight(|| unsafe { akantu_mumps_job(self.raw(), job) });
        count_job(job, infog1);
        infog1
    }

    fn set_icntl(&mut self, i: usize, v: i32) {
        assert!(
            (1..=NB_ICNTL).contains(&i),
            "ICNTL({i}) outside 1..={NB_ICNTL}"
        );
        // SAFETY: live instance; `i` in 1..=60 (asserted above, always on:
        // every caller passes a constant or a key validated by
        // `validate_options`).
        unsafe { akantu_mumps_set_icntl(self.raw(), i as c_int, v) }
    }

    fn icntl(&self, i: usize) -> i32 {
        assert!(
            (1..=NB_ICNTL).contains(&i),
            "ICNTL({i}) outside 1..={NB_ICNTL}"
        );
        // SAFETY: live instance; `i` in 1..=60 (asserted above).
        unsafe { akantu_mumps_get_icntl(self.raw(), i as c_int) }
    }

    fn set_cntl(&mut self, i: usize, v: f64) {
        assert!(
            (1..=NB_CNTL).contains(&i),
            "CNTL({i}) outside 1..={NB_CNTL}"
        );
        // SAFETY: live instance; `i` in 1..=15 (asserted above; keys
        // validated by `validate_options`).
        unsafe { akantu_mumps_set_cntl(self.raw(), i as c_int, v) }
    }

    #[allow(dead_code)] // diagnostics accessor kept next to its setter
    fn cntl(&self, i: usize) -> f64 {
        assert!(
            (1..=NB_CNTL).contains(&i),
            "CNTL({i}) outside 1..={NB_CNTL}"
        );
        // SAFETY: live instance; `i` in 1..=15 (asserted above).
        unsafe { akantu_mumps_get_cntl(self.raw(), i as c_int) }
    }

    fn infog(&self, i: usize) -> i32 {
        assert!(
            (1..=NB_INFOG).contains(&i),
            "INFOG({i}) outside 1..={NB_INFOG}"
        );
        // SAFETY: live instance; `i` in 1..=80 (asserted above).
        unsafe { akantu_mumps_infog(self.raw(), i as c_int) }
    }

    fn rinfog(&self, i: usize) -> f64 {
        assert!(
            (1..=NB_RINFOG).contains(&i),
            "RINFOG({i}) outside 1..={NB_RINFOG}"
        );
        // SAFETY: live instance; `i` in 1..=40 (asserted above).
        unsafe { akantu_mumps_rinfog(self.raw(), i as c_int) }
    }

    /// Hand a distributed matrix to the instance: order `n`, this rank's
    /// `coords` and, for a factorisation, `values` (null pointers when the
    /// rank has no entries). The caller checked (agreed in the native
    /// header) that `coords` and `values` hold exactly `nnz` entries each;
    /// the buffers stay alive and unmodified until [`Instance::clear_matrix`]
    /// right after the job that reads them.
    fn set_matrix(&mut self, n: c_int, coords: &Coordinates, values: Option<&[Real]>) {
        let nnz = coords.irn.len();
        // Always on (once per job): `matrix_job` sets
        // the matrix only after `Coordinates::check` succeeded.
        assert!(
            coords.jcn.len() == nnz && values.is_none_or(|v| v.len() == nnz),
            "MUMPS coordinates and values of different lengths"
        );
        let empty = nnz == 0;
        let irn = if empty {
            std::ptr::null()
        } else {
            coords.irn.as_ptr()
        };
        let jcn = if empty {
            std::ptr::null()
        } else {
            coords.jcn.as_ptr()
        };
        let av = match values {
            Some(v) if !empty => v.as_ptr(),
            _ => std::ptr::null(),
        };
        // SAFETY: live instance; `irn`/`jcn` point to the `nnz` 1-based
        // `MUMPS_INT` coordinates of `coords` and `av` to `nnz` values (or
        // are null when empty / unused) — lengths equal by the caller's
        // agreed check, asserted above (always on). The caller keeps both buffers alive
        // and unmodified until `clear_matrix` right after the one job that
        // reads them; MUMPS does not write through them.
        unsafe {
            akantu_mumps_set_matrix(
                self.raw(),
                n,
                i64::try_from(nnz).unwrap_or(i64::MAX),
                irn,
                jcn,
                av,
            );
        }
    }

    /// Forget the matrix pointers (order and counts are kept: MUMPS checks
    /// N at later jobs).
    fn clear_matrix(&mut self) {
        // SAFETY: live instance; resets the matrix pointers to null.
        unsafe { akantu_mumps_clear_matrix(self.raw()) };
    }

    fn version(&self) -> String {
        // SAFETY: live instance; `version_number` is a NUL-terminated
        // character array inside the structure (zero-initialised by
        // `calloc`, filled by JOB = -1), valid while the instance lives; it
        // is copied out before returning.
        unsafe {
            let p = akantu_mumps_version(self.raw());
            if p.is_null() {
                String::new()
            } else {
                CStr::from_ptr(p).to_string_lossy().into_owned()
            }
        }
    }

    /// Give up the instance without any native call: MUMPS's allocations
    /// and the C structure are leaked on purpose (used when the ranks
    /// disagree on a global outcome, so a collective `JOB = -2` could
    /// hang). The bound communicator (environment guard) is released.
    fn abandon(mut self) {
        self.bound = None;
        COUNT_ABANDONED.fetch_add(1, Ordering::SeqCst);
        std::mem::forget(self);
    }
}

impl Drop for Instance {
    fn drop(&mut self) {
        let ptr = self.ptr;
        match self.bound.take() {
            None => {
                // Never initialised, or its JOB = -2 already ran.
                // SAFETY: the pointer came from `akantu_mumps_alloc` and is
                // freed exactly once (unique owner; `abandon` forgets the
                // instance instead of dropping it).
                unsafe { akantu_mumps_free(ptr.as_ptr()) }
                COUNT_FREED.fetch_add(1, Ordering::SeqCst);
            }
            Some(comm) if comm.broken().is_some() => {
                // The bound communicator is broken (a transport failure was
                // observed on this rank): the blocking collective JOB = -2
                // could wait for ever for peers that are aborting
                // (`docs/ARCHITECTURE.md`: a transport failure aborts
                // the job). Abandon: MUMPS's memory and the C structure are
                // leaked, no native call is made.
                COUNT_ABANDONED.fetch_add(1, Ordering::SeqCst);
                drop(comm);
            }
            Some(comm) => {
                // An initialised instance is never freed without its
                // collective JOB = -2: queued through the
                // lifecycle on its own communicator.
                let q = QueuedInstance(ptr);
                let destroy = move || {
                    let q = q;
                    // SAFETY: the instance was initialised and is uniquely
                    // owned by this closure; JOB = -2 then free, once. The
                    // lifecycle runs the closure on a thread allowed to call
                    // MPI (under the `Serialized` lock) before MPI_Finalize;
                    // every rank queues its instance's destruction at the
                    // same program point (documented obligation of dropping
                    // a solver).
                    let infog1 = unsafe {
                        let infog1 = in_flight(|| akantu_mumps_job(q.0.as_ptr(), JOB_DESTROY));
                        akantu_mumps_free(q.0.as_ptr());
                        infog1
                    };
                    count_job(JOB_DESTROY, infog1);
                    COUNT_FREED.fetch_add(1, Ordering::SeqCst);
                };
                // `false` only for a non-MPI communicator, impossible for a
                // bound instance: the C memory is then leaked rather than
                // touched without MPI.
                let _queued = comm.release_native(destroy);
            }
        }
    }
}

/// Raw instance pointer moved into a queued destruction closure.
struct QueuedInstance(NonNull<c_void>);

// SAFETY: as `Instance` (unique ownership moves into the closure; the
// lifecycle runs it on a thread allowed to call MPI, before finalisation).
unsafe impl Send for QueuedInstance {}

/// Analysis key: `(profile identity, effective type, order, local
/// entries)`. The profile identity
/// ([`SparseMatrixAij::profile_identity`]) is process-unique per matrix
/// object and profile, so a different matrix with colliding release
/// counters is never taken for the analysed one.
type AnalysisKey = (u64, MatrixType, GlobalIdx, usize);

/// Factorisation key: `(profile identity, value release)`.
type FactorKey = (u64, u64);

fn analysis_key(a: &SparseMatrixAij) -> AnalysisKey {
    (a.profile_identity(), a.matrix_type(), a.size(), a.nnz())
}

fn factor_key(a: &SparseMatrixAij) -> FactorKey {
    (a.profile_identity(), a.value_release())
}

/// The 1-based `MUMPS_INT` coordinates of one rank-partial matrix
/// (`ICNTL(18) = 3`), handed to MUMPS at the analysis and again at every
/// factorisation.
#[derive(Debug, Default)]
struct Coordinates {
    irn: Vec<c_int>,
    jcn: Vec<c_int>,
}

impl Coordinates {
    /// Convert `a`'s coordinates (fallible: index width).
    fn of(a: &SparseMatrixAij) -> Result<Self> {
        let (irn, jcn) = a.coo_one_based_i32(u64::try_from(a.nnz()).unwrap_or(u64::MAX))?;
        let c = Self { irn, jcn };
        c.check(a)?;
        Ok(c)
    }

    /// Consistency with `a` at the FFI edge: exactly
    /// `a.nnz()` coordinates each, and an order that fits `MUMPS_INT`.
    fn check(&self, a: &SparseMatrixAij) -> Result<c_int> {
        if self.irn.len() != a.nnz() || self.jcn.len() != a.nnz() {
            return Err(Error::invalid(format!(
                "MUMPS: {} row / {} column coordinates for the {} local entries of matrix '{}' \
                 (refused before MUMPS runs)",
                self.irn.len(),
                self.jcn.len(),
                a.nnz(),
                a.id()
            )));
        }
        c_int::try_from(a.size()).map_err(|_| {
            Error::invalid(format!(
                "MUMPS: order {} of matrix '{}' does not fit MUMPS_INT",
                a.size(),
                a.id()
            ))
        })
    }
}

/// A successful analysis: its key and the coordinates MUMPS analysed. The
/// coordinates exist only together with the key of the analysis that saw
/// them: new coordinates stay local to their solve until
/// its analysis job starts, and are stored with the new key only when it
/// succeeded.
#[derive(Debug)]
struct Analysis {
    key: AnalysisKey,
    coords: Coordinates,
}

/// MUMPS solver (upstream `SparseSolverMumps`, `_fully_distributed`).
#[derive(Debug, Default)]
pub struct MumpsSolver {
    options: MumpsOptions,
    inst: Option<Instance>,
    sym: i32,
    /// The valid analysis with its coordinates (handed to MUMPS again at
    /// every factorisation, `ICNTL(18) = 3` contract).
    analysis: Option<Analysis>,
    factorized: Option<FactorKey>,
    /// Current `ICNTL(14)` once changed by a retry (kept across analyses
    /// and re-initialisations, as upstream keeps it in its structure).
    icntl14: Option<i32>,
    report: MumpsReport,
}

/// Agreed plan of one solve: `(initialise, analyse, factorise)`.
#[derive(Clone, Copy, Debug)]
struct Plan {
    init: bool,
    analyse: bool,
    factorize: bool,
}

/// Words of a rank's plan contribution: wishes `(init, analyse,
/// factorise)`, effective symmetry, instance token (0: none), bound to the
/// solve's communicator.
const PLAN_WORDS: usize = 6;
/// Words of the canonical options descriptor: `ICNTL(1..=60)` overrides,
/// `CNTL(1..=15)` override values, the `CNTL` override mask, the retry
/// right-hand side, the retry ceiling and the persisted `ICNTL(14)`.
const OPTION_WORDS: usize = NB_ICNTL + NB_CNTL + 4;
const CONTRIBUTION_WORDS: usize = PLAN_WORDS + OPTION_WORDS;

/// An optional `i32` setting as one descriptor word (`0`: unset).
fn setting_word(v: Option<i32>) -> u64 {
    v.map_or(0, |v| (1 << 32) | u64::from(v.cast_unsigned()))
}

/// The `i32` carried by the low half of a descriptor/outcome word.
fn word_i32(w: u64) -> i32 {
    ((w & 0xFFFF_FFFF) as u32).cast_signed()
}

/// Human-readable descriptor field `k` with value `w` (refusal messages).
fn describe_option(k: usize, w: u64) -> (String, String) {
    let setting = |w: u64| {
        if w == 0 {
            "unset".to_owned()
        } else {
            word_i32(w).to_string()
        }
    };
    if k < NB_ICNTL {
        (format!("ICNTL({}) override", k + 1), setting(w))
    } else if k < NB_ICNTL + NB_CNTL {
        (
            format!("CNTL({}) override", k - NB_ICNTL + 1),
            format!("{:e}", f64::from_bits(w)),
        )
    } else {
        match k - NB_ICNTL - NB_CNTL {
            0 => ("set of CNTL overrides".to_owned(), format!("{w:#b}")),
            1 => (
                "retry right-hand side (UD-216)".to_owned(),
                if w == 0 {
                    "Original".to_owned()
                } else {
                    "UpstreamSolution".to_owned()
                },
            ),
            2 => ("ICNTL(14) retry ceiling".to_owned(), setting(w)),
            _ => (
                "persisted ICNTL(14) (after a workspace retry)".to_owned(),
                setting(w),
            ),
        }
    }
}

/// The agreed outcome of one native job (identical on every rank).
#[derive(Clone, Copy, Debug)]
struct JobOutcome {
    infog1: i32,
    infog2: i32,
    icntl14: i32,
    /// Rank 0's token proposal (initialisation only).
    token: u64,
}

/// Outer error: transport class (or an ordinary, agreed refusal of a
/// native header); inner: MUMPS or local failure, identical on every rank.
type Native<T> = Result<Result<T>>;

impl MumpsSolver {
    /// Solver with `options` (validated at the first solve).
    #[must_use]
    pub fn with_options(options: MumpsOptions) -> Self {
        Self {
            options,
            ..Self::default()
        }
    }

    /// The options.
    #[must_use]
    pub fn options(&self) -> &MumpsOptions {
        &self.options
    }

    /// Mutable options; ICNTL/CNTL changes apply from the next analysis
    /// (call [`MumpsSolver::invalidate`] to force one). Options are part of
    /// the agreed descriptor of every solve: change them identically on
    /// every rank (a difference is refused before MUMPS runs).
    pub fn options_mut(&mut self) -> &mut MumpsOptions {
        &mut self.options
    }

    /// Force a new analysis and factorisation at the next solve.
    pub fn invalidate(&mut self) {
        self.analysis = None;
        self.factorized = None;
    }

    /// Give up the native instance without any native call when its bound
    /// communicator is broken: a collective `JOB = -2`
    /// could wait for ever. Returns the broken communicator's reason.
    fn abandon_if_broken(&mut self) -> Option<String> {
        let reason = self.bound_communicator().and_then(Communicator::broken)?;
        self.invalidate();
        if let Some(inst) = self.inst.take() {
            inst.abandon();
        }
        Some(reason)
    }

    /// What the last solve did.
    #[must_use]
    pub fn report(&self) -> &MumpsReport {
        &self.report
    }

    /// The communicator the native instance is bound to (`None` before the
    /// first successful initialisation and after [`MumpsSolver::destroy`]).
    #[must_use]
    pub fn bound_communicator(&self) -> Option<&Communicator> {
        self.inst.as_ref().and_then(|i| i.bound.as_ref())
    }

    /// Bytes of `MUMPS_INT` in the linked library (4 for the Debian build).
    #[must_use]
    pub fn mumps_int_bytes() -> usize {
        // SAFETY: shim constant, no preconditions.
        usize::try_from(unsafe { akantu_mumps_int_bytes() }).unwrap_or(0)
    }

    /// Destroy the MUMPS instance (`JOB = -2`, collective) inside an agreed
    /// native collective of **its bound communicator** — call it at the
    /// same point on every rank of that communicator, outside any live
    /// collective scope of it (or within one of its steps). This is also
    /// the explicit rebinding protocol: the next solve initialises a new
    /// instance on the communicator it is given. A refusal (e.g. a live
    /// scope, a contended `Serialized` admission, or under
    /// `MPI_THREAD_MULTIPLE` another thread's native job in flight)
    /// keeps the instance: **repeat `destroy()` until it succeeds** (the
    /// refusal is agreed on every rank). Do not drop the solver instead
    /// while another thread may run a native job: dropping queues `JOB = -2`
    /// through `Communicator::release_native` without the admission, so it
    /// would run concurrently with that job (a failure mode seen during
    /// the port).
    /// Dropping is safe only under the obligation of the
    /// module documentation ("Destruction": on MPI's main thread, at the
    /// same program point on every rank, no other thread inside MPI), the
    /// obligation upstream's destructor has too.
    pub fn destroy(&mut self) -> Result<()> {
        self.invalidate();
        if let Some(reason) = self.abandon_if_broken() {
            return Err(Error::fatal(
                FatalKind::Transport,
                Error::Communication(format!(
                    "mumps:destroy: the bound communicator is broken ({reason}); the MUMPS \
                     instance is abandoned without JOB = -2"
                )),
            ));
        }
        let Some(inst) = self.inst.as_mut() else {
            return Ok(());
        };
        let Some(comm) = inst.bound.clone() else {
            self.inst = None;
            return Ok(());
        };
        comm.native_collective("mumps:destroy", Ok(()), |_| inst.job(JOB_DESTROY))?;
        inst.bound = None;
        inst.token = 0;
        self.inst = None;
        Ok(())
    }

    fn validate_options(&self) -> Result<()> {
        if let Some(&i) = self
            .options
            .icntl
            .keys()
            .find(|&&i| !(1..=NB_ICNTL).contains(&i))
        {
            return Err(Error::invalid(format!(
                "MUMPS ICNTL({i}) does not exist (1..={NB_ICNTL})"
            )));
        }
        if let Some(&i) = self
            .options
            .cntl
            .keys()
            .find(|&&i| !(1..=NB_CNTL).contains(&i))
        {
            return Err(Error::invalid(format!(
                "MUMPS CNTL({i}) does not exist (1..={NB_CNTL})"
            )));
        }
        if let Some((&i, &v)) = self.options.cntl.iter().find(|(_, v)| !v.is_finite()) {
            return Err(Error::invalid(format!(
                "MUMPS CNTL({i}) = {v}: control values must be finite"
            )));
        }
        for (&i, &v) in &self.options.icntl {
            if matches!(i, 5 | 18 | 20 | 21) && v != upstream_icntl(i).unwrap_or(v) {
                return Err(Error::Unsupported(format!(
                    "MUMPS ICNTL({i}) = {v}: the backend passes an assembled distributed matrix \
                     and a centralised dense right-hand side (ICNTL(5) = 0, ICNTL(18) = 3, \
                     ICNTL(20) = ICNTL(21) = 0)"
                )));
            }
            if reserved_icntl(i) && v != 0 {
                return Err(Error::Unsupported(format!(
                    "MUMPS ICNTL({i}) = {v}: ICNTL({i}) is reserved in MUMPS 5.7.3 (user's guide, \
                     section 6.1: no documented behaviour); only its default 0 is accepted"
                )));
            }
            if let Some(arrays) = array_backed_icntl(i, v) {
                return Err(Error::Unsupported(format!(
                    "MUMPS ICNTL({i}) = {v}: this value makes MUMPS read or write {arrays}, which \
                     the backend does not provide (it hands MUMPS only the distributed matrix \
                     during analysis / factorisation and one dense right-hand side during the \
                     solve); refused before MUMPS runs"
                )));
            }
        }
        Ok(())
    }

    /// Local validation of one solve (agreed by the caller).
    fn preflight(
        &self,
        layout: &EquationLayout,
        a: &SparseMatrixAij,
        rhs: &[Real],
        solution: &[Real],
    ) -> Result<()> {
        self.validate_options()?;
        for (v, what) in [(rhs, "right-hand side"), (solution, "solution")] {
            if v.len() != layout.nb_local() {
                return Err(Error::DimensionMismatch {
                    context: if what == "solution" {
                        "MUMPS solve: solution vector (local equations)"
                    } else {
                        "MUMPS solve: right-hand side (local equations)"
                    },
                    expected: layout.nb_local(),
                    got: v.len(),
                });
            }
        }
        if a.size() != layout.system_size() {
            return Err(Error::invalid(format!(
                "MUMPS solve: matrix '{}' has order {}, the system {}",
                a.id(),
                a.size(),
                layout.system_size()
            )));
        }
        let nnz = u64::try_from(a.nnz()).unwrap_or(u64::MAX);
        self.capabilities().check_local(a.size(), nnz)?;
        if let Some((k, v)) = a.values().iter().enumerate().find(|(_, v)| !v.is_finite()) {
            let (i, j, _) = a.coo();
            return Err(Error::Numerical(format!(
                "mumps: non-finite matrix entry {v} at ({}, {}) of matrix '{}'; sparse direct \
                 solvers refuse non-finite systems",
                i[k],
                j[k],
                a.id()
            )));
        }
        if let Some((l, v)) = rhs
            .iter()
            .enumerate()
            .find(|&(l, v)| layout.is_owned(l) && !v.is_finite())
        {
            return Err(Error::Numerical(format!(
                "mumps: non-finite right-hand side entry {v} at local equation {l}; sparse \
                 direct solvers refuse non-finite systems"
            )));
        }
        Ok(())
    }

    /// Canonical descriptor of every setting that drives the native control
    /// flow: compared word for word between ranks.
    fn options_descriptor(&self) -> [u64; OPTION_WORDS] {
        let mut d = [0u64; OPTION_WORDS];
        for (&i, &v) in &self.options.icntl {
            if let Some(w) = i
                .checked_sub(1)
                .and_then(|k| d.get_mut(k).filter(|_| k < NB_ICNTL))
            {
                *w = setting_word(Some(v));
            }
        }
        let mut mask = 0u64;
        for (&i, &v) in &self.options.cntl {
            if (1..=NB_CNTL).contains(&i) {
                d[NB_ICNTL + i - 1] = v.to_bits();
                mask |= 1 << (i - 1);
            }
        }
        d[NB_ICNTL + NB_CNTL] = mask;
        d[NB_ICNTL + NB_CNTL + 1] = match self.options.retry_rhs {
            RetryRhs::Original => 0,
            RetryRhs::UpstreamSolution => 1,
        };
        d[NB_ICNTL + NB_CNTL + 2] = setting_word(self.options.icntl14_ceiling);
        d[NB_ICNTL + NB_CNTL + 3] = setting_word(self.icntl14);
        d
    }

    /// This rank's contribution to the agreed plan of a solve of `a` with
    /// effective MUMPS symmetry `sym` on `comm`.
    fn contribution(
        &self,
        comm: &Communicator,
        a: &SparseMatrixAij,
        sym: i32,
    ) -> [u64; CONTRIBUTION_WORDS] {
        let inst = self.inst.as_ref().filter(|i| i.initialized());
        let init = inst.is_none() || self.sym != sym;
        let analyse = init || self.analysis.as_ref().map(|an| an.key) != Some(analysis_key(a));
        let factorize = analyse || self.factorized != Some(factor_key(a));
        let bound_here = inst
            .and_then(|i| i.bound.as_ref())
            .is_none_or(|b| b.is_same(comm));
        let mut w = [0u64; CONTRIBUTION_WORDS];
        w[0] = u64::from(init);
        w[1] = u64::from(analyse);
        w[2] = u64::from(factorize);
        w[3] = u64::from(sym.unsigned_abs());
        w[4] = inst.map_or(0, |i| i.token);
        w[5] = u64::from(bound_here);
        w[PLAN_WORDS..].copy_from_slice(&self.options_descriptor());
        w
    }

    /// The plan from every rank's contribution (`all`, rank order), or the
    /// refusal — computed from gathered data only, so the verdict is the
    /// same on every rank.
    fn decide(all: &[u64], nb_proc: usize) -> Result<Plan> {
        if all.len() != CONTRIBUTION_WORDS * nb_proc || nb_proc == 0 {
            return Err(Error::invalid("MUMPS plan: malformed gather"));
        }
        let ranks: Vec<&[u64]> = all.chunks(CONTRIBUTION_WORDS).collect();
        let first = ranks[0];
        if let Some(r) = ranks.iter().position(|w| w[5] == 0) {
            return Err(Error::invalid(format!(
                "the MUMPS instance of rank {r} is bound to another communicator (initialised on \
                 a different communicator, group or rank order; a duplicate or a split is \
                 another communicator): destroy() it collectively on its own communicator \
                 before solving on this one"
            )));
        }
        if let Some(r) = ranks.iter().position(|w| w[4] != first[4]) {
            return Err(Error::invalid(format!(
                "ranks drive different MUMPS instances (instance {} on rank 0, {} on rank {r}; 0: \
                 none): every rank must use the same solver collectively",
                first[4], ranks[r][4]
            )));
        }
        if ranks.iter().any(|w| w[3] != first[3]) {
            return Err(Error::invalid(
                "ranks disagree on the effective symmetry of the matrix (agree the promotion \
                 first)",
            ));
        }
        for k in PLAN_WORDS..CONTRIBUTION_WORDS {
            if let Some(r) = ranks.iter().position(|w| w[k] != first[k]) {
                let (field, mine) = describe_option(k - PLAN_WORDS, first[k]);
                let (_, theirs) = describe_option(k - PLAN_WORDS, ranks[r][k]);
                return Err(Error::invalid(format!(
                    "ranks disagree on the MUMPS options before any native job: {field} (rank 0: \
                     {mine}, rank {r}: {theirs}); set the options identically on every rank \
                    "
                )));
            }
        }
        let any = |k: usize| ranks.iter().any(|w| w[k] != 0);
        Ok(Plan {
            init: any(0),
            analyse: any(1),
            factorize: any(2),
        })
    }

    /// Apply upstream's controls (`initMumpsData`, `setOutputLevel`), the
    /// persisted `ICNTL(14)` and the user overrides (all agreed).
    fn apply_controls(&mut self) {
        let icntl14 = self.icntl14;
        let Some(inst) = self.inst.as_mut() else {
            return;
        };
        for i in [1, 2, 3, 4, 5, 8, 18, 20, 21, 28] {
            if let Some(v) = upstream_icntl(i) {
                inst.set_icntl(i, v);
            }
        }
        if let Some(v) = icntl14 {
            inst.set_icntl(14, v);
        }
        for (&i, &v) in &self.options.icntl {
            if i == 14 && icntl14.is_some() {
                continue; // a retry increased it (upstream keeps the increase)
            }
            inst.set_icntl(i, v);
        }
        for (&i, &v) in &self.options.cntl {
            inst.set_cntl(i, v);
        }
    }

    fn snapshot(&mut self) {
        if let Some(inst) = self.inst.as_ref() {
            self.report.infog = (1..=NB_INFOG).map(|i| inst.infog(i)).collect();
            self.report.rinfog = (1..=NB_RINFOG).map(|i| inst.rinfog(i)).collect();
            self.report.icntl14 = inst.icntl(14);
        }
    }

    /// Agree the global outcome of the native job `what` that just ran:
    /// `(INFOG(1), INFOG(2), ICNTL(14))` must be identical on every rank
    /// (MUMPS makes INFOG global; ICNTL(14) derives from agreed settings);
    /// `proposal` travels along (rank 0's is adopted). Every later decision
    /// uses the returned values only. A disagreement abandons the instance
    /// and is an agreed fatal error (identical verdict on every rank).
    fn agree_outcome(
        &mut self,
        comm: &Communicator,
        what: &str,
        proposal: u64,
    ) -> Native<JobOutcome> {
        let Some(inst) = self.inst.as_ref() else {
            return Ok(Err(Error::invalid("MUMPS outcome without an instance")));
        };
        let enc = |v: i32| u64::from(v.cast_unsigned());
        let mine = [
            enc(inst.infog(1)),
            enc(inst.infog(2)),
            enc(inst.icntl(14)),
            proposal,
        ];
        let all = comm.all_gather(&mine)?;
        self.snapshot();
        let ranks: Vec<&[u64]> = all.chunks(4).collect();
        let disagreement = if ranks.len() != comm.size() || ranks.iter().any(|w| w.len() != 4) {
            Some("malformed outcome gather".to_owned())
        } else {
            ranks.iter().position(|w| w[..3] != ranks[0][..3]).map(|r| {
                format!(
                    "rank 0 reports (INFOG(1), INFOG(2), ICNTL(14)) = ({}, {}, {}), rank {r} \
                         ({}, {}, {})",
                    word_i32(ranks[0][0]),
                    word_i32(ranks[0][1]),
                    word_i32(ranks[0][2]),
                    word_i32(ranks[r][0]),
                    word_i32(ranks[r][1]),
                    word_i32(ranks[r][2])
                )
            })
        };
        if let Some(detail) = disagreement {
            if let Some(inst) = self.inst.take() {
                inst.abandon();
            }
            self.invalidate();
            return Ok(Err(Error::fatal(
                FatalKind::Agreed,
                Error::Backend {
                    backend: "mumps",
                    code: -1,
                    message: format!(
                        "MUMPS {what}: the ranks disagree on the global outcome of the job \
                         ({detail}); the native instance is abandoned"
                    ),
                },
            )));
        }
        Ok(Ok(JobOutcome {
            infog1: word_i32(ranks[0][0]),
            infog2: word_i32(ranks[0][1]),
            icntl14: word_i32(ranks[0][2]),
            token: ranks[0][3],
        }))
    }

    /// Initialise (`JOB = -1`) a new instance with `sym` on `comm`, after
    /// the agreed destruction of the previous one. The previous instance
    /// stays owned by the solver until its `JOB = -2` ran (a refused
    /// destruction keeps it intact).
    fn initialize(&mut self, comm: &Communicator, sym: i32) -> Native<()> {
        if let Some(old) = self.inst.as_mut()
            && old.initialized()
        {
            // The agreed plan established that every rank's instance is the
            // same native instance bound to `comm`.
            self.report.jobs.push(JOB_DESTROY);
            comm.native_collective("mumps:destroy", Ok(()), |_| old.job(JOB_DESTROY))?;
            old.bound = None;
            old.token = 0;
        }
        self.inst = None; // frees the destroyed (or never initialised) structure
        self.invalidate();
        let (local, mut slot) = match Instance::alloc() {
            Ok(i) => (Ok(()), Some(i)),
            Err(e) => (Err(e), None),
        };
        self.report.jobs.push(JOB_INIT);
        comm.native_collective("mumps:init", local, |h: NativeComm| {
            let Some(inst) = slot.as_mut() else {
                return 0;
            };
            // SAFETY: live instance; the init fields are plain integers.
            unsafe { akantu_mumps_set_init(inst.raw(), h.fortran(), sym, 1) };
            inst.job(JOB_INIT)
        })?;
        let Some(inst) = slot else {
            return Ok(Err(Error::invalid(
                "MUMPS instance missing after an agreed init",
            )));
        };
        self.inst = Some(inst);
        // The instance identity: rank 0 proposes a fresh token, every rank
        // adopts it with the agreed outcome.
        let proposal = if comm.rank() == 0 {
            NEXT_TOKEN.fetch_add(1, Ordering::SeqCst)
        } else {
            0
        };
        let out = match self.agree_outcome(comm, "initialisation", proposal)? {
            Ok(o) => o,
            Err(e) => return Ok(Err(e)),
        };
        if out.infog1 < 0 {
            // JOB = -1 failed (on every rank): MUMPS owns nothing that needs
            // JOB = -2; the structure is freed.
            self.inst = None;
            return Ok(Err(mumps_error("initialisation", out.infog1, out.infog2)));
        }
        if let Some(inst) = self.inst.as_mut() {
            inst.bound = Some(comm.clone());
            inst.token = out.token;
            self.report.version = inst.version();
        }
        self.sym = sym;
        // Upstream calls setOutputLevel() again after JOB = -1.
        self.apply_controls();
        Ok(Ok(()))
    }

    /// `ICNTL(14) += 10` from the agreed current value; `true` if the
    /// retry continues (`< 90`, or the agreed configured ceiling).
    fn bump_workspace(&mut self, current: i32) -> bool {
        let ceiling = self.options.icntl14_ceiling.unwrap_or(ICNTL14_CEILING);
        let (v, retry) = next_icntl14(current, ceiling);
        if let Some(inst) = self.inst.as_mut() {
            inst.set_icntl(14, v);
        }
        self.icntl14 = Some(v);
        self.report.icntl14 = v;
        retry
    }

    /// The native part of one solve (inside the solve's communication
    /// step). Every branch depends only on the agreed plan, the agreed
    /// options and agreed job outcomes, so every rank issues the same
    /// native jobs. `pending`: the coordinates converted for this solve's
    /// analysis (`Some` exactly when the plan analyses, agreed before).
    fn run_jobs(
        &mut self,
        comm: &Communicator,
        plan: Plan,
        sym: i32,
        a: &SparseMatrixAij,
        mut pending: Option<Coordinates>,
        rhs_root: Option<&[Real]>,
    ) -> Native<Option<Vec<Real>>> {
        self.report.jobs.clear();
        self.report.retries = 0;
        self.report.factorization_retries = 0;
        self.report.nb_proc = comm.size();
        self.report.sym = sym;
        if plan.init
            && let Err(e) = self.initialize(comm, sym)?
        {
            return Ok(Err(e));
        }
        let n = c_int::try_from(a.size()).unwrap_or(c_int::MAX);
        let mut analyse = plan.analyse || plan.init;
        let mut factorize = plan.factorize || analyse;
        // Every `continue` below sets `analyse`/`factorize` for the redo.
        let mut buf: Option<Vec<Real>>;
        let rank0 = comm.rank() == 0;
        loop {
            if analyse {
                // The coordinates of this analysis: the solve's freshly
                // converted ones, or (workspace retry) those of the analysis
                // being redone. The old analysis and its coordinates are
                // dropped together.
                self.factorized = None;
                let coords = pending
                    .take()
                    .or_else(|| self.analysis.take().map(|an| an.coords));
                self.analysis = None;
                self.apply_controls();
                if let Some(inst) = self.inst.as_ref() {
                    self.report.icntl = (1..=NB_ICNTL).map(|i| inst.icntl(i)).collect();
                }
                let out = self.matrix_job(
                    comm,
                    "mumps:analysis",
                    JOB_ANALYSE,
                    a,
                    coords.as_ref(),
                    false,
                );
                let out = match out? {
                    Ok(o) => o,
                    Err(e) => return Ok(Err(e)),
                };
                if out.infog1 < 0 {
                    return Ok(Err(mumps_error("analysis", out.infog1, out.infog2)));
                }
                self.analysis = coords.map(|coords| Analysis {
                    key: analysis_key(a),
                    coords,
                });
            }
            if factorize {
                self.factorized = None;
                // The analysis's own coordinates, only if it is this
                // matrix's (otherwise the native header refuses on every
                // rank; the agreed plan and consistency step make that
                // unreachable).
                let an = self.analysis.take();
                let coords = an
                    .as_ref()
                    .filter(|an| an.key == analysis_key(a))
                    .map(|an| &an.coords);
                let out =
                    self.matrix_job(comm, "mumps:factorization", JOB_FACTORIZE, a, coords, true);
                self.analysis = an;
                let out = match out? {
                    Ok(o) => o,
                    Err(e) => return Ok(Err(e)),
                };
                if out.infog1 == -9 {
                    self.report.retries += 1;
                    self.report.factorization_retries += 1;
                    if self.bump_workspace(out.icntl14) {
                        analyse = true;
                        continue;
                    }
                    return Ok(Err(workspace_exhausted(out.infog2)));
                }
                if out.infog1 < 0 {
                    return Ok(Err(mumps_error("factorization", out.infog1, out.infog2)));
                }
                self.factorized = Some(factor_key(a));
            }
            // Solve: always from a fresh copy of the original right-hand side
            // (UD-216 fix: a retried solve never sees a previous solution).
            buf = rhs_root.map(<[Real]>::to_vec);
            let out = match self.solve_job(comm, n, buf.as_mut())? {
                Ok(o) => o,
                Err(e) => return Ok(Err(e)),
            };
            if out.infog1 == -9 {
                self.report.retries += 1;
                if self.bump_workspace(out.icntl14) {
                    analyse = true;
                    factorize = true;
                    continue;
                }
                return Ok(Err(workspace_exhausted(out.infog2)));
            }
            if out.infog1 < 0 {
                return Ok(Err(mumps_error("solve", out.infog1, out.infog2)));
            }
            break;
        }
        if self.options.retry_rhs == RetryRhs::UpstreamSolution {
            // UD-216 compat: upstream's outer solveInternal() of every retried
            // factorisation solves once more with the inner solution (the
            // count is agreed: it derives from agreed outcomes).
            for _ in 0..self.report.factorization_retries {
                let out = match self.solve_job(comm, n, buf.as_mut())? {
                    Ok(o) => o,
                    Err(e) => return Ok(Err(e)),
                };
                if out.infog1 < 0 {
                    return Ok(Err(mumps_error("solve", out.infog1, out.infog2)));
                }
            }
        }
        if !rank0 {
            return Ok(Ok(None));
        }
        let x = buf.unwrap_or_default();
        if let Some((i, v)) = x.iter().enumerate().find(|(_, v)| !v.is_finite()) {
            return Ok(Err(arithmetic_range_failure(
                "mumps",
                format!("solution entry {i} = {v}"),
            )));
        }
        Ok(Ok(Some(x)))
    }

    /// The analysis (`values = false`) or factorisation job of `a` with
    /// `coords`, then its agreed outcome. The consistency of `coords` with
    /// `a` (`nnz` coordinates each, order within `MUMPS_INT`) is this
    /// rank's **local validation of the native header**: a mismatch on any
    /// rank is refused on every rank before MUMPS runs;
    /// the matrix pointers are set only after a successful check and
    /// cleared right after the job on every path.
    fn matrix_job(
        &mut self,
        comm: &Communicator,
        what: &str,
        job: c_int,
        a: &SparseMatrixAij,
        coords: Option<&Coordinates>,
        values: bool,
    ) -> Native<JobOutcome> {
        let checked = coords
            .ok_or_else(|| {
                Error::invalid(format!(
                    "{what}: no coordinates of matrix '{}' for this job (refused before MUMPS \
                     runs)",
                    a.id()
                ))
            })
            .and_then(|c| c.check(a).map(|n| (n, c)));
        let inst = self
            .inst
            .as_mut()
            .ok_or_else(|| Error::invalid("MUMPS job without an instance"))?;
        if let Ok((n, c)) = &checked {
            inst.set_matrix(*n, c, values.then(|| a.values()));
        }
        self.report.jobs.push(job);
        let ran = comm.native_collective(what, checked.map(drop), |_| inst.job(job));
        inst.clear_matrix();
        ran?;
        self.agree_outcome(comm, what, 0)
    }

    /// `JOB = 3` with the host's right-hand side `rhs` (overwritten by the
    /// solution), then its agreed outcome. The host (rank 0) must hold
    /// exactly `n` entries: the check is the local validation of the native
    /// header. The RHS pointer is cleared on every
    /// path.
    fn solve_job(
        &mut self,
        comm: &Communicator,
        n: c_int,
        rhs: Option<&mut Vec<Real>>,
    ) -> Native<JobOutcome> {
        let host = comm.rank() == 0;
        let len = rhs.as_ref().map(|b| b.len());
        let local = match (host, len) {
            (false, _) => Ok(()),
            (true, Some(l)) if usize::try_from(n).is_ok_and(|n| n == l) => Ok(()),
            (true, l) => Err(Error::invalid(format!(
                "mumps:solve: the host's right-hand side has {l:?} entries, the system {n} \
                 (refused before MUMPS runs)"
            ))),
        };
        let p = match (&local, rhs) {
            (Ok(()), Some(b)) if host => b.as_mut_ptr(),
            _ => std::ptr::null_mut(),
        };
        debug_assert!(!host || local.is_err() || !p.is_null());
        let inst = self
            .inst
            .as_mut()
            .ok_or_else(|| Error::invalid("MUMPS job without an instance"))?;
        // SAFETY: live instance; `p` is null (non-host ranks, or a failed
        // check, then the job is refused on every rank) or points to the
        // host's `n` writable doubles (length checked above, alive and not
        // otherwise borrowed until the pointer is cleared right after the
        // job). The host is rank 0 of the instance's communicator, which the
        // agreed plan checked is the solve's communicator.
        unsafe { akantu_mumps_set_rhs(inst.raw(), p, n) };
        self.report.jobs.push(JOB_SOLVE);
        let ran = comm.native_collective("mumps:solve", local, |_| inst.job(JOB_SOLVE));
        // SAFETY: live instance; clears the RHS pointer (no dangling pointer
        // is left in the structure), whatever the job's outcome.
        unsafe { akantu_mumps_set_rhs(inst.raw(), std::ptr::null_mut(), n) };
        ran?;
        self.agree_outcome(comm, "mumps:solve", 0)
    }

    /// Local consistency of a solve that re-uses the stored analysis (no
    /// analysis planned): it must be `a`'s (identity, type, order, entries)
    /// with `nnz` coordinates, and the factors `a`'s unless a factorisation
    /// is planned (defence in depth; agreed).
    fn check_reuse(&self, a: &SparseMatrixAij, plan: Plan) -> Result<()> {
        let an = self
            .analysis
            .as_ref()
            .filter(|an| an.key == analysis_key(a))
            .ok_or_else(|| {
                Error::invalid(format!(
                    "MUMPS: the stored analysis is not the one of matrix '{}' although no rank \
                     asked for a new one",
                    a.id()
                ))
            })?;
        an.coords.check(a)?;
        if !plan.factorize && self.factorized != Some(factor_key(a)) {
            return Err(Error::invalid(format!(
                "MUMPS: the stored factors are not the ones of matrix '{}' although no rank asked \
                 for a new factorisation",
                a.id()
            )));
        }
        Ok(())
    }
}

/// The user-array structure components a user `ICNTL(i) = v` would make
/// MUMPS 5.7 read or write (user's guide 5.7.3, section 6.1, "Possible
/// variables/arrays involved"), when the backend never provides them;
/// `None` for every other value. Upstream (`SparseSolverMumps`) gives users
/// no access to ICNTL at all (its accessor is private) and itself sets only
/// ICNTL(1–5, 8, 14, 18, 20, 21, 28), none of which is listed here.
fn array_backed_icntl(i: usize, v: i32) -> Option<&'static str> {
    match (i, v) {
        (7, 1) => Some("the user pivot order PERM_IN"),
        (8, -1) => Some("the user scaling arrays COLSCA / ROWSCA"),
        (15, v) if v > 0 => Some("the user block structure NBLK / BLKPTR / BLKVAR"),
        (19, v) if v != 0 => Some("the Schur complement arrays (LISTVAR_SCHUR, SCHUR, …)"),
        (25, v) if v < 0 => {
            Some("a right-hand side array with INFOG(28) columns (null-space basis)")
        }
        (26, v) if v != 0 => Some("the reduced right-hand side REDRHS"),
        (30, v) if v != 0 => {
            Some("the sparse right-hand side arrays (IRHS_PTR, IRHS_SPARSE, RHS_SPARSE)")
        }
        (32, v) if v != 0 => Some("the right-hand side RHS during the factorisation"),
        _ => None,
    }
}

/// ICNTL indices MUMPS 5.7.3 documents as "reserved in current version" or
/// "not used" (user's guide 5.7.3, section 6.1): no behaviour is documented
/// for a non-zero value, so a user value other than 0 is refused.
/// MUMPS's own default for them is 0.
fn reserved_icntl(i: usize) -> bool {
    matches!(i, 40..=47 | 50..=55 | 57 | 59 | 60)
}

/// Upstream's control values (`initMumpsData`, `setOutputLevel` in a
/// release build).
fn upstream_icntl(i: usize) -> Option<i32> {
    match i {
        1..=4 => Some(0),
        5 => Some(0),
        8 => Some(77),
        18 => Some(3),
        20 | 21 => Some(0),
        28 => Some(0),
        _ => None,
    }
}

/// SOLV-003 error mapping of a negative `INFOG(1)` (upstream `printError`).
fn mumps_error(phase: &str, infog1: i32, infog2: i32) -> Error {
    match infog1 {
        -10 => singular_matrix(format!(
            "MUMPS {phase}: numerically singular matrix, INFOG(1) = -10, INFOG(2) = {infog2}"
        )),
        -6 => singular_matrix(format!(
            "MUMPS {phase}: structurally singular matrix, INFOG(1) = -6, structural rank \
             INFOG(2) = {infog2}"
        )),
        _ => Error::Backend {
            backend: "mumps",
            code: i64::from(infog1),
            message: format!(
                "Error in mumps during the {phase}, check the MUMPS user guide: INFOG(1) = \
                 {infog1}, INFOG(2) = {infog2}"
            ),
        },
    }
}

/// Workspace retry rule: the increased `ICNTL(14)` and whether to retry
/// (upstream: `+10`, retry unless the new value is 90; the port retries
/// while it is below the ceiling, so every start terminates).
fn next_icntl14(current: i32, ceiling: i32) -> (i32, bool) {
    let v = current.saturating_add(ICNTL14_STEP);
    (v, v < ceiling)
}

fn workspace_exhausted(infog2: i32) -> Error {
    Error::Backend {
        backend: "mumps",
        code: -9,
        message: format!(
            "The MUMPS workarray is too small INFO(2)={infog2} No further increase possible \
             (ICNTL(14) reached its retry ceiling, upstream {ICNTL14_CEILING})"
        ),
    }
}

impl SparseDirectSolver for MumpsSolver {
    fn kind(&self) -> SparseSolverType {
        SparseSolverType::Mumps
    }

    fn name(&self) -> &'static str {
        "mumps"
    }

    /// Distributed assembled matrix, centralised right-hand side; 32-bit
    /// `MUMPS_INT` coordinates (1-based ⇒ largest 0-based equation
    /// `i32::MAX − 1`, order `≤ i32::MAX`), 64-bit `NNZ_loc`.
    fn capabilities(&self) -> BackendCapabilities {
        let max_order = GlobalIdx::from(i32::MAX);
        BackendCapabilities {
            matrix: MatrixDistribution::DistributedAssembled,
            rhs: RhsDistribution::CentralizedOnRoot,
            max_global_index: max_order - 1,
            max_local_nnz: i64::MAX.unsigned_abs(),
            max_centralized_size: Some(max_order),
        }
    }

    fn parallel_method(&self) -> SolverParallelMethod {
        SolverParallelMethod::FullyDistributed
    }

    fn analysis(&mut self, _: &SparseMatrixAij) -> Result<()> {
        Err(direct_use())
    }

    fn factorize(&mut self, _: &SparseMatrixAij) -> Result<()> {
        Err(direct_use())
    }

    fn solve(&mut self, _: &[Real], _: &mut [Real]) -> Result<()> {
        Err(direct_use())
    }

    fn destroy_internal_data(&mut self) {
        // Upstream `destroyInternalData` (communicator finalisation). A
        // refused collective destruction leaves the instance to `Drop`.
        let _ = self.destroy();
    }

    /// [`MumpsSolver::destroy`]: the factors and the instance are released
    /// (collective, agreed); the next solve initialises a new instance.
    fn release_factorization(&mut self) -> Result<()> {
        self.destroy()
    }

    /// The manager-wide UD-216 switch (D-041 (4)) decides the retry
    /// right-hand side of a solver driven by a DOF manager (no per-object
    /// mixing); standalone use keeps [`MumpsOptions::retry_rhs`]. The
    /// setting is part of every solve's agreed options descriptor.
    fn set_upstream_compat(&mut self, compat: SparseSolverCompat) {
        self.options.retry_rhs = if compat.mumps_retry_with_solution_rhs {
            RetryRhs::UpstreamSolution
        } else {
            RetryRhs::Original
        };
    }

    /// `INFOG`/`RINFOG` figures of the last solve (global values, identical
    /// on every rank; MUMPS 5.7 user guide §6): ordering `INFOG(7)`, real
    /// workspace of the factors estimated by the analysis `INFOG(3)` and
    /// allocated by the factorisation `INFOG(9)`, effective factor entries
    /// `INFOG(29)` (a negative MUMPS value means millions: converted here),
    /// memory in MB — estimated `INFOG(16)` (most
    /// loaded process) / `INFOG(17)` (sum), allocated `INFOG(18)` /
    /// `INFOG(19)`, effectively used `INFOG(21)` / `INFOG(22)`, used by the
    /// solve phase `INFOG(30)` / `INFOG(31)` — elimination flops estimated
    /// by the analysis `RINFOG(1)`, assembly and elimination flops of the
    /// factorisation `RINFOG(2)` / `RINFOG(3)`, the workspace retries and
    /// `ICNTL(14)`. Empty before
    /// the first solve.
    fn statistics(&self) -> Vec<(&'static str, f64)> {
        let r = &self.report;
        if r.infog.is_empty() {
            return Vec::new();
        }
        // INFOG entries that MUMPS stores as `-value / 1e6` when large
        let entries = |i: usize| {
            let v = f64::from(r.infog(i));
            if v < 0.0 { -v * 1.0e6 } else { v }
        };
        vec![
            ("nb_proc", r.nb_proc as f64),
            ("sym", f64::from(r.sym)),
            ("infog7_ordering", f64::from(r.infog(7))),
            ("infog3_factor_reals_estimated", entries(3)),
            ("infog9_factor_reals", entries(9)),
            ("infog29_factor_entries_effective", entries(29)),
            ("infog16_mb_max_estimated", f64::from(r.infog(16))),
            ("infog17_mb_sum_estimated", f64::from(r.infog(17))),
            ("infog18_mb_max_allocated", f64::from(r.infog(18))),
            ("infog19_mb_sum_allocated", f64::from(r.infog(19))),
            ("infog21_mb_max_used", f64::from(r.infog(21))),
            ("infog22_mb_sum_used", f64::from(r.infog(22))),
            ("infog30_mb_max_solve", f64::from(r.infog(30))),
            ("infog31_mb_sum_solve", f64::from(r.infog(31))),
            ("rinfog1_flops_elimination_estimated", r.rinfog(1)),
            ("rinfog2_flops_assembly", r.rinfog(2)),
            ("rinfog3_flops_elimination", r.rinfog(3)),
            ("retries", r.retries as f64),
            ("icntl14", f64::from(r.icntl14)),
        ]
    }

    fn solve_distributed(
        &mut self,
        phases: &mut Phases,
        name: &str,
        layout: &EquationLayout,
        a: Result<&SparseMatrixAij>,
        rhs: &[Real],
        solution: &mut [Real],
    ) -> Result<()> {
        // 1. Local validation (shapes, capacities, finite input), agreed.
        let prepared = a.and_then(|a| {
            self.preflight(layout, a, rhs, solution)?;
            Ok(a)
        });
        let a = phases.agreed(
            &format!("{name}:mumps:preflight"),
            prepared,
            Severity::Recoverable,
        )?;
        // 2. MPI communicator required (identical verdict on every rank: the
        //    backend kind is common to all ranks of a communicator).
        phases.run(&format!("{name}:mumps:mpi"), Ok(()), |comm, ()| {
            Ok(if comm.is_mpi() {
                Ok(())
            } else {
                Err(Error::Unsupported(format!(
                    "the MUMPS sparse solver needs an MPI communicator (this one is {}); use \
                     Communicator::mpi_world, or the eigen/faer solvers",
                    if comm.is_serial() {
                        "serial"
                    } else {
                        "an in-process local world"
                    }
                )))
            })
        })?;
        // 3. Agreed plan: re-initialise / re-analyse
        //    / refactorise if any rank needs it; refused on every rank unless
        //    every rank's instance is the same native instance bound to this
        //    communicator, the effective symmetry is common and the options
        //    descriptor is identical.
        let sym = mumps_symmetry(a);
        let all = phases.run_collective(&format!("{name}:mumps:plan"), Ok(()), |c, ()| {
            // the binding check needs the scope's own communicator
            c.all_gather(&self.contribution(c, a, sym))
        })?;
        let plan = Self::decide(&all, phases.rank_size().1);
        let plan = phases.agreed(
            &format!("{name}:mumps:plan-check"),
            plan,
            Severity::Recoverable,
        )?;
        // 4. Coordinates and consistency: a planned
        //    analysis gets freshly converted coordinates, which stay local to
        //    this solve until its analysis job starts (a later refusal —
        //    e.g. of the RHS gather — drops them and leaves the stored
        //    analysis and its own coordinates intact); otherwise the stored
        //    analysis must be this matrix's. Agreed before any native job.
        let coords = if plan.analyse || plan.init {
            Coordinates::of(a).map(Some)
        } else {
            self.check_reuse(a, plan).map(|()| None)
        };
        let pending = phases.agreed(
            &format!("{name}:mumps:coordinates"),
            coords,
            Severity::Recoverable,
        )?;
        // 5. Centralised right-hand side on rank 0.
        let caps = self.capabilities();
        let sys = centralize::gather_rhs(phases, &format!("{name}:mumps:rhs"), layout, rhs, &caps)?;
        // 6. MUMPS jobs (native collectives inside one communication step).
        let x_root = phases.run(&format!("{name}:mumps:jobs"), Ok(()), |comm, ()| {
            match self.run_jobs(comm, plan, sym, a, pending, sys.rhs.as_deref()) {
                Ok(inner) => Ok(inner),
                // A refused admission, a native job mismatch or a local
                // thread-policy failure is an ordinary, agreed error;
                // transport failures break the scope.
                Err(e) if e.fatal_kind() == Some(FatalKind::Transport) => Err(e),
                Err(e) => Ok(Err(e)),
            }
        });
        if x_root.is_err() {
            self.invalidate();
            // After a transport failure the bound communicator is broken: no
            // JOB = -2 may be attempted on it any more.
            self.abandon_if_broken();
        }
        let x_root = x_root?;
        // 7. Scatter the solution to the owned rows.
        centralize::scatter_solution(
            phases,
            &format!("{name}:mumps:x"),
            &sys,
            Ok(x_root),
            solution,
        )
    }
}

/// Error of the serial direct-use API.
fn direct_use() -> Error {
    Error::Unsupported(
        "the MUMPS sparse solver takes its system collectively through \
         SparseDirectSolver::solve_distributed (AijBackend / DofManager solves)"
            .into(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Upstream's sequence from the default 20: retries at 30..=80, error
    /// at 90; a start that is not ≡ 0 (mod 10) stops too (upstream's
    /// `!= 90` test would loop for ever).
    #[test]
    fn workspace_retry_rule() {
        let mut v = 20;
        let mut retries = 0;
        loop {
            let (n, retry) = next_icntl14(v, ICNTL14_CEILING);
            v = n;
            if !retry {
                break;
            }
            retries += 1;
        }
        assert_eq!((v, retries), (90, 6));
        assert_eq!(next_icntl14(85, 90), (95, false));
        assert_eq!(next_icntl14(-80, 90), (-70, true));
        assert_eq!(next_icntl14(i32::MAX, 90), (i32::MAX, false));
    }

    #[test]
    fn error_mapping() {
        assert!(
            matches!(mumps_error("factorization", -10, 3), Error::LinearSolver(m) if m.contains("singular matrix"))
        );
        assert!(matches!(
            mumps_error("analysis", -6, 2),
            Error::LinearSolver(_)
        ));
        assert!(matches!(
            mumps_error("solve", -13, 99),
            Error::Backend {
                backend: "mumps",
                code: -13,
                ..
            }
        ));
        assert!(matches!(
            workspace_exhausted(7),
            Error::Backend { code: -9, .. }
        ));
    }

    #[test]
    fn upstream_controls() {
        assert_eq!(upstream_icntl(8), Some(77));
        assert_eq!(upstream_icntl(18), Some(3));
        assert_eq!(upstream_icntl(20), Some(0));
        assert_eq!(upstream_icntl(21), Some(0));
        assert_eq!(upstream_icntl(28), Some(0));
        assert_eq!(upstream_icntl(5), Some(0));
        assert_eq!(upstream_icntl(14), None);
        assert_eq!(upstream_icntl(7), None);
    }

    /// One rank's plan contribution for tests: wishes, symmetry, token,
    /// binding verdict and the options descriptor of `s`.
    fn contribution_of(s: &MumpsSolver, wishes: [u64; 3], token: u64, bound: bool) -> Vec<u64> {
        let mut w = vec![wishes[0], wishes[1], wishes[2], 2, token, u64::from(bound)];
        w.extend_from_slice(&s.options_descriptor());
        w
    }

    /// The plan verdict refuses a foreign binding,
    /// different instances, different symmetries and any options
    /// difference, naming the field; equal contributions OR the wishes.
    #[test]
    fn plan_decision() {
        let s = MumpsSolver::default();
        let ok = |ranks: &[Vec<u64>]| MumpsSolver::decide(&ranks.concat(), ranks.len());
        let a = contribution_of(&s, [0, 0, 1], 7, true);
        let b = contribution_of(&s, [0, 1, 1], 7, true);
        let p = ok(&[a.clone(), b.clone()]).expect("agreed");
        assert!(!p.init && p.analyse && p.factorize);
        let err = |r: Result<Plan>| r.err().map(|e| e.to_string()).unwrap_or_default();
        let foreign = contribution_of(&s, [0, 0, 0], 7, false);
        assert!(err(ok(&[a.clone(), foreign])).contains("bound to another communicator"));
        let other = contribution_of(&s, [0, 0, 0], 8, true);
        assert!(err(ok(&[a.clone(), other])).contains("different MUMPS instances"));
        let mut unsym = b.clone();
        unsym[3] = 0;
        assert!(err(ok(&[a.clone(), unsym])).contains("effective symmetry"));
        let mut t = MumpsSolver::default();
        t.options_mut().icntl14_ceiling = Some(-50);
        let c = contribution_of(&t, [0, 0, 1], 7, true);
        let e = err(ok(&[a.clone(), c]));
        assert!(
            e.contains("disagree on the MUMPS options") && e.contains("retry ceiling"),
            "{e}"
        );
        let mut t = MumpsSolver::default();
        t.options_mut().retry_rhs = RetryRhs::UpstreamSolution;
        let e = err(ok(&[contribution_of(&t, [0, 0, 1], 7, true), a.clone()]));
        assert!(e.contains("retry right-hand side"), "{e}");
        let mut t = MumpsSolver::default();
        t.options_mut().icntl.insert(7, 5);
        let e = err(ok(&[a.clone(), contribution_of(&t, [0, 0, 1], 7, true)]));
        assert!(
            e.contains("ICNTL(7) override") && e.contains("unset"),
            "{e}"
        );
        let t = MumpsSolver {
            icntl14: Some(0),
            ..MumpsSolver::default()
        };
        let e = err(ok(&[a.clone(), contribution_of(&t, [0, 0, 1], 7, true)]));
        assert!(e.contains("persisted ICNTL(14)"), "{e}");
        assert!(err(ok(&[a, b[..3].to_vec()])).contains("malformed"));
    }

    /// The descriptor distinguishes an unset override from any value
    /// (including 0 and 0.0) and records the CNTL override set.
    #[test]
    fn options_descriptor_is_canonical() {
        let base = MumpsSolver::default().options_descriptor();
        let mut s = MumpsSolver::default();
        s.options_mut().icntl.insert(14, 0);
        assert_ne!(s.options_descriptor(), base);
        let mut s = MumpsSolver::default();
        s.options_mut().cntl.insert(3, 0.0);
        assert_ne!(s.options_descriptor(), base);
        let mut s = MumpsSolver::default();
        s.options_mut().icntl14_ceiling = Some(0);
        assert_ne!(s.options_descriptor(), base);
        assert_eq!(MumpsSolver::default().options_descriptor(), base);
        assert_eq!(word_i32(setting_word(Some(-80))), -80);
        assert_eq!(describe_option(NB_ICNTL + NB_CNTL + 2, 0).1, "unset");
    }

    /// The manager-wide UD-216 switch sets the retry
    /// right-hand side (and back).
    #[test]
    fn manager_compat_sets_the_retry_rhs() {
        let mut s = MumpsSolver::default();
        s.set_upstream_compat(SparseSolverCompat {
            mumps_retry_with_solution_rhs: true,
        });
        assert_eq!(s.options().retry_rhs, RetryRhs::UpstreamSolution);
        s.set_upstream_compat(SparseSolverCompat::default());
        assert_eq!(s.options().retry_rhs, RetryRhs::Original);
    }

    /// Options are validated: unknown indices and changes of the input
    /// format are refused before MUMPS runs.
    #[test]
    fn option_validation() {
        let mut s = MumpsSolver::default();
        assert!(s.validate_options().is_ok());
        s.options_mut().icntl.insert(61, 1);
        assert!(s.validate_options().is_err());
        let mut s = MumpsSolver::default();
        s.options_mut().icntl.insert(18, 0);
        assert!(matches!(s.validate_options(), Err(Error::Unsupported(_))));
        let mut s = MumpsSolver::default();
        s.options_mut().cntl.insert(0, 1.0);
        assert!(s.validate_options().is_err());
        let mut s = MumpsSolver::default();
        s.options_mut().icntl.insert(7, 5);
        s.options_mut().icntl.insert(18, 3);
        assert!(s.validate_options().is_ok());
        let mut s = MumpsSolver::default();
        s.options_mut().cntl.insert(1, f64::NAN);
        assert!(s.validate_options().is_err());
    }

    /// Every ICNTL value that makes MUMPS read or write
    /// a user array the backend never provides is refused before MUMPS runs;
    /// the neighbouring values, and every value upstream sets itself, stay
    /// accepted.
    #[test]
    fn array_backed_icntl_values_are_refused() {
        let check = |i: usize, v: i32| {
            let mut s = MumpsSolver::default();
            s.options_mut().icntl.insert(i, v);
            s.validate_options()
        };
        for (i, v) in [
            (7, 1),
            (8, -1),
            (15, 1),
            (19, 1),
            (19, 2),
            (19, 3),
            (25, -1),
            (26, 1),
            (26, 2),
            (30, 1),
            (32, 1),
        ] {
            assert!(
                matches!(check(i, v), Err(Error::Unsupported(m)) if m.contains("the backend does not provide")),
                "ICNTL({i}) = {v} must be refused"
            );
        }
        for (i, v) in [
            (7, 0),
            (7, 5),
            (8, 77),
            (8, 7),
            (10, 2),
            (11, 1),
            (11, 2),
            (15, -2),
            (24, 1),
            (25, 1),
            (28, 0),
        ] {
            assert!(check(i, v).is_ok(), "ICNTL({i}) = {v} must be accepted");
        }
        // upstream's own settings (`initMumpsData`, `setOutputLevel`)
        for i in [1, 2, 3, 4, 5, 8, 14, 18, 20, 21, 28] {
            if let Some(v) = upstream_icntl(i) {
                assert!(check(i, v).is_ok(), "upstream ICNTL({i}) = {v}");
            }
        }
        // The reserved indices of MUMPS 5.7.3 accept
        // only 0; the documented neighbours (35–39, 48, 49, 56, 58) stay
        // accepted
        for i in (40..=47).chain(50..=55).chain([57, 59, 60]) {
            for v in [1, -1, 7] {
                assert!(
                    matches!(check(i, v), Err(Error::Unsupported(m)) if m.contains("is reserved in MUMPS")),
                    "reserved ICNTL({i}) = {v} must be refused"
                );
            }
            assert!(
                check(i, 0).is_ok(),
                "reserved ICNTL({i}) = 0 is MUMPS's default"
            );
        }
        for i in [35, 36, 37, 38, 39, 48, 49, 56, 58] {
            assert!(
                check(i, 1).is_ok(),
                "documented ICNTL({i}) = 1 must be accepted"
            );
        }
    }

    /// The FFI-edge check refuses coordinates whose
    /// length differs from the matrix's entry count (shorter: MUMPS would
    /// read past the allocation; longer: wrong coordinates) — the local
    /// validation of every analysis/factorisation header.
    #[test]
    fn coordinates_are_checked_against_the_matrix() {
        let mut a = SparseMatrixAij::new("a", 3, MatrixType::Unsymmetric);
        for (i, j) in [(0, 0), (1, 1), (2, 2), (0, 2)] {
            a.add(i, j, 1.0).expect("entry");
        }
        let c = Coordinates::of(&a).expect("coordinates");
        assert_eq!((c.irn.len(), c.check(&a).ok()), (4, Some(3)));
        let mut shorter = SparseMatrixAij::new("b", 3, MatrixType::Unsymmetric);
        shorter.add(0, 0, 1.0).expect("entry");
        let e = c
            .check(&shorter)
            .err()
            .map(|e| e.to_string())
            .unwrap_or_default();
        assert!(
            e.contains("refused before MUMPS runs") && e.contains("4 row / 4 column"),
            "{e}"
        );
        let mut longer = a.clone();
        longer.add(1, 0, 1.0).expect("entry");
        assert!(c.check(&longer).is_err());
        let s = MumpsSolver::default();
        let plan = Plan {
            init: false,
            analyse: false,
            factorize: true,
        };
        assert!(s.check_reuse(&a, plan).is_err(), "no stored analysis");
    }

    /// The analysis key distinguishes matrices with
    /// equal release counters (and a clone, and a copied profile).
    #[test]
    fn analysis_key_identifies_the_matrix() {
        let build = |pairs: &[(i64, i64)]| {
            let mut m = SparseMatrixAij::new("m", 3, MatrixType::Unsymmetric);
            for &(i, j) in pairs {
                m.add(i, j, 1.0).expect("entry");
            }
            m
        };
        let a = build(&[(0, 1), (1, 2)]);
        let b = build(&[(1, 0), (2, 1)]);
        assert_eq!(
            (a.profile_release(), a.value_release(), a.nnz()),
            (b.profile_release(), b.value_release(), b.nnz())
        );
        assert_ne!(analysis_key(&a), analysis_key(&b));
        assert_ne!(analysis_key(&a), analysis_key(&a.clone()));
        let mut c = build(&[(0, 0), (2, 2)]);
        let before = analysis_key(&c);
        c.copy_profile(&b);
        assert_eq!(c.profile_release(), b.profile_release());
        assert_ne!(analysis_key(&c), before);
        assert_ne!(analysis_key(&c), analysis_key(&b));
        assert_eq!(analysis_key(&a), analysis_key(&a));
        assert_eq!(factor_key(&a), factor_key(&a));
    }
}
