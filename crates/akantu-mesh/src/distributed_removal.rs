// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/synchronizer/element_synchronizer.cc
// (ElementSynchronizer::onElementsRemoved :118-131, renumberElements
// :169-181), src/synchronizer/synchronizer_impl_tmpl.hh (filterScheme
// :353-403), src/synchronizer/node_synchronizer.hh (onNodesRemoved :52-53,
// empty upstream); element_synchronizer.cc onElementsChanged :97-113
// (substituteElements), carried over.

//! The distributed hook of a removal on a distributed mesh (SMM-015):
//! the synchronizer schemes follow the element and node
//! renumbering of the mutation.
//!
//! A mutation of a distributed mesh needs a [`DistributedHook`]
//! (`docs/ARCHITECTURE.md`, stage 2). [`DistributedRemovalHook`] is the
//! one for mutations that **remove or renumber** elements and nodes
//! (`MeshTransaction::{erase_elements, remove_nodes, purify}`, recorded
//! element changes), e.g. a
//! region erased on every rank holding it and its orphaned nodes removed:
//!
//! * **element synchronizer**, per `ElementsRemoved` of the mesh (upstream
//!   `ElementSynchronizer::onElementsRemoved`): `filterScheme` keeps the
//!   received (ghost) elements that survive — the ghost side decides and
//!   sends its kept positions, the owner filters its send scheme with them
//!   — then the elements are renumbered (`renumberElements`);
//! * **node synchronizer**, per `NodesRemoved` of the mesh: the same with
//!   the received (slave) nodes, then the nodes are renumbered. Upstream's
//!   `NodeSynchronizer::onNodesRemoved` is **empty** (UD-1070): its schemes
//!   keep the old local indices after a removal (out of range, or another
//!   node), so the DOF manager's `_ask_nodes` and every nodal exchange read
//!   the wrong nodes.
//!
//! **Consistency (refused, rolled back on every rank).** After the filter a
//! send scheme may still name an entity this rank removed (an owner removed
//! a master node, or erased a local element, whose copy a peer keeps):
//! upstream's `renumberElements` writes `-1` into the scheme; here the
//! removal is refused with an error naming the entity and the peer, and the
//! mutation rolls back on every rank (the hook's `apply` result is
//! agreed). Global ids are kept: the surviving nodes keep theirs (the
//! global numbering may have holes, as after upstream's removal).
//!
//! * **element changes**, per `ElementsChanged` of the mesh (upstream
//!   `ElementSynchronizer::onElementsChanged` → `substituteElements`,
//!   `element_synchronizer.cc:97-113`; local, no message): every scheme
//!   entry naming an old element names its new one. `old` and `new` must
//!   have the same length, `old` no element twice (upstream's debug
//!   assertion) and every element must exist (refused in `prepare`).
//!   Upstream's core never sends `ChangedElementsEvent`; its IGFEM extra
//!   package does (`mesh_igfem_spherical_growing_gel_tmpl.hh:185,
//!   380-391`). Carried over from the hook.
//!   Events are processed in recorded order, so a change and a
//!   removal of one mutation each see the numbering the other left.
//!
//! **Not supported here (refused in `prepare`, before any change):**
//! node or element additions in the same mutation (the insertion protocol
//! is the cohesive model's `CohesiveGlobalDataUpdater`), more than one
//! `NodesRemoved` of the mesh in one mutation, a periodic mesh (its
//! periodic node synchronizer), and a mesh with a facet mesh (the facet
//! synchronizers of the distributed cohesive model).

use std::collections::BTreeMap;

use akantu_comm::phase::Phases;
use akantu_core::{Error, Idx, Result};

use crate::events::MeshEvent;
use crate::mesh::Mesh;
use crate::mutation::{DistributedHook, MeshTransaction};
use crate::shared::MeshPart;
use crate::synchronizers::{NodeSynchronizer, renumber_elements, substitute_elements};

/// The [`DistributedHook`] of a removal on a distributed mesh (see the
/// module documentation). Stateless: nothing is kept outside the mesh, so
/// `commit` and `abort` have nothing to do.
#[derive(Clone, Copy, Debug, Default)]
pub struct DistributedRemovalHook;

impl DistributedRemovalHook {
    /// A hook for one mutation.
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

fn refused(what: &str) -> Error {
    Error::Unsupported(format!(
        "distributed removal hook: {what} (DistributedRemovalHook handles element and node \
         removals and element changes only)"
    ))
}

/// The checks of an `ElementsChanged` before any change: as many new as
/// old elements, no old element twice (upstream's debug assertion in
/// `onElementsChanged`), every element in the mesh.
fn check_element_change(
    mesh: &Mesh,
    old: &[akantu_core::Element],
    new: &[akantu_core::Element],
) -> Result<()> {
    if old.len() != new.len() {
        return Err(Error::DimensionMismatch {
            context: "ElementsChanged: new elements per old element",
            expected: old.len(),
            got: new.len(),
        });
    }
    let mut seen = std::collections::HashSet::with_capacity(old.len());
    if let Some(d) = old.iter().find(|el| !seen.insert(**el)) {
        return Err(Error::invalid(format!(
            "ElementsChanged: the same element cannot appear twice in the list ({d})"
        )));
    }
    for &el in old.iter().chain(new) {
        mesh.connectivities().element_ref(el)?;
    }
    Ok(())
}

/// Map every node of the schemes with `numbering` after the filter; a
/// removed node still in a send scheme is an inconsistent removal.
fn renumber_nodes(sync: &mut NodeSynchronizer, numbering: &[Idx]) -> Result<()> {
    let mut err: Option<Error> = None;
    sync.update_schemes(|scheme, rank, dir| {
        for n in scheme.iter_mut() {
            let new = usize::try_from(*n)
                .ok()
                .and_then(|u| numbering.get(u))
                .copied()
                .unwrap_or(-1);
            if new < 0 {
                err.get_or_insert_with(|| {
                    Error::Mesh(format!(
                        "distributed node removal: node {n} is removed here but still in the \
                         {dir:?} scheme toward rank {rank} (its copy on rank {rank} is kept: \
                         remove a shared node on every rank holding it, or only its copies)"
                    ))
                });
            } else {
                *n = new;
            }
        }
    });
    err.map_or(Ok(()), Err)
}

impl DistributedHook for DistributedRemovalHook {
    fn prepare(&mut self, mesh: &Mesh, events: &[(MeshPart, MeshEvent)]) -> Result<()> {
        if mesh.mesh_facets().is_some() {
            return Err(refused(
                "a distributed mesh with a facet mesh (the facet synchronizers of the \
                 distributed cohesive model)",
            ));
        }
        let mut node_removals = 0usize;
        for (part, e) in events {
            match e {
                MeshEvent::ElementsRemoved { .. } => {}
                MeshEvent::NodesRemoved { .. } => {
                    if *part == MeshPart::Main {
                        node_removals += 1;
                    }
                    if mesh.is_periodic() {
                        return Err(refused(
                            "node removal on a distributed periodic mesh (the periodic node \
                             synchronizer)",
                        ));
                    }
                }
                MeshEvent::NodesAdded { .. } => return Err(refused("node additions")),
                MeshEvent::ElementsAdded { .. } => return Err(refused("element additions")),
                MeshEvent::ElementsChanged { old, new, .. } => {
                    if *part == MeshPart::Main {
                        check_element_change(mesh, old, new)?;
                    }
                }
                MeshEvent::MeshIsDistributed { .. } => return Err(refused("MeshIsDistributed")),
                #[allow(unreachable_patterns)] // MeshEvent is #[non_exhaustive]
                _ => return Err(refused("an unknown event")),
            }
        }
        if node_removals > 1 {
            return Err(refused(
                "more than one node removal in one mutation (compose the numberings into one \
                 remove_nodes call)",
            ));
        }
        Ok(())
    }

    fn apply(
        &mut self,
        tx: &mut MeshTransaction<'_>,
        events: &[(MeshPart, MeshEvent)],
        phases: &mut Phases,
    ) -> Result<()> {
        for (part, e) in events {
            if *part != MeshPart::Main {
                continue;
            }
            match e {
                MeshEvent::ElementsRemoved { new_numbering, .. } => {
                    let Some(mut sync) = tx.mesh().synchronizers().element.clone() else {
                        // no element synchronizer: every rank skips the same
                        // phase only if all lack it; the schemes are built by
                        // the distribution on every rank or on none
                        continue;
                    };
                    let keep = |el: &akantu_core::Element| match new_numbering
                        .get(el.ty(), el.ghost_type())
                    {
                        Ok(num) => num.as_slice().get(el.idx()).is_some_and(|&n| n >= 0),
                        Err(_) => true,
                    };
                    sync.filter_scheme_in(phases, keep)?;
                    renumber_elements(&mut sync, new_numbering).map_err(|e| {
                        Error::Mesh(format!(
                            "distributed element removal: an element removed here is still a \
                             ghost on another rank (erase an element on every rank holding \
                             it): {e}"
                        ))
                    })?;
                    tx.synchronizers_mut(MeshPart::Main)?.element = Some(sync);
                }
                MeshEvent::NodesRemoved { new_numbering, .. } => {
                    let Some(mut sync) = tx.mesh().synchronizers().node.clone() else {
                        continue;
                    };
                    let keep = |n: &Idx| {
                        usize::try_from(*n)
                            .ok()
                            .and_then(|u| new_numbering.get(u))
                            .is_some_and(|&m| m >= 0)
                    };
                    sync.filter_scheme_in(phases, keep)?;
                    renumber_nodes(&mut sync, new_numbering)?;
                    tx.synchronizers_mut(MeshPart::Main)?.node = Some(sync);
                }
                MeshEvent::ElementsChanged { old, new, .. } => {
                    // local, every rank its own list (no message)
                    let map: BTreeMap<akantu_core::Element, akantu_core::Element> =
                        old.iter().copied().zip(new.iter().copied()).collect();
                    if let Some(sync) = tx.synchronizers_mut(MeshPart::Main)?.element.as_mut() {
                        substitute_elements(sync, &map);
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }
}
