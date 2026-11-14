// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): extra_packages/extra-materials/test/
// test_material_damage/test_material_damage_iterative_non_local_parallel.sh
// (`mpirun -np 4`); akantu-rs: the extra non-local laws on distributed meshes under real MPI.

//! Real-MPI scenarios of the extra non-local laws (harness = false,
//! `mpirun -np N mpi_xmat`; scripts/mpi/run-par-tier.sh): TEST-187
//! (upstream's `test_material_damage_iterative_non_local_parallel`, its
//! criterion against the serial run), the four extra non-local laws
//! against the serial run and the refusal of the unsupported combination,
//! at every rank count of the
//! par tier. `PASS`/`FAIL` lines and a `SUMMARY` on rank 0; exit 1 on
//! failure.

mod common;
mod par_xmat;

use std::time::Duration;

use akantu_comm::lifecycle::ThreadLevel;
use akantu_comm::{Communicator, MpiOptions, ReduceOp, ReductionPolicy, SendMode};
use par_xmat::Outcome;

fn world() -> Communicator {
    let s = std::env::var("AKANTU_MPI_TEST_TIMEOUT_S")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(120);
    Communicator::mpi_world(MpiOptions {
        thread_level: ThreadLevel::Funneled,
        timeout: Some(Duration::from_secs(s)),
        send_mode: SendMode::Eager,
        reduction: ReductionPolicy::RankOrdered,
    })
    .expect("MPI world communicator")
}

fn main() {
    std::thread::spawn(|| {
        std::thread::sleep(Duration::from_secs(900));
        eprintln!("WATCHDOG: no completion after 900 s, aborting");
        std::process::abort();
    });
    let control = world();
    type S = fn(Communicator) -> Outcome;
    let scenarios: [(&str, S); 3] = [
        (
            "test_material_damage_iterative_non_local_parallel",
            par_xmat::test_187,
        ),
        (
            "extra_non_local_laws_distributed_vs_serial",
            par_xmat::laws_vs_serial,
        ),
        (
            "xn11_rank_local_maximum_with_the_global_rule_is_refused",
            par_xmat::xn11,
        ),
    ];
    let mut failed = 0u64;
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
            "SUMMARY ranks={} scenarios={} failed={failed}",
            control.size(),
            scenarios.len()
        );
    }
    drop(control);
    std::process::exit(i32::from(failed > 0));
}
