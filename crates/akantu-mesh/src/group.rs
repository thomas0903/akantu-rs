// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/mesh/{group_manager.hh, group_manager.cc,
// group_manager_inline_impl.hh, element_group.hh, element_group.cc,
// element_group_inline_impl.hh, node_group.hh, node_group.cc,
// node_group_inline_impl.hh, mesh_filter.hh}.

//! Element and node groups (upstream `GroupManager`, `ElementGroup`,
//! `NodeGroup`).
//!
//! Groups are created by mesh readers from physical names
//! (`createGroupsFromMeshData`), by users, by surface selectors, by the
//! clustering algorithms (`crate::clusters`) and by the cohesive inserter.
//! They are plain index lists, kept consistent with the mesh by the mesh
//! transactions (cohesive node events rebuild the element groups' node
//! lists; element and node removal renumber them — upstream does not,
//! Report, UD-117).
//!
//! **Identity.** As upstream, every node group is owned
//! by the [`GroupManager`] (upstream `node_groups`: `std::map<ID,
//! std::unique_ptr<NodeGroup>>`) and an element group *refers* to one node
//! group (upstream `ElementGroup::node_group` is a `NodeGroup &`). Here the
//! reference is a stable handle ([`NodeGroupId`]) into manager-owned
//! storage; element groups are reached through views
//! ([`ElementGroupRef`], [`ElementGroupMut`]) that resolve it, so the
//! element group's nodes and the node group can never diverge:
//!
//! * `createElementGroup(name, …)` creates the node group `<name>_nodes`
//!   and refers to it ([`GroupManager::create_element_group`]);
//! * `createElementGroup(name, dim, node_group)` refers to an **existing**
//!   node group, **shared** ([`GroupManager::create_element_group_on_node_group`],
//!   used by `createElementGroupFromNodeGroup` and
//!   `createFilteredElementGroup`): edits through either name (node group
//!   or element group) are seen by both, and several element groups may
//!   share one node group;
//! * renaming a node group keeps every reference (the element group sees
//!   the new name); renaming an element group keeps its node group's name
//!   (upstream `renameGroup`);
//! * destroying (or replacing) a node group that an element group still
//!   refers to is **refused** (upstream erases the `unique_ptr` and leaves
//!   a dangling reference: undefined behaviour, D-003 category A, UD-125);
//!   so is `destroyElementGroup(name, true)` when another element group
//!   shares the node group;
//! * [`GroupManager::copy_element_group`] copies the source's nodes into the
//!   copy's own node group (upstream leaves it empty: D-003 category B,
//!   UD-124); [`GroupManager::copy_node_group`] copies a node list. Copies
//!   are always explicit, never the result of creating a group *on* a node
//!   group.
//!
//! **Object identity.** Every element group and node
//! group carries a process-unique identity ([`ElementGroup::uid`],
//! [`NodeGroup::uid`]) that is never reused: a renamed group keeps it, a
//! destroyed, replaced or re-created group of the same name gets a new one
//! (upstream: a new `ElementGroup` / `NodeGroup` object). State that upstream
//! attaches to the group object, such as its default dumper
//! (`akantu_io::dumper::group::MeshDumpers`), is keyed by this identity.

use std::collections::BTreeMap;
use std::ops::{Deref, DerefMut};
use std::sync::atomic::{AtomicU64, Ordering};

use akantu_core::array_shared::{ArrayRef, SliceRef};
use akantu_core::{
    Array, Element, ElementType, Error, GhostType, Idx, Result, SharedArray,
    SharedElementTypeMapArray, TypeFilter, try_to_usize,
};

use crate::data::MeshData;
use crate::utils::NodeToElements;

/// Upstream `_all_dimensions` as a group dimension: a group created with
/// it keeps it (`addDimension` takes the maximum) and
/// `fillFromNodeGroup` then accepts elements of every dimension.
pub const ALL_DIMENSIONS: usize = usize::MAX;

/// Source of group identities (see the module docs): process-wide, so that
/// identities of different meshes never coincide; values carry no meaning
/// beyond equality.
static NEXT_GROUP_UID: AtomicU64 = AtomicU64::new(1);

fn next_group_uid() -> u64 {
    NEXT_GROUP_UID.fetch_add(1, Ordering::Relaxed)
}

/// A set of nodes (sorted, unique after [`NodeGroup::optimize`]).
///
/// Equality and `Debug` show the name and the nodes, not the identity (so
/// that `Mesh::state_fingerprint` compares contents).
#[derive(Clone)]
pub struct NodeGroup {
    /// Name (changed only by [`GroupManager::rename_node_group`], which
    /// keeps the manager's index consistent).
    name: String,
    /// Node indices (one component). Exportable (D-047): Python
    /// `NodeGroup.getNodes()` views alias it.
    nodes: SharedArray<Idx>,
    /// Object identity (module docs; kept by clones and renames).
    uid: u64,
}

// The values are formatted and compared explicitly: the `Debug` of a
// `SharedArray` is metadata only and its `PartialEq` also compares the id
// (storage migration guide, P18); the identity is not part of the state.
impl std::fmt::Debug for NodeGroup {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NodeGroup")
            .field("name", &self.name)
            .field("nodes", &self.nodes.borrow().as_slice())
            .finish()
    }
}

impl PartialEq for NodeGroup {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name && self.nodes.borrow().as_slice() == other.nodes.borrow().as_slice()
    }
}

impl Eq for NodeGroup {}

impl Default for NodeGroup {
    fn default() -> Self {
        Self::new(String::new())
    }
}

/// Sort `v` and move the unique values to its front (`Vec::dedup` on a
/// sorted vector); returns the number of unique values.
fn sort_unique(v: &mut [Idx]) -> usize {
    v.sort_unstable();
    let mut n = 0;
    for i in 0..v.len() {
        if n == 0 || v[i] != v[n - 1] {
            v[n] = v[i];
            n += 1;
        }
    }
    n
}

fn node_array(nodes: Vec<Idx>) -> SharedArray<Idx> {
    // one component: `from_vec` cannot fail
    SharedArray::new(Array::from_vec(nodes, 1).unwrap_or_else(|_| Array::new(1)))
}

impl NodeGroup {
    /// Empty group (a new identity).
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            nodes: node_array(Vec::new()),
            uid: next_group_uid(),
        }
    }

    /// Group with the given nodes (in that order).
    #[must_use]
    pub fn with_nodes(name: impl Into<String>, nodes: Vec<Idx>) -> Self {
        Self {
            name: name.into(),
            nodes: node_array(nodes),
            uid: next_group_uid(),
        }
    }

    /// Node indices (upstream `getNodes()`; a guard of the exportable
    /// array, ARCHITECTURE.md, "Shared storage and its aliasing rules").
    ///
    /// # Panics
    /// If the list is exported to a foreign runtime and no native section
    /// of it is active (protocol violation, [`SharedArray::borrow`]).
    #[must_use]
    #[track_caller]
    pub fn nodes(&self) -> ArrayRef<'_, Idx> {
        self.nodes.borrow()
    }

    /// Copy of the node indices.
    #[must_use]
    pub fn node_list(&self) -> Vec<Idx> {
        self.nodes.borrow().as_slice().to_vec()
    }

    /// Replace the node list (an exported list is retired: its views go
    /// stale, ARCHITECTURE.md, "Shared storage and its aliasing rules").
    pub fn set_nodes(&mut self, nodes: Vec<Idx>) {
        let mut a = Array::from_vec(nodes, 1).unwrap_or_else(|_| Array::new(1));
        a.set_id(self.nodes.id().to_owned());
        self.nodes.borrow_mut().replace(a);
    }

    /// Append nodes (no duplicate check).
    pub fn extend_nodes(&mut self, nodes: &[Idx]) {
        let mut w = self.nodes.borrow_mut();
        for &n in nodes {
            w.push_value(n);
        }
    }

    /// The exportable node list (fallible guards: `read()`).
    #[must_use]
    pub fn nodes_shared(&self) -> &SharedArray<Idx> {
        &self.nodes
    }

    /// The exportable node list (foreign views, [`crate::exports`]).
    pub(crate) fn nodes_shared_mut(&mut self) -> &mut SharedArray<Idx> {
        &mut self.nodes
    }

    /// Name (upstream `getName`).
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Object identity: unique in the process, kept by a rename, never
    /// reused (a replaced or re-created group of the same name gets a new
    /// one; module docs).
    #[must_use]
    pub fn uid(&self) -> u64 {
        self.uid
    }

    /// Sort and remove duplicates (upstream `optimize`).
    pub fn optimize(&mut self) {
        let mut w = self.nodes.borrow_mut();
        let n = sort_unique(w.as_mut_slice());
        if n < w.len() {
            w.truncate(n);
        }
    }

    /// Append `node` (upstream `add`); with `check_for_duplicate` an
    /// existing node is not added again. Returns the node's position.
    pub fn add(&mut self, node: Idx, check_for_duplicate: bool) -> usize {
        if check_for_duplicate && let Some(p) = self.find(node) {
            return p;
        }
        self.nodes.borrow_mut().push_value(node);
        self.nodes.len() - 1
    }

    /// Remove every occurrence of `node` (upstream `remove`: `Array::erase`
    /// moves the last entry into the hole and the loop re-examines it).
    pub fn remove(&mut self, node: Idx) {
        let mut w = self.nodes.borrow_mut();
        let mut i = 0;
        while i < w.len() {
            if w.as_slice()[i] == node {
                // in bounds: `i < len`
                let _ = w.swap_remove_tuple(i);
            } else {
                i += 1;
            }
        }
    }

    /// Keep only the nodes accepted by `filter` (upstream `applyNodeFilter`,
    /// same swap-remove order as [`NodeGroup::remove`]).
    pub fn apply_node_filter(&mut self, mut filter: impl FnMut(Idx) -> bool) {
        let mut w = self.nodes.borrow_mut();
        let mut i = 0;
        while i < w.len() {
            if filter(w.as_slice()[i]) {
                i += 1;
            } else {
                // in bounds: `i < len`
                let _ = w.swap_remove_tuple(i);
            }
        }
    }

    /// Append the nodes of `other` then [`NodeGroup::optimize`] (upstream
    /// `append`).
    pub fn append(&mut self, other: &NodeGroup) {
        let other = other.node_list();
        self.extend_nodes(&other);
        self.optimize();
    }

    /// Position of `node` (upstream `find`, `-1` → `None`).
    #[must_use]
    pub fn find(&self, node: Idx) -> Option<usize> {
        self.nodes
            .borrow()
            .as_slice()
            .iter()
            .position(|&n| n == node)
    }

    /// Remove every node (upstream `clear`).
    pub fn clear(&mut self) {
        self.nodes.borrow_mut().clear();
    }

    /// Number of nodes.
    #[must_use]
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// `true` if empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    fn apply_node_numbering(&mut self, numbering: &[Idx]) {
        let nodes = self
            .node_list()
            .into_iter()
            .filter_map(|n| {
                let new = *numbering.get(usize::try_from(n).ok()?)?;
                (new >= 0).then_some(new)
            })
            .collect();
        self.set_nodes(nodes);
    }
}

/// Stable handle of a node group inside its [`GroupManager`] (upstream: the
/// `NodeGroup &` an element group holds). Valid while the node group
/// exists; handles are never reused.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NodeGroupId(usize);

/// A set of elements referring to the node group of their nodes. Reached
/// through [`ElementGroupRef`] / [`ElementGroupMut`], which resolve the node
/// group. `Debug` shows the contents, not the identity.
#[derive(Clone)]
pub struct ElementGroup {
    /// Name (changed only by [`GroupManager::rename_element_group`]).
    name: String,
    /// Element indices per `(type, ghost)` (upstream `ElementTypeMapArray<Idx>`).
    /// Exportable (D-047): Python `ElementGroup.getElements()` views alias it.
    pub elements: SharedElementTypeMapArray<Idx>,
    /// Dimension of the elements in the group (upstream `dimension`;
    /// [`ALL_DIMENSIONS`] for `_all_dimensions`).
    pub dimension: usize,
    /// The node group it refers to.
    node_group: NodeGroupId,
    /// Object identity (module docs; kept by clones and renames).
    uid: u64,
}

impl std::fmt::Debug for ElementGroup {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ElementGroup")
            .field("name", &self.name)
            .field("elements", &self.elements)
            .field("dimension", &self.dimension)
            .field("node_group", &self.node_group)
            .finish()
    }
}

impl ElementGroup {
    fn new(name: &str, dimension: usize, node_group: NodeGroupId) -> Self {
        Self {
            name: name.into(),
            elements: SharedElementTypeMapArray::new(format!("group:{name}")),
            dimension,
            node_group,
            uid: next_group_uid(),
        }
    }

    /// Name (upstream `getName`).
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Object identity: unique in the process, kept by a rename, never
    /// reused (a replaced or re-created group of the same name gets a new
    /// one; module docs).
    #[must_use]
    pub fn uid(&self) -> u64 {
        self.uid
    }

    /// Handle of the node group it refers to.
    #[must_use]
    pub fn node_group_id(&self) -> NodeGroupId {
        self.node_group
    }

    /// Elements of `(ty, ghost)`: a guard that dereferences to the list, no
    /// copy; empty if the group has none of that type.
    ///
    /// # Panics
    /// If the block is exported and no native section is active
    /// (protocol violation, [`SharedArray::borrow`]).
    #[must_use]
    #[track_caller]
    pub fn elements_of(&self, ty: ElementType, ghost: GhostType) -> SliceRef<'_, Idx> {
        self.elements
            .shared(ty, ghost)
            .map_or_else(|_| SliceRef::empty(), |a| a.borrow().into_slice())
    }

    /// Present element types of `ghost`, in enum order (upstream
    /// `elementTypes`; blocks may be empty).
    #[must_use]
    pub fn element_types(&self, ghost: GhostType) -> Vec<ElementType> {
        self.elements
            .element_types(TypeFilter::new().ghost(ghost))
            .collect()
    }

    /// Every element of the group: ghost type, then type, then list order.
    /// Does not allocate: each block's guard is held while its elements
    /// are yielded.
    ///
    /// # Panics
    /// If a block is exported and no native section is active (protocol
    /// violation, [`SharedArray::borrow`]).
    pub fn iter_elements(&self) -> impl Iterator<Item = Element> + '_ {
        self.elements
            .as_map()
            .iter_all()
            .flat_map(|(ty, ghost, a)| {
                let list = a.borrow().into_slice();
                (0..list.len()).map(move |i| Element::new(ty, list[i], ghost))
            })
    }

    /// Append an element (upstream `addElement`): the dimension becomes the
    /// maximum of the current one and the element's spatial dimension.
    pub fn add_element(&mut self, el: Element) -> Result<()> {
        let i = el
            .try_idx()
            .ok_or_else(|| Error::invalid("cannot add ElementNull to an element group"))?;
        let (ty, ghost) = (el.ty(), el.ghost_type());
        if !self.elements.exists(ty, ghost) {
            self.elements.alloc(0, 1, ty, ghost, 0)?;
        }
        self.elements
            .get_mut(ty, ghost)?
            .try_push_tuple(&[akantu_core::try_idx(i)?])?;
        self.add_dimension(ty.spatial_dimension());
        Ok(())
    }

    /// Sort and deduplicate every element list (the element part of
    /// upstream `optimize`).
    fn optimize_elements(&mut self) {
        for g in GhostType::ALL {
            for (_, a) in self.elements.as_map_mut().iter_mut(g) {
                let mut w = a.borrow_mut();
                let n = sort_unique(w.as_mut_slice());
                if n < w.len() {
                    w.truncate(n);
                }
            }
        }
    }

    /// Remove every element block (upstream `clear()` = `elements.free()`;
    /// the node group is kept, as upstream).
    pub fn clear(&mut self) {
        self.elements.clear();
    }

    /// Empty the block `(ty, ghost)` if present (upstream `clear(type,
    /// ghost)`).
    ///
    /// # Panics
    /// If the block is exported and no native section is active (protocol
    /// violation, [`SharedArray::borrow_mut`]).
    #[track_caller]
    pub fn clear_type(&mut self, ty: ElementType, ghost: GhostType) {
        if let Some(a) = self.elements.as_map_mut().get_mut(ty, ghost) {
            a.borrow_mut().clear();
        }
    }

    /// `true` if every element block is empty (upstream `empty`).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.elements.total_len() == 0
    }

    /// Upstream `addDimension`: maximum of the current and `dimension`.
    pub fn add_dimension(&mut self, dimension: usize) {
        self.dimension = self.dimension.max(dimension);
    }

    #[track_caller]
    fn apply_element_numbering(&mut self, ty: ElementType, ghost: GhostType, numbering: &[Idx]) {
        if let Some(a) = self.elements.as_map_mut().get_mut(ty, ghost) {
            let mut a = a.borrow_mut();
            let v: Vec<Idx> = a
                .as_slice()
                .iter()
                .filter_map(|&e| {
                    let new = *numbering.get(usize::try_from(e).ok()?)?;
                    (new >= 0).then_some(new)
                })
                .collect();
            if let Ok(mut b) = akantu_core::Array::from_vec(v, 1) {
                b.set_id(a.id().to_owned());
                a.replace(b);
            }
        }
    }
}

/// Shared view of an element group with its (resolved) node group.
#[derive(Clone, Copy, Debug)]
pub struct ElementGroupRef<'a> {
    group: &'a ElementGroup,
    /// The node group the element group refers to (possibly shared).
    pub node_group: &'a NodeGroup,
}

impl<'a> ElementGroupRef<'a> {
    /// Nodes of the group (upstream `getNodeGroup().getNodes()`; a guard,
    /// see [`NodeGroup::nodes`]).
    ///
    /// # Panics
    /// As [`NodeGroup::nodes`] (protocol violation of the exporting code).
    #[must_use]
    #[track_caller]
    pub fn nodes(&self) -> ArrayRef<'a, Idx> {
        self.node_group.nodes.borrow()
    }

    /// Number of nodes (upstream `getNbNodes`).
    #[must_use]
    pub fn nb_nodes(&self) -> usize {
        self.node_group.len()
    }

    /// The underlying element group.
    #[must_use]
    pub fn group(&self) -> &'a ElementGroup {
        self.group
    }

    /// Name of the element group (with the view's lifetime).
    #[must_use]
    pub fn name(&self) -> &'a str {
        &self.group.name
    }

    /// Elements of `(ty, ghost)` (see [`ElementGroup::elements_of`]).
    ///
    /// # Panics
    /// As [`ElementGroup::elements_of`].
    #[must_use]
    #[track_caller]
    pub fn elements_of(&self, ty: ElementType, ghost: GhostType) -> SliceRef<'a, Idx> {
        self.group.elements_of(ty, ghost)
    }
}

impl Deref for ElementGroupRef<'_> {
    type Target = ElementGroup;
    fn deref(&self) -> &ElementGroup {
        self.group
    }
}

/// Mutable view of an element group with its (resolved, possibly shared)
/// node group: node edits through it are edits of that node group.
#[derive(Debug)]
pub struct ElementGroupMut<'a> {
    group: &'a mut ElementGroup,
    /// The node group the element group refers to (possibly shared).
    pub node_group: &'a mut NodeGroup,
}

impl<'a> ElementGroupMut<'a> {
    /// The underlying element group, for the rest of the borrow.
    pub(crate) fn into_group(self) -> &'a mut ElementGroup {
        self.group
    }
}

impl ElementGroupMut<'_> {
    /// Nodes of the group (a guard, see [`NodeGroup::nodes`]).
    ///
    /// # Panics
    /// As [`NodeGroup::nodes`] (protocol violation of the exporting code).
    #[must_use]
    #[track_caller]
    pub fn nodes(&self) -> ArrayRef<'_, Idx> {
        self.node_group.nodes.borrow()
    }

    /// Number of nodes (upstream `getNbNodes`).
    #[must_use]
    pub fn nb_nodes(&self) -> usize {
        self.node_group.len()
    }

    /// Upstream `add(element, add_nodes, check_for_duplicate)`: `nodes` is
    /// the element's connectivity when its nodes are to be added (upstream
    /// reads it from the mesh), `None` otherwise. `check_for_duplicate`
    /// applies to the nodes only, as upstream.
    pub fn add(
        &mut self,
        el: Element,
        nodes: Option<&[Idx]>,
        check_for_duplicate: bool,
    ) -> Result<()> {
        self.group.add_element(el)?;
        for &n in nodes.unwrap_or(&[]) {
            self.node_group.add(n, check_for_duplicate);
        }
        Ok(())
    }

    /// Add a node (upstream `addNode`).
    pub fn add_node(&mut self, node: Idx, check_for_duplicate: bool) {
        self.node_group.add(node, check_for_duplicate);
    }

    /// Remove a node (upstream `removeNode`).
    pub fn remove_node(&mut self, node: Idx) {
        self.node_group.remove(node);
    }

    /// Sort and deduplicate every element list and the node group
    /// (upstream `optimize`).
    pub fn optimize(&mut self) {
        self.group.optimize_elements();
        self.node_group.optimize();
    }
}

impl Deref for ElementGroupMut<'_> {
    type Target = ElementGroup;
    fn deref(&self) -> &ElementGroup {
        self.group
    }
}

impl DerefMut for ElementGroupMut<'_> {
    fn deref_mut(&mut self) -> &mut ElementGroup {
        self.group
    }
}

/// Owner of all groups of a mesh; `BTreeMap` gives deterministic (name)
/// iteration, as upstream's `std::map`. Node groups live in manager-owned
/// storage addressed by [`NodeGroupId`] (see the module docs).
#[derive(Clone, Debug, Default)]
pub struct GroupManager {
    /// Node-group storage (`None` = destroyed; handles are never reused).
    node_store: Vec<Option<NodeGroup>>,
    /// Node-group names → handles.
    node_index: BTreeMap<String, NodeGroupId>,
    element_groups: BTreeMap<String, ElementGroup>,
}

fn missing_node_group(id: NodeGroupId) -> Error {
    Error::Mesh(format!(
        "internal: element group refers to a destroyed node group {id:?}"
    ))
}

impl GroupManager {
    /// Every exportable list of the manager: the element blocks of every
    /// element group, then the node list of every node group (foreign views,
    /// [`crate::exports`]).
    pub(crate) fn for_each_exportable_list(
        &mut self,
        f: &mut dyn FnMut(&mut SharedArray<Idx>) -> Result<()>,
    ) -> Result<()> {
        for g in self.element_groups.values_mut() {
            for gt in GhostType::ALL {
                for (_, a) in g.elements.as_map_mut().iter_mut(gt) {
                    f(a)?;
                }
            }
        }
        for ng in self.node_store.iter_mut().flatten() {
            f(ng.nodes_shared_mut())?;
        }
        Ok(())
    }

    /// Deterministic dump of every group **by value**, for
    /// [`crate::Mesh::state_fingerprint`] (D-071 (3)):
    /// the node-group storage in handle order (destroyed slots included),
    /// the name index, then the element groups in name order with their
    /// blocks in enum order. Values and identifiers only, never export
    /// metadata (`SharedArray`'s `Debug` is metadata-only, so the derived
    /// `Debug` of the manager does not show the lists). The destructuring
    /// patterns make a new field a compile error here.
    ///
    /// # Panics
    /// As [`SharedArray::borrow`]: a list exported to a foreign runtime
    /// read outside a native section of it (protocol violation).
    pub(crate) fn write_state(&self, s: &mut String) {
        use std::fmt::Write as _;
        let Self {
            node_store,
            node_index,
            element_groups,
        } = self;
        for (i, slot) in node_store.iter().enumerate() {
            match slot {
                None => {
                    let _ = writeln!(s, "node_group #{i} destroyed");
                }
                // `uid`: the object identity is not part of the state
                // (module docs; `Debug`/`==` exclude it).
                Some(NodeGroup {
                    name,
                    nodes,
                    uid: _,
                }) => {
                    let _ = writeln!(s, "node_group #{i} {name:?} {:?}", *nodes.borrow());
                }
            }
        }
        let _ = writeln!(s, "node_index {node_index:?}");
        for (key, g) in element_groups {
            let ElementGroup {
                name,
                elements,
                dimension,
                node_group,
                // object identity: not part of the state (module docs)
                uid: _,
            } = g;
            let _ = writeln!(
                s,
                "element_group {key:?} name={name:?} dim={dimension} node_group={node_group:?}"
            );
            crate::data::write_shared_blocks(s, "  ", elements);
        }
    }

    fn store(&self, id: NodeGroupId) -> Result<&NodeGroup> {
        self.node_store
            .get(id.0)
            .and_then(Option::as_ref)
            .ok_or_else(|| missing_node_group(id))
    }

    fn store_mut(&mut self, id: NodeGroupId) -> Result<&mut NodeGroup> {
        self.node_store
            .get_mut(id.0)
            .and_then(Option::as_mut)
            .ok_or_else(|| missing_node_group(id))
    }

    /// Element groups (other than `except`) referring to node group `id`.
    fn referrers(&self, id: NodeGroupId, except: Option<&str>) -> Vec<&str> {
        self.element_groups
            .values()
            .filter(|g| g.node_group == id && Some(g.name.as_str()) != except)
            .map(|g| g.name.as_str())
            .collect()
    }

    fn refuse_referenced(&self, id: NodeGroupId, except: Option<&str>, what: &str) -> Result<()> {
        let users = self.referrers(id, except);
        if users.is_empty() {
            return Ok(());
        }
        let name = self.store(id).map(NodeGroup::name).unwrap_or("?");
        Err(Error::invalid(format!(
            "cannot {what} node group '{name}': element group(s) {} refer to it \
             (upstream leaves a dangling reference, UD-125)",
            users.join(", ")
        )))
    }

    fn insert_node_group(&mut self, group: NodeGroup) -> NodeGroupId {
        let id = NodeGroupId(self.node_store.len());
        self.node_index.insert(group.name.clone(), id);
        self.node_store.push(Some(group));
        id
    }

    fn remove_node_group(&mut self, id: NodeGroupId) {
        if let Some(slot) = self.node_store.get_mut(id.0)
            && let Some(g) = slot.take()
        {
            self.node_index.remove(&g.name);
        }
    }

    /// Rebuild the node list of every element group from the current
    /// connectivities (upstream `ElementGroup::onNodesAdded` for cohesive
    /// node events: the node group is cleared and refilled with the nodes of
    /// the group's elements, then optimized). Element groups are visited in
    /// name order (upstream `GroupManager::onNodesAdded` iterates its
    /// `std::map`), so a node group shared by several element groups ends
    /// up with the nodes of the last one, as upstream.
    ///
    /// # Panics
    /// If a connectivity or group list is exported to a foreign runtime and
    /// no native section of it is active (protocol violation,
    /// [`SharedArray::borrow`]).
    pub fn rebuild_element_group_nodes(&mut self, connectivities: &SharedElementTypeMapArray<Idx>) {
        let Self {
            node_store,
            element_groups,
            ..
        } = self;
        for g in element_groups.values() {
            let Some(Some(ng)) = node_store.get_mut(g.node_group.0) else {
                continue;
            };
            ng.clear();
            for (ty, ghost, els) in g.elements.as_map().iter_all() {
                // an absent block is skipped; a protocol violation panics
                // instead of being taken for absence
                if let Some(conn) = connectivities.as_map().get(ty, ghost) {
                    let conn = conn.borrow();
                    for &e in els.borrow().as_slice() {
                        if let Ok(e) = try_to_usize(e)
                            && e < conn.len()
                        {
                            ng.extend_nodes(conn.tuple(e));
                        }
                    }
                }
            }
            ng.optimize();
        }
    }

    /// Create an empty node group (error if a node group of that name
    /// exists, including the `<name>_nodes` group of an element group).
    pub fn create_node_group(&mut self, name: &str) -> Result<&mut NodeGroup> {
        self.create_node_group_replace(name, false)
    }

    /// Upstream `createNodeGroup(name, replace_group)`: with `replace` an
    /// existing node group is replaced by an empty one — refused while an
    /// element group refers to it (upstream leaves that element group with a
    /// dangling reference, UD-125).
    pub fn create_node_group_replace(
        &mut self,
        name: &str,
        replace: bool,
    ) -> Result<&mut NodeGroup> {
        let id = match self.node_index.get(name).copied() {
            Some(id) => {
                if !replace {
                    return Err(Error::AlreadyExists {
                        kind: "node group",
                        id: name.into(),
                    });
                }
                self.refuse_referenced(id, None, "replace")?;
                *self.store_mut(id)? = NodeGroup::new(name);
                id
            }
            None => self.insert_node_group(NodeGroup::new(name)),
        };
        self.store_mut(id)
    }

    /// Create an empty element group of `dimension` with its node group
    /// `<name>_nodes` (error if the element group or a node group of that
    /// name exists, as upstream `createElementGroup`).
    pub fn create_element_group(
        &mut self,
        name: &str,
        dimension: usize,
    ) -> Result<ElementGroupMut<'_>> {
        self.create_element_group_replace(name, dimension, false)
    }

    /// Upstream `createElementGroup(name, dimension, replace_group)`: with
    /// `replace` an existing element group of that name is replaced by an
    /// empty one, and so is the node group `<name>_nodes` (refused while
    /// another element group refers to that node group, UD-125).
    pub fn create_element_group_replace(
        &mut self,
        name: &str,
        dimension: usize,
        replace: bool,
    ) -> Result<ElementGroupMut<'_>> {
        let node_group_name = format!("{name}_nodes");
        // Validate everything first (no partial change on error).
        if self.element_groups.contains_key(name) && !replace {
            return Err(Error::AlreadyExists {
                kind: "element group",
                id: name.into(),
            });
        }
        if let Some(&id) = self.node_index.get(&node_group_name) {
            if !replace {
                return Err(Error::AlreadyExists {
                    kind: "node group",
                    id: node_group_name,
                });
            }
            self.refuse_referenced(id, Some(name), "replace")?;
        }
        self.element_groups.remove(name);
        let id = match self.node_index.get(&node_group_name).copied() {
            Some(id) => {
                *self.store_mut(id)? = NodeGroup::new(node_group_name);
                id
            }
            None => self.insert_node_group(NodeGroup::new(node_group_name)),
        };
        self.element_groups
            .insert(name.into(), ElementGroup::new(name, dimension, id));
        self.element_group_mut(name)
    }

    /// Upstream `createElementGroup(name, dimension, node_group)`: an empty
    /// element group referring to the existing node group `node_group`
    /// (shared, see the module docs).
    pub fn create_element_group_on_node_group(
        &mut self,
        name: &str,
        dimension: usize,
        node_group: &str,
    ) -> Result<ElementGroupMut<'_>> {
        if self.element_groups.contains_key(name) {
            return Err(Error::AlreadyExists {
                kind: "element group",
                id: name.into(),
            });
        }
        let id = self
            .node_index
            .get(node_group)
            .copied()
            .ok_or_else(|| Error::not_found("node group", node_group))?;
        self.element_groups
            .insert(name.into(), ElementGroup::new(name, dimension, id));
        self.element_group_mut(name)
    }

    /// Upstream `destroyElementGroup(name, destroy_node_group)`: with
    /// `destroy_node_group` the node group it refers to is destroyed too —
    /// refused while another element group shares it (UD-125). A missing
    /// group is not an error (as upstream).
    pub fn destroy_element_group(&mut self, name: &str, destroy_node_group: bool) -> Result<()> {
        let Some(id) = self.element_groups.get(name).map(|g| g.node_group) else {
            return Ok(());
        };
        if destroy_node_group {
            self.refuse_referenced(id, Some(name), "destroy")?;
            self.remove_node_group(id);
        }
        self.element_groups.remove(name);
        Ok(())
    }

    /// Upstream `destroyNodeGroup(name)`; refused while an element group
    /// refers to it (UD-125). A missing group is not an error.
    pub fn destroy_node_group(&mut self, name: &str) -> Result<()> {
        if let Some(&id) = self.node_index.get(name) {
            self.refuse_referenced(id, None, "destroy")?;
            self.remove_node_group(id);
        }
        Ok(())
    }

    /// Upstream `renameElementGroup(name, new_name)` (its node group keeps
    /// its name, as upstream).
    pub fn rename_element_group(&mut self, name: &str, new_name: &str) -> Result<()> {
        if self.element_groups.contains_key(new_name) {
            return Err(Error::AlreadyExists {
                kind: "element group",
                id: new_name.into(),
            });
        }
        let mut g = self
            .element_groups
            .remove(name)
            .ok_or_else(|| Error::not_found("element group", name))?;
        g.name = new_name.into();
        self.element_groups.insert(new_name.into(), g);
        Ok(())
    }

    /// Upstream `renameNodeGroup(name, new_name)`: every element group
    /// referring to it keeps referring to it (and sees the new name).
    pub fn rename_node_group(&mut self, name: &str, new_name: &str) -> Result<()> {
        if self.node_index.contains_key(new_name) {
            return Err(Error::AlreadyExists {
                kind: "node group",
                id: new_name.into(),
            });
        }
        let id = self
            .node_index
            .remove(name)
            .ok_or_else(|| Error::not_found("node group", name))?;
        self.store_mut(id)?.name = new_name.into();
        self.node_index.insert(new_name.into(), id);
        Ok(())
    }

    /// Upstream `copyElementGroup(name, new_name)`: a new element group with
    /// the same dimension and elements and its own node group
    /// `<new_name>_nodes`. Deviation (D-003 category B, UD-124): the nodes
    /// are copied too (upstream leaves the copy's node group empty).
    pub fn copy_element_group(&mut self, name: &str, new_name: &str) -> Result<()> {
        let (elements, dimension, nodes) = {
            let src = self.element_group(name)?;
            (
                src.elements.clone(),
                src.dimension,
                src.node_group.node_list(),
            )
        };
        let mut g = self.create_element_group(new_name, dimension)?;
        g.elements = elements;
        g.node_group.set_nodes(nodes);
        Ok(())
    }

    /// Upstream `copyNodeGroup(name, new_name)`.
    pub fn copy_node_group(&mut self, name: &str, new_name: &str) -> Result<()> {
        let nodes = self.node_group(name)?.node_list();
        self.create_node_group(new_name)?.set_nodes(nodes);
        Ok(())
    }

    /// Upstream `ElementGroup::append(other)` on the element group `name`:
    /// the nodes of `other`'s node group are appended to `name`'s node group
    /// (one node group if they share it), its elements to `name`'s, then
    /// `name` is optimized.
    pub fn append_element_group(&mut self, name: &str, other: &str) -> Result<()> {
        let (elements, nodes) = {
            let o = self.element_group(other)?;
            (o.elements.clone(), o.node_group.node_list())
        };
        let mut g = self.element_group_mut(name)?;
        g.node_group.extend_nodes(&nodes);
        g.node_group.optimize();
        for (ty, ghost, a) in elements.as_map().iter_all() {
            if !g.elements.exists(ty, ghost) {
                g.elements.alloc(0, 1, ty, ghost, 0)?;
            }
            g.elements.get_mut(ty, ghost)?.append(&*a.read()?)?;
        }
        g.optimize();
        Ok(())
    }

    /// Upstream `elementGroupExists`.
    #[must_use]
    pub fn element_group_exists(&self, name: &str) -> bool {
        self.element_groups.contains_key(name)
    }

    /// Upstream `nodeGroupExists`.
    #[must_use]
    pub fn node_group_exists(&self, name: &str) -> bool {
        self.node_index.contains_key(name)
    }

    /// Upstream `getNbElementGroups(dimension)` (`None` = all).
    #[must_use]
    pub fn nb_element_groups(&self, dimension: Option<usize>) -> usize {
        match dimension {
            None => self.element_groups.len(),
            Some(d) => self
                .element_groups
                .values()
                .filter(|g| g.dimension == d)
                .count(),
        }
    }

    /// Node group by name (upstream `getNodeGroup`).
    pub fn node_group(&self, name: &str) -> Result<&NodeGroup> {
        let id = self
            .node_index
            .get(name)
            .copied()
            .ok_or_else(|| Error::not_found("node group", name))?;
        self.store(id)
    }

    /// Mutable node group by name; element groups referring to it see the
    /// edits.
    pub fn node_group_mut(&mut self, name: &str) -> Result<&mut NodeGroup> {
        let id = self
            .node_index
            .get(name)
            .copied()
            .ok_or_else(|| Error::not_found("node group", name))?;
        self.store_mut(id)
    }

    /// Node group of a handle (see [`ElementGroup::node_group_id`]).
    pub fn node_group_by_id(&self, id: NodeGroupId) -> Result<&NodeGroup> {
        self.store(id)
    }

    /// Element group by name, with its node group.
    pub fn element_group(&self, name: &str) -> Result<ElementGroupRef<'_>> {
        let group = self
            .element_groups
            .get(name)
            .ok_or_else(|| Error::not_found("element group", name))?;
        Ok(ElementGroupRef {
            node_group: self.store(group.node_group)?,
            group,
        })
    }

    /// Mutable element group, with its node group.
    pub fn element_group_mut(&mut self, name: &str) -> Result<ElementGroupMut<'_>> {
        let Self {
            node_store,
            element_groups,
            ..
        } = self;
        let group = element_groups
            .get_mut(name)
            .ok_or_else(|| Error::not_found("element group", name))?;
        let id = group.node_group;
        let node_group = node_store
            .get_mut(id.0)
            .and_then(Option::as_mut)
            .ok_or_else(|| missing_node_group(id))?;
        Ok(ElementGroupMut { group, node_group })
    }

    /// Element groups in name order, with their node groups.
    pub fn element_groups(&self) -> impl Iterator<Item = ElementGroupRef<'_>> {
        self.element_groups.values().filter_map(|group| {
            Some(ElementGroupRef {
                node_group: self.store(group.node_group).ok()?,
                group,
            })
        })
    }

    /// All node groups in name order, each once (upstream
    /// `iterateNodeGroups`): standalone ones and those element groups refer
    /// to.
    pub fn node_groups(&self) -> impl Iterator<Item = &NodeGroup> {
        self.node_index
            .values()
            .filter_map(|&id| self.store(id).ok())
    }

    /// Upstream `createGroupsFromMeshData<T>(dataset_name)`
    /// (`group_manager.cc:597-685`) for string, `UInt` and `Int` datasets:
    /// one element group per distinct value (printed with `operator<<`),
    /// of dimension the largest spatial dimension of the types carrying the
    /// value; elements are added with their nodes in (ghost, type, element)
    /// order, then every new group is optimized. Errors: missing dataset,
    /// unsupported dataset type, a dataset whose length differs from the
    /// element count (upstream: debug assertion), an existing group of a
    /// created name (upstream throws).
    pub fn create_groups_from_mesh_data(
        &mut self,
        data: &MeshData,
        connectivities: &SharedElementTypeMapArray<Idx>,
        dataset_name: &str,
    ) -> Result<()> {
        use crate::data::MeshDataValues as V;
        let values = data.elemental(dataset_name)?;
        let mut rows: Vec<(GhostType, ElementType, Vec<String>)> = Vec::new();
        for ghost in GhostType::ALL {
            for ty in values.element_types(ghost) {
                let names: Vec<String> = match values {
                    V::String(m) => m.get(ty, ghost)?.as_slice().to_vec(),
                    V::UInt(m) => m
                        .get(ty, ghost)?
                        .as_slice()
                        .iter()
                        .map(ToString::to_string)
                        .collect(),
                    V::Int(m) => m
                        .get(ty, ghost)?
                        .as_slice()
                        .iter()
                        .map(ToString::to_string)
                        .collect(),
                    other => {
                        return Err(Error::Mesh(format!(
                            "createGroupsFromMeshData: dataset '{dataset_name}' has type {:?}",
                            other.type_code()
                        )));
                    }
                };
                let nb = connectivities.get(ty, ghost).map_or(0, |c| c.len());
                if names.len() != nb {
                    return Err(Error::Mesh(format!(
                        "mesh data '{dataset_name}' has {} entries for {ty}:{ghost}, the mesh {nb} elements",
                        names.len()
                    )));
                }
                rows.push((ghost, ty, names));
            }
        }
        let mut dims: BTreeMap<&str, usize> = BTreeMap::new();
        for (_, ty, names) in &rows {
            for n in names {
                let d = dims.entry(n.as_str()).or_insert(ty.spatial_dimension());
                *d = (*d).max(ty.spatial_dimension());
            }
        }
        // Validate every creation first (no partial result on error).
        for &name in dims.keys() {
            if self.element_groups.contains_key(name) {
                return Err(Error::AlreadyExists {
                    kind: "element group",
                    id: name.into(),
                });
            }
            let ng = format!("{name}_nodes");
            if self.node_group_exists(&ng) {
                return Err(Error::AlreadyExists {
                    kind: "node group",
                    id: ng,
                });
            }
        }
        for (&name, &d) in &dims {
            self.create_element_group(name, d)?;
        }
        for (ghost, ty, names) in &rows {
            let conn = connectivities.get(*ty, *ghost)?;
            for (e, name) in names.iter().enumerate() {
                let mut g = self.element_group_mut(name)?;
                g.add(
                    Element::try_new(*ty, e, *ghost)?,
                    Some(conn.tuple(e)),
                    false,
                )?;
            }
        }
        for &name in dims.keys() {
            self.element_group_mut(name)?.optimize();
        }
        Ok(())
    }

    /// Upstream `ElementGroup::fillFromNodeGroup` for the element group
    /// `name`: every element (of the group's dimension unless
    /// [`ALL_DIMENSIONS`]) whose nodes all belong to the group's node group
    /// is added (without nodes), in node-group order then CSR row order,
    /// then the group is optimized (its elements and its — possibly shared —
    /// node group, as upstream). `node_to_elements` must be built with the
    /// group's dimension (upstream `buildNode2Elements(mesh, csr,
    /// dimension)`).
    pub fn fill_from_node_group(
        &mut self,
        name: &str,
        node_to_elements: &NodeToElements,
        connectivities: &SharedElementTypeMapArray<Idx>,
    ) -> Result<()> {
        let mut g = self.element_group_mut(name)?;
        let dim = g.dimension;
        let nodes = g.node_group.node_list();
        let mut seen = std::collections::BTreeSet::new();
        let mut add = Vec::new();
        for &n in &nodes {
            let Some(row) = node_to_elements.get_row(try_to_usize(n)?) else {
                return Err(Error::OutOfRange {
                    what: "node",
                    index: try_to_usize(n)?,
                    len: node_to_elements.nb_rows(),
                });
            };
            for &el in row {
                if dim != ALL_DIMENSIONS && dim != el.ty().spatial_dimension() {
                    continue;
                }
                if !seen.insert(el) {
                    continue;
                }
                let conn = connectivities.element_ref(el)?;
                if conn.iter().all(|c| nodes.contains(c)) {
                    add.push(el);
                }
            }
        }
        for el in add {
            g.add_element(el)?;
        }
        g.optimize();
        Ok(())
    }

    /// Upstream `createElementGroupFromNodeGroup(name, node_group_name,
    /// dimension)`: an element group on (sharing) the node group, filled
    /// with [`GroupManager::fill_from_node_group`]; nothing is created on
    /// error.
    pub fn create_element_group_from_node_group(
        &mut self,
        name: &str,
        node_group: &str,
        dimension: usize,
        node_to_elements: &NodeToElements,
        connectivities: &SharedElementTypeMapArray<Idx>,
    ) -> Result<()> {
        let saved = self.node_group(node_group)?.clone();
        self.create_element_group_on_node_group(name, dimension, node_group)?;
        if let Err(e) = self.fill_from_node_group(name, node_to_elements, connectivities) {
            self.element_groups.remove(name);
            *self.node_group_mut(node_group)? = saved;
            return Err(e);
        }
        Ok(())
    }

    /// Upstream `createFilteredNodeGroup(name, source, filter)`: a new node
    /// group holding the (optimized) nodes of `source` accepted by `filter`.
    pub fn create_filtered_node_group(
        &mut self,
        name: &str,
        source: &str,
        filter: impl FnMut(Idx) -> bool,
    ) -> Result<()> {
        let src = self.node_group(source)?.clone();
        let g = self.create_node_group(name)?;
        g.append(&src);
        g.apply_node_filter(filter);
        Ok(())
    }

    /// Upstream `createFilteredElementGroup(name, dimension, node_group,
    /// node_filter)`: the filtered node group `<name>_nodes` and an element
    /// group on it (empty: upstream does not fill it either). Element
    /// filters are not implemented upstream (`AKANTU_ERROR`) and are not
    /// offered here.
    pub fn create_filtered_element_group(
        &mut self,
        name: &str,
        dimension: usize,
        node_group: &str,
        filter: impl FnMut(Idx) -> bool,
    ) -> Result<()> {
        let ng = format!("{name}_nodes");
        if self.element_groups.contains_key(name) {
            return Err(Error::AlreadyExists {
                kind: "element group",
                id: name.into(),
            });
        }
        self.create_filtered_node_group(&ng, node_group, filter)?;
        self.create_element_group_on_node_group(name, dimension, &ng)?;
        Ok(())
    }

    /// Element removal: renumber the `(ty, ghost)` lists of every element
    /// group (removed entries dropped, order kept).
    pub(crate) fn apply_element_numbering(
        &mut self,
        ty: ElementType,
        ghost: GhostType,
        numbering: &[Idx],
    ) {
        for g in self.element_groups.values_mut() {
            g.apply_element_numbering(ty, ghost, numbering);
        }
    }

    /// Node removal: renumber every node group once (removed nodes
    /// dropped).
    pub(crate) fn apply_node_numbering(&mut self, numbering: &[Idx]) {
        for g in self.node_store.iter_mut().flatten() {
            g.apply_node_numbering(numbering);
        }
    }

    /// Deterministic text dump (upstream `GroupManager::printself` layout:
    /// element groups with their node groups, then the other node groups).
    #[must_use]
    pub fn describe(&self, indent: usize) -> String {
        use std::fmt::Write;
        let sp = " ".repeat(indent);
        let mut s = format!("{sp}GroupManager [\n");
        let mut seen = std::collections::BTreeSet::new();
        for g in self.element_groups() {
            let _ = writeln!(s, "{sp} ElementGroup [");
            let _ = writeln!(s, "{sp}  + name: {}", g.name());
            let _ = writeln!(s, "{sp}  + dimension: {}", g.dimension);
            for (ty, ghost, a) in g.elements.as_map().iter_all() {
                let _ = writeln!(s, "{sp}  + {ty}:{ghost} {:?}", a.borrow().as_slice());
            }
            let _ = writeln!(
                s,
                "{sp}  NodeGroup [ + name: {} {:?} ]",
                g.node_group.name,
                g.node_group.node_list()
            );
            let _ = writeln!(s, "{sp} ]");
            seen.insert(g.node_group.name.clone());
        }
        for g in self.node_groups() {
            if !seen.contains(&g.name) {
                let _ = writeln!(
                    s,
                    "{sp} NodeGroup [ + name: {} {:?} ]",
                    g.name,
                    g.node_list()
                );
            }
        }
        let _ = writeln!(s, "{sp}]");
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn groups() {
        let mut g = GroupManager::default();
        g.create_element_group("left", 1).unwrap();
        assert!(g.create_element_group("left", 1).is_err());
        let ng = g.create_node_group("pts").unwrap();
        ng.set_nodes(vec![3, 1, 3, 2]);
        ng.optimize();
        assert_eq!(g.node_group("pts").unwrap().node_list(), vec![1, 2, 3]);
        assert!(g.element_group("right").is_err());
        assert_eq!(g.element_groups().count(), 1);
    }

    /// Element groups refer to a `<name>_nodes` node group that is found,
    /// listed and protected like a standalone one (upstream
    /// `createElementGroup` → `createNodeGroup(name + "_nodes")`).
    #[test]
    fn element_group_node_groups_share_the_namespace() {
        let mut g = GroupManager::default();
        g.create_node_group("pts").unwrap();
        g.create_element_group("left", 1)
            .unwrap()
            .node_group
            .set_nodes(vec![0, 4]);
        assert_eq!(
            g.element_group("left").unwrap().node_group.name(),
            "left_nodes"
        );
        assert_eq!(g.node_group("left_nodes").unwrap().node_list(), vec![0, 4]);
        let names: Vec<&str> = g.node_groups().map(NodeGroup::name).collect();
        assert_eq!(names, vec!["left_nodes", "pts"]);
        // Same object: changes through either path are seen by both.
        g.node_group_mut("left_nodes").unwrap().extend_nodes(&[7]);
        assert_eq!(g.element_group("left").unwrap().nodes(), &[0, 4, 7]);
        g.node_group_mut("pts").unwrap().extend_nodes(&[1]);
        assert_eq!(g.node_group("pts").unwrap().node_list(), vec![1]);
        assert!(g.node_group_mut("nope").is_err());
        // Name clashes in both directions.
        assert!(matches!(
            g.create_node_group("left_nodes"),
            Err(Error::AlreadyExists {
                kind: "node group",
                ..
            })
        ));
        g.create_node_group("right_nodes").unwrap();
        assert!(matches!(
            g.create_element_group("right", 1),
            Err(Error::AlreadyExists {
                kind: "node group",
                ..
            })
        ));
        assert!(g.element_group("right").is_err());
    }

    #[test]
    fn node_group_edits_follow_upstream_order() {
        let mut n = NodeGroup::new("n");
        for v in [4, 7, 4, 9, 4] {
            n.add(v, false);
        }
        assert_eq!(n.add(7, true), 1);
        // erase moves the last entry into the hole and re-checks it
        n.remove(4);
        assert_eq!(n.node_list(), vec![9, 7]);
        n.remove(9);
        assert_eq!(n.node_list(), vec![7]);
        n.remove(7);
        assert!(n.is_empty());
        let mut m = NodeGroup::new("m");
        m.set_nodes(vec![1, 2, 3, 4, 5]);
        m.apply_node_filter(|x| x % 2 == 1);
        assert_eq!(m.node_list(), vec![1, 5, 3]);
        assert_eq!(m.find(5), Some(1));
        let mut o = NodeGroup::new("o");
        o.set_nodes(vec![9, 1]);
        m.append(&o);
        assert_eq!(m.node_list(), vec![1, 3, 5, 9]);
    }

    #[test]
    fn replace_destroy_rename_copy() {
        let mut g = GroupManager::default();
        g.create_element_group("a", 2)
            .unwrap()
            .add(
                Element::new(ElementType::Triangle3, 0, GhostType::NotGhost),
                Some(&[0, 1, 2]),
                false,
            )
            .unwrap();
        // replace
        let mut r = g.create_element_group_replace("a", 1, true).unwrap();
        assert!(r.is_empty() && r.nodes().is_empty() && r.dimension == 1);
        r.add(
            Element::new(ElementType::Segment2, 3, GhostType::NotGhost),
            Some(&[4, 5]),
            false,
        )
        .unwrap();
        assert_eq!(g.element_group("a").unwrap().dimension, 1);
        // copy (elements and nodes), rename keeps the node-group name
        g.copy_element_group("a", "b").unwrap();
        assert_eq!(
            g.element_group("b")
                .unwrap()
                .elements_of(ElementType::Segment2, GhostType::NotGhost),
            &[3]
        );
        assert_eq!(g.element_group("b").unwrap().nodes(), &[4, 5]);
        g.rename_element_group("b", "c").unwrap();
        assert_eq!(g.element_group("c").unwrap().node_group.name(), "b_nodes");
        assert!(g.rename_element_group("c", "a").is_err());
        // destroy keeps the node group unless asked
        g.destroy_element_group("c", false).unwrap();
        assert_eq!(g.node_group("b_nodes").unwrap().node_list(), vec![4, 5]);
        assert!(!g.element_group_exists("c"));
        g.destroy_node_group("b_nodes").unwrap();
        assert!(!g.node_group_exists("b_nodes"));
        assert!(g.destroy_node_group("a_nodes").is_err());
        g.destroy_element_group("a", true).unwrap();
        assert_eq!(g.node_groups().count(), 0);
        assert_eq!(g.nb_element_groups(None), 0);
        // node groups
        g.create_node_group("p").unwrap().set_nodes(vec![3, 1]);
        g.copy_node_group("p", "q").unwrap();
        g.rename_node_group("q", "r").unwrap();
        assert_eq!(g.node_group("r").unwrap().node_list(), vec![3, 1]);
        assert!(g.create_node_group_replace("r", true).unwrap().is_empty());
        g.create_filtered_node_group("odd", "p", |n| n == 3)
            .unwrap();
        assert_eq!(g.node_group("odd").unwrap().node_list(), vec![3]);
        g.create_filtered_element_group("fe", 1, "p", |n| n == 1)
            .unwrap();
        assert_eq!(g.element_group("fe").unwrap().nodes(), &[1]);
        assert_eq!(g.node_group("fe_nodes").unwrap().node_list(), vec![1]);
        g.create_element_group_on_node_group("on_p", 2, "p")
            .unwrap();
        let names: Vec<&str> = g.node_groups().map(NodeGroup::name).collect();
        assert_eq!(names, vec!["fe_nodes", "odd", "p", "r"]);
        assert_eq!(g.nb_element_groups(Some(2)), 1);
        assert!(g.describe(0).contains("ElementGroup"));
    }

    /// Shared node groups — identity through edits, renames
    /// and appends; destruction and replacement refused while referenced;
    /// the cohesive rebuild visits element groups in name order.
    #[test]
    fn shared_node_group_rules() {
        let mut g = GroupManager::default();
        g.create_node_group("p").unwrap().set_nodes(vec![0, 1]);
        g.create_element_group_on_node_group("e", 1, "p").unwrap();
        g.create_element_group_on_node_group("f", 1, "p").unwrap();
        // rename the node group: both element groups follow
        g.rename_node_group("p", "q").unwrap();
        assert_eq!(g.element_group("e").unwrap().node_group.name(), "q");
        g.element_group_mut("f").unwrap().add_node(5, false);
        assert_eq!(g.element_group("e").unwrap().nodes(), &[0, 1, 5]);
        // listed once
        assert_eq!(g.node_groups().count(), 1);
        // destruction / replacement refused while referenced
        assert!(g.destroy_node_group("q").is_err());
        assert!(g.create_node_group_replace("q", true).is_err());
        assert!(g.destroy_element_group("e", true).is_err());
        assert!(g.element_group_exists("e"));
        g.destroy_element_group("e", false).unwrap();
        // only f refers to q now: destroying f with its node group works
        g.destroy_element_group("f", true).unwrap();
        assert!(!g.node_group_exists("q"));
        // replacing an element group whose `<name>_nodes` another group
        // shares is refused
        g.create_element_group("a", 1).unwrap();
        g.create_element_group_on_node_group("b", 1, "a_nodes")
            .unwrap();
        assert!(g.create_element_group_replace("a", 1, true).is_err());
        // appending a group sharing the node group: nodes once (optimized)
        g.element_group_mut("a")
            .unwrap()
            .node_group
            .set_nodes(vec![2, 1]);
        g.append_element_group("a", "b").unwrap();
        assert_eq!(g.element_group("b").unwrap().nodes(), &[1, 2]);
        // cohesive rebuild: name order, the last element group wins
        let mut conn: akantu_core::ElementTypeMapArray<Idx> =
            akantu_core::ElementTypeMapArray::new("c");
        conn.alloc(2, 2, ElementType::Segment2, GhostType::NotGhost, 0)
            .unwrap()
            .as_mut_slice()
            .copy_from_slice(&[0, 1, 7, 8]);
        g.element_group_mut("a")
            .unwrap()
            .add_element(Element::new(ElementType::Segment2, 0, GhostType::NotGhost))
            .unwrap();
        g.element_group_mut("b")
            .unwrap()
            .add_element(Element::new(ElementType::Segment2, 1, GhostType::NotGhost))
            .unwrap();
        g.rebuild_element_group_nodes(&conn.into());
        assert_eq!(g.node_group("a_nodes").unwrap().node_list(), vec![7, 8]);
    }

    /// Identities follow the group object (kept by renames
    /// and edits, new for replaced / re-created groups, never reused).
    #[test]
    fn group_identities_follow_the_object() {
        let mut g = GroupManager::default();
        let e0 = g.create_element_group("e", 2).unwrap().uid();
        let n0 = g.node_group("e_nodes").unwrap().uid();
        let p0 = g.create_node_group("p").unwrap().uid();
        assert_ne!(e0, n0);
        assert_ne!(n0, p0);
        // edits and renames keep the identity
        g.element_group_mut("e")
            .unwrap()
            .add_element(Element::new(ElementType::Triangle3, 0, GhostType::NotGhost))
            .unwrap();
        g.node_group_mut("p").unwrap().add(3, true);
        g.rename_element_group("e", "f").unwrap();
        g.rename_node_group("p", "q").unwrap();
        assert_eq!(g.element_group("f").unwrap().uid(), e0);
        assert_eq!(g.node_group("q").unwrap().uid(), p0);
        assert_eq!(g.node_group("e_nodes").unwrap().uid(), n0);
        // replaced groups are new objects (upstream: a new group)
        let e1 = g.create_element_group_replace("f", 2, true).unwrap().uid();
        assert_ne!(e1, e0);
        let p1 = g.create_node_group_replace("q", true).unwrap().uid();
        assert_ne!(p1, p0);
        // destroyed and re-created under the same name: new identity
        g.destroy_element_group("f", true).unwrap();
        let e2 = g.create_element_group("f", 2).unwrap().uid();
        assert!(![e0, e1].contains(&e2));
        g.destroy_node_group("q").unwrap();
        let p2 = g.create_node_group("q").unwrap().uid();
        assert!(![p0, p1].contains(&p2));
        // copies are new objects; equality ignores the identity
        g.copy_node_group("q", "r").unwrap();
        assert_ne!(g.node_group("r").unwrap().uid(), p2);
        let mut a = NodeGroup::new("x");
        let b = NodeGroup::new("x");
        assert_ne!(a.uid(), b.uid());
        assert_eq!(a, b);
        a.add(1, false);
        assert_ne!(a, b);
        assert_ne!(NodeGroup::default().uid(), NodeGroup::default().uid());
        // Debug (and so Mesh::state_fingerprint) shows contents only
        assert_eq!(
            format!("{a:?}"),
            format!("{:?}", {
                let mut c = NodeGroup::new("x");
                c.add(1, false);
                c
            })
        );
        let mut h = GroupManager::default();
        h.create_element_group("f", 2).unwrap();
        let mut k = GroupManager::default();
        k.create_element_group("f", 2).unwrap();
        assert_ne!(
            h.element_group("f").unwrap().uid(),
            k.element_group("f").unwrap().uid()
        );
        assert_eq!(format!("{h:?}"), format!("{k:?}"));
    }
}
