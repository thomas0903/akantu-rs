// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs differential scenarios (no upstream
// counterpart; they replay the oracle probe oracle/probes/parallel, probe
// version 6, which drives upstream's src/mesh/mesh.cc (Mesh::distribute),
// src/mesh_utils/mesh_partition.cc (tweakConnectivity, fillPartitionInformation),
// src/mesh_utils/mesh_partition/{mesh_partition_scotch.cc,
// mesh_partition_mesh_data.cc} and src/synchronizer/node_info_per_processor.cc).

//! A mesh made periodic **before** it is distributed
//! (PART-002 / 003 / 005 / 008) and the distribution of a mesh with sparse
//! Gmsh tags, several element types and several groups
//! (INT-005), compared with `fixtures/parallel/{pbd_strip_scotch,
//! pbd_strip_mesh_data, distribution_sparse_mixed}/np{1,2,4}`. Shared by
//! the local-world tests (`parallel_oracle_periodic.rs`) and the real-MPI
//! binary (`mpi_distribution.rs`). Uses `par_common` (declared by the
//! including binary).
//!
//! Upstream's distribution of a periodic mesh corrupts its heap when a rank
//! whose entry of `synchronizePeriodicity`'s `std::vector<Array<Idx>>` is
//! relocated holds a periodic slave (UD-655, root cause UD-1060): the
//! oracle cases record that precondition, and the strip partitions are
//! chosen so that it does not hold (ranks 0 at 2 ranks, 0 and 1 at 4 ranks
//! hold no slave). The port reproduces the precondition from its own
//! partition and distributes every partition with defined behaviour.

#![allow(dead_code)]
// each binary uses a subset
#![allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
// test indices are small

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use akantu_comm::Communicator;
use akantu_core::enums::{MeshIoType, SpatialDirection};
use akantu_core::{Element, ElementTypeMapArray, GhostType, Idx, Real, TypeFilter};
use akantu_io::{ReadOptions, read_mesh_with};
use akantu_mesh::distribution::{MeshDataPartition, MeshPartitioner, PrescribedPartition};
use akantu_mesh::partition::MeshPartition;
use akantu_mesh::periodic::{make_periodic, periodic_pairs};
use akantu_mesh::{Mesh, NodalDataValues};

use crate::ensure;
use crate::par_common::{
    Outcome, compare_mesh_data_g20, compare_with_oracle, distribute_prescribed, distribute_rcb, ok,
    oracle_partition,
};
// the UD-1060 precondition helpers live in par_common
// (its periodic invariants test uses them too)
pub use crate::par_common::{slave_holders, ud1060_relocated_ranks};

pub const STRIP_SCOTCH: &str = "pbd_strip_scotch";
pub const STRIP_MESH_DATA: &str = "pbd_strip_mesh_data";
pub const SPARSE_MIXED: &str = "distribution_sparse_mixed";

/// A mesh of `oracle/probes/parallel` read on rank 0 (empty elsewhere).
pub fn probe_mesh_on_root(
    comm: &Communicator,
    file: &str,
    dim: usize,
) -> std::result::Result<Mesh, String> {
    let mut m = ok(Mesh::with_communicator(dim, "mesh", comm.clone()), "mesh")?;
    if comm.rank() == 0 {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../oracle/probes/parallel")
            .join(file);
        let opts = ReadOptions {
            fill_unnamed_physical_groups: false,
            write_diana_material_file: false,
        };
        ok(
            read_mesh_with(&mut m, &path, MeshIoType::Auto, &opts),
            "read",
        )?;
    }
    Ok(m)
}

/// The probe's `read_pstrip_periodic_on_root`: `pstrip.msh` on rank 0,
/// made periodic in x there (before the distribution).
pub fn pstrip_periodic_on_root(comm: &Communicator) -> std::result::Result<Mesh, String> {
    let mut m = probe_mesh_on_root(comm, "pstrip.msh", 2)?;
    if comm.rank() == 0 {
        let _ = ok(
            make_periodic(&mut m, SpatialDirection::X, None),
            "make_periodic",
        )?;
    }
    Ok(m)
}

/// The probe's `pbd_strip_part` (barycentre on [0, 24] x [0, 4]).
pub fn pbd_strip_part(x: Real, y: Real, np: usize) -> Idx {
    if np == 2 {
        return if x > 6.0 && x < 18.0 { 0 } else { 1 };
    }
    if x < 6.0 {
        if y > 2.0 { 3 } else { 2 }
    } else if x < 12.0 {
        0
    } else if x < 18.0 {
        1
    } else if x < 23.0 {
        3
    } else {
        2
    }
}

/// The probe's MeshPartitionMeshData mapping of `pbd_strip_mesh_data`.
pub fn pbd_strip_mapping(
    m: &Mesh,
    np: usize,
) -> std::result::Result<ElementTypeMapArray<Idx>, String> {
    let mut mapping = ElementTypeMapArray::new("pbd_partition");
    for ty in m.element_types(TypeFilter::new().dim(2)) {
        let n = m.nb_element(ty, GhostType::NotGhost);
        let a = ok(mapping.alloc(n, 1, ty, GhostType::NotGhost, 0), "alloc")?;
        for e in 0..n {
            let el = ok(Element::try_new(ty, e, GhostType::NotGhost), "element")?;
            let b = ok(m.barycenter(el), "barycenter")?;
            a.as_mut_slice()[e] = pbd_strip_part(b[0], b[1], np);
        }
    }
    Ok(mapping)
}

/// Rank 0: the port's partition of the periodic root mesh against the
/// oracle's (linearised parts, parts and ghost parts of every type) and
/// the UD-1060 precondition computed from it.
pub fn compare_partition_with_oracle(
    m: &Mesh,
    partition: &MeshPartition,
    fx: &akantu_validation::Fixture,
) -> Outcome {
    let p = &fx.manifest().parameters;
    let li = |name: &str| {
        fx.load_i64(name)
            .map(|f| f.data)
            .map_err(|e| format!("{name}: {e}"))
    };
    let strs = |key: &str| -> Vec<String> {
        p[key]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default()
    };
    let lin: Vec<i64> = partition
        .linearized()
        .iter()
        .map(|&v| i64::from(v))
        .collect();
    ensure!(
        lin == li("partition/linearized")?,
        "linearised partition differs"
    );
    let mut types = Vec::new();
    for ty in m.element_types(TypeFilter::new()) {
        if !partition.has_partitions(ty) {
            continue;
        }
        types.push(ty.name().to_owned());
        let parts: Vec<i64> = ok(partition.partition(ty), "parts")?
            .iter()
            .map(|&v| i64::from(v))
            .collect();
        ensure!(
            parts == li(&format!("partition/parts/{}", ty.name()))?,
            "parts of {ty} differ"
        );
        let g = ok(partition.ghost_partition(ty), "ghost parts")?;
        let counts: Vec<i64> = (0..g.nb_rows()).map(|r| g.row(r).len() as i64).collect();
        let values: Vec<i64> = (0..g.nb_rows())
            .flat_map(|r| g.row(r).iter().map(|&v| i64::from(v)))
            .collect();
        ensure!(
            counts == li(&format!("partition/ghost_counts/{}", ty.name()))?,
            "ghost part counts of {ty} differ: {counts:?}"
        );
        ensure!(
            values == li(&format!("partition/ghost_parts/{}", ty.name()))?,
            "ghost parts of {ty} differ: {values:?}"
        );
    }
    ensure!(
        types == strs("partition/types") && types == strs("partition/ghost_types"),
        "partitioned types {types:?} vs oracle {:?} / {:?}",
        strs("partition/types"),
        strs("partition/ghost_types")
    );
    let held = slave_holders(m, partition)?;
    let have: Vec<i64> = held
        .iter()
        .enumerate()
        .flat_map(|(r, &c)| [r as i64, c as i64])
        .collect();
    ensure!(
        have == li("ud1060/slave_holders")?,
        "slave holders {have:?} differ from the oracle's"
    );
    let safe = ud1060_relocated_ranks(partition.nb_parts)
        .iter()
        .all(|&r| held[r] == 0);
    ensure!(
        p["ud1060/upstream_memory_safe"].as_bool() == Some(safe),
        "UD-1060 precondition: port {safe} vs oracle {}",
        p["ud1060/upstream_memory_safe"]
    );
    Ok(())
}

/// The periodic state of a distributed mesh against the oracle's
/// (`record_periodic_state`): flag, `(slave, master)` pairs in local and
/// global ids, `(master, slave)` sorted, and each master's slaves in the
/// order of upstream's multimap (UD-654).
pub fn compare_periodic_state(m: &Mesh, fx: &akantu_validation::Fixture) -> Outcome {
    let p = &fx.manifest().parameters;
    let li = |name: &str| {
        fx.load_i64(name)
            .map(|f| f.data)
            .map_err(|e| format!("{name}: {e}"))
    };
    ensure!(
        p["is_periodic"].as_bool() == Some(m.is_periodic()),
        "is_periodic {} vs oracle {}",
        m.is_periodic(),
        p["is_periodic"]
    );
    let mut sm = Vec::new();
    let mut gsm = Vec::new();
    for n in 0..m.nb_nodes() as Idx {
        if m.node_flag(n).is_periodic_slave() {
            let master = m
                .periodic_master(n)
                .ok_or(format!("slave {n} without master"))?;
            sm.extend([i64::from(n), i64::from(master)]);
            gsm.extend([m.node_global_id(n), m.node_global_id(master)]);
        }
    }
    ensure!(
        sm == li("periodic/slave_master")?,
        "(slave, master) pairs differ: {sm:?}"
    );
    ensure!(
        gsm == li("periodic/global_slave_master")?,
        "global (slave, master) pairs differ: {gsm:?}"
    );
    let pairs: Vec<i64> = periodic_pairs(m)
        .into_iter()
        .flat_map(|(a, b)| [i64::from(a), i64::from(b)])
        .collect();
    ensure!(
        pairs == li("periodic/pairs")?,
        "(master, slave) pairs differ: {pairs:?}"
    );
    let raw = li("periodic/multimap_order")?;
    let mut groups: BTreeMap<i64, Vec<i64>> = BTreeMap::new();
    let mut seen_order: Vec<i64> = Vec::new();
    for pair in raw.as_chunks::<2>().0 {
        if seen_order.last() != Some(&pair[0]) {
            ensure!(
                !groups.contains_key(&pair[0]),
                "oracle multimap order: master {} not contiguous",
                pair[0]
            );
            seen_order.push(pair[0]);
        }
        groups.entry(pair[0]).or_default().push(pair[1]);
    }
    let masters: BTreeSet<i64> = periodic_pairs(m)
        .into_iter()
        .map(|(ms, _)| i64::from(ms))
        .collect();
    ensure!(
        masters == groups.keys().copied().collect::<BTreeSet<_>>(),
        "masters {masters:?} vs oracle {:?}",
        groups.keys()
    );
    for (master, slaves) in &groups {
        let have: Vec<i64> = m
            .periodic_slaves_upstream_order(*master as Idx)
            .map(i64::from)
            .collect();
        ensure!(
            &have == slaves,
            "slaves of master {master} in upstream order {have:?} vs oracle {slaves:?}"
        );
    }
    Ok(())
}

/// `[local lower, local upper, global lower, global upper]` against the
/// oracle's `name` (bitwise).
pub fn compare_bbox(m: &Mesh, fx: &akantu_validation::Fixture, name: &str) -> Outcome {
    let mut have = m.local_bbox().lower.clone();
    have.extend_from_slice(&m.local_bbox().upper);
    have.extend_from_slice(m.lower_bounds());
    have.extend_from_slice(m.upper_bounds());
    let want = fx
        .load_f64(name)
        .map(|f| f.data)
        .map_err(|e| format!("{name}: {e}"))?;
    ensure!(
        have.len() == want.len()
            && have
                .iter()
                .zip(&want)
                .all(|(a, b)| a.to_bits() == b.to_bits()),
        "{name}: port {have:?} vs oracle {want:?}"
    );
    Ok(())
}

fn open_case(
    scenario: &str,
    comm: &Communicator,
) -> std::result::Result<akantu_validation::Fixture, String> {
    akantu_validation::Fixture::open_by_id(&format!(
        "parallel/{scenario}/np{}/rank{}",
        comm.size(),
        comm.rank()
    ))
    .map_err(|e| e.to_string())
}

/// Which partitioner reproduces the oracle's partition of a strip scenario
/// on rank 0.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StripPartition {
    /// The oracle's linearised parts (`PrescribedPartition`).
    OracleParts,
    /// The probe's rule through `MeshDataPartition` (upstream
    /// `MeshPartitionMeshData`).
    MeshDataRule,
}

/// A strip scenario: the periodic root mesh partitioned like the oracle,
/// the partition compared on rank 0 (with the UD-1060 precondition), then
/// distributed with it and every rank compared with the oracle: mesh,
/// synchronizers and groups, physical names and recomputed adjacency,
/// periodic state, bounding boxes.
pub fn strip_matches_oracle(comm: &Communicator, scenario: &str, how: StripPartition) -> Outcome {
    let np = comm.size();
    let mut m = pstrip_periodic_on_root(comm)?;
    let fx = open_case(scenario, comm)?;
    let p = &fx.manifest().parameters;
    if comm.rank() == 0 {
        ensure!(
            p["root_periodic_pairs_before"].as_u64() == Some(periodic_pairs(&m).len() as u64),
            "root pairs before the distribution: {} vs oracle {}",
            periodic_pairs(&m).len(),
            p["root_periodic_pairs_before"]
        );
    }
    ensure!(
        p["distributed"].as_bool() == Some(true),
        "{scenario} np {np}: the oracle did not distribute (UD-1060 precondition)"
    );
    if np == 1 {
        distribute_rcb(&mut m)?;
    } else {
        let mut partitioner: Box<dyn MeshPartitioner> = if comm.rank() == 0 {
            match how {
                StripPartition::OracleParts => {
                    Box::new(PrescribedPartition::new(oracle_partition(scenario, np)?))
                }
                StripPartition::MeshDataRule => {
                    Box::new(MeshDataPartition::new(pbd_strip_mapping(&m, np)?))
                }
            }
        } else {
            Box::new(PrescribedPartition::new(Vec::new()))
        };
        let mut failed = None;
        if comm.rank() == 0 && fx.quantity("partition/root_connectivity_after").is_ok() {
            // upstream partitionates on the tweaked connectivity and restores
            // it (`mesh_partition_scotch.cc:370`); the port never changes it
            let want = fx
                .load_i64("partition/root_connectivity_after")
                .map_err(|e| e.to_string())?
                .data;
            let have: Vec<i64> = ok(
                m.connectivity(akantu_core::ElementType::Triangle3, GhostType::NotGhost),
                "conn",
            )?
            .as_slice()
            .iter()
            .map(|&v| i64::from(v))
            .collect();
            if have != want {
                failed = Some(format!(
                    "{scenario} np {np}: root connectivity after partitioning differs"
                ));
            }
        }
        if comm.rank() == 0 && failed.is_none() {
            match partitioner.partition(&m, np) {
                Ok(part) => {
                    if let Err(e) = compare_partition_with_oracle(&m, &part, &fx) {
                        failed = Some(format!("{scenario} np {np}: partition: {e}"));
                    }
                }
                Err(e) => failed = Some(format!("{scenario} np {np}: partition: {e}")),
            }
        }
        // distribute on every rank before reporting (collective)
        let r = akantu_mesh::distribution::distribute_with(
            &mut m,
            if comm.rank() == 0 {
                Some(partitioner.as_mut())
            } else {
                None
            },
            0,
        );
        if let Some(e) = failed {
            return Err(e);
        }
        let _ = ok(r, "distribute")?;
    }
    compare_with_oracle(&m, &fx).map_err(|e| format!("{scenario} np {np}: {e}"))?;
    compare_mesh_data_g20(&m, &fx).map_err(|e| format!("{scenario} np {np}: {e}"))?;
    compare_periodic_state(&m, &fx).map_err(|e| format!("{scenario} np {np}: {e}"))?;
    compare_bbox(&m, &fx, "mesh/bbox").map_err(|e| format!("{scenario} np {np}: {e}"))?;
    Ok(())
}

/// `pbd_strip_scotch` replayed with the oracle's Scotch partition.
pub fn strip_scotch_matches_oracle(comm: Communicator) -> Outcome {
    strip_matches_oracle(&comm, STRIP_SCOTCH, StripPartition::OracleParts)
}

/// `pbd_strip_mesh_data` replayed with the probe's rule through
/// `MeshDataPartition`.
pub fn strip_mesh_data_matches_oracle(comm: Communicator) -> Outcome {
    strip_matches_oracle(&comm, STRIP_MESH_DATA, StripPartition::MeshDataRule)
}

/// The probe's `distribution_sparse_mixed` set-up on rank 0: `sparse2d.msh`
/// and the Int nodal datum `node_tag` (2 components).
pub fn sparse2d_on_root(comm: &Communicator) -> std::result::Result<Mesh, String> {
    let mut m = probe_mesh_on_root(comm, "sparse2d.msh", 2)?;
    if comm.rank() == 0 {
        let n = m.nb_nodes();
        let mut tag = akantu_core::Array::<akantu_core::Int>::zeros(n, 2);
        for i in 0..n {
            let ii = i as akantu_core::Int;
            tag.as_mut_slice()[2 * i] = (5 * ii) % 13;
            tag.as_mut_slice()[2 * i + 1] = 100 - ii;
        }
        m.data_mut()
            .insert_nodal("node_tag", NodalDataValues::Int(tag));
    }
    Ok(m)
}

/// `sparse2d.msh` distributed with the oracle's Scotch partition
/// equals the oracle rank by rank (mesh, synchronizers, groups, physical
/// names, adjacency, nodal data).
pub fn sparse_mixed_matches_oracle(comm: Communicator) -> Outcome {
    let np = comm.size();
    let mut m = sparse2d_on_root(&comm)?;
    if np > 1 {
        let parts = if comm.rank() == 0 {
            oracle_partition(SPARSE_MIXED, np)?
        } else {
            Vec::new()
        };
        distribute_prescribed(&mut m, parts)?;
    } else {
        distribute_rcb(&mut m)?;
    }
    let fx = open_case(SPARSE_MIXED, &comm)?;
    compare_with_oracle(&m, &fx).map_err(|e| format!("np {np}: {e}"))?;
    compare_mesh_data_g20(&m, &fx).map_err(|e| format!("np {np}: {e}"))?;
    let tag: Vec<i64> = match ok(m.data().nodal("node_tag"), "node_tag")? {
        NodalDataValues::Int(a) => a.as_slice().iter().map(|&v| i64::from(v)).collect(),
        other => return Err(format!("node_tag: {other:?}")),
    };
    ensure!(
        tag == fx
            .load_i64("nodal_data/node_tag")
            .map_err(|e| e.to_string())?
            .data,
        "np {np}: nodal data node_tag differs"
    );
    Ok(())
}

/// Feature `scotch`: the port's Scotch partition of the periodic strip
/// (dual graph on the tweaked connectivity) equals the oracle's at 2 and
/// 4 parts, with the same ghost parts (serial; the partitioner runs on
/// the root only).
#[cfg(feature = "scotch")]
pub fn strip_scotch_partition_matches_oracle(comm: Communicator) -> Outcome {
    use akantu_mesh::scotch::ScotchPartitioner;
    let m = pstrip_periodic_on_root(&comm)?;
    for np in [2usize, 4] {
        let fx = akantu_validation::Fixture::open_by_id(&format!(
            "parallel/{STRIP_SCOTCH}/np{np}/rank0"
        ))
        .map_err(|e| e.to_string())?;
        let seed = fx.manifest().parameters["scotch/seed"]
            .as_i64()
            .ok_or("scotch/seed missing")?;
        let part = ok(ScotchPartitioner::new(seed).partition(&m, np), "scotch")?;
        compare_partition_with_oracle(&m, &part, &fx).map_err(|e| format!("np {np}: {e}"))?;
    }
    Ok(())
}

/// UD-1061: the missing periodic partners appended to a rank by the
/// distribution carry the root's nodal data (upstream reads their values
/// past the end of the received buffer: `fillTagBuffers` packs only the
/// rank's own nodes, `fillNodalData` unpacks every node of the mesh,
/// missing ones included). The strip with the `pbd_strip_mesh_data`
/// partition, an Int and a Real nodal datum keyed by the global id; at 4
/// ranks rank 3 holds missing partners.
pub fn ud1061_missing_periodic_nodes_carry_nodal_data(comm: Communicator) -> Outcome {
    let np = comm.size();
    let mut m = pstrip_periodic_on_root(&comm)?;
    if comm.rank() == 0 {
        let n = m.nb_nodes();
        let mut tag = akantu_core::Array::<akantu_core::Int>::zeros(n, 1);
        let mut x = akantu_core::Array::<Real>::zeros(n, 2);
        for i in 0..n {
            tag.as_mut_slice()[i] = 3 * i as akantu_core::Int + 1;
            x.as_mut_slice()[2 * i] = 0.5 + i as Real;
            x.as_mut_slice()[2 * i + 1] = -(i as Real);
        }
        m.data_mut()
            .insert_nodal("gid_tag", NodalDataValues::Int(tag));
        m.data_mut().insert_nodal("gid_x", NodalDataValues::Real(x));
    }
    if np == 1 {
        distribute_rcb(&mut m)?;
    } else {
        let mut p: Box<dyn MeshPartitioner> = if comm.rank() == 0 {
            Box::new(MeshDataPartition::new(pbd_strip_mapping(&m, np)?))
        } else {
            Box::new(PrescribedPartition::new(Vec::new()))
        };
        let _ = ok(
            akantu_mesh::distribution::distribute_with(
                &mut m,
                if comm.rank() == 0 {
                    Some(p.as_mut())
                } else {
                    None
                },
                0,
            ),
            "distribute",
        )?;
    }
    let tag = match ok(m.data().nodal("gid_tag"), "gid_tag")? {
        NodalDataValues::Int(a) => a.as_slice().to_vec(),
        other => return Err(format!("gid_tag: {other:?}")),
    };
    let x = match ok(m.data().nodal("gid_x"), "gid_x")? {
        NodalDataValues::Real(a) => a.as_slice().to_vec(),
        other => return Err(format!("gid_x: {other:?}")),
    };
    ensure!(
        tag.len() == m.nb_nodes() && x.len() == 2 * m.nb_nodes(),
        "nodal data sizes {} / {} for {} nodes",
        tag.len(),
        x.len(),
        m.nb_nodes()
    );
    let mut missing = 0usize;
    for n in 0..m.nb_nodes() {
        let g = m.node_global_id(n as Idx);
        ensure!(
            i64::from(tag[n]) == 3 * g + 1
                && x[2 * n].to_bits() == (0.5 + g as Real).to_bits()
                && x[2 * n + 1].to_bits() == (-(g as Real)).to_bits(),
            "node {n} (global {g}): nodal data {} / ({}, {})",
            tag[n],
            x[2 * n],
            x[2 * n + 1]
        );
        // a missing partner: a periodic master held by no element of this rank
        if m.node_flag(n as Idx).is_periodic_master() && !node_in_an_element(&m, n as Idx)? {
            missing += 1;
        }
    }
    if np == 4 && comm.rank() == 3 {
        ensure!(missing > 0, "rank 3 holds no missing periodic partner");
    }
    Ok(())
}

fn node_in_an_element(m: &Mesh, node: Idx) -> std::result::Result<bool, String> {
    for ghost in GhostType::ALL {
        for ty in m.element_types(TypeFilter::new().ghost(ghost)) {
            if ok(m.connectivity(ty, ghost), "conn")?
                .as_slice()
                .contains(&node)
            {
                return Ok(true);
            }
        }
    }
    Ok(false)
}
