// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): contract derived from
// src/model/common/dof_manager/dof_manager_petsc.{hh,cc} (AO numbering,
// :167-235), src/solver/sparse_matrix_petsc.cc:262-268 (MatSetValues),
// fixing UD-006 (matrices and vectors must use one numbering), and
// src/model/common/non_linear_solver/non_linear_solver_petsc.cc:100-200
// (SNES FormFunction/FormJacobian, solve lifecycle).

//! The DOF manager works unchanged over a backend
//! with native storage and its own numbering, **through the solve**: linear
//! solves and a native Newton (SNES stand-in) run through the manager and
//! the callback path with the backend-owned matrix — no COO access, no
//! separate native matrix — serially and on two ranks (owned/shared maps).

mod common;

use std::sync::Arc;
use std::time::{Duration, Instant};

use akantu_comm::{Communicator, run_local};
use akantu_core::enums::MatrixType;
use akantu_core::{Array, GhostType};
use akantu_solver::{AijBackend, BackendKind, DofManager, NativeNonLinear, NonLinearSolver};
use common::{DenseDirect, Log, MockNative, SEG, Springs, newton, serial_bar, two_rank_bar};

#[test]
fn dof_manager_is_backend_agnostic() {
    let mesh = serial_bar(2);
    let log: Log = Arc::default();
    let mut dm = DofManager::with_backend(
        "dm",
        Communicator::serial(),
        Box::new(MockNative::new(Arc::clone(&log))),
    );
    dm.register_nodal_dofs(&mesh, "u", 1).unwrap();
    dm.new_matrix("K", MatrixType::Symmetric).unwrap();
    let mut k = Array::new(4);
    k.push_tuple(&[1., -1., -1., 1.]);
    k.push_tuple(&[1., -1., -1., 1.]);
    dm.assemble_elemental_matrices_to_matrix(
        &mesh,
        "K",
        "u",
        k.view(),
        SEG,
        GhostType::NotGhost,
        MatrixType::Symmetric,
        None,
    )
    .unwrap();
    dm.finalize_matrix("K").unwrap();
    assert!(dm.matrix("K").is_err(), "no COO view on a native backend");
    assert_eq!(dm.backend().kind(), BackendKind::Other("mock-native"));
    let mut y = vec![0.0; 3];
    dm.mat_vec(&mesh, "K", &[0.0, 1.0, 2.0], &mut y, 1.0, 0.0)
        .unwrap();
    assert_eq!(
        y,
        [-1.0, 0.0, 1.0],
        "same operator through a permuted native numbering"
    );
    let log = log.lock().unwrap();
    assert_eq!(log.last().unwrap(), "assembly_end K");
    assert_eq!(log.iter().filter(|l| l.starts_with("set")).count(), 2);
}

/// Native Newton through `NativeNonLinear`: the driver's F/J trampolines
/// reach the model callback and the DOF manager, the Jacobian is assembled
/// into the backend-owned matrix, and the linear solves use it.
#[test]
fn native_newton_solves_through_the_manager_without_coo() {
    let log: Log = Arc::default();
    let mut dm = DofManager::with_backend(
        "dm",
        Communicator::serial(),
        Box::new(MockNative::new(Arc::clone(&log))),
    );
    let mut p = Springs::new(serial_bar(4), 4.0, false);
    p.setup(&mut dm);
    let mut snes = NativeNonLinear::new("J");
    snes.solve(&mut dm, &mut p).unwrap();
    assert!(
        snes.converged() && snes.nb_iterations() >= 3,
        "{}",
        snes.nb_iterations()
    );
    let u = dm.dofs("u").unwrap();
    let u = u.as_slice();
    for (x, u) in [0.0, 1.0, 2.0, 3.0, 4.0].iter().zip(u) {
        assert!((u - x).abs() < 1e-10, "u({x}) = {u}");
    }
    assert!(dm.matrix("J").is_err(), "the solve never used a COO matrix");
    let log = log.lock().unwrap();
    assert_eq!(log[0], "snes J");
    let solves = log.iter().filter(|l| *l == "solve J").count();
    assert_eq!(solves, snes.nb_iterations());
    assert_eq!(
        log.iter().filter(|l| l.starts_with("analysis J")).count(),
        1,
        "symbolic analysis once, values refactorised"
    );
}

/// The AIJ backend with a pluggable direct solver and the native backend
/// reach the same equilibrium through the same manager/callback path.
#[test]
fn aij_and_native_backends_reach_the_same_equilibrium() {
    let log: Log = Arc::default();
    let mut aij = AijBackend::default();
    aij.set_solver("J", Box::new(DenseDirect::new(Arc::clone(&log))));
    let mut dm = DofManager::with_backend("dm", Communicator::serial(), Box::new(aij));
    let mut p = Springs::new(serial_bar(4), 4.0, true);
    p.setup(&mut dm);
    let iterations = newton(&mut dm, &mut p).unwrap();
    assert!(iterations >= 3);
    let u_aij = dm.dofs("u").unwrap().as_slice().to_vec();

    let mut dm2 = DofManager::with_backend(
        "dm",
        Communicator::serial(),
        Box::new(MockNative::new(Arc::default())),
    );
    let mut p2 = Springs::new(serial_bar(4), 4.0, false);
    p2.setup(&mut dm2);
    NativeNonLinear::new("J").solve(&mut dm2, &mut p2).unwrap();
    for (a, b) in u_aij.iter().zip(dm2.dofs("u").unwrap().as_slice()) {
        assert!((a - b).abs() < 1e-10);
    }
    let log = log.lock().unwrap();
    assert_eq!(log[0], "analysis J sym=2");
    assert_eq!(log.iter().filter(|l| l.starts_with("analysis")).count(), 1);
    assert_eq!(
        log.iter().filter(|l| l.starts_with("factorize")).count(),
        iterations
    );
}

/// Two ranks, shared node g2 (owner rank 0): the backend reads only owned
/// right-hand-side rows, writes only owned solution rows, and the manager
/// synchronises the shared copy; both ranks converge to `u = x`.
#[test]
fn native_newton_on_two_ranks_maps_owned_and_shared_rows() {
    let t0 = Instant::now();
    let out = run_local(2, Duration::from_secs(20), |comm| {
        let log: Log = Arc::default();
        let mut dm = DofManager::with_backend(
            "dm",
            comm.clone(),
            Box::new(MockNative::new(Arc::clone(&log))),
        );
        let mut p = Springs::new(two_rank_bar(&comm), 4.0, false);
        p.setup(&mut dm);
        let mut snes = NativeNonLinear::new("J");
        let r = snes.solve(&mut dm, &mut p);
        let xs = p.mesh.nodes().as_slice().to_vec();
        let u = dm.dofs("u").unwrap().as_slice().to_vec();
        let layout = dm.layout().clone();
        (
            r.map(|()| snes.nb_iterations()),
            xs,
            u,
            layout.nb_owned(),
            dm.system_size(),
        )
    });
    assert!(t0.elapsed() < Duration::from_secs(10));
    assert_eq!(
        out[0].0.as_ref().unwrap(),
        out[1].0.as_ref().unwrap(),
        "same iteration count on every rank"
    );
    for (_, xs, u, _, size) in &out {
        assert_eq!(*size, 5);
        for (x, u) in xs.iter().zip(u) {
            assert!((u - x).abs() < 1e-10, "u({x}) = {u}");
        }
    }
    assert_eq!((out[0].3, out[1].3), (3, 2), "g2 owned by rank 0 only");
}

/// A model callback failing inside a native trampoline: the trampoline
/// returns a status code, the error is stored in the context and re-raised
/// by the solver (upstream PETSc error-code boundary).
#[test]
fn trampoline_errors_are_stored_and_reraised() {
    let mut dm = DofManager::with_backend(
        "dm",
        Communicator::serial(),
        Box::new(MockNative::new(Arc::default())),
    );
    let mut p = Springs::new(serial_bar(2), 2.0, false);
    p.setup(&mut dm);
    p.fail_residual_at = Some(2);
    let err = NativeNonLinear::new("J")
        .solve(&mut dm, &mut p)
        .unwrap_err();
    assert!(
        err.to_string().contains("injected residual failure"),
        "{err}"
    );
}

/// Backends without a native non-linear solver say so.
#[test]
fn aij_has_no_native_newton() {
    let mut dm = DofManager::new("dm", Communicator::serial());
    let mut p = Springs::new(serial_bar(2), 2.0, false);
    p.setup(&mut dm);
    let err = NativeNonLinear::new("J")
        .solve(&mut dm, &mut p)
        .unwrap_err();
    assert!(matches!(err, akantu_core::Error::Unsupported(_)), "{err}");
}
