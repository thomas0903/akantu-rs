// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). The model suite of the
// performance campaign (no upstream counterpart).

//! Benchmark suite of the models other than plain solid mechanics and
//! cohesive elements (`docs/briefs/perf/SUITE.md`): heat transfer, phase
//! field, contact, structural mechanics, embedded interfaces, IGFEM,
//! node-to-node contact and the geometry queries. Every case builds its
//! mesh in code, times its phases and hashes its results.

pub mod common;
pub mod harness;
pub mod contact;
pub mod embedded;
pub mod heat;
pub mod mesh;
pub mod ntn;
pub mod phase_field;
pub mod structural;

use harness::Case;

/// Every case of the suite, in `--list` order.
#[must_use]
pub fn cases() -> Vec<Case> {
    let mut v = Vec::new();
    for group in [heat::CASES, phase_field::CASES, contact::CASES, structural::CASES, embedded::CASES, ntn::CASES] {
        for c in group {
            v.push(Case {
                name: c.name,
                description: c.description,
                run: c.run,
            });
        }
    }
    v
}
