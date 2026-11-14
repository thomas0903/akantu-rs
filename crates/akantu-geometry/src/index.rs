// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/geometry/tree_type_helper.hh
// (TreeTypeHelper / IntersectionTypeHelper for Triangle<cgal::Cartesian>
// primitives and Segment_3 queries) and the AABB-tree calls of
// mesh_segment_intersector_tmpl.hh (all_intersections,
// all_intersected_primitives).

//! Search structures over triangle primitives (GEOM-003).
//!
//! [`TriangleIndex`] is the interface upstream's code uses on its CGAL AABB
//! tree. Two implementations exist:
//!
//! * [`LinearScan`] (this crate, LGPL): tests every primitive in primitive
//!   order. It reports every intersection the tree reports, in a different
//!   order (unobservable with the default, corrected segment ordering of
//!   the intersector). The tree can additionally miss a primitive whose
//!   bounding box CGAL's floating-point box test rejects although the
//!   triangle test would hit it (rounding-borderline contacts only).
//! * `akantu_geometry_cgal::AabbTree` (GPL-3.0-or-later, opt-in, D-042): a
//!   port of CGAL's `AABB_tree` that reproduces upstream's traversal order
//!   (needed for bit-identical output in upstream-compatibility mode).

use akantu_core::{Result, UInt};

use crate::factory::TrianglePrimitive;
use crate::intersections::{
    TriangleSegmentIntersection, ray_triangle_do_intersect, triangle_segment_intersection,
};
use crate::kernel::{Ray3, Segment3};

/// Queries upstream issues on its `AABB_tree<Triangle_primitive>`.
pub trait TriangleIndex: Sized {
    /// Build over `primitives` (copied, in this order).
    ///
    /// # Errors
    /// Implementation specific.
    fn build(primitives: &[TrianglePrimitive]) -> Result<Self>;

    /// `all_intersections(segment)`: every primitive intersected, with the
    /// intersection object, in the structure's reporting order.
    ///
    /// # Errors
    /// A `CGAL_error()` branch of the intersection construction.
    fn all_intersections(
        &self,
        query: &Segment3,
        out: &mut Vec<(TriangleSegmentIntersection, UInt)>,
    ) -> Result<()>;

    /// `all_intersected_primitives(ray)`: ids of the primitives hit by the
    /// ray, in reporting order.
    fn all_intersected_primitives(&self, query: &Ray3, out: &mut Vec<UInt>);
}

/// Exhaustive search in primitive order (LGPL; see the module docs).
#[derive(Clone, Debug, Default)]
pub struct LinearScan {
    primitives: Vec<TrianglePrimitive>,
}

impl TriangleIndex for LinearScan {
    fn build(primitives: &[TrianglePrimitive]) -> Result<Self> {
        Ok(Self {
            primitives: primitives.to_vec(),
        })
    }

    fn all_intersections(
        &self,
        query: &Segment3,
        out: &mut Vec<(TriangleSegmentIntersection, UInt)>,
    ) -> Result<()> {
        for p in &self.primitives {
            if let Some(i) = triangle_segment_intersection(&p.triangle, query)? {
                out.push((i, p.id));
            }
        }
        Ok(())
    }

    fn all_intersected_primitives(&self, query: &Ray3, out: &mut Vec<UInt>) {
        for p in &self.primitives {
            if ray_triangle_do_intersect(&p.triangle, query) {
                out.push(p.id);
            }
        }
    }
}
