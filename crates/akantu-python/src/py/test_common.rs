// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): test/test_python_interface/test_common.cc
// (module `py11_akantu_test_common`: createArray/getArray/copyArray/
// getRawPointerArray and the Vector/Matrix variants). The GIL, callback and
// staleness hooks are akantu-rs additions for the acceptance tests
// (`docs/ARCHITECTURE.md`; INT-008).

//! `akantu.py11_akantu._test_common` — test support (TEST-153, PY-005/006).
//!
//! The upstream test module keeps `std::shared_ptr`s of arrays, vectors and
//! matrices in global maps keyed by their data address and returns them by
//! reference (zero-copy NumPy views) or by copy. Here the registry holds
//! [`SharedArray`]s behind [`SharedOwner`] borrow flags and returns views
//! through the foreign-view protocol, so the same Python tests exercise the
//! real binding path. Additional hooks drive the runtime from Python:
//! callbacks from inside native sections (re-entering Rust while views are
//! held), callbacks refused while a native guard is alive, storage growth
//! and shrink (stale views), output-buffer calls (`require_attached`) and
//! exceptions crossing the boundary in both directions.
//!
//! Like upstream's, the registry never frees its arrays (test-only module).

use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError};

use akantu::core::array_shared::SharedArray;
use akantu::core::{Array, Error};
use pyo3::prelude::*;
use pyo3::types::PyTuple;

use akantu::solver::callback::{Delegate, InterceptCallback};

use super::arrays::{copy_out, export, export_view};
use super::runtime::{IntoPy, callback, native, runtime, to_py};
use super::solid::{PySolidMechanicsModel, mo};
use crate::foreign_view::numpy_export::{AnyView, ArrayLease, Layout};
use crate::foreign_view::{ForeignView, SharedOwner};

type Owner = Arc<SharedOwner<SharedArray<f64>>>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Array,
    Vector,
    Matrix,
}

impl Kind {
    fn layout(self) -> Layout {
        match self {
            Self::Array => Layout::Rows,
            Self::Vector => Layout::Vector,
            Self::Matrix => Layout::ColumnMajor,
        }
    }
}

static REGISTRY: Mutex<Option<HashMap<usize, (Kind, Owner)>>> = Mutex::new(None);

fn lookup(py: Python<'_>, addr: usize, kind: Kind) -> PyResult<Owner> {
    let reg = REGISTRY.lock().unwrap_or_else(PoisonError::into_inner);
    match reg.as_ref().and_then(|m| m.get(&addr)) {
        Some((k, o)) if *k == kind => Ok(Arc::clone(o)),
        _ => Err(to_py(
            py,
            Error::not_found("test array", format!("{addr:#x}")),
        )),
    }
}

fn data_address(obj: &Bound<'_, PyAny>) -> PyResult<usize> {
    obj.getattr("__array_interface__")?
        .get_item("data")?
        .get_item(0)?
        .extract()
}

/// Create a zeroed `(tuples, components)` array, register it under its data
/// address and return `(address, view)`.
fn create<'py>(
    py: Python<'py>,
    tuples: usize,
    components: usize,
    kind: Kind,
) -> PyResult<Bound<'py, PyTuple>> {
    let owner: Owner = Arc::new(SharedOwner::new(SharedArray::new(Array::zeros(
        tuples, components,
    ))));
    let view = {
        let mut a = owner.try_borrow_mut().py_err(py)?;
        export_view(py, &mut a, kind.layout(), true)?
    };
    let addr = data_address(&view)?;
    REGISTRY
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .get_or_insert_with(HashMap::new)
        .insert(addr, (kind, owner));
    PyTuple::new(py, [addr.into_pyobject(py)?.into_any(), view])
}

fn get<'py>(py: Python<'py>, addr: usize, kind: Kind) -> PyResult<Bound<'py, PyAny>> {
    let owner = lookup(py, addr, kind)?;
    let mut a = owner.try_borrow_mut().py_err(py)?;
    export(py, &mut a, kind.layout(), true)
}

fn copy<'py>(py: Python<'py>, addr: usize, kind: Kind) -> PyResult<Bound<'py, PyAny>> {
    let owner = lookup(py, addr, kind)?;
    let a = owner.try_borrow_mut().py_err(py)?;
    copy_out(py, &a, kind.layout())
}

#[pyfunction]
#[pyo3(name = "createArray")]
fn create_array(py: Python<'_>, size: usize, nb_components: usize) -> PyResult<Bound<'_, PyTuple>> {
    create(py, size, nb_components, Kind::Array)
}

#[pyfunction]
#[pyo3(name = "getArray")]
fn get_array(py: Python<'_>, addr: usize) -> PyResult<Bound<'_, PyAny>> {
    get(py, addr, Kind::Array)
}

/// Read-only zero-copy view (akantu-rs hook: `const Array &` getters).
#[pyfunction]
#[pyo3(name = "getArrayReadOnly")]
fn get_array_read_only(py: Python<'_>, addr: usize) -> PyResult<Bound<'_, PyAny>> {
    let owner = lookup(py, addr, Kind::Array)?;
    let mut a = owner.try_borrow_mut().py_err(py)?;
    export(py, &mut a, Layout::Rows, false)
}

#[pyfunction]
#[pyo3(name = "copyArray")]
fn copy_array(py: Python<'_>, addr: usize) -> PyResult<Bound<'_, PyAny>> {
    copy(py, addr, Kind::Array)
}

#[pyfunction]
#[pyo3(name = "createVector")]
fn create_vector(py: Python<'_>, size: usize) -> PyResult<Bound<'_, PyTuple>> {
    create(py, size, 1, Kind::Vector)
}

#[pyfunction]
#[pyo3(name = "getVector")]
fn get_vector(py: Python<'_>, addr: usize) -> PyResult<Bound<'_, PyAny>> {
    get(py, addr, Kind::Vector)
}

#[pyfunction]
#[pyo3(name = "copyVector")]
fn copy_vector(py: Python<'_>, addr: usize) -> PyResult<Bound<'_, PyAny>> {
    copy(py, addr, Kind::Vector)
}

/// `createMatrix(rows, cols)`: Fortran-ordered `(rows, cols)` view (Eigen
/// column-major storage: one column per tuple).
#[pyfunction]
#[pyo3(name = "createMatrix")]
fn create_matrix(py: Python<'_>, rows: usize, cols: usize) -> PyResult<Bound<'_, PyTuple>> {
    create(py, cols, rows, Kind::Matrix)
}

#[pyfunction]
#[pyo3(name = "getMatrix")]
fn get_matrix(py: Python<'_>, addr: usize) -> PyResult<Bound<'_, PyAny>> {
    get(py, addr, Kind::Matrix)
}

#[pyfunction]
#[pyo3(name = "copyMatrix")]
fn copy_matrix(py: Python<'_>, addr: usize) -> PyResult<Bound<'_, PyAny>> {
    copy(py, addr, Kind::Matrix)
}

/// `getRawPointer*(array)`: data address of the array as the binding
/// receives it (upstream: the `Array`/Eigen proxy's `data()`).
#[pyfunction]
#[pyo3(name = "getRawPointerArray")]
fn get_raw_pointer_array(array: &Bound<'_, PyAny>) -> PyResult<usize> {
    data_address(array)
}

// --- runtime hooks (akantu-rs) --------------------------------------------------

/// Run `f` from inside a native section through the trampoline
/// (`runtime::callback`) and return its result. The callback may re-enter
/// akantu (nested binding calls) and access views.
#[pyfunction]
#[pyo3(name = "callFromNative")]
fn call_from_native(py: Python<'_>, f: Py<PyAny>) -> PyResult<Py<PyAny>> {
    native(py, move || callback(|py| f.call0(py)))
}

/// Test-only: call `f` through the
/// trampoline and swallow its error inside the native section, as native
/// code that handles a callback failure itself would; the call then
/// succeeds (returns `true` if `f` failed).
#[pyfunction]
#[pyo3(name = "swallowCallbackError")]
fn swallow_callback_error(py: Python<'_>, f: Py<PyAny>) -> PyResult<bool> {
    native(py, move || Ok(callback(|py| f.call0(py)).is_err()))
}

/// Hold a native read guard of the registered array while trying to call
/// `f`: the trampoline must refuse (raises `akantu.Exception`, `f` not run).
#[pyfunction]
#[pyo3(name = "callWhileGuarded")]
fn call_while_guarded(py: Python<'_>, addr: usize, f: Py<PyAny>) -> PyResult<Py<PyAny>> {
    let owner = lookup(py, addr, Kind::Array)?;
    native(py, move || {
        let a = owner.try_borrow_mut()?;
        let guard = a.read()?;
        let r = callback(|py| f.call0(py));
        drop(guard);
        r
    })
}

/// Hold the owner's borrow flag (no element guard) while calling `f`:
/// re-entrant mutable access to the same owner from `f` must raise.
#[pyfunction]
#[pyo3(name = "callWhileBorrowed")]
fn call_while_borrowed(py: Python<'_>, addr: usize, f: Py<PyAny>) -> PyResult<Py<PyAny>> {
    let owner = lookup(py, addr, Kind::Array)?;
    native(py, move || {
        let _held = owner.try_borrow_mut()?;
        callback(|py| f.call0(py))
    })
}

/// Sum of the registered array, read natively (a native guard inside a
/// section): proves that Python writes reach the storage.
#[pyfunction]
#[pyo3(name = "nativeSum")]
fn native_sum(py: Python<'_>, addr: usize) -> PyResult<f64> {
    let owner = lookup(py, addr, Kind::Array)?;
    native(py, move || {
        let a = owner.try_borrow_mut()?;
        let r = a.read()?;
        Ok(r.as_slice().iter().sum())
    })
}

/// `array[:] = value` natively.
#[pyfunction]
#[pyo3(name = "nativeFill")]
fn native_fill(py: Python<'_>, addr: usize, value: f64) -> PyResult<()> {
    let owner = lookup(py, addr, Kind::Array)?;
    native(py, move || {
        let mut a = owner.try_borrow_mut()?;
        a.write()?.as_mut_slice().fill(value);
        Ok(())
    })
}

/// Resize the registered array to `size` tuples (growth beyond capacity or
/// any shrink retires exported blocks: old views become stale).
#[pyfunction]
#[pyo3(name = "resizeArray")]
fn resize_array(py: Python<'_>, addr: usize, size: usize) -> PyResult<()> {
    let owner = lookup(py, addr, Kind::Array)?;
    native(py, move || {
        let mut a = owner.try_borrow_mut()?;
        a.write()?.try_resize(size, 0.0)
    })
}

/// Output-buffer entry point: write `value` into the registered array
/// through `out`, which must be an attached view of it
/// (`ForeignView::require_attached`; the write goes through the owner's
/// guard, never through the view pointer).
#[pyfunction]
#[pyo3(name = "fillOutput")]
fn fill_output(py: Python<'_>, addr: usize, out: &Bound<'_, PyAny>, value: f64) -> PyResult<()> {
    let owner = lookup(py, addr, Kind::Array)?;
    let view = lease_view(out)?;
    native(py, move || {
        let mut a = owner.try_borrow_mut()?;
        view.require_attached(&a)?;
        a.write()?.as_mut_slice().fill(value);
        Ok(())
    })
}

/// The `float64` view behind a NumPy array created by the binding (walks
/// `.base` to the `_ArrayLease`).
fn lease_view(obj: &Bound<'_, PyAny>) -> PyResult<ForeignView<f64>> {
    let mut cur = obj.clone();
    loop {
        if let Ok(lease) = cur.cast::<ArrayLease>() {
            return match lease.get().view() {
                AnyView::F64(v) => Ok(v.clone()),
                _ => Err(pyo3::exceptions::PyTypeError::new_err(
                    "expected a float64 akantu array view",
                )),
            };
        }
        let base = cur.getattr("base")?;
        if base.is_none() {
            return Err(pyo3::exceptions::PyTypeError::new_err(
                "not an akantu array view",
            ));
        }
        cur = base;
    }
}

/// `_test_common.contactDetectorSummary(detector)` (PY-086): the
/// parameters a stand-alone `ContactDetector` parsed — `(detection type,
/// projection_tolerance, max_iterations, extension_tolerance, number of
/// positions, selector set)` — which upstream's Python API cannot read
/// back; `None` for a contact model's detector.
#[pyfunction]
#[pyo3(name = "contactDetectorSummary")]
fn contact_detector_summary(
    detector: &Bound<'_, super::physics::PyContactDetector>,
) -> Option<(String, f64, i64, f64, usize, bool)> {
    detector.get().standalone().map(|d| d.summary())
}

/// Raise an akantu error of the given kind from native code
/// (`"not_converged"` → `NLSNotConvergedException`, else
/// `akantu.Exception`).
#[pyfunction]
#[pyo3(name = "raiseNative")]
fn raise_native(py: Python<'_>, kind: &str) -> PyResult<()> {
    let kind = kind.to_owned();
    native(py, move || {
        Err(if kind == "not_converged" {
            Error::NotConverged {
                threshold: 1e-10,
                niter: 10,
                error: 1.0,
            }
        } else {
            Error::invalid("raised from native code")
        })
    })
}

/// `True` while the interpreter's runtime would refuse foreign accesses
/// (diagnostics: a native section is open or the runtime is poisoned).
#[pyfunction]
#[pyo3(name = "runtimeBusy")]
fn runtime_busy() -> bool {
    runtime().enter().is_err()
}

/// Calls a Python `hook()` from inside a solve step, before the model's
/// `phase` (test-only intercepting callback, see [`solve_step_with_hook`]).
struct HookCallback {
    hook: Py<PyAny>,
    phase: String,
    calls: usize,
}

impl HookCallback {
    fn fire(&mut self, at: &str) -> akantu::core::Result<()> {
        if at != self.phase {
            return Ok(());
        }
        self.calls += 1;
        let hook = &self.hook;
        callback(|py| hook.call0(py).map(drop))
    }
}

impl InterceptCallback for HookCallback {
    fn predictor(&mut self, model: &mut Delegate<'_>) -> akantu::core::Result<()> {
        self.fire("predictor")?;
        model.predictor()
    }
    fn assemble_residual(&mut self, model: &mut Delegate<'_>) -> akantu::core::Result<()> {
        self.fire("assemble_residual")?;
        model.assemble_residual()
    }
    fn corrector(&mut self, model: &mut Delegate<'_>) -> akantu::core::Result<()> {
        self.fire("corrector")?;
        model.corrector()
    }
    fn before_solve_step(&mut self, model: &mut Delegate<'_>) -> akantu::core::Result<()> {
        self.fire("before_solve_step")?;
        model.before_solve_step()
    }
    fn after_solve_step(
        &mut self,
        model: &mut Delegate<'_>,
        converged: bool,
    ) -> akantu::core::Result<()> {
        self.fire("after_solve_step")?;
        model.after_solve_step(converged)
    }
}

/// Test-only foundation check of the Python bindings: solve one step of `model` with an intercepting callback that
/// calls the Python callable `hook()` from inside the solve step, before the
/// model's `phase` (`predictor`, `assemble_residual`, `corrector`,
/// `before_solve_step`, `after_solve_step`), through the runtime
/// trampoline. No native guard is alive at that point, so the hook can read
/// and write the model's arrays through views (held or fetched inside the
/// hook). Returns the number of hook calls.
#[pyfunction]
#[pyo3(name = "solveStepWithHook", signature = (model, hook, phase = "assemble_residual".to_owned(), solver_id = String::new()))]
fn solve_step_with_hook(
    py: Python<'_>,
    model: &Bound<'_, PySolidMechanicsModel>,
    hook: Py<PyAny>,
    phase: String,
    solver_id: String,
) -> PyResult<usize> {
    mo(model).with(py, move |m| {
        let mut cb = HookCallback {
            hook,
            phase,
            calls: 0,
        };
        let id = (!solver_id.is_empty()).then_some(solver_id.as_str());
        m.solve_step_intercepted(&mut cb, id)?;
        Ok(cb.calls)
    })
}

/// Test-only probe of the execution-context check of lends:
/// records the context of the calling Python code exactly as a
/// lend for a callback called from there would (`LendContext::current`).
#[pyclass(
    frozen,
    name = "LendContextProbe",
    module = "akantu.py11_akantu._test_common"
)]
#[derive(Debug)]
pub(crate) struct LendContextProbe(super::runtime::LendContext);

#[pymethods]
impl LendContextProbe {
    /// `True` iff a use made now would be admitted: the calling code runs
    /// inside the call that created the probe (its frame is in the running
    /// frame chain).
    fn admits(&self, py: Python<'_>) -> bool {
        self.0.admit(py).is_ok()
    }
}

/// `lendContextProbe()`: a [`LendContextProbe`] of the calling code.
#[pyfunction]
#[pyo3(name = "lendContextProbe")]
pub(crate) fn lend_context_probe(py: Python<'_>) -> LendContextProbe {
    LendContextProbe(super::runtime::LendContext::current(py))
}

/// `callbackMatrixType(model, callback, matrix_id)`: the type the
/// trampoline of a Python solver callback reports for `matrix_id` through
/// native code, re-raising a carried Python exception
/// (see `solver::callback_matrix_type`).
#[pyfunction]
#[pyo3(name = "callbackMatrixType")]
fn callback_matrix_type<'py>(
    model: &Bound<'py, PySolidMechanicsModel>,
    callback: &Bound<'py, PyAny>,
    matrix_id: String,
) -> PyResult<Bound<'py, PyAny>> {
    super::solver::callback_matrix_type(model.py(), &mo(model), callback, matrix_id)
}

/// Register the sub-module.
pub(crate) fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    let t = PyModule::new(m.py(), "_test_common")?;
    t.setattr("__doc__", "Akantu Test function for common ")?;
    t.add_function(wrap_pyfunction!(create_array, &t)?)?;
    t.add_function(wrap_pyfunction!(get_array, &t)?)?;
    t.add_function(wrap_pyfunction!(copy_array, &t)?)?;
    t.add_function(wrap_pyfunction!(get_array_read_only, &t)?)?;
    t.add_function(wrap_pyfunction!(get_raw_pointer_array, &t)?)?;
    t.add_function(wrap_pyfunction!(create_vector, &t)?)?;
    t.add_function(wrap_pyfunction!(get_vector, &t)?)?;
    t.add_function(wrap_pyfunction!(copy_vector, &t)?)?;
    t.add("getRawPointerVector", t.getattr("getRawPointerArray")?)?;
    t.add_function(wrap_pyfunction!(create_matrix, &t)?)?;
    t.add_function(wrap_pyfunction!(get_matrix, &t)?)?;
    t.add_function(wrap_pyfunction!(copy_matrix, &t)?)?;
    t.add("getRawPointerMatrix", t.getattr("getRawPointerArray")?)?;
    t.add_function(wrap_pyfunction!(call_from_native, &t)?)?;
    t.add_function(wrap_pyfunction!(call_while_guarded, &t)?)?;
    t.add_function(wrap_pyfunction!(call_while_borrowed, &t)?)?;
    t.add_function(wrap_pyfunction!(native_sum, &t)?)?;
    t.add_function(wrap_pyfunction!(native_fill, &t)?)?;
    t.add_function(wrap_pyfunction!(resize_array, &t)?)?;
    t.add_function(wrap_pyfunction!(fill_output, &t)?)?;
    t.add_function(wrap_pyfunction!(raise_native, &t)?)?;
    t.add_function(wrap_pyfunction!(contact_detector_summary, &t)?)?;
    t.add_function(wrap_pyfunction!(runtime_busy, &t)?)?;
    t.add_function(wrap_pyfunction!(solve_step_with_hook, &t)?)?;
    t.add_function(wrap_pyfunction!(swallow_callback_error, &t)?)?;
    t.add_function(wrap_pyfunction!(lend_context_probe, &t)?)?;
    t.add_function(wrap_pyfunction!(callback_matrix_type, &t)?)?;
    t.add_class::<LendContextProbe>()?;
    #[cfg(feature = "mpi")]
    t.add_function(wrap_pyfunction!(mpi_lifecycle, &t)?)?;
    m.add_submodule(&t)?;
    Ok(())
}

/// `_test_common.mpiLifecycle()` (MPI build): the state of the
/// process's MPI lifecycle as akantu sees it — `("owned" | "imported",
/// live guards)` while active, `("finalized", 0)` once akantu finalised MPI
/// (or completed a pending finalisation), `("pending", 0)` while a
/// finalisation waits for the main thread, `("uninitialized", 0)` before
/// any use — for the import / exit lifecycle tests (no akantu handle may
/// outlive its owners).
#[cfg(feature = "mpi")]
#[pyfunction]
#[pyo3(name = "mpiLifecycle")]
fn mpi_lifecycle() -> (&'static str, usize) {
    use akantu::comm::lifecycle::InitOwnership;
    let l = akantu::comm::mpi_lifecycle();
    match l.active() {
        Some((InitOwnership::Owned, n)) => ("owned", n),
        Some((InitOwnership::Imported, n)) => ("imported", n),
        None if l.is_finalized() => ("finalized", 0),
        None if l.is_finalize_pending() => ("pending", 0),
        None => ("uninitialized", 0),
    }
}
