// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): the static solve of
// test/test_model/test_common/test_model_solver/test_model_solver.cc; the
// release between two solves is a port extension.

//! `DofManager::release_factorization` / `release_matrix` with the serial
//! sparse solvers (SparseLU translation, faer): a later static solve
//! assembles, analyses and factorises again and gives the same bits
//! (MUMPS: `mumps_mpi_solver.rs`, scenario `release_factorization_mumps`).

mod common;

use akantu_comm::Communicator;
use akantu_core::enums::SparseSolverType;

#[test]
fn release_then_resolve_eigen_and_faer() {
    for sparse in [SparseSolverType::Eigen, SparseSolverType::Faer] {
        common::release::release_keeps_the_bits(&Communicator::serial(), sparse).unwrap();
    }
}

#[test]
fn release_inside_a_collective_operation_is_refused() {
    let mut dm = akantu_solver::DofManager::new("dm", Communicator::serial());
    dm.new_matrix("J", akantu_core::enums::MatrixType::Symmetric)
        .unwrap();
    let r = dm.collective("outer", |dm| dm.release_factorization("J"));
    assert!(
        r.unwrap_err()
            .to_string()
            .contains("outside a collective operation")
    );
    // outside: fine (nothing factorised yet)
    dm.release_factorization("J").unwrap();
    assert!(dm.release_factorization("nope").is_err());
}
