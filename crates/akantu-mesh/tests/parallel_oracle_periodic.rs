// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs differential tests (no upstream counterpart):
// a mesh made periodic before it is distributed (PART-002 /
// 003 / 005 / 008) and the distribution of a mesh with sparse Gmsh tags,
// several element types and several groups (INT-005)
// against fixtures/parallel/{pbd_strip_scotch, pbd_strip_mesh_data,
// distribution_sparse_mixed}/np{1,2,4} (oracle/probes/parallel, probe
// version 6, x86_64 oracle with the D-050 pin).

mod par_common;
mod pbd_common;

use std::time::Duration;

use akantu_comm::run_local;
use par_common::Outcome;

fn at_1_2_4(scenario: &str, f: fn(akantu_comm::Communicator) -> Outcome) {
    let counts = par_common::oracle_rank_counts(scenario);
    assert_eq!(
        counts,
        vec![1, 2, 4],
        "oracle fixtures of {scenario} present"
    );
    for np in counts {
        let out = run_local(np, Duration::from_secs(60), f);
        for (r, o) in out.into_iter().enumerate() {
            if let Err(e) = o {
                panic!("{scenario} np {np}, rank {r}: {e}");
            }
        }
    }
}

/// PART-002 / 003 / 008: the strip made periodic in x on the root,
/// partitioned by the oracle's seeded Scotch (periodic branch) and
/// distributed: rank 0's partition and ghost parts, every rank's mesh,
/// synchronizers, groups, adjacency and periodic pairs equal the oracle.
#[test]
fn pbd_strip_scotch_matches_the_oracle() {
    at_1_2_4(
        pbd_common::STRIP_SCOTCH,
        pbd_common::strip_scotch_matches_oracle,
    );
}

/// PART-002 / 005 / 008: the same strip partitioned by the probe's
/// rule through `MeshDataPartition` (upstream `MeshPartitionMeshData`'s
/// periodic tweak); at 4 ranks rank 3 receives missing periodic partners
/// (upstream `receiveMissingPeriodic`).
#[test]
fn pbd_strip_mesh_data_matches_the_oracle() {
    at_1_2_4(
        pbd_common::STRIP_MESH_DATA,
        pbd_common::strip_mesh_data_matches_oracle,
    );
}

/// INT-005: `sparse2d.msh` (sparse, shuffled node and element
/// tags; Quad4, Tri3, Seg2, Point1; ten physical groups) with an Int nodal
/// datum, distributed with the oracle's Scotch partition, equals the
/// oracle rank by rank.
#[test]
fn distribution_sparse_mixed_matches_the_oracle() {
    at_1_2_4(
        pbd_common::SPARSE_MIXED,
        pbd_common::sparse_mixed_matches_oracle,
    );
}

/// UD-1061: the missing periodic partners appended by the distribution
/// carry the root's nodal data (upstream reads past its buffer); local
/// world 1/2/4, no fixture.
#[test]
fn ud1061_missing_periodic_nodes_carry_nodal_data() {
    for np in [1, 2, 4] {
        let out = run_local(
            np,
            Duration::from_secs(60),
            pbd_common::ud1061_missing_periodic_nodes_carry_nodal_data,
        );
        for (r, o) in out.into_iter().enumerate() {
            if let Err(e) = o {
                panic!("np {np}, rank {r}: {e}");
            }
        }
    }
}

/// The formula
/// `par_common::ud1060_relocated_ranks` (the entries of upstream's per-rank
/// vector that a later `emplace_back` relocates) against the relocations
/// the probe observed on the oracle's toolchain (`parallel/
/// ud1060_vector_growth/np1/rank0`, probe version 7: the same
/// `emplace_back` sequence on a vector of a type with a `noexcept` move,
/// D-030 black-box observation) for 1 to 16 ranks: the same entries move.
/// The record also shows UD-1060 (b): from 3 ranks on entry 0 is relocated
/// more than once (each later relocation frees a buffer still in use).
#[test]
fn ud1060_formula_matches_the_observed_relocations() {
    let f = akantu_validation::Fixture::open_by_id("parallel/ud1060_vector_growth/np1/rank0")
        .expect("the oracle case parallel/ud1060_vector_growth");
    f.require_golden().unwrap();
    f.verify_checksums().unwrap();
    for n in 1..=16usize {
        let observed = f
            .load_i64(&format!("observed/relocations_n{n}"))
            .unwrap()
            .data;
        assert_eq!(observed.len(), n);
        let moved: Vec<usize> = (0..n).filter(|&p| observed[p] > 0).collect();
        assert_eq!(
            moved,
            par_common::ud1060_relocated_ranks(n),
            "{n} ranks: observed relocations {observed:?}"
        );
        if n >= 3 {
            assert!(
                observed[0] >= 2,
                "{n} ranks: entry 0 relocated {} time(s)",
                observed[0]
            );
        }
    }
}
