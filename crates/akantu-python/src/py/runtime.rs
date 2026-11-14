// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): python/py_akantu.cc (exception
// translator registration, `akantu.Exception`, `NLSNotConvergedException`).
// The binding-call protocol is akantu-rs design (docs/ARCHITECTURE.md, "Shared storage and its aliasing rules"; UD-085).

//! The per-interpreter [`ForeignRuntime`] and the protocol every binding
//! call follows (`docs/ARCHITECTURE.md`).
//!
//! * **One runtime per process, meant for the main interpreter.** The
//!   runtime is process-global; the GIL is its interpreter lock
//!   (`gil_used = true`). Sub-interpreters, as measured with PyO3 0.30.1
//!   and CPython 3.13: an *isolated* sub-interpreter
//!   (own GIL) refuses to load the extension ("does not support loading in
//!   subinterpreters"), but a *legacy* sub-interpreter (shared GIL) loads
//!   it — the top-level module declares no `Py_mod_multiple_interpreters`
//!   slot and PyO3's interpreter check covers submodules only — and then
//!   shares this runtime, PyO3's type objects and the akantu runtime with
//!   the main interpreter (the case PyO3 issue #576 warns about; not
//!   supported). `import akantu` does not reach that case today: NumPy
//!   refuses isolated sub-interpreters and SciPy legacy ones before the
//!   extension is loaded; only a direct load of the shared object does.
//! * **Native sections.** Native code that touches exported state runs in
//!   [`native`]: the closure runs inside `RT.enter()?.native()?` and must not
//!   touch Python (it is `Send`, so it cannot capture `Python<'_>` or
//!   `Bound` values). Python arguments are converted **before** the section
//!   (inputs are copied into owned buffers, so no Rust borrow of a NumPy
//!   buffer coexists with a native guard), Python results are built
//!   **after** it. The GIL is never released inside a section: this crate
//!   never calls `Python::detach` (enforced by `tests/binding_rules.rs`).
//! * **Callbacks.** The only way back into Python from inside a section is
//!   [`callback`], which switches the runtime to its foreign phase (refused
//!   while a native guard is alive) and is the only place that re-attaches
//!   to the interpreter.
//! * **Errors.** [`akantu::core::Error`] becomes `akantu.Exception` (or
//!   `akantu.NLSNotConvergedException` for [`Error::NotConverged`], as
//!   upstream's translator); a Python exception raised inside a callback
//!   travels through native code as [`Error::User`] and is re-raised with
//!   its original type and traceback at the binding boundary.

use std::cell::{Cell, RefCell};
use std::sync::{Arc, OnceLock};

use akantu::core::array_shared::ForeignRuntime;
use akantu::core::{Error, Result};
use pyo3::intern;
use pyo3::prelude::*;

use super::errors;
use crate::foreign_view::foreign_callback;
use crate::scoped::ContextKey;
use crate::scoped::python::current_frame;

static RUNTIME: OnceLock<ForeignRuntime> = OnceLock::new();

/// The interpreter's foreign runtime (every exported array is exported to
/// it).
pub(crate) fn runtime() -> &'static ForeignRuntime {
    RUNTIME.get_or_init(ForeignRuntime::new)
}

thread_local! {
    /// Nesting depth of [`native`] on this thread (callbacks may re-enter).
    static DEPTH: Cell<usize> = const { Cell::new(0) };
    /// Number of [`callback`]s running on this thread (a binding call made
    /// from Python code that a native call is calling back).
    static CALLBACKS: Cell<usize> = const { Cell::new(0) };
    /// Python exceptions raised in callbacks, keyed by the message of the
    /// [`Error::User`] that carries them through native code.
    static PENDING: RefCell<Vec<(String, PyErr)>> = const { RefCell::new(Vec::new()) };
}

struct DepthGuard;

impl DepthGuard {
    fn new() -> Self {
        DEPTH.with(|d| d.set(d.get() + 1));
        Self
    }
}

impl Drop for DepthGuard {
    fn drop(&mut self) {
        DEPTH.with(|d| d.set(d.get().saturating_sub(1)));
    }
}

/// Run the native part of a binding call in a native section.
///
/// `f` must not touch Python objects (it is `Send`, so it cannot capture
/// `Python<'_>`/`Bound`); errors are converted to Python exceptions after the
/// section ended. When the outermost section of the thread ends, carried
/// callback exceptions are dropped whatever the outcome
/// (an exception swallowed by native code is never re-raised by a later
/// call).
pub(crate) fn native<R, F>(py: Python<'_>, f: F) -> PyResult<R>
where
    F: FnOnce() -> Result<R> + Send,
    R: Send,
{
    let result = run_native(f).map_err(|e| to_py(py, e));
    if DEPTH.with(Cell::get) == 0 {
        PENDING.with(|p| p.borrow_mut().clear());
    }
    result
}

/// `true` while a [`callback`] is running on this thread: a binding call
/// made now comes from Python code called back by a running native call,
/// which may hold the objects it works on.
pub(crate) fn in_callback() -> bool {
    CALLBACKS.with(Cell::get) > 0
}

struct CallbackGuard;

impl CallbackGuard {
    fn new() -> Self {
        CALLBACKS.with(|c| c.set(c.get() + 1));
        Self
    }
}

impl Drop for CallbackGuard {
    fn drop(&mut self) {
        CALLBACKS.with(|c| c.set(c.get().saturating_sub(1)));
    }
}

fn run_native<R, F>(f: F) -> Result<R>
where
    F: FnOnce() -> Result<R> + Send,
    R: Send,
{
    let mut interp = runtime().enter()?;
    let section = interp.native()?;
    let depth = DepthGuard::new();
    let r = f();
    drop(depth);
    drop(section);
    drop(interp);
    r
}

/// Call into Python from inside a native section (trampoline). Fails —
/// without running `f` — if a native guard of an exported array is alive.
/// A Python exception is carried as [`Error::User`] and restored by
/// [`to_py`] at the boundary.
pub(crate) fn callback<R>(f: impl FnOnce(Python<'_>) -> PyResult<R>) -> Result<R> {
    foreign_callback(runtime(), |_interp| {
        let _depth = CallbackGuard::new();
        Python::attach(|py| match f(py) {
            Ok(v) => Ok(v),
            Err(e) => {
                let msg = format!("Python callback raised {e}");
                PENDING.with(|p| p.borrow_mut().push((msg.clone(), e)));
                Err(Error::User(msg))
            }
        })
    })?
}

/// The execution context of the lends made for one Python callback:
/// the Python frame that was running when the binding call
/// whose native code calls back was made. That frame stays suspended inside
/// the binding call for the whole lend, and a frame that is executing is in
/// the frame chain of exactly one execution context — the one whose C stack
/// carries the binding call (stack-switching libraries such as greenlet save
/// and restore the thread state's frame chain with the C stack and start a
/// new context with an empty chain; a generator or coroutine frame runs on
/// the stack and in the chain of whoever resumes it). So code that finds
/// this frame in its own chain runs nested inside the lending call, on the
/// stack that holds the lent values; any other code — another thread,
/// another greenlet, a generator or coroutine resumed outside the callback —
/// does not find it and is refused like another thread.
#[derive(Debug)]
pub(crate) struct LendContext {
    key: ContextKey,
    /// `None` if no Python frame was running (a binding called directly from
    /// C code): every use is then refused.
    frame: Option<Py<PyAny>>,
}

impl LendContext {
    /// The context of the running Python code (call it from inside
    /// [`callback`], before the Python code of the callback runs).
    pub(crate) fn current(py: Python<'_>) -> Self {
        Self {
            key: ContextKey::fresh(),
            frame: current_frame(py).map(Bound::unbind),
        }
    }

    /// The key of the lends of this context.
    pub(crate) fn key(&self) -> ContextKey {
        self.key
    }

    /// The key of this context if the calling code runs inside its lending
    /// call (the lending frame is in the running frame chain), otherwise the
    /// error of a use from another execution context.
    pub(crate) fn admit(&self, py: Python<'_>) -> Result<ContextKey> {
        if self.runs_inside(py) {
            Ok(self.key)
        } else {
            Err(foreign_context())
        }
    }

    fn runs_inside(&self, py: Python<'_>) -> bool {
        let Some(lending) = &self.frame else {
            return false;
        };
        let lending = lending.bind(py);
        let Some(mut frame) = current_frame(py) else {
            return false;
        };
        let frame_type = frame.get_type();
        // A frame chain is finite and acyclic. `f_back` is a read-only
        // attribute of the immutable built-in frame type: no user code runs.
        loop {
            if frame.is(lending) {
                return true;
            }
            match frame.getattr(intern!(py, "f_back")) {
                Ok(back) if !back.is_none() && back.get_type().is(&frame_type) => frame = back,
                _ => return false,
            }
        }
    }
}

/// The error of a binding call that would reach state lent to a Python
/// callback from another execution context.
pub(crate) fn foreign_context() -> Error {
    Error::invalid(
        "the model is in use by a running call that called a Python callback, and this call \
         was made from another execution context than that callback (another thread, a \
         greenlet, or a generator / coroutine resumed outside the callback): the running \
         call's state is reachable only from inside its callback",
    )
}

/// [`callback`] with lends: `f` runs inside the callback and gets the
/// execution context that every lend it makes for the Python code it calls
/// must belong to.
pub(crate) fn lending_callback<R>(
    f: impl FnOnce(Python<'_>, &Arc<LendContext>) -> PyResult<R>,
) -> Result<R> {
    callback(|py| {
        let cx = Arc::new(LendContext::current(py));
        f(py, &cx)
    })
}

fn take_pending(err: &Error) -> Option<PyErr> {
    let mut cur: Option<&(dyn std::error::Error + 'static)> = Some(err);
    while let Some(e) = cur {
        if let Some(Error::User(msg)) = e.downcast_ref::<Error>() {
            let found = PENDING.with(|p| {
                let mut p = p.borrow_mut();
                p.iter().rposition(|(m, _)| m == msg).map(|i| p.remove(i).1)
            });
            if found.is_some() {
                return found;
            }
        }
        cur = e.source();
    }
    // Native code may have wrapped the carried error into a new message
    // (e.g. "Error applying a boundary condition onto …! [<inner>]"): match
    // the carried message inside the rendered error, most recent first
    // (upstream re-raises the original Python exception, pybind11
    // `error_already_set`).
    let text = err.to_string();
    PENDING.with(|p| {
        let mut p = p.borrow_mut();
        p.iter()
            .rposition(|(m, _)| text.contains(m.as_str()))
            .map(|i| p.remove(i).1)
    })
}

/// Convert an akantu error to the Python exception upstream raises.
pub(crate) fn to_py(py: Python<'_>, err: Error) -> PyErr {
    let restored = take_pending(&err);
    if DEPTH.with(Cell::get) == 0 {
        // Outermost boundary: drop stale carried exceptions (e.g. a callback
        // error that native code handled itself).
        PENDING.with(|p| p.borrow_mut().clear());
    }
    if let Some(e) = restored {
        return e;
    }
    match &err {
        Error::NotConverged { .. } => errors::nls_not_converged(py, err.to_string()),
        _ => errors::exception(py, err.to_string()),
    }
}

/// `Result<T>` → `PyResult<T>` outside native sections.
pub(crate) trait IntoPy<T> {
    fn py_err(self, py: Python<'_>) -> PyResult<T>;
}

impl<T> IntoPy<T> for Result<T> {
    fn py_err(self, py: Python<'_>) -> PyResult<T> {
        self.map_err(|e| to_py(py, e))
    }
}

#[cfg(test)]
mod tests {
    use pyo3::types::PyDict;

    use super::*;
    use crate::py::test_common::lend_context_probe;

    /// A lend made while no Python frame runs (a binding called directly
    /// from C) admits no use at all.
    #[test]
    fn a_lend_without_a_python_frame_admits_nothing() {
        Python::initialize();
        Python::attach(|py| {
            let cx = LendContext::current(py);
            assert!(cx.frame.is_none());
            assert!(cx.admit(py).is_err());
        });
    }

    /// The frame-chain check of [`LendContext::admit`]:
    /// admitted inside the call that lent and in code it calls,
    /// refused after it returned, in a generator resumed outside it and in
    /// another thread while it runs.
    #[test]
    fn a_lend_admits_exactly_the_code_running_inside_the_lending_call() {
        Python::initialize();
        Python::attach(|py| {
            let g = PyDict::new(py);
            let f = pyo3::wrap_pyfunction!(lend_context_probe, py).unwrap();
            g.set_item("probe", f).unwrap();
            py.run(
                cr#"
import threading

def lend():
    p = probe()
    def nested():
        return p.admits()
    out = []
    t = threading.Thread(target=lambda: out.append(p.admits()))
    t.start()
    t.join()
    def gen():
        while True:
            yield p.admits()
    it = gen()
    return p, (p.admits(), nested(), out[0], next(it)), it

p, inside, it = lend()
after = (p.admits(), next(it))
"#,
                Some(&g),
                None,
            )
            .unwrap();
            let inside: (bool, bool, bool, bool) =
                g.get_item("inside").unwrap().unwrap().extract().unwrap();
            assert_eq!(
                inside,
                (true, true, false, true),
                "(call, nested, thread, generator)"
            );
            let after: (bool, bool) = g.get_item("after").unwrap().unwrap().extract().unwrap();
            assert_eq!(
                after,
                (false, false),
                "(after the call, generator resumed outside)"
            );
        });
    }
}
