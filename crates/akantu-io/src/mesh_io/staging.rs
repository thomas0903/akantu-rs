// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/io/mesh_io/mesh_io_msh.cc,
// src/io/mesh_io/mesh_io_diana.cc, src/io/mesh_io.cc (what the readers
// store through `MeshAccessor`, `MeshData` and `GroupManager`),
// src/mesh/group_manager.cc (`createGroupsFromMeshData`),
// src/mesh/element_group.cc (`optimize`).

//! Staging area of a mesh being read.
//!
//! Readers fill a [`MeshBuild`]; [`MeshBuild::prepare`] validates it and
//! builds everything the mesh will receive — arrays, datasets, the complete
//! group manager and the bounding boxes — and the target [`Mesh`] is only
//! modified by [`PreparedMesh::apply`], once the whole file has been parsed
//! and validated and any other fallible work of the read is done: a failing
//! read leaves the mesh untouched (UD-049: no partial mutation).
//!
//! A read never communicates: the target is not
//! distributed, so its global bounding box is its local box, exactly as
//! upstream `Mesh::computeBoundingBox` (`mesh.cc:415-434`) reduces over the
//! communicator only for distributed meshes, and as reading on one rank
//! before distributing (upstream `if (prank == 0) mesh.read(…)`,
//! `docs/ARCHITECTURE.md`) requires. [`PreparedMesh::apply`] performs
//! its checks and allocations first and then only moves values, so no
//! fallible step follows its first change of the mesh.
//!
//! As upstream readers end with `MeshUtils::fillElementToSubElementsData`
//! (`mesh_io_msh.cc:919`, `mesh_io_diana.cc:174`), the
//! staged read computes `element_to_subelement` / `subelement_to_element`
//! in [`MeshBuild::prepare`] (local, no communication, D-037) and
//! [`PreparedMesh::apply`] installs them with the other datasets.

use std::collections::BTreeMap;

use akantu_core::{
    Array, ElementType, ElementTypeMapArray, Error, GhostType, Idx, Int, NodeFlag, Real, Result,
    try_idx,
};
use akantu_mesh::data::NodalDataValues;
use akantu_mesh::{BBox, GroupManager, Mesh, MeshAccessor, MeshDataValues};

/// Per-type `i32` dataset whose arrays may be shorter than the number of
/// elements while reading (upstream resizes tag arrays lazily); missing
/// trailing entries take `fill` at commit time.
#[derive(Debug, Clone, Default)]
pub(crate) struct IntData {
    pub(crate) per_type: BTreeMap<ElementType, Vec<Int>>,
    pub(crate) fill: Int,
}

/// Elemental `f64` dataset (`$ElementData` / `$ElementNodeData`).
#[derive(Debug, Clone)]
pub(crate) struct RealElementalData {
    /// Components per element (for element-node data: per element, i.e.
    /// `nb_component × nb_nodes_per_element`).
    pub(crate) per_type: BTreeMap<ElementType, (usize, Vec<Real>)>,
    /// Upstream `ElementTypeMapArray::isNodal()`.
    pub(crate) nodal: bool,
}

/// Element group being built.
#[derive(Debug, Clone, Default)]
pub(crate) struct GroupBuild {
    /// Upstream `ElementGroup::dimension` (`None` = `_all_dimensions`
    /// while empty).
    pub(crate) dimension: Option<usize>,
    pub(crate) elements: BTreeMap<ElementType, Vec<Idx>>,
    pub(crate) nodes: Vec<Idx>,
}

impl GroupBuild {
    /// Upstream `ElementGroup::addDimension`.
    pub(crate) fn add_dimension(&mut self, d: usize) {
        self.dimension = Some(self.dimension.map_or(d, |c| c.max(d)));
    }

    /// Upstream `ElementGroup::optimize` (sort + unique elements and nodes).
    pub(crate) fn optimize(&mut self) {
        for els in self.elements.values_mut() {
            els.sort_unstable();
            els.dedup();
        }
        self.optimize_nodes();
    }

    /// Upstream `NodeGroup::optimize`.
    pub(crate) fn optimize_nodes(&mut self) {
        self.nodes.sort_unstable();
        self.nodes.dedup();
    }
}

/// Everything a reader produces.
#[derive(Debug, Clone)]
pub(crate) struct MeshBuild {
    pub(crate) dim: usize,
    /// Node coordinates, `dim` values per node.
    pub(crate) nodes: Vec<Real>,
    /// Not-ghost connectivities (`nb_nodes_per_element` values per element).
    pub(crate) conn: BTreeMap<ElementType, Vec<Idx>>,
    pub(crate) int_data: BTreeMap<String, IntData>,
    pub(crate) string_data: BTreeMap<String, BTreeMap<ElementType, Vec<String>>>,
    pub(crate) real_data: BTreeMap<String, RealElementalData>,
    /// Nodal `f64` datasets: `(nb_component, values)` with one tuple per
    /// node.
    pub(crate) nodal_data: BTreeMap<String, (usize, Vec<Real>)>,
    pub(crate) element_groups: BTreeMap<String, GroupBuild>,
    pub(crate) node_groups: BTreeMap<String, Vec<Idx>>,
}

impl MeshBuild {
    pub(crate) fn new(dim: usize) -> Self {
        Self {
            dim,
            nodes: Vec::new(),
            conn: BTreeMap::new(),
            int_data: BTreeMap::new(),
            string_data: BTreeMap::new(),
            real_data: BTreeMap::new(),
            nodal_data: BTreeMap::new(),
            element_groups: BTreeMap::new(),
            node_groups: BTreeMap::new(),
        }
    }

    /// Number of nodes read so far.
    pub(crate) fn nb_nodes(&self) -> usize {
        self.nodes.len() / self.dim
    }

    /// Append a node (first `dim` coordinates of `x`, missing ones = 0);
    /// returns its index.
    pub(crate) fn push_node(&mut self, x: &[Real]) -> Result<Idx> {
        let n = try_idx(self.nb_nodes())?;
        for d in 0..self.dim {
            self.nodes.push(x.get(d).copied().unwrap_or(0.0));
        }
        Ok(n)
    }

    /// Number of elements of `ty` read so far.
    pub(crate) fn nb_elements(&self, ty: ElementType) -> usize {
        self.conn
            .get(&ty)
            .map_or(0, |c| c.len() / ty.nb_nodes_per_element().max(1))
    }

    /// Append an element; returns its index within `ty`.
    pub(crate) fn push_element(&mut self, ty: ElementType, nodes: &[Idx]) -> Result<Idx> {
        debug_assert_eq!(nodes.len(), ty.nb_nodes_per_element());
        let e = try_idx(self.nb_elements(ty))?;
        self.conn.entry(ty).or_default().extend_from_slice(nodes);
        Ok(e)
    }

    /// Set `name[ty][e] = value`, padding skipped entries with the
    /// dataset's fill value (upstream `getDataPointer` resizes the array to
    /// the current element count before `push_back`; the padding is
    /// uninitialised upstream, UD-064).
    pub(crate) fn set_int(&mut self, name: &str, ty: ElementType, e: usize, value: Int, fill: Int) {
        let data = match self.int_data.get_mut(name) {
            Some(d) => d,
            None => self
                .int_data
                .entry(name.to_owned())
                .or_insert_with(|| IntData {
                    per_type: BTreeMap::new(),
                    fill,
                }),
        };
        let fill = data.fill;
        let v = data.per_type.entry(ty).or_default();
        if v.len() <= e {
            v.resize(e + 1, fill);
        }
        v[e] = value;
    }

    /// Element group `name`, created with `dimension` if absent (upstream
    /// `createElementGroup` / `getElementGroup().addDimension`).
    pub(crate) fn group_mut(&mut self, name: &str, dimension: Option<usize>) -> &mut GroupBuild {
        let g = self.element_groups.entry(name.to_owned()).or_default();
        if let Some(d) = dimension {
            g.add_dimension(d);
        }
        g
    }

    /// Upstream `ElementGroup::add(element, add_nodes, false)`.
    pub(crate) fn group_add(&mut self, name: &str, ty: ElementType, e: Idx, add_nodes: bool) {
        let g = match self.element_groups.get_mut(name) {
            Some(g) => g,
            None => self.element_groups.entry(name.to_owned()).or_default(),
        };
        match g.elements.get_mut(&ty) {
            Some(v) => v.push(e),
            None => g.elements.entry(ty).or_default().push(e),
        }
        g.add_dimension(ty.spatial_dimension());
        if add_nodes {
            let n = ty.nb_nodes_per_element();
            let e = usize::try_from(e).unwrap_or(0);
            if let Some(c) = self.conn.get(&ty) {
                g.nodes.extend_from_slice(&c[e * n..(e + 1) * n]);
            }
        }
    }

    /// Upstream `GroupManager::createGroupsFromMeshData<std::string>(name)`:
    /// one element group per distinct value, elements added with their
    /// nodes in (type, element) order, then `optimize()`.
    pub(crate) fn create_groups_from_string_data(&mut self, name: &str) -> Result<()> {
        let Some(data) = self.string_data.get(name) else {
            return Ok(());
        };
        let mut members: BTreeMap<String, Vec<(ElementType, Idx)>> = BTreeMap::new();
        // Consecutive elements mostly share their value: the member list of
        // the previous one is found without a lookup.
        let mut last: Option<&str> = None;
        let mut last_list: Option<&mut Vec<(ElementType, Idx)>> = None;
        for (&ty, values) in data {
            if values.len() != self.nb_elements(ty) {
                return Err(Error::Mesh(format!(
                    "mesh data '{name}' has {} entries for {ty}, the mesh {} elements",
                    values.len(),
                    self.nb_elements(ty)
                )));
            }
            for (e, v) in values.iter().enumerate() {
                let item = (ty, try_idx(e)?);
                if let (Some(l), Some(list)) = (last, last_list.as_deref_mut())
                    && l == v.as_str()
                {
                    list.push(item);
                    continue;
                }
                if !members.contains_key(v.as_str()) {
                    members.insert(v.clone(), Vec::new());
                }
                let list = members.get_mut(v.as_str()).expect("inserted above");
                list.push(item);
                last = Some(v.as_str());
                last_list = Some(list);
            }
        }
        for (group, els) in members {
            for &(ty, _) in &els {
                self.group_mut(&group, Some(ty.spatial_dimension()));
            }
            for (ty, e) in els {
                self.group_add(&group, ty, e, true);
            }
            self.group_mut(&group, None).optimize();
        }
        Ok(())
    }

    /// Fail if `mesh` already holds anything a reader would write.
    pub(crate) fn check_target(mesh: &Mesh) -> Result<()> {
        let has_elements = mesh.connectivities().total_len() > 0
            || mesh.connectivities().as_map().iter_all().next().is_some();
        if mesh.nb_nodes() > 0
            || has_elements
            || !mesh.data().is_empty()
            || mesh.groups().element_groups().next().is_some()
            || mesh.groups().node_groups().next().is_some()
        {
            return Err(Error::invalid(format!(
                "mesh '{}' is not empty: mesh readers fill a freshly created mesh",
                mesh.id()
            )));
        }
        if mesh.is_distributed() {
            return Err(Error::invalid(
                "you cannot read a mesh that is already distributed",
            ));
        }
        Ok(())
    }

    /// Validate every index against `mesh` (which must satisfy
    /// [`MeshBuild::check_target`]) and build every array and dataset the
    /// mesh will receive: all the fallible work of a read, without touching
    /// the mesh. [`PreparedMesh::apply`] then only moves the data
    /// (the file-based readers do their remaining fallible work, such as
    /// writing the Diana material file, between the two).
    #[allow(clippy::too_many_lines)] // one linear validation checklist
    pub(crate) fn prepare(self, mesh: &Mesh) -> Result<PreparedMesh> {
        Self::check_target(mesh)?;
        if mesh.spatial_dimension() != self.dim {
            return Err(Error::DimensionMismatch {
                context: "mesh reader commit (spatial dimension)",
                expected: mesh.spatial_dimension(),
                got: self.dim,
            });
        }
        let nb_nodes = self.nb_nodes();
        try_idx(nb_nodes)?;
        for (ty, c) in &self.conn {
            if let Some(&bad) = c
                .iter()
                .find(|&&n| usize::try_from(n).map_or(true, |n| n >= nb_nodes))
            {
                return Err(Error::Mesh(format!(
                    "{ty} connectivity references node {bad} outside [0, {nb_nodes})"
                )));
            }
        }
        let count = |ty: ElementType| self.nb_elements(ty);
        let mut conn = Vec::new();
        for (&ty, c) in &self.conn {
            conn.push((ty, Array::from_vec(c.clone(), ty.nb_nodes_per_element())?));
        }
        let mut int_maps = Vec::new();
        for (name, d) in &self.int_data {
            let mut m = ElementTypeMapArray::new(name.clone());
            for (&ty, v) in &d.per_type {
                let mut v = v.clone();
                if v.len() > count(ty) {
                    return Err(Error::Mesh(format!(
                        "mesh data '{name}' has more entries than {ty} elements"
                    )));
                }
                v.resize(count(ty), d.fill);
                m.as_map_mut()
                    .insert(ty, GhostType::NotGhost, Array::from_vec(v, 1)?)?;
            }
            int_maps.push((name.clone(), m));
        }
        let mut string_maps = Vec::new();
        for (name, d) in &self.string_data {
            let mut m = ElementTypeMapArray::new(name.clone());
            for (&ty, v) in d {
                if v.len() != count(ty) {
                    return Err(Error::Mesh(format!(
                        "mesh data '{name}' size differs from the number of {ty} elements"
                    )));
                }
                m.as_map_mut()
                    .insert(ty, GhostType::NotGhost, Array::from_vec(v.clone(), 1)?)?;
            }
            string_maps.push((name.clone(), m));
        }
        let mut real_maps = Vec::new();
        for (name, d) in &self.real_data {
            let mut m = ElementTypeMapArray::new(name.clone());
            for (&ty, (nc, v)) in &d.per_type {
                let a = Array::try_from_vec_with_len(v.clone(), count(ty), *nc)?;
                m.as_map_mut().insert(ty, GhostType::NotGhost, a)?;
            }
            real_maps.push((name.clone(), m, d.nodal));
        }
        let mut nodal = Vec::new();
        for (name, (nc, v)) in &self.nodal_data {
            nodal.push((
                name.clone(),
                Array::try_from_vec_with_len(v.clone(), nb_nodes, *nc)?,
            ));
        }
        let check_nodes = |what: &str, nodes: &[Idx]| -> Result<()> {
            match nodes
                .iter()
                .find(|&&n| usize::try_from(n).map_or(true, |n| n >= nb_nodes))
            {
                Some(bad) => Err(Error::Mesh(format!(
                    "{what} references node {bad} outside [0, {nb_nodes})"
                ))),
                None => Ok(()),
            }
        };
        for (name, g) in &self.element_groups {
            check_nodes(&format!("element group '{name}'"), &g.nodes)?;
            for (&ty, els) in &g.elements {
                if let Some(bad) = els
                    .iter()
                    .find(|&&e| usize::try_from(e).map_or(true, |e| e >= count(ty)))
                {
                    return Err(Error::Mesh(format!(
                        "element group '{name}' references {ty} element {bad} outside [0, {})",
                        count(ty)
                    )));
                }
            }
        }
        for (name, nodes) in &self.node_groups {
            check_nodes(&format!("node group '{name}'"), nodes)?;
            // Element group `X` owns node group `X_nodes`; the
            // readers report such clashes with a location.
            if let Some(owner) = name.strip_suffix("_nodes")
                && self.element_groups.contains_key(owner)
            {
                return Err(Error::Mesh(format!(
                    "node group '{name}' clashes with the node group of element group '{owner}'"
                )));
            }
        }
        // The complete group manager (the target has no groups): every
        // creation, and hence every name clash, fails here, before the mesh
        // is touched.
        let mut groups = GroupManager::default();
        for (name, nodes) in self.node_groups {
            groups.create_node_group(&name)?.set_nodes(nodes);
        }
        for (name, g) in self.element_groups {
            // An empty group keeps upstream's `_all_dimensions`; the Rust
            // group stores 0 for it (no element, no dimension). Creating it
            // also creates its node group `<name>_nodes` (upstream
            // `createElementGroup`).
            let mut eg = groups.create_element_group(&name, g.dimension.unwrap_or(0))?;
            eg.node_group.set_nodes(g.nodes);
            for (ty, els) in g.elements {
                eg.elements.as_map_mut().replace(
                    ty,
                    GhostType::NotGhost,
                    Array::from_vec(els, 1)?.into(),
                );
            }
        }
        let (conn, adjacency) = element_to_subelements(self.dim, nb_nodes, conn)?;
        // Bounding box (upstream `Mesh::makeReady` → `computeBoundingBox`):
        // every node of a freshly read mesh is a normal node and the mesh is
        // not distributed, so the global box is the local one.
        let mut bbox = BBox::empty(self.dim);
        for x in self.nodes.chunks_exact(self.dim) {
            bbox.add_point(x);
        }
        Ok(PreparedMesh {
            dim: self.dim,
            nb_nodes,
            nodes: self.nodes,
            conn,
            int_maps,
            string_maps,
            real_maps,
            nodal,
            groups,
            bbox,
            adjacency,
        })
    }
}

/// `(element_to_subelement, subelement_to_element)` of a read mesh.
type Adjacency = akantu_mesh::utils::ElementAdjacency;

/// Staged connectivity blocks of a read.
type Blocks = Vec<(ElementType, Array<Idx>)>;

/// `element_to_subelement` / `subelement_to_element` of the read mesh
/// (upstream `fillElementToSubElementsData` at the end of every reader),
/// computed on a scratch mesh holding the staged connectivities (moved in
/// and back out; coordinates are not needed). `None` when the mesh has no
/// element of dimension − 1.
fn element_to_subelements(
    dim: usize,
    nb_nodes: usize,
    conn: Blocks,
) -> Result<(Blocks, Option<Adjacency>)> {
    let mut scratch = Mesh::new(dim, "read")?;
    {
        let mut acc = MeshAccessor::new(&mut scratch);
        acc.resize_nodes(nb_nodes)?;
        for (ty, a) in conn {
            acc.connectivity_mut(ty, GhostType::NotGhost).replace(a);
        }
    }
    let adjacency = akantu_mesh::utils::compute_element_to_subelements_data(&scratch);
    let mut back = Vec::new();
    let types: Vec<ElementType> = scratch
        .element_types(akantu_core::TypeFilter::new().ghost(GhostType::NotGhost))
        .collect();
    let mut acc = MeshAccessor::new(&mut scratch);
    for ty in types {
        let a = acc
            .connectivity_mut(ty, GhostType::NotGhost)
            .replace(Array::new(ty.nb_nodes_per_element().max(1)));
        back.push((ty, a));
    }
    Ok((back, adjacency?))
}

/// A validated read (see [`MeshBuild::prepare`]).
#[derive(Debug)]
pub(crate) struct PreparedMesh {
    dim: usize,
    nb_nodes: usize,
    nodes: Vec<Real>,
    conn: Vec<(ElementType, Array<Idx>)>,
    int_maps: Vec<(String, ElementTypeMapArray<Int>)>,
    string_maps: Vec<(String, ElementTypeMapArray<String>)>,
    real_maps: Vec<(String, ElementTypeMapArray<Real>, bool)>,
    nodal: Vec<(String, Array<Real>)>,
    /// Every element group (with its `<name>_nodes` node group) and node
    /// group of the read.
    groups: GroupManager,
    /// Local (= global, not distributed) bounding box of the nodes.
    bbox: BBox,
    /// `element_to_subelement` / `subelement_to_element` (upstream
    /// `fillElementToSubElementsData`), if the mesh has facets.
    adjacency: Option<Adjacency>,
}

impl PreparedMesh {
    /// Move the prepared content into `mesh`, which must still satisfy
    /// [`MeshBuild::check_target`]. Every check and allocation (target
    /// emptiness, dimensions, the node, flag and global-id arrays) happens
    /// before the mesh is changed; after that only moves and infallible
    /// insertions remain — no group creation, no communication —
    /// so an error always leaves `mesh` exactly as it was (revision
    /// included).
    pub(crate) fn apply(self, mesh: &mut Mesh) -> Result<()> {
        // ---- checks and allocations: `mesh` is not modified ------------
        MeshBuild::check_target(mesh)?;
        if mesh.spatial_dimension() != self.dim || self.nodes.len() != self.nb_nodes * self.dim {
            return Err(Error::DimensionMismatch {
                context: "mesh reader commit (spatial dimension)",
                expected: mesh.spatial_dimension(),
                got: self.dim,
            });
        }
        if self.bbox.lower.len() != self.dim || self.bbox.upper.len() != self.dim {
            return Err(Error::DimensionMismatch {
                context: "mesh reader commit (bounding box)",
                expected: self.dim,
                got: self.bbox.lower.len(),
            });
        }
        let mut nodes = Array::try_from_vec_with_len(self.nodes, self.nb_nodes, self.dim)?;
        nodes.set_id(mesh.nodes().id().to_owned());
        // The target has no nodes: flags (and global ids, if the target
        // stores them) grow from empty as `MeshAccessor::resize_nodes`
        // would grow them.
        let mut flags = mesh.nodes_flags().clone();
        flags.try_resize(self.nb_nodes, NodeFlag::NORMAL)?;
        let global_ids = match mesh.nodes_global_ids() {
            Some(g) => {
                let mut g = g.clone();
                g.try_resize(self.nb_nodes, akantu_core::INVALID_GLOBAL_IDX)?;
                Some(g)
            }
            None => None,
        };
        // ---- commit: moves only ------------------------------------------
        {
            let mut acc = MeshAccessor::new(mesh);
            // Cannot fail: both boxes have `dim` coordinates (checked above).
            acc.set_bounding_boxes(self.bbox.clone(), self.bbox)?;
            acc.nodes_mut().replace(nodes);
            *acc.nodes_flags_mut() = flags;
            if let Some(g) = global_ids {
                *acc.nodes_global_ids_mut() = g;
            }
            for (ty, mut a) in self.conn {
                let mut slot = acc.connectivity_mut(ty, GhostType::NotGhost);
                a.set_id(slot.id().to_owned());
                slot.replace(a);
            }
        }
        let data = mesh.data_mut();
        for (name, m) in self.int_maps {
            data.insert_elemental(name, MeshDataValues::Int(m.into()));
        }
        for (name, m) in self.string_maps {
            data.insert_elemental(name, MeshDataValues::String(m));
        }
        for (name, m, is_nodal) in self.real_maps {
            data.set_elemental_nodal(&name, is_nodal);
            data.insert_elemental(name, MeshDataValues::Real(m.into()));
        }
        for (name, a) in self.nodal {
            data.insert_nodal(name, NodalDataValues::Real(a));
        }
        if let Some((e2s, s2e)) = self.adjacency {
            data.insert_elemental(
                akantu_mesh::utils::ELEMENT_TO_SUBELEMENT,
                MeshDataValues::ElementList(e2s),
            );
            data.insert_elemental(
                akantu_mesh::utils::SUBELEMENT_TO_ELEMENT,
                MeshDataValues::Element(s2e),
            );
        }
        // The target has no groups (checked above): install the prepared
        // manager as a whole.
        *mesh.groups_mut() = self.groups;
        Ok(())
    }
}
