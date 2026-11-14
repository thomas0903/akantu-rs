// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/mesh_utils/{global_ids_updater.hh,
// global_ids_updater.cc, global_ids_updater_inline_impl.hh}.

//! Global ids of nodes appended to a distributed mesh (MUTIL-011,
//! PART-010; upstream `GlobalIdsUpdater`, used after cohesive node
//! doubling).
//!
//! The new nodes are the last `nb_new` local nodes, with their flags set
//! (local, master, slave or pure ghost) and owner ranks known for slaves
//! when available:
//!
//! 1. **locally** (`updateGlobalIDsLocally`): every rank counts its old and
//!    new local-or-master nodes; one rank-ordered all-reduce gives the old
//!    global count and the total of new owned nodes; an exclusive scan
//!    gives this rank's first new id; the new local-or-master nodes take
//!    consecutive ids after the old global count, in local order;
//! 2. **synchronisation** (`synchronizeGlobalIDs`) over an element
//!    synchronizer with the tag `_giu_global_conn`: a ghost → owner
//!    reduction where each rank sends, per node of each scheme element, the
//!    global id of the nodes it owns (−1 otherwise) and the node's owner
//!    rank; then an owner → ghost exchange with the ids of every node that is
//!    not a pure ghost. A slave receiving an id takes it and the sender's
//!    owner rank.
//!
//! On one rank nothing changes (upstream returns the local count). The
//! computation works on a copy of the ids and owner ranks and returns a
//! [`GlobalIdsUpdate`] that the caller applies (a mutation's distributed
//! hook through `MeshTransaction::set_node_global_id/set_node_prank`, or
//! [`apply_global_ids`] in set-up code). Point-to-point only (no
//! all-gather): one phase per exchange of the caller's scope.

use std::collections::BTreeMap;

use akantu_comm::DataAccessor;
use akantu_comm::buffer::CommunicationBuffer;
use akantu_comm::phase::Phases;
use akantu_comm::{ReduceOp, Synchronizer};
use akantu_core::enums::SynchronizationTag;
use akantu_core::{Element, Error, GlobalIdx, Idx, NodeFlag, Rank, Result, try_idx, try_to_usize};

use crate::mesh::Mesh;

/// Result of [`update_global_ids_in`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GlobalIdsUpdate {
    /// Global id of every local node (old ids kept; new owned nodes
    /// numbered; slaves as received, `-1` if nothing was received).
    pub global_ids: Vec<GlobalIdx>,
    /// Owner ranks set by the synchronisation (slaves), `(node, rank)`.
    pub pranks: BTreeMap<Idx, Rank>,
    /// New global node count.
    pub nb_global_nodes: usize,
    /// Number of new owned nodes over all ranks (upstream return value).
    pub total_new: usize,
}

/// Upstream `GlobalIdsUpdater::updateGlobalIDs(local_nb_new_nodes)`: see the
/// module docs. `sync` is the element synchronizer whose elements contain
/// the new nodes (upstream `CohesiveMeshGlobalDataUpdater`: the cohesive
/// model's `cohesive_synchronizer`, the cohesive elements split off the
/// mesh's element synchronizer and extended on every insertion along the
/// facet synchronizer's schemes, `updateCohesiveSynchronizers`; any element
/// synchronizer of `mesh` whose schemes match on both sides works); `None`
/// skips the synchronisation (upstream: mesh not distributed).
///
/// Re-checked for distributed facets (MUTIL-011 / PART-010): upstream synchronises
/// even when no rank created an owned node (`updateGlobalIDs` calls
/// `synchronizeGlobalIDs` whatever the count); skipped here, which is
/// equivalent — a new slave node always has a new owned counterpart on its
/// owner, so without new owned nodes nothing would be received.
/// New pure-ghost nodes get no id (upstream only updates slaves,
/// `isSlaveNode`), as here.
pub fn update_global_ids_in(
    ph: &mut Phases,
    mesh: &Mesh,
    sync: Option<&Synchronizer<Element>>,
    nb_new: usize,
) -> Result<GlobalIdsUpdate> {
    let n = mesh.nb_nodes();
    let mut global_ids: Vec<GlobalIdx> = (0..n)
        .map(|i| try_idx(i).map(|i| mesh.node_global_id(i)))
        .collect::<Result<_>>()?;
    let (_, size) = ph.rank_size();
    if size == 1 {
        return Ok(GlobalIdsUpdate {
            global_ids,
            pranks: BTreeMap::new(),
            nb_global_nodes: mesh.nb_global_nodes(),
            total_new: nb_new,
        });
    }
    // ---- locally ---------------------------------------------------------
    let prepared = (|| -> Result<[u64; 2]> {
        let old = n.checked_sub(nb_new).ok_or_else(|| {
            Error::InvalidArgument(format!("global ids: {nb_new} new nodes of {n}"))
        })?;
        let owned = |r: std::ops::Range<usize>| -> Result<u64> {
            let mut c = 0u64;
            for i in r {
                if mesh.node_flag(try_idx(i)?).is_local_or_master() {
                    c += 1;
                }
            }
            Ok(c)
        };
        Ok([owned(0..old)?, owned(old..n)?])
    })();
    let counts = ph.run_collective("giu:counts", prepared, |c, local| {
        let mut all = local;
        c.all_reduce(&mut all, ReduceOp::Sum)?;
        let start = c.exclusive_scan_sum_u64(local[1])?;
        Ok((all, start))
    })?;
    let ([old_global, total_new], start) = counts;
    let old = n - nb_new;
    let mut nb_global = usize::try_from(old_global).map_err(|_| Error::invalid("global count"))?;
    for g in &mut global_ids[old..] {
        *g = -1;
    }
    let mut pranks = BTreeMap::new();
    if total_new == 0 {
        return Ok(GlobalIdsUpdate {
            global_ids,
            pranks,
            nb_global_nodes: nb_global,
            total_new: 0,
        });
    }
    let mut next =
        GlobalIdx::try_from(old_global + start).map_err(|_| Error::invalid("id overflow"))?;
    for (i, g) in global_ids.iter_mut().enumerate().skip(old) {
        if mesh.node_flag(try_idx(i)?).is_local_or_master() {
            *g = next;
            next += 1;
        }
    }
    nb_global += usize::try_from(total_new).map_err(|_| Error::invalid("global count"))?;
    // ---- synchronisation ---------------------------------------------------
    if let Some(sync) = sync {
        let rank = ph.rank_size().0;
        let mut acc = GiuAccessor {
            mesh,
            global_ids: &mut global_ids,
            pranks: &mut pranks,
            reduce: true,
            rank,
        };
        sync.slave_reduction_in(ph, &mut acc, SynchronizationTag::GiuGlobalConn)?;
        acc.reduce = false;
        sync.synchronize_in(ph, &mut acc, SynchronizationTag::GiuGlobalConn)?;
    }
    Ok(GlobalIdsUpdate {
        global_ids,
        pranks,
        nb_global_nodes: nb_global,
        total_new: usize::try_from(total_new).map_err(|_| Error::invalid("count"))?,
    })
}

/// Apply an update to a mesh being set up (global ids, slave owner ranks,
/// global node count).
pub fn apply_global_ids(mesh: &mut Mesh, update: &GlobalIdsUpdate) -> Result<()> {
    if update.global_ids.len() != mesh.nb_nodes() {
        return Err(Error::DimensionMismatch {
            context: "apply_global_ids",
            expected: mesh.nb_nodes(),
            got: update.global_ids.len(),
        });
    }
    let mut a = akantu_core::Array::from_vec(update.global_ids.clone(), 1)?;
    a.set_id(format!("{}:nodes_global_ids", mesh.id()));
    mesh.nodes_global_ids = Some(a);
    for (&n, &r) in &update.pranks {
        mesh.nodes_prank.insert(n, r);
    }
    mesh.nb_global_nodes = update.nb_global_nodes;
    Ok(())
}

/// `_giu_global_conn` accessor (upstream `GlobalIdsUpdater` pack/unpack):
/// the sender's rank, then per node of each element `(global id or -1,
/// owner rank)`.
struct GiuAccessor<'a> {
    mesh: &'a Mesh,
    global_ids: &'a mut Vec<GlobalIdx>,
    pranks: &'a mut BTreeMap<Idx, Rank>,
    reduce: bool,
    rank: Rank,
}

impl GiuAccessor<'_> {
    fn flag(&self, n: Idx) -> NodeFlag {
        self.mesh.node_flag(n)
    }
}

impl DataAccessor<Element> for GiuAccessor<'_> {
    fn nb_data(&self, elements: &[Element], tag: SynchronizationTag) -> usize {
        if tag != SynchronizationTag::GiuGlobalConn {
            return 0;
        }
        let nodes: usize = elements
            .iter()
            .map(|e| e.ty().nb_nodes_per_element())
            .fold(0usize, usize::saturating_add);
        nodes.saturating_mul(8 + 4).saturating_add(4)
    }
    fn pack_data(
        &self,
        buffer: &mut CommunicationBuffer,
        elements: &[Element],
        tag: SynchronizationTag,
    ) -> Result<()> {
        if tag != SynchronizationTag::GiuGlobalConn {
            return Ok(());
        }
        buffer.pack(&self.rank);
        for &e in elements {
            for &node in &self.mesh.element_connectivity(e)? {
                let f = self.flag(node);
                let send =
                    (self.reduce && f.is_local_or_master()) || (!self.reduce && !f.is_pure_ghost());
                let index: GlobalIdx = if send {
                    self.global_ids[try_to_usize(node)?]
                } else {
                    -1
                };
                buffer.pack(&index);
                let owner: Rank = if f.is_local_or_master() {
                    self.rank
                } else {
                    self.pranks
                        .get(&node)
                        .copied()
                        .unwrap_or_else(|| self.mesh.node_prank(node))
                };
                buffer.pack(&owner);
            }
        }
        Ok(())
    }
    fn unpack_data(
        &mut self,
        buffer: &mut CommunicationBuffer,
        elements: &[Element],
        tag: SynchronizationTag,
    ) -> Result<()> {
        if tag != SynchronizationTag::GiuGlobalConn {
            return Ok(());
        }
        let proc: Rank = buffer.unpack()?;
        for &e in elements {
            for &node in &self.mesh.element_connectivity(e)? {
                let index: GlobalIdx = buffer.unpack()?;
                let owner: Rank = buffer.unpack()?;
                if index == -1 || !self.flag(node).is_slave() {
                    continue;
                }
                let g = &mut self.global_ids[try_to_usize(node)?];
                // upstream: debug assertion
                if *g != -1 && *g != index {
                    return Err(Error::Communication(format!(
                        "the node {node} already has a global id {g}, from proc {proc}, different \
                         from the one received {index}"
                    )));
                }
                *g = index;
                self.pranks.insert(node, owner);
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MeshAccessor;
    use crate::partition::{MeshPartitioner, RcbPartitioner};
    use akantu_comm::run_local;
    use akantu_core::{ElementType, GhostType, TypeFilter};

    fn grid(comm: akantu_comm::Communicator, n: usize) -> Mesh {
        let mut m = Mesh::with_communicator(2, "g", comm.clone()).unwrap();
        if comm.rank() == 0 {
            let mut a = MeshAccessor::new(&mut m);
            for j in 0..=n {
                for i in 0..=n {
                    a.push_node(&[i as f64, j as f64]).unwrap();
                }
            }
            let w = (n + 1) as Idx;
            let mut c = a.connectivity_mut(ElementType::Quadrangle4, GhostType::NotGhost);
            for j in 0..n as Idx {
                for i in 0..n as Idx {
                    let n0 = j * w + i;
                    c.push_tuple(&[n0, n0 + 1, n0 + w + 1, n0 + w]);
                }
            }
        }
        let mut p = RcbPartitioner;
        let _ = crate::distribution::distribute_with(
            &mut m,
            Some(&mut p as &mut dyn MeshPartitioner),
            0,
        )
        .unwrap();
        m
    }

    /// The last global nodes (ids ≥ `cut`) are moved to the end of every
    /// rank's numbering and forgotten (id −1, global count = `cut`); the
    /// updater gives the owned ones consecutive ids from `cut` in rank
    /// order and the slaves their master's id: every slave's id equals the
    /// id its owner gave the same element node (checked through the element
    /// synchronizer with the original ids as reference).
    #[test]
    fn new_nodes_get_consistent_global_ids() {
        for size in [1usize, 2, 4] {
            let out = run_local(
                size,
                std::time::Duration::from_secs(30),
                |comm| -> std::result::Result<(), String> {
                    let mut m = grid(comm.clone(), 6);
                    let total = m.nb_global_nodes();
                    let cut = total - 9;
                    // permutation: old nodes (id < cut) first, then new ones
                    let n = m.nb_nodes();
                    let mut order: Vec<usize> = (0..n).collect();
                    order.sort_by_key(|&i| (m.node_global_id(i as Idx) >= cut as i64, i));
                    let mut new_of_old = vec![0 as Idx; n];
                    for (new, &old) in order.iter().enumerate() {
                        new_of_old[old] = new as Idx;
                    }
                    let nb_new = order
                        .iter()
                        .filter(|&&i| m.node_global_id(i as Idx) >= cut as i64)
                        .count();
                    let reference: Vec<i64> =
                        order.iter().map(|&i| m.node_global_id(i as Idx)).collect();
                    {
                        let x: Vec<f64> = order
                            .iter()
                            .flat_map(|&i| m.nodes().tuple(i).to_vec())
                            .collect();
                        let f: Vec<NodeFlag> =
                            order.iter().map(|&i| m.node_flag(i as Idx)).collect();
                        let pr: BTreeMap<Idx, Rank> = m
                            .nodes_prank
                            .iter()
                            .map(|(&k, &v)| (new_of_old[k as usize], v))
                            .collect();
                        m.nodes = akantu_core::Array::from_vec(x, 2).unwrap().into();
                        m.nodes_flags = akantu_core::Array::from_vec(f, 1).unwrap();
                        m.nodes_prank = pr.into_iter().collect();
                        let g: Vec<i64> = reference
                            .iter()
                            .map(|&g| if g >= cut as i64 { -1 } else { g })
                            .collect();
                        m.nodes_global_ids = Some(akantu_core::Array::from_vec(g, 1).unwrap());
                        m.nb_global_nodes = cut;
                        for ghost in GhostType::ALL {
                            for ty in m
                                .element_types(TypeFilter::new().ghost(ghost))
                                .collect::<Vec<_>>()
                            {
                                if let Some(a) = m.connectivities.as_map_mut().get_mut(ty, ghost) {
                                    for v in a.borrow_mut().as_mut_slice() {
                                        *v = new_of_old[*v as usize];
                                    }
                                }
                            }
                        }
                    }
                    let es = m.synchronizers().element.clone();
                    let mut ph = Phases::new(&comm, "giu-test");
                    let up = update_global_ids_in(&mut ph, &m, es.as_ref(), nb_new);
                    let up = ph.settle(up).map_err(|e| e.to_string())?;
                    drop(ph);
                    if size == 1 {
                        return Ok(());
                    }
                    if up.nb_global_nodes != total || up.total_new != 9 {
                        return Err(format!("counts {} / {}", up.nb_global_nodes, up.total_new));
                    }
                    // ids of new nodes are a permutation of cut..total over the
                    // owners; slaves agree with their owners (element exchange)
                    apply_global_ids(&mut m, &up).map_err(|e| e.to_string())?;
                    for i in 0..m.nb_nodes() {
                        let f = m.node_flag(i as Idx);
                        let g = m.node_global_id(i as Idx);
                        if !f.is_pure_ghost() && (g < 0 || g >= total as i64) {
                            return Err(format!("node {i} (flag {:#x}) id {g}", f.bits()));
                        }
                    }
                    // consistency: owner → ghost element exchange of the new ids
                    let sync = m.synchronizers().element.clone().unwrap();
                    let mut ids: Vec<i64> = (0..m.nb_nodes())
                        .map(|i| m.node_global_id(i as Idx))
                        .collect();
                    let mut acc = ElementIds {
                        m: &m,
                        ids: &mut ids,
                        bad: 0,
                    };
                    sync.synchronize(&mut acc, SynchronizationTag::Test)
                        .map_err(|e| e.to_string())?;
                    if acc.bad > 0 {
                        return Err(format!(
                            "{} element nodes disagree with their owner",
                            acc.bad
                        ));
                    }
                    Ok(())
                },
            );
            for (r, o) in out.into_iter().enumerate() {
                if let Err(e) = o {
                    panic!("{size} ranks, rank {r}: {e}");
                }
            }
        }
    }

    /// Owners send their element nodes' ids; ghosts compare the ids of
    /// their non-pure-ghost nodes.
    struct ElementIds<'a> {
        m: &'a Mesh,
        ids: &'a mut Vec<i64>,
        bad: usize,
    }

    impl DataAccessor<Element> for ElementIds<'_> {
        fn nb_data(&self, els: &[Element], _: SynchronizationTag) -> usize {
            els.iter().map(|e| e.ty().nb_nodes_per_element() * 8).sum()
        }
        fn pack_data(
            &self,
            b: &mut CommunicationBuffer,
            els: &[Element],
            _: SynchronizationTag,
        ) -> Result<()> {
            for &e in els {
                for &n in &self.m.element_connectivity(e)? {
                    b.pack(&self.ids[n as usize]);
                }
            }
            Ok(())
        }
        fn unpack_data(
            &mut self,
            b: &mut CommunicationBuffer,
            els: &[Element],
            _: SynchronizationTag,
        ) -> Result<()> {
            for &e in els {
                for &n in &self.m.element_connectivity(e)? {
                    let g: i64 = b.unpack()?;
                    if !self.m.node_flag(n).is_pure_ghost() && self.ids[n as usize] != g {
                        self.bad += 1;
                    }
                }
            }
            Ok(())
        }
    }
}
