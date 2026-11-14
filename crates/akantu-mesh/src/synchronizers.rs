// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/synchronizer/{element_synchronizer.hh,
// element_synchronizer.cc, node_synchronizer.hh, node_synchronizer.cc,
// node_synchronizer_inline_impl.hh, periodic_node_synchronizer.hh,
// facet_synchronizer.hh}.

//! Ghost-exchange schemes owned by the mesh (upstream `Mesh` owns
//! `ElementSynchronizer`, `NodeSynchronizer`, `PeriodicNodeSynchronizer`):
//! MPI-016/017 on the generic [`Synchronizer`] (MPI-007…015).
//!
//! * **Element synchronizer** (`<mesh id>:element_synchronizer`): send
//!   scheme toward rank `p` = local elements of the mesh dimension that
//!   are ghosts on `p` (local order, type by type); receive scheme from `p`
//!   = ghost elements owned by `p` (ghost order). Built by
//!   [`crate::distribution`]; element events update it
//!   ([`substitute_elements`], [`renumber_elements`]). Element removals and
//!   changes of a distributed mesh reach it through the distributed hook
//!   [`crate::distributed_removal::DistributedRemovalHook`] (upstream
//!   `ElementSynchronizer::onElementsRemoved`: the collective
//!   `filterScheme`, then `renumberElements`; `onElementsChanged`:
//!   `substituteElements`; MPI-016).
//! * **Node synchronizer** (`<mesh id>:node_synchronizer`): send scheme
//!   toward `p` = nodes mastered here that are slaves on `p`; receive
//!   scheme from `p` = slave nodes owned by `p` — both sorted by global id;
//!   **pure ghost nodes are in no node scheme** (they follow the element
//!   synchronizer). After node additions the receive schemes are rebuilt
//!   from the slave flags and the send schemes from the slaves' global ids
//!   ([`rebuild_node_schemes_in`], upstream `onNodesAdded`).
//!   [`NodeEntities`] provides the root gather/scatter mapping (upstream
//!   `fillEntityToSend`, `canScatterSize`, `gatheredSize`).
//! * **Debug sanity checks** (MPI-014): [`enable_sanity_checks`] attaches
//!   hooks checking barycentres and global connectivities (elements) and
//!   global ids, flags and positions (nodes), from a snapshot of the mesh.
//!
//! Synchronizers are values inside [`MeshSynchronizers`]; accessors are
//! passed per call (no registry of raw pointers). Entity-agnostic and
//! point-to-point only (no all-gathers), so that cohesive insertion
//! can reuse them.

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use akantu_comm::buffer::CommunicationBuffer;
use akantu_comm::phase::Phases;
use akantu_comm::synchronizer::{SanityCheck, ScatterGatherEntities};
use akantu_comm::{Synchronizer, Tag};
use akantu_core::enums::SynchronizationTag;
use akantu_core::{
    Element, ElementTypeMapArray, Error, GlobalIdx, Idx, NodeFlag, NodeIdx, Rank, Real, Result,
    try_idx, try_to_usize,
};

use crate::mesh::Mesh;

/// Element ghost exchanges (upstream `ElementSynchronizer`).
pub type ElementSynchronizer = Synchronizer<Element>;

/// Shared-node exchanges (upstream `NodeSynchronizer`).
pub type NodeSynchronizer = Synchronizer<NodeIdx>;

/// All synchronizers of a mesh (`None` in serial, non-periodic runs).
#[derive(Clone, Debug, Default)]
pub struct MeshSynchronizers {
    /// Element synchronizer.
    pub element: Option<ElementSynchronizer>,
    /// Node synchronizer.
    pub node: Option<NodeSynchronizer>,
    /// Periodic node synchronizer (master ↔ slave).
    pub periodic_node: Option<NodeSynchronizer>,
}

// ------------------------------------------------------- gather / scatter

/// Root gather/scatter mapping of the node synchronizer (upstream
/// `NodeSynchronizer::fillEntityToSend`, `canScatterSize`,
/// `gatheredSize`): the local-or-master nodes in local order with their
/// global ids; local size = local node count, global size = global node
/// count.
#[derive(Clone, Copy, Debug)]
pub struct NodeEntities<'a> {
    /// The distributed mesh.
    pub mesh: &'a Mesh,
}

impl ScatterGatherEntities for NodeEntities<'_> {
    fn owned_entities(&self) -> (Vec<Idx>, Vec<Idx>) {
        let mut local = Vec::new();
        let mut global = Vec::new();
        for n in 0..self.mesh.nb_nodes() {
            let Ok(n) = try_idx(n) else { break };
            if self.mesh.node_flag(n).is_local_or_master() {
                local.push(n);
                global.push(Idx::try_from(self.mesh.node_global_id(n)).unwrap_or(-1));
            }
        }
        (local, global)
    }
    fn local_size(&self) -> usize {
        self.mesh.nb_nodes()
    }
    fn global_size(&self) -> usize {
        self.mesh.nb_global_nodes()
    }
}

// ----------------------------------------------------------- sanity checks

/// Element sanity data (upstream `ElementSynchronizer::packSanityCheckData`):
/// barycentre and global connectivity of every element, snapshotted.
struct ElementSanity {
    dim: usize,
    barycenters: ElementTypeMapArray<Real>,
    global_conn: ElementTypeMapArray<GlobalIdx>,
    pure_ghost: ElementTypeMapArray<bool>,
}

impl ElementSanity {
    fn of(mesh: &Mesh) -> Result<Self> {
        let dim = mesh.spatial_dimension();
        let mut barycenters = ElementTypeMapArray::new("sanity:barycenters");
        let mut global_conn = ElementTypeMapArray::new("sanity:global_conn");
        let mut pure_ghost = ElementTypeMapArray::new("sanity:pure_ghost");
        for (ty, ghost, conn) in mesh.connectivities().as_map().iter_all() {
            barycenters.alloc(0, dim, ty, ghost, 0.0)?;
            *barycenters.get_mut(ty, ghost)? = mesh.barycenters(ty, ghost)?;
            let nnpe = conn.nb_component();
            let g = global_conn.alloc(conn.len(), nnpe, ty, ghost, 0)?;
            let pg = pure_ghost.alloc(conn.len(), nnpe, ty, ghost, false)?;
            for ((gv, pv), &l) in g
                .as_mut_slice()
                .iter_mut()
                .zip(pg.as_mut_slice())
                .zip(conn.borrow().as_slice())
            {
                *gv = mesh.node_global_id(l);
                *pv = mesh.node_flag(l).is_pure_ghost();
            }
        }
        Ok(Self {
            dim,
            barycenters,
            global_conn,
            pure_ghost,
        })
    }
}

impl SanityCheck<Element> for ElementSanity {
    fn data_size(&self, entities: &[Element], _tag: SynchronizationTag) -> usize {
        entities
            .iter()
            .map(|e| (e.ty().nb_nodes_per_element() * 8).saturating_add(self.dim * 8))
            .fold(0usize, usize::saturating_add)
    }
    fn pack(
        &self,
        buffer: &mut CommunicationBuffer,
        entities: &[Element],
        _tag: SynchronizationTag,
    ) -> Result<()> {
        for &e in entities {
            buffer.pack_slice(self.barycenters.element(e)?);
            buffer.pack_slice(self.global_conn.element(e)?);
        }
        Ok(())
    }
    fn unpack(
        &self,
        buffer: &mut CommunicationBuffer,
        entities: &[Element],
        tag: SynchronizationTag,
        proc: Rank,
        rank: Rank,
    ) -> Result<()> {
        let skip_conn = matches!(
            tag,
            SynchronizationTag::SmmcFacetsConn | SynchronizationTag::GiuGlobalConn
        );
        let mut bary = vec![0.0; self.dim];
        for &e in entities {
            buffer.unpack_slice(&mut bary)?;
            let local = self.barycenters.element(e)?;
            let dist = local
                .iter()
                .zip(&bary)
                .map(|(a, b)| (a - b) * (a - b))
                .sum::<Real>()
                .sqrt();
            // upstream Math::are_float_equal(dist, 0.) with the default
            // Math::tolerance
            if !positions_match(dist) {
                return Err(Error::Communication(format!(
                    "unpacking an unknown value for the element {e} (barycenter {local:?} != \
                     buffer {bary:?}) [{dist}] - tag: {tag} comm from {proc} to {rank}"
                )));
            }
            let local_conn = self.global_conn.element(e)?;
            let mut remote = vec![0 as GlobalIdx; local_conn.len()];
            buffer.unpack_slice(&mut remote)?;
            let pg = self.pure_ghost.element(e)?;
            // Upstream only emits a debug warning on a connectivity
            // mismatch (global ids of the not-pure-ghost nodes differ) and
            // continues; the port has no warning channel, so the mismatch
            // is computed but not reported (documented in
            // `enable_sanity_checks`).
            let _same = skip_conn
                || local_conn
                    .iter()
                    .zip(&remote)
                    .zip(pg)
                    .all(|((a, b), &p)| p || a == b);
        }
        Ok(())
    }
}

/// Node sanity data (upstream `NodeSynchronizer::packSanityCheckData`):
/// global id (except for `_giu_global_conn`), flag and position.
struct NodeSanity {
    dim: usize,
    global: Vec<GlobalIdx>,
    flags: Vec<NodeFlag>,
    positions: Vec<Real>,
}

impl SanityCheck<Idx> for NodeSanity {
    fn data_size(&self, entities: &[Idx], tag: SynchronizationTag) -> usize {
        let per = if tag == SynchronizationTag::GiuGlobalConn {
            0
        } else {
            8
        } + 1
            + self.dim * 8;
        entities.len().saturating_mul(per)
    }
    fn pack(
        &self,
        buffer: &mut CommunicationBuffer,
        entities: &[Idx],
        tag: SynchronizationTag,
    ) -> Result<()> {
        for &n in entities {
            let u = try_to_usize(n)?;
            if tag != SynchronizationTag::GiuGlobalConn {
                buffer.pack(self.global.get(u).ok_or_else(|| range(u))?);
            }
            buffer.pack(self.flags.get(u).ok_or_else(|| range(u))?);
            buffer.pack_slice(
                self.positions
                    .get(u * self.dim..(u + 1) * self.dim)
                    .ok_or_else(|| range(u))?,
            );
        }
        Ok(())
    }
    fn unpack(
        &self,
        buffer: &mut CommunicationBuffer,
        entities: &[Idx],
        tag: SynchronizationTag,
        proc: Rank,
        rank: Rank,
    ) -> Result<()> {
        let mut pos = vec![0.0; self.dim];
        for &n in entities {
            let u = try_to_usize(n)?;
            if tag != SynchronizationTag::GiuGlobalConn {
                let g: GlobalIdx = buffer.unpack()?;
                let mine = *self.global.get(u).ok_or_else(|| range(u))?;
                if g != mine {
                    return Err(Error::Communication(format!(
                        "the nodes global ids do not match: {g} != {mine}"
                    )));
                }
            }
            let flag: NodeFlag = buffer.unpack()?;
            let mine = *self.flags.get(u).ok_or_else(|| range(u))?;
            let sh = |f: NodeFlag| f.bits() & NodeFlag::SHARED_MASK.bits();
            let pe = |f: NodeFlag| f.bits() & NodeFlag::PERIODIC_MASK.bits();
            let ok = pe(flag) == pe(mine)
                && ((sh(flag) == NodeFlag::MASTER.bits() && sh(mine) == NodeFlag::SLAVE.bits())
                    || (sh(flag) == NodeFlag::SLAVE.bits() && sh(mine) == NodeFlag::MASTER.bits())
                    || sh(mine) == NodeFlag::PURE_GHOST.bits()
                    || sh(flag) == NodeFlag::PURE_GHOST.bits());
            if !ok {
                return Err(Error::Communication(format!(
                    "the node flags: {:#x} and {:#x}",
                    flag.bits(),
                    mine.bits()
                )));
            }
            buffer.unpack_slice(&mut pos)?;
            let local = self
                .positions
                .get(u * self.dim..(u + 1) * self.dim)
                .ok_or_else(|| range(u))?;
            let dist = local
                .iter()
                .zip(&pos)
                .map(|(a, b)| (a - b) * (a - b))
                .sum::<Real>()
                .sqrt();
            if !positions_match(dist) {
                return Err(Error::Communication(format!(
                    "unpacking an unknown value for the node {n} (position {local:?} != buffer \
                     {pos:?}) [{dist}] - tag: {tag} comm from {proc} to {rank}"
                )));
            }
        }
        Ok(())
    }
}

/// Upstream `Math::are_float_equal(dist, 0.)` with the default
/// `Math::tolerance` (1e2 ε): `|dist| <= 1e2 ε max(|dist|, 1)`.
fn positions_match(dist: Real) -> bool {
    akantu_core::math::are_float_equal(dist, 0.0, akantu_core::math::DEFAULT_TOLERANCE)
}

fn range(u: usize) -> Error {
    Error::Communication(format!("sanity check: node {u} outside the snapshot"))
}

/// Turn the debug sanity checks of the element and node synchronizers on
/// (or off) with hooks built from the current mesh state (upstream: debug
/// builds). Must be called identically on every rank and again after the
/// mesh changes.
///
/// As upstream, a barycentre or node position differing from the sender's
/// by more than `Math::are_float_equal(dist, 0)` allows (default tolerance
/// 1e2 ε, relative to max(dist, 1)), different node global ids or
/// incompatible node flags fail the exchange; an element connectivity
/// mismatch (upstream: a debug warning only) is not reported.
pub fn enable_sanity_checks(mesh: &mut Mesh, enabled: bool) -> Result<()> {
    let element = if enabled {
        Some(Arc::new(ElementSanity::of(mesh)?))
    } else {
        None
    };
    let node = if enabled {
        Some(Arc::new(NodeSanity {
            dim: mesh.spatial_dimension(),
            global: (0..mesh.nb_nodes())
                .map(|n| try_idx(n).map(|n| mesh.node_global_id(n)))
                .collect::<Result<_>>()?,
            flags: mesh.nodes_flags().as_slice().to_vec(),
            positions: mesh.nodes().as_slice().to_vec(),
        }))
    } else {
        None
    };
    if let Some(s) = &mut mesh.synchronizers.element {
        s.set_sanity_checks(enabled);
        if let Some(h) = element {
            s.set_sanity_check_hook(h);
        }
    }
    if let Some(s) = &mut mesh.synchronizers.node {
        s.set_sanity_checks(enabled);
        if let Some(h) = node {
            s.set_sanity_check_hook(h);
        }
    }
    Ok(())
}

// ---------------------------------------------------- scheme maintenance

/// Replace elements of every scheme (upstream
/// `ElementSynchronizer::substituteElements`, the `onElementsChanged`
/// reaction): `old → new`, others kept.
pub fn substitute_elements(
    sync: &mut ElementSynchronizer,
    old_to_new: &BTreeMap<Element, Element>,
) {
    sync.update_schemes(|scheme, _, _| {
        for el in scheme.iter_mut() {
            if let Some(n) = old_to_new.get(el) {
                *el = *n;
            }
        }
    });
}

/// Renumber the elements of every scheme (upstream
/// `ElementSynchronizer::renumberElements`): elements of a `(type, ghost)`
/// present in `new_numbering` take their new index (upstream reads the
/// numbering without checking removals: callers filter removed elements
/// first, `Synchronizer::filter_scheme`).
pub fn renumber_elements(
    sync: &mut ElementSynchronizer,
    new_numbering: &ElementTypeMapArray<Idx>,
) -> Result<()> {
    let mut err = None;
    sync.update_schemes(|scheme, _, _| {
        for el in scheme.iter_mut() {
            if let Ok(num) = new_numbering.get(el.ty(), el.ghost_type()) {
                match num.as_slice().get(el.idx()) {
                    Some(&n) if n >= 0 => *el = Element::new(el.ty(), n, el.ghost_type()),
                    _ => {
                        err.get_or_insert_with(|| {
                            Error::Mesh(format!(
                                "renumber_elements: {el} removed or outside the numbering"
                            ))
                        });
                    }
                }
            }
        }
    });
    err.map_or(Ok(()), Err)
}

/// Tag of the node-scheme rebuild messages (upstream `0xcafe`).
const NODE_SCHEME_TAG: Tag = Tag(29_000);

/// Rebuild the node synchronizer's schemes after node additions (upstream
/// `NodeSynchronizer::onNodesAdded`): the receive scheme of rank `p` =
/// slave nodes whose owner is `p`, in local order; each slave rank sends
/// the global ids of those nodes to their owner (one message per receive
/// scheme, empty ones included for ranks that had a scheme), which
/// rebuilds its send scheme toward that rank from them. One phase of the
/// enclosing scope `ph` (a mutation's distributed hook); point-to-point
/// only.
///
/// The ranks this rank receives from must know to expect a message:
/// upstream keeps the existing send schemes' ranks; so does this function
/// (the set of neighbours does not change on node addition; a new
/// neighbour pair is an error).
pub fn rebuild_node_schemes_in(
    ph: &mut Phases,
    mesh: &Mesh,
    sync: &mut NodeSynchronizer,
) -> Result<()> {
    // local preparation
    type Prepared = (BTreeMap<Rank, Vec<Idx>>, BTreeMap<Rank, Vec<GlobalIdx>>);
    let prepared = (|| -> Result<Prepared> {
        let mut recv: BTreeMap<Rank, Vec<Idx>> = sync
            .communications
            .recv
            .keys()
            .map(|&r| (r, Vec::new()))
            .collect();
        let mut globals: BTreeMap<Rank, Vec<GlobalIdx>> =
            recv.keys().map(|&r| (r, Vec::new())).collect();
        for n in 0..mesh.nb_nodes() {
            let n = try_idx(n)?;
            if !mesh.node_flag(n).is_slave() {
                continue;
            }
            let p = mesh.node_prank(n);
            if p < 0 {
                return Err(Error::Mesh(format!(
                    "the node {n} does not have a valid associated prank"
                )));
            }
            if !recv.contains_key(&p) {
                // a new neighbour would not be expected by its owner
                return Err(Error::Mesh(format!(
                    "node scheme rebuild: new neighbour rank {p} (not supported)"
                )));
            }
            recv.entry(p).or_default().push(n);
            globals.entry(p).or_default().push(mesh.node_global_id(n));
        }
        Ok((recv, globals))
    })();
    let incoming: Vec<Rank> = sync.communications.send.keys().copied().collect();
    let received = ph.run("node_schemes", prepared, |comm, (recv, globals)| {
        let mut sends = Vec::with_capacity(globals.len());
        for (&p, gs) in &globals {
            sends.push(comm.isend(gs.clone(), p, NODE_SCHEME_TAG)?);
        }
        let mut got = Vec::with_capacity(incoming.len());
        for &p in &incoming {
            got.push((p, comm.receive_vec::<GlobalIdx>(p, NODE_SCHEME_TAG)?));
        }
        for s in sends {
            s.wait()?;
        }
        // post-processing: global → local
        let index: HashMap<GlobalIdx, Idx> = (0..mesh.nb_nodes())
            .filter_map(|n| try_idx(n).ok())
            .rev()
            .map(|n| (mesh.node_global_id(n), n))
            .collect();
        let mut send = BTreeMap::new();
        for (p, gs) in got {
            let mut ls = Vec::with_capacity(gs.len());
            for g in gs {
                match index.get(&g) {
                    Some(&l) => ls.push(l),
                    None => {
                        return Ok(Err(Error::Mesh(format!(
                            "the global node {g} is not known on rank {}",
                            comm.rank()
                        ))));
                    }
                }
            }
            send.insert(p, ls);
        }
        Ok(Ok((recv, send)))
    })?;
    let (recv, send) = received;
    sync.communications.recv = recv;
    sync.communications.send = send;
    sync.set_entities_changed();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The position check is upstream's
    /// `Math::are_float_equal(dist, 0.)` with the default tolerance
    /// (1e2 ε ≈ 2.2e-14), not plain machine epsilon.
    #[test]
    fn sanity_position_tolerance_is_upstream_are_float_equal() {
        assert!(positions_match(0.0));
        assert!(positions_match(1e-15)); // refused before the fix (> ε)
        assert!(positions_match(2.0e-14));
        assert!(!positions_match(3.0e-14));
        assert!(!positions_match(1e-12));
        let s = NodeSanity {
            dim: 2,
            global: vec![7],
            flags: vec![NodeFlag::SLAVE],
            positions: vec![1.0, 2.0],
        };
        for (shift, ok) in [(1e-15, true), (1e-12, false)] {
            let mut b = CommunicationBuffer::new();
            b.pack(&(7 as GlobalIdx));
            b.pack(&NodeFlag::MASTER);
            b.pack_slice(&[1.0 + shift, 2.0]);
            let mut b = CommunicationBuffer::from_bytes(b.into_bytes());
            let r = s.unpack(&mut b, &[0], SynchronizationTag::Test, 1, 0);
            assert_eq!(r.is_ok(), ok, "shift {shift}: {r:?}");
        }
    }
}
