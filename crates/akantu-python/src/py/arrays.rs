// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): python/py_aka_array.hh (Array<T> <-> NumPy
// caster: 2-D `(size, nb_component)` C-ordered zero-copy views; `copy`
// policy), pybind11/eigen.h usage for Vector/Matrix (PY-005, PY-006).

//! `Array<T>` ↔ NumPy (PY-005/006).
//!
//! **Views.** Arrays owned by binding objects are stored as
//! [`SharedArray`]s and returned as zero-copy NumPy views through the
//! foreign-view contract (`docs/ARCHITECTURE.md`): shape
//! `(tuples, nb_component)` — `(n, 0)` for zero-component arrays — C order,
//! dtype `float64` (`Real`), `int32` (`Int`/`Idx`, D-012), `uint32`
//! (`UInt`), `bool`; base object an `_ArrayLease`. A view stays memory-safe
//! forever; once the owner retires the allocation (growth beyond capacity,
//! any shrink, replacement, destruction) it is *stale*: reads return the
//! values at retirement, writes no longer reach akantu, and binding calls
//! that take it as an output raise. Re-fetch to get an attached view.
//! `Vector` views are 1-D `(n,)`, `Matrix` views are Fortran-ordered `(r, c)`
//! (Eigen column-major, as upstream).
//!
//! **Copy mode** (port extension, `setArrayCopyMode(True)` or environment
//! variable `AKANTU_PY_COPY_MODE=1` at import): every getter returns an
//! independent copy instead of a view (upstream `return_value_policy::copy`
//! semantics: writes never reach akantu). Use it for multi-threaded scripts
//! that run NumPy operations releasing the GIL on akantu arrays while other
//! threads call akantu (the residual obligation UD-085 of view mode).
//!
//! **Inputs** are copied into owned Rust buffers before the native section
//! of the call (any dtype NumPy can cast safely; sequences accepted).

use std::sync::atomic::{AtomicBool, Ordering};

use akantu::core::array_shared::SharedArray;
use akantu::core::tensor::MatrixX;
use numpy::{PyArrayMethods, PyUntypedArrayMethods};
use pyo3::prelude::*;

use super::runtime::{native, runtime};
use crate::foreign_view::ForeignView;
use crate::foreign_view::numpy_export::{Exportable, Layout, to_numpy};

static COPY_MODE: AtomicBool = AtomicBool::new(false);

/// `true` if getters return copies.
pub(crate) fn copy_mode() -> bool {
    COPY_MODE.load(Ordering::Relaxed)
}

/// Set the export mode (module function `setArrayCopyMode`).
pub(crate) fn set_copy_mode(on: bool) {
    COPY_MODE.store(on, Ordering::Relaxed);
}

/// Owned NumPy array from values (`(rows, nbc)` in the given layout).
pub(crate) fn owned_numpy<'py, T: Exportable>(
    py: Python<'py>,
    values: Vec<T>,
    rows: usize,
    nbc: usize,
    layout: Layout,
) -> PyResult<Bound<'py, PyAny>> {
    let arr = numpy::PyArray1::from_vec(py, values);
    let shaped = match layout {
        Layout::Rows => arr.reshape([rows, nbc])?.into_any(),
        Layout::Vector | Layout::Tuple => arr.into_any(),
        // values are column-major (one column per tuple): (nbc, rows) in F order
        Layout::ColumnMajor => arr.reshape([rows, nbc])?.transpose()?.into_any(),
    };
    Ok(shaped)
}

/// Return `array` (owned by a binding object) to Python: a zero-copy view
/// (or a copy in copy mode).
pub(crate) fn export<'py, T: Exportable + Sync>(
    py: Python<'py>,
    array: &mut SharedArray<T>,
    layout: Layout,
    writable: bool,
) -> PyResult<Bound<'py, PyAny>> {
    if copy_mode() {
        return copy_out(py, array, layout);
    }
    export_view(py, array, layout, writable)
}

/// Zero-copy view of `array` regardless of the copy mode.
pub(crate) fn export_view<'py, T: Exportable + Sync>(
    py: Python<'py>,
    array: &mut SharedArray<T>,
    layout: Layout,
    writable: bool,
) -> PyResult<Bound<'py, PyAny>> {
    let view =
        ForeignView::new(array, runtime(), writable).map_err(|e| super::runtime::to_py(py, e))?;
    to_numpy(py, view, layout)
}

/// Independent copy of `array` as a NumPy array.
pub(crate) fn copy_out<'py, T: Exportable + Sync>(
    py: Python<'py>,
    array: &SharedArray<T>,
    layout: Layout,
) -> PyResult<Bound<'py, PyAny>> {
    let (values, rows, nbc) = native(py, || {
        let a = array.read()?;
        Ok((a.as_slice().to_vec(), a.len(), a.nb_component()))
    })?;
    owned_numpy(py, values, rows, nbc, layout)
}

/// A 2-D (or 1-D → one column) float array input, copied:
/// `(values row-major, rows, cols)`.
pub(crate) fn input_f64(obj: &Bound<'_, PyAny>) -> PyResult<(Vec<f64>, Vec<usize>)> {
    let py = obj.py();
    let np = py.import("numpy")?;
    let arr = np.call_method1("ascontiguousarray", (obj, np.getattr("float64")?))?;
    let arr = arr.cast_into::<numpy::PyArrayDyn<f64>>()?;
    let ro = arr.readonly();
    let shape = ro.shape().to_vec();
    let values = ro.as_slice()?.to_vec();
    Ok((values, shape))
}

/// Matrix input (`[[...], [...]]` or 2-D array) as a column-major
/// [`MatrixX`].
pub(crate) fn input_matrix(obj: &Bound<'_, PyAny>) -> PyResult<MatrixX> {
    let (values, shape) = input_f64(obj)?;
    let (rows, cols) = match shape.as_slice() {
        [r, c] => (*r, *c),
        [n] => (*n, 1),
        _ => {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "expected a 2-D array for a matrix",
            ));
        }
    };
    let mut col_major = vec![0.0; rows * cols];
    for i in 0..rows {
        for j in 0..cols {
            col_major[j * rows + i] = values[i * cols + j];
        }
    }
    MatrixX::from_col_major(rows, cols, col_major).map_err(|e| super::runtime::to_py(obj.py(), e))
}

/// A [`MatrixX`] as an owned Fortran-ordered NumPy array (upstream returns
/// `Matrix<Real>` through pybind11's Eigen caster: column-major).
pub(crate) fn matrix_to_numpy<'py>(py: Python<'py>, m: &MatrixX) -> PyResult<Bound<'py, PyAny>> {
    owned_numpy(
        py,
        m.as_slice().to_vec(),
        m.cols(),
        m.rows(),
        Layout::ColumnMajor,
    )
}

/// A vector as an owned 1-D NumPy array.
pub(crate) fn vector_to_numpy<'py>(py: Python<'py>, v: Vec<f64>) -> PyResult<Bound<'py, PyAny>> {
    let n = v.len();
    owned_numpy(py, v, n, 1, Layout::Vector)
}
