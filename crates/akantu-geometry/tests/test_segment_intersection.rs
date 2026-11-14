// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS):
//   test/test_geometry/test_segment_intersection_triangle_3.cc (segment part)
//   test/test_geometry/test_segment_intersection_tetrahedron_4.cc

//! Upstream `test_segment_intersection_triangle_3` and
//! `test_segment_intersection_tetrahedron_4` with the port's defaults
//! (corrected ordering UD-408, tetrahedron faces UD-407, result connectivity
//! created on demand UD-072 — upstream's two tests throw in the oracle
//! because of UD-072). The same intents are checked for both search
//! structures in `akantu-geometry-cgal`.

mod common;

use akantu_core::{Element, ElementType, GhostType};
use akantu_geometry::kernel::{Point3, Segment3};
use akantu_geometry::{LinearScan, MeshSegmentIntersector, SegmentIntersectorOptions};
use akantu_mesh::Mesh;

fn barycenter(m: &Mesh, e: usize) -> Vec<f64> {
    m.barycenter(Element::try_new(ElementType::Segment2, e, GhostType::NotGhost).unwrap())
        .unwrap()
}

fn p(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

#[test]
fn test_segment_intersection_triangle_3() {
    let tol = 1e-10; // Math::setTolerance(1e-10)
    let mesh = common::read(2, "test_geometry_triangle.msh", "mesh");
    let mut interface_mesh = Mesh::new(2, "interface_mesh").unwrap();
    let opts = SegmentIntersectorOptions {
        point_tolerance: tol,
        ..SegmentIntersectorOptions::default()
    };
    let mut intersector =
        MeshSegmentIntersector::<LinearScan>::new(&mesh, ElementType::Triangle3, opts).unwrap();
    intersector
        .construct_data(&mesh, GhostType::NotGhost)
        .unwrap();

    // Testing a segment going out of the mesh
    let h = Segment3::new(p(0.0, 0.25, 0.0), p(1.0, 0.25, 0.0));
    let v = Segment3::new(p(0.25, 0.0, 0.0), p(0.25, 1.0, 0.0));
    intersector
        .compute_intersection_query_list(&mesh, &[h, v], &mut interface_mesh)
        .unwrap();
    assert_eq!(
        interface_mesh.nb_element(ElementType::Segment2, GhostType::NotGhost),
        4
    );
    assert!(common::are_vector_equal(
        &barycenter(&interface_mesh, 0),
        &[0.125, 0.25],
        tol
    ));

    // Testing a segment completely inside an element
    let inside = Segment3::new(p(0.1, 0.33, 0.0), p(0.1, 0.67, 0.0));
    intersector
        .compute_intersection_query(&mesh, &inside, &mut interface_mesh)
        .unwrap();
    let last = interface_mesh.nb_element(ElementType::Segment2, GhostType::NotGhost) - 1;
    assert!(common::are_vector_equal(
        &barycenter(&interface_mesh, last),
        &[0.1, 0.5],
        tol
    ));
    // Two new nodes per segment, no sharing.
    assert_eq!(interface_mesh.nb_nodes(), 10);
}

/// `true` if `{a, b}` equals `{x, y}` (the upstream test does not know the
/// element order).
fn either_order(a: &[f64], b: &[f64], x: &[f64], y: &[f64], tol: f64) -> bool {
    (common::are_vector_equal(a, x, tol) && common::are_vector_equal(b, y, tol))
        || (common::are_vector_equal(a, y, tol) && common::are_vector_equal(b, x, tol))
}

#[test]
fn test_segment_intersection_tetrahedron_4() {
    let tol = akantu_core::math::DEFAULT_TOLERANCE;
    let mesh = common::read(3, "test_geometry_tetrahedron.msh", "mesh");
    let mut interface_mesh = Mesh::new(3, "interface_mesh").unwrap();
    let mut intersector = MeshSegmentIntersector::<LinearScan>::new(
        &mesh,
        ElementType::Tetrahedron4,
        SegmentIntersectorOptions::default(),
    )
    .unwrap();
    intersector
        .construct_data(&mesh, GhostType::NotGhost)
        .unwrap();
    let nb = |m: &Mesh| m.nb_element(ElementType::Segment2, GhostType::NotGhost);

    // Testing a segment going through the cube
    let segment = Segment3::new(p(0.0, 0.0, 0.0), p(1.0, 1.0, 1.0));
    intersector
        .compute_intersection_query(&mesh, &segment, &mut interface_mesh)
        .unwrap();
    assert_eq!(nb(&interface_mesh), 2);
    let (b1, b2) = (
        barycenter(&interface_mesh, 0),
        barycenter(&interface_mesh, 1),
    );
    let first = [1.0 / 6.0; 3];
    let second = [2.0 / 3.0; 3];
    assert!(
        either_order(&b1, &b2, &first, &second, tol),
        "{b1:?} {b2:?}"
    );

    // Testing a segment completely inside one element
    let inside = Segment3::new(p(0.05, 0.05, 0.05), p(0.06, 0.06, 0.06));
    intersector
        .compute_intersection_query(&mesh, &inside, &mut interface_mesh)
        .unwrap();
    let b = barycenter(&interface_mesh, nb(&interface_mesh) - 1);
    assert!(common::are_vector_equal(&b, &[0.055; 3], tol), "{b:?}");

    // Testing a segment whose end points are inside elements
    let crossing = Segment3::new(p(0.1, 0.1, 0.1), p(0.9, 0.9, 0.9));
    intersector
        .compute_intersection_query(&mesh, &crossing, &mut interface_mesh)
        .unwrap();
    let el1 = nb(&interface_mesh) - 2;
    let (b1, b2) = (
        barycenter(&interface_mesh, el1),
        barycenter(&interface_mesh, el1 + 1),
    );
    assert!(
        either_order(&b1, &b2, &[13.0 / 60.0; 3], &[37.0 / 60.0; 3], tol),
        "{b1:?} {b2:?}"
    );

    // Testing a segment along the edge of elements
    let edge = Segment3::new(p(1.0, 0.0, 0.0), p(0.0, 1.0, 0.0));
    let current = nb(&interface_mesh);
    intersector
        .compute_intersection_query(&mesh, &edge, &mut interface_mesh)
        .unwrap();
    assert_eq!(nb(&interface_mesh), current + 1);
    let b = barycenter(&interface_mesh, nb(&interface_mesh) - 1);
    assert!(common::are_vector_equal(&b, &[0.5, 0.5, 0.0], tol), "{b:?}");
}

#[test]
fn result_mesh_data_is_filled_when_present() {
    // Embedded usage: associated_element / physical_names pre-allocated.
    let mesh = common::read(2, "test_geometry_triangle.msh", "mesh");
    let mut interface_mesh = Mesh::new(2, "interface_mesh").unwrap();
    interface_mesh
        .data_mut()
        .element_map_mut_or_insert("associated_element")
        .unwrap()
        .alloc(
            0,
            1,
            ElementType::Segment2,
            GhostType::NotGhost,
            Element::NULL,
        )
        .unwrap();
    interface_mesh
        .data_mut()
        .strings_mut_or_insert("physical_names")
        .unwrap()
        .alloc(
            0,
            1,
            ElementType::Segment2,
            GhostType::NotGhost,
            String::new(),
        )
        .unwrap();
    let mut intersector = MeshSegmentIntersector::<LinearScan>::new(
        &mesh,
        ElementType::Triangle3,
        SegmentIntersectorOptions::default(),
    )
    .unwrap();
    intersector.set_physical_name("steel");
    let h = Segment3::new(p(0.0, 0.25, 0.0), p(1.0, 0.25, 0.0));
    intersector
        .build_result_from_query_list(&mesh, &[h], &mut interface_mesh)
        .unwrap();
    let assoc = interface_mesh
        .data()
        .element_map("associated_element")
        .unwrap()
        .get(ElementType::Segment2, GhostType::NotGhost)
        .unwrap();
    let elements: Vec<i32> = assoc
        .as_slice()
        .iter()
        .map(akantu_core::Element::index)
        .collect();
    assert_eq!(elements, vec![1, 0]);
    let names = interface_mesh
        .data()
        .strings("physical_names")
        .unwrap()
        .get(ElementType::Segment2, GhostType::NotGhost)
        .unwrap();
    assert!(names.as_slice().iter().all(|n| n == "steel"));
    assert!(
        intersector
            .compute_mesh_query_intersection_point(&h, 0)
            .is_err()
    );
}

/// The queries must use the mesh given to
/// `construct_data`; a mesh of another dimension or element count is refused.
#[test]
fn queries_refuse_a_different_mesh() {
    let mesh = common::read(2, "test_geometry_triangle.msh", "mesh");
    let other = common::read(3, "test_geometry_tetrahedron.msh", "other");
    let si = MeshSegmentIntersector::<LinearScan>::new(
        &mesh,
        ElementType::Triangle3,
        SegmentIntersectorOptions::default(),
    )
    .unwrap();
    let q = Segment3::new(p(0.0, 0.25, 0.0), p(1.0, 0.25, 0.0));
    assert!(si.compute_segments(&mesh, &q).is_ok());
    assert!(si.compute_segments(&other, &q).is_err());
    let mut res = Mesh::new(2, "interface_mesh").unwrap();
    assert!(si.compute_intersection_query(&other, &q, &mut res).is_err());
    assert_eq!(
        res.nb_element(ElementType::Segment2, GhostType::NotGhost),
        0
    );
}
