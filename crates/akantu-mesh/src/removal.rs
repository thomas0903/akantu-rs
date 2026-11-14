// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/mesh/mesh.cc (eraseElements),
// src/mesh/mesh_inline_impl.hh (sendEvent<RemovedElementsEvent>,
// sendEvent<RemovedNodesEvent>, removeNodesFromArray),
// src/mesh/element_type_map_tmpl.hh (onElementsRemoved).

//! Element and node removal with renumbering (MESH-013, MESH-014). Used by
//! `MeshTransaction::{erase_elements, remove_nodes, purify}` (journaled,
//! shared meshes) and by the set-up helpers (`crate::utils::purify_mesh`,
//! `erase_elements_setup`).
//!
//! **Element numbering** (upstream `Mesh::eraseElements`, `mesh.cc:661-725`):
//! per `(type, ghost)` block the numbering starts as the identity, removed
//! elements get `-1`, then holes are filled by moving the **last** surviving
//! element into the **first** hole, repeatedly. A ghost element is removed
//! only when its ghost counter drops to zero.
//!
//! **What follows the renumbering.** Upstream updates the connectivities,
//! the ghost counters (elements) and the coordinates / flags / global ids
//! (nodes) and nothing else: mesh data, groups, periodic pairs, owner ranks
//! and the facet mesh keep stale indices (D-003 category B, UD-117). Here
//! every mesh-owned dependent follows:
//!
//! * element removal: rows of every elemental dataset of the renumbered
//!   blocks, element references (removed → `NULL`), element groups
//!   (removed entries dropped, order kept), in the **index domain** of the
//!   removed elements (below);
//! * node removal: connectivities (removing a node still used by an element
//!   is an error), coordinates, flags, global ids, owner ranks, periodic
//!   pairs (pairs with a removed node dropped, the periodic flag bits of
//!   their other node cleared when it loses its last pair), node groups,
//!   nodal data, local bounding box — of the mesh **and** of its facet
//!   mesh, which shares the node numbering (the
//!   facet mesh is renumbered with the same numbering and a second
//!   `NodesRemoved` event is produced for its consumers, after the
//!   parent's).
//!
//! **Index domains.** A parent mesh and its facet mesh
//! number their elements independently, and both may hold blocks of the
//! same type (a 2-D parent's boundary `Segment2` elements and the facet
//! mesh's `Segment2` facets). Every block and element reference is
//! attributed to one domain:
//!
//! * in the **parent**: every row and every reference denotes a parent
//!   element, except the *values* of `mesh_to_mesh_facet`, which denote
//!   facets;
//! * in the **facet mesh**: rows of blocks and references of types of the
//!   parent's spatial dimension denote **parent** elements (the source
//!   elements of the facet builders: `subelement_to_element` rows of the
//!   parent's cells, `element_to_subelement` entries of the top facets);
//!   rows and references of lower-dimensional types denote **facets**. A
//!   facet mesh holding connectivity blocks of the parent's dimension
//!   would make this ambiguous and is rejected.
//!
//! Removing parent elements compacts only parent-domain rows and remaps
//! only parent-domain references (in both meshes); removing facets
//! compacts and remaps only facet-domain ones (plus the parent's
//! `mesh_to_mesh_facet` values).
//!
//! **Revision and events.** The appliers are
//! crate-private; [`erase_elements_setup`] and [`remove_nodes`] are the
//! public set-up wrappers: they advance the revision of the mesh exactly
//! once and return the events, which the caller is responsible for (a
//! mesh with consumers is changed through the journaled
//! `MeshTransaction`, which records and dispatches them).

use std::collections::HashMap;

use akantu_core::{
    Element, ElementType, ElementTypeMapArray, Error, GhostType, Idx, NodeFlag, Result, try_idx,
    try_to_usize,
};

use crate::data::{compact_rows, compact_shared_rows};
use crate::events::{MeshEvent, MeshEvents};
use crate::mesh::Mesh;
use crate::shared::MeshPart;
use crate::utils::MESH_TO_MESH_FACET;

/// Result of the element-removal planning (no mesh change yet).
#[derive(Clone, Debug)]
pub struct ElementRemoval {
    /// Elements actually removed, in input order (duplicates kept, as
    /// upstream's `RemovedElementsEvent` list).
    pub removed: Vec<Element>,
    /// Per-block old → new numbering (`-1` = removed); only blocks touched
    /// by the input are present.
    pub new_numbering: ElementTypeMapArray<Idx>,
    /// Ghost counters after the decrements, per ghost block touched.
    counters: Vec<(ElementType, Vec<Idx>)>,
}

/// Plan upstream `Mesh::eraseElements(elements)` on `mesh` (validated: every
/// element exists).
pub fn plan_element_removal(mesh: &Mesh, elements: &[Element]) -> Result<ElementRemoval> {
    let mut numbering: ElementTypeMapArray<Idx> = ElementTypeMapArray::new("new_numbering");
    let mut counters: HashMap<ElementType, Vec<Idx>> = HashMap::new();
    let mut removed = Vec::new();
    for &el in elements {
        let i = el
            .try_idx()
            .ok_or_else(|| Error::invalid("cannot erase ElementNull"))?;
        let nb = mesh.nb_element(el.ty(), el.ghost_type());
        if i >= nb {
            return Err(Error::OutOfRange {
                what: "element",
                index: i,
                len: nb,
            });
        }
        if el.ghost_type() == GhostType::Ghost {
            let c = counters.entry(el.ty()).or_insert_with(|| {
                mesh.ghosts_counters()
                    .get(el.ty(), GhostType::Ghost)
                    .map(|a| a.as_slice().to_vec())
                    .unwrap_or_default()
            });
            if c.len() < nb {
                // missing counters count as one copy (upstream: out of range)
                c.resize(nb, 1);
            }
            // Validated before the decrement: a counter
            // below one is invalid input (user-writable through
            // `MeshTransaction::ghosts_counters_mut`), not an overflow.
            if c[i] < 1 {
                return Err(Error::Mesh(format!(
                    "ghost counter of {el} is {} (must be at least 1 to erase it)",
                    c[i]
                )));
            }
            c[i] -= 1;
            if c[i] > 0 {
                continue;
            }
        }
        removed.push(el);
        if !numbering.exists(el.ty(), el.ghost_type()) {
            let a = numbering.alloc(nb, 1, el.ty(), el.ghost_type(), 0)?;
            for (k, v) in a.as_mut_slice().iter_mut().enumerate() {
                *v = try_idx(k)?;
            }
        }
        numbering.element_mut(el)?[0] = -1;
    }
    for ghost in GhostType::ALL {
        for (_, a) in numbering.as_map_mut().iter_mut(ghost) {
            fill_holes(a.as_mut_slice())?;
        }
    }
    let counters = counters.into_iter().collect();
    Ok(ElementRemoval {
        removed,
        new_numbering: numbering,
        counters,
    })
}

/// Upstream hole filling (`mesh.cc:695-722`): the last surviving element
/// moves into the first hole, repeatedly.
fn fill_holes(n: &mut [Idx]) -> Result<()> {
    let last_not_deleted =
        |n: &[Idx], start: usize| -> Option<usize> { (0..start).rev().find(|&k| n[k] != -1) };
    let first_deleted = |n: &[Idx], start: usize| -> usize {
        (start..n.len()).find(|&k| n[k] == -1).unwrap_or(n.len())
    };
    let Some(mut last) = last_not_deleted(n, n.len()) else {
        return Ok(());
    };
    let mut pos = first_deleted(n, 0);
    while pos < last {
        n[last] = try_idx(pos)?;
        match last_not_deleted(n, last) {
            Some(l) => last = l,
            None => break,
        }
        pos = first_deleted(n, pos + 1);
    }
    Ok(())
}

/// Map an element through a numbering (`NULL` if removed, unchanged if its
/// block is not renumbered).
fn remap(numbering: &ElementTypeMapArray<Idx>, e: Element) -> Element {
    let Some(i) = e.try_idx() else { return e };
    match numbering.get(e.ty(), e.ghost_type()) {
        Ok(a) if i < a.len() => {
            let new = a.as_slice()[i];
            if new < 0 {
                Element::NULL
            } else {
                Element::new(e.ty(), new, e.ghost_type())
            }
        }
        _ => e,
    }
}

/// `true` if a block or element reference of type `ty` stored in the facet
/// mesh of a parent of dimension `parent_dim` denotes a **parent** element
/// (see the module docs, index domains).
#[must_use]
pub(crate) fn facet_mesh_type_is_parent(parent_dim: usize, ty: ElementType) -> bool {
    ty.spatial_dimension() == parent_dim
}

/// The facet mesh of `mesh` holds no connectivity block of the parent's
/// dimension (otherwise parent and facet index domains are ambiguous).
pub(crate) fn check_facet_domains(mesh: &Mesh) -> Result<()> {
    let Some(f) = mesh.mesh_facets() else {
        return Ok(());
    };
    let dim = mesh.spatial_dimension();
    for (ty, ghost, _) in f.connectivities().as_map().iter_all() {
        if ty.spatial_dimension() >= dim {
            return Err(Error::Mesh(format!(
                "facet mesh '{}' holds {ty}:{ghost} elements of the parent's dimension {dim}: \
                 parent and facet index domains are ambiguous",
                f.id()
            )));
        }
    }
    Ok(())
}

/// Compact the blocks of `mesh` (connectivities, ghost counters, elemental
/// data rows, element groups) with `numbering`; `rows_only` limits it to
/// the data rows (parent rows kept in the facet mesh).
fn compact_blocks(
    mesh: &mut Mesh,
    numbering: &ElementTypeMapArray<Idx>,
    rows_only: bool,
) -> Result<()> {
    for (ty, ghost, num) in numbering.as_map().iter_all() {
        let num = num.as_slice();
        if !rows_only {
            if let Ok(c) = mesh.connectivities.shared_mut(ty, ghost) {
                compact_shared_rows(c, num)?;
            }
            if let Ok(c) = mesh.ghosts_counters.get_mut(ty, ghost) {
                compact_rows(c, num)?;
            }
            mesh.groups.apply_element_numbering(ty, ghost, num);
        }
        mesh.data.apply_element_numbering(ty, ghost, num)?;
    }
    Ok(())
}

/// The blocks of `numbering` accepted by `keep`.
fn restrict(
    numbering: &ElementTypeMapArray<Idx>,
    keep: impl Fn(ElementType) -> bool,
) -> Result<ElementTypeMapArray<Idx>> {
    let mut out = ElementTypeMapArray::new("new_numbering");
    for (ty, ghost, a) in numbering.as_map().iter_all() {
        if keep(ty) {
            out.as_map_mut().insert(ty, ghost, a.clone())?;
        }
    }
    Ok(out)
}

/// Apply a planned removal to `part` of `mesh` (see the module docs for
/// the dependents and the index domains). Returns the `ElementsRemoved`
/// event. Does not change the revision (callers: the journaled
/// transaction, [`erase_elements_setup`]).
pub(crate) fn apply_element_removal(
    mesh: &mut Mesh,
    part: MeshPart,
    plan: ElementRemoval,
    origin: &str,
) -> Result<MeshEvent> {
    check_facet_domains(mesh)?;
    let ElementRemoval {
        removed,
        new_numbering,
        counters,
    } = plan;
    let parent_dim = mesh.spatial_dimension();
    match part {
        MeshPart::Main => {
            for (ty, c) in counters {
                write_counters(mesh, ty, c)?;
            }
            compact_blocks(mesh, &new_numbering, false)?;
            let f = |e: Element| remap(&new_numbering, e);
            mesh.data
                .remap_element_references(&|name| name != MESH_TO_MESH_FACET, &f);
            if let Some(facets) = mesh.mesh_facets.as_deref_mut() {
                // Only the parent-domain blocks of the facet mesh (rows of
                // the parent's cells, references to them) follow; facet
                // blocks of the same type as removed parent elements are
                // another index space.
                let parent = restrict(&new_numbering, |ty| {
                    facet_mesh_type_is_parent(parent_dim, ty)
                })?;
                compact_blocks(facets, &parent, true)?;
                let g = |e: Element| remap(&parent, e);
                facets.data.remap_element_references(&|_| true, &g);
            }
        }
        MeshPart::Facets => {
            let facets = mesh
                .mesh_facets
                .as_deref_mut()
                .ok_or_else(|| Error::Mesh("the mesh has no facet mesh".into()))?;
            // the facet mesh holds no parent-dimension block (checked):
            // every removed element is a facet
            for (ty, c) in counters {
                write_counters(facets, ty, c)?;
            }
            compact_blocks(facets, &new_numbering, false)?;
            let f = |e: Element| {
                if facet_mesh_type_is_parent(parent_dim, e.ty()) {
                    e
                } else {
                    remap(&new_numbering, e)
                }
            };
            facets.data.remap_element_references(&|_| true, &f);
            mesh.data
                .remap_element_references(&|name| name == MESH_TO_MESH_FACET, &f);
        }
    }
    Ok(MeshEvent::ElementsRemoved {
        elements: removed,
        new_numbering,
        origin: origin.to_owned(),
    })
}

fn write_counters(mesh: &mut Mesh, ty: ElementType, c: Vec<Idx>) -> Result<()> {
    let len = c.len();
    let a = mesh
        .ghosts_counters
        .alloc(len, 1, ty, GhostType::Ghost, 1)?;
    a.as_mut_slice().copy_from_slice(&c);
    Ok(())
}

/// A node-removal numbering (`new_numbering[old] = new`, `-1` if removed)
/// validated and turned into a row map: every entry is `-1` or a new index,
/// the new indices a dense injective map onto `0..kept`. Returns the map
/// (`None` for a removed node) and `kept`. The one validator of the mesh,
/// the DOF manager and the models' nodal arrays;
/// `what` names the caller in the error.
///
/// # Errors
/// `Error::InvalidArgument` naming the first offending new index.
pub fn removal_numbering_map(numbering: &[Idx], what: &str) -> Result<(Vec<Option<usize>>, usize)> {
    let kept = numbering.iter().filter(|&&n| n >= 0).count();
    let mut seen = vec![false; kept];
    let mut map = Vec::with_capacity(numbering.len());
    for &n in numbering {
        if n == -1 {
            map.push(None);
            continue;
        }
        let slot = usize::try_from(n)
            .ok()
            .and_then(|u| seen.get_mut(u).map(|s| (u, s)));
        match slot {
            Some((u, s)) if !*s => {
                *s = true;
                map.push(Some(u));
            }
            _ => {
                return Err(Error::invalid(format!(
                    "{what}: node removal numbering is not a dense injective map (new index {n})"
                )));
            }
        }
    }
    Ok((map, kept))
}

/// Validated node numbering: length = node count, new indices dense and
/// injective. Returns the number of kept nodes.
fn check_node_numbering(nb_nodes: usize, numbering: &[Idx]) -> Result<usize> {
    if numbering.len() != nb_nodes {
        return Err(Error::DimensionMismatch {
            context: "node removal numbering",
            expected: nb_nodes,
            got: numbering.len(),
        });
    }
    removal_numbering_map(numbering, "mesh").map(|(_, kept)| kept)
}

/// Renumber the nodes of one mesh (connectivities already checked).
fn renumber_nodes_of(mesh: &mut Mesh, numbering: &[Idx]) -> Result<()> {
    for g in GhostType::ALL {
        for (_, c) in mesh.connectivities.as_map_mut().iter_mut(g) {
            for n in c.borrow_mut().as_mut_slice() {
                *n = numbering[try_to_usize(*n)?];
            }
        }
    }
    compact_shared_rows(&mut mesh.nodes, numbering)?;
    compact_rows(&mut mesh.nodes_flags, numbering)?;
    if let Some(gid) = &mut mesh.nodes_global_ids {
        compact_rows(gid, numbering)?;
    }
    let map = |n: Idx| -> Option<Idx> {
        let new = *numbering.get(usize::try_from(n).ok()?)?;
        (new >= 0).then_some(new)
    };
    mesh.nodes_prank = mesh
        .nodes_prank
        .iter()
        .filter_map(|(&n, &r)| map(n).map(|m| (m, r)))
        .collect();
    // periodic pairs
    if !mesh.periodic_slave_master.is_empty() || !mesh.periodic_master_slaves.is_empty() {
        let mut lost: Vec<Idx> = Vec::new();
        let mut sm = HashMap::new();
        for (&s, &m) in &mesh.periodic_slave_master {
            match (map(s), map(m)) {
                (Some(s2), Some(m2)) => {
                    sm.insert(s2, m2);
                }
                (s2, m2) => lost.extend(s2.into_iter().chain(m2)),
            }
        }
        let mut ms: HashMap<Idx, Vec<Idx>> = HashMap::new();
        for (&m, slaves) in &mesh.periodic_master_slaves {
            let Some(m2) = map(m) else { continue };
            let kept: Vec<Idx> = slaves.iter().filter_map(|&s| map(s)).collect();
            if !kept.is_empty() {
                ms.insert(m2, kept);
            }
        }
        for n in lost {
            let is_slave = sm.contains_key(&n);
            let is_master = ms.contains_key(&n);
            if !is_slave && !is_master {
                let f = &mut mesh.nodes_flags.as_mut_slice()[try_to_usize(n)?];
                *f = NodeFlag(f.bits() & !NodeFlag::PERIODIC_MASK.bits());
            }
        }
        mesh.periodic_slave_master = sm;
        mesh.periodic_master_slaves = ms;
    }
    mesh.groups.apply_node_numbering(numbering);
    mesh.data.apply_node_numbering(numbering)?;
    let b = mesh.local_bounding_box();
    if !mesh.is_distributed {
        mesh.bbox = b.clone();
    }
    mesh.bbox_local = b;
    Ok(())
}

fn check_no_removed_node_used(mesh: &Mesh, numbering: &[Idx]) -> Result<()> {
    for (ty, ghost, c) in mesh.connectivities.as_map().iter_all() {
        for &n in c.borrow().as_slice() {
            let u = try_to_usize(n)?;
            if numbering.get(u).is_none_or(|&m| m < 0) {
                return Err(Error::Mesh(format!(
                    "node {n} is removed but still used by an element of {ty}:{ghost} of '{}'",
                    mesh.id()
                )));
            }
        }
    }
    Ok(())
}

/// Validated node removal of `mesh` and of its facet mesh (shared node
/// numbering): no change yet. Returns the removed nodes.
pub(crate) fn plan_node_removal(mesh: &Mesh, numbering: &[Idx]) -> Result<Vec<Idx>> {
    mesh.check_consistency()?;
    check_node_numbering(mesh.nb_nodes(), numbering)?;
    check_no_removed_node_used(mesh, numbering)?;
    if let Some(f) = mesh.mesh_facets.as_deref() {
        if f.nb_nodes() != mesh.nb_nodes() {
            return Err(Error::Mesh(format!(
                "facet mesh '{}' has {} nodes, its parent '{}' {}: the shared node numbering is broken",
                f.id(),
                f.nb_nodes(),
                mesh.id(),
                mesh.nb_nodes()
            )));
        }
        f.check_consistency()?;
        check_no_removed_node_used(f, numbering)?;
    }
    numbering
        .iter()
        .enumerate()
        .filter(|&(_, &n)| n < 0)
        .map(|(i, _)| try_idx(i))
        .collect()
}

/// Remove/renumber the nodes of `mesh` and of its facet mesh with
/// `numbering` (old → new, `-1` removed; upstream
/// `RemovedNodesEvent::new_numbering`). Returns the events in dispatch
/// order: `(Main, NodesRemoved)` then, with a facet mesh, `(Facets,
/// NodesRemoved)` with the same numbering. Does not change the revision.
pub(crate) fn apply_node_removal(
    mesh: &mut Mesh,
    numbering: &[Idx],
    origin: &str,
) -> Result<Vec<(MeshPart, MeshEvent)>> {
    let removed = plan_node_removal(mesh, numbering)?;
    renumber_nodes_of(mesh, numbering)?;
    let event = MeshEvent::NodesRemoved {
        nodes: removed,
        new_numbering: numbering.to_vec(),
        origin: origin.to_owned(),
    };
    let mut events = vec![(MeshPart::Main, event.clone())];
    if let Some(f) = mesh.mesh_facets.as_deref_mut() {
        renumber_nodes_of(f, numbering)?;
        events.push((MeshPart::Facets, event));
    }
    Ok(events)
}

/// Set-up wrapper of node removal (no consumers; shared meshes use
/// `MeshTransaction::remove_nodes`): the mesh and its facet mesh are
/// renumbered with `numbering` (see the module docs). Advances
/// the revision once; the returned events (parent, then facet mesh) are
/// the caller's responsibility.
pub fn remove_nodes(mesh: &mut Mesh, numbering: &[Idx], origin: &str) -> Result<MeshEvents> {
    reject_facet_part(mesh, "remove_nodes")?;
    let events = apply_node_removal(mesh, numbering, origin)?;
    mesh.revision += 1;
    Ok(MeshEvents(events.into_iter().map(|(_, e)| e).collect()))
}

/// A node renumbering of a facet mesh alone would break the node numbering
/// it shares with its parent: node operations run on
/// the parent mesh, which renumbers both.
pub(crate) fn reject_facet_part(mesh: &Mesh, what: &str) -> Result<()> {
    if mesh.is_mesh_facets() {
        return Err(Error::Mesh(format!(
            "{what} on a facet mesh: the facet mesh shares its parent's node numbering; \
             renumber the parent mesh (which renumbers both)"
        )));
    }
    Ok(())
}

/// Upstream `Mesh::eraseElements(elements)` on a mesh being set up
/// (no consumers; shared meshes use `MeshTransaction::erase_elements`).
/// Advances the revision once; the returned event is the caller's
/// responsibility.
pub fn erase_elements_setup(mesh: &mut Mesh, elements: &[Element]) -> Result<MeshEvent> {
    mesh.check_consistency()?;
    check_facet_domains(mesh)?;
    let plan = plan_element_removal(mesh, elements)?;
    let ev = apply_element_removal(mesh, MeshPart::Main, plan, "Mesh::eraseElements")?;
    mesh.revision += 1;
    Ok(ev)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn holes_are_filled_from_the_end() {
        // remove 1 and 3 of 6: 5 → 1, 4 → 3
        let mut n = vec![0, -1, 2, -1, 4, 5];
        fill_holes(&mut n).unwrap();
        assert_eq!(n, vec![0, -1, 2, -1, 3, 1]);
        // remove the last ones only: nothing moves
        let mut n = vec![0, 1, -1, -1];
        fill_holes(&mut n).unwrap();
        assert_eq!(n, vec![0, 1, -1, -1]);
        // all removed
        let mut n = vec![-1, -1];
        fill_holes(&mut n).unwrap();
        assert_eq!(n, vec![-1, -1]);
        // first removed, holes in the middle
        let mut n = vec![-1, 1, -1, 3, -1, 5, 6];
        fill_holes(&mut n).unwrap();
        assert_eq!(n, vec![-1, 1, -1, 3, -1, 2, 0]);
    }

    #[test]
    fn node_numberings_are_checked() {
        assert_eq!(check_node_numbering(3, &[1, -1, 0]).unwrap(), 2);
        assert!(check_node_numbering(3, &[1, -1]).is_err());
        assert!(check_node_numbering(2, &[1, 1]).is_err());
        assert!(check_node_numbering(2, &[2, 0]).is_err());
        assert!(check_node_numbering(2, &[-2, 0]).is_err());
    }
}
