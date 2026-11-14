// SPDX-License-Identifier: GPL-3.0-or-later
// akantu-rs — GPL-3.0-or-later (D-042). Ported from Akantu (EPFL/LSMS):
// src/geometry/tree_type_helper.hh (TreeTypeHelper<Triangle<Cartesian>>:
// `CGAL::AABB_tree<CGAL::AABB_traits<Cartesian, Triangle_primitive>>`) and
// the AABB-tree calls of mesh_geom_factory_tmpl.hh /
// mesh_segment_intersector_tmpl.hh.
//
// GPL-DERIVED: this file is a translation of CGAL 5.6.2 package AABB_tree
// (GPL-3.0-or-later OR LicenseRef-Commercial; used here under
// GPL-3.0-or-later):
//   AABB_tree/include/CGAL/AABB_tree.h
//     Copyright (c) 2008,2011 INRIA Sophia-Antipolis (France).
//     Author(s): Camille Wormser, Pierre Alliez, Stephane Tayeb
//   AABB_tree/include/CGAL/AABB_traits.h
//     Copyright (c) 2009 INRIA Sophia-Antipolis (France).
//     Author(s): Stéphane Tayeb, Pierre Alliez, Camille Wormser
//   AABB_tree/include/CGAL/AABB_tree/internal/AABB_node.h
//     Copyright (c) 2008 INRIA Sophia-Antipolis (France).
//     Author(s): Camille Wormser, Pierre Alliez, Stephane Tayeb
//   AABB_tree/include/CGAL/AABB_tree/internal/AABB_traversal_traits.h
//     Copyright (c) 2008-2009 INRIA Sophia-Antipolis (France).
//     Author(s): Camille Wormser, Pierre Alliez, Stephane Tayeb
// The geometric predicates it calls (box/segment, triangle/segment, …) are
// the LGPL ports of akantu-geometry; std::nth_element is the clean-room
// model akantu_geometry::cxx::nth_element (D-030).

//! CGAL `AABB_tree<AABB_traits<Simple_cartesian<double>,
//! Triangle_primitive>>` (GEOM-003): the build (median split along the
//! longest box axis with `std::nth_element`) and the depth-first traversal
//! reproduce CGAL's, so queries report intersections in **upstream's
//! order** — required for bit-identical output of
//! `akantu_geometry::SegmentOrder::Upstream`.

use akantu_core::{Result, UInt};
use akantu_geometry::cxx::nth_element;
use akantu_geometry::factory::TrianglePrimitive;
use akantu_geometry::index::TriangleIndex;
use akantu_geometry::intersections::{
    TriangleSegmentIntersection, bbox_ray_do_intersect, bbox_segment_do_intersect,
    ray_triangle_do_intersect, triangle_segment_intersection,
};
use akantu_geometry::kernel::{Bbox3, Ray3, Segment3};

/// Child of an [`AabbTree`] node (`m_p_left_child` / `m_p_right_child`
/// point either to a node or to a primitive).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Child {
    Node(usize),
    Primitive(usize),
}

/// `AABB_node`.
#[derive(Clone, Copy, Debug)]
struct Node {
    bbox: Bbox3,
    left: Child,
    right: Child,
}

/// CGAL `AABB_traits::Axis`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Axis {
    X,
    Y,
    Z,
}

/// `AABB_traits::longest_axis(bbox)`.
fn longest_axis(b: &Bbox3) -> Axis {
    let dx = b.max[0] - b.min[0];
    let dy = b.max[1] - b.min[1];
    let dz = b.max[2] - b.min[2];
    if dx >= dy {
        if dx >= dz { Axis::X } else { Axis::Z }
    } else if dy >= dz {
        Axis::Y
    } else {
        Axis::Z
    }
}

/// CGAL `AABB_tree` over triangle primitives (see the module docs).
#[derive(Clone, Debug, Default)]
pub struct AabbTree {
    /// `m_primitives` (permuted by the build).
    primitives: Vec<TrianglePrimitive>,
    /// `m_nodes`; node 0 is the root when `primitives.len() > 1`.
    nodes: Vec<Node>,
}

/// What a traversal does at primitives and nodes (CGAL traversal traits).
trait TraversalTraits {
    fn intersection(&mut self, primitive: &TrianglePrimitive) -> Result<()>;
    fn do_intersect(&self, bbox: &Bbox3) -> bool;
}

/// `Listing_intersection_traits` for a `Segment_3` query.
struct ListIntersections<'a> {
    query: &'a Segment3,
    out: &'a mut Vec<(TriangleSegmentIntersection, UInt)>,
}

impl TraversalTraits for ListIntersections<'_> {
    fn intersection(&mut self, p: &TrianglePrimitive) -> Result<()> {
        // `AABB_traits::Intersection`: intersect_3(query, datum) paired
        // with the primitive id.
        if let Some(i) = triangle_segment_intersection(&p.triangle, self.query)? {
            self.out.push((i, p.id));
        }
        Ok(())
    }

    fn do_intersect(&self, bbox: &Bbox3) -> bool {
        bbox_segment_do_intersect(self.query, bbox)
    }
}

/// `Listing_primitive_traits` for a `Ray_3` query.
struct ListPrimitives<'a> {
    query: &'a Ray3,
    out: &'a mut Vec<UInt>,
}

impl TraversalTraits for ListPrimitives<'_> {
    fn intersection(&mut self, p: &TrianglePrimitive) -> Result<()> {
        if ray_triangle_do_intersect(&p.triangle, self.query) {
            self.out.push(p.id);
        }
        Ok(())
    }

    fn do_intersect(&self, bbox: &Bbox3) -> bool {
        bbox_ray_do_intersect(self.query, bbox)
    }
}

impl AabbTree {
    /// `AABB_tree(first, beyond)` followed by `build()`.
    #[must_use]
    pub fn new(primitives: &[TrianglePrimitive]) -> Self {
        let mut t = Self {
            primitives: primitives.to_vec(),
            nodes: Vec::new(),
        };
        t.build();
        t
    }

    /// Number of primitives.
    #[must_use]
    pub fn size(&self) -> usize {
        self.primitives.len()
    }

    /// The primitives in tree order (after the build's permutations).
    #[must_use]
    pub fn primitives(&self) -> &[TrianglePrimitive] {
        &self.primitives
    }

    /// `custom_build(compute_bbox, split_primitives)`.
    fn build(&mut self) {
        self.nodes.clear();
        let n = self.primitives.len();
        if n > 1 {
            self.nodes.reserve(n - 1);
            let root = self.new_node();
            self.expand(root, 0, n, n);
        }
    }

    fn new_node(&mut self) -> usize {
        self.nodes.push(Node {
            bbox: Bbox3 {
                min: [0.0; 3],
                max: [0.0; 3],
            },
            left: Child::Primitive(0),
            right: Child::Primitive(0),
        });
        self.nodes.len() - 1
    }

    /// `Compute_bbox`: bbox of the first primitive, then `+` the others in
    /// order.
    fn compute_bbox(&self, first: usize, beyond: usize) -> Bbox3 {
        let mut b = self.primitives[first].bbox();
        for p in &self.primitives[first + 1..beyond] {
            b = b.union(&p.bbox());
        }
        b
    }

    /// `Split_primitives`: `std::nth_element` at the middle along the
    /// longest axis, comparing reference points with `<`.
    fn split_primitives(&mut self, first: usize, beyond: usize, bbox: &Bbox3) {
        let range = &mut self.primitives[first..beyond];
        let middle = range.len() / 2;
        match longest_axis(bbox) {
            Axis::X => nth_element(range, middle, |a, b| {
                a.reference_point().x < b.reference_point().x
            }),
            Axis::Y => nth_element(range, middle, |a, b| {
                a.reference_point().y < b.reference_point().y
            }),
            Axis::Z => nth_element(range, middle, |a, b| {
                a.reference_point().z < b.reference_point().z
            }),
        }
    }

    /// `AABB_tree::expand(node, first, beyond, range, …)`.
    fn expand(&mut self, node: usize, first: usize, beyond: usize, range: usize) {
        let bbox = self.compute_bbox(first, beyond);
        self.nodes[node].bbox = bbox;
        self.split_primitives(first, beyond, &bbox);
        match range {
            2 => {
                self.nodes[node].left = Child::Primitive(first);
                self.nodes[node].right = Child::Primitive(first + 1);
            }
            3 => {
                let right = self.new_node();
                self.nodes[node].left = Child::Primitive(first);
                self.nodes[node].right = Child::Node(right);
                self.expand(right, first + 1, beyond, 2);
            }
            _ => {
                let new_range = range / 2;
                let left = self.new_node();
                let right = self.new_node();
                self.nodes[node].left = Child::Node(left);
                self.nodes[node].right = Child::Node(right);
                self.expand(left, first, first + new_range, new_range);
                self.expand(right, first + new_range, beyond, range - new_range);
            }
        }
    }

    fn primitive(&self, c: Child) -> &TrianglePrimitive {
        match c {
            Child::Primitive(i) | Child::Node(i) => &self.primitives[i],
        }
    }

    fn node(&self, c: Child) -> usize {
        match c {
            Child::Node(i) | Child::Primitive(i) => i,
        }
    }

    /// `AABB_tree::traversal(query, traits)`.
    fn traversal(&self, traits: &mut impl TraversalTraits) -> Result<()> {
        match self.primitives.len() {
            0 => Ok(()),
            1 => traits.intersection(&self.primitives[0]),
            n => self.node_traversal(0, traits, n),
        }
    }

    /// `AABB_node::traversal(query, traits, nb_primitives)`
    /// (`go_further()` is always true for the listing traits).
    fn node_traversal(
        &self,
        node: usize,
        traits: &mut impl TraversalTraits,
        nb: usize,
    ) -> Result<()> {
        let n = self.nodes[node];
        match nb {
            2 => {
                traits.intersection(self.primitive(n.left))?;
                traits.intersection(self.primitive(n.right))?;
            }
            3 => {
                traits.intersection(self.primitive(n.left))?;
                let r = self.node(n.right);
                if traits.do_intersect(&self.nodes[r].bbox) {
                    self.node_traversal(r, traits, 2)?;
                }
            }
            _ => {
                let (l, r) = (self.node(n.left), self.node(n.right));
                if traits.do_intersect(&self.nodes[l].bbox) {
                    self.node_traversal(l, traits, nb / 2)?;
                    if traits.do_intersect(&self.nodes[r].bbox) {
                        self.node_traversal(r, traits, nb - nb / 2)?;
                    }
                } else if traits.do_intersect(&self.nodes[r].bbox) {
                    self.node_traversal(r, traits, nb - nb / 2)?;
                }
            }
        }
        Ok(())
    }
}

impl TriangleIndex for AabbTree {
    fn build(primitives: &[TrianglePrimitive]) -> Result<Self> {
        Ok(Self::new(primitives))
    }

    fn all_intersections(
        &self,
        query: &Segment3,
        out: &mut Vec<(TriangleSegmentIntersection, UInt)>,
    ) -> Result<()> {
        self.traversal(&mut ListIntersections { query, out })
    }

    fn all_intersected_primitives(&self, query: &Ray3, out: &mut Vec<UInt>) {
        // Ray/primitive tests never fail.
        let _ = self.traversal(&mut ListPrimitives { query, out });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use akantu_geometry::index::LinearScan;
    use akantu_geometry::kernel::{Point3, Triangle3};

    fn grid(n: usize) -> Vec<TrianglePrimitive> {
        let mut v = Vec::new();
        let h = 1.0 / n as f64;
        let mut id = 0;
        for j in 0..n {
            for i in 0..n {
                let (x, y) = (i as f64 * h, j as f64 * h);
                let a = Point3::new(x, y, 0.0);
                let b = Point3::new(x + h, y, 0.0);
                let c = Point3::new(x + h, y + h, 0.0);
                let d = Point3::new(x, y + h, 0.0);
                v.push(TrianglePrimitive {
                    id,
                    triangle: Triangle3::new(a, b, c),
                });
                v.push(TrianglePrimitive {
                    id: id + 1,
                    triangle: Triangle3::new(a, c, d),
                });
                id += 2;
            }
        }
        v
    }

    #[test]
    fn same_intersections_as_the_linear_scan() {
        let prims = grid(7);
        let tree = AabbTree::new(&prims);
        assert_eq!(tree.size(), prims.len());
        assert_eq!(tree.nodes.len(), prims.len() - 1);
        let scan = LinearScan::build(&prims).unwrap();
        for q in [
            Segment3::new(Point3::new(-0.1, 0.33, 0.0), Point3::new(1.2, 0.71, 0.0)),
            Segment3::new(Point3::new(0.5, -1.0, 0.0), Point3::new(0.5, 2.0, 0.0)),
            Segment3::new(Point3::new(0.13, 0.17, 0.0), Point3::new(0.14, 0.18, 0.0)),
            Segment3::new(Point3::new(2.0, 2.0, 0.0), Point3::new(3.0, 3.0, 0.0)),
        ] {
            let mut a = Vec::new();
            let mut b = Vec::new();
            tree.all_intersections(&q, &mut a).unwrap();
            scan.all_intersections(&q, &mut b).unwrap();
            let key = |v: &mut Vec<(TriangleSegmentIntersection, UInt)>| {
                let mut ids: Vec<UInt> = v.iter().map(|e| e.1).collect();
                ids.sort_unstable();
                ids
            };
            assert_eq!(key(&mut a), key(&mut b), "{q:?}");
            let mut ra = Vec::new();
            let mut rb = Vec::new();
            let ray = Ray3::new(q.source, q.target);
            tree.all_intersected_primitives(&ray, &mut ra);
            scan.all_intersected_primitives(&ray, &mut rb);
            ra.sort_unstable();
            rb.sort_unstable();
            assert_eq!(ra, rb);
        }
    }

    #[test]
    fn small_trees() {
        let prims = grid(1);
        let t = AabbTree::new(&prims[..1]);
        let q = Segment3::new(Point3::new(0.0, 0.1, 0.0), Point3::new(1.0, 0.1, 0.0));
        let mut out = Vec::new();
        t.all_intersections(&q, &mut out).unwrap();
        assert_eq!(out.len(), 1);
        let t0 = AabbTree::new(&[]);
        out.clear();
        t0.all_intersections(&q, &mut out).unwrap();
        assert!(out.is_empty());
        let t3 = AabbTree::new(&grid(2)[..3]);
        assert_eq!(t3.nodes.len(), 2);
    }
}
