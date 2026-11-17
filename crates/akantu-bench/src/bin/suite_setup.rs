// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). The set-up benchmark
// suite (no upstream counterpart): meshes, partitioning and distribution,
// the MPI layer, the parser, the dumpers and the Rust twin of the Python
// benchmarks.

//! `suite_setup`: see `crates/akantu-bench/README.md`, section "Set-up
//! suite". `suite_setup --list`; `suite_setup --case <name>[,…]|all --size
//! s|m|l [--reps N] [--out FILE] [--loop-seconds S] [--threads N]
//! [--grain finest|auto] [--local-ranks N]`. MPI cases take the rank count from `mpirun -n`
//! (feature `mpi`), or from `--local-ranks` (in-process ranks).

#![forbid(unsafe_code)]

#[path = "../suite_setup/mod.rs"]
#[allow(dead_code)] // helpers shared by cases that a feature set may leave unused
mod suite_setup;

use std::process::ExitCode;
use std::time::Duration;

use akantu_comm::{Communicator, run_local};
use akantu_core::Result;

use suite_setup::harness::{parse_cli, run_cases};

#[cfg(feature = "mpi")]
fn world() -> Result<Communicator> {
    Communicator::mpi_world(akantu_comm::MpiOptions::default())
}

#[cfg(not(feature = "mpi"))]
fn world() -> Result<Communicator> {
    Ok(Communicator::serial())
}

fn main() -> ExitCode {
    let mut cli = match parse_cli(std::env::args().skip(1)) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("suite_setup: {e}");
            return ExitCode::from(2);
        }
    };
    // before any model is created: the policy of every model of the run
    if let Err(e) = cli.apply_exec() {
        eprintln!("suite_setup: {e}");
        return ExitCode::from(2);
    }
    let cases = suite_setup::cases();
    let results: Vec<Result<()>> = match cli.local_ranks {
        Some(n) if n > 1 => run_local(n, Duration::from_secs(24 * 3600), |c| {
            run_cases(&cli, &cases, &c)
        }),
        _ => vec![world().and_then(|c| run_cases(&cli, &cases, &c))],
    };
    let mut status = ExitCode::SUCCESS;
    for (r, res) in results.into_iter().enumerate() {
        if let Err(e) = res {
            eprintln!("suite_setup (rank {r}): {e}");
            status = ExitCode::FAILURE;
        }
    }
    status
}
