// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Upstream
// `akantu::finalize()` (src/common/aka_common.cc) with the port's release of
// the process-wide PETSc world.
// Its own test binary: `runtime::finalize` ends the runtime.

//! `runtime::finalize` returns `Error::Communication` when the process-wide
//! world of the PETSc DOF managers of serial meshes cannot be released
//! (another thread is creating it): the runtime's communicator is released
//! anyway, MPI is not finalised, and a second call completes. The other
//! thread is simulated without a race by the test-only hook
//! `akantu_solver::model_solver::hold_petsc_world_for_test`, taken in this
//! thread before the call (with the `petsc` feature it holds the world's
//! lock itself, which `release_petsc_world` only tries).

use akantu::core::Error;
use akantu::runtime;

#[test]
fn finalize_reports_an_unreleased_petsc_world_and_completes_on_a_second_call() {
    let hold = akantu::solver::model_solver::hold_petsc_world_for_test();
    let e = runtime::finalize().expect_err("the PETSc world is held");
    assert!(matches!(e, Error::Communication(_)), "{e:?}");
    assert!(
        format!("{e}").contains("not released and MPI was not finalised"),
        "{e}"
    );
    // the runtime's communicator was released all the same
    assert!(!runtime::global().read().unwrap().communicator.is_mpi());
    drop(hold);
    runtime::finalize().expect("the second call completes");
    // idempotent
    runtime::finalize().unwrap();
}
