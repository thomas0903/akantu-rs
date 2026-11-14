// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/model/common/non_linear_solver/
// non_linear_solver_petsc.cc:107-120 (FormFunction / FormJacobian through
// the SNES context pointer), non_linear_solver_tao.cc:160-176
// (FormFunctionGradient / FormHessian).

//! `extern "C"` trampolines of the native non-linear solvers. PETSc calls
//! the shim's static callbacks, which pass the **owned** rows of the trial
//! point (and of the output vector) to these functions; they forward to a
//! Rust [`NativeCallbacks`] object and return its status. A panic is caught
//! (never unwinds into C) and reported as a nonzero status; it is also
//! recorded in a thread-local flag ([`take_caught_panic`]):
//! the model's callbacks run in agreed steps whose panics are caught and
//! agreed before they reach a trampoline, so a panic
//! caught here comes from the back-end's own glue code on this rank only —
//! the other ranks may be blocked in PETSc's collectives, and the driver
//! then ends the job (`docs/ARCHITECTURE.md`, `abort_on_transport_fatal`).

use std::cell::Cell;
use std::ffi::{c_int, c_void};
use std::panic::{AssertUnwindSafe, catch_unwind};

thread_local! {
    /// A trampoline on this thread caught a panic.
    static CAUGHT_PANIC: Cell<bool> = const { Cell::new(false) };
}

/// Whether a trampoline on this thread caught a panic since the last call;
/// clears the flag. A native driver clears it before its solve and reads it
/// when PETSc returns an error that no agreed callback failure explains.
pub fn take_caught_panic() -> bool {
    CAUGHT_PANIC.with(|c| c.replace(false))
}

/// Callbacks of a native non-linear solve (SNES / TAO), over the owned rows
/// of this rank in PETSc order (= the owned local equations in local
/// order). Each returns `true` on success; a failure must be **identical on
/// every rank** (agreed by the implementation), because PETSc's control
/// flow branches on it and a divergent branch would deadlock its
/// collectives. The error itself is kept by the implementation and
/// re-raised after the native solve.
pub trait NativeCallbacks {
    /// `F(x)` into `f` (SNES function; TAO: not used).
    fn residual(&mut self, x: &[f64], f: &mut [f64]) -> bool;
    /// Assemble the Jacobian / Hessian at `x` into the backend's PETSc
    /// matrix (the one given to the solver).
    fn jacobian(&mut self, x: &[f64]) -> bool;
    /// TAO objective at `x` and its gradient into `grad`; `None` on
    /// failure.
    fn objective(&mut self, x: &[f64], grad: &mut [f64]) -> Option<f64>;
}

/// The `user` pointer handed to the shim: a pointer to this (stack-held)
/// fat reference.
pub(crate) type UserData<'a> = &'a mut dyn NativeCallbacks;

/// Rebuild the callback object and the slices from the shim's arguments.
///
/// # Safety
///
/// `user` points to a live `UserData` (the frame of the native solve that
/// passed it, which outlives the solve) used by no one else during the
/// call; `x` (and `out` when non-null) point to `n` valid `f64`s for the
/// duration of the call (PETSc's owned arrays, borrowed by the shim).
unsafe fn with_user<R>(
    user: *mut c_void,
    n: i64,
    f: impl FnOnce(&mut dyn NativeCallbacks, usize) -> R,
) -> Option<R> {
    let n = usize::try_from(n).ok()?;
    if user.is_null() {
        return None;
    }
    // SAFETY: per the function contract, `user` is a unique pointer to a
    // live `UserData`.
    let cb: &mut UserData<'_> = unsafe { &mut *user.cast::<UserData<'_>>() };
    Some(f(&mut **cb, n))
}

/// Shim residual callback (SNES `FormFunction`).
pub(crate) unsafe extern "C" fn residual(
    user: *mut c_void,
    n: i64,
    x: *const f64,
    f: *mut f64,
) -> c_int {
    let r = catch_unwind(AssertUnwindSafe(|| {
        // SAFETY: the shim passes the `user` pointer given to
        // `akantu_petsc_snes_solve` and PETSc's owned arrays of `x` and `f`
        // (length `n`, `x` read-only, `f` writable, distinct vectors) for
        // the duration of this call (`with_user` contract).
        unsafe {
            with_user(user, n, |cb, n| {
                if x.is_null() || f.is_null() {
                    return false;
                }
                let x = std::slice::from_raw_parts(x, n);
                let f = std::slice::from_raw_parts_mut(f, n);
                cb.residual(x, f)
            })
        }
    }));
    status(r)
}

/// Shim Jacobian / Hessian callback (SNES `FormJacobian`, TAO
/// `FormHessian`).
pub(crate) unsafe extern "C" fn jacobian(user: *mut c_void, n: i64, x: *const f64) -> c_int {
    let r = catch_unwind(AssertUnwindSafe(|| {
        // SAFETY: as in `residual`: the solve's `user` pointer and PETSc's
        // owned array of `x` (length `n`) for the duration of the call.
        unsafe {
            with_user(user, n, |cb, n| {
                if x.is_null() {
                    return false;
                }
                cb.jacobian(std::slice::from_raw_parts(x, n))
            })
        }
    }));
    status(r)
}

/// Shim objective callback (TAO `FormFunctionGradient`).
pub(crate) unsafe extern "C" fn objective(
    user: *mut c_void,
    n: i64,
    x: *const f64,
    obj: *mut f64,
    grad: *mut f64,
) -> c_int {
    let r = catch_unwind(AssertUnwindSafe(|| {
        // SAFETY: the solve's `user` pointer, PETSc's owned arrays of `x`
        // and `grad` (length `n`, distinct vectors) and the shim's local
        // `obj` for the duration of the call.
        unsafe {
            with_user(user, n, |cb, n| {
                if x.is_null() || grad.is_null() || obj.is_null() {
                    return false;
                }
                let x = std::slice::from_raw_parts(x, n);
                let g = std::slice::from_raw_parts_mut(grad, n);
                match cb.objective(x, g) {
                    Some(v) => {
                        *obj = v;
                        true
                    }
                    None => false,
                }
            })
        }
    }));
    status(r)
}

fn status(r: std::thread::Result<Option<bool>>) -> c_int {
    match r {
        Ok(Some(true)) => 0,
        Ok(_) => 1,
        Err(_) => {
            CAUGHT_PANIC.with(|c| c.set(true));
            1
        }
    }
}
