// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): python/py_akantu.cc:170-178 (`has_mpi`,
// `getVersion`), python/py_aka_common.cc:59-72,150-166
// (`__initialize`, `getElementTypes`), python/py_aka_error.cc:37-80
// (`DebugLevel`, `debug` sub-module, deprecated module-level aliases),
// python/py_fe_engine.cc:41-51 (`Element`, `ElementNull`),
// src/common/aka_common.cc (`initialize(argc, argv)`).

//! Module-level functions and small types (PY-003, PY-004, PY-007, PY-026,
//! PY-030, PY-056).

use std::sync::PoisonError;
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};

use akantu::core::random::{IdxSlot, RandomGenerator, atol, initial_seed};
use akantu::core::{Element, ElementKind, ElementType, GhostType};
use akantu::parser::SearchScope;
use akantu::parser::cli::{ArgumentParser, ParseFlags, register_akantu_options};
use pyo3::basic::CompareOp;
use pyo3::exceptions::PyDeprecationWarning;
use pyo3::prelude::*;
use pyo3::types::PyDict;

use super::arrays;
use super::enums::{DebugLevel, EnumMeta, EtArg, GhostArg, element_type_obj, ghost_type_obj};
use super::runtime::IntoPy;

// --- debug level ---------------------------------------------------------------

/// Upstream `debug::debugger` level (`dblWarning` = 4 by default after
/// `initialize`).
static DEBUG_LEVEL: AtomicI64 = AtomicI64::new(4);
/// Upstream `debug::debugger.printBacktrace()`.
static PRINT_BACKTRACE: AtomicBool = AtomicBool::new(false);

fn set_level(level: &Bound<'_, DebugLevel>) {
    DEBUG_LEVEL.store(level.get().value, Ordering::Relaxed);
}

fn get_level(py: Python<'_>) -> PyResult<Bound<'_, DebugLevel>> {
    DebugLevel::instance(py, DEBUG_LEVEL.load(Ordering::Relaxed))
}

fn deprecated(py: Python<'_>, what: &str) -> PyResult<()> {
    PyErr::warn(
        py,
        &py.get_type::<PyDeprecationWarning>(),
        &std::ffi::CString::new(format!(
            "{what}() is deprecated, it has moved in the submodule debug"
        ))
        .unwrap_or_default(),
        1,
    )
}

/// `setDebugLevel(level)` (deprecated module-level alias).
#[pyfunction]
#[pyo3(name = "setDebugLevel")]
fn set_debug_level_deprecated(level: &Bound<'_, DebugLevel>) -> PyResult<()> {
    set_level(level);
    deprecated(level.py(), "setDebugLevel")
}

/// `getDebugLevel()` (deprecated module-level alias).
#[pyfunction]
#[pyo3(name = "getDebugLevel")]
fn get_debug_level_deprecated(py: Python<'_>) -> PyResult<Bound<'_, DebugLevel>> {
    deprecated(py, "getDebugLevel")?;
    get_level(py)
}

/// `printBacktrace(flag)` (deprecated module-level alias).
#[pyfunction]
#[pyo3(name = "printBacktrace")]
fn print_backtrace_deprecated(py: Python<'_>, flag: bool) -> PyResult<()> {
    PRINT_BACKTRACE.store(flag, Ordering::Relaxed);
    deprecated(py, "printBacktrace")
}

/// `debug.setDebugLevel(level)`.
#[pyfunction]
#[pyo3(name = "setDebugLevel")]
fn set_debug_level(level: &Bound<'_, DebugLevel>) {
    set_level(level);
}

/// `debug.getDebugLevel()`.
#[pyfunction]
#[pyo3(name = "getDebugLevel")]
fn get_debug_level(py: Python<'_>) -> PyResult<Bound<'_, DebugLevel>> {
    get_level(py)
}

/// `debug.printBacktrace(flag)`.
#[pyfunction]
#[pyo3(name = "printBacktrace")]
fn print_backtrace(flag: bool) {
    PRINT_BACKTRACE.store(flag, Ordering::Relaxed);
}

// --- module functions ------------------------------------------------------------

/// `has_mpi()`: whether the module is the MPI build (feature `mpi`,
/// D-101 (1); upstream: `AKANTU_USE_MPI`). The default wheel is
/// sequential (`False`), as upstream's published one.
#[pyfunction]
fn has_mpi() -> bool {
    cfg!(feature = "mpi")
}

/// The static communicator of the MPI build (D-101 (1)):
/// upstream's `initialize(argc, argv)` creates it
/// (`Communicator::getStaticCommunicator`). Installed in the runtime by
/// [`akantu::runtime::install_static_communicator`]: the MPI world, MPI
/// being initialised by mpi4py first (`akantu/__init__.py` imports it when
/// `has_mpi()`) or by the module (D-015); its release is registered with
/// the process-exit hook (D-067: MPI finalised after a normal end only if
/// akantu initialised it, never a second `MPI_Finalize`). An `atexit`
/// callback releases the runtime's handle while the interpreter is still
/// alive ([`at_exit`]). Returns `(rank, size)`.
#[cfg(feature = "mpi")]
fn install_communicator(py: Python<'_>) -> PyResult<(akantu::core::Rank, usize)> {
    use std::sync::atomic::AtomicBool;
    static AT_EXIT: AtomicBool = AtomicBool::new(false);
    let (rank, size) = super::runtime::native(py, || {
        let c = akantu::runtime::install_static_communicator()?;
        Ok((c.rank(), c.size()))
    })?;
    if !AT_EXIT.swap(true, Ordering::AcqRel) {
        let f = wrap_pyfunction!(at_exit, py)?;
        py.import("atexit")?.call_method1("register", (f,))?;
    }
    Ok((rank, size))
}

/// The module's `atexit` callback (MPI build): `akantu::runtime::finalize`
/// — the runtime's communicator is released (`MPI_Comm_free` of its
/// duplicate of the world while MPI is alive, so that no akantu handle is
/// left when mpi4py finalises MPI after the interpreter's teardown) and an
/// akantu-owned environment is finalised once its last handle is gone
/// (meshes still referenced keep it alive until the teardown frees them;
/// the exit hook covers the rest). It runs on the main thread (Python runs
/// `atexit` callbacks there). An error is reported, not raised (the
/// interpreter is exiting).
#[cfg(feature = "mpi")]
#[pyfunction]
#[pyo3(name = "_mpi_at_exit")]
fn at_exit(py: Python<'_>) {
    if let Err(e) = super::runtime::native(py, akantu::runtime::finalize) {
        e.print(py);
    }
}

/// `getVersion()`: the port's version followed by the upstream version it
/// tracks.
#[pyfunction]
#[pyo3(name = "getVersion")]
fn get_version() -> String {
    format!("{}+akantu.{}", akantu::VERSION, akantu::UPSTREAM_VERSION)
}

/// `__initialize()`: upstream `initialize(argc, argv)` with `sys.argv`:
/// parses the akantu options (`--aka_input_file`, `--aka_debug_level`,
/// `--aka_print_backtrace`, `--aka_seed`; `sys.argv` itself is not
/// modified), reads the input file if one is given, seeds the global random
/// generator (`AKA_SEED`, else `--aka_seed`, else the parser's `seed`, else
/// the time) and sets the debug level (`AKA_DEBUG_LEVEL` overrides).
#[pyfunction]
#[pyo3(name = "__initialize")]
fn initialize(py: Python<'_>) -> PyResult<()> {
    // upstream: the static communicator first (`getStaticCommunicator`),
    // whose rank gives the argument parser's context and the seed factor
    #[cfg(feature = "mpi")]
    let (rank, size) = install_communicator(py)?;
    #[cfg(not(feature = "mpi"))]
    let (rank, size): (akantu::core::Rank, usize) = (0, 1);
    let mut argv: Vec<String> = py.import("sys")?.getattr("argv")?.extract()?;
    let mut args = ArgumentParser::default();
    args.set_parallel_context(rank, i32::try_from(size).unwrap_or(i32::MAX));
    register_akantu_options(&mut args).py_err(py)?;
    args.parse_argv(&mut argv, ParseFlags::REMOVE_PARSED, false)
        .py_err(py)?;
    let infile = args.get_string("aka_input_file").unwrap_or_default();
    PRINT_BACKTRACE.store(
        args.get_bool("aka_print_backtrace").unwrap_or(false),
        Ordering::Relaxed,
    );
    if !infile.is_empty() {
        akantu::runtime::parse_input(&infile).py_err(py)?;
    }
    let env_seed = std::env::var("AKA_SEED").ok();
    let cli_seed = if args.has("aka_seed") {
        Some(args.get_int("aka_seed").py_err(py)?)
    } else {
        None
    };
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX));
    // `static_parser.getParameter("seed", time(nullptr), _ppsc_current_scope)`.
    let file_seed: i64 = akantu::runtime::global()
        .read()
        .unwrap_or_else(PoisonError::into_inner)
        .parser
        .root
        .get_parameter_or("seed", now, SearchScope::Current)
        .py_err(py)?;
    let seed = initial_seed(env_seed.as_deref(), cli_seed, Some(file_seed), now, rank);
    RandomGenerator::<IdxSlot>::seed(seed);
    let mut level = args
        .get_int("aka_debug_level")
        .unwrap_or_else(|_| DEBUG_LEVEL.load(Ordering::Relaxed));
    if let Ok(s) = std::env::var("AKA_DEBUG_LEVEL") {
        level = atol(&s);
    }
    DEBUG_LEVEL.store(level, Ordering::Relaxed);
    {
        let mut rt = akantu::runtime::global()
            .write()
            .unwrap_or_else(PoisonError::into_inner);
        rt.arguments = args;
    }
    Ok(())
}

/// `getElementTypes()`: `{name: ElementType}` of the regular element types.
#[pyfunction]
#[pyo3(name = "getElementTypes")]
fn get_element_types(py: Python<'_>) -> PyResult<Bound<'_, PyDict>> {
    let d = PyDict::new(py);
    let mut types: Vec<ElementType> = ElementType::of_kind(ElementKind::Regular).collect();
    // `std::map<std::string, ElementType>`: keys in byte order.
    types.sort_by_key(|t| t.name());
    for t in types {
        d.set_item(t.name(), element_type_obj(py, t)?)?;
    }
    Ok(d)
}

/// `setArrayCopyMode(flag)` (port extension, `docs/ARCHITECTURE.md`):
/// getters return independent copies instead of zero-copy views.
#[pyfunction]
#[pyo3(name = "setArrayCopyMode")]
fn set_array_copy_mode(flag: bool) {
    arrays::set_copy_mode(flag);
}

/// `getArrayCopyMode()` (port extension).
#[pyfunction]
#[pyo3(name = "getArrayCopyMode")]
fn get_array_copy_mode() -> bool {
    arrays::copy_mode()
}

// --- Element -------------------------------------------------------------------------

/// `Element(type, element, ghost_type=_not_ghost)` (PY-056).
#[pyclass(frozen, name = "Element", module = "akantu.py11_akantu")]
#[derive(Clone, Copy, Debug)]
pub(crate) struct PyElement {
    pub(crate) element: Element,
}

#[pymethods]
impl PyElement {
    /// Upstream names the arguments `(type, ghost_type, ghost_type)` (a
    /// typo: the second is the element index); the port uses
    /// `(type, element, ghost_type)`.
    #[new]
    #[pyo3(signature = (r#type, element, ghost_type = GhostArg(Some(GhostType::NotGhost))))]
    fn new(r#type: EtArg, element: i64, ghost_type: GhostArg) -> PyResult<Self> {
        let ghost = ghost_type.single()?;
        let index = i32::try_from(element).map_err(|_| {
            pyo3::exceptions::PyOverflowError::new_err("element index out of range")
        })?;
        // Upstream stores the triple as given (`Element{type, id, ghost}`);
        // a `_not_defined` type or a negative index is the null element.
        Ok(Self {
            element: Element::new(r#type.0, index, ghost),
        })
    }

    fn __repr__(&self) -> String {
        self.element.to_string()
    }

    /// Element type (port extension).
    #[getter(r#type)]
    fn ty<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        Ok(element_type_obj(py, self.element.ty())?.into_any())
    }

    /// Element index (port extension).
    #[getter]
    fn element(&self) -> i32 {
        self.element.index()
    }

    /// Ghost type (port extension).
    #[getter]
    fn ghost_type<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        Ok(ghost_type_obj(py, self.element.ghost_type())?.into_any())
    }

    fn __richcmp__(&self, other: &Bound<'_, PyAny>, op: CompareOp) -> PyResult<Py<PyAny>> {
        let py = other.py();
        // Upstream defines only `__lt__`; equality is identity (pybind11
        // default), which `NotImplemented` reproduces.
        match op {
            CompareOp::Lt => match other.cast::<Self>() {
                Ok(o) => Ok(pyo3::types::PyBool::new(py, self.element < o.get().element)
                    .to_owned()
                    .into_any()
                    .unbind()),
                Err(_) => Ok(py.NotImplemented()),
            },
            _ => Ok(py.NotImplemented()),
        }
    }
}

/// Register the module-level functions (`register_initialize`,
/// `register_error`, `register_functions`, `has_mpi`, `getVersion`).
pub(crate) fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    let py = m.py();
    m.add_function(wrap_pyfunction!(initialize, m)?)?;
    super::enums::register_enum::<DebugLevel>(m, true)?;
    let debug = PyModule::new(py, "debug")?;
    debug.add_function(wrap_pyfunction!(set_debug_level, &debug)?)?;
    debug.add_function(wrap_pyfunction!(get_debug_level, &debug)?)?;
    debug.add_function(wrap_pyfunction!(print_backtrace, &debug)?)?;
    m.add_submodule(&debug)?;
    m.add_function(wrap_pyfunction!(set_debug_level_deprecated, m)?)?;
    m.add_function(wrap_pyfunction!(get_debug_level_deprecated, m)?)?;
    m.add_function(wrap_pyfunction!(print_backtrace_deprecated, m)?)?;
    m.add_function(wrap_pyfunction!(get_element_types, m)?)?;
    m.add_function(wrap_pyfunction!(set_array_copy_mode, m)?)?;
    m.add_function(wrap_pyfunction!(get_array_copy_mode, m)?)?;
    Ok(())
}

/// Register `Element`/`ElementNull` (`register_fe_engine`, first part).
pub(crate) fn register_element(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PyElement>()?;
    m.add(
        "ElementNull",
        Bound::new(
            m.py(),
            PyElement {
                element: Element::NULL,
            },
        )?,
    )?;
    Ok(())
}

/// Register `has_mpi`/`getVersion` (end of `PYBIND11_MODULE`).
pub(crate) fn register_tail(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(has_mpi, m)?)?;
    m.add_function(wrap_pyfunction!(get_version, m)?)?;
    Ok(())
}
