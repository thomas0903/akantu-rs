// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/model/common/dof_manager/
// dof_manager_petsc.cc:52-125 (PETSc_call, petscErrorHandler,
// PETScSingleton: PetscInitialize with the program arguments and
// `-no_signal_handler`, error handler replaced, PetscFinalize unless MPI is
// already finalised), dof_manager_petsc.cc:148-150 (static_assert on the
// sizes of PetscInt / PetscReal); the global options database written by
// upstream's tests (test/test_model/test_phase_field_model/
// test_phase_field_tao.cc:51-52, `PetscOptionsSetValue(NULL, ...)`).

//! PETSc process state: the re-entrant PETSc lock, error capture and the
//! initialisation/finalisation session.

use std::ffi::{CString, c_char, c_int};
use std::marker::PhantomData;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError, Weak};
use std::thread::ThreadId;

use akantu_comm::{Communicator, NativeComm, ReduceOp};
use akantu_core::{Error, FatalKind, Result};

use super::ffi;

// ------------------------------------------------------------------ lock

struct LockState {
    owner: Option<ThreadId>,
    depth: usize,
}

/// The process-wide re-entrant PETSc lock (the Debian PETSc build is not
/// thread-safe).
///
/// # Lock order
///
/// Outermost first: (1) the process `Serialized` MPI lock
/// (`ThreadPolicy::call`, re-entrant per thread), (2) this PETSc lock
/// (re-entrant per thread), (3) leaf mutexes (the lock's own state, the
/// session/library state, the shim's error buffer, the MPI lifecycle and
/// admission registries) that are never held across a wait for (1) or (2).
/// A thread waits for a lock only while it holds nothing but locks above
/// it, so no cycle can form:
///
/// * the PETSc lock is **waited for** only (a) inside an agreed native entry
///   — [`PetscLock::enter_native`] needs the entry's [`NativeComm`]
///   capability, i.e. runs inside `Communicator::native_collective` /
///   `native_region`, holding (1) under `Serialized` — or (b) inside a
///   destruction run by the MPI lifecycle (`Communicator::release_native`:
///   `PetscFinalize`, `*Destroy`), also under (1);
/// * a thread holding the PETSc lock never waits for (1) except
///   re-entrantly: it performs akantu communication only inside a native
///   region whose thread already holds (1), and the backend takes the PETSc
///   lock inside its native closures, after the agreed admission, never
///   across an agreement of its own (inside a native region the callbacks'
///   agreements do run under the region's PETSc lock: re-entrant on that
///   thread, and other threads' native entries are refused by the
///   admission registry, so only destructions wait for it);
/// * outside native entries the lock is only **tried**
///   ([`PetscLock::try_enter`]: local PETSc calls such as diagnostics or
///   the session's initialisation query), never waited for, and nothing is
///   acquired while holding it.
///
/// Under `Funneled` native entries and destructions run on MPI's main
/// thread only; under `Multiple` native entries refuse each other
/// process-wide.
///
/// **Scope: this order excludes cycles between this process's locks, i.e.
/// for native entries and for destructions that do not wait for other
/// ranks.** It does not make every destruction safe. PETSc destructions are
/// collective in PETSc's contract, and some wait for their peers: a KSP
/// (or SNES / TAO) whose preconditioner factorises with an external
/// package (`pc_factor_mat_solver_type mumps`, the direct configuration at
/// more than one rank; SuperLU_DIST likewise) runs that package's
/// collective job inside `KSPDestroy` → `PCDestroy` → `MatDestroy`
/// (MUMPS `JOB = -2`). Such a destruction released on a worker thread
/// while the main thread is inside a solve can hang under `Serialized` and
/// `Multiple`: the worker holds the PETSc lock (and the MPI lock under
/// `Serialized`) waiting for a peer whose own worker waits for the lock
/// held by that peer's solving main thread (reproduced).
/// PETSc objects therefore follow D-054 exactly like MUMPS's: drop them
/// on MPI's main thread, at the same program point on every rank, with no
/// other thread inside MPI or PETSc; the SPMD destruction order is the
/// caller's obligation (`Communicator::release_native`). The
/// `petsc_mpi_cases::pe02_*` probes cover local destructions only.
#[derive(Debug, Clone, Copy)]
pub struct PetscLock;

static STATE: Mutex<LockState> = Mutex::new(LockState {
    owner: None,
    depth: 0,
});
static RELEASED: Condvar = Condvar::new();

fn state() -> MutexGuard<'static, LockState> {
    STATE.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Holding the PETSc lock on the current thread (`!Send`).
#[derive(Debug)]
pub struct PetscGuard {
    _thread: PhantomData<*const ()>,
}

impl PetscLock {
    /// Take the lock if it is free or already held by this thread; an error
    /// (a local refusal, to be agreed by the caller's native entry) if
    /// another thread holds it.
    pub fn try_enter() -> Result<PetscGuard> {
        let me = std::thread::current().id();
        let mut s = state();
        match s.owner {
            Some(o) if o != me => Err(Error::Communication(
                "PETSc is in use by another thread of this process (the PETSc build is not \
                 thread-safe: use PETSc from one thread at a time)"
                    .into(),
            )),
            _ => {
                s.owner = Some(me);
                s.depth += 1;
                Ok(PetscGuard {
                    _thread: PhantomData,
                })
            }
        }
    }

    /// Take the lock inside an agreed native entry (its capability `nc`),
    /// waiting for another thread to release it: every rank is already
    /// inside the entry, so a refusal would make the ranks diverge inside
    /// the library. Holders that can delay it are leaf calls (which acquire
    /// nothing while holding the lock) and, under `Multiple`, one
    /// destruction (see the lock order above).
    #[must_use]
    pub fn enter_native(nc: &NativeComm<'_>) -> PetscGuard {
        let _ = nc;
        Self::enter_waiting()
    }

    /// Take the lock, waiting for another thread to release it (native
    /// entries and destructions only: they cannot be refused).
    pub(crate) fn enter_waiting() -> PetscGuard {
        let me = std::thread::current().id();
        let mut s = state();
        while matches!(s.owner, Some(o) if o != me) {
            s = RELEASED.wait(s).unwrap_or_else(PoisonError::into_inner);
        }
        s.owner = Some(me);
        s.depth += 1;
        PetscGuard {
            _thread: PhantomData,
        }
    }
}

impl Drop for PetscGuard {
    fn drop(&mut self) {
        let mut s = state();
        s.depth = s.depth.saturating_sub(1);
        if s.depth == 0 {
            s.owner = None;
            drop(s);
            RELEASED.notify_all();
        }
    }
}

// ---------------------------------------------------------------- errors

fn read_c_buffer(buf: &[c_char]) -> String {
    let bytes: Vec<u8> = buf
        .iter()
        .take_while(|&&c| c != 0)
        .map(|&c| u8::from_ne_bytes(c.to_ne_bytes()))
        .collect();
    String::from_utf8_lossy(&bytes).into_owned()
}

/// The message the shim's error handler recorded for `code` (the buffer is
/// cleared either way), or PETSc's generic description of `code`: a message
/// recorded for another code, left behind by an earlier failure that was
/// not read (the fallback below), is dropped instead of being reported for
/// a failure that did not pass PETSc's error handler,
/// unless the codes coincide: a stale message of code X is still returned
/// for a later failure with code X that did not pass the handler
/// (the shim compares codes only).
/// The shim's error buffer is a static that
/// PETSc's error handler writes during any PETSc call, so it is read and
/// reset only under the PETSc lock: taken here with
/// `try_enter`, which succeeds at once for the callers of `check` that
/// already hold it (re-entrant) and, for the two that run after their
/// native entry returned (`set_global_option`, `Session::acquire`), unless
/// another thread is inside PETSc; then the recorded message is left alone
/// and PETSc's generic description is returned (the lock is never waited
/// for outside a native entry, see the lock order above).
fn take_message(code: c_int) -> String {
    if let Ok(_g) = PetscLock::try_enter() {
        let mut buf = [0 as c_char; 4096];
        // SAFETY: `buf` provides `buf.len()` writable characters; the shim
        // writes at most `len - 1` plus a terminator. The PETSc lock is held
        // (`_g`), so no PETSc call of another thread writes the shim's
        // static error buffer meanwhile (every PETSc call holds the lock).
        let n = unsafe { ffi::akantu_petsc_last_error(code, buf.as_mut_ptr(), buf.len()) };
        // SAFETY: no pointer is passed; resets the shim's static buffer,
        // under the PETSc lock held by `_g`.
        unsafe { ffi::akantu_petsc_clear_error() };
        if n > 0 {
            return read_c_buffer(&buf);
        }
    }
    let mut desc = [0 as c_char; 512];
    // SAFETY: `desc` has `desc.len()` writable characters (the shim writes
    // at most `len - 1` plus a terminator); `PetscErrorMessage` only reads
    // PETSc's constant message table, so no lock is needed.
    unsafe { ffi::akantu_petsc_error_message(code, desc.as_mut_ptr(), desc.len()) };
    read_c_buffer(&desc)
}

/// `Ok` for a zero return code, otherwise `Error::Backend` with the
/// recorded PETSc message (upstream `PETSc_call`).
pub(crate) fn check(code: c_int, what: &str) -> Result<()> {
    if code == 0 {
        Ok(())
    } else {
        Err(Error::Backend {
            backend: "petsc",
            code: i64::from(code),
            message: format!("{what}: {}", take_message(code)),
        })
    }
}

/// A PETSc error inside a collective call: the ranks may have diverged
/// inside PETSc, so it is a transport-class failure (`docs/ARCHITECTURE.md`).
pub(crate) fn collective_failure(e: Error) -> Error {
    Error::fatal(FatalKind::Transport, e)
}

/// A C string from Rust text (interior NUL bytes are an argument error).
pub(crate) fn c_string(s: &str, what: &str) -> Result<CString> {
    CString::new(s).map_err(|_| Error::invalid(format!("{what}: '{s}' contains a NUL byte")))
}

// --------------------------------------------------------------- session

/// Arguments given to `PetscInitialize` (upstream: akantu's global
/// argc/argv); `argv[0]` is the program name.
static COMMAND_LINE: Mutex<Vec<String>> = Mutex::new(Vec::new());

/// Arguments passed to `PetscInitialize` at the next initialisation
/// (upstream `initialize(argc, argv)` keeps them for PETSc; the options
/// they contain, e.g. `-ksp_type preonly`, enter PETSc's global database
/// together with `PETSC_OPTIONS`). Without a call PETSc only sees the
/// program name and the environment.
///
/// **Identical on every rank**: the options steer
/// PETSc's collective calls; the next session start agrees a digest of
/// these arguments and refuses a rank-specific command line on every rank
/// (`Error::InvalidArgument`) before `PetscInitialize`. `PETSC_OPTIONS`
/// needs no such care (PETSc broadcasts it from rank 0).
pub fn set_command_line(args: impl IntoIterator<Item = String>) {
    *COMMAND_LINE.lock().unwrap_or_else(PoisonError::into_inner) = args.into_iter().collect();
}

/// Options of PETSc's global database set with [`set_global_option`] before
/// PETSc was initialised: applied, in setting order, right before
/// `PetscInitialize` (upstream's order: its tests call
/// `PetscOptionsSetValue(NULL, …)` before the `PETScSingleton` exists).
static PENDING_GLOBAL: Mutex<Vec<(CString, CString)>> = Mutex::new(Vec::new());

/// Upstream `PetscOptionsSetValue(NULL, "-<name>", value)`
/// (`test_phase_field_tao.cc:51-52`): an option of PETSc's
/// **global** database, which every KSP / SNES / TAO consults after the
/// solver's private database (`*SetFromOptions` on the private database,
/// then on the global one: upstream `ParsablePETSc::updateInternalParameters`,
/// `parsable_petsc.hh:55-67`), so a global value wins over a private one;
/// the native solvers' objects are created at each solve, so it takes effect
/// at the next solve; the KSP only reads options once a private option was
/// set (UD-425). The command line ([`set_command_line`]) and `PETSC_OPTIONS`
/// enter the same database at `PetscInitialize`: an option set here **after**
/// the initialisation replaces their value; one set **before** it is applied
/// before `PetscInitialize` (as upstream), so the command line and
/// `PETSC_OPTIONS` win over it.
///
/// **Collective over `comm`** (outside any collective scope): the options
/// steer PETSc's collective calls, so the ranks agree that `name` / `value`
/// are identical on every rank and that PETSc is initialised on all or none
/// of them (refused on every rank otherwise: `Error::InvalidArgument`,
/// `Error::Unsupported`), then set it inside an agreed native entry
/// (`comm` must be an MPI communicator once PETSc is initialised). Before
/// the initialisation the option is queued; `comm` may then be serial or a
/// subset of the processes, so the queue is agreed again over the
/// communicator of the session start ([`Session::acquire`]): queues that
/// differ between its ranks are refused there on every rank, before
/// `PetscInitialize`, like a differing command line.
///
/// **The value is not validated here** (PETSc stores option strings; nor
/// are a solver's private options): an invalid value surfaces at the next
/// solve that reads it, inside the solve's agreed native collective, as
/// `Error::Fatal { kind: Transport }` — the process's communicators are
/// marked broken and the runtime abort policy applies — where upstream
/// throws an ordinary exception (`docs/ARCHITECTURE.md`).
pub fn set_global_option(comm: &Communicator, name: &str, value: &str) -> Result<()> {
    let strings = c_string(&format!("-{name}"), "PETSc option name")
        .and_then(|n| c_string(value, "PETSc option value").map(|v| (n, v)));
    let mut flag: c_int = 0;
    let local = strings
        .as_ref()
        .map(|_| ())
        .map_err(|e| Error::invalid(e.to_string()))
        .and_then(|()| {
            // A leaf use of the PETSc lock (tried, released before the
            // agreement below), as `Session::acquire`.
            let _g = PetscLock::try_enter().map_err(|e| Error::Communication(e.to_string()))?;
            // SAFETY: one valid `int` out-pointer; PetscInitialized is a local
            // query, run under the PETSc lock.
            check(
                unsafe { ffi::akantu_petsc_is_initialized(&raw mut flag) },
                "PetscInitialized",
            )
        });
    let digest = command_line_digest(&[name.to_owned(), value.to_owned()]);
    let initialized = flag != 0;
    // [any rank failed locally, max(digest), !min(digest), any initialised,
    //  any uninitialised]
    let mut states = [
        u64::from(local.is_err()),
        digest,
        !digest,
        u64::from(local.is_ok() && initialized),
        u64::from(local.is_ok() && !initialized),
    ];
    comm.all_reduce(&mut states, ReduceOp::Max)?;
    local?;
    let (n, v) = strings?;
    if states[0] == 1 {
        return Err(Error::Communication(
            "PETSc global option: the local checks failed on another rank".into(),
        ));
    }
    if states[1] != !states[2] {
        return Err(Error::InvalidArgument(format!(
            "PETSc global option -{name}: the name or value differs between the ranks; options \
             steer PETSc's collective calls and must be identical on every rank"
        )));
    }
    match (states[3] == 1, states[4] == 1) {
        (true, true) => Err(Error::Unsupported(
            "PETSc global option: PETSc is initialised on some ranks of this communicator only"
                .into(),
        )),
        (true, false) => {
            let rc = comm.native_collective("PetscOptionsSetValue", Ok(()), |nc| {
                let _g = PetscLock::enter_native(&nc);
                // SAFETY: a null database selects PETSc's global one (PETSc
                // is initialised on every rank: agreed above); NUL-terminated
                // name and value valid for the call (PETSc copies them); run
                // under the PETSc lock inside the agreed native entry.
                unsafe {
                    ffi::akantu_petsc_options_set(std::ptr::null_mut(), n.as_ptr(), v.as_ptr())
                }
            })?;
            check(rc, "PetscOptionsSetValue").map_err(collective_failure)
        }
        _ => {
            PENDING_GLOBAL
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push((n, v));
            Ok(())
        }
    }
}

/// The C `argv` kept alive for the whole PETSc session (PETSc keeps the
/// pointers given to `PetscInitialize`).
struct Argv {
    _strings: Vec<CString>,
    pointers: Vec<*mut c_char>,
}

// SAFETY: the pointers point into `_strings`, owned by the same value and
// never mutated by Rust; PETSc reads them only under the PETSc lock.
unsafe impl Send for Argv {}
// SAFETY: see `Send`: no interior mutation through `&Argv`.
unsafe impl Sync for Argv {}

/// The arguments `PetscInitialize` gets (`argv[0]` defaults to the
/// program name `akantu-rs`).
fn command_line() -> Vec<String> {
    let mut args = COMMAND_LINE
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone();
    if args.is_empty() {
        args.push("akantu-rs".into());
    }
    args
}

/// Digest of the global options queued by [`set_global_option`] before
/// PETSc starts (agreed over the session's communicator
/// before `PetscInitialize`, like the command line; `set_global_option`
/// agrees them only over the caller's communicator, which may be serial or
/// a subset of the processes).
fn pending_options_digest(pending: &[(CString, CString)]) -> u64 {
    pending.iter().fold(0x8422_2325_cbf2_9ce4, |h, (n, v)| {
        [n, v].iter().fold(h, |h, s| {
            let b = s.as_bytes();
            akantu_comm::phase::digest_bytes(
                akantu_comm::phase::digest_bytes(h, &b.len().to_le_bytes()),
                b,
            )
        })
    })
}

/// Digest of a command line (agreed before
/// `PetscInitialize`, the options it contains steer PETSc's collectives).
fn command_line_digest(args: &[String]) -> u64 {
    args.iter().fold(0xcbf2_9ce4_8422_2325, |h, a| {
        akantu_comm::phase::digest_bytes(
            akantu_comm::phase::digest_bytes(h, &a.len().to_le_bytes()),
            a.as_bytes(),
        )
    })
}

impl Argv {
    fn new(args: &[String]) -> Result<Self> {
        let strings = args
            .iter()
            .map(|a| c_string(a, "PETSc command line"))
            .collect::<Result<Vec<_>>>()?;
        let mut pointers: Vec<*mut c_char> =
            strings.iter().map(|s| s.as_ptr().cast_mut()).collect();
        pointers.push(std::ptr::null_mut());
        Ok(Self {
            _strings: strings,
            pointers,
        })
    }
}

/// The library state owned by akantu (one per process).
struct Library {
    /// akantu initialised PETSc and has not finalised it yet.
    owned: bool,
    /// The communicator PETSc was initialised on (`PETSC_COMM_WORLD`) still
    /// has a live akantu handle.
    world_alive: bool,
    /// The argv PETSc keeps (alive until `PetscFinalize`).
    argv: Option<Arc<Argv>>,
    /// `PETSC_COMM_WORLD`: akantu's own duplicate of the first PETSc user's
    /// communicator, alive until `PetscFinalize`
    /// returned, so PETSc never finalises on a freed MPI handle whatever
    /// the order in which the user drops its communicators.
    world: Option<Communicator>,
}

static LIBRARY: Mutex<Library> = Mutex::new(Library {
    owned: false,
    world_alive: false,
    argv: None,
    world: None,
});

/// Serialises session acquisitions of the process (tried, never waited
/// for: a concurrent acquisition is refused and agreed).
static ACQUIRE: Mutex<()> = Mutex::new(());

fn library() -> MutexGuard<'static, Library> {
    LIBRARY.lock().unwrap_or_else(PoisonError::into_inner)
}

impl std::fmt::Debug for Argv {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Argv")
            .field("argc", &self.pointers.len().saturating_sub(1))
            .finish()
    }
}

/// Take the finalisation out of the library state (it clears `owned`):
/// the returned closure runs `PetscFinalize` (collective over
/// `PETSC_COMM_WORLD`) and then frees the world duplicate. The caller
/// queues it with [`queue_finalize`] **after** releasing the library lock
/// (the lifecycle may run it at once, under the MPI thread policy).
fn take_finalize(lib: &mut Library) -> Option<Box<dyn FnOnce() + Send>> {
    if !lib.owned {
        return None;
    }
    lib.owned = false;
    let argv = lib.argv.take();
    let world = lib.world.take();
    FINALIZE_PENDING.store(true, Ordering::SeqCst);
    Some(Box::new(move || {
        {
            let _g = PetscLock::enter_waiting();
            // SAFETY: PETSc was initialised by akantu (`owned`), every PETSc
            // object was destroyed before (objects hold a session token,
            // whose last drop is one of the two triggers) and
            // PETSC_COMM_WORLD is akantu's duplicate `world`, freed only
            // below, after PetscFinalize returned; runs
            // under the MPI thread policy (lifecycle) and the PETSc lock.
            // The argv kept by PETSc stays alive until it returned.
            let rc = unsafe { ffi::akantu_petsc_finalize() };
            if rc != 0 {
                log::error!("PetscFinalize failed: {}", take_message(rc));
            }
        }
        drop(argv);
        // Its MPI_Comm_free is queued behind this destruction.
        drop(world);
        FINALIZE_PENDING.store(false, Ordering::SeqCst);
    }))
}

/// Queue a finalisation taken with [`take_finalize`] through the MPI
/// lifecycle (before `MPI_Finalize`, SPMD order). Never called with the
/// library lock held.
fn queue_finalize(job: Option<Box<dyn FnOnce() + Send>>) {
    if let Some(job) = job {
        akantu_comm::mpi_lifecycle().release_handle(job);
    }
}

/// A session token: alive while a PETSc object (or backend) of the process
/// exists.
#[derive(Debug)]
struct SessionInner {
    /// akantu initialised PETSc (and finalises it).
    owned: bool,
}

impl Drop for SessionInner {
    /// Upstream finalises PETSc once, at exit (static `PETScSingleton`):
    /// the port finalises it when both the last PETSc user and the world
    /// communicator are gone — never between two uses on a live world
    /// (PETSc 3.22 does not restore TAO's type registry after a
    /// re-initialisation).
    fn drop(&mut self) {
        if !self.owned {
            return;
        }
        let job = {
            let mut lib = library();
            if lib.world_alive {
                None
            } else {
                take_finalize(&mut lib)
            }
        };
        queue_finalize(job);
    }
}

/// Current session of the process (a weak reference: the token ends with
/// its last user).
static CURRENT: Mutex<Option<Weak<SessionInner>>> = Mutex::new(None);

/// A finalisation was queued and has not run yet (deferred to MPI's main
/// thread): a new session must wait for it.
static FINALIZE_PENDING: AtomicBool = AtomicBool::new(false);

/// A user of the PETSc library (upstream `PETScSingleton::getInstance()`):
/// cloned by every PETSc object. PETSc is finalised (if akantu initialised
/// it) once the last clone is dropped **and** the communicator it was
/// initialised on has no akantu handle left.
#[derive(Clone, Debug)]
pub struct Session {
    inner: Arc<SessionInner>,
}

/// PETSc version of the linked library (`major`, `minor`, `subminor`).
#[must_use]
pub fn version() -> (i32, i32, i32) {
    let (mut a, mut b, mut c) = (0, 0, 0);
    // SAFETY: three valid `int` out-pointers; reads compile-time constants.
    unsafe { ffi::akantu_petsc_version(&raw mut a, &raw mut b, &raw mut c) };
    (a, b, c)
}

/// Relation of PETSc's world communicator (`PETSC_COMM_WORLD`) to another
/// communicator (`MPI_Comm_compare`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorldRelation {
    /// PETSc is not initialised.
    NotInitialized,
    /// The same MPI communicator (`MPI_IDENT`).
    Identical,
    /// Same group and rank order, different context (`MPI_CONGRUENT`, e.g.
    /// a duplicate).
    Congruent,
    /// Same processes in another order (`MPI_SIMILAR`).
    Similar,
    /// Different processes (`MPI_UNEQUAL`).
    Unequal,
}

/// Relation of `PETSC_COMM_WORLD` to the communicator of the native entry
/// `nc` (diagnostics and tests). A local MPI call: run it
/// inside an agreed native entry (thread policy held).
pub fn world_relation(nc: &NativeComm<'_>) -> Result<WorldRelation> {
    let _g = PetscLock::enter_native(nc);
    let mut r: c_int = -1;
    // SAFETY: `nc.raw()` is the live communicator of the agreed native entry
    // (thread policy held); `r` is a valid `int` out-pointer; the shim only
    // reads PETSc's initialisation flag and calls the local
    // `MPI_Comm_compare`.
    check(
        unsafe { ffi::akantu_petsc_world_compare(nc.raw(), &raw mut r) },
        "MPI_Comm_compare(PETSC_COMM_WORLD)",
    )?;
    Ok(match r {
        0 => WorldRelation::Identical,
        1 => WorldRelation::Congruent,
        2 => WorldRelation::Similar,
        3 => WorldRelation::Unequal,
        _ => WorldRelation::NotInitialized,
    })
}

/// OpenBLAS kernels in use (D-050): results of the
/// default KSP (ILU / GMRES through BLAS) depend on them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BlasInfo {
    /// `openblas_get_corename()` (`None`: the BLAS is not OpenBLAS).
    pub core: Option<String>,
    /// `openblas_get_num_threads()`.
    pub threads: Option<i32>,
}

/// The [`BlasInfo`] of this process.
#[must_use]
pub fn blas_info() -> BlasInfo {
    // SAFETY: shim functions without preconditions: a dlsym lookup in the
    // global scope and, if found, OpenBLAS's own getters; the core name is
    // a static NUL-terminated string of OpenBLAS (or null), copied out.
    let (core, threads) = unsafe {
        let p = ffi::akantu_petsc_blas_corename();
        let core = if p.is_null() {
            None
        } else {
            Some(std::ffi::CStr::from_ptr(p).to_string_lossy().into_owned())
        };
        (core, ffi::akantu_petsc_blas_threads())
    };
    BlasInfo {
        core,
        threads: (threads >= 0).then_some(threads),
    }
}

/// Local checks of the linked PETSc against the types akantu passes
/// (upstream `static_assert(sizeof(Int) == sizeof(PetscInt))`, same for
/// `Real`).
fn check_types() -> Result<()> {
    // SAFETY: argument-less shim functions returning `sizeof` constants.
    let (int, scalar, real) = unsafe {
        (
            ffi::akantu_petsc_int_size(),
            ffi::akantu_petsc_scalar_size(),
            ffi::akantu_petsc_real_size(),
        )
    };
    let i32_size = c_int::try_from(std::mem::size_of::<ffi::PetscInt>()).unwrap_or(0);
    let f64_size = c_int::try_from(std::mem::size_of::<f64>()).unwrap_or(0);
    if int != i32_size || scalar != f64_size || real != f64_size {
        return Err(Error::Unsupported(format!(
            "PETSc built with sizeof(PetscInt) = {int}, sizeof(PetscScalar) = {scalar}, \
             sizeof(PetscReal) = {real}: akantu-rs needs 32-bit PetscInt and real double \
             scalars (the oracle's Debian build)"
        )));
    }
    Ok(())
}

impl Session {
    /// Join (or start) the PETSc session of this process. **Collective**
    /// over `comm`, called outside any collective scope (backend
    /// construction): the ranks agree on the library state first — all
    /// initialised (joined), none (PETSc initialised collectively, see
    /// below), or mixed (refused with `Error::Unsupported` on every rank) —
    /// then on the outcome.
    ///
    /// Initialisation (upstream `PETScSingleton`): the ranks first agree
    /// that the command line given to [`set_command_line`] is identical on
    /// every rank (its options, e.g. `-ksp_type`, steer
    /// PETSc's collectives; a rank-specific one would make one rank fail
    /// inside a native collective while the others block in it — refused
    /// with `Error::InvalidArgument` on every rank, before PETSc runs;
    /// `PETSC_OPTIONS` needs no check, PETSc broadcasts it from rank 0),
    /// and so are the global options queued by [`set_global_option`]
    /// before the initialisation (that call agrees only
    /// over its own communicator). Then akantu duplicates `comm` and initialises PETSc on that
    /// duplicate, which becomes `PETSC_COMM_WORLD` and stays alive until
    /// `PetscFinalize` returned.
    ///
    /// Thread rules: the PETSc lock is not held across
    /// the agreement (only tried for the local initialisation query, then
    /// taken inside the agreed native entry); acquisitions of the process
    /// are serialised by a tried mutex — a concurrent acquisition on
    /// another thread is a local refusal, agreed like every other local
    /// failure.
    pub fn acquire(comm: &Communicator) -> Result<Self> {
        let serialised = match ACQUIRE.try_lock() {
            Ok(g) => Ok(g),
            Err(std::sync::TryLockError::Poisoned(p)) => Ok(p.into_inner()),
            Err(std::sync::TryLockError::WouldBlock) => Err(Error::Communication(
                "PETSc session: another thread of this process is acquiring a PETSc session \
                 (acquisitions are serialised: retry, or create PETSc backends from one thread)"
                    .into(),
            )),
        };
        let existing = CURRENT
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .as_ref()
            .and_then(Weak::upgrade);
        let mut flag: c_int = 0;
        let local = serialised
            .as_ref()
            .map(|_| ())
            .map_err(|e| Error::Communication(e.to_string()))
            .and_then(|()| {
                if existing.is_none() && FINALIZE_PENDING.load(Ordering::SeqCst) {
                    Err(Error::Communication(
                        "PETSc session: the previous session's PetscFinalize is still queued \
                         (complete the MPI lifecycle's deferred destructions on MPI's main \
                         thread first)"
                            .into(),
                    ))
                } else {
                    Ok(())
                }
            })
            .and_then(|()| check_types())
            .and_then(|()| {
                // A leaf use of the PETSc lock (tried, released before the
                // agreement below).
                let _g = PetscLock::try_enter().map_err(|e| Error::Communication(e.to_string()))?;
                // SAFETY: one valid `int` out-pointer; PetscInitialized is a
                // local query, run under the PETSc lock.
                check(
                    unsafe { ffi::akantu_petsc_is_initialized(&raw mut flag) },
                    "PetscInitialized",
                )
            });
        let initialized = flag != 0;
        let args = command_line();
        let digest = command_line_digest(&args);
        // the queued global options, agreed like the command line;
        // exactly these are applied at the initialisation
        let pending = PENDING_GLOBAL
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        let pending_digest = pending_options_digest(&pending);
        // [any rank failed locally, any initialised, any uninitialised,
        //  max(digest), max(!digest) = !min(digest), the same for the
        //  pending options]
        let mut states = [
            u64::from(local.is_err()),
            u64::from(local.is_ok() && initialized),
            u64::from(local.is_ok() && !initialized),
            digest,
            !digest,
            pending_digest,
            !pending_digest,
        ];
        comm.all_reduce(&mut states, ReduceOp::Max)?;
        local?;
        if states[0] == 1 {
            return Err(Error::Communication(
                "PETSc session: the local checks failed on another rank".into(),
            ));
        }
        let inner = match (states[1] == 1, states[2] == 1) {
            (true, true) => {
                return Err(Error::Unsupported(
                    "PETSc is initialised on some ranks of this communicator only: the first \
                     PETSc use of every process must go through the same communicator (it \
                     becomes PETSC_COMM_WORLD)"
                        .into(),
                ));
            }
            (true, false) => existing.unwrap_or_else(|| {
                // Initialised earlier by akantu on a world that is still
                // alive (finalised later), or outside akantu (a host
                // application: never finalised by akantu).
                Arc::new(SessionInner {
                    owned: library().owned,
                })
            }),
            _ => {
                if states[3] != !states[4] {
                    return Err(Error::InvalidArgument(
                        "PETSc session: the PETSc command line (set_command_line) differs \
                         between the ranks; its options steer PETSc's collective calls and must \
                         be identical on every rank"
                            .into(),
                    ));
                }
                if states[5] != !states[6] {
                    return Err(Error::InvalidArgument(
                        "PETSc session: the global options set before PETSc starts \
                         (set_global_option, queued until the initialisation) differ between \
                         the ranks; their options steer PETSc's collective calls and must be \
                         identical on every rank (set them through the \
                         communicator of the first PETSc use, or identically on every process)"
                            .into(),
                    ));
                }
                // PETSC_COMM_WORLD: akantu's own duplicate (agreed by its
                // header; refused, e.g. by the Serialized creation rule, on
                // every rank alike).
                let world = comm.duplicate()?;
                let argv = Argv::new(&args).map(Arc::new);
                let argc = argv
                    .as_ref()
                    .map(|a| c_int::try_from(a.pointers.len() - 1).unwrap_or(c_int::MAX))
                    .unwrap_or(0);
                let local = argv
                    .as_ref()
                    .map(|_| ())
                    .map_err(|e| Error::invalid(e.to_string()));
                let ptrs = argv
                    .as_ref()
                    .map(|a| a.pointers.as_ptr().cast_mut())
                    .unwrap_or(std::ptr::null_mut());
                // the agreed snapshot leaves the queue (an option queued by
                // another thread since stays queued)
                {
                    let mut q = PENDING_GLOBAL
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner);
                    let k = pending.len().min(q.len());
                    q.drain(..k);
                }
                let rc = world.native_collective("PetscInitialize", local, |nc| {
                    let _g = PetscLock::enter_native(&nc);
                    // Global options set before the initialisation (agreed
                    // by `set_global_option`), in order, before
                    // PetscInitialize (upstream's order).
                    for (n, v) in &pending {
                        // SAFETY: a null database is PETSc's global one,
                        // which PetscOptionsSetValue may write before
                        // PetscInitialize; NUL-terminated strings valid for
                        // the call (copied by PETSc); under the PETSc lock.
                        let rc = unsafe {
                            ffi::akantu_petsc_options_set(
                                std::ptr::null_mut(),
                                n.as_ptr(),
                                v.as_ptr(),
                            )
                        };
                        if rc != 0 {
                            return rc;
                        }
                    }
                    // SAFETY: `nc.raw()` is the live communicator handed out
                    // by `native_collective` on every rank (agreed entry,
                    // MPI thread policy held) — akantu's duplicate `world`,
                    // kept in the library state until PetscFinalize
                    // returned; `ptrs` points to `argc` valid C strings plus
                    // a NULL terminator, kept alive in the session (`argv`)
                    // until `PetscFinalize`.
                    unsafe { ffi::akantu_petsc_initialize(nc.raw(), argc, ptrs) }
                })?;
                check(rc, "PetscInitialize").map_err(collective_failure)?;
                {
                    let mut lib = library();
                    lib.owned = true;
                    lib.world_alive = true;
                    lib.argv = argv.ok();
                    lib.world = Some(world);
                }
                // Upstream `onCommunicatorFinalize` analogue: the first
                // user's last handle ends the library's world; PETSc is
                // finalised then, or when the last user goes.
                comm.on_finalize(|| {
                    let job = {
                        let mut lib = library();
                        lib.world_alive = false;
                        let users = CURRENT
                            .lock()
                            .unwrap_or_else(PoisonError::into_inner)
                            .as_ref()
                            .is_some_and(|w| w.strong_count() > 0);
                        if users { None } else { take_finalize(&mut lib) }
                    };
                    queue_finalize(job);
                });
                Arc::new(SessionInner { owned: true })
            }
        };
        *CURRENT.lock().unwrap_or_else(PoisonError::into_inner) = Some(Arc::downgrade(&inner));
        drop(serialised);
        Ok(Self { inner })
    }

    /// `true` if akantu initialised PETSc in this session (and finalises
    /// it with the last user).
    #[must_use]
    pub fn owns_library(&self) -> bool {
        self.inner.owned
    }
}
