// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): test/test_geometry/test_geometry_intersection.cc
// (Cartesian part; the spherical-kernel part is in akantu-geometry-cgal).

//! Upstream `test_geometry_intersection`, Cartesian part: the factory over
//! `test_geometry_triangle.msh` and `all_intersections` of the segment
//! `(0, 0.25)–(1, 0.25)` yield `[(0.25,0.25), (0.75,0.25)]` first and
//! `[(0,0.25), (0.25,0.25)]` last. Here with the LGPL linear scan (the
//! primitive order equals the AABB traversal order for this two-triangle
//! mesh; the CGAL tree version is in `akantu-geometry-cgal`).

mod common;

use akantu_core::GhostType;
use akantu_core::math::DEFAULT_TOLERANCE;
use akantu_geometry::factory::{MeshGeomFactory, TetrahedronPrimitives};
use akantu_geometry::helpers::compare_segments;
use akantu_geometry::index::{LinearScan, TriangleIndex};
use akantu_geometry::intersections::TriangleSegmentIntersection;
use akantu_geometry::kernel::{Point3, Segment3};

#[test]
fn test_geometry_intersection_cartesian() {
    let mesh = common::read(2, "test_geometry_triangle.msh", "mesh");
    let mut factory = MeshGeomFactory::new(
        akantu_core::ElementType::Triangle3,
        TetrahedronPrimitives::Faces,
    )
    .unwrap();
    factory.construct_data(&mesh, GhostType::NotGhost).unwrap();
    let tree = LinearScan::build(factory.primitive_list()).unwrap();

    let a = Point3::new(0.0, 0.25, 0.0);
    let b = Point3::new(1.0, 0.25, 0.0);
    let line = Segment3::new(a, b);
    let intermediate = Point3::new(0.25, 0.25, 0.0);
    let end = Point3::new(0.75, 0.25, 0.0);
    let result_0 = Segment3::new(a, intermediate);
    let result_1 = Segment3::new(intermediate, end);

    let mut list = Vec::new();
    tree.all_intersections(&line, &mut list).unwrap();
    assert_eq!(list.len(), 2);
    let (intersection_0, intersection_1) = (list[1].0, list[0].0);
    match intersection_0 {
        TriangleSegmentIntersection::Segment(s) => {
            assert!(compare_segments(&s, &result_0, DEFAULT_TOLERANCE))
        }
        other => panic!("{other:?}"),
    }
    match intersection_1 {
        TriangleSegmentIntersection::Segment(s) => {
            assert!(compare_segments(&s, &result_1, DEFAULT_TOLERANCE))
        }
        other => panic!("{other:?}"),
    }
    assert_eq!((list[0].1, list[1].1), (0, 1), "primitive ids");
}
