// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Wiring test:
// AijBackend's multi-rank solve with a centralised direct solver (upstream
// SolverVectorDistributed gather/scatter on rank 0, src/solver/
// solver_vector_distributed.cc), reusing the springs model of
// backend_abstraction.rs.

//! The AIJ backend on two ranks with the Eigen `SparseLU` translation and
//! the faer backend: same equilibrium and iteration count as in serial,
//! owned rows written by the backend, shared rows synchronised.

mod common;

use std::time::{Duration, Instant};

use akantu_comm::{Communicator, run_local};
use akantu_linalg::direct::SparseSolverFactory;
use akantu_solver::{AijBackend, DofManager};
use common::{Springs, newton, serial_bar, two_rank_bar};

fn backend(name: &str) -> Box<AijBackend> {
    let mut aij = AijBackend::default();
    aij.set_solver(
        "J",
        SparseSolverFactory::default().create_by_name(name).unwrap(),
    );
    Box::new(aij)
}

#[test]
fn centralized_solve_on_two_ranks_matches_serial() {
    for name in ["eigen", "faer"] {
        let mut dm = DofManager::with_backend("dm", Communicator::serial(), backend(name));
        let mut p = Springs::new(serial_bar(4), 4.0, true);
        p.setup(&mut dm);
        let it_serial = newton(&mut dm, &mut p).unwrap();
        let u_serial = dm.dofs("u").unwrap().as_slice().to_vec();

        let t0 = Instant::now();
        let out = run_local(2, Duration::from_secs(20), |comm| {
            let mut dm = DofManager::with_backend("dm", comm.clone(), backend(name));
            let mut p = Springs::new(two_rank_bar(&comm), 4.0, true);
            p.setup(&mut dm);
            let it = newton(&mut dm, &mut p);
            let xs = p.mesh.nodes().as_slice().to_vec();
            (it, xs, dm.dofs("u").unwrap().as_slice().to_vec())
        });
        assert!(t0.elapsed() < Duration::from_secs(10));
        for (rank, (it, xs, u)) in out.iter().enumerate() {
            assert_eq!(
                *it.as_ref().unwrap(),
                it_serial,
                "{name}: rank {rank} iterations"
            );
            for (x, ui) in xs.iter().zip(u) {
                let us = u_serial[*x as usize];
                assert!(
                    (ui - us).abs() < 1e-12,
                    "{name}: rank {rank} u({x}) = {ui} vs serial {us}"
                );
                assert!((ui - x).abs() < 1e-10);
            }
        }
    }
}
