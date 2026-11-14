// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): python/py_aka_array.hh (aka_array_cast:
// NumPy array over `src.data()` with a base object; type_caster::load with
// forcecast copies, UD-024), python/py_constitutive_law.cc (internal-field
// handles returned to Python). Access protocol: akantu-rs design
// (UD-085).

//! Foreign views of akantu arrays — the binding side of the foreign-view
//! contract (`docs/ARCHITECTURE.md`). **This module is the only place
//! where an export-lease pointer is dereferenced.**
//!
//! A [`ForeignView`] is what the base object of a NumPy array (or a C view
//! handle) holds: an [`ExportLease`] of a [`SharedArray`], a row window
//! (NumPy basic slicing along axis 0) and a writability flag.
//!
//! * **Lifetime.** The lease keeps the allocation alive: a view is never
//!   dangling. After a relocation (growth beyond capacity, any shrink, a
//!   replacement) or the owner's destruction it is *stale-but-safe*: reads
//!   see the values at retirement, writes no longer reach the array, and
//!   entry points taking a view as an **output** call
//!   [`ForeignView::require_attached`] (UD-024/UD-085).
//! * **Access.** Every element access takes the runtime's [`Interpreter`]
//!   and first calls [`ExportLease::check_foreign_access`]; it fails unless
//!   the runtime is in its foreign phase, i.e. no native guard of any
//!   exported array is alive or acquirable and no other thread can access
//!   foreign-side ([`akantu::core::array_shared`]). Hence no live `&[T]`/
//!   `&mut [T]` of the allocation coexists with an access through the
//!   pointer, and accesses are ordered by the runtime's acquire/release
//!   phase transitions.
//!
//! Two adversarial programs are therefore impossible in safe code.
//! A retained native slice across a foreign write does not compile:
//!
//! ```compile_fail
//! use akantu::core::{Array, ForeignRuntime, SharedArray};
//! use py11_akantu::foreign_view::ForeignView;
//! # fn main() -> akantu::core::Result<()> {
//! let rt = ForeignRuntime::new();
//! let mut a = SharedArray::new(Array::<f64>::zeros(1, 1));
//! let v = ForeignView::new(&mut a, &rt, true)?;
//! let mut interp = rt.enter()?;
//! let section = interp.native()?;
//! let s = a.read()?;            // native slice of the exported allocation
//! v.write(&interp, 0, 0, 2.0)?; // error[E0502]: `interp` is mutably borrowed
//! assert_eq!(s.as_slice()[0], 2.0);
//! drop(section);
//! # Ok(()) }
//! ```
//!
//! and the original form of the second no longer type-checks (a plain `Array`
//! cannot be exported, and element access needs the interpreter):
//!
//! ```compile_fail
//! use akantu::core::Array;
//! use py11_akantu::foreign_view::ForeignView;
//! let mut a = Array::<f64>::zeros(1, 1);
//! let v = ForeignView::new(&mut a, true);
//! let s = a.as_slice();
//! v.write(0, 0, 2.0).unwrap();
//! assert_eq!(s[0], 2.0);
//! ```
//!
//! Releasing the section while the native guard is alive turns the same
//! sequence into a runtime rejection (`tests/foreign_views.rs`).
//!
//! **NumPy and C pointers** (`as_ptr`) are handed over only by the binding
//! layer, under the obligations stated on [`ForeignView::as_ptr`].

use std::ops::Range;

use akantu::core::array_shared::{ForeignRuntime, Interpreter, SharedArray};
use akantu::core::array_storage::ExportLease;
use akantu::core::{Error, Result};

/// A NumPy/C view of a [`SharedArray`] (owns a lease).
#[derive(Debug, Clone)]
pub struct ForeignView<T: Copy> {
    lease: ExportLease<T>,
    first_row: usize,
    rows: usize,
    writable: bool,
}

// SAFETY: a `ForeignView` is a lease (allocation keep-alive behind an
// `Arc`) plus plain integers. Moving or sharing it across threads performs
// no access to the viewed values. Every dereference of the lease pointer
// happens in a method that takes `&Interpreter` and first succeeds
// `ExportLease::check_foreign_access`: the interpreter token is `!Send +
// !Sync` and exclusive per runtime, so all such dereferences are confined to
// the single thread holding that runtime and are ordered with every native
// guard by the runtime's acquire/release phase transitions. `T: Send` is
// required because values are copied out on whichever thread holds the
// interpreter.
unsafe impl<T: Copy + Send> Send for ForeignView<T> {}
// SAFETY: as above — `&ForeignView` exposes only metadata and accesses that
// need the (thread-confined, exclusive) interpreter of the lease's runtime.
unsafe impl<T: Copy + Send> Sync for ForeignView<T> {}

impl<T: Copy> ForeignView<T> {
    /// Export `array` to `runtime` (upstream `aka_array_cast(array, base,
    /// writeable)`).
    pub fn new(
        array: &mut SharedArray<T>,
        runtime: &ForeignRuntime,
        writable: bool,
    ) -> Result<Self> {
        let lease = array.export(runtime)?;
        let rows = lease.len();
        Ok(Self {
            lease,
            first_row: 0,
            rows,
            writable,
        })
    }

    /// A view of the whole allocation of `lease` (an export taken by the
    /// owner, e.g. through a mesh domain's export slot).
    #[must_use]
    pub fn from_lease(lease: ExportLease<T>, writable: bool) -> Self {
        let rows = lease.len();
        Self {
            lease,
            first_row: 0,
            rows,
            writable,
        }
    }

    /// The export lease of this view.
    #[must_use]
    pub fn lease(&self) -> &ExportLease<T> {
        &self.lease
    }

    /// NumPy shape `(rows, nb_component)` of this (possibly sliced) view.
    #[must_use]
    pub fn shape(&self) -> (usize, usize) {
        (self.rows, self.lease.nb_component())
    }

    /// `view[range, :]` — a view of the same allocation (shares the lease).
    pub fn rows(&self, range: Range<usize>) -> Result<Self> {
        if range.start > range.end || range.end > self.rows {
            return Err(Error::OutOfRange {
                what: "view row slice",
                index: range.end.max(range.start),
                len: self.rows,
            });
        }
        Ok(Self {
            lease: self.lease.clone(),
            first_row: self.first_row + range.start,
            rows: range.end - range.start,
            writable: self.writable,
        })
    }

    /// Read-only copy of this view (NumPy `a.setflags(write=False)` view).
    #[must_use]
    pub fn read_only(&self) -> Self {
        Self {
            writable: false,
            ..self.clone()
        }
    }

    fn offset(&self, i: usize, j: usize) -> Result<usize> {
        let c = self.lease.nb_component();
        if i >= self.rows {
            return Err(Error::OutOfRange {
                what: "view row",
                index: i,
                len: self.rows,
            });
        }
        if j >= c {
            return Err(Error::OutOfRange {
                what: "view component",
                index: j,
                len: c,
            });
        }
        Ok((self.first_row + i) * c + j)
    }

    fn check_writable(&self) -> Result<()> {
        if self.writable {
            Ok(())
        } else {
            Err(Error::invalid("assignment destination is read-only"))
        }
    }

    /// Element read (what NumPy does when indexing the view).
    pub fn read(&self, interp: &Interpreter, i: usize, j: usize) -> Result<T> {
        let k = self.offset(i, j)?;
        self.lease.check_foreign_access(interp)?;
        // SAFETY: `k < (first_row + rows) * nbc <= nb_values` (checked offset;
        // the row window lies inside the lease). The lease keeps its
        // allocation of ≥ `nb_values` initialised `T: Copy` values alive.
        // `check_foreign_access` succeeded and `interp` stays borrowed, so
        // no native guard (hence no `&[T]`/`&mut [T]`) of this allocation is
        // alive and no other thread accesses it foreign-side.
        Ok(unsafe { self.lease.as_ptr().add(k).read() })
    }

    /// Element write (NumPy assignment into the view).
    pub fn write(&self, interp: &Interpreter, i: usize, j: usize, value: T) -> Result<()> {
        self.check_writable()?;
        let k = self.offset(i, j)?;
        self.lease.check_foreign_access(interp)?;
        // SAFETY: as in `read`; the write targets the leased block only.
        unsafe { self.lease.as_ptr().add(k).write(value) };
        Ok(())
    }

    /// `view[:] = value`.
    pub fn fill(&self, interp: &Interpreter, value: T) -> Result<()> {
        self.check_writable()?;
        self.lease.check_foreign_access(interp)?;
        let n = self.rows * self.lease.nb_component();
        let base = self.first_row * self.lease.nb_component();
        for k in base..base + n {
            // SAFETY: `k < (first_row + rows) * nbc <= nb_values`; admission
            // as in `read`.
            unsafe { self.lease.as_ptr().add(k).write(value) };
        }
        Ok(())
    }

    /// `out[:] = view.ravel()` (`np.array(view)`); `out` must hold exactly
    /// `rows * nb_component` values.
    pub fn copy_to(&self, interp: &Interpreter, out: &mut [T]) -> Result<()> {
        let n = self.rows * self.lease.nb_component();
        if out.len() != n {
            return Err(Error::DimensionMismatch {
                context: "ForeignView::copy_to",
                expected: n,
                got: out.len(),
            });
        }
        self.lease.check_foreign_access(interp)?;
        let base = self.first_row * self.lease.nb_component();
        // SAFETY: source range `base..base + n` lies inside the leased block
        // (admission as in `read`). `out` cannot overlap it: a Rust slice of
        // an exported allocation exists only under a native guard, and the
        // successful admission check proves none is alive; retired blocks
        // are reachable only through leases (raw pointers).
        unsafe {
            std::ptr::copy_nonoverlapping(self.lease.as_ptr().add(base), out.as_mut_ptr(), n);
        }
        Ok(())
    }

    /// `true` if NumPy may write through this view.
    #[must_use]
    pub fn is_writable(&self) -> bool {
        self.writable
    }

    /// `true` iff the view still aliases `array`'s live values.
    #[must_use]
    pub fn is_attached_to(&self, array: &SharedArray<T>) -> bool {
        array.is_attached(&self.lease)
    }

    /// `true` once the owner retired the viewed allocation.
    #[must_use]
    pub fn is_stale(&self) -> bool {
        self.lease.is_retired()
    }

    /// For binding entry points taking an **output** buffer: refuse a view
    /// that would no longer reach `array` (UD-024/UD-085: never write
    /// elsewhere). The output is then written through a native guard of
    /// `array`, never through the view pointer.
    pub fn require_attached(&self, array: &SharedArray<T>) -> Result<()> {
        if self.is_attached_to(array) {
            Ok(())
        } else {
            Err(Error::invalid(
                "stale array view: the array was resized, replaced or destroyed; fetch it again",
            ))
        }
    }

    /// First value of this view, for the NumPy (`PyArray_New` data pointer,
    /// base object = this view) or C (`akantu_view_acquire`) handover.
    ///
    /// Obtaining the pointer is safe. **Obligations of the code that
    /// dereferences it** (the NumPy/C side, set up by the binding layer):
    /// only `shape()` values from it, never after this view (lease) is
    /// dropped, never through a read-only view for writing, and only while
    /// the thread holds the runtime in its foreign phase — for Python: with
    /// the GIL held and outside any akantu call (or inside an akantu
    /// callback), which the binding guarantees by entering a
    /// [`akantu::core::array_shared::NativeSection`] for every native call
    /// and never releasing the GIL inside one; for C: between
    /// `akantu_runtime_enter` and `akantu_runtime_leave`.
    #[must_use]
    pub fn as_ptr(&self) -> *mut T {
        self.lease
            .as_ptr()
            .wrapping_add(self.first_row * self.lease.nb_component())
    }
}

/// Run `f` as a callback into foreign code from inside a native section
/// (Python trampoline: law/solver/BC callbacks). Fails — without running
/// `f` — if a native guard of an exported array is alive.
pub fn foreign_callback<R>(
    runtime: &ForeignRuntime,
    f: impl FnOnce(&mut Interpreter) -> R,
) -> Result<R> {
    let mut cb = runtime.callback()?;
    Ok(f(&mut cb))
}

/// Owner of an object reachable from foreign code (PyO3 `PyCell` borrow
/// flag analogue): native mutable access is exclusive and fails promptly
/// (re-entrant binding calls, concurrent native users). It does not govern
/// foreign accesses to exported values — the runtime phase does.
#[derive(Debug, Default)]
pub struct SharedOwner<T> {
    inner: std::sync::Mutex<T>,
}

impl<T> SharedOwner<T> {
    /// Wrap `value`.
    pub fn new(value: T) -> Self {
        Self {
            inner: std::sync::Mutex::new(value),
        }
    }

    /// Exclusive access or an immediate error ("Already borrowed").
    pub fn try_borrow_mut(&self) -> Result<std::sync::MutexGuard<'_, T>> {
        match self.inner.try_lock() {
            Ok(g) => Ok(g),
            Err(std::sync::TryLockError::WouldBlock) => Err(Error::invalid(
                "object is in use by a native computation or another thread (already borrowed)",
            )),
            Err(std::sync::TryLockError::Poisoned(_)) => {
                Err(Error::invalid("object poisoned by a panic"))
            }
        }
    }
}

/// NumPy arrays over [`ForeignView`]s (feature `python`). This is
/// the binding half of [`ForeignView::as_ptr`]: the only place where a lease
/// pointer is handed to NumPy.
///
/// Every array created here has an `ArrayLease` as its NumPy base object,
/// which owns (a clone of) the view, hence the lease: NumPy keeps the base
/// alive as long as the array or any array derived from it (slices,
/// reshapes, transposes — NumPy points their `base` at the same object), so
/// the allocation outlives every NumPy access. Shape and strides are derived
/// from the view's own shape; the `WRITEABLE` flag mirrors the view's
/// writability.
#[cfg(feature = "python")]
pub mod numpy_export {
    use std::ffi::{c_int, c_void};

    use numpy::npyffi::{self, NPY_ARRAY_WRITEABLE, npy_intp};
    use numpy::{Element, PY_ARRAY_API, PyArrayDescrMethods};
    use pyo3::exceptions::PyOverflowError;
    use pyo3::prelude::*;
    use pyo3::{PyErr, ffi};

    use super::ForeignView;

    /// How the `(rows, nb_component)` window of a view is presented.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum Layout {
        /// `(rows, nb_component)`, C order — upstream `Array<T>` caster.
        Rows,
        /// `(rows,)` — upstream `Vector<T>` (Eigen column vector) by
        /// reference; requires `nb_component == 1`.
        Vector,
        /// `(nb_component, rows)`, Fortran order — upstream `Matrix<T>`
        /// (Eigen, column-major) by reference: the view stores one column
        /// per tuple.
        ColumnMajor,
        /// `(nb_component,)` — one tuple as a 1-D vector (upstream
        /// `VectorProxy` row views passed to boundary-condition functors);
        /// requires `rows == 1`.
        Tuple,
    }

    /// Typed views a lease object can hold.
    #[derive(Debug)]
    pub enum AnyView {
        /// `Real`
        F64(ForeignView<f64>),
        /// `Int` / `Idx`
        I32(ForeignView<i32>),
        /// `UInt`
        U32(ForeignView<u32>),
        /// `bool`
        Bool(ForeignView<bool>),
    }

    impl AnyView {
        fn is_stale(&self) -> bool {
            match self {
                Self::F64(v) => v.is_stale(),
                Self::I32(v) => v.is_stale(),
                Self::U32(v) => v.is_stale(),
                Self::Bool(v) => v.is_stale(),
            }
        }
    }

    /// Element types that can be exported (dtype + lease wrapper).
    pub trait Exportable: Copy + Element + Send + Sync + 'static {
        /// Wrap a view for storage in an `ArrayLease`.
        fn wrap(view: ForeignView<Self>) -> AnyView;
        /// The view held by `any`, if it has this element type.
        fn typed(any: &AnyView) -> Option<&ForeignView<Self>>;
    }
    impl Exportable for f64 {
        fn wrap(view: ForeignView<Self>) -> AnyView {
            AnyView::F64(view)
        }
        fn typed(any: &AnyView) -> Option<&ForeignView<Self>> {
            match any {
                AnyView::F64(v) => Some(v),
                _ => None,
            }
        }
    }
    impl Exportable for i32 {
        fn wrap(view: ForeignView<Self>) -> AnyView {
            AnyView::I32(view)
        }
        fn typed(any: &AnyView) -> Option<&ForeignView<Self>> {
            match any {
                AnyView::I32(v) => Some(v),
                _ => None,
            }
        }
    }
    impl Exportable for u32 {
        fn wrap(view: ForeignView<Self>) -> AnyView {
            AnyView::U32(view)
        }
        fn typed(any: &AnyView) -> Option<&ForeignView<Self>> {
            match any {
                AnyView::U32(v) => Some(v),
                _ => None,
            }
        }
    }
    impl Exportable for bool {
        fn wrap(view: ForeignView<Self>) -> AnyView {
            AnyView::Bool(view)
        }
        fn typed(any: &AnyView) -> Option<&ForeignView<Self>> {
            match any {
                AnyView::Bool(v) => Some(v),
                _ => None,
            }
        }
    }

    /// NumPy base object of every akantu view: owns the export lease.
    #[pyclass(frozen, name = "_ArrayLease", module = "akantu.py11_akantu")]
    #[derive(Debug)]
    pub struct ArrayLease {
        view: AnyView,
    }

    #[pymethods]
    impl ArrayLease {
        /// `True` once the viewed allocation was retired by its owner
        /// (resize, shrink, replacement, destruction): the array is then
        /// stale-but-safe (`docs/ARCHITECTURE.md`).
        fn is_stale(&self) -> bool {
            self.view.is_stale()
        }

        fn __repr__(&self) -> String {
            format!(
                "<akantu view lease ({})>",
                if self.view.is_stale() {
                    "stale"
                } else {
                    "attached"
                }
            )
        }
    }

    impl ArrayLease {
        /// The view held by this lease object.
        #[must_use]
        pub fn view(&self) -> &AnyView {
            &self.view
        }
    }

    fn intp(v: usize) -> PyResult<npy_intp> {
        npy_intp::try_from(v).map_err(|_| PyOverflowError::new_err("array extent exceeds npy_intp"))
    }

    /// Create a NumPy array over `view` (zero-copy) with an `ArrayLease`
    /// owning the view as base object.
    pub fn to_numpy<'py, T: Exportable>(
        py: Python<'py>,
        view: ForeignView<T>,
        layout: Layout,
    ) -> PyResult<Bound<'py, PyAny>> {
        let (rows, nbc) = view.shape();
        let size = std::mem::size_of::<T>();
        let (mut dims, mut strides): (Vec<npy_intp>, Vec<npy_intp>) = match layout {
            Layout::Rows => (
                vec![intp(rows)?, intp(nbc)?],
                vec![intp(nbc.saturating_mul(size))?, intp(size)?],
            ),
            Layout::Vector => {
                if nbc != 1 {
                    return Err(pyo3::exceptions::PyValueError::new_err(
                        "a vector view needs exactly one component per tuple",
                    ));
                }
                (vec![intp(rows)?], vec![intp(size)?])
            }
            Layout::ColumnMajor => (
                vec![intp(nbc)?, intp(rows)?],
                vec![intp(size)?, intp(nbc.saturating_mul(size))?],
            ),
            Layout::Tuple => {
                if rows != 1 {
                    return Err(pyo3::exceptions::PyValueError::new_err(
                        "a tuple view needs exactly one row",
                    ));
                }
                (vec![intp(nbc)?], vec![intp(size)?])
            }
        };
        let flags: c_int = if view.writable {
            NPY_ARRAY_WRITEABLE
        } else {
            0
        };
        let data = view.as_ptr();
        let nd = c_int::try_from(dims.len()).unwrap_or(2);
        let lease = Bound::new(
            py,
            ArrayLease {
                view: T::wrap(view),
            },
        )?;
        // SAFETY (pointer handover, obligations of `ForeignView::as_ptr`):
        // * `data`, `dims` and `strides` describe exactly the view's window:
        //   for `Rows`/`ColumnMajor` the elements `data + i*nbc + j` with
        //   `i < rows`, `j < nbc` (the same set in both layouts), for
        //   `Vector` (`nbc == 1`) `data + i`, `i < rows`, for `Tuple`
        //   (`rows == 1`) `data + j`, `j < nbc`; all lie inside the
        //   leased allocation because the view's row window lies inside the
        //   lease (`ForeignView::rows` checks it) and the lease keeps that
        //   allocation (≥ `(first_row + rows) * nbc` initialised `T`) alive.
        // * The lease outlives every NumPy access: `lease` (which owns the
        //   view and so the lease) becomes the array's base object below,
        //   and NumPy keeps the base alive for this array and every array
        //   derived from it. If base installation fails the array is
        //   released before returning, so no array without base escapes.
        // * Read-only views get no `WRITEABLE` flag.
        // * Accesses happen only in the runtime's foreign phase: the binding
        //   enters a native section for every akantu call and never releases
        //   the GIL inside one, so NumPy (which runs with the GIL held) cannot
        //   overlap a native guard — the residual multi-threaded obligation
        //   is UD-085 (NumPy releasing the GIL on an akantu view).
        // * `PyArray_NewFromDescr` steals the descriptor reference
        //   (`into_dtype_ptr` increments it) and returns a new reference or
        //   NULL with a Python error set; `PyArray_SetBaseObject` steals the
        //   reference passed (also on failure) and returns -1 on failure.
        unsafe {
            let arr = PY_ARRAY_API.PyArray_NewFromDescr(
                py,
                npyffi::get_type_object(py, npyffi::NpyTypes::PyArray_Type),
                T::get_dtype(py).into_dtype_ptr(),
                nd,
                dims.as_mut_ptr(),
                strides.as_mut_ptr(),
                data.cast::<c_void>(),
                flags,
                std::ptr::null_mut(),
            );
            if arr.is_null() {
                return Err(PyErr::fetch(py));
            }
            let base = lease.into_any().into_ptr();
            if PY_ARRAY_API.PyArray_SetBaseObject(py, arr.cast::<npyffi::PyArrayObject>(), base) < 0
            {
                ffi::Py_DecRef(arr);
                return Err(PyErr::fetch(py));
            }
            Ok(Bound::from_owned_ptr(py, arr))
        }
    }
}
