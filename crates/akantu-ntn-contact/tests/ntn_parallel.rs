// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs differential tests (no upstream counterpart: no upstream
// test runs the split-node contact in parallel; the probe calls the public
// NTNBaseContact::initParallel(), D-083): MPI-051 / MPI-052.

//! Split-node contact on distributed meshes in the local world: the oracle
//! replays at 1, 2 and 4 ranks (scenarios in `par_ntn`, shared with the
//! real-MPI binary `mpi_ntn`), and the cases
//! normals, per-rank pairing refusal and agreed sums.

#[path = "../../akantu-mesh/tests/par_common/mod.rs"]
mod par_common;
mod par_ntn;

use std::time::Duration;

use akantu_comm::{Communicator, run_local};
use par_ntn::Outcome;

fn on_oracle_ranks(scenario: &str, expected: &[usize], f: fn(Communicator) -> Outcome) {
    let counts = par_common::oracle_rank_counts(scenario);
    assert_eq!(counts, expected, "oracle fixtures present for {scenario}");
    for np in counts {
        for (r, o) in run_local(np, Duration::from_secs(300), f)
            .into_iter()
            .enumerate()
        {
            if let Err(e) = o {
                panic!("{scenario} np {np} rank {r}: {e}");
            }
        }
    }
}

#[test]
fn ntn_compression_matches_the_oracle() {
    on_oracle_ranks(
        "ntn_compression",
        &[1, 2, 4, 8],
        par_ntn::ntn_compression_matches_oracle,
    );
}

#[test]
fn ntrf_coulomb_matches_the_oracle() {
    on_oracle_ranks(
        "ntrf_coulomb",
        &[1, 2, 4, 8],
        par_ntn::ntrf_coulomb_matches_oracle,
    );
}

#[test]
fn distributed_contact_without_init_parallel_is_refused() {
    for np in [1, 2, 4] {
        for (r, o) in run_local(
            np,
            Duration::from_secs(120),
            par_ntn::refused_without_init_parallel,
        )
        .into_iter()
        .enumerate()
        {
            if let Err(e) = o {
                panic!("np {np} rank {r}: {e}");
            }
        }
    }
}

#[test]
fn rank_specific_contact_step_failure_is_agreed() {
    for np in [2, 4] {
        for (r, o) in run_local(
            np,
            Duration::from_secs(120),
            par_ntn::contact_step_failure_is_agreed,
        )
        .into_iter()
        .enumerate()
        {
            if let Err(e) = o {
                panic!("np {np} rank {r}: {e}");
            }
        }
    }
}

fn on_ranks(ranks: &[usize], f: fn(Communicator) -> Outcome) {
    for &np in ranks {
        for (r, o) in run_local(np, Duration::from_secs(300), f)
            .into_iter()
            .enumerate()
        {
            if let Err(e) = o {
                panic!("np {np} rank {r}: {e}");
            }
        }
    }
}

/// Case `v140_d`: `update_normals_of` exchanges
/// `_cf_nodal` first, as upstream's `updateNormals`.
#[test]
fn update_normals_after_init_parallel() {
    on_oracle_ranks(
        "ntn_compression",
        &[1, 2, 4, 8],
        par_ntn::update_normals_after_init_parallel,
    );
}

/// The oracle's `updateNormals()` after `initParallel()` and
/// 40 steps (probe `ntn_update_normals`), bitwise.
#[test]
fn ntn_update_normals_matches_the_oracle() {
    on_oracle_ranks(
        "ntn_update_normals",
        &[1, 2, 4, 8],
        par_ntn::ntn_update_normals_matches_oracle,
    );
}

/// UD-1105: a partition separating the interface is refused
/// at `init_parallel` on every rank (2 and 4 ranks; nothing to refuse on 1).
#[test]
fn separated_partition_is_refused() {
    on_ranks(&[1, 2, 4], par_ntn::separated_partition_is_refused);
}

/// A rank-local error before a contact sum is agreed.
#[test]
fn contact_sums_failure_is_agreed() {
    on_ranks(&[2, 4], par_ntn::contact_sums_failure_is_agreed);
}

/// Every addition of contact nodes after
/// `init_parallel` is refused on more than one rank (agreed, recoverable);
/// on one rank the additions run.
#[test]
fn additions_after_init_parallel_are_refused() {
    // the fixture is recorded at 8 ranks too
    on_oracle_ranks(
        "ntn_compression",
        &[1, 2, 4, 8],
        par_ntn::additions_after_init_parallel_are_refused,
    );
}
