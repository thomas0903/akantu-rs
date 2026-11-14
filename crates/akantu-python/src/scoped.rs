// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs design: replaces upstream's `getModel()` returning
// a long-lived `SolidMechanicsModel &` to Python from inside material
// callbacks (python/py_material.cc), which re-enters an object the solver is
// currently using.

//! Phase-scoped handles: Rust state lent to foreign code for the duration of
//! one callback.
//!
//! [`with_scoped`] lends `&T` (e.g. `&dyn ModelServices` for
//! `self.getModel()` inside `initMaterial`) as a [`Scoped`] handle that
//! foreign code may store anywhere. The handle is usable only until the
//! lending call returns: afterwards every use returns an error. Uses and the
//! end of the scope are synchronised, so a handle used from another thread
//! can never outlive the borrowed value (`T: Sync`; model services are
//! `Sync`):
//!
//! * a use ([`Scoped::with`]) either finds the scope open and is counted as
//!   *in flight* for its whole duration, or finds it closed and is
//!   **refused** — it never blocks, also when it is nested inside another
//!   in-flight use of the same scope;
//! * the end of the scope closes it (no new use is admitted) and then waits
//!   only for the uses already in flight, which keep the borrowed value
//!   alive. (A use that itself waits for the lending thread to return
//!   would wait forever: an application-level cycle, not a protocol path.)
//!
//! The scope state is guarded by a mutex that is never held while user code
//! runs. (A read-write lock held across uses deadlocked: a nested read
//! queued behind the scope end's pending write, which waited for the outer
//! read — `tests/foreign_views.rs::nested_scoped_use_during_scope_end_is_refused_not_blocked`.)
//!
//! **Execution contexts.** The lent value may live on
//! the stack of the lending call. A coroutine library that switches C stacks
//! (greenlet) can suspend the foreign code a callback runs while the lending
//! call is alive, copy that stack away and run other foreign code of the same
//! OS thread on the same addresses. Every lend therefore belongs to one
//! [`ContextKey`], and every use presents the key of the execution context
//! its caller runs in: a use is admitted only if the presented key is the
//! lend's own. The caller (the Python bindings: `py/runtime.rs`
//! `LendContext`) presents a lend's key only after checking that it runs
//! inside the lending call on the C stack that holds it (the Python frame
//! that made the lending call is in the running frame chain); any other
//! context is refused like another thread.
//!
//! **Compile-time guarantees** (probes CF1–CF3). The lent
//! `&mut T` cannot escape its use, nor can a reference returned by a
//! [`SolverAccess`] method, and the lender cannot use the value while it is
//! lent. Each probe below must fail to compile; the control before it
//! compiles and differs only in the marked line.
//!
//! ```
//! use py11_akantu::scoped::{ContextKey, ScopedMut};
//! pub fn control(h: &ScopedMut<Vec<f64>>, k: ContextKey) -> usize {
//!     h.with_mut(k, |v| v.len()).unwrap() // a value, not the borrow
//! }
//! ```
//!
//! ```compile_fail
//! use py11_akantu::scoped::{ContextKey, ScopedMut};
//! pub fn cf1(h: &ScopedMut<Vec<f64>>, k: ContextKey) -> &'static mut Vec<f64> {
//!     h.with_mut(k, |v| v).unwrap() // CF1: the `&mut T` escapes the use
//! }
//! ```
//!
//! ```
//! use py11_akantu::scoped::{ContextKey, ScopedMut, SolverAccess};
//! pub fn control(h: &ScopedMut<dyn SolverAccess>, k: ContextKey) -> bool {
//!     h.with_mut(k, |a| a.dofs().is_ok()).unwrap() // a value, not the borrow
//! }
//! ```
//!
//! ```compile_fail
//! use akantu::solver::DofManager;
//! use py11_akantu::scoped::{ContextKey, ScopedMut, SolverAccess};
//! pub fn cf2(h: &ScopedMut<dyn SolverAccess>, k: ContextKey) -> &'static mut DofManager {
//!     h.with_mut(k, |a| a.dofs().unwrap()).unwrap() // CF2: `dofs()` escapes the use
//! }
//! ```
//!
//! ```
//! use py11_akantu::scoped::{ContextKey, with_scoped_mut};
//! let mut x = vec![1.0_f64];
//! let k = ContextKey::fresh();
//! let n = with_scoped_mut(&mut x, k, |h| {
//!     h.with_mut(k, |v| v.push(2.0)).unwrap(); // through the handle
//!     h.with_mut(k, |v| v.len()).unwrap()
//! });
//! assert_eq!(n, 2);
//! ```
//!
//! ```compile_fail,E0499
//! use py11_akantu::scoped::{ContextKey, with_scoped_mut};
//! let mut x = vec![1.0_f64];
//! let k = ContextKey::fresh();
//! let n = with_scoped_mut(&mut x, k, |h| {
//!     x.push(2.0); // CF3: the lender uses the value while it is lent
//!     h.with_mut(k, |v| v.len()).unwrap()
//! });
//! assert_eq!(n, 2);
//! ```

use std::marker::PhantomData;
use std::ptr::NonNull;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};

use akantu::core::enums::MatrixType;
use akantu::core::{Error, Result};
use akantu::model::constitutive_law::ModelServices;
use akantu::solver::DofManager;

/// The execution context a lend belongs to. A use of a
/// handle presents the key of the context its caller runs in and is
/// admitted only if it is the lend's key. Keys are never reused in a
/// process (a 64-bit counter: 2^64 lends cannot happen).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ContextKey(u64);

impl ContextKey {
    /// A key not handed out before in this process.
    #[must_use]
    pub fn fresh() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        Self(NEXT.fetch_add(1, Ordering::Relaxed))
    }
}

/// Why a use was refused because of its execution context.
const FOREIGN_CONTEXT: &str = "used from another execution context than the callback that \
     provided it (another thread, a greenlet, or a generator / coroutine resumed outside the \
     callback)";

/// Shared state of one lending call.
#[derive(Debug)]
struct ScopeState {
    /// `true` until the lending call starts returning.
    open: bool,
    /// Uses currently running `f` on the lent value.
    in_flight: usize,
}

#[derive(Debug)]
struct Scope {
    state: Mutex<ScopeState>,
    /// Signalled when the last in-flight use of a closed scope ends.
    idle: Condvar,
    /// The execution context of the lend.
    key: ContextKey,
}

impl Scope {
    fn new(key: ContextKey) -> Arc<Self> {
        Arc::new(Self {
            state: Mutex::new(ScopeState {
                open: true,
                in_flight: 0,
            }),
            idle: Condvar::new(),
            key,
        })
    }

    fn lock(&self) -> MutexGuard<'_, ScopeState> {
        // No user code runs under this lock (only flag/counter updates), so
        // a poisoned lock still holds consistent state.
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Admit one use from the context `caller`, or say why not (another
    /// context, or the scope is closed).
    fn begin_use(
        self: &Arc<Self>,
        caller: ContextKey,
    ) -> std::result::Result<InFlight, &'static str> {
        if caller != self.key {
            return Err(FOREIGN_CONTEXT);
        }
        let mut st = self.lock();
        if !st.open {
            return Err("used outside the callback that provided it");
        }
        st.in_flight += 1;
        Ok(InFlight(Arc::clone(self)))
    }
}

/// One admitted use; ends it on drop (also on unwind out of `f`).
struct InFlight(Arc<Scope>);

impl Drop for InFlight {
    fn drop(&mut self) {
        let mut st = self.0.lock();
        st.in_flight -= 1;
        if st.in_flight == 0 && !st.open {
            self.0.idle.notify_all();
        }
    }
}

/// A handle on `T` valid only during one [`with_scoped`] call.
pub struct Scoped<T: ?Sized + 'static> {
    ptr: NonNull<T>,
    scope: Arc<Scope>,
    _marker: PhantomData<&'static T>,
}

impl<T: ?Sized + 'static> std::fmt::Debug for Scoped<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Scoped")
            .field("valid", &self.is_valid())
            .finish()
    }
}

impl<T: ?Sized + 'static> Clone for Scoped<T> {
    fn clone(&self) -> Self {
        Self {
            ptr: self.ptr,
            scope: Arc::clone(&self.scope),
            _marker: PhantomData,
        }
    }
}

// SAFETY: a `Scoped<T>` only ever yields `&T`, inside `with`, while that
// use is counted in flight, and a use is admitted only while the lending
// call (and so the borrowed value) is alive and only for a caller that
// presents the lend's execution-context key (which its callers present only
// from inside the lending call on its own stack);
// `T: Sync` makes such shared access from another thread sound. The scope
// end waits until no admitted use is in flight, so it cannot overlap a use.
// `T: Send` is not needed: a `Scoped` never moves, mutates or drops the `T`
// — only the lender does.
unsafe impl<T: ?Sized + Sync + 'static> Send for Scoped<T> {}
// SAFETY: as above; `with` takes `&self` and only hands out `&T`.
unsafe impl<T: ?Sized + Sync + 'static> Sync for Scoped<T> {}

impl<T: ?Sized + 'static> Scoped<T> {
    /// Run `f` on the lent value for a caller running in the execution
    /// context `caller`, or fail — without waiting — if `caller` is not the
    /// lend's context or if its phase has ended (or is ending).
    pub fn with<R>(&self, caller: ContextKey, f: impl FnOnce(&T) -> R) -> Result<R> {
        // Named binding on purpose: the in-flight guard must live until `f`
        // returns (`let Ok(_) = …` would drop it at once and make the
        // dereference below unsound).
        let _use = self
            .scope
            .begin_use(caller)
            .map_err(|why| Error::invalid(format!("model handle {why}")))?;
        // SAFETY: the use was admitted while the scope was open, i.e. while
        // `with_scoped` was running and the `&'a T` it was created from was
        // alive, for the lend's own execution context (whose callers present
        // its key only from inside the lending call, on the stack that holds
        // it: a stack-switching coroutine that copied that stack away runs
        // in another context and is refused); `_use` keeps it counted in
        // flight until `f` returns (or unwinds), and the scope end
        // (`EndScope`) does not return — so the borrow does not end — while
        // a use is in flight. Admission, release and the close are all
        // ordered by the scope mutex, which is the happens-before edge
        // between this access and the end of the borrow. Only shared access
        // is handed out.
        Ok(f(unsafe { self.ptr.as_ref() }))
    }

    /// `true` until the lending call starts returning; once `false`, no new
    /// use is admitted (uses already in flight may still be running, and
    /// see `false` from inside their own callback).
    #[must_use]
    pub fn is_valid(&self) -> bool {
        self.scope.lock().open
    }
}

struct EndScope(Arc<Scope>);

impl Drop for EndScope {
    fn drop(&mut self) {
        // Close (refuse new uses), then wait for the in-flight ones (also on
        // unwind). Never holds the lock while a use runs.
        let mut st = self.0.lock();
        st.open = false;
        while st.in_flight > 0 {
            st = self.0.idle.wait(st).unwrap_or_else(PoisonError::into_inner);
        }
    }
}

/// Lend `value` to `f` as a [`Scoped`] handle of `T + 'static` type
/// (the lifetime is erased and re-imposed dynamically), for uses from the
/// execution context `key`.
pub fn with_scoped<T: ?Sized + 'static, R>(
    value: &T,
    key: ContextKey,
    f: impl FnOnce(Scoped<T>) -> R,
) -> R {
    let scope = Scope::new(key);
    let _end = EndScope(Arc::clone(&scope));
    let handle = Scoped {
        ptr: NonNull::from(value),
        scope,
        _marker: PhantomData,
    };
    f(handle)
}

/// Lend phase-scoped model services (`self.getModel()` inside a law
/// callback) for uses from the execution context `key`. The trait object's
/// lifetime bound is erased for storage and re-imposed dynamically by the
/// scope's admission protocol.
pub fn with_model_services<'a, R>(
    services: &'a (dyn ModelServices + 'a),
    key: ContextKey,
    f: impl FnOnce(Scoped<dyn ModelServices>) -> R,
) -> R {
    let scope = Scope::new(key);
    let _end = EndScope(Arc::clone(&scope));
    let short: *const (dyn ModelServices + 'a) = services;
    // SAFETY: only the trait-object lifetime bound changes (same fat-pointer
    // layout). The `'static` bound is never relied upon: every dereference
    // goes through `Scoped::with`, which admits a use only while this call
    // (and therefore `'a`) is running and only for the execution context
    // `key`, and `_end` keeps `'a` alive until no admitted use is in flight.
    let erased: *const (dyn ModelServices + 'static) = unsafe { std::mem::transmute(short) };
    let ptr =
        NonNull::new(erased.cast_mut()).unwrap_or_else(|| unreachable!("references are non-null"));
    f(Scoped {
        ptr,
        scope,
        _marker: PhantomData,
    })
}

// ---- exclusive lends (`&mut T`) for one callback -------------------

/// State of one exclusive lending call.
#[derive(Debug)]
struct ExclusiveState {
    /// `true` until the lending call starts returning.
    open: bool,
    /// A use is running `f` on the lent value.
    in_use: bool,
}

#[derive(Debug)]
struct ExclusiveScope {
    state: Mutex<ExclusiveState>,
    /// Signalled when the use in flight of a closed scope ends.
    idle: Condvar,
    /// The lending thread: the only thread on which a use is admitted.
    thread: std::thread::ThreadId,
    /// The execution context of the lend: the only one admitted.
    key: ContextKey,
}

impl ExclusiveScope {
    fn new(key: ContextKey) -> Arc<Self> {
        Arc::new(Self {
            state: Mutex::new(ExclusiveState {
                open: true,
                in_use: false,
            }),
            idle: Condvar::new(),
            thread: std::thread::current().id(),
            key,
        })
    }

    fn lock(&self) -> MutexGuard<'_, ExclusiveState> {
        // no user code runs under this lock (flag updates only)
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Admit one exclusive use on the lending thread from the lend's
    /// execution context `caller`, or say why not.
    fn begin_use(
        self: &Arc<Self>,
        caller: ContextKey,
    ) -> std::result::Result<ExclusiveUse, &'static str> {
        if std::thread::current().id() != self.thread {
            return Err("used from another thread than the callback that provided it");
        }
        if caller != self.key {
            return Err(FOREIGN_CONTEXT);
        }
        let mut st = self.lock();
        if !st.open {
            return Err("used outside the callback that provided it");
        }
        if st.in_use {
            return Err("already in use by a running call (re-entrant use)");
        }
        st.in_use = true;
        Ok(ExclusiveUse(Arc::clone(self)))
    }
}

/// One admitted exclusive use; ends it on drop (also on unwind out of `f`).
struct ExclusiveUse(Arc<ExclusiveScope>);

impl Drop for ExclusiveUse {
    fn drop(&mut self) {
        let mut st = self.0.lock();
        st.in_use = false;
        if !st.open {
            self.0.idle.notify_all();
        }
    }
}

/// An exclusive handle on `T` (a `&mut T` lent by [`with_scoped_mut`] or
/// [`with_solver_access`]), valid only during the lending call, usable only
/// on the lending thread, from the lend's execution context and by one use
/// at a time. Every other use returns an error; nothing ever blocks except
/// the end of the lending call, which waits for the use in flight (only
/// possible if a caller presents the lend's key from a coroutine suspended on
/// the lending thread, which the bindings never do).
pub struct ScopedMut<T: ?Sized + 'static> {
    ptr: NonNull<T>,
    scope: Arc<ExclusiveScope>,
    _marker: PhantomData<&'static mut T>,
}

impl<T: ?Sized + 'static> std::fmt::Debug for ScopedMut<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ScopedMut")
            .field("valid", &self.is_valid())
            .finish()
    }
}

impl<T: ?Sized + 'static> Clone for ScopedMut<T> {
    fn clone(&self) -> Self {
        Self {
            ptr: self.ptr,
            scope: Arc::clone(&self.scope),
            _marker: PhantomData,
        }
    }
}

// SAFETY: a `ScopedMut<T>` dereferences its pointer only inside `with_mut`,
// after `begin_use` checked that the current thread is the lending thread,
// that the caller presents the lend's execution-context key, that the
// lending call is still running and that no other use is in flight. Moving or sharing the handle to another thread therefore never
// moves or shares the `T`: that thread can only clone/drop the handle (an
// `Arc` of a `Mutex`/`Condvar` state, `Send + Sync`) or have its uses
// refused. Neither `T: Send` nor `T: Sync` is needed, because every access
// to the `T` happens on the thread that lent it.
unsafe impl<T: ?Sized + 'static> Send for ScopedMut<T> {}
// SAFETY: as above; `&ScopedMut<T>` only allows `with_mut` (thread-checked,
// exclusive) and `is_valid` (reads the scope flag under its mutex).
unsafe impl<T: ?Sized + 'static> Sync for ScopedMut<T> {}

impl<T: ?Sized + 'static> ScopedMut<T> {
    /// Run `f` on the lent value for a caller running in the execution
    /// context `caller`, or fail — without waiting — on another thread, if
    /// `caller` is not the lend's context, if the lending call has ended (or
    /// is ending), or if another use is in flight (also a nested use from
    /// inside `f`).
    pub fn with_mut<R>(&self, caller: ContextKey, f: impl FnOnce(&mut T) -> R) -> Result<R> {
        // Named binding on purpose: the use must stay admitted until `f`
        // returns (see `Scoped::with`).
        let _use = self
            .scope
            .begin_use(caller)
            .map_err(|why| Error::invalid(format!("callback handle {why}")))?;
        // SAFETY: the use was admitted on the lending thread, for the lend's
        // own execution context (whose callers present its key only from
        // inside the lending call, on the stack that holds it: a
        // stack-switching coroutine that copied that stack away runs in
        // another context and is refused), while the
        // lending call — and so the `&'a mut T` the handle was created from
        // — was alive. The lender handed that borrow over for the whole call
        // (`with_scoped_mut` keeps it and never uses it while `f` runs),
        // and every other use of the handle is refused while this one is in
        // flight (`in_use`, uses on other threads refused outright), so the
        // `&mut T` created here is the only reference to the value for the
        // duration of `f`. The scope end (`EndExclusive`) does not return —
        // so the borrow does not end — while a use is in flight; admission,
        // release and close are ordered by the scope mutex.
        Ok(f(unsafe { &mut *self.ptr.as_ptr() }))
    }

    /// `true` until the lending call starts returning.
    #[must_use]
    pub fn is_valid(&self) -> bool {
        self.scope.lock().open
    }
}

struct EndExclusive(Arc<ExclusiveScope>);

impl Drop for EndExclusive {
    fn drop(&mut self) {
        // Close (refuse new uses), then wait for a use in flight (also on
        // unwind). Never holds the lock while a use runs.
        let mut st = self.0.lock();
        st.open = false;
        while st.in_use {
            st = self.0.idle.wait(st).unwrap_or_else(PoisonError::into_inner);
        }
    }
}

/// Lend `value` exclusively to `f` as a [`ScopedMut`] handle for uses from
/// the execution context `key` (the reference's lifetime is erased and
/// re-imposed dynamically; `T` itself is `'static`, so no lifetime inside the
/// value is erased).
pub fn with_scoped_mut<T: ?Sized + 'static, R>(
    value: &mut T,
    key: ContextKey,
    f: impl FnOnce(ScopedMut<T>) -> R,
) -> R {
    let scope = ExclusiveScope::new(key);
    let _end = EndExclusive(Arc::clone(&scope));
    f(ScopedMut {
        ptr: NonNull::from(value),
        scope,
        _marker: PhantomData,
    })
}

/// What a Python solver callback may use of the running solve while one of
/// its methods is being called back (upstream: `self.model.assembleMatrix`,
/// `model.getDOFManager()…` from an `InterceptSolverCallback`, which reach
/// the model the solver is driving). Implemented by the bindings for the
/// model callback's `Delegate` and for the DOF manager alone.
pub trait SolverAccess {
    /// The model callback's `getMatrixType`.
    fn matrix_type(&self, matrix_id: &str) -> Result<MatrixType>;
    /// The model callback's `assembleMatrix`.
    fn assemble_matrix(&mut self, matrix_id: &str) -> Result<()>;
    /// The model callback's `assembleLumpedMatrix`.
    fn assemble_lumped_matrix(&mut self, matrix_id: &str) -> Result<()>;
    /// The model callback's `assembleResidual`.
    fn assemble_residual(&mut self) -> Result<()>;
    /// The model callback's `predictor`.
    fn predictor(&mut self) -> Result<()>;
    /// The model callback's `corrector`.
    fn corrector(&mut self) -> Result<()>;
    /// The model callback's `beforeSolveStep`.
    fn before_solve_step(&mut self) -> Result<()>;
    /// The model callback's `afterSolveStep`.
    fn after_solve_step(&mut self, converged: bool) -> Result<()>;
    /// The DOF manager of the running solve.
    fn dofs(&mut self) -> Result<&mut DofManager>;
}

/// Lend solve access (a trait object borrowing the running solve) to `f`
/// as a [`ScopedMut`] handle for uses from the execution context `key`. The
/// trait object's lifetime bound is erased for storage and re-imposed
/// dynamically by the handle's admission protocol (as
/// [`with_model_services`]).
pub fn with_solver_access<'a, R>(
    access: &'a mut (dyn SolverAccess + 'a),
    key: ContextKey,
    f: impl FnOnce(ScopedMut<dyn SolverAccess>) -> R,
) -> R {
    let scope = ExclusiveScope::new(key);
    let _end = EndExclusive(Arc::clone(&scope));
    let short: *mut (dyn SolverAccess + 'a) = access;
    // SAFETY: only the trait-object lifetime bound changes (same fat-pointer
    // layout). The `'static` bound is never relied upon: every dereference
    // goes through `ScopedMut::with_mut`, which admits a use only on this
    // thread, for the execution context `key`, while this call (and
    // therefore `'a`) is running and no other use is in flight, and `_end`
    // keeps `'a` alive until the use in flight ended. Inside `with_mut` the closure receives `&mut (dyn SolverAccess
    // + 'static)` for the duration of the use only: the trait's methods
    // return nothing that outlives that borrow, and an unsized trait object
    // cannot be moved out of it.
    let erased: *mut (dyn SolverAccess + 'static) = unsafe { std::mem::transmute(short) };
    let ptr = NonNull::new(erased).unwrap_or_else(|| unreachable!("references are non-null"));
    f(ScopedMut {
        ptr,
        scope,
        _marker: PhantomData,
    })
}

// ---- the running Python frame (feature `python`) --------

/// The Python side of [`ContextKey`]: the frame chain of the running thread
/// state, from which the bindings decide whether a caller runs inside a
/// lending call (`py/runtime.rs` `LendContext`).
#[cfg(feature = "python")]
pub(crate) mod python {
    use pyo3::prelude::*;

    /// The frame object of the running thread state's current Python frame
    /// (C API `PyEval_GetFrame`, stable ABI), `None` if no Python frame is
    /// running (or CPython could not create the frame object).
    ///
    /// The C API is used rather than `sys._getframe()`, which is looked up
    /// through the `sys` module at run time and can be replaced by Python
    /// code (debuggers and profilers do): the frame returned here is the one
    /// the interpreter is executing in this execution context, whatever user
    /// code did. Following `f_back` from it (a read-only attribute of the
    /// immutable built-in frame type, which runs no user code) walks the
    /// frames this context is executing, innermost first.
    pub(crate) fn current_frame(py: Python<'_>) -> Option<Bound<'_, PyAny>> {
        // SAFETY: `py` proves this thread is attached to the interpreter.
        // `PyEval_GetFrame` takes no argument and returns a *borrowed*
        // reference to the current frame object of the thread state, or NULL
        // (no frame; on 3.11+ also when the frame object cannot be created,
        // in which case CPython clears the error itself).
        let frame = unsafe { pyo3::ffi::PyEval_GetFrame() };
        // SAFETY: `frame` is NULL or a valid borrowed `PyFrameObject *` (an
        // object pointer) owned by the running frame, which stays alive
        // while this code runs inside it; `from_borrowed_ptr_or_opt` takes a
        // new strong reference at once (NULL gives `None`).
        unsafe { Bound::from_borrowed_ptr_or_opt(py, frame.cast()) }
    }
}
