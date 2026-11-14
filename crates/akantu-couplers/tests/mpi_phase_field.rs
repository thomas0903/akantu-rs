// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs (ported intent: examples/c++/phase_field_model/
// phase_field_parallel.cc, PARALLEL): the solid–phase-field coupler on
// distributed meshes on real MPI.

//! Real-MPI phase-field scenarios (harness = false, `mpirun -np N
//! mpi_phase_field`; scripts/mpi/run-par-tier.sh). `PASS`/`FAIL` lines for
//! the scenarios that ran, `NOT RUN <name>: <why>` for those that do not
//! run at this rank count and a `SUMMARY … not_run=K`
//! on rank 0 (`scenarios=` counts the scenarios that ran); exit 1 on
//! failure.

#[path = "../../akantu-mesh/tests/par_common/mod.rs"]
mod par_common;
mod par_phase_field;

use std::time::Duration;

use akantu_comm::lifecycle::ThreadLevel;
use akantu_comm::{Communicator, MpiOptions, ReduceOp, ReductionPolicy, SendMode};
use par_phase_field::Outcome;

fn world() -> Communicator {
    let s = std::env::var("AKANTU_MPI_TEST_TIMEOUT_S")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(60);
    Communicator::mpi_world(MpiOptions {
        thread_level: ThreadLevel::Funneled,
        timeout: Some(Duration::from_secs(s)),
        send_mode: SendMode::Eager,
        reduction: ReductionPolicy::RankOrdered,
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
    let control = world();
    let size = control.size();
    type S = Box<dyn Fn(Communicator) -> Outcome>;
    let mut scenarios: Vec<(&str, S)> = vec![
        (
            "phase_field_coupler_distributed_vs_serial",
            Box::new(|c| {
                par_phase_field::distributed_vs_serial(c, 8, 4, 2, par_phase_field::ENVELOPE)
            }),
        ),
        (
            "phase_field_model_synchronize_tags",
            Box::new(par_phase_field::model_synchronize_tags),
        ),
        // fails if distributed element events are accepted
        (
            "pf_distributed_element_events_refused",
            Box::new(par_phase_field::pf_distributed_element_events_refused),
        ),
        (
            "ud004_ghost_gradient_switch",
            Box::new(par_phase_field::ud004_ghost_gradient_switch),
        ),
        // UD-905: one convergence answer on every rank
        (
            "check_convergence_is_collective",
            Box::new(par_phase_field::check_convergence_is_collective),
        ),
    ];
    // A scenario that does not run at this rank count is
    // not listed (no PASS, not counted) and says why
    let mut not_run = par_common::NotRun::default();
    // rank-specific failures agreed on every rank: meaningful from 2 ranks
    let multi: [Named; 3] = [
        (
            "pf_selector_failure_is_agreed",
            par_phase_field::pf_selector_failure_is_agreed,
        ),
        (
            "pf_init_failures_are_agreed_fatal",
            par_phase_field::pf_init_failures_are_agreed_fatal,
        ),
        (
            "pfm_damage_failure_is_agreed",
            par_phase_field::pfm_damage_failure_is_agreed,
        ),
    ];
    for (name, f) in multi {
        if size > 1 {
            scenarios.push((name, Box::new(f)));
        } else {
            not_run.add(name, "a rank-specific failure needs at least 2 ranks");
        }
    }
    // the oracle: bitwise at 1 rank (class B); at 2 and 4 ranks every
    // recorded upstream result of the variation study (D-082, class E:
    // upstream's distributed MUMPS solve is not repeatable):
    // the fixture must be present
    let square = "phase_field_square_n8";
    if let Err(e) = par_common::missing_oracle_fixtures("parallel", &[square]) {
        println!("FAIL start-up (rank {}): {e}", control.rank());
        drop(control);
        std::process::exit(1);
    }
    let recorded = par_common::oracle_rank_counts(square).contains(&size);
    if recorded && size == 1 {
        scenarios.push((
            "phase_field_square_matches_oracle",
            Box::new(par_phase_field::phase_field_square_matches_oracle),
        ));
    } else {
        not_run.add(
            "phase_field_square_matches_oracle",
            "bitwise at one rank only (upstream's distributed result varies run to run, \
             D-082: phase_field_square_matches_result_set compares at 2 and 4 ranks)",
        );
    }
    if recorded && (size == 2 || size == 4) {
        scenarios.push((
            "phase_field_square_matches_result_set",
            Box::new(par_phase_field::phase_field_square_matches_result_set),
        ));
    } else if size == 1 {
        not_run.add(
            "phase_field_square_matches_result_set",
            "one rank: compared bitwise by phase_field_square_matches_oracle",
        );
    } else {
        not_run.add(
            "phase_field_square_matches_result_set",
            format!(
                "the D-082 result sets are recorded at 2 and 4 ranks only; {}",
                par_common::not_recorded("parallel", square, size)
            ),
        );
    }
    not_run.print(control.rank());
    let mut failed = 0u64;
    let count = scenarios.len();
    for (name, f) in &scenarios {
        let comm = control.duplicate().expect("duplicate");
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
            "SUMMARY ranks={} scenarios={count} failed={failed} not_run={}",
            control.size(),
            not_run.count()
        );
    }
    drop(control);
    std::process::exit(i32::from(failed > 0));
}
