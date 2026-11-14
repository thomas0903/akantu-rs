// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): differential test of src/mesh/mesh_periodic.cc
// (makePeriodic, makeConsistentForPeriodicity) and
// src/synchronizer/periodic_node_synchronizer_tmpl.hh (local part) against
// the oracle cases `fixtures/mesh_utils/periodic_order`.

//! UD-654 regression for the mesh-level periodic reduction:
//! `akantu_mesh::periodic::make_consistent_for_periodicity_real`
//! with `Sum` must reproduce upstream's bits on 67 adversarial cases (2,640
//! masters, 374 with 2–7 slaves), which needs the slaves of every master
//! accumulated in reverse insertion order; summing them in insertion order
//! or in sorted order (the former rules) changes 1,624 resp. 1,164 values.

mod periodic_order_common;

use akantu_core::enums::ReduceOperation;
use akantu_mesh::periodic::make_consistent_for_periodicity_real;

#[test]
fn periodic_order_matches_the_oracle() {
    let r = periodic_order_common::replay(&|mesh, a| {
        make_consistent_for_periodicity_real(mesh, a, ReduceOperation::Sum).unwrap();
    });
    periodic_order_common::assert_replay(&r, "akantu_mesh::periodic");
}
