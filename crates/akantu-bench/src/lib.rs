// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu's JOSS scalability test (EPFL/LSMS):
// performance-testing-cohesive @ 483bace8 (cohesive_extrinsic.cc), as
// instrumented by the akantu-rs harness benchmarks/cohesive-perf/.

//! # akantu-bench
//!
//! Benchmark drivers. [`driver::run`] is the Rust port of the Akantu JOSS
//! cohesive scalability test (`performance-testing-cohesive`,
//! `cohesive_extrinsic.cc`): the same input files, phases, Chrono CSV
//! (`timmings_*.csv` rows) and output files (`energies.csv`,
//! `insertions.csv`, `summary.json`, `summary_rank<r>.json`,
//! `partition.csv`) as the C++ harness in `benchmarks/cohesive-perf/`, so
//! that both series share one collection and analysis path. The binary
//! `cohesive_extrinsic` wraps it (`--aka_input_file input.dat`, as the C++
//! executable). See `benchmarks/cohesive-perf/README.md`. Cargo features,
//! off by default: `mpi`, `scotch` and `mumps` (runs under real MPI, with
//! Scotch partitioning, and with the parallel MUMPS static solve).

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod chrono;
pub mod config;
pub mod diag;
pub mod driver;
pub mod load_check;
pub mod memory;
pub mod output;

pub use config::{BlockedNode, CompatMode, Config, ModelChoice, PartitionChoice};
pub use driver::{RunReport, run};
