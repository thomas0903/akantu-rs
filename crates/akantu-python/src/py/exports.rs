// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): python/py_aka_array.hh (`Array<T> &`
// returned with `return_value_policy::reference`: NumPy views aliasing the
// arrays owned by `Mesh`, `SolidMechanicsModel`, materials). Export
// bookkeeping: akantu-rs design (decision records D-024, D-047;
// docs/ARCHITECTURE.md).

//! Zero-copy exports of Rust-owned arrays (D-047).
//!
//! Every array the Python API returns by reference upstream is stored by
//! its Rust owner as a [`SharedArray`] and handed to NumPy as a view of
//! that very allocation ([`ForeignView`] → `numpy_export::to_numpy`): no
//! mirror, no copy, no synchronisation at call boundaries. Writes from
//! Python land in the owner's storage immediately, also from inside Python
//! callbacks; reads always see the live values (ARCHITECTURE.md, "Shared storage and its aliasing rules" phase protocol).
//!
//! An [`ExportTable`] caches, per owner, one attached export lease per array
//! that Python fetched. It is pure bookkeeping (no values are copied):
//!
//! * a getter called while the owner is free re-uses the cached lease when it
//!   still covers the live values, or exports the array again (the cache is
//!   refreshed whenever the binding holds the owner mutably);
//! * a getter called from inside a Python callback of a running call on the
//!   same owner — which holds the owner, so the array cannot be borrowed —
//!   returns a view of the cached lease ([`ExportTable::cached`]): it aliases
//!   the live array as long as the lease is attached; a lease whose block
//!   was retired by the running call raises instead of returning stale
//!   data.

use std::collections::BTreeMap;
use std::fmt::Debug;

use akantu::core::array_shared::SharedArray;
use akantu::core::array_storage::ExportLease;
use akantu::core::{Error, Result};
use pyo3::prelude::*;

use super::arrays::copy_mode;
use super::runtime::runtime;
use crate::foreign_view::ForeignView;
use crate::foreign_view::numpy_export::{AnyView, Exportable, Layout, to_numpy};

/// Something an export lease can be taken from: a [`SharedArray`] the
/// binding holds mutably, or an export slot of a mesh domain
/// (`akantu::mesh::ExportSlot`).
pub(crate) trait ExportSource<T: Copy> {
    /// A new lease of the current allocation.
    fn lease(&mut self) -> Result<ExportLease<T>>;
    /// `true` iff `lease` still views the live values.
    fn attached(&self, lease: &ExportLease<T>) -> bool;
    /// Current number of tuples.
    fn tuples(&self) -> usize;
}

impl<T: Copy> ExportSource<T> for SharedArray<T> {
    fn lease(&mut self) -> Result<ExportLease<T>> {
        self.export(runtime())
    }
    fn attached(&self, lease: &ExportLease<T>) -> bool {
        self.is_attached(lease)
    }
    fn tuples(&self) -> usize {
        self.len()
    }
}

impl<T: Copy> ExportSource<T> for akantu::mesh::ExportSlot<'_, T> {
    fn lease(&mut self) -> Result<ExportLease<T>> {
        self.export(runtime())
    }
    fn attached(&self, lease: &ExportLease<T>) -> bool {
        self.is_attached(lease)
    }
    fn tuples(&self) -> usize {
        self.len()
    }
}

/// Cached export leases of one Rust owner, keyed by `K`.
#[derive(Debug)]
pub(crate) struct ExportTable<K: Ord> {
    map: BTreeMap<K, AnyView>,
}

impl<K: Ord> Default for ExportTable<K> {
    fn default() -> Self {
        Self {
            map: BTreeMap::new(),
        }
    }
}

impl<K: Ord + Clone + Debug> ExportTable<K> {
    /// An attached, writable view of the whole array of `src` under `key`:
    /// the cached lease if it still covers every live tuple, otherwise a new
    /// export (native section required: the owner is held mutably).
    pub(crate) fn view<T: Exportable>(
        &mut self,
        key: K,
        src: &mut impl ExportSource<T>,
    ) -> Result<ForeignView<T>> {
        if let Some(v) = self.map.get(&key).and_then(T::typed)
            && src.attached(v.lease())
            && v.shape().0 == src.tuples()
        {
            return Ok(v.clone());
        }
        let v = ForeignView::from_lease(src.lease()?, true);
        self.map.insert(key, T::wrap(v.clone()));
        Ok(v)
    }

    /// The cached view of `key`, for a getter running while its owner is
    /// held by the call that is running a Python callback: error if the
    /// array was never exported or if the running call retired its block.
    pub(crate) fn cached<T: Exportable>(&self, key: &K) -> Result<ForeignView<T>> {
        let v = self.map.get(key).and_then(T::typed).ok_or_else(|| {
            Error::invalid(format!(
                "{key:?} is not available inside this callback: the object is in use by the \
                 running call and the array was not fetched before (fetch it before the call)"
            ))
        })?;
        if v.is_stale() {
            return Err(Error::invalid(format!(
                "{key:?} was reallocated by the running call: it cannot be fetched inside this \
                 callback (fetch it again after the call)"
            )));
        }
        // a growth within capacity keeps the lease attached on the old
        // prefix; the owner is held, so it cannot be re-exported here
        if !v.lease().covers_live_tuples() {
            return Err(Error::invalid(format!(
                "{key:?} was resized by the running call: it cannot be fetched inside this \
                 callback (fetch it again after the call)"
            )));
        }
        Ok(v.clone())
    }

    /// Drop the entry of `key` (the array no longer exists).
    pub(crate) fn remove(&mut self, key: &K) {
        self.map.remove(key);
    }

    /// Cached keys.
    pub(crate) fn keys(&self) -> Vec<K> {
        self.map.keys().cloned().collect()
    }

    /// `true` if `key` is cached.
    pub(crate) fn contains(&self, key: &K) -> bool {
        self.map.contains_key(key)
    }
}

/// The NumPy array of `view` (zero-copy; an independent copy in copy mode,
/// taken from the view in the foreign phase). Read-only views get no
/// `WRITEABLE` flag.
pub(crate) fn into_numpy<'py, T: Exportable>(
    py: Python<'py>,
    view: ForeignView<T>,
    layout: Layout,
) -> PyResult<Bound<'py, PyAny>> {
    let a = to_numpy(py, view, layout)?;
    if copy_mode() {
        return a.call_method0("copy");
    }
    Ok(a)
}

#[cfg(test)]
mod tests {
    use akantu::core::Array;

    use super::*;

    /// A running call that grows an exported array
    /// within capacity and then calls back must not hand the callback a
    /// view of the old prefix.
    #[test]
    fn cached_lease_is_refused_after_a_growth_within_capacity() {
        let mut a = SharedArray::new(Array::<f64>::zeros(2, 1));
        a.write().unwrap().try_reserve(8).unwrap();
        let mut table = ExportTable::<u8>::default();
        let mut interp = runtime().enter().unwrap();
        {
            let _s = interp.native().unwrap();
            table.view::<f64>(0, &mut a).unwrap();
        }
        assert!(table.cached::<f64>(&0).is_ok());
        let generation = a.generation();
        {
            // the running call grows the array (no reallocation) ...
            let _s = interp.native().unwrap();
            a.write().unwrap().push_value(1.0);
        }
        assert_eq!(a.generation(), generation, "growth within capacity");
        // ... and calls back: the getter refuses instead of a short view
        let err = table.cached::<f64>(&0).unwrap_err();
        assert!(err.to_string().contains("resized by the running call"));
        // after the call the binding re-exports the whole array
        {
            let _s = interp.native().unwrap();
            assert_eq!(table.view::<f64>(0, &mut a).unwrap().shape().0, 3);
        }
        assert!(table.cached::<f64>(&0).is_ok());
    }
}
