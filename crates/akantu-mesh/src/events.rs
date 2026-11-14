// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/mesh/mesh_events.hh,
// src/common/aka_event_handler_manager.hh, and the event subclasses
// CohesiveNewNodesEvent (mesh_utils/cohesive_element_inserter.hh),
// NewConstitutiveLawElementsEvent (constitutive_laws_handler.hh),
// NewIGFEMElementsEvent/NewIGFEMNodesEvent (extra_packages/igfem).

//! Mesh-change events (decision record D-019).
//!
//! Upstream: `Mesh` is an `EventHandlerManager<MeshEventHandler>` holding raw
//! pointers to handlers sorted by priority; mutating code calls
//! `mesh.sendEvent(event)` and every handler reacts synchronously.
//!
//! akantu-rs: events are **transaction data**. They are recorded by a staged
//! mutation of the [`crate::SharedMesh`] domain and delivered by it, after
//! commit, to every registered [`MeshEventHandler`] in ascending
//! [`EventPriority`] (mesh 5 < FE engine 9 < synchronizer 10 < DOF manager 20
//! < model 94 < non-local manager 100) — among consumers of equal priority
//! the most recently registered first, as upstream's `std::lower_bound`
//! insertion (CORE-036) — with acknowledgements and a failure state (see
//! [`crate::mutation`]). There is no manual dispatch: a
//! component cannot miss or replay an event.

use std::any::Any;
use std::sync::Arc;

use akantu_comm::phase::Phases;
use akantu_core::enums::EventPriority;
use akantu_core::{Element, ElementTypeMapArray, GhostType, Idx, NodeIdx, Result};

use crate::mesh::Mesh;

/// Payload extension for packages that need extra data on standard events
/// (e.g. IGFEM); handlers downcast it.
pub type EventExtension = Arc<dyn Any + Send + Sync>;

/// One mesh change.
#[derive(Clone)]
#[non_exhaustive]
pub enum MeshEvent {
    /// Nodes appended (upstream `NewNodesEvent`).
    NodesAdded {
        /// New local node indices.
        nodes: Vec<NodeIdx>,
        /// For duplicated nodes (cohesive insertion), the node each new node
        /// was copied from (upstream `CohesiveNewNodesEvent::old_nodes`).
        old_nodes: Option<Vec<NodeIdx>>,
        /// Emitter.
        origin: String,
        /// Extra payload.
        extension: Option<EventExtension>,
    },
    /// Nodes removed and renumbered (upstream `RemovedNodesEvent`);
    /// `new_numbering[old] = new` or `INVALID_IDX` if removed.
    NodesRemoved {
        /// Removed nodes.
        nodes: Vec<NodeIdx>,
        /// Old → new numbering.
        new_numbering: Vec<Idx>,
        /// Emitter.
        origin: String,
    },
    /// Elements appended (upstream `NewElementsEvent`).
    ElementsAdded {
        /// New elements.
        elements: Vec<Element>,
        /// Constitutive law of each new element, when the emitter knows it
        /// (upstream `NewConstitutiveLawElementsEvent`).
        constitutive_law: Option<Vec<Idx>>,
        /// Emitter.
        origin: String,
        /// Extra payload.
        extension: Option<EventExtension>,
    },
    /// Elements removed and renumbered (upstream `RemovedElementsEvent`).
    ElementsRemoved {
        /// Removed elements.
        elements: Vec<Element>,
        /// Per-type old → new numbering (`INVALID_IDX` if removed).
        new_numbering: ElementTypeMapArray<Idx>,
        /// Emitter.
        origin: String,
    },
    /// Elements changed identity (upstream `ChangedElementsEvent`).
    ElementsChanged {
        /// Old ids.
        old: Vec<Element>,
        /// New ids.
        new: Vec<Element>,
        /// Per-type old → new numbering.
        new_numbering: ElementTypeMapArray<Idx>,
        /// Emitter.
        origin: String,
    },
    /// The mesh was distributed (upstream `MeshIsDistributedEvent`).
    MeshIsDistributed {
        /// Emitter.
        origin: String,
    },
}

impl std::fmt::Debug for MeshEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NodesAdded { nodes, origin, .. } => {
                write!(f, "NodesAdded({} from {origin})", nodes.len())
            }
            Self::NodesRemoved { nodes, origin, .. } => {
                write!(f, "NodesRemoved({} from {origin})", nodes.len())
            }
            Self::ElementsAdded {
                elements, origin, ..
            } => write!(f, "ElementsAdded({} from {origin})", elements.len()),
            Self::ElementsRemoved {
                elements, origin, ..
            } => write!(f, "ElementsRemoved({} from {origin})", elements.len()),
            Self::ElementsChanged { old, origin, .. } => {
                write!(f, "ElementsChanged({} from {origin})", old.len())
            }
            Self::MeshIsDistributed { origin } => write!(f, "MeshIsDistributed(from {origin})"),
        }
    }
}

/// Whether `event` names elements of the `(ty, ghost)` block: an
/// `ElementsAdded` listing one, an `ElementsRemoved` / `ElementsChanged`
/// whose numbering covers the block (or listing one).
fn names_block(event: &MeshEvent, ty: akantu_core::ElementType, ghost: GhostType) -> bool {
    let in_block = |e: &Element| e.ty() == ty && e.ghost_type() == ghost;
    match event {
        MeshEvent::ElementsAdded { elements, .. } => elements.iter().any(in_block),
        MeshEvent::ElementsRemoved {
            elements,
            new_numbering,
            ..
        } => new_numbering.get(ty, ghost).is_ok() || elements.iter().any(in_block),
        MeshEvent::ElementsChanged {
            old,
            new,
            new_numbering,
            ..
        } => {
            new_numbering.get(ty, ghost).is_ok()
                || old.iter().any(in_block)
                || new.iter().any(in_block)
        }
        _ => false,
    }
}

/// The composition rule of element events in one mutation
/// (`docs/ARCHITECTURE.md`, "Event rules").
///
/// Consumers receive every event of a mutation after the commit, with the
/// **final** mesh, and read it at the indices an element event names (the
/// FE engines compute the shapes of added elements there, the constitutive
/// law handlers size their element maps to the final block counts). An
/// event names the elements of a `(type, ghost)` block in the numbering
/// current when it was recorded, so an event that **renumbers** a block (a
/// non-identity `ElementsRemoved` or `ElementsChanged` numbering: elements
/// removed, holes filled by the last ones; upstream's `ChangedElementsEvent`
/// derives from `RemovedElementsEvent` and carries a numbering,
/// `mesh_events.hh:123-135`) invalidates every
/// **earlier** event of the same mutation that names that block: a second
/// erasure of the block (the first one's kept indices), an addition to it
/// (the added indices, moved into the holes or removed) or a change. Such
/// a composition is refused: record every erasure of a block in **one**
/// `erase_elements` call, **before** adding elements to it, and port an
/// upstream sequence that adds then removes as separate mutations.
/// Erasures of different blocks, additions after an erasure and any number
/// of additions compose.
///
/// `events` are the events of one mesh part that will be dispatched, in
/// recorded order. The mesh domain applies the rule in the capability
/// stage (3b) of a mutation for every part that has consumers
/// ([`crate::SharedMesh::mutate`]); a part without consumers receives
/// nothing and composes freely.
///
/// # Errors
/// `Error::Unsupported` (recoverable) naming the block and both events.
pub fn check_element_event_composition(events: &[&MeshEvent]) -> Result<()> {
    for (j, later) in events.iter().enumerate() {
        let (MeshEvent::ElementsRemoved { new_numbering, .. }
        | MeshEvent::ElementsChanged { new_numbering, .. }) = later
        else {
            continue;
        };
        for (ty, ghost, numbering) in new_numbering.as_map().iter_all() {
            let identity = numbering
                .as_slice()
                .iter()
                .enumerate()
                .all(|(i, &n)| usize::try_from(n).is_ok_and(|n| n == i));
            if identity {
                continue;
            }
            if let Some(i) = events[..j].iter().position(|e| names_block(e, ty, ghost)) {
                return Err(akantu_core::Error::Unsupported(format!(
                    "mesh mutation: event #{j} ({:?}) renumbers the {ty:?} ({ghost:?}) elements \
                     that the earlier event #{i} ({:?}) of the same mutation names; consumers \
                     read the final mesh at an event's indices: erase all \
                     elements of a block in one erase_elements call, before adding elements \
                     to it, or record the sequence as separate mutations",
                    later, events[i]
                )));
            }
        }
    }
    Ok(())
}

/// The composition rule of node events in one mutation on a **periodic or
/// distributed** mesh (`docs/ARCHITECTURE.md`,
/// "Event rules", rule 1).
///
/// A `NodesRemoved` names nodes in the numbering current when it was
/// recorded; consumers renumber their per-node state with it after the
/// commit and read the **final** mesh's node pairs (the periodic pairs, the
/// node synchronizer's schemes) through the result. A later node event of
/// the same mutation — another removal (a pure permutation included, which
/// keeps the node count), or an addition — leaves the final pairs in
/// another numbering than the one the first removal produces, so the
/// renumbering reads them at the wrong indices. On such a mesh a
/// `NodesRemoved` must therefore be the **last** node event of its
/// mutation: compose the removals into one numbering (one `remove_nodes`
/// call) and record additions before it, or use separate mutations. On a
/// serial, non-periodic mesh nothing is read through node pairs, and the
/// DOF manager's capability check decides (rule 1's other clauses).
///
/// `events` are the events of one mesh part that will be dispatched, in
/// recorded order; `pairs_read` says whether the mesh is periodic or
/// distributed. The mesh domain applies the rule in the capability stage
/// (3b) for every part that has consumers, before any consumer check
/// ([`crate::SharedMesh::mutate`]); the DOF manager's node-count guard
/// stays as a second line.
///
/// # Errors
/// `Error::Unsupported` (recoverable) naming the removal and the later
/// event.
pub fn check_node_event_composition(events: &[&MeshEvent], pairs_read: bool) -> Result<()> {
    if !pairs_read {
        return Ok(());
    }
    let Some(i) = events
        .iter()
        .position(|e| matches!(e, MeshEvent::NodesRemoved { .. }))
    else {
        return Ok(());
    };
    let later = events[i + 1..].iter().position(|e| {
        matches!(
            e,
            MeshEvent::NodesAdded { .. } | MeshEvent::NodesRemoved { .. }
        )
    });
    match later {
        Some(k) => {
            let j = i + 1 + k;
            Err(akantu_core::Error::Unsupported(format!(
                "mesh mutation: node event #{j} ({:?}) follows the node removal #{i} ({:?}) of \
                 the same mutation on a periodic or distributed mesh; the removal's numbering \
                 would read the final mesh's node pairs at the wrong indices: make the removal \
                 the last node event of its mutation (one \
                 remove_nodes call with the composed numbering, additions before it), or record \
                 the sequence as separate mutations",
                events[j], events[i]
            )))
        }
        None => Ok(()),
    }
}

/// Ordered list of events produced by one mutating operation.
#[derive(Clone, Debug, Default)]
#[must_use = "mesh events must be dispatched to all mesh-dependent components"]
pub struct MeshEvents(pub Vec<MeshEvent>);

impl MeshEvents {
    /// No events.
    pub fn none() -> Self {
        Self(Vec::new())
    }

    /// Append an event.
    pub fn push(&mut self, e: MeshEvent) {
        self.0.push(e);
    }

    /// `true` if empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Events in order.
    #[must_use]
    pub fn events(&self) -> &[MeshEvent] {
        &self.0
    }
}

/// Component that reacts to mesh changes (upstream `MeshEventHandler`).
///
/// A component may take part at **several priority levels** (a model is
/// one consumer whose FE engines react at 9, DOF manager at 20, own fields
/// and laws at 94, non-local manager at 100): the mesh domain calls
/// [`MeshEventHandler::on_mesh_event_at`] once per level, interleaved with
/// the other consumers exactly as upstream's single sorted handler list.
pub trait MeshEventHandler {
    /// Ordering key (upstream `EventHandlerPriority`) of single-level
    /// consumers.
    fn event_priority(&self) -> EventPriority;
    /// All levels this consumer reacts at (ascending); default: one level.
    fn event_priorities(&self) -> Vec<EventPriority> {
        vec![self.event_priority()]
    }
    /// React to one committed event; `mesh` is the already-updated mesh.
    fn on_mesh_event(&mut self, mesh: &Mesh, event: &MeshEvent) -> Result<()>;

    /// Capability check before commit: refuse an event
    /// this consumer cannot handle while the mutation can still be rolled
    /// back. The mesh domain calls it once per consumer for every event of
    /// the consumer's part that will be dispatched (non-zero global count),
    /// after the topology, hook and count stages and **before** the commit;
    /// `mesh` is the staged, final state of that part. Before any consumer,
    /// the domain refuses the element-event compositions consumers cannot
    /// follow on the final mesh ([`check_element_event_composition`]),
    /// so a check sees only events whose element
    /// indices are valid in the final mesh, and on a periodic or
    /// distributed mesh a node removal followed by another node event
    /// ([`check_node_event_composition`]).
    ///
    /// * **Agreement:** an error on any rank is agreed **recoverable**: the
    ///   mutation is rolled back on every rank and nothing is dispatched.
    ///   The refusing rank returns this consumer's error unchanged (its type
    ///   is the contract); the **other ranks** return a recoverable
    ///   `Error::Communication` ("phase '…:capabilities' failed on another
    ///   rank (recoverable)"), so code matching on the variant must accept
    ///   both. A typed fatal result (`Error::Fatal`) is
    ///   agreed fatal (the central rule of ARCHITECTURE.md, "Agreement across ranks") and poisons the domain on every
    ///   rank.
    /// * **Side-effect free and local:** no communication, no change of
    ///   observable state (interior mutability only for diagnostics). It
    ///   must not depend on state the dispatch of earlier events would
    ///   change, nor on the order in which consumers are checked: the domain
    ///   checks consumers in registration order, not in the dispatch
    ///   schedule (which runs equal priorities newest first), and the
    ///   accept/refuse outcome is the same in any order.
    /// * **Forwarding obligation:** a handler that
    ///   forwards the dispatch methods to inner handlers or components
    ///   (a model's FE engines and DOF manager, a coordinator's wrapped
    ///   model) must forward this check to the **same** inner handlers and
    ///   add its own refusals. The method is therefore **required** (no
    ///   default): every implementor states its check, and a consumer that
    ///   accepts every event returns `Ok(())` with a comment saying why its
    ///   dispatch cannot refuse (failures that are left to dispatch are
    ///   fatal there, after the commit).
    ///
    /// # Errors
    /// The refusal, recoverable unless it is a typed fatal error.
    fn check_mesh_event(&self, mesh: &Mesh, event: &MeshEvent) -> Result<()>;
    /// React at one of [`MeshEventHandler::event_priorities`]; defaults to
    /// [`MeshEventHandler::on_mesh_event`].
    fn on_mesh_event_at(
        &mut self,
        mesh: &Mesh,
        event: &MeshEvent,
        priority: EventPriority,
    ) -> Result<()> {
        let _ = priority;
        self.on_mesh_event(mesh, event)
    }

    /// React at one level as one step of the mutation's collective phase
    /// sequence. This is what the mesh domain calls.
    ///
    /// A consumer that **communicates** while reacting (DOF numbering,
    /// synchronizer updates, ghost exchanges) must override this method and
    /// perform every communication through `phases` — `Phases::run` for a
    /// prepared step (local validation agreed before any message),
    /// `Phases::collective` for agreed matched collectives, synchronizer
    /// `*_in` methods — never through a raw communicator, and must not
    /// start another collective after a failed phase (`phases.is_failed()`).
    /// The domain agrees the returned result afterwards (when the scope
    /// already failed, it classifies the result without communicating: a
    /// typed fatal result still decides this rank's fatal kind). Non-communicating consumers keep the default,
    /// which calls [`MeshEventHandler::on_mesh_event_at`].
    fn on_mesh_event_phased(
        &mut self,
        mesh: &Mesh,
        event: &MeshEvent,
        priority: EventPriority,
        phases: &mut Phases,
    ) -> Result<()> {
        let _ = phases;
        self.on_mesh_event_at(mesh, event, priority)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_record() {
        let mut ev = MeshEvents::none();
        assert!(ev.is_empty());
        ev.push(MeshEvent::NodesAdded {
            nodes: vec![4, 5],
            old_nodes: Some(vec![1, 2]),
            origin: "cohesive".into(),
            extension: None,
        });
        assert_eq!(
            format!("{:?}", ev.events()[0]),
            "NodesAdded(2 from cohesive)"
        );
    }
}
