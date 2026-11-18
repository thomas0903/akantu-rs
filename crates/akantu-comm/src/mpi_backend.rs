// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/synchronizer/communicator_mpi_inline_impl.hh,
// mpi_communicator_data.hh, communicator.cc (static communicator, MPI_Init /
// MPI_Finalize ownership), communicator_inline_impl.hh (receiveAnyNumber).

//! Real MPI backend of [`crate::Communicator`] (Cargo feature `mpi`).
//!
//! Built directly on rsmpi's raw bindings (`mpi::ffi`, i.e. `mpi-sys`)
//! because the safe rsmpi wrappers discard MPI return codes: akantu's
//! communicators use `MPI_ERRORS_RETURN` (`docs/ARCHITECTURE.md`) and every call
//! below checks its return code and translates a failure into
//! `Error::Fatal { kind: Transport }` (the communicator state is unknown; the
//! top-level runtime aborts, see [`crate::Communicator::abort_on_transport_fatal`]).
//! Exceptions: calls on a communicator akantu does not own yet — validating
//! and duplicating an imported host handle, duplicating `MPI_COMM_WORLD` —
//! run under that communicator's own error handler (`MPI_ERRORS_ARE_FATAL`
//! by default), which `MPI_ERRORS_RETURN` replaces only on akantu's
//! duplicate. A message longer than its posted receive is a wire-contract
//! error, detected by akantu (MPI receives always have the matched length).
//! Not every possible MPI implementation failure is
//! recoverable: after a transport error the runtime must abort.
//!
//! Design points (`docs/ARCHITECTURE.md`, docs/TESTING.md):
//!
//! * **Owned buffers.** Every in-flight operation (`MpiInFlight`) owns the
//!   memory MPI may access (data buffers, count and displacement arrays) and
//!   an [`MpiEnvironment`] guard; it is completed before the memory is
//!   released, or — after a timeout — leaked together with its guard, so a
//!   buffer is never freed while MPI may still access it and MPI is never
//!   finalised under a pending request.
//! * **Bytes on the wire.** Payloads travel as `MPI_UINT8_T` in akantu's
//!   little-endian packing ([`crate::buffer`]), as in the local world; counts
//!   are `int` (messages above `i32::MAX` bytes are rejected locally).
//! * **Intracommunicators only**. An imported handle is
//!   rejected if it is `MPI_COMM_NULL` or an intercommunicator
//!   (`MPI_Comm_test_inter`) before any collective is issued, and every
//!   adopted communicator is checked again: every size computation below
//!   assumes the local group is the whole group.
//! * **MPI never gets a receive buffer shorter than the matched message**.
//!   The pinned Open MPI 5.0.7 writes a truncated
//!   message past the posted receive buffer for self-messages of 4 KiB or
//!   more (btl `self`) and for inter-process messages of 64 KiB or more
//!   over TCP (btl `tcp`) (`docs/DEPENDENCIES.md`).
//!   Hence: (1) every send to the own rank, every receive and probe
//!   from the own rank, and the self part of an any-source probe go through
//!   the in-process `crate::self_channel::SelfChannel` (MPI's
//!   posting-order, truncation, synchronous-completion and cancellation
//!   semantics) — `MpiBackend::isend` and `MpiBackend::post_recv`
//!   refuse the own rank; (2) every receive from another rank — posted
//!   (`Communicator::irecv`) or dynamic (`receive_vec`) — is a slot of the
//!   `PostedEngine`, filled in posting order by `MPI_Improbe` +
//!   `MPI_Imrecv` into a buffer of exactly the matched length, so MPI never
//!   truncates; a message longer than its posted receive is delivered whole
//!   and reported by the caller as an ordinary truncation error. No
//!   `MPI_Irecv`/`MPI_Recv`/`MPI_Cancel` is used. The collectives write
//!   only lengths agreed in the header.
//! * **Posted receives**: posting-order matching whatever the wait
//!   order; receive progress for synchronous senders while the process
//!   waits in any akantu operation (every completion polls and runs the
//!   matching engine of every communicator with waiting slots); dropping an
//!   unmatched receive withdraws it, a matched message is received and
//!   discarded.
//! * **Matched dynamic receives** are slots without length limit,
//!   waited for at once under **one** operation deadline covering matching
//!   and payload (no blocking `MPI_Mprobe`/`MPI_Mrecv`, which could wait
//!   forever and, under `Serialized`, hold the process MPI lock).
//! * **Checked collectives**. Every blocking collective starts with
//!   a fixed-size header all-gather `(sequence, info, ok, descriptor)`, the
//!   descriptor being `crate::communicator::CollDesc` (operation, root,
//!   reduction operator, element type, agreed count, reduction policy):
//!   ranks in different collectives, disagreeing on the contract, or with a
//!   failed local validation (invalid root, unsupported operator, invalid
//!   root data, sizes beyond the int count limit) all fail **together**
//!   before any data collective is posted (no hang, communicator usable).
//!   The header has a fixed 64-byte payload slot: an all-gather, gather or
//!   broadcast whose contributions all fit (an agreement of the collective
//!   error protocol, a scalar reduction) completes with the header alone.
//! * **Reduction policies** (D-039). Rank-ordered (default): reductions,
//!   scans and their loc variants gather the contributions and fold them in
//!   rank order on every rank (`crate::communicator`), bitwise identical to
//!   the serial backend and the local world. Native: upstream's blocking
//!   `MPI_Allreduce`, `MPI_Reduce`, `MPI_Scan`, `MPI_Exscan` (in place) with
//!   the typed predefined datatypes and operators (`MPI_DOUBLE_INT`,
//!   `MPI_MINLOC`/`MPI_MAXLOC`, `MPI_IN_PLACE` obtained from a C shim
//!   compiled with `mpicc`, `src/mpi_shim.c`), after the checked header.
//! * **Optional progress deadline.** Waits always poll (`MPI_Test` /
//!   `MPI_Improbe`), paced per wait by `crate::pacing::Pacer`
//!   (spin 64 polls, then yield for 20 ms, then sleep a fixed
//!   250 µs per poll, never past the deadline; the effective sleep quantum
//!   depends on the kernel's timer slack, ≈ 3 ms in the OrbStack container);
//!   with
//!   [`MpiOptions::timeout`] they fail with a transport error at the
//!   deadline (tests' no-hang guarantee), without it they wait as long as
//!   MPI would.
//!   Duplication is non-blocking (`MPI_Comm_idup`, polled, deadline
//!   enforced). **Exceptions**: the native-policy
//!   reductions/scans and `MPI_Comm_split` are blocking MPI calls — MPI-3
//!   has no non-blocking split, and OpenMPI's non-blocking reductions
//!   (libnbc) use other algorithms than the blocking ones upstream calls —
//!   so after their checked header they have **no deadline** (a peer that
//!   stalls between the header and the call stalls them; a peer that dies
//!   is torn down by the launcher).
//! * **No lock held across a remote wait under `Serialized`**.
//!   Every wait polls and releases the process MPI lock between
//!   polls. The two blocking calls above are **exclusive operations**, and
//!   every communicator creation takes part in the same admission rule
//!   (Open MPI's blocking `MPI_Comm_split` also waits
//!   for the lower-id communicator creations in flight in its process):
//!   under `Serialized` a native reduction is refused while another native
//!   reduction or a committed split is in flight, a split is refused while
//!   a native reduction, another committed split or any `MPI_Comm_idup` is
//!   in flight, a duplication is refused while a committed split is in
//!   flight — each with an agreed (non-fatal) error on every rank — and
//!   `mpi_world()`/imports (no header) wait, without the MPI lock and
//!   bounded by their deadline, until no committed split is in flight (see
//!   `ExclusiveGuard` for the deadlock-freedom argument).
//! * **Emergency abort**. `MPI_Abort` is called only where the
//!   thread level permits it (see `Communicator::abort`); otherwise the
//!   process asks the launcher to abort the job through `PMIx_Abort` when a
//!   PMIx client library is loaded (a PMIx call, bounded wait) and
//!   terminates with `_exit(code)` without calling MPI. `PMIx_Abort` is
//!   resolved and its helper thread started **once, when MPI starts**
//!   (`MpiBackend::world`, `MpiBackend::import`), so
//!   the abort path does no symbol lookup, spawns no thread and allocates
//!   nothing before its bounded wait. The path
//!   takes neither the `Stderr` lock nor the `Stdout` lock:
//!   stdout is not flushed, so output buffered by Rust's line
//!   writer (an incomplete line) may be lost.
//! * **Thread policy.** Every call runs through the lifecycle's
//!   [`ThreadPolicy`] (main thread only under `Single`/`Funneled`,
//!   serialised under `Serialized`); `MPI_Comm_free` goes through
//!   [`Lifecycle::release_handle`] (deferred to the main thread when needed,
//!   always before `MPI_Finalize`).

#![allow(unsafe_code)] // FFI edge: every unsafe block states its invariants (`SAFETY:`).

use std::collections::{HashMap, VecDeque};
use std::ffi::{c_char, c_int};
use std::sync::atomic::{AtomicBool, AtomicI64, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, OnceLock, PoisonError};
use std::thread::ThreadId;
use std::time::{Duration, Instant};

use akantu_core::{Error, FatalKind, Rank, Result};
use mpi::ffi;

use crate::communicator::{
    CollDesc, CollOp, DESC_WORDS, HeaderStatus, MAX_MESSAGE_BYTES, ReductionPolicy, STATUS_WORDS,
    SendMode, check_entries, set_verdict, status_error, status_verdict,
};
use crate::lifecycle::{
    Lifecycle, MpiEnvironment, MpiRuntime, ThreadKey, ThreadLevel, ThreadPolicy,
};
use crate::pacing::Pacer;
use crate::self_channel::{SelfChannel, WaitError};

/// Largest tag every MPI implementation must accept (`MPI_TAG_UB >= 32767`).
pub const MAX_PORTABLE_TAG: i32 = 32767;

/// Options of an MPI communicator (inherited by duplicates and splits).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MpiOptions {
    /// Thread level required from MPI (`MPI_Init_thread`, or checked against
    /// an imported environment).
    pub thread_level: ThreadLevel,
    /// Progress deadline of the waits (`None`: block in MPI). A wait that
    /// reaches it fails with `Error::Fatal { kind: Transport }` and breaks
    /// the communicator; the pending operation is leaked (never freed under
    /// MPI's feet). It bounds point-to-point completions, probes, matched
    /// receives, messages to self, every checked-collective header and the
    /// payload of the rank-ordered collectives, broadcasts, gathers,
    /// scatters, barriers and duplications, and the admission wait of
    /// `mpi_world()` / imports for a split in flight under `Serialized`
    /// (its timeout is also `Error::Fatal { kind: Transport }`,
    /// but no communicator exists yet to break). **Not
    /// bounded** after their (bounded) headers: the
    /// blocking native-policy reductions and scans
    /// ([`ReductionPolicy::NativeMpi`]) and `MPI_Comm_split`
    /// ([`crate::Communicator::split`]) — a peer that stalls between the
    /// header and the call stalls them. **Paused**:
    /// under `Serialized`, while one of those blocking calls holds the
    /// process MPI lock, the polled waits of the process's other threads
    /// wait for the lock at their next poll, so their deadlines are only
    /// enforced once the blocking call has returned. The same holds for a
    /// native library collective or region
    /// ([`crate::Communicator::native_collective`],
    /// [`crate::Communicator::native_region`]: a PETSc `SNESSolve` may hold
    /// the lock for the whole solve) — and while a native
    /// registration lives, other threads' duplications are refused and
    /// their `mpi_world()` / imports wait for it up to this deadline (then
    /// fail fatally); on the registered thread itself those
    /// creations are refused at once.
    pub timeout: Option<Duration>,
    /// Point-to-point send mode: [`SendMode::Eager`] = standard mode
    /// (`MPI_Isend`; buffered or rendezvous at MPI's discretion),
    /// [`SendMode::Rendezvous`] = synchronous mode (`MPI_Issend`), which
    /// exposes any reliance on eager buffering.
    pub send_mode: SendMode,
    /// Reduction policy (D-039): [`ReductionPolicy::RankOrdered`] (default)
    /// or [`ReductionPolicy::NativeMpi`]. Part of every collective's
    /// descriptor (ranks with different policies fail together) and
    /// inherited by duplicates and splits.
    pub reduction: ReductionPolicy,
}

impl Default for MpiOptions {
    fn default() -> Self {
        Self {
            thread_level: ThreadLevel::Funneled,
            timeout: None,
            send_mode: SendMode::Eager,
            reduction: ReductionPolicy::RankOrdered,
        }
    }
}

// ---------------------------------------------------------------- runtime

/// Counters of MPI handle creation/destruction and of the finalisation
/// (evidence for the lifecycle acceptance tests).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HandleStats {
    /// Communicators created by akantu (`MPI_Comm_dup` / `MPI_Comm_split`).
    pub comms_created: usize,
    /// Communicators freed (`MPI_Comm_free`).
    pub comms_freed: usize,
    /// `comms_freed` at the moment akantu called `MPI_Finalize`.
    pub freed_at_finalize: Option<usize>,
    /// Thread that called `MPI_Finalize` (akantu-owned environments only).
    pub finalized_on: Option<ThreadId>,
}

static COMMS_CREATED: AtomicUsize = AtomicUsize::new(0);
static COMMS_FREED: AtomicUsize = AtomicUsize::new(0);
static FREED_AT_FINALIZE: AtomicUsize = AtomicUsize::new(usize::MAX);
static FINALIZED_ON: Mutex<Option<ThreadId>> = Mutex::new(None);

/// Current [`HandleStats`].
#[must_use]
pub fn handle_stats() -> HandleStats {
    let f = FREED_AT_FINALIZE.load(Ordering::SeqCst);
    HandleStats {
        comms_created: COMMS_CREATED.load(Ordering::SeqCst),
        comms_freed: COMMS_FREED.load(Ordering::SeqCst),
        freed_at_finalize: (f != usize::MAX).then_some(f),
        finalized_on: *FINALIZED_ON
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner),
    }
}

/// [`MpiRuntime`] over rsmpi: initialisation through
/// `mpi::initialize_with_threading` (rsmpi's process-wide lock prevents two
/// concurrent `MPI_Init_thread`), finalisation by dropping the rsmpi
/// `Universe` (`MPI_Finalize`), `MPI_Is_thread_main` through the raw
/// binding.
#[derive(Default)]
pub struct RsmpiRuntime {
    universe: Mutex<Option<mpi::environment::Universe>>,
}

impl std::fmt::Debug for RsmpiRuntime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RsmpiRuntime").finish_non_exhaustive()
    }
}

fn to_threading(level: ThreadLevel) -> mpi::Threading {
    match level {
        ThreadLevel::Single => mpi::Threading::Single,
        ThreadLevel::Funneled => mpi::Threading::Funneled,
        ThreadLevel::Serialized => mpi::Threading::Serialized,
        ThreadLevel::Multiple => mpi::Threading::Multiple,
    }
}

fn from_threading(t: mpi::Threading) -> ThreadLevel {
    match t {
        mpi::Threading::Single => ThreadLevel::Single,
        mpi::Threading::Funneled => ThreadLevel::Funneled,
        mpi::Threading::Serialized => ThreadLevel::Serialized,
        mpi::Threading::Multiple => ThreadLevel::Multiple,
    }
}

impl MpiRuntime for RsmpiRuntime {
    fn is_initialized(&self) -> bool {
        mpi::environment::is_initialized()
    }

    fn is_finalized(&self) -> bool {
        mpi::environment::is_finalized()
    }

    fn init_thread(&self, required: ThreadLevel) -> Result<ThreadLevel> {
        let (universe, provided) = mpi::initialize_with_threading(to_threading(required))
            .ok_or_else(|| {
                Error::Communication("MPI_Init_thread: MPI is already initialised".into())
            })?;
        *self
            .universe
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(universe);
        Ok(from_threading(provided))
    }

    fn query_thread(&self) -> ThreadLevel {
        from_threading(mpi::environment::threading_support())
    }

    fn is_thread_main(&self) -> bool {
        let mut flag: c_int = 0;
        // SAFETY: MPI is initialised (the lifecycle only asks after
        // `MPI_Initialized` returned true) and `flag` is a valid,
        // exclusively borrowed `int` for the duration of the call.
        let rc = unsafe { ffi::MPI_Is_thread_main(&raw mut flag) };
        rc == success() && flag != 0
    }

    fn finalize(&self) -> Result<()> {
        let universe = self
            .universe
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take()
            .ok_or_else(|| {
                Error::Communication("MPI_Finalize: MPI was not initialised by akantu".into())
            })?;
        FREED_AT_FINALIZE.store(COMMS_FREED.load(Ordering::SeqCst), Ordering::SeqCst);
        // the main thread's id cached at initialisation: `finalize` may run
        // in the process-exit hook, where `std::thread::current()` aborts
        *FINALIZED_ON
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = ThreadKey::cached_std_id();
        drop(universe); // rsmpi's Universe::drop calls MPI_Finalize
        Ok(())
    }
}

static LIFECYCLE: OnceLock<Lifecycle<RsmpiRuntime>> = OnceLock::new();

// ------------------------------------------------------------ process exit

/// Release callbacks of [`release_at_exit`], run in registration order by
/// the exit hook before the lifecycle's shutdown.
static EXIT_RELEASES: Mutex<Vec<fn() -> bool>> = Mutex::new(Vec::new());
/// `true` once the C library accepted the exit hook (set only after a
/// successful registration, so a refused one is retried).
static EXIT_HOOK_REGISTERED: Mutex<bool> = Mutex::new(false);
/// A panic happened since the exit hook was registered (D-067 (2)).
static PANICKED: AtomicBool = AtomicBool::new(false);
/// The panic hook that sets [`PANICKED`] is installed once per process.
static PANIC_FLAG_HOOK: std::sync::Once = std::sync::Once::new();

// SAFETY (declaration): glibc's `on_exit(3)` (GNU extension, declared in
// <stdlib.h>; used from its manual page only, D-030) has the C prototype
// `int on_exit(void (*function)(int, void *), void *arg);`. The declaration
// matches it: `extern "C" fn(c_int, *mut c_void)` is a non-null C function
// pointer taking `int` and `void *`, `*mut c_void` is `void *`, `c_int` is
// `int`. It registers `function` to be called at normal process
// termination with the status given to `exit` (or returned from `main`)
// and `arg`; the list is shared with `atexit` and run in reverse order of
// registration; it returns 0 on success, nonzero otherwise.
#[cfg(all(target_os = "linux", target_env = "gnu"))]
unsafe extern "C" {
    fn on_exit(
        function: extern "C" fn(c_int, *mut std::ffi::c_void),
        arg: *mut std::ffi::c_void,
    ) -> c_int;
}

// SAFETY (declaration): the C library's `atexit` (ISO C 7.22.4.2; POSIX),
// prototype `int atexit(void (*func)(void));`, matched by a non-null
// `extern "C" fn()` pointer and a `c_int` result; linked through libc by
// std. Used on targets without `on_exit` (macOS, musl).
#[cfg(not(all(target_os = "linux", target_env = "gnu")))]
unsafe extern "C" {
    fn atexit(callback: extern "C" fn()) -> c_int;
}

/// The `on_exit` callback: the exit status decides (D-067 (2)). A panic
/// must not unwind into the C library (it would abort the process): it is
/// caught and the hook's work is abandoned.
#[cfg(all(target_os = "linux", target_env = "gnu"))]
extern "C" fn run_exit_hook(status: c_int, _arg: *mut std::ffi::c_void) {
    let _ = std::panic::catch_unwind(|| exit_hook(Some(status)));
}

/// The `atexit` callback (no exit status on these targets); panics are
/// caught as in the `on_exit` callback.
#[cfg(not(all(target_os = "linux", target_env = "gnu")))]
extern "C" fn run_exit_hook() {
    let _ = std::panic::catch_unwind(|| exit_hook(None));
}

/// Whether the exit hook may finalise MPI (D-067 (2)): only after a normal
/// end — no panic since the hook was registered, and an exit status of 0
/// where the C library reports it (`on_exit`; `None` elsewhere).
fn normal_end(panicked: bool, status: Option<c_int>) -> bool {
    !panicked && status.is_none_or(|s| s == 0)
}

/// The process-exit hook (D-067): after a normal end, while MPI is
/// initialised and not finalised, run the release callbacks, then complete
/// the lifecycle's finalisation in exit mode (try-lock forms). When a lock
/// is held by another thread it gives up — no further MPI call, MPI stays
/// unfinalised, one emergency note on standard error.
fn exit_hook(status: Option<c_int>) {
    if !normal_end(PANICKED.load(Ordering::SeqCst), status) {
        // a panic or a failure exit: as upstream's uncaught exception
        // (`std::terminate`), MPI is not finalised; the launcher sees the
        // rank's abnormal end and tears the job down
        return;
    }
    let lifecycle = mpi_lifecycle();
    lifecycle.begin_exit();
    let runtime = lifecycle.runtime();
    // `MPI_Finalized` / `MPI_Initialized` may be called at any time, from
    // any thread. Finalised by the host (or by akantu): nothing to do, and
    // no MPI call may follow (D-067 (1)).
    if runtime.is_finalized() || !runtime.is_initialized() {
        return;
    }
    let releases = match EXIT_RELEASES.try_lock() {
        Ok(g) => g.clone(),
        Err(std::sync::TryLockError::Poisoned(p)) => p.into_inner().clone(),
        Err(std::sync::TryLockError::WouldBlock) => {
            exit_note("another thread is registering an exit release");
            return;
        }
    };
    for release in releases {
        if !release() {
            exit_note("the akantu runtime lock is held (by another thread, or by the exiting one)");
            return;
        }
    }
    // errors cannot be reported at exit; the launcher sees an unfinalised
    // MPI (documented failure modes of `release_at_exit`)
    let _ = lifecycle.shutdown();
    if let Some(blocker) = lifecycle.exit_blocker() {
        exit_note(blocker.describe());
    }
}

/// The exit hook's one emergency note (D-067 (3)): POSIX `write` to the
/// standard error descriptor, without std's `Stderr` lock.
fn exit_note(why: &str) {
    emergency_note(&format!(
        "akantu: MPI left unfinalised at exit: {why} (the launcher reports this rank; D-067)\n"
    ));
}

/// Chain a panic hook that records any panic (D-067 (2)) in front of the
/// current one, once per process. The previous hook still runs; it is not
/// restored (the flag must cover the whole rest of the process).
fn install_panic_flag() {
    PANIC_FLAG_HOOK.call_once(|| {
        if std::thread::panicking() {
            // `take_hook`/`set_hook` panic on a panicking thread; the
            // process is ending abnormally anyway
            PANICKED.store(true, Ordering::SeqCst);
            return;
        }
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            PANICKED.store(true, Ordering::SeqCst);
            previous(info);
        }));
    });
}

/// Register the C-level exit hook once per process (`on_exit` on glibc
/// Linux, `atexit` elsewhere). Returns `Ok(true)` if this call registered
/// it, `Ok(false)` if it already was; a refusal leaves it unregistered (a
/// later call tries again).
///
/// # Errors
/// `Error::Communication` if the C library refuses the registration.
pub(crate) fn register_exit_hook() -> Result<bool> {
    let mut registered = EXIT_HOOK_REGISTERED
        .lock()
        .unwrap_or_else(PoisonError::into_inner);
    if *registered {
        return Ok(false);
    }
    #[cfg(all(target_os = "linux", target_env = "gnu"))]
    // SAFETY: `on_exit` is glibc's registration function with the C
    // signature `int on_exit(void (*)(int, void *), void *)` (on_exit(3)),
    // which the declaration above matches: `run_exit_hook` is an
    // `extern "C" fn(c_int, *mut c_void)` that lives for the whole program
    // (a function item) and never unwinds into C (it catches panics); the
    // argument pointer is null and never dereferenced. Registration is
    // thread-safe (MT-Safe in the manual). Everything the callback reads
    // (`PANICKED`, `EXIT_RELEASES`, the lifecycle) is a static initialised
    // before or independently of the registration.
    let rc = unsafe { on_exit(run_exit_hook, std::ptr::null_mut()) };
    #[cfg(not(all(target_os = "linux", target_env = "gnu")))]
    // SAFETY: `atexit` is the C library's registration function
    // (`int atexit(void (*)(void))`); its argument is a non-unwinding
    // `extern "C" fn()` without arguments that lives for the whole program
    // (a function item), as `atexit` requires. Registration is permitted
    // from any thread. The callback catches panics instead of unwinding
    // into C and reads only statics.
    let rc = unsafe { atexit(run_exit_hook) };
    if rc != 0 {
        return Err(Error::Communication(format!(
            "the C library refused the MPI finalisation exit hook (returned {rc})"
        )));
    }
    *registered = true;
    Ok(true)
}

/// `true` once the process-exit hook is registered (by
/// [`release_at_exit`], i.e. by the facade's `runtime::initialize` with an
/// MPI communicator).
#[must_use]
pub fn exit_hook_registered() -> bool {
    *EXIT_HOOK_REGISTERED
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
}

/// Facade entry point of the process-exit finalisation (decision D-067):
/// the counterpart of upstream's
/// `std::atexit(finalize)` in `akantu::initialize` (`aka_common.cc:121`)
/// together with the destruction of the static communicator at exit,
/// which finalises MPI when akantu initialised it (`communicator.cc:58-62`).
/// The facade's `runtime::initialize` calls it once, with the release of
/// its static communicator; other code may register the release of its own
/// long-lived handles (each callback is kept; none can replace another).
///
/// At normal process termination (`exit`, or a return from `main`) the
/// hook runs the callbacks in registration order — each drops its handles
/// and returns `true`, or returns `false` if it cannot do so without
/// blocking (then the hook gives up) — and completes the lifecycle's
/// finalisation ([`Lifecycle::shutdown`]: models, native backends,
/// requests, communicators, `MPI_Finalize`, `docs/ARCHITECTURE.md`). It does
/// nothing:
///
/// * after an abnormal end (D-067 (2)), as upstream's uncaught exception
///   ends in `std::terminate` without finalising: on glibc Linux the hook
///   is registered with `on_exit` and finalises only for exit status 0
///   (a panicking `main` exits with 101, an `Err` from `main` with 1); on
///   every target, the first call of this function chains a panic hook in
///   front of the current one that records any panic (also a caught one,
///   or one on another thread) and the hook does nothing once one
///   happened. On targets without `on_exit` (macOS, musl) a non-zero
///   `exit` without a panic still finalises MPI (documented residual, the
///   same exposure as upstream's static destructor after `exit(1)`). A
///   panic hook installed later that does not call the previous one hides
///   panics from the flag;
/// * once MPI is finalised by anyone (D-067 (1)): a host that imported
///   akantu and finalised MPI itself (`MPI_Finalize` at the end of its
///   `main`, rsmpi's `Universe` drop, mpi4py at interpreter exit) gets no
///   MPI call at all from the hook;
/// * when a lock is held by another thread (D-067 (3)): the exit path
///   only tries the lifecycle lock, the wait for another drainer and the
///   `Serialized` MPI lock (callbacks must do the same with their own
///   locks); then MPI stays unfinalised, one emergency note goes to
///   standard error and the launcher reports the rank, as before the hook
///   existed.
///
/// The hook runs on the thread that terminates the process, after that
/// thread's thread-local destructors have run: it must not call
/// `std::thread::current()` or print through std's locked
/// `stdout`/`stderr`; the lifecycle's thread checks are safe there. Under
/// `Single`/`Funneled` a process terminated from another thread than MPI's
/// main thread leaves MPI unfinalised (MPI forbids finalising elsewhere,
/// D-023), and MPI is only finalised once the last handle is released, so
/// handles that are never dropped (leaked, owned by statics, alive at
/// `std::process::exit`) keep it alive as well.
///
/// **Unloading:** a shared object that contains akantu must not be
/// unloaded (`dlclose`) after an MPI `initialize`. On glibc the `on_exit`
/// handler of an unloaded object is *not* run at unload time and `exit`
/// later jumps into the unmapped code (measured on glibc 2.41,
/// SIGSEGV at exit); on `atexit` targets the hook runs at
/// unload time. Nothing in this workspace unloads akantu (CPython never
/// unloads extension modules).
///
/// # Errors
/// `Error::Communication` if the C library refuses the registration (the
/// callback is then not kept and a later call registers again).
pub fn release_at_exit(release: fn() -> bool) -> Result<()> {
    let mut releases = EXIT_RELEASES.lock().unwrap_or_else(PoisonError::into_inner);
    install_panic_flag();
    register_exit_hook()?;
    releases.push(release);
    Ok(())
}

/// The process-wide MPI lifecycle (`docs/ARCHITECTURE.md`). Statics are never
/// dropped: a pending finalisation is completed by the main thread's
/// [`Lifecycle::poll`] / [`Lifecycle::shutdown`] (facade exit, Python
/// `atexit`) or by the last guard dropped on the main thread.
pub fn mpi_lifecycle() -> &'static Lifecycle<RsmpiRuntime> {
    LIFECYCLE.get_or_init(|| Lifecycle::new(RsmpiRuntime::default()))
}

/// The number of processes of the job, `MPI_Comm_size(MPI_COMM_WORLD)`: a
/// **local** query — no collective call, no communicator created. MPI is
/// initialised by akantu at `thread_level` if it is not yet (owned), or
/// imported, as [`crate::Communicator::mpi_world`] does (an MPI job is
/// expected to initialise MPI on every process, as upstream's
/// `akantu::initialize` does). The returned guard keeps the environment
/// alive: dropping the last guard of an akantu-owned environment finalises
/// MPI (`docs/ARCHITECTURE.md`), so a caller that refuses something on this size
/// before any collective call (the PETSc DOF manager of a serial mesh)
/// keeps the guard for as long as MPI
/// must stay usable in the process.
///
/// # Errors
/// The lifecycle's initialisation errors (MPI already finalised, thread
/// level not provided); the thread policy's refusal (a call off MPI's main
/// thread under `Single` / `Funneled`).
pub fn world_size(
    thread_level: ThreadLevel,
) -> Result<(MpiEnvironment<'static, RsmpiRuntime>, usize)> {
    let env = mpi_lifecycle().initialize(thread_level)?;
    resolve_pmix_abort();
    let mut s: c_int = 0;
    let sp = &raw mut s;
    // SAFETY: MPI is initialised and not finalised while the guard `env`
    // lives, so the predefined `MPI_COMM_WORLD` is a valid communicator;
    // `sp` points to the live, exclusively borrowed `c_int` `s` for the
    // duration of the call; `MPI_Comm_size` is a local call. It runs under
    // the thread policy (`ThreadPolicy::call`: the main-thread check under
    // `Single` / `Funneled`, the process lock under `Serialized`).
    let rc = env
        .policy()
        .call(|| unsafe { ffi::MPI_Comm_size(comm_world(), sp) })?;
    check(rc, "MPI_Comm_size(MPI_COMM_WORLD)")?;
    let size = usize::try_from(s)
        .map_err(|_| Error::Communication("MPI_COMM_WORLD has a negative size".into()))?;
    Ok((env, size))
}

// ------------------------------------------------------------ raw handles

/// A raw communicator handle.
#[derive(Clone, Copy, Debug)]
struct RawComm(ffi::MPI_Comm);

// SAFETY: an MPI communicator handle is a process-wide identifier, not
// thread-affine memory; which threads may pass it to MPI is governed by
// MPI's thread level, enforced at every call site through `ThreadPolicy`
// (and `Lifecycle::release_handle` for its destruction). The emergency
// abort follows the same policy (`MpiBackend::abort`).
unsafe impl Send for RawComm {}
// SAFETY: see `Send`; the handle itself is never mutated through `&self`.
// Concurrent use is permitted by MPI only under `Multiple`; under
// `Serialized` every call takes the policy lock; callers must still order
// the collectives of one communicator (MPI's ordering rule).
unsafe impl Sync for RawComm {}

/// The communicator as handed to a native library inside
/// [`crate::Communicator::native_collective`] /
/// [`crate::Communicator::native_region`] (MUMPS takes the Fortran handle,
/// PETSc the C handle). Only valid during that call: a library keeping a
/// communicator must duplicate it (MUMPS at `JOB = -1`, PETSc's
/// `PetscCommDuplicate`), and the akantu communicator must outlive every
/// native object created on it (native objects hold a clone).
///
/// The capability is bound to the call: its lifetime is
/// the closure's (`for<'n> FnOnce(NativeComm<'n>)`), so it cannot be kept,
/// returned or stored outside the agreed native entry, and it names the
/// akantu communicator it was issued for ([`NativeComm::is_for`]): native
/// objects check that a capability is their own communicator's.
#[derive(Clone, Copy, Debug)]
pub struct NativeComm<'n> {
    raw: ffi::MPI_Comm,
    fortran: c_int,
    rank: Rank,
    size: usize,
    /// Identity of the issuing communicator (its shared state).
    owner: usize,
    _call: std::marker::PhantomData<&'n ()>,
}

impl NativeComm<'_> {
    /// `true` if this capability was issued for `comm` (or a clone of it):
    /// a duplicate or a split is another communicator, even when congruent.
    #[must_use]
    pub fn is_for(&self, comm: &crate::Communicator) -> bool {
        self.owner == comm.identity()
    }

    /// The C handle (`MPI_Comm`).
    #[must_use]
    pub fn raw(&self) -> ffi::MPI_Comm {
        self.raw
    }

    /// The Fortran handle (`MPI_Comm_c2f`, MUMPS `comm_fortran`).
    #[must_use]
    pub fn fortran(&self) -> i32 {
        self.fortran
    }

    /// Rank of this process in the communicator.
    #[must_use]
    pub fn rank(&self) -> Rank {
        self.rank
    }

    /// Number of processes of the communicator.
    #[must_use]
    pub fn size(&self) -> usize {
        self.size
    }
}

/// A raw request handle (owned by exactly one `MpiInFlight`).
#[derive(Debug)]
struct RawRequest(ffi::MPI_Request);

// SAFETY: as for `RawComm`: a request handle identifies an MPI-internal
// object; its use is serialised by its unique owner (no `Clone`, no `Sync`;
// testing or completing it needs `&mut`) and the thread policy. Moving the
// owner does not move the heap buffers MPI accesses.
unsafe impl Send for RawRequest {}

thread_local! {
    /// Native regions/collectives/admitted tickets of the current thread:
    /// while > 0 the thread holds a native
    /// registration (or needs none), so its own nested native calls and
    /// native-policy reductions are not admitted again, and its header-less
    /// communicator creations are refused at once.
    static NATIVE_DEPTH: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

fn native_depth() -> usize {
    NATIVE_DEPTH.with(std::cell::Cell::get)
}

/// `true` while the current thread is inside a native region, collective
/// or admitted ticket (a native library may communicate outside the
/// collective protocol: D-126 keeps every agreement there).
pub(crate) fn in_native_region() -> bool {
    native_depth() > 0
}

/// Marks the current thread as inside a native call (panic-safe).
struct NativeDepth;

impl NativeDepth {
    fn enter() -> Self {
        NATIVE_DEPTH.with(|d| d.set(d.get() + 1));
        Self
    }
}

impl Drop for NativeDepth {
    fn drop(&mut self) {
        NATIVE_DEPTH.with(|d| d.set(d.get().saturating_sub(1)));
    }
}

/// Local admission of a native region ([`crate::Communicator::native_ticket`]):
/// the verdict (thread policy, `Serialized` split-class admission) and the
/// registration it holds. Thread-confined (`!Send`): the region must run on
/// the thread that was admitted.
pub struct NativeTicket {
    issued_for: usize,
    registration: Option<ExclusiveGuard>,
    verdict: Result<()>,
    /// An admitted ticket marks its thread as holding a native
    /// registration from admission on: the agreement of
    /// its verdict and the region's own native-policy reductions are not
    /// refused by the ticket's registration.
    _depth: Option<NativeDepth>,
    _thread: std::marker::PhantomData<*const ()>,
}

impl std::fmt::Debug for NativeTicket {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NativeTicket")
            .field("registered", &self.registration.is_some())
            .field("verdict", &self.verdict)
            .finish_non_exhaustive()
    }
}

impl NativeTicket {
    pub(crate) fn refused(issued_for: usize, e: Error) -> Self {
        Self {
            issued_for,
            registration: None,
            verdict: Err(e),
            _depth: None,
            _thread: std::marker::PhantomData,
        }
    }

    pub(crate) fn issued_for(&self) -> usize {
        self.issued_for
    }

    /// The local verdict, to be agreed on every rank before
    /// [`crate::Communicator::native_region`] (a refusal is an
    /// `Error::Communication`, a non-MPI communicator `Error::Unsupported`).
    pub fn verdict(&self) -> Result<()> {
        match &self.verdict {
            Ok(()) => Ok(()),
            Err(Error::Unsupported(m)) => Err(Error::Unsupported(m.clone())),
            Err(Error::Communication(m)) => Err(Error::Communication(m.clone())),
            Err(e) => Err(Error::Communication(e.to_string())),
        }
    }
}

fn success() -> c_int {
    c_int::try_from(ffi::MPI_SUCCESS).unwrap_or(0)
}

fn byte_type() -> ffi::MPI_Datatype {
    // SAFETY: reading an immutable extern constant initialised by the rsmpi
    // C shim (a predefined MPI datatype handle).
    unsafe { ffi::RSMPI_UINT8_T }
}

fn comm_world() -> ffi::MPI_Comm {
    // SAFETY: reading an immutable extern constant of the rsmpi C shim.
    unsafe { ffi::RSMPI_COMM_WORLD }
}

fn comm_null() -> ffi::MPI_Comm {
    // SAFETY: reading an immutable extern constant of the rsmpi C shim.
    unsafe { ffi::RSMPI_COMM_NULL }
}

fn any_source() -> c_int {
    // SAFETY: reading an immutable extern constant of the rsmpi C shim.
    unsafe { ffi::RSMPI_ANY_SOURCE }
}

fn undefined() -> c_int {
    // SAFETY: reading an immutable extern constant of the rsmpi C shim.
    unsafe { ffi::RSMPI_UNDEFINED }
}

fn errors_return() -> ffi::MPI_Errhandler {
    // SAFETY: reading an immutable extern constant of the rsmpi C shim.
    unsafe { ffi::RSMPI_ERRORS_RETURN }
}

fn request_null() -> ffi::MPI_Request {
    // SAFETY: reading an immutable extern constant of the rsmpi C shim.
    unsafe { ffi::RSMPI_REQUEST_NULL }
}

fn message_null() -> ffi::MPI_Message {
    // SAFETY: reading an immutable extern constant of the rsmpi C shim.
    unsafe { ffi::RSMPI_MESSAGE_NULL }
}

fn empty_status() -> ffi::MPI_Status {
    // SAFETY: `MPI_Status` is a plain C struct of integer fields (public
    // `MPI_SOURCE`/`MPI_TAG`/`MPI_ERROR` plus implementation fields, checked
    // on the pinned OpenMPI 5 ABI); the all-zero bit pattern is a valid
    // value, and MPI overwrites it.
    unsafe { std::mem::zeroed() }
}

// The akantu C shim (`src/mpi_shim.c`, compiled with `mpicc` by build.rs):
// predefined handles that are C macros in `mpi.h` and not exported by the
// rsmpi shim, plus the C layout of the `MPI_DOUBLE_INT` pair. The
// declarations match the shim's C signatures; `MPI_Datatype`/`MPI_Op` are
// bindgen `#[repr(transparent)]` newtypes of the C handle types, `usize` is
// `size_t` and `*mut c_void` is `void *`. Every call states its own
// `SAFETY`.
unsafe extern "C" {
    fn akantu_mpi_double_int() -> ffi::MPI_Datatype;
    fn akantu_mpi_minloc() -> ffi::MPI_Op;
    fn akantu_mpi_maxloc() -> ffi::MPI_Op;
    fn akantu_double_int_size() -> usize;
    fn akantu_double_int_loc_offset() -> usize;
    fn akantu_mpi_in_place() -> *mut std::ffi::c_void;
    /// `MPI_Comm_c2f` (Fortran handle for native libraries).
    fn akantu_mpi_comm_c2f(comm: ffi::MPI_Comm) -> c_int;
    /// POSIX `_exit`: immediate process termination (emergency fallback).
    fn _exit(status: c_int) -> !;
    /// POSIX `dlsym` (the emergency fallback's optional `PMIx_Abort`).
    fn dlsym(handle: *mut std::ffi::c_void, symbol: *const c_char) -> *mut std::ffi::c_void;
    /// POSIX `write` (emergency diagnostics without locks or panics).
    fn write(fd: c_int, buf: *const std::ffi::c_void, count: usize) -> isize;
}

fn error_string(rc: c_int) -> String {
    const LEN: usize = ffi::MPI_MAX_ERROR_STRING as usize;
    let mut buf: [c_char; LEN + 1] = [0; LEN + 1];
    let mut len: c_int = 0;
    // SAFETY: `buf` provides MPI_MAX_ERROR_STRING writable characters (plus
    // a terminator) as the standard requires; `len` is a valid `int`.
    // `MPI_Error_string` is one of the always thread-safe routines.
    let r = unsafe { ffi::MPI_Error_string(rc, buf.as_mut_ptr(), &raw mut len) };
    if r != success() {
        return format!("MPI error code {rc}");
    }
    let n = usize::try_from(len).unwrap_or(0).min(LEN);
    let bytes: Vec<u8> = buf[..n]
        .iter()
        .map(|&c| u8::from_ne_bytes(c.to_ne_bytes()))
        .collect();
    format!("{} (code {rc})", String::from_utf8_lossy(&bytes))
}

/// A transport failure: `Error::Fatal { kind: Transport }`.
pub(crate) fn transport(msg: impl Into<String>) -> Error {
    Error::fatal(FatalKind::Transport, Error::Communication(msg.into()))
}

fn check(rc: c_int, what: &str) -> Result<()> {
    if rc == success() {
        Ok(())
    } else {
        Err(transport(format!("{what} failed: {}", error_string(rc))))
    }
}

/// `true` if `rc`'s error class is `MPI_ERR_TRUNCATE`.
fn is_truncation(policy: &ThreadPolicy, rc: c_int) -> bool {
    let mut class: c_int = 0;
    let c = &raw mut class;
    // SAFETY: `class` is a valid, exclusively borrowed `int` for the call.
    let r = policy.call(|| unsafe { ffi::MPI_Error_class(rc, c) });
    matches!(r, Ok(r) if r == success())
        && u32::try_from(class).is_ok_and(|c| c == ffi::MPI_ERR_TRUNCATE)
}

/// `v` with a real heap allocation behind its data pointer. An empty
/// `Vec`'s pointer is dangling and equals its alignment — `(void*)1` for
/// bytes, which is exactly OpenMPI's `MPI_IN_PLACE` (and could alias
/// another empty buffer): every buffer handed to MPI goes through here.
fn allocated<T>(mut v: Vec<T>) -> Vec<T> {
    if v.capacity() == 0 {
        v.reserve(1);
    }
    v
}

/// The header's inline slot for a payload: the payload and its length if
/// it fits in [`INLINE_BYTES`], else an empty slot.
fn inline_slot(payload: &Result<Vec<u8>>) -> ([u8; INLINE_BYTES], usize) {
    let mut slot = [0u8; INLINE_BYTES];
    match payload {
        Ok(p) if p.len() <= INLINE_BYTES => {
            slot[..p.len()].copy_from_slice(p);
            (slot, p.len())
        }
        _ => (slot, 0),
    }
}

/// `len` zero bytes, [`allocated`].
fn zeroed(len: usize) -> Vec<u8> {
    allocated(vec![0u8; len])
}

fn count(n: usize, what: &str) -> Result<c_int> {
    c_int::try_from(n).map_err(|_| {
        Error::InvalidArgument(format!(
            "{what}: {n} bytes exceed the MPI int count limit ({})",
            c_int::MAX
        ))
    })
}

fn check_tag(tag: i32) -> Result<c_int> {
    if (0..=MAX_PORTABLE_TAG).contains(&tag) {
        Ok(tag)
    } else {
        Err(Error::InvalidArgument(format!(
            "message tag {tag} outside the portable MPI range 0..={MAX_PORTABLE_TAG}"
        )))
    }
}

/// Outcome of completing a request.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Progress {
    /// Completed successfully (`status` filled).
    Done,
    /// Completed with `MPI_ERR_TRUNCATE` and deallocated by MPI: a
    /// receive got a longer message than its buffer (wire-contract error).
    Truncated,
    /// Still pending at the deadline.
    Pending,
}

/// Classify the return code of `MPI_Wait`/`MPI_Test`.
fn classify(
    policy: &ThreadPolicy,
    rc: c_int,
    req: &RawRequest,
    what: &str,
) -> Result<Option<Progress>> {
    if rc == success() {
        return Ok(None);
    }
    // A truncated receive completes (MPI deallocates the request); any
    // other error leaves the request state unknown.
    if req.0 == request_null() && is_truncation(policy, rc) {
        return Ok(Some(Progress::Truncated));
    }
    Err(transport(format!("{what} failed: {}", error_string(rc))))
}

/// Complete `req` by polling `MPI_Test` (bounded by `deadline`; the
/// `Serialized` lock is released between polls). Every poll also matches
/// the process's waiting posted receives ([`pump_all`]): posted receives are
/// akantu-level slots, matched whenever some thread
/// waits in akantu — as `MPI_Wait` would progress real posted receives.
fn complete(
    policy: &ThreadPolicy,
    req: &mut RawRequest,
    status: &mut ffi::MPI_Status,
    deadline: Option<Instant>,
    what: &str,
) -> Result<Progress> {
    let mut pacer = Pacer::new();
    loop {
        let mut flag: c_int = 0;
        let (p, f, s) = (&raw mut req.0, &raw mut flag, &raw mut *status);
        // SAFETY: `req` is an active (or null) request owned by the caller,
        // whose buffers outlive the call; `flag` and `status` are valid.
        let rc = policy
            .call(|| unsafe { ffi::MPI_Test(p, f, s) })
            .map_err(|e| transport(format!("{what}: {e}")))?;
        if let Some(p) = classify(policy, rc, req, what)? {
            return Ok(p);
        }
        if flag != 0 {
            return Ok(Progress::Done);
        }
        if deadline.is_some_and(|d| Instant::now() >= d) {
            return Ok(Progress::Pending);
        }
        pump_all();
        pacer.pause(deadline);
    }
}

// --------------------------------------------------------------- in flight

/// MPI operations of the process handed to MPI and not yet completed (or
/// leaked): pending work that needs MPI progress (a
/// thread waiting for a message to self keeps progressing MPI at a short
/// cadence while it is non-zero).
static ACTIVE_OPS: AtomicUsize = AtomicUsize::new(0);

/// One non-blocking MPI operation and every piece of memory it may access.
pub(crate) struct MpiInFlight {
    req: RawRequest,
    bufs: Vec<Vec<u8>>,
    ints: Vec<Vec<c_int>>,
    /// Completed (successfully, truncated, or leaked).
    done: bool,
    /// Counted in [`ACTIVE_OPS`] (handed to MPI, not yet done).
    active: bool,
    truncated: bool,
    status: ffi::MPI_Status,
    what: &'static str,
    timeout: Option<Duration>,
    /// Operation deadline fixed at creation (a matched receive continues
    /// the deadline of its matching probe); `None`: the
    /// communicator's timeout from the start of each completion.
    deadline: Option<Instant>,
    policy: ThreadPolicy,
    /// `MPI_Comm_idup`: the new handle MPI writes on completion (boxed: its
    /// address must not change while the request is active).
    comm_out: Option<Box<RawComm>>,
    /// Keeps MPI alive while the request is outstanding (`None` once
    /// leaked after a timeout: the guard is leaked too).
    env: Option<MpiEnvironment<'static, RsmpiRuntime>>,
    /// `Serialized` registration of an `MPI_Comm_idup`:
    /// released with the completed operation, kept for ever if
    /// the operation is leaked.
    hold: Option<ExclusiveGuard>,
}

impl std::fmt::Debug for MpiInFlight {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MpiInFlight")
            .field("what", &self.what)
            .field("done", &self.done)
            .finish_non_exhaustive()
    }
}

impl MpiInFlight {
    /// An operation owning `bufs`/`ints`, not yet started (see [`start`]).
    fn new(
        env: &MpiEnvironment<'static, RsmpiRuntime>,
        timeout: Option<Duration>,
        bufs: Vec<Vec<u8>>,
        ints: Vec<Vec<c_int>>,
        what: &'static str,
        deadline: Option<Instant>,
    ) -> Self {
        Self {
            req: RawRequest(request_null()),
            bufs,
            ints,
            done: false,
            active: false,
            truncated: false,
            status: empty_status(),
            what,
            timeout,
            deadline,
            policy: env.policy().clone(),
            comm_out: None,
            env: Some(env.clone()),
            hold: None,
        }
    }

    /// One `MPI_Test`; `true` once complete. Like `MPI_Test` for real
    /// posted receives, it also matches the process's waiting posted
    /// receives ([`pump_all`]) first.
    pub(crate) fn test(&mut self) -> Result<bool> {
        if self.done {
            return Ok(true);
        }
        pump_all();
        let mut flag: c_int = 0;
        let (p, f, s) = (&raw mut self.req.0, &raw mut flag, &raw mut self.status);
        // SAFETY: the request is active and owned by `self`, whose buffers
        // stay allocated until completion; `flag`/`status` are valid.
        let rc = self
            .policy
            .call(|| unsafe { ffi::MPI_Test(p, f, s) })
            .map_err(|e| transport(format!("{}: {e}", self.what)))?;
        match classify(&self.policy, rc, &self.req, self.what) {
            Ok(Some(_)) => {
                self.set_done();
                self.truncated = true;
            }
            Ok(None) if flag != 0 => self.set_done(),
            Ok(None) => {}
            Err(e) => {
                self.leak();
                return Err(e);
            }
        }
        Ok(self.done)
    }

    /// Complete (bounded by the deadline) and return the owned buffers. At
    /// the deadline the operation is leaked (buffers and guard) and a
    /// transport error is returned.
    pub(crate) fn wait(mut self) -> Result<Vec<Vec<u8>>> {
        self.finish()?;
        Ok(std::mem::take(&mut self.bufs))
    }

    /// Complete a matched receive: `Some(bytes)` (the received length), or
    /// `None` if MPI reported a truncation (`MPI_ERR_TRUNCATE`; impossible
    /// for the exact-length receives of this backend).
    pub(crate) fn wait_recv(mut self) -> Result<Option<Vec<u8>>> {
        self.finish()?;
        if self.truncated {
            return Ok(None);
        }
        let mut n: c_int = 0;
        let (sp, np) = (&raw const self.status, &raw mut n);
        // SAFETY: `status` is the status of the completed receive; `n` is a
        // valid `int`.
        let rc = self
            .policy
            .call(|| unsafe { ffi::MPI_Get_count(sp, byte_type(), np) })
            .map_err(|e| transport(format!("MPI_Get_count: {e}")))?;
        check(rc, "MPI_Get_count")?;
        let mut buf = self.bufs.pop().unwrap_or_default();
        let len = usize::try_from(n)
            .ok()
            .filter(|&l| l <= buf.len())
            .ok_or_else(|| transport("MPI_Get_count: undefined count"))?;
        buf.truncate(len);
        Ok(Some(buf))
    }

    /// Complete an `MPI_Comm_idup` (bounded by the deadline) and return the
    /// new communicator handle.
    fn wait_comm(mut self) -> Result<RawComm> {
        self.finish()?;
        match self.comm_out.take() {
            Some(c) if c.0 != comm_null() => Ok(*c),
            _ => Err(transport(format!("{}: no communicator created", self.what))),
        }
    }

    fn finish(&mut self) -> Result<()> {
        if self.done {
            return Ok(());
        }
        let deadline = self
            .deadline
            .or_else(|| self.timeout.map(|t| Instant::now() + t));
        match complete(
            &self.policy,
            &mut self.req,
            &mut self.status,
            deadline,
            self.what,
        ) {
            Ok(Progress::Done) => {
                self.set_done();
                Ok(())
            }
            Ok(Progress::Truncated) => {
                self.set_done();
                self.truncated = true;
                Ok(())
            }
            Ok(Progress::Pending) => {
                self.leak();
                Err(transport(format!(
                    "{} timed out after {:?} (operation leaked; the communicator is broken)",
                    self.what,
                    self.timeout.unwrap_or_default()
                )))
            }
            Err(e) => {
                // The request state is unknown: never free its memory.
                self.leak();
                Err(e)
            }
        }
    }

    /// Give up on an incomplete operation without ever freeing what MPI may
    /// still access: buffers, count arrays and the environment guard (MPI is
    /// then never finalised by akantu — the runtime aborts on the transport
    /// error). No MPI call is made.
    pub(crate) fn leak(&mut self) {
        if self.done {
            return;
        }
        std::mem::forget(std::mem::take(&mut self.bufs));
        std::mem::forget(std::mem::take(&mut self.ints));
        std::mem::forget(self.comm_out.take());
        if let Some(env) = self.env.take() {
            std::mem::forget(env);
        }
        if let Some(hold) = self.hold.take() {
            hold.leak();
        }
        // No progress is owed to a leaked operation (its family is broken).
        self.set_done();
    }

    /// Mark the operation complete (or given up).
    fn set_done(&mut self) {
        self.done = true;
        if std::mem::take(&mut self.active) {
            ACTIVE_OPS.fetch_sub(1, Ordering::SeqCst);
        }
    }
}

impl Drop for MpiInFlight {
    fn drop(&mut self) {
        // Completion before release (bounded by the deadline; leaked on
        // timeout). The communicator-level requests route this through
        // their communicator's classifier before getting here;
        // errors cannot be reported from Drop.
        let _ = self.finish();
    }
}

// ------------------------------------------------------ native reductions

/// MPI datatype and in-memory stride of a wire type code (D-039, native
/// policy). Little-endian packed scalars equal their native C layout (the
/// crate requires a little-endian target); `ValueLoc` is laid out as the
/// C struct `{ double; int; }` described by `MPI_DOUBLE_INT`.
fn native_type(policy: &ThreadPolicy, code: u8) -> Result<(ffi::MPI_Datatype, usize)> {
    // SAFETY: only reads immutable extern constants of the rsmpi C shim
    // (predefined datatype handles).
    let scalar = unsafe {
        match code {
            1 => Some((ffi::RSMPI_UINT8_T, 1)),
            2 => Some((ffi::RSMPI_INT8_T, 1)),
            3 => Some((ffi::RSMPI_INT32_T, 4)),
            4 => Some((ffi::RSMPI_UINT32_T, 4)),
            5 => Some((ffi::RSMPI_INT64_T, 8)),
            6 => Some((ffi::RSMPI_UINT64_T, 8)),
            7 => Some((ffi::RSMPI_FLOAT, 4)),
            8 => Some((ffi::RSMPI_DOUBLE, 8)),
            _ => None,
        }
    };
    if let Some(t) = scalar {
        return Ok(t);
    }
    let t = match code {
        9 => {
            let (size, off) = double_int_layout(policy)?;
            if off != 8 {
                return Err(Error::InvalidArgument(format!(
                    "native reduction: unsupported MPI_DOUBLE_INT layout (loc at offset {off})"
                )));
            }
            // SAFETY: the shim returns the predefined handle `MPI_DOUBLE_INT`.
            (unsafe { akantu_mpi_double_int() }, size)
        }
        _ => {
            return Err(Error::InvalidArgument(format!(
                "native reduction: no MPI datatype for type code {code}"
            )));
        }
    };
    Ok(t)
}

/// `(sizeof, offsetof(loc))` of the C pair, checked once against the
/// extent MPI reports for `MPI_DOUBLE_INT` (lower bound 0).
fn double_int_layout(policy: &ThreadPolicy) -> Result<(usize, usize)> {
    static LAYOUT: OnceLock<std::result::Result<(usize, usize), String>> = OnceLock::new();
    if let Some(l) = LAYOUT.get() {
        return l.clone().map_err(Error::InvalidArgument);
    }
    // SAFETY: shim accessors without arguments (C `sizeof`/`offsetof`).
    let (size, off) = unsafe { (akantu_double_int_size(), akantu_double_int_loc_offset()) };
    let (mut lb, mut extent): (ffi::MPI_Aint, ffi::MPI_Aint) = (0, 0);
    let (l, e) = (&raw mut lb, &raw mut extent);
    // SAFETY: the predefined handle `MPI_DOUBLE_INT` and two valid
    // `MPI_Aint` out-parameters.
    let rc = policy
        .call(|| unsafe { ffi::MPI_Type_get_extent(akantu_mpi_double_int(), l, e) })
        .map_err(|e| transport(format!("MPI_Type_get_extent: {e}")))?;
    check(rc, "MPI_Type_get_extent")?;
    let r = if lb == 0 && usize::try_from(extent).ok() == Some(size) {
        Ok((size, off))
    } else {
        Err(format!(
            "MPI_DOUBLE_INT extent {extent} (lb {lb}) differs from the C pair size {size}"
        ))
    };
    LAYOUT
        .get_or_init(|| r)
        .clone()
        .map_err(Error::InvalidArgument)
}

/// Predefined MPI operator of a reduction-operator descriptor code.
fn native_op(code: u64) -> Result<ffi::MPI_Op> {
    // SAFETY: only reads immutable extern constants of the rsmpi C shim, or
    // calls the akantu shim's argument-free accessors returning the
    // predefined `MPI_MINLOC`/`MPI_MAXLOC` handles.
    let op = unsafe {
        match code {
            1 => ffi::RSMPI_SUM,
            2 => ffi::RSMPI_MIN,
            3 => ffi::RSMPI_MAX,
            4 => ffi::RSMPI_PROD,
            5 => ffi::RSMPI_LAND,
            6 => ffi::RSMPI_BAND,
            7 => ffi::RSMPI_LOR,
            8 => ffi::RSMPI_BOR,
            9 => ffi::RSMPI_LXOR,
            10 => ffi::RSMPI_BXOR,
            11 => akantu_mpi_minloc(),
            12 => akantu_mpi_maxloc(),
            _ => {
                return Err(Error::InvalidArgument(format!(
                    "native reduction: no MPI operator for code {code}"
                )));
            }
        }
    };
    Ok(op)
}

/// akantu packing → MPI memory layout (only `ValueLoc` differs).
fn to_native(code: u8, stride: usize, packed: &[u8]) -> Vec<u8> {
    if code != 9 {
        return packed.to_vec();
    }
    let mut out = Vec::with_capacity(packed.len() / 12 * stride);
    for item in packed.as_chunks::<12>().0 {
        out.extend_from_slice(item);
        out.resize(out.len() + (stride - 12), 0);
    }
    out
}

/// MPI memory layout → akantu packing.
fn from_native(code: u8, stride: usize, native: &[u8]) -> Vec<u8> {
    if code != 9 {
        return native.to_vec();
    }
    native
        .chunks_exact(stride)
        .flat_map(|item| item[..12].iter().copied())
        .collect()
}

/// Start the non-blocking call `f` of `op` (which already owns every buffer
/// `f` passes to MPI); `f` receives the address of `op`'s request handle
/// (`op` is borrowed, so it does not move while MPI writes it). If the
/// thread policy refuses the call, nothing was handed to MPI and the
/// operation is marked complete (dropped without MPI calls); if MPI returns
/// an error from the initiation itself, the operation is **leaked**
/// (conservatively: MPI might retain a pointer to its buffers).
fn start(
    policy: &ThreadPolicy,
    op: &mut MpiInFlight,
    f: impl FnOnce(*mut ffi::MPI_Request) -> c_int,
) -> Result<()> {
    let what = op.what;
    let r = &raw mut op.req.0;
    let rc = match policy.call(|| f(r)) {
        Ok(rc) => rc,
        Err(e) => {
            op.done = true; // never started: nothing to complete
            return Err(transport(format!("{what}: {e}")));
        }
    };
    if let Err(e) = check(rc, what) {
        op.leak();
        return Err(e);
    }
    op.active = true;
    ACTIVE_OPS.fetch_add(1, Ordering::SeqCst);
    Ok(())
}

// --------------------------------------------------------- posted receives

/// Waiting posted-receive slots in the whole process (fast path of
/// [`pump_all`]).
static WAITING_SLOTS: AtomicUsize = AtomicUsize::new(0);

/// Engines with at least one waiting slot (strong references while they
/// wait; every waiting slot belongs to a live request or call holding its
/// communicator, so the communicator handle stays valid while listed).
static PUMPED: Mutex<Vec<Arc<PostedEngine>>> = Mutex::new(Vec::new());

/// Posted receives of one communicator: akantu-level
/// slots, never an MPI receive with a fixed buffer.
#[derive(Default)]
struct PostedState {
    /// Waiting (unmatched) slots per `(source, tag)`, in posting order.
    waiting: HashMap<(Rank, c_int), VecDeque<u64>>,
    /// Number of waiting slots (sum of the queue lengths).
    n_waiting: usize,
    /// Matched slots: the exact-length receive of their message (possibly
    /// still in flight).
    matched: HashMap<u64, MpiInFlight>,
    next: u64,
    /// A failed matching step (transport error), reported to every later
    /// operation on this communicator.
    failed: Option<String>,
}

/// Owner of the posted slot of a blocking receive ([`MpiBackend::recv`]):
/// while armed, dropping it (an unwinding wait)
/// abandons the slot — withdrawn if still waiting, its matched receive
/// leaked (no MPI wait inside an unwinding destructor). A poisoned engine
/// is left alone (the poison already stops every later use).
struct PostedSlotOwner<'a> {
    posted: &'a Arc<PostedEngine>,
    key: (Rank, c_int),
    slot: u64,
    armed: bool,
}

impl Drop for PostedSlotOwner<'_> {
    fn drop(&mut self) {
        if !self.armed {
            return;
        }
        if let Ok(mut st) = self.posted.lock() {
            self.posted.abandon(&mut st, self.key, self.slot);
        }
    }
}

/// The posted-receive matching engine of one communicator.
///
/// Open MPI 5.0.7 writes a truncated message past the posted receive buffer
/// for self-messages of 4 KiB or more (btl `self`) **and** for inter-process
/// messages of 64 KiB or more over TCP (btl `tcp`): any fixed-size
/// `MPI_Irecv` that a longer message can match is unsafe
/// (`docs/DEPENDENCIES.md`). This backend therefore never hands MPI
/// a receive buffer shorter than the matched message: a posted receive is a
/// **slot** queued per `(source, tag)` in posting order; the engine matches
/// arrived messages with `MPI_Improbe` (MPI's non-overtaking order per
/// source and tag) and receives each into a buffer of exactly its length
/// (`MPI_Get_count` + `MPI_Imrecv`), assigned to the oldest waiting slot of
/// its key. Truncation is then detected from the delivered length (an
/// ordinary error, message consumed) — MPI never truncates. The price:
/// a message longer than its posted receive is
/// allocated and received whole (≤ [`MAX_MESSAGE_BYTES`]) before the error;
/// a cap or discard path would need a receive shorter than the message,
/// which is exactly what this Open MPI overruns. Matched
/// (dynamic) receives are slots too, so posting order holds across both
/// kinds; probes run the engine first and never report a message that
/// belongs to a waiting slot.
///
/// **Progress.** The engine runs inside every akantu wait of the process
/// (every completion polls and calls [`pump_all`]), so a posted receive
/// lets a peer's synchronous send complete while this process waits in any
/// akantu operation — what `MPI_Wait` gives real posted receives.
/// Not during the blocking native-policy reductions and `MPI_Comm_split`
/// (documented limitation).
pub(crate) struct PostedEngine {
    comm: RawComm,
    policy: ThreadPolicy,
    env: MpiEnvironment<'static, RsmpiRuntime>,
    timeout: Option<Duration>,
    state: Mutex<PostedState>,
}

impl PostedEngine {
    fn lock(&self) -> Result<MutexGuard<'_, PostedState>> {
        self.state
            .lock()
            .map_err(|_| transport("poisoned posted-receive state"))
    }

    /// Add a waiting slot for `key` (posting order).
    fn post(self: &Arc<Self>, st: &mut PostedState, key: (Rank, c_int)) -> u64 {
        let slot = st.next;
        st.next += 1;
        st.waiting.entry(key).or_default().push_back(slot);
        st.n_waiting += 1;
        WAITING_SLOTS.fetch_add(1, Ordering::SeqCst);
        if st.n_waiting == 1
            && let Ok(mut g) = PUMPED.lock()
        {
            g.push(Arc::clone(self));
        }
        slot
    }

    /// Remove waiting slot `slot` of `key` (or the oldest one, `None`);
    /// returns the removed slot.
    fn unwait(
        self: &Arc<Self>,
        st: &mut PostedState,
        key: (Rank, c_int),
        slot: Option<u64>,
    ) -> Option<u64> {
        let q = st.waiting.get_mut(&key)?;
        let pos = match slot {
            Some(s) => q.iter().position(|&x| x == s)?,
            None => 0,
        };
        let removed = q.remove(pos)?;
        if q.is_empty() {
            st.waiting.remove(&key);
        }
        st.n_waiting -= 1;
        WAITING_SLOTS.fetch_sub(1, Ordering::SeqCst);
        if st.n_waiting == 0
            && let Ok(mut g) = PUMPED.lock()
        {
            g.retain(|e| !Arc::ptr_eq(e, self));
        }
        Some(removed)
    }

    /// Give up slot `slot`: withdrawn if waiting, its matched receive leaked
    /// (never freed under MPI's feet) if matched — after a transport error.
    fn abandon(self: &Arc<Self>, st: &mut PostedState, key: (Rank, c_int), slot: u64) {
        if self.unwait(st, key, Some(slot)).is_none()
            && let Some(mut op) = st.matched.remove(&slot)
        {
            op.leak();
        }
    }

    /// Match waiting slots with arrived messages, oldest slot of each key
    /// first. Called with the state locked; a failure is recorded (the
    /// message state is unknown) and returned.
    fn pump(self: &Arc<Self>, st: &mut PostedState) -> Result<()> {
        if let Some(f) = &st.failed {
            return Err(transport(f.clone()));
        }
        if st.n_waiting == 0 {
            return Ok(());
        }
        let keys: Vec<(Rank, c_int)> = st.waiting.keys().copied().collect();
        for key in keys {
            while st.waiting.contains_key(&key) {
                match self.match_one(key) {
                    Ok(None) => break,
                    Ok(Some(op)) => {
                        if let Some(slot) = self.unwait(st, key, None) {
                            st.matched.insert(slot, op);
                        }
                    }
                    Err(e) => {
                        st.failed = Some(e.to_string());
                        return Err(e);
                    }
                }
            }
        }
        Ok(())
    }

    /// `MPI_Improbe` for `key`; on a match, start `MPI_Imrecv` into a buffer
    /// of exactly the matched length.
    fn match_one(&self, (src, tag): (Rank, c_int)) -> Result<Option<MpiInFlight>> {
        let mut flag: c_int = 0;
        let mut msg = message_null();
        let mut status = empty_status();
        let (f, m, s, comm) = (&raw mut flag, &raw mut msg, &raw mut status, self.comm.0);
        // SAFETY: `comm` is valid while a slot waits (see `PUMPED`); the
        // out-parameters are valid; status and message are read only once
        // `flag` is set.
        let rc = self
            .policy
            .call(|| unsafe { ffi::MPI_Improbe(src, tag, comm, f, m, s) })
            .map_err(|e| transport(format!("MPI_Improbe: {e}")))?;
        check(rc, "MPI_Improbe")?;
        if flag == 0 {
            return Ok(None);
        }
        // From here on the matched message must be consumed: a failure
        // abandons it (transport error, the runtime aborts).
        let mut n: c_int = 0;
        let (sp, np) = (&raw const status, &raw mut n);
        // SAFETY: `status` is the status of the matched message.
        let rc = self
            .policy
            .call(|| unsafe { ffi::MPI_Get_count(sp, byte_type(), np) })
            .map_err(|e| transport(format!("MPI_Get_count: {e}")))?;
        check(rc, "MPI_Get_count")?;
        let len = usize::try_from(n)
            .ok()
            .filter(|&l| l <= MAX_MESSAGE_BYTES)
            .ok_or_else(|| transport("MPI_Get_count: undefined count"))?;
        let mut buf = zeroed(len);
        let b = buf.as_mut_ptr();
        let mut op = MpiInFlight::new(
            &self.env,
            self.timeout,
            vec![buf],
            Vec::new(),
            "MPI_Imrecv",
            None,
        );
        let m = &raw mut msg;
        // SAFETY: `b` has exactly `n` writable bytes — the matched message's
        // length, so MPI cannot truncate (no overrun on any transport) —
        // owned by `op` until completion; `msg` is the matched handle,
        // consumed by MPI_Imrecv; `r` points to `op`'s request.
        start(&self.policy, &mut op, |r| unsafe {
            ffi::MPI_Imrecv(b.cast(), n, byte_type(), m, r)
        })?;
        Ok(Some(op))
    }
}

/// Run the matching engine of every communicator of the process with a
/// waiting posted receive (engines busy on another thread are skipped;
/// failures are recorded in the engine and reported by its owner).
fn pump_all() {
    pump_engines(None);
}

/// [`pump_all`] except `skip` (just pumped by the caller).
fn pump_others(skip: &Arc<PostedEngine>) {
    pump_engines(Some(skip));
}

fn pump_engines(skip: Option<&Arc<PostedEngine>>) {
    if WAITING_SLOTS.load(Ordering::SeqCst) == 0 {
        return;
    }
    let engines: Vec<Arc<PostedEngine>> = match PUMPED.lock() {
        Ok(g) => g.clone(),
        Err(_) => return,
    };
    for e in engines {
        if skip.is_some_and(|k| Arc::ptr_eq(k, &e)) || e.policy.check().is_err() {
            continue;
        }
        if let Ok(mut st) = e.state.try_lock() {
            let _ = e.pump(&mut st);
        }
    }
}

// ---------------------------------------------------- exclusive operations

/// Operations of the `Serialized` admission rule
/// (see `ExclusiveGuard`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Exclusive {
    /// A blocking native-policy reduction or scan, registered before its
    /// checked header until the blocking call returned.
    Native,
    /// A **committed** `MPI_Comm_split`: registered after its validation
    /// header, before its commit header, until the blocking call returned.
    Split,
    /// A polled `MPI_Comm_idup` (`duplicate`, `mpi_world`, imports),
    /// registered before its header (if any) until the duplication
    /// completed — for ever if the operation was leaked.
    Idup,
}

/// Registered operations of the process (`Serialized` only).
#[derive(Debug)]
struct Registry {
    native: usize,
    split: usize,
    idup: usize,
}

impl Registry {
    /// The admission rule: `true` if `kind` may register now.
    fn admits(&self, kind: Exclusive) -> bool {
        match kind {
            Exclusive::Native => self.native == 0 && self.split == 0,
            Exclusive::Split => self.native == 0 && self.split == 0 && self.idup == 0,
            Exclusive::Idup => self.split == 0,
        }
    }

    fn count(&mut self, kind: Exclusive) -> &mut usize {
        match kind {
            Exclusive::Native => &mut self.native,
            Exclusive::Split => &mut self.split,
            Exclusive::Idup => &mut self.idup,
        }
    }
}

/// One mutex-protected state: every admission decision and registration is
/// atomic with respect to all three counters.
static REGISTRY: Mutex<Registry> = Mutex::new(Registry {
    native: 0,
    split: 0,
    idup: 0,
});
/// Signalled whenever a registration ends (waiting `Idup` admissions).
static REGISTRY_FREED: Condvar = Condvar::new();

/// The registry (plain counters, consistent even after a panic elsewhere).
fn registry() -> MutexGuard<'static, Registry> {
    REGISTRY.lock().unwrap_or_else(PoisonError::into_inner)
}

/// A registered operation of the `Serialized` admission rule.
///
/// **Why.** Under `Serialized` the process MPI lock is held for the whole
/// of an MPI call. Every wait of this backend polls and releases the lock
/// between polls, except two blocking calls: the native-policy reductions
/// and scans (upstream's blocking `MPI_Allreduce` & co.; OpenMPI's
/// non-blocking reductions use other algorithms) and `MPI_Comm_split`
/// (MPI-3 has no non-blocking split). Such a call holds the lock until it
/// returns. A blocking call waits for its peers to enter it — and Open MPI's
/// blocking `MPI_Comm_split` **also** waits for every communicator creation
/// already in flight in its process whose parent has a lower context id
/// (Open MPI 5.0.7 `ompi/communicator/comm_cid.c`: a context-id allocation
/// proceeds only while it is the process-wide lowest pending one,
/// `ompi_comm_cid_lowest_id`), e.g. a polled `MPI_Comm_idup` of
/// `duplicate()` or `mpi_world()` on another thread, which in turn needs
/// its peers to start it. The resulting cross-rank cycle (split ×
/// duplicate, split × `mpi_world()`) was reproduced.
///
/// **Rule** (one mutex-protected registry, [`Registry::admits`]):
///
/// * a native reduction is admitted iff no native reduction and no committed
///   split is registered; its verdict is a local validation result agreed
///   by its checked header (a refusal fails it on every rank with the same
///   non-fatal `Error::Communication`, the communicator stays usable);
/// * a split runs its validation header first, then **commits**: admitted
///   iff no native reduction, no committed split and no `MPI_Comm_idup` is
///   registered; the verdict is agreed by a second (commit) header, and
///   only a split committed on every rank calls `MPI_Comm_split`;
/// * a `duplicate()` is admitted iff no committed split is registered,
///   agreed by its header; it stays registered until its `MPI_Comm_idup`
///   completed (for ever if the operation was leaked after a timeout);
/// * `mpi_world()` and imports have no header to agree on a refusal: they
///   wait — without the MPI lock, bounded by their deadline — until no
///   committed split is registered, then register like a duplication. A
///   timeout of that wait is `Error::Fatal { kind: Transport }`:
///   nothing was handed to MPI on this rank, but its peers may
///   have been admitted and started their `MPI_Comm_idup`, which can then
///   only time out and leak; the creation's total time can reach about
///   twice the deadline (admission wait, then the duplication's own one).
///
/// **No fairness**: refusals are agreed and non-fatal,
/// but nothing orders contending creations, so under `Serialized` a split
/// can be refused repeatedly while other threads keep duplicating (and two
/// threads splitting can refuse each other many times). Callers creating
/// communicators from several threads serialise their creations per
/// process, or retry with randomised back-off.
///
/// A leaked duplication (after a timeout, kept registered for ever) also
/// refuses every later split of the process, and Open MPI orders later
/// duplications behind its context id, so they time out too:
/// after such a fatal transport error the process must abort.
///
/// Refused operations do not register.
///
/// **Deadlock freedom.** A wait that never ends needs a thread blocked in
/// MPI for ever while holding a process lock: only a blocking call `E`
/// (polled waits release the lock between polls, every other call of this
/// backend is local or non-blocking). `E` started only after its last
/// header succeeded, so every participant `P` of `E` was admitted on its
/// process and stays registered until its call returns.
///
/// 1. *Peers enter `E`.* `P` only needs its process's lock. A lock held for
///    ever there means another blocking call `F` registered on the same
///    process at the same time as `P` — excluded: native and committed
///    split exclude each other, and themselves.
/// 2. *No creation to wait for (split).* When `P` committed, no
///    `MPI_Comm_idup` was registered on its process, and none can start
///    while `P` is registered (duplications are refused, world/imports
///    wait); no other split is committed and a pending (uncommitted) split
///    has no context-id allocation in flight (it is in a polled header). So
///    the only context-id allocation in flight on every participant's
///    process is `E`'s own: Open MPI's lowest-id rule never delays it.
///    Registration covers the whole time an `MPI_Comm_idup` may be in flight
///    (from before its start until its completion is observed; a leaked one
///    for ever), so the counter never under-approximates.
/// 3. *Waiting admissions.* A world/import waiting for a committed split
///    `S` holds no lock and no registration; `S`'s commit header needs only
///    peers that passed its validation header (they reach their commit
///    attempt without waiting), and `S` itself completes by 1 and 2.
///
/// Under `Multiple` there is no process lock (Open MPI progresses
/// concurrent blocking calls) and only native-library collectives register
/// (as split class, refusing each other: one native job per process at a
/// time; agreed refusals, no lock, so no cycle); under
/// `Single`/`Funneled` only MPI's main thread calls MPI. MPI calls of a
/// host application outside akantu (including its own communicator
/// creations) are not covered. Native solver libraries join the rule
/// through `Communicator::native_collective` and
/// `Communicator::native_region`: their blocking calls
/// create communicators, so they register as [`Exclusive::Split`] after an
/// agreed admission; a native region's callbacks run on the registered
/// thread, which holds the (re-entrant) `Serialized` lock for the whole
/// region, so no other thread of the process can be inside MPI meanwhile
/// and nested native calls of that thread need no second registration. The
/// collective destruction of a native object in a destructor
/// (`Communicator::release_native`) cannot agree an admission and is
/// documented as an SPMD-ordering obligation of the caller.
struct ExclusiveGuard(Exclusive);

impl ExclusiveGuard {
    /// Register `kind` if the rule admits it now; otherwise a description
    /// of what is in flight.
    fn try_enter(kind: Exclusive) -> std::result::Result<Self, String> {
        let mut r = registry();
        if !r.admits(kind) {
            return Err(format!(
                "in flight in this process: {} native reduction(s)/scan(s), {} committed \
                 split(s), {} communicator duplication(s)",
                r.native, r.split, r.idup
            ));
        }
        *r.count(kind) += 1;
        Ok(Self(kind))
    }

    /// Register `kind` once the rule admits it, waiting (no MPI lock) at
    /// most until `deadline`; `None` at the deadline.
    fn enter_waiting(kind: Exclusive, deadline: Option<Instant>) -> Option<Self> {
        let mut r = registry();
        loop {
            if r.admits(kind) {
                *r.count(kind) += 1;
                return Some(Self(kind));
            }
            r = match deadline {
                None => REGISTRY_FREED
                    .wait(r)
                    .unwrap_or_else(PoisonError::into_inner),
                Some(d) => {
                    let now = Instant::now();
                    if now >= d {
                        return None;
                    }
                    REGISTRY_FREED
                        .wait_timeout(r, d - now)
                        .unwrap_or_else(PoisonError::into_inner)
                        .0
                }
            };
        }
    }

    /// Stay registered for ever: the MPI operation's state is unknown (a
    /// leaked operation or a failed blocking call).
    fn leak(self) {
        std::mem::forget(self);
    }
}

impl Drop for ExclusiveGuard {
    fn drop(&mut self) {
        let mut r = registry();
        let c = r.count(self.0);
        *c = c.saturating_sub(1);
        drop(r);
        REGISTRY_FREED.notify_all();
    }
}

// ----------------------------------------------------------------- backend

/// A communicator created by akantu (`MPI_Comm_dup`/`MPI_Comm_split`),
/// freed through the lifecycle with its last handle.
#[derive(Debug)]
struct OwnedComm {
    raw: RawComm,
}

impl Drop for OwnedComm {
    fn drop(&mut self) {
        let raw = self.raw;
        mpi_lifecycle().release_handle(move || {
            if raw.free() == success() {
                COMMS_FREED.fetch_add(1, Ordering::SeqCst);
            }
        });
    }
}

impl RawComm {
    /// `MPI_Comm_free` (called by [`OwnedComm`]'s queued destruction only).
    fn free(mut self) -> c_int {
        // SAFETY: `self` was created by MPI_Comm_dup/MPI_Comm_split and is
        // uniquely owned by one `OwnedComm`, whose drop queues exactly one
        // call; the lifecycle runs it under the thread policy and before
        // MPI_Finalize (the owner's guard is dropped after this enqueue).
        // MPI keeps the objects pending operations need alive internally.
        unsafe { ffi::MPI_Comm_free(&raw mut self.0) }
    }
}

/// Reject an intercommunicator: every size computation
/// of this backend assumes the local group is the whole group.
fn ensure_intra(policy: &ThreadPolicy, comm: RawComm, what: &str) -> Result<()> {
    let mut flag: c_int = 0;
    let f = &raw mut flag;
    // SAFETY: `comm` is a valid communicator handle (caller's contract) and
    // `flag` a valid `int`; `MPI_Comm_test_inter` is local.
    let rc = policy
        .call(|| unsafe { ffi::MPI_Comm_test_inter(comm.0, f) })
        .map_err(|e| transport(format!("MPI_Comm_test_inter: {e}")))?;
    check(rc, "MPI_Comm_test_inter")?;
    if flag != 0 {
        return Err(Error::InvalidArgument(format!(
            "{what}: intercommunicators are not supported (akantu needs an intracommunicator)"
        )));
    }
    Ok(())
}

/// Header words: sequence, info, ok, the descriptor, then the protocol
/// status (D-126, [`crate::communicator::HeaderStatus`]).
const HEADER_WORDS: usize = 3 + DESC_WORDS + STATUS_WORDS;
/// Bytes of the header words.
const HEADER_WORDS_BYTES: usize = 8 * HEADER_WORDS;
/// Inline payload slot of every header (zero-padded): a payload collective
/// whose contributions all fit (`info ≤ INLINE_BYTES` on every rank: an
/// agreement, a scalar reduction) completes with the header all-gather
/// alone, without its second MPI collective. Every rank sends the same
/// header size whatever it puts in the slot, so the header collectives
/// always match.
const INLINE_BYTES: usize = 64;
/// Bytes sent by every rank in the header all-gather.
const HEADER_BYTES: usize = HEADER_WORDS_BYTES + INLINE_BYTES;

/// Environment variable selecting how the header of every checked
/// collective is exchanged (D-126): `auto` (default), `allgather`,
/// `gather-broadcast`. It must be the same on every rank.
pub const ENV_COLLECTIVE_HEADER: &str = "AKANTU_COLLECTIVE_HEADER";

/// Communicator size from which `auto` exchanges headers by gather and
/// broadcast: below it Open MPI's linear all-gather (one round, size − 1
/// messages per rank) is as fast; from 64 ranks on its size (size − 1)
/// messages dominate (`docs/benchmarks/perf-campaign/net/`).
pub const HEADER_GATHER_FROM: usize = 64;

/// How the header of every checked collective is exchanged.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HeaderExchange {
    /// [`HeaderExchange::GatherBroadcast`] from [`HEADER_GATHER_FROM`]
    /// ranks, [`HeaderExchange::AllGather`] below.
    Auto,
    /// `MPI_Iallgather`.
    AllGather,
    /// `MPI_Igather` on rank 0, then `MPI_Ibcast` from rank 0.
    GatherBroadcast,
}

/// Process setting (0 = unread, then 1 + variant index).
static HEADER_EXCHANGE: AtomicU64 = AtomicU64::new(0);

/// Set how headers are exchanged (programs; the same on every rank,
/// before any communicator is used). Otherwise `AKANTU_COLLECTIVE_HEADER`
/// is read once (an invalid value counts as `auto`).
pub fn set_header_exchange(h: HeaderExchange) {
    let code = match h {
        HeaderExchange::Auto => 1,
        HeaderExchange::AllGather => 2,
        HeaderExchange::GatherBroadcast => 3,
    };
    HEADER_EXCHANGE.store(code, Ordering::Relaxed);
}

/// The header exchange in effect.
#[must_use]
pub fn header_exchange() -> HeaderExchange {
    let mut c = HEADER_EXCHANGE.load(Ordering::Relaxed);
    if c == 0 {
        c = match std::env::var(ENV_COLLECTIVE_HEADER)
            .ok()
            .as_deref()
            .map(str::trim)
        {
            Some("allgather") => 2,
            Some("gather-broadcast") => 3,
            _ => 1,
        };
        HEADER_EXCHANGE.store(c, Ordering::Relaxed);
    }
    match c {
        2 => HeaderExchange::AllGather,
        3 => HeaderExchange::GatherBroadcast,
        _ => HeaderExchange::Auto,
    }
}

/// Whether a communicator of `size` ranks exchanges headers by gather and
/// broadcast.
fn header_by_gather(size: usize) -> bool {
    match header_exchange() {
        HeaderExchange::AllGather => false,
        HeaderExchange::GatherBroadcast => size > 1,
        HeaderExchange::Auto => size >= HEADER_GATHER_FROM,
    }
}

/// Wait at most this long for the `Serialized` MPI lock before the
/// emergency abort falls back to process termination.
const ABORT_LOCK_WAIT: Duration = Duration::from_secs(2);

/// Wait at most this long for `PMIx_Abort` in the non-MPI fallback before
/// the process exits anyway.
const PMIX_ABORT_WAIT: Duration = Duration::from_secs(2);

/// One rank's handle of an MPI communicator.
pub(crate) struct MpiBackend {
    // Field order is drop order: the communicator's destruction is queued
    // before the guard is released.
    comm: OwnedComm,
    rank: Rank,
    size: usize,
    opts: MpiOptions,
    seq: AtomicU64,
    /// Messages to self: never handed to MPI.
    own: SelfChannel,
    /// Posted receives from other ranks.
    posted: Arc<PostedEngine>,
    env: MpiEnvironment<'static, RsmpiRuntime>,
}

impl std::fmt::Debug for MpiBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MpiBackend")
            .field("rank", &self.rank)
            .field("size", &self.size)
            .field("opts", &self.opts)
            .finish_non_exhaustive()
    }
}

impl MpiBackend {
    /// A duplicate of `MPI_COMM_WORLD`, initialising MPI if needed (owned)
    /// or importing it (mpi4py, host application).
    pub(crate) fn world(opts: MpiOptions) -> Result<Self> {
        let env = mpi_lifecycle().initialize(opts.thread_level)?;
        resolve_pmix_abort();
        let hold = Self::creation_without_header(&env, opts, "duplication of MPI_COMM_WORLD")?;
        Self::duplicate_raw(env, RawComm(comm_world()), opts, hold)
    }

    /// A duplicate of a host **intra**communicator (imported, e.g. from
    /// mpi4py). `MPI_COMM_NULL` and intercommunicators are rejected before
    /// any collective.
    ///
    /// # Safety
    ///
    /// `raw` is `MPI_COMM_NULL` or a valid communicator handle that stays
    /// valid for the duration of the call.
    pub(crate) unsafe fn import(raw: ffi::MPI_Comm, opts: MpiOptions) -> Result<Self> {
        if raw == comm_null() {
            return Err(Error::InvalidArgument(
                "cannot import MPI_COMM_NULL as a communicator".into(),
            ));
        }
        let env = mpi_lifecycle().initialize(opts.thread_level)?;
        resolve_pmix_abort();
        ensure_intra(env.policy(), RawComm(raw), "communicator import")?;
        let hold = Self::creation_without_header(&env, opts, "communicator import")?;
        Self::duplicate_raw(env, RawComm(raw), opts, hold)
    }

    /// `Serialized` admission of a duplication without a header
    /// (`mpi_world()`, imports, `ExclusiveGuard`): wait,
    /// without the MPI lock and bounded by the options' deadline, until no
    /// committed split is in flight in the process, then register.
    ///
    /// At the deadline nothing was handed to MPI on **this** rank, but the
    /// outcome is not agreed (no header): peers may already have been
    /// admitted and started their `MPI_Comm_idup`, which can then only time
    /// out and leak. The timeout is therefore
    /// `Error::Fatal { kind: Transport }` — the job must abort
    /// (`abort_on_transport_fatal`), never retry the creation.
    fn creation_without_header(
        env: &MpiEnvironment<'static, RsmpiRuntime>,
        opts: MpiOptions,
        what: &str,
    ) -> Result<Option<ExclusiveGuard>> {
        if env.policy().provided() != ThreadLevel::Serialized {
            return Ok(None);
        }
        if native_depth() > 0 {
            // This thread holds a native registration (a
            // region or collective in progress, or an admitted ticket),
            // which counts as a committed split for the rule and ends only
            // after this call returns: waiting would only run into the
            // deadline. Refused at once; nothing was handed to MPI. Callers
            // in a region's callbacks run in SPMD order, so every rank's
            // region thread is refused the same way.
            return Err(Error::Communication(format!(
                "{what}: refused under MPI_THREAD_SERIALIZED: this thread is inside a native \
                 library region or collective (its registration counts as a committed split \
                 until the region ends); create communicators before or \
                 after native regions"
            )));
        }
        let deadline = opts.timeout.map(|t| Instant::now() + t);
        ExclusiveGuard::enter_waiting(Exclusive::Idup, deadline)
            .map(Some)
            .ok_or_else(|| {
                transport(format!(
                    "{what}: timed out after {:?} waiting for a split in flight in this process \
                     (MPI_THREAD_SERIALIZED admission rule); nothing was \
                     handed to MPI on this rank, but peers may have started their duplication \
                     (outcome not agreed): abort the job",
                    opts.timeout.unwrap_or_default()
                ))
            })
    }

    /// Duplicate `parent` with the non-blocking `MPI_Comm_idup` (MPI-3),
    /// completed by polling under the progress deadline:
    /// no process MPI lock is held while peers are awaited.
    /// `hold` is the operation's `Serialized` registration,
    /// owned by the in-flight operation: released once it
    /// completed (or was never handed to MPI), kept for ever if leaked.
    fn duplicate_raw(
        env: MpiEnvironment<'static, RsmpiRuntime>,
        parent: RawComm,
        opts: MpiOptions,
        hold: Option<ExclusiveGuard>,
    ) -> Result<Self> {
        let mut op = MpiInFlight::new(
            &env,
            opts.timeout,
            Vec::new(),
            Vec::new(),
            "MPI_Comm_idup",
            None,
        );
        op.hold = hold;
        let o = &raw mut op.comm_out.insert(Box::new(RawComm(comm_null()))).0;
        // SAFETY: `parent` is a valid intracommunicator (MPI_COMM_WORLD, an
        // akantu communicator, or a host handle checked by `ensure_intra`
        // under `import`'s contract). `o` points into the box owned by `op`
        // (heap address unchanged by moves), where MPI writes the new handle
        // on completion; it is kept until completion or leaked with the
        // request. `r` points to `op`'s request. A failure is handled by the
        // parent's error handler (the duplicate gets `MPI_ERRORS_RETURN` in
        // `adopt`).
        start(env.policy(), &mut op, |r| unsafe {
            ffi::MPI_Comm_idup(parent.0, o, r)
        })?;
        let out = op.wait_comm()?;
        COMMS_CREATED.fetch_add(1, Ordering::SeqCst);
        Self::adopt(env, out, opts)
    }

    /// Take ownership of a freshly created communicator: freed on every
    /// path from here on.
    fn adopt(
        env: MpiEnvironment<'static, RsmpiRuntime>,
        raw: RawComm,
        opts: MpiOptions,
    ) -> Result<Self> {
        let comm = OwnedComm { raw };
        let policy = env.policy().clone();
        let call = |what: &str, f: &mut dyn FnMut() -> c_int| -> Result<()> {
            let rc = policy
                .call(f)
                .map_err(|e| transport(format!("{what}: {e}")))?;
            check(rc, what)
        };
        // SAFETY: `raw` is the valid communicator just created (owned by
        // `comm` from the first line on); the predefined error handler is an
        // immutable shim constant.
        call("MPI_Comm_set_errhandler", &mut || unsafe {
            ffi::MPI_Comm_set_errhandler(raw.0, errors_return())
        })?;
        // Defence in depth: the size below is the receive size of
        // every header all-gather, valid for intracommunicators only.
        ensure_intra(&policy, raw, "adopted communicator")?;
        let (mut r, mut s): (c_int, c_int) = (0, 0);
        let (rp, sp) = (&raw mut r, &raw mut s);
        // SAFETY: `raw` is the valid communicator just created (owned by
        // `comm`); `rp` points to the live local `r` for the call.
        call("MPI_Comm_rank", &mut || unsafe {
            ffi::MPI_Comm_rank(raw.0, rp)
        })?;
        // SAFETY: as above; `sp` points to the live local `s` for the call.
        call("MPI_Comm_size", &mut || unsafe {
            ffi::MPI_Comm_size(raw.0, sp)
        })?;
        Ok(Self {
            comm,
            rank: r,
            size: usize::try_from(s).map_err(|_| transport("negative communicator size"))?,
            opts,
            seq: AtomicU64::new(0),
            own: SelfChannel::default(),
            posted: Arc::new(PostedEngine {
                comm: raw,
                policy: env.policy().clone(),
                env: env.clone(),
                timeout: opts.timeout,
                state: Mutex::new(PostedState::default()),
            }),
            env,
        })
    }

    pub(crate) fn rank(&self) -> Rank {
        self.rank
    }

    pub(crate) fn size(&self) -> usize {
        self.size
    }

    pub(crate) fn options(&self) -> MpiOptions {
        self.opts
    }

    pub(crate) fn policy(&self) -> &ThreadPolicy {
        self.env.policy()
    }

    fn raw(&self) -> ffi::MPI_Comm {
        self.comm.raw.0
    }

    fn deadline(&self) -> Option<Instant> {
        self.opts.timeout.map(|t| Instant::now() + t)
    }

    /// One MPI call under the thread policy, return code checked.
    fn call(&self, what: &str, f: impl FnOnce() -> c_int) -> Result<()> {
        let rc = self
            .env
            .policy()
            .call(f)
            .map_err(|e| transport(format!("{what}: {e}")))?;
        check(rc, what)
    }

    /// An in-flight operation owning `bufs`/`ints`, not yet started.
    fn in_flight(
        &self,
        bufs: Vec<Vec<u8>>,
        ints: Vec<Vec<c_int>>,
        what: &'static str,
        deadline: Option<Instant>,
    ) -> MpiInFlight {
        MpiInFlight::new(&self.env, self.opts.timeout, bufs, ints, what, deadline)
    }

    /// [`start`] under this communicator's thread policy.
    fn start(
        &self,
        op: &mut MpiInFlight,
        f: impl FnOnce(*mut ffi::MPI_Request) -> c_int,
    ) -> Result<()> {
        start(self.env.policy(), op, f)
    }

    /// Apply the `Serialized` admission rule to `kind`
    /// (`ExclusiveGuard`): the verdict is a local validation
    /// result, agreed through the caller's header; a refused operation does
    /// not register. Other thread levels: always admitted, nothing
    /// registered.
    ///
    /// A native-policy reduction of a thread that holds a native
    /// registration (inside a native region or collective, or holding an
    /// admitted ticket) is covered by that registration:
    /// the registration already excludes every other exclusive operation of
    /// the process — at its admission none was in flight, and none is
    /// admitted while it lives — so the thread's own blocking reductions
    /// are the only exclusive calls of its process, which is all the
    /// deadlock-freedom argument needs (step 1). Splits and duplications of
    /// such a thread stay refused (agreed by their headers).
    fn admit(&self, kind: Exclusive, what: &str) -> (Option<ExclusiveGuard>, Result<()>) {
        if self.policy().provided() != ThreadLevel::Serialized {
            return (None, Ok(()));
        }
        if kind == Exclusive::Native && native_depth() > 0 {
            return (None, Ok(()));
        }
        match ExclusiveGuard::try_enter(kind) {
            Ok(g) => (Some(g), Ok(())),
            Err(busy) => (
                None,
                Err(Error::Communication(format!(
                    "{what}: refused under MPI_THREAD_SERIALIZED: another thread of this process \
                     is in a conflicting MPI operation ({busy}); native-policy reductions/scans \
                     and splits must not overlap, and a split must not overlap a communicator \
                     creation"
                ))),
            ),
        }
    }

    /// Admission of a native-library collective (`native_collective`):
    /// split class under `Serialized` (the rule above), and under
    /// `Multiple` the same registration so that **two native-library jobs
    /// never run at the same time in one process**
    /// (MUMPS, ScaLAPACK/BLACS and OpenBLAS make no promise for concurrent
    /// instances on several threads). Under `Multiple` nothing else
    /// registers, so only native-library collectives refuse each other; a
    /// refusal is a local validation result agreed by the native header (an
    /// ordinary error on every rank, retry with back-off), never a wait on a
    /// process lock (which would recreate the cross-rank cycle described
    /// above). `Single`/`Funneled`: one MPI thread, nothing to register.
    ///
    /// A thread already inside a native region or collective (or holding
    /// its admitted ticket) is covered by that registration: nested native
    /// calls of the same thread are not admitted again.
    fn admit_native(&self, what: &str) -> (Option<ExclusiveGuard>, Result<()>) {
        if native_depth() > 0 {
            return (None, Ok(()));
        }
        match self.policy().provided() {
            ThreadLevel::Serialized => self.admit(Exclusive::Split, what),
            ThreadLevel::Multiple => match ExclusiveGuard::try_enter(Exclusive::Split) {
                Ok(g) => (Some(g), Ok(())),
                Err(busy) => (
                    None,
                    Err(Error::Communication(format!(
                        "{what}: refused under MPI_THREAD_MULTIPLE: another thread of this \
                         process is in a native library collective ({busy}); native library \
                         jobs (MUMPS, PETSc) of one process never run concurrently: \
                         retry, or drive native solvers from one thread"
                    ))),
                ),
            },
            _ => (None, Ok(())),
        }
    }

    // ------------------------------------------------------ point to point

    /// Refuse the own rank on an MPI point-to-point path:
    /// messages to self must go through the self channel.
    fn not_self(&self, peer: Rank, what: &str) -> Result<()> {
        if peer == self.rank {
            return Err(transport(format!(
                "{what}: internal error: a message to self must not be handed to MPI \
                 (self channel)"
            )));
        }
        Ok(())
    }

    /// Post a send of `bytes` to **another** rank (owned by the returned
    /// operation; messages to self use [`MpiBackend::send_to_self`]).
    pub(crate) fn isend(
        &self,
        bytes: Vec<u8>,
        dest: Rank,
        tag: i32,
        synchronous: bool,
    ) -> Result<MpiInFlight> {
        self.not_self(dest, "MPI_Isend")?;
        let n = count(bytes.len(), "send")?;
        let tag = check_tag(tag)?;
        let bytes = allocated(bytes);
        let synchronous = synchronous || self.opts.send_mode == SendMode::Rendezvous;
        let what = if synchronous {
            "MPI_Issend"
        } else {
            "MPI_Isend"
        };
        let buf = bytes.as_ptr();
        let mut op = self.in_flight(vec![bytes], Vec::new(), what, None);
        let comm = self.raw();
        // SAFETY: `buf` points to `n` initialised bytes owned by `op` (heap
        // allocation unchanged by the move into it) and kept until
        // completion; `dest` was range checked by the caller; `r` points to
        // `op`'s request, which is not moved during the call.
        self.start(&mut op, |r| unsafe {
            if synchronous {
                ffi::MPI_Issend(buf.cast(), n, byte_type(), dest, tag, comm, r)
            } else {
                ffi::MPI_Isend(buf.cast(), n, byte_type(), dest, tag, comm, r)
            }
        })?;
        Ok(op)
    }

    /// Post a receive from **another** rank `source` with `tag`
    /// (receives from self use
    /// [`MpiBackend::post_from_self`]): a slot of the matching engine
    /// (`PostedEngine`), matched in posting order by an exact-length
    /// receive — at once if the message already arrived. The posted length
    /// is the caller's: a longer message is delivered whole and reported as
    /// truncated by the caller. Returns the slot.
    pub(crate) fn post_recv(&self, source: Rank, tag: i32) -> Result<u64> {
        self.not_self(source, "posted receive")?;
        let key = (source, check_tag(tag)?);
        let mut st = self.posted.lock()?;
        let slot = self.posted.post(&mut st, key);
        if let Err(e) = self.posted.pump(&mut st) {
            self.posted.abandon(&mut st, key, slot);
            return Err(e);
        }
        Ok(slot)
    }

    /// Wait for the message of the posted slot `slot` (whole message, its
    /// length checked by the caller), under **one** deadline covering
    /// matching and payload. At the deadline the slot is
    /// withdrawn (or its matched receive leaked) and a transport error is
    /// returned.
    pub(crate) fn wait_posted(&self, slot: u64, source: Rank, tag: i32) -> Result<Vec<u8>> {
        let key = (source, check_tag(tag)?);
        let deadline = self.deadline();
        let mut pacer = Pacer::new();
        loop {
            let matched = {
                let mut st = self.posted.lock()?;
                let pumped = self.posted.pump(&mut st);
                match st.matched.remove(&slot) {
                    Some(op) => op,
                    None => {
                        let waiting = st.waiting.get(&key).is_some_and(|q| q.contains(&slot));
                        if let Err(e) = pumped {
                            self.posted.abandon(&mut st, key, slot);
                            return Err(e);
                        }
                        if !waiting {
                            return Err(transport(format!(
                                "posted receive slot {slot} (tag {tag} from rank {source}) is unknown"
                            )));
                        }
                        if deadline.is_some_and(|d| Instant::now() >= d) {
                            self.posted.abandon(&mut st, key, slot);
                            return Err(transport(format!(
                                "rank {}: timed out after {:?} waiting for a message with tag {tag} \
                                 from rank {source} (the communicator is broken)",
                                self.rank,
                                self.opts.timeout.unwrap_or_default(),
                            )));
                        }
                        drop(st);
                        // This engine was just pumped.
                        pump_others(&self.posted);
                        pacer.pause(deadline);
                        continue;
                    }
                }
            };
            let mut op = matched;
            op.deadline = deadline;
            return op
                .wait_recv()?
                .ok_or_else(|| transport("MPI_Imrecv: matched message truncated"));
        }
    }

    /// Cancel the posted slot `slot` (the request was dropped): a waiting
    /// slot is withdrawn (the next message goes to later receives); a
    /// matched one is completed (bounded by the deadline) and its message
    /// discarded — `MPI_Cancel` semantics. On a broken communicator family
    /// (`broken`) a matched receive is leaked instead of waited for.
    pub(crate) fn cancel_posted(
        &self,
        slot: u64,
        source: Rank,
        tag: i32,
        broken: bool,
    ) -> Result<()> {
        let key = (source, check_tag(tag)?);
        let matched = {
            let mut st = self.posted.lock()?;
            if self.posted.unwait(&mut st, key, Some(slot)).is_some() {
                return Ok(());
            }
            st.matched.remove(&slot)
        };
        match matched {
            None => Ok(()),
            Some(mut op) if broken => {
                op.leak();
                Ok(())
            }
            Some(op) => op.wait().map(|_| ()),
        }
    }

    /// Matched receive of the next message from `source` with `tag`,
    /// whatever its length: a posted slot without length limit, waited for
    /// at once (posting order with the posted receives holds; one deadline
    /// for matching and payload). From self: the next
    /// queued self-message.
    ///
    /// The posted slot is owned by a drop guard for the whole wait:
    /// `wait_posted` removes the slot on every return path,
    /// and if the wait unwinds instead (a panic between `post_recv` and the
    /// slot's removal, outside the engine lock), the guard abandons it, so
    /// the engine never keeps a waiting slot without an owner (K2 (b)).
    pub(crate) fn recv(&self, source: Rank, tag: i32) -> Result<Vec<u8>> {
        if source == self.rank {
            return self.recv_from_self(tag);
        }
        let key = (source, check_tag(tag)?);
        let slot = self.post_recv(source, tag)?;
        let mut owner = PostedSlotOwner {
            posted: &self.posted,
            key,
            slot,
            armed: true,
        };
        let r = self.wait_posted(slot, source, tag);
        owner.armed = false;
        r
    }

    // ----------------------------------------------------- messages to self

    /// A self-channel failure: a passed deadline or a poisoned lock is a
    /// transport failure, like a timed-out MPI wait.
    fn own_error(&self, e: WaitError, what: &str) -> Error {
        match e {
            WaitError::Timeout => transport(format!(
                "rank {}: {what} timed out after {:?} (the communicator is broken)",
                self.rank,
                self.opts.timeout.unwrap_or_default()
            )),
            WaitError::Poisoned => transport(format!("{what}: poisoned self-message channel")),
            WaitError::Progress(e) => transport(format!("{what}: {e}")),
        }
    }

    /// One MPI progress step without side effects:
    /// `MPI_Iprobe` for a message from this rank itself — never present,
    /// since messages to self never enter MPI — so the probe
    /// never matches and Open MPI runs its progress engine (pending sends,
    /// rendezvous and pipeline steps), as a blocking MPI receive would.
    fn poke(&self) -> Result<()> {
        let mut flag: c_int = 0;
        let mut st = empty_status();
        let (f, s, comm, me) = (&raw mut flag, &raw mut st, self.raw(), self.rank);
        // SAFETY: valid communicator, own rank, `MPI_ANY_TAG` (an immutable
        // constant of the rsmpi shim); out-parameters valid; nothing is
        // received.
        self.call("MPI_Iprobe (progress)", || unsafe {
            ffi::MPI_Iprobe(me, ffi::RSMPI_ANY_TAG, comm, f, s)
        })
    }

    /// Progress work of a thread waiting in the self channel:
    /// match the process's posted receives and make one MPI
    /// progress call, as a blocking MPI receive from self would. Reports
    /// pending MPI work (in-flight operations, waiting posted receives), so
    /// that the wait keeps a short progress cadence while there is any.
    fn own_progress(&self) -> impl FnMut() -> std::result::Result<bool, String> + '_ {
        move || {
            pump_all();
            self.poke().map_err(|e| e.to_string())?;
            Ok(ACTIVE_OPS.load(Ordering::SeqCst) > 0 || WAITING_SLOTS.load(Ordering::SeqCst) > 0)
        }
    }

    /// Thread-level admission of a self-channel **wait**:
    /// the wait's progress hook calls MPI (`MPI_Iprobe`), so a
    /// thread the policy refuses (a worker under `Single`/`Funneled`) is
    /// refused at the start of the wait — deterministically, whether the
    /// message is already queued or arrives during the wait — exactly as a
    /// remote request's wait is (fatal transport error, family broken).
    fn own_wait_admit(&self, what: &str) -> Result<()> {
        self.policy()
            .check()
            .map_err(|e| transport(format!("{what}: {e}")))
    }

    /// Thread-level admission and tag check of a self-channel operation (no
    /// MPI call, no MPI lock).
    fn own_admit(&self, tag: i32, what: &str) -> Result<i32> {
        self.policy()
            .check()
            .map_err(|e| transport(format!("{what}: {e}")))?;
        check_tag(tag)
    }

    /// Send `bytes` to self ([`crate::self_channel`]).
    /// Returns the id of a synchronous send still waiting for a receive
    /// (`synchronous`, or the communicator's [`SendMode::Rendezvous`]).
    pub(crate) fn send_to_self(
        &self,
        bytes: Vec<u8>,
        tag: i32,
        synchronous: bool,
    ) -> Result<Option<u64>> {
        const WHAT: &str = "send to self";
        let tag = self.own_admit(tag, WHAT)?;
        count(bytes.len(), WHAT)?;
        let sync = synchronous || self.opts.send_mode == SendMode::Rendezvous;
        self.own
            .send(tag, bytes, sync)
            .map_err(|e| self.own_error(e, WHAT))
    }

    /// `true` once the synchronous self-send `id` was matched.
    pub(crate) fn test_send_to_self(&self, id: u64) -> Result<bool> {
        self.own
            .test_send(id)
            .map_err(|e| self.own_error(e, "send to self"))
    }

    /// Wait (bounded by the deadline) until the synchronous self-send `id`
    /// was matched by a receive.
    pub(crate) fn wait_send_to_self(&self, id: u64) -> Result<()> {
        self.own_wait_admit("synchronous send to self")?;
        let mut progress = self.own_progress();
        self.own
            .wait_send(id, self.deadline(), Some(&mut progress))
            .map_err(|e| self.own_error(e, "synchronous send to self"))
    }

    /// Post a receive from self with `tag`; returns the slot.
    pub(crate) fn post_from_self(&self, len: usize, tag: i32) -> Result<u64> {
        const WHAT: &str = "receive from self";
        let tag = self.own_admit(tag, WHAT)?;
        count(len, WHAT)?;
        self.own.post(tag).map_err(|e| self.own_error(e, WHAT))
    }

    /// Wait (bounded by the deadline) for the posted receive `slot`: the
    /// whole message (the caller reports a longer one as truncated).
    pub(crate) fn wait_from_self(&self, tag: i32, slot: u64) -> Result<Vec<u8>> {
        self.own_wait_admit("receive from self")?;
        let mut progress = self.own_progress();
        self.own
            .wait_posted(tag, slot, self.deadline(), Some(&mut progress))
            .map_err(|e| self.own_error(e, "receive from self"))
    }

    /// Cancel the posted receive `slot` (withdrawn, or its message
    /// discarded).
    pub(crate) fn cancel_from_self(&self, tag: i32, slot: u64) {
        self.own.cancel(tag, slot);
    }

    /// Matched receive of the next self-message with `tag` (bounded by the
    /// deadline).
    fn recv_from_self(&self, tag: i32) -> Result<Vec<u8>> {
        const WHAT: &str = "matched receive from self";
        let tag = self.own_admit(tag, WHAT)?;
        let mut progress = self.own_progress();
        self.own
            .take(tag, self.deadline(), Some(&mut progress))
            .map_err(|e| self.own_error(e, WHAT))
    }

    /// Test hook: `MPI_Iprobe` with the invalid source rank `size` — a
    /// genuine MPI error (`MPI_ERR_RANK`) returned through
    /// `MPI_ERRORS_RETURN` (no user buffer is involved).
    pub(crate) fn inject_invalid_rank_error(&self) -> Result<()> {
        let src = c_int::try_from(self.size).unwrap_or(c_int::MAX);
        let mut flag: c_int = 0;
        let mut st = empty_status();
        let (f, s, comm) = (&raw mut flag, &raw mut st, self.raw());
        // SAFETY: valid communicator and out-parameters; the invalid source
        // is rejected by MPI's argument checking (no buffer is accessed).
        self.call("MPI_Iprobe", || unsafe {
            ffi::MPI_Iprobe(src, 0, comm, f, s)
        })
    }

    /// Probe for a message (`block`: wait for one, bounded by the
    /// deadline). Returns `(source, tag, bytes)`. Messages to self are
    /// probed in the self channel: only there for `source` = this
    /// rank, there first for any source (MPI does not order messages of
    /// different sources).
    pub(crate) fn probe(
        &self,
        source: Option<Rank>,
        tag: i32,
        block: bool,
    ) -> Result<Option<(Rank, i32, usize)>> {
        let tag = check_tag(tag)?;
        let src = source.unwrap_or_else(any_source);
        let (own, mpi) = (
            source.is_none_or(|s| s == self.rank),
            source != Some(self.rank),
        );
        let deadline = self.deadline();
        let mut pacer = Pacer::new();
        loop {
            if own {
                let n = self
                    .own_admit(tag, "probe")
                    .and_then(|t| self.own.peek(t).map_err(|e| self.own_error(e, "probe")))?;
                if let Some(n) = n {
                    return Ok(Some((self.rank, tag, n)));
                }
            }
            let mut flag: c_int = 0;
            let mut st = empty_status();
            if mpi {
                // Messages that belong to a waiting posted slot are matched
                // first and never reported (MPI semantics): run the engine,
                // probe under its lock, and repeat while the probed message
                // belongs to a waiting slot and the engine makes progress.
                let mut eng = self.posted.lock()?;
                loop {
                    self.posted.pump(&mut eng)?;
                    flag = 0;
                    let (f, s, comm) = (&raw mut flag, &raw mut st, self.raw());
                    // SAFETY: valid communicator; out-params valid.
                    self.call("MPI_Iprobe", || unsafe {
                        ffi::MPI_Iprobe(src, tag, comm, f, s)
                    })?;
                    let before = eng.n_waiting;
                    if flag == 0 || !eng.waiting.contains_key(&(st.MPI_SOURCE, tag)) {
                        break;
                    }
                    self.posted.pump(&mut eng)?;
                    if eng.n_waiting == before {
                        flag = 0; // matched by a waiting slot meanwhile
                        break;
                    }
                }
            }
            if flag != 0 {
                let mut n: c_int = 0;
                let (sp, np) = (&raw const st, &raw mut n);
                // SAFETY: `st` is the status of the probed message.
                self.call("MPI_Get_count", || unsafe {
                    ffi::MPI_Get_count(sp, byte_type(), np)
                })?;
                let len =
                    usize::try_from(n).map_err(|_| transport("MPI_Get_count: undefined count"))?;
                return Ok(Some((st.MPI_SOURCE, st.MPI_TAG, len)));
            }
            if !block {
                return Ok(None);
            }
            if deadline.is_some_and(|d| Instant::now() >= d) {
                return Err(transport(format!(
                    "rank {}: probe timed out after {:?} (tag {tag})",
                    self.rank,
                    self.opts.timeout.unwrap_or_default()
                )));
            }
            pump_all();
            pacer.pause(deadline);
        }
    }

    // --------------------------------------------------------- collectives

    /// Checked-collective header: every rank learns
    /// every rank's `(sequence, info, ok, descriptor)`. Fails on **every**
    /// rank — before any payload — if the sequence numbers or descriptors
    /// differ or a rank's local validation failed (`local = Err`; that rank
    /// returns its own error, the others a common `Error::Communication`).
    /// Returns the local value and every rank's `info`.
    fn header<T>(&self, desc: &CollDesc, info: u64, local: Result<T>) -> Result<(T, Vec<u64>)> {
        self.header_inline(desc, info, local, &[])
            .map(|(v, infos, _)| (v, infos))
    }

    /// [`MpiBackend::header`] that also carries `inline` (at most
    /// [`INLINE_BYTES`]; longer is not inlined) in the header's payload
    /// slot. Returns every rank's inline block as well when **every** rank's
    /// `info` (the payload length of the payload collectives) is at most
    /// [`INLINE_BYTES`]: the caller then has every contribution and skips
    /// its data collective. The decision is taken from the gathered infos,
    /// identical on every rank.
    #[allow(clippy::type_complexity)] // the value, the infos, the inline blocks
    fn header_inline<T>(
        &self,
        desc: &CollDesc,
        info: u64,
        local: Result<T>,
        inline: &[u8],
    ) -> Result<(T, Vec<u64>, Option<Vec<Vec<u8>>>)> {
        let seq = self.seq.fetch_add(1, Ordering::SeqCst);
        let mut mine = Vec::with_capacity(HEADER_BYTES);
        for w in [seq, info, u64::from(local.is_ok())]
            .into_iter()
            .chain(desc.words())
            .chain(HeaderStatus::words(desc.status))
        {
            mine.extend_from_slice(&w.to_le_bytes());
        }
        if inline.len() <= INLINE_BYTES {
            mine.extend_from_slice(inline);
        }
        mine.resize(HEADER_BYTES, 0);
        let total = HEADER_BYTES
            .checked_mul(self.size)
            .ok_or_else(|| transport("header size overflow"))?;
        let all = if header_by_gather(self.size) {
            self.header_gather_broadcast(mine, total)?
        } else {
            let mut all = zeroed(total);
            let n = count(HEADER_BYTES, "header")?;
            let (s, d) = (mine.as_ptr(), all.as_mut_ptr());
            let mut op =
                self.in_flight(vec![mine, all], Vec::new(), "MPI_Iallgather (header)", None);
            let comm = self.raw();
            // SAFETY: `mine` holds HEADER_BYTES bytes, `all` HEADER_BYTES × size
            // where size is the size of this **intra**communicator (checked at
            // adoption): exactly what MPI writes. Both are owned by the
            // in-flight operation (heap unchanged) until completion.
            self.start(&mut op, |r| unsafe {
                ffi::MPI_Iallgather(s.cast(), n, byte_type(), d.cast(), n, byte_type(), comm, r)
            })?;
            let mut bufs = op.wait()?;
            bufs.pop().unwrap_or_default()
        };
        let word = |r: usize, k: usize| {
            let o = r * HEADER_BYTES + 8 * k;
            let mut w = [0u8; 8];
            w.copy_from_slice(&all[o..o + 8]);
            u64::from_le_bytes(w)
        };
        let me = usize::try_from(self.rank).unwrap_or(0);
        // D-126: a protocol status reported by any rank decides first
        // (whatever the sequence numbers and descriptors say): the
        // collective fails on every rank before any payload moves
        if let Some(v) = status_verdict((0..self.size).map(|r| {
            let mut st = [0u64; STATUS_WORDS];
            for (k, w) in st.iter_mut().enumerate() {
                *w = word(r, 3 + DESC_WORDS + k);
            }
            st
        })) {
            set_verdict(v);
            return Err(status_error(v, local.err()));
        }
        if let Some(r) = (0..self.size).find(|&r| word(r, 0) != word(me, 0)) {
            let e = Error::Communication(format!(
                "collective mismatch: rank {} at collective #{}, rank {r} at #{} (operation {:?})",
                self.rank,
                word(me, 0),
                word(r, 0),
                desc.op
            ));
            return Err(local.err().unwrap_or(e));
        }
        let entries: Vec<([u64; DESC_WORDS], bool)> = (0..self.size)
            .map(|r| {
                let mut d = [0u64; DESC_WORDS];
                for (k, w) in d.iter_mut().enumerate() {
                    *w = word(r, 3 + k);
                }
                (d, word(r, 2) != 0)
            })
            .collect();
        if let Err(msg) = check_entries(&entries) {
            return Err(local.err().unwrap_or(Error::Communication(msg)));
        }
        let infos: Vec<u64> = (0..self.size).map(|r| word(r, 1)).collect();
        let inlined = infos.iter().all(|&i| i <= INLINE_BYTES as u64).then(|| {
            infos
                .iter()
                .enumerate()
                .map(|(r, &i)| {
                    let o = r * HEADER_BYTES + HEADER_WORDS_BYTES;
                    // i ≤ INLINE_BYTES: inside the slot of rank r
                    all[o..o + usize::try_from(i).unwrap_or(0)].to_vec()
                })
                .collect()
        });
        Ok((local?, infos, inlined))
    }

    /// The header all-gather as a gather on rank 0 followed by a broadcast
    /// of the gathered headers from rank 0 (`MPI_Igather` + `MPI_Ibcast`,
    /// polled like every wait): every rank receives every rank's header, as
    /// from `MPI_Iallgather`, with 2 (size − 1) messages instead of the
    /// size (size − 1) of Open MPI's linear non-blocking all-gather (D-126).
    fn header_gather_broadcast(&self, mine: Vec<u8>, total: usize) -> Result<Vec<u8>> {
        let n = count(HEADER_BYTES, "header")?;
        let nt = count(total, "header")?;
        let comm = self.raw();
        let mut all = zeroed(total);
        let (s, d) = (mine.as_ptr(), all.as_mut_ptr());
        let mut op = self.in_flight(vec![mine, all], Vec::new(), "MPI_Igather (header)", None);
        // SAFETY: `mine` holds HEADER_BYTES bytes; `all` holds HEADER_BYTES ×
        // size bytes (significant on the root, rank 0, which every rank
        // names), the size of this **intra**communicator (checked at
        // adoption); both are owned by the in-flight operation until it
        // completes.
        self.start(&mut op, |r| unsafe {
            ffi::MPI_Igather(
                s.cast(),
                n,
                byte_type(),
                d.cast(),
                n,
                byte_type(),
                0,
                comm,
                r,
            )
        })?;
        let mut bufs = op.wait()?;
        let mut all = bufs.pop().unwrap_or_default();
        if all.len() != total {
            return Err(transport("header gather: buffer lost"));
        }
        let b = all.as_mut_ptr();
        let mut op = self.in_flight(vec![all], Vec::new(), "MPI_Ibcast (header)", None);
        // SAFETY: every rank passes the same count (`total` bytes, from the
        // communicator size), its buffer of exactly that length is owned by
        // the in-flight operation until it completes; root 0 exists.
        self.start(&mut op, |r| unsafe {
            ffi::MPI_Ibcast(b.cast(), nt, byte_type(), 0, comm, r)
        })?;
        let mut bufs = op.wait()?;
        bufs.pop()
            .ok_or_else(|| transport("header broadcast: buffer lost"))
    }

    /// Barrier (a header all-gather is synchronising).
    pub(crate) fn barrier(&self, desc: &CollDesc) -> Result<()> {
        self.header(desc, 0, Ok(())).map(|_| ())
    }

    fn counts_from(infos: &[u64], what: &str) -> Result<(Vec<c_int>, Vec<c_int>, usize)> {
        let mut counts = Vec::with_capacity(infos.len());
        let mut displs = Vec::with_capacity(infos.len());
        let mut total: usize = 0;
        for &i in infos {
            let n = usize::try_from(i).unwrap_or(usize::MAX);
            displs.push(count(total, what)?);
            counts.push(count(n, what)?);
            total = total
                .checked_add(n)
                .ok_or_else(|| Error::InvalidArgument(format!("{what}: size overflow")))?;
        }
        count(total, what)?;
        Ok((counts, displs, total))
    }

    /// All-gather of variable-size byte contributions. A local error
    /// (`mine = Err`) is reported through the header, so every rank fails
    /// without posting the data collective; the rank keeps its own error.
    pub(crate) fn all_gather_v(
        &self,
        desc: &CollDesc,
        mine: Result<Vec<u8>>,
    ) -> Result<Vec<Vec<u8>>> {
        let len = mine.as_ref().map_or(0, Vec::len) as u64;
        let (slot, n_inline) = inline_slot(&mine);
        let (mine, infos, inlined) = self.header_inline(desc, len, mine, &slot[..n_inline])?;
        if let Some(all) = inlined {
            return Ok(all);
        }
        // Every rank computes the same counts: a size overflow fails on all.
        let (counts, displs, total) = Self::counts_from(&infos, "all_gather_v")?;
        let layout = (counts.clone(), displs.clone());
        let (counts, displs) = (allocated(counts), allocated(displs));
        let mine = allocated(mine);
        let mut all = zeroed(total);
        let n = count(mine.len(), "all_gather_v")?;
        let (s, d, c, o) = (
            mine.as_ptr(),
            all.as_mut_ptr(),
            counts.as_ptr(),
            displs.as_ptr(),
        );
        let mut op = self.in_flight(
            vec![mine, all],
            vec![counts, displs],
            "MPI_Iallgatherv",
            None,
        );
        let comm = self.raw();
        // SAFETY: send buffer `n` bytes, receive buffer `total` bytes with
        // `counts`/`displs` describing disjoint in-bounds blocks for every
        // rank of this intracommunicator; all four allocations are owned by
        // the in-flight operation and kept until completion (MPI-3 requires
        // the arrays to stay valid).
        self.start(&mut op, |r| unsafe {
            ffi::MPI_Iallgatherv(
                s.cast(),
                n,
                byte_type(),
                d.cast(),
                c,
                o,
                byte_type(),
                comm,
                r,
            )
        })?;
        let mut bufs = op.wait()?;
        let all = bufs.pop().unwrap_or_default();
        split_blocks(&all, &layout.0, &layout.1)
    }

    /// Gather of variable-size byte contributions on `root` (agreed and
    /// validated by the header).
    pub(crate) fn gather_v(
        &self,
        desc: &CollDesc,
        mine: Result<Vec<u8>>,
        root: Rank,
    ) -> Result<Option<Vec<Vec<u8>>>> {
        let len = mine.as_ref().map_or(0, Vec::len) as u64;
        let (slot, n_inline) = inline_slot(&mine);
        let (mine, infos, inlined) = self.header_inline(desc, len, mine, &slot[..n_inline])?;
        let is_root = self.rank == root;
        if let Some(all) = inlined {
            return Ok(is_root.then_some(all));
        }
        let (counts, displs, total) = Self::counts_from(&infos, "gather_v")?;
        let layout = (counts.clone(), displs.clone());
        let (counts, displs) = (allocated(counts), allocated(displs));
        let mine = allocated(mine);
        let mut all = zeroed(if is_root { total } else { 0 });
        let n = count(mine.len(), "gather_v")?;
        let (s, d, c, o) = (
            mine.as_ptr(),
            all.as_mut_ptr(),
            counts.as_ptr(),
            displs.as_ptr(),
        );
        let mut op = self.in_flight(vec![mine, all], vec![counts, displs], "MPI_Igatherv", None);
        let comm = self.raw();
        // SAFETY: as for MPI_Iallgatherv; the receive arguments are only
        // significant (and sized `total`) on the root, which every rank
        // agreed on and validated in the header.
        self.start(&mut op, |r| unsafe {
            ffi::MPI_Igatherv(
                s.cast(),
                n,
                byte_type(),
                d.cast(),
                c,
                o,
                byte_type(),
                root,
                comm,
                r,
            )
        })?;
        let mut bufs = op.wait()?;
        let all = bufs.pop().unwrap_or_default();
        if is_root {
            split_blocks(&all, &layout.0, &layout.1).map(Some)
        } else {
            Ok(None)
        }
    }

    /// Scatter of variable-size byte blocks from `root` (`per_rank` is
    /// significant on the root only). Every root-side layout computation
    /// (counts, displacements, total) happens **before** the header, so an
    /// invalid root input fails on every rank before any payload.
    pub(crate) fn scatter_v(
        &self,
        desc: &CollDesc,
        per_rank: Result<Option<Vec<Vec<u8>>>>,
        root: Rank,
    ) -> Result<Vec<u8>> {
        type RootLayout = Option<(Vec<u8>, Vec<c_int>, Vec<c_int>, Vec<u8>)>;
        let is_root = self.rank == root;
        // The root's layout is validated here, not only by
        // the caller — `is_root ⇒ Some(blocks)` with exactly `size` blocks;
        // the blocks of a non-root rank are insignificant and ignored.
        let prepared: Result<RootLayout> = per_rank.and_then(|blocks| {
            if !is_root {
                return Ok(None);
            }
            let Some(blocks) = blocks else {
                return Err(Error::InvalidArgument(
                    "scatter_v: the root must provide the blocks".into(),
                ));
            };
            if blocks.len() != self.size {
                return Err(Error::InvalidArgument(format!(
                    "scatter_v: root provided {} blocks for {} ranks",
                    blocks.len(),
                    self.size
                )));
            }
            let mut counts_bytes = Vec::with_capacity(8 * blocks.len());
            let (mut counts, mut displs) = (Vec::new(), Vec::new());
            let mut total = 0usize;
            for block in &blocks {
                counts_bytes.extend_from_slice(&(block.len() as u64).to_le_bytes());
                displs.push(count(total, "scatter_v")?);
                counts.push(count(block.len(), "scatter_v")?);
                total = total
                    .checked_add(block.len())
                    .ok_or_else(|| Error::InvalidArgument("scatter_v: size overflow".into()))?;
            }
            count(total, "scatter_v")?;
            let mut concat = Vec::with_capacity(total);
            for block in &blocks {
                concat.extend_from_slice(block);
            }
            Ok(Some((counts_bytes, counts, displs, concat)))
        });
        let (prepared, _) = self.header(desc, 0, prepared)?;
        if is_root && prepared.is_none() {
            // Unreachable after the validation above; never hand MPI the
            // empty arrays as the root's send side.
            return Err(transport("scatter_v: root layout missing"));
        }
        let (counts_bytes, counts, displs, concat) = prepared.unwrap_or_default();
        // Counts first (one u64 per rank), then the data.
        let counts_bytes = allocated(counts_bytes);
        let mut my_count = zeroed(8);
        let (s, d) = (counts_bytes.as_ptr(), my_count.as_mut_ptr());
        let mut op = self.in_flight(
            vec![counts_bytes, my_count],
            Vec::new(),
            "MPI_Iscatter",
            None,
        );
        let comm = self.raw();
        // SAFETY: the root's send buffer holds 8 × size bytes (one u64 per
        // rank: the root's layout exists and has `size` blocks, checked in
        // this function before the header and again after it), every
        // receive buffer 8 bytes; both owned by the in-flight operation
        // until completion. The send side is insignificant on non-roots.
        self.start(&mut op, |r| unsafe {
            ffi::MPI_Iscatter(
                s.cast(),
                8,
                byte_type(),
                d.cast(),
                8,
                byte_type(),
                root,
                comm,
                r,
            )
        })?;
        let mut bufs = op.wait()?;
        let mine = bufs.pop().unwrap_or_default();
        let mut w = [0u8; 8];
        w.copy_from_slice(mine.get(..8).ok_or_else(|| transport("scatter count"))?);
        let len = usize::try_from(u64::from_le_bytes(w)).unwrap_or(usize::MAX);
        // The root checked every count against the int limit before the
        // header: a larger count here is a corrupted transfer.
        let n = count(len, "scatter_v").map_err(|e| transport(e.to_string()))?;
        let (concat, counts, displs) = (allocated(concat), allocated(counts), allocated(displs));
        let mut out = zeroed(len);
        let (s, c, o, d) = (
            concat.as_ptr(),
            counts.as_ptr(),
            displs.as_ptr(),
            out.as_mut_ptr(),
        );
        let mut op = self.in_flight(
            vec![concat, out],
            vec![counts, displs],
            "MPI_Iscatterv",
            None,
        );
        let comm = self.raw();
        // SAFETY: the root's send buffer and count/displacement arrays
        // describe `size` in-bounds blocks of `concat` (computed in this
        // function before the header; the root's layout exists, checked
        // above); every receive buffer holds exactly the announced count;
        // all owned until completion.
        self.start(&mut op, |r| unsafe {
            ffi::MPI_Iscatterv(
                s.cast(),
                c,
                o,
                byte_type(),
                d.cast(),
                n,
                byte_type(),
                root,
                comm,
                r,
            )
        })?;
        let mut bufs = op.wait()?;
        Ok(bufs.pop().unwrap_or_default())
    }

    /// Broadcast of `bytes` from `root`; every rank passes a buffer of the
    /// same length (agreed element count and type in the header).
    pub(crate) fn broadcast(
        &self,
        desc: &CollDesc,
        bytes: Result<Vec<u8>>,
        root: Rank,
    ) -> Result<Vec<u8>> {
        let len = bytes.as_ref().map_or(0, Vec::len) as u64;
        let (slot, n_inline) = inline_slot(&bytes);
        let (bytes, infos, inlined) = self.header_inline(desc, len, bytes, &slot[..n_inline])?;
        if let Some(r) = infos.iter().position(|&i| i != infos[0]) {
            return Err(Error::Communication(format!(
                "broadcast: ranks disagree on the length ({} bytes on rank 0, {} on rank {r})",
                infos[0], infos[r]
            )));
        }
        if let Some(mut all) = inlined {
            let r = usize::try_from(root).ok().filter(|&r| r < all.len());
            return r
                .map(|r| all.swap_remove(r))
                .ok_or_else(|| transport("broadcast: root outside the communicator"));
        }
        let n = count(bytes.len(), "broadcast")?;
        let mut bytes = allocated(bytes);
        let b = bytes.as_mut_ptr();
        let mut op = self.in_flight(vec![bytes], Vec::new(), "MPI_Ibcast", None);
        let comm = self.raw();
        // SAFETY: `bytes` holds `n` bytes on every rank (agreed above) and
        // is owned by the in-flight operation until completion; `root` was
        // validated and agreed in the header.
        self.start(&mut op, |r| unsafe {
            ffi::MPI_Ibcast(b.cast(), n, byte_type(), root, comm, r)
        })?;
        let mut bufs = op.wait()?;
        Ok(bufs.pop().unwrap_or_default())
    }

    /// Native typed reduction or scan (D-039, [`ReductionPolicy::NativeMpi`]):
    /// the **same blocking calls as upstream** (`communicator_mpi_inline_impl.hh`)
    /// — `MPI_Allreduce`, `MPI_Reduce`, `MPI_Scan`, `MPI_Exscan` with
    /// `MPI_IN_PLACE` (`MPI_Reduce`: in place on the root only, where the
    /// standard allows it; upstream passes it on every rank) — because
    /// OpenMPI implements its non-blocking collectives with other algorithms
    /// (libnbc), i.e. possibly another association order. Datatype from the
    /// agreed element type, operator from the agreed reduction operator,
    /// count from the agreed element count; the datatype/operator lookup is
    /// part of the local validation, so a lookup failure is agreed too. The
    /// data collective runs only after a successful checked header (every
    /// rank entered the same collective with the same contract); being
    /// blocking, it has **no progress deadline** after the header
    /// and is an **exclusive operation** under `Serialized`
    /// (refused on every rank while another one is in flight in the
    /// process, `ExclusiveGuard`). Returns this rank's result in
    /// akantu packing (`MPI_Reduce`: empty on non-root ranks; `MPI_Exscan`:
    /// zero on rank 0, where MPI leaves it undefined).
    pub(crate) fn native_reduce(&self, desc: &CollDesc, mine: Result<Vec<u8>>) -> Result<Vec<u8>> {
        let policy = self.policy().clone();
        let (_exclusive, admitted) = self.admit(Exclusive::Native, "native reduction");
        let prepared = admitted.and(mine).and_then(|bytes| {
            let (dt, stride) = native_type(&policy, desc.ty)?;
            let op = native_op(desc.reduce.map_or(0, crate::communicator::ReduceOp::code))?;
            let items = usize::try_from(desc.count.unwrap_or(0)).unwrap_or(usize::MAX);
            let n = c_int::try_from(items).map_err(|_| {
                Error::InvalidArgument("native reduction: count exceeds the MPI int limit".into())
            })?;
            items
                .checked_mul(stride)
                .ok_or_else(|| Error::InvalidArgument("native reduction: size overflow".into()))?;
            // The buffer holds exactly the agreed count
            // of packed elements (12 bytes for a value/location pair, the
            // native stride otherwise), so `to_native` yields `n × stride`
            // bytes for the blocking call below. A local validation, agreed
            // through the header like every other one.
            let packed = if desc.ty == 9 { 12 } else { stride };
            if items.checked_mul(packed) != Some(bytes.len()) {
                return Err(Error::InvalidArgument(format!(
                    "native reduction: {} bytes for {items} elements of {packed} bytes",
                    bytes.len()
                )));
            }
            Ok((to_native(desc.ty, stride, &bytes), dt, op, n, stride))
        });
        let len = prepared.as_ref().map_or(0, |p| p.0.len()) as u64;
        let ((data, dt, mpi_op, n, stride), _) = self.header(desc, len, prepared)?;
        let mut data = allocated(data);
        let mut dummy = zeroed(1);
        let (b, x, comm) = (data.as_mut_ptr(), dummy.as_mut_ptr(), self.raw());
        let root = desc.root.unwrap_or(0);
        let kind = desc.op;
        let is_root = self.rank == root;
        // SAFETY: shim accessor returning the predefined `MPI_IN_PLACE`.
        let in_place = unsafe { akantu_mpi_in_place() };
        let what = match kind {
            CollOp::Reduce => "MPI_Reduce",
            CollOp::Scan => "MPI_Scan",
            CollOp::ExclusiveScan => "MPI_Exscan",
            _ => "MPI_Allreduce",
        };
        // SAFETY: `data` holds `n` elements of `dt` (stride bytes each,
        // checked against MPI's extent for the pair type; the input length
        // checked against the agreed count above) and is exclusively
        // borrowed for this blocking call; `MPI_IN_PLACE` is passed only
        // where the standard allows it (all ranks of allreduce/scan/exscan,
        // the root of reduce; non-roots pass `data` as send buffer and an
        // allocated, insignificant receive buffer); datatype, operator,
        // count and root were agreed by every rank in the header and the
        // operator is defined for the datatype (validated on every rank).
        // Under `Serialized` this rank holds the exclusive-operation
        // registration (`_exclusive`) until the call returned.
        self.call(what, || unsafe {
            match kind {
                CollOp::Reduce if is_root => {
                    ffi::MPI_Reduce(in_place, b.cast(), n, dt, mpi_op, root, comm)
                }
                CollOp::Reduce => ffi::MPI_Reduce(b.cast(), x.cast(), n, dt, mpi_op, root, comm),
                CollOp::Scan => ffi::MPI_Scan(in_place, b.cast(), n, dt, mpi_op, comm),
                CollOp::ExclusiveScan => ffi::MPI_Exscan(in_place, b.cast(), n, dt, mpi_op, comm),
                _ => ffi::MPI_Allreduce(in_place, b.cast(), n, dt, mpi_op, comm),
            }
        })?;
        if kind == CollOp::Reduce && !is_root {
            return Ok(Vec::new());
        }
        if kind == CollOp::ExclusiveScan && self.rank == 0 {
            // Every `CommData` zero is the all-zero bit pattern.
            data.fill(0);
        }
        Ok(from_native(desc.ty, stride, &data))
    }

    /// Non-blocking barrier (`MPI_Ibarrier`, MPI-3; used by
    /// `receive_any_number`).
    pub(crate) fn ibarrier(&self) -> Result<MpiInFlight> {
        let mut op = self.in_flight(Vec::new(), Vec::new(), "MPI_Ibarrier", None);
        let comm = self.raw();
        // SAFETY: valid communicator; `r` points to `op`'s request.
        self.start(&mut op, |r| unsafe { ffi::MPI_Ibarrier(comm, r) })?;
        Ok(op)
    }

    /// `MPI_Comm_split` (`color = None`: `MPI_UNDEFINED`, no new
    /// communicator), preceded by two checked headers: validation (an
    /// invalid color is a local failure, agreed on every rank), then the
    /// `Serialized` **commit** (`ExclusiveGuard`: a
    /// refusal is agreed on every rank; the header runs on every thread
    /// level so that all ranks issue the same collectives). MPI-3 has no
    /// non-blocking split: the call blocks with **no deadline** after the
    /// headers.
    pub(crate) fn split(
        &self,
        desc: &CollDesc,
        color: Result<Option<u32>>,
        key: i32,
    ) -> Result<Option<Self>> {
        let c = color.and_then(|color| match color {
            None => Ok(undefined()),
            Some(c) => c_int::try_from(c)
                .map_err(|_| Error::InvalidArgument(format!("split: color {c} exceeds int"))),
        });
        let (c, _) = self.header(desc, 0, c)?;
        let (commit, admitted) = self.admit(Exclusive::Split, "split");
        self.header(desc, 0, admitted)?;
        let mut out = comm_null();
        let (o, comm) = (&raw mut out, self.raw());
        // SAFETY: valid communicator; `out` receives the new handle
        // (MPI_COMM_NULL for MPI_UNDEFINED). Blocking: every rank entered
        // it after the successful headers (committed on every rank under
        // `Serialized`).
        let called = self.call("MPI_Comm_split", || unsafe {
            ffi::MPI_Comm_split(comm, c, key, o)
        });
        match (called, commit) {
            (Ok(()), commit) => drop(commit),
            (Err(e), commit) => {
                // State unknown: keep the registration (later splits and
                // duplications are refused; the runtime aborts).
                if let Some(g) = commit {
                    g.leak();
                }
                return Err(e);
            }
        }
        if out == comm_null() {
            return Ok(None);
        }
        COMMS_CREATED.fetch_add(1, Ordering::SeqCst);
        Self::adopt(self.env.clone(), RawComm(out), self.opts).map(Some)
    }

    /// A blocking native-library collective (`Communicator::native_collective`):
    /// local validation (caller's verdict, thread policy), the
    /// `Serialized` admission — split class, since native libraries create
    /// communicators inside their calls (`ExclusiveGuard`); under
    /// `Multiple` the same registration keeps native jobs of one process
    /// from overlapping ([`MpiBackend::admit_native`]) —
    /// and the
    /// **job identity** (`what`, ranks entering different
    /// native jobs fail on every rank before the library runs) agreed by
    /// one checked header, then the handle conversion and `f` in **one**
    /// thread-policy call (holding the `Serialized` lock throughout: the
    /// conversion `MPI_Comm_c2f` is itself an MPI call)
    /// with the registration kept until it returned.
    pub(crate) fn native_collective<R>(
        &self,
        desc: &CollDesc,
        what: &str,
        local: Result<()>,
        owner: usize,
        f: impl for<'n> FnOnce(NativeComm<'n>) -> R,
    ) -> Result<R> {
        let local = local.and_then(|()| self.policy().check());
        let (registration, admitted) = self.admit_native(what);
        let job = crate::phase::phase_id(what);
        let ((), jobs) = self.header(desc, job, local.and(admitted))?;
        // Every rank received every rank's job identity: the verdict is
        // common knowledge, nobody enters the library alone.
        if let Some(r) = jobs.iter().position(|&j| j != job) {
            drop(registration);
            return Err(Error::Communication(format!(
                "native collective mismatch: rank {} enters '{what}' (job id {job:#018x}), rank \
                 {r} a different native job (job id {:#018x}); no rank entered the library",
                self.rank, jobs[r]
            )));
        }
        let out = self.enter_native(what, owner, f);
        drop(registration);
        out
    }

    /// Duplicate (`MPI_Comm_idup`, polled under the deadline), preceded by
    /// a checked header.
    /// Under `Serialized` it is refused (agreed by the header) while a
    /// committed split is in flight in the process, and stays registered
    /// until its `MPI_Comm_idup` completed
    /// (`ExclusiveGuard`).
    pub(crate) fn duplicate(&self, desc: &CollDesc) -> Result<Self> {
        let (hold, admitted) = self.admit(Exclusive::Idup, "duplicate");
        self.header(desc, 0, admitted)?;
        Self::duplicate_raw(self.env.clone(), self.comm.raw, self.opts, hold)
    }

    /// Run `f` with the native handle under the thread policy (holding the
    /// `Serialized` lock, re-entrant on this thread), the Fortran handle
    /// converted under the same lock.
    fn enter_native<R>(
        &self,
        what: &str,
        owner: usize,
        f: impl for<'n> FnOnce(NativeComm<'n>) -> R,
    ) -> Result<R> {
        let (raw, rank, size) = (self.raw(), self.rank, self.size);
        self.policy()
            .call(|| {
                // SAFETY: shim wrapper of `MPI_Comm_c2f` on this live
                // communicator (owned by `self`, freed only after its last
                // handle); a local conversion, run under the thread policy
                // like every other MPI call.
                let fortran = unsafe { akantu_mpi_comm_c2f(raw) };
                let _depth = NativeDepth::enter();
                f(NativeComm {
                    raw,
                    fortran,
                    rank,
                    size,
                    owner,
                    _call: std::marker::PhantomData,
                })
            })
            .map_err(|e| transport(format!("{what}: {e}")))
    }

    /// Local admission of a native region (`Communicator::native_ticket`).
    pub(crate) fn native_ticket(&self, issued_for: usize, what: &str) -> NativeTicket {
        let policy = self.policy().check();
        let (registration, admitted) = self.admit_native(what);
        let verdict = policy.and(admitted);
        let depth = verdict.is_ok().then(NativeDepth::enter);
        NativeTicket {
            issued_for,
            registration,
            verdict,
            _depth: depth,
            _thread: std::marker::PhantomData,
        }
    }

    /// A native region (`Communicator::native_region`): the agreed ticket's
    /// registration is held while `f` runs under the thread policy.
    pub(crate) fn native_region<R>(
        &self,
        ticket: NativeTicket,
        what: &str,
        f: impl for<'n> FnOnce(NativeComm<'n>) -> R,
    ) -> Result<R> {
        ticket.verdict()?;
        let out = self.enter_native(what, ticket.issued_for, f);
        drop(ticket);
        out
    }

    /// The emergency abort (see `Communicator::abort` for
    /// the policy): `MPI_Abort` only where the thread level permits it on
    /// this thread, otherwise immediate process termination without MPI.
    /// Never returns.
    pub(crate) fn abort(&self, code: i32) -> ! {
        // No stdout flush: it would wait for the
        // `Stdout` lock, which another thread may hold for ever (e.g. a
        // `println!` blocked on a full pipe). Buffered output may be lost.
        let policy = self.policy();
        let raw = RawComm(self.raw());
        match policy.provided() {
            ThreadLevel::Multiple => mpi_abort(raw, code),
            // The thread inside a native region holds the
            // (re-entrant) lock itself and may call MPI_Abort at once.
            ThreadLevel::Serialized if policy.held_by_current_thread() => mpi_abort(raw, code),
            ThreadLevel::Serialized => match policy.try_serial_lock(ABORT_LOCK_WAIT) {
                Some(_serialized) => mpi_abort(raw, code),
                None => terminate(
                    code,
                    "MPI_THREAD_SERIALIZED and another thread holds the MPI lock",
                ),
            },
            level @ (ThreadLevel::Single | ThreadLevel::Funneled) => {
                if policy.on_main_thread() {
                    mpi_abort(raw, code)
                } else {
                    // static texts: no allocation on the abort path
                    terminate(
                        code,
                        if level == ThreadLevel::Single {
                            "MPI_THREAD_SINGLE and this is not MPI's main thread"
                        } else {
                            "MPI_THREAD_FUNNELED and this is not MPI's main thread"
                        },
                    )
                }
            }
        }
    }
}

/// `MPI_Abort` on `comm`; the caller established that the thread level
/// permits the call on this thread (and holds the `Serialized` lock when
/// needed).
fn mpi_abort(comm: RawComm, code: i32) -> ! {
    // SAFETY: `comm` is a live akantu communicator (its backend is alive),
    // and the caller checked the thread-level precondition of MPI_Abort.
    unsafe {
        ffi::MPI_Abort(comm.0, code);
    }
    terminate(code, "MPI_Abort returned")
}

/// Non-MPI emergency termination: the launcher is asked to abort the job
/// through `PMIx_Abort` when the process has a PMIx client (bounded wait),
/// then `_exit(code)` (no destructors, no `atexit` handlers, no MPI call).
/// Nothing on this path allocates, looks up a symbol or creates a thread:
/// the notes are written from static text and a stack
/// buffer.
fn terminate(code: i32, why: &'static str) -> ! {
    emergency_note("akantu: emergency termination without MPI_Abort (");
    emergency_note(why);
    emergency_note_code("): process exits with code ", code, "\n");
    pmix_abort_bounded(code);
    // SAFETY: POSIX `_exit` terminates the process immediately; it has no
    // preconditions (skipping destructors and MPI finalisation is the
    // intent of this emergency path).
    unsafe { _exit(code) }
}

/// `dlsym`'s `RTLD_DEFAULT` pseudo-handle (search the global scope): `-2`
/// on macOS, `NULL` on glibc and musl. Other systems use other values
/// (FreeBSD and Solaris: `-2`), so the lookup is compiled only on Linux and
/// macOS; elsewhere the fallback only `_exit`s.
#[cfg(target_os = "macos")]
const RTLD_DEFAULT: *mut std::ffi::c_void = -2isize as *mut std::ffi::c_void;
#[cfg(target_os = "linux")]
const RTLD_DEFAULT: *mut std::ffi::c_void = std::ptr::null_mut();

/// `pmix_status_t PMIx_Abort(int status, const char msg[], pmix_proc_t
/// procs[], size_t nprocs)` (PMIx standard v4/v5, `pmix.h`;
/// `pmix_status_t` is a 32-bit `int`).
type PmixAbort = unsafe extern "C" fn(c_int, *const c_char, *mut std::ffi::c_void, usize) -> c_int;

const PMIX_ABORT_MSG: &std::ffi::CStr =
    c"akantu: transport failure on a thread that may not call MPI_Abort";

/// `PMIx_Abort` of the process's PMIx client library, if one is loaded
/// (Open MPI 5 links `libpmix`; MPI libraries without PMIx: `None`).
/// Called once, when MPI starts ([`resolve_pmix_abort`]), never on the
/// abort path.
#[cfg(any(target_os = "linux", target_os = "macos"))]
fn pmix_abort_symbol() -> Option<PmixAbort> {
    // SAFETY: `dlsym` with the `RTLD_DEFAULT` pseudo-handle of this target
    // and a NUL-terminated symbol name only reads them; it returns NULL
    // when no loaded object defines the symbol.
    let p = unsafe { dlsym(RTLD_DEFAULT, c"PMIx_Abort".as_ptr()) };
    if p.is_null() {
        return None;
    }
    // SAFETY: a non-NULL address of the global symbol `PMIx_Abort` is the
    // PMIx library's function, whose C signature is `PmixAbort` (PMIx
    // standard); POSIX guarantees that a `dlsym` result of a function can
    // be converted to a function pointer of the same size.
    Some(unsafe { std::mem::transmute::<*mut std::ffi::c_void, PmixAbort>(p) })
}

/// No lookup on other systems.
#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn pmix_abort_symbol() -> Option<PmixAbort> {
    None
}

/// `PmixState::request` value meaning "no abort requested yet".
const NO_REQUEST: i64 = i64::MIN;

/// State shared by the abort path and the helper thread (allocated when the
/// aborter starts, never on the abort path).
struct PmixState {
    /// [`NO_REQUEST`], or the exit code of the first abort request.
    request: AtomicI64,
    /// `PMIx_Abort` returned.
    done: AtomicBool,
}

/// The `PMIx_Abort` request of the non-MPI fallback, prepared when MPI
/// starts: the function pointer is resolved and a
/// helper thread, parked until a request arrives, is created then. The
/// abort path only stores the code, unparks the helper and waits, bounded
/// — no `dlsym` (loader lock), no thread creation (stack-cache/TLS locks),
/// no allocation (from `Communicator::abort_on_transport_fatal` on).
/// The helper never calls MPI.
struct PmixAborter {
    helper: std::thread::Thread,
    state: Arc<PmixState>,
}

impl PmixAborter {
    /// Start the parked helper thread for `call` (`None` if the thread
    /// cannot be created: the fallback then only `_exit`s).
    fn start(call: PmixAbort) -> Option<Self> {
        let state = Arc::new(PmixState {
            request: AtomicI64::new(NO_REQUEST),
            done: AtomicBool::new(false),
        });
        let shared = Arc::clone(&state);
        let helper = std::thread::Builder::new()
            .name("akantu-pmix-abort".into())
            .stack_size(256 * 1024)
            .spawn(move || {
                loop {
                    let code = shared.request.load(Ordering::SeqCst);
                    if code != NO_REQUEST {
                        // SAFETY: `call` is `PMIx_Abort` (`pmix_abort_symbol`;
                        // a test double with the same signature in the unit
                        // tests); the message is a NUL-terminated static
                        // string, `procs = NULL` with `nprocs = 0` means
                        // "every process of the caller's namespace" (PMIx
                        // standard). PMIx APIs are thread-safe. `code` came
                        // from an `i32`.
                        let _ = unsafe {
                            call(
                                c_int::try_from(code).unwrap_or(1),
                                PMIX_ABORT_MSG.as_ptr(),
                                std::ptr::null_mut(),
                                0,
                            )
                        };
                        shared.done.store(true, Ordering::SeqCst);
                        return;
                    }
                    // spurious wake-ups loop back to the check
                    std::thread::park();
                }
            })
            .ok()?;
        Some(Self {
            helper: helper.thread().clone(),
            state,
        })
    }

    /// Ask the helper to call `PMIx_Abort(code, …)` (only the first request
    /// of the process is sent) and wait at most `wait` for it to return.
    /// No allocation, lookup or thread creation. Returns whether it
    /// returned within the wait.
    fn request(&self, code: i32, wait: Duration) -> bool {
        let _ = self.state.request.compare_exchange(
            NO_REQUEST,
            i64::from(code),
            Ordering::SeqCst,
            Ordering::SeqCst,
        );
        self.helper.unpark();
        let deadline = Instant::now() + wait;
        while !self.state.done.load(Ordering::SeqCst) && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(1));
        }
        self.state.done.load(Ordering::SeqCst)
    }
}

/// `None` until MPI started; then `Some(None)` without a PMIx client (or if
/// the helper could not be created) and `Some(Some(_))` otherwise.
static PMIX_ABORTER: OnceLock<Option<PmixAborter>> = OnceLock::new();

/// Resolve `PMIx_Abort` and start its helper, once per process, when MPI
/// starts (every [`MpiBackend`] descends from `MpiBackend::world` or
/// `MpiBackend::import`, which call this right after the lifecycle's
/// initialisation, so the abort path — a method of a backend — always runs
/// after it). Remaining dependency: the helper thread exists from then on
/// (parked); if it cannot be created, the fallback is the plain `_exit`.
fn resolve_pmix_abort() {
    PMIX_ABORTER.get_or_init(|| pmix_abort_symbol().and_then(PmixAborter::start));
}

/// Ask the launcher to abort the job with `code`
/// through `PMIx_Abort(code, msg, NULL, 0)` — what Open MPI 5's `MPI_Abort`
/// does after its MPI-level checks (`ompi_rte_abort`: `PMIx_Abort`, a
/// 100 µs pause for the PMIx thread, `_exit`) — instead of relying on the
/// launcher noticing a non-zero exit without finalisation: with every rank
/// `_exit`ing at once that teardown path occasionally hangs in Open MPI
/// 5.0.7's `prterun` (PRRTE 3.0.8; unchanged through 3.0.12, see
/// `docs/DEPENDENCIES.md`). `PMIx_Abort` is a PMIx call, not an MPI call,
/// so the MPI thread level does not restrict it. The prepared helper
/// ([`PmixAborter`], resolved when MPI started) calls it, and it is waited
/// for at most [`PMIX_ABORT_WAIT`] (it blocks until the server
/// acknowledges, and the launcher usually terminates the process
/// meanwhile): the emergency path never blocks for ever. Returns whether
/// `PMIx_Abort` was called and returned within the wait.
fn pmix_abort_bounded(code: i32) -> bool {
    let Some(Some(aborter)) = PMIX_ABORTER.get() else {
        return false;
    };
    emergency_note_code(
        "akantu: asking the launcher to abort the job through PMIx_Abort (code ",
        code,
        ")\n",
    );
    let done = aborter.request(code, PMIX_ABORT_WAIT);
    // as `ompi_rte_abort`: a little delay for the PMIx thread to get the
    // request out
    std::thread::sleep(Duration::from_micros(100));
    done
}

/// [`emergency_note`] of `prefix`, `code` in decimal and `suffix`, without
/// allocating.
fn emergency_note_code(prefix: &str, code: i32, suffix: &str) {
    let mut buf = [0u8; 11];
    emergency_note(prefix);
    emergency_note_bytes(crate::communicator::decimal(code, &mut buf));
    emergency_note(suffix);
}

/// Write `msg` to the standard error descriptor with POSIX `write`, ignoring
/// every error: the emergency paths must neither panic
/// (`eprintln!` panics when stderr fails, e.g. `EPIPE`) nor wait for the
/// `std::io::Stderr` lock.
pub(crate) fn emergency_note(msg: &str) {
    emergency_note_bytes(msg.as_bytes());
}

/// [`emergency_note`] of raw bytes.
fn emergency_note_bytes(bytes: &[u8]) {
    let mut done = 0usize;
    while done < bytes.len() {
        let rest = &bytes[done..];
        // SAFETY: `rest` is a valid readable slice for the call; `write`
        // reads at most `rest.len()` bytes from it; file descriptor 2 may
        // be closed or broken, which `write` reports as an error.
        let n = unsafe { write(2, rest.as_ptr().cast(), rest.len()) };
        match usize::try_from(n) {
            Ok(n) if n > 0 => done += n,
            _ => return,
        }
    }
}

/// Split `all` into the blocks described by `counts`/`displs` (checked).
fn split_blocks(all: &[u8], counts: &[c_int], displs: &[c_int]) -> Result<Vec<Vec<u8>>> {
    counts
        .iter()
        .zip(displs)
        .map(|(&c, &d)| {
            let (c, d) = (
                usize::try_from(c).unwrap_or(0),
                usize::try_from(d).unwrap_or(0),
            );
            d.checked_add(c)
                .and_then(|e| all.get(d..e))
                .map(<[u8]>::to_vec)
                .ok_or_else(|| transport("gathered block outside the receive buffer"))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Under Open MPI 5 (which links `libpmix`) the non-MPI abort
    /// fallback finds `PMIx_Abort`, so it asks the launcher to abort the
    /// job instead of relying on the non-zero exit (whose teardown
    /// occasionally hangs in `prterun`). Other MPI libraries may lack it
    /// (the fallback then only `_exit`s).
    #[test]
    fn pmix_abort_is_found_under_open_mpi_5() {
        const LEN: usize = ffi::MPI_MAX_LIBRARY_VERSION_STRING as usize;
        let mut buf: [c_char; LEN + 1] = [0; LEN + 1];
        let mut len: c_int = 0;
        // SAFETY: `MPI_Get_library_version` may be called before `MPI_Init`
        // and from any thread (MPI-4.1 §9.1.1); `buf` holds
        // `MPI_MAX_LIBRARY_VERSION_STRING` characters plus a terminator.
        let rc = unsafe { ffi::MPI_Get_library_version(buf.as_mut_ptr(), &raw mut len) };
        assert_eq!(rc, success());
        let n = usize::try_from(len).unwrap_or(0).min(LEN);
        let bytes: Vec<u8> = buf[..n].iter().map(|&c| c.to_ne_bytes()[0]).collect();
        let version = String::from_utf8_lossy(&bytes);
        if version.starts_with("Open MPI v5") {
            assert!(pmix_abort_symbol().is_some(), "{version}");
            // Resolved, with its parked helper, once —
            // what `MpiBackend::world` / `import` do when MPI starts
            resolve_pmix_abort();
            assert!(matches!(PMIX_ABORTER.get(), Some(Some(_))), "{version}");
        }
    }

    static DOUBLE_CALLS: AtomicUsize = AtomicUsize::new(0);
    static DOUBLE_CODE: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(0);

    /// Test double of a `PMIx_Abort` whose server never answers.
    extern "C" fn never_returns(
        code: c_int,
        _: *const c_char,
        _: *mut std::ffi::c_void,
        _: usize,
    ) -> c_int {
        DOUBLE_CODE.store(code, Ordering::SeqCst);
        DOUBLE_CALLS.fetch_add(1, Ordering::SeqCst);
        loop {
            std::thread::park();
        }
    }

    static FAILING_CALLS: AtomicUsize = AtomicUsize::new(0);

    /// Test double of a `PMIx_Abort` that fails at once (no server).
    extern "C" fn fails_at_once(
        _: c_int,
        _: *const c_char,
        _: *mut std::ffi::c_void,
        _: usize,
    ) -> c_int {
        FAILING_CALLS.fetch_add(1, Ordering::SeqCst);
        -1
    }

    /// The abort-path request of a prepared
    /// aborter is bounded when `PMIx_Abort` never returns (the helper
    /// thread was created beforehand, the request only unparks it), sends
    /// the code of the first request only, and returns at once when
    /// `PMIx_Abort` fails at once.
    #[test]
    fn pmix_abort_request_is_bounded_and_sent_once() {
        let blocked = PmixAborter::start(never_returns).expect("helper thread");
        let t = Instant::now();
        assert!(!blocked.request(71, Duration::from_millis(300)));
        let waited = t.elapsed();
        assert!(
            waited >= Duration::from_millis(300) && waited < Duration::from_secs(5),
            "{waited:?}"
        );
        assert!(!blocked.request(5, Duration::from_millis(50)));
        assert_eq!(DOUBLE_CALLS.load(Ordering::SeqCst), 1);
        assert_eq!(DOUBLE_CODE.load(Ordering::SeqCst), 71);

        let failing = PmixAborter::start(fails_at_once).expect("helper thread");
        let t = Instant::now();
        assert!(failing.request(71, Duration::from_secs(2)));
        assert!(t.elapsed() < Duration::from_secs(1), "{:?}", t.elapsed());
        assert!(failing.request(71, Duration::from_secs(2)));
        assert_eq!(FAILING_CALLS.load(Ordering::SeqCst), 1);
    }

    /// The `Serialized` admission table.
    #[test]
    fn admission_rule_table() {
        use Exclusive::{Idup, Native, Split};
        let at = |native, split, idup| Registry {
            native,
            split,
            idup,
        };
        let idle = at(0, 0, 0);
        assert!(idle.admits(Native) && idle.admits(Split) && idle.admits(Idup));
        // A native reduction excludes natives and splits, not duplications.
        let n = at(1, 0, 0);
        assert!(!n.admits(Native) && !n.admits(Split) && n.admits(Idup));
        // A committed split excludes everything.
        let s = at(0, 1, 0);
        assert!(!s.admits(Native) && !s.admits(Split) && !s.admits(Idup));
        // A duplication in flight excludes splits only.
        let d = at(0, 0, 3);
        assert!(d.admits(Native) && !d.admits(Split) && d.admits(Idup));
    }

    /// Registration, release, leak and the bounded wait of a header-less
    /// creation (one test: the registry is process-wide).
    #[test]
    fn registry_guards() {
        let split = ExclusiveGuard::try_enter(Exclusive::Split).expect("idle registry");
        let busy = ExclusiveGuard::try_enter(Exclusive::Idup)
            .err()
            .expect("refused");
        assert!(busy.contains("1 committed split"), "{busy}");
        let t0 = Instant::now();
        let waited = ExclusiveGuard::enter_waiting(
            Exclusive::Idup,
            Some(Instant::now() + Duration::from_millis(50)),
        );
        assert!(waited.is_none());
        assert!(t0.elapsed() >= Duration::from_millis(50));
        // A waiting creation is admitted as soon as the split ends.
        std::thread::scope(|s| {
            let h = s.spawn(|| {
                ExclusiveGuard::enter_waiting(
                    Exclusive::Idup,
                    Some(Instant::now() + Duration::from_secs(10)),
                )
            });
            std::thread::sleep(Duration::from_millis(20));
            drop(split);
            let dup = h.join().expect("waiter").expect("admitted");
            assert!(ExclusiveGuard::try_enter(Exclusive::Split).is_err());
            assert!(ExclusiveGuard::try_enter(Exclusive::Native).is_ok());
            drop(dup);
        });
        assert!(registry().admits(Exclusive::Split));
        // A leaked registration stays.
        ExclusiveGuard::try_enter(Exclusive::Idup)
            .expect("idle")
            .leak();
        assert!(ExclusiveGuard::try_enter(Exclusive::Split).is_err());
        registry().idup -= 1; // test cleanup
    }

    /// D-067 (2): MPI is finalised at exit only after a
    /// normal end — never after a panic, and (where the C library reports
    /// it, `on_exit`) only for exit status 0.
    #[test]
    fn the_exit_hook_finalises_only_after_a_normal_end() {
        assert!(normal_end(false, Some(0)));
        assert!(normal_end(false, None), "atexit targets: no status");
        assert!(!normal_end(false, Some(1)), "Err from main");
        assert!(!normal_end(false, Some(3)), "process::exit(3)");
        assert!(!normal_end(false, Some(101)), "panicking main");
        assert!(!normal_end(true, Some(0)), "panic on another thread");
        assert!(!normal_end(true, None), "panic, atexit targets");
    }
}
