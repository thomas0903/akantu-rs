// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/mesh_utils/mesh_partition.{hh,cc}
// (MeshPartition: linearisation, buildDualGraph, fillPartitionInformation,
// tweak/restoreConnectivity), src/mesh_utils/mesh_partition/
// mesh_partition_mesh_data.{hh,cc} (MeshPartitionMeshData),
// mesh_partition_scotch.{hh,cc} (MeshPartitionScotch::partitionate, behind
// the `scotch` feature, see `crate::scotch`).

//! Element partitions of a mesh (MUTIL-012…014, PART-002/003/005; decision
//! record D-020, `docs/ARCHITECTURE.md`).
//!
//! A [`MeshPartition`] assigns every not-ghost element of the partitioned
//! dimension a part (rank) and lists, per element, the **ghost parts**: the
//! other parts owning an element that shares a node with it (upstream
//! `ghost_partitions_csr`). Lower-dimensional elements (boundary facets read
//! from the file) take the minimum part of their adjacent elements
//! (`element_to_subelement`) and inherit that element's ghost parts
//! (upstream `fillPartitionInformation`, `mesh_partition.cc:212-322`).
//! A lower-dimensional element adjacent to no element (e.g. a physical
//! point on no segment) follows the parts that hold its nodes (**UD-231**,
//! upstream puts it in part 0, giving its nodes two
//! owners); [`PartitionCompat`] reproduces upstream.
//!
//! Partitioners ([`MeshPartitioner`]):
//! * [`PrescribedPartition`] — a linearised part array (user input or a
//!   golden fixture recorded from the oracle; used by differential tests so
//!   that Rust and C++ distribute identically, D-009);
//! * [`MeshDataPartition`] — upstream `MeshPartitionMeshData`: parts read
//!   from an `ElementTypeMapArray<Idx>` per element of the mesh dimension;
//! * [`RcbPartitioner`] — pure-Rust recursive coordinate bisection of the
//!   element barycentres, a **different** partitioner for builds without
//!   Scotch (never claimed equal to Scotch);
//! * `ScotchPartitioner` (feature `scotch`) — upstream
//!   `MeshPartitionScotch`: dual graph (a port of METIS `MeshToDual`) given
//!   to `SCOTCH_graphPart` with the default strategy, seeded with the
//!   global random seed (`SCOTCH_randomSeed`).
//!
//! The partition is computed on the root rank only (centralised
//! distribution, the documented capacity limit of this path).

use akantu_core::{
    Element, ElementKind, ElementType, ElementTypeMap, ElementTypeMapArray, Error, GhostType, Idx,
    Int, Real, Result, TypeFilter, try_idx, try_to_usize,
};

use crate::mesh::Mesh;
use crate::utils::ELEMENT_TO_SUBELEMENT;

const NG: GhostType = GhostType::NotGhost;

/// Upstream-compatibility switches of the partition information (D-003
/// category B defects). The default (`false` everywhere) is the corrected
/// behaviour; differential replays against the unpatched oracle set the
/// switch of the defect they reproduce.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PartitionCompat {
    /// **UD-231**: a lower-dimensional element whose
    /// `element_to_subelement` row is empty (adjacent to no element of the
    /// next dimension, e.g. a physical point on no segment) goes to part 0
    /// without ghost parts (upstream `mesh_partition.cc:284-287`), so its
    /// nodes are `_normal` on rank 0 **and** on the rank whose elements
    /// hold them (two owners: wrong global DOF count). Default: see
    /// [`fill_partition_information_with`].
    pub orphan_lower_dimensional_in_part_zero: bool,
}

/// Compressed rows of parts (upstream `CSR<Idx>`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PartCsr {
    /// Row offsets (`len = rows + 1`).
    pub offsets: Vec<usize>,
    /// Parts of all rows.
    pub values: Vec<Idx>,
}

impl PartCsr {
    /// Parts of row `i` (empty for a row outside the table).
    #[must_use]
    pub fn row(&self, i: usize) -> &[Idx] {
        match (self.offsets.get(i), self.offsets.get(i + 1)) {
            (Some(&a), Some(&b)) => &self.values[a..b],
            _ => &[],
        }
    }

    /// Number of rows.
    #[must_use]
    pub fn nb_rows(&self) -> usize {
        self.offsets.len().saturating_sub(1)
    }

    fn push_row(&mut self, parts: impl IntoIterator<Item = Idx>) {
        if self.offsets.is_empty() {
            self.offsets.push(0);
        }
        self.values.extend(parts);
        self.offsets.push(self.values.len());
    }
}

/// Which partitioner produced a partition.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PartitionerKind {
    /// Scotch with its version string (`SCOTCH_VERSION.RELEASE.PATCHLEVEL`).
    Scotch {
        /// Version of the linked library.
        version: String,
    },
    /// Prescribed (user array / oracle fixture).
    Prescribed,
    /// From element data (upstream `MeshPartitionMeshData`).
    MeshData,
    /// Pure-Rust recursive coordinate bisection.
    RecursiveCoordinateBisection,
}

/// Everything that determines a partition (stored with fixtures and in
/// parity tests).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PartitionMetadata {
    /// Partitioner.
    pub kind: PartitionerKind,
    /// Strategy string (`""` = the library default, as upstream:
    /// `SCOTCH_stratInit` without a strategy).
    pub strategy: String,
    /// Seed given to `SCOTCH_randomSeed` (upstream
    /// `RandomGenerator<Int>::seed()`); `None` when no randomness is
    /// involved.
    pub seed: Option<i64>,
    /// `SCOTCH_randomReset` called before partitioning (upstream: no; the
    /// shim reseeds before every partition so that one process can compute
    /// several partitions, see `crate::scotch`).
    pub random_reset: bool,
    /// Width of `SCOTCH_Num` in bits (0 without Scotch).
    pub index_width: u32,
    /// FNV-1a hash of the ordered dual graph (vertex order = linearised
    /// element order, edge order, vertex and edge weights); 0 when no graph
    /// was built.
    pub graph_hash: u64,
    /// Number of parts.
    pub nb_parts: usize,
    /// Scotch threads: the value of `SCOTCH_PTHREAD_NUMBER` when the
    /// partition was computed (`None` when unset or without Scotch; a
    /// parity condition, ARCHITECTURE, the oracle
    /// fixtures record `"1"`).
    pub pthread_number: Option<String>,
}

impl PartitionMetadata {
    fn simple(kind: PartitionerKind, nb_parts: usize) -> Self {
        Self {
            pthread_number: None,
            kind,
            strategy: String::new(),
            seed: None,
            random_reset: false,
            index_width: 0,
            graph_hash: 0,
            nb_parts,
        }
    }
}

/// An element partition (upstream `MeshPartition` after `partitionate`).
#[derive(Clone, Debug)]
pub struct MeshPartition {
    /// Number of parts (upstream `nb_partitions`).
    pub nb_parts: usize,
    /// Dimension of the partitioned elements (upstream `spatial_dimension`).
    pub spatial_dimension: usize,
    /// Part of every not-ghost element of the partitioned types (upstream
    /// `partitions`): the types of `spatial_dimension` and of every lower
    /// dimension present in the mesh.
    pub partitions: ElementTypeMapArray<Idx>,
    /// Ghost parts per element (upstream `ghost_partitions_csr`, not-ghost
    /// slot).
    pub ghost_partitions: ElementTypeMap<PartCsr>,
    /// Provenance.
    pub metadata: PartitionMetadata,
}

impl MeshPartition {
    /// Whether `ty` was partitioned (upstream `hasPartitions`).
    #[must_use]
    pub fn has_partitions(&self, ty: ElementType) -> bool {
        self.partitions.exists(ty, NG)
    }

    /// Parts of the elements of `ty` (upstream `getPartition(type)`).
    pub fn partition(&self, ty: ElementType) -> Result<&[Idx]> {
        self.partitions.get(ty, NG).map(|a| a.as_slice())
    }

    /// Ghost parts of the elements of `ty`.
    pub fn ghost_partition(&self, ty: ElementType) -> Result<&PartCsr> {
        self.ghost_partitions
            .get(ty, NG)
            .ok_or_else(|| Error::not_found("ghost partition", ty.name()))
    }

    /// Parts of the elements of the partitioned dimension in linearised
    /// order (types in enum order, then element index): the array a
    /// [`PrescribedPartition`] reproduces this partition from.
    #[must_use]
    pub fn linearized(&self) -> Vec<Idx> {
        let mut out = Vec::new();
        for ty in self
            .partitions
            .element_types(TypeFilter::new().dim(self.spatial_dimension))
        {
            if let Ok(a) = self.partitions.get(ty, NG) {
                out.extend_from_slice(a.as_slice());
            }
        }
        out
    }
}

/// Computes partitions (upstream `MeshPartition::partitionate`). Called on
/// the root rank only.
pub trait MeshPartitioner {
    /// Partition `mesh` into `nb_parts` parts.
    fn partition(&mut self, mesh: &Mesh, nb_parts: usize) -> Result<MeshPartition>;
}

// ------------------------------------------------------------ linearisation

/// `(type, offset)` of every not-ghost type of dimension `dim` (all kinds),
/// in `elementTypes` order (upstream `linearized_offsets`).
#[must_use]
pub fn linearized_offsets(mesh: &Mesh, dim: usize) -> Vec<(ElementType, usize)> {
    let mut total = 0;
    mesh.element_types(TypeFilter::new().dim(dim))
        .map(|ty| {
            let o = total;
            total += mesh.nb_element(ty, NG);
            (ty, o)
        })
        .collect()
}

fn unlinearized(offsets: &[(ElementType, usize)], lin: usize) -> Result<Element> {
    let mut found = None;
    for &(t, o) in offsets {
        if lin >= o {
            found = Some((t, o));
        }
    }
    let (t, o) = found.ok_or_else(|| Error::Mesh(format!("linear element {lin} out of range")))?;
    Element::try_new(t, lin - o, NG)
}

/// Connectivity of element `e` of `(ty, not ghost)` with periodic slaves
/// replaced by their masters when `periodic` (upstream
/// `tweakConnectivity`, applied to the types of the partitioned dimension
/// only).
fn effective_nodes<'m>(
    mesh: &'m Mesh,
    ty: ElementType,
    e: usize,
    periodic: bool,
    scratch: &'m mut Vec<Idx>,
) -> Result<&'m [Idx]> {
    let block = mesh.connectivity(ty, NG)?;
    let conn = block.tuple(e);
    scratch.clear();
    if !periodic {
        scratch.extend_from_slice(conn);
        return Ok(scratch.as_slice());
    }
    scratch.extend(conn.iter().map(|&n| mesh.periodic_master(n).unwrap_or(n)));
    Ok(scratch)
}

/// Node → element rows over the not-ghost elements of dimension `dim` (all
/// kinds, `elementTypes` order), on the (periodically tweaked)
/// connectivity: upstream `MeshUtils::buildNode2Elements(mesh, csr, dim)`
/// as called by the partitioners. One CSR of **linearised** element
/// indices ([`linearized_offsets`]); a row lists its elements in element
/// order (types in enum order, then index: the order of `Element` for
/// not-ghost elements).
struct LinearNodeToElements {
    offsets: Vec<usize>,
    elements: Vec<usize>,
}

impl LinearNodeToElements {
    fn row(&self, node: Idx) -> Result<&[usize]> {
        let n = try_to_usize(node)?;
        match (self.offsets.get(n), self.offsets.get(n + 1)) {
            (Some(&a), Some(&b)) => Ok(&self.elements[a..b]),
            _ => Err(Error::OutOfRange {
                what: "node",
                index: n,
                len: self.offsets.len().saturating_sub(1),
            }),
        }
    }
}

fn node_to_elements(mesh: &Mesh, dim: usize, periodic: bool) -> Result<LinearNodeToElements> {
    let nb_nodes = mesh.nb_nodes();
    let offsets_t = linearized_offsets(mesh, dim);
    let mut scratch = Vec::new();
    let check = |n: Idx| -> Result<usize> {
        let i = try_to_usize(n)?;
        if i < nb_nodes {
            Ok(i)
        } else {
            Err(Error::OutOfRange {
                what: "node",
                index: i,
                len: nb_nodes,
            })
        }
    };
    // counts, then the rows in element order
    let mut offsets = vec![0usize; nb_nodes + 1];
    for &(ty, _) in &offsets_t {
        for e in 0..mesh.nb_element(ty, NG) {
            for &n in effective_nodes(mesh, ty, e, periodic, &mut scratch)? {
                offsets[check(n)? + 1] += 1;
            }
        }
    }
    for i in 0..nb_nodes {
        offsets[i + 1] += offsets[i];
    }
    let mut fill = offsets.clone();
    let mut elements = vec![0usize; offsets[nb_nodes]];
    for &(ty, off) in &offsets_t {
        for e in 0..mesh.nb_element(ty, NG) {
            for &n in effective_nodes(mesh, ty, e, periodic, &mut scratch)? {
                let i = check(n)?;
                elements[fill[i]] = off + e;
                fill[i] += 1;
            }
        }
    }
    Ok(LinearNodeToElements { offsets, elements })
}

// --------------------------------------------------------------- dual graph

/// Dual graph of the elements of the mesh dimension (upstream
/// `MeshPartition::buildDualGraph`, a C++ port of METIS `MeshToDual`), in
/// the CSR layout `SCOTCH_graphBuild` takes.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DualGraph {
    /// Row offsets (`verttab`, `len = nb_vertices + 1`).
    pub xadj: Vec<Int>,
    /// Neighbours (`edgetab`).
    pub adjncy: Vec<Int>,
    /// Edge loads (`edlotab`).
    pub edge_loads: Vec<Int>,
    /// Vertex loads (`velotab`).
    pub vertex_loads: Vec<Int>,
}

impl DualGraph {
    /// FNV-1a hash of the ordered graph (recorded in [`PartitionMetadata`]).
    #[must_use]
    pub fn hash(&self) -> u64 {
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        let mut eat = |v: &[Int]| {
            h ^= v.len() as u64;
            h = h.wrapping_mul(0x0100_0000_01b3);
            for x in v {
                for b in x.to_le_bytes() {
                    h ^= u64::from(b);
                    h = h.wrapping_mul(0x0100_0000_01b3);
                }
            }
        };
        eat(&self.xadj);
        eat(&self.adjncy);
        eat(&self.vertex_loads);
        eat(&self.edge_loads);
        h
    }
}

/// Upstream `buildDualGraph` (`mesh_partition.cc:83-184`): two elements of
/// the mesh dimension are adjacent when they share at least as many nodes
/// as the smallest facet of the element (and its own node count); two
/// cohesive elements are never adjacent. Neighbours are listed in
/// `Element` order (upstream `std::map<Element, Int>`), vertices in
/// linearised order. Connectivities are periodically tweaked when the mesh
/// is periodic (as upstream's partitioners do before calling it).
///
/// Deviation (**UD-234**, category A): upstream sizes the graph by the
/// number of elements **with** a neighbour and indexes it by linear element
/// number, so a mesh with an isolated element writes out of bounds; the
/// port lists every element (an isolated one with no edge). Identical for
/// meshes without isolated elements.
pub fn build_dual_graph(
    mesh: &Mesh,
    edge_load: &dyn Fn(Element, Element) -> Int,
    vertex_load: &dyn Fn(Element) -> Int,
) -> Result<DualGraph> {
    mesh.check_consistency()?;
    let dim = mesh.spatial_dimension();
    let periodic = mesh.is_periodic();
    let offsets = linearized_offsets(mesh, dim);
    let n: usize = offsets
        .last()
        .map_or(0, |&(t, o)| o + mesh.nb_element(t, NG));
    let n2e = node_to_elements(mesh, dim, periodic)?;
    let cohesive: Vec<bool> = offsets
        .iter()
        .map(|&(t, _)| t.kind() == ElementKind::Cohesive)
        .collect();
    let type_of = |lin: usize| offsets.partition_point(|&(_, o)| o <= lin) - 1;
    let mut neighbours: Vec<Vec<usize>> = vec![Vec::new(); n];
    let mut scratch = Vec::new();
    // shared-node counts per adjacent element: `stamp[c] == v + 1` marks
    // `count[c]` as counted for vertex v; the touched elements, sorted,
    // are the adjacent elements in `Element` order (upstream iterates a
    // `std::map<Element, Int>`)
    let mut stamp = vec![0usize; n];
    let mut count = vec![0usize; n];
    let mut touched: Vec<usize> = Vec::new();
    for (ti, &(ty, off)) in offsets.iter().enumerate() {
        let magic = (0..ty.nb_facet_types())
            .filter_map(|t| ty.facet_type(t))
            .map(ElementType::nb_nodes_per_element)
            .fold(ty.nb_nodes_per_element(), usize::min);
        for e in 0..mesh.nb_element(ty, NG) {
            let v = off + e;
            touched.clear();
            for &node in effective_nodes(mesh, ty, e, periodic, &mut scratch)? {
                for &c in n2e.row(node)? {
                    if stamp[c] == v + 1 {
                        count[c] += 1;
                    } else {
                        stamp[c] = v + 1;
                        count[c] = 1;
                        touched.push(c);
                    }
                }
            }
            touched.sort_unstable();
            for &adj in &touched {
                if adj == v || count[adj] < magic {
                    continue;
                }
                if cohesive[ti] && cohesive[type_of(adj)] {
                    continue;
                }
                neighbours[v].push(adj);
            }
        }
    }
    let to_int = |v: usize| -> Result<Int> {
        Int::try_from(v).map_err(|_| Error::CapacityExceeded {
            what: "dual graph index (Int)",
            requested: v as u128,
            limit: Int::MAX as u128,
        })
    };
    let mut g = DualGraph {
        xadj: Vec::with_capacity(n + 1),
        ..DualGraph::default()
    };
    g.xadj.push(0);
    for (v, nb) in neighbours.iter().enumerate() {
        let elv = unlinearized(&offsets, v)?;
        g.vertex_loads.push(vertex_load(elv));
        for &w in nb {
            g.adjncy.push(to_int(w)?);
            g.edge_loads
                .push(edge_load(elv, unlinearized(&offsets, w)?));
        }
        g.xadj.push(to_int(g.adjncy.len())?);
    }
    Ok(g)
}

// ---------------------------------------------------- partition information

/// Upstream `MeshPartition::fillPartitionInformation`
/// (`mesh_partition.cc:212-322`) from the parts of the elements of
/// dimension `dim` in linearised order.
///
/// * Elements of dimension `dim`: ghost parts = sorted distinct parts,
///   other than its own, of the elements of dimension `dim` sharing one of
///   its (periodically tweaked) nodes.
/// * Lower dimensions, from `dim − 1` down to 0 (types in enum order):
///   part = minimum part of the adjacent elements listed by
///   `element_to_subelement` (0 when the row is empty); ghost parts = the
///   adjacent parts and the ghost parts of the first adjacent element with
///   that minimum, minus the part itself, sorted.
///
/// * Orphan lower-dimensional elements (empty `element_to_subelement`
///   row): see [`fill_partition_information_with`] (UD-231).
///
/// Errors: a part outside `0..nb_parts`, a wrong array length, a lower-
/// dimensional type without `element_to_subelement` data (upstream throws),
/// a row of null elements only (upstream reads a sentinel element:
/// undefined behaviour), or an orphan element no part can hold (UD-231).
pub fn fill_partition_information(
    mesh: &Mesh,
    dim: usize,
    nb_parts: usize,
    linearized_parts: &[Idx],
    metadata: PartitionMetadata,
) -> Result<MeshPartition> {
    fill_partition_information_with(
        mesh,
        dim,
        nb_parts,
        linearized_parts,
        metadata,
        PartitionCompat::default(),
    )
}

/// [`fill_partition_information`] with upstream-compatibility switches.
///
/// **Orphan lower-dimensional elements (UD-231).** A node
/// is owned by exactly one rank only if every part holding it in a local
/// element also holds it in a ghost element whenever two or more parts
/// hold it locally (the flag rules of the distribution: normal = local
/// only, master = local and ghost, lowest master wins). Elements of the
/// partitioned dimension and adjacent facets satisfy this by construction.
/// By default an element with an empty `element_to_subelement` row takes
/// the minimum part among the parts that already hold **every** one of its
/// claimed nodes locally (elements of dimension `dim` and the
/// lower-dimensional elements assigned before it, in processing order;
/// nodes held by no part do not constrain), and as ghost parts the other
/// parts holding any of its nodes. It goes to part 0 when none of its nodes
/// is held yet. If no part holds all its claimed nodes (e.g. a segment
/// between two parts that share no node there), no part can hold it
/// without giving a node two owners: the partition is refused, naming the
/// element. The resulting ownership is checked for the nodes of orphan
/// elements; every other node satisfies it by construction (it is held
/// locally only by elements of dimension `dim`, whose node-based ghost
/// layer covers it, and by adjacent facets placed in a part of one of
/// their elements).
///
/// That construction argument needs **conforming adjacency**: the element
/// whose part a non-orphan lower-dimensional element takes (the first
/// adjacent element of minimum part) must contain all its nodes. This is
/// checked for every such element (cost ∝ its nodes, nothing for a mesh
/// without lower-dimensional elements); if one fails — `element_to_subelement`
/// data naming an element that does not contain it — the ownership check
/// covers the nodes of **every** lower-dimensional element, so a partition
/// that would give a node two owners is refused as before
/// (the remaining nodes are held only by elements of dimension
/// `dim`).
///
/// The placement is greedy, in processing order (dimensions descending,
/// types in enum order, element index), and each orphan takes the
/// *minimum* admissible part. Chains of orphans
/// joined by nodes that no element of dimension `dim` holds can therefore
/// be refused although another choice would place them all: an orphan
/// whose admissible parts are {1, 2} takes part 1, and a later orphan
/// sharing an otherwise unheld node with it and a node held only by part 2
/// is refused (part 2 for the first would have placed both). Conforming
/// lower-dimensional elements are not affected (they have a common part).
///
/// Only the nodes of orphan elements are tracked: a
/// mesh without orphans costs no more than the compat mode, and one with
/// orphans tracks the parts of those nodes only.
///
/// `compat.orphan_lower_dimensional_in_part_zero` reproduces upstream
/// (part 0, no ghost parts, no check).
pub fn fill_partition_information_with(
    mesh: &Mesh,
    dim: usize,
    nb_parts: usize,
    linearized_parts: &[Idx],
    metadata: PartitionMetadata,
    compat: PartitionCompat,
) -> Result<MeshPartition> {
    mesh.check_consistency()?;
    let offsets = linearized_offsets(mesh, dim);
    let n: usize = offsets
        .last()
        .map_or(0, |&(t, o)| o + mesh.nb_element(t, NG));
    if linearized_parts.len() != n {
        return Err(Error::DimensionMismatch {
            context: "partition: one part per element of the partitioned dimension",
            expected: n,
            got: linearized_parts.len(),
        });
    }
    let nb = try_idx(nb_parts)?;
    if let Some(&bad) = linearized_parts.iter().find(|&&p| p < 0 || p >= nb) {
        return Err(Error::InvalidArgument(format!(
            "partition: part {bad} outside 0..{nb_parts}"
        )));
    }
    let n2e = node_to_elements(mesh, dim, mesh.is_periodic())?;
    let mut partitions = ElementTypeMapArray::new("partition");
    let mut ghost: ElementTypeMap<PartCsr> = ElementTypeMap::default();
    let mut scratch = Vec::new();
    for &(ty, off) in &offsets {
        let ne = mesh.nb_element(ty, NG);
        let parts = partitions.alloc(ne, 1, ty, NG, 0)?;
        let mut csr = PartCsr::default();
        csr.offsets.push(0);
        for e in 0..ne {
            let part = linearized_parts[off + e];
            parts.as_mut_slice()[e] = part;
            let mut adj: Vec<Idx> = Vec::new();
            for &node in effective_nodes(mesh, ty, e, mesh.is_periodic(), &mut scratch)? {
                for &a in n2e.row(node)? {
                    let ap = linearized_parts[a];
                    if ap != part {
                        adj.push(ap);
                    }
                }
            }
            adj.sort_unstable();
            adj.dedup();
            csr.push_row(adj);
        }
        ghost.insert(ty, NG, csr)?;
    }
    // parts holding each node of an orphan element in a local element
    // (UD-231 default rule), on the actual connectivity (the node flags of
    // the distribution); `None` without orphans or in compat mode
    let corrected = !compat.orphan_lower_dimensional_in_part_zero;
    let mut holders = if corrected {
        OrphanHolders::new(mesh, dim)?
    } else {
        None
    };
    // An element taking the part of an adjacent element
    // that does not contain it voids the construction argument
    let mut nonconforming = false;
    if let Some(h) = holders.as_mut() {
        for &(ty, _) in &offsets {
            h.add(mesh, ty, partitions.get(ty, NG)?.as_slice())?;
        }
    }
    // lower dimensions, from dim - 1 down to 0
    for sp in (0..dim).rev() {
        let types: Vec<ElementType> = mesh.element_types(TypeFilter::new().dim(sp)).collect();
        for ty in types {
            let ne = mesh.nb_element(ty, NG);
            let e2s = mesh
                .data()
                .element_lists(ELEMENT_TO_SUBELEMENT)
                .and_then(|m| m.get(ty, NG).cloned())
                .map_err(|_| {
                    Error::Mesh(format!(
                        "partition: no element_to_subelement data for {ty} (upstream \
                         getElementToSubelement throws)"
                    ))
                })?;
            if e2s.len() != ne {
                return Err(Error::DimensionMismatch {
                    context: "partition: element_to_subelement rows",
                    expected: ne,
                    got: e2s.len(),
                });
            }
            let mut parts_v = vec![0 as Idx; ne];
            let mut csr = PartCsr::default();
            csr.offsets.push(0);
            for (i, adjs) in e2s.as_slice().iter().enumerate() {
                if adjs.is_empty() {
                    if let Some(h) = holders.as_mut() {
                        let (part, ghosts) = orphan_part(mesh, ty, i, dim, h)?;
                        parts_v[i] = part;
                        for &n in mesh.connectivity(ty, NG)?.tuple(i) {
                            insert_sorted(h.of_mut(n)?, part);
                        }
                        csr.push_row(ghosts);
                    } else {
                        // upstream (UD-231 compat): part 0, no ghost parts
                        parts_v[i] = 0;
                        csr.push_row(std::iter::empty());
                    }
                    continue;
                }
                let mut min_part = Idx::MAX;
                let mut min_el: Option<Element> = None;
                let mut set: std::collections::BTreeSet<Idx> = std::collections::BTreeSet::new();
                for &a in adjs {
                    if a.is_null() {
                        continue;
                    }
                    let ap = *partitions
                        .get(a.ty(), NG)
                        .map_err(|_| {
                            Error::Mesh(format!(
                                "partition: {ty} element {i} is adjacent to {a}, whose type is \
                                 not partitioned"
                            ))
                        })?
                        .as_slice()
                        .get(a.idx())
                        .ok_or_else(|| {
                            Error::Mesh(format!("partition: adjacent element {a} out of range"))
                        })?;
                    if ap < min_part {
                        min_part = ap;
                        min_el = Some(a);
                    }
                    set.insert(ap);
                }
                let min_el = min_el.ok_or_else(|| {
                    Error::Mesh(format!(
                        "partition: {ty} element {i} has only null adjacent elements (upstream \
                         reads a sentinel element: undefined behaviour)"
                    ))
                })?;
                parts_v[i] = min_part;
                if corrected && !nonconforming {
                    let host = mesh.connectivity(min_el.ty(), NG)?;
                    let host = host.get_tuple(min_el.idx())?;
                    nonconforming = !mesh
                        .connectivity(ty, NG)?
                        .tuple(i)
                        .iter()
                        .all(|n| host.contains(n));
                }
                if let Some(g) = ghost.get(min_el.ty(), NG) {
                    set.extend(g.row(min_el.idx()).iter().copied());
                }
                set.remove(&min_part);
                csr.push_row(set);
            }
            if let Some(h) = holders.as_mut() {
                h.add(mesh, ty, &parts_v)?;
            }
            let a = partitions.alloc(ne, 1, ty, NG, 0)?;
            a.as_mut_slice().copy_from_slice(&parts_v);
            ghost.insert(ty, NG, csr)?;
        }
    }
    if nonconforming {
        let (slot, nb_slots) = lower_dimensional_node_slots(mesh, dim)?;
        check_unique_owners(mesh, &partitions, &ghost, &slot, nb_slots)?;
    } else if let Some(h) = holders.as_ref() {
        check_unique_owners(mesh, &partitions, &ghost, &h.slot, h.parts.len())?;
    }
    Ok(MeshPartition {
        nb_parts,
        spatial_dimension: dim,
        partitions,
        ghost_partitions: ghost,
        metadata,
    })
}

/// Slots of the nodes of every element of dimension `< dim` (the full
/// ownership check on non-conforming adjacency), in
/// ascending node order; `(slot, number of slots)`.
fn lower_dimensional_node_slots(mesh: &Mesh, dim: usize) -> Result<(Vec<usize>, usize)> {
    let n = mesh.nb_nodes();
    let mut slot = vec![NO_SLOT; n];
    for sp in 0..dim {
        for ty in mesh.element_types(TypeFilter::new().dim(sp)) {
            for &node in mesh.connectivity(ty, NG)?.as_slice() {
                let k = try_to_usize(node)?;
                *slot.get_mut(k).ok_or(Error::OutOfRange {
                    what: "node",
                    index: k,
                    len: n,
                })? = 0;
            }
        }
    }
    let mut count = 0;
    for s in slot.iter_mut().filter(|s| **s != NO_SLOT) {
        *s = count;
        count += 1;
    }
    Ok((slot, count))
}

fn insert_sorted(v: &mut Vec<Idx>, p: Idx) {
    if let Err(at) = v.binary_search(&p) {
        v.insert(at, p);
    }
}

/// No slot: the node belongs to no orphan element.
const NO_SLOT: usize = usize::MAX;

/// UD-231 default rule: the parts holding each node of an orphan element
/// in a local element (only those nodes are ever
/// read, so only those are tracked).
struct OrphanHolders {
    /// Slot of each mesh node in `parts` ([`NO_SLOT`]: not tracked).
    slot: Vec<usize>,
    /// Sorted holder parts per tracked node.
    parts: Vec<Vec<Idx>>,
}

impl OrphanHolders {
    /// Track the nodes of the orphan elements (empty
    /// `element_to_subelement` row) of the dimensions below `dim`; `None`
    /// when there are none. Types whose adjacency data is missing or
    /// malformed are skipped here (the partition reports them).
    fn new(mesh: &Mesh, dim: usize) -> Result<Option<Self>> {
        let n = mesh.nb_nodes();
        let mut slot: Vec<usize> = Vec::new();
        let mut count = 0;
        let lists = mesh.data().element_lists(ELEMENT_TO_SUBELEMENT).ok();
        for sp in (0..dim).rev() {
            for ty in mesh.element_types(TypeFilter::new().dim(sp)) {
                let Some(e2s) = lists.and_then(|m| m.get(ty, NG).ok()) else {
                    continue;
                };
                let conn = mesh.connectivity(ty, NG)?;
                if e2s.len() != conn.len() {
                    continue;
                }
                for (i, row) in e2s.as_slice().iter().enumerate() {
                    if !row.is_empty() {
                        continue;
                    }
                    if slot.is_empty() {
                        slot = vec![NO_SLOT; n];
                    }
                    for &node in conn.tuple(i) {
                        let k = try_to_usize(node)?;
                        let s = slot.get_mut(k).ok_or(Error::OutOfRange {
                            what: "node",
                            index: k,
                            len: n,
                        })?;
                        if *s == NO_SLOT {
                            *s = count;
                            count += 1;
                        }
                    }
                }
            }
        }
        Ok((count > 0).then(|| Self {
            slot,
            parts: vec![Vec::new(); count],
        }))
    }

    /// Holder parts of `node` (empty if not tracked).
    fn of(&self, node: Idx) -> Result<&[Idx]> {
        let k = try_to_usize(node)?;
        Ok(match self.slot.get(k) {
            Some(&s) if s != NO_SLOT => &self.parts[s],
            _ => &[],
        })
    }

    /// Holder parts of the tracked `node` (a node of an orphan element).
    fn of_mut(&mut self, node: Idx) -> Result<&mut Vec<Idx>> {
        let k = try_to_usize(node)?;
        match self.slot.get(k) {
            Some(&s) if s != NO_SLOT => Ok(&mut self.parts[s]),
            _ => Err(Error::Mesh(format!(
                "partition: node {node} of an orphan element is not tracked"
            ))),
        }
    }

    /// Record `parts[e]` as a holder of every tracked node of element `e`
    /// of `ty`.
    fn add(&mut self, mesh: &Mesh, ty: ElementType, parts: &[Idx]) -> Result<()> {
        let conn = mesh.connectivity(ty, NG)?;
        for (e, &p) in parts.iter().enumerate() {
            for &n in conn.tuple(e) {
                let k = try_to_usize(n)?;
                let s = *self.slot.get(k).ok_or(Error::OutOfRange {
                    what: "node",
                    index: k,
                    len: self.slot.len(),
                })?;
                if s != NO_SLOT {
                    insert_sorted(&mut self.parts[s], p);
                }
            }
        }
        Ok(())
    }
}

/// UD-231 default: part and ghost parts of the orphan element `i` of `ty`
/// (see [`fill_partition_information_with`]).
fn orphan_part(
    mesh: &Mesh,
    ty: ElementType,
    i: usize,
    dim: usize,
    holders: &OrphanHolders,
) -> Result<(Idx, Vec<Idx>)> {
    let mut common: Option<Vec<Idx>> = None;
    let mut any: Vec<Idx> = Vec::new();
    for &n in mesh.connectivity(ty, NG)?.tuple(i) {
        let h = holders.of(n)?;
        if h.is_empty() {
            continue;
        }
        for &p in h {
            insert_sorted(&mut any, p);
        }
        common = Some(match common {
            None => h.to_vec(),
            Some(c) => c
                .into_iter()
                .filter(|p| h.binary_search(p).is_ok())
                .collect(),
        });
    }
    match common {
        None => Ok((0, Vec::new())),
        Some(c) => {
            let part = *c.first().ok_or_else(|| {
                Error::Mesh(format!(
                    "partition: {ty} element {i} is adjacent to no element of the next \
                     dimension and its nodes are held by different parts {any:?} (elements of \
                     dimension {dim}): no part can hold it without giving one of its nodes two \
                     owners (UD-231; PartitionCompat::orphan_lower_dimensional_in_part_zero \
                     reproduces upstream's part 0)"
                ))
            })?;
            any.retain(|&p| p != part);
            Ok((part, any))
        }
    }
}

/// Every node has exactly one owner after distribution: a node held
/// locally by two or more parts is held as a ghost by each of them (the
/// distribution's flag rules; UD-231 post-check, an invariant of the
/// default rule). Checked for the nodes with a slot (`slot[k] <
/// nb_slots`: the nodes of orphan elements); the others satisfy it by
/// construction (see [`fill_partition_information_with`]).
fn check_unique_owners(
    mesh: &Mesh,
    partitions: &ElementTypeMapArray<Idx>,
    ghost: &ElementTypeMap<PartCsr>,
    slot: &[usize],
    nb_slots: usize,
) -> Result<()> {
    let mut local: Vec<Vec<Idx>> = vec![Vec::new(); nb_slots];
    let mut ghosts: Vec<Vec<Idx>> = vec![Vec::new(); nb_slots];
    for (ty, parts) in partitions.as_map().iter(NG) {
        let conn = mesh.connectivity(ty, NG)?;
        let rows = ghost.get(ty, NG);
        for (e, &p) in parts.as_slice().iter().enumerate() {
            for &node in conn.tuple(e) {
                let k = try_to_usize(node)?;
                let s = slot.get(k).copied().unwrap_or(NO_SLOT);
                if s >= nb_slots {
                    continue;
                }
                insert_sorted(&mut local[s], p);
                for &q in rows.map_or(&[][..], |r| r.row(e)) {
                    insert_sorted(&mut ghosts[s], q);
                }
            }
        }
    }
    // ascending node order (the first offending node is reported)
    for (k, &s) in slot.iter().enumerate() {
        if s >= nb_slots {
            continue;
        }
        let (l, g) = (&local[s], &ghosts[s]);
        if l.len() > 1 && l.iter().any(|p| g.binary_search(p).is_err()) {
            return Err(Error::Mesh(format!(
                "partition: node {k} would have several owners (held locally by parts {l:?}, \
                 as a ghost by {g:?}; UD-231)"
            )));
        }
    }
    Ok(())
}

// ------------------------------------------------------------- partitioners

/// A partition given as the parts of the elements of the mesh dimension in
/// linearised order (types in enum order, then element index) — user input
/// or the `partition` array of an oracle fixture.
#[derive(Clone, Debug)]
pub struct PrescribedPartition {
    /// Linearised parts.
    pub parts: Vec<Idx>,
    /// Upstream-compatibility switches of the lower-dimensional parts.
    pub compat: PartitionCompat,
}

impl PrescribedPartition {
    /// From linearised parts.
    #[must_use]
    pub fn new(parts: Vec<Idx>) -> Self {
        Self {
            parts,
            compat: PartitionCompat::default(),
        }
    }

    /// With upstream-compatibility switches (differential replays).
    #[must_use]
    pub fn with_compat(mut self, compat: PartitionCompat) -> Self {
        self.compat = compat;
        self
    }
}

impl MeshPartitioner for PrescribedPartition {
    fn partition(&mut self, mesh: &Mesh, nb_parts: usize) -> Result<MeshPartition> {
        fill_partition_information_with(
            mesh,
            mesh.spatial_dimension(),
            nb_parts,
            &self.parts,
            PartitionMetadata::simple(PartitionerKind::Prescribed, nb_parts),
            self.compat,
        )
    }
}

/// Upstream `MeshPartitionMeshData`: the part of every not-ghost element of
/// the mesh dimension read from `mapping` (one value per element). Upstream
/// asserts in debug builds that exactly `nb_parts` distinct parts occur; the
/// port checks it always.
#[derive(Clone, Debug)]
pub struct MeshDataPartition {
    /// Part per element.
    pub mapping: ElementTypeMapArray<Idx>,
    /// Upstream-compatibility switches of the lower-dimensional parts.
    pub compat: PartitionCompat,
}

impl MeshDataPartition {
    /// From a part per element of the mesh dimension.
    #[must_use]
    pub fn new(mapping: ElementTypeMapArray<Idx>) -> Self {
        Self {
            mapping,
            compat: PartitionCompat::default(),
        }
    }

    /// With upstream-compatibility switches (differential replays).
    #[must_use]
    pub fn with_compat(mut self, compat: PartitionCompat) -> Self {
        self.compat = compat;
        self
    }
}

impl MeshPartitioner for MeshDataPartition {
    fn partition(&mut self, mesh: &Mesh, nb_parts: usize) -> Result<MeshPartition> {
        let dim = mesh.spatial_dimension();
        let mut parts = Vec::new();
        let mut seen = std::collections::BTreeSet::new();
        for ty in mesh.element_types(TypeFilter::new().dim(dim)) {
            let a = self.mapping.get(ty, NG)?;
            if a.len() != mesh.nb_element(ty, NG) || a.nb_component() != 1 {
                return Err(Error::DimensionMismatch {
                    context: "MeshDataPartition: one part per element",
                    expected: mesh.nb_element(ty, NG),
                    got: a.len(),
                });
            }
            parts.extend_from_slice(a.as_slice());
            seen.extend(a.as_slice().iter().copied());
        }
        if seen.len() != nb_parts {
            return Err(Error::InvalidArgument(format!(
                "MeshDataPartition: {} distinct parts for {nb_parts} requested (upstream debug \
                 assertion)",
                seen.len()
            )));
        }
        fill_partition_information_with(
            mesh,
            dim,
            nb_parts,
            &parts,
            PartitionMetadata::simple(PartitionerKind::MeshData, nb_parts),
            self.compat,
        )
    }
}

/// Pure-Rust recursive coordinate bisection of the element barycentres
/// (elements of the mesh dimension): the longest extent is split at the
/// weighted median, parts proportionally to the element counts. A
/// **different** partitioner from Scotch, for builds without it (D-020).
#[derive(Clone, Copy, Debug, Default)]
pub struct RcbPartitioner;

impl MeshPartitioner for RcbPartitioner {
    fn partition(&mut self, mesh: &Mesh, nb_parts: usize) -> Result<MeshPartition> {
        if nb_parts == 0 {
            return Err(Error::InvalidArgument("partition into 0 parts".into()));
        }
        let dim = mesh.spatial_dimension();
        let mut centers: Vec<(Vec<Real>, usize)> = Vec::new();
        for ty in mesh.element_types(TypeFilter::new().dim(dim)) {
            for e in 0..mesh.nb_element(ty, NG) {
                let c = mesh.barycenter(Element::try_new(ty, e, NG)?)?;
                let i = centers.len();
                centers.push((c, i));
            }
        }
        let mut parts = vec![0 as Idx; centers.len()];
        rcb(&mut centers, 0, nb_parts, &mut parts)?;
        fill_partition_information(
            mesh,
            dim,
            nb_parts,
            &parts,
            PartitionMetadata::simple(PartitionerKind::RecursiveCoordinateBisection, nb_parts),
        )
    }
}

fn rcb(items: &mut [(Vec<Real>, usize)], first: usize, nb: usize, out: &mut [Idx]) -> Result<()> {
    if nb <= 1 || items.len() <= 1 {
        let p = try_idx(first)?;
        for (_, i) in items.iter() {
            out[*i] = p;
        }
        return Ok(());
    }
    let d = items.first().map_or(0, |(c, _)| c.len());
    let axis = (0..d)
        .map(|k| {
            let (lo, hi) = items
                .iter()
                .fold((Real::INFINITY, Real::NEG_INFINITY), |(lo, hi), (c, _)| {
                    (lo.min(c[k]), hi.max(c[k]))
                });
            (k, hi - lo)
        })
        .fold(
            (0, Real::NEG_INFINITY),
            |b, x| if x.1 > b.1 { x } else { b },
        )
        .0;
    items.sort_by(|a, b| a.0[axis].total_cmp(&b.0[axis]).then(a.1.cmp(&b.1)));
    let left = nb / 2;
    let cut = items.len() * left / nb;
    let (l, r) = items.split_at_mut(cut);
    rcb(l, first, left, out)?;
    rcb(r, first + left, nb - left, out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MeshAccessor;

    /// 2 × 2 quadrangles: linearisation, dual graph (4-neighbourhood by
    /// edges) and ghost parts of a prescribed 2-part split.
    fn quad_mesh() -> Mesh {
        let mut m = Mesh::new(2, "q").unwrap();
        {
            let mut a = MeshAccessor::new(&mut m);
            for j in 0..3 {
                for i in 0..3 {
                    a.push_node(&[f64::from(i), f64::from(j)]).unwrap();
                }
            }
            let mut c = a.connectivity_mut(ElementType::Quadrangle4, NG);
            for j in 0..2 {
                for i in 0..2 {
                    let n0 = j * 3 + i;
                    c.push_tuple(&[n0, n0 + 1, n0 + 4, n0 + 3]);
                }
            }
        }
        m
    }

    #[test]
    fn dual_graph_of_quadrangles_uses_edge_adjacency() {
        let m = quad_mesh();
        let g = build_dual_graph(&m, &|_, _| 1, &|_| 1).unwrap();
        assert_eq!(g.xadj, vec![0, 2, 4, 6, 8]);
        assert_eq!(g.adjncy, vec![1, 2, 0, 3, 0, 3, 1, 2]);
        assert_eq!(g.vertex_loads, vec![1; 4]);
    }

    #[test]
    fn ghost_parts_include_node_neighbours() {
        let m = quad_mesh();
        let p = PrescribedPartition::new(vec![0, 1, 1, 1])
            .partition(&m, 2)
            .unwrap();
        let g = p.ghost_partition(ElementType::Quadrangle4).unwrap();
        // every element touches the centre node 4
        assert_eq!(g.row(0), &[1]);
        assert_eq!(g.row(1), &[0]);
        assert_eq!(g.row(3), &[0]);
        assert_eq!(p.linearized(), vec![0, 1, 1, 1]);
    }

    #[test]
    fn invalid_parts_are_rejected() {
        let m = quad_mesh();
        assert!(
            PrescribedPartition::new(vec![0, 2, 1, 1])
                .partition(&m, 2)
                .is_err()
        );
        assert!(
            PrescribedPartition::new(vec![0, 1])
                .partition(&m, 2)
                .is_err()
        );
    }

    /// Three quadrangles in a row (parts 0, 1, 2), a boundary segment and
    /// `extra` lower-dimensional elements; `element_to_subelement` filled.
    fn strip(extra: &[(ElementType, Vec<Idx>)]) -> Mesh {
        let mut m = Mesh::new(2, "strip").unwrap();
        {
            let mut a = MeshAccessor::new(&mut m);
            for j in 0..2 {
                for i in 0..4 {
                    a.push_node(&[f64::from(i), f64::from(j)]).unwrap();
                }
            }
            a.push_node(&[5.0, 5.0]).unwrap(); // node 8: in no quadrangle
            let mut c = a.connectivity_mut(ElementType::Quadrangle4, NG);
            for i in 0..3 {
                c.push_tuple(&[i, i + 1, i + 5, i + 4]);
            }
            drop(c);
            a.connectivity_mut(ElementType::Segment2, NG)
                .push_tuple(&[0, 4]);
            for (ty, conn) in extra {
                a.connectivity_mut(*ty, NG).push_tuple(conn);
            }
        }
        crate::utils::fill_element_to_subelements_data(&mut m).unwrap();
        m
    }

    /// UD-231: orphan points follow the parts holding
    /// their node — an interface node (parts 1, 2) → part 1, ghost in 2; a
    /// node of part 2 only → part 2; a node of no element → part 0.
    /// Upstream (compat): part 0, no ghost parts.
    #[test]
    fn orphan_points_follow_their_nodes() {
        let pts = [
            (ElementType::Point1, vec![6]),
            (ElementType::Point1, vec![3]),
            (ElementType::Point1, vec![8]),
        ];
        // one point per call (one Point1 block)
        for (k, want_part, want_ghost) in [(0, 1, vec![2]), (1, 2, vec![]), (2, 0, vec![])] {
            let m = strip(&pts[k..=k]);
            let p = PrescribedPartition::new(vec![0, 1, 2])
                .partition(&m, 3)
                .unwrap();
            assert_eq!(p.partition(ElementType::Point1).unwrap(), &[want_part]);
            let g = p.ghost_partition(ElementType::Point1).unwrap();
            assert_eq!(g.row(0), want_ghost.as_slice(), "point {k}");
            // the boundary segment of quadrangle 0 is not an orphan
            assert_eq!(p.partition(ElementType::Segment2).unwrap(), &[0]);
            let c = PrescribedPartition::new(vec![0, 1, 2])
                .with_compat(PartitionCompat {
                    orphan_lower_dimensional_in_part_zero: true,
                })
                .partition(&m, 3)
                .unwrap();
            assert_eq!(c.partition(ElementType::Point1).unwrap(), &[0]);
            assert!(
                c.ghost_partition(ElementType::Point1)
                    .unwrap()
                    .row(0)
                    .is_empty()
            );
        }
    }

    /// UD-231: an orphan segment between nodes held only by parts 0 and 2
    /// cannot be placed without giving a node two owners: refused (the
    /// compat switch reproduces upstream's part 0).
    #[test]
    fn orphan_segment_across_parts_is_refused() {
        let m = strip(&[(ElementType::Segment2, vec![0, 3])]);
        let e = PrescribedPartition::new(vec![0, 1, 2])
            .partition(&m, 3)
            .unwrap_err()
            .to_string();
        assert!(e.contains("UD-231") && e.contains("element 1"), "{e}");
        let c = PrescribedPartition::new(vec![0, 1, 2])
            .with_compat(PartitionCompat {
                orphan_lower_dimensional_in_part_zero: true,
            })
            .partition(&m, 3)
            .unwrap();
        assert_eq!(c.partition(ElementType::Segment2).unwrap(), &[0, 0]);
    }

    /// The dual graph of the port before the counting-array rewrite
    /// (`BTreeMap<Element, usize>` of shared-node counts per element, a
    /// `Vec<Element>` per node): the reference of
    /// `dual_graph_matches_the_ordered_map_construction`.
    fn reference_dual_graph(
        mesh: &Mesh,
        edge_load: &dyn Fn(Element, Element) -> Int,
        vertex_load: &dyn Fn(Element) -> Int,
    ) -> DualGraph {
        use std::collections::BTreeMap;
        let dim = mesh.spatial_dimension();
        let offsets = linearized_offsets(mesh, dim);
        let lin = |el: Element| {
            offsets
                .iter()
                .find(|(t, _)| *t == el.ty())
                .map(|(_, o)| o + el.idx())
                .unwrap()
        };
        let n: usize = offsets
            .last()
            .map_or(0, |&(t, o)| o + mesh.nb_element(t, NG));
        let mut rows: Vec<Vec<Element>> = vec![Vec::new(); mesh.nb_nodes()];
        for &(ty, _) in &offsets {
            let conn = mesh.connectivity(ty, NG).unwrap();
            for e in 0..mesh.nb_element(ty, NG) {
                for &nd in conn.tuple(e) {
                    rows[nd as usize].push(Element::try_new(ty, e, NG).unwrap());
                }
            }
        }
        let mut g = DualGraph::default();
        g.xadj.push(0);
        for &(ty, _) in &offsets {
            let magic = (0..ty.nb_facet_types())
                .filter_map(|t| ty.facet_type(t))
                .map(ElementType::nb_nodes_per_element)
                .fold(ty.nb_nodes_per_element(), usize::min);
            let conn = mesh.connectivity(ty, NG).unwrap();
            for e in 0..mesh.nb_element(ty, NG) {
                let el = Element::try_new(ty, e, NG).unwrap();
                let mut hits: BTreeMap<Element, usize> = BTreeMap::new();
                for &nd in conn.tuple(e) {
                    for &c in &rows[nd as usize] {
                        *hits.entry(c).or_default() += 1;
                    }
                }
                g.vertex_loads.push(vertex_load(el));
                for (adj, count) in hits {
                    if adj == el || count < magic {
                        continue;
                    }
                    if el.ty().kind() == ElementKind::Cohesive
                        && adj.ty().kind() == ElementKind::Cohesive
                    {
                        continue;
                    }
                    g.adjncy.push(Int::try_from(lin(adj)).unwrap());
                    g.edge_loads.push(edge_load(el, adj));
                }
                g.xadj.push(Int::try_from(g.adjncy.len()).unwrap());
            }
        }
        assert_eq!(g.xadj.len(), n + 1);
        g
    }

    /// The counting-array dual graph equals the ordered-map construction
    /// (same neighbour order, loads and offsets) on a mixed triangle and
    /// quadrangle mesh with scrambled connectivities and element-dependent
    /// loads.
    #[test]
    fn dual_graph_matches_the_ordered_map_construction() {
        let mut m = Mesh::new(2, "mixed").unwrap();
        let (nx, ny) = (9usize, 7usize);
        {
            let mut a = MeshAccessor::new(&mut m);
            for j in 0..=ny {
                for i in 0..=nx {
                    #[allow(clippy::cast_precision_loss)] // small
                    a.push_node(&[i as f64, j as f64]).unwrap();
                }
            }
            let id = |i: usize, j: usize| Idx::try_from(j * (nx + 1) + i).unwrap();
            // a pseudo-random visiting order of the cells
            let mut cells: Vec<(usize, usize)> =
                (0..ny).flat_map(|j| (0..nx).map(move |i| (i, j))).collect();
            let mut r = 12_345u64;
            for k in (1..cells.len()).rev() {
                r = r.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
                cells.swap(k, (r >> 33) as usize % (k + 1));
            }
            for (k, &(i, j)) in cells.iter().enumerate() {
                let (a0, a1, a2, a3) = (id(i, j), id(i + 1, j), id(i + 1, j + 1), id(i, j + 1));
                if k % 3 == 0 {
                    a.connectivity_mut(ElementType::Quadrangle4, NG)
                        .push_tuple(&[a1, a2, a3, a0]);
                } else {
                    let mut c = a.connectivity_mut(ElementType::Triangle3, NG);
                    c.push_tuple(&[a0, a1, a2]);
                    c.push_tuple(&[a2, a3, a0]);
                }
            }
        }
        let el_key = |e: Element| Int::try_from(e.idx() * 3 + e.ty().index()).unwrap();
        let edge = |a: Element, b: Element| 1 + (el_key(a) * 7 + el_key(b)) % 5;
        let vertex = |a: Element| 1 + el_key(a) % 4;
        let got = build_dual_graph(&m, &edge, &vertex).unwrap();
        let want = reference_dual_graph(&m, &edge, &vertex);
        assert_eq!(got, want);
        assert!(got.adjncy.len() > 100);
    }

    /// Deterministic pseudo-random sequence of the cohesive dual-graph tests.
    fn lcg(r: &mut u64) -> usize {
        *r = r.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
        (*r >> 33) as usize
    }

    /// Element-dependent edge and vertex loads (so that a vertex or a
    /// neighbour taken from the wrong element shows in the loads).
    fn graph_with_loads(m: &Mesh) -> (DualGraph, DualGraph) {
        let key = |e: Element| Int::try_from(e.idx() * 31 + e.ty().index()).unwrap();
        let edge = |a: Element, b: Element| 1 + (key(a) * 7 + key(b)) % 11;
        let vertex = |a: Element| 1 + key(a) % 5;
        (
            build_dual_graph(m, &edge, &vertex).unwrap(),
            reference_dual_graph(m, &edge, &vertex),
        )
    }

    /// The counting-array dual graph equals the ordered-map construction
    /// on a 2-D mesh with triangles, quadrangles, `_cohesive_2d_4` elements
    /// (unopened ones with repeated nodes, several sharing nodes, two on
    /// the same edge) and lower-dimensional `_segment_2` elements: the rule
    /// that two cohesive elements are never adjacent and the linear-index →
    /// type lookup are exercised.
    #[test]
    fn dual_graph_with_cohesive_elements_matches_the_reference_2d() {
        let mut m = Mesh::new(2, "cohesive-2d").unwrap();
        let (nx, ny) = (7usize, 6usize);
        let id = |i: usize, j: usize| Idx::try_from(j * (nx + 1) + i).unwrap();
        {
            let mut a = MeshAccessor::new(&mut m);
            for j in 0..=ny {
                for i in 0..=nx {
                    #[allow(clippy::cast_precision_loss)] // small
                    a.push_node(&[i as f64, j as f64]).unwrap();
                }
            }
            let mut r = 7u64;
            for j in 0..ny {
                for i in 0..nx {
                    let (a0, a1, a2, a3) = (id(i, j), id(i + 1, j), id(i + 1, j + 1), id(i, j + 1));
                    if lcg(&mut r).is_multiple_of(3) {
                        a.connectivity_mut(ElementType::Quadrangle4, NG)
                            .push_tuple(&[a0, a1, a2, a3]);
                    } else {
                        let mut c = a.connectivity_mut(ElementType::Triangle3, NG);
                        c.push_tuple(&[a0, a1, a2]);
                        c.push_tuple(&[a2, a3, a0]);
                    }
                }
            }
            {
                let mut coh = a.connectivity_mut(ElementType::Cohesive2d4, NG);
                // unopened cohesive elements on interior vertical edges (repeated
                // nodes); neighbours along a column share a node
                for j in 0..ny {
                    for i in [2usize, 5] {
                        let (p, q) = (id(i, j), id(i, j + 1));
                        coh.push_tuple(&[p, q, p, q]);
                    }
                }
                // two cohesive elements on the same edge, and opened ones
                // joining two different edges
                coh.push_tuple(&[id(3, 3), id(4, 3), id(3, 3), id(4, 3)]);
                coh.push_tuple(&[id(3, 3), id(4, 3), id(3, 3), id(4, 3)]);
                coh.push_tuple(&[id(1, 1), id(2, 1), id(1, 2), id(2, 2)]);
                coh.push_tuple(&[id(5, 4), id(6, 4), id(5, 5), id(6, 5)]);
            }
            let mut seg = a.connectivity_mut(ElementType::Segment2, NG);
            for i in 0..nx {
                seg.push_tuple(&[id(i, 0), id(i + 1, 0)]);
            }
        }
        let (got, want) = graph_with_loads(&m);
        assert_eq!(got, want);
        // the cohesive elements have neighbours (bulk ones only)
        let n_bulk =
            m.nb_element(ElementType::Triangle3, NG) + m.nb_element(ElementType::Quadrangle4, NG);
        let n = got.xadj.len() - 1;
        let first_coh = n - m.nb_element(ElementType::Cohesive2d4, NG);
        assert!(
            first_coh >= n_bulk,
            "cohesive elements are listed after the bulk types"
        );
        for v in first_coh..n {
            let (a, b) = (got.xadj[v] as usize, got.xadj[v + 1] as usize);
            assert!(b > a, "cohesive vertex {v} without neighbour");
            assert!(got.adjncy[a..b].iter().all(|&w| (w as usize) < first_coh));
        }
    }

    /// The 3-D variant: tetrahedra (Kuhn split cells), hexahedra and
    /// `_cohesive_3d_6` elements (unopened, sharing nodes, two on the same
    /// face, opened ones).
    #[test]
    fn dual_graph_with_cohesive_elements_matches_the_reference_3d() {
        let mut m = Mesh::new(3, "cohesive-3d").unwrap();
        let n = 4usize;
        let id =
            |i: usize, j: usize, k: usize| Idx::try_from((k * (n + 1) + j) * (n + 1) + i).unwrap();
        {
            let mut a = MeshAccessor::new(&mut m);
            for k in 0..=n {
                for j in 0..=n {
                    for i in 0..=n {
                        #[allow(clippy::cast_precision_loss)] // small
                        a.push_node(&[i as f64, j as f64, k as f64]).unwrap();
                    }
                }
            }
            let mut r = 11u64;
            for k in 0..n {
                for j in 0..n {
                    for i in 0..n {
                        // corner c = x + 2y + 4z of the cell
                        let v: Vec<Idx> = (0..8)
                            .map(|c| id(i + (c & 1), j + ((c >> 1) & 1), k + ((c >> 2) & 1)))
                            .collect();
                        if lcg(&mut r).is_multiple_of(3) {
                            a.connectivity_mut(ElementType::Hexahedron8, NG)
                                .push_tuple(&[v[0], v[1], v[3], v[2], v[4], v[5], v[7], v[6]]);
                        } else {
                            let mut c = a.connectivity_mut(ElementType::Tetrahedron4, NG);
                            for t in [
                                [0, 1, 3, 7],
                                [0, 1, 5, 7],
                                [0, 2, 3, 7],
                                [0, 2, 6, 7],
                                [0, 4, 5, 7],
                                [0, 4, 6, 7],
                            ] {
                                c.push_tuple(&[v[t[0]], v[t[1]], v[t[2]], v[t[3]]]);
                            }
                        }
                    }
                }
            }
            let mut coh = a.connectivity_mut(ElementType::Cohesive3d6, NG);
            for j in 0..n {
                for k in 0..n {
                    // unopened, on the plane x = 2, neighbours share an edge
                    let (p, q, s) = (id(2, j, k), id(2, j + 1, k), id(2, j + 1, k + 1));
                    coh.push_tuple(&[p, q, s, p, q, s]);
                }
            }
            let (p, q, s) = (id(1, 1, 1), id(2, 1, 1), id(2, 2, 1));
            coh.push_tuple(&[p, q, s, p, q, s]);
            coh.push_tuple(&[p, q, s, p, q, s]);
            coh.push_tuple(&[
                id(1, 1, 2),
                id(2, 1, 2),
                id(2, 2, 2),
                id(1, 1, 3),
                id(2, 1, 3),
                id(2, 2, 3),
            ]);
        }
        let (got, want) = graph_with_loads(&m);
        assert_eq!(got, want);
        assert!(got.adjncy.len() > 500);
    }

    /// UD-234: an isolated element (sharing no node) is a vertex without
    /// edges; upstream sizes `xadj` by the elements that have neighbours
    /// and writes out of bounds.
    #[test]
    fn dual_graph_lists_isolated_elements() {
        let mut m = quad_mesh();
        {
            let mut a = MeshAccessor::new(&mut m);
            let n0 = a.push_node(&[10.0, 10.0]).unwrap();
            for x in [[11.0, 10.0], [11.0, 11.0], [10.0, 11.0]] {
                a.push_node(&x).unwrap();
            }
            a.connectivity_mut(ElementType::Quadrangle4, NG)
                .push_tuple(&[n0, n0 + 1, n0 + 2, n0 + 3]);
        }
        let g = build_dual_graph(&m, &|_, _| 1, &|_| 1).unwrap();
        assert_eq!(g.xadj, vec![0, 2, 4, 6, 8, 8]);
        assert_eq!(g.vertex_loads, vec![1; 5]);
        let p = PrescribedPartition::new(vec![0, 0, 1, 1, 1])
            .partition(&m, 2)
            .unwrap();
        assert!(
            p.ghost_partition(ElementType::Quadrangle4)
                .unwrap()
                .row(4)
                .is_empty()
        );
    }

    // ------------------------------------------- (b)

    /// The earlier UD-231 default rule: holders of every
    /// node, built eagerly, and the ownership post-check on every node
    /// (reference for `orphan_tracking_matches_the_eager_rule`).
    fn reference_default_fill(
        mesh: &Mesh,
        dim: usize,
        lin: &[Idx],
    ) -> Result<Vec<(ElementType, Vec<Idx>, PartCsr)>> {
        let offsets = linearized_offsets(mesh, dim);
        let n2e = node_to_elements(mesh, dim, mesh.is_periodic())?;
        let mut partitions = ElementTypeMapArray::new("partition");
        let mut ghost: ElementTypeMap<PartCsr> = ElementTypeMap::default();
        let mut scratch = Vec::new();
        for &(ty, off) in &offsets {
            let ne = mesh.nb_element(ty, NG);
            let parts = partitions.alloc(ne, 1, ty, NG, 0)?;
            let mut csr = PartCsr::default();
            csr.offsets.push(0);
            for e in 0..ne {
                let part = lin[off + e];
                parts.as_mut_slice()[e] = part;
                let mut adj: Vec<Idx> = Vec::new();
                for &node in effective_nodes(mesh, ty, e, mesh.is_periodic(), &mut scratch)? {
                    for &a in n2e.row(node)? {
                        let ap = lin[a];
                        if ap != part {
                            adj.push(ap);
                        }
                    }
                }
                adj.sort_unstable();
                adj.dedup();
                csr.push_row(adj);
            }
            ghost.insert(ty, NG, csr)?;
        }
        let add = |ty: ElementType, parts: &[Idx], h: &mut [Vec<Idx>]| -> Result<()> {
            let conn = mesh.connectivity(ty, NG)?;
            for (e, &p) in parts.iter().enumerate() {
                for &n in conn.tuple(e) {
                    insert_sorted(&mut h[try_to_usize(n)?], p);
                }
            }
            Ok(())
        };
        let mut holders = vec![Vec::new(); mesh.nb_nodes()];
        for &(ty, _) in &offsets {
            add(ty, partitions.get(ty, NG)?.as_slice(), &mut holders)?;
        }
        for sp in (0..dim).rev() {
            let types: Vec<ElementType> = mesh.element_types(TypeFilter::new().dim(sp)).collect();
            for ty in types {
                let ne = mesh.nb_element(ty, NG);
                let e2s = mesh
                    .data()
                    .element_lists(ELEMENT_TO_SUBELEMENT)?
                    .get(ty, NG)?
                    .clone();
                let mut parts_v = vec![0 as Idx; ne];
                let mut csr = PartCsr::default();
                csr.offsets.push(0);
                for (i, adjs) in e2s.as_slice().iter().enumerate() {
                    if adjs.is_empty() {
                        // eager orphan placement over all nodes' holders
                        let mut common: Option<Vec<Idx>> = None;
                        let mut any: Vec<Idx> = Vec::new();
                        for &n in mesh.connectivity(ty, NG)?.tuple(i) {
                            let h = &holders[try_to_usize(n)?];
                            if h.is_empty() {
                                continue;
                            }
                            for &p in h {
                                insert_sorted(&mut any, p);
                            }
                            common = Some(match common {
                                None => h.clone(),
                                Some(c) => c
                                    .into_iter()
                                    .filter(|p| h.binary_search(p).is_ok())
                                    .collect(),
                            });
                        }
                        let (part, ghosts) = match common {
                            None => (0, Vec::new()),
                            Some(c) => {
                                let part = *c.first().ok_or_else(|| {
                                    Error::Mesh(format!(
                                        "{ty} element {i} is adjacent to no element of the next"
                                    ))
                                })?;
                                any.retain(|&p| p != part);
                                (part, any)
                            }
                        };
                        parts_v[i] = part;
                        for &n in mesh.connectivity(ty, NG)?.tuple(i) {
                            insert_sorted(&mut holders[try_to_usize(n)?], part);
                        }
                        csr.push_row(ghosts);
                        continue;
                    }
                    let mut min_part = Idx::MAX;
                    let mut min_el = None;
                    let mut set = std::collections::BTreeSet::new();
                    for &a in adjs {
                        let ap = partitions.get(a.ty(), NG)?.as_slice()[a.idx()];
                        if ap < min_part {
                            min_part = ap;
                            min_el = Some(a);
                        }
                        set.insert(ap);
                    }
                    let min_el = min_el.unwrap();
                    parts_v[i] = min_part;
                    if let Some(g) = ghost.get(min_el.ty(), NG) {
                        set.extend(g.row(min_el.idx()).iter().copied());
                    }
                    set.remove(&min_part);
                    csr.push_row(set);
                }
                add(ty, &parts_v, &mut holders)?;
                partitions
                    .alloc(ne, 1, ty, NG, 0)?
                    .as_mut_slice()
                    .copy_from_slice(&parts_v);
                ghost.insert(ty, NG, csr)?;
            }
        }
        // full post-check: every node of the mesh
        let all = (0..mesh.nb_nodes()).collect::<Vec<_>>();
        check_unique_owners(mesh, &partitions, &ghost, &all, all.len())?;
        Ok(partitions
            .as_map()
            .iter(NG)
            .map(|(ty, a)| {
                (
                    ty,
                    a.as_slice().to_vec(),
                    ghost.get(ty, NG).unwrap().clone(),
                )
            })
            .collect())
    }

    /// The default rule now tracks only the nodes
    /// of orphan elements and checks ownership only there. On seeded
    /// random partitions of a 4 × 3 quadrangle grid with random boundary
    /// segments (orphan or not), orphan segments and points (some on a
    /// node of no quadrangle), the parts, the ghost parts and the
    /// accept/refuse decision equal the eager rule's (which checks every
    /// node), and every accepted partition passes the full ownership
    /// check.
    #[test]
    fn orphan_tracking_matches_the_eager_rule() {
        let mut state: u64 = 0x9e37_79b9_7f4a_7c15;
        let mut next = |m: usize| -> usize {
            state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            usize::try_from(state >> 33).unwrap() % m
        };
        let (nx, ny) = (4, 3);
        let nodes_x = nx + 1;
        let nb_grid = nodes_x * (ny + 1);
        let (mut accepted, mut refused, mut without_orphans) = (0, 0, 0);
        let mut post_check_refusals = 0;
        for case in 0..400 {
            let mut m = Mesh::new(2, "grid").unwrap();
            {
                let mut a = MeshAccessor::new(&mut m);
                for j in 0..=ny {
                    for i in 0..=nx {
                        a.push_node(&[f64::from(i as u8), f64::from(j as u8)])
                            .unwrap();
                    }
                }
                // two nodes in no quadrangle
                a.push_node(&[9.0, 9.0]).unwrap();
                a.push_node(&[9.0, 8.0]).unwrap();
                let idx = |i: usize, j: usize| Idx::try_from(j * nodes_x + i).unwrap();
                let mut q = a.connectivity_mut(ElementType::Quadrangle4, NG);
                for j in 0..ny {
                    for i in 0..nx {
                        q.push_tuple(&[idx(i, j), idx(i + 1, j), idx(i + 1, j + 1), idx(i, j + 1)]);
                    }
                }
                let nb_nodes = nb_grid + 2;
                let node = |k: usize| Idx::try_from(k).unwrap();
                // segments: a boundary edge (so that points have
                // adjacency data), then grid edges (not orphans) or random
                // pairs
                drop(q);
                a.connectivity_mut(ElementType::Segment2, NG)
                    .push_tuple(&[idx(0, 0), idx(0, 1)]);
                let nseg = next(4);
                for _ in 0..nseg {
                    let s = if next(2) == 0 {
                        let (i, j) = (next(nx), next(ny + 1));
                        [idx(i, j), idx(i + 1, j)]
                    } else {
                        let a0 = next(nb_nodes);
                        let mut b0 = next(nb_nodes);
                        if b0 == a0 {
                            b0 = (a0 + 1) % nb_nodes;
                        }
                        [node(a0), node(b0)]
                    };
                    a.connectivity_mut(ElementType::Segment2, NG).push_tuple(&s);
                }
                let npt = next(4);
                for _ in 0..npt {
                    let k = next(nb_nodes);
                    a.connectivity_mut(ElementType::Point1, NG)
                        .push_tuple(&[node(k)]);
                }
            }
            crate::utils::fill_element_to_subelements_data(&mut m).unwrap();
            // In every third case one non-orphan segment's
            // adjacency names a random quadrangle (non-conforming data,
            // usually one that does not contain it), where only the
            // ownership post-check can refuse
            if case % 3 == 2 {
                let lists = m
                    .data_mut()
                    .element_lists_mut(ELEMENT_TO_SUBELEMENT)
                    .unwrap();
                let rows = lists.get_mut(ElementType::Segment2, NG).unwrap();
                let candidates: Vec<usize> = (0..rows.len())
                    .filter(|&r| !rows.as_slice()[r].is_empty())
                    .collect();
                let r = candidates[next(candidates.len())];
                rows.as_mut_slice()[r] = vec![Element::new(
                    ElementType::Quadrangle4,
                    next(nx * ny).try_into().unwrap(),
                    NG,
                )];
            }
            let nb_parts = 2 + next(3);
            let lin: Vec<Idx> = (0..nx * ny)
                .map(|_| Idx::try_from(next(nb_parts)).unwrap())
                .collect();
            if OrphanHolders::new(&m, 2).unwrap().is_none() {
                without_orphans += 1;
            }
            let got = PrescribedPartition::new(lin.clone()).partition(&m, nb_parts);
            let want = reference_default_fill(&m, 2, &lin);
            match (got, want) {
                (Ok(p), Ok(w)) => {
                    accepted += 1;
                    for (ty, parts, csr) in &w {
                        assert_eq!(
                            p.partition(*ty).unwrap(),
                            parts.as_slice(),
                            "case {case} {ty}"
                        );
                        assert_eq!(p.ghost_partition(*ty).unwrap(), csr, "case {case} {ty}");
                    }
                    let types = p.partitions.as_map().iter(NG).count();
                    assert_eq!(types, w.len(), "case {case}");
                }
                (Err(e), Err(r)) => {
                    refused += 1;
                    let (e, r) = (e.to_string(), r.to_string());
                    if r.contains("several owners") {
                        post_check_refusals += 1;
                    }
                    assert!(e.contains("UD-231"), "case {case}: {e}");
                    // the same orphan refused, or the same post-check node
                    let r = r.trim_start_matches("mesh error: ");
                    assert!(e.contains(r), "case {case}: {e} vs {r}");
                }
                (g, w) => panic!("case {case}: lazy {g:?} vs eager {w:?}"),
            }
        }
        // the generator exercises all three paths, and the ownership
        // post-check refuses some cases on its own (non-conforming
        // adjacency): removing it fails this test
        assert!(
            accepted > 50 && refused > 20 && without_orphans > 20 && post_check_refusals > 10,
            "{accepted} {refused} {without_orphans} {post_check_refusals}"
        );
    }

    /// A segment on the right edge
    /// of the last of three quadrangles (parts 0, 1, 2) whose
    /// `element_to_subelement` row names quadrangle 0, which does not
    /// contain it. The segment takes part 0, so node 3 would be held
    /// locally by parts 0 and 2 and as a ghost only by part 1: two owners.
    /// The partition is refused, as by the earlier rule, although the
    /// segment is no orphan (no node is tracked by the orphan rule).
    #[test]
    fn fu04_nonconforming_adjacency_is_refused() {
        let mut m = Mesh::new(2, "nc").unwrap();
        {
            let mut a = MeshAccessor::new(&mut m);
            for j in 0..2_u8 {
                for i in 0..4_u8 {
                    a.push_node(&[f64::from(i), f64::from(j)]).unwrap();
                }
            }
            let mut q = a.connectivity_mut(ElementType::Quadrangle4, NG);
            q.push_tuple(&[0, 1, 5, 4]);
            q.push_tuple(&[1, 2, 6, 5]);
            q.push_tuple(&[2, 3, 7, 6]);
            drop(q);
            a.connectivity_mut(ElementType::Segment2, NG)
                .push_tuple(&[3, 7]);
        }
        crate::utils::fill_element_to_subelements_data(&mut m).unwrap();
        // conforming: accepted, the segment in part 2
        let p = PrescribedPartition::new(vec![0, 1, 2])
            .partition(&m, 3)
            .unwrap();
        assert_eq!(p.partition(ElementType::Segment2).unwrap(), &[2]);
        {
            let lists = m
                .data_mut()
                .element_lists_mut(ELEMENT_TO_SUBELEMENT)
                .unwrap();
            let rows = lists.get_mut(ElementType::Segment2, NG).unwrap();
            rows.as_mut_slice()[0] = vec![Element::new(ElementType::Quadrangle4, 0, NG)];
        }
        assert!(OrphanHolders::new(&m, 2).unwrap().is_none());
        let e = PrescribedPartition::new(vec![0, 1, 2])
            .partition(&m, 3)
            .unwrap_err()
            .to_string();
        assert!(
            e.contains(
                "node 3 would have several owners (held locally by parts [0, 2], as a ghost by [1]"
            ),
            "{e}"
        );
        // the eager rule (every node checked) refuses identically
        let r = reference_default_fill(&m, 2, &[0, 1, 2])
            .unwrap_err()
            .to_string();
        assert_eq!(e, r);
        // upstream compat: no check, accepted
        let compat = PartitionCompat {
            orphan_lower_dimensional_in_part_zero: true,
        };
        assert!(
            PrescribedPartition::new(vec![0, 1, 2])
                .with_compat(compat)
                .partition(&m, 3)
                .is_ok()
        );
    }

    #[test]
    fn rcb_splits_into_balanced_parts() {
        let m = quad_mesh();
        let p = RcbPartitioner.partition(&m, 2).unwrap();
        let l = p.linearized();
        assert_eq!(l.iter().filter(|&&x| x == 0).count(), 2);
    }
}
