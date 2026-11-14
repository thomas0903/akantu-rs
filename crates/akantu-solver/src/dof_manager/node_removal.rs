// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/model/common/dof_manager/dof_manager.cc
// (onNodesRemoved :901-902, empty upstream; the numbering protocol of
// registerDOFs / updateDOFsData :590-626, :768-870 it re-runs),
// src/mesh/mesh_inline_impl.hh (removeNodesFromArray :126-145, the
// compaction rule of the nodal arrays).

//! Node removal with DOF renumbering (SMM-015, UD-209).
//!
//! Upstream's `DOFManager::onNodesRemoved` is empty (`dof_manager.cc:
//! 901-902`): after `SolidMechanicsModel::onNodesRemoved` compacted the
//! model's nodal arrays (`removeNodesFromArray`), the equation numbers,
//! associated nodes and global numbering stay those of the old node set
//! (UD-209). The port renumbers (D-003 category B, corrected; no compat
//! mode: the stale numbering indexes the compacted arrays out of range or
//! assembles into the equations of other DOFs, which no program can rely
//! on).
//!
//! # Algorithm (normative)
//!
//! A removal is followed by a **fresh numbering**: the numbering a
//! registration of the same families, in registration order, on the
//! current mesh would produce ([`DofManager::register_nodal_dofs`] /
//! [`DofManager::register_generic_dofs`]):
//!
//! 1. **Tuples.** A whole-mesh family keeps one tuple per node, the tuple
//!    of a node being its index: tuple `t` moves to `new_numbering[t]`
//!    (upstream `removeNodesFromArray`), removed tuples are dropped. A
//!    group-supported family takes the order of its group's node list in
//!    the mesh (as a registration on the group does); every surviving
//!    tuple must be in that list and the list must hold no other node.
//!    Generic families are unchanged.
//! 2. **Arrays.** Values, every derivative, increment, previous values,
//!    blocked flags and the split solution follow their tuples.
//! 3. **Equations.** One block per family, families in registration order,
//!    nodes in tuple order, components innermost; periodic slaves alias
//!    their master's equation (`plan_block`). The node-addition blocks of
//!    earlier insertions are collapsed into the family's single block.
//! 4. **Global numbers** (collective, `number_block`): owned DOFs get
//!    `exclusive_scan(nb_owned)` per block from a fresh offset; shared
//!    (slave) DOFs receive their owner's numbers through the node
//!    synchronizer (`_ask_nodes`; on a distributed periodic mesh the
//!    periodic masters' numbers first, `_giu_global_conn`). The node
//!    synchronizer's schemes must already follow the removal (the mesh's
//!    distributed removal hook, `akantu_mesh::distributed_removal::DistributedRemovalHook`).
//! 5. **Per-equation state** follows the DOFs through the old → new local
//!    equation map: residual, solution of the last solve, every lumped
//!    matrix and the blocked snapshot of the last
//!    [`DofManager::update_global_blocked_dofs`]; the lumped block
//!    couplings (D-059) are dropped and **every** matrix loses its profile
//!    and values (`resize_global_arrays`), so the owners reassemble them.
//!
//! The result is independent of the numbering history: after a removal the
//! manager equals a manager registered afresh on the current mesh with the
//! surviving values (`crates/akantu-solver/tests/dof_node_removal.rs`).
//!
//! # Composition with other node events of one mutation
//!
//! Consumers receive every event of a mutation after the commit, with the
//! **final** mesh (`docs/ARCHITECTURE.md`, "Composition of mesh
//! events"). An event names nodes in the numbering current when it was
//! recorded, so a consumer reading the final mesh at those indices reads
//! another node when a later node event of the mutation renumbered them.
//! Rule: **record node removals before node additions**, and compose the
//! removals of one mutation into one numbering where a family reads the
//! mesh. The capability check refuses, recoverably and before the commit:
//!
//! * a `NodesAdded` naming a node beyond the final count (an addition
//!   followed by a removal);
//! * a `NodesRemoved` preceded by an addition (its numbering maps more
//!   nodes than the whole-mesh families hold), even a pure permutation;
//! * with a group-supported family, a `NodesRemoved` preceded by another
//!   removal (its numbering maps fewer nodes than held), and the group part
//!   of the renumbering of the first node event (planned in the check on
//!   the final mesh's group lists: a later addition joining the group is
//!   refused there);
//! * on a distributed or periodic mesh, a `NodesRemoved` that is not the
//!   last node event (its kept count differs from the final node count):
//!   the renumbering reads the flags and periodic pairs of the final mesh.
//!   This guard is unreachable with today's hooks — `DistributedRemovalHook`
//!   refuses node additions and a second removal, and no transaction
//!   creates periodic pairs — and is kept for future hooks.
//!
//! On a serial, non-periodic mesh with whole-mesh families only, every node
//! is owned, normal and non-periodic: nothing is read, and removals
//! followed by additions, or several removals, compose. Upstream sequences
//! that add nodes and then remove or renumber them (IGFEM's interface move:
//! `removeAdditionalNodes` renumbers the nodes added earlier in the same
//! update) are ported as separate mutations.
//!
//! **Variable bounds.** Bounds of a native (`petsc_tao`) solver name local
//! equations; they are not moved by the renumbering but invalidated
//! ([`crate::ModelSolver::on_dof_mesh_event`]): set them
//! again after any mesh change.
//!
//! # Collective protocol
//!
//! As for node additions: a local preflight computes every
//! new array and the plan (no observable change) and is agreed
//! (recoverable); the numbering collectives run as agreed phases on fresh
//! structures (an agreed failure leaves the manager unchanged); the commit
//! writes the family arrays computed by the preflight into the existing
//! arrays (each shrinks or keeps its length), translates the per-equation
//! state into new vectors and resets the matrices — a failure there is
//! agreed fatal and poisons the manager on every rank.

use std::collections::{BTreeMap, HashMap};

use akantu_comm::phase::{Phases, Severity};
use akantu_core::array_shared::SharedArray;
use akantu_core::enums::DofSupportType;
use akantu_core::{Array, Error, Id, Idx, NodeFlag, Real, Result};
use akantu_linalg::{EquationLayout, EquationOwnership};
use akantu_mesh::Mesh;

use super::{
    BlockPlan, DofData, DofManager, check_local_equations, number_block, periodic_numbering_sync,
    plan_block, to_u64,
};

/// The new content of one family (computed in the preflight).
struct FamilyPlan {
    id: Id,
    nbc: usize,
    /// Shell receiving the new local equations, associated nodes and
    /// periodic-slave flags from `number_block` (its arrays are empty).
    shell: DofData,
    block: BlockPlan,
    /// `None` for generic families (unchanged arrays).
    arrays: Option<CompactedArrays>,
}

/// Compacted family arrays (written in place at commit).
struct CompactedArrays {
    len: usize,
    values: Vec<Real>,
    derivatives: Vec<(usize, Vec<Real>)>,
    increment: Option<Vec<Real>>,
    previous: Option<Vec<Real>>,
    blocked: Option<Vec<bool>>,
    solution: Vec<Real>,
    node_to_tuple: Option<HashMap<Idx, usize>>,
}

/// The renumbering of one removal.
struct Renumbering {
    families: Vec<FamilyPlan>,
    /// Old local equation → new local equation (`None`: removed).
    eq_map: Vec<Option<usize>>,
    /// New local system size.
    nb_local: usize,
}

/// `new_numbering` validated as a removal numbering: entries `-1` or a
/// dense injective map onto `0..kept`. Returns `kept`.
fn kept_count(new_numbering: &[Idx]) -> Result<usize> {
    // the mesh's validator (one copy)
    akantu_mesh::removal::removal_numbering_map(new_numbering, "DOF manager").map(|(_, k)| k)
}

/// Every node is owned, normal and non-periodic: no mesh state needs to be
/// read for the numbering (see the module docs, composition).
fn trivially_owned(mesh: &Mesh) -> bool {
    !mesh.is_distributed() && !mesh.is_periodic()
}

/// Rows of `src` (`nbc` components) moved to `map[t]`; `len` new rows.
fn compact<T: Copy>(src: &[T], nbc: usize, map: &[Option<usize>], len: usize, fill: T) -> Vec<T> {
    let mut out = vec![fill; len * nbc];
    for (t, m) in map.iter().enumerate() {
        if let Some(nt) = *m {
            out[nt * nbc..(nt + 1) * nbc].copy_from_slice(&src[t * nbc..(t + 1) * nbc]);
        }
    }
    out
}

/// Write `values` into `a` in place (it shrinks or keeps its length).
fn write_compacted<T: Copy + Default>(
    a: &mut SharedArray<T>,
    len: usize,
    values: &[T],
) -> Result<()> {
    let mut g = a.write()?;
    g.truncate(len);
    g.as_mut_slice().copy_from_slice(values);
    Ok(())
}

/// The plan of a block numbered on a serial non-periodic mesh without
/// reading it (every DOF owned, normal, non-periodic).
///
/// The recorded node global ids ([`DofData::associated_node_gids`]) are the
/// node indices: the global id of a node of a serial mesh without global
/// ids. This block is only planned for an intermediate removal of a
/// mutation whose later removal shrank the mesh further (module docs,
/// composition); the mutation's last removal keeps as many nodes as the
/// final mesh has and is numbered with [`plan_block`], which reads the ids
/// in the mesh.
fn owned_block(nb_dofs: usize, first_local: usize, nodes: &[Idx], nbc: usize) -> Result<BlockPlan> {
    check_local_equations(first_local, nb_dofs)?;
    let associated_gids = nodes
        .iter()
        .flat_map(|&n| std::iter::repeat_n(Real::from(n), nbc))
        .collect();
    Ok(BlockPlan {
        ownership: vec![EquationOwnership::Owned; nb_dofs],
        flags: vec![NodeFlag::NORMAL; nb_dofs],
        nb_owned: to_u64(nb_dofs)?,
        local_eqs: (first_local..first_local + nb_dofs)
            .map(akantu_core::try_idx)
            .collect::<Result<_>>()?,
        associated: nodes
            .iter()
            .flat_map(|&n| std::iter::repeat_n(n, nbc))
            .collect(),
        associated_gids,
        periodic_slave: Vec::new(),
    })
}

/// The tuples of a group-supported family after a removal: old tuple →
/// new tuple, new tuple → node, in the order of the group's node list in
/// `mesh` (as a registration on the group); every surviving tuple must be
/// in the list and the list must hold nothing else. Local, read-only (the
/// capability check runs it too).
fn group_tuples(
    mesh: &Mesh,
    data: &DofData,
    node_to_tuple: &HashMap<Idx, usize>,
    new_numbering: &[Idx],
) -> Result<(Vec<Option<usize>>, Vec<Idx>)> {
    let old_len = data.dofs.len();
    let group = data
        .group_support
        .as_deref()
        .ok_or_else(|| Error::invalid("a family with a node-to-tuple map has no group support"))?;
    let nodes: Vec<Idx> = mesh
        .groups()
        .element_group(group)?
        .nodes()
        .as_slice()
        .to_vec();
    let position: HashMap<Idx, usize> = nodes.iter().enumerate().map(|(t, &n)| (n, t)).collect();
    let mut map = vec![None; old_len];
    let mut hit = vec![false; nodes.len()];
    for (&old_node, &t) in node_to_tuple {
        let new_node = usize::try_from(old_node)
            .ok()
            .and_then(|u| new_numbering.get(u))
            .copied()
            .ok_or_else(|| Error::OutOfRange {
                what: "node of a group-supported family",
                index: usize::try_from(old_node).unwrap_or(usize::MAX),
                len: new_numbering.len(),
            })?;
        if new_node < 0 {
            continue;
        }
        let nt = *position.get(&new_node).ok_or_else(|| {
            Error::Mesh(format!(
                "DOF manager node removal: node {old_node} (now {new_node}) of the family \
                 supported by group '{group}' is no longer in the group"
            ))
        })?;
        let slot = map.get_mut(t).ok_or(Error::OutOfRange {
            what: "tuple of a group-supported family",
            index: t,
            len: old_len,
        })?;
        *slot = Some(nt);
        hit[nt] = true;
    }
    if let Some(t) = hit.iter().position(|h| !*h) {
        return Err(Error::Mesh(format!(
            "DOF manager node removal: node {} of group '{group}' has no DOF of the family \
             supported by the group (the group changed outside the DOF manager, or a node \
             addition of the same mutation joined it)",
            nodes[t]
        )));
    }
    Ok((map, nodes))
}

impl DofManager {
    /// Renumber after a node removal (upstream `DOFManager::onNodesRemoved`,
    /// empty upstream: UD-209): `new_numbering[old] = new` or `-1` (the
    /// payload of `MeshEvent::NodesRemoved`); `mesh` is the mesh after the
    /// removal. See the module documentation of
    /// `dof_manager::node_removal` for the algorithm. Collective; see
    /// [`DofManager::on_nodes_removed_phased`].
    pub fn on_nodes_removed(&mut self, mesh: &Mesh, new_numbering: &[Idx]) -> Result<()> {
        self.collective("nodes_removed", |dm| {
            dm.lend(|dm, ph| dm.on_nodes_removed_in(mesh, new_numbering, ph))
        })
    }

    /// [`DofManager::on_nodes_removed`] as phases of an enclosing collective
    /// operation (a mesh mutation): the local preflight (validated
    /// numbering and group support, every new array computed) is agreed
    /// before any collective, and an agreed failure leaves the manager
    /// unchanged; a failure after the new numbering was committed (array
    /// commit, matrix reset) is agreed fatal and poisons the manager on
    /// every rank; a transport failure poisons it locally.
    pub fn on_nodes_removed_phased(
        &mut self,
        mesh: &Mesh,
        new_numbering: &[Idx],
        ph: &mut Phases,
    ) -> Result<()> {
        self.check_poison()?;
        let r = self.on_nodes_removed_in(mesh, new_numbering, ph);
        self.settle(ph, "nodes_removed", r)
    }

    /// The capability check of a node event before the mesh commit (see
    /// the module docs, composition): a removal preceded by another node
    /// event of its mutation is refused when that is detectable from the
    /// numbering's length (an addition before it; a removal before it with
    /// a group-supported family), a removal on a distributed or periodic
    /// mesh must be the last node event of its mutation, the group part of
    /// the renumbering is planned here (its errors become recoverable), and
    /// an addition must name nodes of the final mesh.
    pub(super) fn check_node_event(
        &self,
        mesh: &Mesh,
        event: &akantu_mesh::MeshEvent,
    ) -> Result<()> {
        use akantu_mesh::MeshEvent;
        let has_nodal = self
            .families
            .values()
            .any(|f| f.support == DofSupportType::Nodal);
        if !has_nodal {
            return Ok(());
        }
        match event {
            MeshEvent::NodesRemoved { new_numbering, .. } => {
                let kept = kept_count(new_numbering)?;
                let len = new_numbering.len();
                let nodal = || {
                    self.families
                        .values()
                        .filter(|f| f.support == DofSupportType::Nodal)
                };
                // the nodes the manager holds: one tuple per node in every
                // whole-mesh family (none: group families only)
                let held = nodal()
                    .find(|f| f.node_to_tuple.is_none())
                    .map(|f| f.dofs.len());
                let groups: Vec<(&DofData, &HashMap<Idx, usize>)> = nodal()
                    .filter_map(|f| f.node_to_tuple.as_ref().map(|m| (f, m)))
                    .collect();
                if let Some(held) = held {
                    if len > held {
                        return Err(Error::Unsupported(format!(
                            "DOF manager: a node removal preceded by a node addition in the \
                             same mutation (its numbering maps {len} nodes, the manager holds \
                             {held}; the addition reads the final mesh at indices the removal \
                             renumbers): record node removals before node additions"
                        )));
                    }
                    if len < held && !groups.is_empty() {
                        return Err(Error::Unsupported(format!(
                            "DOF manager: a second node removal in one mutation with a \
                             group-supported family (its numbering maps {len} nodes, the \
                             manager holds {held}; the group's renumbering reads the final \
                             group): compose the numberings into one removal"
                        )));
                    }
                }
                // Guard for future distributed hooks:
                // unreachable today — `DistributedRemovalHook` refuses node
                // additions and a second removal, and no transaction creates
                // periodic pairs — but the renumbering would read the flags
                // and pairs of nodes a later node event renumbered.
                if !trivially_owned(mesh) && kept != mesh.nb_nodes() {
                    return Err(Error::Unsupported(format!(
                        "DOF manager: a node removal on a distributed or periodic mesh followed \
                         by another node event in the same mutation (it keeps {kept} nodes, the \
                         final mesh has {}; the renumbering reads the node flags and periodic \
                         pairs of the final mesh: make the removal the last node event)",
                        mesh.nb_nodes()
                    )));
                }
                // No node event before this one changed the manager (the
                // length matches), so the dispatch plans exactly this: the
                // group families' errors (a later addition joining the
                // group, a group changed outside the manager) are refused
                // here, recoverably, instead of at dispatch.
                // With group families only, the length cannot be
                // checked; the plan is run as well.
                if held.is_none_or(|h| h == len) {
                    for (f, map) in groups {
                        group_tuples(mesh, f, map, new_numbering)?;
                    }
                }
                Ok(())
            }
            MeshEvent::NodesAdded { nodes, .. } => {
                let n = mesh.nb_nodes();
                if let Some(&bad) = nodes
                    .iter()
                    .find(|&&x| usize::try_from(x).map_or(true, |u| u >= n))
                {
                    return Err(Error::Unsupported(format!(
                        "DOF manager: added node {bad} is not a node of the final mesh ({n} \
                         nodes): a node addition followed, in the same mutation, by a removal \
                         that renumbers the added nodes (record the removal first)"
                    )));
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }

    pub(super) fn on_nodes_removed_in(
        &mut self,
        mesh: &Mesh,
        new_numbering: &[Idx],
        ph: &mut Phases,
    ) -> Result<()> {
        // ---- local preflight ------------------------------------------------
        let pre = self.plan_renumbering(mesh, new_numbering);
        let mut plan = ph.agreed("dof:nodes_removed:preflight", pre, Severity::Recoverable)?;
        // ---- collectives on fresh structures (agreed) --------------------
        let sync = mesh.synchronizers().node.clone();
        let periodic = periodic_numbering_sync(mesh);
        let mut layout = EquationLayout::new();
        let mut flags: Vec<NodeFlag> = Vec::new();
        for f in &mut plan.families {
            let block = std::mem::take(&mut f.block);
            let nodal = f.arrays.is_some() && f.shell.support == DofSupportType::Nodal;
            number_block(
                ph,
                &mut layout,
                &mut flags,
                &mut f.shell,
                block,
                f.nbc,
                if nodal { sync.as_ref() } else { None },
                if nodal { periodic.as_ref() } else { None },
            )?;
        }
        // ---- commit (fatal) -----------------------------------------------
        let commit = self.commit_renumbering(plan, layout, flags, mesh, sync, periodic);
        ph.agree("dof:nodes_removed:commit", commit, Severity::Fatal)
    }

    /// Local preflight: validated numbering, per family the new tuples and
    /// arrays and the numbering plan, the old → new equation map. No
    /// observable change.
    fn plan_renumbering(&self, mesh: &Mesh, new_numbering: &[Idx]) -> Result<Renumbering> {
        self.check_poison()?;
        let kept = kept_count(new_numbering)?;
        // The surviving nodes are read in the mesh (as a registration does)
        // whenever they exist there; only a serial non-periodic mesh, where
        // every node is owned, normal and non-periodic, is numbered without
        // reading it when a later removal of the same mutation shrank it
        // further (module docs, composition).
        let read_mesh = kept <= mesh.nb_nodes();
        if !read_mesh && !trivially_owned(mesh) {
            return Err(Error::Mesh(format!(
                "DOF manager: the removal keeps {kept} nodes but the mesh has {}",
                mesh.nb_nodes()
            )));
        }
        let mut families = Vec::with_capacity(self.order.len());
        let mut first_local = 0usize;
        let mut eq_map: Vec<Option<usize>> = vec![None; self.layout.nb_local()];
        for id in &self.order {
            let data = self
                .families
                .get(id)
                .ok_or_else(|| Error::not_found("dof family", id.clone()))?;
            let nbc = data.dofs.nb_component();
            let old_len = data.dofs.len();
            let shell = DofData {
                support: data.support,
                group_support: data.group_support.clone(),
                dofs: Array::try_zeros(0, nbc)?.into(),
                derivatives: BTreeMap::new(),
                increment: None,
                previous: None,
                blocked: None,
                solution: Array::try_zeros(0, nbc)?,
                local_equation_numbers: Vec::new(),
                associated_nodes: Vec::new(),
                associated_node_gids: Vec::new(),
                node_to_tuple: None,
                periodic_slave: Vec::new(),
            };
            if data.support != DofSupportType::Nodal {
                // generic: one owned block, arrays unchanged
                let n = old_len * nbc;
                let block = owned_block(n, first_local, &[], nbc)?;
                for (k, &old) in data.local_equation_numbers.iter().enumerate() {
                    if let Ok(o) = usize::try_from(old)
                        && let Some(slot) = eq_map.get_mut(o)
                        && slot.is_none()
                    {
                        *slot = Some(first_local + k);
                    }
                }
                first_local += n;
                families.push(FamilyPlan {
                    id: id.clone(),
                    nbc,
                    shell,
                    block,
                    arrays: None,
                });
                continue;
            }
            // ---- tuples: old tuple → new tuple, new tuple → new node -------
            let (tuple_map, new_nodes): (Vec<Option<usize>>, Vec<Idx>) = match &data.node_to_tuple {
                None => {
                    if old_len != new_numbering.len() {
                        return Err(Error::DimensionMismatch {
                            context: "DOF manager node removal: the numbering must map every \
                                      tuple of a whole-mesh family",
                            expected: old_len,
                            got: new_numbering.len(),
                        });
                    }
                    let map = new_numbering
                        .iter()
                        .map(|&n| usize::try_from(n).ok())
                        .collect();
                    let nodes = (0..kept).map(akantu_core::try_idx).collect::<Result<_>>()?;
                    (map, nodes)
                }
                Some(node_to_tuple) => group_tuples(mesh, data, node_to_tuple, new_numbering)?,
            };
            let new_len = new_nodes.len();
            akantu_core::checked_local_dofs(new_len, nbc)?;
            // ---- block plan ---------------------------------------------------
            let block = if read_mesh {
                plan_block(mesh, None, &new_nodes, nbc, first_local)?
            } else {
                owned_block(new_len * nbc, first_local, &new_nodes, nbc)?
            };
            // ---- old → new equations (first DOF of an equation wins) --------
            for (t, m) in tuple_map.iter().enumerate() {
                let Some(nt) = *m else { continue };
                for c in 0..nbc {
                    let old = data.local_equation_numbers.get(t * nbc + c).copied();
                    let new = block.local_eqs.get(nt * nbc + c).copied();
                    if let (Some(old), Some(new)) = (old, new)
                        && let (Ok(o), Ok(n)) = (usize::try_from(old), usize::try_from(new))
                        && let Some(slot) = eq_map.get_mut(o)
                        && slot.is_none()
                    {
                        *slot = Some(n);
                    }
                }
            }
            first_local = check_local_equations(first_local, block.ownership.len())?;
            // ---- compacted arrays -------------------------------------------
            let values = compact(data.dofs.read()?.as_slice(), nbc, &tuple_map, new_len, 0.0);
            let mut derivatives = Vec::new();
            for (&o, d) in &data.derivatives {
                derivatives.push((
                    o,
                    compact(d.read()?.as_slice(), nbc, &tuple_map, new_len, 0.0),
                ));
            }
            let increment = match &data.increment {
                Some(a) => Some(compact(a.read()?.as_slice(), nbc, &tuple_map, new_len, 0.0)),
                None => None,
            };
            let previous = match &data.previous {
                Some(a) => Some(compact(a.read()?.as_slice(), nbc, &tuple_map, new_len, 0.0)),
                None => None,
            };
            let blocked = match &data.blocked {
                Some(a) => Some(compact(
                    a.read()?.as_slice(),
                    nbc,
                    &tuple_map,
                    new_len,
                    false,
                )),
                None => None,
            };
            let solution = if data.solution.len() == old_len {
                compact(data.solution.as_slice(), nbc, &tuple_map, new_len, 0.0)
            } else {
                vec![0.0; new_len * nbc]
            };
            let node_to_tuple = data
                .node_to_tuple
                .as_ref()
                .map(|_| new_nodes.iter().enumerate().map(|(t, &n)| (n, t)).collect());
            families.push(FamilyPlan {
                id: id.clone(),
                nbc,
                shell,
                block,
                arrays: Some(CompactedArrays {
                    len: new_len,
                    values,
                    derivatives,
                    increment,
                    previous,
                    blocked,
                    solution,
                    node_to_tuple,
                }),
            });
        }
        Ok(Renumbering {
            families,
            eq_map,
            nb_local: first_local,
        })
    }

    /// Commit a numbered renumbering: arrays in place, numbering, the
    /// per-equation state translated, every matrix reset.
    fn commit_renumbering(
        &mut self,
        plan: Renumbering,
        layout: EquationLayout,
        flags: Vec<NodeFlag>,
        mesh: &Mesh,
        sync: Option<akantu_comm::Synchronizer<Idx>>,
        periodic: Option<akantu_comm::Synchronizer<Idx>>,
    ) -> Result<()> {
        let Renumbering {
            families,
            eq_map,
            nb_local,
        } = plan;
        if layout.nb_local() != nb_local {
            return Err(Error::invalid(format!(
                "DOF manager node removal: {} local equations numbered, {nb_local} planned",
                layout.nb_local()
            )));
        }
        for f in families {
            let data = self
                .families
                .get_mut(&f.id)
                .ok_or_else(|| Error::not_found("dof family", f.id.clone()))?;
            if let Some(a) = f.arrays {
                write_compacted(&mut data.dofs, a.len, &a.values)?;
                for (o, v) in &a.derivatives {
                    if let Some(d) = data.derivatives.get_mut(o) {
                        write_compacted(d, a.len, v)?;
                    }
                }
                if let (Some(d), Some(v)) = (data.increment.as_mut(), &a.increment) {
                    write_compacted(d, a.len, v)?;
                }
                if let (Some(d), Some(v)) = (data.previous.as_mut(), &a.previous) {
                    write_compacted(d, a.len, v)?;
                }
                if let (Some(d), Some(v)) = (data.blocked.as_mut(), &a.blocked) {
                    write_compacted(d, a.len, v)?;
                }
                data.solution.try_resize(a.len, 0.0)?;
                data.solution.as_mut_slice().copy_from_slice(&a.solution);
                data.node_to_tuple = a.node_to_tuple;
            }
            data.local_equation_numbers = f.shell.local_equation_numbers;
            data.associated_nodes = f.shell.associated_nodes;
            // the node global ids follow the renumbering
            // (the trace dump `nodes.mtx` reads them)
            data.associated_node_gids = f.shell.associated_node_gids;
            data.periodic_slave = f.shell.periodic_slave;
        }
        // per-equation state follows its DOFs
        let translate = |old: &[Real]| -> Vec<Real> {
            let mut out = vec![0.0; nb_local];
            for (o, m) in eq_map.iter().enumerate() {
                if let (Some(n), Some(&v)) = (*m, old.get(o)) {
                    out[n] = v;
                }
            }
            out
        };
        self.residual.translate(translate);
        self.solution = translate(&self.solution);
        for v in self.lumped.values_mut() {
            v.values = translate(&v.values);
        }
        self.lumped_blocks.clear();
        let map_indexes = |idx: &[usize]| -> Vec<usize> {
            let mut v: Vec<usize> = idx
                .iter()
                .filter_map(|&e| eq_map.get(e).copied().flatten())
                .collect();
            v.sort_unstable();
            v.dedup();
            v
        };
        self.blocked_indexes = map_indexes(&self.blocked_indexes);
        self.previous_blocked_indexes = map_indexes(&self.previous_blocked_indexes);
        if !self.global_blocked.is_empty() {
            self.global_blocked = vec![false; nb_local];
            for &e in &self.blocked_indexes {
                self.global_blocked[e] = true;
            }
        }
        self.data_cache = vec![0.0; nb_local];
        self.layout = layout;
        self.dofs_flag = flags;
        self.node_sync = sync;
        self.periodic_sync = periodic;
        self.mesh_periodic = mesh.is_periodic();
        self.resize_global_arrays()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numberings_are_validated() {
        assert_eq!(kept_count(&[1, -1, 0]).unwrap(), 2);
        assert!(kept_count(&[1, 1]).is_err());
        assert!(kept_count(&[2, 0]).is_err());
        assert!(kept_count(&[-2, 0]).is_err());
        assert_eq!(kept_count(&[]).unwrap(), 0);
    }

    #[test]
    fn rows_follow_their_new_index() {
        let src = [1.0, 2.0, 3.0, 4.0, 5.0, 6.0];
        // 3 tuples of 2: tuple 0 removed, 1 → 1, 2 → 0
        let out = compact(&src, 2, &[None, Some(1), Some(0)], 2, 0.0);
        assert_eq!(out, vec![5.0, 6.0, 3.0, 4.0]);
    }
}
