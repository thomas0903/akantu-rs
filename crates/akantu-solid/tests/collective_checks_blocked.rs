// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). No upstream
// counterpart: the blocked-DOF snapshot a failed collective operation leaves
// under the two collective-check modes (D-018, D-126).
//
// Every non-linear solver updates the DOF manager's blocked snapshot
// (`update_global_blocked_dofs`: the blocked indexes, their flags and the
// release that tells the integration schemes and `apply_boundary` that the
// boundary changed) right after the step `before_solve_step`, as upstream.
// When the model's `before_solve_step` fails on one rank, under every-phase
// checks no rank updates the snapshot; under commit points the peers learn
// of the failure only at the next agreement, so they must put their
// snapshot back there. Otherwise, after a change of the boundary conditions,
// their `has_blocked_dofs_changed()` would be `false` at the retry while the
// failing rank's is `true`: they would neither rebuild `J` nor re-apply its
// boundary rows, and the retried solve would differ from the every-phase
// run. Checked for the static Newton–Raphson solve (pseudo-time), the
// implicit dynamic one (trapezoidal rule) and the explicit lumped solver,
// failing on each rank at 2 ranks; a failure of the stiffness assembly
// (after the update) is the control.

#[path = "../../akantu-mesh/tests/par_common/mod.rs"]
mod par_common;
mod par_smm;

use std::time::Duration;

use akantu_comm::phase::{CollectiveChecks, testing};
use akantu_comm::{Communicator, run_local};
use akantu_core::enums::AnalysisMethod;
use akantu_mesh::SharedMesh;
use akantu_model::ModelOptions;
use akantu_parser::Parser;
use akantu_solid::SolidMechanicsModel;
use par_smm::{Part, ok};

#[derive(Debug)]
struct Out {
    /// The injected solve's error (`None`: no injection).
    failed: Option<String>,
    /// The blocked snapshot after it: changed flag, release, indexes, flags.
    snapshot: (bool, u64, Vec<usize>, Vec<bool>),
    /// The displacement and velocity after the retried solve, bitwise.
    u: Vec<u64>,
}

fn run(
    comm: &Communicator,
    method: AnalysisMethod,
    mode: CollectiveChecks,
    failing: Option<usize>,
    pattern: &str,
) -> Result<Out, String> {
    comm.set_collective_checks(mode);
    let rank = usize::try_from(comm.rank()).unwrap();
    let m = par_smm::distributed_cube(comm, 3, &Part::Rcb)?;
    let shared = SharedMesh::new(m);
    let parser = ok(Parser::parse_str(par_smm::DAT, "material.dat"), "parser")?;
    let mut model = ok(
        SolidMechanicsModel::with_parser(shared.clone(), None, "smm", parser),
        "model",
    )?;
    ok(model.init_full(&ModelOptions::new(method)), "init")?;
    if method != AnalysisMethod::Static {
        let dt = ok(model.stable_time_step(), "dt")? * 0.5;
        ok(model.set_time_step(dt, None), "set dt")?;
    }
    let (coords, nb) = {
        let mm = ok(shared.read(), "read")?;
        (mm.nodes().as_slice().to_vec(), mm.nb_nodes())
    };
    {
        let mut b_g = ok(model.blocked_dofs_mut(), "blocked")?;
        let b = b_g.as_mut_slice();
        for n in 0..nb {
            if coords[3 * n + 2] == 0.0 {
                b[3 * n..3 * n + 3].fill(true);
            }
        }
    }
    {
        let mut f_g = ok(model.external_force_mut(), "f")?;
        let f = f_g.as_mut_slice();
        for n in 0..nb {
            if coords[3 * n + 2] == 1.0 {
                f[3 * n] = 1e-3 * (1.0 + coords[3 * n + 1]);
                f[3 * n + 2] = -2e-3;
            }
        }
    }
    ok(model.solve_step(None), "first solve")?;
    // the user changes the boundary conditions: x blocked on the upper half
    {
        let mut b_g = ok(model.blocked_dofs_mut(), "blocked")?;
        let b = b_g.as_mut_slice();
        for n in 0..nb {
            if coords[3 * n + 2] > 0.5 {
                b[3 * n] = true;
            }
        }
    }
    let mut failed = None;
    if let Some(f) = failing {
        if rank == f {
            testing::fail_phase(pattern, 1);
        }
        let r = model.solve_step(None);
        testing::clear();
        failed = Some(match r {
            Ok(()) => return Err(format!("rank {rank}: the injected solve succeeded")),
            Err(e) => e.to_string(),
        });
    }
    let dm = model.dof_manager();
    let snapshot = (
        dm.has_blocked_dofs_changed(),
        dm.blocked_release(),
        dm.global_blocked_dofs_indexes().to_vec(),
        dm.global_blocked_dofs().to_vec(),
    );
    ok(model.solve_step(None), "retried solve")?;
    let mut u: Vec<u64> = ok(model.displacement(), "u")?
        .as_slice()
        .iter()
        .map(|x| x.to_bits())
        .collect();
    if method != AnalysisMethod::Static {
        u.extend(
            ok(model.velocity(), "v")?
                .as_slice()
                .iter()
                .map(|x| x.to_bits()),
        );
    }
    Ok(Out {
        failed,
        snapshot,
        u,
    })
}

fn check(method: AnalysisMethod, np: usize, failing: usize, pattern: &str) {
    let out = run_local(np, Duration::from_secs(120), |comm| {
        let reference = run(&comm, method, CollectiveChecks::EveryPhase, None, pattern)?;
        let every = run(
            &comm,
            method,
            CollectiveChecks::EveryPhase,
            Some(failing),
            pattern,
        )?;
        let commit = run(
            &comm,
            method,
            CollectiveChecks::CommitPoints,
            Some(failing),
            pattern,
        )?;
        Ok::<_, String>((reference, every, commit))
    });
    let mut diffs = Vec::new();
    for (rank, r) in out.into_iter().enumerate() {
        let (reference, every, commit) =
            r.unwrap_or_else(|e| panic!("{method:?}, {pattern}, rank {rank}: {e}"));
        assert!(every.failed.is_some() && commit.failed.is_some());
        // a failure before the update changes nothing: the retry gives the
        // run without failure (a failed stiffness assembly leaves a stale
        // `J` boundary on the failing rank under every-phase checks too:
        // there only the modes are compared)
        if pattern == "model:before_solve_step" {
            assert_eq!(
                reference.u, every.u,
                "{method:?}, rank {rank}: every-phase run after the failure differs from the \
                 run without it"
            );
        }
        if every.snapshot != commit.snapshot {
            diffs.push(format!(
                "rank {rank}: blocked snapshot after the failed solve: every-phase (changed {}, \
                 release {}), commit points (changed {}, release {}){}",
                every.snapshot.0,
                every.snapshot.1,
                commit.snapshot.0,
                commit.snapshot.1,
                if every.snapshot.2 == commit.snapshot.2 {
                    ""
                } else {
                    ", indexes differ"
                }
            ));
        }
        if every.u != commit.u {
            let n = every
                .u
                .iter()
                .zip(&commit.u)
                .filter(|(a, b)| a != b)
                .count();
            diffs.push(format!(
                "rank {rank}: retried solve: {n} of {} values differ between the modes",
                every.u.len()
            ));
        }
    }
    assert!(
        diffs.is_empty(),
        "{method:?}, {pattern}, rank {failing} failing:\n{}",
        diffs.join("\n")
    );
}

#[test]
fn failed_before_solve_step_leaves_the_every_phase_blocked_snapshot_static() {
    for failing in [0, 1] {
        check(
            AnalysisMethod::Static,
            2,
            failing,
            "model:before_solve_step",
        );
    }
}

#[test]
fn failed_before_solve_step_leaves_the_every_phase_blocked_snapshot_implicit_dynamic() {
    for failing in [0, 1] {
        check(
            AnalysisMethod::ImplicitDynamic,
            2,
            failing,
            "model:before_solve_step",
        );
    }
}

#[test]
fn failed_before_solve_step_leaves_the_every_phase_blocked_snapshot_explicit_lumped() {
    for failing in [0, 1] {
        check(
            AnalysisMethod::ExplicitLumpedMass,
            2,
            failing,
            "model:before_solve_step",
        );
    }
}

/// Control: a failure of the deferred `model:assemble_matrix:K` step after
/// the update (every rank updated under both modes).
#[test]
fn failed_stiffness_assembly_leaves_the_every_phase_blocked_snapshot() {
    for failing in [0, 1] {
        check(
            AnalysisMethod::Static,
            2,
            failing,
            "model:assemble_matrix:K",
        );
    }
}

/// Control: a failure of the model's predictor hook, after the update
/// (every rank updated under both modes; the update must stand).
#[test]
fn failed_predictor_after_the_update_keeps_the_blocked_snapshot() {
    for method in [AnalysisMethod::Static, AnalysisMethod::ExplicitLumpedMass] {
        for failing in [0, 1] {
            check(method, 2, failing, "model:predictor");
        }
    }
}
