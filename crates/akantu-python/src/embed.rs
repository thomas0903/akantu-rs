// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): examples/c++/solid_mechanics_model/
// boundary_conditions/python_user_defined_bc/python_user_defined_bc.cc (the
// `SineBoundary` functor delegating to Python; `pybind11::embed`).

//! Embedding CPython in a Rust program (upstream's `pybind11::embed`, EX-012).
//!
//! Upstream's `python_user_defined_bc` example starts an interpreter
//! (`py::scoped_interpreter`) and implements a C++ Dirichlet functor whose
//! `operator()` calls a method of a Python object with the node's rows as
//! NumPy views (`VectorProxy` → pybind11 Eigen maps), so that the Python
//! code writes the displacement and the blocked flags in place.
//!
//! [`PythonDirichletFunctor`](crate::embed::PythonDirichletFunctor) is that functor for Rust programs: the
//! displacement and blocked-flag rows are handed over as **zero-copy
//! writable views** of the live model arrays (the same export protocol as
//! the Python package's own Dirichlet functors, D-047; no new `unsafe`: the
//! views come from [`crate::foreign_view`]), the coordinates as a read-only
//! array. Applying it (or anything else that may call Python) must happen
//! inside [`native`](crate::embed::native), the native section of the phase protocol, from a thread
//! attached to the interpreter ([`pyo3::Python::attach`]).

use akantu::core::array_shared::SharedArray;
use akantu::core::{Error, Idx, Real, Result};
use akantu::model::boundary_condition::DirichletFunctor;
use numpy::{PyArray1, PyArrayMethods};
use pyo3::prelude::*;

pub use pyo3;

use crate::foreign_view::ForeignView;
use crate::foreign_view::numpy_export::{Layout, to_numpy};
use crate::py::runtime::{callback, runtime};

/// A Dirichlet functor calling `obj.<method>(primal, coord, flags)` for
/// every node of the group (argument order of upstream's
/// `py_sin_boundary.attr("compute")(primal, coord, flags)`).
///
/// `primal` and `flags` are writable NumPy views of the node's rows of the
/// model's displacement and blocked-DOF arrays; `coord` is a read-only array
/// of the node's coordinates. A Python exception is returned as
/// [`Error::User`] (and re-raised in Python by [`native`]).
#[derive(Debug)]
pub struct PythonDirichletFunctor {
    obj: Py<PyAny>,
    method: String,
}

impl PythonDirichletFunctor {
    /// The functor of `obj.<method>`.
    #[must_use]
    pub fn new(obj: Py<PyAny>, method: impl Into<String>) -> Self {
        Self {
            obj,
            method: method.into(),
        }
    }
}

fn read_only<'py>(py: Python<'py>, v: &[Real]) -> PyResult<Bound<'py, PyAny>> {
    let a = PyArray1::from_slice(py, v);
    a.getattr("flags")?.setattr("writeable", false)?;
    Ok(a.into_any())
}

impl DirichletFunctor for PythonDirichletFunctor {
    /// Called on copies of the rows (not by `apply_dirichlet`, which uses
    /// [`DirichletFunctor::apply_shared`]): the copies are copied back.
    fn apply(
        &self,
        _node: Idx,
        flags: &mut [bool],
        primal: &mut [Real],
        coord: &[Real],
    ) -> Result<()> {
        let (f, p) = callback(|py| {
            let fl = PyArray1::from_slice(py, flags);
            let pr = PyArray1::from_slice(py, primal);
            let co = read_only(py, coord)?;
            self.obj
                .call_method1(py, self.method.as_str(), (&pr, &co, &fl))?;
            Ok((fl.to_vec()?, pr.to_vec()?))
        })?;
        if f.len() != flags.len() || p.len() != primal.len() {
            return Err(Error::invalid("the Python functor resized its row views"));
        }
        flags.copy_from_slice(&f);
        primal.copy_from_slice(&p);
        Ok(())
    }

    fn calls_foreign(&self) -> bool {
        true
    }

    /// Views of the live rows (upstream `VectorProxy` row views).
    fn apply_shared(
        &self,
        _node: Idx,
        row: usize,
        blocked: &mut SharedArray<bool>,
        primal: &mut SharedArray<Real>,
        coord: &[Real],
    ) -> Result<()> {
        let rt = runtime();
        let fv = ForeignView::new(blocked, rt, true)?.rows(row..row + 1)?;
        let pv = ForeignView::new(primal, rt, true)?.rows(row..row + 1)?;
        callback(|py| {
            let fl = to_numpy(py, fv, Layout::Tuple)?;
            let pr = to_numpy(py, pv, Layout::Tuple)?;
            let co = read_only(py, coord)?;
            self.obj
                .call_method1(py, self.method.as_str(), (pr, co, fl))?;
            Ok(())
        })
    }
}

/// Run native akantu code that may call Python functors (e.g.
/// `model.apply_dirichlet_bc(&python_functor, group)`) in a native section of
/// the interpreter's runtime; an error (including a Python exception raised
/// by a functor) becomes a Python exception.
///
/// # Errors
/// The error of `f`, as the Python exception upstream raises.
pub fn native<R, F>(py: Python<'_>, f: F) -> PyResult<R>
where
    F: FnOnce() -> Result<R> + Send,
    R: Send,
{
    crate::py::runtime::native(py, f)
}
