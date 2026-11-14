// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs (no upstream counterpart: no upstream test runs the
// split-node contact in parallel; the probe calls NTNBaseContact::initParallel(),
// D-083): the split-node contact on distributed meshes on real MPI.

//! Real-MPI split-node contact scenarios (harness = false, `mpirun -np N
//! mpi_ntn`; scripts/mpi/run-par-tests.sh). `PASS`/`FAIL` lines for the
//! scenarios that ran, `NOT RUN <name>: <why>` for those that do not run at
//! this rank count and a `SUMMARY … not_run=K` on rank 0
//! (`scenarios=` counts the scenarios that ran); exit 1 on failure. The
//! oracle replays run at every
//! rank count the probe recorded (1, 2, 4; 8 where it was recorded
//! it).

#[path = "../../akantu-mesh/tests/par_common/mod.rs"]
mod par_common;
mod par_ntn;

use std::time::Duration;

use akantu_comm::lifecycle::ThreadLevel;
use akantu_comm::{Communicator, MpiOptions, ReduceOp, ReductionPolicy, SendMode};
use par_ntn::Outcome;

fn world_with(reduction: ReductionPolicy) -> Communicator {
    let s = std::env::var("AKANTU_MPI_TEST_TIMEOUT_S")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(60);
    Communicator::mpi_world(MpiOptions {
        thread_level: ThreadLevel::Funneled,
        timeout: Some(Duration::from_secs(s)),
        send_mode: SendMode::Eager,
        reduction,
    })
    .expect("MPI world communicator")
}

/// A scenario that runs only at some rank counts: its name and function.
type Named = (&'static str, fn(Communicator) -> Outcome);

fn main() {
    std::thread::spawn(|| {
        std::thread::sleep(Duration::from_secs(900));
        eprintln!("WATCHDOG: no completion after 900 s, aborting");
        std::process::abort();
    });
    let control = world_with(ReductionPolicy::RankOrdered);
    // upstream's MPI_Allreduce for the global sums (D-039)
    let native = world_with(ReductionPolicy::NativeMpi);
    type S = Box<dyn Fn(Communicator) -> Outcome>;
    let mut scenarios: Vec<(&str, S, bool)> = Vec::new();
    // the replays on the oracle's strip partitions, at every rank count the
    // probe recorded (fixtures/parallel/<fixture>/np<P>: 1, 2 and 4 ranks;
    // 8 where it was recorded)
    let size = control.size();
    let recorded = |fixture: &str| par_common::oracle_rank_counts(fixture).contains(&size);
    // (scenario, oracle fixture it replays, function)
    type Replay = (&'static str, &'static str, fn(Communicator) -> Outcome);
    let oracle: [Replay; 4] = [
        (
            "ntn_compression_matches_oracle",
            "ntn_compression",
            par_ntn::ntn_compression_matches_oracle,
        ),
        (
            "ntrf_coulomb_matches_oracle",
            "ntrf_coulomb",
            par_ntn::ntrf_coulomb_matches_oracle,
        ),
        // the ntn_compression setting and partition (no oracle values)
        (
            "update_normals_after_init_parallel",
            "ntn_compression",
            par_ntn::update_normals_after_init_parallel,
        ),
        (
            "ntn_update_normals_matches_oracle",
            "ntn_update_normals",
            par_ntn::ntn_update_normals_matches_oracle,
        ),
    ];
    // A scenario that does not run at this rank count is
    // not listed (no PASS, not counted) and says why
    let mut not_run = par_common::NotRun::default();
    // Every listed fixture is recorded at some count
    let fixtures: Vec<&str> = oracle.iter().map(|(_, f, _)| *f).collect();
    if let Err(e) = par_common::missing_oracle_fixtures("parallel", &fixtures) {
        println!("FAIL start-up (rank {}): {e}", control.rank());
        drop(native);
        drop(control);
        std::process::exit(1);
    }
    for (name, fixture, f) in oracle {
        if recorded(fixture) {
            scenarios.push((name, Box::new(f), false));
        } else {
            not_run.add(name, par_common::not_recorded("parallel", fixture, size));
        }
    }
    // NativeMpi policy: the external work reduced by MPI_Allreduce as
    // upstream, compared bitwise (at 8 ranks its summation order differs
    // from the rank order)
    if recorded("ntn_compression") {
        scenarios.push((
            "ntn_compression_matches_oracle_native_mpi",
            Box::new(par_ntn::ntn_compression_matches_oracle),
            true,
        ));
    } else {
        not_run.add(
            "ntn_compression_matches_oracle_native_mpi",
            par_common::not_recorded("parallel", "ntn_compression", size),
        );
    }
    // contact nodes added after `init_parallel` on more than one rank are
    // refused: on the ntn_compression setting and
    // partition, wherever that fixture is recorded (the local world runs
    // it at 1 / 2 / 4 / 8 ranks)
    if recorded("ntn_compression") {
        scenarios.push((
            "additions_after_init_parallel_are_refused",
            Box::new(par_ntn::additions_after_init_parallel_are_refused),
            false,
        ));
    } else {
        not_run.add(
            "additions_after_init_parallel_are_refused",
            par_common::not_recorded("parallel", "ntn_compression", size),
        );
    }
    scenarios.push((
        "separated_partition_is_refused",
        Box::new(par_ntn::separated_partition_is_refused),
        false,
    ));
    scenarios.push((
        "distributed_contact_without_init_parallel_is_refused",
        Box::new(par_ntn::refused_without_init_parallel),
        false,
    ));
    // rank-specific failures agreed on every rank: meaningful from 2 ranks
    let multi: [Named; 2] = [
        (
            "contact_sums_failure_is_agreed",
            par_ntn::contact_sums_failure_is_agreed,
        ),
        (
            "rank_specific_contact_step_failure_is_agreed",
            par_ntn::contact_step_failure_is_agreed,
        ),
    ];
    for (name, f) in multi {
        if size > 1 {
            scenarios.push((name, Box::new(f), false));
        } else {
            not_run.add(name, "a rank-specific failure needs at least 2 ranks");
        }
    }
    not_run.print(control.rank());
    let mut failed = 0u64;
    let count = scenarios.len();
    for (name, f, nat) in &scenarios {
        let comm = if *nat {
            native.duplicate()
        } else {
            control.duplicate()
        }
        .expect("duplicate");
        let t0 = std::time::Instant::now();
        let r = f(comm);
        let mut bad = [u64::from(r.is_err())];
        control
            .all_reduce(&mut bad, ReduceOp::Sum)
            .expect("all_reduce");
        if let Err(e) = &r {
            println!("FAIL {name} (rank {}): {e}", control.rank());
        }
        if bad[0] > 0 {
            failed += 1;
        } else if control.rank() == 0 {
            println!("PASS {name} ({:.2} s)", t0.elapsed().as_secs_f64());
        }
    }
    if control.rank() == 0 {
        println!(
            "SUMMARY ranks={} scenarios={} failed={failed} not_run={}",
            control.size(),
            count,
            not_run.count()
        );
    }
    drop(native);
    drop(control);
    std::process::exit(i32::from(failed > 0));
}
