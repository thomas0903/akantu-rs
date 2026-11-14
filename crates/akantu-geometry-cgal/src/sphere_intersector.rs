// SPDX-License-Identifier: GPL-3.0-or-later
// akantu-rs — GPL-3.0-or-later (D-042). Ported from Akantu (EPFL/LSMS),
// LGPL-3.0-or-later code conveyed here under GPL-3.0-or-later:
//   src/geometry/mesh_sphere_intersector.hh, mesh_sphere_intersector_tmpl.hh
//   (v5 code at 35a8517; the v4.0 code, git a23d4b769, documents the
//   intended behaviour of UD-410/411/412), the Spherical case of
//   mesh_geom_factory_tmpl.hh (AddPrimitiveHelper<_triangle, Spherical>),
//   aabb_primitives/line_arc.hh, mesh_abstract_intersector*.hh.
// It is built on the GPL-derived spherical kernel of this crate
// (crate::spherical: CGAL Circular_kernel_3 / Algebraic_kernel_for_spheres).

//! Sphere–mesh intersection for IGFEM (GEOM-008): upstream
//! `MeshSphereIntersector<dim, type>::computeMeshQueryIntersectionPoint`.
//!
//! Every triangle edge (line arc, element order then local edge
//! `(a,b), (b,c), (c,a)`) is intersected with the sphere in the exact
//! spherical kernel; an edge contributes when the result is exactly one
//! point. The point is converted to doubles; it is *on the mesh* when it is
//! close to an end point of the edge (no new node), else it reuses a close
//! node created after `nb_old_nodes` or a close pending intersection point,
//! or becomes a new pending intersection point. `new_node_per_elem` stores,
//! per element, the number of new nodes and the pairs (node id, edge id).
//!
//! Upstream defects (D-003; registered in `docs/UPSTREAM_DEFECTS.md`):
//! * UD-409 (A): the v5 class does not compile (unique_ptr / const member /
//!   type errors); the port implements its evident intent.
//! * UD-410 (B): v5 numbers a reused or new pending point as
//!   `nodes.size()` (the first pending id) instead of
//!   `nodes.size() + index`; default continues the count (v4),
//!   [`NodeNumbering::UpstreamV5`] reproduces v5.
//! * UD-411 (B): v5 compares points with Eigen `isApprox` (relative,
//!   precision `1e-12`) and ignores `tol_intersection_on_node`, whose setter
//!   even takes a `UInt`; default uses `Math::are_vector_equal` with
//!   `tol_intersection_on_node` (`1e-10`, settable as a real, v4);
//!   [`ProximityTest::UpstreamEigenIsApprox`] reproduces v5.
//! * UD-412 (B): v5's test `not pair and pair->second != 1` never rejects
//!   tangent points; default rejects them (v4, intent),
//!   [`TangentPolicy::UpstreamAccept`] reproduces v5.
//! * UD-413 (A): more than two new nodes in one element overflows
//!   `new_node_per_elem` (checked only by a debug assertion upstream; it
//!   throws in the oracle): always an error here.
//!
//! Upstream also changes the global `Math::tolerance` during the call; the
//! port passes the tolerance explicitly and has no global state.

use akantu_core::eigen_order::{redux_seq, redux_tree};
use akantu_core::math::are_float_equal;
use akantu_core::{Array, Element, ElementType, Error, GhostType, Int, Real, Result, try_to_usize};
use akantu_mesh::Mesh;

use crate::spherical::{LineArc, SkPoint, SkSphere};

/// Treatment of tangent intersection points (UD-412).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TangentPolicy {
    /// Only multiplicity-1 points create nodes (v4, intent; default).
    #[default]
    Reject,
    /// v5 behaviour: tangent points are accepted too.
    UpstreamAccept,
}

/// Proximity test between intersection points and nodes (UD-411).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ProximityTest {
    /// `Math::are_vector_equal(dim, a, b)` with `Math::tolerance =
    /// tol_intersection_on_node` (v4; default, tolerance `1e-10`).
    MathTolerance(Real),
    /// v5: Eigen `isApprox` with the default precision `1e-12` (and Eigen's
    /// summation orders for fixed/run-time sized vectors).
    UpstreamEigenIsApprox,
}

impl Default for ProximityTest {
    fn default() -> Self {
        Self::MathTolerance(1e-10)
    }
}

/// Node ids stored in `new_node_per_elem` for pending points (UD-410).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum NodeNumbering {
    /// `nodes.size() + index in intersection_points` (v4; default).
    #[default]
    Continued,
    /// v5: always `nodes.size()` for pending points.
    UpstreamV5,
}

/// Options of [`MeshSphereIntersector`].
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SphereIntersectorOptions {
    /// UD-412.
    pub tangent: TangentPolicy,
    /// UD-411.
    pub proximity: ProximityTest,
    /// UD-410.
    pub numbering: NodeNumbering,
}

impl SphereIntersectorOptions {
    /// Every v5 behaviour (bit-identical to the compile-fixed upstream
    /// class, oracle patch `oracle/patches/geometry/`).
    #[must_use]
    pub fn upstream() -> Self {
        Self {
            tangent: TangentPolicy::UpstreamAccept,
            proximity: ProximityTest::UpstreamEigenIsApprox,
            numbering: NodeNumbering::UpstreamV5,
        }
    }
}

/// Akantu `Line_arc<cgal::Spherical>` with its element and edge ids.
#[derive(Clone, Debug)]
pub struct LineArcPrimitive {
    /// The arc.
    pub arc: LineArc,
    /// Element index (`id()`).
    pub id: u32,
    /// Local edge index 0, 1, 2 (`segId()`).
    pub seg_id: u32,
}

/// Element types upstream's sphere intersector accepts.
#[must_use]
pub fn is_supported_type(ty: ElementType) -> bool {
    matches!(
        ty,
        ElementType::Triangle3 | ElementType::IgfemTriangle4 | ElementType::IgfemTriangle5
    )
}

/// `MeshGeomFactory<dim, type, Line_arc<SK>, SK>::constructData`: the three
/// edges `(a,b)`, `(b,c)`, `(c,a)` of every element of `(ty, ghost)` as
/// line arcs through `(x, y, 0)` points.
///
/// # Errors
/// Unsupported type, invalid mesh, degenerate edge.
pub fn line_arc_primitives(
    mesh: &Mesh,
    ty: ElementType,
    ghost: GhostType,
) -> Result<Vec<LineArcPrimitive>> {
    if !matches!(
        ty,
        ElementType::Triangle3
            | ElementType::Triangle6
            | ElementType::IgfemTriangle4
            | ElementType::IgfemTriangle5
    ) {
        return Err(Error::UnsupportedElementType {
            ty,
            context: "MeshGeomFactory (spherical line-arc primitives)",
        });
    }
    let dim = mesh.spatial_dimension();
    if dim < 2 {
        return Err(Error::DimensionMismatch {
            context: "MeshGeomFactory (spherical): mesh dimension",
            expected: 2,
            got: dim,
        });
    }
    let n = mesh.nb_element(ty, ghost);
    let mut out = Vec::with_capacity(3 * n);
    let nodes = mesh.nodes();
    for e in 0..n {
        let coords =
            mesh.extract_nodal_values_from_element(&nodes, Element::try_new(ty, e, ghost)?)?;
        let p = |k: usize| SkPoint::try_from_f64([coords[k * dim], coords[k * dim + 1], 0.0]);
        let (a, b, c) = (p(0)?, p(1)?, p(2)?);
        let id = u32::try_from(e)
            .map_err(|_| Error::InvalidArgument("element index exceeds UInt".into()))?;
        for (seg_id, (s, t)) in [(&a, &b), (&b, &c), (&c, &a)].into_iter().enumerate() {
            out.push(LineArcPrimitive {
                arc: LineArc::from_points(s, t)?,
                id,
                seg_id: u32::try_from(seg_id).unwrap_or(0),
            });
        }
    }
    Ok(out)
}

/// Upstream `MeshSphereIntersector<dim, type>` (see the module docs).
#[derive(Debug)]
pub struct MeshSphereIntersector {
    ty: ElementType,
    dim: usize,
    options: SphereIntersectorOptions,
    primitives: Vec<LineArcPrimitive>,
    /// `nb_element × (1 + 4·(dim − 1))`.
    new_node_per_elem: Array<Int>,
    /// `n × dim`.
    intersection_points: Array<Real>,
}

/// Which side of an Eigen expression fixes the compile-time size
/// (`isApprox(x, y)`: the difference takes `x`'s).
#[derive(Clone, Copy)]
enum Sized {
    /// `Vector<Real>` (run-time size): sequential sums.
    Dynamic,
    /// `Vector<Real, dim>` / `make_view<dim>` maps: halving-tree sums.
    Fixed,
}

fn eigen_sum(len: usize, kind: Sized, t: &impl Fn(usize) -> f64) -> f64 {
    if len == 0 {
        return 0.0;
    }
    match kind {
        Sized::Dynamic => redux_seq(len, t, |a, b| a + b),
        Sized::Fixed => redux_tree(0, len, t, |a, b| a + b),
    }
}

/// Eigen `x.isApprox(y)` with `prec = 1e-12` and the summation orders of
/// the operand kinds.
fn eigen_is_approx(x: &[Real], xk: Sized, y: &[Real], yk: Sized) -> bool {
    let n = x.len();
    let d = eigen_sum(n, xk, &|i| (x[i] - y[i]) * (x[i] - y[i]));
    let nx = eigen_sum(n, xk, &|i| x[i] * x[i]);
    let ny = eigen_sum(n, yk, &|i| y[i] * y[i]);
    let m = if ny < nx { ny } else { nx };
    let prec = akantu_geometry::intersection::EIGEN_DUMMY_PRECISION;
    d <= prec * prec * m
}

impl MeshSphereIntersector {
    /// `MeshSphereIntersector(mesh)` for elements of type `ty` (the
    /// constructor does not call `constructData`).
    ///
    /// # Errors
    /// Unsupported type (`AKANTU_ERROR("Not ready for mesh type …")`).
    pub fn new(mesh: &Mesh, ty: ElementType, options: SphereIntersectorOptions) -> Result<Self> {
        if !is_supported_type(ty) {
            return Err(Error::UnsupportedElementType {
                ty,
                context: "MeshSphereIntersector (Not ready for mesh type)",
            });
        }
        let dim = mesh.spatial_dimension();
        if !(2..=3).contains(&dim) {
            return Err(Error::DimensionMismatch {
                context: "MeshSphereIntersector: spatial dimension",
                expected: 2,
                got: dim,
            });
        }
        Ok(Self {
            ty,
            dim,
            options,
            primitives: Vec::new(),
            new_node_per_elem: Array::new(1 + 4 * (dim - 1)),
            intersection_points: Array::new(dim),
        })
    }

    /// `setToleranceIntersectionOnNode(tol)` (a real here, UD-411): the
    /// tolerance of the default proximity test.
    pub fn set_tolerance_intersection_on_node(&mut self, tol: Real) {
        if let ProximityTest::MathTolerance(_) = self.options.proximity {
            self.options.proximity = ProximityTest::MathTolerance(tol);
        }
    }

    /// The options.
    #[must_use]
    pub fn options(&self) -> &SphereIntersectorOptions {
        &self.options
    }

    /// `getNbSegByEl()` (3 for triangles).
    #[must_use]
    pub fn nb_seg_by_el(&self) -> usize {
        3
    }

    /// `getNewNodePerElem()`: column 0 = number of new nodes, then pairs
    /// (node id, edge id).
    #[must_use]
    pub fn new_node_per_elem(&self) -> &Array<Int> {
        &self.new_node_per_elem
    }

    /// `getIntersectionPoints()` (pending new nodes, `dim` components).
    #[must_use]
    pub fn intersection_points(&self) -> &Array<Real> {
        &self.intersection_points
    }

    /// The line-arc primitives (`factory.getPrimitiveList()`).
    #[must_use]
    pub fn primitive_list(&self) -> &[LineArcPrimitive] {
        &self.primitives
    }

    /// `constructData(ghost_type)`: zero `new_node_per_elem` (one row per
    /// element), clear the intersection points, rebuild the primitives.
    ///
    /// # Errors
    /// Invalid mesh or degenerate edge.
    pub fn construct_data(&mut self, mesh: &Mesh, ghost: GhostType) -> Result<()> {
        let n = mesh.nb_element(self.ty, ghost);
        self.new_node_per_elem.try_resize(n, 0)?;
        self.new_node_per_elem.fill(0);
        self.intersection_points.truncate(0);
        self.primitives = line_arc_primitives(mesh, self.ty, ghost)?;
        Ok(())
    }

    fn approx(&self, x: &[Real], xk: Sized, y: &[Real], yk: Sized) -> bool {
        match self.options.proximity {
            ProximityTest::MathTolerance(tol) => {
                x.iter().zip(y).all(|(&a, &b)| are_float_equal(a, b, tol))
            }
            ProximityTest::UpstreamEigenIsApprox => eigen_is_approx(x, xk, y, yk),
        }
    }

    /// `computeMeshQueryIntersectionPoint(query, nb_old_nodes)` (see the
    /// module docs). `mesh` provides the current nodes (only nodes from
    /// `nb_old_nodes` on are candidates for reuse).
    ///
    /// # Errors
    /// More than two new nodes in one element (UD-413), invalid input.
    pub fn compute_mesh_query_intersection_point(
        &mut self,
        mesh: &Mesh,
        query: &SkSphere,
        nb_old_nodes: Int,
    ) -> Result<()> {
        let dim = self.dim;
        let nodes = mesh.nodes();
        if nodes.nb_component() != dim {
            return Err(Error::DimensionMismatch {
                context: "MeshSphereIntersector: mesh nodes",
                expected: dim,
                got: nodes.nb_component(),
            });
        }
        let nb_nodes = nodes.len();
        let first_candidate = try_to_usize(nb_old_nodes)?;
        let ncomp = self.new_node_per_elem.nb_component();
        for k in 0..self.primitives.len() {
            let results = self.primitives[k].arc.intersect_sphere(query)?;
            if results.len() != 1 {
                continue; // not just one point
            }
            let (point, multiplicity) = &results[0];
            if *multiplicity != 1 && self.options.tangent == TangentPolicy::Reject {
                continue;
            }
            let p3 = point.to_double()?;
            let new_node = &p3[..dim];

            let mut is_on_mesh = false;
            let mut is_new = true;
            // Existing nodes created after nb_old_nodes.
            let mut old_node_id = first_candidate;
            for n in first_candidate..nb_nodes {
                if self.approx(new_node, Sized::Dynamic, nodes.tuple(n), Sized::Fixed) {
                    is_new = false;
                    break;
                }
                old_node_id += 1;
            }
            // Pending intersection points.
            if is_new {
                let mut idx = 0;
                let mut found = false;
                for ip in self.intersection_points.tuples() {
                    if self.approx(new_node, Sized::Dynamic, ip, Sized::Fixed) {
                        found = true;
                        break;
                    }
                    idx += 1;
                }
                if found {
                    is_new = false;
                }
                if self.options.numbering == NodeNumbering::Continued {
                    // v4: the counter runs on through the pending points
                    // (found index, or the index of the point appended below).
                    old_node_id = nb_nodes + idx;
                }
            }
            // Close to an end point of the edge: on the mesh.
            let prim = &self.primitives[k];
            let s3 = prim.arc.source().to_double()?;
            let t3 = prim.arc.target().to_double()?;
            if self.approx(&s3[..dim], Sized::Fixed, new_node, Sized::Dynamic)
                || self.approx(&t3[..dim], Sized::Fixed, new_node, Sized::Dynamic)
            {
                is_on_mesh = true;
                is_new = false;
            }
            if is_new {
                self.intersection_points.try_push_tuple(new_node)?;
            }
            if !is_on_mesh {
                let element = usize::try_from(prim.id)
                    .map_err(|_| Error::InvalidArgument("element id".into()))?;
                if element >= self.new_node_per_elem.len() {
                    return Err(Error::OutOfRange {
                        what: "element (new_node_per_elem)",
                        index: element,
                        len: self.new_node_per_elem.len(),
                    });
                }
                let seg_id = Int::try_from(prim.seg_id).unwrap_or(0);
                let node_id = Int::try_from(old_node_id)
                    .map_err(|_| Error::InvalidArgument("node id exceeds Int".into()))?;
                let row = self.new_node_per_elem.tuple_mut(element);
                row[0] += 1;
                let nb = usize::try_from(row[0]).unwrap_or(usize::MAX);
                if 2 * nb >= ncomp {
                    return Err(Error::CapacityExceeded {
                        what: "new_node_per_elem columns (You might have to interface crossing the same material)",
                        requested: u128::try_from(2 * nb + 1).unwrap_or(u128::MAX),
                        limit: u128::try_from(ncomp).unwrap_or(u128::MAX),
                    });
                }
                row[2 * nb - 1] = node_id;
                row[2 * nb] = seg_id;
            }
        }
        Ok(())
    }

    /// `computeMeshQueryListIntersectionPoint(query_list, nb_old_nodes)`.
    ///
    /// # Errors
    /// See [`Self::compute_mesh_query_intersection_point`].
    pub fn compute_mesh_query_list_intersection_point(
        &mut self,
        mesh: &Mesh,
        queries: &[SkSphere],
        nb_old_nodes: Int,
    ) -> Result<()> {
        for q in queries {
            self.compute_mesh_query_intersection_point(mesh, q, nb_old_nodes)?;
        }
        Ok(())
    }

    /// `computeIntersectionQuery`: removed upstream (`AKANTU_ERROR`).
    ///
    /// # Errors
    /// Always.
    pub fn compute_intersection_query(&self, _query: &SkSphere) -> Result<()> {
        Err(Error::NotImplemented(
            "MeshSphereIntersector::computeIntersectionQuery (replaced by computeMeshQueryIntersectionPoint upstream)",
        ))
    }

    /// `buildResultFromQueryList`: removed upstream (`AKANTU_ERROR`).
    ///
    /// # Errors
    /// Always.
    pub fn build_result_from_query_list(&self, _queries: &[SkSphere]) -> Result<()> {
        Err(Error::NotImplemented(
            "MeshSphereIntersector::buildResultFromQueryList (no longer implemented upstream)",
        ))
    }
}
