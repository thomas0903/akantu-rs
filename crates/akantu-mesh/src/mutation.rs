// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/mesh/mesh.cc:105-150 (sendEvent<
// NewNodesEvent>), src/mesh/element_group.cc:176-202 (node groups rebuilt
// after cohesive node events), src/mesh_utils/global_ids_updater*.cc,
// src/model/solid_mechanics/solid_mechanics_model_cohesive/
// solid_mechanics_model_cohesive.cc:49-94 (CohesiveMeshGlobalDataUpdater),
// src/mesh_utils/cohesive_element_inserter_helper.cc:506,857,947 (facet
// events sent during insertion), src/mesh/mesh.cc (eraseElements,
// initMeshFacets), src/mesh_utils/mesh_utils.cc (purifyMesh).

//! Coordinated, staged mesh mutation
//! (D-019 revised).
//!
//! [`SharedMesh::mutate`] runs the stages below inside one
//! [`Phases`] scope on the mesh communicator. **No collective is started
//! before the agreement that makes it safe**, every stage boundary is an
//! agreement, and **every** error exit before commit restores the complete
//! mesh state (or, after a transport failure, poisons the domain):
//!
//! | # | Stage | Communication | On failure |
//! |---|---|---|---|
//! | – | **re-entry check**: the domain is idle (no mutation or dispatch in progress on this rank) | none — refused **locally** before any collective | error, nothing changed |
//! | 0 | **preflight**: not poisoned; the handlers are exactly the registered consumers; every consumer acknowledged the current revision; the dispatch schedule is computed; the write lock is taken | agreement | nothing changed (recoverable) |
//! | 1 | **topology** (caller's closure) through the journaled [`MeshTransaction`], event validation, local dependents (element-group node lists), schedule descriptor (event kinds/parts, consumer schedule, hook/facet presence) | agreement **with descriptor digest** | full rollback (recoverable) |
//! | 2 | **distributed hook** ([`DistributedHook`]): local `prepare`, agreement, collective `apply` through the journaled transaction and the phase API, agreement | hook phases | full rollback + `abort` (recoverable; transport failure ⇒ poisoned) |
//! | 3 | **bounding boxes** (local box after the hook's flags, then the global reduction) and **global event counts** | matched collectives | full rollback + `abort` (transport failure ⇒ poisoned) |
//! | 3b | **capability check**: for every part with consumers, the composition rule of element events ([`crate::events::check_element_event_composition`]) and, on a periodic or distributed mesh, of node events ([`crate::events::check_node_event_composition`]); then every consumer of its part checks each event that will be dispatched ([`MeshEventHandler::check_mesh_event`], local, on the staged mesh) | agreement | full rollback + `abort` (recoverable) |
//! | 4 | **commit**: journal discarded, revision bumped, hook `commit`; the write lock is kept (shared use) until acknowledgement | — | — |
//! | 5 | **dispatch**: events with a non-zero global count, in recorded order; per event every consumer of its part in the agreed order (ascending priority; at equal priority the most recently registered first, as upstream's `std::lower_bound` insertion) through [`MeshEventHandler::on_mesh_event_phased`]; one agreement per (event, consumer, level), skipped when the consumer's own phase already failed | consumer phases + agreements | domain **poisoned** on all ranks (fatal) |
//! | 6 | **acknowledge**: every consumer's revision = committed revision | — | — |
//!
//! The journal covers topology appends/reconnections of the mesh and its
//! facet mesh, node flags, global ids, owner ranks, journaled rows of the
//! adjacency datasets, and copy-on-write snapshots of element/node groups,
//! mesh data, ghost counters, synchronizer schemes and bounding boxes. The
//! mesh-data snapshot is an entry of the journal itself, so a rollback
//! replays data snapshots and data-row edits in strict reverse order (exact
//! for any interleaving). A failure after commit cannot
//! be undone (consumers may have resized and copied their fields): the
//! domain is poisoned — the documented recovery is to rebuild the
//! simulation, never to continue on inconsistent state.
//!
//! **Events and numbering domains.** Every event is validated
//! when it is recorded, against the mesh state at that moment
//! ([`MeshTransaction::record`]): a removal numbering maps the count before
//! its operation onto the current one, so successive removals and
//! removals followed by additions compose for the mesh; what may be
//! dispatched to consumers, which read the final mesh, is restricted by
//! the composition rules of stage 3b (element events:
//! [`crate::events::check_element_event_composition`]; node events: on a
//! periodic or distributed mesh a removal is the last node event of its
//! mutation,
//! [`crate::events::check_node_event_composition`], and the DOF manager's
//! check). An event is dispatched
//! when its measure summed over the ranks is non-zero; a non-identity
//! numbering counts even without removals (a purify permutation,
//! UD-135). The facet mesh shares the parent's node set: node appends,
//! duplications, removals and purification are requested on
//! [`MeshPart::Main`] and applied to both parts (flags, global ids and
//! owner ranks too), a node removal records `(Main, NodesRemoved)` then
//! `(Facets, NodesRemoved)` with the same numbering (UD-136), a
//! `NodesAdded` recorded on `Main` gets its `Facets` twin, and
//! the cross-part invariants (`crate::utils::check_facet_mesh_consistency`)
//! are validated at the end of the topology stage and after the hook.
//! The node events form a complete stream
//! ([`MeshTransaction::record`]): every appended node is
//! announced once, in order, before any removal maps it, so the consumers
//! of both parts can follow the shared node set from the events alone; a
//! facet mesh is never created under registered `Facets` consumers.
//!
//! Observational difference to upstream (checked by the INT-003
//! fixture): upstream updates groups and the bounding box inside the node
//! event (`mesh.cc:114-150`) and sends facet events during insertion;
//! here all mesh-owned dependents are updated before the first event is
//! dispatched, so consumers of the element event already see the final
//! groups and boxes.

use std::collections::HashMap;

use akantu_comm::phase::{Phases, Severity, digest_bytes};
use akantu_core::enums::EventPriority;
use akantu_core::{
    Array, Element, ElementType, ElementTypeMapArray, Error, FatalKind, GhostType, GlobalIdx, Idx,
    NodeFlag, Rank, Real, Result, SharedArray, SharedElementTypeMapArray, try_to_usize,
};

use crate::MeshEventHandler;
use crate::data::MeshData;
use crate::events::{MeshEvent, MeshEvents};
use crate::group::GroupManager;
use crate::mesh::{BBox, Mesh};
use crate::shared::{ConsumerToken, MeshPart, Phase, SharedMesh};
use crate::synchronizers::MeshSynchronizers;

enum Undo {
    Nodes {
        part: MeshPart,
        len: usize,
    },
    Elements {
        part: MeshPart,
        ty: ElementType,
        ghost: GhostType,
        len: usize,
        created: bool,
    },
    Reconnect {
        part: MeshPart,
        element: Element,
        old: Vec<Idx>,
    },
    Flag {
        part: MeshPart,
        node: usize,
        old: NodeFlag,
    },
    GlobalId {
        part: MeshPart,
        node: usize,
        old: GlobalIdx,
    },
    GlobalIdsCreated {
        part: MeshPart,
    },
    NbGlobalNodes {
        part: MeshPart,
        old: usize,
    },
    Prank {
        part: MeshPart,
        node: Idx,
        old: Option<Rank>,
    },
    /// Whole node/element state of a part before a removal (renumbering
    /// touches every array).
    Core {
        part: MeshPart,
        core: Box<CoreState>,
    },
    /// The facet mesh was created by this transaction.
    FacetsCreated,
    /// The node synchronizer of a part before
    /// [`MeshTransaction::replace_node_synchronizer`] (instead of a snapshot
    /// of every synchronizer of the part; performance).
    /// (Boxed: the entries of every journal stay small.)
    NodeSynchronizer {
        part: MeshPart,
        old: Option<Box<crate::synchronizers::NodeSynchronizer>>,
    },
    /// The node lists of the element groups of a part before
    /// [`MeshTransaction::rebuild_element_group_nodes`] (instead of a
    /// snapshot of the whole group manager; performance).
    GroupNodes {
        part: MeshPart,
        lists: Vec<(crate::group::NodeGroupId, Vec<Idx>)>,
    },
    /// Copy-on-write snapshot of a part's mesh data, taken on the first
    /// [`MeshTransaction::data_mut`] access. Kept **in the journal**
    /// so that rollback restores it at its position: data-row
    /// edits journaled after it are undone first (the snapshot then covers
    /// them), edits journaled before it are undone on top of the restored
    /// snapshot — exact for any interleaving of row edits and snapshots.
    Data {
        part: MeshPart,
        data: Box<MeshData>,
    },
    /// One row of an `Element`-valued elemental dataset was replaced
    /// (journaled adjacency edit, [`MeshTransaction::set_element_data_row`]).
    DataElementRow {
        part: MeshPart,
        name: std::borrow::Cow<'static, str>,
        element: Element,
        old: Vec<Element>,
    },
    /// One row of an `ElementList` dataset was replaced
    /// ([`MeshTransaction::set_element_list_row`]).
    DataListRow {
        part: MeshPart,
        name: std::borrow::Cow<'static, str>,
        element: Element,
        old: Vec<Element>,
    },
    /// Rows were appended to a block of an `Element` / `ElementList`
    /// dataset (truncate to `len`; remove the block / dataset if created).
    DataRows {
        part: MeshPart,
        name: std::borrow::Cow<'static, str>,
        ty: ElementType,
        ghost: GhostType,
        len: usize,
        created_block: bool,
        created_dataset: bool,
    },
}

/// Node and element arrays of one mesh part (removal snapshots).
struct CoreState {
    nodes: SharedArray<Real>,
    flags: Array<NodeFlag>,
    global_ids: Option<Array<GlobalIdx>>,
    prank: HashMap<Idx, Rank>,
    connectivities: SharedElementTypeMapArray<Idx>,
    ghosts_counters: ElementTypeMapArray<Idx>,
    slave_master: HashMap<Idx, Idx>,
    master_slaves: HashMap<Idx, Vec<Idx>>,
    bbox: (BBox, BBox),
}

impl CoreState {
    fn of(m: &Mesh) -> Self {
        Self {
            nodes: m.nodes.clone(),
            flags: m.nodes_flags.clone(),
            global_ids: m.nodes_global_ids.clone(),
            prank: m.nodes_prank.clone(),
            connectivities: m.connectivities.clone(),
            ghosts_counters: m.ghosts_counters.clone(),
            slave_master: m.periodic_slave_master.clone(),
            master_slaves: m.periodic_master_slaves.clone(),
            bbox: (m.bbox.clone(), m.bbox_local.clone()),
        }
    }

    fn restore(self, m: &mut Mesh) {
        m.nodes = self.nodes;
        m.nodes_flags = self.flags;
        m.nodes_global_ids = self.global_ids;
        m.nodes_prank = self.prank;
        m.connectivities = self.connectivities;
        m.ghosts_counters = self.ghosts_counters;
        m.periodic_slave_master = self.slave_master;
        m.periodic_master_slaves = self.master_slaves;
        (m.bbox, m.bbox_local) = self.bbox;
    }
}

/// Copy-on-write snapshots of mesh-owned components, per part
/// (`[Main, Facets]`), restored on rollback.
#[derive(Default)]
struct Saved {
    groups: [Option<GroupManager>; 2],
    /// A mesh-data snapshot of the part is in the journal ([`Undo::Data`]).
    data: [bool; 2],
    synchronizers: [Option<MeshSynchronizers>; 2],
    ghosts_counters: [Option<ElementTypeMapArray<Idx>>; 2],
    bbox: [Option<(BBox, BBox)>; 2],
}

/// The name of an adjacency dataset for the journal: the usual names
/// borrowed, others copied (one allocation less per journaled row edit;
/// performance).
fn dataset_name(name: &str) -> std::borrow::Cow<'static, str> {
    use crate::utils::{ELEMENT_TO_SUBELEMENT, MESH_TO_MESH_FACET, SUBELEMENT_TO_ELEMENT};
    match name {
        ELEMENT_TO_SUBELEMENT => std::borrow::Cow::Borrowed(ELEMENT_TO_SUBELEMENT),
        SUBELEMENT_TO_ELEMENT => std::borrow::Cow::Borrowed(SUBELEMENT_TO_ELEMENT),
        MESH_TO_MESH_FACET => std::borrow::Cow::Borrowed(MESH_TO_MESH_FACET),
        other => std::borrow::Cow::Owned(other.to_owned()),
    }
}

fn slot(part: MeshPart) -> usize {
    match part {
        MeshPart::Main => 0,
        MeshPart::Facets => 1,
    }
}

/// Journaled edits of one mutation: topology (stage 1) and distributed
/// state (stage 2, through the [`DistributedHook`]). Every mutable access
/// to mesh state goes through this type, so a rollback restores the
/// complete state.
pub struct MeshTransaction<'m> {
    mesh: &'m mut Mesh,
    journal: Vec<Undo>,
    saved: Saved,
    events: Vec<(MeshPart, MeshEvent)>,
    /// First event found invalid when it was recorded (reported by the
    /// topology stage).
    invalid_event: Option<Error>,
    /// Set while the transaction itself records a node event: the `Facets`
    /// twin of a `NodesAdded` or the two events of its
    /// removal path.
    recording_internal: bool,
    /// Size of the shared node set as announced so far to the consumers of
    /// both parts (node event stream): the node
    /// count at the start of the transaction, advanced by every `NodesAdded`
    /// and `NodesRemoved` recorded on [`MeshPart::Main`].
    announced_nodes: usize,
    /// `MeshPart::Facets` consumers are registered: the facet mesh must not
    /// be created by this transaction (they could not follow it).
    facet_consumers: bool,
    /// The state the transaction started from passed the cross-part check
    /// ([`crate::utils::check_facet_mesh_consistency`]) and the domain
    /// allows the restricted form: the adjacency references are read only
    /// from the rows this transaction replaced or appended
    /// ([`MeshTransaction::touched_rows`]).
    restricted_check: bool,
}

impl std::fmt::Debug for MeshTransaction<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MeshTransaction")
            .field("journal", &self.journal.len())
            .field("events", &self.events.len())
            .finish()
    }
}

fn part_mut(mesh: &mut Mesh, part: MeshPart) -> Result<&mut Mesh> {
    match part {
        MeshPart::Main => Ok(mesh),
        MeshPart::Facets => mesh
            .mesh_facets
            .as_deref_mut()
            .ok_or_else(|| Error::Mesh("the mesh has no facet mesh".into())),
    }
}

fn part_ref(mesh: &Mesh, part: MeshPart) -> Result<&Mesh> {
    match part {
        MeshPart::Main => Ok(mesh),
        MeshPart::Facets => mesh
            .mesh_facets()
            .ok_or_else(|| Error::Mesh("facet event without facet mesh".into())),
    }
}

fn check_node(m: &Mesh, n: usize) -> Result<()> {
    if n < m.nodes.len() {
        Ok(())
    } else {
        Err(Error::OutOfRange {
            what: "node",
            index: n,
            len: m.nodes.len(),
        })
    }
}

impl<'m> MeshTransaction<'m> {
    fn new(mesh: &'m mut Mesh, facet_consumers: bool, restricted_check: bool) -> Self {
        let announced_nodes = mesh.nodes.len();
        Self {
            mesh,
            journal: Vec::new(),
            saved: Saved::default(),
            events: Vec::new(),
            invalid_event: None,
            recording_internal: false,
            announced_nodes,
            facet_consumers,
            restricted_check,
        }
    }

    /// The adjacency rows this transaction replaced or appended
    /// ([`crate::utils::TouchedRows`]), or `None` when the journal holds an
    /// edit after which the restricted cross-part check proves nothing: a
    /// removal (blocks shrink, every row is renumbered), a snapshot of a
    /// part's mesh data (any row may have changed), or a facet mesh
    /// created by the transaction; also `None` when more replaced rows than
    /// an eighth of the facet mesh's `element_to_subelement` rows were
    /// journaled (the full check is then the cheaper one).
    fn touched_rows(&self) -> Option<crate::utils::TouchedRows> {
        use crate::utils::{
            AdjacencySet as A, ELEMENT_TO_SUBELEMENT, MESH_TO_MESH_FACET, SUBELEMENT_TO_ELEMENT,
        };
        let set = |part: MeshPart, name: &str| match (part, name) {
            (MeshPart::Facets, ELEMENT_TO_SUBELEMENT) => Some(A::ElementToSubelement),
            (MeshPart::Facets, SUBELEMENT_TO_ELEMENT) => Some(A::SubelementToElement),
            (MeshPart::Main, MESH_TO_MESH_FACET) => Some(A::MeshToMeshFacet),
            _ => None,
        };
        let mut t = crate::utils::TouchedRows::default();
        for u in &self.journal {
            match u {
                Undo::Core { .. } | Undo::Data { .. } | Undo::FacetsCreated => return None,
                Undo::DataElementRow {
                    part,
                    name,
                    element,
                    ..
                }
                | Undo::DataListRow {
                    part,
                    name,
                    element,
                    ..
                } => {
                    if let Some(a) = set(*part, name) {
                        t.rows.push((a, *element));
                    }
                }
                Undo::DataRows {
                    part,
                    name,
                    ty,
                    ghost,
                    len,
                    ..
                } => {
                    if let Some(a) = set(*part, name) {
                        // one range per block, from its lowest appended row
                        // (rows are appended one by one: a range per append
                        // would read the block's tail once per append)
                        match t
                            .tails
                            .iter_mut()
                            .find(|(b, bt, bg, _)| *b == a && bt == ty && bg == ghost)
                        {
                            Some(e) => e.3 = e.3.min(*len),
                            None => t.tails.push((a, *ty, *ghost, *len)),
                        }
                    }
                }
                Undo::Nodes { .. }
                | Undo::Elements { .. }
                | Undo::Reconnect { .. }
                | Undo::Flag { .. }
                | Undo::GlobalId { .. }
                | Undo::GlobalIdsCreated { .. }
                | Undo::NbGlobalNodes { .. }
                | Undo::Prank { .. }
                | Undo::GroupNodes { .. }
                | Undo::NodeSynchronizer { .. } => {}
            }
        }
        // a transaction that touched a large share of the rows (a
        // whole-mesh insertion) is checked in full: cheaper than the
        // touched rows, and it proves more
        let facet_rows = self.mesh.mesh_facets().map_or(0, |f| {
            f.data()
                .element_lists(crate::utils::ELEMENT_TO_SUBELEMENT)
                .map_or(0, |m| m.as_map().iter_all().map(|(_, _, a)| a.len()).sum())
        });
        if t.rows.len() > facet_rows / 8 {
            return None;
        }
        // a row edited several times is read once
        let mut seen = std::collections::HashSet::with_capacity(t.rows.len());
        t.rows.retain(|r| seen.insert(*r));
        Some(t)
    }

    /// The cross-part check of the mesh as it is now: restricted to the
    /// touched rows when the transaction started from a checked state and
    /// its journal allows it, in full otherwise.
    fn check_facet_consistency(&self) -> Result<()> {
        let touched = if self.restricted_check {
            self.touched_rows()
        } else {
            None
        };
        crate::utils::check_facet_mesh_consistency_rows(self.mesh, touched.as_ref())
    }

    /// Read access to the mesh being modified.
    #[must_use]
    pub fn mesh(&self) -> &Mesh {
        self.mesh
    }

    /// Events recorded so far (dispatch order).
    #[must_use]
    pub fn events(&self) -> &[(MeshPart, MeshEvent)] {
        &self.events
    }

    /// The parts holding the shared node set: the mesh
    /// and, when it exists, its facet mesh. Node numbering changes are
    /// requested on [`MeshPart::Main`]; asking for them on the facet mesh
    /// alone is refused.
    fn node_parts(&self, part: MeshPart, what: &str) -> Result<&'static [MeshPart]> {
        if part == MeshPart::Facets {
            return Err(Error::Mesh(format!(
                "{what} on the facet mesh: it shares the parent's node numbering \
                 (docs/ARCHITECTURE.md, mesh representation); change the nodes through MeshPart::Main"
            )));
        }
        Ok(if self.mesh.mesh_facets.is_some() {
            &[MeshPart::Main, MeshPart::Facets]
        } else {
            &[MeshPart::Main]
        })
    }

    /// Append node coordinates `x` with `flag` to the shared node set.
    fn append_shared_node(
        &mut self,
        parts: &[MeshPart],
        x: &[Real],
        flag: NodeFlag,
    ) -> Result<Idx> {
        let len = self.mesh.nodes.len();
        let n = akantu_core::try_idx(len)?;
        for &p in parts {
            if part_ref(self.mesh, p)?.nodes.len() != len {
                return Err(Error::Mesh(
                    "the facet mesh does not share the parent's node numbering".into(),
                ));
            }
        }
        for &p in parts {
            let m = part_mut(self.mesh, p)?;
            m.nodes.borrow_mut().try_push_tuple(x)?;
            m.nodes_flags.push_value(flag);
            if let Some(g) = &mut m.nodes_global_ids {
                g.push_value(akantu_core::INVALID_GLOBAL_IDX);
            }
            self.journal.push(Undo::Nodes { part: p, len });
        }
        Ok(n)
    }

    /// Append a copy of node `old` (coordinates and flags) to the shared
    /// node set — the mesh and its facet mesh (cohesive node doubling).
    /// Returns the new node. `part` must be [`MeshPart::Main`].
    pub fn duplicate_node(&mut self, part: MeshPart, old: Idx) -> Result<Idx> {
        let parts = self.node_parts(part, "duplicate_node")?;
        let o = try_to_usize(old)?;
        check_node(self.mesh, o)?;
        let x: Vec<Real> = self.mesh.nodes.borrow().tuple(o).to_vec();
        let flag = self.mesh.nodes_flags.as_slice()[o];
        self.append_shared_node(parts, &x, flag)
    }

    /// Append a new node with flag `NORMAL` to the shared node set — the
    /// mesh and its facet mesh. `part` must be [`MeshPart::Main`].
    /// The `NodesAdded` event is recorded by the caller on
    /// [`MeshPart::Main`]; the transaction records its `Facets` twin
    /// (see [`MeshTransaction::record`]).
    pub fn push_node(&mut self, part: MeshPart, x: &[Real]) -> Result<Idx> {
        let parts = self.node_parts(part, "push_node")?;
        self.append_shared_node(parts, x, NodeFlag::NORMAL)
    }

    /// Append an element of `(ty, ghost)` with validated connectivity.
    pub fn push_element(
        &mut self,
        part: MeshPart,
        ty: ElementType,
        ghost: GhostType,
        conn: &[Idx],
    ) -> Result<Element> {
        let m = part_mut(self.mesh, part)?;
        for &n in conn {
            check_node(m, try_to_usize(n)?)?;
        }
        if conn.len() != ty.nb_nodes_per_element() {
            return Err(Error::DimensionMismatch {
                context: "MeshTransaction::push_element",
                expected: ty.nb_nodes_per_element(),
                got: conn.len(),
            });
        }
        let created = !m.connectivities.exists(ty, ghost);
        // upstream ElementTypeMapArray::alloc id: "<container id>:<type>[:ghost]"
        let id = format!(
            "{}:{ty}{}",
            m.connectivities.id(),
            if ghost == GhostType::Ghost {
                ":ghost"
            } else {
                ""
            }
        );
        let c = m
            .connectivities
            .as_map_mut()
            .get_or_insert_with(ty, ghost, || {
                akantu_core::Array::with_id(conn.len(), id).into()
            });
        let len = c.len();
        let element = Element::try_new(ty, len, ghost);
        // the guard ends with the push (explicit guard `Drop`)
        let pushed = c.borrow_mut().try_push_tuple(conn);
        if let Err(e) = pushed {
            if created {
                m.connectivities.remove(ty, ghost);
            }
            return Err(e);
        }
        self.journal.push(Undo::Elements {
            part,
            ty,
            ghost,
            len,
            created,
        });
        element
    }

    /// Replace the nodes of an existing element (re-connection to doubled
    /// nodes).
    pub fn set_element_nodes(
        &mut self,
        part: MeshPart,
        element: Element,
        conn: &[Idx],
    ) -> Result<()> {
        let m = part_mut(self.mesh, part)?;
        for &n in conn {
            check_node(m, try_to_usize(n)?)?;
        }
        let (mut block, i) = m.connectivities.element_guard_mut(element)?;
        let slot = block.tuple_mut(i);
        if slot.len() != conn.len() {
            return Err(Error::DimensionMismatch {
                context: "MeshTransaction::set_element_nodes",
                expected: slot.len(),
                got: conn.len(),
            });
        }
        let old = slot.to_vec();
        slot.copy_from_slice(conn);
        self.journal.push(Undo::Reconnect { part, element, old });
        Ok(())
    }

    /// Every part sharing the node set, for an edit requested on `part`
    /// (flags, global ids and owner ranks are shared like the coordinates).
    fn shared_parts(&self, part: MeshPart) -> Result<&'static [MeshPart]> {
        part_ref(self.mesh, part)?;
        Ok(if self.mesh.mesh_facets.is_some() {
            &[MeshPart::Main, MeshPart::Facets]
        } else {
            &[MeshPart::Main]
        })
    }

    /// Set the flag of node `node` (distributed status of new or shared
    /// nodes, upstream `CohesiveMeshGlobalDataUpdater`) in the shared node
    /// set (the mesh and its facet mesh, whichever `part` names).
    pub fn set_node_flag(&mut self, part: MeshPart, node: Idx, flag: NodeFlag) -> Result<()> {
        let parts = self.shared_parts(part)?;
        let n = try_to_usize(node)?;
        for &p in parts {
            check_node(part_mut(self.mesh, p)?, n)?;
        }
        for &p in parts {
            let m = part_mut(self.mesh, p)?;
            let f = &mut m.nodes_flags.as_mut_slice()[n];
            self.journal.push(Undo::Flag {
                part: p,
                node: n,
                old: *f,
            });
            *f = flag;
        }
        Ok(())
    }

    /// Set the global id of node `node` in the shared node set (the
    /// global-id array is created, initialised to local ids, if a part had
    /// none).
    pub fn set_node_global_id(&mut self, part: MeshPart, node: Idx, id: GlobalIdx) -> Result<()> {
        let parts = self.shared_parts(part)?;
        let n = try_to_usize(node)?;
        for &p in parts {
            check_node(part_ref(self.mesh, p)?, n)?;
        }
        for &p in parts {
            let m = part_mut(self.mesh, p)?;
            if m.nodes_global_ids.is_none() {
                let len = m.nodes.len();
                let ids = (0..len)
                    .map(|i| akantu_core::try_global_idx(u64::try_from(i).unwrap_or(u64::MAX)))
                    .collect::<Result<Vec<_>>>()?;
                m.nodes_global_ids = Some(akantu_core::Array::from_vec(ids, 1)?);
                self.journal.push(Undo::GlobalIdsCreated { part: p });
            }
            let g = &mut m
                .nodes_global_ids
                .as_mut()
                .ok_or_else(|| Error::Mesh("global ids missing".into()))?
                .as_mut_slice()[n];
            self.journal.push(Undo::GlobalId {
                part: p,
                node: n,
                old: *g,
            });
            *g = id;
        }
        Ok(())
    }

    /// Set the global node count of the shared node set (the mesh and its
    /// facet mesh; upstream `MeshAccessor::setNbGlobalNodes` in
    /// `GlobalIdsUpdater::updateGlobalIDsLocally` after cohesive node
    /// doubling).
    pub fn set_nb_global_nodes(&mut self, part: MeshPart, n: usize) -> Result<()> {
        let parts = self.shared_parts(part)?;
        for &p in parts {
            let m = part_mut(self.mesh, p)?;
            self.journal.push(Undo::NbGlobalNodes {
                part: p,
                old: m.nb_global_nodes,
            });
            m.nb_global_nodes = n;
        }
        Ok(())
    }

    /// Set (or clear with `None`) the owner rank of node `node` in the
    /// shared node set.
    pub fn set_node_prank(&mut self, part: MeshPart, node: Idx, rank: Option<Rank>) -> Result<()> {
        let parts = self.shared_parts(part)?;
        for &p in parts {
            check_node(part_ref(self.mesh, p)?, try_to_usize(node)?)?;
        }
        for &p in parts {
            let m = part_mut(self.mesh, p)?;
            let old = match rank {
                Some(r) => m.nodes_prank.insert(node, r),
                None => m.nodes_prank.remove(&node),
            };
            self.journal.push(Undo::Prank { part: p, node, old });
        }
        Ok(())
    }

    /// Groups of `part` (snapshot on first access, restored on rollback).
    pub fn groups_mut(&mut self, part: MeshPart) -> Result<&mut GroupManager> {
        let m = part_mut(self.mesh, part)?;
        let s = &mut self.saved.groups[slot(part)];
        if s.is_none() {
            // the snapshot is the state before the transaction: node lists
            // rebuilt earlier in it are put back in the copy
            let mut g = m.groups.clone();
            for u in self.journal.iter().rev() {
                if let Undo::GroupNodes { part: p, lists } = u
                    && *p == part
                {
                    g.restore_node_lists(lists);
                }
            }
            *s = Some(g);
        }
        Ok(&mut m.groups)
    }

    /// Mesh data of `part` (`element_to_subelement`, `subelement_to_element`,
    /// …; snapshot on first access).
    pub fn data_mut(&mut self, part: MeshPart) -> Result<&mut MeshData> {
        let m = part_mut(self.mesh, part)?;
        let taken = &mut self.saved.data[slot(part)];
        if !*taken {
            *taken = true;
            self.journal.push(Undo::Data {
                part,
                data: Box::new(m.data.clone()),
            });
        }
        Ok(&mut m.data)
    }

    /// Synchronizer schemes of `part` (snapshot on first access).
    pub fn synchronizers_mut(&mut self, part: MeshPart) -> Result<&mut MeshSynchronizers> {
        let m = part_mut(self.mesh, part)?;
        let s = &mut self.saved.synchronizers[slot(part)];
        if s.is_none() {
            // the snapshot is the state before the transaction: a node
            // synchronizer replaced earlier in it is put back in the copy
            let mut copy = m.synchronizers.clone();
            for u in &self.journal {
                if let Undo::NodeSynchronizer { part: p, old } = u
                    && *p == part
                {
                    // the first replacement holds the original
                    copy.node = old.as_deref().cloned();
                    break;
                }
            }
            *s = Some(copy);
        }
        Ok(&mut m.synchronizers)
    }

    /// Replace the node synchronizer of `part` (the distributed cohesive
    /// hook's rebuilt node schemes); journaled: the replaced synchronizer
    /// is kept for the rollback, the other synchronizers of the part are
    /// not copied.
    ///
    /// # Errors
    /// An absent part.
    pub fn replace_node_synchronizer(
        &mut self,
        part: MeshPart,
        node: Option<crate::synchronizers::NodeSynchronizer>,
    ) -> Result<()> {
        let m = part_mut(self.mesh, part)?;
        let old = std::mem::replace(&mut m.synchronizers.node, node);
        if self.saved.synchronizers[slot(part)].is_none() {
            self.journal.push(Undo::NodeSynchronizer {
                part,
                old: old.map(Box::new),
            });
        }
        Ok(())
    }

    /// Ghost counters of `part` (snapshot on first access).
    pub fn ghosts_counters_mut(&mut self, part: MeshPart) -> Result<&mut ElementTypeMapArray<Idx>> {
        let m = part_mut(self.mesh, part)?;
        let s = &mut self.saved.ghosts_counters[slot(part)];
        if s.is_none() {
            *s = Some(m.ghosts_counters.clone());
        }
        Ok(&mut m.ghosts_counters)
    }

    /// Snapshot everything a removal in `part` may change (core arrays,
    /// groups, data, ghost counters, bounding boxes).
    fn save_for_removal(&mut self, part: MeshPart) -> Result<()> {
        let core = Box::new(CoreState::of(part_ref(self.mesh, part)?));
        self.journal.push(Undo::Core { part, core });
        self.groups_mut(part)?;
        self.data_mut(part)?;
        self.ghosts_counters_mut(part)?;
        self.save_bbox(part)
    }

    /// Erase elements of `part` with upstream's renumbering (upstream
    /// `Mesh::eraseElements`; see [`crate::removal`] for the numbering, the
    /// index domains and the dependents that follow) and record the
    /// `ElementsRemoved` event. Removing parent elements also renumbers the
    /// parent-domain rows and references kept in the facet mesh (never its
    /// facet blocks); removing facets also updates the
    /// parent's `mesh_to_mesh_facet` values.
    pub fn erase_elements(&mut self, part: MeshPart, elements: &[Element]) -> Result<()> {
        crate::removal::check_facet_domains(self.mesh)?;
        let target = part_ref(self.mesh, part)?;
        target.check_consistency()?;
        let plan = crate::removal::plan_element_removal(target, elements)?;
        self.save_for_removal(part)?;
        let other = match part {
            MeshPart::Main => MeshPart::Facets,
            MeshPart::Facets => MeshPart::Main,
        };
        if part_ref(self.mesh, other).is_ok() {
            self.data_mut(other)?;
        }
        let ev =
            crate::removal::apply_element_removal(self.mesh, part, plan, "Mesh::eraseElements")?;
        self.record(part, ev);
        Ok(())
    }

    /// Remove/renumber the shared nodes with `numbering` (old → new, `-1`
    /// removed; upstream `RemovedNodesEvent`), see [`crate::removal`]: the
    /// mesh and its facet mesh (shared node numbering) follow. Records the
    /// `NodesRemoved` event of the mesh and then, with a facet mesh, the
    /// same event for the facet mesh's consumers. `part`
    /// must be [`MeshPart::Main`]: a facet-only renumbering would break the
    /// shared numbering.
    pub fn remove_nodes(&mut self, part: MeshPart, numbering: &[Idx]) -> Result<()> {
        self.remove_nodes_with_origin(part, numbering, "Mesh::removeNodes")
    }

    fn remove_nodes_with_origin(
        &mut self,
        part: MeshPart,
        numbering: &[Idx],
        origin: &str,
    ) -> Result<()> {
        let parts = self.node_parts(part, "node removal")?;
        crate::removal::plan_node_removal(self.mesh, numbering)?;
        for &p in parts {
            self.save_for_removal(p)?;
        }
        for (p, ev) in crate::removal::apply_node_removal(self.mesh, numbering, origin)? {
            // the removal returns the `Facets` twin itself
            self.recording_internal = true;
            self.record(p, ev);
            self.recording_internal = false;
        }
        Ok(())
    }

    /// Upstream `MeshUtils::purifyMesh`: unused nodes removed and every
    /// node renumbered by first appearance
    /// ([`crate::utils::purify_numbering`]; nodes used only by facets are
    /// kept). The event is recorded even for a pure permutation (upstream
    /// sends it for an empty removal list). `part` must be
    /// [`MeshPart::Main`].
    pub fn purify(&mut self, part: MeshPart) -> Result<()> {
        self.node_parts(part, "purify")?;
        let numbering = crate::utils::purify_numbering(self.mesh)?;
        self.remove_nodes_with_origin(part, &numbering, "MeshUtils::purifyMesh")
    }

    /// Replace one row of the `Element`-valued elemental dataset `name` of
    /// `part` (adjacency rows such as `subelement_to_element`; cohesive
    /// insertion), journaled row by row instead of a snapshot of
    /// the whole mesh data, so an insertion pass costs O(edited rows).
    ///
    /// # Errors
    /// Unknown dataset or type mismatch, element out of range, or a row of
    /// the wrong length.
    pub fn set_element_data_row(
        &mut self,
        part: MeshPart,
        name: &str,
        element: Element,
        row: &[Element],
    ) -> Result<()> {
        let m = part_mut(self.mesh, part)?;
        let slot = m.data.element_map_mut(name)?.element_mut(element)?;
        if slot.len() != row.len() {
            return Err(Error::DimensionMismatch {
                context: "MeshTransaction::set_element_data_row",
                expected: slot.len(),
                got: row.len(),
            });
        }
        let old = slot.to_vec();
        slot.copy_from_slice(row);
        self.journal.push(Undo::DataElementRow {
            part,
            name: dataset_name(name),
            element,
            old,
        });
        Ok(())
    }

    /// Replace one row of the `ElementList` dataset `name` of `part`
    /// (`element_to_subelement`), journaled (see
    /// [`MeshTransaction::set_element_data_row`]).
    ///
    /// # Errors
    /// Unknown dataset or type mismatch, element out of range.
    pub fn set_element_list_row(
        &mut self,
        part: MeshPart,
        name: &str,
        element: Element,
        row: Vec<Element>,
    ) -> Result<()> {
        let m = part_mut(self.mesh, part)?;
        let slot = m.data.element_lists_mut(name)?.element_mut(element)?;
        let old = std::mem::replace(&mut slot[0], row);
        self.journal.push(Undo::DataListRow {
            part,
            name: dataset_name(name),
            element,
            old,
        });
        Ok(())
    }

    /// Append rows (`rows.len() / nb_component` of them) to the `(ty,
    /// ghost)` block of the `Element`-valued dataset `name` of `part`,
    /// creating the dataset or block if missing (adjacency contract,
    /// ARCHITECTURE: one row per element); journaled.
    ///
    /// # Errors
    /// A dataset of another type, a component-count mismatch, or a length
    /// that is not a multiple of `nb_component`.
    pub fn push_element_data_rows(
        &mut self,
        part: MeshPart,
        name: &str,
        ty: ElementType,
        ghost: GhostType,
        nb_component: usize,
        rows: &[Element],
    ) -> Result<()> {
        if nb_component == 0 || !rows.len().is_multiple_of(nb_component) {
            return Err(Error::DimensionMismatch {
                context: "MeshTransaction::push_element_data_rows",
                expected: nb_component,
                got: rows.len(),
            });
        }
        let m = part_mut(self.mesh, part)?;
        let created_dataset = !m.data.has_elemental(name);
        let map = m.data.element_map_mut_or_insert(name)?;
        let created_block = !map.exists(ty, ghost);
        if created_block {
            map.alloc(0, nb_component, ty, ghost, Element::NULL)?;
        }
        let block = map.get_mut(ty, ghost)?;
        if block.nb_component() != nb_component {
            return Err(Error::DimensionMismatch {
                context: "MeshTransaction::push_element_data_rows: components",
                expected: block.nb_component(),
                got: nb_component,
            });
        }
        let len = block.len();
        for r in rows.chunks_exact(nb_component) {
            block.try_push_tuple(r)?;
        }
        self.journal.push(Undo::DataRows {
            part,
            name: dataset_name(name),
            ty,
            ghost,
            len,
            created_block,
            created_dataset,
        });
        Ok(())
    }

    /// Append one row to the `(ty, ghost)` block of the `ElementList`
    /// dataset `name` of `part` (created if missing); journaled.
    ///
    /// # Errors
    /// A dataset of another type.
    pub fn push_element_list_row(
        &mut self,
        part: MeshPart,
        name: &str,
        ty: ElementType,
        ghost: GhostType,
        row: Vec<Element>,
    ) -> Result<()> {
        let m = part_mut(self.mesh, part)?;
        let created_dataset = !m.data.has_elemental(name);
        let map = m.data.element_lists_mut_or_insert(name)?;
        let created_block = !map.exists(ty, ghost);
        if created_block {
            map.alloc(0, 1, ty, ghost, Vec::new())?;
        }
        let block = map.get_mut(ty, ghost)?;
        let len = block.len();
        block.try_push_tuple(&[row])?;
        self.journal.push(Undo::DataRows {
            part,
            name: dataset_name(name),
            ty,
            ghost,
            len,
            created_block,
            created_dataset,
        });
        Ok(())
    }

    /// Upstream `Mesh::initMeshFacets()` inside a mutation: builds and
    /// installs the facet mesh (no-op if it exists) and the parent's
    /// `mesh_to_mesh_facet` data ([`crate::utils::create_mesh_facets`]);
    /// rolled back with the transaction. No event (the parent's topology
    /// is unchanged).
    ///
    /// A mesh distributed over several ranks needs collectives to build its
    /// facet mesh, which a topology closure must not start: build it before
    /// the mutation with
    /// [`crate::facet_synchronizer::create_distributed_mesh_facets`] and
    /// install it here with [`MeshTransaction::install_mesh_facets`]
    /// (refused here, before any change).
    ///
    /// Refused (before any change) while [`MeshPart::Facets`] consumers
    /// are registered: they could not follow a facet mesh that appears
    /// during a mutation (no event announces it; see
    /// [`MeshTransaction::record`], node event stream). Upstream creates
    /// the facet mesh before the facet FE engine (constructor of
    /// `SolidMechanicsModelCohesive`).
    pub fn init_mesh_facets(&mut self) -> Result<()> {
        if self.mesh.mesh_facets.is_some() {
            return Ok(());
        }
        self.check_no_facet_consumers("init_mesh_facets")?;
        if self.mesh.is_distributed() && self.mesh.communicator().size() > 1 {
            return Err(Error::Unsupported(
                "initMeshFacets inside a mutation of a mesh distributed over several ranks: \
                 prepare the facet mesh with create_distributed_mesh_facets before the mutation \
                 and install it with MeshTransaction::install_mesh_facets"
                    .into(),
            ));
        }
        let (facets, m2f) = crate::utils::create_mesh_facets(self.mesh, "mesh_facets")?;
        if let Some(m2f) = m2f {
            // as the set-up variant: an existing dataset of
            // another type is an error, not silently replaced
            crate::utils::check_mesh_to_mesh_facet_type(self.mesh)?;
            self.data_mut(MeshPart::Main)?.insert_elemental(
                crate::utils::MESH_TO_MESH_FACET,
                crate::data::MeshDataValues::Element(m2f),
            );
        }
        self.mesh.mesh_facets = Some(Box::new(facets));
        self.journal.push(Undo::FacetsCreated);
        Ok(())
    }

    /// Install a facet mesh prepared collectively before the mutation
    /// ([`crate::facet_synchronizer::create_distributed_mesh_facets`]; the
    /// distributed counterpart of [`MeshTransaction::init_mesh_facets`]).
    /// Refused (before any change) if the mesh already has a facet mesh, if
    /// the parent changed since the facet mesh was built (revision) or if
    /// the facet mesh does not share the parent's node set, or while
    /// [`MeshPart::Facets`] consumers are registered (as
    /// [`MeshTransaction::init_mesh_facets`]); rolled back with the
    /// transaction. No event.
    pub fn install_mesh_facets(
        &mut self,
        prepared: crate::facet_synchronizer::PreparedMeshFacets,
    ) -> Result<()> {
        if self.mesh.mesh_facets.is_some() {
            return Err(Error::Mesh(format!(
                "install_mesh_facets: mesh '{}' already has a facet mesh",
                self.mesh.id()
            )));
        }
        self.check_no_facet_consumers("install_mesh_facets")?;
        if prepared.parent_revision != self.mesh.revision || !self.journal.is_empty() {
            return Err(Error::Mesh(format!(
                "install_mesh_facets: the facet mesh was built from revision {} of '{}', which \
                 is now at revision {} (or already edited in this transaction)",
                prepared.parent_revision,
                self.mesh.id(),
                self.mesh.revision
            )));
        }
        let facets = prepared.mesh_facets;
        if !facets.is_mesh_facets()
            || facets.nb_nodes() != self.mesh.nb_nodes()
            || facets.spatial_dimension() != self.mesh.spatial_dimension()
        {
            return Err(Error::Mesh(
                "install_mesh_facets: not a facet mesh of this mesh (node set differs)".into(),
            ));
        }
        if let Some(m2f) = prepared.mesh_to_mesh_facet {
            crate::utils::check_mesh_to_mesh_facet_type(self.mesh)?;
            self.data_mut(MeshPart::Main)?.insert_elemental(
                crate::utils::MESH_TO_MESH_FACET,
                crate::data::MeshDataValues::Element(m2f),
            );
        }
        self.mesh.mesh_facets = Some(Box::new(facets));
        self.journal.push(Undo::FacetsCreated);
        Ok(())
    }

    fn check_no_facet_consumers(&self, what: &str) -> Result<()> {
        if self.facet_consumers {
            return Err(Error::Mesh(format!(
                "{what}: MeshPart::Facets consumers are registered on mesh '{}', which has no \
                 facet mesh yet; they cannot follow a facet mesh created during a mutation — \
                 create it before registering them",
                self.mesh.id()
            )));
        }
        Ok(())
    }

    fn save_bbox(&mut self, part: MeshPart) -> Result<()> {
        let m = part_mut(self.mesh, part)?;
        let s = &mut self.saved.bbox[slot(part)];
        if s.is_none() {
            *s = Some((m.bbox.clone(), m.bbox_local.clone()));
        }
        Ok(())
    }

    /// Record an event for dispatch (stage 5), in dispatch order.
    ///
    /// **Numbering domains.** Consumers apply the events
    /// in order, so every event is relative to the mesh state at the moment
    /// it is recorded, which is when it is validated: added nodes and
    /// elements must exist, and a removal numbering must map the entity
    /// count *before* its operation (its length) densely onto the current
    /// count. Successive removals, and removals followed by additions,
    /// therefore compose for the mesh; an invalid event is reported by the
    /// topology stage (the mutation is rolled back). Consumers read the
    /// **final** mesh at an event's indices, so when the part has
    /// consumers an erasure that renumbers a block named by an earlier
    /// event (a second erasure of the block, an addition to it) is refused
    /// in the capability stage
    /// ([`crate::events::check_element_event_composition`]): erase a block's
    /// elements in one call, before adding to it.
    ///
    /// **Node events on the facet mesh.** The facet
    /// mesh shares the parent's node set, so every node event of
    /// [`MeshPart::Main`] has a [`MeshPart::Facets`] twin with the same
    /// payload, recorded here automatically right after it (as node
    /// removals record theirs): facet consumers that track nodal state see
    /// every addition and removal in order. Node events are therefore
    /// recorded on `Main` only; recording one on `Facets` directly is an
    /// invalid event (it would duplicate the twin). `NodesRemoved` events
    /// (with their `Facets` twin) are recorded by the transaction's removal
    /// path only ([`MeshTransaction::remove_nodes`],
    /// [`MeshTransaction::purify`]); recording one directly is an invalid
    /// event (the node set was not renumbered, and before this rule the
    /// facet consumers never received it).
    ///
    /// **Node event stream.** The node events of a
    /// mutation describe every change of the shared node set, in order, so
    /// that a consumer tracking nodal state (one value per node) can follow
    /// them without reading the mesh (upstream `DOFManager::onNodesAdded`
    /// appends the listed nodes): a `NodesAdded` lists exactly the next
    /// nodes not yet announced, in increasing order (`announced …
    /// announced + k`), and each of its `old_nodes` entries is a node
    /// announced before the new node it is paired with; a `NodesRemoved`
    /// maps exactly the announced nodes; at the end of the topology stage
    /// and after the distributed hook every appended node has been
    /// announced. A violation is an invalid event (the mutation rolls
    /// back). Element events are not constrained by this rule: as upstream
    /// (`CohesiveMeshGlobalDataUpdater::updateData` sends the elements
    /// before the nodes), an `ElementsAdded` may use nodes announced by a
    /// later `NodesAdded` of the same mutation; consumers read them from
    /// the committed mesh.
    pub fn record(&mut self, part: MeshPart, event: MeshEvent) {
        let node_event = matches!(
            event,
            MeshEvent::NodesAdded { .. } | MeshEvent::NodesRemoved { .. }
        );
        let refused = if !node_event || self.recording_internal {
            None
        } else if part == MeshPart::Facets {
            Some(
                "node events are recorded on MeshPart::Main; the MeshPart::Facets twin \
                 is recorded automatically",
            )
        } else if matches!(event, MeshEvent::NodesRemoved { .. }) {
            Some(
                "NodesRemoved events are recorded by the transaction's removal path \
                 (remove_nodes, purify) together with their MeshPart::Facets twin; a node \
                 removal recorded directly describes a renumbering that did not happen",
            )
        } else {
            None
        };
        if let Some(msg) = refused {
            if self.invalid_event.is_none() {
                self.invalid_event = Some(Error::Mesh(msg.into()));
            }
            return;
        }
        if self.invalid_event.is_none()
            && let Err(e) = self.validate_event(part, &event).and_then(|()| {
                if part == MeshPart::Main && node_event {
                    self.announce_nodes(&event)
                } else {
                    Ok(())
                }
            })
        {
            self.invalid_event = Some(e);
        }
        let twin = (part == MeshPart::Main
            && self.mesh.mesh_facets.is_some()
            && matches!(event, MeshEvent::NodesAdded { .. }))
        .then(|| event.clone());
        self.events.push((part, event));
        if let Some(t) = twin {
            self.recording_internal = true;
            self.record(MeshPart::Facets, t);
            self.recording_internal = false;
        }
    }

    /// Advance the announced node count with a node event of
    /// [`MeshPart::Main`] (see [`MeshTransaction::record`], node event
    /// stream); unchanged on error.
    fn announce_nodes(&mut self, event: &MeshEvent) -> Result<()> {
        let announced = self.announced_nodes;
        match event {
            MeshEvent::NodesAdded {
                nodes, old_nodes, ..
            } => {
                for (i, &n) in nodes.iter().enumerate() {
                    let expected = announced + i;
                    if usize::try_from(n).ok() != Some(expected) {
                        return Err(Error::Mesh(format!(
                            "NodesAdded lists node {n} at position {i}, but the consumers know \
                             {announced} nodes: an addition lists exactly the next nodes not \
                             yet announced, in order ({announced}..{})",
                            announced + nodes.len()
                        )));
                    }
                    if let Some(old) = old_nodes
                        && let Some(&o) = old.get(i)
                        && !usize::try_from(o).is_ok_and(|o| o < expected)
                    {
                        return Err(Error::Mesh(format!(
                            "NodesAdded: old node {o} of new node {n} is not a node announced \
                             before it"
                        )));
                    }
                }
                self.announced_nodes = announced + nodes.len();
            }
            MeshEvent::NodesRemoved { new_numbering, .. } => {
                if new_numbering.len() != announced {
                    return Err(Error::Mesh(format!(
                        "NodesRemoved maps {} nodes, but the consumers know {announced}: nodes \
                         were appended without a NodesAdded event before the removal",
                        new_numbering.len()
                    )));
                }
                self.announced_nodes = new_numbering.iter().filter(|&&n| n >= 0).count();
            }
            _ => {}
        }
        Ok(())
    }

    /// Validate one event against the current mesh (see
    /// [`MeshTransaction::record`]).
    fn validate_event(&self, part: MeshPart, e: &MeshEvent) -> Result<()> {
        let m = part_ref(self.mesh, part)?;
        match e {
            MeshEvent::NodesAdded {
                nodes, old_nodes, ..
            } => {
                for &n in nodes {
                    check_node(m, try_to_usize(n)?)?;
                }
                if let Some(old) = old_nodes {
                    if old.len() != nodes.len() {
                        return Err(Error::DimensionMismatch {
                            context: "NodesAdded::old_nodes",
                            expected: nodes.len(),
                            got: old.len(),
                        });
                    }
                    for &n in old {
                        check_node(m, try_to_usize(n)?)?;
                    }
                }
            }
            MeshEvent::ElementsAdded { elements, .. } => {
                // validation only: borrowed, never copied
                for &el in elements {
                    m.connectivities.element_ref(el)?;
                }
            }
            MeshEvent::NodesRemoved { new_numbering, .. } => {
                validate_numbering(new_numbering, m.nb_nodes(), "NodesRemoved")?;
            }
            MeshEvent::ElementsRemoved { new_numbering, .. } => {
                for (ty, ghost, num) in new_numbering.as_map().iter_all() {
                    validate_numbering(num.as_slice(), m.nb_element(ty, ghost), "ElementsRemoved")?;
                }
            }
            _ => {}
        }
        Ok(())
    }

    /// Stage 1 validation: the events (validated when recorded) and the
    /// cross-part invariants of the mesh and its facet mesh
    /// ([`crate::utils::check_facet_mesh_consistency`]).
    fn validate_events(&mut self) -> Result<()> {
        if let Some(e) = self.invalid_event.take() {
            return Err(e);
        }
        let nb_nodes = self.mesh.nodes.len();
        if self.announced_nodes != nb_nodes {
            return Err(Error::Mesh(format!(
                "nodes {}..{nb_nodes} of mesh '{}' were appended without a NodesAdded event \
                 (the consumers know {} nodes)",
                self.announced_nodes,
                self.mesh.id(),
                self.announced_nodes
            )));
        }
        self.check_facet_consistency()
    }

    /// Stage 1 local dependents (upstream `ElementGroup::onNodesAdded` for
    /// cohesive node events): on a mesh that is **not distributed**, the
    /// element-group node lists are rebuilt from the connectivity, through
    /// the group snapshot, when a cohesive `NodesAdded` of the main mesh lists
    /// at least one node (upstream sends its `CohesiveNewNodesEvent` only for
    /// `nb_new_nodes > 0`, the local count on a serial mesh,
    /// `solid_mechanics_model_cohesive.cc:76-91`). On a distributed mesh
    /// nothing is rebuilt here: upstream decides from the global count of new
    /// owned nodes (`updateGlobalIDs`), which only the distributed hook knows,
    /// so the cohesive hook calls
    /// [`MeshTransaction::rebuild_element_group_nodes`] (UPSTREAM_DEFECTS
    /// UD-1250: rebuilding at every distributed insertion dropped pure-ghost
    /// group members too early).
    fn update_local_dependents(&mut self) -> Result<()> {
        if self.mesh.is_distributed() {
            return Ok(());
        }
        let cohesive_nodes = self.events.iter().any(|(p, e)| {
            *p == MeshPart::Main
                && matches!(
                    e,
                    MeshEvent::NodesAdded {
                        nodes,
                        old_nodes: Some(_),
                        ..
                    } if !nodes.is_empty()
                )
        });
        if cohesive_nodes {
            self.rebuild_element_group_nodes(MeshPart::Main)?;
        }
        Ok(())
    }

    /// Rebuild the node list of every element group of `part` from the
    /// connectivity (upstream `ElementGroup::onNodesAdded` for a cohesive
    /// node event, `element_group.cc:176-202`: each group's nodes become the
    /// nodes of its elements, not ghost and ghost), journaled through the
    /// group snapshot (rolled back with the transaction). For the distributed
    /// cohesive hook, which calls it when upstream would send its node event
    /// (`updateGlobalIDs(…) > 0`, UD-1250).
    ///
    /// # Errors
    /// An absent part.
    pub fn rebuild_element_group_nodes(&mut self, part: MeshPart) -> Result<()> {
        let m = part_mut(self.mesh, part)?;
        if self.saved.groups[slot(part)].is_none() {
            // only the node lists change: they are journaled instead of a
            // snapshot of every group
            let lists = m.groups.element_group_node_lists();
            self.journal.push(Undo::GroupNodes { part, lists });
        }
        m.groups.rebuild_element_group_nodes(&m.connectivities);
        Ok(())
    }

    /// Undo every journaled edit and restore every snapshot.
    ///
    /// # Panics
    /// If an exported array of the mesh is touched outside a native section
    /// of its runtime (a protocol violation, [`SharedArray::borrow_mut`]);
    /// absent blocks are skipped.
    fn rollback(self) {
        let mesh = self.mesh;
        // Strict reverse journal order, mesh-data snapshots included
        // ([`Undo::Data`]): a snapshot restored at its
        // own position supersedes every data edit made after it, and the
        // row edits made before it are undone on top of it.
        for u in self.journal.into_iter().rev() {
            match u {
                Undo::Nodes { part, len } => {
                    if let Ok(m) = part_mut(mesh, part) {
                        m.nodes.borrow_mut().truncate(len);
                        m.nodes_flags.truncate(len);
                        if let Some(g) = &mut m.nodes_global_ids {
                            g.truncate(len);
                        }
                    }
                }
                Undo::Elements {
                    part,
                    ty,
                    ghost,
                    len,
                    created,
                } => {
                    if let Ok(m) = part_mut(mesh, part) {
                        if created {
                            m.connectivities.remove(ty, ghost);
                        } else if let Some(c) = m.connectivities.as_map_mut().get_mut(ty, ghost) {
                            // absent block skipped; a protocol violation
                            // panics
                            c.borrow_mut().truncate(len);
                        }
                    }
                }
                Undo::Reconnect { part, element, old } => {
                    // absent block or row skipped; a protocol violation
                    // panics
                    if let Ok(m) = part_mut(mesh, part)
                        && let Some(c) = m
                            .connectivities
                            .as_map_mut()
                            .get_mut(element.ty(), element.ghost_type())
                        && let Some(i) = element.try_idx()
                    {
                        let mut block = c.borrow_mut();
                        if let Ok(row) = block.get_tuple_mut(i) {
                            row.copy_from_slice(&old);
                        }
                    }
                }
                Undo::Flag { part, node, old } => {
                    if let Ok(m) = part_mut(mesh, part)
                        && let Some(f) = m.nodes_flags.as_mut_slice().get_mut(node)
                    {
                        *f = old;
                    }
                }
                Undo::GlobalId { part, node, old } => {
                    if let Ok(m) = part_mut(mesh, part)
                        && let Some(g) = m
                            .nodes_global_ids
                            .as_mut()
                            .and_then(|g| g.as_mut_slice().get_mut(node))
                    {
                        *g = old;
                    }
                }
                Undo::GlobalIdsCreated { part } => {
                    if let Ok(m) = part_mut(mesh, part) {
                        m.nodes_global_ids = None;
                    }
                }
                Undo::NbGlobalNodes { part, old } => {
                    if let Ok(m) = part_mut(mesh, part) {
                        m.nb_global_nodes = old;
                    }
                }
                Undo::Prank { part, node, old } => {
                    if let Ok(m) = part_mut(mesh, part) {
                        match old {
                            Some(r) => m.nodes_prank.insert(node, r),
                            None => m.nodes_prank.remove(&node),
                        };
                    }
                }
                Undo::Core { part, core } => {
                    if let Ok(m) = part_mut(mesh, part) {
                        core.restore(m);
                    }
                }
                Undo::FacetsCreated => {
                    mesh.mesh_facets = None;
                }
                Undo::NodeSynchronizer { part, old } => {
                    if let Ok(m) = part_mut(mesh, part) {
                        m.synchronizers.node = old.map(|b| *b);
                    }
                }
                Undo::GroupNodes { part, lists } => {
                    if let Ok(m) = part_mut(mesh, part) {
                        m.groups.restore_node_lists(&lists);
                    }
                }
                Undo::Data { part, data } => {
                    if let Ok(m) = part_mut(mesh, part) {
                        m.data = *data;
                    }
                }
                Undo::DataElementRow {
                    part,
                    name,
                    element,
                    old,
                } => {
                    if let Ok(m) = part_mut(mesh, part)
                        && let Ok(map) = m.data.element_map_mut(&name)
                        && let Ok(row) = map.element_mut(element)
                    {
                        row.copy_from_slice(&old);
                    }
                }
                Undo::DataListRow {
                    part,
                    name,
                    element,
                    old,
                } => {
                    if let Ok(m) = part_mut(mesh, part)
                        && let Ok(map) = m.data.element_lists_mut(&name)
                        && let Ok(row) = map.element_mut(element)
                    {
                        row[0] = old;
                    }
                }
                Undo::DataRows {
                    part,
                    name,
                    ty,
                    ghost,
                    len,
                    created_block,
                    created_dataset,
                } => {
                    if let Ok(m) = part_mut(mesh, part) {
                        if created_dataset {
                            m.data.remove_elemental(&name);
                        } else if let Ok(values) = m.data.elemental_mut(&name) {
                            undo_data_rows(values, ty, ghost, len, created_block);
                        }
                    }
                }
            }
        }
        let Saved {
            groups,
            data: _,
            synchronizers,
            ghosts_counters,
            bbox,
        } = self.saved;
        for part in [MeshPart::Main, MeshPart::Facets] {
            let k = slot(part);
            let Ok(m) = part_mut(mesh, part) else {
                continue;
            };
            if let Some(g) = groups[k].clone() {
                m.groups = g;
            }
            if let Some(s) = synchronizers[k].clone() {
                m.synchronizers = s;
            }
            if let Some(c) = ghosts_counters[k].clone() {
                m.ghosts_counters = c;
            }
            if let Some((b, l)) = bbox[k].clone() {
                m.bbox = b;
                m.bbox_local = l;
            }
        }
    }
}

/// A removal numbering is consistent with the mesh after its operation: the
/// kept entities are numbered densely `0..current` without repetition (the
/// numbering's length is the count before the operation).
fn validate_numbering(numbering: &[Idx], current: usize, what: &str) -> Result<()> {
    let mut seen = vec![false; current];
    let mut kept = 0usize;
    for &n in numbering {
        if n < -1 {
            return Err(Error::Mesh(format!("{what}: invalid new index {n}")));
        }
        if n >= 0 {
            kept += 1;
            match seen.get_mut(try_to_usize(n)?) {
                Some(s) if !*s => *s = true,
                _ => {
                    return Err(Error::Mesh(format!(
                        "{what}: numbering is not a dense injective map onto the {current} entities"
                    )));
                }
            }
        }
    }
    if kept != current {
        return Err(Error::Mesh(format!(
            "{what}: numbering keeps {kept} entities, the mesh has {current}"
        )));
    }
    Ok(())
}

/// Undo of [`Undo::DataRows`] on one dataset.
fn undo_data_rows(
    values: &mut crate::data::MeshDataValues,
    ty: ElementType,
    ghost: GhostType,
    len: usize,
    created_block: bool,
) {
    use crate::data::MeshDataValues as V;
    match values {
        V::Element(map) => {
            if created_block {
                map.remove(ty, ghost);
            } else if let Ok(b) = map.get_mut(ty, ghost) {
                b.truncate(len);
            }
        }
        V::ElementList(map) => {
            if created_block {
                map.remove(ty, ghost);
            } else if let Ok(b) = map.get_mut(ty, ghost) {
                b.truncate(len);
            }
        }
        _ => {}
    }
}

/// Distributed dependents of a mutation (stage 2): synchronizer schemes,
/// node flags, owner ranks and global ids of new entities, supplied by the
/// component owning the insertion protocol (upstream
/// `CohesiveMeshGlobalDataUpdater`). Required when the mesh is distributed
/// and the mutation records events; must be given on **every** rank (its
/// presence is part of the agreed schedule).
///
/// Explicit prepare/commit/abort: mesh edits made in
/// `apply` go through the journaled [`MeshTransaction`] and are rolled back
/// by the domain; state the hook keeps **outside** the mesh must be staged
/// in `prepare`/`apply` and made permanent only in `commit` (or discarded
/// in `abort`).
pub trait DistributedHook {
    /// Local preparation (no communication, no mesh change): validate the
    /// recorded events, size buffers, stage external state. Agreed on all
    /// ranks before `apply`.
    fn prepare(&mut self, mesh: &Mesh, events: &[(MeshPart, MeshEvent)]) -> Result<()>;
    /// Collective update of distributed mesh state through `tx`; every
    /// communication goes through `phases` ([`Phases::run`] /
    /// [`Phases::collective`], synchronizer `*_in` methods). The outcome is
    /// agreed on all ranks afterwards.
    fn apply(
        &mut self,
        tx: &mut MeshTransaction<'_>,
        events: &[(MeshPart, MeshEvent)],
        phases: &mut Phases,
    ) -> Result<()>;
    /// Every rank committed the mutation.
    fn commit(&mut self) {}
    /// The mutation was rolled back on every rank after a successful
    /// `prepare` on this rank.
    fn abort(&mut self) {}
}

/// Result of a committed mutation.
#[derive(Clone, Debug)]
pub struct CommittedMutation {
    /// New revision.
    pub revision: u64,
    /// Events in dispatch order with their mesh part (events whose global
    /// count is zero are dropped on every rank).
    pub events: Vec<(MeshPart, MeshEvent)>,
}

pub(crate) fn sorted_levels(h: &dyn MeshEventHandler) -> Vec<EventPriority> {
    let mut v = h.event_priorities();
    v.sort();
    v.dedup();
    v
}

fn is_identity(numbering: &[Idx]) -> bool {
    numbering
        .iter()
        .enumerate()
        .all(|(i, &n)| usize::try_from(n).is_ok_and(|n| n == i))
}

/// Local measure of an event's observable change, summed over the ranks:
/// an event whose global measure is zero changes nothing anywhere and is
/// not dispatched. A non-identity numbering is observable even when it
/// removes nothing (a purify permutation).
fn event_len(e: &MeshEvent) -> usize {
    match e {
        MeshEvent::NodesAdded { nodes, .. } => nodes.len(),
        MeshEvent::NodesRemoved {
            nodes,
            new_numbering,
            ..
        } => nodes.len().max(usize::from(!is_identity(new_numbering))),
        MeshEvent::ElementsAdded { elements, .. } => elements.len(),
        MeshEvent::ElementsRemoved {
            elements,
            new_numbering,
            ..
        } => elements.len().max(usize::from(is_permuting(new_numbering))),
        // An `ElementsChanged` counts by both lists and
        // by its numbering, so that a renumbering with empty lists (or with
        // `old` empty) is dispatched and checked like `ElementsRemoved`
        MeshEvent::ElementsChanged {
            old,
            new,
            new_numbering,
            ..
        } => old
            .len()
            .max(new.len())
            .max(usize::from(is_permuting(new_numbering))),
        MeshEvent::MeshIsDistributed { .. } => 1,
    }
}

/// Whether a per-type element numbering moves any element (a non-identity
/// map of some block).
fn is_permuting(new_numbering: &ElementTypeMapArray<Idx>) -> bool {
    new_numbering
        .as_map()
        .iter_all()
        .any(|(_, _, a)| !is_identity(a.as_slice()))
}

fn event_kind(e: &MeshEvent) -> u8 {
    match e {
        MeshEvent::NodesAdded { old_nodes, .. } => 1 + u8::from(old_nodes.is_some()),
        MeshEvent::NodesRemoved { .. } => 3,
        MeshEvent::ElementsAdded { .. } => 4,
        MeshEvent::ElementsRemoved { .. } => 5,
        MeshEvent::ElementsChanged { .. } => 6,
        MeshEvent::MeshIsDistributed { .. } => 7,
    }
}

/// One dispatch step: `(handler index, level, consumer name, part)`.
pub(crate) type DispatchEntry = (usize, EventPriority, String, MeshPart);

/// Digest of the agreed schedule: event kinds and parts in order, the
/// consumer dispatch schedule (names, levels, parts), hook and facet-mesh
/// presence. Identical on every rank iff the ranks will run the same
/// collective sequence.
fn schedule_digest(
    events: &[(MeshPart, MeshEvent)],
    order: &[DispatchEntry],
    hook: bool,
    facets: bool,
) -> u64 {
    let mut h = digest_bytes(0xcbf2_9ce4_8422_2325, b"mesh-schedule");
    h = digest_bytes(h, &(events.len() as u64).to_le_bytes());
    for (part, e) in events {
        h = digest_bytes(h, &[slot(*part) as u8, event_kind(e)]);
    }
    h = digest_bytes(h, &(order.len() as u64).to_le_bytes());
    for (_, level, name, part) in order {
        h = digest_bytes(h, name.as_bytes());
        h = digest_bytes(h, &[0, level.0, slot(*part) as u8]);
    }
    digest_bytes(h, &[u8::from(hook), u8::from(facets)])
}

/// Stage 3b: every consumer of an event's part checks each event that will
/// be dispatched (non-zero global count), once per consumer.
/// Consumers are checked in **registration order** (consumer ids
/// grow with registration and are identical on every rank), independent of
/// the dispatch schedule (ascending priority, newest first among equal
/// priorities): the outcome never depended on the order, and with
/// this order neither does which refusal a rank reports when several
/// consumers refuse (brief item 1). The first refusal is
/// returned.
///
/// Before the consumers, the composition rule of element events
/// ([`crate::events::check_element_event_composition`])
/// is applied to the dispatched events of every part that has consumers:
/// consumers read the final mesh at an event's element indices, so an
/// erasure that renumbers a block named by an earlier event of the mutation
/// is refused here, recoverably, instead of failing (or corrupting a
/// consumer's state) at dispatch. On a periodic or distributed mesh the
/// node rule ([`crate::events::check_node_event_composition`])
/// refuses a node removal followed by another node event of the
/// part.
fn check_capabilities(
    mesh: &Mesh,
    events: &[(MeshPart, MeshEvent)],
    counts: &[u64],
    handlers: &[(&ConsumerToken, &mut dyn MeshEventHandler)],
) -> Result<()> {
    for part in [MeshPart::Main, MeshPart::Facets] {
        if !handlers.iter().any(|(t, _)| t.part() == part) {
            continue;
        }
        let dispatched: Vec<&MeshEvent> = events
            .iter()
            .zip(counts)
            .filter(|((p, _), c)| *p == part && **c != 0)
            .map(|((_, e), _)| e)
            .collect();
        crate::events::check_element_event_composition(&dispatched)?;
        let pairs_read = mesh.is_periodic()
            || mesh.is_distributed()
            || part_ref(mesh, part).is_ok_and(|m| m.is_periodic() || m.is_distributed());
        crate::events::check_node_event_composition(&dispatched, pairs_read)?;
    }
    let mut consumers: Vec<usize> = (0..handlers.len()).collect();
    consumers.sort_by_key(|&k| handlers[k].0.id());
    for ((part, event), &count) in events.iter().zip(counts) {
        if count == 0 {
            continue;
        }
        let m = part_ref(mesh, *part)?;
        for &k in &consumers {
            let (token, handler) = &handlers[k];
            if token.part() != *part {
                continue;
            }
            // the consumer's error, unchanged (its type is the contract)
            handler.check_mesh_event(m, event)?;
        }
    }
    Ok(())
}

impl SharedMesh {
    /// Run a staged mutation (see module docs). `handlers` must contain the
    /// handler of **every** registered consumer exactly once, paired with
    /// its token; `change` performs the topology edits and records events;
    /// `distributed` (stage 2) is required when the mesh is distributed and
    /// the change records events.
    pub fn mutate(
        &self,
        handlers: &mut [(&ConsumerToken, &mut dyn MeshEventHandler)],
        change: impl FnOnce(&mut MeshTransaction<'_>) -> Result<()>,
        distributed: Option<&mut dyn DistributedHook>,
    ) -> Result<CommittedMutation> {
        // Re-entry (a consumer or callback mutating during dispatch) or a
        // concurrent mutation from another thread: refused locally, before
        // any collective — the peers may be inside a consumer's exchange.
        // A poisoned domain never communicates again (after an agreed
        // fatal failure every rank is poisoned; after a transport failure
        // the runtime aborts).
        if let Some(e) = self.state()?.poison_error() {
            return Err(e);
        }
        self.begin_mutation()?;
        // settled on every error path, committed on success (D-126)
        let mut phases = Phases::settling(&self.0.comm, "mesh-mutation");
        let result = self.mutate_inner(&mut phases, handlers, change, distributed);
        // Owner exit, for **every** error path — preflight and
        // topology agreements included: a transport failure observed by
        // this rank, or an agreed fatal failure, leaves the domain unusable
        // and the error becomes `Error::Fatal` (the runtime's abort
        // signal). A refused scope (another collective scope active on the
        // communicator in this thread) communicated nothing: recoverable.
        let result = match result {
            Ok(c) => Ok(c),
            Err(e) => {
                // Every error of `mutate_inner` passed through an agreement
                // or a scope collective (which leave the scope failed) —
                // except a local failure between an agreement and the next
                // collective, or after the last agreement (internal
                // invariants: bounding-box parts, the state lock). Leaving
                // a still-running scope means the peers continue without
                // learning about it: the abort path.
                // Under commit points (D-126) a local failure folded into
                // the deferred outcomes reaches the peers through the scope's
                // settle: agreed (the peers learn it at their next agreement,
                // before any commit), not the abort path.
                if !phases.is_failed() && phases.has_deferred() {
                    let _ = phases.settle(Err::<(), _>(Error::Mesh(e.to_string())));
                }
                if !phases.is_failed() {
                    phases.mark_broken();
                }
                let kind = if phases.is_broken() {
                    Some(FatalKind::Transport)
                } else if phases.is_fatal() {
                    Some(FatalKind::Agreed)
                } else {
                    self.state()
                        .ok()
                        .and_then(|s| s.poisoned.as_ref().map(|(k, _)| *k))
                };
                match kind {
                    Some(k) => {
                        self.poison(k, format!("mesh mutation failed: {e}"));
                        Err(Error::fatal(k, e))
                    }
                    None => Err(e),
                }
            }
        };
        // A failure to reset the phase never replaces the classified error.
        let reset = self.state().map(|mut s| s.phase = Phase::Idle);
        result.and_then(|c| reset.map(|()| c))
    }

    fn begin_mutation(&self) -> Result<()> {
        let mut s = self.state()?;
        if s.phase != Phase::Idle {
            return Err(Error::Mesh(
                "re-entrant mesh mutation (a mutation is already in progress on this rank)".into(),
            ));
        }
        s.phase = Phase::Mutating;
        Ok(())
    }

    /// Record the poisoning; the first reason is kept, the kind only ever
    /// becomes more urgent (`Transport` over `Agreed`).
    fn poison(&self, kind: FatalKind, reason: String) {
        if let Ok(mut s) = self.state() {
            match &mut s.poisoned {
                None => s.poisoned = Some((kind, reason)),
                Some((k, _)) => *k = k.most_urgent(kind),
            }
        }
    }

    #[allow(clippy::too_many_lines)] // the stages read best as one sequence
    fn mutate_inner(
        &self,
        ph: &mut Phases,
        handlers: &mut [(&ConsumerToken, &mut dyn MeshEventHandler)],
        change: impl FnOnce(&mut MeshTransaction<'_>) -> Result<()>,
        mut distributed: Option<&mut dyn DistributedHook>,
    ) -> Result<CommittedMutation> {
        // ---- stage 0: preflight (local), agreed ---------------------------
        let pre = self
            .preflight(handlers)
            .and_then(|order| Ok((order, self.try_write()?)));
        // D-126: the stages before the commit change nothing irreversible
        // (every rank rolls back on any failure): their agreements are
        // deferred under commit points and agreed at the next communication
        // or at the commit point (stage 3b)
        let (order, mut guard) = match pre {
            Ok(v) => {
                ph.defer("preflight", Ok(()), Severity::Recoverable)?;
                v
            }
            Err(e) => {
                return Err(own_error(ph.defer(
                    "preflight",
                    Err(e),
                    Severity::Recoverable,
                )));
            }
        };
        let facet_consumers = order.iter().any(|(_, _, _, p)| *p == MeshPart::Facets);
        let restricted = {
            let s = self.state()?;
            !s.full_consistency_checks && s.facets_verified == Some(s.committed)
        };
        let mut tx = MeshTransaction::new(&mut guard, facet_consumers, restricted);

        // ---- stage 1: topology + local dependents + schedule, agreed -------
        let hook_present = distributed.is_some();
        let distributed_mesh = tx.mesh.is_distributed();
        let topo = change(&mut tx)
            .and_then(|()| tx.validate_events())
            .and_then(|()| tx.update_local_dependents())
            .and_then(|()| {
                if distributed_mesh && !hook_present && !tx.events.is_empty() {
                    Err(Error::Mesh(
                        "distributed mesh mutation without synchronizer/global-id update".into(),
                    ))
                } else {
                    Ok(())
                }
            });
        let digest = schedule_digest(
            &tx.events,
            &order,
            hook_present && distributed_mesh,
            tx.mesh.mesh_facets.is_some(),
        );
        if let Err(e) = ph.defer_digest("topology", topo, digest, Severity::Recoverable) {
            tx.rollback();
            return Err(e);
        }

        // ---- stages 2-3: distributed hook, bounding boxes, counts ----------
        let mut prepared = false;
        let staged = (|| -> Result<Vec<u64>> {
            if distributed_mesh && let Some(h) = distributed.as_deref_mut() {
                let events = std::mem::take(&mut tx.events);
                let prep = h.prepare(tx.mesh, &events);
                prepared = prep.is_ok();
                let applied = ph
                    .defer("hook:prepare", prep, Severity::Recoverable)
                    .map(|()| h.apply(&mut tx, &events, ph));
                // Events the hook records during `apply` (relative to the
                // state after the change, validated when recorded) follow
                // the change's events instead of being dropped;
                // their schedule is agreed below.
                let hook_events = std::mem::replace(&mut tx.events, events);
                let nb_hook_events = hook_events.len();
                tx.events.extend(hook_events);
                // the hook's edits keep the shared node set consistent
                let applied = applied?.and_then(|()| tx.validate_events());
                // Agreed — or, when one of the hook's internal phases
                // already failed, classified by the scope **without
                // communicating**: a transport-kind result,
                // or an agreed-kind one after a recoverable failure (no
                // longer agreeable), breaks the scope (abort path).
                ph.defer("hook:apply", applied, Severity::Recoverable)?;
                // The global counts below reduce one entry per event: the
                // extended schedule must agree on every rank.
                let digest = schedule_digest(
                    &tx.events[tx.events.len() - nb_hook_events..],
                    &[],
                    true,
                    tx.mesh.mesh_facets.is_some(),
                );
                ph.defer_digest("hook:schedule", Ok(()), digest, Severity::Recoverable)?;
            }
            // Bounding boxes after the hook, whenever nodes or elements were
            // added or removed (upstream recomputes them in the node events
            // and in the element-removal event).
            let nodes_changed = tx.events.iter().any(|(_, e)| {
                matches!(
                    e,
                    MeshEvent::NodesAdded { .. }
                        | MeshEvent::NodesRemoved { .. }
                        | MeshEvent::ElementsRemoved { .. }
                )
            });
            if nodes_changed {
                let parts: Vec<MeshPart> = if tx.mesh.mesh_facets.is_some() {
                    vec![MeshPart::Main, MeshPart::Facets]
                } else {
                    vec![MeshPart::Main]
                };
                let mut locals = Vec::with_capacity(parts.len());
                let (mut lower, mut upper) = (Vec::new(), Vec::new());
                for &p in &parts {
                    tx.save_bbox(p)?;
                    let b = part_ref(tx.mesh, p)?.local_bounding_box();
                    lower.extend_from_slice(&b.lower);
                    upper.extend_from_slice(&b.upper);
                    locals.push(b);
                }
                // a checked reduction: carries what is deferred (D-126)
                ph.run_collective("bbox", Ok(()), |c, ()| {
                    crate::mesh::all_sum_bounds(c, &mut lower, &mut upper)
                })?;
                let mut off = 0;
                for (&p, local) in parts.iter().zip(locals) {
                    let m = part_mut(tx.mesh, p)?;
                    let d = local.lower.len();
                    m.bbox = BBox {
                        lower: lower[off..off + d].to_vec(),
                        upper: upper[off..off + d].to_vec(),
                    };
                    m.bbox_local = local;
                    off += d;
                }
            }
            // Global event counts (lengths matched by the digest agreement).
            let mut counts: Vec<u64> = tx
                .events
                .iter()
                .map(|(_, e)| u64::try_from(event_len(e)).unwrap_or(u64::MAX))
                .collect();
            ph.run_collective("counts", Ok(()), |c, ()| {
                c.all_reduce(&mut counts, akantu_comm::ReduceOp::Sum)
            })?;
            // ---- stage 3b: consumer capability check, agreed -------------
            // the commit point (D-126): every outcome before the commit is
            // agreed here, immediately, in every mode
            let check = check_capabilities(tx.mesh, &tx.events, &counts, handlers);
            ph.commit("capabilities", check, Severity::Recoverable)?;
            Ok(counts)
        })();
        let counts = match staged {
            Ok(c) => c,
            Err(e) => {
                tx.rollback();
                if prepared && let Some(h) = distributed.as_deref_mut() {
                    h.abort();
                }
                if ph.is_broken() {
                    self.poison(
                        FatalKind::Transport,
                        format!(
                            "transport failure during a mesh mutation (rolled back locally): {e}"
                        ),
                    );
                }
                return Err(e);
            }
        };

        // ---- stage 4: commit ------------------------------------------------
        let events: Vec<(MeshPart, MeshEvent)> = std::mem::take(&mut tx.events)
            .into_iter()
            .zip(counts)
            .filter(|(_, c)| *c > 0)
            .map(|(e, _)| e)
            .collect();
        drop(tx); // journal and snapshots discarded
        guard.revision += 1;
        let revision = {
            let mut s = self.state()?;
            s.committed += 1;
            s.phase = Phase::Dispatching;
            // the committed state passed the cross-part check (stage 1 and,
            // with a hook, after it)
            s.facets_verified = Some(s.committed);
            s.committed
        };
        if let Some(h) = distributed {
            h.commit();
        }

        // ---- stage 5: dispatch ----------------------------------------------
        let mesh: &Mesh = &guard;
        let mut failure: Option<Error> = None;
        'events: for (ei, (part, event)) in events.iter().enumerate() {
            for (k, level, name, cpart) in &order {
                if cpart != part {
                    continue;
                }
                let local = part_ref(mesh, *part).and_then(|m| {
                    handlers[*k]
                        .1
                        .on_mesh_event_phased(m, event, *level, &mut *ph)
                });
                // Agreed — or, when one of the consumer's internal phases
                // already failed, classified by the scope without
                // communicating (as for the hook): the
                // consumer's result decides the observing rank's fatal
                // kind, not only the error that failed the internal phase.
                // after the commit a failure poisons the domain on every
                // rank (fatal); under commit points the consumer's outcome
                // joins the deferred ones, agreed at the next communication
                // or at the end of the dispatch (D-126)
                if let Err(e) = ph.defer(
                    &format!("dispatch:{ei}:{name}:{}", level.0),
                    local,
                    Severity::Fatal,
                ) {
                    failure = Some(e);
                    break 'events;
                }
            }
        }
        // the end of the dispatch: a commit point (what is deferred is
        // agreed, as fatal on every rank if anything failed anywhere)
        if ph.has_deferred() {
            let end = match failure.take() {
                None => Ok(()),
                Some(e) => Err(e),
            };
            if let Err(e) = ph.commit("dispatch:end", end, Severity::Fatal) {
                failure = Some(e);
            }
        }
        drop(guard);
        if let Some(e) = failure {
            // After commit: agreed on every rank (dispatch agreements),
            // unless this rank observed a transport failure.
            let kind = if ph.is_broken() {
                FatalKind::Transport
            } else {
                FatalKind::Agreed
            };
            self.poison(
                kind,
                format!("consumer update failed after commit of revision {revision}: {e}"),
            );
            return Err(Error::fatal(kind, e));
        }

        // ---- stage 6: acknowledgements ----------------------------------------
        let mut s = self.state()?;
        for (t, _) in handlers.iter() {
            if let Some(c) = s.consumers.get_mut(&t.id()) {
                c.acked_revision = revision;
            }
        }
        Ok(CommittedMutation { revision, events })
    }

    /// Local preflight: poisoning, consumer coverage, acknowledgements; the
    /// dispatch schedule in ascending level order, the newest consumer
    /// first among equal levels (upstream `std::lower_bound` insertion).
    fn preflight(
        &self,
        handlers: &[(&ConsumerToken, &mut dyn MeshEventHandler)],
    ) -> Result<Vec<DispatchEntry>> {
        let s = self.state()?;
        if let Some(e) = s.poison_error() {
            return Err(e);
        }
        let mut given: Vec<_> = handlers.iter().map(|(t, _)| t.id()).collect();
        given.sort();
        if given.windows(2).any(|w| w[0] == w[1]) {
            return Err(Error::Mesh("a mesh consumer was given twice".into()));
        }
        let mut order = Vec::new();
        for (k, (t, h)) in handlers.iter().enumerate() {
            if !t.belongs_to(self) {
                return Err(Error::Mesh("consumer token of another mesh".into()));
            }
            match s.consumers.get(&t.id()) {
                Some(c) if c.priorities != sorted_levels(&**h) || c.part != t.part() => {
                    return Err(Error::Mesh(format!(
                        "consumer '{}' changed its priority or part",
                        c.name
                    )));
                }
                Some(c) => {
                    for &p in &c.priorities {
                        order.push((p, t.id(), k, c.name.clone(), c.part));
                    }
                }
                None => return Err(Error::Mesh("unregistered mesh consumer".into())),
            }
        }
        let missing: Vec<&str> = s
            .consumers
            .iter()
            .filter(|(id, _)| given.binary_search(id).is_err())
            .map(|(_, c)| c.name.as_str())
            .collect();
        if !missing.is_empty() {
            return Err(Error::Mesh(format!(
                "mesh mutation without registered consumers: {}",
                missing.join(", ")
            )));
        }
        let committed = s.committed;
        if let Some(c) = s.consumers.values().find(|c| c.acked_revision != committed) {
            return Err(Error::Mesh(format!(
                "consumer '{}' missed a mesh revision",
                c.name
            )));
        }
        // ascending level; at equal level the most recently registered
        // consumer first (upstream `registerEventHandler` inserts with
        // `std::lower_bound`, `aka_event_handler_manager.hh:70-73`,
        // CORE-036). Consumer ids grow with registration and
        // are identical on every rank, so the schedule is agreed as before.
        order.sort_by_key(|o| (o.0, std::cmp::Reverse(o.1)));
        Ok(order
            .into_iter()
            .map(|(p, _, k, n, part)| (k, p, n, part))
            .collect())
    }

    /// Run a legacy event-producing operation (distribution, facet
    /// creation, purification) as a mutation: `op` edits the mesh directly
    /// and returns its events (no consumer can receive them). Allowed only
    /// while the mesh has **no** consumers (set-up phase) and no mutation is
    /// in progress. The mesh revision advances exactly once.
    pub fn setup(&self, op: impl FnOnce(&mut Mesh) -> Result<MeshEvents>) -> Result<u64> {
        {
            let s = self.state()?;
            if !s.consumers.is_empty() {
                return Err(Error::Mesh(
                    "set-up operations require a mesh without consumers; use mutate()".into(),
                ));
            }
            if s.phase != Phase::Idle {
                return Err(Error::Mesh(
                    "set-up operation during a mesh mutation".into(),
                ));
            }
        }
        // A set-up operation is not transactional: one that fails part-way
        // keeps its edits. The mesh counts as checked again only when the
        // operation and the check below succeed; until then every mutation
        // takes the full cross-part check.
        self.state()?.facets_verified = None;
        let mut g = self.try_write()?;
        let before = g.revision;
        let _events = op(&mut g)?;
        // The set-up operation must leave the facet mesh sharing the
        // parent's node numbering: rejected before the revision step (the
        // edit itself cannot be undone; the flag cleared above makes every
        // later mutation run the full check, which reports the same
        // inconsistency at stage 1).
        crate::utils::check_facet_mesh_consistency(&g)?;
        // exactly one revision per set-up operation, whether or not `op`
        // advanced it itself
        g.revision = before + 1;
        drop(g);
        let mut s = self.state()?;
        s.committed += 1;
        s.facets_verified = Some(s.committed);
        Ok(s.committed)
    }
}

/// The error of a failed agreement on a local error (the agreement returns
/// the rank's own error).
fn own_error(r: Result<()>) -> Error {
    r.err()
        .unwrap_or_else(|| Error::Mesh("unreachable: failed agreement returned Ok".into()))
}

/// The journal entries stay small (a whole-mesh insertion journals
/// millions of them; a large inline variant made it slower).
#[cfg(test)]
const _: () = assert!(std::mem::size_of::<Undo>() <= 64);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{MeshAccessor, MeshEventHandler};
    use akantu_comm::Communicator;
    use akantu_core::enums::EventPriority;

    /// Consumer that poisons the domain-state lock while reacting (a
    /// panicking thread holding it), so that the local acknowledgement
    /// after the last dispatch agreement fails.
    struct PoisonsState(SharedMesh);

    impl MeshEventHandler for PoisonsState {
        fn check_mesh_event(&self, _: &Mesh, _: &MeshEvent) -> Result<()> {
            // test consumer: accepts every event (no capability limits)
            Ok(())
        }
        fn event_priority(&self) -> EventPriority {
            EventPriority(50)
        }
        fn on_mesh_event(&mut self, _: &Mesh, _: &MeshEvent) -> Result<()> {
            let d = &self.0.0;
            let _ = std::thread::scope(|s| {
                s.spawn(|| {
                    let _g = d.state.lock();
                    panic!("injected panic while holding the mesh state lock");
                })
                .join()
            });
            Ok(())
        }
    }

    /// Consumer at two levels that refuses node removals in the capability
    /// check and records every check and dispatch.
    #[derive(Default)]
    struct Refuses {
        checks: std::cell::RefCell<Vec<&'static str>>,
        dispatched: Vec<&'static str>,
    }

    fn kind(e: &MeshEvent) -> &'static str {
        match e {
            MeshEvent::NodesAdded { .. } => "nodes added",
            MeshEvent::NodesRemoved { .. } => "nodes removed",
            MeshEvent::ElementsAdded { .. } => "elements added",
            MeshEvent::ElementsRemoved { .. } => "elements removed",
            _ => "other",
        }
    }

    impl MeshEventHandler for Refuses {
        fn event_priority(&self) -> EventPriority {
            EventPriority(94)
        }
        fn event_priorities(&self) -> Vec<EventPriority> {
            vec![EventPriority(20), EventPriority(94)]
        }
        fn on_mesh_event(&mut self, _: &Mesh, e: &MeshEvent) -> Result<()> {
            self.dispatched.push(kind(e));
            Ok(())
        }
        fn check_mesh_event(&self, _: &Mesh, e: &MeshEvent) -> Result<()> {
            self.checks.borrow_mut().push(kind(e));
            match e {
                MeshEvent::NodesRemoved { .. } => {
                    Err(Error::NotImplemented("node removal (test consumer)"))
                }
                _ => Ok(()),
            }
        }
    }

    /// Default check (accepts everything); records dispatches.
    #[derive(Default)]
    struct Accepts(Vec<&'static str>);

    impl MeshEventHandler for Accepts {
        fn check_mesh_event(&self, _: &Mesh, _: &MeshEvent) -> Result<()> {
            // test consumer: accepts every event (no capability limits)
            Ok(())
        }
        fn event_priority(&self) -> EventPriority {
            EventPriority(50)
        }
        fn on_mesh_event(&mut self, _: &Mesh, e: &MeshEvent) -> Result<()> {
            self.0.push(kind(e));
            Ok(())
        }
    }

    /// Stage 3b: an event a consumer cannot
    /// handle is refused before the commit — recoverable, the mesh rolled
    /// back, nothing dispatched, the domain usable — instead of poisoning
    /// the domain at dispatch. A multi-level consumer is checked once per
    /// event; events that are not dispatched (zero global count: an
    /// identity purify) are not checked, so they still commit.
    #[test]
    fn capability_refusal_rolls_back_before_commit() {
        let comm = Communicator::serial();
        let mut m = Mesh::with_communicator(1, "bar", comm.clone()).unwrap();
        {
            let mut a = MeshAccessor::new(&mut m);
            for x in [0.0, 1.0, 2.0] {
                a.push_node(&[x]).unwrap();
            }
            let mut c = a.connectivity_mut(ElementType::Segment2, GhostType::NotGhost);
            c.push_tuple(&[0, 1]);
            c.push_tuple(&[1, 2]);
        }
        let mesh = SharedMesh::new(m);
        let ta = mesh
            .register_multilevel_consumer(
                "refuses",
                &[EventPriority(20), EventPriority(94)],
                MeshPart::Main,
            )
            .unwrap();
        let tb = mesh
            .register_consumer("accepts", EventPriority(50), MeshPart::Main)
            .unwrap();
        let (mut a, mut b) = (Refuses::default(), Accepts::default());
        let revision = mesh.revision().unwrap();
        let nodes_before = mesh.read().unwrap().nodes().as_slice().to_vec();

        // 1. node append plus removal of the appended node: refused
        let r = mesh.mutate(
            &mut [(&ta, &mut a), (&tb, &mut b)],
            |tx| {
                let n = tx.push_node(MeshPart::Main, &[3.0])?;
                tx.record(
                    MeshPart::Main,
                    MeshEvent::NodesAdded {
                        nodes: vec![n],
                        old_nodes: None,
                        origin: "test".into(),
                        extension: None,
                    },
                );
                tx.remove_nodes(MeshPart::Main, &[0, 1, 2, -1])
            },
            None,
        );
        assert!(matches!(r, Err(Error::NotImplemented(_))), "{r:?}");
        assert!(r.as_ref().unwrap_err().fatal_kind().is_none());
        // checked once each (two levels, one check), in order; nothing
        // dispatched; mesh rolled back; revision unchanged; not poisoned
        assert_eq!(*a.checks.borrow(), ["nodes added", "nodes removed"]);
        assert!(a.dispatched.is_empty() && b.0.is_empty());
        assert_eq!(mesh.revision().unwrap(), revision);
        assert_eq!(mesh.read().unwrap().nodes().as_slice(), &nodes_before[..]);
        assert!(comm.broken().is_none());

        // 2. the domain stays usable: an accepted event commits and is
        // dispatched to both consumers (at both levels of the first)
        a.checks.borrow_mut().clear();
        mesh.mutate(
            &mut [(&ta, &mut a), (&tb, &mut b)],
            |tx| {
                let n = tx.push_node(MeshPart::Main, &[3.0])?;
                tx.record(
                    MeshPart::Main,
                    MeshEvent::NodesAdded {
                        nodes: vec![n],
                        old_nodes: None,
                        origin: "test".into(),
                        extension: None,
                    },
                );
                Ok(())
            },
            None,
        )
        .unwrap();
        assert_eq!(*a.checks.borrow(), ["nodes added"]);
        assert_eq!(a.dispatched, ["nodes added", "nodes added"]);
        assert_eq!(b.0, ["nodes added"]);
        assert_eq!(mesh.read().unwrap().nb_nodes(), 4);

        // 3. purify removes the unused node 3: dispatched, hence checked
        // and refused (rolled back)
        a.checks.borrow_mut().clear();
        let r = mesh.mutate(
            &mut [(&ta, &mut a), (&tb, &mut b)],
            |tx| tx.purify(MeshPart::Main),
            None,
        );
        assert!(matches!(r, Err(Error::NotImplemented(_))), "{r:?}");
        assert_eq!(*a.checks.borrow(), ["nodes removed"]);
        assert_eq!(mesh.read().unwrap().nb_nodes(), 4);

        // 4. an element on node 3 makes purify the identity: its event is
        // not dispatched, hence not checked, and the mutation commits
        a.checks.borrow_mut().clear();
        let before = mesh.revision().unwrap();
        mesh.mutate(
            &mut [(&ta, &mut a), (&tb, &mut b)],
            |tx| {
                tx.push_element(
                    MeshPart::Main,
                    ElementType::Segment2,
                    GhostType::NotGhost,
                    &[2, 3],
                )?;
                tx.purify(MeshPart::Main)
            },
            None,
        )
        .unwrap();
        assert!(
            !a.checks.borrow().contains(&"nodes removed"),
            "{:?}",
            a.checks.borrow()
        );
        assert_eq!(mesh.revision().unwrap(), before + 1);
        assert_eq!(mesh.read().unwrap().nb_nodes(), 4);
    }

    /// Logs `(name, level)` of every dispatch.
    struct Logs {
        name: &'static str,
        levels: Vec<EventPriority>,
        log: std::sync::Arc<std::sync::Mutex<Vec<(&'static str, u8)>>>,
    }

    impl MeshEventHandler for Logs {
        fn check_mesh_event(&self, _: &Mesh, _: &MeshEvent) -> Result<()> {
            // test consumer: accepts every event (no capability limits)
            Ok(())
        }
        fn event_priority(&self) -> EventPriority {
            self.levels[0]
        }
        fn event_priorities(&self) -> Vec<EventPriority> {
            self.levels.clone()
        }
        fn on_mesh_event(&mut self, _: &Mesh, _: &MeshEvent) -> Result<()> {
            Ok(())
        }
        fn on_mesh_event_at(
            &mut self,
            _: &Mesh,
            _: &MeshEvent,
            level: EventPriority,
        ) -> Result<()> {
            self.log.lock().unwrap().push((self.name, level.0));
            Ok(())
        }
    }

    /// CORE-036: upstream's `registerEventHandler`
    /// inserts with `std::lower_bound` on the priority
    /// (`aka_event_handler_manager.hh:70-73`), so among handlers of equal
    /// priority the most recently registered runs **first**; lower
    /// priority values run before higher ones. A multi-level consumer takes
    /// one place per level, as the separate upstream handlers it stands for.
    #[test]
    fn equal_priorities_dispatch_the_newest_consumer_first() {
        let mut m = Mesh::new(1, "bar").unwrap();
        {
            let mut a = MeshAccessor::new(&mut m);
            a.push_node(&[0.0]).unwrap();
        }
        let mesh = SharedMesh::new(m);
        let log = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let mk = |name, levels: &[u8]| Logs {
            name,
            levels: levels.iter().map(|&l| EventPriority(l)).collect(),
            log: log.clone(),
        };
        let mut a = mk("a", &[50]);
        let mut b = mk("b", &[50]);
        let mut c = mk("c", &[40]);
        let mut d = mk("d", &[40, 50]);
        let reg = |h: &Logs| {
            mesh.register_multilevel_consumer(h.name, &h.levels, MeshPart::Main)
                .unwrap()
        };
        let (ta, tb, tc, td) = (reg(&a), reg(&b), reg(&c), reg(&d));
        mesh.mutate(
            // the order of the handler list does not matter
            &mut [(&tc, &mut c), (&ta, &mut a), (&td, &mut d), (&tb, &mut b)],
            |tx| {
                let n = tx.push_node(MeshPart::Main, &[1.0])?;
                tx.record(
                    MeshPart::Main,
                    MeshEvent::NodesAdded {
                        nodes: vec![n],
                        old_nodes: None,
                        origin: "test".into(),
                        extension: None,
                    },
                );
                Ok(())
            },
            None,
        )
        .unwrap();
        assert_eq!(
            *log.lock().unwrap(),
            vec![("d", 40), ("c", 40), ("d", 50), ("b", 50), ("a", 50)]
        );
    }

    /// Refuses node removals with its own name (capability check only).
    struct RefusesNamed(&'static str, EventPriority);

    impl MeshEventHandler for RefusesNamed {
        fn event_priority(&self) -> EventPriority {
            self.1
        }
        fn on_mesh_event(&mut self, _: &Mesh, _: &MeshEvent) -> Result<()> {
            Ok(())
        }
        fn check_mesh_event(&self, _: &Mesh, e: &MeshEvent) -> Result<()> {
            match e {
                MeshEvent::NodesRemoved { .. } => Err(Error::Mesh(self.0.into())),
                _ => Ok(()),
            }
        }
    }

    /// The capability check does not depend on the
    /// dispatch order. Consumers are checked in **registration order**
    /// (oldest first, identical on every rank), not in the dispatch schedule
    /// (ascending priority, newest first among equal priorities), so
    /// when several consumers refuse, the refusal reported is the oldest
    /// refusing consumer's whatever the priorities and the order of the
    /// handler list.
    #[test]
    fn capability_check_is_independent_of_the_dispatch_order() {
        for (p_old, p_new) in [(50, 50), (60, 40), (40, 60)] {
            for swap_list in [false, true] {
                let comm = Communicator::serial();
                let mut m = Mesh::with_communicator(1, "bar", comm.clone()).unwrap();
                {
                    let mut a = MeshAccessor::new(&mut m);
                    a.push_node(&[0.0]).unwrap();
                }
                let mesh = SharedMesh::new(m);
                let mut old = RefusesNamed("old", EventPriority(p_old));
                let mut new = RefusesNamed("new", EventPriority(p_new));
                let t_old = mesh
                    .register_consumer("old", old.1, MeshPart::Main)
                    .unwrap();
                let t_new = mesh
                    .register_consumer("new", new.1, MeshPart::Main)
                    .unwrap();
                let change = |tx: &mut MeshTransaction<'_>| {
                    let n = tx.push_node(MeshPart::Main, &[1.0])?;
                    tx.record(
                        MeshPart::Main,
                        MeshEvent::NodesAdded {
                            nodes: vec![n],
                            old_nodes: None,
                            origin: "test".into(),
                            extension: None,
                        },
                    );
                    tx.remove_nodes(MeshPart::Main, &[0, -1])
                };
                let r = if swap_list {
                    mesh.mutate(&mut [(&t_new, &mut new), (&t_old, &mut old)], change, None)
                } else {
                    mesh.mutate(&mut [(&t_old, &mut old), (&t_new, &mut new)], change, None)
                };
                let msg = match &r {
                    Err(Error::Mesh(m)) => m.clone(),
                    other => panic!("expected a refusal, got {other:?}"),
                };
                assert_eq!(
                    msg, "old",
                    "priorities ({p_old}, {p_new}), swapped {swap_list}"
                );
                assert_eq!(mesh.read().unwrap().nb_nodes(), 1);
            }
        }
    }

    /// An error leaving the mutation while its scope is still
    /// running (here: the acknowledgement after every dispatch agreement
    /// succeeded) was never agreed — the peers return success — so it takes
    /// the abort path: `Transport`, communicator broken. Previously the lock
    /// error was returned as an ordinary, recoverable error.
    #[test]
    fn unagreed_failure_after_the_last_agreement_takes_the_abort_path() {
        let comm = Communicator::serial();
        let mut m = Mesh::with_communicator(1, "bar", comm.clone()).unwrap();
        {
            let mut a = MeshAccessor::new(&mut m);
            a.push_node(&[0.0]).unwrap();
        }
        let mesh = SharedMesh::new(m);
        let token = mesh
            .register_consumer("model", EventPriority(50), MeshPart::Main)
            .unwrap();
        let mut consumer = PoisonsState(mesh.clone());
        let e = mesh
            .mutate(
                &mut [(&token, &mut consumer)],
                |tx| {
                    let n = tx.push_node(MeshPart::Main, &[1.0])?;
                    tx.record(
                        MeshPart::Main,
                        MeshEvent::NodesAdded {
                            nodes: vec![n],
                            old_nodes: None,
                            origin: "test".into(),
                            extension: None,
                        },
                    );
                    Ok(())
                },
                None,
            )
            .unwrap_err();
        assert_eq!(e.fatal_kind(), Some(FatalKind::Transport), "{e}");
        assert!(comm.broken().is_some());
    }

    /// Element events composed in one mutation are
    /// checked against the composition rule in the capability stage when
    /// the part has consumers. A second erasure of a block, and an addition
    /// to a block followed by an erasure of it, are refused before the
    /// commit (recoverable, rolled back, nothing dispatched, the domain
    /// usable); one call erasing both elements, an erasure followed by an
    /// addition, and erasures of two different blocks commit. Without
    /// consumers the same sequences commit.
    #[test]
    fn element_event_composition_is_refused_before_commit() {
        let comm = Communicator::serial();
        let bar = || {
            let mut m = Mesh::with_communicator(1, "bar", comm.clone()).unwrap();
            {
                let mut a = MeshAccessor::new(&mut m);
                for x in [0.0, 1.0, 2.0, 3.0, 4.0] {
                    a.push_node(&[x]).unwrap();
                }
                let mut c = a.connectivity_mut(ElementType::Segment2, GhostType::NotGhost);
                for i in 0..4 {
                    c.push_tuple(&[i, i + 1]);
                }
                drop(c);
                let mut p = a.connectivity_mut(ElementType::Point1, GhostType::NotGhost);
                p.push_tuple(&[0]);
                p.push_tuple(&[4]);
            }
            m
        };
        let m = bar();
        let seg = |i: Idx| Element::new(ElementType::Segment2, i, GhostType::NotGhost);
        let pt = |i: Idx| Element::new(ElementType::Point1, i, GhostType::NotGhost);
        let mesh = SharedMesh::new(m);
        let token = mesh
            .register_consumer("accepts", EventPriority(50), MeshPart::Main)
            .unwrap();
        let mut b = Accepts::default();
        let add_then_erase = |tx: &mut MeshTransaction<'_>| -> Result<()> {
            let e = tx.push_element(
                MeshPart::Main,
                ElementType::Segment2,
                GhostType::NotGhost,
                &[0, 2],
            )?;
            tx.record(
                MeshPart::Main,
                MeshEvent::ElementsAdded {
                    elements: vec![e],
                    constitutive_law: None,
                    origin: "test".into(),
                    extension: None,
                },
            );
            tx.erase_elements(MeshPart::Main, &[seg(1)])
        };
        let refused = |r: Result<CommittedMutation>, b: &Accepts| {
            let e = r.expect_err("refused before the commit");
            assert!(matches!(e, Error::Unsupported(_)), "{e:?}");
            assert!(e.fatal_kind().is_none(), "recoverable: {e:?}");
            assert!(format!("{e}").contains("of the same mutation names"), "{e}");
            assert!(b.0.is_empty(), "nothing dispatched");
        };
        let revision = mesh.revision().unwrap();
        // 1. two erasures of the Segment2 block
        let r = mesh.mutate(
            &mut [(&token, &mut b)],
            |tx| {
                tx.erase_elements(MeshPart::Main, &[seg(3)])?;
                tx.erase_elements(MeshPart::Main, &[seg(0)])
            },
            None,
        );
        refused(r, &b);
        // 2. an element added, then an older one erased (the hole filled
        // with the added one)
        let r = mesh.mutate(&mut [(&token, &mut b)], add_then_erase, None);
        refused(r, &b);
        assert_eq!(mesh.revision().unwrap(), revision);
        assert_eq!(
            mesh.read()
                .unwrap()
                .nb_element(ElementType::Segment2, GhostType::NotGhost),
            4
        );
        assert!(comm.broken().is_none());
        // 3. erasures of two different blocks in two calls, then an
        // addition to an erased block: commits
        mesh.mutate(
            &mut [(&token, &mut b)],
            |tx| {
                tx.erase_elements(MeshPart::Main, &[seg(0)])?;
                tx.erase_elements(MeshPart::Main, &[pt(1)])?;
                let e = tx.push_element(
                    MeshPart::Main,
                    ElementType::Segment2,
                    GhostType::NotGhost,
                    &[0, 1],
                )?;
                tx.record(
                    MeshPart::Main,
                    MeshEvent::ElementsAdded {
                        elements: vec![e],
                        constitutive_law: None,
                        origin: "test".into(),
                        extension: None,
                    },
                );
                Ok(())
            },
            None,
        )
        .unwrap();
        assert_eq!(
            b.0,
            ["elements removed", "elements removed", "elements added"]
        );
        // 4. one call with both elements commits
        mesh.mutate(
            &mut [(&token, &mut b)],
            |tx| tx.erase_elements(MeshPart::Main, &[seg(3), seg(0)]),
            None,
        )
        .unwrap();
        assert_eq!(
            mesh.read()
                .unwrap()
                .nb_element(ElementType::Segment2, GhostType::NotGhost),
            2
        );
        // 5. without consumers the refused sequences compose
        let free = SharedMesh::new(bar());
        free.mutate(&mut [], add_then_erase, None).unwrap();
        free.mutate(
            &mut [],
            |tx| {
                tx.erase_elements(MeshPart::Main, &[seg(0)])?;
                tx.erase_elements(MeshPart::Main, &[seg(0)])
            },
            None,
        )
        .unwrap();
    }

    /// The composition rule on event lists: what
    /// renumbers a block named earlier is refused; identity numberings,
    /// other blocks and additions after an erasure are not.
    #[test]
    fn element_event_composition_rule() {
        use crate::events::check_element_event_composition as rule;
        let ty = ElementType::Triangle3;
        let el = |i: Idx, g: GhostType| Element::new(ty, i, g);
        let removed = |g: GhostType, num: Vec<Idx>| {
            let mut n = ElementTypeMapArray::new("n");
            n.as_map_mut()
                .insert(ty, g, Array::from_vec(num, 1).unwrap())
                .unwrap();
            MeshEvent::ElementsRemoved {
                elements: Vec::new(),
                new_numbering: n,
                origin: "t".into(),
            }
        };
        let added = |els: Vec<Element>| MeshEvent::ElementsAdded {
            elements: els,
            constitutive_law: None,
            origin: "t".into(),
            extension: None,
        };
        let ng = GhostType::NotGhost;
        let g = GhostType::Ghost;
        let erase0 = removed(ng, vec![-1, 0, 1]);
        let erase_again = removed(ng, vec![-1, 0]);
        let identity = removed(ng, vec![0, 1, 2]);
        let erase_ghost = removed(g, vec![1, -1]);
        assert!(rule(&[&erase0, &erase_again]).is_err());
        assert!(rule(&[&added(vec![el(3, ng)]), &erase0]).is_err());
        // an addition after an erasure, a ghost block, an identity numbering
        assert!(rule(&[&erase0, &added(vec![el(2, ng)])]).is_ok());
        assert!(rule(&[&erase0, &erase_ghost]).is_ok());
        assert!(rule(&[&added(vec![el(2, g)]), &erase0]).is_ok());
        assert!(rule(&[&erase0, &identity]).is_ok());
        assert!(rule(&[&added(vec![el(3, ng)]), &added(vec![el(4, ng)])]).is_ok());
        // an identity numbering names the block for a later erasure
        assert!(rule(&[&identity, &erase0]).is_err());
        // A non-identity `ElementsChanged` numbering
        // renumbers its block like an erasure (upstream's
        // `ChangedElementsEvent` derives from `RemovedElementsEvent`)
        let changed = |g: GhostType, num: Vec<Idx>| {
            let MeshEvent::ElementsRemoved { new_numbering, .. } = removed(g, num) else {
                unreachable!("built above")
            };
            MeshEvent::ElementsChanged {
                old: Vec::new(),
                new: Vec::new(),
                new_numbering,
                origin: "t".into(),
            }
        };
        let renumbering_change = changed(ng, vec![-1, 0, 1]);
        let e = rule(&[&added(vec![el(3, ng)]), &renumbering_change]).unwrap_err();
        assert!(format!("{e}").contains("ElementsChanged"), "{e}");
        assert!(rule(&[&erase0, &changed(ng, vec![1, 0])]).is_err());
        assert!(rule(&[&renumbering_change, &changed(ng, vec![1, 0])]).is_err());
        // an identity change, or a change of another block, composes
        assert!(rule(&[&added(vec![el(3, ng)]), &changed(ng, vec![0, 1, 2, 3])]).is_ok());
        assert!(rule(&[&erase0, &changed(g, vec![1, 0])]).is_ok());
        // a change names its block for a later erasure (as before)
        assert!(rule(&[&changed(ng, vec![0, 1, 2]), &erase0]).is_err());
    }

    /// At the mesh domain (serial; the
    /// agreed 2-rank variant needs `DistributedRemovalHook` to accept
    /// recorded `ElementsChanged`): an erasure of a block followed by a
    /// recorded `ElementsChanged` whose numbering of that block is not the
    /// identity is refused before the commit (recoverable, rolled back,
    /// nothing dispatched, the domain usable); with an identity numbering
    /// the same mutation commits.
    #[test]
    fn elements_changed_renumbering_after_an_erasure_is_refused_before_commit() {
        let comm = Communicator::serial();
        let mut m = Mesh::with_communicator(1, "bar", comm.clone()).unwrap();
        {
            let mut a = MeshAccessor::new(&mut m);
            for x in [0.0, 1.0, 2.0, 3.0, 4.0] {
                a.push_node(&[x]).unwrap();
            }
            let mut c = a.connectivity_mut(ElementType::Segment2, GhostType::NotGhost);
            for i in 0..4 {
                c.push_tuple(&[i, i + 1]);
            }
        }
        let mesh = SharedMesh::new(m);
        let token = mesh
            .register_consumer("accepts", EventPriority(50), MeshPart::Main)
            .unwrap();
        let mut b = Accepts::default();
        let seg = |i: Idx| Element::new(ElementType::Segment2, i, GhostType::NotGhost);
        let change = |num: Vec<Idx>| {
            move |tx: &mut MeshTransaction<'_>| -> Result<()> {
                tx.erase_elements(MeshPart::Main, &[seg(3)])?;
                let mut n = ElementTypeMapArray::new("changed");
                n.as_map_mut()
                    .insert(
                        ElementType::Segment2,
                        GhostType::NotGhost,
                        Array::from_vec(num, 1)?,
                    )
                    .map(|_| ())?;
                tx.record(
                    MeshPart::Main,
                    MeshEvent::ElementsChanged {
                        // element 0 changes into element 1 (listed
                        // elements; the empty-list case is
                        // `elements_changed_with_empty_lists_is_counted_by_its_numbering`)
                        old: vec![seg(0)],
                        new: vec![seg(1)],
                        new_numbering: n,
                        origin: "test".into(),
                    },
                );
                Ok(())
            }
        };
        let revision = mesh.revision().unwrap();
        let e = mesh
            .mutate(&mut [(&token, &mut b)], change(vec![1, 0, 2]), None)
            .expect_err("refused before the commit");
        assert!(matches!(e, Error::Unsupported(_)), "{e:?}");
        assert!(e.fatal_kind().is_none(), "recoverable: {e:?}");
        assert!(format!("{e}").contains("ElementsChanged"), "{e}");
        assert!(b.0.is_empty(), "nothing dispatched");
        assert_eq!(mesh.revision().unwrap(), revision);
        assert_eq!(
            mesh.read()
                .unwrap()
                .nb_element(ElementType::Segment2, GhostType::NotGhost),
            4
        );
        // the identity numbering composes (the domain is usable)
        mesh.mutate(&mut [(&token, &mut b)], change(vec![0, 1, 2]), None)
            .unwrap();
        assert_eq!(
            mesh.read()
                .unwrap()
                .nb_element(ElementType::Segment2, GhostType::NotGhost),
            3
        );
    }

    /// An `ElementsChanged` with empty `old` / `new`
    /// lists and a non-identity numbering is dispatched, and so checked, by
    /// its numbering (before: counted by `old` only, so it committed with
    /// nothing dispatched and its renumbering reached no consumer). After an
    /// erasure of the block it is refused before the commit (recoverable,
    /// rolled back, nothing dispatched); with an identity numbering and
    /// empty lists it changes nothing and is still not dispatched.
    #[test]
    fn elements_changed_with_empty_lists_is_counted_by_its_numbering() {
        let comm = Communicator::serial();
        let mut m = Mesh::with_communicator(1, "bar", comm.clone()).unwrap();
        {
            let mut a = MeshAccessor::new(&mut m);
            for x in [0.0, 1.0, 2.0, 3.0, 4.0] {
                a.push_node(&[x]).unwrap();
            }
            let mut c = a.connectivity_mut(ElementType::Segment2, GhostType::NotGhost);
            for i in 0..4 {
                c.push_tuple(&[i, i + 1]);
            }
        }
        let mesh = SharedMesh::new(m);
        let token = mesh
            .register_consumer("accepts", EventPriority(50), MeshPart::Main)
            .unwrap();
        let mut b = Accepts::default();
        let seg = |i: Idx| Element::new(ElementType::Segment2, i, GhostType::NotGhost);
        let numbering = |num: Vec<Idx>| {
            let mut n = ElementTypeMapArray::new("changed");
            n.as_map_mut()
                .insert(
                    ElementType::Segment2,
                    GhostType::NotGhost,
                    Array::from_vec(num, 1).unwrap(),
                )
                .unwrap();
            n
        };
        let change = |num: Vec<Idx>, erase: bool| {
            move |tx: &mut MeshTransaction<'_>| -> Result<()> {
                if erase {
                    tx.erase_elements(MeshPart::Main, &[seg(3)])?;
                }
                tx.record(
                    MeshPart::Main,
                    MeshEvent::ElementsChanged {
                        old: vec![],
                        new: vec![],
                        new_numbering: numbering(num),
                        origin: "test".into(),
                    },
                );
                Ok(())
            }
        };
        // the global count of the event alone
        let ev = |num: Vec<Idx>| MeshEvent::ElementsChanged {
            old: vec![],
            new: vec![],
            new_numbering: numbering(num),
            origin: "t".into(),
        };
        assert_eq!(event_len(&ev(vec![1, 0, 2])), 1);
        assert_eq!(event_len(&ev(vec![0, 1, 2])), 0);
        let one_new = MeshEvent::ElementsChanged {
            old: vec![],
            new: vec![seg(1)],
            new_numbering: numbering(vec![0, 1, 2]),
            origin: "t".into(),
        };
        assert_eq!(event_len(&one_new), 1, "counted by `new` as well");
        // refused before the commit after the erasure
        let revision = mesh.revision().unwrap();
        let e = mesh
            .mutate(&mut [(&token, &mut b)], change(vec![1, 0, 2], true), None)
            .expect_err("refused before the commit");
        assert!(matches!(e, Error::Unsupported(_)), "{e:?}");
        assert!(e.fatal_kind().is_none(), "recoverable: {e:?}");
        assert!(format!("{e}").contains("ElementsChanged"), "{e}");
        assert!(b.0.is_empty(), "nothing dispatched");
        assert_eq!(mesh.revision().unwrap(), revision);
        assert_eq!(
            mesh.read()
                .unwrap()
                .nb_element(ElementType::Segment2, GhostType::NotGhost),
            4
        );
        // a renumbering change alone is dispatched to the consumer
        mesh.mutate(
            &mut [(&token, &mut b)],
            change(vec![1, 0, 2, 3], false),
            None,
        )
        .unwrap();
        assert_eq!(b.0, ["other"], "the change is dispatched");
        // an identity change with empty lists is not dispatched
        b.0.clear();
        mesh.mutate(&mut [(&token, &mut b)], change(vec![0, 1, 2], true), None)
            .unwrap();
        assert_eq!(b.0, ["elements removed"]);
        assert_eq!(
            mesh.read()
                .unwrap()
                .nb_element(ElementType::Segment2, GhostType::NotGhost),
            3
        );
    }

    /// The node composition rule on event lists: on
    /// a periodic or distributed mesh a removal followed by any node event
    /// — a pure permutation, a second removal, an addition — is refused;
    /// additions before the removal and element events after it compose;
    /// on a serial non-periodic mesh the domain leaves node events to the
    /// DOF manager's check.
    #[test]
    fn node_event_composition_rule() {
        use crate::events::check_node_event_composition as rule;
        let removed = |num: Vec<Idx>| MeshEvent::NodesRemoved {
            nodes: Vec::new(),
            new_numbering: num,
            origin: "t".into(),
        };
        let added = MeshEvent::NodesAdded {
            nodes: vec![3],
            old_nodes: None,
            origin: "t".into(),
            extension: None,
        };
        let elements = MeshEvent::ElementsAdded {
            elements: vec![Element::new(ElementType::Segment2, 0, GhostType::NotGhost)],
            constitutive_law: None,
            origin: "t".into(),
            extension: None,
        };
        let remove = removed(vec![0, -1, 1, 2]);
        let permute = removed(vec![1, 0, 2]);
        for later in [&permute, &added, &remove] {
            let e = rule(&[&remove, &elements, later], true).unwrap_err();
            assert!(matches!(e, Error::Unsupported(_)), "{e:?}");
            assert!(
                format!("{e}").contains("make the removal the last node event"),
                "{e}"
            );
        }
        assert!(rule(&[&added, &remove], true).is_ok());
        assert!(rule(&[&remove], true).is_ok());
        assert!(rule(&[&remove, &elements], true).is_ok());
        assert!(rule(&[&remove, &permute], false).is_ok());
        assert!(rule(&[&remove, &added], false).is_ok());
    }

    /// At the mesh domain: on a serial **periodic**
    /// mesh, a node removal followed by a pure permutation in one mutation
    /// (the node count is kept, so no count guard sees it) is refused in
    /// the capability stage when the part has consumers — recoverable,
    /// rolled back, nothing dispatched; the composed numbering in one call
    /// commits, and so does the refused sequence without consumers.
    #[test]
    fn node_removal_not_last_on_a_periodic_mesh_is_refused_before_commit() {
        let comm = Communicator::serial();
        let line = || {
            let mut m = Mesh::with_communicator(1, "line", comm.clone()).unwrap();
            {
                let mut a = MeshAccessor::new(&mut m);
                for x in 0..6 {
                    a.push_node(&[f64::from(x)]).unwrap();
                }
                let mut c = a.connectivity_mut(ElementType::Segment2, GhostType::NotGhost);
                for i in 0..4 {
                    c.push_tuple(&[i, i + 1]);
                }
                drop(c);
                a.add_periodic_pair(4, 0).unwrap();
            }
            assert!(m.is_periodic());
            m
        };
        let first: [Idx; 6] = [0, 1, 2, 3, 4, -1];
        let second: [Idx; 5] = [0, 1, 4, 3, 2];
        let two_calls = |tx: &mut MeshTransaction<'_>| -> Result<()> {
            tx.remove_nodes(MeshPart::Main, &first)?;
            tx.remove_nodes(MeshPart::Main, &second)
        };
        let mesh = SharedMesh::new(line());
        let token = mesh
            .register_consumer("accepts", EventPriority(50), MeshPart::Main)
            .unwrap();
        let mut b = Accepts::default();
        let revision = mesh.revision().unwrap();
        let e = mesh
            .mutate(&mut [(&token, &mut b)], two_calls, None)
            .expect_err("refused before the commit");
        assert!(matches!(e, Error::Unsupported(_)), "{e:?}");
        assert!(e.fatal_kind().is_none(), "recoverable: {e:?}");
        assert!(
            format!("{e}").contains("make the removal the last node event"),
            "{e}"
        );
        assert!(b.0.is_empty(), "nothing dispatched");
        assert_eq!(mesh.revision().unwrap(), revision);
        assert_eq!(mesh.read().unwrap().nb_nodes(), 6);
        // the composed numbering in one call commits
        let composed: Vec<Idx> = first
            .iter()
            .map(|&n| usize::try_from(n).map_or(-1, |n| second[n]))
            .collect();
        mesh.mutate(
            &mut [(&token, &mut b)],
            |tx| tx.remove_nodes(MeshPart::Main, &composed),
            None,
        )
        .unwrap();
        assert_eq!(b.0, ["nodes removed"]);
        assert_eq!(mesh.read().unwrap().nb_nodes(), 5);
        // without consumers the two calls compose
        let free = SharedMesh::new(line());
        free.mutate(&mut [], two_calls, None).unwrap();
        assert_eq!(free.read().unwrap().nb_nodes(), 5);
    }
}
