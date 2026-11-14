// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): test/test_synchronizer/{test_synchronizers_fixture.hh,
// test_data_distribution.cc, test_node_synchronizer.cc,
// test_synchronizer_communication.cc, test_data_accessor.hh}.

//! Distribution and entity-synchronizer scenarios shared by the local-world
//! tests (`upstream_synchronizers.rs`, 1/2/4 ranks as threads) and the
//! real-MPI binary (`mpi_distribution.rs`, `mpirun -np 1/2/4/8`). Every
//! scenario takes the communicator and returns `Err(message)` on failure.

#![allow(dead_code)]
// each binary uses a subset
// `ensure!(a <= b, …)` negates a float comparison on purpose (NaN fails)
#![allow(clippy::neg_cmp_op_on_partial_ord)]

use akantu_validation::reduce::NanMinMax;
use std::path::PathBuf;

use akantu_comm::buffer::CommunicationBuffer;
use akantu_comm::{Communicator, DataAccessor};
use akantu_core::enums::{MeshIoType, SynchronizationTag};
use akantu_core::{
    Element, ElementTypeMapArray, GhostType, Idx, NodeFlag, Rank, Real, Result, TypeFilter,
};
use akantu_io::{ReadOptions, read_mesh_with};
use akantu_mesh::distribution::{
    MeshPartitioner, PrescribedPartition, RcbPartitioner, distribute_with,
};
use akantu_mesh::synchronizers::{NodeEntities, enable_sanity_checks};
use akantu_mesh::{BBox, Mesh, MeshDataValues, NodalDataValues};

/// Scenario outcome.
pub type Outcome = std::result::Result<(), String>;

#[macro_export]
macro_rules! ensure {
    ($cond:expr, $($fmt:tt)+) => {
        if !($cond) {
            return Err(format!("{}:{}: {}", file!(), line!(), format!($($fmt)+)));
        }
    };
}

/// `Result` → `Outcome` with context.
pub fn ok<T>(r: Result<T>, what: &str) -> std::result::Result<T, String> {
    r.map_err(|e| format!("{what}: {e}"))
}

/// A frozen Gmsh mesh of the upstream test tree.
pub fn frozen(rel: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/meshes/gmsh-4.13.1/test")
        .join(rel)
}

/// Upstream `TestSynchronizerFixture::SetUp`: a 3-D mesh, `cube.msh` read
/// on rank 0 only.
pub fn read_on_root(
    comm: &Communicator,
    rel: &str,
    dim: usize,
) -> std::result::Result<Mesh, String> {
    let mut m = ok(Mesh::with_communicator(dim, "mesh", comm.clone()), "mesh")?;
    if comm.rank() == 0 {
        let opts = ReadOptions {
            fill_unnamed_physical_groups: false,
            write_diana_material_file: false,
        };
        ok(
            read_mesh_with(&mut m, &frozen(rel), MeshIoType::Auto, &opts),
            "read",
        )?;
    }
    Ok(m)
}

/// Distribute with the pure-Rust bisection partitioner (Scotch is a
/// Docker-only feature; the scenarios check partitioner-independent
/// properties).
pub fn distribute_rcb(m: &mut Mesh) -> Outcome {
    let mut p = RcbPartitioner;
    let _ = ok(
        distribute_with(m, Some(&mut p as &mut dyn MeshPartitioner), 0),
        "distribute",
    )?;
    Ok(())
}

/// Distribute with a prescribed linearised partition (root only).
pub fn distribute_prescribed(m: &mut Mesh, parts: Vec<Idx>) -> Outcome {
    let mut p = PrescribedPartition::new(parts);
    let _ = ok(
        distribute_with(m, Some(&mut p as &mut dyn MeshPartitioner), 0),
        "distribute",
    )?;
    Ok(())
}

/// Barycentres of every element (upstream `initBarycenters`).
pub fn barycenters(m: &Mesh) -> std::result::Result<ElementTypeMapArray<Real>, String> {
    let mut out = ElementTypeMapArray::new("barycenters");
    for ghost in GhostType::ALL {
        for ty in m.element_types(TypeFilter::new().ghost(ghost)) {
            let b = ok(m.barycenters(ty, ghost), "barycenters")?;
            let a = ok(out.alloc(0, m.spatial_dimension(), ty, ghost, 0.0), "alloc")?;
            *a = b;
        }
    }
    Ok(out)
}

// ------------------------------------------------------- data distribution

/// Upstream `TestSynchronizerFixture.DataDistribution`: elemental
/// barycentres and nodal ids attached on the root travel with their
/// elements and nodes.
pub fn data_distribution(comm: Communicator) -> Outcome {
    let mut m = read_on_root(&comm, "test_synchronizer/cube.msh", 3)?;
    if comm.rank() == 0 {
        let b = barycenters(&m)?;
        m.data_mut()
            .insert_elemental("barycenters", MeshDataValues::Real(b.into()));
        let gids: Vec<u32> = (0..m.nb_nodes()).map(|n| n as u32).collect();
        m.data_mut().insert_nodal(
            "gid",
            NodalDataValues::UInt(ok(akantu_core::Array::from_vec(gids, 1), "gid")?),
        );
    }
    distribute_rcb(&mut m)?;
    let dim = m.spatial_dimension();
    let data = ok(m.data().reals("barycenters"), "barycenters data")?;
    for ghost in GhostType::ALL {
        for ty in m.element_types(TypeFilter::new().ghost(ghost)) {
            let stored = ok(data.get(ty, ghost), "block")?;
            ensure!(
                stored.len() == m.nb_element(ty, ghost),
                "{ty}:{ghost}: {} values for {} elements",
                stored.len(),
                m.nb_element(ty, ghost)
            );
            let now = ok(m.barycenters(ty, ghost), "barycenters")?;
            for (a, b) in stored
                .as_slice()
                .chunks(dim)
                .zip(now.as_slice().chunks(dim))
            {
                let dist = a
                    .iter()
                    .zip(b)
                    .map(|(x, y)| (x - y).abs())
                    .fold(0.0, NanMinMax::nan_max);
                ensure!(dist <= 1e-7, "{ty}:{ghost}: barycenter moved by {dist}");
            }
        }
    }
    if comm.size() > 1 {
        let NodalDataValues::UInt(g) = ok(m.data().nodal("gid"), "gid")? else {
            return Err("gid has the wrong type".into());
        };
        for (n, &v) in g.as_slice().iter().enumerate() {
            let gid = m.node_global_id(Idx::try_from(n).unwrap());
            ensure!(i64::from(v) == gid, "node {n}: gid {v} != global id {gid}");
        }
    }
    Ok(())
}

/// Upstream `TestSynchronizerFixture.DataDistributionTags`: the Gmsh
/// `tag_0` dataset has one value per local element after distribution.
pub fn data_distribution_tags(comm: Communicator) -> Outcome {
    let mut m = read_on_root(&comm, "test_synchronizer/cube.msh", 3)?;
    distribute_rcb(&mut m)?;
    let tags = ok(m.data().ints("tag_0"), "tag_0")?;
    for ty in m.element_types(TypeFilter::new()) {
        let n = tags.get(ty, GhostType::NotGhost).map_or(0, |a| a.len());
        ensure!(
            n == m.nb_element(ty, GhostType::NotGhost),
            "{ty}: {n} tags for {} elements",
            m.nb_element(ty, GhostType::NotGhost)
        );
    }
    Ok(())
}

// ------------------------------------------------------- node synchronizer

const MAX_INT: i32 = i32::MAX;

/// Upstream `DataAccessorTest` (one `int` per node).
pub struct NodeAccessor<'a>(pub &'a mut [i32]);

impl DataAccessor<Idx> for NodeAccessor<'_> {
    fn nb_data(&self, nodes: &[Idx], _: SynchronizationTag) -> usize {
        nodes.len() * 4
    }
    fn pack_data(
        &self,
        b: &mut CommunicationBuffer,
        nodes: &[Idx],
        _: SynchronizationTag,
    ) -> Result<()> {
        for &n in nodes {
            b.pack(&self.0[n as usize]);
        }
        Ok(())
    }
    fn unpack_data(
        &mut self,
        b: &mut CommunicationBuffer,
        nodes: &[Idx],
        _: SynchronizationTag,
    ) -> Result<()> {
        for &n in nodes {
            self.0[n as usize] = b.unpack()?;
        }
        Ok(())
    }
}

fn node_data(m: &Mesh) -> Vec<i32> {
    (0..m.nb_nodes())
        .map(|n| {
            let n = n as Idx;
            let g = m.node_global_id(n) as i32;
            let f = m.node_flag(n);
            if f.is_master() {
                g
            } else if f.is_local() {
                -g
            } else if f.is_slave() {
                MAX_INT
            } else {
                -MAX_INT
            }
        })
        .collect()
}

fn check_node_data(m: &Mesh, d: &[i32]) -> Outcome {
    for (n, &v) in d.iter().enumerate() {
        let n = n as Idx;
        let g = m.node_global_id(n) as i32;
        let f = m.node_flag(n);
        let want = if f.is_master() {
            g
        } else if f.is_local() {
            -g
        } else if f.is_slave() {
            g
        } else {
            -MAX_INT
        };
        ensure!(v == want, "node {n} (flag {:#x}): {v} != {want}", f.bits());
    }
    Ok(())
}

fn distributed_cube(comm: &Communicator) -> std::result::Result<Mesh, String> {
    let mut m = read_on_root(comm, "test_synchronizer/cube.msh", 3)?;
    distribute_rcb(&mut m)?;
    Ok(m)
}

/// Upstream `TestNodeSynchronizerFixture.Synchrone` (and `SynchroneOnce`).
pub fn node_synchronizer_synchronize(comm: Communicator) -> Outcome {
    let m = distributed_cube(&comm)?;
    let mut d = node_data(&m);
    let sync = m
        .synchronizers()
        .node
        .clone()
        .ok_or("no node synchronizer")?;
    ok(
        sync.synchronize(&mut NodeAccessor(&mut d), SynchronizationTag::Test),
        "synchronize",
    )?;
    check_node_data(&m, &d)
}

/// Upstream `TestNodeSynchronizerFixture.Asynchrone`.
pub fn node_synchronizer_asynchronous(comm: Communicator) -> Outcome {
    let m = distributed_cube(&comm)?;
    let mut d = node_data(&m);
    let sync = m
        .synchronizers()
        .node
        .clone()
        .ok_or("no node synchronizer")?;
    ok(
        sync.asynchronous_synchronize(&NodeAccessor(&mut d.clone()), SynchronizationTag::Test),
        "asynchronous_synchronize",
    )?;
    std::thread::sleep(std::time::Duration::from_micros(
        10 + 37 * comm.rank() as u64,
    ));
    ok(
        sync.wait_end_synchronize(&mut NodeAccessor(&mut d), SynchronizationTag::Test),
        "wait_end_synchronize",
    )?;
    check_node_data(&m, &d)
}

/// Upstream `TestNodeSynchronizerFixture.Gather`.
pub fn node_synchronizer_gather(comm: Communicator) -> Outcome {
    let m = distributed_cube(&comm)?;
    let d = node_data(&m);
    let sync = m
        .synchronizers()
        .node
        .clone()
        .ok_or("no node synchronizer")?;
    let all = ok(sync.gather(&NodeEntities { mesh: &m }, &d, 1), "gather")?;
    if comm.rank() == 0 {
        let all = all.ok_or("root got nothing")?;
        ensure!(
            all.len() == m.nb_global_nodes(),
            "gathered {} values",
            all.len()
        );
        for (i, v) in all.iter().enumerate() {
            ensure!(i as i32 == v.abs(), "gathered[{i}] = {v}");
        }
    }
    Ok(())
}

/// Upstream `TestNodeSynchronizerFixture.Scatter`.
pub fn node_synchronizer_scatter(comm: Communicator) -> Outcome {
    let m = distributed_cube(&comm)?;
    let mut local = vec![MAX_INT; m.nb_nodes()];
    let sync = m
        .synchronizers()
        .node
        .clone()
        .ok_or("no node synchronizer")?;
    let all: Option<Vec<i32>> =
        (comm.rank() == 0).then(|| (0..m.nb_global_nodes() as i32).collect());
    ok(
        sync.scatter(&NodeEntities { mesh: &m }, &mut local, all.as_deref(), 1),
        "scatter",
    )?;
    for (n, &v) in local.iter().enumerate() {
        let n = n as Idx;
        let want = if m.node_flag(n).is_pure_ghost() {
            MAX_INT
        } else {
            m.node_global_id(n) as i32
        };
        ensure!(v == want, "node {n}: {v} != {want}");
    }
    Ok(())
}

// ---------------------------------------------------- element synchronizer

/// Upstream `TestAccessor` (barycentres compared on unpack, 1e-15).
pub struct BarycenterAccessor<'a> {
    pub dim: usize,
    pub bary: &'a ElementTypeMapArray<Real>,
    pub max_dist: Real,
    pub unpacked: usize,
}

impl DataAccessor<Element> for BarycenterAccessor<'_> {
    fn nb_data(&self, els: &[Element], _: SynchronizationTag) -> usize {
        els.len() * self.dim * 8
    }
    fn pack_data(
        &self,
        b: &mut CommunicationBuffer,
        els: &[Element],
        _: SynchronizationTag,
    ) -> Result<()> {
        for &e in els {
            b.pack_slice(self.bary.element(e)?);
        }
        Ok(())
    }
    fn unpack_data(
        &mut self,
        b: &mut CommunicationBuffer,
        els: &[Element],
        _: SynchronizationTag,
    ) -> Result<()> {
        let mut v = vec![0.0; self.dim];
        for &e in els {
            b.unpack_slice(&mut v)?;
            let l = self.bary.element(e)?;
            let d = l
                .iter()
                .zip(&v)
                .map(|(a, b)| (a - b).abs())
                .fold(0.0, NanMinMax::nan_max);
            self.max_dist = self.max_dist.nan_max(d);
            self.unpacked += 1;
        }
        Ok(())
    }
}

/// Upstream `TestElementSynchronizerFixture.{SynchroneOnce, Synchrone,
/// Asynchrone}`: ghost barycentres received from their owners equal the
/// local ones (1e-15; here exactly), with the sanity checks on.
pub fn element_synchronizer_communication(comm: Communicator) -> Outcome {
    let mut m = distributed_cube(&comm)?;
    ok(enable_sanity_checks(&mut m, true), "sanity")?;
    let b = barycenters(&m)?;
    let sync = m
        .synchronizers()
        .element
        .clone()
        .ok_or("no element synchronizer")?;
    let nb_recv: usize = sync.communications.recv.values().map(Vec::len).sum();
    let mut acc = BarycenterAccessor {
        dim: 3,
        bary: &b,
        max_dist: 0.0,
        unpacked: 0,
    };
    ok(
        sync.synchronize(&mut acc, SynchronizationTag::Test),
        "synchronize",
    )?;
    ensure!(
        acc.max_dist <= 1e-15,
        "barycenter distance {}",
        acc.max_dist
    );
    ensure!(
        acc.max_dist == 0.0,
        "barycenters not bitwise: {}",
        acc.max_dist
    );
    ensure!(
        acc.unpacked == nb_recv,
        "unpacked {} of {nb_recv}",
        acc.unpacked
    );
    ok(
        sync.asynchronous_synchronize(&acc, SynchronizationTag::Test),
        "asynchronous",
    )?;
    std::thread::sleep(std::time::Duration::from_micros(
        10 + 23 * comm.rank() as u64,
    ));
    ok(
        sync.wait_end_synchronize(&mut acc, SynchronizationTag::Test),
        "wait_end",
    )?;
    ensure!(
        acc.max_dist == 0.0,
        "barycenters not bitwise: {}",
        acc.max_dist
    );
    // node synchronizer with sanity checks
    let mut d = node_data(&m);
    let ns = m
        .synchronizers()
        .node
        .clone()
        .ok_or("no node synchronizer")?;
    ok(
        ns.synchronize(&mut NodeAccessor(&mut d), SynchronizationTag::Test),
        "node sanity",
    )?;
    check_node_data(&m, &d)
}

// ------------------------------------------------------------- invariants

/// Global invariants of a distributed mesh, gathered on rank 0: every
/// element of the mesh dimension is local on exactly one rank; every
/// global node has exactly one owner (normal or master); slaves and pure
/// ghosts point to the owner; node and element schemes are symmetric
/// (sender's list == receiver's list, by global ids / global
/// connectivity); coordinates match the global node.
pub fn distribution_invariants(comm: Communicator) -> Outcome {
    let mut m = read_on_root(&comm, "test_synchronizer/cube.msh", 3)?;
    let global_nb_nodes = m.nb_nodes();
    let global_elems: usize = m.nb_element_filtered(TypeFilter::new().dim(3));
    let global_coords = m.nodes().as_slice().to_vec();
    distribute_rcb(&mut m)?;
    let me = comm.rank();
    // local summary: (gid, flag, prank) per node; schemes in gids
    let mut summary: Vec<i64> = Vec::new();
    summary.push(m.nb_element_filtered(TypeFilter::new().dim(3)) as i64);
    summary.push(m.nb_nodes() as i64);
    for n in 0..m.nb_nodes() {
        let n = n as Idx;
        summary.push(m.node_global_id(n));
        summary.push(i64::from(m.node_flag(n).bits()));
        summary.push(i64::from(m.node_prank(n)));
    }
    let ns = m.synchronizers().node.as_ref().ok_or("no node sync")?;
    for sch in [&ns.communications.send, &ns.communications.recv] {
        summary.push(sch.len() as i64);
        for (&q, nodes) in sch {
            summary.push(i64::from(q));
            summary.push(nodes.len() as i64);
            summary.extend(nodes.iter().map(|&n| m.node_global_id(n)));
        }
    }
    let es = m
        .synchronizers()
        .element
        .as_ref()
        .ok_or("no element sync")?;
    for sch in [&es.communications.send, &es.communications.recv] {
        summary.push(sch.len() as i64);
        for (&q, els) in sch {
            summary.push(i64::from(q));
            summary.push(els.len() as i64);
            for &e in els {
                let c = ok(m.element_connectivity(e), "conn")?;
                let mut g: Vec<i64> = c.iter().map(|&n| m.node_global_id(n)).collect();
                g.sort_unstable();
                summary.push(g.len() as i64);
                summary.extend(g);
            }
        }
    }
    // coordinates check locally
    if comm.rank() == 0 {
        // root keeps its own copy; others checked against the gathered gids
    }
    let coords = m.nodes().as_slice().to_vec();
    let all = ok(comm.gather_v(&summary, 0), "gather")?;
    let all_coords = ok(comm.gather_v(&coords, 0), "gather coords")?;
    if me != 0 {
        return Ok(());
    }
    let all = all.ok_or("no gather")?;
    let all_coords = all_coords.ok_or("no gather")?;
    let size = all.len();
    let mut owners = vec![Vec::<Rank>::new(); global_nb_nodes];
    let mut elems = 0usize;
    type Scheme = std::collections::BTreeMap<(Rank, Rank), Vec<Vec<i64>>>;
    let mut node_send: Scheme = Default::default();
    let mut node_recv: Scheme = Default::default();
    let mut el_send: Scheme = Default::default();
    let mut el_recv: Scheme = Default::default();
    let mut nodes_of: Vec<Vec<(i64, NodeFlag, Rank)>> = Vec::new();
    for (p, s) in all.iter().enumerate() {
        let p = p as Rank;
        let mut it = s.iter().copied();
        let mut next = || it.next().ok_or_else(|| "short summary".to_string());
        elems += next()? as usize;
        let nn = next()? as usize;
        let mut nodes = Vec::with_capacity(nn);
        for l in 0..nn {
            let g = next()?;
            let f = NodeFlag(next()? as u8);
            let pr = next()? as Rank;
            let x = &all_coords[p as usize][l * 3..l * 3 + 3];
            ensure!(
                x == &global_coords[g as usize * 3..g as usize * 3 + 3],
                "rank {p} node {l}: coordinates differ from global node {g}"
            );
            if f.is_local_or_master() {
                owners[g as usize].push(p);
            }
            nodes.push((g, f, pr));
        }
        nodes_of.push(nodes);
        for (k, sch) in [&mut node_send, &mut node_recv].into_iter().enumerate() {
            let nq = next()? as usize;
            for _ in 0..nq {
                let q = next()? as Rank;
                let len = next()? as usize;
                let mut v = Vec::with_capacity(len);
                for _ in 0..len {
                    v.push(vec![next()?]);
                }
                let key = if k == 0 { (p, q) } else { (q, p) };
                sch.insert(key, v);
            }
        }
        for (k, sch) in [&mut el_send, &mut el_recv].into_iter().enumerate() {
            let nq = next()? as usize;
            for _ in 0..nq {
                let q = next()? as Rank;
                let len = next()? as usize;
                let mut v = Vec::with_capacity(len);
                for _ in 0..len {
                    let nc = next()? as usize;
                    let mut c = Vec::with_capacity(nc);
                    for _ in 0..nc {
                        c.push(next()?);
                    }
                    v.push(c);
                }
                let key = if k == 0 { (p, q) } else { (q, p) };
                sch.insert(key, v);
            }
        }
    }
    ensure!(
        elems == global_elems,
        "{elems} local elements for {global_elems}"
    );
    for (g, o) in owners.iter().enumerate() {
        ensure!(
            o.len() == 1 || size == 1 && o.len() == 1,
            "global node {g} owners {o:?}"
        );
    }
    for (p, nodes) in nodes_of.iter().enumerate() {
        for &(g, f, pr) in nodes {
            if !f.is_local_or_master() {
                ensure!(
                    pr == owners[g as usize][0],
                    "rank {p}: node {g} prank {pr}, owner {:?}",
                    owners[g as usize]
                );
            }
        }
    }
    ensure!(node_send == node_recv, "node schemes are not symmetric");
    ensure!(el_send == el_recv, "element schemes are not symmetric");
    Ok(())
}

/// ARCHITECTURE.md, "Agreement across ranks" collective error protocol for distribution: a rank-specific
/// failure (a non-root rank holding a non-empty mesh; then a root partition
/// for the wrong number of parts) is reported on **every** rank, promptly
/// (far below the operation timeout), and every mesh is left unchanged;
/// a later correct distribution on the same communicator succeeds.
pub fn distribution_failure_agreed(comm: Communicator) -> Outcome {
    let t0 = std::time::Instant::now();
    let size = comm.size();
    let rank = comm.rank();
    // (1) the last rank holds a node
    let mut m = read_on_root(&comm, "test_synchronizer/cube.msh", 3)?;
    if size > 1 && rank == size as Rank - 1 {
        ok(
            akantu_mesh::MeshAccessor::new(&mut m)
                .push_node(&[0.0, 0.0, 0.0])
                .map(|_| ()),
            "push",
        )?;
    }
    let before = m.state_fingerprint();
    let r = {
        let mut p = RcbPartitioner;
        distribute_with(&mut m, Some(&mut p as &mut dyn MeshPartitioner), 0)
    };
    if size > 1 {
        ensure!(
            r.is_err(),
            "rank {rank}: a rank-specific failure must fail everywhere"
        );
        ensure!(
            m.state_fingerprint() == before,
            "rank {rank}: mesh changed by a failed distribution"
        );
    } else {
        ensure!(r.is_ok(), "one rank: {r:?}");
    }
    // (2) wrong number of parts on the root
    if size > 1 {
        let mut m = read_on_root(&comm, "test_synchronizer/cube.msh", 3)?;
        let n = m.nb_element_filtered(TypeFilter::new().dim(3));
        let parts: Vec<Idx> = vec![0; n];
        let mut p = PrescribedPartition::new(parts);
        // a partition into one part on more ranks: the root's preflight fails
        let r = if rank == 0 {
            let part = ok(p.partition(&m, 1), "partition")?;
            akantu_mesh::distribution::distribute(&mut m, Some(&part), 0)
        } else {
            akantu_mesh::distribution::distribute(&mut m, None, 0)
        };
        ensure!(
            r.is_err(),
            "rank {rank}: wrong part count must fail everywhere"
        );
    }
    // (3) the communicator is still usable
    let mut m = read_on_root(&comm, "test_synchronizer/cube.msh", 3)?;
    distribute_rcb(&mut m)?;
    let dt = t0.elapsed();
    ensure!(
        dt < std::time::Duration::from_secs(10),
        "took {dt:?}: a rank waited for a timeout"
    );
    Ok(())
}

// ------------------------------------------------- oracle comparison helpers

/// Compare what this rank holds with the oracle's record of the same rank
/// (`parallel/<scenario>/np<P>/rank<r>`, oracle/probes/parallel): node
/// global ids, flags, owner ranks and coordinates (bitwise), every
/// connectivity block (local ids), ghost counters, element and node
/// synchronizer schemes (entity order), element and node groups, and the
/// `tag_0`/`tag_1` mesh data. Exact; returns the first difference.
pub fn compare_with_oracle(m: &Mesh, fx: &akantu_validation::Fixture) -> Outcome {
    use akantu_core::ElementType;
    let p = &fx.manifest().parameters;
    let li = |name: &str| fx.load_i64(name).map_err(|e| format!("{name}: {e}"));
    let lf = |name: &str| fx.load_f64(name).map_err(|e| format!("{name}: {e}"));
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
    ensure!(
        p["nb_global_nodes"].as_u64() == Some(m.nb_global_nodes() as u64),
        "nb_global_nodes {} vs oracle {}",
        m.nb_global_nodes(),
        p["nb_global_nodes"]
    );
    ensure!(m.is_distributed(), "mesh not distributed");
    let n = m.nb_nodes();
    let gids: Vec<i64> = (0..n).map(|i| m.node_global_id(i as Idx)).collect();
    ensure!(
        gids == li("nodes/global_ids")?.data,
        "node global ids differ"
    );
    let flags: Vec<i64> = m
        .nodes_flags()
        .as_slice()
        .iter()
        .map(|f| i64::from(f.bits()))
        .collect();
    ensure!(flags == li("nodes/flags")?.data, "node flags differ");
    let pranks: Vec<i64> = (0..n).map(|i| i64::from(m.node_prank(i as Idx))).collect();
    ensure!(
        pranks == li("nodes/pranks")?.data,
        "node owner ranks differ"
    );
    let x = lf("nodes/coordinates")?.data;
    ensure!(
        x.len() == m.nodes().as_slice().len()
            && x.iter()
                .zip(m.nodes().as_slice())
                .all(|(a, b)| akantu_validation::exact_eq(*a, *b)), // D-046
        "coordinates differ"
    );
    // blocks
    let mut mine = Vec::new();
    for ghost in GhostType::ALL {
        for ty in m.element_types(TypeFilter::new().ghost(ghost)) {
            let g = if ghost == GhostType::NotGhost {
                "not_ghost"
            } else {
                "ghost"
            };
            mine.push(format!("{}/{g}", ty.name()));
        }
    }
    ensure!(
        mine == strs("blocks"),
        "blocks {mine:?} vs oracle {:?}",
        strs("blocks")
    );
    for key in &mine {
        let (tn, g) = key.split_once('/').unwrap();
        let ty: ElementType = tn.parse().map_err(|e| format!("{e}"))?;
        let ghost = if g == "ghost" {
            GhostType::Ghost
        } else {
            GhostType::NotGhost
        };
        let conn: Vec<i64> = ok(m.connectivity(ty, ghost), "conn")?
            .as_slice()
            .iter()
            .map(|&v| i64::from(v))
            .collect();
        ensure!(
            conn == li(&format!("conn/{key}"))?.data,
            "connectivity {key} differs"
        );
        if ghost == GhostType::Ghost {
            let gc: Vec<i64> = m
                .ghosts_counters()
                .get(ty, ghost)
                .map(|a| a.as_slice().iter().map(|&v| i64::from(v)).collect())
                .unwrap_or_default();
            ensure!(
                gc == li(&format!("ghosts_counters/{key}"))?.data,
                "ghost counters {key} differ"
            );
        }
        for tag in ["tag_0", "tag_1"] {
            let name = format!("mesh_data/{tag}/{key}");
            let want = fx.load_i64(&name).ok().map(|f| f.data);
            let have: Option<Vec<i64>> = m
                .data()
                .ints(tag)
                .ok()
                .and_then(|d| d.get(ty, ghost).ok())
                .map(|a| a.as_slice().iter().map(|&v| i64::from(v)).collect());
            ensure!(want == have, "{name}: {have:?} vs oracle {want:?}");
        }
    }
    // element schemes
    let table = strs("type_table");
    let es = m
        .synchronizers()
        .element
        .as_ref()
        .ok_or("no element synchronizer")?;
    let mut keys = Vec::new();
    for (dir, sch) in [
        ("send", &es.communications.send),
        ("recv", &es.communications.recv),
    ] {
        for (q, els) in sch {
            let key = format!("{dir}/{q}");
            let want = li(&format!("element_scheme/{key}"))?.data;
            let mut have = Vec::with_capacity(els.len() * 3);
            for e in els {
                let code = table
                    .iter()
                    .position(|t| t == e.ty().name())
                    .ok_or_else(|| format!("type {} not in the oracle table", e.ty()))?;
                have.extend([
                    code as i64,
                    i64::from(e.index()),
                    i64::from(e.ghost_type() == GhostType::Ghost),
                ]);
            }
            ensure!(
                have == want,
                "element scheme {key} differs ({} vs {} entries)",
                have.len() / 3,
                want.len() / 3
            );
            keys.push(key);
        }
    }
    ensure!(
        keys == strs("element_schemes"),
        "element schemes {keys:?} vs {:?}",
        strs("element_schemes")
    );
    let ns = m
        .synchronizers()
        .node
        .as_ref()
        .ok_or("no node synchronizer")?;
    let mut keys = Vec::new();
    for (dir, sch) in [
        ("send", &ns.communications.send),
        ("recv", &ns.communications.recv),
    ] {
        for (q, nodes) in sch {
            let key = format!("{dir}/{q}");
            let have: Vec<i64> = nodes.iter().map(|&v| i64::from(v)).collect();
            ensure!(
                have == li(&format!("node_scheme/{key}"))?.data,
                "node scheme {key} differs"
            );
            keys.push(key);
        }
    }
    ensure!(
        keys == strs("node_schemes"),
        "node schemes {keys:?} vs {:?}",
        strs("node_schemes")
    );
    // groups
    let egs: Vec<String> = m
        .groups()
        .element_groups()
        .map(|g| g.name().to_owned())
        .collect();
    ensure!(
        egs == strs("element_groups"),
        "element groups {egs:?} vs {:?}",
        strs("element_groups")
    );
    for g in m.groups().element_groups() {
        ensure!(
            p[format!("element_group_dimension/{}", g.name())].as_u64()
                == Some(g.group().dimension as u64),
            "element group {} dimension",
            g.name()
        );
        for ghost in GhostType::ALL {
            for ty in g.group().element_types(ghost) {
                let gname = if ghost == GhostType::NotGhost {
                    "not_ghost"
                } else {
                    "ghost"
                };
                let name = format!("element_group/{}/{}/{gname}", g.name(), ty.name());
                let have: Vec<i64> = g
                    .elements_of(ty, ghost)
                    .iter()
                    .map(|&v| i64::from(v))
                    .collect();
                ensure!(have == li(&name)?.data, "{name} differs");
            }
        }
    }
    // The oracle's non-empty element-group blocks are
    // exactly the port's (the loop above only checks the port's blocks: a
    // block the port lacks, e.g. a group's ghost elements, went unnoticed;
    // upstream also keeps empty type blocks, which the port does not create)
    let mut have_blocks = std::collections::BTreeSet::new();
    for g in m.groups().element_groups() {
        for ghost in GhostType::ALL {
            for ty in g.group().element_types(ghost) {
                if g.elements_of(ty, ghost).is_empty() {
                    continue;
                }
                let gname = if ghost == GhostType::NotGhost {
                    "not_ghost"
                } else {
                    "ghost"
                };
                have_blocks.insert(format!("element_group/{}/{}/{gname}", g.name(), ty.name()));
            }
        }
    }
    let want_blocks: std::collections::BTreeSet<String> = fx
        .manifest()
        .quantities
        .iter()
        .filter(|q| q.name.starts_with("element_group/") && q.shape.iter().product::<usize>() > 0)
        .map(|q| q.name.clone())
        .collect();
    ensure!(
        have_blocks == want_blocks,
        "element group blocks differ: only here {:?}, only in the oracle {:?}",
        have_blocks.difference(&want_blocks).collect::<Vec<_>>(),
        want_blocks.difference(&have_blocks).collect::<Vec<_>>()
    );
    let ngs: Vec<String> = m
        .groups()
        .node_groups()
        .map(|g| g.name().to_owned())
        .collect();
    ensure!(
        ngs == strs("node_groups"),
        "node groups {ngs:?} vs {:?}",
        strs("node_groups")
    );
    for g in m.groups().node_groups() {
        let have: Vec<i64> = g.nodes().iter().map(|&v| i64::from(v)).collect();
        ensure!(
            have == li(&format!("node_group/{}", g.name()))?.data,
            "node group {} differs",
            g.name()
        );
    }
    Ok(())
}

/// Rank counts recorded by the oracle for `scenario` (fixtures present).
pub fn oracle_rank_counts(scenario: &str) -> Vec<usize> {
    oracle_rank_counts_in("parallel", scenario)
}

/// Rank counts recorded by the oracle for `scenario` under
/// `fixtures/<area>/` (`np<P>` directories present), sorted.
pub fn oracle_rank_counts_in(area: &str, scenario: &str) -> Vec<usize> {
    let root = akantu_validation::fixtures_root().join(area).join(scenario);
    let mut out: Vec<usize> = std::fs::read_dir(&root)
        .map(|d| {
            d.filter_map(|e| e.ok())
                .filter_map(|e| e.file_name().to_str()?.strip_prefix("np")?.parse().ok())
                .collect()
        })
        .unwrap_or_default();
    out.sort_unstable();
    out
}

/// The oracle fixtures a real-MPI binary replays, each
/// with at least one recorded rank count. [`oracle_rank_counts`] maps an
/// unreadable directory to "no counts", so a wrong fixtures root or a
/// removed case would otherwise drop every replay of the binary silently;
/// the binaries call this at start-up and fail with the returned message
/// (every rank reads the same files, so every rank decides the same).
/// Not `np1`: `nl_mazars_two_strips` is recorded at 3 ranks only.
pub fn missing_oracle_fixtures(area: &str, fixtures: &[&str]) -> std::result::Result<(), String> {
    let missing: Vec<&str> = fixtures
        .iter()
        .copied()
        .filter(|f| oracle_rank_counts_in(area, f).is_empty())
        .collect();
    if missing.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "no recorded rank count under {} for {missing:?} (fixtures root or case missing)",
            akantu_validation::fixtures_root().join(area).display()
        ))
    }
}

/// The scenarios of a real-MPI binary that do not run
/// at this rank count, each with the reason. They print no `PASS` line and
/// are not counted in the `SUMMARY` line's `scenarios=` (only scenarios
/// that ran, i.e. compared something, are); rank 0 prints `NOT RUN <name>:
/// <why>` for each and the `SUMMARY` line ends with `not_run=<count>`.
#[derive(Default)]
pub struct NotRun(Vec<(String, String)>);

impl NotRun {
    /// Record that `name` does not run here, and why.
    pub fn add(&mut self, name: impl Into<String>, why: impl Into<String>) {
        self.0.push((name.into(), why.into()));
    }

    /// Number of scenarios recorded as not run.
    pub fn count(&self) -> usize {
        self.0.len()
    }

    /// The `NOT RUN` lines (rank 0 only).
    pub fn print(&self, rank: Rank) {
        if rank == 0 {
            for (name, why) in &self.0 {
                println!("NOT RUN {name}: {why}");
            }
        }
    }
}

/// Reason of an oracle replay that is not run at `size` ranks: no fixture
/// recorded at that count.
pub fn not_recorded(area: &str, fixture: &str, size: usize) -> String {
    format!(
        "no oracle fixture at {size} ranks (fixtures/{area}/{fixture}: np{:?})",
        oracle_rank_counts_in(area, fixture)
    )
}

/// The association of `MPI_Allreduce(MPI_SUM)` observed for the oracle's
/// Open MPI 5.0.7 (one node, default MCA selection) at the rank counts of
/// the recorded fixtures: the fold of `parts` (one contribution per rank,
/// in rank order) in that association
/// (rank order at 2 and 3 ranks, pairwise at 4 and 8, mixed trees at 5–7).
/// It is a fact about the recorded fixtures, used only to check data that
/// were recorded with it, never a property of MPI the port relies on
/// (D-039: the port folds in rank order). A rank count without an entry
/// here is refused, not guessed.
pub fn oracle_allreduce_fold(parts: &[Real]) -> std::result::Result<Real, String> {
    match *parts {
        [a] => Ok(a),
        [a, b] => Ok(a + b),
        [a, b, c, d] => Ok((a + b) + (c + d)),
        [a, b, c, d, e, f, g, h] => Ok(((a + b) + (c + d)) + ((e + f) + (g + h))),
        _ => Err(format!(
            "no observed MPI_Allreduce association of the oracle's Open MPI at {} ranks \
             (1, 2, 4 and 8 are tabulated here): refused, not guessed",
            parts.len()
        )),
    }
}

/// The port's rank-ordered fold of one contribution per rank
/// (`((p0 + p1) + p2) + …`, `ReductionPolicy::RankOrdered`, D-039); `None`
/// for no contribution.
pub fn rank_order_fold(parts: &[Real]) -> Option<Real> {
    let (first, rest) = parts.split_first()?;
    Some(rest.iter().fold(*first, |acc, &p| acc + p))
}

/// The oracle's linearised Scotch partition of `scenario` at `np` ranks.
pub fn oracle_partition(scenario: &str, np: usize) -> std::result::Result<Vec<Idx>, String> {
    let fx = akantu_validation::Fixture::open_by_id(&format!("parallel/{scenario}/np{np}/rank0"))
        .map_err(|e| e.to_string())?;
    let lin = fx
        .load_i64("partition/linearized")
        .map_err(|e| e.to_string())?;
    Ok(lin.data.iter().map(|&v| v as Idx).collect())
}

/// Distribution of `cube.msh` with the oracle's Scotch partition equals
/// the oracle's, rank by rank.
pub fn distribution_matches_oracle(comm: Communicator) -> Outcome {
    let np = comm.size();
    let scenario = "distribution_cube_msh";
    let mut m = read_on_root(&comm, "test_synchronizer/cube.msh", 3)?;
    if np > 1 {
        let parts = if comm.rank() == 0 {
            oracle_partition(scenario, np)?
        } else {
            Vec::new()
        };
        distribute_prescribed(&mut m, parts)?;
    } else {
        distribute_rcb(&mut m)?;
    }
    let fx = akantu_validation::Fixture::open_by_id(&format!(
        "parallel/{scenario}/np{np}/rank{}",
        comm.rank()
    ))
    .map_err(|e| e.to_string())?;
    compare_with_oracle(&m, &fx)
}

/// Bit patterns of a box (bitwise comparison, `-0.0 ≠ 0.0`; default NaNs
/// canonicalised, D-046 (3)).
fn box_bits(b: &BBox) -> (Vec<u64>, Vec<u64>) {
    (
        b.lower
            .iter()
            .map(|&v| akantu_validation::canonical_bits(v))
            .collect(),
        b.upper
            .iter()
            .map(|&v| akantu_validation::canonical_bits(v))
            .collect(),
    )
}

/// MESH-005, upstream `computeBoundingBox`,
/// `mesh.cc:415-430`: after the distribution of `cube.msh` (oracle
/// partition) every rank's **global** box is the serial mesh's box and the
/// oracle's (min/max over every oracle rank's `nodes/coordinates`), i.e.
/// the `all_reduce` Min/Max of the local boxes; the **local** box is the
/// box over this rank's nodes, pure ghosts included (upstream's pure-ghost
/// test is commented out), and equals the box over the oracle rank's
/// coordinates. Bitwise (min/max of copied coordinates).
pub fn bounding_boxes_match_oracle(comm: Communicator) -> Outcome {
    let np = comm.size();
    let rank = comm.rank();
    let scenario = "distribution_cube_msh";
    let mut m = read_on_root(&comm, "test_synchronizer/cube.msh", 3)?;
    if np > 1 {
        let parts = if rank == 0 {
            oracle_partition(scenario, np)?
        } else {
            Vec::new()
        };
        distribute_prescribed(&mut m, parts)?;
    } else {
        distribute_rcb(&mut m)?;
    }
    let serial = read_on_root(&Communicator::serial(), "test_synchronizer/cube.msh", 3)?;
    ensure!(
        box_bits(m.bbox()) == box_bits(serial.bbox()),
        "rank {rank}/{np}: global box {:?} != serial box {:?}",
        m.bbox(),
        serial.bbox()
    );
    let mut oracle_global = BBox::empty(3);
    let mut oracle_local = BBox::empty(3);
    for r in 0..np {
        let fx =
            akantu_validation::Fixture::open_by_id(&format!("parallel/{scenario}/np{np}/rank{r}"))
                .map_err(|e| e.to_string())?;
        let x = fx
            .load_f64("nodes/coordinates")
            .map_err(|e| e.to_string())?
            .data;
        for p in x.chunks(3) {
            oracle_global.add_point(p);
            if r == usize::try_from(rank).unwrap_or(usize::MAX) {
                oracle_local.add_point(p);
            }
        }
    }
    ensure!(
        box_bits(m.bbox()) == box_bits(&oracle_global),
        "rank {rank}/{np}: global box {:?} != oracle {:?}",
        m.bbox(),
        oracle_global
    );
    ensure!(
        box_bits(m.local_bbox()) == box_bits(&oracle_local),
        "rank {rank}/{np}: local box {:?} != oracle rank box {:?}",
        m.local_bbox(),
        oracle_local
    );
    let mut own = BBox::empty(3);
    let mut pure_ghosts = 0usize;
    for (n, x) in m.nodes().tuples().enumerate() {
        own.add_point(x);
        if ok(
            m.is_pure_ghost_node(ok(akantu_core::try_idx(n), "node index")?),
            "flag",
        )? {
            pure_ghosts += 1;
            ensure!(
                m.local_bbox().contains(x),
                "rank {rank}/{np}: pure ghost node {n} outside the local box"
            );
        }
    }
    ensure!(
        box_bits(m.local_bbox()) == box_bits(&own),
        "rank {rank}/{np}: local box is not the box of the rank's nodes"
    );
    ensure!(
        np == 1 || pure_ghosts > 0,
        "rank {rank}/{np}: no pure ghost node (the scenario must cover them)"
    );
    ensure!(m.is_distributed(), "rank {rank}/{np}: mesh not distributed");
    for d in 0..3 {
        ensure!(
            m.bbox().lower[d] <= m.local_bbox().lower[d]
                && m.local_bbox().upper[d] <= m.bbox().upper[d],
            "rank {rank}/{np}: local box outside the global box"
        );
    }
    Ok(())
}

/// Scotch (feature `scotch`, Docker image docker/scotch): the port's
/// `ScotchPartitioner` with the oracle's seed (1234) on `cube.msh` gives
/// the oracle's partition (`partition/linearized` of
/// `distribution_cube_msh` at every recorded part count above 1: 2, 4 and,
/// later also 8; both with `SCOTCH_PTHREAD_NUMBER=1`). Runs on
/// rank 0 of any communicator.
#[cfg(feature = "scotch")]
pub fn scotch_partition_matches_oracle(comm: Communicator) -> Outcome {
    use akantu_mesh::scotch::{ScotchPartitioner, scotch_num_bits, scotch_version};
    if comm.rank() != 0 {
        return Ok(());
    }
    let m = read_on_root(&Communicator::serial(), "test_synchronizer/cube.msh", 3)?;
    println!(
        "INFO scotch {} ({}-bit SCOTCH_Num), SCOTCH_PTHREAD_NUMBER={:?}",
        scotch_version(),
        scotch_num_bits(),
        std::env::var("SCOTCH_PTHREAD_NUMBER").ok()
    );
    let counts: Vec<usize> = oracle_rank_counts("distribution_cube_msh")
        .into_iter()
        .filter(|&n| n > 1)
        .collect();
    ensure!(
        counts.starts_with(&[2, 4]),
        "oracle partitions of distribution_cube_msh: {counts:?}"
    );
    for np in counts {
        let want = oracle_partition("distribution_cube_msh", np)?;
        let p = ok(ScotchPartitioner::new(1234).partition(&m, np), "scotch")?;
        // parity conditions (ARCHITECTURE): the
        // library version and thread count recorded with the oracle's
        // partition
        let fx = akantu_validation::Fixture::open_by_id(&format!(
            "parallel/distribution_cube_msh/np{np}/rank0"
        ))
        .map_err(|e| e.to_string())?;
        let params = &fx.manifest().parameters;
        let oracle_version = params["scotch/version"].as_str().unwrap_or("?");
        let oracle_threads = params["scotch/pthread_number"].as_str().unwrap_or("?");
        ensure!(
            p.metadata.kind
                == akantu_mesh::partition::PartitionerKind::Scotch {
                    version: oracle_version.to_owned()
                },
            "Scotch {:?} vs the oracle's {oracle_version}",
            p.metadata.kind
        );
        ensure!(
            p.metadata.pthread_number.as_deref() == Some(oracle_threads),
            "SCOTCH_PTHREAD_NUMBER {:?} vs the oracle's {oracle_threads}",
            p.metadata.pthread_number
        );
        ensure!(p.metadata.seed == params["scotch/seed"].as_i64(), "seed");
        let have = p.linearized();
        let same = have.iter().zip(&want).filter(|(a, b)| a == b).count();
        println!(
            "INFO scotch {np} parts: {same}/{} elements in the oracle's part, graph hash {:#x}",
            want.len(),
            p.metadata.graph_hash
        );
        ensure!(
            have == want,
            "Scotch {np}-part partition differs from the oracle's"
        );
    }
    Ok(())
}

/// UD-1060: ranks whose entry of upstream's per-rank pair vector is
/// relocated by a later `emplace_back` (0 .. 2^k − 1, 2^k the largest
/// power of two below the rank count): the pattern observed black-box with
/// the oracle's toolchain (g++ 14.2.0: capacities 1, 2, 4, 8, …;
/// D-030), not a property of the standard.
pub fn ud1060_relocated_ranks(np: usize) -> Vec<usize> {
    if np <= 1 {
        return Vec::new();
    }
    let mut top = 1;
    while 2 * top < np {
        top *= 2;
    }
    (0..top).collect()
}

/// Number of periodic slave nodes every rank will hold (local and ghost
/// elements of every type) under `partition` of the periodic root mesh.
pub fn slave_holders(
    m: &Mesh,
    partition: &akantu_mesh::partition::MeshPartition,
) -> std::result::Result<Vec<usize>, String> {
    let mut held: Vec<std::collections::BTreeSet<Idx>> =
        vec![std::collections::BTreeSet::new(); partition.nb_parts];
    for ty in m.element_types(TypeFilter::new()) {
        if !partition.has_partitions(ty) {
            continue;
        }
        let parts = ok(partition.partition(ty), "parts")?;
        let ghost = ok(partition.ghost_partition(ty), "ghost parts")?;
        let conn = ok(m.connectivity(ty, GhostType::NotGhost), "conn")?;
        for (e, nodes) in conn.tuples().enumerate() {
            let mut ranks = vec![parts[e] as usize];
            ranks.extend(ghost.row(e).iter().map(|&q| q as usize));
            for &n in nodes {
                if m.node_flag(n).is_periodic_slave() {
                    for &r in &ranks {
                        held[r].insert(n);
                    }
                }
            }
        }
    }
    Ok(held.iter().map(std::collections::BTreeSet::len).collect())
}

/// Distribution of a periodic mesh (upstream
/// `NodeInfoPerProc::synchronizePeriodicity`, `fillPeriodicPairs`,
/// `receiveMissingPeriodic`): a 2-D quadrangle grid made periodic along x on
/// the root, distributed by bisection. On every rank: each pair whose slave
/// is local is present with the root's global ids; a partner that is not
/// local was appended as a pure-ghost node with the partner's coordinates,
/// owned by the partner's owner and received from it; periodic flags set;
/// node schemes symmetric across ranks (by global id). At 2 and 4 ranks the
/// partition violates UD-1060's precondition (a relocated rank holds
/// periodic slaves; asserted): upstream's distribution is undefined there
/// and this test is the port's no-oracle evidence.
pub fn periodic_distribution_invariants(comm: Communicator) -> Outcome {
    use akantu_core::enums::SpatialDirection;
    let n = 8usize;
    let mut m = ok(Mesh::with_communicator(2, "per", comm.clone()), "mesh")?;
    let mut root_pairs: Vec<(i64, i64)> = Vec::new();
    if comm.rank() == 0 {
        {
            let mut a = akantu_mesh::MeshAccessor::new(&mut m);
            for j in 0..=n {
                for i in 0..=n {
                    ok(a.push_node(&[i as Real, j as Real]), "node")?;
                }
            }
            let w = (n + 1) as Idx;
            let mut c =
                a.connectivity_mut(akantu_core::ElementType::Quadrangle4, GhostType::NotGhost);
            for j in 0..n as Idx {
                for i in 0..n as Idx {
                    let n0 = j * w + i;
                    c.push_tuple(&[n0, n0 + 1, n0 + w + 1, n0 + w]);
                }
            }
        }
        ok(m.compute_bounding_box(), "bbox")?;
        let _ = ok(
            akantu_mesh::periodic::make_periodic(&mut m, SpatialDirection::X, None),
            "make_periodic",
        )?;
        // periodic_pairs lists (master, slave)
        root_pairs = akantu_mesh::periodic::periodic_pairs(&m)
            .into_iter()
            .map(|(mm, s)| (i64::from(s), i64::from(mm)))
            .collect();
    }
    let coords_root = m.nodes().as_slice().to_vec();
    let all_pairs = ok(
        comm.broadcast_vec(
            &root_pairs
                .iter()
                .flat_map(|&(s, mm)| [s, mm])
                .collect::<Vec<_>>(),
            0,
        ),
        "bcast pairs",
    )?;
    let coords_root = ok(comm.broadcast_vec(&coords_root, 0), "bcast coords")?;
    // The UD-1060 precondition of this partition (the
    // RCB partition distribute_rcb computes on the root): violated means
    // upstream's synchronizePeriodicity is undefined on it, so this test is
    // the no-oracle evidence for that class
    let mut held_violated = vec![0i64; 2];
    if comm.rank() == 0 && comm.size() > 1 {
        let part = ok(RcbPartitioner.partition(&m, comm.size()), "rcb")?;
        let held = slave_holders(&m, &part)?;
        let reloc = ud1060_relocated_ranks(comm.size());
        held_violated = vec![
            reloc.iter().map(|&r| held[r] as i64).sum(),
            i64::from(reloc.iter().any(|&r| held[r] > 0)),
        ];
        println!(
            "INFO periodic invariants np {}: slave holders {held:?}, relocated ranks {reloc:?}",
            comm.size()
        );
    }
    let held_violated = ok(comm.broadcast_vec(&held_violated, 0), "bcast precondition")?;
    if matches!(comm.size(), 2 | 4) {
        // rank 0 (2 ranks) and ranks 0, 1 (4 ranks) hold slave nodes: the
        // class on which upstream is undefined (measured: 9 / 6 + 6)
        ensure!(
            held_violated[1] == 1 && held_violated[0] > 0,
            "np {}: the RCB partition satisfies the UD-1060 precondition ({} slaves on the \
             relocated ranks): the test no longer covers upstream's undefined class",
            comm.size(),
            held_violated[0]
        );
    }
    distribute_rcb(&mut m)?;
    if comm.size() == 1 {
        ensure!(m.is_periodic(), "one rank: mesh stays periodic");
        return Ok(());
    }
    ensure!(
        m.is_periodic(),
        "rank {}: distributed mesh lost its periodicity",
        comm.rank()
    );
    let pairs: std::collections::HashMap<i64, i64> =
        all_pairs.chunks(2).map(|p| (p[0], p[1])).collect();
    for l in 0..m.nb_nodes() {
        let l = l as Idx;
        let g = m.node_global_id(l);
        let f = m.node_flag(l);
        let x = m.nodes();
        let x = x.tuple(l as usize);
        ensure!(
            x == &coords_root[2 * g as usize..2 * g as usize + 2],
            "node {l} (global {g}) has wrong coordinates"
        );
        if f.is_periodic_slave() {
            let master = m
                .periodic_master(l)
                .ok_or(format!("slave {l} without master"))?;
            ensure!(
                pairs.get(&g) == Some(&m.node_global_id(master)),
                "pair of global {g}: master {} vs root {:?}",
                m.node_global_id(master),
                pairs.get(&g)
            );
            ensure!(
                m.node_flag(master).is_periodic_master(),
                "master {master} flag"
            );
        }
    }
    // node schemes symmetric by global ids
    let ns = m.synchronizers().node.as_ref().ok_or("no node sync")?;
    let mut mine: Vec<i64> = Vec::new();
    for (dir, sch) in [
        (0i64, &ns.communications.send),
        (1, &ns.communications.recv),
    ] {
        for (&q, nodes) in sch {
            mine.push(dir);
            mine.push(i64::from(q));
            mine.push(nodes.len() as i64);
            mine.extend(nodes.iter().map(|&n| m.node_global_id(n)));
        }
    }
    let all = ok(comm.all_gather_v(&mine), "gather schemes")?;
    let parse = |p: usize, v: &[i64]| {
        let mut out = std::collections::BTreeMap::new();
        let mut i = 0;
        while i < v.len() {
            let (dir, q, len) = (v[i], v[i + 1] as usize, v[i + 2] as usize);
            let key = if dir == 0 { (p, q) } else { (q, p) };
            out.insert((dir, key), v[i + 3..i + 3 + len].to_vec());
            i += 3 + len;
        }
        out
    };
    let mut send = std::collections::BTreeMap::new();
    let mut recv = std::collections::BTreeMap::new();
    for (p, v) in all.iter().enumerate() {
        for ((dir, key), list) in parse(p, v) {
            if dir == 0 {
                send.insert(key, list);
            } else {
                recv.insert(key, list);
            }
        }
    }
    ensure!(
        send == recv,
        "node schemes are not symmetric with periodic missing nodes"
    );
    Ok(())
}

// ------------------------------------------------- (UD-231)

/// The adversarial 2-D mesh (`oracle/probes/parallel/adv2d.msh`,
/// written by `make_adv2d.py`): interleaved Quad4/Tri3 cells, Seg2 groups
/// with an interior crack, Point1 groups `pts` (on segments) and `orphan`
/// (node 18 = (4, 2), on no segment).
pub fn adv2d_on_root(comm: &Communicator) -> std::result::Result<Mesh, String> {
    let mut m = ok(Mesh::with_communicator(2, "adv2d", comm.clone()), "mesh")?;
    if comm.rank() == 0 {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../oracle/probes/parallel/adv2d.msh");
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

/// The cell rule of the probe's `adv2d_part`, linearised over the
/// elements of dimension 2 (types in enum order): cell (i, j) of the
/// barycentre; i ≤ 1 → 0, i = 2 → 1, i = 3 → (j = 0 ? 1 : 2), i = 4 → 2,
/// i = 5 → 2 (Quad4) / 3 (Tri3); modulo `np`.
pub fn adv2d_parts(m: &Mesh, np: usize) -> std::result::Result<Vec<Idx>, String> {
    let mut out = Vec::new();
    let types: Vec<akantu_core::ElementType> = m.element_types(TypeFilter::new().dim(2)).collect();
    for ty in types {
        for e in 0..m.nb_element(ty, GhostType::NotGhost) {
            let b = ok(
                m.barycenter(Element::new(ty, e as Idx, GhostType::NotGhost)),
                "barycenter",
            )?;
            let (i, j) = (b[0].floor() as i64, b[1].floor() as i64);
            let p: i64 = match i {
                ..=1 => 0,
                2 => 1,
                3 if j == 0 => 1,
                3 | 4 => 2,
                _ if ty == akantu_core::ElementType::Quadrangle4 => 2,
                _ => 3,
            };
            out.push((p % np as i64) as Idx);
        }
    }
    Ok(out)
}

/// UD-231 compat switch on.
pub fn upstream_orphans() -> akantu_mesh::partition::PartitionCompat {
    akantu_mesh::partition::PartitionCompat {
        orphan_lower_dimensional_in_part_zero: true,
    }
}

/// `adv2d` distributed with the cell rule at `comm.size()` parts (1 rank:
/// bisection into one part), with or without upstream's orphan placement.
pub fn adv2d_distributed(comm: &Communicator, upstream: bool) -> std::result::Result<Mesh, String> {
    let mut m = adv2d_on_root(comm)?;
    if comm.size() == 1 {
        distribute_rcb(&mut m)?;
        return Ok(m);
    }
    let parts = if comm.rank() == 0 {
        adv2d_parts(&m, comm.size())?
    } else {
        Vec::new()
    };
    let mut p = PrescribedPartition::new(parts);
    if upstream {
        p = p.with_compat(upstream_orphans());
    }
    let _ = ok(
        distribute_with(&mut m, Some(&mut p as &mut dyn MeshPartitioner), 0),
        "distribute",
    )?;
    Ok(m)
}

/// Owned local nodes (normal or master), summed over the ranks.
pub fn owned_node_entries(comm: &Communicator, m: &Mesh) -> std::result::Result<u64, String> {
    let mut n = [m
        .nodes_flags()
        .as_slice()
        .iter()
        .filter(|f| f.is_local_or_master())
        .count() as u64];
    ok(
        comm.all_reduce(&mut n, akantu_comm::ReduceOp::Sum),
        "all_reduce",
    )?;
    Ok(n[0])
}

/// UD-231: default rule: the orphan point (node 18,
/// interior to part 2 at 4 parts) is placed with its node's owner — every
/// global node has exactly one owner (28 owned entries for 28 nodes; on
/// the earlier code and upstream: 29 at 4 ranks, node 18 `_normal` on ranks 0 and
/// 2), the orphan point is local on exactly one rank, which owns its node,
/// and the node-entity gather is complete.
pub fn adv2d_orphan_point_has_one_owner(comm: Communicator) -> Outcome {
    let m = adv2d_distributed(&comm, false)?;
    let nb_global = m.nb_global_nodes();
    ensure!(nb_global == 28, "adv2d has 28 nodes, got {nb_global}");
    let owned = owned_node_entries(&comm, &m)?;
    ensure!(
        owned == nb_global as u64,
        "{owned} owned node entries for {nb_global} global nodes (a node with two owners)"
    );
    // the orphan point: local on one rank, which owns its node 18
    let mut here = 0u64;
    for g in m.groups().element_groups() {
        if g.name() != "orphan" {
            continue;
        }
        let local = g.elements_of(akantu_core::ElementType::Point1, GhostType::NotGhost);
        here = local.len() as u64;
        for &e in &local {
            let conn = ok(
                m.connectivity(akantu_core::ElementType::Point1, GhostType::NotGhost),
                "conn",
            )?;
            let node = conn.tuple(e as usize)[0];
            ensure!(
                m.node_global_id(node) == 18,
                "orphan point on global node {}",
                m.node_global_id(node)
            );
            ensure!(
                m.node_flag(node).is_local_or_master(),
                "rank {} holds the orphan point but does not own node 18 (flag {:#x})",
                comm.rank(),
                m.node_flag(node).bits()
            );
            if comm.size() == 4 {
                ensure!(
                    comm.rank() == 2,
                    "the orphan point belongs to part 2 at 4 parts"
                );
            }
        }
    }
    let mut total = [here];
    ok(
        comm.all_reduce(&mut total, akantu_comm::ReduceOp::Sum),
        "all_reduce",
    )?;
    ensure!(
        total[0] == 1,
        "the orphan point is local on {} ranks",
        total[0]
    );
    // a gather of one value per node fills every global slot once
    if let Some(ns) = &m.synchronizers().node {
        let e = NodeEntities { mesh: &m };
        let vals: Vec<Real> = (0..m.nb_nodes())
            .map(|n| m.node_global_id(n as Idx) as Real)
            .collect();
        let g = ok(ns.gather(&e, &vals, 1), "gather")?;
        if let Some(g) = g {
            ensure!(g.len() == nb_global, "gathered {} values", g.len());
            for (i, v) in g.iter().enumerate() {
                ensure!(*v == i as Real, "slot {i} holds {v}");
            }
        }
    }
    Ok(())
}

/// Compat mode: `PartitionCompat::
/// orphan_lower_dimensional_in_part_zero` reproduces upstream's placement
/// (the orphan point in part 0) and with it the duplicated owner at 4
/// ranks (29 owned entries), as the oracle fixture records; at 2 ranks
/// node 18 is in part 0 anyway (28).
pub fn adv2d_compat_reproduces_upstream_owners(comm: Communicator) -> Outcome {
    let m = adv2d_distributed(&comm, true)?;
    let owned = owned_node_entries(&comm, &m)?;
    let want = if comm.size() == 4 { 29 } else { 28 };
    ensure!(
        owned == want,
        "compat: {owned} owned entries, upstream has {want}"
    );
    // the checked gather refuses the duplicated owner (on every rank)
    if comm.size() == 4 {
        let ns = m
            .synchronizers()
            .node
            .as_ref()
            .ok_or("no node synchronizer")?;
        let e = NodeEntities { mesh: &m };
        let vals = vec![0.0 as Real; m.nb_nodes()];
        match ns.gather(&e, &vals, 1) {
            Ok(_) => return Err("a gather over a duplicated owner must be refused".into()),
            Err(err) => ensure!(
                comm.rank() != 0
                    || err
                        .to_string()
                        .contains("global id 18 owned by ranks 0 and 2"),
                "unexpected error: {err}"
            ),
        }
    }
    Ok(())
}

/// The oracle's distribution of `adv2d` (fixtures
/// `parallel/distribution_adv2d/np{1,2,4}`) — the port with the oracle's
/// partition (checked equal to the cell rule) and the UD-231 compat switch
/// equals the oracle rank by rank; the oracle itself has 29 owned node
/// entries at 4 ranks (node 18 owned by ranks 0 and 2).
pub fn adv2d_matches_oracle_in_compat(comm: Communicator) -> Outcome {
    let np = comm.size();
    let scenario = "distribution_adv2d";
    if np > 1 && comm.rank() == 0 {
        let probe = adv2d_on_root(&comm)?;
        ensure!(
            oracle_partition(scenario, np)? == adv2d_parts(&probe, np)?,
            "the oracle's partition is the cell rule"
        );
    }
    let m = adv2d_distributed(&comm, true)?;
    let fx = akantu_validation::Fixture::open_by_id(&format!(
        "parallel/{scenario}/np{np}/rank{}",
        comm.rank()
    ))
    .map_err(|e| e.to_string())?;
    compare_with_oracle(&m, &fx)?;
    let flags = fx.load_i64("nodes/flags").map_err(|e| e.to_string())?;
    let mut oracle_owned = [flags
        .data
        .iter()
        .filter(|&&f| NodeFlag(f as u8).is_local_or_master())
        .count() as u64];
    ok(
        comm.all_reduce(&mut oracle_owned, akantu_comm::ReduceOp::Sum),
        "all_reduce",
    )?;
    let want = if np == 4 { 29 } else { 28 };
    ensure!(
        oracle_owned[0] == want,
        "oracle: {} owned node entries, expected {want} (UD-231)",
        oracle_owned[0]
    );
    Ok(())
}

// -------------------------------------------------------

/// Upstream `GroupManager::synchronizeGroupNames`,
/// called by `distributeMeshCentralized`): group names that exist only on
/// non-root ranks before distribution are known on every rank afterwards
/// (empty there); on a name conflict the root keeps its definition and a
/// non-root rank keeps its own (`checkAndAddGroups` adds missing names
/// only). Before the fix a group created on rank 1 stayed on rank 1 only.
pub fn group_names_of_non_root_ranks_are_shared(comm: Communicator) -> Outcome {
    let size = comm.size();
    let mut m = read_on_root(&comm, "test_synchronizer/cube.msh", 3)?;
    let last = size as Rank - 1;
    // rank r > 0 creates "only_on_<r>" (node group + a 1-D element group
    // on it); the last rank also redefines a root group name with another
    // dimension
    if comm.rank() > 0 {
        let r = comm.rank();
        let g = m.groups_mut();
        ok(
            g.create_node_group(&format!("only_on_{r}_nodes")),
            "node group",
        )?;
        ok(
            g.create_element_group_on_node_group(
                &format!("only_on_{r}"),
                1,
                &format!("only_on_{r}_nodes"),
            ),
            "element group",
        )?;
    }
    let root_groups: Vec<String> = if comm.rank() == 0 {
        m.groups()
            .element_groups()
            .map(|g| g.name().to_owned())
            .collect()
    } else {
        Vec::new()
    };
    let conflict = "conflict";
    if comm.rank() == 0 {
        let g = m.groups_mut();
        ok(g.create_node_group("conflict_root_nodes"), "node group")?;
        ok(
            g.create_element_group_on_node_group(conflict, 3, "conflict_root_nodes"),
            "element group",
        )?;
    } else if comm.rank() == last {
        let g = m.groups_mut();
        ok(g.create_node_group("conflict_last_nodes"), "node group")?;
        ok(
            g.create_element_group_on_node_group(conflict, 0, "conflict_last_nodes"),
            "element group",
        )?;
    }
    distribute_rcb(&mut m)?;
    let groups = m.groups();
    for r in 1..size {
        let eg = format!("only_on_{r}");
        ensure!(
            groups.element_group_exists(&eg) && groups.node_group_exists(&format!("{eg}_nodes")),
            "rank {}: group {eg} of rank {r} missing after distribution",
            comm.rank()
        );
        let g = ok(groups.element_group(&eg), "group")?;
        ensure!(g.group().dimension == 1, "{eg}: dimension");
        ensure!(
            g.node_group.name() == format!("{eg}_nodes"),
            "{eg}: node group"
        );
        ensure!(
            GhostType::ALL.into_iter().all(|gt| g
                .group()
                .element_types(gt)
                .into_iter()
                .all(|t| g.elements_of(t, gt).is_empty())),
            "{eg}: must stay empty"
        );
    }
    let c = ok(groups.element_group(conflict), "conflict group")?;
    let (dim, ng) = if size > 1 && comm.rank() == last {
        (0, "conflict_last_nodes")
    } else {
        (3, "conflict_root_nodes")
    };
    ensure!(
        c.group().dimension == dim && c.node_group.name() == ng,
        "rank {}: conflict group defined as ({}, {}), expected ({dim}, {ng})",
        comm.rank(),
        c.group().dimension,
        c.node_group.name()
    );
    // every rank knows the same names
    let names: Vec<String> = groups
        .element_groups()
        .map(|g| g.name().to_owned())
        .collect();
    if comm.rank() == 0 {
        for n in &root_groups {
            ensure!(names.contains(n), "root group {n} lost");
        }
    }
    let mut count = [names.len() as u64];
    let mut min = count;
    ok(
        comm.all_reduce(&mut count, akantu_comm::ReduceOp::Max),
        "all_reduce",
    )?;
    ok(
        comm.all_reduce(&mut min, akantu_comm::ReduceOp::Min),
        "all_reduce",
    )?;
    ensure!(count == min, "element group counts differ across ranks");
    Ok(())
}

/// Partitions computed by several
/// threads at once (local-world ranks, library users) equal the
/// single-threaded partition — Scotch's process-global context is
/// serialised by the port. Race-dependent without the lock (not a
/// deterministic reproduction of the defect).
#[cfg(feature = "scotch")]
pub fn scotch_concurrent_partitions_are_reproducible(comm: Communicator) -> Outcome {
    use akantu_mesh::scotch::ScotchPartitioner;
    if comm.rank() != 0 {
        return Ok(());
    }
    let m = read_on_root(&Communicator::serial(), "test_synchronizer/cube.msh", 3)?;
    let want = ok(ScotchPartitioner::new(1234).partition(&m, 4), "scotch")?.linearized();
    for round in 0..3 {
        let results: Vec<std::result::Result<Vec<Idx>, String>> = std::thread::scope(|s| {
            let hs: Vec<_> = (0..4)
                .map(|_| {
                    s.spawn(|| {
                        ok(ScotchPartitioner::new(1234).partition(&m, 4), "scotch")
                            .map(|p| p.linearized())
                    })
                })
                .collect();
            hs.into_iter()
                .map(|h| h.join().unwrap_or_else(|_| Err("thread panicked".into())))
                .collect()
        });
        for (t, r) in results.into_iter().enumerate() {
            ensure!(r? == want, "round {round}, thread {t}: partition differs");
        }
    }
    Ok(())
}

/// UD-235: the distribution preconditions the port enforces
/// beyond upstream are agreed recoverable refusals on every rank — a root
/// mesh with a facet mesh, mesh data on a non-root rank — and the meshes
/// and the communicator stay usable (a plain distribution follows).
pub fn distribution_preconditions_refused(comm: Communicator) -> Outcome {
    if comm.size() == 1 {
        return Ok(());
    }
    let last = comm.size() as Rank - 1;
    // (1) the root has a facet mesh
    let mut m = read_on_root(&comm, "test_synchronizer/cube.msh", 3)?;
    if comm.rank() == 0 {
        let _ = ok(akantu_mesh::utils::init_mesh_facets(&mut m), "facets")?;
    }
    let mut p = RcbPartitioner;
    let r = distribute_with(&mut m, Some(&mut p as &mut dyn MeshPartitioner), 0);
    ensure!(r.is_err(), "a root facet mesh must be refused");
    let e = r.err().map(|e| e.to_string()).unwrap_or_default();
    ensure!(
        comm.rank() != 0 || e.contains("facet mesh"),
        "root error: {e}"
    );
    ensure!(!m.is_distributed(), "the mesh is unchanged");
    // (2) mesh data on the last rank
    let mut m = read_on_root(&comm, "test_synchronizer/cube.msh", 3)?;
    if comm.rank() == last {
        let _ = ok(m.data_mut().ints_mut_or_insert("stray"), "data")?;
    }
    let r = distribute_with(&mut m, Some(&mut p as &mut dyn MeshPartitioner), 0);
    ensure!(r.is_err(), "mesh data on a non-root rank must be refused");
    let e = r.err().map(|e| e.to_string()).unwrap_or_default();
    ensure!(
        comm.rank() != last || e.contains("holds mesh data"),
        "rank {last} error: {e}"
    );
    // (3) the communicator is still usable
    let mut m = read_on_root(&comm, "test_synchronizer/cube.msh", 3)?;
    distribute_rcb(&mut m)?;
    ensure!(m.is_distributed(), "distributed afterwards");
    Ok(())
}

/* ---------------------------------------------------------------------- */
/* Parallel probe v2                                                       */
/* ---------------------------------------------------------------------- */

/// The oracle's element triple `(type code, element, ghost)`; null elements
/// `(-1, -1, -1)`.
fn element_triple(e: &Element, table: &[String]) -> std::result::Result<[i64; 3], String> {
    if e.is_null() {
        return Ok([-1, -1, -1]);
    }
    let code = table
        .iter()
        .position(|t| t == e.ty().name())
        .ok_or_else(|| format!("type {} not in the oracle table", e.ty()))?;
    Ok([
        code as i64,
        i64::from(e.index()),
        i64::from(e.ghost_type() == GhostType::Ghost),
    ])
}

fn param_strings(fx: &akantu_validation::Fixture, key: &str) -> Vec<String> {
    fx.manifest().parameters[key]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

fn block_keys(m: &Mesh) -> Vec<(String, akantu_core::ElementType, GhostType)> {
    let mut out = Vec::new();
    for ghost in GhostType::ALL {
        for ty in m.element_types(TypeFilter::new().ghost(ghost)) {
            let g = if ghost == GhostType::NotGhost {
                "not_ghost"
            } else {
                "ghost"
            };
            out.push((format!("{}/{g}", ty.name()), ty, ghost));
        }
    }
    out
}

/// Every rank's `physical_names`, `element_to_subelement` /
/// `subelement_to_element` (recomputed after the distribution, upstream
/// `fillElementToSubElementsData`) equal the oracle's, block by block.
pub fn compare_mesh_data_g20(m: &Mesh, fx: &akantu_validation::Fixture) -> Outcome {
    let p = &fx.manifest().parameters;
    let table = param_strings(fx, "type_table");
    let li = |name: &str| fx.load_i64(name).map_err(|e| format!("{name}: {e}"));
    let blocks = block_keys(m);
    // physical names
    let mut names_blocks = Vec::new();
    if let Ok(d) = m.data().strings("physical_names") {
        for (key, ty, ghost) in &blocks {
            if let Ok(a) = d.get(*ty, *ghost) {
                names_blocks.push(key.clone());
                let want = param_strings(fx, &format!("physical_names/{key}"));
                ensure!(
                    a.as_slice() == want.as_slice(),
                    "physical_names {key}: {:?} vs oracle {want:?}",
                    a.as_slice()
                );
            }
        }
    }
    ensure!(
        names_blocks == param_strings(fx, "physical_names_blocks"),
        "physical_names blocks {names_blocks:?} vs oracle {:?}",
        param_strings(fx, "physical_names_blocks")
    );
    // element_to_subelement
    let mut e2s_blocks = Vec::new();
    if let Ok(d) = m.data().element_lists("element_to_subelement") {
        for (key, ty, ghost) in &blocks {
            if let Ok(a) = d.get(*ty, *ghost) {
                e2s_blocks.push(key.clone());
                let mut counts = Vec::new();
                let mut flat = Vec::new();
                for list in a.as_slice() {
                    counts.push(list.len() as i64);
                    for e in list {
                        flat.extend(element_triple(e, &table)?);
                    }
                }
                ensure!(
                    counts == li(&format!("element_to_subelement/{key}/counts"))?.data,
                    "element_to_subelement {key}: counts differ"
                );
                ensure!(
                    flat == li(&format!("element_to_subelement/{key}/elements"))?.data,
                    "element_to_subelement {key}: elements differ"
                );
            }
        }
    }
    ensure!(
        e2s_blocks == param_strings(fx, "element_to_subelement_blocks"),
        "element_to_subelement blocks {e2s_blocks:?} vs oracle {:?}",
        param_strings(fx, "element_to_subelement_blocks")
    );
    // subelement_to_element
    let mut s2e_blocks = Vec::new();
    if let Ok(d) = m.data().element_map("subelement_to_element") {
        for (key, ty, ghost) in &blocks {
            if let Ok(a) = d.get(*ty, *ghost) {
                s2e_blocks.push(key.clone());
                ensure!(
                    p[format!("subelement_to_element/{key}/nb_component")].as_u64()
                        == Some(a.nb_component() as u64),
                    "subelement_to_element {key}: nb_component {}",
                    a.nb_component()
                );
                let mut flat = Vec::new();
                for e in a.as_slice() {
                    flat.extend(element_triple(e, &table)?);
                }
                ensure!(
                    flat == li(&format!("subelement_to_element/{key}/elements"))?.data,
                    "subelement_to_element {key}: elements differ"
                );
            }
        }
    }
    ensure!(
        s2e_blocks == param_strings(fx, "subelement_to_element_blocks"),
        "subelement_to_element blocks {s2e_blocks:?} vs oracle {:?}",
        param_strings(fx, "subelement_to_element_blocks")
    );
    Ok(())
}

/// The probe's `distribution_square_data` set-up on rank 0: frozen
/// `test_mesh_periodic/square.msh`, nodal data `shared` (Real, upstream
/// `TestMeshPeriodic::SetUp`) and `node_tag` (Int, 2 components).
fn square_with_nodal_data(comm: &Communicator) -> std::result::Result<Mesh, String> {
    let mut m = read_on_root(comm, "test_mesh_utils/test_mesh_periodic/square.msh", 2)?;
    if comm.rank() == 0 {
        let n = m.nb_nodes();
        let mut shared = akantu_core::Array::<Real>::zeros(n, 1);
        for g in ["x-minus", "x-plus", "y-minus", "y-plus"] {
            let nodes = ok(m.groups().element_group(g), g)?.nodes().to_vec();
            for node in nodes {
                shared.as_mut_slice()[node as usize] += 1.0;
            }
        }
        let mut tag = akantu_core::Array::<akantu_core::Int>::zeros(n, 2);
        for i in 0..n {
            let ii = i as akantu_core::Int;
            tag.as_mut_slice()[2 * i] = (7 * ii) % 11;
            tag.as_mut_slice()[2 * i + 1] = -ii;
        }
        m.data_mut()
            .insert_nodal("shared", NodalDataValues::Real(shared));
        m.data_mut()
            .insert_nodal("node_tag", NodalDataValues::Int(tag));
    }
    Ok(m)
}

fn distribute_like_oracle(
    comm: &Communicator,
    m: &mut Mesh,
    scenario: &str,
) -> std::result::Result<akantu_validation::Fixture, String> {
    let np = comm.size();
    if np > 1 {
        let parts = if comm.rank() == 0 {
            oracle_partition(scenario, np)?
        } else {
            Vec::new()
        };
        distribute_prescribed(m, parts)?;
    } else {
        distribute_rcb(m)?;
    }
    akantu_validation::Fixture::open_by_id(&format!(
        "parallel/{scenario}/np{np}/rank{}",
        comm.rank()
    ))
    .map_err(|e| e.to_string())
}

/// MUTIL-010, MESH-030: `square.msh` with nodal data distributed
/// with the oracle's Scotch partition equals the oracle rank by rank — the
/// mesh, synchronizers and groups ([`compare_with_oracle`]), the
/// `physical_names`, the recomputed adjacency and the nodal data in local
/// order (bitwise).
pub fn square_data_matches_oracle(comm: Communicator) -> Outcome {
    let mut m = square_with_nodal_data(&comm)?;
    let fx = distribute_like_oracle(&comm, &mut m, "distribution_square_data")?;
    compare_with_oracle(&m, &fx)?;
    compare_mesh_data_g20(&m, &fx)?;
    let shared = match ok(m.data().nodal("shared"), "shared")? {
        NodalDataValues::Real(a) => a.as_slice().to_vec(),
        other => return Err(format!("shared: {other:?}")),
    };
    let want = fx
        .load_f64("nodal_data/shared")
        .map_err(|e| e.to_string())?
        .data;
    ensure!(
        shared.len() == want.len()
            && shared
                .iter()
                .zip(&want)
                .all(|(a, b)| a.to_bits() == b.to_bits()),
        "nodal data shared differs"
    );
    let tag: Vec<i64> = match ok(m.data().nodal("node_tag"), "node_tag")? {
        NodalDataValues::Int(a) => a.as_slice().iter().map(|&v| i64::from(v)).collect(),
        other => return Err(format!("node_tag: {other:?}")),
    };
    ensure!(
        tag == fx
            .load_i64("nodal_data/node_tag")
            .map_err(|e| e.to_string())?
            .data,
        "nodal data node_tag differs"
    );
    Ok(())
}

// ------------------------------------------------- MPI-016 element removal

/// MPI-016: element removals of a
/// distributed mesh through the mesh's distributed removal hook
/// [`akantu_mesh::distributed_removal::DistributedRemovalHook`] (upstream
/// `ElementSynchronizer::onElementsRemoved`: `filterScheme`, then
/// `renumberElements`). Every rank removes, as local and as ghost elements,
/// the tetrahedra whose barycentre has `x < 0.35` (a global criterion, so
/// the removal is consistent). Afterwards no scheme names a removed
/// element (by barycentre), every scheme index is inside its block, the
/// schemes lost exactly the removed entries, and a barycentre exchange is
/// bitwise again (every received ghost matches its owner's element).
pub fn element_removal_updates_the_element_synchronizer(comm: Communicator) -> Outcome {
    element_removal_scenario(&comm, false)
}

/// The same removal with the orphaned nodes removed in the same mutation
/// (`MeshTransaction::purify` step 2; the removed hook
/// `ElementRemovalSynchronizerUpdate` refused this): the element schemes
/// as above, and the node schemes follow the node renumbering — nodes were
/// removed, and a node exchange gives every slave its master's global id
/// again (`check_node_data`).
pub fn element_removal_with_purify_updates_both_synchronizers(comm: Communicator) -> Outcome {
    element_removal_scenario(&comm, true)
}

fn element_removal_scenario(comm: &Communicator, purify: bool) -> Outcome {
    use akantu_mesh::distributed_removal::DistributedRemovalHook;
    use akantu_mesh::{MeshPart, SharedMesh};
    const X_CUT: Real = 0.35;
    let m = distributed_cube(comm)?;
    let nb_nodes_before = m.nb_nodes();
    let dim = m.spatial_dimension();
    let b = barycenters(&m)?;
    let sync = m
        .synchronizers()
        .element
        .clone()
        .ok_or("no element synchronizer")?;
    let removed_x = |el: &Element| -> std::result::Result<bool, String> {
        let a = ok(b.get(el.ty(), el.ghost_type()), "barycentre block")?;
        Ok(a.as_slice()[el.idx() * dim] < X_CUT)
    };
    let mut list = Vec::new();
    for ghost in GhostType::ALL {
        for ty in m.element_types(TypeFilter::new().dim(dim).ghost(ghost)) {
            for e in 0..m.nb_element(ty, ghost) {
                let el = Element::new(ty, Idx::try_from(e).map_err(|e| e.to_string())?, ghost);
                if removed_x(&el)? {
                    list.push(el);
                }
            }
        }
    }
    let mut kept_entries = 0;
    let mut removed_entries = 0;
    for scheme in sync
        .communications
        .send
        .values()
        .chain(sync.communications.recv.values())
    {
        for el in scheme {
            if removed_x(el)? {
                removed_entries += 1;
            } else {
                kept_entries += 1;
            }
        }
    }
    let shared = SharedMesh::new(m);
    ok(
        shared.mutate(
            &mut [],
            |tx| {
                tx.erase_elements(MeshPart::Main, &list)?;
                if purify {
                    tx.purify(MeshPart::Main)?;
                }
                Ok(())
            },
            Some(&mut DistributedRemovalHook::new()),
        ),
        "mutate",
    )?;
    let m = ok(shared.read(), "read")?;
    let b2 = barycenters(&m)?;
    let sync = m
        .synchronizers()
        .element
        .clone()
        .ok_or("no element synchronizer after the removal")?;
    let mut after = 0;
    for scheme in sync
        .communications
        .send
        .values()
        .chain(sync.communications.recv.values())
    {
        for el in scheme {
            let n = m.nb_element(el.ty(), el.ghost_type());
            ensure!(el.idx() < n, "{el} outside its block ({n} elements)");
            let a = ok(b2.get(el.ty(), el.ghost_type()), "barycentre block")?;
            ensure!(
                a.as_slice()[el.idx() * dim] >= X_CUT,
                "removed element {el} still in a scheme"
            );
            after += 1;
        }
    }
    ensure!(
        after == kept_entries,
        "{after} scheme entries after the removal, {kept_entries} expected \
         ({removed_entries} removed)"
    );
    if comm.size() > 1 {
        let mut total = [removed_entries as u64];
        ok(
            comm.all_reduce(&mut total, akantu_comm::ReduceOp::Sum),
            "reduce",
        )?;
        ensure!(total[0] > 0, "the cut removes no shared element");
    }
    let nb_recv: usize = sync.communications.recv.values().map(Vec::len).sum();
    let mut acc = BarycenterAccessor {
        dim,
        bary: &b2,
        max_dist: 0.0,
        unpacked: 0,
    };
    ok(
        sync.synchronize(&mut acc, SynchronizationTag::Test),
        "synchronize",
    )?;
    ensure!(
        acc.max_dist == 0.0 && acc.unpacked == nb_recv,
        "barycentres after the removal: distance {}, unpacked {} of {nb_recv}",
        acc.max_dist,
        acc.unpacked
    );
    if purify {
        let removed_nodes = nb_nodes_before - m.nb_nodes();
        let mut total = [removed_nodes as u64];
        ok(
            comm.all_reduce(&mut total, akantu_comm::ReduceOp::Sum),
            "reduce",
        )?;
        ensure!(total[0] > 0, "the purify removes no node");
        if let Some(node_sync) = m.synchronizers().node.clone() {
            for scheme in node_sync
                .communications
                .send
                .values()
                .chain(node_sync.communications.recv.values())
            {
                for &n in scheme {
                    ensure!(
                        usize::try_from(n).is_ok_and(|n| n < m.nb_nodes()),
                        "node {n} of a node scheme outside the {} nodes",
                        m.nb_nodes()
                    );
                }
            }
            let mut d = node_data(&m);
            ok(
                node_sync.synchronize(&mut NodeAccessor(&mut d), SynchronizationTag::Test),
                "node synchronize",
            )?;
            check_node_data(&m, &d)?;
        }
    }
    Ok(())
}

/// MPI-016, the `onElementsChanged` half (step 3; upstream
/// `ElementSynchronizer::onElementsChanged` → `substituteElements`,
/// `element_synchronizer.cc:97-113`): on every rank two local tetrahedra
/// `a` (the first element of a send scheme) and `b` (the last local
/// element of its type with another barycentre) exchange their
/// connectivities, recorded as `ElementsChanged { old: [a, b], new: [b,
/// a] }` and passed through `DistributedRemovalHook`. Afterwards the send
/// schemes name the elements at their new indices (`a`'s entry is `b`'s
/// and conversely, every other entry unchanged) and a barycentre exchange
/// is bitwise again; without the substitution the owners would send the
/// barycentres of the swapped elements.
pub fn element_changes_update_the_element_synchronizer(comm: Communicator) -> Outcome {
    use akantu_mesh::distributed_removal::DistributedRemovalHook;
    use akantu_mesh::{MeshEvent, MeshPart, SharedMesh};
    let m = distributed_cube(&comm)?;
    let dim = m.spatial_dimension();
    let b0 = barycenters(&m)?;
    let sync0 = m
        .synchronizers()
        .element
        .clone()
        .ok_or("no element synchronizer")?;
    let first_send = sync0
        .communications
        .send
        .values()
        .flat_map(|v| v.iter())
        .next()
        .copied();
    let swap: Option<(Element, Element)> = match first_send {
        None => None,
        Some(a) => {
            let ba = ok(b0.element(a), "barycentre")?.to_vec();
            let n = m.nb_element(a.ty(), a.ghost_type());
            let mut other = None;
            for i in (0..n).rev() {
                let e = Element::new(
                    a.ty(),
                    Idx::try_from(i).map_err(|e| e.to_string())?,
                    a.ghost_type(),
                );
                if e != a && ok(b0.element(e), "barycentre")? != ba.as_slice() {
                    other = Some(e);
                    break;
                }
            }
            other.map(|b| (a, b))
        }
    };
    let shared = SharedMesh::new(m);
    ok(
        shared.mutate(
            &mut [],
            |tx| {
                let Some((a, b)) = swap else {
                    return Ok(());
                };
                let ca = tx
                    .mesh()
                    .connectivity(a.ty(), a.ghost_type())?
                    .tuple(a.idx())
                    .to_vec();
                let cb = tx
                    .mesh()
                    .connectivity(b.ty(), b.ghost_type())?
                    .tuple(b.idx())
                    .to_vec();
                tx.set_element_nodes(MeshPart::Main, a, &cb)?;
                tx.set_element_nodes(MeshPart::Main, b, &ca)?;
                let n = tx.mesh().nb_element(a.ty(), a.ghost_type());
                let mut perm: Vec<Idx> = (0..n).map(|i| i as Idx).collect();
                perm.swap(a.idx(), b.idx());
                let mut new_numbering = ElementTypeMapArray::new("new_numbering");
                new_numbering.as_map_mut().insert(
                    a.ty(),
                    a.ghost_type(),
                    akantu_core::Array::from_vec(perm, 1)?,
                )?;
                tx.record(
                    MeshPart::Main,
                    MeshEvent::ElementsChanged {
                        old: vec![a, b],
                        new: vec![b, a],
                        new_numbering,
                        origin: "mpi016 test".into(),
                    },
                );
                Ok(())
            },
            Some(&mut DistributedRemovalHook::new()),
        ),
        "mutate",
    )?;
    let m = ok(shared.read(), "read")?;
    let b1 = barycenters(&m)?;
    let sync = m
        .synchronizers()
        .element
        .clone()
        .ok_or("no element synchronizer after the change")?;
    // the send schemes name the substituted elements, the receive schemes
    // are unchanged
    for (p, old) in &sync0.communications.send {
        let new = sync.communications.send.get(p).ok_or("send scheme lost")?;
        ensure!(
            old.len() == new.len(),
            "send scheme toward {p} changed its length"
        );
        for (o, n) in old.iter().zip(new) {
            let want = match swap {
                Some((a, b)) if *o == a => b,
                Some((a, b)) if *o == b => a,
                _ => *o,
            };
            ensure!(*n == want, "send scheme toward {p}: {n} instead of {want}");
        }
    }
    ensure!(
        sync.communications.recv == sync0.communications.recv,
        "the receive schemes changed"
    );
    if comm.size() > 1 {
        let mut total = [u64::from(swap.is_some())];
        ok(
            comm.all_reduce(&mut total, akantu_comm::ReduceOp::Sum),
            "reduce",
        )?;
        ensure!(total[0] > 0, "no rank changed a shared element");
    }
    let nb_recv: usize = sync.communications.recv.values().map(Vec::len).sum();
    let mut acc = BarycenterAccessor {
        dim,
        bary: &b1,
        max_dist: 0.0,
        unpacked: 0,
    };
    ok(
        sync.synchronize(&mut acc, SynchronizationTag::Test),
        "synchronize",
    )?;
    ensure!(
        acc.max_dist == 0.0 && acc.unpacked == nb_recv,
        "barycentres after the change: distance {}, unpacked {} of {nb_recv}",
        acc.max_dist,
        acc.unpacked
    );
    Ok(())
}

/// At local world 2 (and 1 / 4): on a distributed mesh with a consumer,
/// an erasure of a block
/// followed by a recorded `ElementsChanged` whose numbering of that block
/// is not the identity is refused in the capability stage (D-095 rule 2)
/// — recoverable on every rank, rolled back, the
/// revision unchanged — and the same mutation with an identity numbering
/// commits through `DistributedRemovalHook` (the domain stays usable).
pub fn element_change_renumbering_after_an_erasure_is_refused(comm: Communicator) -> Outcome {
    use akantu_core::enums::EventPriority;
    use akantu_mesh::distributed_removal::DistributedRemovalHook;
    use akantu_mesh::{MeshEvent, MeshEventHandler, MeshPart, SharedMesh};
    const X_CUT: Real = 0.35;
    struct Accepts;
    impl MeshEventHandler for Accepts {
        fn check_mesh_event(&self, _: &Mesh, _: &MeshEvent) -> Result<()> {
            // test consumer: accepts every event (no capability limits)
            Ok(())
        }
        fn event_priority(&self) -> EventPriority {
            EventPriority(50)
        }
        fn on_mesh_event(&mut self, _: &Mesh, _: &MeshEvent) -> Result<()> {
            Ok(())
        }
    }
    let m = distributed_cube(&comm)?;
    let dim = m.spatial_dimension();
    let b = barycenters(&m)?;
    let mut list = Vec::new();
    for ghost in GhostType::ALL {
        for ty in m.element_types(TypeFilter::new().dim(dim).ghost(ghost)) {
            let a = ok(b.get(ty, ghost), "barycentre block")?;
            for e in 0..m.nb_element(ty, ghost) {
                if a.as_slice()[e * dim] < X_CUT {
                    list.push(Element::new(
                        ty,
                        Idx::try_from(e).map_err(|e| e.to_string())?,
                        ghost,
                    ));
                }
            }
        }
    }
    let ty = m
        .element_types(TypeFilter::new().dim(dim).ghost(GhostType::NotGhost))
        .next()
        .ok_or("no element type")?;
    let nb_before = m.nb_element(ty, GhostType::NotGhost);
    let shared = SharedMesh::new(m);
    let token = ok(
        shared.register_consumer("accepts", EventPriority(50), MeshPart::Main),
        "register",
    )?;
    let mut consumer = Accepts;
    // the erasure, then an `ElementsChanged` of the erased block's
    // survivors: its last element changes into itself (so the event is
    // dispatched, and checked), the numbering swaps its last two elements
    // (`swap`) or is the identity
    let change = |swap: bool| {
        let list = list.clone();
        move |tx: &mut akantu_mesh::MeshTransaction<'_>| -> Result<()> {
            tx.erase_elements(MeshPart::Main, &list)?;
            let n = tx.mesh().nb_element(ty, GhostType::NotGhost);
            let mut perm: Vec<Idx> = (0..n).map(|i| i as Idx).collect();
            let mut old = Vec::new();
            if n >= 2 {
                if swap {
                    perm.swap(n - 1, n - 2);
                }
                old.push(Element::new(ty, (n - 1) as Idx, GhostType::NotGhost));
            }
            let mut new_numbering = ElementTypeMapArray::new("changed");
            new_numbering.as_map_mut().insert(
                ty,
                GhostType::NotGhost,
                akantu_core::Array::from_vec(perm, 1)?,
            )?;
            tx.record(
                MeshPart::Main,
                MeshEvent::ElementsChanged {
                    new: old.clone(),
                    old,
                    new_numbering,
                    origin: "lr14-05 test 1".into(),
                },
            );
            Ok(())
        }
    };
    let revision = ok(shared.revision(), "revision")?;
    let r = shared.mutate(
        &mut [(&token, &mut consumer)],
        change(true),
        Some(&mut DistributedRemovalHook::new()),
    );
    let Err(e) = r else {
        return Err("the renumbering ElementsChanged after the erasure committed".into());
    };
    ensure!(e.fatal_kind().is_none(), "not recoverable: {e:?}");
    let named = format!("{e}").contains("ElementsChanged");
    let mut flags = [u64::from(named)];
    ok(
        comm.all_reduce(&mut flags, akantu_comm::ReduceOp::Max),
        "reduce",
    )?;
    ensure!(flags[0] == 1, "no rank named the ElementsChanged rule: {e}");
    ensure!(
        ok(shared.revision(), "revision")? == revision,
        "revision moved"
    );
    ensure!(
        ok(shared.read(), "read")?.nb_element(ty, GhostType::NotGhost) == nb_before,
        "the erasure was not rolled back"
    );
    // the identity numbering composes; the domain is usable
    ok(
        shared.mutate(
            &mut [(&token, &mut consumer)],
            change(false),
            Some(&mut DistributedRemovalHook::new()),
        ),
        "identity numbering",
    )?;
    ensure!(
        ok(shared.revision(), "revision")? != revision,
        "the identity mutation did not commit"
    );
    Ok(())
}
