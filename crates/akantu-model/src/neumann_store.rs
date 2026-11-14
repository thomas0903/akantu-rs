// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Port addition (D-105,
// UD-1280) to the Neumann application of src/model/common/boundary_condition/
// boundary_condition_tmpl.hh and to the doubled-node hook of
// src/model/solid_mechanics/solid_mechanics_model_cohesive/
// solid_mechanics_model_cohesive.cc (`onNodesAdded`); no upstream counterpart.

//! The per-boundary-element record of a model's Neumann loads (D-105,
//! UD-1280, category B).
//!
//! Upstream's Neumann `applyBC` adds the integrated traction of every
//! boundary element straight into the nodal dual array (`external_force`);
//! when cohesive insertion doubles a node, `onNodesAdded` gives the new copy
//! a zero dual value and the original copy keeps the load of every boundary
//! element it had, although some of them now belong to the new copy. Which
//! copy is the original follows from the doubled facet's orientation, which
//! on a distributed mesh depends on the partition (UD-1280).
//!
//! A [`NeumannStore`] keeps, per boundary element (local **and** ghost, as
//! the application integrates both), the nodal forces the model's Neumann
//! calls integrated on it, accumulated over the calls, in the layout of the
//! elemental arrays (`nodes per element × components`, node-major). When
//! nodes are doubled, [`NeumannStore::move_doubled_nodes`] moves each
//! element's recorded contribution at a doubled node from the original copy
//! to the copy the element now uses; nodal values of other origin stay with
//! the original copy. No communication: every rank holds the boundary
//! elements it integrated, i.e. every element whose contribution its dual
//! array holds.
//!
//! **Node identity of the records.** The insertion that doubles a
//! node first gives every boundary element the connectivity of its facet
//! (upstream `Mesh::sendEvent<NewNodesEvent>`, `mesh.cc:118-140`; the
//! port's `sync_boundary_connectivity`), so most boundary rows are
//! rewritten — mostly as permutations of the same nodes (1,839 of 2,798
//! rows at the first doubling of the benchmark's h = 0.12 cube), with the
//! new copies in place of the doubled nodes — and no event says so. The
//! store therefore keeps, with each entry, the node whose dual value holds
//! its load ([`NeumannStore::holders`]), and the original of every copy a
//! node event announced. Before any comparison with the dual array, any
//! recorded application and any move, every record is brought to its
//! element's current row by node identity: an entry goes to the slot of the
//! node holding it, or of the copy doubled from that node which the element
//! now uses (its load still held by the original until the move brings it
//! there). A row that did not change is left exactly as it was. A current
//! row that is neither a permutation of the holding nodes nor those nodes
//! with copies doubled from them in their place is refused with
//! `Error::Unsupported` (in the manner of D-111): in the capability check of
//! the node event ([`NeumannStore::check_nodes_doubled`], stage 3b of the
//! mutation, on the staged mesh), before the commit and agreed on every
//! rank of a distributed mesh; a row rewritten without any event is refused
//! by the store's next operation, with nothing changed. **What the
//! corrected placement guarantees:** after a doubling, every copy carries
//! exactly the recorded loads of the boundary elements that use it, each
//! element's share at each of its nodes as integrated (up to the rounding
//! of the move), whatever the element's shares and whatever the partition.
//! Verified with distinct shares (`_quadrangle_4` faces of a non-affine
//! hexahedral mesh, `_segment_3`, `_triangle_6`, a position-dependent
//! traction on `_triangle_3`) at 1, 2 and 4 ranks
//! (`akantu-cohesive/tests/doubled_node_unequal_shares.rs`, the unit tests
//! below). Before the UD-1280 fix the records were kept by slot of the row, so
//! a move after a rewrite took the share recorded for another node of the
//! element: exact only where an element's nodal shares are equal
//! (`_triangle_3` and `_segment_2` with the default one-point rules, every
//! benchmark, example and replay of the port), otherwise a fraction of an
//! elemental share on the wrong copy, dependent on the partition. Not the
//! store's matter: the boundary FE engine keeps the jacobians and normals
//! of a rewritten row's old node order (upstream too), so an application
//! made after a rewrite integrates a non-affine element with them.
//!
//! **Writes by other paths (D-105 as amended by D-108).** The store also
//! keeps the dual array as its last update left it (`seen`). Before each
//! recorded application and each move it compares the dual array with that
//! view, node by node and bit for bit. A node whose value another path
//! changed since (the user editing the external force, a foreign view, a
//! functor writing through a view, a boundary synchronisation) is
//! * **reset** when its new value is zero (every component): its entries in
//!   the records (each element's contribution **at that node**, the other
//!   nodes' entries of the same elements are kept) are forgotten — the usual
//!   "zero the external force, apply the tractions again" loop of upstream's
//!   examples therefore records exactly the loads of the last application,
//!   and a record never moves a load that is gone;
//! * otherwise **written**: the change is taken as an **added** nodal force
//!   of other origin. That part stays with the original copy at a doubling,
//!   and the Neumann records of the node still move.
//!
//! **Consequence: rescaling or replacing Neumann loads by writing the
//! external force array (a load ramp `f ← λ f`, hand-computed values) on a
//! model that may insert cohesive elements at loaded faces is not supported
//! by the corrected default.** The records still hold the loads as they
//! were applied, and a doubling moves them in full. Zero the array and
//! apply the loads again, or set the compat switch
//! `doubled_node_keeps_external_force` (upstream's placement: nothing
//! moves). The store cannot tell an addition from a replacement, so it
//! makes the case visible instead: a node written by another path while it
//! held a record is marked ("written since it was recorded", one flag per
//! node, cleared by a reset), and a move that takes a record from a marked
//! node counts it ([`NeumannStoreStats::moved_from_written_nodes`],
//! [`MoveReport::moved_from_written_nodes`]) and prints one warning at
//! `dblWarning` per move ([`akantu_core::aka_debug_warning!`]). The count
//! also includes additive writes, whose result is right: the warning says
//! what the result depends on.
//!
//! **Cost** (D-096: measured, not optimised): per recorded call, one pass
//! over the group's elements (the record), two over the dual array (the
//! comparison and the copy) and one over the recorded elements (the
//! alignment check: a comparison of each record's nodes with the element's
//! row), plus one more over the recorded elements when another path wrote a
//! node; per node event and per doubling, two or three passes over the
//! recorded elements. A model that never applies a Neumann load has no
//! store.
//!
//! **Periodic meshes:** the record holds each element's own contribution,
//! before the periodic sum of the application; a doubled periodic node
//! moves its elements' contributions, its partner's share stays.

use std::collections::{HashMap, HashSet};

use akantu_core::{
    Array, Element, ElementType, ElementTypeMapArray, Error, GhostType, INVALID_IDX, Idx, Real,
    Result, TypeFilter,
};
use akantu_mesh::Mesh;

/// The elemental contributions of one block in one Neumann application:
/// the integrated nodal forces of the elements `ids` (one row per entry).
#[derive(Debug)]
pub(crate) struct BlockContribution {
    pub(crate) ty: ElementType,
    pub(crate) ghost: GhostType,
    pub(crate) ids: Vec<Idx>,
    pub(crate) values: Array<Real>,
}

/// Counters of a store (diagnostics and tests).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NeumannStoreStats {
    /// Recorded Neumann applications.
    pub applications: u64,
    /// Recorded entries forgotten (one per element and node: an element's
    /// contribution at a node) because their node was reset by another path
    /// (see the module documentation).
    pub forgotten_entries: u64,
    /// Node values found written by another path (reset or not).
    pub foreign_writes: u64,
    /// Of them, node values set to zero (resets).
    pub reset_nodes: u64,
    /// Elemental node contributions moved to a new copy.
    pub moved_contributions: u64,
    /// Of them, contributions moved from an original copy that another path
    /// had written (to a non-zero value) while it held a record: correct if
    /// that write added a force, wrong if it rescaled or replaced Neumann
    /// loads (D-108; module documentation).
    pub moved_from_written_nodes: u64,
    /// Recorded entries released by an application: the element now
    /// uses a copy of the node that holds the entry's load, which was not
    /// moved (upstream's placement, `doubled_node_keeps_external_force`), and
    /// the application adds a load at the copy; the old load stays in the
    /// dual array at the original copy, as a value of no recorded element.
    /// Never counted by the corrected default, whose moves bring every entry
    /// to the copy its element uses.
    pub released_entries: u64,
}

/// What one [`NeumannStore::move_doubled_nodes`] did.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MoveReport {
    /// Doubled pairs of the event.
    pub pairs: usize,
    /// Elemental node contributions moved (one per element and new node).
    pub moved: usize,
    /// Recorded entries forgotten first (one per element and node; nodes
    /// reset by another path).
    pub forgotten: usize,
    /// Of the moved contributions, those taken from an original copy that
    /// another path had written while it held a record (D-108); a non-zero
    /// count prints one warning.
    pub moved_from_written_nodes: usize,
}

/// The per-boundary-element record of a model's Neumann loads (see the
/// module documentation).
#[derive(Clone, Debug)]
pub struct NeumannStore {
    forces: ElementTypeMapArray<Real>,
    /// Per recorded block, per element and slot: the node whose dual value
    /// holds the entry's load ([`NeumannStore::holders`]).
    holders: ElementTypeMapArray<Idx>,
    /// Per node: the node it was doubled from (a copy announced by a
    /// `NodesAdded` with `old_nodes`), `INVALID_IDX` for any other node.
    doubled_from: Vec<Idx>,
    seen: Vec<Real>,
    /// Per node: written by another path (to a non-zero value) while it
    /// held a record, and not reset since.
    written: Vec<bool>,
    nb_dof: usize,
    stats: NeumannStoreStats,
}

fn block_err(ty: ElementType, ghost: GhostType, what: &str) -> Error {
    Error::invalid(format!(
        "Neumann store (UD-1280): block ({ty}, {ghost:?}): {what}"
    ))
}

/// The refusal of a row the record cannot follow by node identity
/// (in the manner of D-111: `Error::Unsupported`, recoverable).
fn row_refusal(ty: ElementType, ghost: GhostType, e: usize, held: &[Idx], cur: &[Idx]) -> Error {
    Error::Unsupported(format!(
        "Neumann store (UD-1280): the connectivity of the recorded boundary element \
         ({ty}, {ghost:?}) {e} is now {cur:?}, which is neither a permutation of the nodes \
         {held:?} that hold its recorded loads nor those nodes with copies doubled from them \
         in their place: the record cannot follow it by node identity (zero the external force \
         before such a change and apply the loads again after it; a node renumbering earlier in \
         the same mutation is not composed: make it a mutation of its own)"
    ))
}

/// The re-alignment of one element's record: its entries and the
/// nodes holding them, in the slot order of the element's current row.
#[derive(Debug)]
struct Realign {
    ty: ElementType,
    ghost: GhostType,
    e: usize,
    entries: Vec<Real>,
    holders: Vec<Idx>,
}

/// What the dual array says about the nodes since the store's last update
/// (read-only scan, applied afterwards).
#[derive(Debug, Default)]
struct ViewChanges {
    nb_changed: u64,
    /// Nodes set to zero (every component).
    reset: HashSet<usize>,
    /// Nodes changed to another value and not yet marked "written".
    written: HashSet<usize>,
}

impl NeumannStore {
    /// An empty store for a dual array whose current values are `dual`
    /// (`nb_dof` components per node): those values are not Neumann loads
    /// of the store's and are never moved.
    ///
    /// # Errors
    /// `dual` not a whole number of nodes.
    pub fn new(dual: &[Real], nb_dof: usize) -> Result<Self> {
        if nb_dof == 0 || !dual.len().is_multiple_of(nb_dof) {
            return Err(Error::DimensionMismatch {
                context: "Neumann store: dual array components",
                expected: nb_dof,
                got: dual.len(),
            });
        }
        let nb_nodes = dual.len() / nb_dof;
        Ok(Self {
            forces: ElementTypeMapArray::new("neumann_element_forces"),
            holders: ElementTypeMapArray::new("neumann_element_force_nodes"),
            doubled_from: vec![INVALID_IDX; nb_nodes],
            seen: dual.to_vec(),
            written: vec![false; nb_nodes],
            nb_dof,
            stats: NeumannStoreStats::default(),
        })
    }

    /// Whether node `node` is marked as written by another path (to a
    /// non-zero value) while it held a record, and not reset since (module
    /// documentation; `false` for a node out of range).
    #[must_use]
    pub fn is_written(&self, node: usize) -> bool {
        self.written.get(node).copied().unwrap_or(false)
    }

    /// The recorded contributions: per `(type, ghost)` block of the boundary
    /// elements that received a load, one row per element of the mesh's
    /// block (`nodes per element × components`, node-major; zero for an
    /// element without a record). Entry `k` of a row is the load held by
    /// the node in slot `k` of the same row of [`NeumannStore::holders`]:
    /// after every operation of the store that is the element's
    /// current connectivity row, except at a slot whose node is a copy
    /// doubled from the holding node while the load stayed at the original
    /// (upstream's placement, `doubled_node_keeps_external_force`).
    #[must_use]
    pub fn forces(&self) -> &ElementTypeMapArray<Real> {
        &self.forces
    }

    /// The node holding each recorded entry: same blocks and rows as
    /// [`NeumannStore::forces`], one node per slot (`INVALID_IDX` for a slot
    /// that never held a load).
    #[must_use]
    pub fn holders(&self) -> &ElementTypeMapArray<Idx> {
        &self.holders
    }

    /// Components per node.
    #[must_use]
    pub fn nb_dof(&self) -> usize {
        self.nb_dof
    }

    /// Counters.
    #[must_use]
    pub fn stats(&self) -> NeumannStoreStats {
        self.stats
    }

    /// The blocks of the store.
    fn blocks(&self) -> Vec<(ElementType, GhostType)> {
        let mut out = Vec::new();
        for ghost in GhostType::ALL {
            out.extend(
                self.forces
                    .element_types(TypeFilter {
                        spatial_dimension: None,
                        ghost_type: ghost,
                        kind: None,
                    })
                    .map(|ty| (ty, ghost)),
            );
        }
        out
    }

    /// Whether node `c` is a copy doubled (directly or through other
    /// copies) from node `h`, by the copies announced so far and the pairs
    /// `pending` (new → old) of a node event not yet followed.
    fn descends_from(&self, c: Idx, h: Idx, pending: &HashMap<Idx, Idx>) -> Result<bool> {
        let mut n = c;
        for _ in 0..=self.doubled_from.len() + pending.len() {
            let parent = match pending.get(&n) {
                Some(&p) => p,
                None => usize::try_from(n)
                    .ok()
                    .and_then(|i| self.doubled_from.get(i).copied())
                    .unwrap_or(INVALID_IDX),
            };
            if parent < 0 {
                return Ok(false);
            }
            if parent == h {
                return Ok(true);
            }
            n = parent;
        }
        Err(Error::invalid(
            "Neumann store: a cycle in the doubled node pairs",
        ))
    }

    /// The re-alignment of one element's record to its current row `cur`:
    /// every loaded entry (not zero, and not held at a node of
    /// `skip`, whose entries are about to be forgotten) goes to the slot of
    /// its holding node or of a copy doubled from it; `None` when every
    /// loaded entry already is in that slot.
    ///
    /// # Errors
    /// `Error::Unsupported` when the row is neither a permutation of the
    /// holding nodes nor them with copies in their place.
    #[allow(clippy::too_many_arguments)] // one element's view of the store
    fn realign_row(
        &self,
        (ty, ghost, e): (ElementType, GhostType, usize),
        row: &[Real],
        held: &[Idx],
        cur: &[Idx],
        pending: &HashMap<Idx, Idx>,
        skip: &HashSet<usize>,
    ) -> Result<Option<Realign>> {
        let d = self.nb_dof;
        let nnpe = cur.len();
        let loaded = |k: usize| {
            row[k * d..(k + 1) * d].iter().any(|v| *v != 0.0)
                && usize::try_from(held[k]).map_or(true, |h| !skip.contains(&h))
        };
        if (0..nnpe).all(|k| !loaded(k) || held[k] == cur[k]) {
            return Ok(None);
        }
        let mut to = vec![usize::MAX; nnpe];
        let mut taken = vec![false; nnpe];
        let mut identity = true;
        for k in 0..nnpe {
            if !loaded(k) {
                continue;
            }
            let h = held[k];
            if h < 0 {
                return Err(block_err(ty, ghost, "a loaded entry without its node"));
            }
            let mut found = None;
            for (j, &c) in cur.iter().enumerate() {
                if c == h || self.descends_from(c, h, pending)? {
                    if found.is_some() {
                        return Err(row_refusal(ty, ghost, e, held, cur));
                    }
                    found = Some(j);
                }
            }
            let Some(j) = found else {
                return Err(row_refusal(ty, ghost, e, held, cur));
            };
            if taken[j] {
                return Err(row_refusal(ty, ghost, e, held, cur));
            }
            taken[j] = true;
            to[k] = j;
            identity &= j == k;
        }
        if identity {
            return Ok(None);
        }
        let mut entries = vec![0.0; nnpe * d];
        let mut holders = cur.to_vec();
        for (k, &j) in to.iter().enumerate() {
            if j == usize::MAX {
                continue;
            }
            entries[j * d..(j + 1) * d].copy_from_slice(&row[k * d..(k + 1) * d]);
            holders[j] = held[k];
        }
        Ok(Some(Realign {
            ty,
            ghost,
            e,
            entries,
            holders,
        }))
    }

    /// The re-alignment of every recorded element (or of `only`) to the
    /// mesh's current rows (read-only; [`NeumannStore::realign_row`]).
    ///
    /// # Errors
    /// A recorded block of another layout than the mesh's; a row the record
    /// cannot follow (`Error::Unsupported`); mesh errors.
    fn plan_alignment(
        &self,
        mesh: &Mesh,
        pending: &HashMap<Idx, Idx>,
        skip: &HashSet<usize>,
        only: Option<&[Element]>,
    ) -> Result<Vec<Realign>> {
        let d = self.nb_dof;
        let mut plan = Vec::new();
        for (ty, ghost) in self.blocks() {
            let conn = mesh.connectivity(ty, ghost)?;
            let nnpe = conn.nb_component();
            let block = self
                .forces
                .get(ty, ghost)
                .map_err(|_| block_err(ty, ghost, "missing"))?;
            let width = block.nb_component();
            if nnpe == 0 || width != nnpe * d || block.len() > conn.len() {
                return Err(block_err(ty, ghost, "layout"));
            }
            let holders = self
                .holders
                .get(ty, ghost)
                .map_err(|_| block_err(ty, ghost, "nodes of the records missing"))?;
            if holders.nb_component() != nnpe || holders.len() != block.len() {
                return Err(block_err(ty, ghost, "layout of the nodes of the records"));
            }
            let (conn, rows, held) = (conn.as_slice(), block.as_slice(), holders.as_slice());
            let mut one = |e: usize| -> Result<()> {
                if let Some(r) = self.realign_row(
                    (ty, ghost, e),
                    &rows[e * width..(e + 1) * width],
                    &held[e * nnpe..(e + 1) * nnpe],
                    &conn[e * nnpe..(e + 1) * nnpe],
                    pending,
                    skip,
                )? {
                    plan.push(r);
                }
                Ok(())
            };
            match only {
                None => (0..block.len()).try_for_each(&mut one)?,
                Some(list) => list
                    .iter()
                    .filter(|el| {
                        el.ty() == ty && el.ghost_type() == ghost && el.idx() < block.len()
                    })
                    .try_for_each(|el| one(el.idx()))?,
            }
        }
        Ok(plan)
    }

    /// Write a plan of [`NeumannStore::plan_alignment`] (validated).
    fn apply_alignment(&mut self, plan: Vec<Realign>) {
        for r in plan {
            if let Ok(b) = self.forces.get_mut(r.ty, r.ghost) {
                b.tuple_mut(r.e).copy_from_slice(&r.entries);
            }
            if let Ok(h) = self.holders.get_mut(r.ty, r.ghost) {
                h.tuple_mut(r.e).copy_from_slice(&r.holders);
            }
        }
    }

    /// The doubled pairs `(new_nodes[i], old_nodes[i])` of a node event,
    /// validated against `nb_nodes` (lengths, ranges, a cycle), as a map
    /// new → old.
    fn pairs(new_nodes: &[Idx], old_nodes: &[Idx], nb_nodes: usize) -> Result<HashMap<Idx, Idx>> {
        if new_nodes.len() != old_nodes.len() {
            return Err(Error::DimensionMismatch {
                context: "Neumann store: doubled node pairs",
                expected: new_nodes.len(),
                got: old_nodes.len(),
            });
        }
        let mut origin: HashMap<Idx, Idx> = HashMap::with_capacity(new_nodes.len());
        for (&n, &o) in new_nodes.iter().zip(old_nodes) {
            let (nu, ou) = (akantu_core::try_to_usize(n)?, akantu_core::try_to_usize(o)?);
            if nu >= nb_nodes || ou >= nb_nodes {
                return Err(Error::OutOfRange {
                    what: "doubled node",
                    index: nu.max(ou),
                    len: nb_nodes,
                });
            }
            origin.insert(n, o);
        }
        for &n in origin.keys() {
            let mut r = n;
            let mut steps = 0;
            while let Some(&o) = origin.get(&r) {
                r = o;
                steps += 1;
                if steps > origin.len() {
                    return Err(Error::invalid(
                        "Neumann store: a cycle in the doubled node pairs",
                    ));
                }
            }
        }
        Ok(origin)
    }

    /// Validate pairs against the copies already announced: a new node
    /// announced with another original is refused (nothing changed).
    fn check_registration(&self, pairs: &HashMap<Idx, Idx>) -> Result<()> {
        for (&n, &o) in pairs {
            let known = usize::try_from(n)
                .ok()
                .and_then(|i| self.doubled_from.get(i).copied())
                .unwrap_or(INVALID_IDX);
            if known >= 0 && known != o {
                return Err(Error::invalid(format!(
                    "Neumann store: node {n} was announced as a copy of node {known}, not {o}"
                )));
            }
        }
        Ok(())
    }

    /// Record the pairs (validated by [`NeumannStore::check_registration`]).
    fn register(&mut self, pairs: &HashMap<Idx, Idx>) {
        for (&n, &o) in pairs {
            if let Some(slot) = usize::try_from(n)
                .ok()
                .and_then(|i| self.doubled_from.get_mut(i))
            {
                *slot = o;
            }
        }
    }

    /// The capability check of a node event (stage 3b, before the commit):
    /// with the event's doubled pairs (`old_nodes` of a
    /// `NodesAdded`, empty for nodes that double nothing), every recorded
    /// element's current row (the staged mesh: the insertion has already
    /// copied the facet rows into the boundary rows) must be a permutation
    /// of the nodes holding its loads, or those nodes with copies doubled
    /// from them in their place. A refusal is recoverable and, on a
    /// distributed mesh, agreed on every rank by the mutation (D-111).
    ///
    /// With `dual` (the external force before the event),
    /// the entries held at nodes another path has zeroed since the
    /// store's last update are left out, as the next application or move
    /// would forget them: "zero the external force, change the rows, apply
    /// the loads again" passes.
    ///
    /// # Errors
    /// Malformed pairs; `Error::Unsupported` for a row the record cannot
    /// follow; a recorded block of another layout than the mesh's; `dual`
    /// of another size than the store's view of it.
    pub fn check_nodes_doubled(
        &self,
        mesh: &Mesh,
        dual: Option<&[Real]>,
        new_nodes: &[Idx],
        old_nodes: &[Idx],
    ) -> Result<()> {
        let pairs = Self::pairs(new_nodes, old_nodes, mesh.nb_nodes())?;
        self.check_registration(&pairs)?;
        let skip = match dual {
            Some(d) => self.scan_view(d)?.reset,
            None => HashSet::new(),
        };
        self.plan_alignment(mesh, &pairs, &skip, None)?;
        Ok(())
    }

    /// A node event followed (after [`NeumannStore::on_nodes_added`]): the
    /// copies of `new_nodes[i]` from `old_nodes[i]` are recorded, and every
    /// record is brought to its element's current row by node identity:
    /// an entry goes to the slot of the node holding its load, or
    /// of the copy doubled from it that the element now uses (the load
    /// itself stays where it is until [`NeumannStore::move_doubled_nodes`]
    /// moves it). Nothing changes for a row that did not change.
    ///
    /// With `dual` (the external force after the event's resize), the
    /// writes by other paths are taken first, as
    /// [`NeumannStore::forget_foreign_writes`] does (the
    /// same entries left out as by the check).
    ///
    /// # Errors
    /// See [`NeumannStore::check_nodes_doubled`] (nothing changed).
    pub fn on_nodes_doubled(
        &mut self,
        mesh: &Mesh,
        dual: Option<&[Real]>,
        new_nodes: &[Idx],
        old_nodes: &[Idx],
    ) -> Result<()> {
        let pairs = Self::pairs(new_nodes, old_nodes, self.doubled_from.len())?;
        self.check_registration(&pairs)?;
        let changes = match dual {
            Some(d) => Some(self.scan_view(d)?),
            None => None,
        };
        let empty = HashSet::new();
        let skip = changes.as_ref().map_or(&empty, |c| &c.reset);
        let plan = self.plan_alignment(mesh, &pairs, skip, None)?;
        self.register(&pairs);
        if let (Some(c), Some(d)) = (changes, dual) {
            self.apply_view_changes(c, d);
        }
        self.apply_alignment(plan);
        Ok(())
    }

    /// Compare the dual array with the store's view of it (module
    /// documentation): at every node **reset** by another path (its value
    /// changed to zero in every component) the recorded entries are
    /// forgotten — each element's contribution at that node, not the
    /// element's other entries — and the node's
    /// "written" mark is cleared; a node changed to any other value carries
    /// an added force of other origin, keeps its records and is marked
    /// "written" if it holds a record (D-108). The node of an entry is the
    /// node holding its load ([`NeumannStore::holders`]). Afterwards every
    /// record is brought to its element's current row by node identity.
    /// Returns the number of entries forgotten.
    ///
    /// # Errors
    /// `dual` of another size than the store's view of it; a recorded block
    /// of another layout than the mesh's; `Error::Unsupported` for a row
    /// the record cannot follow (nothing changed); mesh errors.
    pub fn forget_foreign_writes(&mut self, mesh: &Mesh, dual: &[Real]) -> Result<usize> {
        let changes = self.scan_view(dual)?;
        let plan = self.plan_alignment(mesh, &HashMap::new(), &changes.reset, None)?;
        let forgotten = self.apply_view_changes(changes, dual);
        self.apply_alignment(plan);
        Ok(forgotten)
    }

    /// The nodes whose dual value another path changed since the store's
    /// last update (read-only).
    fn scan_view(&self, dual: &[Real]) -> Result<ViewChanges> {
        if dual.len() != self.seen.len() {
            return Err(Error::DimensionMismatch {
                context: "Neumann store: dual array length (the store missed a node event)",
                expected: self.seen.len(),
                got: dual.len(),
            });
        }
        let d = self.nb_dof;
        let mut out = ViewChanges::default();
        for (n, (a, b)) in dual
            .chunks_exact(d)
            .zip(self.seen.chunks_exact(d))
            .enumerate()
        {
            if a.iter().zip(b).any(|(x, y)| x.to_bits() != y.to_bits()) {
                out.nb_changed += 1;
                if a.iter().all(|x| *x == 0.0) {
                    // a reset: the node's loads are gone
                    out.reset.insert(n);
                } else if !self.written.get(n).copied().unwrap_or(false) {
                    // marked below if the node holds a record
                    out.written.insert(n);
                }
            }
        }
        Ok(out)
    }

    /// Apply a scan of [`NeumannStore::scan_view`] (blocks validated by a
    /// plan of [`NeumannStore::plan_alignment`] or by construction):
    /// forget the entries held at reset nodes, mark the written nodes that
    /// hold an entry, take `dual` as the view. Returns the entries
    /// forgotten.
    fn apply_view_changes(&mut self, changes: ViewChanges, dual: &[Real]) -> usize {
        let ViewChanges {
            nb_changed,
            reset,
            written,
        } = changes;
        if nb_changed == 0 {
            return 0;
        }
        self.stats.foreign_writes += nb_changed;
        self.stats.reset_nodes += reset.len() as u64;
        for &n in &reset {
            if let Some(w) = self.written.get_mut(n) {
                *w = false;
            }
        }
        let d = self.nb_dof;
        let mut forgotten = 0;
        if !(reset.is_empty() && written.is_empty()) {
            for (ty, ghost) in self.blocks() {
                let (Ok(block), Ok(held)) =
                    (self.forces.get_mut(ty, ghost), self.holders.get(ty, ghost))
                else {
                    continue;
                };
                let nnpe = held.nb_component();
                if nnpe == 0 || block.nb_component() != nnpe * d {
                    continue;
                }
                let rows = block.as_mut_slice();
                for (entry, &h) in rows.chunks_exact_mut(d).zip(held.as_slice()) {
                    let Ok(h) = usize::try_from(h) else {
                        continue;
                    };
                    if entry.iter().all(|v| *v == 0.0) {
                        continue;
                    }
                    if reset.contains(&h) {
                        entry.fill(0.0);
                        forgotten += 1;
                    } else if written.contains(&h)
                        && let Some(w) = self.written.get_mut(h)
                    {
                        *w = true;
                    }
                }
            }
        }
        self.stats.forgotten_entries += forgotten as u64;
        self.seen.copy_from_slice(dual);
        forgotten
    }

    /// Validate the contributions of one application against the store and
    /// the mesh (nothing changed on error).
    pub(crate) fn check_contributions(
        &self,
        mesh: &Mesh,
        contributions: &[BlockContribution],
    ) -> Result<()> {
        for c in contributions {
            let n = mesh.nb_element(c.ty, c.ghost);
            let nnpe = c.ty.nb_nodes_per_element();
            let width = nnpe * self.nb_dof;
            if c.values.nb_component() != width || c.values.len() != c.ids.len() {
                return Err(block_err(c.ty, c.ghost, "contribution layout"));
            }
            if mesh.connectivity(c.ty, c.ghost)?.nb_component() != nnpe {
                return Err(block_err(c.ty, c.ghost, "connectivity layout"));
            }
            if let Ok(b) = self.forces.get(c.ty, c.ghost) {
                if b.nb_component() != width {
                    return Err(block_err(c.ty, c.ghost, "components"));
                }
                if b.len() > n {
                    return Err(block_err(c.ty, c.ghost, "more records than elements"));
                }
            }
            for &e in &c.ids {
                if usize::try_from(e).map_or(true, |e| e >= n) {
                    return Err(block_err(c.ty, c.ghost, "element out of range"));
                }
            }
        }
        Ok(())
    }

    /// Add the contributions of one application (validated by
    /// [`NeumannStore::check_contributions`], after the records were
    /// brought to the current rows by [`NeumannStore::forget_foreign_writes`]).
    /// A contribution at slot `k` is held by the node of slot `k` of the
    /// element's current row; an entry there that is held by the node the
    /// copy was doubled from (a load that was not moved: upstream's
    /// placement) is released first ([`NeumannStoreStats::released_entries`]).
    pub(crate) fn add_contributions(
        &mut self,
        mesh: &Mesh,
        contributions: Vec<BlockContribution>,
    ) -> Result<()> {
        self.stats.applications += 1;
        let d = self.nb_dof;
        for c in contributions {
            let n = mesh.nb_element(c.ty, c.ghost);
            let width = c.values.nb_component();
            let conn = mesh.connectivity(c.ty, c.ghost)?;
            let nnpe = conn.nb_component();
            let conn = conn.as_slice();
            let block = self.forces.alloc(n, width, c.ty, c.ghost, 0.0)?;
            let rows = block.as_mut_slice();
            let held = self.holders.alloc(n, nnpe, c.ty, c.ghost, INVALID_IDX)?;
            let held = held.as_mut_slice();
            for (pos, &e) in c.ids.iter().enumerate() {
                let e = akantu_core::try_to_usize(e)?;
                let src = &c.values.as_slice()[pos * width..(pos + 1) * width];
                for k in 0..nnpe {
                    let v = &src[k * d..(k + 1) * d];
                    if v.iter().all(|x| *x == 0.0) {
                        continue;
                    }
                    let node = conn[e * nnpe + k];
                    let entry = &mut rows[e * width + k * d..e * width + (k + 1) * d];
                    let h = &mut held[e * nnpe + k];
                    if *h != node {
                        if entry.iter().any(|x| *x != 0.0) {
                            entry.fill(0.0);
                            self.stats.released_entries += 1;
                        }
                        *h = node;
                    }
                    for (r, x) in entry.iter_mut().zip(v) {
                        *r += x;
                    }
                }
            }
        }
        Ok(())
    }

    /// Record `dual` as the store's view of the dual array (after an
    /// update).
    pub(crate) fn snapshot(&mut self, dual: &[Real]) {
        self.seen.clear();
        self.seen.extend_from_slice(dual);
    }

    /// Move the recorded contributions of doubled nodes (D-105): for every
    /// pair `(new_nodes[i], old_nodes[i])` (local node indices of the rank,
    /// local, slave or ghost) and every recorded element whose connectivity
    /// now holds the new copy, the element's contribution at that node is
    /// moved from the node holding it (the original copy) to the new one.
    /// The records are first brought to the elements' current rows **by node
    /// identity** (the insertion copies the facet rows into the
    /// boundary rows, mostly permutations of the same nodes, before this
    /// move), so the contribution moved is the one recorded at the original
    /// node, whatever its slot. A new node doubled from a node that is
    /// itself new in the same event (a chained pair list) moves from the
    /// event's original node, the one whose dual value holds the load; the
    /// cohesive inserter pairs every new node with an original node, so it
    /// never produces a chain (kept for any pair list,
    /// tested directly). Writes by another path are handled first
    /// ([`NeumannStore::forget_foreign_writes`], by the node holding each
    /// entry): a node reset just before the doubling moves nothing, and a
    /// node written just before it is marked. A contribution taken from an
    /// original copy marked "written" is counted
    /// ([`MoveReport::moved_from_written_nodes`]) and a non-zero count
    /// prints one warning at `dblWarning` (D-108).
    ///
    /// # Errors
    /// Pair lists of different lengths, nodes out of range, a cycle in the
    /// pairs, a new node announced with another original, a dual array of
    /// another size than the mesh's nodes, `Error::Unsupported` for a row
    /// the record cannot follow (nothing changed in all these cases); mesh
    /// errors.
    pub fn move_doubled_nodes(
        &mut self,
        mesh: &Mesh,
        dual: &mut [Real],
        new_nodes: &[Idx],
        old_nodes: &[Idx],
    ) -> Result<MoveReport> {
        let d = self.nb_dof;
        if dual.len() != mesh.nb_nodes() * d {
            return Err(Error::DimensionMismatch {
                context: "Neumann store: dual array vs mesh nodes",
                expected: mesh.nb_nodes() * d,
                got: dual.len(),
            });
        }
        let origin = Self::pairs(new_nodes, old_nodes, dual.len() / d)?;
        self.check_registration(&origin)?;
        let changes = self.scan_view(dual)?;
        let plan = self.plan_alignment(mesh, &origin, &changes.reset, None)?;
        // nothing fails from here on (layouts validated by the plan)
        self.register(&origin);
        let forgotten = self.apply_view_changes(changes, dual);
        self.apply_alignment(plan);
        let mut moved = 0;
        let mut from_written = 0;
        let mut written_origins: HashSet<usize> = HashSet::new();
        for (ty, ghost) in self.blocks() {
            let conn = mesh.connectivity(ty, ghost)?;
            let nnpe = conn.nb_component();
            let (Ok(block), Ok(held)) =
                (self.forces.get(ty, ghost), self.holders.get_mut(ty, ghost))
            else {
                return Err(block_err(ty, ghost, "missing"));
            };
            let conn = &conn.as_slice()[..held.len() * nnpe];
            let held = held.as_mut_slice();
            for ((c, h), &n) in block
                .as_slice()
                .chunks_exact(d)
                .zip(held.iter_mut())
                .zip(conn)
            {
                if *h == n || c.iter().all(|v| *v == 0.0) {
                    continue;
                }
                let (nu, o) = (
                    akantu_core::try_to_usize(n)?,
                    akantu_core::try_to_usize(*h)?,
                );
                for (j, v) in c.iter().enumerate() {
                    dual[nu * d + j] += v;
                    dual[o * d + j] -= v;
                }
                *h = n;
                moved += 1;
                if self.written.get(o).copied().unwrap_or(false) {
                    from_written += 1;
                    written_origins.insert(o);
                }
            }
        }
        self.stats.moved_contributions += moved as u64;
        self.stats.moved_from_written_nodes += from_written as u64;
        if from_written > 0 {
            akantu_core::aka_debug_warning!(
                "Neumann store (D-105, D-108, UD-1280): {from_written} recorded Neumann \
                 contribution(s) moved from {} doubled node(s) whose external force another \
                 path had written since they were recorded; the move is right if that write \
                 added a force, wrong if it rescaled or replaced Neumann loads (not supported by \
                 the corrected default: zero the external force and apply the loads again, or \
                 set doubled_node_keeps_external_force)",
                written_origins.len()
            );
        }
        self.snapshot(dual);
        Ok(MoveReport {
            pairs: new_nodes.len(),
            moved,
            forgotten,
            moved_from_written_nodes: from_written,
        })
    }

    /// Nodes appended (`nb_nodes` after the event): the dual array grew with
    /// zeros, and the new nodes are not marked "written" (their copies, if
    /// any, are recorded by [`NeumannStore::on_nodes_doubled`]).
    pub fn on_nodes_added(&mut self, nb_nodes: usize) {
        self.seen.resize(nb_nodes * self.nb_dof, 0.0);
        self.written.resize(nb_nodes, false);
        self.doubled_from.resize(nb_nodes, INVALID_IDX);
    }

    /// Validate a node compaction against the store (nothing changed).
    ///
    /// # Errors
    /// A numbering of another length than the store's nodes, or not a
    /// compaction.
    pub fn check_nodes_removed(&self, new_numbering: &[Idx]) -> Result<()> {
        crate::model::node_compaction_plan(self.seen.len() / self.nb_dof, new_numbering)?;
        Ok(())
    }

    /// Nodes removed (`new_numbering[old] = new` or `INVALID_IDX`): the
    /// dual array was compacted the same way; the "written" marks, the
    /// nodes holding the records and the copies' originals follow their
    /// nodes (an entry held at a removed node is dropped with the node's
    /// dual value).
    ///
    /// # Errors
    /// A numbering of another length than the store's nodes.
    pub fn on_nodes_removed(&mut self, new_numbering: &[Idx]) -> Result<()> {
        let (plan, kept) =
            crate::model::node_compaction_plan(self.seen.len() / self.nb_dof, new_numbering)?;
        let renumber = |n: Idx| -> Idx {
            usize::try_from(n)
                .ok()
                .and_then(|i| plan.get(i).copied().flatten())
                .and_then(|to| Idx::try_from(to).ok())
                .unwrap_or(INVALID_IDX)
        };
        let mut a = Array::from_vec(std::mem::take(&mut self.seen), self.nb_dof)?;
        crate::model::apply_node_compaction(&mut a, &(plan.clone(), kept));
        self.seen = a.as_slice().to_vec();
        let mut written = vec![false; kept];
        let mut doubled_from = vec![INVALID_IDX; kept];
        for (old, to) in plan.iter().enumerate() {
            let Some(to) = to else { continue };
            if let (Some(&w), Some(slot)) = (self.written.get(old), written.get_mut(*to)) {
                *slot = w;
            }
            if let (Some(&p), Some(slot)) = (self.doubled_from.get(old), doubled_from.get_mut(*to))
            {
                *slot = renumber(p);
            }
        }
        self.written = written;
        self.doubled_from = doubled_from;
        let d = self.nb_dof;
        for (ty, ghost) in self.blocks() {
            let (Ok(block), Ok(held)) = (
                self.forces.get_mut(ty, ghost),
                self.holders.get_mut(ty, ghost),
            ) else {
                continue;
            };
            for (entry, h) in block
                .as_mut_slice()
                .chunks_exact_mut(d)
                .zip(held.as_mut_slice())
            {
                if *h < 0 {
                    continue;
                }
                *h = renumber(*h);
                if *h < 0 {
                    entry.fill(0.0);
                }
            }
        }
        Ok(())
    }

    /// Elements appended: the recorded blocks grow (zero rows) to hold the
    /// new elements' indices (in the numbering of the event, so that a later
    /// renumbering of the same mutation applies, `docs/ARCHITECTURE.md`, "Staged mutation").
    ///
    /// # Errors
    /// Index overflow.
    pub fn on_elements_added(&mut self, elements: &[Element]) -> Result<()> {
        for el in elements {
            let need = el.idx() + 1;
            if let Ok(b) = self.forces.get_mut(el.ty(), el.ghost_type())
                && b.len() < need
            {
                b.try_resize(need, 0.0)?;
            }
            if let Ok(h) = self.holders.get_mut(el.ty(), el.ghost_type())
                && h.len() < need
            {
                h.try_resize(need, INVALID_IDX)?;
            }
        }
        Ok(())
    }

    /// Validate an element renumbering against the recorded blocks
    /// (nothing changed; for the check stage before a commit).
    ///
    /// # Errors
    /// A numbering of a recorded block of another length than the block, or
    /// with indices that are not a compaction.
    pub fn check_elements_removed(&self, new_numbering: &ElementTypeMapArray<Idx>) -> Result<()> {
        for (ty, ghost) in self.blocks() {
            let Ok(num) = new_numbering.get(ty, ghost) else {
                continue;
            };
            let block = self
                .forces
                .get(ty, ghost)
                .map_err(|_| block_err(ty, ghost, "missing"))?;
            if num.len() < block.len() {
                return Err(block_err(ty, ghost, "renumbering shorter than the records"));
            }
            let kept = num.as_slice().iter().filter(|&&n| n >= 0).count();
            for &n in num.as_slice() {
                if n != INVALID_IDX && usize::try_from(n).map_or(true, |n| n >= kept) {
                    return Err(block_err(ty, ghost, "renumbering is not a compaction"));
                }
            }
        }
        Ok(())
    }

    /// Elements removed and renumbered: each record follows its element,
    /// the records of removed elements are dropped (their loads stay in the
    /// dual array, as values of no recorded element).
    ///
    /// # Errors
    /// See [`NeumannStore::check_elements_removed`].
    pub fn on_elements_removed(&mut self, new_numbering: &ElementTypeMapArray<Idx>) -> Result<()> {
        self.check_elements_removed(new_numbering)?;
        fn compact<T: Copy>(a: &mut Array<T>, num: &[Idx], fill: T) -> Result<()> {
            let width = a.nb_component();
            let kept = num.iter().filter(|&&n| n >= 0).count();
            let old = a.as_slice().to_vec();
            let mut new = vec![fill; kept * width];
            for (e, &to) in num.iter().enumerate() {
                if to < 0 || (e + 1) * width > old.len() {
                    continue;
                }
                let to = akantu_core::try_to_usize(to)?;
                new[to * width..(to + 1) * width].copy_from_slice(&old[e * width..(e + 1) * width]);
            }
            *a = Array::from_vec(new, width)?;
            Ok(())
        }
        for (ty, ghost) in self.blocks() {
            let Ok(num) = new_numbering.get(ty, ghost) else {
                continue;
            };
            let num = num.as_slice();
            let block = self
                .forces
                .get_mut(ty, ghost)
                .map_err(|_| block_err(ty, ghost, "missing"))?;
            compact(block, num, 0.0)?;
            if let Ok(h) = self.holders.get_mut(ty, ghost) {
                compact(h, num, INVALID_IDX)?;
            }
        }
        Ok(())
    }

    /// Validate an `ElementsChanged` against the recorded blocks (nothing
    /// changed; for the check stage before a commit).
    ///
    /// The store follows the event's **lists** (upstream
    /// `substituteElements`: `new[i]` takes the record of `old[i]`). Its
    /// **numbering** is not applied by the mesh domain — the connectivity,
    /// the groups and the element data stay where they are unless the
    /// recorder moved them itself, as upstream's `Mesh::sendEvent` applies
    /// a removal's numbering and not a change's, whose only upstream emitter
    /// (IGFEM) sends the numbering of the erasure that follows — and no
    /// other consumer of the model applies it either (FE engines,
    /// constitutive laws). So the store cannot tell from the numbering alone
    /// whether the elements of a recorded block moved: a numbering that
    /// moves (or removes) an element of a recorded block is accepted only
    /// when the lists state the same move for that element (`old = e`,
    /// `new = numbering[e]`, same block), and then following the lists is
    /// following the numbering. Any other such numbering — in particular a
    /// renumbering with empty lists, which is dispatched to this store,
    /// is refused here, before the commit: recoverable, and the check stage
    /// is agreed on every rank of a distributed mesh. A record that moves
    /// to an element must also be able to follow that element's row (the
    /// staged mesh) by node identity.
    ///
    /// # Errors
    /// Lists of different lengths; `Error::Unsupported` for a numbering of
    /// a recorded block that the lists do not carry, or for a substitute
    /// whose row the record cannot follow.
    pub fn check_elements_changed(
        &self,
        mesh: &Mesh,
        old: &[Element],
        new: &[Element],
        new_numbering: &ElementTypeMapArray<Idx>,
    ) -> Result<()> {
        if old.len() != new.len() {
            return Err(Error::DimensionMismatch {
                context: "Neumann store: changed elements",
                expected: old.len(),
                got: new.len(),
            });
        }
        // a record moves only to an element of the same layout, whose row it
        // can follow by node identity
        let none: HashMap<Idx, Idx> = HashMap::new();
        for (o, n) in old.iter().zip(new) {
            let (Ok(b), Ok(h)) = (
                self.forces.get(o.ty(), o.ghost_type()),
                self.holders.get(o.ty(), o.ghost_type()),
            ) else {
                continue;
            };
            if o.idx() >= b.len() {
                continue;
            }
            if n.ty().nb_nodes_per_element() != o.ty().nb_nodes_per_element() {
                return Err(Error::Unsupported(format!(
                    "Neumann store (UD-1280): an ElementsChanged substitutes the recorded element \
                     {o:?} by {n:?}, whose nodes per element differ: the record cannot follow it"
                )));
            }
            let cur = mesh.connectivity(n.ty(), n.ghost_type())?;
            if n.idx() >= cur.len() || h.len() != b.len() {
                return Err(block_err(n.ty(), n.ghost_type(), "substitute out of range"));
            }
            self.realign_row(
                (n.ty(), n.ghost_type(), n.idx()),
                b.tuple(o.idx()),
                h.tuple(o.idx()),
                cur.tuple(n.idx()),
                &none,
                &HashSet::new(),
            )?;
        }
        for (ty, ghost) in self.blocks() {
            let Ok(num) = new_numbering.get(ty, ghost) else {
                continue;
            };
            let mut listed: Option<HashMap<usize, usize>> = None;
            for (e, &to) in num.as_slice().iter().enumerate() {
                let to = usize::try_from(to).ok();
                if to == Some(e) {
                    continue;
                }
                let pairs = listed.get_or_insert_with(|| {
                    let in_block = |x: &Element| x.ty() == ty && x.ghost_type() == ghost;
                    old.iter()
                        .zip(new)
                        .filter(|(o, n)| in_block(o) && in_block(n))
                        .map(|(o, n)| (o.idx(), n.idx()))
                        .collect()
                });
                if to.is_none() || pairs.get(&e).copied() != to {
                    return Err(Error::Unsupported(format!(
                        "Neumann store (UD-1280): an ElementsChanged numbering moves element \
                         {e} of the recorded block ({ty}, {ghost:?}) to {} without the event's \
                         lists stating that move; the mesh does not apply a change's numbering, \
                         so the record cannot follow it (state the move in the old / new lists, \
                         or erase elements with erase_elements)",
                        to.map_or_else(|| "nowhere".to_owned(), |t| t.to_string())
                    )));
                }
            }
        }
        Ok(())
    }

    /// Elements changed identity (`old[i]` is now `new[i]`, upstream
    /// `substituteElements`): `new[i]` takes the record of `old[i]` and the
    /// nodes holding it, brought to `new[i]`'s row by node identity;
    /// `old[i]` keeps its own row until an erasure removes it (upstream's
    /// old elements leave by a later `RemovedElementsEvent`). The numbering
    /// is accepted only as [`NeumannStore::check_elements_changed`] says.
    ///
    /// # Errors
    /// See [`NeumannStore::check_elements_changed`]; a target block of
    /// another layout.
    pub fn on_elements_changed(
        &mut self,
        mesh: &Mesh,
        old: &[Element],
        new: &[Element],
        new_numbering: &ElementTypeMapArray<Idx>,
    ) -> Result<()> {
        self.check_elements_changed(mesh, old, new, new_numbering)?;
        let mut rows: Vec<(Element, Vec<Real>, Vec<Idx>)> = Vec::new();
        for (o, n) in old.iter().zip(new) {
            let row = match self.forces.get(o.ty(), o.ghost_type()) {
                Ok(b) if o.idx() < b.len() => b.tuple(o.idx()).to_vec(),
                _ => Vec::new(),
            };
            let held = match self.holders.get(o.ty(), o.ghost_type()) {
                Ok(h) if o.idx() < h.len() => h.tuple(o.idx()).to_vec(),
                _ => Vec::new(),
            };
            rows.push((*n, row, held));
        }
        let mut moved_to: Vec<Element> = Vec::new();
        for (n, row, held) in rows {
            let nnpe = n.ty().nb_nodes_per_element();
            let width = nnpe * self.nb_dof;
            if row.is_empty() && !self.forces.exists(n.ty(), n.ghost_type()) {
                continue;
            }
            let b = match self.forces.get_mut(n.ty(), n.ghost_type()) {
                Ok(b) => b,
                Err(_) => self.forces.alloc(0, width, n.ty(), n.ghost_type(), 0.0)?,
            };
            if b.nb_component() != width {
                return Err(block_err(n.ty(), n.ghost_type(), "components"));
            }
            if b.len() <= n.idx() {
                b.try_resize(n.idx() + 1, 0.0)?;
            }
            let len = b.len();
            let dst = b.tuple_mut(n.idx());
            if row.is_empty() {
                dst.fill(0.0);
            } else if row.len() == dst.len() {
                dst.copy_from_slice(&row);
            } else {
                return Err(block_err(n.ty(), n.ghost_type(), "components"));
            }
            let h = self
                .holders
                .alloc(len, nnpe, n.ty(), n.ghost_type(), INVALID_IDX)?;
            let dst = h.tuple_mut(n.idx());
            if held.len() == dst.len() {
                dst.copy_from_slice(&held);
            } else {
                dst.fill(INVALID_IDX);
            }
            moved_to.push(n);
        }
        let plan = self.plan_alignment(mesh, &HashMap::new(), &HashSet::new(), Some(&moved_to))?;
        self.apply_alignment(plan);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_store_sees_the_current_values_as_foreign() {
        let s = NeumannStore::new(&[1.0, 2.0, 3.0, 4.0], 2).unwrap();
        assert_eq!(s.nb_dof(), 2);
        assert_eq!(s.stats(), NeumannStoreStats::default());
        assert!(NeumannStore::new(&[1.0, 2.0, 3.0], 2).is_err());
    }

    use akantu_core::debug::{DebugLevel, capture};
    use akantu_mesh::MeshAccessor;

    const SEG: ElementType = ElementType::Segment2;
    const TRI: ElementType = ElementType::Triangle3;
    const G: GhostType = GhostType::NotGhost;

    /// A 2-D bar: nodes on the x axis, the given segments.
    fn bar(nb_nodes: usize, segs: &[[Idx; 2]]) -> Mesh {
        let mut m = Mesh::new(2, "bar").unwrap();
        {
            let mut a = MeshAccessor::new(&mut m);
            for i in 0..nb_nodes {
                #[allow(clippy::cast_precision_loss)] // a small node index
                a.push_node(&[i as Real, 0.0]).unwrap();
            }
            let mut c = a.connectivity_mut(SEG, G);
            for s in segs {
                c.push_tuple(s);
            }
        }
        m
    }

    /// One recorded application, in the order of `apply_neumann_recorded`:
    /// segment `e` receives `v` (two nodes × two components).
    fn apply(st: &mut NeumannStore, mesh: &Mesh, dual: &mut [Real], rows: &[(Idx, [Real; 4])]) {
        let c = vec![BlockContribution {
            ty: SEG,
            ghost: G,
            ids: rows.iter().map(|r| r.0).collect(),
            values: Array::from_vec(rows.iter().flat_map(|r| r.1).collect(), 4).unwrap(),
        }];
        st.check_contributions(mesh, &c).unwrap();
        st.forget_foreign_writes(mesh, dual).unwrap();
        st.add_contributions(mesh, c).unwrap();
        let conn = mesh.connectivity(SEG, G).unwrap();
        for (e, v) in rows {
            for k in 0..2 {
                let n = usize::try_from(conn.tuple(usize::try_from(*e).unwrap())[k]).unwrap();
                for j in 0..2 {
                    dual[2 * n + j] += v[2 * k + j];
                }
            }
        }
        st.snapshot(dual);
    }

    /// Σ of the recorded entries at the node holding each, every
    /// block.
    fn record_sum(st: &NeumannStore, mesh: &Mesh) -> Vec<Real> {
        let mut sum = vec![0.0; 2 * mesh.nb_nodes()];
        for ty in [SEG, TRI] {
            let (Ok(b), Ok(h)) = (st.forces().get(ty, G), st.holders().get(ty, G)) else {
                continue;
            };
            for (entry, &n) in b.as_slice().as_chunks::<2>().0.iter().zip(h.as_slice()) {
                if entry.iter().all(|v| *v == 0.0) {
                    continue;
                }
                let n = usize::try_from(n).expect("a loaded entry has its node");
                for j in 0..2 {
                    sum[2 * n + j] += entry[j];
                }
            }
        }
        sum
    }

    /// Double node `old` into a new node and reconnect segment `e` (local
    /// node `k`) to it; returns the new node.
    fn double(
        mesh: &mut Mesh,
        dual: &mut Vec<Real>,
        st: &mut NeumannStore,
        e: usize,
        k: usize,
    ) -> Idx {
        let n = {
            let mut a = MeshAccessor::new(mesh);
            let n = a.push_node(&[0.5, 1.0]).unwrap();
            a.connectivity_mut(SEG, G).tuple_mut(e)[k] = n;
            n
        };
        dual.resize(2 * mesh.nb_nodes(), 0.0);
        st.on_nodes_added(mesh.nb_nodes());
        n
    }

    /// A chained pair list (node 4 doubles node 3, which
    /// doubles node 1 in the same event) moves from the event's original
    /// node 1, whose dual value holds the load. The inserter never produces
    /// a chain; the mutant "move from the direct parent" fails here (node 3
    /// would go negative and node 1 keep the moved load).
    #[test]
    fn a_chained_pair_moves_from_the_event_original() {
        let mut mesh = bar(3, &[[0, 1], [1, 2]]);
        let mut dual = vec![0.0; 6];
        let mut st = NeumannStore::new(&dual, 2).unwrap();
        apply(
            &mut st,
            &mesh,
            &mut dual,
            &[(0, [1., 2., 3., 4.]), (1, [5., 6., 7., 8.])],
        );
        assert_eq!(dual, [1., 2., 8., 10., 7., 8.]);
        {
            let mut a = MeshAccessor::new(&mut mesh);
            assert_eq!(a.push_node(&[1.0, 1.0]).unwrap(), 3);
            assert_eq!(a.push_node(&[1.0, 2.0]).unwrap(), 4);
            a.connectivity_mut(SEG, G).tuple_mut(1)[0] = 4;
        }
        dual.resize(10, 0.0);
        st.on_nodes_added(5);
        let r = st
            .move_doubled_nodes(&mesh, &mut dual, &[3, 4], &[1, 3])
            .unwrap();
        assert_eq!(
            (r.pairs, r.moved, r.forgotten, r.moved_from_written_nodes),
            (2, 1, 0, 0)
        );
        assert_eq!(dual, [1., 2., 3., 4., 7., 8., 0., 0., 5., 6.]);
        assert_eq!(record_sum(&st, &mesh), dual);
        // a cycle in the pairs is refused, nothing changed
        let before = dual.clone();
        assert!(
            st.move_doubled_nodes(&mesh, &mut dual, &[3, 4], &[4, 3])
                .is_err()
        );
        assert_eq!(dual, before);
    }

    /// A reset forgets the entries at the zeroed node
    /// only. Segment 0 with node 0 zeroed and loaded again keeps its entry at
    /// node 1, so the external force stays the sum of the records exactly
    /// (the mutant "forget the whole row" leaves node 1 short of 3, 4).
    #[test]
    fn a_reset_forgets_the_entries_of_the_zeroed_node_only() {
        let mesh = bar(3, &[[0, 1], [1, 2]]);
        let mut dual = vec![0.0; 6];
        let mut st = NeumannStore::new(&dual, 2).unwrap();
        apply(
            &mut st,
            &mesh,
            &mut dual,
            &[(0, [1., 2., 3., 4.]), (1, [5., 6., 7., 8.])],
        );
        dual[0] = 0.0;
        dual[1] = 0.0;
        apply(&mut st, &mesh, &mut dual, &[(0, [1., 2., 3., 4.])]);
        let b = st.forces().get(SEG, G).unwrap();
        assert_eq!(b.tuple(0), [1., 2., 6., 8.], "the entry at node 1 is kept");
        assert_eq!(
            b.tuple(1),
            [5., 6., 7., 8.],
            "the other element is untouched"
        );
        assert_eq!(dual, [1., 2., 11., 14., 7., 8.]);
        assert_eq!(record_sum(&st, &mesh), dual);
        let s = st.stats();
        assert_eq!(
            (s.foreign_writes, s.reset_nodes, s.forgotten_entries),
            (1, 1, 1)
        );

        // "zero everything, apply again" (unchanged): the last application only
        dual.fill(0.0);
        apply(
            &mut st,
            &mesh,
            &mut dual,
            &[(0, [1., 2., 3., 4.]), (1, [5., 6., 7., 8.])],
        );
        let b = st.forces().get(SEG, G).unwrap();
        assert_eq!(
            (b.tuple(0), b.tuple(1)),
            ([1., 2., 3., 4.].as_slice(), [5., 6., 7., 8.].as_slice())
        );
        assert_eq!(record_sum(&st, &mesh), dual);
        assert_eq!(st.stats().forgotten_entries, 1 + 4);
    }

    /// D-108: a node written (non-zero) by another path
    /// while it holds a record is marked; a move that takes a record from it
    /// is counted and prints one warning. The move itself is unchanged: the
    /// unscaled record moves in full.
    #[test]
    fn a_move_from_a_written_node_is_counted_and_warned() {
        let mut mesh = bar(3, &[[0, 1], [1, 2]]);
        let mut dual = vec![0.0; 6];
        let mut st = NeumannStore::new(&dual, 2).unwrap();
        apply(
            &mut st,
            &mesh,
            &mut dual,
            &[(0, [1., 2., 3., 4.]), (1, [5., 6., 7., 8.])],
        );
        // a load ramp written into the array
        for v in &mut dual {
            *v *= 0.5;
        }
        let n = double(&mut mesh, &mut dual, &mut st, 1, 0);
        let (r, out) = capture(DebugLevel::WARNING, || {
            st.move_doubled_nodes(&mesh, &mut dual, &[n], &[1]).unwrap()
        });
        assert_eq!((r.moved, r.moved_from_written_nodes), (1, 1));
        assert_eq!(st.stats().moved_from_written_nodes, 1);
        assert_eq!(st.stats().foreign_writes, 3);
        assert!(
            st.is_written(1) && !st.is_written(3),
            "the new copy is not marked"
        );
        assert_eq!(
            out.matches("/!\\").count(),
            1,
            "one warning per move: {out}"
        );
        assert!(
            out.contains("1 recorded Neumann contribution(s) moved from 1 doubled node(s)")
                && out.contains("doubled_node_keeps_external_force"),
            "{out}"
        );
        // the move is the corrected default's, unchanged: the record moves in full
        assert_eq!(dual, [0.5, 1., -1., -1., 3.5, 4., 5., 6.]);
    }

    /// A node reset just before a doubling: the entries of the elements that
    /// now use the new copy were recorded at the original, whose load is
    /// gone, so they are forgotten too and nothing moves (before, the move
    /// put +c on the new copy and −c on the zeroed original).
    #[test]
    fn a_reset_just_before_a_doubling_moves_nothing() {
        let mut mesh = bar(3, &[[0, 1], [1, 2]]);
        let mut dual = vec![0.0; 6];
        let mut st = NeumannStore::new(&dual, 2).unwrap();
        apply(
            &mut st,
            &mesh,
            &mut dual,
            &[(0, [1., 2., 3., 4.]), (1, [5., 6., 7., 8.])],
        );
        dual[2] = 0.0;
        dual[3] = 0.0;
        let n = double(&mut mesh, &mut dual, &mut st, 1, 0);
        let r = st.move_doubled_nodes(&mesh, &mut dual, &[n], &[1]).unwrap();
        assert_eq!((r.moved, r.forgotten), (0, 2), "{r:?}");
        assert_eq!(dual, [1., 2., 0., 0., 7., 8., 0., 0.]);
        assert_eq!(record_sum(&st, &mesh), dual);
    }

    /// A node written just before a doubling that moves every one of its
    /// records (no element keeps a record at the original) is marked all the
    /// same: its entries are found at the new copy.
    #[test]
    fn a_written_node_whose_records_all_move_is_counted() {
        let mut mesh = bar(2, &[[0, 1]]);
        let mut dual = vec![0.0; 4];
        let mut st = NeumannStore::new(&dual, 2).unwrap();
        apply(&mut st, &mesh, &mut dual, &[(0, [1., 2., 3., 4.])]);
        dual[2] *= 0.1;
        let n = double(&mut mesh, &mut dual, &mut st, 0, 1);
        let (r, out) = capture(DebugLevel::WARNING, || {
            st.move_doubled_nodes(&mesh, &mut dual, &[n], &[1]).unwrap()
        });
        assert_eq!((r.moved, r.moved_from_written_nodes), (1, 1), "{r:?}");
        assert!(st.is_written(1));
        assert_eq!(out.matches("/!\\").count(), 1, "{out}");
    }

    /// The mark needs a record at the time of the write: a force written
    /// before the node's first record, or a reset after the write, moves
    /// without a count or a warning.
    #[test]
    fn a_write_before_the_record_or_a_reset_is_not_counted() {
        // a force at node 1 written before any record there
        let mut mesh = bar(3, &[[0, 1], [1, 2]]);
        let mut dual = vec![0.0; 6];
        let mut st = NeumannStore::new(&dual, 2).unwrap();
        apply(&mut st, &mesh, &mut dual, &[(1, [0., 0., 7., 8.])]);
        dual[2] += 100.0;
        apply(&mut st, &mesh, &mut dual, &[(0, [1., 2., 3., 4.])]);
        assert_eq!(st.stats().foreign_writes, 1);
        assert!(!st.is_written(1));
        let n = double(&mut mesh, &mut dual, &mut st, 0, 1);
        let (r, out) = capture(DebugLevel::WARNING, || {
            st.move_doubled_nodes(&mesh, &mut dual, &[n], &[1]).unwrap()
        });
        assert_eq!((r.moved, r.moved_from_written_nodes), (1, 0));
        assert!(out.is_empty(), "{out}");
        assert_eq!(
            dual,
            [1., 2., 100., 0., 7., 8., 3., 4.],
            "the user force stays"
        );

        // written after the record, then reset and loaded again: not counted
        let mut mesh = bar(3, &[[0, 1], [1, 2]]);
        let mut dual = vec![0.0; 6];
        let mut st = NeumannStore::new(&dual, 2).unwrap();
        let loads = [(0, [1., 2., 3., 4.]), (1, [5., 6., 7., 8.])];
        apply(&mut st, &mesh, &mut dual, &loads);
        dual[2] *= 2.0;
        apply(&mut st, &mesh, &mut dual, &[]);
        assert!(st.is_written(1));
        dual.fill(0.0);
        apply(&mut st, &mesh, &mut dual, &loads);
        assert!(!st.is_written(1), "a reset clears the mark");
        let n = double(&mut mesh, &mut dual, &mut st, 1, 0);
        let (r, out) = capture(DebugLevel::WARNING, || {
            st.move_doubled_nodes(&mesh, &mut dual, &[n], &[1]).unwrap()
        });
        assert_eq!((r.moved, r.moved_from_written_nodes), (1, 0));
        assert!(out.is_empty(), "{out}");
    }

    /// The marks follow a node compaction.
    #[test]
    fn the_marks_follow_a_node_compaction() {
        let mesh = bar(3, &[[0, 1], [1, 2]]);
        let mut dual = vec![0.0; 6];
        let mut st = NeumannStore::new(&dual, 2).unwrap();
        apply(
            &mut st,
            &mesh,
            &mut dual,
            &[(0, [1., 2., 3., 4.]), (1, [5., 6., 7., 8.])],
        );
        dual[4] = -1.0;
        apply(&mut st, &mesh, &mut dual, &[]);
        assert!(st.is_written(2) && !st.is_written(1));
        // node 0 removed: 1 -> 0, 2 -> 1
        st.check_nodes_removed(&[INVALID_IDX, 0, 1]).unwrap();
        st.on_nodes_removed(&[INVALID_IDX, 0, 1]).unwrap();
        assert!(st.is_written(1) && !st.is_written(0) && !st.is_written(2));
    }

    /// An `ElementsChanged` numbering that moves an
    /// element of a recorded block is accepted only when the lists state the
    /// same move; a renumbering with empty lists is refused (recoverable).
    /// The record of a substituted element must be able to follow the
    /// substitute's row by node identity.
    #[test]
    fn a_change_numbering_is_followed_only_through_the_lists() {
        let mut mesh = bar(3, &[[0, 1], [1, 2]]);
        let mut dual = vec![0.0; 6];
        let mut st = NeumannStore::new(&dual, 2).unwrap();
        apply(&mut st, &mesh, &mut dual, &[(0, [1., 2., 3., 4.])]);
        let numbering = |ty: ElementType, num: Vec<Idx>| {
            let mut m = ElementTypeMapArray::new("changed");
            m.as_map_mut()
                .insert(ty, G, Array::from_vec(num, 1).unwrap())
                .unwrap();
            m
        };
        let seg = |i: Idx| Element::new(SEG, i, G);
        let swap = numbering(SEG, vec![1, 0]);
        let e = st
            .check_elements_changed(&mesh, &[], &[], &swap)
            .unwrap_err();
        assert!(matches!(e, Error::Unsupported(_)) && !e.is_fatal(), "{e:?}");
        assert!(st.on_elements_changed(&mesh, &[], &[], &swap).is_err());
        let removing = numbering(SEG, vec![INVALID_IDX, 0]);
        assert!(
            st.check_elements_changed(&mesh, &[seg(0)], &[seg(1)], &removing)
                .is_err()
        );
        // The lists state the swap but the rows were not swapped: the
        // record of segment 0 (nodes 0, 1) cannot follow segment 1 (1, 2)
        let e = st
            .check_elements_changed(&mesh, &[seg(0), seg(1)], &[seg(1), seg(0)], &swap)
            .unwrap_err();
        assert!(
            matches!(e, Error::Unsupported(_)) && format!("{e}").contains("UD-1280"),
            "{e:?}"
        );
        // the rows swapped with the identities: followed
        {
            let mut a = MeshAccessor::new(&mut mesh);
            let mut c = a.connectivity_mut(SEG, G);
            c.tuple_mut(0).copy_from_slice(&[1, 2]);
            c.tuple_mut(1).copy_from_slice(&[0, 1]);
        }
        st.check_elements_changed(&mesh, &[seg(0), seg(1)], &[seg(1), seg(0)], &swap)
            .unwrap();
        // an identity numbering, or a numbering of a block without a record
        st.check_elements_changed(&mesh, &[], &[], &numbering(SEG, vec![0, 1]))
            .unwrap();
        st.check_elements_changed(&mesh, &[], &[], &numbering(TRI, vec![1, 0]))
            .unwrap();
        // a record never moves to an element of another layout
        let tri = Element::new(TRI, 0, G);
        let e = st
            .check_elements_changed(&mesh, &[seg(0)], &[tri], &numbering(SEG, vec![0, 1]))
            .unwrap_err();
        assert!(matches!(e, Error::Unsupported(_)), "{e:?}");
        st.on_elements_changed(&mesh, &[seg(0), seg(1)], &[seg(1), seg(0)], &swap)
            .unwrap();
        let b = st.forces().get(SEG, G).unwrap();
        assert_eq!(
            (b.tuple(0), b.tuple(1)),
            ([0.; 4].as_slice(), [1., 2., 3., 4.].as_slice())
        );
        assert_eq!(st.holders().get(SEG, G).unwrap().tuple(1), [0, 1]);
        assert_eq!(record_sum(&st, &mesh), dual);
    }

    // ----: the records follow rewritten rows by node identity --------

    /// Distinct nodal shares of one triangle: node 0 → s0, 1 → s1, 2 → s2.
    const S: [[Real; 2]; 3] = [[1., 10.], [2., 20.], [3., 30.]];

    /// A 2-D mesh of triangles (`nb_nodes` nodes on a line, the geometry
    /// plays no part in the store).
    fn tris(nb_nodes: usize, rows: &[[Idx; 3]]) -> Mesh {
        let mut m = Mesh::new(2, "tris").unwrap();
        {
            let mut a = MeshAccessor::new(&mut m);
            for i in 0..nb_nodes {
                #[allow(clippy::cast_precision_loss)] // a small node index
                a.push_node(&[i as Real, 1.0]).unwrap();
            }
            let mut c = a.connectivity_mut(TRI, G);
            for r in rows {
                c.push_tuple(r);
            }
        }
        m
    }

    /// One recorded application on triangle `e`: `share(n)` at each node `n`
    /// of its current row (the order of `apply_neumann_recorded`).
    fn apply_tri(
        st: &mut NeumannStore,
        mesh: &Mesh,
        dual: &mut [Real],
        e: usize,
        share: impl Fn(usize) -> [Real; 2],
    ) -> Result<()> {
        let row: Vec<usize> = mesh
            .connectivity(TRI, G)?
            .tuple(e)
            .iter()
            .map(|&n| usize::try_from(n).unwrap())
            .collect();
        let values: Vec<Real> = row.iter().flat_map(|&n| share(n)).collect();
        let c = vec![BlockContribution {
            ty: TRI,
            ghost: G,
            ids: vec![Idx::try_from(e).unwrap()],
            values: Array::from_vec(values, 6)?,
        }];
        st.check_contributions(mesh, &c)?;
        st.forget_foreign_writes(mesh, dual)?;
        st.add_contributions(mesh, c)?;
        for &n in &row {
            let v = share(n);
            dual[2 * n] += v[0];
            dual[2 * n + 1] += v[1];
        }
        st.snapshot(dual);
        Ok(())
    }

    /// Triangle 0 = [0, 1, 2] loaded with the distinct shares `S`; `extra`
    /// nodes appended (unconnected) for the copies of a test.
    fn loaded_triangle(extra: usize) -> (Mesh, Vec<Real>, NeumannStore) {
        let mesh = tris(3 + extra, &[[0, 1, 2]]);
        let mut dual = vec![0.0; 2 * (3 + extra)];
        let mut st = NeumannStore::new(&dual, 2).unwrap();
        apply_tri(&mut st, &mesh, &mut dual, 0, |n| S[n]).unwrap();
        (mesh, dual, st)
    }

    fn set_row(mesh: &mut Mesh, e: usize, row: [Idx; 3]) {
        let mut a = MeshAccessor::new(mesh);
        a.connectivity_mut(TRI, G)
            .tuple_mut(e)
            .copy_from_slice(&row);
    }

    /// The brief's case: the row [0, 1, 2] recorded with
    /// the shares (s0, s1, s2) is rewritten as [1, 2, 0′] (a rotation, and
    /// the copy 3 of node 0 in node 0's place), as the insertion copies the
    /// facet row before its node event. The move takes s0 from node 0 to
    /// node 3, not s2 (the share recorded in the slot the copy now
    /// occupies: before the fix node 3 got (3, 30) and node 0 kept
    /// (−2, −20)). Both orders of the model are covered: the store told of
    /// the copies by the node event first, or only by the move.
    #[test]
    fn a_rotated_row_moves_the_share_of_the_doubled_node() {
        for announced in [true, false] {
            let (mut mesh, mut dual, mut st) = loaded_triangle(1);
            set_row(&mut mesh, 0, [1, 2, 3]);
            st.on_nodes_added(4);
            if announced {
                st.check_nodes_doubled(&mesh, None, &[3], &[0]).unwrap();
                st.on_nodes_doubled(&mesh, None, &[3], &[0]).unwrap();
                // the record already follows the row, the load is still at 0
                let b = st.forces().get(TRI, G).unwrap();
                assert_eq!(b.tuple(0), [2., 20., 3., 30., 1., 10.]);
                assert_eq!(st.holders().get(TRI, G).unwrap().tuple(0), [1, 2, 0]);
            }
            let r = st.move_doubled_nodes(&mesh, &mut dual, &[3], &[0]).unwrap();
            assert_eq!((r.moved, r.forgotten), (1, 0), "{r:?}");
            assert_eq!(
                dual,
                [0., 0., 2., 20., 3., 30., 1., 10.],
                "node 3 carries s0 (announced {announced})"
            );
            assert_eq!(st.holders().get(TRI, G).unwrap().tuple(0), [1, 2, 3]);
            assert_eq!(record_sum(&st, &mesh), dual);
        }
    }

    /// A reset after a rotation forgets the entry of the zeroed node (s1),
    /// not the entry recorded in the slot that node now occupies (s0, the
    /// result before the fix).
    #[test]
    fn a_reset_after_a_rotation_forgets_the_entry_of_the_zeroed_node() {
        let (mut mesh, mut dual, mut st) = loaded_triangle(0);
        set_row(&mut mesh, 0, [1, 2, 0]);
        dual[2] = 0.0;
        dual[3] = 0.0;
        assert_eq!(st.forget_foreign_writes(&mesh, &dual).unwrap(), 1);
        let b = st.forces().get(TRI, G).unwrap();
        assert_eq!(b.tuple(0), [0., 0., 3., 30., 1., 10.], "s1 forgotten");
        assert_eq!(record_sum(&st, &mesh), dual);
        assert_eq!(dual, [1., 10., 0., 0., 3., 30.]);
    }

    /// An application after a rotation adds each node's share to that
    /// node's entry (before the fix the second application was added slot
    /// by slot to the entries of other nodes).
    #[test]
    fn an_application_after_a_rotation_adds_to_the_entry_of_each_node() {
        let (mut mesh, mut dual, mut st) = loaded_triangle(0);
        set_row(&mut mesh, 0, [2, 0, 1]);
        apply_tri(&mut st, &mesh, &mut dual, 0, |n| S[n]).unwrap();
        let b = st.forces().get(TRI, G).unwrap();
        assert_eq!(b.tuple(0), [6., 60., 2., 20., 4., 40.]);
        assert_eq!(st.holders().get(TRI, G).unwrap().tuple(0), [2, 0, 1]);
        assert_eq!(record_sum(&st, &mesh), dual);
        assert_eq!(st.stats().released_entries, 0);
    }

    /// A row that is neither a permutation of the recorded nodes nor them
    /// with copies doubled from them is refused (`Error::Unsupported`,
    /// recoverable), by the check stage of a node event and by any later
    /// operation, with nothing changed; zeroing the external force (a reset
    /// of every node) makes the next application possible again.
    #[test]
    fn a_row_the_record_cannot_follow_is_refused_with_nothing_changed() {
        let (mut mesh, mut dual, mut st) = loaded_triangle(1);
        // node 3 is not a copy of node 0
        set_row(&mut mesh, 0, [1, 2, 3]);
        let before = (
            st.forces().get(TRI, G).unwrap().as_slice().to_vec(),
            st.stats(),
        );
        let e = st.check_nodes_doubled(&mesh, None, &[], &[]).unwrap_err();
        assert!(matches!(e, Error::Unsupported(_)) && !e.is_fatal(), "{e:?}");
        assert!(format!("{e}").contains("UD-1280"), "{e}");
        // a copy of another node does not make it followable either
        assert!(st.check_nodes_doubled(&mesh, None, &[3], &[1]).is_err());
        dual[0] += 5.0; // a foreign write, not absorbed by a refused call
        let e = st.forget_foreign_writes(&mesh, &dual).unwrap_err();
        assert!(matches!(e, Error::Unsupported(_)), "{e:?}");
        let e = apply_tri(&mut st, &mesh, &mut dual, 0, |n| S[n % 3]).unwrap_err();
        assert!(matches!(e, Error::Unsupported(_)), "{e:?}");
        assert_eq!(
            (
                st.forces().get(TRI, G).unwrap().as_slice().to_vec(),
                st.stats()
            ),
            before,
            "nothing changed"
        );
        // zero the external force and apply again: accepted
        dual.fill(0.0);
        apply_tri(&mut st, &mesh, &mut dual, 0, |n| S[n % 3]).unwrap();
        assert_eq!(st.stats().forgotten_entries, 3);
        assert_eq!(record_sum(&st, &mesh), dual);
    }

    /// Upstream's placement (no move): after the doubling the entry of the
    /// element now using the copy stays held by the original (a reset of
    /// the original forgets it); an application that loads the copy
    /// releases it — the old load stays at the original as a value of no
    /// recorded element — and records the new one at the copy.
    #[test]
    fn without_a_move_the_original_keeps_the_entry_until_an_application_releases_it() {
        let (mut mesh, mut dual, mut st) = loaded_triangle(1);
        set_row(&mut mesh, 0, [3, 1, 2]);
        st.on_nodes_added(4);
        dual.resize(8, 0.0);
        st.on_nodes_doubled(&mesh, None, &[3], &[0]).unwrap();
        assert_eq!(st.holders().get(TRI, G).unwrap().tuple(0), [0, 1, 2]);
        assert_eq!(record_sum(&st, &mesh), dual, "the load is held at 0");
        let mut reset = dual.clone();
        reset[0] = 0.0;
        reset[1] = 0.0;
        let mut r = st.clone();
        assert_eq!(r.forget_foreign_writes(&mesh, &reset).unwrap(), 1);
        assert_eq!(record_sum(&r, &mesh), reset);
        // the application: the copy gets its share, s0 stays at node 0
        apply_tri(&mut st, &mesh, &mut dual, 0, |n| S[n % 3]).unwrap();
        assert_eq!(st.stats().released_entries, 1);
        assert_eq!(st.holders().get(TRI, G).unwrap().tuple(0), [3, 1, 2]);
        assert_eq!(dual, [1., 10., 4., 40., 6., 60., 1., 10.]);
        let mut held = record_sum(&st, &mesh);
        held[0] += 1.0;
        held[1] += 10.0;
        assert_eq!(held, dual, "records + the released s0 at node 0");
    }

    /// The copies are remembered across node events: a copy (4) of a copy
    /// (3) of the recorded node 0, named by a row rewritten at a later event
    /// than the first doubling, is followed, and the move brings the entry
    /// from the node holding it to the copy the element uses.
    #[test]
    fn a_copy_of_a_copy_is_followed_across_node_events() {
        let (mut mesh, mut dual, mut st) = loaded_triangle(2);
        // event 1: 3 doubles 0, the element's row not rewritten yet
        st.on_nodes_added(5);
        st.on_nodes_doubled(&mesh, None, &[3], &[0]).unwrap();
        // event 2: 4 doubles 3, the row rewritten as a rotation naming 4
        set_row(&mut mesh, 0, [2, 4, 1]);
        st.check_nodes_doubled(&mesh, None, &[4], &[3]).unwrap();
        let r = st.move_doubled_nodes(&mesh, &mut dual, &[4], &[3]).unwrap();
        assert_eq!(r.moved, 1, "{r:?}");
        assert_eq!(dual, [0., 0., 2., 20., 3., 30., 0., 0., 1., 10.]);
        assert_eq!(record_sum(&st, &mesh), dual);
        // a new node announced with another original is refused
        assert!(st.move_doubled_nodes(&mesh, &mut dual, &[4], &[1]).is_err());
    }

    /// The nodes holding the records and the copies' originals follow a node
    /// compaction.
    #[test]
    fn the_nodes_of_the_records_follow_a_node_compaction() {
        // nodes 0, 1 unused; triangle [2, 3, 4] with the shares S; node 5 a
        // copy of node 2 (the row not rewritten yet)
        let mesh = tris(6, &[[2, 3, 4]]);
        let mut dual = vec![0.0; 12];
        let mut st = NeumannStore::new(&dual, 2).unwrap();
        apply_tri(&mut st, &mesh, &mut dual, 0, |n| S[n - 2]).unwrap();
        st.on_nodes_doubled(&mesh, None, &[5], &[2]).unwrap();
        // nodes 0 and 1 removed: 2..=5 -> 0..=3
        let numbering = [INVALID_IDX, INVALID_IDX, 0, 1, 2, 3];
        st.check_nodes_removed(&numbering).unwrap();
        st.on_nodes_removed(&numbering).unwrap();
        assert_eq!(st.holders().get(TRI, G).unwrap().tuple(0), [0, 1, 2]);
        let mut dual = dual[4..].to_vec();
        // the row rewritten as a rotation naming the copy (now node 3)
        let mesh = tris(4, &[[1, 2, 3]]);
        let r = st.move_doubled_nodes(&mesh, &mut dual, &[], &[]).unwrap();
        assert_eq!(r.moved, 1, "the entry follows the copy 3 of node 0: {r:?}");
        assert_eq!(dual, [0., 0., 2., 20., 3., 30., 1., 10.]);
        assert_eq!(record_sum(&st, &mesh), dual);
    }
}
