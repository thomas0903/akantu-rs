// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/mesh/mesh_periodic.cc,
// src/mesh/mesh_inline_impl.hh (addPeriodicSlave, getPeriodicMaster,
// getPeriodicSlaves, getConnectivityWithPeriodicity),
// src/synchronizer/periodic_node_synchronizer{.cc,_tmpl.hh} (local part).

//! Periodicity (upstream `Mesh::makePeriodic`, MESH-028/029).
//!
//! `make_periodic*` pairs the nodes of the lower ("left") and upper
//! ("right") boundary of one direction: positions projected on the
//! boundary plane (direction coordinate set to 0), and for every left node
//! the first right node at distance `< 1e-10 · L` (`L` = mesh length in that
//! direction, quadratic search as upstream) is matched. Matches merge
//! master/slave chains exactly as upstream (`match_found` /
//! `updating_master`): calling it for x, then y, then z leaves one master
//! per class of periodic copies (corner nodes get one master and up to
//! seven slaves in 3-D). Flags get the periodic bits, the maps are filled,
//! the mesh is marked periodic.
//!
//! Deviations / scope (`docs/UPSTREAM_DEFECTS.md`):
//! * processing order: upstream sorts both lists with Eigen's fuzzy
//!   `operator<` (`isMuchSmallerThan`), which is not a strict weak ordering
//!   in general (UD-121, category A); the lists are processed in their given
//!   order here. For boundaries whose nodes match one-to-one (conforming
//!   periodic meshes) the resulting pairing does not depend on the order;
//!   the differential tests compare the maps and flags with the oracle;
//! * on a mesh distributed over several ranks the boundary nodes in
//!   bounding-box intersections are exchanged and missing partners appended
//!   as pure-ghost periodic nodes
//!   ([`crate::periodic_synchronizer`]); on one rank (a mesh "distributed"
//!   on a single process, UD-011) the exchange is empty;
//! * the distributed `makePeriodic` recomputes the bounding boxes after
//!   appending the received pure ghosts, as upstream's
//!   `sendEvent<NewNodesEvent>` does (UD-255 withdrawn):
//!   local box over every node, then the Min/Max reduction for the global
//!   box, before the node synchronizer reacts to the event;
//! * a received pure-ghost periodic slave matched as `node1` / `node2`
//!   (UD-812, category B, **latent and unreachable**): upstream would drop
//!   the master global id sent with a slave and then read
//!   `periodic_slave_master[node]` with `operator[]`, pairing the slave's
//!   class with local node 0; the port would resolve the transmitted master
//!   id to its local node (an agreed error if it is not known locally), and
//!   [`MakePeriodicOptions::upstream_ghost_slave_master`] would reproduce
//!   upstream. **No mesh reaches this path, upstream or here**:
//!   only nodes with `isLocalOrMasterNode` are sent
//!   (`mesh_periodic.cc:176`, mask `_local_master_mask = 0xCC`; here
//!   [`NodeFlag::LOCAL_MASTER_MASK`]), every periodic slave fails that test
//!   (`0x5_ & 0xCC = 0x40`), so a slave is never sent and no received node
//!   carries the slave bit without a slave → master entry
//!   (`mesh_periodic.cc:235-237` is dead code). The triple wire format
//!   `[global id, periodic bits, master global id]` (master always `-1` in
//!   practice) and the switch are kept; both modes give identical pairs on
//!   every mesh;
//! * [`make_consistent_for_periodicity`] performs the local part of the
//!   periodic synchronizer (master → slave copies, slave → master
//!   reductions; the `_real`/`_bool` variants add the parallel exchanges of
//!   the periodic node synchronizer on distributed meshes). The reductions follow upstream's iteration order of its
//!   `std::unordered_multimap` master → slaves, established black-box with
//!   the oracle's standard library (D-030): per master, the slaves in
//!   **reverse insertion order** ([`Mesh::periodic_slaves_upstream_order`];
//!   UD-654, category C, superseding UD-132's sorted order); masters are
//!   independent of each other. Bitwise on 67 adversarial oracle cases
//!   (2,640 masters, 374 with 2–7 slaves,
//!   `fixtures/mesh_utils/periodic_order`, test `periodic_order_oracle`).
//!   The rule is specific to the reference standard library (another one
//!   may iterate equivalent keys differently). The distributed reduction
//!   (`periodic_synchronizer::reduce_synchronize_array_pbc`) uses the same
//!   order (D-061).
//!
//! The legacy `MeshUtils::computePBCMap` (MUTIL-009: built but no caller
//! anywhere in the reference, comparator not a strict weak ordering) is not
//! ported: exception under D-028.

use std::collections::HashMap;

use akantu_comm::phase::Phases;
use akantu_comm::{Communicator, Tag};
use akantu_core::enums::{ReduceOperation, SpatialDirection};
use akantu_core::{Array, Error, GlobalIdx, Idx, NodeFlag, Rank, Real, Result, try_to_usize};

use crate::events::MeshEvents;
use crate::mesh::Mesh;

/// Relative tolerance of upstream `makePeriodic`.
pub const PERIODIC_TOLERANCE: Real = 1e-10;

fn direction_index(mesh: &Mesh, direction: SpatialDirection) -> Result<usize> {
    let d = match direction {
        SpatialDirection::X => 0,
        SpatialDirection::Y => 1,
        SpatialDirection::Z => 2,
    };
    if d >= mesh.spatial_dimension() {
        return Err(Error::invalid(format!(
            "direction {direction} does not exist in dimension {}",
            mesh.spatial_dimension()
        )));
    }
    Ok(d)
}

/// Compatibility switches of `makePeriodic`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MakePeriodicOptions {
    /// UD-812 compatibility (distributed `makePeriodic`): a received
    /// pure-ghost periodic slave matched with another node takes **local
    /// node 0** as its master, and the entry `slave → 0` is inserted into
    /// the slave → master map (upstream reads `periodic_slave_master[node]`
    /// with `operator[]`, `mesh_periodic.cc:352-359`, after dropping the
    /// master id it would receive, `mesh_periodic.cc:235-237`). Default
    /// (`false`): the master global id the sender transmits, resolved to its
    /// local node; an agreed recoverable error naming the pair if that node
    /// is not known locally.
    ///
    /// **Latent: no mesh reaches this path**. Only
    /// nodes with `isLocalOrMasterNode` are sent (`mesh_periodic.cc:176`,
    /// [`NodeFlag::LOCAL_MASTER_MASK`] `= 0xCC`) and every periodic slave
    /// fails that test, so no received node is a periodic slave without a
    /// slave → master entry; both values of the switch give identical pairs
    /// on every mesh (UD-812, category B, latent / unreachable).
    pub upstream_ghost_slave_master: bool,
}

/// Pair opposite boundary nodes along `direction` and flag them
/// master/slave (upstream `makePeriodic(direction)`, or with `groups =
/// Some((left, right))` upstream `makePeriodic(direction, group_1,
/// group_2)` on the node lists of two **element** groups). The bounds are
/// the mesh's (global) bounding box.
pub fn make_periodic(
    mesh: &mut Mesh,
    direction: SpatialDirection,
    groups: Option<(&str, &str)>,
) -> Result<MeshEvents> {
    make_periodic_with(mesh, direction, groups, MakePeriodicOptions::default())
}

/// [`make_periodic`] with compatibility switches.
pub fn make_periodic_with(
    mesh: &mut Mesh,
    direction: SpatialDirection,
    groups: Option<(&str, &str)>,
    options: MakePeriodicOptions,
) -> Result<MeshEvents> {
    let lists = (|| -> Result<(usize, Vec<Idx>, Vec<Idx>)> {
        let d = direction_index(mesh, direction)?;
        let (left, right) = match groups {
            Some((a, b)) => (
                mesh.groups().element_group(a)?.nodes().as_slice().to_vec(),
                mesh.groups().element_group(b)?.nodes().as_slice().to_vec(),
            ),
            None => {
                let (lo, hi) = (mesh.bbox().lower[d], mesh.bbox().upper[d]);
                let length = hi - lo;
                let (mut l, mut r) = (Vec::new(), Vec::new());
                for (n, x) in mesh.nodes().tuples().enumerate() {
                    let n = akantu_core::try_idx(n)?;
                    if ((x[d] - lo) / length).abs() < PERIODIC_TOLERANCE {
                        l.push(n);
                    }
                    if ((x[d] - hi) / length).abs() < PERIODIC_TOLERANCE {
                        r.push(n);
                    }
                }
                (l, r)
            }
        };
        Ok((d, left, right))
    })();
    make_periodic_checked(mesh, lists, options)
}

/// Upstream `makePeriodic(direction, list_left, list_right)` (see the
/// module docs). Node indices out of range are an error (before any
/// change; agreed on every rank of a distributed mesh).
pub fn make_periodic_lists(
    mesh: &mut Mesh,
    direction: SpatialDirection,
    left: &[Idx],
    right: &[Idx],
) -> Result<MeshEvents> {
    make_periodic_lists_with(mesh, direction, left, right, MakePeriodicOptions::default())
}

/// [`make_periodic_lists`] with compatibility switches.
pub fn make_periodic_lists_with(
    mesh: &mut Mesh,
    direction: SpatialDirection,
    left: &[Idx],
    right: &[Idx],
    options: MakePeriodicOptions,
) -> Result<MeshEvents> {
    let lists = direction_index(mesh, direction).map(|d| (d, left.to_vec(), right.to_vec()));
    make_periodic_checked(mesh, lists, options)
}

/// Validate the node lists, then pair them (serial / one rank) or run the
/// distributed protocol (every local failure agreed first).
fn make_periodic_checked(
    mesh: &mut Mesh,
    lists: Result<(usize, Vec<Idx>, Vec<Idx>)>,
    options: MakePeriodicOptions,
) -> Result<MeshEvents> {
    let nb = mesh.nb_nodes();
    let lists = lists.and_then(|(d, left, right)| {
        for &n in left.iter().chain(&right) {
            let u = try_to_usize(n)?;
            if u >= nb {
                return Err(Error::OutOfRange {
                    what: "node",
                    index: u,
                    len: nb,
                });
            }
        }
        Ok((d, left, right))
    });
    if mesh.is_distributed() && mesh.communicator().size() > 1 {
        return make_periodic_distributed(mesh, lists, options);
    }
    let (d, left, right) = lists?;
    let length = mesh.bbox().upper[d] - mesh.bbox().lower[d];
    let project = |n: Idx| -> Vec<Real> {
        let mut x = mesh.nodes().tuple(akantu_core::to_usize(n)).to_vec();
        x[d] = 0.0;
        x
    };
    let lp: Vec<(Idx, Vec<Real>)> = left.iter().map(|&n| (n, project(n))).collect();
    let rp: Vec<(Idx, Vec<Real>)> = right.iter().map(|&n| (n, project(n))).collect();
    let mut matches = Vec::new();
    for (n1, p1) in &lp {
        for (n2, p2) in &rp {
            if distance(p1, p2) / length < PERIODIC_TOLERANCE {
                matches.push((*n1, *n2));
                break;
            }
        }
    }
    // on a copy of the maps: the mesh is unchanged on error
    let mut state = PeriodicState::of(mesh, options);
    for (n1, n2) in matches {
        state.match_found(n1, n2)?;
    }
    state.put_back(mesh);
    update_periodic_synchronizer(mesh);
    mesh.is_periodic = true;
    crate::periodic_synchronizer::mirror_synchronizers_to_facets(mesh);
    mesh.revision += 1;
    Ok(MeshEvents::none())
}

/// Tags of the boundary-node exchange of the distributed `makePeriodic`
/// (left / right lists; `[global id, periodic bits, master global id]`
/// triples, then coordinates).
const NODES_TAG: [Tag; 2] = [Tag(29_300), Tag(29_301)];
const COORDS_TAG: [Tag; 2] = [Tag(29_302), Tag(29_303)];

/// A boundary node with its position projected on the boundary plane
/// (upstream `NodeInfo`).
struct NodeInfo {
    node: Idx,
    position: Vec<Real>,
}

/// New pure-ghost periodic nodes staged by the distributed `makePeriodic`.
struct StagedNode {
    global: GlobalIdx,
    position: Vec<Real>,
    flag: NodeFlag,
    owner: Rank,
}

/// "No master" in the master field of the boundary-node messages (a node
/// that is not a periodic slave).
const NO_MASTER: GlobalIdx = -1;

/// A received pure-ghost periodic slave and the master global id its owner
/// transmitted (upstream receives it and drops it; UD-812).
#[derive(Clone, Debug)]
struct ReceivedSlave {
    global: GlobalIdx,
    from: Rank,
    master_global: GlobalIdx,
    /// The master's local node (`None`: not known on this rank).
    master_local: Option<Idx>,
}

/// Upstream `Mesh::makePeriodic(direction, list_left, list_right)` on a mesh
/// distributed over several ranks (`mesh_periodic.cc:122-422`, see
/// [`crate::periodic_synchronizer`]). One scope; the mesh changes only
/// after every rank succeeded.
#[allow(clippy::too_many_lines)] // one upstream function, staged
fn make_periodic_distributed(
    mesh: &mut Mesh,
    lists: Result<(usize, Vec<Idx>, Vec<Idx>)>,
    options: MakePeriodicOptions,
) -> Result<MeshEvents> {
    use crate::periodic_synchronizer as ps;
    use crate::upstream_bbox::FlaggedBox;
    let comm = mesh.communicator().clone();
    let rank = comm.rank();
    let dim = mesh.spatial_dimension();
    let mut ph = Phases::new(&comm, format!("make_periodic:{}", mesh.id()));
    let staged = (|| -> Result<_> {
        // ---- local lists and projected boxes; every local failure agreed
        let prepared = lists.map(|(d, left, right)| {
            let mut out: [Vec<NodeInfo>; 2] = [Vec::new(), Vec::new()];
            let mut boxes = [FlaggedBox::new(dim), FlaggedBox::new(dim)];
            for (side, nodes) in [left, right].into_iter().enumerate() {
                for n in nodes {
                    let mut p = mesh.nodes().tuple(akantu_core::to_usize(n)).to_vec();
                    p[d] = 0.0;
                    boxes[side].add_point(&p);
                    out[side].push(NodeInfo {
                        node: n,
                        position: p,
                    });
                }
            }
            (d, out, boxes)
        });
        let mut packed = Vec::with_capacity(6 * dim);
        if let Ok((_, _, boxes)) = &prepared {
            boxes[0].pack(&mut packed);
            boxes[1].pack(&mut packed);
        }
        let (d, mut lists, boxes) = ps::agreed(&mut ph, "periodic:lists", prepared)?;
        let length = mesh.bbox().upper[d] - mesh.bbox().lower[d];
        let project = |x: &[Real]| -> Vec<Real> {
            let mut p = x.to_vec();
            p[d] = 0.0;
            p
        };
        // ---- boxes of every rank (upstream BBox::intersection(other, comm))
        let all = ph.collective("periodic:boxes", |c| c.all_gather(&packed))?;
        let mut remote: Vec<[FlaggedBox; 2]> = Vec::with_capacity(comm.size());
        for chunk in all.chunks_exact(6 * dim) {
            remote.push([
                FlaggedBox::unpack(&chunk[..3 * dim], dim)?,
                FlaggedBox::unpack(&chunk[3 * dim..], dim)?,
            ]);
        }
        // intersections[side][p]: my opposite box ∩ p's `side` box
        let mut inter: [Vec<Option<FlaggedBox>>; 2] = [Vec::new(), Vec::new()];
        for r in &remote {
            inter[0].push(boxes[1].intersection(&r[0]));
            inter[1].push(boxes[0].intersection(&r[1]));
        }
        // ---- messages: owned nodes of `side` inside my_side ∩ p's other side
        let mut messages: Vec<(usize, Rank, Vec<GlobalIdx>, Vec<Real>)> = Vec::new();
        for side in 0..2 {
            for (p, b) in inter[1 - side].iter().enumerate() {
                let (Some(b), Ok(p)) = (b, Rank::try_from(p)) else {
                    continue;
                };
                if p == rank {
                    // upstream sends to itself: every node is known (no effect)
                    continue;
                }
                let mut ids = Vec::new();
                let mut xs = Vec::new();
                let nodes = mesh.nodes();
                for info in &lists[side] {
                    if !b.contains(&info.position)
                        || !mesh.node_flag(info.node).is_local_or_master()
                    {
                        continue;
                    }
                    let periodic =
                        mesh.node_flag(info.node).bits() & NodeFlag::PERIODIC_MASK.bits();
                    ids.push(mesh.node_global_id(info.node));
                    ids.push(GlobalIdx::from(periodic));
                    // upstream sends the master of a slave (and the slaves
                    // of a master, which the port does not need: upstream
                    // drops both on receipt)
                    ids.push(if periodic == NodeFlag::PERIODIC_SLAVE.bits() {
                        mesh.periodic_master(info.node)
                            .map_or(NO_MASTER, |m| mesh.node_global_id(m))
                    } else {
                        NO_MASTER
                    });
                    xs.extend_from_slice(nodes.tuple(akantu_core::to_usize(info.node)));
                }
                messages.push((side, p, ids, xs));
            }
        }
        let sources: Vec<(usize, Rank)> = (0..2)
            .flat_map(|side| {
                inter[side]
                    .iter()
                    .enumerate()
                    .filter(|(p, b)| b.is_some() && *p != akantu_core::to_usize(rank))
                    .filter_map(move |(p, _)| Rank::try_from(p).ok().map(|p| (side, p)))
                    .collect::<Vec<_>>()
            })
            .collect();
        let received = ph.run(
            "periodic:nodes",
            Ok(messages),
            |c: &Communicator, messages| {
                let mut reqs_i = Vec::with_capacity(messages.len());
                let mut reqs_x = Vec::with_capacity(messages.len());
                for (side, p, ids, xs) in messages {
                    reqs_i.push(c.isend(ids, p, NODES_TAG[side])?);
                    reqs_x.push(c.isend(xs, p, COORDS_TAG[side])?);
                }
                let mut got = Vec::with_capacity(sources.len());
                for &(side, p) in &sources {
                    let ids = c.receive_vec::<GlobalIdx>(p, NODES_TAG[side])?;
                    let xs = c.receive_vec::<Real>(p, COORDS_TAG[side])?;
                    got.push((side, p, ids, xs));
                }
                for r in reqs_i {
                    r.wait()?;
                }
                for r in reqs_x {
                    r.wait()?;
                }
                for (_, p, ids, xs) in &got {
                    if ids.len() % 3 != 0 || xs.len() != ids.len() / 3 * dim {
                        return Ok(Err(Error::Communication(format!(
                            "makePeriodic: malformed boundary nodes from rank {p}"
                        ))));
                    }
                }
                Ok(Ok(got))
            },
        )?;
        // ---- new pure ghosts (receive order: left lists, then right, rank order)
        let mut local_of: HashMap<GlobalIdx, Idx> = HashMap::new();
        for n in (0..mesh.nb_nodes()).rev() {
            let n = akantu_core::try_idx(n)?;
            local_of.insert(mesh.node_global_id(n), n);
        }
        let mut staged: Vec<StagedNode> = Vec::new();
        let mut received_slaves: HashMap<Idx, ReceivedSlave> = HashMap::new();
        for (side, p, ids, xs) in received {
            for (k, &[g, f, master]) in ids.as_chunks::<3>().0.iter().enumerate() {
                if local_of.contains_key(&g) {
                    continue;
                }
                let l = akantu_core::try_idx(mesh.nb_nodes() + staged.len())?;
                local_of.insert(g, l);
                let x = xs[k * dim..(k + 1) * dim].to_vec();
                let periodic = u8::try_from(f)
                    .map_err(|_| Error::Communication("makePeriodic: flag".into()))?;
                if periodic & NodeFlag::PERIODIC_MASK.bits() == NodeFlag::PERIODIC_SLAVE.bits() {
                    received_slaves.insert(
                        l,
                        ReceivedSlave {
                            global: g,
                            from: p,
                            master_global: master,
                            master_local: None,
                        },
                    );
                }
                staged.push(StagedNode {
                    global: g,
                    position: x.clone(),
                    flag: NodeFlag(
                        (periodic & NodeFlag::PERIODIC_MASK.bits()) | NodeFlag::PURE_GHOST.bits(),
                    ),
                    owner: p,
                });
                lists[side].push(NodeInfo {
                    node: l,
                    position: project(&x),
                });
            }
        }
        // masters of the received slaves, resolved on the complete node set
        // (existing and staged nodes; UD-812)
        for r in received_slaves.values_mut() {
            r.master_local = local_of.get(&r.master_global).copied();
        }
        // ---- upstream `this->sendEvent(event)` → `computeBoundingBox()`:
        // local box over every node incl. the new pure ghosts, collective
        // Min/Max for the global box, then the node synchronizer
        let bounding_boxes = crate::grid_synchronizer::boxes_with_new_nodes_in(
            &mut ph,
            "periodic:bbox",
            mesh,
            staged.iter().map(|s| s.position.as_slice()),
        )?;
        // ---- node synchronizer reacts to the NewNodesEvent (pure ghosts are
        // in no node scheme: computed on the current mesh)
        let node_sync = match mesh.synchronizers().node.as_ref() {
            Some(ns) => {
                let mut ns = ns.clone();
                crate::synchronizers::rebuild_node_schemes_in(&mut ph, mesh, &mut ns)?;
                Some(ns)
            }
            None => None,
        };
        // ---- matching on the staged state (local; a received slave whose
        // master is not known locally is an agreed error, UD-812)
        let mut state = PeriodicState::of(mesh, options);
        state.flags.extend(staged.iter().map(|s| s.flag));
        state.received_slaves = received_slaves;
        let mut matches = Vec::new();
        for a in &lists[0] {
            for b in &lists[1] {
                if distance(&a.position, &b.position) / length < PERIODIC_TOLERANCE {
                    matches.push((a.node, b.node));
                    break;
                }
            }
        }
        let matched = matches
            .into_iter()
            .try_for_each(|(n1, n2)| state.match_found(n1, n2));
        ps::agreed(&mut ph, "periodic:match", matched)?;
        // ---- periodic synchronizer update and node-scheme filter
        let new_pranks: HashMap<Idx, Rank> = staged
            .iter()
            .enumerate()
            .filter_map(|(k, s)| {
                akantu_core::try_idx(mesh.nb_nodes() + k)
                    .ok()
                    .map(|l| (l, s.owner))
            })
            .collect();
        let new_globals: HashMap<Idx, GlobalIdx> = staged
            .iter()
            .enumerate()
            .filter_map(|(k, s)| {
                akantu_core::try_idx(mesh.nb_nodes() + k)
                    .ok()
                    .map(|l| (l, s.global))
            })
            .collect();
        let view = ps::PeriodicView {
            mesh,
            flags: &state.flags,
            new_pranks: &new_pranks,
            new_globals: &new_globals,
            master_slaves: &state.master_slaves,
        };
        let mut periodic_sync = mesh
            .synchronizers()
            .periodic_node
            .clone()
            .unwrap_or_else(|| {
                akantu_comm::Synchronizer::new(
                    format!("{}:periodic_synchronizer:masters", mesh.id()),
                    comm.clone(),
                )
            });
        ps::update_periodic_synchronizer_view_in(&mut ph, &view, &mut periodic_sync)?;
        let node_sync = ps::filtered_node_synchronizer_in(&mut ph, mesh, &state.flags, node_sync)?;
        Ok((staged, state, periodic_sync, node_sync, bounding_boxes))
    })();
    let (staged, state, periodic_sync, node_sync, (bbox_local, bbox)) = ph.settle(staged)?;
    drop(ph);
    // ---- commit (local)
    let mut added = Vec::with_capacity(staged.len());
    for s in &staged {
        let l = akantu_core::try_idx(mesh.nodes.len())?;
        mesh.nodes.borrow_mut().try_push_tuple(&s.position)?;
        mesh.nodes_flags.push_value(s.flag);
        if let Some(g) = &mut mesh.nodes_global_ids {
            g.push_value(s.global);
        }
        mesh.nodes_prank.insert(l, s.owner);
        if let Some(f) = mesh.mesh_facets.as_deref_mut() {
            f.nodes.borrow_mut().try_push_tuple(&s.position)?;
            f.nodes_flags.push_value(s.flag);
            if let Some(g) = &mut f.nodes_global_ids {
                g.push_value(s.global);
            }
            f.nodes_prank.insert(l, s.owner);
        }
        added.push(l);
    }
    state.put_back(mesh);
    // the boxes upstream's `sendEvent<NewNodesEvent>` computed (the
    // facet mesh does not receive the event upstream)
    mesh.bbox_local = bbox_local;
    mesh.bbox = bbox;
    mesh.synchronizers.periodic_node = Some(periodic_sync);
    if let Some(ns) = node_sync {
        mesh.synchronizers.node = Some(ns);
    }
    mesh.is_periodic = true;
    crate::periodic_synchronizer::mirror_synchronizers_to_facets(mesh);
    mesh.revision += 1;
    let mut ev = MeshEvents::none();
    ev.push(crate::events::MeshEvent::NodesAdded {
        nodes: added,
        old_nodes: None,
        origin: "Mesh::makePeriodic".into(),
        extension: None,
    });
    Ok(ev)
}

/// Eigen `distance` of dynamic vectors: sequential sum of squares, one
/// square root.
fn distance(a: &[Real], b: &[Real]) -> Real {
    let mut s = 0.0;
    for (i, (&x, &y)) in a.iter().zip(b).enumerate() {
        let t = x - y;
        s = if i == 0 { t * t } else { s + t * t };
    }
    s.sqrt()
}

/// Maps and flags being updated (a copy of the mesh's, installed by
/// [`PeriodicState::put_back`] on success).
struct PeriodicState {
    slave_master: HashMap<Idx, Idx>,
    master_slaves: HashMap<Idx, Vec<Idx>>,
    flags: Vec<NodeFlag>,
    /// Received pure-ghost periodic slaves (distributed `makePeriodic`):
    /// they carry the slave flag but have no slave → master entry (UD-812).
    received_slaves: HashMap<Idx, ReceivedSlave>,
    /// UD-812 compatibility (see [`MakePeriodicOptions`]).
    upstream_ghost_slave_master: bool,
}

impl PeriodicState {
    fn of(mesh: &Mesh, options: MakePeriodicOptions) -> Self {
        Self {
            slave_master: mesh.periodic_slave_master.clone(),
            master_slaves: mesh.periodic_master_slaves.clone(),
            flags: mesh.nodes_flags.as_slice().to_vec(),
            received_slaves: HashMap::new(),
            upstream_ghost_slave_master: options.upstream_ghost_slave_master,
        }
    }

    fn put_back(self, mesh: &mut Mesh) {
        mesh.periodic_slave_master = self.slave_master;
        mesh.periodic_master_slaves = self.master_slaves;
        mesh.nodes_flags.as_mut_slice().copy_from_slice(&self.flags);
    }

    fn flag(&mut self, n: Idx) -> &mut NodeFlag {
        &mut self.flags[akantu_core::to_usize(n)]
    }

    fn is_master(&self, n: Idx) -> bool {
        self.flags[akantu_core::to_usize(n)].is_periodic_master()
    }

    fn is_slave(&self, n: Idx) -> bool {
        self.flags[akantu_core::to_usize(n)].is_periodic_slave()
    }

    /// Master of the periodic slave `slave` (upstream
    /// `periodic_slave_master[slave]`), `other` being the node it is matched
    /// with (error message). A slave without an entry is a received pure
    /// ghost (UD-812): upstream's `operator[]` inserts `slave → 0` and
    /// returns local node 0 (compatibility switch); by default the master
    /// id its owner transmitted, resolved locally, or an error.
    fn master_of_slave(&mut self, slave: Idx, other: Idx) -> Result<Idx> {
        if let Some(&m) = self.slave_master.get(&slave) {
            return Ok(m);
        }
        if self.upstream_ghost_slave_master {
            self.slave_master.insert(slave, 0);
            return Ok(0);
        }
        match self.received_slaves.get(&slave) {
            Some(ReceivedSlave {
                master_local: Some(m),
                ..
            }) => Ok(*m),
            Some(r) => Err(Error::Mesh(format!(
                "makePeriodic: the pure-ghost periodic slave {} (local node {slave}, received \
                 from rank {}) is matched with local node {other}, but its master {} is not \
                 known on this rank (UD-812)",
                r.global,
                r.from,
                if r.master_global == NO_MASTER {
                    "(none sent)".to_owned()
                } else {
                    r.master_global.to_string()
                }
            ))),
            None => Err(Error::Mesh(format!(
                "makePeriodic: the periodic slave {slave} (matched with {other}) has no master"
            ))),
        }
    }

    /// Upstream `Mesh::addPeriodicSlave`.
    fn add_periodic_slave(&mut self, slave: Idx, master: Idx) {
        if master == slave {
            return;
        }
        let slaves = self.master_slaves.entry(master).or_default();
        if !slaves.contains(&slave) {
            slaves.push(slave);
        }
        self.slave_master.insert(slave, master);
        let mask = !NodeFlag::PERIODIC_MASK.bits();
        let f = self.flag(slave);
        *f = NodeFlag((f.bits() & mask) | NodeFlag::PERIODIC_SLAVE.bits());
        let f = self.flag(master);
        *f = NodeFlag((f.bits() & mask) | NodeFlag::PERIODIC_MASTER.bits());
    }

    /// Upstream `updating_master` lambda.
    ///
    /// Upstream copies the old master's slaves through a temporary multimap
    /// (`tmp_master_slave`, `mesh_periodic.cc:315-338`) before re-adding
    /// them to the new master with `addPeriodicSlave`. Under UD-654's rule
    /// the old group iterates in reverse insertion order, is inserted into
    /// the temporary in that order, and the temporary iterates it reversed
    /// again: the new master receives the old slaves in their **original**
    /// insertion order (after `old_master` itself), which is what the direct
    /// loop below does. The insertion order of [`Mesh::periodic_slaves`] thus
    /// stays upstream's, on which the reductions rely.
    fn updating_master(&mut self, old_master: Idx, new_master: Idx) {
        if old_master == new_master {
            return;
        }
        let slaves = self.master_slaves.remove(&old_master).unwrap_or_default();
        for &s in &slaves {
            self.slave_master.insert(s, new_master);
        }
        let f = self.flag(old_master);
        *f = NodeFlag(f.bits() & !NodeFlag::PERIODIC_MASTER.bits());
        self.add_periodic_slave(old_master, new_master);
        for s in slaves {
            self.add_periodic_slave(s, new_master);
        }
    }

    /// Upstream `match_found` lambda (`mesh_periodic.cc:336-378`). Fails
    /// only for a received pure-ghost slave whose master is not known
    /// locally (UD-812 default; the state is then discarded).
    fn match_found(&mut self, node1: Idx, node2: Idx) -> Result<()> {
        let mut master = node1;
        let mut node1_side_master = false;
        if self.is_master(node1) {
            node1_side_master = true;
        } else if self.is_slave(node1) {
            node1_side_master = true;
            master = self.master_of_slave(node1, node2)?;
        }
        let node2_master = if self.is_slave(node2) {
            self.master_of_slave(node2, node1)?
        } else {
            node2
        };
        if node1_side_master {
            if self.is_slave(node2) {
                self.updating_master(node2_master, master);
                return Ok(());
            }
            if self.is_master(node2) {
                self.updating_master(node2, master);
                return Ok(());
            }
            self.add_periodic_slave(node2, master);
        } else {
            if self.is_slave(node2) {
                self.add_periodic_slave(node1, node2_master);
                return Ok(());
            }
            if self.is_master(node2) {
                self.add_periodic_slave(node1, node2);
                return Ok(());
            }
            self.add_periodic_slave(node2, node1);
        }
        Ok(())
    }
}

/// Upstream `updatePeriodicSynchronizer` (local part): the periodic node
/// synchronizer exists (empty schemes in serial) and periodic slaves are
/// filtered out of the node synchronizer's schemes.
fn update_periodic_synchronizer(mesh: &mut Mesh) {
    if mesh.synchronizers.periodic_node.is_none() {
        mesh.synchronizers.periodic_node = Some(akantu_comm::Synchronizer::new(
            format!("{}:periodic_synchronizer:masters", mesh.id()),
            mesh.communicator().clone(),
        ));
    }
    let flags = mesh.nodes_flags.as_slice().to_vec();
    if let Some(node_sync) = mesh.synchronizers.node.as_mut() {
        let keep = |n: &Idx| {
            usize::try_from(*n)
                .ok()
                .and_then(|u| flags.get(u))
                .is_none_or(|f| !f.is_periodic_slave())
        };
        for list in node_sync.communications.send.values_mut() {
            list.retain(keep);
        }
        for list in node_sync.communications.recv.values_mut() {
            list.retain(keep);
        }
    }
}

/// Upstream `wipePeriodicInfo`: maps cleared, periodic flag bits cleared,
/// mesh no longer periodic.
pub fn wipe_periodic_info(mesh: &mut Mesh) {
    mesh.is_periodic = false;
    mesh.periodic_slave_master.clear();
    mesh.periodic_master_slaves.clear();
    for f in mesh.nodes_flags.as_mut_slice() {
        *f = NodeFlag(f.bits() & !NodeFlag::PERIODIC_MASK.bits());
    }
    mesh.revision += 1;
}

/// Every `(master, slave)` pair, sorted by master then slave. The `Id`
/// copies of [`make_consistent_for_periodicity`] use it (their order is
/// irrelevant: a master is never a slave); the `Sum`/`Or` reductions use
/// reverse insertion order instead ([`Mesh::periodic_slaves_upstream_order`],
/// UD-654).
#[must_use]
pub fn periodic_pairs(mesh: &Mesh) -> Vec<(Idx, Idx)> {
    let mut v: Vec<(Idx, Idx)> = mesh
        .periodic_master_slaves
        .iter()
        .flat_map(|(&m, s)| s.iter().map(move |&s| (m, s)))
        .collect();
    v.sort_unstable();
    v
}

/// Local part of upstream `Mesh::makeConsistentForPeriodicity(array,
/// operation)` (`mesh_periodic.cc:445-490`): `Id` copies every master value
/// to its slaves; `Sum`/`Or` first zero periodic nodes that are parallel
/// slaves or pure ghosts, reduce every slave into its master, then copy the
/// masters back. No-op on a non-periodic mesh. The parallel node
/// synchronization around it belongs to the parallel layer (refused for
/// meshes distributed over several ranks).
pub fn make_consistent_for_periodicity<T: Copy + Default>(
    mesh: &Mesh,
    array: &mut Array<T>,
    operation: ReduceOperation,
    reduce: impl Fn(T, T) -> T,
) -> Result<()> {
    if !mesh.is_periodic() {
        return Ok(());
    }
    if array.len() != mesh.nb_nodes() {
        return Err(Error::DimensionMismatch {
            context: "makeConsistentForPeriodicity (array length = nb_nodes)",
            expected: mesh.nb_nodes(),
            got: array.len(),
        });
    }
    if mesh.is_distributed() && mesh.communicator().size() > 1 {
        return Err(Error::Unsupported(
            "makeConsistentForPeriodicity of a generic array on a mesh distributed over several \
             ranks: use make_consistent_for_periodicity_real / _bool (parallel exchanges)"
                .into(),
        ));
    }
    let pairs = periodic_pairs(mesh);
    let nbc = array.nb_component();
    // one reusable tuple buffer (no allocation per pair)
    let mut scratch: Vec<T> = vec![T::default(); nbc];
    let mut copy_masters_to_slaves = |a: &mut Array<T>| {
        for &(m, s) in &pairs {
            scratch.copy_from_slice(a.tuple(akantu_core::to_usize(m)));
            a.tuple_mut(akantu_core::to_usize(s))
                .copy_from_slice(&scratch);
        }
    };
    match operation {
        ReduceOperation::Id => copy_masters_to_slaves(array),
        ReduceOperation::Sum | ReduceOperation::Or => {
            let remote =
                NodeFlag::PERIODIC.bits() | NodeFlag::PURE_GHOST.bits() | NodeFlag::SLAVE.bits();
            let ghost = NodeFlag::PERIODIC.bits() | NodeFlag::PURE_GHOST.bits();
            let slave = NodeFlag::PERIODIC.bits() | NodeFlag::SLAVE.bits();
            for (n, f) in mesh.nodes_flags().as_slice().iter().enumerate() {
                let m = f.bits() & remote;
                if m == slave || m == ghost {
                    array.tuple_mut(n).fill(T::default());
                }
            }
            // upstream reduces in the iteration order of its
            // `std::unordered_multimap` master → slaves: per master, the
            // slaves in reverse insertion order (UD-654, established
            // black-box on the oracle's standard library, D-030; regression
            // `tests/periodic_order_oracle.rs`); the masters are independent
            // of each other
            let mut masters: Vec<Idx> = mesh.periodic_master_slaves.keys().copied().collect();
            masters.sort_unstable();
            let order: Vec<(Idx, Idx)> = masters
                .iter()
                .flat_map(|&m| mesh.periodic_slaves_upstream_order(m).map(move |s| (m, s)))
                .collect();
            // slave values before any reduction, flat (one buffer)
            let mut slave_values: Vec<T> = Vec::with_capacity(order.len() * nbc);
            for &(_, s) in &order {
                slave_values.extend_from_slice(array.tuple(akantu_core::to_usize(s)));
            }
            for (&(m, _), v) in order.iter().zip(slave_values.chunks_exact(nbc.max(1))) {
                let t = array.tuple_mut(akantu_core::to_usize(m));
                for c in 0..nbc {
                    t[c] = reduce(t[c], v[c]);
                }
            }
            copy_masters_to_slaves(array);
        }
    }
    Ok(())
}

/// [`make_consistent_for_periodicity`] for `Real` arrays (`Sum` adds).
pub fn make_consistent_for_periodicity_real(
    mesh: &Mesh,
    array: &mut Array<Real>,
    operation: ReduceOperation,
) -> Result<()> {
    if mesh.is_periodic() && mesh.is_distributed() && mesh.communicator().size() > 1 {
        // the array length is checked and agreed inside the collective
        // scope
        return crate::periodic_synchronizer::make_consistent_for_periodicity_real_distributed(
            mesh, array, operation,
        );
    }
    make_consistent_for_periodicity(mesh, array, operation, |a, b| a + b)
}

/// [`make_consistent_for_periodicity`] for `bool` arrays (`Sum`/`Or` are
/// logical or).
pub fn make_consistent_for_periodicity_bool(
    mesh: &Mesh,
    array: &mut Array<bool>,
    operation: ReduceOperation,
) -> Result<()> {
    if mesh.is_periodic() && mesh.is_distributed() && mesh.communicator().size() > 1 {
        // the array length is checked and agreed inside the collective
        // scope
        return crate::periodic_synchronizer::make_consistent_for_periodicity_bool_distributed(
            mesh, array, operation,
        );
    }
    make_consistent_for_periodicity(mesh, array, operation, |a, b| a | b)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::accessor::MeshAccessor;
    use akantu_core::{ElementType, GhostType};

    fn unit_square(n: usize) -> Mesh {
        let mut m = Mesh::new(2, "sq").unwrap();
        {
            let mut a = MeshAccessor::new(&mut m);
            #[allow(clippy::cast_precision_loss)]
            let h = 1.0 / n as Real;
            for j in 0..=n {
                for i in 0..=n {
                    #[allow(clippy::cast_precision_loss)]
                    a.push_node(&[i as Real * h, j as Real * h]).unwrap();
                }
            }
            let mut c = a.connectivity_mut(ElementType::Quadrangle4, GhostType::NotGhost);
            for j in 0..n {
                for i in 0..n {
                    let p = |i: usize, j: usize| Idx::try_from(j * (n + 1) + i).unwrap();
                    c.push_tuple(&[p(i, j), p(i + 1, j), p(i + 1, j + 1), p(i, j + 1)]);
                }
            }
        }
        m.compute_bounding_box().unwrap();
        m
    }

    /// `periodic_sum_contributions_real` (the local part of the distributed
    /// `_sum`) on one rank: followed by the master →
    /// slave copy it is the serial `_sum` (two implementations of the same
    /// steps), bitwise on non-dyadic data with several slaves per master;
    /// a non-periodic mesh is a no-op; a wrong length is refused.
    #[test]
    fn sum_contributions_are_the_serial_sum_before_the_copy() {
        let mut m = unit_square(3);
        let mut plain = Array::filled(m.nb_nodes(), 2, 1.5);
        crate::periodic_synchronizer::periodic_sum_contributions_real(&m, &mut plain).unwrap();
        assert!(plain.as_slice().iter().all(|&v| v == 1.5));
        let _ = make_periodic(&mut m, SpatialDirection::X, None).unwrap();
        let _ = make_periodic(&mut m, SpatialDirection::Y, None).unwrap();
        let mut input = Array::filled(m.nb_nodes(), 2, 0.0);
        for n in 0..m.nb_nodes() {
            #[allow(clippy::cast_precision_loss)] // small node ids
            let g = n as Real;
            input
                .tuple_mut(n)
                .copy_from_slice(&[1. / (3. + g), 0.1 * (g + 1.)]);
        }
        let mut contrib = input.clone();
        crate::periodic_synchronizer::periodic_sum_contributions_real(&m, &mut contrib).unwrap();
        let mut sum = input.clone();
        make_consistent_for_periodicity_real(&m, &mut sum, ReduceOperation::Sum).unwrap();
        let mut source: Vec<usize> = (0..m.nb_nodes()).collect();
        for (ms, sl) in periodic_pairs(&m) {
            source[akantu_core::to_usize(sl)] = akantu_core::to_usize(ms);
        }
        for (n, &s) in source.iter().enumerate() {
            for c in 0..2 {
                assert_eq!(
                    sum.tuple(n)[c].to_bits(),
                    contrib.tuple(s)[c].to_bits(),
                    "node {n} component {c}"
                );
            }
        }
        // the corner master 0 accumulated its three slaves
        assert_ne!(contrib.tuple(0)[0].to_bits(), input.tuple(0)[0].to_bits());
        let mut short = Array::filled(m.nb_nodes() - 1, 2, 0.0);
        assert!(matches!(
            crate::periodic_synchronizer::periodic_sum_contributions_real(&m, &mut short),
            Err(Error::DimensionMismatch { .. })
        ));
    }

    #[test]
    fn square_x_then_y_has_one_corner_master() {
        let mut m = unit_square(3);
        let _ = make_periodic(&mut m, SpatialDirection::X, None).unwrap();
        assert_eq!(m.periodic_master(3), Some(0));
        assert_eq!(m.periodic_master(7), Some(4));
        let _ = make_periodic(&mut m, SpatialDirection::Y, None).unwrap();
        assert!(m.is_periodic());
        // corners 3, 12, 15 are slaves of 0
        for s in [3, 12, 15] {
            assert_eq!(m.periodic_master(s), Some(0), "{s}");
        }
        let mut slaves = m.periodic_slaves(0).to_vec();
        slaves.sort_unstable();
        assert_eq!(slaves, vec![3, 12, 15]);
        assert!(m.node_flag(0).is_periodic_master() && m.node_flag(15).is_periodic_slave());
        // symmetry: every slave's master lists it
        for (m_, s) in periodic_pairs(&m) {
            assert_eq!(m.periodic_master(s), Some(m_));
            assert!(!m.node_flag(m_).is_periodic_slave());
        }
        let mut v = Array::filled(m.nb_nodes(), 1, 1.0);
        make_consistent_for_periodicity_real(&m, &mut v, ReduceOperation::Sum).unwrap();
        assert_eq!(v.as_slice()[0], 4.0);
        assert_eq!(v.as_slice()[15], 4.0);
        assert_eq!(v.as_slice()[1], 2.0);
        assert_eq!(v.as_slice()[5], 1.0);
        let mut b = Array::filled(m.nb_nodes(), 1, false);
        b.as_mut_slice()[12] = true;
        make_consistent_for_periodicity_bool(&m, &mut b, ReduceOperation::Or).unwrap();
        assert!(b.as_slice()[0] && b.as_slice()[3] && !b.as_slice()[1]);
        assert!(
            make_consistent_for_periodicity_real(&m, &mut Array::zeros(2, 1), ReduceOperation::Id)
                .is_err()
        );
        wipe_periodic_info(&mut m);
        assert!(!m.is_periodic() && m.node_flag(0).bits() == 0);
    }

    /// UD-812 state built directly: node 4 is a received pure-ghost
    /// periodic slave (global 40, from rank 1) whose transmitted master is
    /// global 20 = local node 2 (`master_known`) or unknown here; node 2 is
    /// already the master of node 3; node 5 is a plain node.
    fn ud812_state(compat: bool, master_known: bool) -> PeriodicState {
        let mut flags = vec![NodeFlag::NORMAL; 6];
        flags[2] = NodeFlag::PERIODIC_MASTER;
        flags[3] = NodeFlag::PERIODIC_SLAVE;
        flags[4] = NodeFlag(NodeFlag::PURE_GHOST.bits() | NodeFlag::PERIODIC_SLAVE.bits());
        PeriodicState {
            slave_master: HashMap::from([(3, 2)]),
            master_slaves: HashMap::from([(2, vec![3])]),
            flags,
            received_slaves: HashMap::from([(
                4,
                ReceivedSlave {
                    global: 40,
                    from: 1,
                    master_global: 20,
                    master_local: master_known.then_some(2),
                },
            )]),
            upstream_ghost_slave_master: compat,
        }
    }

    #[test]
    fn ud812_received_ghost_slave_takes_its_transmitted_master() {
        // as node1 and as node2 of the match: the class of node 2 gains node 5
        for (n1, n2) in [(4, 5), (5, 4)] {
            let mut st = ud812_state(false, true);
            st.match_found(n1, n2).unwrap();
            assert_eq!(st.slave_master.get(&5), Some(&2), "match ({n1}, {n2})");
            assert_eq!(st.master_slaves[&2], vec![3, 5]);
            assert!(st.flags[2].is_periodic_master() && st.flags[5].is_periodic_slave());
            // the received slave keeps its flags and gets no map entry
            assert!(!st.slave_master.contains_key(&4));
            assert!(!st.flags[0].is_periodic_master());
        }
    }

    #[test]
    fn ud812_compat_pairs_with_local_node_zero() {
        // upstream: periodic_slave_master[node] (operator[]) inserts
        // `node -> 0` and returns local node 0
        for (n1, n2) in [(4, 5), (5, 4)] {
            let mut st = ud812_state(true, true);
            st.match_found(n1, n2).unwrap();
            assert_eq!(st.slave_master.get(&4), Some(&0), "match ({n1}, {n2})");
            assert_eq!(st.slave_master.get(&5), Some(&0));
            assert_eq!(st.master_slaves[&0], vec![5]);
            assert!(st.flags[0].is_periodic_master() && st.flags[5].is_periodic_slave());
            // node 2's class is untouched
            assert_eq!(st.master_slaves[&2], vec![3]);
        }
        // also when the transmitted master is unknown here
        let mut st = ud812_state(true, false);
        st.match_found(4, 5).unwrap();
        assert_eq!(st.slave_master.get(&4), Some(&0));
    }

    #[test]
    fn ud812_unknown_transmitted_master_is_an_error() {
        for (n1, n2) in [(4, 5), (5, 4)] {
            let mut st = ud812_state(false, false);
            let e = st.match_found(n1, n2).unwrap_err().to_string();
            assert!(
                e.contains("40")
                    && e.contains("20")
                    && e.contains("rank 1")
                    && e.contains("UD-812"),
                "{e}"
            );
        }
    }

    #[test]
    fn bad_inputs_are_errors() {
        let mut m = unit_square(1);
        assert!(make_periodic(&mut m, SpatialDirection::Z, None).is_err());
        assert!(make_periodic_lists(&mut m, SpatialDirection::X, &[9], &[0]).is_err());
        assert!(make_periodic(&mut m, SpatialDirection::X, Some(("a", "b"))).is_err());
    }
}
