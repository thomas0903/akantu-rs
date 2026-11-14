// SPDX-License-Identifier: GPL-3.0-or-later
// akantu-rs — GPL-3.0-or-later (D-042). Ported from Akantu (EPFL/LSMS):
//   test/test_geometry/test_geometry_intersection.cc
//   extra_packages/igfem/test/test_geometry/test_spherical_kernel.cc

//! Upstream `test_geometry_intersection` (complete: CGAL AABB tree over the
//! triangle mesh, spherical kernel line-arc/sphere intersection, line-arc
//! factory) and IGFEM's `test_spherical_kernel`.

mod common;

use akantu_core::math::DEFAULT_TOLERANCE;
use akantu_core::{ElementType, GhostType};
use akantu_geometry::factory::{MeshGeomFactory, TetrahedronPrimitives};
use akantu_geometry::helpers::compare_segments;
use akantu_geometry::index::TriangleIndex;
use akantu_geometry::intersections::TriangleSegmentIntersection;
use akantu_geometry::kernel::{Point3, Segment3};
use akantu_geometry_cgal::AabbTree;
use akantu_geometry_cgal::sphere_intersector::line_arc_primitives;
use akantu_geometry_cgal::spherical::{
    CircularArcPoint, LineArc, SkPoint, SkSphere, compare_arc_points,
};

#[test]
fn test_geometry_intersection() {
    let mesh = common::read(2, "test_geometry_triangle.msh", "mesh");
    let mut factory =
        MeshGeomFactory::new(ElementType::Triangle3, TetrahedronPrimitives::Faces).unwrap();
    factory.construct_data(&mesh, GhostType::NotGhost).unwrap();
    let tree = AabbTree::build(factory.primitive_list()).unwrap();

    let a = Point3::new(0.0, 0.25, 0.0);
    let b = Point3::new(1.0, 0.25, 0.0);
    let line = Segment3::new(a, b);
    let intermediate = Point3::new(0.25, 0.25, 0.0);
    let end = Point3::new(0.75, 0.25, 0.0);
    let result_0 = Segment3::new(a, intermediate);
    let result_1 = Segment3::new(intermediate, end);

    let mut list = Vec::new();
    tree.all_intersections(&line, &mut list).unwrap();
    let intersection_0 = list.last().map(|e| e.0);
    let intersection_1 = list.first().map(|e| e.0);
    match intersection_0 {
        Some(TriangleSegmentIntersection::Segment(s)) => {
            assert!(compare_segments(&s, &result_0, DEFAULT_TOLERANCE))
        }
        other => panic!("{other:?}"),
    }
    match intersection_1 {
        Some(TriangleSegmentIntersection::Segment(s)) => {
            assert!(compare_segments(&s, &result_1, DEFAULT_TOLERANCE))
        }
        other => panic!("{other:?}"),
    }

    // Spherical kernel: SK::Sphere_3((0,0,0), 3) and the line arc of the
    // segment (0,0,0)–(2,2,2) meet at (1,1,1).
    let sphere = SkSphere::try_from_f64([0.0, 0.0, 0.0], 3.0).unwrap();
    let arc = LineArc::from_points(
        &SkPoint::try_from_f64([0.0, 0.0, 0.0]).unwrap(),
        &SkPoint::try_from_f64([2.0, 2.0, 2.0]).unwrap(),
    )
    .unwrap();
    let s_results = arc.intersect_sphere(&sphere).unwrap();
    let front = &s_results.first().expect("one intersection").0;
    let expected = CircularArcPoint::from_point(&SkPoint::try_from_f64([1.0, 1.0, 1.0]).unwrap());
    assert!(compare_arc_points(front, &expected, DEFAULT_TOLERANCE).unwrap());

    // MeshGeomFactory<2, _triangle_3, Line_arc<SK>, SK>::constructData
    let arcs = line_arc_primitives(&mesh, ElementType::Triangle3, GhostType::NotGhost).unwrap();
    assert_eq!(arcs.len(), 6);
    assert_eq!(
        arcs.iter().map(|p| (p.id, p.seg_id)).collect::<Vec<_>>(),
        vec![(0, 0), (0, 1), (0, 2), (1, 0), (1, 1), (1, 2)]
    );
}

/// IGFEM `test_spherical_kernel`: the arc (0.3,0.4,0)–(0.5,−0.2,0) meets
/// the sphere ((0.4,0,0), 0.3²) in exactly one non-tangent point.
#[test]
fn test_spherical_kernel() {
    let b = SkPoint::try_from_f64([0.3, 0.4, 0.0]).unwrap();
    let c = SkPoint::try_from_f64([0.5, -0.2, 0.0]).unwrap();
    let s2 = LineArc::from_points(&b, &c).unwrap();
    let sphere = SkSphere::try_from_f64([0.4, 0.0, 0.0], 0.3 * 0.3).unwrap();
    let s_results = s2.intersect_sphere(&sphere).unwrap();
    assert_eq!(s_results.len(), 1, "just one point");
    let (p, m) = &s_results[0];
    assert_eq!(*m, 1, "not a point tangent to the sphere");
    let [x, y, z] = p.to_double().unwrap();
    // t² − 1.3 t + 0.2 = 0 on x = 0.3 + 0.2 t, y = 0.4 − 0.6 t.
    let t = (1.3 - 0.89_f64.sqrt()) / 2.0;
    assert!((x - (0.3 + 0.2 * t)).abs() < 1e-15, "{x}");
    assert!((y - (0.4 - 0.6 * t)).abs() < 1e-15, "{y}");
    assert_eq!(z, 0.0);
    // Exact representation: an extended Root_of_2 (a0 + a1·√γ).
    assert!(p.c[0].is_extended());
}
