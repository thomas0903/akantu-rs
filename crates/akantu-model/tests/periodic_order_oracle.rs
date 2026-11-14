// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): differential test of
// src/mesh/mesh_periodic.cc (makeConsistentForPeriodicity) as used by the
// models, against the oracle cases `fixtures/mesh_utils/periodic_order`.

//! UD-654 regression for the model-level periodic reduction
//! (`akantu_model::model::make_consistent_for_periodicity`, used by the
//! boundary conditions and the solid model): same cases and assertions as
//! `akantu-mesh/tests/periodic_order_oracle.rs`.

#[path = "../../akantu-mesh/tests/periodic_order_common/mod.rs"]
mod periodic_order_common;

use akantu_model::model::{ReduceOperation, make_consistent_for_periodicity};

#[test]
fn periodic_order_matches_the_oracle() {
    let r = periodic_order_common::replay(&|mesh, a| {
        make_consistent_for_periodicity(mesh, a, ReduceOperation::Sum, |x, y| x + y).unwrap();
    });
    periodic_order_common::assert_replay(&r, "akantu_model::model");
}
