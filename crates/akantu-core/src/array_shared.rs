// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs design (decision record D-024): access protocol
// for arrays exported to foreign code. Upstream (python/py_aka_array.hh,
// pybind11) hands raw `Array` pointers to NumPy with no access protocol.

//! Exportable arrays and the foreign-access phase protocol
//! (`docs/ARCHITECTURE.md`).
//!
//! Foreign code (NumPy, C clients) accesses exported values through raw
//! pointers that no Rust borrow can see. Soundness therefore needs a
//! protocol that makes **native element borrows** and **foreign accesses**
//! of exported allocations mutually exclusive, and serialises foreign
//! accesses among themselves. This module provides it without `unsafe`:
//!
//! * [`SharedArray<T>`] is the only exportable array type. Its elements are
//!   reachable natively **only through guards** ([`SharedArray::read`] →
//!   [`ArrayRef`], [`SharedArray::write`] → [`ArrayMut`]). While the
//!   allocation is exported, acquiring a guard requires the runtime to be in
//!   its *native phase* and counts the guard in the runtime; otherwise the
//!   acquisition fails. A plain [`crate::Array`] can never be exported, so
//!   un-guarded `as_slice()`/`as_mut_slice()` never alias foreign memory.
//! * [`ForeignRuntime`] (one per Python interpreter / C context) is in the
//!   **foreign phase** by default: foreign code may run, native guards on
//!   arrays exported to it cannot be taken. Foreign accesses need an
//!   [`Interpreter`] token: exclusive across threads (the GIL stand-in;
//!   `!Send + !Sync`, re-entrant on its thread only in the foreign phase).
//! * [`Interpreter::native`] opens a [`NativeSection`] (binding entry
//!   point): native phase, guards may be acquired on any thread. It borrows
//!   the interpreter mutably, so no foreign access through that token can
//!   happen inside it.
//! * [`ForeignRuntime::callback`] (trampoline into foreign code from inside a
//!   section) switches back to the foreign phase **only if no native guard
//!   is alive**; it fails otherwise. A section that ends while guards are
//!   still alive (guards leaked into another thread or `mem::forget`) leaves
//!   the runtime *poisoned*: foreign accesses are refused until the last
//!   such guard is dropped.
//!
//! Every phase change is a compare-and-swap on one atomic word with
//! acquire/release ordering, so a guard release happens-before the next
//! foreign access and a foreign access happens-before the next guard. Any
//! sequence of safe calls (including leaks) can only make operations fail,
//! never overlap a native borrow with a foreign access.
//!
//! **Guarantee relied upon by unsafe binding code.** If
//! [`crate::array_storage::ExportLease::check_foreign_access`]`(interp)`
//! returns `Ok(())`, then while `interp` stays borrowed no native guard of
//! any allocation exported to that runtime is alive or can be acquired, and
//! no other thread can perform foreign accesses on it.

use std::fmt;
use std::marker::PhantomData;
use std::ops::{Deref, DerefMut, Index, IndexMut};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::ThreadId;

use crate::array::{Array, ArrayViewMut, TuplesMut};
use crate::array_storage::ExportLease;
use crate::error::{Error, Result};
use crate::types::{Id, Real};

/// Native phase (a section is active, or a poisoned leftover).
const NATIVE: u64 = 1 << 63;
/// A section ended while native guards were alive.
const POISON: u64 = 1 << 62;
/// Live native guards on allocations exported to the runtime.
const COUNT: u64 = (1 << 32) - 1;

#[derive(Debug, Default)]
struct LockState {
    owner: Option<ThreadId>,
    depth: usize,
}

#[derive(Debug, Default)]
struct RuntimeState {
    /// `NATIVE | POISON | count`; `0` = foreign phase.
    phase: AtomicU64,
    /// Interpreter ownership (thread, re-entrancy depth).
    lock: Mutex<LockState>,
}

impl RuntimeState {
    fn lock(&self) -> MutexGuard<'_, LockState> {
        // Nothing panics while the lock is held; recover anyway.
        self.lock
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Compare-and-swap loop (`fetch_update` without its MSRV/deprecation
    /// churn): applies `f` until it returns `None` (→ `Err(current)`) or the
    /// swap succeeds (→ `Ok(previous)`).
    fn update(&self, f: impl Fn(u64) -> Option<u64>) -> std::result::Result<u64, u64> {
        let mut prev = self.phase.load(Ordering::Acquire);
        while let Some(next) = f(prev) {
            match self
                .phase
                .compare_exchange_weak(prev, next, Ordering::AcqRel, Ordering::Acquire)
            {
                Ok(p) => return Ok(p),
                Err(p) => prev = p,
            }
        }
        Err(prev)
    }

    fn begin_native(&self) -> Result<()> {
        self.update(|w| {
            if w & NATIVE == 0 {
                Some(NATIVE) // count is 0 in the foreign phase
            } else if w & POISON != 0 {
                Some(w & !POISON) // adopt leftover guards
            } else {
                None
            }
        })
        .map(drop)
        .map_err(|_| Error::invalid("a native section is already active on this runtime"))
    }

    fn end_native(&self) {
        let _ = self.update(|w| {
            if w & NATIVE == 0 {
                None // already foreign (a leaked callback): keep
            } else if w & COUNT == 0 {
                Some(0)
            } else {
                Some(w | POISON)
            }
        });
    }

    fn begin_callback(&self) -> Result<()> {
        self.phase
            .compare_exchange(NATIVE, 0, Ordering::AcqRel, Ordering::Acquire)
            .map(drop)
            .map_err(|w| {
                if w & NATIVE == 0 {
                    Error::invalid("foreign callback requested outside a native section")
                } else {
                    Error::invalid(format!(
                        "{} native element borrow(s) of exported arrays are alive across a \
                         foreign callback",
                        w & COUNT
                    ))
                }
            })
    }

    fn end_callback(&self) {
        let _ = self.update(|w| Some((w & COUNT) | NATIVE));
    }

    fn acquire_guard(&self) -> Result<()> {
        self.update(|w| (w & NATIVE != 0 && w & POISON == 0 && w & COUNT < COUNT).then_some(w + 1))
            .map(drop)
            .map_err(|w| {
                if w & NATIVE == 0 || w & POISON != 0 {
                    Error::invalid(
                        "array is exported to foreign code: native access requires an active \
                         native section of its runtime",
                    )
                } else {
                    Error::CapacityExceeded {
                        what: "native array guards",
                        requested: u128::from(COUNT) + 1,
                        limit: u128::from(COUNT),
                    }
                }
            })
    }

    fn release_guard(&self) {
        let _ = self.update(|w| {
            let k = (w & COUNT).saturating_sub(1);
            if w & POISON != 0 && k == 0 {
                Some(0)
            } else {
                Some((w & !COUNT) | k)
            }
        });
    }

    fn is_foreign(&self) -> bool {
        self.phase.load(Ordering::Acquire) & NATIVE == 0
    }
}

/// A foreign runtime: the phase state shared by every allocation exported to
/// it (one per Python interpreter, one per C context, one per test).
#[derive(Clone, Debug, Default)]
pub struct ForeignRuntime {
    state: Arc<RuntimeState>,
}

impl ForeignRuntime {
    /// A fresh runtime in the foreign phase, interpreter free.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// `true` for clones of the same runtime.
    #[must_use]
    pub fn same_as(&self, other: &ForeignRuntime) -> bool {
        Arc::ptr_eq(&self.state, &other.state)
    }

    /// Acquire the interpreter (GIL stand-in). Exclusive across threads;
    /// re-entrant on the owning thread while the runtime is in the foreign
    /// phase (a binding entry point called from a foreign callback).
    pub fn enter(&self) -> Result<Interpreter> {
        let me = std::thread::current().id();
        let mut l = self.state.lock();
        match l.owner {
            None => {
                l.owner = Some(me);
                l.depth = 1;
            }
            Some(o) if o == me => {
                if !self.state.is_foreign() {
                    return Err(Error::invalid(
                        "native code may not re-enter its foreign runtime",
                    ));
                }
                l.depth += 1;
            }
            Some(_) => {
                return Err(Error::invalid(
                    "foreign runtime is in use by another thread",
                ));
            }
        }
        Ok(Interpreter {
            rt: self.clone(),
            _not_send_sync: PhantomData,
        })
    }

    /// Switch back to the foreign phase for a callback into foreign code
    /// (trampoline). Requires: the calling thread holds the interpreter, a
    /// native section is active and **no native guard is alive**. The phase
    /// returns to native when the [`ForeignCallback`] drops.
    pub fn callback(&self) -> Result<ForeignCallback> {
        let me = std::thread::current().id();
        let mut l = self.state.lock();
        if l.owner != Some(me) {
            return Err(Error::invalid(
                "foreign callbacks run on the thread that holds the runtime",
            ));
        }
        self.state.begin_callback()?;
        l.depth += 1;
        Ok(ForeignCallback {
            interp: Interpreter {
                rt: self.clone(),
                _not_send_sync: PhantomData,
            },
        })
    }

    /// `true` while no native section (or poisoned leftover) is active.
    #[must_use]
    pub fn is_foreign_phase(&self) -> bool {
        self.state.is_foreign()
    }

    /// Number of live native guards on allocations exported to this runtime
    /// (diagnostics).
    #[must_use]
    pub fn native_guards(&self) -> u64 {
        self.state.phase.load(Ordering::Acquire) & COUNT
    }

    /// `true` if a section ended while guards were alive and they still are.
    #[must_use]
    pub fn is_poisoned(&self) -> bool {
        self.state.phase.load(Ordering::Acquire) & POISON != 0
    }
}

/// Exclusive right to run foreign code on a [`ForeignRuntime`] (the GIL
/// stand-in). `!Send + !Sync`: it stays on the thread that acquired it.
pub struct Interpreter {
    rt: ForeignRuntime,
    _not_send_sync: PhantomData<*const ()>,
}

impl fmt::Debug for Interpreter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Interpreter")
            .field("foreign_phase", &self.rt.is_foreign_phase())
            .finish()
    }
}

impl Drop for Interpreter {
    fn drop(&mut self) {
        let mut l = self.rt.state.lock();
        l.depth = l.depth.saturating_sub(1);
        if l.depth == 0 {
            l.owner = None;
        }
    }
}

impl Interpreter {
    /// The runtime this token belongs to.
    #[must_use]
    pub fn runtime(&self) -> &ForeignRuntime {
        &self.rt
    }

    /// `Ok(())` iff the runtime is in the foreign phase. Because the token is
    /// held (and only a `&mut` borrow of a token can start a section), the
    /// phase cannot change while `&self` is borrowed.
    pub fn check_foreign(&self) -> Result<()> {
        if self.rt.state.is_foreign() {
            Ok(())
        } else if self.rt.is_poisoned() {
            Err(Error::invalid(
                "native element borrows outlived their section: foreign access refused until \
                 they are dropped",
            ))
        } else {
            Err(Error::invalid(
                "foreign access to an exported array during a native section",
            ))
        }
    }

    /// Enter the native phase (a binding entry point running native code).
    pub fn native(&mut self) -> Result<NativeSection<'_>> {
        self.rt.state.begin_native()?;
        Ok(NativeSection { interp: self })
    }
}

/// The native phase of a runtime, opened from its interpreter.
#[derive(Debug)]
pub struct NativeSection<'a> {
    interp: &'a mut Interpreter,
}

impl NativeSection<'_> {
    /// The runtime of this section.
    #[must_use]
    pub fn runtime(&self) -> &ForeignRuntime {
        &self.interp.rt
    }
}

impl Drop for NativeSection<'_> {
    fn drop(&mut self) {
        self.interp.rt.state.end_native();
    }
}

/// The foreign phase re-opened for a callback from inside a native section.
/// Dereferences to an [`Interpreter`] (foreign accesses, nested
/// [`Interpreter::native`] for binding calls made by the callback).
#[derive(Debug)]
pub struct ForeignCallback {
    interp: Interpreter,
}

impl Deref for ForeignCallback {
    type Target = Interpreter;
    fn deref(&self) -> &Interpreter {
        &self.interp
    }
}

impl DerefMut for ForeignCallback {
    fn deref_mut(&mut self) -> &mut Interpreter {
        &mut self.interp
    }
}

impl Drop for ForeignCallback {
    fn drop(&mut self) {
        // Back to the enclosing section (the `interp` field, dropped after
        // this, releases the re-entrant interpreter depth).
        self.interp.rt.state.end_callback();
    }
}

/// A counted native guard (decrements its runtime's count on drop).
#[derive(Debug)]
struct GuardCount {
    rt: ForeignRuntime,
    /// Export-all test mode: the guard also counts on the thread that took
    /// it (for [`export_all::callback_site`]).
    #[cfg(feature = "export-all")]
    _thread: export_all::ThreadGuard,
}

impl GuardCount {
    /// Count a guard of `rt` (the caller acquired it on the runtime).
    #[inline]
    fn new(rt: ForeignRuntime) -> Self {
        Self {
            rt,
            #[cfg(feature = "export-all")]
            _thread: export_all::ThreadGuard::new(),
        }
    }
}

impl Drop for GuardCount {
    fn drop(&mut self) {
        self.rt.state.release_guard();
    }
}

/// Test-only **export-all mode** (`docs/ARCHITECTURE.md`, "Shared storage and its aliasing rules", `docs/TESTING.md`): with the cargo feature `export-all` of
/// `akantu-core` (never enabled by a product crate; the gate's `exportall`
/// tier builds the workspace tests with
/// `--features akantu-core/export-all`), every [`SharedArray`] behaves as if
/// it were exported to one process-wide runtime that stays in its native
/// phase: its blocks are retired (fresh allocation, generation + 1) on every
/// change that retires an exported block, every guard is counted on that
/// runtime and on the thread that took it, and the built-in boundary
/// conditions take their foreign-functor paths. [`SharedArray::is_exported`]
/// keeps reporting real leases only. Without the feature, `ENABLED` is
/// `false` and nothing here changes behaviour.
pub mod export_all {
    /// `true` in a build with the test-only feature `export-all`.
    pub const ENABLED: bool = cfg!(feature = "export-all");

    #[cfg(feature = "export-all")]
    pub(in crate::array_shared) use imp::{ThreadGuard, global_count};

    #[cfg(feature = "export-all")]
    mod imp {
        use super::super::{ForeignRuntime, GuardCount, NATIVE};
        use crate::error::Result;
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::{Arc, OnceLock};

        fn global() -> &'static ForeignRuntime {
            static RT: OnceLock<ForeignRuntime> = OnceLock::new();
            RT.get_or_init(|| {
                let rt = ForeignRuntime::new();
                // a native phase that never ends: guards are always admitted
                rt.state.phase.store(NATIVE, Ordering::Release);
                rt
            })
        }

        /// A guard of an array the mode treats as exported.
        pub(in crate::array_shared) fn global_count() -> Result<GuardCount> {
            let rt = global();
            rt.state.acquire_guard()?;
            Ok(GuardCount::new(rt.clone()))
        }

        thread_local! {
            static GUARDS: Arc<AtomicUsize> = Arc::new(AtomicUsize::new(0));
        }

        /// One guard counted on the thread that took it; released on the
        /// same counter wherever the guard drops.
        #[derive(Debug)]
        pub(in crate::array_shared) struct ThreadGuard(Arc<AtomicUsize>);

        impl ThreadGuard {
            pub(in crate::array_shared) fn new() -> Self {
                let c = GUARDS.with(Arc::clone);
                c.fetch_add(1, Ordering::Relaxed);
                Self(c)
            }
        }

        impl Drop for ThreadGuard {
            fn drop(&mut self) {
                self.0.fetch_sub(1, Ordering::Relaxed);
            }
        }

        /// Guards taken on this thread and still alive.
        pub(super) fn alive_on_this_thread() -> usize {
            GUARDS.with(|c| c.load(Ordering::Relaxed))
        }
    }

    /// A point where native code may call back into foreign code: a solver
    /// callback step (`DofManager::step`), a boundary-condition functor on
    /// its foreign path, a foreign material's method. In the export-all mode
    /// it **panics** if a native guard taken on this thread is still alive
    /// there (a real foreign runtime would refuse the callback; see "Shared
    /// storage and its aliasing rules" in `docs/ARCHITECTURE.md`); otherwise it
    /// does nothing.
    ///
    /// # Panics
    /// In the export-all mode only, if a guard taken on this thread is alive.
    #[inline]
    #[track_caller]
    pub fn callback_site(site: &str) {
        #[cfg(feature = "export-all")]
        {
            let n = imp::alive_on_this_thread();
            assert!(
                n == 0,
                "export-all mode: {n} native guard(s) alive on this thread at the foreign-callback \
                 site `{site}` (docs/ARCHITECTURE.md, shared storage and its aliasing rules)"
            );
        }
        #[cfg(not(feature = "export-all"))]
        let _ = site;
    }

    /// Native guards taken on the calling thread and still alive (always 0
    /// without the feature).
    #[must_use]
    pub fn guards_alive_on_this_thread() -> usize {
        #[cfg(feature = "export-all")]
        {
            imp::alive_on_this_thread()
        }
        #[cfg(not(feature = "export-all"))]
        {
            0
        }
    }
}

/// An exportable array: elements reachable natively only through guards.
pub struct SharedArray<T> {
    array: Array<T>,
}

impl<T> Default for SharedArray<T> {
    fn default() -> Self {
        Self::new(Array::default())
    }
}

impl<T> fmt::Debug for SharedArray<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SharedArray")
            .field("id", &self.array.id())
            .field("len", &self.array.len())
            .field("nb_component", &self.array.nb_component())
            .field("exported", &self.is_exported())
            .field("generation", &self.generation())
            .finish()
    }
}

impl<T> From<Array<T>> for SharedArray<T> {
    fn from(array: Array<T>) -> Self {
        Self::new(array)
    }
}

impl<T: Clone> Clone for SharedArray<T> {
    /// A copy of the values in a fresh, unexported allocation (reads them
    /// through [`SharedArray::borrow`]; panics like it on a protocol
    /// violation, as `RefCell::clone` does on a conflicting borrow).
    #[track_caller]
    fn clone(&self) -> Self {
        Self::new(self.borrow().clone())
    }
}

impl<T: PartialEq> PartialEq for SharedArray<T> {
    /// Value equality (id, shape and values, as [`Array`]); reads both
    /// arrays through [`SharedArray::borrow`] and panics like it on a
    /// protocol violation (as `RefCell::eq` on a conflicting borrow).
    #[track_caller]
    fn eq(&self, other: &Self) -> bool {
        *self.borrow() == *other.borrow()
    }
}

impl<T: Eq> Eq for SharedArray<T> {}

#[cold]
#[track_caller]
fn protocol_violation(id: &str, e: &Error) -> ! {
    panic!(
        "array '{id}' accessed outside the foreign-view protocol (exported, no native section \
         of its runtime): {e}"
    )
}

impl<T> SharedArray<T> {
    /// Wrap `array` (not exported yet).
    #[must_use]
    pub fn new(array: Array<T>) -> Self {
        #[cfg(feature = "export-all")]
        let array = {
            let mut array = array;
            array.data.export_all = true;
            array
        };
        Self { array }
    }

    /// Number of tuples (metadata: no guard needed).
    #[must_use]
    pub fn len(&self) -> usize {
        self.array.len()
    }

    /// `true` if there are no tuples.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.array.is_empty()
    }

    /// Components per tuple.
    #[must_use]
    pub fn nb_component(&self) -> usize {
        self.array.nb_component()
    }

    /// Identifier.
    #[must_use]
    pub fn id(&self) -> &str {
        self.array.id()
    }

    /// Change the identifier (metadata).
    pub fn set_id(&mut self, id: impl Into<Id>) {
        self.array.set_id(id);
    }

    /// `true` while a lease of the current allocation exists.
    #[must_use]
    pub fn is_exported(&self) -> bool {
        self.array.data.is_exported()
    }

    /// Allocation generation (bumped when an exported block is retired).
    #[must_use]
    pub fn generation(&self) -> u64 {
        self.array.data.generation()
    }

    #[inline]
    fn count(&self) -> Result<Option<GuardCount>> {
        match self.array.data.export_runtime() {
            #[cfg(feature = "export-all")]
            None if self.array.data.export_all => export_all::global_count().map(Some),
            None => Ok(None),
            Some(rt) => {
                rt.state.acquire_guard()?;
                Ok(Some(GuardCount::new(rt.clone())))
            }
        }
    }

    /// Shared native access. Fails while the array is exported and its
    /// runtime is not in a native section.
    #[inline]
    pub fn read(&self) -> Result<ArrayRef<'_, T>> {
        let count = self.count()?;
        Ok(ArrayRef {
            array: &self.array,
            _count: count,
        })
    }

    /// Exclusive native access (values and size). Same admission rule as
    /// [`SharedArray::read`].
    #[inline]
    pub fn write(&mut self) -> Result<ArrayMut<'_, T>> {
        let count = self.count()?;
        Ok(ArrayMut {
            array: &mut self.array,
            _count: count,
        })
    }

    /// Shared native access that cannot fail in a correct program (the
    /// [`std::cell::RefCell::borrow`] analogue, for owners whose accessors
    /// were infallible before their fields became exportable).
    ///
    /// Same admission rule as [`SharedArray::read`]: free while the array is
    /// not exported; while it is, only inside a native section of its
    /// runtime — which every binding entry point opens before touching
    /// native state (`docs/ARCHITECTURE.md`).
    ///
    /// # Panics
    /// If the array is exported and its runtime is not in a native section
    /// (a protocol violation of the code that exported it, like a double
    /// `RefCell` borrow), or the runtime's guard counter would overflow.
    #[track_caller]
    pub fn borrow(&self) -> ArrayRef<'_, T> {
        match self.read() {
            Ok(g) => g,
            Err(e) => protocol_violation(self.id(), &e),
        }
    }

    /// Exclusive native access that cannot fail in a correct program (the
    /// [`std::cell::RefCell::borrow_mut`] analogue); see
    /// [`SharedArray::borrow`].
    ///
    /// # Panics
    /// As [`SharedArray::borrow`].
    #[track_caller]
    pub fn borrow_mut(&mut self) -> ArrayMut<'_, T> {
        let id = self.array.id().to_owned();
        match self.write() {
            Ok(g) => g,
            Err(e) => protocol_violation(&id, &e),
        }
    }

    /// Unwrap into a plain array. An exported allocation is retired (its
    /// views keep the old block) and the values move to a fresh one, which
    /// needs native access; on error `self` is dropped (retired).
    pub fn into_inner(mut self) -> Result<Array<T>> {
        {
            let _count = self.count()?;
            self.array.data.detach_if_exported();
        }
        let array = std::mem::take(&mut self.array);
        #[cfg(feature = "export-all")]
        let array = {
            let mut array = array;
            array.data.export_all = false; // a plain array again
            array
        };
        Ok(array)
    }
}

impl<T: Copy> SharedArray<T> {
    /// Export the current allocation to `runtime` (NumPy/C view). The lease
    /// keeps the allocation alive; accesses through it follow the phase
    /// protocol. `&mut self` proves no native guard is alive.
    pub fn export(&mut self, runtime: &ForeignRuntime) -> Result<ExportLease<T>> {
        let nbc = self.array.nb_component();
        let tuples = self.array.len();
        self.array.data.export_lease(tuples, nbc, runtime)
    }

    /// `true` iff `lease` still views this array's live values and tuples.
    #[must_use]
    pub fn is_attached(&self, lease: &ExportLease<T>) -> bool {
        self.array.data.is_attached(lease, self.array.len())
            && lease.nb_component() == self.array.nb_component()
    }
}

/// Shared native guard of a [`SharedArray`]; dereferences to the array.
///
/// Its borrow lasts as long as its run-time count: to the end of its scope or
/// an explicit `drop` (explicit [`Drop`], D-071 (2)), so
/// holding it across a call that needs the owner mutably does not compile:
///
/// ```compile_fail,E0502
/// use akantu_core::array::Array;
/// use akantu_core::array_shared::SharedArray;
/// let mut a = SharedArray::new(Array::<f64>::zeros(2, 1));
/// let g = a.read().unwrap();
/// let first = g.as_slice()[0];
/// a.write().unwrap().fill(first); // the guard `g` is still alive
/// drop(g);
/// ```
///
/// Ending it first compiles:
///
/// ```
/// use akantu_core::array::Array;
/// use akantu_core::array_shared::SharedArray;
/// let mut a = SharedArray::new(Array::<f64>::zeros(2, 1));
/// let g = a.read().unwrap();
/// let first = g.as_slice()[0];
/// drop(g);
/// a.write().unwrap().fill(first);
/// ```
#[derive(Debug)]
pub struct ArrayRef<'a, T> {
    array: &'a Array<T>,
    _count: Option<GuardCount>,
}

/// Explicit (empty) `Drop`: drop-check then keeps the guard's borrow alive
/// until the guard is dropped, exactly as long as its count (the count itself
/// is released by the `GuardCount` field). Without it the borrow ended at the
/// guard's last use while the count lived on to the end of the scope, so a
/// foreign callback in between was refused at run time instead of the code
/// being rejected at compile time (D-071 (2)).
impl<T> Drop for ArrayRef<'_, T> {
    fn drop(&mut self) {}
}

impl<T> Deref for ArrayRef<'_, T> {
    type Target = Array<T>;
    fn deref(&self) -> &Array<T> {
        self.array
    }
}

impl<T: PartialEq> PartialEq<[T]> for ArrayRef<'_, T> {
    /// Values compared with a slice (tuple-major).
    fn eq(&self, other: &[T]) -> bool {
        self.array.as_slice() == other
    }
}

impl<T: PartialEq> PartialEq<&[T]> for ArrayRef<'_, T> {
    fn eq(&self, other: &&[T]) -> bool {
        self.array.as_slice() == *other
    }
}

impl<T: PartialEq, const N: usize> PartialEq<&[T; N]> for ArrayRef<'_, T> {
    fn eq(&self, other: &&[T; N]) -> bool {
        self.array.as_slice() == &other[..]
    }
}

impl<T: PartialEq, const N: usize> PartialEq<[T; N]> for ArrayRef<'_, T> {
    fn eq(&self, other: &[T; N]) -> bool {
        self.array.as_slice() == &other[..]
    }
}

impl<T: PartialEq> PartialEq<ArrayRef<'_, T>> for ArrayRef<'_, T> {
    /// Same values (tuple-major).
    fn eq(&self, other: &ArrayRef<'_, T>) -> bool {
        self.array.as_slice() == other.array.as_slice()
    }
}

impl<T: PartialEq> PartialEq<Vec<T>> for ArrayRef<'_, T> {
    fn eq(&self, other: &Vec<T>) -> bool {
        self.array.as_slice() == other.as_slice()
    }
}

impl<T: PartialEq> PartialEq<ArrayRef<'_, T>> for Vec<T> {
    fn eq(&self, other: &ArrayRef<'_, T>) -> bool {
        self.as_slice() == other.array.as_slice()
    }
}

impl<'a, T> From<&'a Array<T>> for ArrayRef<'a, T> {
    /// A guard over a plain array (never exported, so nothing to count):
    /// lets code holding plain arrays implement interfaces that hand out
    /// guards (e.g. `ModelServices::nodal_field`).
    fn from(array: &'a Array<T>) -> Self {
        Self {
            array,
            _count: None,
        }
    }
}

impl<T: Clone> ArrayRef<'_, T> {
    /// Copy of the values (tuple-major), as `<[T]>::to_vec` on the slice
    /// the accessor returned before the array became exportable.
    #[must_use]
    pub fn to_vec(&self) -> Vec<T> {
        self.array.as_slice().to_vec()
    }
}

impl<'a, T> ArrayRef<'a, T> {
    /// Narrow the guard to a part of the array (one tuple, a range of
    /// values, the whole storage) without copying: the
    /// [`std::cell::Ref::map`] analogue. The returned [`SliceRef`] keeps the
    /// guard's count, so the slice stays protected for as long as it lives.
    ///
    /// The closure is higher-ranked over the borrow, exactly as
    /// `Ref::map`'s: it cannot keep the array reference or any slice of it
    /// outside the returned guard (with the guard's own
    /// lifetime in the closure's signature, safe code could hold a `&[T]`
    /// past the guard while a foreign view wrote the same memory).
    ///
    /// ```compile_fail,E0521
    /// use akantu_core::array::Array;
    /// use akantu_core::array_shared::SharedArray;
    /// let a = SharedArray::new(Array::<f64>::zeros(4, 1));
    /// let mut smuggled: Option<&[f64]> = None;
    /// let guard = a.read().unwrap().map_slice(|arr| {
    ///     smuggled = Some(arr.as_slice()); // must not escape the guard
    ///     arr.as_slice()
    /// });
    /// drop(guard);
    /// assert_eq!(smuggled.unwrap().len(), 4);
    /// ```
    ///
    /// The same program without the escape compiles:
    ///
    /// ```
    /// use akantu_core::array::Array;
    /// use akantu_core::array_shared::SharedArray;
    /// let a = SharedArray::new(Array::<f64>::zeros(4, 1));
    /// let guard = a.read().unwrap().map_slice(|arr| arr.as_slice());
    /// assert_eq!(guard.len(), 4);
    /// ```
    #[must_use]
    pub fn map_slice(mut self, f: impl FnOnce(&Array<T>) -> &[T]) -> SliceRef<'a, T> {
        // the count moves into the new guard (`ArrayRef` has an explicit
        // `Drop`, so it is taken, not moved out; no `unsafe`)
        let count = self._count.take();
        SliceRef {
            slice: f(self.array),
            _count: count,
        }
    }

    /// Fallible [`ArrayRef::map_slice`] (e.g. a checked tuple access); on
    /// error the guard is released. Higher-ranked like `map_slice`:
    /// the array reference cannot leave the closure.
    ///
    /// ```compile_fail,E0521
    /// use akantu_core::array::Array;
    /// use akantu_core::array_shared::SharedArray;
    /// let a = SharedArray::new(Array::<f64>::zeros(3, 1));
    /// let mut smuggled: Option<&Array<f64>> = None;
    /// let guard = a
    ///     .read()
    ///     .unwrap()
    ///     .try_map_slice(|arr| {
    ///         smuggled = Some(arr); // must not escape the guard
    ///         Ok::<_, ()>(arr.as_slice())
    ///     })
    ///     .unwrap();
    /// drop(guard);
    /// assert_eq!(smuggled.unwrap().len(), 3);
    /// ```
    pub fn try_map_slice<E>(
        self,
        f: impl FnOnce(&Array<T>) -> std::result::Result<&[T], E>,
    ) -> std::result::Result<SliceRef<'a, T>, E> {
        let mut this = self;
        let count = this._count.take(); // released on error (dropped here)
        Ok(SliceRef {
            slice: f(this.array)?,
            _count: count,
        })
    }

    /// The one-tuple guard of tuple `i` (checked; see
    /// [`ArrayRef::map_slice`]).
    #[inline]
    pub fn into_tuple(self, i: usize) -> Result<SliceRef<'a, T>> {
        self.try_map_slice(|a| a.get_tuple(i))
    }

    /// The guard of the whole value storage (tuple-major).
    #[must_use]
    #[inline]
    pub fn into_slice(self) -> SliceRef<'a, T> {
        self.map_slice(Array::as_slice)
    }
}

/// A shared native guard narrowed to a slice of a [`SharedArray`] (one
/// tuple, e.g. an element's connectivity, or a whole one-component block,
/// e.g. a group's element list): dereferences to `[T]` without copying.
///
/// It is the non-allocating return type of the per-element accessors
/// (`Mesh::element_connectivity`, `ElementGroup::elements_of`).
/// Like [`ArrayRef`] it counts as a native guard while the array
/// is exported (ARCHITECTURE.md, "Shared storage and its aliasing rules"): it must not be held across a callback into foreign
/// code. [`SliceRef::empty`] and slices of plain arrays count nothing.
///
/// Like [`ArrayRef`] it has an explicit [`Drop`]: its borrow lasts as long
/// as its count.
///
/// ```compile_fail,E0502
/// use akantu_core::array::Array;
/// use akantu_core::array_shared::SharedArray;
/// let mut a = SharedArray::new(Array::<i32>::zeros(3, 1));
/// let s = a.read().unwrap().into_tuple(0).unwrap();
/// let v = s[0];
/// a.write().unwrap().fill(v); // the guard `s` is still alive
/// drop(s);
/// ```
pub struct SliceRef<'a, T> {
    slice: &'a [T],
    _count: Option<GuardCount>,
}

/// Explicit (empty) `Drop`, as for [`ArrayRef`].
impl<T> Drop for SliceRef<'_, T> {
    fn drop(&mut self) {}
}

impl<T> SliceRef<'_, T> {
    /// The empty slice (an absent block); holds no guard.
    #[must_use]
    pub fn empty() -> Self {
        Self {
            slice: &[],
            _count: None,
        }
    }

    /// The values (borrowed from the guard).
    #[must_use]
    pub fn as_slice(&self) -> &[T] {
        self.slice
    }
}

impl<'a, T> From<&'a [T]> for SliceRef<'a, T> {
    /// A guard over a plain slice (not exportable storage: nothing to
    /// count).
    fn from(slice: &'a [T]) -> Self {
        Self {
            slice,
            _count: None,
        }
    }
}

impl<T: fmt::Debug> fmt::Debug for SliceRef<'_, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self.slice, f)
    }
}

impl<T> Deref for SliceRef<'_, T> {
    type Target = [T];
    fn deref(&self) -> &[T] {
        self.slice
    }
}

impl<T> AsRef<[T]> for SliceRef<'_, T> {
    fn as_ref(&self) -> &[T] {
        self.slice
    }
}

impl<'b, T> IntoIterator for &'b SliceRef<'_, T> {
    type Item = &'b T;
    type IntoIter = std::slice::Iter<'b, T>;
    fn into_iter(self) -> Self::IntoIter {
        self.slice.iter()
    }
}

impl<T: PartialEq> PartialEq for SliceRef<'_, T> {
    fn eq(&self, other: &Self) -> bool {
        self.slice == other.slice
    }
}

impl<T: Eq> Eq for SliceRef<'_, T> {}

impl<T: PartialEq> PartialEq<[T]> for SliceRef<'_, T> {
    fn eq(&self, other: &[T]) -> bool {
        self.slice == other
    }
}

impl<T: PartialEq> PartialEq<&[T]> for SliceRef<'_, T> {
    fn eq(&self, other: &&[T]) -> bool {
        self.slice == *other
    }
}

impl<T: PartialEq, const N: usize> PartialEq<[T; N]> for SliceRef<'_, T> {
    fn eq(&self, other: &[T; N]) -> bool {
        self.slice == &other[..]
    }
}

impl<T: PartialEq, const N: usize> PartialEq<&[T; N]> for SliceRef<'_, T> {
    fn eq(&self, other: &&[T; N]) -> bool {
        self.slice == &other[..]
    }
}

impl<T: PartialEq> PartialEq<Vec<T>> for SliceRef<'_, T> {
    fn eq(&self, other: &Vec<T>) -> bool {
        self.slice == other.as_slice()
    }
}

impl<T: PartialEq> PartialEq<SliceRef<'_, T>> for Vec<T> {
    fn eq(&self, other: &SliceRef<'_, T>) -> bool {
        self.as_slice() == other.slice
    }
}

impl<T: PartialEq> PartialEq<SliceRef<'_, T>> for [T] {
    fn eq(&self, other: &SliceRef<'_, T>) -> bool {
        self == other.slice
    }
}

impl<T: PartialEq> PartialEq<SliceRef<'_, T>> for &[T] {
    fn eq(&self, other: &SliceRef<'_, T>) -> bool {
        *self == other.slice
    }
}

/// Exclusive native guard of a [`SharedArray`].
///
/// Dereferences to `&Array<T>` and forwards the mutating operations, but
/// never hands out `&mut Array<T>`: swapping the storage out of its
/// `SharedArray` would move an exported allocation into an un-guarded
/// array. [`ArrayMut::replace`] retires an exported block instead.
///
/// Like [`ArrayRef`] it has an explicit [`Drop`]: its borrow lasts as long
/// as its count.
///
/// ```compile_fail,E0502
/// use akantu_core::array::Array;
/// use akantu_core::array_shared::SharedArray;
/// let mut a = SharedArray::new(Array::<f64>::zeros(2, 1));
/// let mut w = a.write().unwrap();
/// w.fill(1.0);
/// assert_eq!(a.len(), 2); // the guard `w` is still alive
/// drop(w);
/// ```
#[derive(Debug)]
pub struct ArrayMut<'a, T> {
    array: &'a mut Array<T>,
    _count: Option<GuardCount>,
}

/// Explicit (empty) `Drop`, as for [`ArrayRef`].
impl<T> Drop for ArrayMut<'_, T> {
    fn drop(&mut self) {}
}

impl<T> Deref for ArrayMut<'_, T> {
    type Target = Array<T>;
    fn deref(&self) -> &Array<T> {
        self.array
    }
}

impl<T> Index<(usize, usize)> for ArrayMut<'_, T> {
    type Output = T;
    fn index(&self, ij: (usize, usize)) -> &T {
        &self.array[ij]
    }
}

impl<T> IndexMut<(usize, usize)> for ArrayMut<'_, T> {
    fn index_mut(&mut self, ij: (usize, usize)) -> &mut T {
        &mut self.array[ij]
    }
}

impl<T> ArrayMut<'_, T> {
    /// Mutable flat data.
    pub fn as_mut_slice(&mut self) -> &mut [T] {
        self.array.as_mut_slice()
    }

    /// Mutable borrowed view.
    pub fn view_mut(&mut self) -> ArrayViewMut<'_, T> {
        self.array.view_mut()
    }

    /// Tuple `i` (panics out of bounds, see [`Array::tuple_mut`]).
    #[track_caller]
    pub fn tuple_mut(&mut self, i: usize) -> &mut [T] {
        self.array.tuple_mut(i)
    }

    /// Checked tuple `i`.
    pub fn get_tuple_mut(&mut self, i: usize) -> Result<&mut [T]> {
        self.array.get_tuple_mut(i)
    }

    /// Mutable tuple iterator.
    pub fn tuples_mut(&mut self) -> TuplesMut<'_, T> {
        self.array.tuples_mut()
    }

    /// Fixed-size tuple view.
    pub fn as_fixed_mut<const N: usize>(&mut self) -> Result<&mut [[T; N]]> {
        self.array.as_fixed_mut::<N>()
    }

    /// See [`Array::try_reserve`] (relocates an exported block).
    pub fn try_reserve(&mut self, additional: usize) -> Result<()> {
        self.array.try_reserve(additional)
    }

    /// See [`Array::clear`] (detaches an export).
    pub fn clear(&mut self) {
        self.array.clear();
    }

    /// See [`Array::truncate`] (detaches an export on a real shrink).
    pub fn truncate(&mut self, new_len: usize) {
        self.array.truncate(new_len);
    }

    /// See [`Array::swap_remove_tuple`] (detaches an export).
    pub fn swap_remove_tuple(&mut self, i: usize) -> Result<()> {
        self.array.swap_remove_tuple(i)
    }

    /// Replace the whole array by `new`, returning the old values. An
    /// exported block is retired (its views go stale with the old values)
    /// and the returned array owns a fresh allocation.
    pub fn replace(&mut self, new: Array<T>) -> Array<T> {
        let mut old = std::mem::replace(&mut *self.array, new);
        old.data.detach_if_exported();
        #[cfg(feature = "export-all")]
        {
            // the returned array is plain, the installed one shared
            old.data.export_all = false;
            self.array.data.export_all = true;
        }
        old
    }
}

impl<T: Clone> ArrayMut<'_, T> {
    /// See [`Array::try_resize`].
    pub fn try_resize(&mut self, new_len: usize, fill: T) -> Result<()> {
        self.array.try_resize(new_len, fill)
    }

    /// See [`Array::resize`] (infallible; panics like it on allocation
    /// failure or size overflow — use `try_resize` for user sizes).
    #[track_caller]
    pub fn resize(&mut self, new_len: usize, fill: T) {
        self.array.resize(new_len, fill);
    }

    /// See [`Array::try_push_tuple`].
    pub fn try_push_tuple(&mut self, tuple: &[T]) -> Result<()> {
        self.array.try_push_tuple(tuple)
    }

    /// See [`Array::push_tuple`] (pre-validated tuple size).
    #[track_caller]
    pub fn push_tuple(&mut self, tuple: &[T]) {
        self.array.push_tuple(tuple);
    }

    /// See [`Array::push_value`].
    pub fn push_value(&mut self, value: T) {
        self.array.push_value(value);
    }

    /// See [`Array::fill`].
    pub fn fill(&mut self, value: T) {
        self.array.fill(value);
    }

    /// See [`Array::fill_tuples`].
    pub fn fill_tuples(&mut self, tuple: &[T]) -> Result<()> {
        self.array.fill_tuples(tuple)
    }

    /// See [`Array::append`].
    pub fn append(&mut self, other: &Array<T>) -> Result<()> {
        self.array.append(other)
    }

    /// See [`Array::copy_from`] (in place for equal lengths: views stay
    /// attached through a history restore).
    pub fn copy_from(&mut self, other: &Array<T>) -> Result<()> {
        self.array.copy_from(other)
    }
}

impl<T> ArrayMut<'_, T> {
    /// See [`Array::tuple_matrix_mut`] (any `T`, like the array's own
    /// accessor; for `Real` the type is
    /// [`Matrix<R, C>`](crate::tensor::Matrix)).
    pub fn tuple_matrix_mut<const R: usize, const C: usize>(
        &mut self,
        i: usize,
    ) -> Result<&mut [[T; R]; C]> {
        self.array.tuple_matrix_mut::<R, C>(i)
    }
}

impl<T: Clone + Default> ArrayMut<'_, T> {
    /// See [`Array::zero`].
    pub fn zero(&mut self) {
        self.array.zero();
    }
}

impl ArrayMut<'_, Real> {
    /// See [`Array::add_assign`].
    pub fn add_assign(&mut self, other: &Array<Real>) -> Result<()> {
        self.array.add_assign(other)
    }

    /// See [`Array::sub_assign`].
    pub fn sub_assign(&mut self, other: &Array<Real>) -> Result<()> {
        self.array.sub_assign(other)
    }

    /// See [`Array::scale`].
    pub fn scale(&mut self, alpha: Real) {
        self.array.scale(alpha);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn exported(n: usize) -> (ForeignRuntime, SharedArray<f64>, ExportLease<f64>) {
        let rt = ForeignRuntime::new();
        let mut a = SharedArray::new(Array::zeros(n, 1));
        let lease = a.export(&rt).unwrap();
        (rt, a, lease)
    }

    #[test]
    fn unexported_arrays_need_no_runtime() {
        let mut a = SharedArray::new(Array::<f64>::zeros(2, 1));
        a.write().unwrap().as_mut_slice()[1] = 3.0;
        assert_eq!(a.read().unwrap().as_slice(), &[0.0, 3.0]);
        assert_eq!(a.into_inner().unwrap().as_slice(), &[0.0, 3.0]);
    }

    #[test]
    fn exported_arrays_need_a_native_section() {
        let (rt, mut a, lease) = exported(2);
        assert!(a.read().is_err() && a.write().is_err(), "foreign phase");
        let mut interp = rt.enter().unwrap();
        lease.check_foreign_access(&interp).unwrap();
        {
            let _s = interp.native().unwrap();
            a.write().unwrap().fill(1.0);
            let r = a.read().unwrap();
            assert_eq!(rt.native_guards(), 1);
            assert!(rt.callback().is_err(), "guard alive across a callback");
            drop(r);
            let cb = rt.callback().unwrap();
            lease.check_foreign_access(&cb).unwrap();
            assert!(a.read().is_err(), "foreign phase inside the callback");
        }
        lease.check_foreign_access(&interp).unwrap();
        assert!(rt.is_foreign_phase() && rt.native_guards() == 0);
    }

    #[test]
    fn interpreter_is_exclusive_and_reentrant_only_when_foreign() {
        let rt = ForeignRuntime::new();
        let mut i1 = rt.enter().unwrap();
        let i2 = rt.enter().unwrap(); // re-entrant, foreign phase
        let rt2 = rt.clone();
        assert!(
            std::thread::spawn(move || rt2.enter().is_err())
                .join()
                .unwrap()
        );
        {
            let _s = i1.native().unwrap();
            assert!(rt.enter().is_err(), "native code re-entering");
            assert!(i2.check_foreign().is_err(), "other token sees the phase");
        }
        drop(i2);
        drop(i1);
        let rt3 = rt.clone();
        assert!(
            std::thread::spawn(move || rt3.enter().is_ok())
                .join()
                .unwrap()
        );
    }

    #[test]
    fn leaked_guards_poison_until_dropped() {
        let (rt, a, lease) = exported(1);
        let mut interp = rt.enter().unwrap();
        let s = interp.native().unwrap();
        let g = a.read().unwrap();
        drop(s);
        assert!(rt.is_poisoned());
        assert!(lease.check_foreign_access(&interp).is_err());
        assert!(a.read().is_err(), "no section: no new guards");
        drop(g);
        assert!(!rt.is_poisoned());
        lease.check_foreign_access(&interp).unwrap();
        // A forgotten guard poisons for good; a new section adopts it.
        // (`ManuallyDrop` = `mem::forget` that can be undone at the end, so
        // Miri runs without leak suppression.)
        let s = interp.native().unwrap();
        let forgotten = std::mem::ManuallyDrop::new(a.read().unwrap());
        drop(s);
        assert!(lease.check_foreign_access(&interp).is_err());
        {
            let _s = interp.native().unwrap();
            assert!(a.read().is_ok(), "adopted by the new section");
            drop(std::mem::ManuallyDrop::into_inner(forgotten));
        }
        lease.check_foreign_access(&interp).unwrap();
    }

    #[test]
    fn leaked_sections_and_callbacks_fail_closed() {
        let (rt, a, lease) = exported(1);
        let mut interp = rt.enter().unwrap();
        std::mem::forget(interp.native().unwrap());
        assert!(lease.check_foreign_access(&interp).is_err());
        assert!(interp.native().is_err());
        // A section holds no resource, so forgetting it leaks nothing; a
        // forgotten callback is emulated with `ManuallyDrop` (see above).
        let cb = std::mem::ManuallyDrop::new(rt.callback().unwrap()); // owner thread
        assert!(a.read().is_err(), "stuck foreign: native refused");
        lease.check_foreign_access(&interp).unwrap();
        drop(std::mem::ManuallyDrop::into_inner(cb));
        assert!(lease.check_foreign_access(&interp).is_err(), "native again");
    }

    /// The test-only export-all mode: with the feature, every
    /// `SharedArray` retires its block on a shrink and counts its guards on
    /// the thread that took them; `is_exported` still reports real leases
    /// only, and arrays taken out are plain again. Without the feature the
    /// same program observes no export at all.
    #[test]
    fn export_all_mode_treats_every_shared_array_as_exported() {
        let on = export_all::ENABLED;
        let mut a = SharedArray::new(Array::from_vec(vec![1.0, 2.0, 3.0], 1).unwrap());
        let before = export_all::guards_alive_on_this_thread();
        {
            let g = a.read().unwrap();
            assert_eq!(
                export_all::guards_alive_on_this_thread(),
                before + usize::from(on)
            );
            // released on the counter of the thread that took it
            std::thread::scope(|s| s.spawn(move || drop(g)).join().unwrap());
        }
        assert_eq!(export_all::guards_alive_on_this_thread(), before);
        a.write().unwrap().truncate(2);
        assert_eq!(a.generation() > 0, on, "a shrink retires the block");
        assert!(!a.is_exported(), "is_exported reports real leases only");
        let mut b = SharedArray::new(Array::from_vec(vec![5.0], 1).unwrap());
        let old = b
            .write()
            .unwrap()
            .replace(Array::from_vec(vec![6.0, 7.0], 1).unwrap());
        b.write().unwrap().truncate(1);
        assert_eq!(b.generation() > 0, on, "the installed array is shared");
        let mut plain = a.into_inner().unwrap();
        let mut old = old;
        let generations = (plain.data.generation(), old.data.generation());
        plain.truncate(1);
        old.truncate(0);
        assert_eq!(
            (plain.data.generation(), old.data.generation()),
            generations,
            "plain arrays never retire"
        );
        assert_eq!(plain.as_slice(), &[1.0]);
    }

    /// Negative control of the export-all callback check:
    /// with the feature, a guard alive at a foreign-callback site
    /// panics; once it is dropped (or without the feature) the site passes.
    #[test]
    fn callback_site_refuses_a_live_guard_in_export_all_mode() {
        let a = SharedArray::new(Array::from_vec(vec![1.0, 2.0], 1).unwrap());
        let g = a.read().unwrap();
        let held = std::panic::catch_unwind(|| export_all::callback_site("test: guard alive"));
        assert_eq!(held.is_err(), export_all::ENABLED);
        drop(g);
        export_all::callback_site("test: no guard alive");
    }

    #[test]
    fn borrow_and_clone_follow_the_admission_rule() {
        let mut a = SharedArray::new(Array::from_vec(vec![1.0, 2.0], 1).unwrap());
        a.borrow_mut().as_mut_slice()[0] = 3.0;
        assert_eq!(a.borrow().as_slice(), &[3.0, 2.0]);
        let rt = ForeignRuntime::new();
        let lease = a.export(&rt).unwrap();
        let outside = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| a.borrow().len()));
        assert!(
            outside.is_err(),
            "exported, foreign phase: protocol violation"
        );
        let mut interp = rt.enter().unwrap();
        let b = {
            let _s = interp.native().unwrap();
            let b = a.clone();
            assert!(b == a);
            b
        };
        assert!(!b.is_exported(), "a clone is a fresh allocation");
        assert_eq!(b.borrow().as_slice(), &[3.0, 2.0]);
        lease.check_foreign_access(&interp).unwrap();
    }

    #[test]
    fn leases_know_when_the_array_grew_within_capacity() {
        let rt = ForeignRuntime::new();
        let mut a = SharedArray::new(Array::<f64>::zeros(2, 1));
        a.write().unwrap().try_reserve(4).unwrap();
        let lease = a.export(&rt).unwrap();
        assert!(lease.covers_live_tuples());
        let mut interp = rt.enter().unwrap();
        {
            let _s = interp.native().unwrap();
            a.write().unwrap().push_value(1.0);
            assert!(a.is_attached(&lease), "growth within capacity: attached");
        }
        assert!(!lease.covers_live_tuples(), "old prefix only");
        let l2 = {
            let _s = interp.native().unwrap();
            a.export(&rt).unwrap()
        };
        assert!(l2.covers_live_tuples() && l2.len() == 3);
        {
            // a shrink retires the block: the stale lease keeps its prefix
            let _s = interp.native().unwrap();
            a.write().unwrap().truncate(1);
        }
        assert!(l2.is_retired());
        lease.check_foreign_access(&interp).unwrap();
    }

    #[test]
    fn replace_and_into_inner_retire_the_export() {
        let (rt, mut a, lease) = exported(2);
        let mut interp = rt.enter().unwrap();
        let _s = interp.native().unwrap();
        let old = a.write().unwrap().replace(Array::zeros(3, 1));
        assert!(lease.is_retired() && !a.is_attached(&lease));
        assert_eq!(old.len(), 2);
        let mut b = SharedArray::new(Array::<f64>::zeros(1, 1));
        let l2 = b.export(&rt).unwrap();
        let plain = b.into_inner().unwrap();
        assert!(l2.is_retired() && plain.len() == 1);
    }
}
