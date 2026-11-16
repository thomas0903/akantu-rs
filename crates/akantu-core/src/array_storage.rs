// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs design (decision record D-024):
// replaces the raw pointer export of upstream python/py_aka_array.hh:126
// (`array_type_t({size, nbc}, src.data(), base)`), which dangles when the
// Array is resized, restored or destroyed (UD-024 family, UD-085).

//! Export-capable storage of [`crate::Array`] (foreign-view contract,
//! `docs/ARCHITECTURE.md`).
//!
//! This module owns the **lifetime** half of the contract; the **access**
//! half (who may touch exported values when) is the phase protocol of
//! [`crate::array_shared`].
//!
//! * **Owner.** The values live in one heap allocation owned by an
//!   `ArrayStorage` (private to the crate: only
//!   [`crate::array_shared::SharedArray`] can export one). Foreign code
//!   (NumPy, C) holds an [`ExportLease`]: pointer, shape, generation and a
//!   keep-alive anchor that names the [`ForeignRuntime`] whose protocol
//!   governs every access through the pointer.
//! * **Exported** means "at least one lease is alive" (an atomic lease
//!   counter in the anchor, decremented with `Release` when a lease drops
//!   and read with `Acquire`).
//! * **No reallocation under an export.** While exported, an operation that
//!   would grow beyond the capacity, **shrink** (`truncate`, `clear`,
//!   erase, a shorter copy), `into_vec`, a replacement or the destruction of
//!   the owner moves the values into a **fresh** allocation and *retires*
//!   the old block into the anchor, where it stays valid until the last lease
//!   drops. Value writes, `fill`, equal-length copies (history restore) and
//!   growth **within** capacity keep the allocation. Consequently a lease
//!   never covers dead values of its allocation.
//! * **Generation.** Increases whenever an exported allocation is retired. A
//!   lease is *attached* iff its anchor is the storage's current one, the
//!   block is not retired, its generation is current and it covers only live
//!   values **and live tuples**. Detachment is permanent ("stale-but-safe"):
//!   reads see the values at retirement time, writes no longer reach the
//!   array.
//! * **Shape**. A lease records its **tuple count**
//!   independently of its value count: a zero-component array `(n, 0)`
//!   exports shape `(n, 0)`, never `(0, 0)`. The owning
//!   [`crate::Array`] retires an exported block on **every** decrease of its
//!   tuple count, even when no value is removed (zero components), so
//!   staleness stays monotone for those arrays too.
//!
//! This module contains **no `unsafe`** and never dereferences a lease
//! pointer; that is done by the audited binding layer
//! (`akantu-python::foreign_view`) after
//! [`ExportLease::check_foreign_access`] succeeded.

use std::fmt;
use std::ops::{Deref, DerefMut};
use std::ptr::NonNull;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use crate::array_shared::{ForeignRuntime, Interpreter};
use crate::error::{Error, Result};

/// Keep-alive anchor of one exported allocation.
struct Anchor<T> {
    /// The retired block (set once, when the owner stops using it).
    retired: Mutex<Option<Vec<T>>>,
    /// Number of live leases.
    leases: AtomicUsize,
    /// Tuple count of the owning array while this anchor is its current
    /// one (set at every export, updated by the owner on every tuple-count
    /// change; frozen once the block is retired). Lets a cached lease tell
    /// that the array grew within capacity after it was taken.
    live_tuples: AtomicUsize,
    /// Runtime whose phase protocol governs foreign accesses.
    runtime: ForeignRuntime,
}

impl<T> Anchor<T> {
    fn retire(&self, v: Vec<T>) {
        match self.retired.lock() {
            Ok(mut r) => *r = Some(v),
            // Never free memory a lease may still point to: a poisoned
            // anchor (impossible: nothing panics under this lock) still
            // stores the block.
            Err(p) => *p.into_inner() = Some(v),
        }
    }

    fn is_retired(&self) -> bool {
        match self.retired.lock() {
            Ok(r) => r.is_some(),
            Err(p) => p.into_inner().is_some(),
        }
    }

    fn is_exported(&self) -> bool {
        // Acquire pairs with the Release decrement of the last dropped
        // lease: whatever that lease's holder did happens-before the
        // caller's subsequent un-guarded use of the allocation.
        self.leases.load(Ordering::Acquire) > 0
    }
}

/// Owner of an array's values (a `Vec<T>` plus a lazily created export
/// anchor). Dereferences to `[T]`.
pub(crate) struct ArrayStorage<T> {
    vec: Vec<T>,
    anchor: Option<Arc<Anchor<T>>>,
    generation: u64,
    /// Test-only export-all mode (cargo feature `export-all`, never enabled
    /// by a product crate; `docs/ARCHITECTURE.md`): set while the
    /// storage is owned by a `SharedArray`, which then retires its blocks and
    /// counts its guards as if it were exported.
    #[cfg(feature = "export-all")]
    pub(crate) export_all: bool,
}

/// A foreign export of an allocation: raw pointer + shape + keep-alive.
///
/// Created only by [`crate::array_shared::SharedArray::export`]. The
/// pointer may be dereferenced only by code that obtained
/// `Ok(())` from [`ExportLease::check_foreign_access`] and keeps the
/// [`Interpreter`] borrowed for the duration of the access (see
/// [`crate::array_shared`] for the guarantee). The lease is deliberately
/// `!Send`/`!Sync` (raw pointer); bindings that move it across threads wrap
/// it under their own audited `unsafe impl` whose justification is that
/// protocol.
pub struct ExportLease<T> {
    ptr: NonNull<T>,
    values: usize,
    /// Tuple count at export (kept separately: `values / nb_component` is
    /// undefined for zero-component arrays).
    tuples: usize,
    nb_component: usize,
    generation: u64,
    anchor: Arc<Anchor<T>>,
}

impl<T> fmt::Debug for ExportLease<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ExportLease")
            .field("values", &self.values)
            .field("tuples", &self.tuples)
            .field("nb_component", &self.nb_component)
            .field("generation", &self.generation)
            .field("retired", &self.anchor.is_retired())
            .finish()
    }
}

impl<T> Clone for ExportLease<T> {
    /// Another lease of the same allocation (a NumPy view of a view).
    fn clone(&self) -> Self {
        self.anchor.leases.fetch_add(1, Ordering::Relaxed);
        Self {
            ptr: self.ptr,
            values: self.values,
            tuples: self.tuples,
            nb_component: self.nb_component,
            generation: self.generation,
            anchor: Arc::clone(&self.anchor),
        }
    }
}

impl<T> Drop for ExportLease<T> {
    fn drop(&mut self) {
        self.anchor.leases.fetch_sub(1, Ordering::Release);
    }
}

impl<T> ExportLease<T> {
    /// Base pointer of the exported values (the allocation stays valid for
    /// `nb_values()` values as long as the lease lives). Obtaining it is
    /// safe; dereferencing it is governed by
    /// [`ExportLease::check_foreign_access`].
    #[must_use]
    pub fn as_ptr(&self) -> *mut T {
        self.ptr.as_ptr()
    }

    /// Number of exported values.
    #[must_use]
    pub fn nb_values(&self) -> usize {
        self.values
    }

    /// Number of exported tuples (recorded at export, also for
    /// zero-component arrays).
    #[must_use]
    pub fn len(&self) -> usize {
        self.tuples
    }

    /// `true` if no tuple is exported.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.tuples == 0
    }

    /// Components per tuple.
    #[must_use]
    pub fn nb_component(&self) -> usize {
        self.nb_component
    }

    /// Generation of the storage at export time.
    #[must_use]
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// `true` once the owner retired the exported allocation (the lease is
    /// then detached for good).
    #[must_use]
    pub fn is_retired(&self) -> bool {
        self.anchor.is_retired()
    }

    /// `true` iff the lease still covers every tuple the owning array had
    /// when it last changed its tuple count on this allocation: `false`
    /// after a growth within capacity (the lease then views only the old
    /// prefix, ARCHITECTURE.md, "Shared storage and its aliasing rules"). Holders that cannot re-export (a getter inside a
    /// callback of the running call) refuse such a lease.
    /// Not meaningful once the lease is retired.
    #[must_use]
    pub fn covers_live_tuples(&self) -> bool {
        // Acquire pairs with the owner's Release store (both happen around
        // phase changes that already order them; explicit for clarity)
        self.anchor.live_tuples.load(Ordering::Acquire) <= self.tuples
    }

    /// The runtime governing accesses through this lease.
    #[must_use]
    pub fn runtime(&self) -> &ForeignRuntime {
        &self.anchor.runtime
    }

    /// Admission check for one foreign access through [`Self::as_ptr`].
    ///
    /// `Ok(())` guarantees — for as long as `interp` stays borrowed — that
    /// `interp` belongs to this lease's runtime and the runtime is in its
    /// **foreign phase**: no native element guard of any allocation exported
    /// to it is alive or can be acquired, and no other thread can perform
    /// foreign accesses on it ([`crate::array_shared`]). The allocation
    /// itself is kept alive by the lease.
    pub fn check_foreign_access(&self, interp: &Interpreter) -> Result<()> {
        if !interp.runtime().same_as(&self.anchor.runtime) {
            return Err(Error::invalid(
                "array view accessed with the interpreter of another runtime",
            ));
        }
        interp.check_foreign()
    }
}

impl<T> Default for ArrayStorage<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> From<Vec<T>> for ArrayStorage<T> {
    fn from(vec: Vec<T>) -> Self {
        Self {
            vec,
            anchor: None,
            generation: 0,
            #[cfg(feature = "export-all")]
            export_all: false,
        }
    }
}

impl<T> Deref for ArrayStorage<T> {
    type Target = [T];
    fn deref(&self) -> &[T] {
        &self.vec
    }
}

impl<T> DerefMut for ArrayStorage<T> {
    fn deref_mut(&mut self) -> &mut [T] {
        &mut self.vec
    }
}

impl<T: Clone> Clone for ArrayStorage<T> {
    /// A clone is a new, unexported allocation.
    fn clone(&self) -> Self {
        Self::from(self.vec.clone())
    }
}

impl<T: PartialEq> PartialEq for ArrayStorage<T> {
    fn eq(&self, other: &Self) -> bool {
        self.vec == other.vec
    }
}

impl<T: fmt::Debug> fmt::Debug for ArrayStorage<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.vec.fmt(f)
    }
}

impl<T> Drop for ArrayStorage<T> {
    fn drop(&mut self) {
        // Moves the `Vec` header into the anchor; no element is touched, so
        // no phase is required.
        if self.is_exported() {
            self.retire_current(Vec::new());
        }
    }
}

impl<T> ArrayStorage<T> {
    /// Empty storage.
    #[must_use]
    pub(crate) const fn new() -> Self {
        Self {
            vec: Vec::new(),
            anchor: None,
            generation: 0,
            #[cfg(feature = "export-all")]
            export_all: false,
        }
    }

    /// `true` while a lease of the current allocation exists.
    #[must_use]
    pub(crate) fn is_exported(&self) -> bool {
        self.anchor.as_ref().is_some_and(|a| a.is_exported())
    }

    /// `true` if a change of the allocation must retire it: a lease of it
    /// exists, or (test-only export-all mode) the storage belongs to a
    /// `SharedArray`, which the mode treats as exported.
    #[inline]
    fn retires_on_change(&self) -> bool {
        #[cfg(feature = "export-all")]
        if self.export_all {
            return true;
        }
        self.is_exported()
    }

    /// `true` if neither a lease of the allocation exists nor the test-only
    /// export-all mode applies: un-guarded access and an exchange of the
    /// allocation are unobservable by foreign code.
    #[inline]
    pub(crate) fn is_plain(&self) -> bool {
        !self.retires_on_change()
    }

    /// Exchange the values (the allocations) of `self` and `other` when both
    /// are [plain](Self::is_plain); `false`, with nothing changed, otherwise.
    /// No value is copied; the generations and export anchors stay with
    /// their storages (neither allocation has a live lease).
    pub(crate) fn try_swap_values(&mut self, other: &mut ArrayStorage<T>) -> bool {
        if !self.is_plain() || !other.is_plain() {
            return false;
        }
        std::mem::swap(&mut self.vec, &mut other.vec);
        true
    }

    /// The runtime the current allocation is exported to (`None` if not
    /// exported).
    #[must_use]
    #[inline]
    pub(crate) fn export_runtime(&self) -> Option<&ForeignRuntime> {
        self.anchor
            .as_ref()
            .filter(|a| a.is_exported())
            .map(|a| &a.runtime)
    }

    /// Allocation generation (see module docs).
    #[must_use]
    pub(crate) fn generation(&self) -> u64 {
        self.generation
    }

    /// Capacity in values.
    #[must_use]
    pub(crate) fn capacity(&self) -> usize {
        self.vec.capacity()
    }

    /// Install `fresh` as the current allocation and retire the old one into
    /// the anchor (callers check `is_exported()` first).
    fn retire_current(&mut self, fresh: Vec<T>) {
        let old = std::mem::replace(&mut self.vec, fresh);
        if let Some(a) = self.anchor.take() {
            a.retire(old);
        }
        self.generation += 1;
    }

    /// If exported, move the values into a fresh allocation of the same
    /// capacity and retire the exported block. Returns `true` if it did.
    ///
    /// # Panics
    /// Aborts/panics like `Vec::with_capacity` if the fresh allocation fails
    /// (infallible shrinking operations; use the `try_*` growth paths for
    /// user-controlled sizes).
    pub(crate) fn detach_if_exported(&mut self) -> bool {
        if !self.retires_on_change() {
            return false;
        }
        let mut fresh = Vec::with_capacity(self.vec.capacity());
        // Moves the values; the old block keeps its bytes for the leases.
        fresh.append(&mut self.vec);
        self.retire_current(fresh);
        true
    }

    /// Make room for `additional` values; under an export, a growth beyond
    /// the capacity relocates into a fresh allocation (never `realloc`).
    /// No state change on error.
    pub(crate) fn try_reserve(
        &mut self,
        additional: usize,
    ) -> std::result::Result<(), std::collections::TryReserveError> {
        if self.vec.capacity() - self.vec.len() >= additional {
            return Ok(());
        }
        if self.retires_on_change() {
            let target = self
                .vec
                .len()
                .checked_add(additional)
                .map_or(usize::MAX, |n| n.max(self.vec.capacity().saturating_mul(2)));
            let mut fresh = Vec::new();
            fresh.try_reserve_exact(target)?;
            fresh.append(&mut self.vec);
            self.retire_current(fresh);
            Ok(())
        } else {
            self.vec.try_reserve(additional)
        }
    }

    /// Remove all values (detaches an export, see [`Self::truncate`]).
    pub(crate) fn clear(&mut self) {
        self.truncate(0);
    }

    /// Keep the first `n` values. Under an export a real shrink first moves
    /// the values to a fresh allocation, so leases keep the pre-shrink
    /// values and never cover dead ones.
    pub(crate) fn truncate(&mut self, n: usize) {
        if n < self.vec.len() {
            self.detach_if_exported();
            self.vec.truncate(n);
        }
    }

    /// Take the values out. Under an export the lease keeps the old block:
    /// the values are moved to a fresh vector.
    pub(crate) fn into_vec(mut self) -> Vec<T> {
        self.detach_if_exported();
        self.anchor = None;
        std::mem::take(&mut self.vec)
    }

    /// Export the current allocation to `runtime` with foreign shape
    /// `(tuples, nb_component)` (`tuples · nb_component` must equal the
    /// value count). Fails if it is currently exported to another runtime.
    pub(crate) fn export_lease(
        &mut self,
        tuples: usize,
        nb_component: usize,
        runtime: &ForeignRuntime,
    ) -> Result<ExportLease<T>> {
        if tuples.checked_mul(nb_component) != Some(self.vec.len()) {
            return Err(Error::invalid(format!(
                "export shape ({tuples}, {nb_component}) does not match {} values",
                self.vec.len()
            )));
        }
        let reuse = match &self.anchor {
            Some(a) if a.runtime.same_as(runtime) => true,
            Some(a) if a.is_exported() => {
                return Err(Error::invalid(
                    "array is already exported to another foreign runtime",
                ));
            }
            _ => false,
        };
        if !reuse {
            self.anchor = Some(Arc::new(Anchor {
                retired: Mutex::new(None),
                leases: AtomicUsize::new(0),
                live_tuples: AtomicUsize::new(tuples),
                runtime: runtime.clone(),
            }));
        }
        let anchor = match &self.anchor {
            Some(a) => Arc::clone(a),
            None => unreachable!("anchor installed above"),
        };
        anchor.live_tuples.store(tuples, Ordering::Release);
        anchor.leases.fetch_add(1, Ordering::Relaxed);
        Ok(ExportLease {
            ptr: NonNull::new(self.vec.as_mut_ptr()).unwrap_or(NonNull::dangling()),
            values: self.vec.len(),
            tuples,
            nb_component,
            generation: self.generation,
            anchor,
        })
    }

    /// Record the owner's new tuple count in the current anchor (if any):
    /// called by [`crate::Array`] on every tuple-count change.
    pub(crate) fn note_tuples(&self, tuples: usize) {
        if let Some(a) = &self.anchor {
            a.live_tuples.store(tuples, Ordering::Release);
        }
    }

    /// `true` iff `lease` views the current allocation at the current
    /// generation and covers only live values and live tuples (`tuples`:
    /// the owner's current tuple count).
    #[must_use]
    pub(crate) fn is_attached(&self, lease: &ExportLease<T>, tuples: usize) -> bool {
        match &self.anchor {
            Some(a) => {
                Arc::ptr_eq(a, &lease.anchor)
                    && !lease.anchor.is_retired()
                    && lease.generation == self.generation
                    && lease.values <= self.vec.len()
                    && lease.tuples <= tuples
            }
            None => false,
        }
    }
}

impl<T: Clone> ArrayStorage<T> {
    /// Resize to `n` values, new ones set to `value` (growth follows
    /// [`ArrayStorage::try_reserve`], shrinking [`ArrayStorage::truncate`]).
    pub(crate) fn try_resize(
        &mut self,
        n: usize,
        value: T,
    ) -> std::result::Result<(), std::collections::TryReserveError> {
        if n < self.vec.len() {
            self.truncate(n);
        } else {
            self.try_reserve(n - self.vec.len())?;
            self.vec.resize(n, value);
        }
        Ok(())
    }

    /// Append values (capacity growth follows [`ArrayStorage::try_reserve`]).
    pub(crate) fn try_extend_from_slice(
        &mut self,
        values: &[T],
    ) -> std::result::Result<(), std::collections::TryReserveError> {
        self.try_reserve(values.len())?;
        self.vec.extend_from_slice(values);
        Ok(())
    }

    /// Replace the values by a copy of `other`: in place if the lengths are
    /// equal (history restore keeps views attached); otherwise growth/shrink
    /// rules apply.
    pub(crate) fn try_clone_from_slice(
        &mut self,
        other: &[T],
    ) -> std::result::Result<(), std::collections::TryReserveError> {
        if other.len() == self.vec.len() {
            self.vec.clone_from_slice(other);
            return Ok(());
        }
        if other.len() > self.vec.len() {
            self.try_reserve(other.len() - self.vec.len())?;
        }
        self.truncate(other.len());
        let n = self.vec.len();
        self.vec.clone_from_slice(&other[..n]);
        self.vec.extend_from_slice(&other[n..]);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rt() -> ForeignRuntime {
        ForeignRuntime::new()
    }

    /// Export with the tuple count of a plain `nbc`-component array.
    fn export<T>(
        s: &mut ArrayStorage<T>,
        nbc: usize,
        r: &ForeignRuntime,
    ) -> Result<ExportLease<T>> {
        let tuples = s.len() / nbc;
        s.export_lease(tuples, nbc, r)
    }

    fn attached<T>(s: &ArrayStorage<T>, l: &ExportLease<T>) -> bool {
        s.is_attached(l, s.len() / l.nb_component())
    }

    /// The lease keeps the tuple count of a zero-component array
    /// and rejects shapes that do not match the values.
    #[test]
    fn zero_component_leases_keep_their_tuple_count() {
        let r = rt();
        let mut s: ArrayStorage<f64> = ArrayStorage::new();
        let l = s.export_lease(3, 0, &r).unwrap();
        assert_eq!((l.len(), l.nb_component(), l.nb_values()), (3, 0, 0));
        assert!(!l.is_empty());
        assert!(s.is_attached(&l, 3) && s.is_attached(&l, 4));
        assert!(!s.is_attached(&l, 2), "fewer live tuples than exported");
        let mut t: ArrayStorage<f64> = vec![1.0, 2.0].into();
        assert!(t.export_lease(3, 1, &r).is_err());
        assert!(t.export_lease(1, 0, &r).is_err());
    }

    #[test]
    fn unexported_storage_behaves_like_a_vec() {
        let mut s: ArrayStorage<f64> = vec![1.0, 2.0].into();
        s.try_extend_from_slice(&[3.0]).unwrap();
        s.try_resize(5, 0.5).unwrap();
        assert_eq!(&*s, &[1.0, 2.0, 3.0, 0.5, 0.5]);
        assert!(!s.is_exported());
        assert_eq!(s.generation(), 0);
        s.truncate(1);
        assert_eq!(s.generation(), 0, "no export, no relocation");
        assert_eq!(s.clone().into_vec(), vec![1.0]);
    }

    #[test]
    fn growth_under_export_relocates_and_retires() {
        let r = rt();
        let mut s: ArrayStorage<f64> = Vec::with_capacity(2).into();
        s.try_extend_from_slice(&[1.0, 2.0]).unwrap();
        let lease = export(&mut s, 1, &r).unwrap();
        assert!(s.is_exported() && attached(&s, &lease));
        s[0] = 10.0; // value write: stays attached
        assert!(attached(&s, &lease));
        s.try_extend_from_slice(&[3.0]).unwrap(); // beyond capacity
        assert!(!attached(&s, &lease));
        assert!(lease.is_retired());
        assert_eq!(s.generation(), 1);
        assert_eq!(&*s, &[10.0, 2.0, 3.0]);
        drop(lease);
        assert!(!s.is_exported(), "new allocation not exported");
    }

    #[test]
    fn growth_within_capacity_keeps_the_export() {
        let r = rt();
        let mut s: ArrayStorage<f64> = Vec::with_capacity(4).into();
        s.try_extend_from_slice(&[1.0, 2.0]).unwrap();
        let lease = export(&mut s, 1, &r).unwrap();
        s.try_extend_from_slice(&[3.0]).unwrap();
        assert!(attached(&s, &lease), "the view covers a live prefix");
        assert_eq!(s.generation(), 0);
    }

    #[test]
    fn shrinking_under_export_detaches_permanently() {
        let r = rt();
        let mut s: ArrayStorage<u32> = Vec::with_capacity(8).into();
        s.try_extend_from_slice(&[1, 2, 3]).unwrap();
        let lease = export(&mut s, 1, &r).unwrap();
        s.truncate(1);
        assert!(lease.is_retired() && !attached(&s, &lease));
        assert_eq!(s.capacity(), 8, "fresh block keeps the capacity");
        s.try_extend_from_slice(&[7, 7]).unwrap(); // re-grow within capacity
        assert!(!attached(&s, &lease), "staleness is monotone");
        assert_eq!(&*s, &[1, 7, 7]);
        let mut t: ArrayStorage<u32> = vec![1, 2].into();
        let l2 = export(&mut t, 2, &r).unwrap();
        t.try_resize(1, 0).unwrap();
        assert!(l2.is_retired());
        let mut u: ArrayStorage<u32> = vec![1, 2].into();
        let l3 = export(&mut u, 2, &r).unwrap();
        u.try_clone_from_slice(&[5]).unwrap();
        assert!(l3.is_retired());
        assert_eq!(&*u, &[5]);
    }

    #[test]
    fn drop_and_into_vec_retire_exports() {
        let r = rt();
        let mut s: ArrayStorage<u32> = vec![7, 8].into();
        let l1 = export(&mut s, 2, &r).unwrap();
        let v = s.clone(); // clones are fresh allocations
        assert!(!attached(&v, &l1));
        drop(s);
        assert!(l1.is_retired());
        let mut t: ArrayStorage<u32> = vec![1].into();
        let l2 = export(&mut t, 1, &r).unwrap();
        assert_eq!(t.into_vec(), vec![1]);
        assert!(l2.is_retired());
    }

    #[test]
    fn equal_length_copies_stay_attached() {
        let r = rt();
        let mut s: ArrayStorage<f64> = vec![1.0, 2.0].into();
        let lease = export(&mut s, 1, &r).unwrap();
        s.try_clone_from_slice(&[3.0, 4.0]).unwrap();
        assert!(attached(&s, &lease));
        assert_eq!(s.generation(), 0);
    }

    #[test]
    fn lease_counting_and_runtime_binding() {
        let (r1, r2) = (rt(), rt());
        let mut s: ArrayStorage<f64> = vec![1.0].into();
        let a = export(&mut s, 1, &r1).unwrap();
        let b = a.clone();
        assert!(export(&mut s, 1, &r2).is_err(), "one runtime at a time");
        drop(a);
        assert!(s.is_exported(), "clone still alive");
        drop(b);
        assert!(!s.is_exported());
        let c = export(&mut s, 1, &r2).unwrap(); // re-export elsewhere
        assert!(c.runtime().same_as(&r2) && attached(&s, &c));
    }
}
