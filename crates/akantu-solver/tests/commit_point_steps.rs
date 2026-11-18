// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). No upstream
// counterpart: the steps a rank runs after a peer's local failure in a
// collective operation of the DOF manager, under both collective-check
// modes (D-018, D-126), on the in-process local world.
//
// The step sequence of `NonLinearSolverLumped::solve_step` (before,
// predictor, assemble_residual, solve, corrector, after), each a
// `DofManager::step` without communication, with a commit point before the
// corrector as the time-step adapter has. Rank 0's residual assembly fails
// locally. Under every-phase checks the peers stop at the failed step's
// agreement; under commit points they may run the steps that change
// transient data only, and stop at the commit point, before the corrector.

use std::time::Duration;

use akantu_comm::phase::CollectiveChecks;
use akantu_comm::run_local;
use akantu_core::Error;
use akantu_solver::DofManager;

const STEPS: [&str; 6] = [
    "before_solve_step",
    "predictor",
    "assemble_residual",
    "solve_lumped",
    "corrector",
    "after_solve_step",
];

fn run(mode: CollectiveChecks) -> Vec<(bool, Vec<&'static str>)> {
    run_local(2, Duration::from_secs(5), |c| {
        c.set_collective_checks(mode);
        let mut dm = DofManager::new("dm", c.clone());
        let mut ran: Vec<&'static str> = Vec::new();
        let rank = c.rank();
        let r = dm.collective("nls", |d| {
            for name in STEPS {
                if name == "corrector" {
                    d.commit_point("corrector:dofs")?;
                }
                d.step(name, |_| {
                    if rank == 0 && name == "assemble_residual" {
                        return Err(Error::Numerical("residual failed".into()));
                    }
                    ran.push(name);
                    Ok(())
                })?;
            }
            Ok(())
        });
        (r.is_err(), ran)
    })
}

#[test]
fn peers_stop_at_the_commit_point_after_a_local_failure() {
    let every = run(CollectiveChecks::EveryPhase);
    let commit = run(CollectiveChecks::CommitPoints);
    for out in [&every, &commit] {
        assert!(out.iter().all(|(failed, _)| *failed), "{out:?}");
        assert_eq!(out[0].1, ["before_solve_step", "predictor"]);
    }
    assert_eq!(
        every[1].1,
        ["before_solve_step", "predictor", "assemble_residual"]
    );
    assert_eq!(
        commit[1].1,
        [
            "before_solve_step",
            "predictor",
            "assemble_residual",
            "solve_lumped"
        ]
    );
}
