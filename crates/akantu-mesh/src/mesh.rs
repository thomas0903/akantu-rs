// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/mesh/mesh.hh, mesh.cc,
// mesh_inline_impl.hh, src/common/aka_math_tmpl.hh (Math::barycenter).

//! The [`Mesh`] structure.

use std::collections::HashMap;

use akantu_comm::Communicator;
use akantu_core::array_shared::{ArrayRef, SliceRef};
use akantu_core::dim::check_dim;
use akantu_core::{
    Array, Element, ElementType, ElementTypeMapArray, Error, GhostType, GlobalIdx, Id, Idx,
    NO_RANK, NodeFlag, Rank, Real, Result, SharedArray, SharedElementTypeMapArray, TypeFilter,
    to_usize, try_to_usize,
};

use crate::data::MeshData;
use crate::group::GroupManager;
use crate::synchronizers::MeshSynchronizers;

/// Axis-aligned bounding box (upstream `BBox`): moved to
/// [`akantu_core::bbox`], re-exported here.
pub use akantu_core::bbox::BBox;

/// A finite-element mesh (one per MPI rank once distributed).
///
/// Invariants (checked by [`Mesh::check_consistency`]):
/// * `nodes.nb_component() == spatial_dimension`;
/// * every connectivity array of type `t` has
///   `t.nb_nodes_per_element()` components and only valid node indices;
/// * `nodes_flags.len() == nodes.len()`; `nodes_global_ids`, when present,
///   has the same length.
#[derive(Debug)]
pub struct Mesh {
    pub(crate) id: Id,
    pub(crate) spatial_dimension: usize,
    /// Exportable (D-047): Python `getNodes()` views alias it.
    pub(crate) nodes: SharedArray<Real>,
    pub(crate) nodes_global_ids: Option<Array<GlobalIdx>>,
    pub(crate) nodes_flags: Array<NodeFlag>,
    pub(crate) nodes_prank: HashMap<Idx, Rank>,
    /// Exportable (D-047): Python `getConnectivity()` views alias it.
    pub(crate) connectivities: SharedElementTypeMapArray<Idx>,
    pub(crate) ghosts_counters: ElementTypeMapArray<Idx>,
    pub(crate) bbox: BBox,
    pub(crate) bbox_local: BBox,
    pub(crate) mesh_facets: Option<Box<Mesh>>,
    pub(crate) is_mesh_facets: bool,
    pub(crate) is_distributed: bool,
    pub(crate) is_periodic: bool,
    pub(crate) periodic_slave_master: HashMap<Idx, Idx>,
    pub(crate) periodic_master_slaves: HashMap<Idx, Vec<Idx>>,
    pub(crate) groups: GroupManager,
    pub(crate) data: MeshData,
    pub(crate) synchronizers: MeshSynchronizers,
    /// Number of global nodes once global ids exist (set by distribution;
    /// upstream `nb_global_nodes`).
    pub(crate) nb_global_nodes: usize,
    pub(crate) communicator: Communicator,
    /// Incremented by every topology change; components record the revision
    /// they are consistent with (see `docs/ARCHITECTURE.md`).
    pub(crate) revision: u64,
}

impl Mesh {
    /// Empty serial mesh (upstream `Mesh(spatial_dimension, id)`).
    pub fn new(spatial_dimension: usize, id: impl Into<Id>) -> Result<Self> {
        Self::with_communicator(spatial_dimension, id, Communicator::serial())
    }

    /// Empty mesh attached to a communicator (upstream
    /// `Mesh(spatial_dimension, communicator, id)`).
    pub fn with_communicator(
        spatial_dimension: usize,
        id: impl Into<Id>,
        communicator: Communicator,
    ) -> Result<Self> {
        let dim = check_dim(spatial_dimension)?;
        let id = id.into();
        Ok(Self {
            nodes: Array::with_id(dim, format!("{id}:coordinates")).into(),
            nodes_global_ids: None,
            nodes_flags: Array::with_id(1, format!("{id}:nodes_flags")),
            nodes_prank: HashMap::new(),
            connectivities: SharedElementTypeMapArray::new(format!("{id}:connectivities")),
            ghosts_counters: ElementTypeMapArray::new(format!("{id}:ghosts_counters")),
            bbox: BBox::empty(dim),
            bbox_local: BBox::empty(dim),
            mesh_facets: None,
            is_mesh_facets: false,
            is_distributed: false,
            is_periodic: false,
            periodic_slave_master: HashMap::new(),
            periodic_master_slaves: HashMap::new(),
            groups: GroupManager::default(),
            data: MeshData::default(),
            synchronizers: MeshSynchronizers::default(),
            nb_global_nodes: 0,
            communicator,
            revision: 0,
            id,
            spatial_dimension: dim,
        })
    }

    /// Identifier.
    #[must_use]
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Spatial dimension (1–3).
    #[must_use]
    pub fn spatial_dimension(&self) -> usize {
        self.spatial_dimension
    }

    /// Topology revision counter.
    #[must_use]
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// Communicator the mesh lives on.
    #[must_use]
    pub fn communicator(&self) -> &Communicator {
        &self.communicator
    }

    // ----------------------------------------------------------------- nodes

    /// Node coordinates (`nb_nodes × dim`): a guard of the exportable
    /// array (ARCHITECTURE.md, "Shared storage and its aliasing rules"; free while no foreign view exists).
    ///
    /// # Panics
    /// If the nodes are exported to a foreign runtime and no native section
    /// of it is active (a protocol violation of the exporting code, see
    /// [`SharedArray::borrow`]).
    #[must_use]
    #[track_caller]
    pub fn nodes(&self) -> ArrayRef<'_, Real> {
        self.nodes.borrow()
    }

    /// The exportable node array (foreign views; metadata).
    #[must_use]
    pub fn nodes_shared(&self) -> &SharedArray<Real> {
        &self.nodes
    }

    /// Number of local nodes (including ghosts).
    #[must_use]
    pub fn nb_nodes(&self) -> usize {
        self.nodes.len()
    }

    /// Node flags.
    #[must_use]
    pub fn nodes_flags(&self) -> &Array<NodeFlag> {
        &self.nodes_flags
    }

    /// Flag of node `n`.
    #[must_use]
    pub fn node_flag(&self, n: Idx) -> NodeFlag {
        self.nodes_flags.as_slice()[to_usize(n)]
    }

    /// Global id of local node `n` (identity when not distributed —
    /// upstream `getNodeGlobalId`).
    #[must_use]
    pub fn node_global_id(&self, n: Idx) -> GlobalIdx {
        match &self.nodes_global_ids {
            Some(g) => g.as_slice()[to_usize(n)],
            None => GlobalIdx::from(n),
        }
    }

    /// Global ids, if the mesh carries them.
    #[must_use]
    pub fn nodes_global_ids(&self) -> Option<&Array<GlobalIdx>> {
        self.nodes_global_ids.as_ref()
    }

    /// Local id of a global node id (upstream `getNodeLocalId`: identity
    /// without global ids, else the first local node with that global id;
    /// `None` = upstream `-1`). Linear search, as upstream.
    #[must_use]
    pub fn node_local_id(&self, global: GlobalIdx) -> Option<Idx> {
        match &self.nodes_global_ids {
            None => Idx::try_from(global).ok().filter(|&n| n >= 0),
            Some(g) => g
                .as_slice()
                .iter()
                .position(|&x| x == global)
                .and_then(|p| Idx::try_from(p).ok()),
        }
    }

    /// Number of global nodes (upstream `getNbGlobalNodes`: the local node
    /// count without global ids).
    #[must_use]
    pub fn nb_global_nodes(&self) -> usize {
        if self.nodes_global_ids.is_some() {
            self.nb_global_nodes
        } else {
            self.nb_nodes()
        }
    }

    /// Flag of node `n`, checked (`Error::OutOfRange`).
    pub fn get_node_flag(&self, n: Idx) -> Result<NodeFlag> {
        let u = try_to_usize(n)?;
        self.nodes_flags
            .as_slice()
            .get(u)
            .copied()
            .ok_or(Error::OutOfRange {
                what: "node",
                index: u,
                len: self.nodes_flags.len(),
            })
    }

    /// Upstream `isPureGhostNode` (checked).
    pub fn is_pure_ghost_node(&self, n: Idx) -> Result<bool> {
        self.get_node_flag(n).map(NodeFlag::is_pure_ghost)
    }

    /// Upstream `isLocalOrMasterNode` (checked): `(flag & 0xCC) == 0`.
    pub fn is_local_or_master_node(&self, n: Idx) -> Result<bool> {
        self.get_node_flag(n).map(NodeFlag::is_local_or_master)
    }

    /// Upstream `isLocalNode` (checked).
    pub fn is_local_node(&self, n: Idx) -> Result<bool> {
        self.get_node_flag(n).map(NodeFlag::is_local)
    }

    /// Upstream `isMasterNode` (checked).
    pub fn is_master_node(&self, n: Idx) -> Result<bool> {
        self.get_node_flag(n).map(NodeFlag::is_master)
    }

    /// Upstream `isSlaveNode` (checked).
    pub fn is_slave_node(&self, n: Idx) -> Result<bool> {
        self.get_node_flag(n).map(NodeFlag::is_slave)
    }

    /// Upstream `isPeriodicSlave` (checked).
    pub fn is_periodic_slave(&self, n: Idx) -> Result<bool> {
        self.get_node_flag(n).map(NodeFlag::is_periodic_slave)
    }

    /// Upstream `isPeriodicMaster` (checked).
    pub fn is_periodic_master(&self, n: Idx) -> Result<bool> {
        self.get_node_flag(n).map(NodeFlag::is_periodic_master)
    }

    /// Owner rank of a shared/ghost node, [`NO_RANK`] for local nodes
    /// (upstream `getNodePrank`).
    #[must_use]
    pub fn node_prank(&self, n: Idx) -> Rank {
        self.nodes_prank.get(&n).copied().unwrap_or(NO_RANK)
    }

    // -------------------------------------------------------------- elements

    /// All connectivities (exportable blocks).
    #[must_use]
    pub fn connectivities(&self) -> &SharedElementTypeMapArray<Idx> {
        &self.connectivities
    }

    /// Connectivity of `(ty, ghost)` (a guard, ARCHITECTURE.md, "Shared storage and its aliasing rules").
    #[inline]
    pub fn connectivity(&self, ty: ElementType, ghost: GhostType) -> Result<ArrayRef<'_, Idx>> {
        self.connectivities.get(ty, ghost)
    }

    /// Nodes of one element (upstream `getConnectivity(element)`): a guard
    /// that dereferences to the connectivity row, no copy (ARCHITECTURE.md, "Shared storage and its aliasing rules").
    /// Like every native guard it must not be held across a
    /// callback into foreign code.
    #[inline]
    pub fn element_connectivity(&self, element: Element) -> Result<SliceRef<'_, Idx>> {
        self.connectivities.element_ref(element)
    }

    /// Number of elements of `(ty, ghost)`, `0` if absent (upstream
    /// `getNbElement(type, ghost)`).
    #[must_use]
    pub fn nb_element(&self, ty: ElementType, ghost: GhostType) -> usize {
        self.connectivities
            .shared(ty, ghost)
            .map_or(0, SharedArray::len)
    }

    /// Number of elements over all types passing `filter`.
    #[must_use]
    pub fn nb_element_filtered(&self, filter: TypeFilter) -> usize {
        self.element_types(filter)
            .map(|t| self.nb_element(t, filter.ghost_type))
            .sum()
    }

    /// Present element types passing `filter`, in enum order.
    pub fn element_types(&self, filter: TypeFilter) -> impl Iterator<Item = ElementType> + '_ {
        self.connectivities.element_types(filter)
    }

    /// Iterate over every element passing `filter` (upstream
    /// `for_each_element`), in deterministic order.
    pub fn elements(&self, filter: TypeFilter) -> impl Iterator<Item = Element> + '_ {
        let g = filter.ghost_type;
        self.element_types(filter).flat_map(move |t| {
            (0..self.nb_element(t, g)).map(move |e| Element::new(t, akantu_core::to_idx(e), g))
        })
    }

    /// Connectivity of `element` with periodic slaves replaced by their
    /// masters (upstream `getConnectivityWithPeriodicity`).
    ///
    /// Returns a `Vec` because the values differ from the stored row (a
    /// borrow is impossible); its callers are set-up paths.
    /// Per-element code that needs the stored row uses
    /// [`Mesh::element_connectivity`] (no copy).
    pub fn connectivity_with_periodicity(&self, element: Element) -> Result<Vec<Idx>> {
        let mut c = self.element_connectivity(element)?.to_vec();
        if self.is_periodic {
            for n in &mut c {
                if let Some(&m) = self.periodic_slave_master.get(n) {
                    *n = m;
                }
            }
        }
        Ok(c)
    }

    /// Global node ids of the facets of local facet type `t` of `element`
    /// (upstream `getFacetConnectivity(element, t)`: one row per facet).
    ///
    /// Returns new rows because they are gathered from the element's row in
    /// the facet's local order (a borrow is impossible); its callers are
    /// set-up paths.
    pub fn facet_connectivity(&self, element: Element, t: usize) -> Result<Vec<Vec<Idx>>> {
        let conn = self.element_connectivity(element)?;
        let fc = element
            .ty()
            .facet_connectivity(t)
            .ok_or(Error::UnsupportedElementType {
                ty: element.ty(),
                context: "Mesh::facet_connectivity (facet type index)",
            })?;
        (0..fc.nb_facets)
            .map(|f| {
                fc.facet(f)
                    .map(|n| {
                        conn.get(n).copied().ok_or(Error::OutOfRange {
                            what: "local node",
                            index: n,
                            len: conn.len(),
                        })
                    })
                    .collect()
            })
            .collect()
    }

    /// Values of a nodal array at the nodes of `element`, column per node
    /// (upstream `extractNodalValuesFromElement`: `nb_component × nb_nodes`
    /// matrix, returned column-major).
    ///
    /// Allocates the returned `Vec` (the values are gathered from scattered
    /// rows, so they cannot be borrowed): fine on set-up paths (its callers
    /// today: geometry factory, CGAL intersectors). Per-element loops use
    /// [`Mesh::extract_nodal_values_from_element_into`] with a reused buffer
    /// (D-056 item 3).
    pub fn extract_nodal_values_from_element<T: Copy>(
        &self,
        nodal: &Array<T>,
        element: Element,
    ) -> Result<Vec<T>> {
        let conn = self.element_connectivity(element)?;
        let mut out = Vec::with_capacity(conn.len() * nodal.nb_component());
        for &n in &conn {
            out.extend_from_slice(nodal.get_tuple(try_to_usize(n)?)?);
        }
        Ok(out)
    }

    /// [`Mesh::extract_nodal_values_from_element`] into `out` (no
    /// allocation): `out` must hold `nb_nodes_per_element × nb_component`
    /// values (else [`Error::DimensionMismatch`], `out` untouched). Same
    /// values and errors otherwise; on a node error `out` holds the values
    /// of the preceding nodes.
    pub fn extract_nodal_values_from_element_into<T: Copy>(
        &self,
        nodal: &Array<T>,
        element: Element,
        out: &mut [T],
    ) -> Result<()> {
        let conn = self.element_connectivity(element)?;
        let nbc = nodal.nb_component();
        if out.len() != conn.len() * nbc {
            return Err(Error::DimensionMismatch {
                context: "Mesh::extract_nodal_values_from_element_into: output length",
                expected: conn.len() * nbc,
                got: out.len(),
            });
        }
        for (k, &n) in conn.iter().enumerate() {
            // in bounds: `out.len() == conn.len() * nbc`, `k < conn.len()`
            out[k * nbc..(k + 1) * nbc].copy_from_slice(nodal.get_tuple(try_to_usize(n)?)?);
        }
        Ok(())
    }

    /// Barycenters of every element of `(ty, ghost)` (upstream
    /// `getBarycenters`), `nb_element × dim`.
    pub fn barycenters(&self, ty: ElementType, ghost: GhostType) -> Result<Array<Real>> {
        let n = self.nb_element(ty, ghost);
        let mut out = Array::try_zeros(n, self.spatial_dimension)?;
        if n == 0 {
            return Ok(out);
        }
        // One guard per array for the whole block; same
        // arithmetic as `barycenter_into` per element.
        let conn = self.connectivity(ty, ghost)?;
        let nodes = self.nodes.read()?;
        for e in 0..n {
            barycenter_of(conn.get_tuple(e)?, &nodes, out.tuple_mut(e))?;
        }
        Ok(out)
    }

    /// Barycenter of an element: arithmetic mean of its node coordinates,
    /// accumulated in local node order (upstream `getBarycenter`).
    pub fn barycenter(&self, element: Element) -> Result<Vec<Real>> {
        let mut x = vec![0.0; self.spatial_dimension];
        self.barycenter_into(element, &mut x)?;
        Ok(x)
    }

    /// [`Mesh::barycenter`] written into `out` (`spatial_dimension`
    /// values), without allocating (same arithmetic).
    pub fn barycenter_into(&self, element: Element, out: &mut [Real]) -> Result<()> {
        if out.len() != self.spatial_dimension {
            return Err(Error::DimensionMismatch {
                context: "Mesh::barycenter_into",
                expected: self.spatial_dimension,
                got: out.len(),
            });
        }
        let conn = self.element_connectivity(element)?;
        let nodes = self.nodes.read()?;
        barycenter_of(&conn, &nodes, out)
    }

    // ------------------------------------------------------ bounding boxes

    /// Global bounding box (valid after [`Mesh::compute_bounding_box`]).
    #[must_use]
    pub fn bbox(&self) -> &BBox {
        &self.bbox
    }

    /// Local (this rank) bounding box.
    #[must_use]
    pub fn local_bbox(&self) -> &BBox {
        &self.bbox_local
    }

    /// Recompute the bounding boxes (upstream `computeBoundingBox`): the
    /// local box over every local node; the global box is reduced over the
    /// communicator **only for a distributed mesh** (collective then; inside
    /// a mesh mutation the domain performs the local and the collective part
    /// as separate, agreed phases), the local box otherwise (D-037
    /// follow-up).
    pub fn compute_bounding_box(&mut self) -> Result<()> {
        let b = self.local_bounding_box();
        let mut g = b.clone();
        if self.is_distributed {
            all_sum_bounds(&self.communicator, &mut g.lower, &mut g.upper)?;
        }
        self.bbox_local = b;
        self.bbox = g;
        Ok(())
    }

    /// Bounding box of this rank's nodes (local, no communication). As
    /// upstream `computeBoundingBox` (`mesh.cc:415-422`, the pure-ghost
    /// test is commented out there) **every** node counts, pure ghosts
    /// included; the global box is unaffected because ghost nodes are
    /// copies of nodes another rank owns.
    #[must_use]
    pub fn local_bounding_box(&self) -> BBox {
        let mut b = BBox::empty(self.spatial_dimension);
        for x in self.nodes.borrow().tuples() {
            b.add_point(x);
        }
        b
    }

    /// Lower corner of the global box (upstream `getLowerBounds`).
    #[must_use]
    pub fn lower_bounds(&self) -> &[Real] {
        &self.bbox.lower
    }

    /// Upper corner of the global box (upstream `getUpperBounds`).
    #[must_use]
    pub fn upper_bounds(&self) -> &[Real] {
        &self.bbox.upper
    }

    /// Deterministic dump of the **complete** mesh state (coordinates,
    /// flags, global ids, owner ranks, connectivities, ghost counters,
    /// bounding boxes, periodicity, groups, mesh data, synchronizer
    /// schemes, revision; recursively the facet mesh), with hash maps
    /// sorted. Used to compare states before/after a rolled-back mutation
    /// and for diagnostics.
    ///
    /// Exportable storage (nodes, connectivities, group lists, numeric mesh
    /// data) is formatted **by value** in a fixed order, never through
    /// `SharedArray`'s metadata-only `Debug`, and export metadata
    /// (allocation generation, export state) is not part of the state
    /// (D-071 (3)): a rollback that restores every value
    /// gives the same fingerprint whether or not the arrays were exported.
    ///
    /// # Panics
    /// As [`SharedArray::borrow`]: an array exported to a foreign runtime
    /// read outside a native section of it (protocol violation).
    #[must_use]
    pub fn state_fingerprint(&self) -> String {
        use std::fmt::Write;
        let mut s = String::new();
        let _ = writeln!(
            s,
            "mesh {} dim={} rev={} distributed={} periodic={} is_facets={}",
            self.id,
            self.spatial_dimension,
            self.revision,
            self.is_distributed,
            self.is_periodic,
            self.is_mesh_facets
        );
        let _ = writeln!(s, "nodes {:?}", self.nodes.borrow().as_slice());
        let flags: Vec<u8> = self
            .nodes_flags
            .as_slice()
            .iter()
            .map(|f| f.bits())
            .collect();
        let _ = writeln!(s, "flags {flags:?}");
        let _ = writeln!(
            s,
            "global_ids {:?}",
            self.nodes_global_ids.as_ref().map(Array::as_slice)
        );
        let mut pranks: Vec<_> = self.nodes_prank.iter().collect();
        pranks.sort();
        let _ = writeln!(s, "pranks {pranks:?}");
        for (ty, ghost, c) in self.connectivities.as_map().iter_all() {
            let _ = writeln!(s, "conn {ty}:{ghost} {:?}", c.borrow().as_slice());
        }
        for (ty, ghost, c) in self.ghosts_counters.as_map().iter_all() {
            let _ = writeln!(s, "ghosts_counters {ty}:{ghost} {:?}", c.as_slice());
        }
        let _ = writeln!(s, "bbox {:?} local {:?}", self.bbox, self.bbox_local);
        let mut sm: Vec<_> = self.periodic_slave_master.iter().collect();
        sm.sort();
        let mut ms: Vec<_> = self.periodic_master_slaves.iter().collect();
        ms.sort();
        let _ = writeln!(s, "periodic {sm:?} {ms:?}");
        // groups and mesh data by value: their lists are exportable
        // `SharedArray`s, whose `Debug` is metadata-only
        let _ = writeln!(s, "groups");
        self.groups.write_state(&mut s);
        let _ = writeln!(s, "data");
        self.data.write_state(&mut s);
        let sync = &self.synchronizers;
        let _ = writeln!(
            s,
            "sync element {:?}",
            sync.element.as_ref().map(|x| (&x.id, &x.communications))
        );
        let _ = writeln!(
            s,
            "sync node {:?}",
            sync.node.as_ref().map(|x| (&x.id, &x.communications))
        );
        let _ = writeln!(
            s,
            "sync periodic {:?}",
            sync.periodic_node
                .as_ref()
                .map(|x| (&x.id, &x.communications))
        );
        if let Some(f) = &self.mesh_facets {
            let _ = writeln!(s, "facets:\n{}", f.state_fingerprint());
        }
        s
    }

    // -------------------------------------------------------------- status

    /// `true` once distributed over several ranks.
    #[must_use]
    pub fn is_distributed(&self) -> bool {
        self.is_distributed
    }

    /// `true` if periodicity was set up.
    #[must_use]
    pub fn is_periodic(&self) -> bool {
        self.is_periodic
    }

    /// `true` for the facet mesh of another mesh.
    #[must_use]
    pub fn is_mesh_facets(&self) -> bool {
        self.is_mesh_facets
    }

    /// Facet mesh, if built (upstream `getMeshFacets`).
    #[must_use]
    pub fn mesh_facets(&self) -> Option<&Mesh> {
        self.mesh_facets.as_deref()
    }

    /// Mutable facet mesh.
    pub fn mesh_facets_mut(&mut self) -> Option<&mut Mesh> {
        self.mesh_facets.as_deref_mut()
    }

    /// Periodic master of a slave node (upstream `getPeriodicMaster`).
    #[must_use]
    pub fn periodic_master(&self, slave: Idx) -> Option<Idx> {
        self.periodic_slave_master.get(&slave).copied()
    }

    /// Periodic slaves of a master node in **insertion order** (the
    /// sequence of upstream's `addPeriodicSlave` calls; empty for a node
    /// that is not a periodic master).
    ///
    /// Upstream's `getPeriodicSlaves` returns the same set but iterates it
    /// in the **reverse** order: the equal range of its
    /// `std::unordered_multimap` master → slaves, whose equivalent keys
    /// iterate in reverse insertion order with the oracle's standard
    /// library (UD-654, established black-box, D-030). A port of an
    /// upstream loop over `getPeriodicSlaves` uses
    /// [`Mesh::periodic_slaves_upstream_order`].
    #[must_use]
    pub fn periodic_slaves(&self, master: Idx) -> &[Idx] {
        self.periodic_master_slaves
            .get(&master)
            .map_or(&[], Vec::as_slice)
    }

    /// Periodic slaves of a master node in upstream's `getPeriodicSlaves`
    /// iteration order: [`Mesh::periodic_slaves`] reversed (UD-654). The
    /// periodic reductions (`make_consistent_for_periodicity`) accumulate
    /// the slaves into their master in this order.
    pub fn periodic_slaves_upstream_order(
        &self,
        master: Idx,
    ) -> std::iter::Copied<std::iter::Rev<std::slice::Iter<'_, Idx>>> {
        self.periodic_slaves(master).iter().rev().copied()
    }

    /// Per-element counters of ghost copies (upstream `getGhostsCounters`,
    /// used by the cohesive inserter in parallel).
    #[must_use]
    pub fn ghosts_counters(&self) -> &ElementTypeMapArray<Idx> {
        &self.ghosts_counters
    }

    /// Groups.
    #[must_use]
    pub fn groups(&self) -> &GroupManager {
        &self.groups
    }

    /// Mutable groups (group edits do not change topology).
    pub fn groups_mut(&mut self) -> &mut GroupManager {
        &mut self.groups
    }

    /// Mesh data (upstream `MeshData`: `tag_0`, `physical_names`,
    /// `element_to_subelement`, …).
    #[must_use]
    pub fn data(&self) -> &MeshData {
        &self.data
    }

    /// Mutable mesh data.
    pub fn data_mut(&mut self) -> &mut MeshData {
        &mut self.data
    }

    /// Ghost-exchange schemes (empty in serial).
    #[must_use]
    pub fn synchronizers(&self) -> &MeshSynchronizers {
        &self.synchronizers
    }

    /// Upstream `createGroupsFromMeshData<T>(name)` on this mesh's data and
    /// connectivities (see [`GroupManager::create_groups_from_mesh_data`]).
    pub fn create_groups_from_mesh_data(&mut self, name: &str) -> Result<()> {
        self.groups
            .create_groups_from_mesh_data(&self.data, &self.connectivities, name)
    }

    /// Upstream `ElementGroup::add(element, add_nodes, check_for_duplicate)`
    /// on the element group `group` (the nodes come from this mesh).
    pub fn element_group_add(
        &mut self,
        group: &str,
        element: Element,
        add_nodes: bool,
        check_for_duplicate: bool,
    ) -> Result<()> {
        // borrowed, not copied; the `else` validates
        let nodes = if add_nodes {
            Some(self.connectivities.element_ref(element)?)
        } else {
            self.connectivities.element_ref(element)?;
            None
        };
        self.groups
            .element_group_mut(group)?
            .add(element, nodes.as_deref(), check_for_duplicate)
    }

    /// Upstream `ElementGroup::fillFromNodeGroup` on the element group
    /// `group` of this mesh.
    pub fn fill_element_group_from_node_group(&mut self, group: &str) -> Result<()> {
        let d = self.groups.element_group(group)?.dimension;
        let csr = crate::utils::build_node_to_elements(
            self,
            (d != crate::group::ALL_DIMENSIONS).then_some(d),
        )?;
        self.groups
            .fill_from_node_group(group, &csr, &self.connectivities)
    }

    /// Upstream `createElementGroupFromNodeGroup(name, node_group,
    /// dimension)` (`dimension` = [`crate::group::ALL_DIMENSIONS`] for
    /// `_all_dimensions`, upstream's default).
    pub fn create_element_group_from_node_group(
        &mut self,
        name: &str,
        node_group: &str,
        dimension: usize,
    ) -> Result<()> {
        let csr = crate::utils::build_node_to_elements(
            self,
            (dimension != crate::group::ALL_DIMENSIONS).then_some(dimension),
        )?;
        self.groups.create_element_group_from_node_group(
            name,
            node_group,
            dimension,
            &csr,
            &self.connectivities,
        )
    }

    /// Deterministic text description (upstream `Mesh::printself`: id,
    /// dimension, nodes, connectivities, groups).
    #[must_use]
    pub fn describe(&self) -> String {
        use std::fmt::Write;
        let mut s = String::from("Mesh [\n");
        let _ = writeln!(s, " + id                : {}", self.id);
        let _ = writeln!(s, " + spatial dimension : {}", self.spatial_dimension);
        let _ = writeln!(
            s,
            " + nodes [ {} x {} ] {:?}",
            self.nodes.len(),
            self.nodes.nb_component(),
            self.nodes.borrow().as_slice()
        );
        let _ = writeln!(s, " + connectivities [");
        for (ty, ghost, c) in self.connectivities.as_map().iter_all() {
            let _ = writeln!(
                s,
                "  + {ty}:{ghost} [ {} x {} ] {:?}",
                c.len(),
                c.nb_component(),
                c.borrow().as_slice()
            );
        }
        let _ = writeln!(s, " ]");
        s.push_str(&self.groups.describe(1));
        s.push_str("]\n");
        s
    }

    /// Validate the structural invariants listed on [`Mesh`].
    pub fn check_consistency(&self) -> Result<()> {
        let nb_nodes = self.nb_nodes();
        if self.nodes.nb_component() != self.spatial_dimension {
            return Err(Error::Mesh(
                "nodes have the wrong number of components".into(),
            ));
        }
        if self.nodes_flags.len() != nb_nodes {
            return Err(Error::Mesh("nodes_flags size differs from nb_nodes".into()));
        }
        if let Some(g) = &self.nodes_global_ids
            && g.len() != nb_nodes
        {
            return Err(Error::Mesh(
                "nodes_global_ids size differs from nb_nodes".into(),
            ));
        }
        for (ty, ghost, conn) in self.connectivities.as_map().iter_all() {
            if conn.nb_component() != ty.nb_nodes_per_element() {
                return Err(Error::Mesh(format!(
                    "connectivity {ty}:{ghost} has {} components, expected {}",
                    conn.nb_component(),
                    ty.nb_nodes_per_element()
                )));
            }
            if let Some(&bad) = conn
                .borrow()
                .as_slice()
                .iter()
                .find(|&&n| !try_to_usize(n).is_ok_and(|n| n < nb_nodes))
            {
                return Err(Error::Mesh(format!(
                    "connectivity {ty}:{ghost} references node {bad} outside [0, {nb_nodes})"
                )));
            }
        }
        Ok(())
    }
}

/// Arithmetic mean of the coordinates of the nodes `conn`, accumulated in
/// local node order then divided by the node count (upstream
/// `getBarycenter`), written into `out` (`spatial_dimension` values).
fn barycenter_of(conn: &[Idx], nodes: &Array<Real>, out: &mut [Real]) -> Result<()> {
    out.fill(0.0);
    for &n in conn {
        for (xi, &c) in out.iter_mut().zip(nodes.get_tuple(try_to_usize(n)?)?) {
            *xi += c;
        }
    }
    #[allow(clippy::cast_precision_loss)] // node counts are tiny
    let inv = conn.len() as Real;
    for xi in out {
        *xi /= inv;
    }
    Ok(())
}

/// Upstream `BBox::allSum` (`aka_bbox.hh:172-185`) on concatenated bounds:
/// **one** `Min` reduction of `[lower, −1·upper]`, the upper bounds negated
/// back. Same values as a `Min`/`Max` pair, and the same operand on ties
/// (`±0`) as upstream whatever the MPI implementation's tie rule for
/// `MIN`/`MAX` (decision D-051). Negation is upstream's
/// `Real(-1.) *` bit for bit on every non-NaN value, and bounds are never
/// NaN (`BBox::add_point` skips NaN coordinates). Collective.
pub(crate) fn all_sum_bounds(
    comm: &akantu_comm::Communicator,
    lower: &mut [Real],
    upper: &mut [Real],
) -> Result<()> {
    let mut rec: Vec<Real> = lower
        .iter()
        .copied()
        .chain(upper.iter().map(|&u| -u))
        .collect();
    comm.all_reduce(&mut rec, akantu_comm::ReduceOp::Min)?;
    let (lo, up) = rec.split_at(lower.len());
    lower.copy_from_slice(lo);
    for (u, &r) in upper.iter_mut().zip(up) {
        *u = -r;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::accessor::MeshAccessor;

    fn two_triangles() -> Mesh {
        let mut m = Mesh::new(2, "m").unwrap();
        let mut a = MeshAccessor::new(&mut m);
        for x in [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]] {
            a.push_node(&x).unwrap();
        }
        let mut conn = a.connectivity_mut(ElementType::Triangle3, GhostType::NotGhost);
        conn.push_tuple(&[0, 1, 2]);
        conn.push_tuple(&[0, 2, 3]);
        drop(conn);
        m
    }

    #[test]
    fn basic_queries() {
        let mut m = two_triangles();
        assert!(Mesh::new(4, "bad").is_err());
        m.check_consistency().unwrap();
        assert_eq!(m.nb_nodes(), 4);
        assert_eq!(m.nb_element(ElementType::Triangle3, GhostType::NotGhost), 2);
        assert_eq!(
            m.nb_element(ElementType::Quadrangle4, GhostType::NotGhost),
            0
        );
        assert_eq!(m.nb_element_filtered(TypeFilter::new().dim(2)), 2);
        let e = Element::new(ElementType::Triangle3, 1, GhostType::NotGhost);
        assert_eq!(m.element_connectivity(e).unwrap(), &[0, 2, 3]);
        let b = m.barycenter(e).unwrap();
        assert!((b[0] - 1.0 / 3.0).abs() < 1e-15 && (b[1] - 2.0 / 3.0).abs() < 1e-15);
        assert_eq!(m.elements(TypeFilter::new()).count(), 2);
        assert_eq!(m.node_global_id(3), 3);
        assert_eq!(m.node_prank(3), NO_RANK);
        assert!(m.node_flag(0).is_local());
        m.compute_bounding_box().unwrap();
        assert_eq!(m.bbox().lower, vec![0.0, 0.0]);
        assert_eq!(m.bbox().upper, vec![1.0, 1.0]);
        assert!(!m.is_distributed() && !m.is_periodic() && m.mesh_facets().is_none());
    }

    #[test]
    fn element_queries_and_global_ids() {
        let mut m = two_triangles();
        let e = Element::new(ElementType::Triangle3, 1, GhostType::NotGhost);
        // facets of triangle 1 = (0,2,3): (0,2), (2,3), (3,0)
        assert_eq!(
            m.facet_connectivity(e, 0).unwrap(),
            vec![vec![0, 2], vec![2, 3], vec![3, 0]]
        );
        assert!(m.facet_connectivity(e, 1).is_err());
        let vals = Array::from_vec(vec![0.0, 1.0, 10.0, 11.0, 20.0, 21.0, 30.0, 31.0], 2).unwrap();
        assert_eq!(
            m.extract_nodal_values_from_element(&vals, e).unwrap(),
            vec![0.0, 1.0, 20.0, 21.0, 30.0, 31.0]
        );
        // the non-allocating form: same values, length checked
        let mut buf = [f64::NAN; 6];
        m.extract_nodal_values_from_element_into(&vals, e, &mut buf)
            .unwrap();
        assert_eq!(buf, [0.0, 1.0, 20.0, 21.0, 30.0, 31.0]);
        let mut short = [f64::NAN; 5];
        assert!(matches!(
            m.extract_nodal_values_from_element_into(&vals, e, &mut short),
            Err(Error::DimensionMismatch {
                expected: 6,
                got: 5,
                ..
            })
        ));
        assert!(short.iter().all(|x| x.is_nan()), "untouched on error");
        let three_nodes = Array::from_vec(vec![0.0, 1.0, 10.0, 11.0, 20.0, 21.0], 2).unwrap();
        assert_eq!(
            m.extract_nodal_values_from_element_into(&three_nodes, e, &mut buf)
                .unwrap_err()
                .to_string(),
            m.extract_nodal_values_from_element(&three_nodes, e)
                .unwrap_err()
                .to_string(),
            "same node error as the Vec form"
        );
        assert_eq!(m.connectivity_with_periodicity(e).unwrap(), vec![0, 2, 3]);
        let b = m
            .barycenters(ElementType::Triangle3, GhostType::NotGhost)
            .unwrap();
        assert_eq!(b.len(), 2);
        assert_eq!(b.tuple(1), m.barycenter(e).unwrap().as_slice());
        assert_eq!(m.node_local_id(3), Some(3));
        assert_eq!(m.node_local_id(-4), None);
        assert_eq!(m.nb_global_nodes(), 4);
        {
            let mut a = MeshAccessor::new(&mut m);
            a.nodes_global_ids_mut()
                .as_mut_slice()
                .copy_from_slice(&[10, 11, 12, 13]);
            a.add_periodic_pair(3, 0).unwrap();
        }
        assert_eq!(m.node_local_id(12), Some(2));
        assert_eq!(m.node_local_id(2), None);
        assert_eq!(m.connectivity_with_periodicity(e).unwrap(), vec![0, 2, 0]);
        assert!(m.is_periodic_slave(3).unwrap() && m.is_periodic_master(0).unwrap());
        assert!(m.is_local_node(1).unwrap() && m.is_local_or_master_node(1).unwrap());
        assert!(!m.is_master_node(1).unwrap() && !m.is_slave_node(1).unwrap());
        assert!(!m.is_pure_ghost_node(1).unwrap());
        assert!(m.get_node_flag(4).is_err() && m.is_local_node(-1).is_err());
        assert!(m.describe().contains("_triangle_3"));
    }

    #[test]
    fn consistency_errors() {
        let mut m = two_triangles();
        MeshAccessor::new(&mut m)
            .connectivity_mut(ElementType::Triangle3, GhostType::NotGhost)
            .push_tuple(&[0, 1, 9]);
        assert!(m.check_consistency().is_err());
    }
}
