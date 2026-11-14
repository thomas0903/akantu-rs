// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): test/test_synchronizer/{test_data_distribution.cc,
// test_node_synchronizer.cc, test_synchronizer_communication.cc} (the
// PARALLEL `test_synchronizers` gtest suite) on real MPI.

//! Real-MPI distribution and entity-synchronizer scenarios (harness = false,
//! `mpirun -np N mpi_distribution`; scripts/mpi/run-par-tests.sh). Prints
//! `PASS <name>` / `FAIL <name>: …` per scenario that ran, `NOT RUN
//! <name>: <why>` for each scenario that does not run at this rank count,
//! and `SUMMARY ranks=N scenarios=S failed=F
//! not_run=K` on rank 0 (S counts the scenarios that ran); exits 1 on
//! failure.

mod par_common;
mod pbd_common;

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
    // (scenario, oracle fixture it replays, function)
    type Scenario3 = (&'static str, &'static str, fn(Communicator) -> Outcome);
    let scenarios: &[Scenario] = &[
        ("test_data_distribution", par_common::data_distribution),
        (
            "test_data_distribution_tags",
            par_common::data_distribution_tags,
        ),
        (
            "test_node_synchronizer_synchrone",
            par_common::node_synchronizer_synchronize,
        ),
        (
            "test_node_synchronizer_asynchrone",
            par_common::node_synchronizer_asynchronous,
        ),
        (
            "test_node_synchronizer_gather",
            par_common::node_synchronizer_gather,
        ),
        (
            "test_node_synchronizer_scatter",
            par_common::node_synchronizer_scatter,
        ),
        (
            "test_synchronizer_communication_element",
            par_common::element_synchronizer_communication,
        ),
        (
            "distribution_invariants_cube",
            par_common::distribution_invariants,
        ),
        (
            "distribution_failure_agreed",
            par_common::distribution_failure_agreed,
        ),
        (
            "periodic_distribution_invariants",
            par_common::periodic_distribution_invariants,
        ),
        // UD-231
        (
            "pa02_orphan_point_has_one_owner",
            par_common::adv2d_orphan_point_has_one_owner,
        ),
        (
            "pa03_group_names_of_non_root_ranks_are_shared",
            par_common::group_names_of_non_root_ranks_are_shared,
        ),
    ];
    let size = control.size();
    let mut scenarios = scenarios.to_vec();
    // A scenario that does not run at this rank count is
    // not listed (no PASS, not counted) and says why
    let mut not_run = par_common::NotRun::default();
    // UD-235: refusals agreed between ranks
    if size > 1 {
        scenarios.push((
            "ud235_distribution_preconditions_refused",
            par_common::distribution_preconditions_refused,
        ));
    } else {
        not_run.add(
            "ud235_distribution_preconditions_refused",
            "the refusals are agreed between ranks: needs at least 2 ranks",
        );
    }
    // oracle replays at every rank count the probe recorded
    // (fixtures/parallel/<fixture>/np<P>: 1, 2 and 4 ranks, and 8 ranks for
    // distribution_cube_msh); each needs the oracle's
    // partition of that rank count (at 8 ranks without it rank 0 failed
    // before `distribute` while the other ranks waited in it)
    let recorded = |fixture: &str| par_common::oracle_rank_counts(fixture).contains(&size);
    let oracle: &[Scenario3] = &[
        (
            "distribution_matches_oracle",
            "distribution_cube_msh",
            par_common::distribution_matches_oracle,
        ),
        (
            "distributed_bounding_boxes_match_the_oracle",
            "distribution_cube_msh",
            par_common::bounding_boxes_match_oracle,
        ),
        (
            "distribution_adv2d_matches_oracle_in_compat",
            "distribution_adv2d",
            par_common::adv2d_matches_oracle_in_compat,
        ),
        (
            "pbd_strip_scotch_matches_oracle",
            "pbd_strip_scotch",
            pbd_common::strip_scotch_matches_oracle,
        ),
        (
            "pbd_strip_mesh_data_matches_oracle",
            "pbd_strip_mesh_data",
            pbd_common::strip_mesh_data_matches_oracle,
        ),
        (
            "distribution_sparse_mixed_matches_oracle",
            "distribution_sparse_mixed",
            pbd_common::sparse_mixed_matches_oracle,
        ),
    ];
    // Every listed fixture is recorded at some count
    let fixtures: Vec<&str> = oracle.iter().map(|(_, f, _)| *f).collect();
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
    // upstream's owner counts of adv2d at 1, 2 and 4 ranks,
    // and UD-1061 (the strip rule has 2 or 4 parts)
    if [1, 2, 4].contains(&size) {
        scenarios.push((
            "pa02_compat_reproduces_upstream_owners",
            par_common::adv2d_compat_reproduces_upstream_owners,
        ));
        scenarios.push((
            "ud1061_missing_periodic_nodes_carry_nodal_data",
            pbd_common::ud1061_missing_periodic_nodes_carry_nodal_data,
        ));
    } else {
        not_run.add(
            "pa02_compat_reproduces_upstream_owners",
            "upstream's owner counts of adv2d are recorded at 1, 2 and 4 ranks only",
        );
        not_run.add(
            "ud1061_missing_periodic_nodes_carry_nodal_data",
            "the strip partition rule (pbd_strip_mesh_data) has 1, 2 or 4 parts",
        );
    }
    // the port's Scotch partition against every recorded part count,
    // computed on one process
    let scotch = [
        "scotch_partition_matches_oracle",
        "pbd_strip_scotch_partition_matches_oracle",
    ];
    #[cfg(feature = "scotch")]
    if size == 1 {
        scenarios.push((scotch[0], par_common::scotch_partition_matches_oracle));
        scenarios.push((scotch[1], pbd_common::strip_scotch_partition_matches_oracle));
    } else {
        for name in scotch {
            not_run.add(
                name,
                "the partitioner runs on one process (every recorded part count is compared \
                 at 1 rank)",
            );
        }
    }
    #[cfg(not(feature = "scotch"))]
    for name in scotch {
        not_run.add(name, "built without the feature `scotch`");
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
