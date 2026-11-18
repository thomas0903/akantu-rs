// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). No upstream
// counterpart: the collective error protocol (D-018, D-126) on the
// distributed extrinsic cohesive scenario of oracle/probes/parallel_cohesive
// (the phases of the cohesive benchmark's step: insertion check with its
// mesh transaction, explicit solve, energies), on the in-process local world
// (the real-MPI binary mpi_cohesive.rs runs the same scenarios).
//
// One rank fails one chosen phase locally (`akantu_comm::phase::testing`)
// at 2 and 4 ranks, under both collective-check modes; every rank must end
// the run with an error, in bounded time (no rank waits for the world's
// timeout), with the agreed severity class, and no rank may commit a mesh
// revision another rank rolled back.

mod par_coh;

use akantu_comm::phase::{Phases, Severity, testing};
use akantu_comm::run_local;
use akantu_core::Error;
use par_coh::checks::{self, FAILURES, MODES};

fn each(name: &str) {
    let &(_, pattern, nth, fatal) = FAILURES.iter().find(|f| f.0 == name).unwrap();
    for mode in MODES {
        for np in [2, 4] {
            for failing in [0, np - 1] {
                let what =
                    format!("{pattern} #{nth}, {mode:?}, {np} ranks, rank {failing} failing");
                let seen = run_local(np, par_coh::local_timeout(), |comm| {
                    checks::failure_run(&comm, mode, pattern, nth, failing)
                });
                for (r, s) in seen.iter().enumerate() {
                    checks::check_failure(
                        s,
                        r,
                        failing,
                        fatal,
                        par_coh::local_timeout() / 2,
                        &what,
                    )
                    .unwrap_or_else(|e| panic!("{e}"));
                    assert_eq!(
                        s.revision, seen[0].revision,
                        "{what}: rank {r} committed {} mesh revisions, rank 0 {}",
                        s.revision, seen[0].revision
                    );
                }
            }
        }
    }
}

/// A model callback of the explicit solve (the residual assembly) fails on
/// one rank: recoverable on every rank.
#[test]
fn solve_step_failure_reaches_every_rank() {
    each("solve_step");
}

/// The insertion check's first decision (are facets flagged anywhere) fails
/// on one rank: recoverable on every rank, nothing inserted.
#[test]
fn insertion_check_failure_reaches_every_rank() {
    each("insertion_check");
}

/// The energies' reduction fails on one rank: recoverable on every rank.
#[test]
fn energy_failure_reaches_every_rank() {
    each("energy");
}

/// The first insertion's mesh transaction fails before its commit (the
/// topology stage) on one rank: every rank rolls back (the same committed
/// revision everywhere), recoverable.
#[test]
fn transaction_failure_before_the_commit_rolls_back_every_rank() {
    each("transaction_before_commit");
}

/// A consumer of the first insertion's transaction fails after the commit
/// on one rank: the domain is poisoned on every rank (agreed fatal), at the
/// same committed revision.
#[test]
fn transaction_failure_after_the_commit_poisons_every_rank() {
    each("transaction_after_commit");
}

/// The injection itself is local and fires once (control).
#[test]
fn injection_fires_once_on_its_thread() {
    testing::fail_phase("x/y", 2);
    let c = akantu_comm::Communicator::serial();
    let mut ph = Phases::new(&c, "x");
    assert!(ph.agree("y", Ok(()), Severity::Recoverable).is_ok());
    let e = ph.agree("y", Ok(()), Severity::Recoverable);
    assert!(matches!(e, Err(Error::Communication(m)) if m.contains("injected failure at x/y")));
    drop(ph);
    let mut ph = Phases::new(&c, "x");
    assert!(ph.agree("y", Ok(()), Severity::Recoverable).is_ok());
}
