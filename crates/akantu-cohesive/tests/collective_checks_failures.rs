// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). No upstream
// counterpart: the collective error protocol (D-018, D-126) on the
// distributed extrinsic cohesive scenario of oracle/probes/parallel_cohesive
// (the phases of the cohesive benchmark's step: insertion check with its
// mesh transaction, explicit solve, energies), on the in-process local world.
//
// One rank fails one chosen phase locally (`akantu_comm::phase::testing`)
// at 2 and 4 ranks, under both collective-check modes; every rank must end
// the run with an error, in bounded time (no rank waits for the world's
// timeout), with the agreed severity class, and no rank may commit a mesh
// revision another rank rolled back.

mod par_coh;

use std::time::{Duration, Instant};

use akantu_cohesive::par_scenarios::{self, RunObserver};
use akantu_comm::phase::{CollectiveChecks, testing};
use akantu_comm::run_local;
use akantu_core::{Error, FatalKind, Result};

/// Arms the injection after the initialisation, so that only the steps
/// count.
struct Arm {
    pattern: &'static str,
    nth: u32,
    fail: bool,
    mesh: akantu_mesh::SharedMesh,
    start: u64,
}

impl RunObserver for Arm {
    fn after_init(&mut self, _m: &mut akantu_cohesive::SolidMechanicsModelCohesive) -> Result<()> {
        // revisions committed from here on are collective (identical
        // counts on every rank when no rank commits alone)
        self.start = self.mesh.revision()?;
        if self.fail {
            testing::fail_phase(self.pattern, self.nth);
        }
        Ok(())
    }
}

/// What one rank observed.
#[derive(Debug)]
struct Seen {
    error: Option<String>,
    fatal: Option<FatalKind>,
    /// Mesh revisions committed during the steps.
    revision: u64,
    elapsed: Duration,
}

/// Run the scenario with an injected failure of `pattern` (its `nth`
/// occurrence) on rank `failing`.
fn run(
    np: usize,
    mode: CollectiveChecks,
    pattern: &'static str,
    nth: u32,
    failing: usize,
) -> Vec<Seen> {
    let spec = par_scenarios::spec("extrinsic_tri").unwrap();
    run_local(np, par_coh::local_timeout(), |comm| {
        comm.set_collective_checks(mode);
        let mesh = par_coh::distributed(
            &spec,
            &comm,
            akantu_mesh::utils::FacetFlipMode::UpstreamInverse,
        )
        .expect("distributed mesh");
        let mut arm = Arm {
            pattern,
            nth,
            fail: usize::try_from(comm.rank()).unwrap() == failing,
            mesh: mesh.clone(),
            start: 0,
        };
        let t0 = Instant::now();
        let r = par_scenarios::run(&spec, mesh.clone(), &mut arm);
        let elapsed = t0.elapsed();
        testing::clear();
        let revision = mesh.revision().map_or(u64::MAX, |r| r - arm.start);
        let (error, fatal) = match r {
            Ok(_) => (None, None),
            Err(e) => (Some(e.to_string()), e.fatal_kind()),
        };
        Seen {
            error,
            fatal,
            revision,
            elapsed,
        }
    })
}

/// Every rank failed, the failing rank with the injected error, the others
/// with a remote failure; nobody waited for the timeout; one committed
/// revision on every rank; the expected fatal kind everywhere.
fn check(seen: &[Seen], failing: usize, fatal: Option<FatalKind>, what: &str) {
    for (r, s) in seen.iter().enumerate() {
        let e = s
            .error
            .as_deref()
            .unwrap_or_else(|| panic!("{what}: rank {r} ended without an error"));
        if r == failing {
            assert!(e.contains("injected failure"), "{what}: rank {r}: {e}");
        } else {
            assert!(
                e.contains("failed on another rank") || e.contains("injected failure"),
                "{what}: rank {r}: {e}"
            );
        }
        assert_eq!(s.fatal, fatal, "{what}: rank {r}: {e}");
        assert!(
            s.elapsed < par_coh::local_timeout() / 2,
            "{what}: rank {r} took {:?} (a wait ran into the timeout)",
            s.elapsed
        );
        assert_eq!(
            s.revision, seen[0].revision,
            "{what}: rank {r} committed {} mesh revisions, rank 0 {}",
            s.revision, seen[0].revision
        );
    }
}

const MODES: [CollectiveChecks; 2] = [CollectiveChecks::EveryPhase, CollectiveChecks::CommitPoints];

fn each(pattern: &'static str, nth: u32, fatal: Option<FatalKind>) {
    for mode in MODES {
        for np in [2, 4] {
            for failing in [0, np - 1] {
                let seen = run(np, mode, pattern, nth, failing);
                check(
                    &seen,
                    failing,
                    fatal,
                    &format!("{pattern} #{nth}, {mode:?}, {np} ranks, rank {failing} failing"),
                );
            }
        }
    }
}

/// A model callback of the explicit solve (the residual assembly) fails on
/// one rank: recoverable on every rank.
#[test]
fn solve_step_failure_reaches_every_rank() {
    each("model:assemble_residual", 3, None);
}

/// The insertion check's first decision (are facets flagged anywhere) fails
/// on one rank: recoverable on every rank, nothing inserted.
#[test]
fn insertion_check_failure_reaches_every_rank() {
    each("ce:insertion_any:prepare", 3, None);
}

/// The energies' reduction fails on one rank: recoverable on every rank.
#[test]
fn energy_failure_reaches_every_rank() {
    each("smm:energy/smm:all_reduce:prepare", 2, None);
}

/// The first insertion's mesh transaction fails before its commit (the
/// topology stage) on one rank: every rank rolls back (the same committed
/// revision everywhere), recoverable.
#[test]
fn transaction_failure_before_the_commit_rolls_back_every_rank() {
    each("mesh-mutation/topology", 1, None);
}

/// A consumer of the first insertion's transaction fails after the commit
/// on one rank: the domain is poisoned on every rank (agreed fatal), at the
/// same committed revision.
#[test]
fn transaction_failure_after_the_commit_poisons_every_rank() {
    each("mesh-mutation/dispatch", 1, Some(FatalKind::Agreed));
}

/// The injection itself is local and fires once (control).
#[test]
fn injection_fires_once_on_its_thread() {
    testing::fail_phase("x/y", 2);
    let c = akantu_comm::Communicator::serial();
    let mut ph = akantu_comm::phase::Phases::new(&c, "x");
    assert!(
        ph.agree("y", Ok(()), akantu_comm::phase::Severity::Recoverable)
            .is_ok()
    );
    let e = ph.agree("y", Ok(()), akantu_comm::phase::Severity::Recoverable);
    assert!(matches!(e, Err(Error::Communication(m)) if m.contains("injected failure at x/y")));
    drop(ph);
    let mut ph = akantu_comm::phase::Phases::new(&c, "x");
    assert!(
        ph.agree("y", Ok(()), akantu_comm::phase::Severity::Recoverable)
            .is_ok()
    );
}
