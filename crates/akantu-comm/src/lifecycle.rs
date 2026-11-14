// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/synchronizer/communicator.cc
// (Communicator::getStaticCommunicator, MPI_Init/MPI_Finalize ownership),
// src/common/aka_common.cc (initialize/finalize), python/akantu/__init__.py
// (mpi4py imported first when MPI is enabled).

//! MPI lifecycle and thread-level policy (D-015/D-023).
//!
//! Rules (normative; the rsmpi backend implements
//! [`MpiRuntime`], tests use a mock):
//!
//! * **Ownership.** If MPI is already initialised when akantu first needs it
//!   (mpi4py imported first, a host C/C++/Fortran application), the
//!   environment is [`InitOwnership::Imported`]: akantu never calls
//!   `MPI_Finalize`. Otherwise akantu initialises it
//!   ([`InitOwnership::Owned`]) with `MPI_Init_thread(required)` and
//!   finalises it when the **last** [`MpiEnvironment`] guard is dropped.
//! * **Handle lifetime.** Every communicator, request, PETSc/MUMPS backend
//!   object and model holding a communicator keeps a guard (directly or
//!   through its communicator), so finalisation happens strictly after
//!   models → native backends (PETSc objects, then `PetscFinalize`) →
//!   requests → communicators. Imported communicators (e.g. from
//!   `mpi4py.MPI.Comm`) are duplicated (`MPI_Comm_dup`) so akantu's tags
//!   never collide with the host's traffic; the duplicate is freed with its
//!   last handle.
//! * **No re-initialisation.** After an owned finalisation, `initialize`
//!   returns an error: MPI forbids `MPI_Init` after `MPI_Finalize`.
//! * **Thread level.** `provided < required` is an error at initialisation.
//!   Under `Single`/`Funneled` every MPI call must come from MPI's **main
//!   thread** ([`ThreadPolicy::check`] returns an error otherwise); under
//!   `Serialized` calls are serialised by a process-wide lock; only
//!   `Multiple` allows concurrent calls. A cloned communicator does **not**
//!   grant use from another thread.
//! * **Main thread identity**. For an owned
//!   initialisation the main thread is the thread that called
//!   `MPI_Init_thread`. For an imported environment akantu asks MPI
//!   (`MPI_Is_thread_main`): under `Single`/`Funneled` the first entry must
//!   come from MPI's main thread (an mpi4py-first program entering akantu
//!   from a worker thread gets an error instead of a wrong identity); under
//!   `Serialized`/`Multiple` the identity is recorded only if the entering
//!   thread is MPI's main thread.
//! * **Destruction and finalisation follow the policy too**.
//!   Freeing a communicator, request or native backend object is an MPI
//!   call: [`Lifecycle::release_handle`] runs it at once if the current
//!   thread may call MPI (under the `Serialized` lock), otherwise it is
//!   **deferred** to the main thread ([`Lifecycle::poll`]). `MPI_Finalize`
//!   is only ever called on the main thread, after every deferred
//!   destruction: when the last guard is dropped on another thread the
//!   lifecycle becomes *finalisation pending* and the main thread completes
//!   it at its next [`Lifecycle::poll`]/[`Lifecycle::shutdown`] (the facade
//!   calls `shutdown` at exit and from Python `atexit`), or when the
//!   lifecycle itself is dropped there.
//! * **No destruction is overtaken by finalisation**.
//!   Guard count, lifecycle state, the deferred-destruction queue and the
//!   *draining* flag live under **one** lock. Every destruction is queued
//!   under that lock; a holder queues its handles' destructions before it
//!   drops its guard, so every such enqueue happens-before the transition
//!   to *finalisation pending* (set by the last guard drop, under the same
//!   lock). The terminal transition to *finalised* is taken under the lock
//!   only when the queue is empty **and** no thread is draining it; the
//!   main thread drains (and waits for another drainer) until then. A
//!   destruction released after the terminal transition (only possible
//!   without a live guard, a contract violation) has nothing left to free
//!   and is dropped. Destruction closures run **outside** the lifecycle
//!   lock, one drainer at a time; a release issued from inside a running
//!   destruction (a native object freeing a nested handle) is queued and
//!   run by the same drain loop, never re-entering the `Serialized` MPI
//!   lock it holds.
//! * **No MPI call once MPI is finalised** (decision D-067 (1)).
//!   A host that imported akantu may finalise MPI itself (a C or
//!   Fortran host's `MPI_Finalize`, rsmpi's `Universe` drop, mpi4py at
//!   interpreter exit) while akantu objects still hold handles. Every
//!   destruction released, queued or drained once `MPI_Finalized` is true
//!   is dropped without running (a debug note, no error; counted by
//!   [`Lifecycle::dropped_after_finalize`]), and a pending finalisation is
//!   completed without a second `MPI_Finalize`.
//! * **Exit mode** (D-067 (3)). The process-exit hook of
//!   the MPI backend switches the lifecycle into exit mode first: from then
//!   on the lifecycle lock, the wait for another drainer and the
//!   `Serialized` MPI lock are only *tried*. When one is held by another
//!   thread the exit path gives up — no further MPI call, MPI stays
//!   unfinalised and the launcher reports the rank, as before the hook
//!   existed — and records why (`Lifecycle::exit_blocker`) for the hook's
//!   one emergency note.
//! * **Errors.** Communicators use `MPI_ERRORS_RETURN`; MPI failures become
//!   `Error::Communication` and follow the collective protocol of
//!   [`crate::phase`]; only the top-level runtime may call `MPI_Abort`.

use std::cell::Cell;
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, TryLockError};
use std::thread::ThreadId;

use akantu_core::{Error, Result};

/// MPI thread support levels (ordered as in the MPI standard).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum ThreadLevel {
    /// `MPI_THREAD_SINGLE`
    Single,
    /// `MPI_THREAD_FUNNELED`
    Funneled,
    /// `MPI_THREAD_SERIALIZED`
    Serialized,
    /// `MPI_THREAD_MULTIPLE`
    Multiple,
}

/// Who initialised MPI.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InitOwnership {
    /// akantu called `MPI_Init_thread` and will call `MPI_Finalize`.
    Owned,
    /// MPI was initialised by the host (mpi4py, application); never
    /// finalised by akantu.
    Imported,
}

/// The process-level MPI operations the lifecycle needs.
pub trait MpiRuntime: Send + Sync + std::fmt::Debug {
    /// `MPI_Initialized`.
    fn is_initialized(&self) -> bool;
    /// `MPI_Finalized`.
    fn is_finalized(&self) -> bool;
    /// `MPI_Init_thread(required) -> provided`.
    fn init_thread(&self, required: ThreadLevel) -> Result<ThreadLevel>;
    /// `MPI_Query_thread`.
    fn query_thread(&self) -> ThreadLevel;
    /// `MPI_Is_thread_main` for the calling thread.
    fn is_thread_main(&self) -> bool;
    /// `MPI_Finalize`.
    fn finalize(&self) -> Result<()>;
}

/// A deferred MPI destruction (communicator, request, native object).
type Deferred = Box<dyn FnOnce() + Send>;

enum State {
    Uninitialized,
    Active {
        ownership: InitOwnership,
        policy: ThreadPolicy,
        guards: usize,
    },
    /// Last guard dropped off the main thread: the main thread finalises.
    FinalizePending {
        policy: ThreadPolicy,
    },
    Finalized,
}

impl std::fmt::Debug for State {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Uninitialized => write!(f, "Uninitialized"),
            Self::Active { guards, .. } => write!(f, "Active({guards} guards)"),
            Self::FinalizePending { .. } => write!(f, "FinalizePending"),
            Self::Finalized => write!(f, "Finalized"),
        }
    }
}

/// Everything the ordering contract couples, under one lock.
struct Inner {
    state: State,
    /// Destructions waiting to be run.
    deferred: Vec<Deferred>,
    /// A thread is running queued destructions (at most one at a time).
    draining: bool,
}

thread_local! {
    /// The current thread is running a destruction closure.
    static IN_DESTRUCTION: Cell<bool> = const { Cell::new(false) };
    /// This thread's [`ThreadKey`] (0: not assigned yet).
    static THREAD_KEY: Cell<u64> = const { Cell::new(0) };
    /// `std::thread::current().id()`, cached by [`ThreadKey::current_with_id`].
    static STD_THREAD_ID: Cell<Option<ThreadId>> = const { Cell::new(None) };
}

/// Next [`ThreadKey`] to hand out.
static NEXT_THREAD_KEY: AtomicU64 = AtomicU64::new(1);

/// Identity of a thread that stays usable after the thread's
/// thread-local destructors ran.
///
/// `std::thread::current()` panics (and, in an `extern "C"` callback,
/// aborts the process) once the calling thread's thread-local data has
/// been destroyed, which on macOS is the case in C `atexit` handlers (the
/// exiting thread's TLS destructors run first; measured with Rust 1.99.1:
/// "use of std::thread::current() is not possible after the thread's local
/// data has been destroyed"). On glibc (2.39, 2.41) it still answers there,
/// but nothing guarantees it. The process-exit finalisation hook
/// (`register_exit_hook` of the MPI backend) drops communicators and
/// finalises MPI, so the thread checks of the lifecycle use this key — a
/// process-unique number kept in a `const`-initialised thread local
/// without destructor, which is readable for the whole life of the thread —
/// instead of `ThreadId`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ThreadKey(u64);

impl ThreadKey {
    /// The calling thread's key (assigned on first use).
    pub(crate) fn current() -> Self {
        Self(THREAD_KEY.with(|k| {
            let v = k.get();
            if v != 0 {
                return v;
            }
            let n = NEXT_THREAD_KEY.fetch_add(1, Ordering::Relaxed);
            k.set(n);
            n
        }))
    }

    /// The calling thread's key and `ThreadId`, which is cached for
    /// [`ThreadKey::cached_std_id`]. Calls `std::thread::current()`: only
    /// while the thread's thread-local data is alive (initialisation).
    pub(crate) fn current_with_id() -> (Self, ThreadId) {
        let id = std::thread::current().id();
        STD_THREAD_ID.with(|c| c.set(Some(id)));
        (Self::current(), id)
    }

    /// The `ThreadId` cached by [`ThreadKey::current_with_id`] on this
    /// thread, if any (never calls `std::thread::current()`).
    // read by the MPI backend's `finalize` only (feature `mpi`)
    #[cfg_attr(not(feature = "mpi"), allow(dead_code))]
    pub(crate) fn cached_std_id() -> Option<ThreadId> {
        STD_THREAD_ID.with(Cell::get)
    }
}

/// Why the exit path left MPI unfinalised (D-067 (3)): the lock or wait it
/// would have blocked on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ExitBlocker {
    /// Another thread held the lifecycle lock.
    LifecycleLock,
    /// Another thread was running queued MPI destructions.
    Drainer,
    /// Another thread held the `Serialized` MPI lock.
    SerialLock,
}

impl ExitBlocker {
    const ALL: [Self; 3] = [Self::LifecycleLock, Self::Drainer, Self::SerialLock];

    fn code(self) -> u8 {
        match self {
            Self::LifecycleLock => 1,
            Self::Drainer => 2,
            Self::SerialLock => 3,
        }
    }

    /// What was held, for the emergency note.
    pub(crate) fn describe(self) -> &'static str {
        match self {
            Self::LifecycleLock => "another thread holds the MPI lifecycle lock",
            Self::Drainer => "another thread is running queued MPI destructions",
            Self::SerialLock => "another thread holds the Serialized MPI lock",
        }
    }

    fn error(self) -> Error {
        Error::Communication(format!(
            "process exit: MPI left unfinalised ({}; D-067)",
            self.describe()
        ))
    }
}

/// Process-level MPI lifecycle (one per process; the facade owns it).
pub struct Lifecycle<R: MpiRuntime> {
    runtime: R,
    inner: Mutex<Inner>,
    /// Signalled when a drainer finishes.
    drained: Condvar,
    /// Exit mode (D-067 (3)): set by the process-exit hook; every lock and
    /// wait of the lifecycle is then only tried.
    exiting: AtomicBool,
    /// First [`ExitBlocker`] the exit path met (`ExitBlocker::code`, 0:
    /// none); once set, the exit path makes no further MPI call.
    exit_blocked: AtomicU8,
    /// Destructions dropped because MPI was already finalised (D-067 (1)).
    dropped_after_finalize: AtomicUsize,
}

impl<R: MpiRuntime> std::fmt::Debug for Lifecycle<R> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let state = self.inner.lock().map(|g| format!("{:?}", g.state));
        f.debug_struct("Lifecycle")
            .field("runtime", &self.runtime)
            .field("state", &state)
            .finish_non_exhaustive()
    }
}

/// Clears the draining flag (and wakes waiters) even if a destruction
/// closure panics. In exit mode it only tries the lifecycle lock (a held
/// lock leaves the flag set: the exit path has given up anyway).
struct DrainGuard<'a, R: MpiRuntime> {
    lifecycle: &'a Lifecycle<R>,
}

impl<R: MpiRuntime> Drop for DrainGuard<'_, R> {
    fn drop(&mut self) {
        let l = self.lifecycle;
        let mut g = if l.exiting() {
            match l.inner.try_lock() {
                Ok(g) => g,
                Err(TryLockError::Poisoned(p)) => p.into_inner(),
                Err(TryLockError::WouldBlock) => {
                    l.give_up(ExitBlocker::LifecycleLock);
                    return;
                }
            }
        } else {
            l.inner
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
        };
        g.draining = false;
        drop(g);
        l.drained.notify_all();
    }
}

/// Marks the current thread as running a destruction (panic-safe).
struct InDestruction;

impl InDestruction {
    fn enter() -> Self {
        IN_DESTRUCTION.with(|c| c.set(true));
        Self
    }
}

impl Drop for InDestruction {
    fn drop(&mut self) {
        IN_DESTRUCTION.with(|c| c.set(false));
    }
}

/// Thread-use policy derived from the provided level.
#[derive(Clone, Debug)]
pub struct ThreadPolicy {
    provided: ThreadLevel,
    /// MPI's main thread, when known (always for owned initialisations
    /// and for imported `Single`/`Funneled`).
    main_thread: Option<ThreadId>,
    /// The same thread's [`ThreadKey`], used by every thread check (usable
    /// in process-exit hooks).
    main_key: Option<ThreadKey>,
    serial_lock: Arc<Mutex<()>>,
}

impl ThreadPolicy {
    /// Level granted by MPI.
    #[must_use]
    pub fn provided(&self) -> ThreadLevel {
        self.provided
    }

    /// MPI's main thread, if known.
    #[must_use]
    pub fn main_thread(&self) -> Option<ThreadId> {
        self.main_thread
    }

    /// `true` if the current thread is MPI's main thread. Never calls
    /// `std::thread::current()`, so it also answers in a process-exit hook
    /// after the thread's thread-local destructors ran.
    #[must_use]
    pub fn on_main_thread(&self) -> bool {
        self.main_key.is_some() && self.main_key == Some(ThreadKey::current())
    }

    /// Check that the current thread may issue an MPI call.
    pub fn check(&self) -> Result<()> {
        match self.provided {
            ThreadLevel::Single | ThreadLevel::Funneled if !self.on_main_thread() => {
                Err(Error::Communication(format!(
                    "MPI call from a thread other than MPI's main thread under {:?}",
                    self.provided
                )))
            }
            _ => Ok(()),
        }
    }

    /// `true` if the current thread holds this policy's `Serialized` lock
    /// (inside [`ThreadPolicy::call`], e.g. a native region).
    /// Always `false` under the other thread levels.
    #[must_use]
    pub fn held_by_current_thread(&self) -> bool {
        self.provided == ThreadLevel::Serialized && {
            let key = Arc::as_ptr(&self.serial_lock) as usize;
            SERIAL_HELD.with(|h| h.borrow().contains(&key))
        }
    }

    /// Emergency access to the `Serialized` MPI lock:
    /// polls `try_lock` until `wait` has elapsed and returns `None` if
    /// another thread held the lock throughout (e.g. a thread stuck in a
    /// failed blocking MPI call) — the caller must then not call MPI. A
    /// poisoned lock still serialises and is returned.
    #[must_use]
    pub fn try_serial_lock(&self, wait: std::time::Duration) -> Option<MutexGuard<'_, ()>> {
        let deadline = std::time::Instant::now() + wait;
        loop {
            match self.serial_lock.try_lock() {
                Ok(g) => return Some(g),
                Err(std::sync::TryLockError::Poisoned(p)) => return Some(p.into_inner()),
                Err(std::sync::TryLockError::WouldBlock) => {}
            }
            if std::time::Instant::now() >= deadline {
                return None;
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
    }

    /// Run `f` as one MPI call under the policy (serialised for
    /// `Serialized`).
    ///
    /// Under `Serialized` the lock is **re-entrant per thread**:
    /// a call made while the current thread already holds this policy's
    /// lock — a native library region (`Communicator::native_region`, PETSc
    /// `SNESSolve`) whose callbacks communicate through akantu on the same
    /// thread — runs `f` directly. MPI's `Serialized` contract only forbids
    /// concurrent calls from *different* threads; before this rule such a
    /// nested call deadlocked, so no existing caller changes behaviour.
    pub fn call<T>(&self, f: impl FnOnce() -> T) -> Result<T> {
        self.check()?;
        if self.provided == ThreadLevel::Serialized {
            let key = Arc::as_ptr(&self.serial_lock) as usize;
            if SERIAL_HELD.with(|h| h.borrow().contains(&key)) {
                return Ok(f());
            }
            let _g = self
                .serial_lock
                .lock()
                .map_err(|_| Error::Communication("poisoned MPI lock".into()))?;
            let _held = SerialHeld::enter(key);
            Ok(f())
        } else {
            Ok(f())
        }
    }
}

thread_local! {
    /// Serialized locks (by address) held by the current thread through
    /// [`ThreadPolicy::call`] (re-entrancy).
    static SERIAL_HELD: std::cell::RefCell<Vec<usize>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// Marks a `Serialized` lock as held by the current thread (panic-safe).
struct SerialHeld(usize);

impl SerialHeld {
    fn enter(key: usize) -> Self {
        SERIAL_HELD.with(|h| h.borrow_mut().push(key));
        Self(key)
    }
}

impl Drop for SerialHeld {
    fn drop(&mut self) {
        SERIAL_HELD.with(|h| {
            let mut h = h.borrow_mut();
            if let Some(i) = h.iter().rposition(|&k| k == self.0) {
                h.remove(i);
            }
        });
    }
}

/// Guard keeping MPI alive; cloned into every communicator/backend.
#[derive(Debug)]
pub struct MpiEnvironment<'l, R: MpiRuntime> {
    lifecycle: &'l Lifecycle<R>,
    ownership: InitOwnership,
    policy: ThreadPolicy,
}

impl<R: MpiRuntime> MpiEnvironment<'_, R> {
    /// Who initialised MPI.
    #[must_use]
    pub fn ownership(&self) -> InitOwnership {
        self.ownership
    }

    /// Thread policy.
    #[must_use]
    pub fn policy(&self) -> &ThreadPolicy {
        &self.policy
    }
}

impl<R: MpiRuntime> Clone for MpiEnvironment<'_, R> {
    fn clone(&self) -> Self {
        if let Ok(mut g) = self.lifecycle.inner.lock()
            && let State::Active { guards, .. } = &mut g.state
        {
            *guards += 1;
        }
        Self {
            lifecycle: self.lifecycle,
            ownership: self.ownership,
            policy: self.policy.clone(),
        }
    }
}

impl<R: MpiRuntime> Drop for MpiEnvironment<'_, R> {
    fn drop(&mut self) {
        self.lifecycle.release();
    }
}

impl<R: MpiRuntime> Lifecycle<R> {
    /// Lifecycle over `runtime` (nothing is initialised yet).
    pub fn new(runtime: R) -> Self {
        Self {
            runtime,
            inner: Mutex::new(Inner {
                state: State::Uninitialized,
                deferred: Vec::new(),
                draining: false,
            }),
            drained: Condvar::new(),
            exiting: AtomicBool::new(false),
            exit_blocked: AtomicU8::new(0),
            dropped_after_finalize: AtomicUsize::new(0),
        }
    }

    /// The lifecycle lock (an error if poisoned). In exit mode it is only
    /// tried: a lock held by another thread, or an exit path that has
    /// already given up, is an error ([`ExitBlocker`]).
    fn lock(&self) -> Result<MutexGuard<'_, Inner>> {
        if self.exiting() {
            if let Some(b) = self.exit_blocker() {
                return Err(b.error());
            }
            return match self.inner.try_lock() {
                Ok(g) => Ok(g),
                Err(TryLockError::Poisoned(_)) => {
                    Err(Error::Communication("poisoned MPI lifecycle".into()))
                }
                Err(TryLockError::WouldBlock) => {
                    self.give_up(ExitBlocker::LifecycleLock);
                    Err(ExitBlocker::LifecycleLock.error())
                }
            };
        }
        self.inner
            .lock()
            .map_err(|_| Error::Communication("poisoned MPI lifecycle".into()))
    }

    /// [`Lifecycle::lock`] for the infallible paths (`None`: poisoned, or an
    /// exit-mode refusal).
    fn lock_opt(&self) -> Option<MutexGuard<'_, Inner>> {
        self.lock().ok()
    }

    fn exiting(&self) -> bool {
        self.exiting.load(Ordering::Acquire)
    }

    /// Switch to exit mode (the process-exit hook, D-067 (3)); never undone.
    #[cfg_attr(not(feature = "mpi"), allow(dead_code))] // the hook is the MPI backend's
    pub(crate) fn begin_exit(&self) {
        self.exiting.store(true, Ordering::Release);
    }

    /// Record why the exit path gives up (the first reason is kept).
    fn give_up(&self, why: ExitBlocker) {
        let _ =
            self.exit_blocked
                .compare_exchange(0, why.code(), Ordering::AcqRel, Ordering::Acquire);
    }

    /// Why the exit path gave up, if it did (D-067 (3)).
    pub(crate) fn exit_blocker(&self) -> Option<ExitBlocker> {
        let code = self.exit_blocked.load(Ordering::Acquire);
        ExitBlocker::ALL.into_iter().find(|b| b.code() == code)
    }

    /// Run `f` as one MPI call under `policy` ([`ThreadPolicy::call`]); in
    /// exit mode the `Serialized` lock is only tried (D-067 (3)).
    fn policy_call<T>(&self, policy: &ThreadPolicy, f: impl FnOnce() -> T) -> Result<T> {
        if !self.exiting() {
            return policy.call(f);
        }
        match self.try_serial(policy) {
            Ok(_serial) => Ok(f()),
            Err(e) => {
                if self.exit_blocker().is_some() {
                    // the exit path gave up: no code of `f` runs, not even
                    // its captures' destructors
                    std::mem::forget(f);
                }
                Err(e)
            }
        }
    }

    /// Exit mode: the thread check of `policy`, then the `Serialized` lock
    /// if the level needs it, only tried (`None` for other levels).
    fn try_serial<'p>(&self, policy: &'p ThreadPolicy) -> Result<Option<MutexGuard<'p, ()>>> {
        if let Some(b) = self.exit_blocker() {
            return Err(b.error());
        }
        policy.check()?;
        if policy.provided != ThreadLevel::Serialized {
            return Ok(None);
        }
        match policy.serial_lock.try_lock() {
            Ok(g) => Ok(Some(g)),
            Err(TryLockError::Poisoned(_)) => Err(Error::Communication("poisoned MPI lock".into())),
            Err(TryLockError::WouldBlock) => {
                self.give_up(ExitBlocker::SerialLock);
                Err(ExitBlocker::SerialLock.error())
            }
        }
    }

    /// `n` destructions were dropped because MPI is already finalised
    /// (D-067 (1)): counted, and a debug note outside exit mode (the exit
    /// path prints nothing but its one emergency note).
    fn note_dropped(&self, n: usize) {
        if n == 0 {
            return;
        }
        self.dropped_after_finalize.fetch_add(n, Ordering::Relaxed);
        if !self.exiting() {
            akantu_core::aka_debug_info!(
                "{n} MPI handle destruction(s) dropped: MPI is already finalised (D-067)"
            );
        }
    }

    /// Number of handle destructions dropped without running because MPI
    /// was already finalised when they were released, queued or drained
    /// (D-067 (1): no MPI call after `MPI_Finalize`).
    pub fn dropped_after_finalize(&self) -> usize {
        self.dropped_after_finalize.load(Ordering::Relaxed)
    }

    /// Underlying runtime.
    pub fn runtime(&self) -> &R {
        &self.runtime
    }

    /// Obtain a guard, initialising MPI if needed (see module rules).
    pub fn initialize(&self, required: ThreadLevel) -> Result<MpiEnvironment<'_, R>> {
        let mut g = self.lock()?;
        let s = &mut g.state;
        let (ownership, policy) = match &mut *s {
            State::Finalized | State::FinalizePending { .. } => {
                return Err(Error::Communication(
                    "MPI was finalised (or its finalisation is pending) in this process and cannot be re-initialised".into(),
                ));
            }
            State::Active {
                ownership,
                policy,
                guards,
            } => {
                // A host may finalise the MPI it
                // initialised (rsmpi's `Universe` drop, mpi4py at exit)
                // while an imported environment is active; a new guard
                // would then let the caller make MPI calls after
                // `MPI_Finalize` (D-067 (1)). `MPI_Finalized` may be called
                // at any time; asked for an owned environment too (a host
                // finalising akantu's MPI is as erroneous).
                if self.runtime.is_finalized() {
                    return Err(Error::Communication(
                        "MPI was finalised by the host while akantu's MPI environment is active: \
                         no new MPI handle can be created (finalise MPI only after every akantu \
                         communicator was dropped)"
                            .into(),
                    ));
                }
                if policy.provided < required {
                    return Err(Error::Communication(format!(
                        "MPI provides {:?}, {required:?} required",
                        policy.provided
                    )));
                }
                *guards += 1;
                (*ownership, policy.clone())
            }
            State::Uninitialized => {
                if self.runtime.is_finalized() {
                    return Err(Error::Communication(
                        "MPI was finalised by the host and cannot be re-initialised".into(),
                    ));
                }
                let (ownership, provided, main_thread) = if self.runtime.is_initialized() {
                    let provided = self.runtime.query_thread();
                    let is_main = self.runtime.is_thread_main();
                    if provided <= ThreadLevel::Funneled && !is_main {
                        return Err(Error::Communication(format!(
                            "imported MPI environment provides {provided:?}: akantu must first be \
                             entered from MPI's main thread (MPI_Is_thread_main is false here)"
                        )));
                    }
                    let main = is_main.then(ThreadKey::current_with_id);
                    (InitOwnership::Imported, provided, main)
                } else {
                    let provided = self.runtime.init_thread(required)?;
                    (
                        InitOwnership::Owned,
                        provided,
                        Some(ThreadKey::current_with_id()),
                    )
                };
                let policy = ThreadPolicy {
                    provided,
                    main_thread: main_thread.map(|(_, id)| id),
                    main_key: main_thread.map(|(key, _)| key),
                    serial_lock: Arc::new(Mutex::new(())),
                };
                if provided < required {
                    // Keep MPI initialised (it cannot be re-initialised) but
                    // refuse to hand out a guard with insufficient support.
                    *s = State::Active {
                        ownership,
                        policy,
                        guards: 0,
                    };
                    return Err(Error::Communication(format!(
                        "MPI provides {provided:?}, {required:?} required"
                    )));
                }
                *s = State::Active {
                    ownership,
                    policy: policy.clone(),
                    guards: 1,
                };
                (ownership, policy)
            }
        };
        Ok(MpiEnvironment {
            lifecycle: self,
            ownership,
            policy,
        })
    }

    fn release(&self) {
        let Some(mut g) = self.lock_opt() else { return };
        let State::Active {
            ownership,
            guards,
            policy,
        } = &mut g.state
        else {
            return;
        };
        *guards = guards.saturating_sub(1);
        if *guards > 0 || *ownership != InitOwnership::Owned {
            return;
        }
        let policy = policy.clone();
        let on_main = policy.on_main_thread();
        // Under the same lock as every enqueue: every destruction
        // queued by a guard holder is already in the queue.
        g.state = State::FinalizePending { policy };
        drop(g);
        // A guard dropped inside a running destruction (a closure owning
        // the last guard) must not wait for its own drain: the drain loop
        // that runs it finalises afterwards.
        if on_main && !IN_DESTRUCTION.with(Cell::get) {
            // Errors at finalisation cannot be propagated from Drop.
            let _ = self.finish_pending();
        }
        // Otherwise never finalise off the main thread (MPI requires the
        // initialising thread, for every thread level): the main thread
        // completes it at its next poll/shutdown.
    }

    /// Run a native-handle destruction (`MPI_Comm_free`, `MPI_Request_free`,
    /// PETSc `*Destroy`) under the thread policy. The destruction is queued
    /// under the lifecycle lock; if the current thread may call
    /// MPI, is not already running a destruction and no other thread is
    /// draining, it drains the queue at once (serialised under
    /// `Serialized`); otherwise the active drainer or the main thread's
    /// next [`Lifecycle::poll`] runs it — always before finalisation.
    /// Without an active environment (never initialised, or already
    /// finalised) the destruction is dropped (nothing to free); so it is
    /// once MPI is finalised by anyone (`MPI_Finalized`, e.g. a host that
    /// finalised the MPI it initialised: D-067 (1), a debug note, see
    /// [`Lifecycle::dropped_after_finalize`]). In exit mode, a lock held by
    /// another thread leaves the destruction unrun (D-067 (3)).
    pub fn release_handle(&self, destroy: impl FnOnce() + Send + 'static) {
        let mpi_finalized = self.runtime.is_finalized();
        let Some(mut g) = self.lock_opt() else {
            if self.exiting() {
                // the exit path gave up: run no code of the destruction,
                // not even its captures' destructors
                std::mem::forget(destroy);
            }
            return;
        };
        let policy = match &g.state {
            State::Active { policy, .. } | State::FinalizePending { policy } => policy.clone(),
            _ => return,
        };
        if mpi_finalized {
            drop(g);
            drop(destroy);
            self.note_dropped(1);
            return;
        }
        g.deferred.push(Box::new(destroy));
        let drain_here = policy.check().is_ok() && !g.draining && !IN_DESTRUCTION.with(Cell::get);
        if drain_here {
            g.draining = true;
        }
        drop(g);
        if drain_here {
            let _ = self.drain(&policy);
            // A destruction may have dropped the last guard on the main
            // thread (finalisation pending): complete it here.
            if policy.on_main_thread() && self.is_finalize_pending() {
                let _ = self.finish_pending();
            }
        }
    }

    /// Run queued destructions until the queue is empty. The caller set
    /// `draining`; it is cleared on return (also on panic). Closures run
    /// outside the lifecycle lock. Returns the first error (every queued
    /// destruction is still attempted). Once MPI is finalised (checked
    /// before each destruction) the rest of the queue is dropped unrun
    /// (D-067 (1)); when the exit path gives up, the rest is left unrun
    /// without running any of its code (D-067 (3)).
    fn drain(&self, policy: &ThreadPolicy) -> Result<()> {
        let _guard = DrainGuard { lifecycle: self };
        let mut first_err = None;
        loop {
            let batch = {
                let mut g = self.lock()?;
                if g.deferred.is_empty() {
                    return first_err.map_or(Ok(()), Err);
                }
                std::mem::take(&mut g.deferred)
            };
            let mut batch = batch.into_iter();
            while let Some(d) = batch.next() {
                if self.runtime.is_finalized() {
                    let mut dropped: Vec<Deferred> = vec![d];
                    dropped.extend(batch);
                    if let Some(mut g) = self.lock_opt() {
                        dropped.append(&mut g.deferred);
                    }
                    let n = dropped.len();
                    // dropped outside the lifecycle lock (captures' drops
                    // may release nested handles)
                    drop(dropped);
                    self.note_dropped(n);
                    return first_err.map_or(Ok(()), Err);
                }
                let _mark = InDestruction::enter();
                if let Err(e) = self.policy_call(policy, d) {
                    if self.exit_blocker().is_some() {
                        batch.for_each(std::mem::forget);
                        return Err(e);
                    }
                    first_err.get_or_insert(e);
                }
            }
        }
    }

    /// Number of destructions waiting to be run.
    pub fn deferred_len(&self) -> usize {
        self.inner.lock().map_or(0, |g| g.deferred.len())
    }

    /// On MPI's main thread: run deferred destructions, then a pending
    /// finalisation. Returns `Ok(true)` if MPI was finalised by this call.
    /// On another thread: an error under `Single`/`Funneled` with pending
    /// work, otherwise `Ok(false)`.
    pub fn poll(&self) -> Result<bool> {
        let (policy, pending_work) = {
            let g = self.lock()?;
            match &g.state {
                State::Active { policy, .. } | State::FinalizePending { policy } => {
                    (policy.clone(), !g.deferred.is_empty())
                }
                _ => return Ok(false),
            }
        };
        if !policy.on_main_thread() {
            if pending_work && policy.check().is_err() {
                return Err(Error::Communication(
                    "deferred MPI destructions must be completed on MPI's main thread".into(),
                ));
            }
            return Ok(false);
        }
        self.finish_pending()
    }

    /// Main-thread shutdown (facade exit / Python `atexit`): like
    /// [`Lifecycle::poll`], but an error if called from another thread
    /// while a finalisation is pending.
    pub fn shutdown(&self) -> Result<bool> {
        let pending_elsewhere = matches!(
            self.lock().as_deref().map(|g| &g.state),
            Ok(State::FinalizePending { policy }) if !policy.on_main_thread()
        );
        if pending_elsewhere {
            return Err(Error::Communication(
                "MPI finalisation must run on MPI's main thread".into(),
            ));
        }
        self.poll()
    }

    /// Main thread only: drain every deferred destruction (waiting for
    /// another drainer), then — under the lock, with an empty queue and no
    /// drainer — take the terminal transition and call `MPI_Finalize` if
    /// pending (serialised under `Serialized`). If MPI is already finalised
    /// (by the host, or by a direct `MPI_Finalize`), the queue is dropped
    /// and a pending finalisation ends without a second `MPI_Finalize`
    /// (D-067 (1)). In exit mode nothing is waited for (D-067 (3)).
    fn finish_pending(&self) -> Result<bool> {
        let mut first_err = None;
        loop {
            let mut g = self.lock()?;
            while g.draining {
                if self.exiting() {
                    self.give_up(ExitBlocker::Drainer);
                    return Err(ExitBlocker::Drainer.error());
                }
                g = self
                    .drained
                    .wait(g)
                    .map_err(|_| Error::Communication("poisoned MPI lifecycle".into()))?;
            }
            let policy = match &g.state {
                State::Active { policy, .. } | State::FinalizePending { policy } => policy.clone(),
                _ => return first_err.map_or(Ok(false), Err),
            };
            if self.runtime.is_finalized() {
                let dropped = std::mem::take(&mut g.deferred);
                if matches!(g.state, State::FinalizePending { .. }) {
                    g.state = State::Finalized;
                }
                drop(g);
                let n = dropped.len();
                drop(dropped);
                self.note_dropped(n);
                return first_err.map_or(Ok(false), Err);
            }
            if !g.deferred.is_empty() {
                g.draining = true;
                drop(g);
                if let Err(e) = self.drain(&policy) {
                    first_err.get_or_insert(e);
                }
                continue;
            }
            if matches!(g.state, State::FinalizePending { .. }) {
                // Exit mode: the `Serialized` lock is taken (tried) before
                // the terminal transition, so a refusal leaves the state as
                // it is.
                let exit_serial = if self.exiting() {
                    Some(self.try_serial(&policy)?)
                } else {
                    None
                };
                // Terminal transition: no destruction queued, none running,
                // and none can be queued by a guard holder any more. The
                // state moves to Finalized even if MPI_Finalize reports an
                // error: no second attempt.
                g.state = State::Finalized;
                drop(g);
                match exit_serial {
                    Some(_serial) => self.runtime.finalize()?,
                    None => policy.call(|| self.runtime.finalize())??,
                }
                return first_err.map_or(Ok(true), Err);
            }
            return first_err.map_or(Ok(false), Err);
        }
    }

    /// `true` once akantu finalised MPI (or completed a pending
    /// finalisation of an environment finalised outside the lifecycle).
    pub fn is_finalized(&self) -> bool {
        matches!(
            self.lock().as_deref().map(|g| &g.state),
            Ok(State::Finalized)
        )
    }

    /// The ownership and the number of live guards while MPI is active
    /// through the lifecycle; `None` before the first [`Lifecycle::initialize`],
    /// while a finalisation is pending and once finalised. Diagnostics (the
    /// Python module's lifecycle tests check that no akantu handle outlives
    /// its owners).
    pub fn active(&self) -> Option<(InitOwnership, usize)> {
        match self.lock().as_deref().map(|g| &g.state) {
            Ok(State::Active {
                ownership, guards, ..
            }) => Some((*ownership, *guards)),
            _ => None,
        }
    }

    /// `true` while a finalisation waits for the main thread.
    pub fn is_finalize_pending(&self) -> bool {
        matches!(
            self.lock().as_deref().map(|g| &g.state),
            Ok(State::FinalizePending { .. })
        )
    }
}

impl<R: MpiRuntime> Drop for Lifecycle<R> {
    fn drop(&mut self) {
        // The facade owns the lifecycle on the main thread; elsewhere the
        // pending finalisation is left to MPI's own exit handling (never
        // called off the main thread).
        let _ = self.poll();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

    #[derive(Debug, Default)]
    struct MockMpi {
        initialized: AtomicBool,
        finalized: AtomicBool,
        init_calls: AtomicUsize,
        finalize_calls: AtomicUsize,
        max_level: Option<ThreadLevel>,
        /// MPI's main thread (set by `init_thread`, or by the test for an
        /// imported environment).
        main: Mutex<Option<ThreadId>>,
        finalize_thread: Mutex<Option<ThreadId>>,
    }

    impl MpiRuntime for MockMpi {
        fn is_initialized(&self) -> bool {
            self.initialized.load(Ordering::SeqCst)
        }
        fn is_finalized(&self) -> bool {
            self.finalized.load(Ordering::SeqCst)
        }
        fn init_thread(&self, required: ThreadLevel) -> Result<ThreadLevel> {
            self.init_calls.fetch_add(1, Ordering::SeqCst);
            self.initialized.store(true, Ordering::SeqCst);
            *self.main.lock().unwrap() = Some(std::thread::current().id());
            Ok(self.max_level.map_or(required, |m| m.min(required)))
        }
        fn query_thread(&self) -> ThreadLevel {
            self.max_level.unwrap_or(ThreadLevel::Multiple)
        }
        fn is_thread_main(&self) -> bool {
            *self.main.lock().unwrap() == Some(std::thread::current().id())
        }
        fn finalize(&self) -> Result<()> {
            assert!(self.is_thread_main(), "MPI_Finalize off the main thread");
            self.finalize_calls.fetch_add(1, Ordering::SeqCst);
            self.finalized.store(true, Ordering::SeqCst);
            *self.finalize_thread.lock().unwrap() = Some(std::thread::current().id());
            Ok(())
        }
    }

    /// The thread identity of the lifecycle's checks is
    /// stable per thread, distinct between threads, and the policy's main
    /// thread is the initialising one (`on_main_thread` never calls
    /// `std::thread::current()`, so it also answers in a process-exit hook;
    /// that path is exercised by `initialize_mpi forget` under mpirun).
    #[test]
    fn thread_keys_identify_threads_without_thread_ids() {
        let main = ThreadKey::current();
        assert_eq!(main, ThreadKey::current(), "stable on one thread");
        let (with_id, id) = ThreadKey::current_with_id();
        assert_eq!(with_id, main);
        assert_eq!(id, std::thread::current().id());
        assert_eq!(ThreadKey::cached_std_id(), Some(id));
        let (other, other_cached) = std::thread::spawn(|| {
            let k = ThreadKey::current();
            (k, ThreadKey::cached_std_id())
        })
        .join()
        .unwrap();
        assert_ne!(other, main, "distinct threads, distinct keys");
        assert_eq!(other_cached, None, "nothing cached on a fresh thread");

        let l = Lifecycle::new(MockMpi::default());
        let env = l.initialize(ThreadLevel::Funneled).unwrap();
        assert!(env.policy().on_main_thread());
        assert_eq!(env.policy().main_thread(), Some(id));
        let policy = env.policy().clone();
        let (on_main, check) =
            std::thread::spawn(move || (policy.on_main_thread(), policy.check().is_err()))
                .join()
                .unwrap();
        assert!(!on_main && check, "a worker is not MPI's main thread");
        drop(env);
    }

    #[test]
    fn rust_first_owns_and_finalizes_after_last_handle() {
        let l = Lifecycle::new(MockMpi::default());
        let env = l.initialize(ThreadLevel::Funneled).unwrap();
        assert_eq!(env.ownership(), InitOwnership::Owned);
        let comm_guard = env.clone(); // e.g. held by a communicator
        let backend_guard = l.initialize(ThreadLevel::Single).unwrap(); // a backend
        drop(env);
        drop(backend_guard);
        assert_eq!(
            l.runtime().finalize_calls.load(Ordering::SeqCst),
            0,
            "still in use"
        );
        drop(comm_guard);
        assert_eq!(l.runtime().finalize_calls.load(Ordering::SeqCst), 1);
        assert!(l.is_finalized());
        assert!(
            l.initialize(ThreadLevel::Single).is_err(),
            "no MPI_Init after MPI_Finalize"
        );
        assert_eq!(l.runtime().init_calls.load(Ordering::SeqCst), 1);
    }

    /// The last guard dropped on a worker never finalises there;
    /// the main thread completes the pending finalisation.
    #[test]
    fn last_guard_on_a_worker_defers_finalization_to_the_main_thread() {
        for level in [ThreadLevel::Funneled, ThreadLevel::Multiple] {
            let l = Lifecycle::new(MockMpi::default());
            let env = l.initialize(level).unwrap();
            std::thread::scope(|sc| {
                sc.spawn(move || drop(env)).join().unwrap();
            });
            assert_eq!(l.runtime().finalize_calls.load(Ordering::SeqCst), 0);
            assert!(l.is_finalize_pending());
            assert!(l.initialize(level).is_err(), "no new guard once pending");
            std::thread::scope(|sc| {
                assert!(
                    sc.spawn(|| l.shutdown()).join().unwrap().is_err(),
                    "shutdown refused off the main thread"
                );
            });
            assert!(l.poll().unwrap(), "main thread finalises");
            assert_eq!(
                *l.runtime().finalize_thread.lock().unwrap(),
                Some(std::thread::current().id())
            );
            assert!(l.is_finalized());
        }
    }

    /// Native-handle destruction obeys the policy: deferred to the
    /// main thread under FUNNELED, immediate under MULTIPLE; finalisation
    /// runs after every deferred destruction.
    #[test]
    fn handle_destruction_follows_the_thread_policy() {
        use std::sync::atomic::AtomicU32;
        let freed = Arc::new(AtomicU32::new(0));
        let l = Lifecycle::new(MockMpi::default());
        let env = l.initialize(ThreadLevel::Funneled).unwrap();
        std::thread::scope(|sc| {
            let f = Arc::clone(&freed);
            let lr = &l;
            sc.spawn(move || {
                lr.release_handle(move || {
                    f.fetch_add(1, Ordering::SeqCst);
                });
                assert!(lr.poll().is_err(), "a worker cannot run deferred MPI calls");
                drop(env); // last guard, on the worker
            })
            .join()
            .unwrap();
        });
        assert_eq!((freed.load(Ordering::SeqCst), l.deferred_len()), (0, 1));
        assert!(l.shutdown().unwrap());
        assert_eq!(freed.load(Ordering::SeqCst), 1, "freed before MPI_Finalize");
        assert_eq!(l.runtime().finalize_calls.load(Ordering::SeqCst), 1);

        let l = Lifecycle::new(MockMpi::default());
        let env = l.initialize(ThreadLevel::Multiple).unwrap();
        let f = Arc::clone(&freed);
        std::thread::scope(|sc| {
            sc.spawn(|| {
                l.release_handle(move || {
                    f.fetch_add(1, Ordering::SeqCst);
                })
            })
            .join()
            .unwrap();
        });
        assert_eq!((freed.load(Ordering::SeqCst), l.deferred_len()), (2, 0));
        drop(env); // main thread: finalises at once
        assert!(l.is_finalized());
    }

    /// Barrier whose waits time out (a regression panics instead of
    /// hanging the test binary).
    struct TimedBarrier {
        n: usize,
        state: Mutex<(usize, u64)>,
        cv: std::sync::Condvar,
    }

    impl TimedBarrier {
        fn new(n: usize) -> Arc<Self> {
            Arc::new(Self {
                n,
                state: Mutex::new((0, 0)),
                cv: std::sync::Condvar::new(),
            })
        }
        fn wait(&self) {
            let mut g = self.state.lock().unwrap();
            let generation = g.1;
            g.0 += 1;
            if g.0 == self.n {
                g.0 = 0;
                g.1 += 1;
                self.cv.notify_all();
                return;
            }
            let (g, t) = self
                .cv
                .wait_timeout_while(g, std::time::Duration::from_secs(5), |s| s.1 == generation)
                .unwrap();
            assert!(!t.timed_out() || g.1 != generation, "barrier timed out");
        }
    }

    type Events = Arc<Mutex<Vec<&'static str>>>;

    /// Mock whose finalisation is logged next to the destructions.
    #[derive(Debug, Default)]
    struct LoggingMpi {
        inner: MockMpi,
        log: Events,
    }

    impl MpiRuntime for LoggingMpi {
        fn is_initialized(&self) -> bool {
            self.inner.is_initialized()
        }
        fn is_finalized(&self) -> bool {
            self.inner.is_finalized()
        }
        fn init_thread(&self, r: ThreadLevel) -> Result<ThreadLevel> {
            self.inner.init_thread(r)
        }
        fn query_thread(&self) -> ThreadLevel {
            self.inner.query_thread()
        }
        fn is_thread_main(&self) -> bool {
            self.inner.is_thread_main()
        }
        fn finalize(&self) -> Result<()> {
            self.log.lock().unwrap().push("finalize");
            self.inner.finalize()
        }
    }

    fn logging() -> (Lifecycle<LoggingMpi>, Events) {
        let log: Events = Arc::default();
        let l = Lifecycle::new(LoggingMpi {
            inner: MockMpi::default(),
            log: Arc::clone(&log),
        });
        (l, log)
    }

    /// Destruction order, deterministic FUNNELED reproduction: main runs
    /// the queued D1 (taken in its batch) while worker B — holding the last
    /// guard — queues D2 and drops its guard between the two barriers. The
    /// main thread must run D2 before finalising (previously: `["finalize"]`
    /// with D2 left in the queue forever).
    #[test]
    fn finalisation_never_overtakes_a_destruction_queued_during_a_drain_funneled() {
        let (l, log) = logging();
        let env = l.initialize(ThreadLevel::Funneled).unwrap();
        let (b1, b2) = (TimedBarrier::new(2), TimedBarrier::new(2));
        let (ga, gb) = (env.clone(), env.clone());
        drop(env);
        std::thread::scope(|sc| {
            // Worker A: queue D1 (deferred under FUNNELED), drop its guard.
            let (lr, lg, x1, x2) = (&l, Arc::clone(&log), Arc::clone(&b1), Arc::clone(&b2));
            sc.spawn(move || {
                lr.release_handle(move || {
                    x1.wait();
                    x2.wait();
                    lg.lock().unwrap().push("destroy1");
                });
                drop(ga);
            })
            .join()
            .unwrap();
            assert_eq!(l.deferred_len(), 1);
            // Worker B: meets B1 while main runs D1, queues D2, drops the
            // last guard (finalisation pending), meets B2.
            let (lr, lg, x1, x2) = (&l, Arc::clone(&log), Arc::clone(&b1), Arc::clone(&b2));
            let worker_b = sc.spawn(move || {
                x1.wait();
                lr.release_handle(move || lg.lock().unwrap().push("destroy2"));
                drop(gb);
                assert!(lr.is_finalize_pending());
                x2.wait();
            });
            // Main: poll takes D1 and runs it (blocking at the barriers).
            assert!(l.poll().unwrap(), "main finalises after both");
            worker_b.join().unwrap();
        });
        assert_eq!(*log.lock().unwrap(), ["destroy1", "destroy2", "finalize"]);
        assert_eq!(l.deferred_len(), 0);
        assert!(l.is_finalized());
    }

    /// Destruction order under SERIALIZED, main draining: main releases D1 itself and
    /// runs it (holding the serialisation lock) while worker B queues D2
    /// between the barriers. B must not block on the serialisation lock
    /// (previously a dead-lock: B ran D2 at once under that lock while main
    /// waited for B at the barrier); main's drain loop runs D2; the last
    /// guard (main's) then finalises after both.
    #[test]
    fn destruction_queued_during_a_main_drain_runs_first_serialized() {
        let (l, log) = logging();
        let env = l.initialize(ThreadLevel::Serialized).unwrap();
        let (b1, b2) = (TimedBarrier::new(2), TimedBarrier::new(2));
        let gb = env.clone();
        std::thread::scope(|sc| {
            let (lr, lg, x1, x2) = (&l, Arc::clone(&log), Arc::clone(&b1), Arc::clone(&b2));
            let worker_b = sc.spawn(move || {
                x1.wait();
                lr.release_handle(move || lg.lock().unwrap().push("destroy2"));
                drop(gb);
                x2.wait();
            });
            let (lg, x1, x2) = (Arc::clone(&log), Arc::clone(&b1), Arc::clone(&b2));
            l.release_handle(move || {
                x1.wait();
                x2.wait();
                lg.lock().unwrap().push("destroy1");
            });
            worker_b.join().unwrap();
        });
        assert_eq!(*log.lock().unwrap(), ["destroy1", "destroy2"]);
        drop(env); // last guard, on the main thread
        assert_eq!(*log.lock().unwrap(), ["destroy1", "destroy2", "finalize"]);
    }

    /// Destruction order under SERIALIZED, worker draining while the last guard drops
    /// on another worker: worker A (which released its guard first — the
    /// field-order hazard of a handle outliving its guard) drains D1; worker
    /// B queues D2 and drops the last guard (finalisation pending); the
    /// main thread's poll waits for the drainer and finalises only after
    /// D1 and D2.
    #[test]
    fn main_waits_for_a_worker_drain_before_finalising_serialized() {
        let (l, log) = logging();
        let env = l.initialize(ThreadLevel::Serialized).unwrap();
        let (b1, b2) = (TimedBarrier::new(2), TimedBarrier::new(2));
        let (ga, gb) = (env.clone(), env.clone());
        drop(env);
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::scope(|sc| {
            let (lr, lg, x1, x2) = (&l, Arc::clone(&log), Arc::clone(&b1), Arc::clone(&b2));
            sc.spawn(move || {
                drop(ga);
                lr.release_handle(move || {
                    x1.wait();
                    x2.wait();
                    lg.lock().unwrap().push("destroy1");
                });
            });
            let (lr, lg, x1, x2) = (&l, Arc::clone(&log), Arc::clone(&b1), Arc::clone(&b2));
            sc.spawn(move || {
                x1.wait();
                lr.release_handle(move || lg.lock().unwrap().push("destroy2"));
                drop(gb);
                tx.send(()).unwrap();
                x2.wait();
            });
            rx.recv().unwrap();
            assert!(l.is_finalize_pending());
            assert!(l.poll().unwrap());
        });
        assert_eq!(*log.lock().unwrap(), ["destroy1", "destroy2", "finalize"]);
    }

    /// A destruction releasing a nested handle (PETSc object owning
    /// a communicator) neither re-enters the SERIALIZED lock nor is lost:
    /// the nested destruction is queued and run by the same drain, before
    /// finalisation, under FUNNELED and SERIALIZED.
    #[test]
    fn nested_releases_inside_a_destruction_are_drained_before_finalisation() {
        for level in [ThreadLevel::Funneled, ThreadLevel::Serialized] {
            let (l, log) = logging();
            let l = Arc::new(l);
            let env = l.initialize(level).unwrap();
            let (lr, lg) = (Arc::clone(&l), Arc::clone(&log));
            l.release_handle(move || {
                let lg2 = Arc::clone(&lg);
                lr.release_handle(move || lg2.lock().unwrap().push("nested"));
                lg.lock().unwrap().push("outer");
            });
            drop(env);
            assert_eq!(
                *log.lock().unwrap(),
                ["outer", "nested", "finalize"],
                "{level:?}"
            );
        }
    }

    /// A destruction closure owning the **last** guard (keeping MPI alive
    /// until the native object is freed) drops it while running on the
    /// main thread: no self-wait on the running drain, and MPI is
    /// finalised right after the destruction, under FUNNELED and
    /// SERIALIZED.
    #[test]
    fn destruction_owning_the_last_guard_finalises_after_it() {
        for level in [ThreadLevel::Funneled, ThreadLevel::Serialized] {
            let (l, log) = logging();
            // Destruction closures are 'static: give the guard a 'static
            // lifecycle (leaked; test only).
            let l: &'static Lifecycle<LoggingMpi> = Box::leak(Box::new(l));
            let guard = l.initialize(level).unwrap();
            let lg = Arc::clone(&log);
            l.release_handle(move || {
                lg.lock().unwrap().push("destroy");
                drop(guard);
            });
            assert_eq!(*log.lock().unwrap(), ["destroy", "finalize"], "{level:?}");
            assert!(l.is_finalized());
        }
    }

    /// An mpi4py-first program entering akantu from a worker thread
    /// under FUNNELED is rejected (MPI's main thread is asked, not assumed).
    #[test]
    fn imported_funneled_requires_mpis_main_thread() {
        let mock = MockMpi {
            max_level: Some(ThreadLevel::Funneled),
            ..Default::default()
        };
        mock.initialized.store(true, Ordering::SeqCst);
        *mock.main.lock().unwrap() = Some(std::thread::current().id());
        let l = Lifecycle::new(mock);
        std::thread::scope(|sc| {
            let r = sc
                .spawn(|| l.initialize(ThreadLevel::Funneled).map(|_| ()))
                .join()
                .unwrap();
            assert!(r.unwrap_err().to_string().contains("main thread"));
        });
        let env = l.initialize(ThreadLevel::Funneled).unwrap();
        assert!(env.policy().on_main_thread());
        // Imported MULTIPLE entered from a worker: allowed, identity unknown.
        let mock = MockMpi::default();
        mock.initialized.store(true, Ordering::SeqCst);
        *mock.main.lock().unwrap() = Some(std::thread::current().id());
        let l2 = Lifecycle::new(mock);
        std::thread::scope(|sc| {
            sc.spawn(|| {
                let env = l2.initialize(ThreadLevel::Multiple).unwrap();
                assert_eq!(env.policy().main_thread(), None);
                assert!(env.policy().check().is_ok());
            })
            .join()
            .unwrap();
        });
        assert_eq!(l2.runtime().finalize_calls.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn mpi4py_first_is_imported_and_never_finalized() {
        let mock = MockMpi::default();
        mock.initialized.store(true, Ordering::SeqCst); // `import mpi4py`
        *mock.main.lock().unwrap() = Some(std::thread::current().id());
        let l = Lifecycle::new(mock);
        let env = l.initialize(ThreadLevel::Serialized).unwrap();
        assert_eq!(env.ownership(), InitOwnership::Imported);
        drop(env);
        assert_eq!(l.runtime().finalize_calls.load(Ordering::SeqCst), 0);
        // Repeated library setup in the same process is fine.
        drop(l.initialize(ThreadLevel::Single).unwrap());
        assert_eq!(l.runtime().init_calls.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn thread_level_is_negotiated_and_enforced() {
        let l = Lifecycle::new(MockMpi {
            max_level: Some(ThreadLevel::Funneled),
            ..Default::default()
        });
        assert!(
            l.initialize(ThreadLevel::Multiple).is_err(),
            "provided < required"
        );
        let env = l.initialize(ThreadLevel::Funneled).unwrap();
        assert!(env.policy().check().is_ok());
        let policy = env.policy().clone();
        let from_other_thread = std::thread::spawn(move || policy.check()).join().unwrap();
        assert!(
            from_other_thread.is_err(),
            "FUNNELED: only the initialising thread may call MPI"
        );
        assert_eq!(env.policy().call(|| 5).unwrap(), 5);
    }

    /// Under `Serialized` the lock is re-entrant on the holding
    /// thread (a native region's callbacks communicate through akantu on
    /// the same thread) and still excludes other threads.
    #[test]
    fn serialized_lock_is_reentrant_per_thread_and_exclusive_across_threads() {
        let l = Lifecycle::new(MockMpi::default());
        let env = l.initialize(ThreadLevel::Serialized).unwrap();
        let policy = env.policy().clone();
        let inner = policy
            .call(|| policy.call(|| policy.call(|| 7)).unwrap().unwrap())
            .unwrap();
        assert_eq!(
            inner, 7,
            "nested calls on the holding thread do not deadlock"
        );
        // Another thread waits while this thread holds the lock.
        let (tx, rx) = std::sync::mpsc::channel();
        policy
            .call(|| {
                let p2 = policy.clone();
                let h = std::thread::spawn(move || {
                    p2.call(|| tx.send(()).unwrap()).unwrap();
                });
                assert!(
                    rx.recv_timeout(std::time::Duration::from_millis(100))
                        .is_err(),
                    "another thread entered while the lock was held"
                );
                h
            })
            .unwrap()
            .join()
            .unwrap();
        assert!(
            rx.recv().is_ok(),
            "the other thread ran once the lock was released"
        );
        // The held marker ends with the outermost call: a later call from
        // another thread is not starved.
        let p4 = policy.clone();
        let other = std::thread::spawn(move || p4.call(|| 1).unwrap());
        assert_eq!(other.join().unwrap(), 1);
    }

    // ---------------------------------------------- D-067

    /// A mock whose `MPI_Finalized` can also be set from inside a
    /// destruction (a host finalising MPI while akantu handles are alive).
    #[derive(Debug, Default)]
    struct HostMpi {
        inner: MockMpi,
        host_finalized: Arc<AtomicBool>,
    }

    impl MpiRuntime for HostMpi {
        fn is_initialized(&self) -> bool {
            self.inner.is_initialized()
        }
        fn is_finalized(&self) -> bool {
            self.inner.is_finalized() || self.host_finalized.load(Ordering::SeqCst)
        }
        fn init_thread(&self, r: ThreadLevel) -> Result<ThreadLevel> {
            self.inner.init_thread(r)
        }
        fn query_thread(&self) -> ThreadLevel {
            self.inner.query_thread()
        }
        fn is_thread_main(&self) -> bool {
            self.inner.is_thread_main()
        }
        fn finalize(&self) -> Result<()> {
            self.inner.finalize()
        }
    }

    /// An environment imported from a host at `FUNNELED` (MPI initialised,
    /// the calling thread is MPI's main thread).
    fn host_first() -> Lifecycle<HostMpi> {
        let mut mock = HostMpi::default();
        mock.inner.max_level = Some(ThreadLevel::Funneled);
        mock.inner.initialized.store(true, Ordering::SeqCst);
        *mock.inner.main.lock().unwrap() = Some(std::thread::current().id());
        Lifecycle::new(mock)
    }

    /// D-067 (1): the host finalises the MPI it
    /// initialised while akantu still holds handles; a destruction queued
    /// before (by a worker, deferred under FUNNELED) and one released after
    /// are dropped without running — no `MPI_Comm_free` after
    /// `MPI_Finalize` — and akantu never finalises the imported MPI.
    #[test]
    fn destructions_after_the_host_finalised_are_dropped_unrun() {
        let l = host_first();
        let env = l.initialize(ThreadLevel::Funneled).unwrap();
        assert_eq!(env.ownership(), InitOwnership::Imported);
        let ran = Arc::new(AtomicUsize::new(0));
        std::thread::scope(|sc| {
            let (lr, r) = (&l, Arc::clone(&ran));
            sc.spawn(move || {
                lr.release_handle(move || {
                    r.fetch_add(1, Ordering::SeqCst);
                });
            })
            .join()
            .unwrap();
        });
        assert_eq!(l.deferred_len(), 1);
        l.runtime().host_finalized.store(true, Ordering::SeqCst);
        let r = Arc::clone(&ran);
        l.release_handle(move || {
            r.fetch_add(1, Ordering::SeqCst);
        });
        assert!(!l.poll().unwrap());
        assert_eq!(
            ran.load(Ordering::SeqCst),
            0,
            "an MPI call after MPI_Finalize"
        );
        assert_eq!(l.deferred_len(), 0);
        assert_eq!(l.dropped_after_finalize(), 2);
        drop(env);
        assert!(!l.shutdown().unwrap());
        assert_eq!(l.runtime().inner.finalize_calls.load(Ordering::SeqCst), 0);
    }

    /// After a first import the lifecycle is `Active`;
    /// when the host then finalises MPI, a second import (`from_mpi`,
    /// `mpi_world`, `world_size`, …) is refused by `initialize` before any
    /// MPI call (before: a guard was handed out and the import called
    /// `MPI_Comm_test_inter` after `MPI_Finalize`). The guard count is
    /// unchanged, and the first environment's release still drops its
    /// destructions unrun.
    #[test]
    fn a_new_import_after_the_host_finalised_is_refused() {
        let l = host_first();
        let env = l.initialize(ThreadLevel::Funneled).unwrap();
        assert_eq!(env.ownership(), InitOwnership::Imported);
        // a second import while MPI is alive is fine
        drop(l.initialize(ThreadLevel::Funneled).unwrap());
        l.runtime().host_finalized.store(true, Ordering::SeqCst);
        let e = l
            .initialize(ThreadLevel::Funneled)
            .expect_err("an import after MPI_Finalize");
        assert!(matches!(e, Error::Communication(_)), "{e:?}");
        assert!(format!("{e}").contains("finalised by the host"), "{e}");
        let ran = Arc::new(AtomicUsize::new(0));
        let r = Arc::clone(&ran);
        l.release_handle(move || {
            r.fetch_add(1, Ordering::SeqCst);
        });
        assert_eq!(
            ran.load(Ordering::SeqCst),
            0,
            "an MPI call after MPI_Finalize"
        );
        drop(env);
        assert!(!l.shutdown().unwrap());
        assert_eq!(l.runtime().inner.finalize_calls.load(Ordering::SeqCst), 0);
    }

    /// D-067 (1): a drain checks `MPI_Finalized` before each destruction —
    /// the host finalises MPI during the first one, the second is dropped.
    #[test]
    fn a_drain_stops_once_mpi_is_finalised() {
        let l = host_first();
        let env = l.initialize(ThreadLevel::Funneled).unwrap();
        let log: Events = Arc::default();
        let host = Arc::clone(&l.runtime().host_finalized);
        std::thread::scope(|sc| {
            let (lr, lg1, lg2) = (&l, Arc::clone(&log), Arc::clone(&log));
            sc.spawn(move || {
                lr.release_handle(move || {
                    lg1.lock().unwrap().push("destroy1");
                    host.store(true, Ordering::SeqCst);
                });
                lr.release_handle(move || lg2.lock().unwrap().push("destroy2"));
            })
            .join()
            .unwrap();
        });
        assert_eq!(l.deferred_len(), 2);
        assert!(!l.poll().unwrap());
        assert_eq!(*log.lock().unwrap(), ["destroy1"]);
        assert_eq!(l.dropped_after_finalize(), 1);
        drop(env);
    }

    /// D-067 (1): an owned environment finalised outside the lifecycle
    /// (a direct `MPI_Finalize`) while its finalisation is pending: the
    /// main thread completes the transition without a second
    /// `MPI_Finalize`.
    #[test]
    fn a_pending_finalisation_after_an_external_finalize_skips_mpi_finalize() {
        let l = Lifecycle::new(HostMpi::default());
        let env = l.initialize(ThreadLevel::Funneled).unwrap();
        assert_eq!(env.ownership(), InitOwnership::Owned);
        std::thread::scope(|sc| {
            sc.spawn(move || drop(env)).join().unwrap();
        });
        assert!(l.is_finalize_pending());
        l.runtime().host_finalized.store(true, Ordering::SeqCst);
        assert!(!l.poll().unwrap());
        assert!(l.is_finalized());
        assert_eq!(l.runtime().inner.finalize_calls.load(Ordering::SeqCst), 0);
    }

    /// D-067 (3): in exit mode a worker holding the
    /// `Serialized` MPI lock in a blocking call (with a destruction queued)
    /// does not block the exit path: it gives up, runs neither the
    /// destruction nor `MPI_Finalize`, and names the lock.
    #[test]
    fn exit_mode_gives_up_on_a_held_serialized_lock() {
        let (l, log) = logging();
        let env = l.initialize(ThreadLevel::Serialized).unwrap();
        let policy = env.policy().clone();
        let (locked_tx, locked_rx) = std::sync::mpsc::channel();
        let (free_tx, free_rx) = std::sync::mpsc::channel::<()>();
        std::thread::scope(|sc| {
            let worker = sc.spawn(move || {
                policy
                    .call(|| {
                        locked_tx.send(()).unwrap();
                        let _ = free_rx.recv_timeout(std::time::Duration::from_secs(20));
                    })
                    .unwrap();
            });
            locked_rx.recv().unwrap();
            l.begin_exit();
            let t0 = std::time::Instant::now();
            let lg = Arc::clone(&log);
            l.release_handle(move || lg.lock().unwrap().push("destroy"));
            drop(env);
            let shutdown = l.shutdown();
            let elapsed = t0.elapsed();
            free_tx.send(()).unwrap();
            worker.join().unwrap();
            assert!(
                elapsed < std::time::Duration::from_secs(5),
                "blocked {elapsed:?}"
            );
            assert!(shutdown.is_err());
        });
        assert_eq!(l.exit_blocker(), Some(ExitBlocker::SerialLock));
        assert!(log.lock().unwrap().is_empty(), "{:?}", log.lock().unwrap());
        assert_eq!(l.runtime().inner.finalize_calls.load(Ordering::SeqCst), 0);
    }

    /// D-067 (3): in exit mode the main thread does not wait for another
    /// thread's drain (the `drained` condvar) before finalising.
    #[test]
    fn exit_mode_does_not_wait_for_another_drainer() {
        let (l, log) = logging();
        let env = l.initialize(ThreadLevel::Multiple).unwrap();
        let (in_tx, in_rx) = std::sync::mpsc::channel();
        let (out_tx, out_rx) = std::sync::mpsc::channel::<()>();
        std::thread::scope(|sc| {
            let (lr, lg) = (&l, Arc::clone(&log));
            let worker = sc.spawn(move || {
                lr.release_handle(move || {
                    in_tx.send(()).unwrap();
                    let _ = out_rx.recv_timeout(std::time::Duration::from_secs(20));
                    lg.lock().unwrap().push("destroy");
                });
            });
            in_rx.recv().unwrap();
            l.begin_exit();
            let t0 = std::time::Instant::now();
            drop(env);
            let shutdown = l.shutdown();
            let elapsed = t0.elapsed();
            out_tx.send(()).unwrap();
            worker.join().unwrap();
            assert!(
                elapsed < std::time::Duration::from_secs(5),
                "blocked {elapsed:?}"
            );
            assert!(shutdown.is_err());
        });
        assert_eq!(l.exit_blocker(), Some(ExitBlocker::Drainer));
        assert_eq!(*log.lock().unwrap(), ["destroy"], "no MPI_Finalize");
        assert_eq!(l.runtime().inner.finalize_calls.load(Ordering::SeqCst), 0);
    }

    /// D-067 (3): in exit mode a lifecycle lock held by another thread is
    /// not waited for.
    #[test]
    fn exit_mode_does_not_wait_for_the_lifecycle_lock() {
        let (l, log) = logging();
        let env = l.initialize(ThreadLevel::Funneled).unwrap();
        let (held_tx, held_rx) = std::sync::mpsc::channel();
        let (free_tx, free_rx) = std::sync::mpsc::channel::<()>();
        std::thread::scope(|sc| {
            let lr = &l;
            let holder = sc.spawn(move || {
                let _g = lr.inner.lock().unwrap();
                held_tx.send(()).unwrap();
                let _ = free_rx.recv_timeout(std::time::Duration::from_secs(20));
            });
            held_rx.recv().unwrap();
            l.begin_exit();
            let t0 = std::time::Instant::now();
            let lg = Arc::clone(&log);
            l.release_handle(move || lg.lock().unwrap().push("destroy"));
            drop(env);
            let shutdown = l.shutdown();
            let elapsed = t0.elapsed();
            free_tx.send(()).unwrap();
            holder.join().unwrap();
            assert!(
                elapsed < std::time::Duration::from_secs(5),
                "blocked {elapsed:?}"
            );
            assert!(shutdown.is_err());
        });
        assert_eq!(l.exit_blocker(), Some(ExitBlocker::LifecycleLock));
        assert!(log.lock().unwrap().is_empty());
        assert_eq!(l.runtime().inner.finalize_calls.load(Ordering::SeqCst), 0);
    }

    /// Exit mode on a free path still finalises (the hook's normal case).
    #[test]
    fn exit_mode_finalises_when_nothing_is_held() {
        let (l, log) = logging();
        let env = l.initialize(ThreadLevel::Serialized).unwrap();
        l.begin_exit();
        let lg = Arc::clone(&log);
        l.release_handle(move || lg.lock().unwrap().push("destroy"));
        drop(env);
        assert!(!l.shutdown().unwrap(), "finalised by the last guard's drop");
        assert_eq!(*log.lock().unwrap(), ["destroy", "finalize"]);
        assert_eq!(l.exit_blocker(), None);
        assert!(l.is_finalized());
    }
}
