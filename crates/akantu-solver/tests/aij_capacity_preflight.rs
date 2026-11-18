// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Regression test:
// the AIJ
// backend's centralised solve (upstream SolverVectorDistributed gather on
// rank 0, src/solver/solver_vector_distributed.cc, with
// src/solver/sparse_solver.hh capacities) enforces the backend's
// capabilities before any conversion, copy or gather.

//! `AijBackend::solve` with a mock backend declaring capability limits: a
//! limit exceeded on any rank (or by the root aggregate) fails **every**
//! rank promptly, before any payload is gathered and before the backend is
//! invoked; the matrix distribution is gated as well as the RHS one. Before
//! the fix a two-rank mock with limits `(size 1, index 0,
//! nnz 0)` solved a size-2, two-entry system.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use akantu_comm::phase::Phases;
use akantu_comm::{Communicator, run_local};
use akantu_core::enums::{MatrixType, SparseSolverType};
use akantu_core::{Error, Real, Result};
use akantu_linalg::direct::{
    BackendCapabilities, MatrixDistribution, RhsDistribution, SparseDirectSolver,
    centralized_capabilities,
};
use akantu_linalg::layout::EquationOwnership::Owned;
use akantu_linalg::{EquationLayout, SparseMatrixAij};
use akantu_solver::backend::{AijBackend, BackendContext, LinearSystemBackend};

/// A backend that only counts invocations (`x = b` on solve).
#[derive(Debug)]
struct Limited {
    caps: BackendCapabilities,
    calls: Arc<AtomicUsize>,
}

impl SparseDirectSolver for Limited {
    fn kind(&self) -> SparseSolverType {
        SparseSolverType::Eigen
    }
    fn name(&self) -> &'static str {
        "limited-mock"
    }
    fn capabilities(&self) -> BackendCapabilities {
        self.caps
    }
    fn analysis(&mut self, _: &SparseMatrixAij) -> Result<()> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
    fn factorize(&mut self, _: &SparseMatrixAij) -> Result<()> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
    fn solve(&mut self, b: &[Real], x: &mut [Real]) -> Result<()> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        x.copy_from_slice(b);
        Ok(())
    }
}

fn caps(index: i64, nnz: u64, size: Option<i64>) -> BackendCapabilities {
    BackendCapabilities {
        max_global_index: index,
        max_local_nnz: nnz,
        max_centralized_size: size,
        ..centralized_capabilities()
    }
}

/// Two ranks, system of order 2, rank `r` owns global equation `r`; rank
/// `r` stores the diagonal entry `(r, r)` plus `extra[r]` off-diagonal
/// entries. Returns each rank's `(solve result, backend calls)`.
fn run(per_rank_caps: [BackendCapabilities; 2], extra: [usize; 2]) -> Vec<(Result<()>, usize)> {
    let t0 = Instant::now();
    let out = run_local(2, Duration::from_millis(1500), |comm| {
        let r = comm.rank();
        let mut layout = EquationLayout::new();
        layout
            .append_block(&[Owned], i64::from(r), 2, |_| None)
            .unwrap();
        let calls = Arc::new(AtomicUsize::new(0));
        let mut aij = AijBackend::default();
        aij.new_matrix("J", MatrixType::Unsymmetric, 2).unwrap();
        let mut terms = vec![(i64::from(r), i64::from(r), 2.0)];
        for k in 0..extra[r as usize] {
            terms.push((i64::from(r), i64::from(1 - r), 1e-3 * (k + 1) as f64));
        }
        aij.add_terms("J", &terms).unwrap();
        aij.set_solver(
            "J",
            Box::new(Limited {
                caps: per_rank_caps[r as usize],
                calls: calls.clone(),
            }),
        );
        let mut ph = Phases::new(&comm, "solve");
        let mut ctx = BackendContext {
            layout: &layout,
            phases: &mut ph,
        };
        let mut sol = [0.0];
        let res = aij.solve("J", &mut ctx, &[1.0 + f64::from(r)], &mut sol);
        // the scope's owner settles it (D-126: what is deferred is agreed)
        let res = ph.settle(res);
        (res, calls.load(Ordering::SeqCst))
    });
    assert!(
        t0.elapsed() < Duration::from_secs(1),
        "no rank waited for a peer"
    );
    out
}

fn all_refused(out: &[(Result<()>, usize)], what: &str) {
    for (rank, (res, calls)) in out.iter().enumerate() {
        assert!(res.is_err(), "{what}: rank {rank} solved");
        assert_eq!(*calls, 0, "{what}: rank {rank} invoked the backend");
    }
}

#[test]
fn reviewer_limits_refuse_the_system_on_every_rank() {
    // (size = 1, index = 0, nnz = 0) against a size-2, two-entry system
    let c = caps(0, 0, Some(1));
    let out = run([c, c], [0, 0]);
    all_refused(&out, "reviewer limits");
    for (res, _) in &out {
        assert!(
            matches!(res, Err(Error::CapacityExceeded { .. })),
            "{res:?}"
        );
    }
}

#[test]
fn each_limit_is_enforced_before_any_gather() {
    let big = i64::from(u32::MAX);
    // global index: equation 1 not representable
    all_refused(
        &run([caps(0, 100, Some(big)); 2], [0, 0]),
        "max_global_index",
    );
    // centralised order
    all_refused(
        &run([caps(big, 100, Some(1)); 2], [0, 0]),
        "max_centralized_size",
    );
    // no centralised allocation at all
    all_refused(
        &run([caps(big, 100, None); 2], [0, 0]),
        "no centralised size",
    );
    // one rank only over its local limit (rank 1 has 3 entries, rank 0 one)
    let out = run([caps(big, 2, Some(big)); 2], [0, 2]);
    all_refused(&out, "one-rank local nnz");
    assert!(matches!(out[1].0, Err(Error::CapacityExceeded { .. })));
    // root aggregate (2 + 2 entries > 3) with every rank locally fine
    let out = run([caps(big, 3, Some(big)); 2], [1, 1]);
    all_refused(&out, "aggregate nnz");
    for (res, _) in &out {
        assert!(
            matches!(res, Err(Error::CapacityExceeded { what, .. }) if what.contains("rank 0")),
            "{res:?}"
        );
    }
    // ranks with different limits still agree (rank 1 alone too small)
    all_refused(
        &run([caps(big, 100, Some(big)), caps(big, 3, Some(big))], [1, 1]),
        "asymmetric limits",
    );
}

#[test]
fn matrix_distribution_is_gated_as_well_as_the_rhs() {
    // MUMPS-like: centralised RHS, distributed-assembled matrix
    let mumps = BackendCapabilities {
        matrix: MatrixDistribution::DistributedAssembled,
        ..centralized_capabilities()
    };
    // A distributed-assembled backend gets the system through
    // `solve_distributed`; this mock does not implement it, so
    // the trait's default refuses on every rank (agreed), before the
    // backend's centralised API is invoked.
    let out = run([mumps; 2], [0, 0]);
    all_refused(&out, "distributed matrix");
    assert!(
        matches!(out[0].0, Err(Error::Unsupported(_))),
        "{:?}",
        out[0].0
    );
    // backend-owned matrix with a centralised RHS: not a centralised backend
    let odd = BackendCapabilities {
        matrix: MatrixDistribution::BackendOwned,
        ..centralized_capabilities()
    };
    all_refused(&run([odd; 2], [0, 0]), "backend-owned matrix");
    // distributed RHS
    let petsc = BackendCapabilities {
        matrix: MatrixDistribution::BackendOwned,
        rhs: RhsDistribution::Distributed,
        ..centralized_capabilities()
    };
    all_refused(&run([petsc; 2], [0, 0]), "distributed rhs");
}

#[test]
fn generous_limits_solve_and_serial_limits_apply() {
    let big = i64::from(u32::MAX);
    let out = run([caps(big, 4, Some(2)); 2], [1, 1]);
    assert!(out.iter().all(|(r, _)| r.is_ok()), "{out:?}");
    assert!(out[0].1 > 0, "rank 0 ran the backend");
    assert_eq!(out[1].1, 0, "only rank 0 runs a centralised backend");

    // serial: the same limits apply to the whole matrix
    for (c, ok) in [
        (caps(big, 4, Some(2)), true),
        (caps(0, 4, Some(2)), false),
        (caps(big, 1, Some(2)), false),
        (caps(big, 4, Some(1)), false),
    ] {
        let calls = Arc::new(AtomicUsize::new(0));
        let mut aij = AijBackend::default();
        aij.new_matrix("J", MatrixType::Unsymmetric, 2).unwrap();
        aij.add_terms("J", &[(0, 0, 2.0), (1, 1, 2.0)]).unwrap();
        aij.set_solver(
            "J",
            Box::new(Limited {
                caps: c,
                calls: calls.clone(),
            }),
        );
        let layout = EquationLayout::serial(2).unwrap();
        let comm = Communicator::serial();
        let mut ph = Phases::new(&comm, "solve");
        let mut ctx = BackendContext {
            layout: &layout,
            phases: &mut ph,
        };
        let mut sol = [0.0; 2];
        let r = aij.solve("J", &mut ctx, &[1.0, 2.0], &mut sol);
        assert_eq!(r.is_ok(), ok, "{c:?}: {r:?}");
        if !ok {
            assert_eq!(calls.load(Ordering::SeqCst), 0);
        }
    }
}
