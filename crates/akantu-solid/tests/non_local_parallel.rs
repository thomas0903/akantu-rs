// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): the parallel part of
// test/test_model/test_common/test_non_local_toolbox/
// {test_build_neighborhood_parallel.cc, test_pair_computation.cc (level 2)};
// akantu-rs differential tests against oracle/probes/par4.

//! Non-local laws on distributed meshes in the local world (MPI-048…050,
//! TEST-079 / 080): the oracle replays at 1, 2 and 4 ranks (scenarios in
//! `par_nl`, shared with the real-MPI binary `mpi_smm`), and the corrected
//! default against the serial run; the cases
//! (multi-point elements, the UD-1100 compat outcome, two neighbourhoods
//! over a damaging history).

mod non_local_common;
#[path = "../../akantu-mesh/tests/par_common/mod.rs"]
mod par_common;
mod par_nl;

use std::time::Duration;

use akantu_comm::{Communicator, run_local};
use par_nl::Outcome;

fn on_ranks(ranks: &[usize], f: impl Fn(Communicator) -> Outcome + Send + Sync + Copy + 'static) {
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

fn oracle_ranks(scenario: &str, expected: &[usize]) -> Vec<usize> {
    let counts = par_common::oracle_rank_counts(scenario);
    assert_eq!(counts, expected, "oracle fixtures present for {scenario}");
    counts
}

#[test]
fn nl_region_matches_the_oracle() {
    on_ranks(
        &oracle_ranks("nl_region", &[1, 2, 4, 8]),
        par_nl::region_matches_oracle,
    );
}

#[test]
fn nl_pair_matches_the_oracle() {
    on_ranks(
        &oracle_ranks("nl_pair", &[1, 2]),
        par_nl::pair_matches_oracle,
    );
}

#[test]
fn nl_mazars_matches_the_oracle() {
    on_ranks(&oracle_ranks("nl_mazars", &[1, 2, 4]), |c| {
        par_nl::mazars_matches_oracle(c, "nl_mazars")
    });
}

#[test]
fn nl_mixed_matches_the_oracle() {
    on_ranks(&oracle_ranks("nl_mixed", &[1, 2, 4]), |c| {
        par_nl::mazars_matches_oracle(c, "nl_mixed")
    });
}

#[test]
fn nl_mazars_two_matches_the_oracle() {
    on_ranks(&oracle_ranks("nl_mazars_two", &[1]), |c| {
        par_nl::mazars_matches_oracle(c, "nl_mazars_two")
    });
}

/// Upstream's multi-pass compat erasure where upstream
/// completes (strips on 3 ranks: the second pass erases on one rank).
#[test]
fn nl_mazars_two_strips_matches_the_oracle() {
    on_ranks(&oracle_ranks("nl_mazars_two_strips", &[3]), |c| {
        par_nl::mazars_matches_oracle(c, "nl_mazars_two_strips")
    });
}

/// UD-1103: `damage_wf` under `WeightExchange::Upstream`
/// against the oracle, and the corrected exchange rejected by it.
#[test]
fn nl_damage_wf_matches_the_oracle() {
    on_ranks(
        &oracle_ranks("nl_damage_wf", &[1, 2, 4, 8]),
        par_nl::damage_wf_matches_oracle,
    );
}

#[test]
fn nl_region_default_vs_serial() {
    // 3 and 8 ranks: no oracle partition recorded, the bisection partitioner
    // (the path of the par tier at 8 processes)
    on_ranks(&[1, 2, 3, 4, 8], |c| {
        par_nl::region_default_vs_serial(c, 1e-14)
    });
}

/// TEST-079 (`test_build_neighborhood_parallel`), upstream's program.
#[test]
fn test_build_neighborhood_parallel() {
    on_ranks(&[1, 2, 4], par_nl::test_build_neighborhood_parallel);
}

/// TEST-080 (`test_pair_computation`), upstream's program and criteria.
#[test]
fn test_pair_computation() {
    on_ranks(&[1, 2, 4], par_nl::test_pair_computation);
}

/// MPI-050: the neighbourhood max criterion on a distributed mesh.
#[test]
fn max_criterion_distributed_vs_serial() {
    on_ranks(&[1, 2, 4], par_nl::max_criterion_vs_serial);
}

/// UD-1103: `damage_wf`'s ghost damage exchanged by default, not in compat.
#[test]
fn damage_wf_weight_exchange_ud1103() {
    on_ranks(&[1, 2, 4], par_nl::damage_wf_exchange);
}

/// UD-1101: the default cleanup keeps the distribution's ghost layer.
#[test]
fn mixed_mesh_cleanup_ud1101() {
    on_ranks(&[1, 2, 4], |c| par_nl::mixed_cleanup_ud1101(c, 1e-14));
}

/// Case `v140_c`: the corrected default on
/// quadrangles (four integration points per element) against the serial
/// run; it fails if the points of an element are unpacked in reverse order.
#[test]
fn multi_point_elements_default_vs_serial() {
    on_ranks(&[2, 4], par_nl::v140::multi_point_default_vs_serial);
}

/// The neighbourhood maximum on quadrangles (four points).
#[test]
fn max_criterion_multi_point_vs_serial() {
    on_ranks(&[1, 2, 4], par_nl::max_criterion_multi_point_vs_serial);
}

/// Case `v140_b`: upstream's sequence with two
/// neighbourhoods where upstream aborts (UD-1100): the same outcome on
/// every rank, an error naming UD-1100.
#[test]
fn ud1100_compat_outcome_is_agreed() {
    par_nl::v140::agreed_local("nl_mazars_two", 2);
    par_nl::v140::agreed_local("nl_mazars_two", 4);
    par_nl::v140::agreed_local("nl_pair", 2);
    par_nl::v140::agreed_local("nl_pair", 4);
}

/// Case `v140_a`: two neighbourhoods, corrected
/// default, a damaging history: damage within 1e-14, internal forces
/// within the existing 1e-12 rule.
#[test]
fn two_neighbourhoods_default_vs_serial() {
    on_ranks(&[2, 4], par_nl::v140::two_neighbourhoods_default_vs_serial);
}

/// D-097 (1): `SummationOrder::KeyOrder` refused at
/// `init_full` on a distributed mesh with the recoverable
/// `Error::Unsupported`, agreed on every rank, the mesh unchanged, and the
/// same model initialised by `init_materials` once the route is changed;
/// accepted at one rank on a mesh that is not
/// distributed.
#[test]
fn key_order_is_refused_on_a_distributed_mesh() {
    on_ranks(
        &[1, 2, 4, 8],
        par_nl::key_order_refused_on_a_distributed_mesh,
    );
}

/// Rank-specific failures of the new collectives are agreed on every rank.
#[test]
fn rank_specific_failures_are_agreed() {
    on_ranks(&[2, 4], par_nl::nl_init_failure_is_agreed);
    on_ranks(&[2, 4], par_nl::nl_cycle_failure_is_agreed);
    on_ranks(&[2, 4], par_nl::max_criterion_failure_is_agreed);
}
