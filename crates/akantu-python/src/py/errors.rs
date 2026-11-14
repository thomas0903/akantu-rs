// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): python/py_akantu.cc:136-170
// (`py::exception<akantu::debug::Exception>(mod, "Exception")`,
// `py::exception<debug::NLSNotConvergedException>(mod,
// "NLSNotConvergedException")`, translator).

//! Exception classes of `akantu.py11_akantu` (PY-004).
//!
//! As upstream, `akantu.Exception` and `akantu.NLSNotConvergedException`
//! both derive directly from Python's `Exception` (not from each other) and
//! live in module `akantu.py11_akantu`.

use pyo3::exceptions::PyException;
use pyo3::prelude::*;
use pyo3::sync::PyOnceLock;
use pyo3::types::PyType;

static EXCEPTION: PyOnceLock<Py<PyType>> = PyOnceLock::new();
static NLS_NOT_CONVERGED: PyOnceLock<Py<PyType>> = PyOnceLock::new();

fn new_type(py: Python<'_>, name: &std::ffi::CStr) -> Py<PyType> {
    // Creating a subclass of `Exception` with a static name cannot fail
    // short of memory exhaustion; there is no way to report an error from
    // the lazy initialiser, so fall back to `Exception` itself.
    PyErr::new_type(py, name, None, Some(&py.get_type::<PyException>()), None)
        .unwrap_or_else(|_| py.get_type::<PyException>().unbind())
}

/// `akantu.Exception` (upstream `akantu::debug::Exception`).
pub(crate) fn exception_type(py: Python<'_>) -> &Bound<'_, PyType> {
    EXCEPTION
        .get_or_init(py, || new_type(py, c"akantu.py11_akantu.Exception"))
        .bind(py)
}

/// `akantu.NLSNotConvergedException`.
pub(crate) fn nls_not_converged_type(py: Python<'_>) -> &Bound<'_, PyType> {
    NLS_NOT_CONVERGED
        .get_or_init(py, || {
            new_type(py, c"akantu.py11_akantu.NLSNotConvergedException")
        })
        .bind(py)
}

/// An `akantu.Exception` with `message`.
pub(crate) fn exception(py: Python<'_>, message: String) -> PyErr {
    PyErr::from_type(exception_type(py).clone(), message)
}

/// An `akantu.NLSNotConvergedException` with `message`.
pub(crate) fn nls_not_converged(py: Python<'_>, message: String) -> PyErr {
    PyErr::from_type(nls_not_converged_type(py).clone(), message)
}

/// Register both classes in the module.
pub(crate) fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    let py = m.py();
    m.add("Exception", exception_type(py))?;
    m.add("NLSNotConvergedException", nls_not_converged_type(py))?;
    Ok(())
}
