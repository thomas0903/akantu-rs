// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Stack use of the
// cohesive-contact coupler's constructor (coupler_solid_cohesive_contact.cc
// constructor; solid_mechanics_model_cohesive.cc:122-158), case
// `v131_cohesive_contact_constructor_stack`.

mod common;

use akantu_couplers::CouplerSolidCohesiveContact;
use akantu_mesh::SharedMesh;
use akantu_parser::Parser;
use common::*;

/// The unoptimised constructors copy the cohesive model
/// by value about 27 times, so its inline size is stack. The branch
/// held the 4.3 kB staged insertion-flag map inline (the cohesive model
/// 63 kB → 67.5 kB) and `CouplerSolidCohesiveContact::with_parser` alone
/// overflowed a 1.75 MiB thread in a debug build (arm64), where the
/// package's base constructed it; with the map boxed it constructs again.
/// Measured (arm64, debug): the constructor needed 1.625–1.75
/// MiB, with `akantu-core/export-all` (its larger shared arrays)
/// 1.8125–1.875 MiB. With the cohesive model's facet FE engine boxed
/// (the model 63,528 → 42,616 B) it needs
/// 1.4375–1.5 MiB, with export-all 1.5625–1.625 MiB (64 KiB steps; every
/// threshold 256 KiB lower). The thread: 1.5 MiB, 1.75 MiB with export-all
/// (the same headroom as before the box); `AKANTU_STACK_TEST_BYTES`
/// overrides it for measurements. An inline field of the map's size added
/// to the model aborts this test (stack overflow).
fn stack_bytes() -> usize {
    std::env::var("AKANTU_STACK_TEST_BYTES")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(if akantu_core::array_shared::export_all::ENABLED {
            7 << 18
        } else {
            3 << 19
        })
}

#[test]
fn cohesive_contact_coupler_constructs_on_a_small_thread() {
    std::thread::Builder::new()
        .stack_size(stack_bytes())
        .spawn(|| {
            let mesh = read_mesh(
                &frozen_mesh("examples/c++/contact_mechanics_model/hertz/cohesive-contact.msh"),
                2,
            );
            let parser = Parser::parse_str(
                include_str!("data/contact/material-cohesive.dat"),
                "material-cohesive.dat",
            )
            .unwrap();
            let c = CouplerSolidCohesiveContact::with_parser(
                SharedMesh::new(mesh),
                None,
                "coupler_solid_contact",
                parser,
            )
            .unwrap();
            std::hint::black_box(&c);
        })
        .unwrap()
        .join()
        .unwrap();
}
