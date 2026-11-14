// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): test/test_synchronizer/{test_data_distribution.cc,
// test_node_synchronizer.cc, test_synchronizer_communication.cc} (the
// `test_synchronizers` gtest suite, PARALLEL) on the in-process local world
// at 1, 2 and 4 ranks; the same scenarios run on real MPI in
// `mpi_distribution.rs` (scripts/mpi/run-par-tests.sh).

mod par_common;

use std::time::Duration;

use akantu_comm::{Communicator, run_local};
use par_common::Outcome;

fn on_ranks(f: fn(Communicator) -> Outcome) {
    for n in [1, 2, 4] {
        let out = run_local(n, Duration::from_secs(60), f);
        for (r, o) in out.into_iter().enumerate() {
            if let Err(e) = o {
                panic!("{n} ranks, rank {r}: {e}");
            }
        }
    }
}

#[test]
fn test_data_distribution_data_distribution() {
    on_ranks(par_common::data_distribution);
}

#[test]
fn test_data_distribution_data_distribution_tags() {
    on_ranks(par_common::data_distribution_tags);
}

#[test]
fn test_node_synchronizer_synchrone() {
    on_ranks(par_common::node_synchronizer_synchronize);
}

#[test]
fn test_node_synchronizer_asynchrone() {
    on_ranks(par_common::node_synchronizer_asynchronous);
}

#[test]
fn test_node_synchronizer_gather() {
    on_ranks(par_common::node_synchronizer_gather);
}

#[test]
fn test_node_synchronizer_scatter() {
    on_ranks(par_common::node_synchronizer_scatter);
}

#[test]
fn test_synchronizer_communication_element() {
    on_ranks(par_common::element_synchronizer_communication);
}

#[test]
fn distribution_invariants_cube() {
    on_ranks(par_common::distribution_invariants);
}

#[test]
fn distribution_failure_agreed() {
    on_ranks(par_common::distribution_failure_agreed);
}

#[test]
fn periodic_distribution_invariants() {
    on_ranks(par_common::periodic_distribution_invariants);
}

/// UD-231: an orphan point gets its node's owner.
#[test]
fn pa02_orphan_point_has_one_owner() {
    on_ranks(par_common::adv2d_orphan_point_has_one_owner);
}

/// UD-231: the compat switch reproduces upstream.
#[test]
fn pa02_compat_reproduces_upstream_owners() {
    on_ranks(par_common::adv2d_compat_reproduces_upstream_owners);
}

/// Non-root group names are shared (upstream
/// `synchronizeGroupNames`).
#[test]
fn pa03_group_names_of_non_root_ranks_are_shared() {
    on_ranks(par_common::group_names_of_non_root_ranks_are_shared);
}

/// UD-235: distribution preconditions refused on every rank.
#[test]
fn ud235_distribution_preconditions_refused() {
    on_ranks(par_common::distribution_preconditions_refused);
}

/// MPI-016: element removals update the element synchronizer
/// (upstream `ElementSynchronizer::onElementsRemoved`), through the mesh's
/// `DistributedRemovalHook`.
#[test]
fn mpi016_element_removal_updates_the_element_synchronizer() {
    on_ranks(par_common::element_removal_updates_the_element_synchronizer);
}

/// MPI-016 / MPI-017: the same removal with the orphaned nodes purified in
/// the same mutation: element and node schemes follow.
#[test]
fn mpi016_element_removal_with_purify_updates_both_synchronizers() {
    on_ranks(par_common::element_removal_with_purify_updates_both_synchronizers);
}

/// MPI-016: recorded element changes substitute the scheme entries
/// (upstream `ElementSynchronizer::onElementsChanged`
/// step 3).
#[test]
fn mpi016_element_changes_update_the_element_synchronizer() {
    on_ranks(par_common::element_changes_update_the_element_synchronizer);
}

/// Agreed at local world 2 (1 and 4 too).
#[test]
fn lr14_05_element_change_renumbering_after_an_erasure_is_refused() {
    on_ranks(par_common::element_change_renumbering_after_an_erasure_is_refused);
}
