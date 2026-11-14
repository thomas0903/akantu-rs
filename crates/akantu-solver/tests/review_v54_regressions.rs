// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs regression tests for the solver
// stack ported from Akantu (EPFL/LSMS): src/model/common/non_linear_solver/
// non_linear_solver_newton_raphson.cc (the Newton–Raphson solver allocates
// its sparse solver, `SparseSolverFactory`), src/solver/sparse_solver_mumps.cc
// (the UD-216 workspace retry) and the D-041 rule that solver-stack
// upstream compatibility is switched per DOF manager.

//! The manager-wide UD-216 switch (`UpstreamCompat::
//! mumps_retry_with_solution_rhs`) reaches the sparse solver of **every**
//! solve of the manager — the installed one, a reallocated one, and back —
//! through `SparseDirectSolver::set_upstream_compat`. Serial, no MPI: a
//! recording solver wraps the default sparse solver (the real MUMPS path is
//! exercised by `mumps_mpi_solver` `ud216_manager_compat`).

mod common;

use std::sync::{Arc, Mutex};

use akantu_comm::Communicator;
use akantu_core::enums::{MatrixType, SparseSolverType};
use akantu_core::{Idx, Real, Result};
use akantu_linalg::direct::{
    BackendCapabilities, SparseDirectSolver, SparseSolverCompat, SparseSolverFactory,
};
use akantu_linalg::{SparseMatrixAij, TermsToAssemble};
use akantu_solver::{DofManager, UpstreamCompat};
use common::serial_bar;

/// The default sparse solver plus a log of the compatibility switches it
/// received.
#[derive(Debug)]
struct Recording {
    inner: Box<dyn SparseDirectSolver>,
    log: Arc<Mutex<Vec<bool>>>,
}

impl Recording {
    fn new(log: &Arc<Mutex<Vec<bool>>>) -> Self {
        Self {
            inner: SparseSolverFactory::default()
                .create(SparseSolverType::Eigen, 1)
                .expect("eigen"),
            log: Arc::clone(log),
        }
    }
}

impl SparseDirectSolver for Recording {
    fn kind(&self) -> SparseSolverType {
        self.inner.kind()
    }
    fn capabilities(&self) -> BackendCapabilities {
        self.inner.capabilities()
    }
    fn analysis(&mut self, a: &SparseMatrixAij) -> Result<()> {
        self.inner.analysis(a)
    }
    fn factorize(&mut self, a: &SparseMatrixAij) -> Result<()> {
        self.inner.factorize(a)
    }
    fn solve(&mut self, b: &[Real], x: &mut [Real]) -> Result<()> {
        self.inner.solve(b, x)
    }
    fn set_upstream_compat(&mut self, compat: SparseSolverCompat) {
        self.log
            .lock()
            .expect("log")
            .push(compat.mumps_retry_with_solution_rhs);
    }
}

fn last(log: &Arc<Mutex<Vec<bool>>>) -> Option<bool> {
    log.lock().expect("log").last().copied()
}

/// `DofManager::set_upstream_compat` selects UD-216
/// behaviour for every sparse solver of the manager, including one
/// installed after the switch (reallocation), and the corrected default
/// comes back when the switch is cleared.
#[test]
fn mu06_manager_wide_ud216_switch_reaches_every_solver() {
    let mesh = serial_bar(3);
    let mut dm = DofManager::new("dm", Communicator::serial());
    dm.register_nodal_dofs(&mesh, "u", 1).unwrap();
    dm.new_matrix("J", MatrixType::Unsymmetric).unwrap();
    let n = dm.local_system_size();
    let mut diag = TermsToAssemble::new("u", "u");
    for i in 0..n {
        diag.add(i as Idx, i as Idx, 2.0);
    }
    dm.assemble_preassembled_matrix("J", &diag).unwrap();
    dm.finalize_matrix("J").unwrap();
    let b = vec![1.0; n];
    let mut x = vec![0.0; n];

    let first: Arc<Mutex<Vec<bool>>> = Arc::default();
    dm.backend_mut()
        .unwrap()
        .set_sparse_solver("J", Box::new(Recording::new(&first)))
        .unwrap();
    dm.solve_linear_system("J", &b, &mut x).unwrap();
    assert_eq!(last(&first), Some(false), "corrected default");
    assert!(x.iter().all(|&v| v == 0.5), "{x:?}");

    dm.set_upstream_compat(UpstreamCompat {
        mumps_retry_with_solution_rhs: true,
        ..UpstreamCompat::default()
    });
    assert!(
        dm.upstream_compat()
            .sparse_solver()
            .mumps_retry_with_solution_rhs
    );
    dm.solve_linear_system("J", &b, &mut x).unwrap();
    assert_eq!(
        last(&first),
        Some(true),
        "manager switch on the installed solver"
    );

    // reallocation (as Newton–Raphson's `install_sparse_solver` does)
    let second: Arc<Mutex<Vec<bool>>> = Arc::default();
    dm.backend_mut()
        .unwrap()
        .set_sparse_solver("J", Box::new(Recording::new(&second)))
        .unwrap();
    dm.solve_linear_system("J", &b, &mut x).unwrap();
    assert_eq!(
        last(&second),
        Some(true),
        "manager switch on a reallocated solver"
    );

    dm.set_upstream_compat(UpstreamCompat::default());
    dm.solve_linear_system("J", &b, &mut x).unwrap();
    assert_eq!(last(&second), Some(false), "default restored");
    assert!(x.iter().all(|&v| v == 0.5), "{x:?}");
}
