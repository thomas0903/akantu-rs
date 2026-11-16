// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/mesh_utils/mesh_utils.hh, mesh_utils.cc,
// mesh_utils_inline_impl.hh, src/mesh/mesh.cc (initMeshFacets,
// fillNodesToElements, getAssociatedElements, getGlobalConnectivity),
// src/mesh/mesh_inline_impl.hh (getBarycenter, getFacetConnectivity,
// getElementToSubelement/getSubelementToElement data accessors).

//! Topological utilities (upstream `MeshUtils`).
//!
//! All functions are deterministic and reproduce the upstream iteration
//! order: ghost type (`NotGhost` first), element type in enum order,
//! element index, then facet type and local facet order. This order
//! determines facet numbering, facet node order (hence normal orientation),
//! the content and slot order of `element_to_subelement` /
//! `subelement_to_element`, and therefore cohesive insertion and DOF
//! numbering downstream.
//!
//! Every public function validates the mesh first
//! ([`Mesh::check_consistency`]): malformed connectivities (wrong component
//! count, node indices out of range) are reported as `Error::Mesh`, never a
//! panic.
//!
//! Mesh data written by the facet builders (names are upstream's):
//!
//! | dataset | stored in | rows | values |
//! |---|---|---|---|
//! | [`ELEMENT_TO_SUBELEMENT`] (`Vec<Element>`, 1 component) | facet mesh | facets | elements of dimension + 1 sharing the facet, in node→element CSR order, plus `Element::NULL` for a boundary facet built by `buildFacets*` |
//! | [`SUBELEMENT_TO_ELEMENT`] (`Element`, `nb_facets_per_element` components) | facet mesh | elements of the parent dimension | their facets in **creation order** (first free slot) |
//! | [`MESH_TO_MESH_FACET`] (`Element`) | parent mesh | parent elements of dimension − 1 | facet with the same barycenter |

use akantu_core::{
    Element, ElementKind, ElementType, ElementTypeMapArray, Error, GhostType, GlobalIdx, Idx, Rank,
    Real, Result, TypeFilter, try_idx, try_to_usize,
};

use crate::events::{MeshEvent, MeshEvents};
use crate::mesh::Mesh;

/// Mesh data `element_to_subelement`.
pub const ELEMENT_TO_SUBELEMENT: &str = "element_to_subelement";
/// Mesh data `subelement_to_element`.
pub const SUBELEMENT_TO_ELEMENT: &str = "subelement_to_element";
/// Mesh data `mesh_to_mesh_facet`.
pub const MESH_TO_MESH_FACET: &str = "mesh_to_mesh_facet";
/// Mesh data `physical_names`.
pub const PHYSICAL_NAMES: &str = "physical_names";

/// Upstream `Math::tolerance` (`1e2 · DBL_EPSILON`, `aka_extern.cc:71`).
pub const MATH_TOLERANCE: Real = 1e2 * Real::EPSILON;

// --------------------------------------------------------------- node → element

/// CSR node → elements map (upstream `CSR<Element>`): row `n` lists the
/// elements containing node `n`, in visiting order (a node repeated in an
/// element's connectivity lists that element once per occurrence, as
/// upstream).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NodeToElements {
    /// Offsets, `nb_nodes + 1` entries.
    pub offsets: Vec<usize>,
    /// Elements, grouped by node.
    pub elements: Vec<Element>,
}

impl NodeToElements {
    /// Number of rows (nodes).
    #[must_use]
    pub fn nb_rows(&self) -> usize {
        self.offsets.len().saturating_sub(1)
    }

    /// Row of node `node`, `None` if out of range.
    #[must_use]
    pub fn get_row(&self, node: usize) -> Option<&[Element]> {
        let (a, b) = (*self.offsets.get(node)?, *self.offsets.get(node + 1)?);
        self.elements.get(a..b)
    }

    /// Row of an already validated node (internal: callers validated the
    /// connectivities against the node count).
    fn row(&self, node: usize) -> &[Element] {
        &self.elements[self.offsets[node]..self.offsets[node + 1]]
    }
}

/// CSR node → element indices of one `(type, ghost)` block (upstream
/// `CSR<Idx>` of `buildNode2ElementsElementTypeMap`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NodeToElementIndices {
    /// Offsets, `nb_nodes + 1` entries.
    pub offsets: Vec<usize>,
    /// Element indices, grouped by node.
    pub elements: Vec<Idx>,
}

impl NodeToElementIndices {
    /// Row of node `node`, `None` if out of range.
    #[must_use]
    pub fn get_row(&self, node: usize) -> Option<&[Idx]> {
        let (a, b) = (*self.offsets.get(node)?, *self.offsets.get(node + 1)?);
        self.elements.get(a..b)
    }
}

fn exclusive_scan(counts: &[usize]) -> Vec<usize> {
    let mut offsets = Vec::with_capacity(counts.len() + 1);
    let mut acc = 0;
    offsets.push(0);
    for &c in counts {
        acc += c;
        offsets.push(acc);
    }
    offsets
}

/// Validated node index (connectivities are checked beforehand).
fn ux(n: Idx) -> usize {
    usize::try_from(n).unwrap_or(usize::MAX)
}

/// Elements of `dimension` of both ghost types (all kinds), in upstream
/// `for_each_element` order, of an already validated mesh.
fn node_to_elements_unchecked(mesh: &Mesh, dimension: usize) -> Result<NodeToElements> {
    let n = mesh.nb_nodes();
    let mut counts = vec![0usize; n];
    for ghost in GhostType::ALL {
        for ty in mesh.element_types(TypeFilter::new().dim(dimension).ghost(ghost)) {
            for &node in mesh.connectivity(ty, ghost)?.as_slice() {
                counts[ux(node)] += 1;
            }
        }
    }
    let offsets = exclusive_scan(&counts);
    let mut cursor = offsets.clone();
    let mut elements = vec![Element::NULL; offsets[n]];
    for ghost in GhostType::ALL {
        for ty in mesh.element_types(TypeFilter::new().dim(dimension).ghost(ghost)) {
            let conn = mesh.connectivity(ty, ghost)?;
            for (e, nodes) in conn.tuples().enumerate() {
                let el = Element::try_new(ty, e, ghost)?;
                for &node in nodes {
                    let c = &mut cursor[ux(node)];
                    elements[*c] = el;
                    *c += 1;
                }
            }
        }
    }
    Ok(NodeToElements { offsets, elements })
}

/// Node → element CSR over the elements of `spatial_dimension` (default:
/// the mesh dimension; upstream maps `_all_dimensions` to it too) of both
/// ghost types and all kinds (upstream `MeshUtils::buildNode2Elements`).
pub fn build_node_to_elements(
    mesh: &Mesh,
    spatial_dimension: Option<usize>,
) -> Result<NodeToElements> {
    mesh.check_consistency()?;
    node_to_elements_unchecked(mesh, spatial_dimension.unwrap_or(mesh.spatial_dimension()))
}

/// Node → element-index CSR of one `(ty, ghost)` block (upstream
/// `MeshUtils::buildNode2ElementsElementTypeMap`; error if the block is
/// absent, as upstream `getConnectivity` throws).
pub fn build_node_to_elements_of_type(
    mesh: &Mesh,
    ty: ElementType,
    ghost: GhostType,
) -> Result<NodeToElementIndices> {
    mesh.check_consistency()?;
    let n = mesh.nb_nodes();
    let conn = mesh.connectivity(ty, ghost)?;
    let mut counts = vec![0usize; n];
    for &node in conn.as_slice() {
        counts[ux(node)] += 1;
    }
    let offsets = exclusive_scan(&counts);
    let mut cursor = offsets.clone();
    let mut elements = vec![0; offsets[n]];
    for (e, nodes) in conn.tuples().enumerate() {
        let e = try_idx(e)?;
        for &node in nodes {
            let c = &mut cursor[ux(node)];
            elements[*c] = e;
            *c += 1;
        }
    }
    Ok(NodeToElementIndices { offsets, elements })
}

/// Sorted set of the elements around every node (upstream
/// `Mesh::fillNodesToElements(dimension)`: `std::set<Element>` per node over
/// both ghost types and all kinds; `None` = all dimensions).
///
/// Deviation (documented): upstream caches these sets in the mesh
/// (empty after `Mesh::read`, refilled with **all** dimensions by every
/// element event); here the map is computed on demand and owned by the
/// caller (contact detection, Python `getAssociatedElements`).
pub fn fill_nodes_to_elements(
    mesh: &Mesh,
    spatial_dimension: Option<usize>,
) -> Result<NodeToElements> {
    mesh.check_consistency()?;
    let n = mesh.nb_nodes();
    let mut sets: Vec<Vec<Element>> = vec![Vec::new(); n];
    for ghost in GhostType::ALL {
        let filter = TypeFilter {
            spatial_dimension,
            ghost_type: ghost,
            kind: None,
        };
        for ty in mesh.element_types(filter) {
            let conn = mesh.connectivity(ty, ghost)?;
            for (e, nodes) in conn.tuples().enumerate() {
                let el = Element::try_new(ty, e, ghost)?;
                for &node in nodes {
                    sets[ux(node)].push(el);
                }
            }
        }
    }
    let mut offsets = Vec::with_capacity(n + 1);
    let mut elements = Vec::new();
    offsets.push(0);
    for mut s in sets {
        s.sort_unstable();
        s.dedup();
        elements.extend(s);
        offsets.push(elements.len());
    }
    Ok(NodeToElements { offsets, elements })
}

/// Elements sharing node `node` (upstream `getAssociatedElements`); error
/// for a node outside the map.
pub fn associated_elements(map: &NodeToElements, node: Idx) -> Result<&[Element]> {
    let u = try_to_usize(node)?;
    map.get_row(u).ok_or(Error::OutOfRange {
        what: "node",
        index: u,
        len: map.nb_rows(),
    })
}

/// Elements around the nodes of `nodes`, concatenated in list order
/// (upstream `getAssociatedElements(node_list, elements)`).
pub fn associated_elements_of_list(map: &NodeToElements, nodes: &[Idx]) -> Result<Vec<Element>> {
    let mut out = Vec::new();
    for &n in nodes {
        out.extend_from_slice(associated_elements(map, n)?);
    }
    Ok(out)
}

// ------------------------------------------------------------- facet building

/// One facet found by the planning pass; its nodes and connected elements
/// live in the plan's flat buffers (no per-facet
/// allocation while planning).
#[derive(Clone, Copy, Debug)]
struct PlannedFacet {
    ty: ElementType,
    ghost: GhostType,
    /// Offset of the facet's nodes in [`FacetPlan::conn`].
    conn: usize,
    /// Range of its connected elements in [`FacetPlan::elements`].
    elements: (usize, usize),
}

/// Everything `buildFacetsDimension` reads from the source mesh, so that the
/// writes can go to the same mesh afterwards (in-place `buildFacets`, facet
/// mesh passes).
#[derive(Clone, Debug)]
struct FacetPlan {
    /// `(ghost, type, nb_elements)` of the source types of `dimension`.
    init: Vec<(GhostType, ElementType, usize)>,
    facets: Vec<PlannedFacet>,
    /// Facet nodes, concatenated.
    conn: Vec<Idx>,
    /// Connected elements of every facet, concatenated.
    elements: Vec<Element>,
}

/// Facet element types of `ty` (structural and IGFEM types have none).
/// Upstream's structural classes list one facet type, `_not_defined`, with
/// no node (`element_class_structural.hh:282-291`), while their facet local
/// connectivity is their geometry's; `buildFacetsDimension` then loops over
/// `arange(1, nb_nodes_per_facet = 0)`, an unbounded range in
/// akantu_iterators (`!=` termination), reads past the facet's nodes and
/// fails on an out-of-range node row (oracle probe `oracle/probes/ud131`:
/// `AssertException` on `rows_offsets` for the three structural types,
/// undefined behaviour without assertions). UD-131 (A): no facets.
fn facet_types_of(ty: ElementType) -> &'static [ElementType] {
    ty.facet_types().unwrap_or(&[])
}

/// Owner ranks of elements (upstream `ElementSynchronizer::getRank` over
/// `element_to_prank`, built on first use): this rank for local elements,
/// the rank of the receive scheme holding a ghost element.
struct ElementRanks<'a> {
    mesh: &'a Mesh,
    ghosts: Option<std::collections::HashMap<Element, Rank>>,
}

impl<'a> ElementRanks<'a> {
    fn new(mesh: &'a Mesh) -> Self {
        Self { mesh, ghosts: None }
    }

    fn rank(&mut self, el: Element) -> Result<Rank> {
        if el.ghost_type() == GhostType::NotGhost {
            return Ok(self.mesh.communicator().rank());
        }
        if self.ghosts.is_none() {
            let sync = self.mesh.synchronizers().element.as_ref().ok_or_else(|| {
                Error::Mesh(format!(
                    "facet between a local and a ghost element ({el}) without an element \
                     synchronizer"
                ))
            })?;
            let mut map = std::collections::HashMap::new();
            for (&r, els) in &sync.communications.recv {
                for &e in els {
                    // upstream assigns in scheme order (the last one wins)
                    map.insert(e, r);
                }
            }
            self.ghosts = Some(map);
        }
        self.ghosts
            .as_ref()
            .and_then(|m| m.get(&el).copied())
            .ok_or_else(|| Error::Mesh(format!("ghost element {el} is in no receive scheme")))
    }
}

/// Planning pass of upstream `MeshUtils::buildFacetsDimension`
/// (`mesh_utils.cc:181-410`) over a validated mesh.
#[allow(clippy::too_many_lines)] // one upstream loop nest, kept together
fn plan_facets(mesh: &Mesh, boundary_only: bool, dimension: usize) -> Result<FacetPlan> {
    let csr = node_to_elements_unchecked(mesh, dimension)?;
    // the rows list their elements in the order the connectivities are
    // walked; when that is the elements' order (`Element::cmp`, the usual
    // case) the occurrences of an element in a row are found by binary
    // search instead of a scan (performance; the same counts)
    let rows_sorted = (0..csr.offsets.len().saturating_sub(1))
        .all(|n| csr.row(n).windows(2).all(|w| w[0] <= w[1]));
    let occurrences = |row: &[Element], e: &Element| -> usize {
        if rows_sorted {
            let lo = row.partition_point(|x| x < e);
            row[lo..].iter().take_while(|x| *x == e).count()
        } else {
            row.iter().filter(|x| *x == e).count()
        }
    };
    let mut ranks = ElementRanks::new(mesh);
    let mut init = Vec::new();
    for ghost in GhostType::ALL {
        for ty in mesh.element_types(TypeFilter::new().dim(dimension).ghost(ghost)) {
            init.push((ghost, ty, mesh.nb_element(ty, ghost)));
        }
    }
    let mut facets = Vec::new();
    let mut facet_conn: Vec<Idx> = Vec::new();
    let mut facet_elements: Vec<Element> = Vec::new();
    let mut facet: Vec<Idx> = Vec::new();
    let mut counter: Vec<usize> = Vec::new();
    let mut connected: Vec<Element> = Vec::new();
    for &(ghost, ty, nb_element) in &init {
        let conn = mesh.connectivity(ty, ghost)?;
        for (ft, &facet_type) in facet_types_of(ty).iter().enumerate() {
            let fc = ty.facet_connectivity(ft).ok_or_else(|| {
                Error::Mesh(format!(
                    "{ty} has no local connectivity for facet type {ft}"
                ))
            })?;
            let nnpf = facet_type.nb_nodes_per_element();
            if fc.nb_nodes_per_facet != nnpf || nnpf == 0 {
                return Err(Error::Mesh(format!(
                    "facet table of {ty} lists {} nodes per {facet_type}",
                    fc.nb_nodes_per_facet
                )));
            }
            for el in 0..nb_element {
                let current = Element::try_new(ty, el, ghost)?;
                let nodes = conn.tuple(el);
                for f in 0..fc.nb_facets {
                    facet.clear();
                    facet.extend((0..nnpf).map(|n| nodes[fc.node(f, n)]));
                    // Elements of the first node's row sharing every other
                    // facet node (counted with multiplicity, as upstream).
                    let row0 = csr.row(ux(facet[0]));
                    counter.clear();
                    counter.resize(row0.len(), 0);
                    for (c, e) in counter.iter_mut().zip(row0) {
                        for &n in &facet[1..] {
                            *c += occurrences(csr.row(ux(n)), e);
                        }
                    }
                    connected.clear();
                    let mut minimum = Element::NULL;
                    for (&c, &e) in counter.iter().zip(row0) {
                        if c != nnpf - 1 {
                            continue;
                        }
                        // std::min(minimum, e): e only if strictly smaller
                        if e < minimum {
                            minimum = e;
                        }
                        connected.push(e);
                    }
                    // the facet is created once, by its smallest element
                    if minimum != current {
                        continue;
                    }
                    if facet.iter().all(|&n| mesh.node_flag(n).is_pure_ghost()) {
                        continue;
                    }
                    if boundary_only && connected.len() != 1 {
                        continue;
                    }
                    let first = facet_elements.len();
                    facet_elements.extend_from_slice(&connected);
                    let mut facet_ghost = ghost;
                    if connected.len() == 1 {
                        facet_elements.push(Element::NULL);
                    } else if connected.len() == 2 {
                        let g = [connected[0].ghost_type(), connected[1].ghost_type()];
                        if (g[0] == GhostType::NotGhost) ^ (g[1] == GhostType::NotGhost) {
                            if !mesh.is_distributed() {
                                return Err(Error::Mesh(format!(
                                    "facet between local {} and ghost {} in a non-distributed mesh",
                                    connected[0], connected[1]
                                )));
                            }
                            let prank = [ranks.rank(connected[0])?, ranks.rank(connected[1])?];
                            // index of the ghost element ("ugly trick")
                            let ghost_one = usize::from(g[0] != GhostType::Ghost);
                            facet_ghost = if prank[ghost_one] > prank[1 - ghost_one] {
                                GhostType::NotGhost
                            } else {
                                GhostType::Ghost
                            };
                        }
                    }
                    let off = facet_conn.len();
                    facet_conn.extend_from_slice(&facet);
                    facets.push(PlannedFacet {
                        ty: facet_type,
                        ghost: facet_ghost,
                        conn: off,
                        elements: (first, facet_elements.len()),
                    });
                }
            }
        }
    }
    Ok(FacetPlan {
        init,
        facets,
        conn: facet_conn,
        elements: facet_elements,
    })
}

/// Fail before any write if a dataset the builders write has another type.
fn check_facet_data_types(target: &Mesh) -> Result<()> {
    use crate::data::MeshDataTypeCode as C;
    for (name, want) in [
        (ELEMENT_TO_SUBELEMENT, C::ElementList),
        (SUBELEMENT_TO_ELEMENT, C::Element),
    ] {
        if let Ok(code) = target.data().type_code(name)
            && code != want
        {
            return Err(Error::Mesh(format!(
                "mesh data '{name}' of '{}' has type {code:?}, the facet builders write {want:?}",
                target.id()
            )));
        }
    }
    Ok(())
}

/// Connectivity block `(ty, ghost)` of `target`, created empty if absent
/// (upstream `MeshAccessor::getConnectivity`).
fn connectivity_block(
    target: &mut Mesh,
    ty: ElementType,
    ghost: GhostType,
) -> Result<akantu_core::array_shared::ArrayMut<'_, Idx>> {
    if !target.connectivities.exists(ty, ghost) {
        target
            .connectivities
            .alloc(0, ty.nb_nodes_per_element(), ty, ghost, 0)?;
    }
    target.connectivities.get_mut(ty, ghost)
}

/// Writing pass of `buildFacetsDimension`: allocations in upstream order,
/// then every planned facet appended with its `element_to_subelement` row
/// and registered in the first free `subelement_to_element` slot of each
/// connected element. Returns the new facets (upstream `NewElementsEvent`
/// list).
fn apply_facet_plan(plan: FacetPlan, target: &mut Mesh) -> Result<Vec<Element>> {
    check_facet_data_types(target)?;
    for &(ghost, ty, nb) in &plan.init {
        target
            .data
            .element_map_mut_or_insert(SUBELEMENT_TO_ELEMENT)?
            .alloc(nb, ty.nb_facets_per_element(), ty, ghost, Element::NULL)?;
        for &ft in facet_types_of(ty) {
            let nf = target.nb_element(ft, ghost);
            target
                .data
                .element_lists_mut_or_insert(ELEMENT_TO_SUBELEMENT)?
                .alloc(nf, 1, ft, ghost, Vec::new())?;
            connectivity_block(target, ft, ghost)?;
        }
    }
    let mut created = Vec::with_capacity(plan.facets.len());
    for pf in plan.facets {
        let nnpf = pf.ty.nb_nodes_per_element();
        let facet_nodes = &plan.conn[pf.conn..pf.conn + nnpf];
        let elements = &plan.elements[pf.elements.0..pf.elements.1];
        let mut conn = connectivity_block(target, pf.ty, pf.ghost)?;
        let index = conn.len();
        conn.try_push_tuple(facet_nodes)?;
        drop(conn); // end the connectivity guard before the other fields
        if pf.ghost == GhostType::Ghost {
            // Ghost facets get a ghost counter of 1 (upstream creates an
            // empty counter array and never fills it, UD-120).
            let counters = &mut target.ghosts_counters;
            if !counters.exists(pf.ty, pf.ghost) {
                counters.alloc(0, 1, pf.ty, pf.ghost, 1)?;
            }
            counters
                .get_mut(pf.ty, pf.ghost)?
                .try_resize(index + 1, 1)?;
        }
        let e2s = target
            .data
            .element_lists_mut_or_insert(ELEMENT_TO_SUBELEMENT)?
            .alloc(index, 1, pf.ty, pf.ghost, Vec::new())?;
        // one `Vec<Element>` per facet is the dataset's representation
        // (upstream `std::vector<Element>` per facet)
        e2s.try_push_tuple(&[elements.to_vec()])?;
        let facet_element = Element::try_new(pf.ty, index, pf.ghost)?;
        created.push(facet_element);
        let s2e = target.data.element_map_mut(SUBELEMENT_TO_ELEMENT)?;
        for &loc in elements {
            if loc.is_null() {
                continue;
            }
            let slots = s2e.element_mut(loc)?;
            if let Some(slot) = slots.iter_mut().find(|e| e.is_null()) {
                *slot = facet_element;
            }
        }
    }
    Ok(created)
}

/// Upstream `MeshUtils::buildFacetsDimension(mesh, mesh_facets,
/// boundary_only, dimension)` with distinct source and target meshes:
/// facets of the elements of `dimension` of `mesh` appended to
/// `mesh_facets` (which must share `mesh`'s node numbering). Returns the
/// new facets (upstream `NewElementsEvent` list, the caller's
/// responsibility); advances the revision of `mesh_facets` once.
pub fn build_facets_dimension(
    mesh: &Mesh,
    mesh_facets: &mut Mesh,
    boundary_only: bool,
    dimension: usize,
) -> Result<Vec<Element>> {
    let created = facets_dimension(mesh, mesh_facets, boundary_only, dimension)?;
    mesh_facets.revision += 1;
    Ok(created)
}

fn facets_dimension(
    mesh: &Mesh,
    mesh_facets: &mut Mesh,
    boundary_only: bool,
    dimension: usize,
) -> Result<Vec<Element>> {
    mesh.check_consistency()?;
    check_shared_nodes(mesh, mesh_facets)?;
    let plan = plan_facets(mesh, boundary_only, dimension)?;
    apply_facet_plan(plan, mesh_facets)
}

/// [`build_facets_dimension`] with `mesh_facets` as its own source (the
/// lower-dimensional passes of `buildAllFacets`); advances the revision
/// once.
pub fn build_facets_dimension_in_place(
    mesh_facets: &mut Mesh,
    boundary_only: bool,
    dimension: usize,
) -> Result<Vec<Element>> {
    let created = facets_dimension_in_place(mesh_facets, boundary_only, dimension)?;
    mesh_facets.revision += 1;
    Ok(created)
}

fn facets_dimension_in_place(
    mesh_facets: &mut Mesh,
    boundary_only: bool,
    dimension: usize,
) -> Result<Vec<Element>> {
    mesh_facets.check_consistency()?;
    let plan = plan_facets(mesh_facets, boundary_only, dimension)?;
    apply_facet_plan(plan, mesh_facets)
}

fn check_shared_nodes(mesh: &Mesh, mesh_facets: &Mesh) -> Result<()> {
    if mesh.nb_nodes() != mesh_facets.nb_nodes()
        || mesh.spatial_dimension() != mesh_facets.spatial_dimension()
    {
        return Err(Error::Mesh(format!(
            "facet mesh '{}' ({} nodes, dim {}) does not share the node numbering of '{}' ({} nodes, dim {})",
            mesh_facets.id(),
            mesh_facets.nb_nodes(),
            mesh_facets.spatial_dimension(),
            mesh.id(),
            mesh.nb_nodes(),
            mesh.spatial_dimension()
        )));
    }
    Ok(())
}

/// Upstream `MeshUtils::buildAllFacets(mesh, mesh_facets, from_dimension,
/// to_dimension)`: facets of the elements of `from_dimension` of `mesh`
/// (facets of dimension `from − 1`), then, on `mesh_facets` itself, the
/// facets of its elements of dimension `from − 1, …, to + 1` (upstream loop
/// `for (i = from − 1; i > to; --i)`): with `to = 0` every level down to
/// points is built. Returns the new facets of every pass, in pass order;
/// advances the revision of `mesh_facets` once.
pub fn build_all_facets(
    mesh: &Mesh,
    mesh_facets: &mut Mesh,
    from_dimension: usize,
    to_dimension: usize,
) -> Result<Vec<Vec<Element>>> {
    let mut passes = vec![facets_dimension(mesh, mesh_facets, false, from_dimension)?];
    let mut i = from_dimension;
    while i > 0 && i - 1 > to_dimension {
        i -= 1;
        passes.push(facets_dimension_in_place(mesh_facets, false, i)?);
    }
    mesh_facets.revision += 1;
    Ok(passes)
}

/// Empty facet mesh sharing `mesh`'s node numbering (upstream
/// `Mesh(spatial_dimension, mesh.nodes, id)` + `defineMeshParent`): copies
/// of the coordinates, flags, global ids and owner ranks (the Rust facet
/// mesh owns copies that mesh transactions keep mirrored), the mesh's
/// communicator and distribution status; bounding boxes computed locally
/// over all nodes (upstream constructor).
pub fn new_facet_mesh(mesh: &Mesh, id: &str) -> Result<Mesh> {
    let mut f = Mesh::with_communicator(
        mesh.spatial_dimension(),
        format!("{}:{id}", mesh.id()),
        mesh.communicator().clone(),
    )?;
    let nodes_id = f.nodes.id().to_owned();
    f.nodes = mesh.nodes.clone();
    f.nodes.set_id(nodes_id);
    let flags_id = f.nodes_flags.id().to_owned();
    f.nodes_flags = mesh.nodes_flags.clone();
    f.nodes_flags.set_id(flags_id);
    f.nodes_global_ids.clone_from(&mesh.nodes_global_ids);
    f.nodes_prank.clone_from(&mesh.nodes_prank);
    f.is_mesh_facets = true;
    f.is_distributed = mesh.is_distributed;
    // upstream constructor: computeBoundingBox() before the facet mesh is
    // marked distributed, i.e. the local box over all nodes for both
    let b = f.local_bounding_box();
    f.bbox_local = b.clone();
    f.bbox = b;
    Ok(f)
}

/// Barycenter, Euclidean norm and distance with upstream's arithmetic
/// (`Math::barycenter`: sequential sum from zero then one division;
/// Eigen `norm`/`distance` of a dynamic vector: sequential sum of squares,
/// one square root).
fn norm(x: &[Real]) -> Real {
    let mut s = 0.0;
    for (i, &v) in x.iter().enumerate() {
        s = if i == 0 { v * v } else { s + v * v };
    }
    s.sqrt()
}

fn distance(a: &[Real], b: &[Real]) -> Real {
    let mut s = 0.0;
    for (i, (&x, &y)) in a.iter().zip(b).enumerate() {
        let d = x - y;
        s = if i == 0 { d * d } else { s + d * d };
    }
    s.sqrt()
}

/// First facet (lowest index) whose barycenter lies at a distance below
/// `tol` of `x` (upstream linear `find_if`, `mesh.cc:310-325`). Candidates
/// are pre-selected on one coordinate through that coordinate's sorted
/// index: a facet at computed distance `< tol` differs by at most
/// `tol · (1 + 4ε)` in every coordinate (the computed sum of squares is at
/// least each computed square), the window below is twice as wide; of the
/// coordinates the one whose window holds the fewest facets is scanned (a
/// window on one coordinate alone holds a whole face of a box-shaped mesh;
/// performance: every window holds every match, so the result is the same).
/// Non-finite queries fall back to the linear scan.
fn first_facet_within(bary: &FacetBarycenters, x: &[Real], tol: Real) -> Option<usize> {
    let w = 2.0 * tol;
    let n = bary.len();
    if n < 32 || !w.is_finite() || x.iter().any(|v| !v.is_finite()) || x.len() != bary.dim {
        return (0..n).find(|&j| distance(x, bary.point(j)) < tol);
    }
    // the narrowest window
    let mut best: Option<(usize, usize, usize)> = None;
    for (c, &xc) in x.iter().enumerate() {
        let (lo, hi) = (xc - w, xc + w);
        let sorted = &bary.sorted[c];
        let start = sorted.partition_point(|&j| bary.coord(j, c) < lo);
        let end = start + sorted[start..].partition_point(|&j| bary.coord(j, c) <= hi);
        if best.is_none_or(|(_, s, e)| end - start < e - s) {
            best = Some((c, start, end));
        }
    }
    let (c, start, end) = best?;
    bary.sorted[c][start..end]
        .iter()
        .filter(|&&j| distance(x, bary.point(j)) < tol)
        .min()
        .copied()
}

/// Barycenters of one facet block (flat, `dim` values each;
/// one buffer per block, not one vector per facet) and, per coordinate,
/// their indices sorted on it.
struct FacetBarycenters {
    dim: usize,
    points: Vec<Real>,
    sorted: Vec<Vec<usize>>,
}

impl FacetBarycenters {
    fn new(dim: usize, points: Vec<Real>) -> Self {
        let n = points.len() / dim.max(1);
        let sorted = (0..dim)
            .map(|c| {
                let mut s: Vec<usize> = (0..n).collect();
                s.sort_by(|&i, &j| {
                    points[i * dim + c]
                        .total_cmp(&points[j * dim + c])
                        .then(i.cmp(&j))
                });
                s
            })
            .collect();
        Self {
            dim,
            points,
            sorted,
        }
    }

    fn len(&self) -> usize {
        self.points.len() / self.dim.max(1)
    }

    fn point(&self, j: usize) -> &[Real] {
        &self.points[j * self.dim..(j + 1) * self.dim]
    }

    fn coord(&self, j: usize, c: usize) -> Real {
        self.points[j * self.dim + c]
    }
}

/// Transfer of the parent's `physical_names` to the facet mesh
/// (`mesh.cc:279-340`): every parent element of dimension − 1 finds the
/// first facet of the same type and ghost type at the same barycenter
/// (tolerance `Math::tolerance · max(1, ‖x‖)`); its name is copied and the
/// parent records `mesh_to_mesh_facet`. Returns the parent's
/// `mesh_to_mesh_facet` map; writes `physical_names` into `facets` and
/// creates the facet groups from it.
pub(crate) fn transfer_physical_names(
    mesh: &Mesh,
    facets: &mut Mesh,
) -> Result<ElementTypeMapArray<Element>> {
    let dim = mesh.spatial_dimension();
    let sub = dim - 1;
    let names = mesh.data().strings(PHYSICAL_NAMES)?;
    let mut m2f = match mesh.data().element_map(MESH_TO_MESH_FACET) {
        Ok(existing) => existing.clone(),
        Err(_) => ElementTypeMapArray::new(MESH_TO_MESH_FACET),
    };
    for ghost in GhostType::ALL {
        for ty in mesh.element_types(TypeFilter::new().dim(sub).ghost(ghost)) {
            // Upstream default `Element()` is a non-canonical null
            // (`_not_defined, 0, _not_ghost`); the canonical NULL here.
            m2f.alloc(mesh.nb_element(ty, ghost), 1, ty, ghost, Element::NULL)?;
        }
    }
    let mut facet_names: ElementTypeMapArray<String> = match facets.data().strings(PHYSICAL_NAMES) {
        Ok(existing) => existing.clone(),
        Err(_) => ElementTypeMapArray::new(PHYSICAL_NAMES),
    };
    // Facet barycenters per (type, ghost), with an index sorted on x.
    let mut bary: std::collections::BTreeMap<(GhostType, ElementType), FacetBarycenters> =
        std::collections::BTreeMap::new();
    for ghost in GhostType::ALL {
        for ty in facets.element_types(TypeFilter::new().dim(sub).ghost(ghost)) {
            let n = facets.nb_element(ty, ghost);
            facet_names.alloc(n, 1, ty, ghost, String::new())?;
            let mut points = vec![0.0; n * dim];
            for (e, p) in points.chunks_exact_mut(dim).enumerate() {
                facets.barycenter_into(Element::try_new(ty, e, ghost)?, p)?;
            }
            bary.insert((ghost, ty), FacetBarycenters::new(dim, points));
        }
    }
    let mut x = vec![0.0; dim];
    for ghost in GhostType::ALL {
        for ty in mesh.element_types(TypeFilter::new().dim(sub).ghost(ghost)) {
            for e in 0..mesh.nb_element(ty, ghost) {
                let element = Element::try_new(ty, e, ghost)?;
                mesh.barycenter_into(element, &mut x)?;
                let n = norm(&x);
                let mut tol = MATH_TOLERANCE;
                if n > tol {
                    tol *= n;
                }
                let b = bary.get(&(ghost, ty)).ok_or_else(|| {
                    Error::Mesh(format!(
                        "no facet of type {ty}:{ghost} in the facet mesh for element {element}"
                    ))
                })?;
                let Some(j) = first_facet_within(b, &x, tol) else {
                    continue; // upstream: debug info, element skipped
                };
                let facet = Element::try_new(ty, j, ghost)?;
                let name = names.element(element)?.first().cloned().unwrap_or_default();
                facet_names.element_mut(facet)?[0] = name;
                m2f.element_mut(element)?[0] = facet;
            }
        }
    }
    facets.data.insert_elemental(
        PHYSICAL_NAMES,
        crate::data::MeshDataValues::String(facet_names),
    );
    facets.create_groups_from_mesh_data(PHYSICAL_NAMES)?;
    Ok(m2f)
}

/// The facet mesh of `mesh` as upstream `Mesh::initMeshFacets(id)` builds
/// it (`mesh.cc:233-345`): a new facet mesh with every facet dimension down
/// to points (`buildAllFacets(mesh, mesh_facets, 0)`), then the transfer
/// of `physical_names`. Returns the facet mesh and, when the parent has
/// `physical_names`, the parent's new `mesh_to_mesh_facet` map. Pure: the
/// parent is not modified (see [`init_mesh_facets`] and
/// `MeshTransaction::init_mesh_facets`).
///
/// A mesh distributed over several ranks also gets the facet synchronizer,
/// the exchange of global facet connectivities and the flip of the ghost
/// facets: **collective** over the mesh's communicator, in its
/// own scope ([`crate::facet_synchronizer::create_distributed_mesh_facets`]
/// with the default options; inside a mesh mutation prepare it before and
/// install it with `MeshTransaction::install_mesh_facets`). On one rank (a
/// mesh "distributed" on a single process, UD-011) the facet mesh is marked
/// distributed with an empty facet synchronizer, as upstream, without
/// communicating.
pub fn create_mesh_facets(
    mesh: &Mesh,
    id: &str,
) -> Result<(Mesh, Option<ElementTypeMapArray<Element>>)> {
    create_mesh_facets_with(
        mesh,
        id,
        crate::facet_synchronizer::DistributedFacetsOptions::default(),
    )
}

/// [`create_mesh_facets`] with explicit options for distributed meshes
/// (facet flip mode, UD-122).
pub fn create_mesh_facets_with(
    mesh: &Mesh,
    id: &str,
    options: crate::facet_synchronizer::DistributedFacetsOptions,
) -> Result<(Mesh, Option<ElementTypeMapArray<Element>>)> {
    if mesh.is_distributed() && mesh.communicator().size() > 1 {
        let p = crate::facet_synchronizer::create_distributed_mesh_facets(mesh, id, options)?;
        return Ok((p.mesh_facets, p.mesh_to_mesh_facet));
    }
    mesh.check_consistency()?;
    let mut facets = new_facet_mesh(mesh, id)?;
    build_all_facets(mesh, &mut facets, mesh.spatial_dimension(), 0)?;
    if mesh.is_distributed() {
        // one rank: the facet synchronizer has no neighbour (upstream builds
        // it from the empty element schemes; the connectivity exchange and
        // the flip change nothing), so no collective is needed
        facets.is_distributed = true;
        let mut sync = akantu_comm::Synchronizer::new(
            format!("{}:facet_synchronizer", facets.id()),
            facets.communicator().clone(),
        );
        if let Some(es) = &mesh.synchronizers().element {
            sync.communications.recv = es
                .communications
                .recv
                .keys()
                .map(|&r| (r, Vec::new()))
                .collect();
        }
        facets.synchronizers.element = Some(sync);
    }
    let m2f = if mesh.data().has_elemental(PHYSICAL_NAMES) {
        Some(transfer_physical_names(mesh, &mut facets)?)
    } else {
        None
    };
    Ok((facets, m2f))
}

/// Upstream `Mesh::initMeshFacets("mesh_facets")` on a mesh being set up
/// (no consumers; use `SharedMesh::setup`, or the transaction method for a
/// shared mesh): builds and installs the facet mesh once (advancing the
/// revision once); a second call is a no-op. No main-mesh event is produced
/// (the parent's topology does not change; the facet events of upstream
/// reach no handler). On a distributed mesh this is a collective operation
/// (every rank calls it; a rank-specific failure is an error on every rank
/// and nothing is installed anywhere).
pub fn init_mesh_facets(mesh: &mut Mesh) -> Result<MeshEvents> {
    init_mesh_facets_with(
        mesh,
        crate::facet_synchronizer::DistributedFacetsOptions::default(),
    )
}

/// [`init_mesh_facets`] with explicit options for distributed meshes.
pub fn init_mesh_facets_with(
    mesh: &mut Mesh,
    options: crate::facet_synchronizer::DistributedFacetsOptions,
) -> Result<MeshEvents> {
    if mesh.mesh_facets.is_some() {
        return Ok(MeshEvents::none());
    }
    let (facets, m2f) = create_mesh_facets_with(mesh, "mesh_facets", options)?;
    if let Some(m2f) = m2f {
        check_mesh_to_mesh_facet_type(mesh)?;
        mesh.data.insert_elemental(
            MESH_TO_MESH_FACET,
            crate::data::MeshDataValues::Element(m2f),
        );
    }
    mesh.mesh_facets = Some(Box::new(facets));
    mesh.revision += 1;
    Ok(MeshEvents::none())
}

/// An existing `mesh_to_mesh_facet` dataset of `mesh` has the type the
/// facet construction writes (`Element`); shared by the set-up and the
/// transaction variants of `initMeshFacets`.
pub(crate) fn check_mesh_to_mesh_facet_type(mesh: &Mesh) -> Result<()> {
    match mesh.data().type_code(MESH_TO_MESH_FACET) {
        Ok(code) if code != crate::data::MeshDataTypeCode::Element => Err(Error::Mesh(format!(
            "mesh data '{MESH_TO_MESH_FACET}' has type {code:?}, initMeshFacets writes Element"
        ))),
        _ => Ok(()),
    }
}

/// Upstream `MeshUtils::buildFacets(mesh)` (`mesh_utils.cc:129-145`): the
/// existing elements of dimension − 1 are cleared and the **boundary**
/// facets of the elements of the mesh dimension are appended to the mesh
/// itself, with `element_to_subelement` / `subelement_to_element` written
/// into the mesh's own data. Set-up operation: the returned event lists the
/// new facets; as upstream, the cleared elements produce no event and their
/// mesh data / groups are left as they were.
///
/// Deviation (UD-118, category B): upstream marks the mesh itself
/// as a facet mesh of itself (`defineMeshParent(mesh)` is never undone), so
/// a later node removal skips its flags and global ids; not reproduced.
pub fn build_facets(mesh: &mut Mesh) -> Result<MeshEvents> {
    mesh.check_consistency()?;
    let dim = mesh.spatial_dimension();
    let plan = plan_facets(mesh, true, dim)?;
    check_facet_data_types(mesh)?;
    for ghost in GhostType::ALL {
        let types: Vec<ElementType> = mesh
            .element_types(TypeFilter::new().dim(dim - 1).ghost(ghost))
            .collect();
        for ty in types {
            mesh.connectivities.get_mut(ty, ghost)?.clear();
        }
    }
    let created = apply_facet_plan(plan, mesh)?;
    mesh.revision += 1;
    let mut ev = MeshEvents::none();
    ev.push(MeshEvent::ElementsAdded {
        elements: created,
        constitutive_law: None,
        origin: "MeshUtils::buildFacets".into(),
        extension: None,
    });
    Ok(ev)
}

/// Upstream `MeshUtils::fillElementToSubElementsData(mesh)`
/// (`mesh_utils.cc:654-785`), run by the mesh readers: for a mesh that
/// already contains elements of dimension − 1 (read from file), computes
/// `element_to_subelement` (no `Element::NULL` for boundary facets, unlike
/// the facet builders) and `subelement_to_element` for every dimension,
/// from node sharing on the P1 nodes of each facet; connected elements are
/// listed in `Element` order. Does nothing when the mesh has no
/// (not-ghost) element of dimension − 1.
///
/// An element with more facets than slots is an error (upstream: debug
/// assertion, out-of-bounds write in release; UD-119, category A).
pub fn fill_element_to_subelements_data(mesh: &mut Mesh) -> Result<()> {
    if let Some((e2s, s2e)) = compute_element_to_subelements_data(mesh)? {
        mesh.data.insert_elemental(
            ELEMENT_TO_SUBELEMENT,
            crate::data::MeshDataValues::ElementList(e2s),
        );
        mesh.data.insert_elemental(
            SUBELEMENT_TO_ELEMENT,
            crate::data::MeshDataValues::Element(s2e),
        );
    }
    Ok(())
}

/// `(element_to_subelement, subelement_to_element)` maps (see
/// [`compute_element_to_subelements_data`]).
pub type ElementAdjacency = (
    ElementTypeMapArray<Vec<Element>>,
    ElementTypeMapArray<Element>,
);

/// `element_to_subelement` and `subelement_to_element` as
/// [`fill_element_to_subelements_data`] computes them (starting from the
/// mesh's existing datasets), without modifying the mesh: `None` when the
/// mesh has no (not-ghost) element of dimension − 1. Used by the mesh
/// readers, which stage the result inside their read transaction (upstream
/// readers end with `fillElementToSubElementsData`, `mesh_io_msh.cc:919`,
/// `mesh_io_diana.cc:174`).
pub fn compute_element_to_subelements_data(mesh: &Mesh) -> Result<Option<ElementAdjacency>> {
    mesh.check_consistency()?;
    let dim = mesh.spatial_dimension();
    let nb_not_ghost = |m: &Mesh, d: usize| m.nb_element_filtered(TypeFilter::new().dim(d));
    if nb_not_ghost(mesh, dim - 1) == 0 {
        return Ok(None);
    }
    if let Ok(code) = mesh.data().type_code(ELEMENT_TO_SUBELEMENT)
        && code != crate::data::MeshDataTypeCode::ElementList
    {
        return Err(Error::Mesh(format!(
            "mesh data '{ELEMENT_TO_SUBELEMENT}' has type {code:?}"
        )));
    }
    check_facet_data_types(mesh)?;
    let mut e2s = mesh
        .data()
        .element_lists(ELEMENT_TO_SUBELEMENT)
        .cloned()
        .unwrap_or_else(|_| ElementTypeMapArray::new(ELEMENT_TO_SUBELEMENT));
    let mut s2e = mesh
        .data()
        .element_map(SUBELEMENT_TO_ELEMENT)
        .cloned()
        .unwrap_or_else(|_| ElementTypeMapArray::new(SUBELEMENT_TO_ELEMENT));
    // reusable scratch: candidate elements of a facet
    // with their multiplicity, sorted by `Element` afterwards
    let mut candidates: Vec<Element> = Vec::new();
    let mut connected: Vec<Element> = Vec::new();
    for sp in (1..=dim).rev() {
        if nb_not_ghost(mesh, sp) == 0 {
            continue;
        }
        for ghost in GhostType::ALL {
            for ty in mesh.element_types(TypeFilter::new().dim(sp).ghost(ghost)) {
                let a = s2e.alloc(
                    mesh.nb_element(ty, ghost),
                    ty.nb_facets_per_element(),
                    ty,
                    ghost,
                    Element::NULL,
                )?;
                a.fill(Element::NULL);
            }
            for ty in mesh.element_types(TypeFilter::new().dim(sp - 1).ghost(ghost)) {
                let a = e2s.alloc(mesh.nb_element(ty, ghost), 1, ty, ghost, Vec::new())?;
                a.fill(Vec::new());
            }
        }
        let csr = node_to_elements_unchecked(mesh, sp)?;
        for ghost in GhostType::ALL {
            for ty in mesh.element_types(TypeFilter::new().dim(sp - 1).ghost(ghost)) {
                let p1 = ty.p1_type().ok_or(Error::UnsupportedElementType {
                    ty,
                    context: "fillElementToSubElementsData (no P1 type)",
                })?;
                let nnpf = p1.nb_nodes_per_element();
                let conn = mesh.connectivity(ty, ghost)?;
                for (e, nodes) in conn.tuples().enumerate() {
                    let facet_element = Element::try_new(ty, e, ghost)?;
                    // upstream counts the elements of every P1 node in a
                    // std::map<Element, count>: same set, Element order
                    candidates.clear();
                    for &node in &nodes[..nnpf] {
                        candidates.extend_from_slice(csr.row(ux(node)));
                    }
                    candidates.sort_unstable();
                    connected.clear();
                    for run in candidates.chunk_by(|a, b| a == b) {
                        if run.len() == nnpf {
                            connected.push(run[0]);
                        }
                    }
                    e2s.element_mut(facet_element)?
                        .first_mut()
                        .ok_or_else(|| Error::Mesh("element_to_subelement without rows".into()))?
                        .extend_from_slice(&connected);
                    for &c in &connected {
                        let slots = s2e.element_mut(c)?;
                        let slot = slots.iter_mut().find(|x| x.is_null()).ok_or_else(|| {
                            Error::Mesh(format!(
                                "element {c} seems to have too many facets (more than {})",
                                c.ty().nb_facets_per_element()
                            ))
                        })?;
                        *slot = facet_element;
                    }
                }
            }
        }
    }
    Ok(Some((e2s, s2e)))
}

// ------------------------------------------------------- cross-part checks

/// Cross-part invariants of a mesh and its facet mesh (ARCHITECTURE),
/// checked by the mesh transactions before every
/// commit; `Ok` without a facet mesh:
///
/// * **shared node numbering**: same node count, bitwise identical
///   coordinates, identical flags, global ids and owner ranks (upstream
///   shares the node array);
/// * **unambiguous index domains**: the facet mesh holds no element of the
///   parent's dimension (see `crate::removal`);
/// * **adjacency data**: every block of the facet mesh's
///   [`ELEMENT_TO_SUBELEMENT`] (`Vec<Element>`, one component) /
///   [`SUBELEMENT_TO_ELEMENT`] (`Element`) and of the parent's
///   [`MESH_TO_MESH_FACET`] (`Element`, one component) has one row per
///   element of its domain, and every non-null reference designates an
///   existing element of its domain. Components that add elements to a
///   block carrying such data (cohesive insertion) must add the rows too.
pub fn check_facet_mesh_consistency(mesh: &Mesh) -> Result<()> {
    check_facet_mesh_consistency_rows(mesh, None)
}

/// The adjacency datasets checked by [`check_facet_mesh_consistency`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AdjacencySet {
    /// The facet mesh's [`ELEMENT_TO_SUBELEMENT`].
    ElementToSubelement,
    /// The facet mesh's [`SUBELEMENT_TO_ELEMENT`].
    SubelementToElement,
    /// The parent's [`MESH_TO_MESH_FACET`].
    MeshToMeshFacet,
}

/// Rows of the adjacency datasets a mesh transaction replaced or appended
/// (the restricted form of [`check_facet_mesh_consistency`]).
#[derive(Debug, Default)]
pub(crate) struct TouchedRows {
    /// Replaced rows: dataset and the row's element.
    pub(crate) rows: Vec<(AdjacencySet, Element)>,
    /// Appended rows: dataset, block and the first appended row.
    pub(crate) tails: Vec<(AdjacencySet, ElementType, GhostType, usize)>,
}

/// [`check_facet_mesh_consistency`] with the references of the adjacency
/// datasets read from every row (`touched = None`) or only from the rows
/// in `touched`. Everything else (node arrays, index domains, types, row
/// counts and components of every block) is checked in full either way.
///
/// What the restricted form proves (used by the mesh transactions when
/// the state they started from passed the check): if every reference was
/// in range before, no block shrank and the datasets changed only in the
/// rows of `touched`, then every reference is in range now — an untouched
/// row holds the references it held, and their blocks only grew.
pub(crate) fn check_facet_mesh_consistency_rows(
    mesh: &Mesh,
    touched: Option<&TouchedRows>,
) -> Result<()> {
    use crate::data::MeshDataTypeCode as C;
    let Some(f) = mesh.mesh_facets() else {
        return Ok(());
    };
    // rows of `set` to read: every row, or only the touched ones
    let touched_rows = |set: AdjacencySet| {
        touched.map(|t| {
            (
                t.rows
                    .iter()
                    .filter(move |(s, _)| *s == set)
                    .map(|&(_, e)| e),
                t.tails
                    .iter()
                    .filter(move |(s, ..)| *s == set)
                    .map(|&(_, ty, g, start)| (ty, g, start)),
            )
        })
    };
    let missing = |name: &str, e: Element| {
        Error::Mesh(format!(
            "{name}: the transaction edited the row of {e}, which does not exist"
        ))
    };
    let broken = |what: &str| {
        Err(Error::Mesh(format!(
            "facet mesh '{}' does not share the node numbering of '{}' ({what} differ)",
            f.id(),
            mesh.id()
        )))
    };
    if f.nb_nodes() != mesh.nb_nodes() {
        return broken("node counts");
    }
    let same_bits = f
        .nodes
        .borrow()
        .as_slice()
        .iter()
        .zip(mesh.nodes.borrow().as_slice())
        .all(|(a, b)| a.to_bits() == b.to_bits());
    if !same_bits || f.nodes.nb_component() != mesh.nodes.nb_component() {
        return broken("coordinates");
    }
    if f.nodes_flags.as_slice() != mesh.nodes_flags.as_slice() {
        return broken("node flags");
    }
    if f.nodes_global_ids
        .as_ref()
        .map(akantu_core::Array::as_slice)
        != mesh
            .nodes_global_ids
            .as_ref()
            .map(akantu_core::Array::as_slice)
    {
        return broken("global ids");
    }
    if f.nodes_prank != mesh.nodes_prank {
        return broken("owner ranks");
    }
    crate::removal::check_facet_domains(mesh)?;
    let dim = mesh.spatial_dimension();
    let count = |ty: ElementType, g: GhostType| {
        if crate::removal::facet_mesh_type_is_parent(dim, ty) {
            mesh.nb_element(ty, g)
        } else {
            f.nb_element(ty, g)
        }
    };
    // block size of the last block looked up (the references of a block
    // mostly designate one block; the mesh does not change during the check)
    let last = std::cell::Cell::new(None::<(ElementType, GhostType, usize)>);
    let check_ref = |owner: &str, e: Element| -> Result<()> {
        if e.is_null() {
            return Ok(());
        }
        let (ty, g) = (e.ty(), e.ghost_type());
        let n = match last.get() {
            Some((t, gg, n)) if t == ty && gg == g => n,
            _ => {
                let n = count(ty, g);
                last.set(Some((ty, g, n)));
                n
            }
        };
        if e.idx() < n {
            Ok(())
        } else {
            Err(Error::Mesh(format!(
                "{owner} references {e}, outside its block of {n} elements"
            )))
        }
    };
    let check_rows = |name: &str, ty: ElementType, g: GhostType, rows: usize, expected: usize| {
        if rows == expected {
            Ok(())
        } else {
            Err(Error::Mesh(format!(
                "{name} of {ty}:{g} has {rows} rows for {expected} elements"
            )))
        }
    };
    let typed = |m: &Mesh, name: &str, want: C| -> Result<bool> {
        match m.data().type_code(name) {
            Ok(code) if code == want => Ok(true),
            Ok(code) => Err(Error::Mesh(format!(
                "mesh data '{name}' of '{}' has type {code:?}, expected {want:?}",
                m.id()
            ))),
            Err(_) => Ok(false),
        }
    };
    if typed(f, ELEMENT_TO_SUBELEMENT, C::ElementList)? {
        for (ty, g, rows) in f
            .data()
            .element_lists(ELEMENT_TO_SUBELEMENT)?
            .as_map()
            .iter_all()
        {
            check_rows(ELEMENT_TO_SUBELEMENT, ty, g, rows.len(), count(ty, g))?;
            if rows.nb_component() != 1 && !rows.is_empty() {
                return Err(Error::Mesh(format!(
                    "{ELEMENT_TO_SUBELEMENT} of {ty}:{g} has {} components",
                    rows.nb_component()
                )));
            }
            if touched.is_none() {
                for list in rows.as_slice() {
                    for &e in list {
                        check_ref(ELEMENT_TO_SUBELEMENT, e)?;
                    }
                }
            }
        }
        if let Some((one, tails)) = touched_rows(AdjacencySet::ElementToSubelement) {
            let lists = f.data().element_lists(ELEMENT_TO_SUBELEMENT)?;
            for el in one {
                let list = lists
                    .get(el.ty(), el.ghost_type())
                    .ok()
                    .and_then(|a| a.as_slice().get(el.idx()))
                    .ok_or_else(|| missing(ELEMENT_TO_SUBELEMENT, el))?;
                for &e in list {
                    check_ref(ELEMENT_TO_SUBELEMENT, e)?;
                }
            }
            for (ty, g, start) in tails {
                if let Ok(a) = lists.get(ty, g) {
                    for list in a.as_slice().get(start..).unwrap_or(&[]) {
                        for &e in list {
                            check_ref(ELEMENT_TO_SUBELEMENT, e)?;
                        }
                    }
                }
            }
        }
    }
    if typed(f, SUBELEMENT_TO_ELEMENT, C::Element)? {
        for (ty, g, rows) in f
            .data()
            .element_map(SUBELEMENT_TO_ELEMENT)?
            .as_map()
            .iter_all()
        {
            check_rows(SUBELEMENT_TO_ELEMENT, ty, g, rows.len(), count(ty, g))?;
            // one slot per facet of the element
            if !rows.is_empty()
                && matches!(ty.kind(), ElementKind::Regular | ElementKind::Cohesive)
                && rows.nb_component() != ty.nb_facets_per_element()
            {
                return Err(Error::Mesh(format!(
                    "{SUBELEMENT_TO_ELEMENT} of {ty}:{g} has {} components, expected {} \
                     (facets per element)",
                    rows.nb_component(),
                    ty.nb_facets_per_element()
                )));
            }
            if touched.is_none() {
                for &e in rows.as_slice() {
                    check_ref(SUBELEMENT_TO_ELEMENT, e)?;
                }
            }
        }
        if let Some((one, tails)) = touched_rows(AdjacencySet::SubelementToElement) {
            let map = f.data().element_map(SUBELEMENT_TO_ELEMENT)?;
            for el in one {
                let row = map
                    .element(el)
                    .map_err(|_| missing(SUBELEMENT_TO_ELEMENT, el))?;
                for &e in row {
                    check_ref(SUBELEMENT_TO_ELEMENT, e)?;
                }
            }
            for (ty, g, start) in tails {
                if let Ok(a) = map.get(ty, g) {
                    let nc = a.nb_component();
                    let from = start.saturating_mul(nc);
                    for &e in a.as_slice().get(from..).unwrap_or(&[]) {
                        check_ref(SUBELEMENT_TO_ELEMENT, e)?;
                    }
                }
            }
        }
        // Debug builds: reciprocity of the two maps for regular elements
        // (every facet listed in an element's `subelement_to_element` row
        // lists the element in its `element_to_subelement` row).
        #[cfg(debug_assertions)]
        if typed(f, ELEMENT_TO_SUBELEMENT, C::ElementList)? {
            check_adjacency_reciprocity(f)?;
        }
    }
    let check_m2f = |e: Element| -> Result<()> {
        if !e.is_null() && e.idx() >= f.nb_element(e.ty(), e.ghost_type()) {
            return Err(Error::Mesh(format!(
                "{MESH_TO_MESH_FACET} references facet {e} outside the facet mesh"
            )));
        }
        Ok(())
    };
    if typed(mesh, MESH_TO_MESH_FACET, C::Element)? {
        for (ty, g, rows) in mesh
            .data()
            .element_map(MESH_TO_MESH_FACET)?
            .as_map()
            .iter_all()
        {
            check_rows(
                MESH_TO_MESH_FACET,
                ty,
                g,
                rows.len(),
                mesh.nb_element(ty, g),
            )?;
            if touched.is_none() {
                for &e in rows.as_slice() {
                    check_m2f(e)?;
                }
            }
        }
        if let Some((one, tails)) = touched_rows(AdjacencySet::MeshToMeshFacet) {
            let map = mesh.data().element_map(MESH_TO_MESH_FACET)?;
            for el in one {
                let row = map
                    .element(el)
                    .map_err(|_| missing(MESH_TO_MESH_FACET, el))?;
                for &e in row {
                    check_m2f(e)?;
                }
            }
            for (ty, g, start) in tails {
                if let Ok(a) = map.get(ty, g) {
                    let from = start.saturating_mul(a.nb_component());
                    for &e in a.as_slice().get(from..).unwrap_or(&[]) {
                        check_m2f(e)?;
                    }
                }
            }
        }
    }
    Ok(())
}

/// Debug check of [`check_facet_mesh_consistency`]: for every regular
/// element `e` of a `subelement_to_element` block of the facet mesh `f` and
/// every non-null facet `s` in its row, `element_to_subelement(s)` contains
/// `e`. Cohesive rows are not checked.
#[cfg(debug_assertions)]
fn check_adjacency_reciprocity(f: &Mesh) -> Result<()> {
    let s2e = f.data().element_map(SUBELEMENT_TO_ELEMENT)?;
    let e2s = f.data().element_lists(ELEMENT_TO_SUBELEMENT)?;
    for (ty, g, rows) in s2e.as_map().iter_all() {
        if ty.kind() != ElementKind::Regular {
            continue;
        }
        let nc = rows.nb_component().max(1);
        for (i, row) in rows.as_slice().chunks(nc).enumerate() {
            let el = Element::try_new(ty, i, g)?;
            for &s in row {
                if s.is_null() {
                    continue;
                }
                let Ok(list) = e2s.get(s.ty(), s.ghost_type()) else {
                    continue;
                };
                let Some(l) = list.as_slice().get(s.idx()) else {
                    continue;
                };
                if !l.contains(&el) {
                    return Err(Error::Mesh(format!(
                        "{SUBELEMENT_TO_ELEMENT}({el}) lists {s}, whose \
                         {ELEMENT_TO_SUBELEMENT} row does not list {el}"
                    )));
                }
            }
        }
    }
    Ok(())
}

// ------------------------------------------------------------------ purify

/// New node numbering of upstream `MeshUtils::purifyMesh`
/// (`mesh_utils.cc:497-534`): nodes numbered by first appearance over every
/// connectivity (ghost types, element types of all dimensions and kinds,
/// connectivity order); unused nodes get `-1`. Note that the nodes are
/// renumbered even when none is unused.
///
/// Deviation (UD-126, category B): when the mesh has a facet mesh sharing its
/// node numbering, nodes used only by facets (possible after parent
/// elements were erased) are kept, numbered after the parent's nodes in
/// first-appearance order over the facet connectivities; upstream removes
/// them and leaves the facet mesh pointing at wrong nodes. Without such
/// nodes the numbering is upstream's.
pub fn purify_numbering(mesh: &Mesh) -> Result<Vec<Idx>> {
    mesh.check_consistency()?;
    let mut numbering = vec![-1; mesh.nb_nodes()];
    let mut next: Idx = 0;
    let mut visit = |m: &Mesh| -> Result<()> {
        for ghost in GhostType::ALL {
            for ty in m.element_types(TypeFilter::new().ghost(ghost)) {
                for &n in m.connectivity(ty, ghost)?.as_slice() {
                    let slot = &mut numbering[ux(n)];
                    if *slot < 0 {
                        *slot = next;
                        next += 1;
                    }
                }
            }
        }
        Ok(())
    };
    visit(mesh)?;
    if let Some(f) = mesh.mesh_facets()
        && f.nb_nodes() == mesh.nb_nodes()
    {
        f.check_consistency()?;
        visit(f)?;
    }
    Ok(numbering)
}

/// Upstream `MeshUtils::purifyMesh(mesh)` on a mesh being set up: unused
/// nodes removed, every node renumbered by first appearance
/// ([`purify_numbering`]); connectivities, coordinates, flags, global ids,
/// owner ranks, periodic pairs, groups and nodal data follow — of the facet
/// mesh too (upstream updates coordinates/flags/global ids only, see
/// `crate::removal`). Advances the revision once. Returns the
/// `NodesRemoved` events (parent, then facet mesh), also for a pure
/// permutation (upstream sends the event even when no node is removed,
/// `mesh_utils.cc:505-531`).
pub fn purify_mesh(mesh: &mut Mesh) -> Result<MeshEvents> {
    crate::removal::reject_facet_part(mesh, "purify_mesh")?;
    let numbering = purify_numbering(mesh)?;
    let events = crate::removal::apply_node_removal(mesh, &numbering, "MeshUtils::purifyMesh")?;
    mesh.revision += 1;
    Ok(MeshEvents(events.into_iter().map(|(_, e)| e).collect()))
}

// ------------------------------------------------------ distribution helpers

/// Upstream `MeshUtils::renumberNodesInConnectivity`
/// (`mesh_utils.cc:473-494`): every node of `list` not yet in
/// `renumbering` gets the next number (`renumbering.len()`), and `list` is
/// rewritten with the new numbers.
pub fn renumber_nodes_in_connectivity(
    list: &mut [Idx],
    renumbering: &mut std::collections::BTreeMap<Idx, Idx>,
) -> Result<()> {
    for n in list {
        let next = try_idx(renumbering.len())?;
        *n = *renumbering.entry(*n).or_insert(next);
    }
    Ok(())
}

/// Local part of upstream `MeshUtils::renumberMeshNodes`
/// (`mesh_utils.cc:423-470`, used by distribution): `local_connectivities`
/// holds `nb_local + nb_ghost` elements of `ty`; the renumbering starts
/// from `old_nodes` (`old_nodes[i] → i`), new nodes are numbered by first
/// appearance, the connectivity is rewritten, `old_nodes` becomes the
/// new → old map (sorted by new number), and the local / ghost blocks are
/// split. Returns `(local, ghost)` connectivities.
pub fn renumber_mesh_nodes(
    local_connectivities: &mut [Idx],
    nb_local: usize,
    nb_ghost: usize,
    ty: ElementType,
    old_nodes: &mut Vec<Idx>,
) -> Result<(Vec<Idx>, Vec<Idx>)> {
    let nnpe = ty.nb_nodes_per_element();
    let total = (nb_local + nb_ghost)
        .checked_mul(nnpe)
        .ok_or_else(|| Error::invalid("renumber_mesh_nodes: size overflow"))?;
    if local_connectivities.len() < total {
        return Err(Error::DimensionMismatch {
            context: "renumber_mesh_nodes (connectivity length)",
            expected: total,
            got: local_connectivities.len(),
        });
    }
    let mut map = std::collections::BTreeMap::new();
    for (i, &n) in old_nodes.iter().enumerate() {
        map.insert(n, try_idx(i)?);
    }
    renumber_nodes_in_connectivity(&mut local_connectivities[..total], &mut map)?;
    let mut new_to_old = vec![0; map.len()];
    for (&old, &new) in &map {
        new_to_old[try_to_usize(new)?] = old;
    }
    *old_nodes = new_to_old;
    let split = nb_local * nnpe;
    Ok((
        local_connectivities[..split].to_vec(),
        local_connectivities[split..total].to_vec(),
    ))
}

/// Global connectivity of every block of `mesh` (upstream
/// `Mesh::getGlobalConnectivity`): local node ids mapped through the
/// global ids (identity without them).
pub fn global_connectivity(mesh: &Mesh) -> Result<ElementTypeMapArray<GlobalIdx>> {
    mesh.check_consistency()?;
    let mut out = ElementTypeMapArray::new(format!("{}:global_connectivity", mesh.id()));
    for (ty, ghost, conn) in mesh.connectivities().as_map().iter_all() {
        let a = out.alloc(conn.len(), conn.nb_component(), ty, ghost, 0)?;
        for (g, &l) in a.as_mut_slice().iter_mut().zip(conn.borrow().as_slice()) {
            *g = mesh.node_global_id(l);
        }
    }
    Ok(out)
}

/// How [`flip_facets_local_with`] reorders a facet whose remote global
/// connectivity differs from the local one (UD-122).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FacetFlipMode {
    /// Corrected default (D-003 category B): `conn[i]` becomes the local
    /// node whose global id is `remote[i]`, so the facet's global
    /// connectivity equals the remote one (the order upstream's commented
    /// alternative at `mesh_utils.cc:608-613` computes).
    #[default]
    RemoteOrder,
    /// Upstream compatibility: `conn[pos(remote[i])] = conn_old[i]` with
    /// `pos` the position in the local global connectivity
    /// (`mesh_utils.cc:601-606`), i.e. the **inverse** of the remote
    /// permutation — equal to the remote order for transpositions, not for
    /// 3-/4-cycles or higher-order permutations.
    UpstreamInverse,
}

/// One planned facet flip (no mesh change yet).
struct PlannedFlip {
    ty: ElementType,
    facet: usize,
    /// Offset of the new connectivity in the plan's flat buffer.
    conn: usize,
    /// Swap the two neighbours / the first two sub-facets.
    invert: bool,
}

/// Local part of upstream `MeshUtils::flipFacets(mesh_facets,
/// remote_global_connectivities, gt_facet)` (`mesh_utils.cc:537-650`) with
/// the corrected default ([`FacetFlipMode::RemoteOrder`]); see
/// [`flip_facets_local_with`].
pub fn flip_facets_local(
    mesh_facets: &mut Mesh,
    remote_global_connectivities: &ElementTypeMapArray<GlobalIdx>,
    gt_facet: GhostType,
) -> Result<()> {
    flip_facets_local_with(
        mesh_facets,
        remote_global_connectivities,
        gt_facet,
        FacetFlipMode::default(),
    )
}

/// Local part of upstream `MeshUtils::flipFacets`: every facet of
/// dimension − 1 and ghost type `gt_facet` whose global connectivity
/// differs from the remote one is reordered according to `mode`; in 3-D a
/// facet whose remote P1 order is only a rotation of the local one keeps
/// its adjacency, otherwise (and always in 1-D/2-D) the two neighbours in
/// `element_to_subelement` are swapped (unless the second is `NULL`) and
/// the first two `subelement_to_element` entries of the facet are swapped.
/// The exchange of the remote connectivities belongs to the facet
/// synchronizer.
///
/// Atomic: every facet is planned and validated first —
/// remote block shapes, permutations (upstream: debug assertion), the
/// presence, type, row count and shape of the adjacency data of every
/// facet to invert (upstream reads `element_per_facet[1]` and swaps
/// `subfacets(0)`/`(1)` unchecked: a short row is undefined behaviour,
/// UD-123) — and only then applied; the application is journaled, so an
/// error leaves the mesh unchanged. Advances the revision once on success;
/// no event (upstream sends none).
#[allow(clippy::too_many_lines)] // planning checklist + journaled application
pub fn flip_facets_local_with(
    mesh_facets: &mut Mesh,
    remote_global_connectivities: &ElementTypeMapArray<GlobalIdx>,
    gt_facet: GhostType,
    mode: FacetFlipMode,
) -> Result<()> {
    use crate::data::MeshDataTypeCode as C;
    mesh_facets.check_consistency()?;
    let dim = mesh_facets.spatial_dimension();
    let types: Vec<ElementType> = mesh_facets
        .element_types(TypeFilter::new().dim(dim - 1).ghost(gt_facet))
        .collect();
    // An adjacency dataset of another type is an error, never skipped.
    for (name, want) in [
        (ELEMENT_TO_SUBELEMENT, C::ElementList),
        (SUBELEMENT_TO_ELEMENT, C::Element),
    ] {
        if let Ok(code) = mesh_facets.data().type_code(name)
            && code != want
            && !types.is_empty()
        {
            return Err(Error::Mesh(format!(
                "flip_facets_local: mesh data '{name}' has type {code:?}, expected {want:?}"
            )));
        }
    }
    // ---- planning: no change --------------------------------------------
    let mut plans: Vec<PlannedFlip> = Vec::new();
    let mut new_conns: Vec<Idx> = Vec::new();
    let (mut local_g, mut sorted_l, mut sorted_r) = (Vec::new(), Vec::new(), Vec::new());
    for &ty in &types {
        let conn = mesh_facets.connectivity(ty, gt_facet)?;
        let remote = remote_global_connectivities.get(ty, gt_facet)?;
        let nnpf = conn.nb_component();
        if remote.nb_component() != nnpf || remote.len() != conn.len() {
            return Err(Error::DimensionMismatch {
                context: "flip_facets_local (remote global connectivity)",
                expected: conn.len() * nnpf,
                got: remote.len() * remote.nb_component(),
            });
        }
        let p1 = ty.p1_type().map_or(nnpf, ElementType::nb_nodes_per_element);
        let first_plan = plans.len();
        for (f, (local, remote)) in conn.tuples().zip(remote.tuples()).enumerate() {
            local_g.clear();
            local_g.extend(local.iter().map(|&n| mesh_facets.node_global_id(n)));
            if local_g.as_slice() == remote {
                continue;
            }
            sorted_l.clone_from(&local_g);
            sorted_r.clear();
            sorted_r.extend_from_slice(remote);
            sorted_l.sort_unstable();
            sorted_r.sort_unstable();
            if sorted_l != sorted_r {
                return Err(Error::Mesh(format!(
                    "facets are not permutations of each other: {local_g:?} and {remote:?}"
                )));
            }
            let off = new_conns.len();
            new_conns.extend_from_slice(local);
            let out = &mut new_conns[off..];
            for (i, g) in remote.iter().enumerate() {
                // a permutation: every remote node is found
                let pos = local_g
                    .iter()
                    .position(|x| x == g)
                    .ok_or_else(|| Error::Mesh("flip_facets_local: node not found".into()))?;
                match mode {
                    FacetFlipMode::RemoteOrder => out[i] = local[pos],
                    FacetFlipMode::UpstreamInverse => out[pos] = local[i],
                }
            }
            let mut invert = true;
            if dim == 3 {
                let start = remote.iter().position(|&g| g == local_g[0]).unwrap_or(0);
                let mut n = 1;
                while n < p1 && local_g[n] == remote[(start + n) % p1] {
                    n += 1;
                }
                if n == p1 {
                    invert = false;
                }
            }
            plans.push(PlannedFlip {
                ty,
                facet: f,
                conn: off,
                invert,
            });
        }
        // adjacency data of the facets to invert (upstream requires both
        // blocks for every facet type it touches)
        if plans[first_plan..].iter().any(|p| p.invert) {
            let e2s = mesh_facets
                .data()
                .element_lists(ELEMENT_TO_SUBELEMENT)?
                .get(ty, gt_facet)?;
            let s2e = mesh_facets
                .data()
                .element_map(SUBELEMENT_TO_ELEMENT)?
                .get(ty, gt_facet)?;
            for (name, rows, comps, min_comps) in [
                (ELEMENT_TO_SUBELEMENT, e2s.len(), e2s.nb_component(), 1),
                (SUBELEMENT_TO_ELEMENT, s2e.len(), s2e.nb_component(), 2),
            ] {
                if rows != conn.len() || comps < min_comps {
                    return Err(Error::Mesh(format!(
                        "flip_facets_local: {name} of {ty}:{gt_facet} has {rows} rows of {comps} \
                         components for {} facets (at least {min_comps} components needed)",
                        conn.len()
                    )));
                }
            }
            for p in plans[first_plan..].iter().filter(|p| p.invert) {
                let n = e2s.as_slice()[p.facet].len();
                if n < 2 {
                    return Err(Error::Mesh(format!(
                        "flip_facets_local: {ELEMENT_TO_SUBELEMENT} row of facet {ty}:{} has {n} \
                         entries (two are needed to invert it)",
                        p.facet
                    )));
                }
            }
        }
    }
    if plans.is_empty() {
        mesh_facets.revision += 1;
        return Ok(());
    }
    // ---- journaled application ------------------------------------------
    let backup_conns: Vec<(ElementType, akantu_core::Array<Idx>)> = types
        .iter()
        .map(|&ty| Ok((ty, mesh_facets.connectivity(ty, gt_facet)?.clone())))
        .collect::<Result<_>>()?;
    let backup_e2s = mesh_facets
        .data()
        .elemental(ELEMENT_TO_SUBELEMENT)
        .ok()
        .cloned();
    let backup_s2e = mesh_facets
        .data()
        .elemental(SUBELEMENT_TO_ELEMENT)
        .ok()
        .cloned();
    let applied = (|| -> Result<()> {
        for p in &plans {
            let nnpf = p.ty.nb_nodes_per_element();
            mesh_facets
                .connectivities
                .get_mut(p.ty, gt_facet)?
                .get_tuple_mut(p.facet)?
                .copy_from_slice(&new_conns[p.conn..p.conn + nnpf]);
        }
        for p in plans.iter().filter(|p| p.invert) {
            let el = Element::try_new(p.ty, p.facet, gt_facet)?;
            let e2s = mesh_facets.data.element_lists_mut(ELEMENT_TO_SUBELEMENT)?;
            let list = e2s
                .element_mut(el)?
                .first_mut()
                .ok_or_else(|| Error::Mesh("flip_facets_local: empty e2s row".into()))?;
            if list.len() >= 2 && !list[1].is_null() {
                list.swap(0, 1);
            }
            let row = mesh_facets
                .data
                .element_map_mut(SUBELEMENT_TO_ELEMENT)?
                .element_mut(el)?;
            if row.len() >= 2 {
                row.swap(0, 1);
            }
        }
        Ok(())
    })();
    if let Err(e) = applied {
        for (ty, a) in backup_conns {
            mesh_facets
                .connectivities
                .as_map_mut()
                .replace(ty, gt_facet, a.into());
        }
        if let Some(d) = backup_e2s {
            mesh_facets.data.insert_elemental(ELEMENT_TO_SUBELEMENT, d);
        }
        if let Some(d) = backup_s2e {
            mesh_facets.data.insert_elemental(SUBELEMENT_TO_ELEMENT, d);
        }
        return Err(e);
    }
    mesh_facets.revision += 1;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::accessor::MeshAccessor;
    use akantu_core::ElementType as Et;

    fn square_of_two_triangles() -> Mesh {
        let mut m = Mesh::new(2, "m").unwrap();
        let mut a = MeshAccessor::new(&mut m);
        for x in [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]] {
            a.push_node(&x).unwrap();
        }
        let mut c = a.connectivity_mut(Et::Triangle3, GhostType::NotGhost);
        c.push_tuple(&[0, 1, 2]);
        c.push_tuple(&[0, 2, 3]);
        drop(c);
        m
    }

    #[test]
    fn csr_rows_follow_visiting_order() {
        let m = square_of_two_triangles();
        let csr = build_node_to_elements(&m, None).unwrap();
        let t = |i| Element::new(Et::Triangle3, i, GhostType::NotGhost);
        assert_eq!(csr.get_row(0).unwrap(), &[t(0), t(1)]);
        assert_eq!(csr.get_row(1).unwrap(), &[t(0)]);
        assert_eq!(csr.get_row(3).unwrap(), &[t(1)]);
        assert!(csr.get_row(4).is_none());
        assert!(associated_elements(&csr, 9).is_err());
        assert!(associated_elements(&csr, -1).is_err());
        let typed = build_node_to_elements_of_type(&m, Et::Triangle3, GhostType::NotGhost).unwrap();
        assert_eq!(typed.get_row(2).unwrap(), &[0, 1]);
        assert!(build_node_to_elements_of_type(&m, Et::Quadrangle4, GhostType::NotGhost).is_err());
        // none in dimension 1
        let csr1 = build_node_to_elements(&m, Some(1)).unwrap();
        assert!(csr1.elements.is_empty() && csr1.nb_rows() == 4);
    }

    #[test]
    fn facets_of_two_triangles() {
        let mut m = square_of_two_triangles();
        let _ = init_mesh_facets(&mut m).unwrap();
        let f = m.mesh_facets().unwrap();
        let seg = f.connectivity(Et::Segment2, GhostType::NotGhost).unwrap();
        assert_eq!(seg.as_slice(), &[0, 1, 1, 2, 2, 0, 2, 3, 3, 0]);
        let e2s = f.data().element_lists(ELEMENT_TO_SUBELEMENT).unwrap();
        let rows = e2s.get(Et::Segment2, GhostType::NotGhost).unwrap();
        let t = |i| Element::new(Et::Triangle3, i, GhostType::NotGhost);
        assert_eq!(rows.as_slice()[2], vec![t(0), t(1)]);
        assert_eq!(rows.as_slice()[0], vec![t(0), Element::NULL]);
        let pts = f.connectivity(Et::Point1, GhostType::NotGhost).unwrap();
        assert_eq!(pts.len(), 4);
        drop((seg, pts));
        // second call is a no-op
        assert!(init_mesh_facets(&mut m).unwrap().is_empty());
    }

    #[test]
    fn malformed_meshes_are_errors_not_panics() {
        let mut m = square_of_two_triangles();
        MeshAccessor::new(&mut m)
            .connectivity_mut(Et::Triangle3, GhostType::NotGhost)
            .push_tuple(&[0, 1, 7]);
        assert!(build_node_to_elements(&m, None).is_err());
        assert!(init_mesh_facets(&mut m).is_err());
        assert!(m.mesh_facets().is_none());
        assert!(build_facets(&mut m).is_err());
        assert!(purify_numbering(&m).is_err());
        assert!(fill_element_to_subelements_data(&mut m).is_err());
        let mut e = Mesh::new(3, "empty").unwrap();
        let _ = init_mesh_facets(&mut e).unwrap();
        assert_eq!(
            e.mesh_facets()
                .unwrap()
                .nb_element_filtered(TypeFilter::new()),
            0
        );
        let _ = purify_mesh(&mut Mesh::new(1, "e").unwrap()).unwrap();
    }

    #[test]
    fn windowed_search_equals_linear_search() {
        // deterministic pseudo-random barycenters with clusters of
        // near-duplicates around the tolerance
        let mut s: u64 = 0x9e37_79b9_7f4a_7c15;
        let mut next = || {
            s ^= s << 13;
            s ^= s >> 7;
            s ^= s << 17;
            #[allow(clippy::cast_precision_loss)]
            let v = (s % 1000) as Real / 997.0;
            v
        };
        let mut bary = Vec::new();
        for _ in 0..400 {
            let x = vec![next(), next(), next()];
            bary.push(x.clone());
            let mut y = x;
            y[0] += MATH_TOLERANCE * 0.5;
            bary.push(y);
        }
        let flat = FacetBarycenters::new(3, bary.concat());
        for q in bary.iter().step_by(3) {
            let tol = MATH_TOLERANCE * norm(q).max(1.0);
            let linear = bary.iter().position(|b| distance(q, b) < tol);
            assert_eq!(first_facet_within(&flat, q, tol), linear);
            let mut off = q.clone();
            off[1] += 1e-3;
            let linear = bary.iter().position(|b| distance(&off, b) < tol);
            assert_eq!(first_facet_within(&flat, &off, tol), linear);
        }
    }

    /// Local facet flip: a remote order that only rotates a 3-D facet keeps
    /// the adjacency; a reversed one swaps the neighbours and the first two
    /// sub-facets; 2-D facets always swap; non-permutations are errors.
    #[test]
    fn flip_facets_local_follows_upstream() {
        let mut m = Mesh::new(3, "t").unwrap();
        {
            let mut a = MeshAccessor::new(&mut m);
            for x in [
                [0.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [0.0, 1.0, 0.0],
                [0.0, 0.0, 1.0],
                [1.0, 1.0, 1.0],
            ] {
                a.push_node(&x).unwrap();
            }
            let mut c = a.connectivity_mut(Et::Tetrahedron4, GhostType::Ghost);
            c.push_tuple(&[0, 1, 2, 3]);
            c.push_tuple(&[1, 2, 3, 4]);
            drop(c);
            a.set_distributed();
        }
        let (mut f, _) = create_mesh_facets_unchecked(&m);
        let ghost_tri = f
            .connectivity(Et::Triangle3, GhostType::Ghost)
            .unwrap()
            .clone();
        let n = ghost_tri.len();
        // remote = local (no change) except facet 0 rotated, facet 1 reversed
        let mut remote = ElementTypeMapArray::new("r");
        let r = remote
            .alloc(n, 3, Et::Triangle3, GhostType::Ghost, 0)
            .unwrap();
        for i in 0..n {
            for (k, &v) in ghost_tri.tuple(i).iter().enumerate() {
                r.tuple_mut(i)[k] = i64::from(v);
            }
        }
        let t0 = ghost_tri.tuple(0).to_vec();
        r.tuple_mut(0)
            .copy_from_slice(&[i64::from(t0[1]), i64::from(t0[2]), i64::from(t0[0])]);
        let t1 = ghost_tri.tuple(1).to_vec();
        r.tuple_mut(1)
            .copy_from_slice(&[i64::from(t1[0]), i64::from(t1[2]), i64::from(t1[1])]);
        let e2s_before = f
            .data()
            .element_lists(ELEMENT_TO_SUBELEMENT)
            .unwrap()
            .clone();
        // corrected default: the global order is the remote one
        let mut corrected = Mesh::new(3, "c").unwrap();
        std::mem::swap(&mut corrected, &mut f);
        let (mut f, _) = create_mesh_facets_unchecked(&m);
        flip_facets_local(&mut corrected, &remote, GhostType::Ghost).unwrap();
        let conn = corrected
            .connectivity(Et::Triangle3, GhostType::Ghost)
            .unwrap();
        assert_eq!(conn.tuple(0), &[t0[1], t0[2], t0[0]]);
        assert_eq!(conn.tuple(1), &[t1[0], t1[2], t1[1]]);
        flip_facets_local_with(
            &mut f,
            &remote,
            GhostType::Ghost,
            FacetFlipMode::UpstreamInverse,
        )
        .unwrap();
        let conn = f.connectivity(Et::Triangle3, GhostType::Ghost).unwrap();
        // upstream writes conn[position of remote[i] in local] = local[i]:
        // the inverse permutation (UD-122: a rotated remote order is
        // rotated the other way); a transposition is its own inverse
        assert_eq!(conn.tuple(0), &[t0[2], t0[0], t0[1]]);
        assert_eq!(conn.tuple(1), &[t1[0], t1[2], t1[1]]);
        let e2s = f.data().element_lists(ELEMENT_TO_SUBELEMENT).unwrap();
        let g = |m: &ElementTypeMapArray<Vec<Element>>, i: usize| -> Vec<Element> {
            m.get(Et::Triangle3, GhostType::Ghost).unwrap().as_slice()[i].clone()
        };
        assert_eq!(g(e2s, 0), g(&e2s_before, 0)); // rotation: unchanged
        let (b, a) = (g(&e2s_before, 1), g(e2s, 1));
        if !b[1].is_null() {
            assert_eq!((a[0], a[1]), (b[1], b[0]));
        } else {
            assert_eq!(a, b);
        }
        remote
            .get_mut(Et::Triangle3, GhostType::Ghost)
            .unwrap()
            .tuple_mut(2)
            .copy_from_slice(&[99, 98, 97]);
        drop(conn);
        assert!(flip_facets_local(&mut f, &remote, GhostType::Ghost).is_err());
    }

    /// Facet mesh of a (distributed, single-rank) mesh with ghost elements
    /// only: ghost facets without interface (both neighbours ghost).
    fn create_mesh_facets_unchecked(m: &Mesh) -> (Mesh, Option<ElementTypeMapArray<Element>>) {
        create_mesh_facets(m, "mesh_facets").unwrap()
    }

    #[test]
    fn renumbering_helpers() {
        let mut conn = vec![5, 7, 9, 7, 9, 11];
        let mut old = vec![9];
        let (l, g) = renumber_mesh_nodes(&mut conn, 1, 1, Et::Triangle3, &mut old).unwrap();
        assert_eq!(l, vec![1, 2, 0]);
        assert_eq!(g, vec![2, 0, 3]);
        assert_eq!(old, vec![9, 5, 7, 11]);
        assert!(renumber_mesh_nodes(&mut conn, 3, 0, Et::Triangle3, &mut old).is_err());
    }
}
