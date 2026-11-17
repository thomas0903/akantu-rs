// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Benchmark binary of
// the model suite (no upstream counterpart).

//! `suite_models`: timing cases of heat transfer, phase field, contact,
//! structural mechanics, embedded interfaces, IGFEM, node-to-node contact
//! and geometry queries (`crates/akantu-bench/README.md`, section "Model
//! suite").
//!
//! ```text
//! suite_models --list
//! suite_models --case <name>[,<name>...]|all --size s|m|l [--reps N] [--out FILE]
//!              [--loop-seconds S] [--threads N] [--grain finest|auto]
//! ```

#[path = "../suite_models/mod.rs"]
mod suite_models;

use std::process::ExitCode;

fn main() -> ExitCode {
    let cases = suite_models::cases();
    suite_models::harness::main(&cases, suite_models::common::world)
}
