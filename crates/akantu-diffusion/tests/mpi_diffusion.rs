// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). DIFF-010:
// the heat-transfer model on distributed meshes on real MPI.

//! Real-MPI diffusion scenarios (harness = false, `mpirun -np N
//! mpi_diffusion`; scripts/mpi/run-par-tier.sh). `PASS`/`FAIL` lines for
//! the scenarios that ran, `NOT RUN <name>: <why>` for those that do not
//! run at this rank count and a `SUMMARY … not_run=K`
//! on rank 0 (`scenarios=` counts the scenarios that ran); exit 1 on
//! failure.

#[path = "../../akantu-mesh/tests/par_common/mod.rs"]
mod par_common;
mod par_diffusion;

use std::time::Duration;

use akantu_comm::lifecycle::ThreadLevel;
use akantu_comm::{Communicator, MpiOptions, ReduceOp, ReductionPolicy, SendMode};
use par_diffusion::{Outcome, Run};

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
            "diffusion_explicit_distributed_vs_serial",
            Box::new(|c| par_diffusion::distributed_vs_serial(c, Run::Explicit, 4, 30, 1e-13)),
        ),
        (
            "diffusion_static_distributed_vs_serial",
            Box::new(|c| par_diffusion::distributed_vs_serial(c, Run::Static, 3, 1, 1e-14)),
        ),
        (
            "diffusion_implicit_distributed_vs_serial",
            Box::new(|c| par_diffusion::distributed_vs_serial(c, Run::Implicit, 3, 5, 1e-12)),
        ),
        (
            "diffusion_model_synchronize_tags",
            Box::new(par_diffusion::model_synchronize_tags),
        ),
        (
            "diffusion_ud275_compat_reproduces_upstream_failure",
            Box::new(par_diffusion::ud275_compat_reproduces_upstream_failure),
        ),
        (
            "diffusion_thermal_energy_by_node_counts_each_node_once",
            Box::new(par_diffusion::thermal_energy_by_node_counts_each_node_once),
        ),
    ];
    // A scenario that does not run at this rank count is
    // not listed (no PASS, not counted) and says why
    let mut not_run = par_common::NotRun::default();
    // the oracle replay at every rank count the probe recorded (1, 2, 4 and
    // 8): the fixture must be present
    let gradient = "diffusion_gradient_cube_n3";
    if let Err(e) = par_common::missing_oracle_fixtures("parallel", &[gradient]) {
        println!("FAIL start-up (rank {}): {e}", control.rank());
        drop(control);
        std::process::exit(1);
    }
    if par_common::oracle_rank_counts(gradient).contains(&size) {
        scenarios.push((
            "diffusion_gradient_matches_oracle",
            Box::new(par_diffusion::diffusion_gradient_matches_oracle),
        ));
    } else {
        not_run.add(
            "diffusion_gradient_matches_oracle",
            par_common::not_recorded("parallel", gradient, size),
        );
    }
    // rank-specific failures agreed on every rank: meaningful from 2 ranks
    let multi: [Named; 2] = [
        (
            "diffusion_rank_specific_selector_failure_is_agreed",
            par_diffusion::rank_specific_selector_failure_is_agreed,
        ),
        (
            "diffusion_rank_specific_init_failure_is_agreed_fatal",
            par_diffusion::rank_specific_init_failure_is_agreed_fatal,
        ),
    ];
    for (name, f) in multi {
        if size > 1 {
            scenarios.push((name, Box::new(f)));
        } else {
            not_run.add(name, "a rank-specific failure needs at least 2 ranks");
        }
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
