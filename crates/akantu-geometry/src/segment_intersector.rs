// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/geometry/mesh_abstract_intersector.hh,
// mesh_abstract_intersector_tmpl.hh, mesh_geom_intersector.hh,
// mesh_geom_intersector_tmpl.hh, mesh_segment_intersector.hh,
// mesh_segment_intersector_tmpl.hh.

//! Segment–mesh intersection (GEOM-005, GEOM-006, GEOM-007): upstream
//! `MeshSegmentIntersector<dim, type>`, used by the embedded-reinforcement
//! model to cut reinforcement segments by a background mesh of triangles
//! (2-D) or tetrahedra (3-D) into `_segment_2` elements of a result mesh.
//!
//! For each query segment:
//! 1. `all_intersections` over the triangle primitives (the search structure
//!    is the type parameter `I`, see [`crate::index`]);
//! 2. `computeSegments`: segment results are collected; with no result at
//!    all, a query lying strictly inside one primitive is detected with two
//!    opposite rays; for tetrahedra, point results are completed into a
//!    segment through the element (tetrahedron branch);
//! 3. every non-degenerate collected segment becomes a `_segment_2` with two
//!    new nodes (no node sharing), and — if the result mesh carries them —
//!    `associated_element` / `physical_names` data.
//!
//! Upstream defects and their handling (D-003):
//! * UD-072: upstream throws when the result mesh has no `_segment_2`
//!   connectivity yet; the port creates it (no switch).
//! * UD-407: tetrahedra become one flat triangle upstream;
//!   [`SegmentIntersectorOptions::tetrahedra`].
//! * UD-408: upstream collects segments in a `std::set` with the invalid
//!   comparator `segmentPairsLess`, so duplicates are not reliably removed
//!   and the element order depends on the AABB traversal order and the
//!   red–black tree shape. Default: sort by `(min, max)` (exact
//!   lexicographic), remove exact duplicates keeping the smallest element
//!   index (then the orientation starting at `min`) — independent of the
//!   search structure. [`SegmentOrder::Upstream`] reproduces upstream
//!   bit for bit when combined with the CGAL-compatible AABB tree of
//!   `akantu-geometry-cgal`.
//! * UD-599 / UD-601: the face construction (upstream's, in
//!   `Simple_cartesian<double>` like CGAL) reports a query lying along a
//!   shared edge or face once per element, with end points that may differ
//!   in the last bits, and for a query through a vertex at a grazing angle
//!   it can keep near-duplicates whose end points differ far beyond any
//!   point tolerance or lose a whole piece.
//!   [`PieceAssembly::Tiled`] assembles the pieces of a query by parameter
//!   interval instead ([`crate::tiling`]): they cover the part of the query
//!   inside the mesh exactly once, gaps inside the mesh are rebuilt.
//!   Default [`PieceAssembly::Collected`] (upstream: the collected set as
//!   is); the embedded model selects `Tiled` by default.

use std::collections::BTreeSet;

use akantu_core::math::DEFAULT_TOLERANCE;
use akantu_core::{Element, ElementType, Error, GhostType, Idx, Int, Real, Result, UInt, try_idx};
use akantu_mesh::{Mesh, MeshAccessor};

use crate::cxx::RbSet;
use crate::factory::{MeshGeomFactory, TetrahedronPrimitives, primitive_element};
use crate::helpers::{compare_points, segment_pair_order, segment_pairs_less};
use crate::index::{LinearScan, TriangleIndex};
use crate::intersections::TriangleSegmentIntersection;
use crate::kernel::{Point3, Ray3, Segment3, Sign, Tetrahedron3, compare_xyz};
use crate::tiling::{Simplex, TilingReport, tile};

/// Ordering / duplicate handling of the segments of one query (UD-408).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SegmentOrder {
    /// Corrected (default): exact lexicographic order on `(min(), max())`;
    /// exact duplicates (either orientation) keep the smallest element
    /// index, then the orientation whose source is `min()`.
    #[default]
    Lexicographic,
    /// Upstream: libstdc++ `std::set` with `segmentPairsLess` (clean-room
    /// model [`crate::cxx::RbSet`]); depends on the reporting order of the
    /// search structure.
    Upstream,
}

/// How the pieces of one query are assembled (UD-599, UD-601).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PieceAssembly {
    /// Upstream: every segment of the collected set (exact duplicates
    /// removed per [`SegmentOrder`]), degenerate ones included.
    #[default]
    Collected,
    /// Corrected: the pieces tile the part of the query inside the mesh by
    /// parameter interval (duplicates and near-duplicates dropped, overlaps
    /// clipped, slivers dropped, gaps inside the mesh rebuilt by clipping the
    /// query against the elements; [`crate::tiling`]). Needs
    /// [`TetrahedronPrimitives::Faces`] for `_tetrahedron_4`.
    Tiled,
}

/// Options of [`MeshSegmentIntersector`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SegmentIntersectorOptions {
    /// Segment ordering (UD-408). With [`PieceAssembly::Tiled`] it orders
    /// the tiled pieces (`Upstream`: inserted into upstream's set in
    /// parameter order).
    pub order: SegmentOrder,
    /// Tetrahedron primitives (UD-407).
    pub tetrahedra: TetrahedronPrimitives,
    /// Upstream's global `Math::tolerance`, used by `comparePoints` in the
    /// tetrahedron branch (default `1e2·ε`), and by the tiled assembly to
    /// decide when two positions are the same point.
    pub point_tolerance: Real,
    /// Assembly of the pieces of one query (UD-599, UD-601).
    pub assembly: PieceAssembly,
}

impl Default for SegmentIntersectorOptions {
    fn default() -> Self {
        Self {
            order: SegmentOrder::Lexicographic,
            tetrahedra: TetrahedronPrimitives::Faces,
            point_tolerance: DEFAULT_TOLERANCE,
            assembly: PieceAssembly::Collected,
        }
    }
}

impl SegmentIntersectorOptions {
    /// Every upstream behaviour (UD-407, UD-408 and UD-599/UD-601
    /// compatibility).
    #[must_use]
    pub fn upstream() -> Self {
        Self {
            order: SegmentOrder::Upstream,
            tetrahedra: TetrahedronPrimitives::UpstreamTriangle,
            point_tolerance: DEFAULT_TOLERANCE,
            assembly: PieceAssembly::Collected,
        }
    }
}

/// The segment collection of one query (`std::set<pair_type,
/// segmentPairsLess>` upstream).
enum SegmentSet {
    Upstream(RbSet<(Segment3, Int)>),
    Lexicographic(Vec<(Segment3, Int)>),
}

impl SegmentSet {
    fn new(order: SegmentOrder) -> Self {
        match order {
            SegmentOrder::Upstream => Self::Upstream(RbSet::new()),
            SegmentOrder::Lexicographic => Self::Lexicographic(Vec::new()),
        }
    }

    fn insert(&mut self, segment: Segment3, element: Int) {
        match self {
            Self::Upstream(s) => {
                s.insert((segment, element), segment_pairs_less);
            }
            Self::Lexicographic(v) => {
                match v.binary_search_by(|(s, _)| segment_pair_order(s, &segment)) {
                    Ok(i) => {
                        let rank = |s: &Segment3, e: Int| {
                            (e, compare_xyz(s.source, s.min()) != Sign::Zero)
                        };
                        if rank(&segment, element) < rank(&v[i].0, v[i].1) {
                            v[i] = (segment, element);
                        }
                    }
                    Err(i) => v.insert(i, (segment, element)),
                }
            }
        }
    }

    fn into_vec(self) -> Vec<(Segment3, Int)> {
        match self {
            Self::Upstream(s) => s.into_in_order(),
            Self::Lexicographic(v) => v,
        }
    }
}

/// Upstream `MeshSegmentIntersector<dim, type>` over the search structure
/// `I` (default: the LGPL [`LinearScan`]).
#[derive(Debug)]
pub struct MeshSegmentIntersector<I: TriangleIndex = LinearScan> {
    factory: MeshGeomFactory,
    index: Option<I>,
    options: SegmentIntersectorOptions,
    physical_name: String,
    /// Spatial dimension and element count (of the factory's type and the
    /// ghost type) of the mesh given to `construct_data`.
    fingerprint: Option<(usize, GhostType, usize)>,
    /// The elements as simplices ([`PieceAssembly::Tiled`] only).
    simplices: Vec<Simplex>,
}

/// Convert a primitive id (`UInt`) to the `Int` stored in the pairs.
fn id_to_int(id: UInt) -> Result<Int> {
    Int::try_from(id).map_err(|_| Error::InvalidArgument("primitive id exceeds Int".into()))
}

impl<I: TriangleIndex> MeshSegmentIntersector<I> {
    /// `MeshSegmentIntersector(mesh, result_mesh)`: builds the primitive
    /// structure of `ty` elements of `mesh` (the constructor calls
    /// `constructData()`).
    ///
    /// # Errors
    /// Unsupported element type, mesh/type dimension mismatch.
    pub fn new(mesh: &Mesh, ty: ElementType, options: SegmentIntersectorOptions) -> Result<Self> {
        let mut s = Self {
            factory: MeshGeomFactory::new(ty, options.tetrahedra)?,
            index: None,
            options,
            physical_name: String::new(),
            fingerprint: None,
            simplices: Vec::new(),
        };
        s.construct_data(mesh, GhostType::NotGhost)?;
        Ok(s)
    }

    /// `constructData(ghost_type)`: rebuild primitives and search structure.
    ///
    /// # Errors
    /// See [`MeshGeomFactory::construct_data`].
    /// [`PieceAssembly::Tiled`] with [`TetrahedronPrimitives::UpstreamTriangle`]
    /// tetrahedra (upstream's flat triangles are not the elements).
    pub fn construct_data(&mut self, mesh: &Mesh, ghost: GhostType) -> Result<()> {
        let ty = self.factory.element_type();
        if self.options.assembly == PieceAssembly::Tiled
            && ty == ElementType::Tetrahedron4
            && self.factory.tetrahedra() == TetrahedronPrimitives::UpstreamTriangle
        {
            return Err(Error::InvalidArgument(
                "MeshSegmentIntersector: PieceAssembly::Tiled needs TetrahedronPrimitives::Faces \
                 (UD-407 corrected) for _tetrahedron_4"
                    .into(),
            ));
        }
        self.factory.construct_data(mesh, ghost)?;
        self.index = Some(I::build(self.factory.primitive_list())?);
        self.simplices.clear();
        if self.options.assembly == PieceAssembly::Tiled {
            let nodes = mesh.nodes();
            let n = mesh.nb_element(ty, ghost);
            self.simplices.reserve(n);
            for e in 0..n {
                let el = Element::try_new(ty, e, ghost)?;
                let c = mesh.extract_nodal_values_from_element(&nodes, el)?;
                let dim = ty.spatial_dimension();
                let p = |k: usize| {
                    Point3::new(
                        c[k * dim],
                        c[k * dim + 1],
                        if dim == 3 { c[k * dim + 2] } else { 0.0 },
                    )
                };
                self.simplices.push(if ty == ElementType::Tetrahedron4 {
                    Simplex::tetrahedron(p(0), p(1), p(2), p(3))
                } else {
                    Simplex::triangle(p(0), p(1), p(2))
                });
            }
        }
        self.fingerprint = Some((
            mesh.spatial_dimension(),
            ghost,
            mesh.nb_element(self.factory.element_type(), ghost),
        ));
        Ok(())
    }

    /// Refuse a mesh that cannot be the one given to `construct_data`
    /// (different dimension or element count).
    fn check_mesh(&self, mesh: &Mesh) -> Result<()> {
        if let Some((dim, ghost, nb)) = self.fingerprint {
            let got = (
                mesh.spatial_dimension(),
                mesh.nb_element(self.factory.element_type(), ghost),
            );
            if got != (dim, nb) {
                return Err(Error::InvalidArgument(format!(
                    "MeshSegmentIntersector: mesh (dimension {}, {} elements) is not the mesh \
                     given to construct_data (dimension {dim}, {nb} elements)",
                    got.0, got.1
                )));
            }
        }
        Ok(())
    }

    /// `setPhysicalName(name)`: name stored in `physical_names` for the
    /// segments created by the following queries.
    pub fn set_physical_name(&mut self, name: impl Into<String>) {
        self.physical_name = name.into();
    }

    /// The options.
    #[must_use]
    pub fn options(&self) -> &SegmentIntersectorOptions {
        &self.options
    }

    /// The primitive factory.
    #[must_use]
    pub fn factory(&self) -> &MeshGeomFactory {
        &self.factory
    }

    fn index(&self) -> Result<&I> {
        self.index.as_ref().ok_or_else(|| {
            Error::InvalidArgument("MeshSegmentIntersector: constructData not called".into())
        })
    }

    /// The segments `computeIntersectionQuery` would create for `query`, in
    /// creation order, with their element indices (`computeSegments` and
    /// the set iteration; degenerate segments included).
    ///
    /// Contract: `mesh` must be the mesh given to
    /// [`construct_data`](Self::construct_data) (upstream holds a reference
    /// to it). Primitive ids refer to that mesh; the tetrahedron branch also
    /// reads `mesh`'s connectivity and nodes, so another mesh would mix two
    /// geometries. A mesh whose dimension or element count differs is
    /// refused with an error; a different mesh of the same size is not
    /// detected.
    ///
    /// With [`PieceAssembly::Tiled`] the pieces are the tiled ones
    /// ([`Self::compute_segments_with_report`]).
    ///
    /// # Errors
    /// A `CGAL_error()` in an intersection construction, invalid mesh;
    /// tiled: non-finite coordinates.
    pub fn compute_segments(&self, mesh: &Mesh, query: &Segment3) -> Result<Vec<(Segment3, Int)>> {
        Ok(self.compute_segments_with_report(mesh, query)?.0)
    }

    /// [`Self::compute_segments`] with the counters of the tiled assembly
    /// (all zero but `candidates`/`kept` with [`PieceAssembly::Collected`]).
    ///
    /// # Errors
    /// See [`Self::compute_segments`].
    pub fn compute_segments_with_report(
        &self,
        mesh: &Mesh,
        query: &Segment3,
    ) -> Result<(Vec<(Segment3, Int)>, TilingReport)> {
        let collected = self.collect_segments(mesh, query)?.into_vec();
        match self.options.assembly {
            PieceAssembly::Collected => {
                let n = collected.len();
                Ok((
                    collected,
                    TilingReport {
                        candidates: n,
                        kept: n,
                        ..TilingReport::default()
                    },
                ))
            }
            PieceAssembly::Tiled => {
                let (pieces, report) = tile(
                    query,
                    &collected,
                    &self.simplices,
                    self.options.point_tolerance,
                )?;
                // output order of `order` (pieces are disjoint: no merging)
                let mut set = SegmentSet::new(self.options.order);
                for (segment, element) in pieces {
                    set.insert(segment, element);
                }
                Ok((set.into_vec(), report))
            }
        }
    }

    /// `computeSegments`: the collected segment set of `query`.
    fn collect_segments(&self, mesh: &Mesh, query: &Segment3) -> Result<SegmentSet> {
        self.check_mesh(mesh)?;
        let index = self.index()?;
        let mut intersections = Vec::new();
        index.all_intersections(query, &mut intersections)?;
        let mut set = SegmentSet::new(self.options.order);
        if intersections.is_empty() {
            // Query completely outside the mesh or inside one primitive:
            // a primitive hit by both opposite rays contains the query.
            let mut r1 = Vec::new();
            let mut r2 = Vec::new();
            index.all_intersected_primitives(&Ray3::new(query.source, query.target), &mut r1);
            index.all_intersected_primitives(&Ray3::new(query.target, query.source), &mut r2);
            let r1: BTreeSet<Int> = r1.into_iter().map(id_to_int).collect::<Result<_>>()?;
            let r2: BTreeSet<Int> = r2.into_iter().map(id_to_int).collect::<Result<_>>()?;
            if let Some(&id) = r2.iter().find(|id| r1.contains(id)) {
                set.insert(*query, id);
            }
            return Ok(set);
        }
        let ty = self.factory.element_type();
        for (intersection, el) in intersections {
            let el_int = id_to_int(el)?;
            match intersection {
                TriangleSegmentIntersection::Segment(s) => set.insert(s, el_int),
                TriangleSegmentIntersection::Point(point) => {
                    if ty == ElementType::Tetrahedron4 {
                        self.tetrahedron_branch(mesh, query, point, el, &mut set)?;
                    }
                }
            }
        }
        Ok(set)
    }

    /// The `dim == 3 and type == _tetrahedron_4` branch of `computeSegments`
    /// for a point result `point` on element `el`.
    fn tetrahedron_branch(
        &self,
        mesh: &Mesh,
        query: &Segment3,
        point: Point3,
        el: UInt,
        set: &mut SegmentSet,
    ) -> Result<()> {
        let el_int = id_to_int(el)?;
        let (coords, facets) = self
            .factory
            .element_primitives(mesh, primitive_element(el)?)?;
        let local = I::build(&facets)?;
        let mut locals = Vec::new();
        local.all_intersections(query, &mut locals)?;
        let mut out_point_found = false;
        for (li, _) in locals {
            if let TriangleSegmentIntersection::Point(lp) = li
                && !compare_points(point, lp, self.options.point_tolerance)
            {
                set.insert(Segment3::new(point, lp), el_int);
                out_point_found = true;
            }
        }
        if !out_point_found {
            let p = |n: usize| Point3::new(coords[3 * n], coords[3 * n + 1], coords[3 * n + 2]);
            let tetra = Tetrahedron3::new(p(0), p(1), p(2), p(3));
            let strictly_inside =
                |x: Point3| tetra.has_on_bounded_side(x) && !tetra.has_on_boundary(x);
            let inside = if strictly_inside(query.source) {
                Some(query.source)
            } else if strictly_inside(query.target) {
                Some(query.target)
            } else {
                None
            };
            if let Some(ip) = inside {
                set.insert(Segment3::new(ip, point), el_int);
            }
        }
        Ok(())
    }

    /// `computeIntersectionQuery(query)`: append the `_segment_2` elements
    /// of `query` to `result_mesh` (see the module documentation).
    ///
    /// # Errors
    /// Result mesh dimension differs from the element type's, invalid mesh,
    /// or a `CGAL_error()`.
    pub fn compute_intersection_query(
        &self,
        mesh: &Mesh,
        query: &Segment3,
        result_mesh: &mut Mesh,
    ) -> Result<()> {
        let segments = self.compute_segments(mesh, query)?;
        let ty = self.factory.element_type();
        let dim = ty.spatial_dimension();
        if result_mesh.spatial_dimension() != dim {
            return Err(Error::DimensionMismatch {
                context: "MeshSegmentIntersector: result mesh dimension",
                expected: dim,
                got: result_mesh.spatial_dimension(),
            });
        }
        let valid_data = {
            let data = result_mesh.data();
            data.element_map("associated_element")
                .is_ok_and(|m| m.exists(ElementType::Segment2, GhostType::NotGhost))
                && data
                    .strings("physical_names")
                    .is_ok_and(|m| m.exists(ElementType::Segment2, GhostType::NotGhost))
        };
        let mut acc = MeshAccessor::new(result_mesh);
        // UD-072: upstream calls getConnectivity(_segment_2) before
        // addConnectivityType and throws on a fresh result mesh.
        acc.connectivity_mut(ElementType::Segment2, GhostType::NotGhost);
        acc.connectivity_mut(ElementType::Segment2, GhostType::Ghost);
        for (segment, element) in segments {
            if segment.is_degenerate() {
                continue;
            }
            let n: Idx = try_idx(acc.mesh_mut().nb_nodes())?;
            acc.connectivity_mut(ElementType::Segment2, GhostType::NotGhost)
                .try_push_tuple(&[n, n + 1])?;
            acc.push_node(&segment.source.to_array()[..dim])?;
            acc.push_node(&segment.target.to_array()[..dim])?;
            if valid_data {
                let data = acc.mesh_mut().data_mut();
                data.element_map_mut("associated_element")?
                    .get_mut(ElementType::Segment2, GhostType::NotGhost)?
                    .try_push_tuple(&[Element::new(ty, element, GhostType::NotGhost)])?;
                data.strings_mut("physical_names")?
                    .get_mut(ElementType::Segment2, GhostType::NotGhost)?
                    .try_push_tuple(std::slice::from_ref(&self.physical_name))?;
            }
        }
        Ok(())
    }

    /// `computeIntersectionQueryList(query_list)`.
    ///
    /// # Errors
    /// See [`Self::compute_intersection_query`].
    pub fn compute_intersection_query_list(
        &self,
        mesh: &Mesh,
        queries: &[Segment3],
        result_mesh: &mut Mesh,
    ) -> Result<()> {
        for q in queries {
            self.compute_intersection_query(mesh, q, result_mesh)?;
        }
        Ok(())
    }

    /// `buildResultFromQueryList(query_list)` (= the query list).
    ///
    /// # Errors
    /// See [`Self::compute_intersection_query`].
    pub fn build_result_from_query_list(
        &self,
        mesh: &Mesh,
        queries: &[Segment3],
        result_mesh: &mut Mesh,
    ) -> Result<()> {
        self.compute_intersection_query_list(mesh, queries, result_mesh)
    }

    /// `computeMeshQueryIntersectionPoint`: not implemented upstream for
    /// segments (`AKANTU_ERROR`).
    ///
    /// # Errors
    /// Always.
    pub fn compute_mesh_query_intersection_point(
        &self,
        _query: &Segment3,
        _nb_old_nodes: Int,
    ) -> Result<()> {
        Err(Error::NotImplemented(
            "MeshSegmentIntersector::computeMeshQueryIntersectionPoint (not implemented upstream)",
        ))
    }
}
