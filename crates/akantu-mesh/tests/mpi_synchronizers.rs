// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): test/test_synchronizer/{test_facet_synchronizer.cc,
// test_grid_synchronizer.cc, test_grid_synchronizer_check_neighbors.cc},
// test/test_mesh_utils/test_mesh_periodic/test_mesh_periodic.cc (PARALLEL) on
// real MPI.

//! Real-MPI scenarios (harness = false, `mpirun -np N
//! mpi_synchronizers`; scripts/mpi/run-par-tests.sh): distributed facets
//! and the facet, grid and periodic node synchronizers, with the oracle
//! comparisons at every rank count the probe recorded (1/2/4, some at 8).
//! Prints `PASS <name>` / `FAIL <name>: …` per scenario that ran, `NOT
//! RUN <name>: <why>` for each scenario that does not run at this rank
//! count, and `SUMMARY ranks=N scenarios=S failed=F
//! not_run=K` on rank 0 (S counts the scenarios that ran); exits 1 on
//! failure.

mod par2a_common;
mod par_common;

use std::time::Duration;

use akantu_comm::lifecycle::ThreadLevel;
use akantu_comm::{Communicator, MpiOptions, ReduceOp, SendMode};
use par_common::Outcome;

fn timeout() -> Duration {
    let s = std::env::var("AKANTU_MPI_TEST_TIMEOUT_S")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(30);
    Duration::from_secs(s)
}

fn watchdog(secs: u64) {
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(secs));
        eprintln!("WATCHDOG: no completion after {secs} s, aborting");
        std::process::abort();
    });
}

fn world() -> Communicator {
    Communicator::mpi_world(MpiOptions {
        thread_level: ThreadLevel::Funneled,
        timeout: Some(timeout()),
        send_mode: SendMode::Eager,
        ..MpiOptions::default()
    })
    .expect("MPI world communicator")
}

fn main() {
    watchdog(600);
    let control = world();
    type Scenario = (&'static str, fn(Communicator) -> Outcome);
    let mut scenarios: Vec<Scenario> = vec![
        (
            "test_facet_synchronizer",
            par2a_common::test_facet_synchronizer,
        ),
        (
            "distributed_facets_invariants",
            par2a_common::facet_invariants,
        ),
        (
            "distributed_facets_failure_agreed",
            par2a_common::facet_failure_agreed,
        ),
        (
            "distributed_facets_transaction_install",
            par2a_common::facet_transaction_install,
        ),
        (
            "test_grid_synchronizer_check_neighbors",
            par2a_common::test_grid_synchronizer,
        ),
        (
            "grid_synchronizer_through_mutation",
            par2a_common::grid_synchronizer_through_mutation,
        ),
        (
            "grid_synchronizer_keeps_facet_mesh_consistent",
            par2a_common::grid_synchronizer_keeps_facet_mesh_consistent,
        ),
        (
            "test_mesh_periodic_distributed",
            par2a_common::test_mesh_periodic,
        ),
        (
            "periodic_failure_agreed",
            par2a_common::periodic_failure_agreed,
        ),
        (
            "ud251_mixed_facet_types",
            par2a_common::ud251_mixed_facet_types,
        ),
        (
            "ud254_periodic_after_distribution",
            par2a_common::ud254_periodic_after_distribution,
        ),
        ("ud813_empty_part", par2a_common::ud813_empty_part),
    ];
    let size = control.size();
    // A scenario that does not run at this rank count is
    // not listed (no PASS, not counted) and says why
    let mut not_run = par_common::NotRun::default();
    // rank-specific failures of the grid synchronizer: the last rank fails,
    // the others must agree
    for (name, f) in [
        (
            "grid_synchronizer_failure_agreed",
            par2a_common::grid_failure_agreed as fn(Communicator) -> Outcome,
        ),
        (
            "grid_cells_failure_agreed",
            par2a_common::grid_cells_failure_agreed,
        ),
    ] {
        if size > 1 {
            scenarios.push((name, f));
        } else {
            not_run.add(name, "a rank-specific failure needs at least 2 ranks");
        }
    }
    // oracle replays at every rank count the probe recorded
    // (fixtures/parallel/<fixture>/np<P>: 1, 2 and 4 ranks; facets_cube_n4,
    // grid_cube_n4 and periodic_3d_xyz also at 8)
    let recorded = |fixture: &str| par_common::oracle_rank_counts(fixture).contains(&size);
    // (scenario, oracle fixture it replays, function)
    type Replay = (&'static str, &'static str, fn(Communicator) -> Outcome);
    let oracle: &[Replay] = &[
        (
            "facets_cube_msh_matches_oracle",
            "facets_cube_msh",
            par2a_common::facets_cube_msh_matches_oracle,
        ),
        (
            "facets_cube_n4_matches_oracle",
            "facets_cube_n4",
            par2a_common::facets_cube_n4_matches_oracle,
        ),
        (
            "facets_adv2d_matches_oracle",
            "facets_adv2d",
            par2a_common::facets_adv2d_matches_oracle,
        ),
        (
            "grid_tri_n8_matches_oracle",
            "grid_tri_n8",
            par2a_common::grid_tri_n8_matches_oracle,
        ),
        (
            "grid_cube_n4_matches_oracle",
            "grid_cube_n4",
            par2a_common::grid_cube_n4_matches_oracle,
        ),
        (
            "v101_facets_tri_quadrants_matches_oracle",
            "v101_facets_tri_quadrants",
            par2a_common::v101_facets_tri_quadrants_matches_oracle,
        ),
        (
            "v101_facets_tet_quadrants_matches_oracle",
            "v101_facets_tet_quadrants",
            par2a_common::v101_facets_tet_quadrants_matches_oracle,
        ),
        (
            "v101_grid_wide_matches_oracle",
            "v101_grid_wide",
            par2a_common::v101_grid_wide_matches_oracle,
        ),
        (
            "v101_grid_adv2d_matches_oracle",
            "v101_grid_adv2d",
            par2a_common::v101_grid_adv2d_matches_oracle,
        ),
    ];
    // Every listed fixture is recorded at some count
    let mut fixtures: Vec<String> = oracle.iter().map(|(_, f, _)| f.to_string()).collect();
    fixtures.extend(
        par2a_common::PERIODIC_TYPES
            .iter()
            .map(|&(d, p)| par2a_common::periodic_scenario_name(d, p)),
    );
    fixtures.extend(
        par2a_common::V101_PERIODIC
            .iter()
            .map(|&(d, n, _)| format!("v101_periodic_{d}d_{}", &"xyz"[..n])),
    );
    let fixtures: Vec<&str> = fixtures.iter().map(String::as_str).collect();
    if let Err(e) = par_common::missing_oracle_fixtures("parallel", &fixtures) {
        println!("FAIL start-up (rank {}): {e}", control.rank());
        drop(control);
        std::process::exit(1);
    }
    for (name, fixture, f) in oracle {
        if recorded(fixture) {
            scenarios.push((*name, *f));
        } else {
            not_run.add(*name, par_common::not_recorded("parallel", fixture, size));
        }
    }
    // the TestMeshPeriodic cases recorded at this rank count (each one
    // where recorded; par2a_common::periodic_matches_oracle)
    if par2a_common::PERIODIC_TYPES
        .iter()
        .any(|&(d, p)| recorded(&par2a_common::periodic_scenario_name(d, p)))
    {
        scenarios.push((
            "periodic_matches_oracle",
            par2a_common::periodic_matches_oracle,
        ));
    } else {
        not_run.add(
            "periodic_matches_oracle",
            format!("no TestMeshPeriodic case recorded at {size} ranks"),
        );
    }
    if par2a_common::V101_PERIODIC
        .iter()
        .all(|&(d, n, _)| recorded(&format!("v101_periodic_{d}d_{}", &"xyz"[..n])))
    {
        scenarios.push((
            "v101_periodic_matches_oracle",
            par2a_common::v101_periodic_matches_oracle,
        ));
    } else {
        not_run.add(
            "v101_periodic_matches_oracle",
            format!("the adversarial periodic cases are not all recorded at {size} ranks"),
        );
    }
    not_run.print(control.rank());
    let mut failed = 0u64;
    for (name, f) in &scenarios {
        // a fresh duplicate per scenario (own scope registry)
        let comm = control.duplicate().expect("duplicate");
        let t0 = std::time::Instant::now();
        let r = f(comm);
        let mut bad = [u64::from(r.is_err())];
        control
            .all_reduce(&mut bad, ReduceOp::Sum)
            .expect("all_reduce");
        match &r {
            Ok(()) if bad[0] == 0 => {
                if control.rank() == 0 {
                    println!("PASS {name} ({:.2} s)", t0.elapsed().as_secs_f64());
                }
            }
            Ok(()) => failed += 1,
            Err(e) => {
                failed += 1;
                println!("FAIL {name} (rank {}): {e}", control.rank());
            }
        }
        if bad[0] > 0 && r.is_ok() && control.rank() == 0 {
            println!("FAIL {name}: failed on another rank");
        }
    }
    if control.rank() == 0 {
        println!(
            "SUMMARY ranks={} scenarios={} failed={failed} not_run={}",
            control.size(),
            scenarios.len(),
            not_run.count()
        );
    }
    // last guard dropped on the main thread: akantu finalises MPI
    drop(control);
    std::process::exit(i32::from(failed > 0));
}
