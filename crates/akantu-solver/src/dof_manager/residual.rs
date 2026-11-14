// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/solver/solver_vector.hh,
// src/solver/solver_vector_default.hh (the residual `SolverVector` of a
// `DOFManager`), python/py_solver.cc:85-92 (`SolverVector.getValues()`: a
// view of the vector, "Is not copied"). Exportable storage: akantu-rs design
// (D-024, D-047; docs/ARCHITECTURE.md).

//! The residual of a [`super::DofManager`] (PY-045).
//!
//! Upstream's Python `SolverVector.getValues()` returns a writable NumPy
//! view of the residual's `Array<Real>` that follows every later assembly.
//! The port keeps the residual's values in exportable storage
//! ([`SharedArray`], one component, one tuple per local equation) so that
//! the bindings can hand out the same zero-copy view: every assembly and
//! zeroing writes in place; only a resize (a renumbering of the
//! equations) may move the values, which retires an exported block (the
//! view keeps the old values: stale but safe, where upstream's view would
//! dangle). Native code reaches the values through guards
//! ([`ResidualVector::values`], [`ResidualVector::values_mut`]), which
//! follow the foreign-view phase protocol like every exported array.
//!
//! Release counter (upstream `release_`): `zero` increments it, as
//! upstream's `set`; a resize does not (upstream `resizeGlobalArrays`
//! resizes the array behind the vector).

use akantu_core::Real;
use akantu_core::array_shared::{ArrayMut, SharedArray, SliceRef};
use akantu_linalg::SolverVector;

/// The residual vector of a DOF manager: upstream's `SolverVector` whose
/// values are exportable storage.
#[derive(Debug, Default)]
pub struct ResidualVector {
    values: SharedArray<Real>,
    release: u64,
    distributed: bool,
}

impl ResidualVector {
    /// Shared native access to the values of the local equations.
    ///
    /// # Panics
    /// As [`SharedArray::borrow`]: if the values are exported and their
    /// runtime is not in a native section (a protocol violation of the
    /// caller, like a conflicting `RefCell` borrow).
    #[must_use]
    #[track_caller]
    pub fn values(&self) -> SliceRef<'_, Real> {
        self.values.borrow().into_slice()
    }

    /// Exclusive native access to the values (in-place writes; the guard
    /// never hands out the array itself).
    ///
    /// # Panics
    /// As [`ResidualVector::values`].
    #[track_caller]
    pub fn values_mut(&mut self) -> ArrayMut<'_, Real> {
        self.values.borrow_mut()
    }

    /// The exportable storage of the values (for the bindings' export
    /// tables; `&mut` proves no native guard is alive).
    pub fn shared_mut(&mut self) -> &mut SharedArray<Real> {
        &mut self.values
    }

    /// Number of local equations (upstream `localSize`; metadata, no guard).
    #[must_use]
    pub fn local_size(&self) -> usize {
        self.values.len()
    }

    /// Release counter (upstream `release()`).
    #[must_use]
    pub fn release(&self) -> u64 {
        self.release
    }

    /// `true` for upstream's `SolverVectorDistributed` behaviour (and for
    /// the PETSc manager's vectors, upstream `SolverVectorPETSc`): the
    /// manager's kind of vectors, [`super::DofManager::vectors_distributed`].
    #[must_use]
    pub fn is_distributed(&self) -> bool {
        self.distributed
    }

    /// Set the kind (the manager, when it learns the mesh's distribution).
    /// Values and release are unchanged.
    pub(crate) fn set_distributed(&mut self, distributed: bool) {
        self.distributed = distributed;
    }

    /// Resize to `n` local equations, new entries zero, values kept in
    /// place while the capacity allows (upstream `resizeGlobalArrays`).
    #[track_caller]
    pub(crate) fn resize(&mut self, n: usize) {
        // one component (`Array::default()`): one tuple per equation
        debug_assert_eq!(self.values.nb_component(), 1);
        self.values.borrow_mut().resize(n, 0.0);
    }

    /// Replace the values by `f(old)` (node removal: the per-equation
    /// values follow their equations), in place while the length is kept;
    /// a shrink retires an exported block first (the view keeps the
    /// pre-removal values: stale but safe, D-047).
    #[track_caller]
    pub(crate) fn translate(&mut self, f: impl FnOnce(&[Real]) -> Vec<Real>) {
        let new = f(&self.values());
        let mut g = self.values.borrow_mut();
        g.resize(new.len(), 0.0);
        g.as_mut_slice().copy_from_slice(&new);
    }

    /// Set every entry to zero (upstream `zero` → `set(0.)`), in place.
    #[track_caller]
    pub fn zero(&mut self) {
        self.values.borrow_mut().fill(0.0);
        self.release = self.release.wrapping_add(1);
    }

    /// A [`SolverVector`] holding a copy of the values (for code that needs
    /// the general vector type; the copy does not follow the residual). It
    /// keeps the residual's distributed flag, which
    /// selects the rank-suffixed file names of `save_vector*`.
    #[must_use]
    #[track_caller]
    pub fn to_solver_vector(&self) -> SolverVector {
        let mut v = SolverVector::new("residual", self.distributed);
        v.values = self.values().to_vec();
        v
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The copy keeps the values and the distributed
    /// flag (it was dropped by `SolverVector::zeros`).
    #[test]
    fn to_solver_vector_keeps_the_distributed_flag() {
        for distributed in [false, true] {
            let mut r = ResidualVector {
                distributed,
                ..ResidualVector::default()
            };
            r.resize(3);
            r.values_mut()
                .as_mut_slice()
                .copy_from_slice(&[1.0, -2.0, 3.5]);
            let v = r.to_solver_vector();
            assert_eq!(v.values, [1.0, -2.0, 3.5]);
            assert_eq!(v.is_distributed(), distributed);
        }
    }
}
