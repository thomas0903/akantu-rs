// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). No upstream
// counterpart: a time-step solver without integration scheme (a port
// extension: the non-linear solver drives the model's hooks directly) under
// both collective-check modes (D-018, D-126), on the in-process local world.
//
// Such a model changes its state in its own hooks, so none of them may run
// on a rank after a peer's failure that every-phase checks would have
// stopped it at. The model below records each hook it ran (its persistent
// state); a failure is injected in each step of the explicit lumped solve in
// turn, on each rank at 2 ranks, and the records must be the same under both
// modes on every rank.

mod common;

use std::time::Duration;

use akantu_comm::phase::{CollectiveChecks, testing};
use akantu_comm::run_local;
use akantu_core::enums::{NonLinearSolverType, TimeStepSolverType};
use akantu_core::{Array, Result};
use akantu_solver::{DofManager, ModelSolver, ModelSolverOptions, SolverCallback};

/// A scheme-less model: `u ← u + Δ` from the lumped solve, every hook
/// recorded.
struct Recorder {
    ran: Vec<&'static str>,
}

impl SolverCallback for Recorder {
    fn assemble_lumped_matrix(&mut self, dm: &mut DofManager, id: &str) -> Result<()> {
        self.ran.push("assemble_lumped_matrix");
        let n = dm.dofs("u")?.len();
        let ones = Array::from_vec(vec![1.0; n], 1)?;
        dm.assemble_to_lumped_matrix("u", ones.view(), id, 1.0)
    }

    fn assemble_residual(&mut self, dm: &mut DofManager) -> Result<()> {
        self.ran.push("assemble_residual");
        if !dm.has_lumped_matrix("M") {
            dm.new_lumped_matrix("M")?;
            self.assemble_lumped_matrix(dm, "M")?;
        }
        let n = dm.dofs("u")?.len();
        let f = Array::from_vec(vec![0.5; n], 1)?;
        dm.assemble_to_residual("u", f.view(), 1.0)
    }

    fn predictor(&mut self, _: &mut DofManager) -> Result<()> {
        self.ran.push("predictor");
        Ok(())
    }

    fn corrector(&mut self, _: &mut DofManager) -> Result<()> {
        self.ran.push("corrector");
        Ok(())
    }

    fn before_solve_step(&mut self, _: &mut DofManager) -> Result<()> {
        self.ran.push("before_solve_step");
        Ok(())
    }

    fn after_solve_step(&mut self, _: &mut DofManager, _: bool) -> Result<()> {
        self.ran.push("after_solve_step");
        Ok(())
    }
}

/// The hooks one rank ran in a solve whose step `pattern` fails on rank
/// `failing` (no failure: `failing` out of range), and whether it failed.
fn run(mode: CollectiveChecks, pattern: &str, failing: usize) -> Vec<(bool, Vec<&'static str>)> {
    let pattern = pattern.to_owned();
    run_local(2, Duration::from_secs(20), move |c| {
        c.set_collective_checks(mode);
        let mesh = common::my_model::gen_mesh(5);
        let mut dm = DofManager::new("dm", c.clone());
        dm.register_nodal_dofs(&mesh, "u", 1).unwrap();
        let mut ms = ModelSolver::new(dm);
        let mut model = Recorder { ran: Vec::new() };
        let opts = ModelSolverOptions {
            time_step_solver_type: Some(TimeStepSolverType::DynamicLumped),
            non_linear_solver_type: Some(NonLinearSolverType::Lumped),
            ..ModelSolverOptions::default()
        };
        ms.get_new_solver("s", &opts, &mut model).unwrap();
        // no integration scheme: the solver drives the model directly
        if usize::try_from(c.rank()).unwrap() == failing {
            testing::fail_phase(&pattern, 1);
        }
        let r = ms.solve_step(&mut model, None);
        testing::clear();
        (r.is_err(), model.ran)
    })
}

#[test]
fn scheme_less_hooks_stop_where_every_phase_checks_stop() {
    let reference = run(CollectiveChecks::EveryPhase, "none", usize::MAX);
    assert!(reference.iter().all(|(failed, _)| !failed), "{reference:?}");
    for pattern in [
        "/before_solve_step",
        "/predictor",
        "/assemble_residual",
        "/solve_lumped",
        "/corrector",
        "/after_solve_step",
    ] {
        for failing in [0, 1] {
            let every = run(CollectiveChecks::EveryPhase, pattern, failing);
            let commit = run(CollectiveChecks::CommitPoints, pattern, failing);
            for (rank, (e, c)) in every.iter().zip(&commit).enumerate() {
                assert!(
                    e.0 && c.0,
                    "{pattern}, rank {failing} failing: rank {rank} succeeded"
                );
                assert_eq!(
                    e.1, c.1,
                    "{pattern}, rank {failing} failing: rank {rank} ran other hooks under commit \
                     points"
                );
            }
        }
    }
}
