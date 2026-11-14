// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs differential test (no upstream counterpart): the mesh
// distributed by the port with the oracle's Scotch partition (prescribed,
// D-009) equals the oracle's distributed mesh rank by rank —
// fixtures/parallel/distribution_cube_msh/np{1,2,4,8}/rank*
// (oracle/probes/parallel; np8 recorded).

mod par_common;
mod pbd_common;

use std::time::Duration;

use akantu_comm::run_local;

#[test]
fn distribution_cube_msh_matches_the_oracle() {
    // 1, 2, 4 ranks, and 8 ranks
    let counts = par_common::oracle_rank_counts("distribution_cube_msh");
    assert_eq!(counts, vec![1, 2, 4, 8], "oracle fixtures present");
    for np in counts {
        let out = run_local(
            np,
            Duration::from_secs(60),
            par_common::distribution_matches_oracle,
        );
        for (r, o) in out.into_iter().enumerate() {
            if let Err(e) = o {
                panic!("np {np}, rank {r}: {e}");
            }
        }
    }
}

/// MESH-005: global and local bounding boxes
/// of the distributed mesh at 1/2/4/8 ranks against the serial box and the
/// oracle's coordinates.
#[test]
fn distributed_bounding_boxes_match_the_oracle() {
    for np in par_common::oracle_rank_counts("distribution_cube_msh") {
        let out = run_local(
            np,
            Duration::from_secs(60),
            par_common::bounding_boxes_match_oracle,
        );
        for (r, o) in out.into_iter().enumerate() {
            if let Err(e) = o {
                panic!("np {np}, rank {r}: {e}");
            }
        }
    }
}

/// UD-231: the adversarial 2-D mesh with an orphan
/// point equals the oracle's distribution in compat mode at 1/2/4 ranks.
#[test]
fn distribution_adv2d_matches_the_oracle_in_compat() {
    let counts = par_common::oracle_rank_counts("distribution_adv2d");
    assert_eq!(counts, vec![1, 2, 4], "oracle fixtures present");
    for np in counts {
        let out = run_local(
            np,
            Duration::from_secs(60),
            par_common::adv2d_matches_oracle_in_compat,
        );
        for (r, o) in out.into_iter().enumerate() {
            if let Err(e) = o {
                panic!("np {np}, rank {r}: {e}");
            }
        }
    }
}

/// Feature `scotch` (Docker): the port's Scotch partition equals the
/// oracle's for the same graph, seed and thread count.
#[cfg(feature = "scotch")]
#[test]
fn scotch_partition_matches_the_oracle() {
    if let Err(e) = par_common::scotch_partition_matches_oracle(akantu_comm::Communicator::serial())
    {
        panic!("{e}");
    }
}

/// PART-003; feature `scotch`, Docker:
/// the port's Scotch partition of the strip made periodic in x (dual graph
/// on the tweakConnectivity connectivity, upstream
/// `mesh_partition_scotch.cc:244`) equals the oracle's
/// `parallel/pbd_strip_scotch` partition at 2 and 4 parts, ghost parts
/// included.
#[cfg(feature = "scotch")]
#[test]
fn pbd_strip_scotch_partition_matches_the_oracle() {
    if let Err(e) =
        pbd_common::strip_scotch_partition_matches_oracle(akantu_comm::Communicator::serial())
    {
        panic!("{e}");
    }
}

/// Concurrent Scotch partitions are
/// serialised and reproducible.
#[cfg(feature = "scotch")]
#[test]
fn scotch_concurrent_partitions_are_reproducible() {
    if let Err(e) = par_common::scotch_concurrent_partitions_are_reproducible(
        akantu_comm::Communicator::serial(),
    ) {
        panic!("{e}");
    }
}

/// MUTIL-010, MESH-030:
/// `fixtures/parallel/distribution_square_data/np{1,2,4}` — physical
/// names, the recomputed element/facet adjacency and nodal data of every
/// rank after the distribution (parallel probe v2, x86_64 oracle).
#[test]
fn distribution_square_data_matches_the_oracle() {
    let counts = par_common::oracle_rank_counts("distribution_square_data");
    assert_eq!(counts, vec![1, 2, 4], "oracle fixtures present");
    for np in counts {
        let out = run_local(
            np,
            Duration::from_secs(60),
            par_common::square_data_matches_oracle,
        );
        for (r, o) in out.into_iter().enumerate() {
            if let Err(e) = o {
                panic!("np {np}, rank {r}: {e}");
            }
        }
    }
}

/// PART-001/PART-003 (feature `scotch`, Docker): upstream's
/// `Mesh::distribute(_edge_weight_function, _vertex_weight_function)` loads
/// reach Scotch through `ScotchPartitioner::with_loads`. Unit loads give
/// the default partition (upstream's defaults are 1); vertex loads change
/// the graph and the parts balance the load (Scotch's default strategy);
/// edge loads change the graph; reciprocal arcs with different loads are
/// rejected by `SCOTCH_graphCheck` (an error, no undefined behaviour).
#[cfg(feature = "scotch")]
#[test]
fn scotch_loads_reach_the_partitioner() {
    use akantu_core::{Element, GhostType, Int, TypeFilter};
    use akantu_mesh::partition::{MeshPartition, MeshPartitioner};
    use akantu_mesh::scotch::ScotchPartitioner;
    let m = par_common::read_on_root(
        &akantu_comm::Communicator::serial(),
        "test_synchronizer/cube.msh",
        3,
    )
    .unwrap();
    // x of every element's barycentre
    let x = m.nodes().as_slice().to_vec();
    let mut xb = std::collections::HashMap::new();
    for ty in m.element_types(TypeFilter::new().dim(3)) {
        let conn = m.connectivity(ty, GhostType::NotGhost).unwrap();
        let nn = ty.nb_nodes_per_element();
        for (e, el) in conn.as_slice().chunks_exact(nn).enumerate() {
            let s: f64 = el.iter().map(|&n| x[3 * n as usize]).sum();
            xb.insert(
                Element::try_new(ty, e, GhostType::NotGhost).unwrap(),
                s / nn as f64,
            );
        }
    }
    let (lo, hi) = xb
        .values()
        .fold((f64::MAX, f64::MIN), |(l, h), &v| (l.min(v), h.max(v)));
    let mid = 0.5 * (lo + hi);
    let heavy = |e: Element| -> Int { if xb[&e] < mid { 10 } else { 1 } };
    let part_loads = |p: &MeshPartition, load: &dyn Fn(Element) -> Int| -> Vec<i64> {
        let mut out = vec![0i64; p.nb_parts];
        for ty in m.element_types(TypeFilter::new().dim(3)) {
            let parts = p.partitions.get(ty, GhostType::NotGhost).unwrap();
            for (e, &q) in parts.as_slice().iter().enumerate() {
                let el = Element::try_new(ty, e, GhostType::NotGhost).unwrap();
                out[q as usize] += i64::from(load(el));
            }
        }
        out
    };
    let imbalance = |loads: &[i64]| -> f64 {
        let total: i64 = loads.iter().sum();
        *loads.iter().max().unwrap() as f64 * loads.len() as f64 / total as f64
    };
    let default = ScotchPartitioner::new(1234).partition(&m, 4).unwrap();
    let unit = ScotchPartitioner::new(1234)
        .with_loads(&|_, _| 1, &|_| 1)
        .partition(&m, 4)
        .unwrap();
    assert_eq!(
        unit.linearized(),
        default.linearized(),
        "unit loads = defaults"
    );
    assert_eq!(unit.metadata.graph_hash, default.metadata.graph_hash);
    let weighted = ScotchPartitioner::new(1234)
        .with_loads(&|_, _| 1, &heavy)
        .partition(&m, 4)
        .unwrap();
    assert_ne!(weighted.metadata.graph_hash, default.metadata.graph_hash);
    let (wi, di) = (
        imbalance(&part_loads(&weighted, &heavy)),
        imbalance(&part_loads(&default, &heavy)),
    );
    println!("INFO vertex-load imbalance (max/avg): weighted {wi:.4}, default partition {di:.4}");
    // measured on x86_64 (Scotch 7.0.7, seed 1234): weighted 1.0018, the
    // default partition 1.8234 of the same loads
    assert!(wi <= 1.1, "weighted partition imbalance {wi}");
    assert!(di > 1.1, "the default partition ignores the loads ({di})");
    // symmetric edge loads: another graph, a valid partition
    let edge =
        |a: Element, b: Element| -> Int { if xb[&a] < mid && xb[&b] < mid { 100 } else { 1 } };
    let cut = ScotchPartitioner::new(1234)
        .with_loads(&edge, &|_| 1)
        .partition(&m, 4)
        .unwrap();
    assert_ne!(cut.metadata.graph_hash, default.metadata.graph_hash);
    assert_eq!(cut.linearized().len(), default.linearized().len());
    // asymmetric arc loads: refused by SCOTCH_graphCheck
    let asym = |a: Element, b: Element| -> Int { if a < b { 1 } else { 2 } };
    let r = ScotchPartitioner::new(1234)
        .with_loads(&asym, &|_| 1)
        .partition(&m, 4);
    assert!(r.is_err(), "asymmetric arc loads must be an error");
}
