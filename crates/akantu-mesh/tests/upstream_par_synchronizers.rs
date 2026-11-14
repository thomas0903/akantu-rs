// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): test/test_synchronizer/{test_facet_synchronizer.cc,
// test_grid_synchronizer.cc, test_grid_synchronizer_check_neighbors.cc},
// test/test_mesh_utils/test_mesh_periodic/test_mesh_periodic.cc (PARALLEL) on
// the in-process local world at 1, 2 and 4 ranks; the same scenarios run on
// real MPI in `mpi_synchronizers.rs` (scripts/mpi/run-par-tests.sh).

//! Distributed facets and the facet, grid and periodic node
//! synchronizers (upstream tests and independent invariants).

mod par2a_common;
mod par_common;

use std::time::Duration;

use akantu_comm::{Communicator, run_local};
use par_common::Outcome;

fn on_ranks(f: fn(Communicator) -> Outcome) {
    for n in [1, 2, 4] {
        let out = run_local(n, Duration::from_secs(60), f);
        let errors: Vec<String> = out
            .into_iter()
            .enumerate()
            .filter_map(|(r, o)| o.err().map(|e| format!("rank {r}: {e}")))
            .collect();
        assert!(errors.is_empty(), "{n} ranks:\n{}", errors.join("\n"));
    }
}

/// TEST-170: `TestFacetSynchronizerFixture.{SynchroneOnce, Synchrone,
/// Asynchrone}`.
#[test]
fn test_facet_synchronizer_synchrone_asynchrone() {
    on_ranks(par2a_common::test_facet_synchronizer);
}

/// PART-012 / MPI-020 invariants: one owner per facet, minimum-rank ghost
/// owners, symmetric schemes, global connectivities equal to the owner's.
#[test]
fn distributed_facets_invariants() {
    on_ranks(par2a_common::facet_invariants);
}

/// ARCHITECTURE.md, "Agreement across ranks": a rank-specific failure while building the facet mesh is agreed.
#[test]
fn distributed_facets_failure_agreed() {
    on_ranks(par2a_common::facet_failure_agreed);
}

/// A prepared distributed facet mesh installed by a mesh transaction.
#[test]
fn distributed_facets_transaction_install() {
    on_ranks(par2a_common::facet_transaction_install);
}

/// TEST-230: `test_grid_synchronizer.cc` + `..._check_neighbors.cc`
/// (same intent): parallel pair list = serial pair list, ghost data.
#[test]
fn test_grid_synchronizer_check_neighbors() {
    on_ranks(par2a_common::test_grid_synchronizer);
}

/// The grid synchronizer applied by a mesh mutation equals the set-up path.
#[test]
fn grid_synchronizer_through_mutation() {
    on_ranks(par2a_common::grid_synchronizer_through_mutation);
}

/// ARCHITECTURE.md, "Agreement across ranks" / UD-253: rank-specific grid synchronizer failures are agreed.
#[test]
fn grid_synchronizer_failure_agreed() {
    on_ranks(par2a_common::grid_failure_agreed);
}

/// Failing `GridCells::cell_index` /
/// `elements_in_cell_range` on one rank is an agreed error, meshes
/// unchanged.
#[test]
fn grid_cells_failure_agreed() {
    on_ranks(par2a_common::grid_cells_failure_agreed);
}

/// MESH-028/029 + MPI-019: `test_mesh_periodic.cc` (6 types) on the
/// distributed mesh.
#[test]
fn test_mesh_periodic_distributed() {
    on_ranks(par2a_common::test_mesh_periodic);
}

/// ARCHITECTURE.md, "Agreement across ranks": a rank-specific failure of the distributed `makePeriodic` is
/// agreed.
#[test]
fn periodic_failure_agreed() {
    on_ranks(par2a_common::periodic_failure_agreed);
}

/// UD-251: different local facet types on neighbouring ranks (upstream
/// deadlocks) give consistent facet schemes.
#[test]
fn ud251_mixed_facet_types() {
    on_ranks(par2a_common::ud251_mixed_facet_types);
}

/// UD-254: periodic mesh distributed after `makePeriodic`.
#[test]
fn ud254_periodic_after_distribution() {
    on_ranks(par2a_common::ud254_periodic_after_distribution);
}

/// The grid synchronizer keeps the facet mesh consistent (shared nodes,
/// adjacency rows of the new ghost elements).
#[test]
fn grid_synchronizer_keeps_facet_mesh_consistent() {
    on_ranks(par2a_common::grid_synchronizer_keeps_facet_mesh_consistent);
}

/// A partition with an empty part (port
/// extension; upstream refuses it by a debug assertion).
#[test]
fn ud813_empty_part_completes() {
    on_ranks(par2a_common::ud813_empty_part);
    // 8 ranks: five empty parts
    let out = run_local(8, Duration::from_secs(60), par2a_common::ud813_empty_part);
    let errors: Vec<String> = out
        .into_iter()
        .enumerate()
        .filter_map(|(r, o)| o.err().map(|e| format!("rank {r}: {e}")))
        .collect();
    assert!(errors.is_empty(), "8 ranks:\n{}", errors.join("\n"));
}
