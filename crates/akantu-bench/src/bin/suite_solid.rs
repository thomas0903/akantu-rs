// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Benchmark-suite
// binary for solid mechanics and cohesive elements; no upstream
// counterpart.

//! `suite_solid`: the solid-mechanics and cohesive cases of the benchmark
//! suite (`akantu_bench::suite_solid`; command line and output in
//! `akantu_bench::suite`).

use std::process::ExitCode;

use akantu_comm::Communicator;
use akantu_core::Result;

#[cfg(feature = "mpi")]
fn world() -> Result<Communicator> {
    Communicator::mpi_world(akantu_comm::MpiOptions::default())
}

#[cfg(not(feature = "mpi"))]
fn world() -> Result<Communicator> {
    Ok(Communicator::serial())
}

fn main() -> ExitCode {
    let comm = match world() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("suite_solid: {e}");
            return ExitCode::FAILURE;
        }
    };
    akantu_bench::suite::run_main("solid", &akantu_bench::suite_solid::cases(), &comm)
}
