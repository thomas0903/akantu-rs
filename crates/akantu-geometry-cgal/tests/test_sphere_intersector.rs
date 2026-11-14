// SPDX-License-Identifier: GPL-3.0-or-later
// akantu-rs — GPL-3.0-or-later (D-042). Ported from Akantu (EPFL/LSMS):
//   test/test_geometry/test_segment_intersection_triangle_3.cc (the
//   spherical-kernel block disabled with `#if 0` upstream),
//   src/geometry/mesh_sphere_intersector_tmpl.hh (behaviour checks).

//! Sphere intersector (GEOM-008). Upstream's only test of the class is
//! disabled (`#if 0`) and written against a removed API; its intent —
//! spheres `((0,1,0), 0.2²)` and `((1,0,0), 0.4999999999)` on
//! `test_geometry_triangle.msh` give 1 new node in element 0 and 2 in
//! element 1 — holds with the default (v4) semantics. The v5 semantics
//! (UD-410/411/412, `SphereIntersectorOptions::upstream()`) are checked
//! separately, together with exact-geometry invariants.

mod common;

use akantu_core::{ElementType, GhostType, Real};
use akantu_geometry_cgal::spherical::SkSphere;
use akantu_geometry_cgal::{
    MeshSphereIntersector, NodeNumbering, ProximityTest, SphereIntersectorOptions, TangentPolicy,
};

fn run(opts: SphereIntersectorOptions) -> MeshSphereIntersector {
    let mesh = common::read(2, "test_geometry_triangle.msh", "mesh");
    let mut si = MeshSphereIntersector::new(&mesh, ElementType::Triangle3, opts).unwrap();
    si.construct_data(&mesh, GhostType::NotGhost).unwrap();
    let spheres = [
        SkSphere::try_from_f64([0.0, 1.0, 0.0], 0.2 * 0.2).unwrap(),
        SkSphere::try_from_f64([1.0, 0.0, 0.0], 0.499_999_999_9).unwrap(),
    ];
    let nb_old = i32::try_from(mesh.nb_nodes()).unwrap();
    si.compute_mesh_query_list_intersection_point(&mesh, &spheres, nb_old)
        .unwrap();
    si
}

#[test]
fn test_sphere_intersection_triangle_3_default() {
    let si = run(SphereIntersectorOptions::default());
    let nn = si.new_node_per_elem();
    assert_eq!(nn.nb_component(), 5);
    assert_eq!(nn.len(), 2);
    assert_eq!(nn.tuple(0)[0], 1, "element 0: {:?}", nn.tuple(0));
    assert_eq!(nn.tuple(1)[0], 2, "element 1: {:?}", nn.tuple(1));
    // Three new nodes (upstream prints nodes 5, 6, 7).
    let ip = si.intersection_points();
    assert_eq!(ip.len(), 3);
    // Sphere 1 cuts element 1 on edges (b,c) = (0.5,0.5)–(0,1) and
    // (c,a) = (0,1)–(0,0); sphere 2 cuts element 0 on (a,b) = (0,0)–(1,0).
    assert_eq!(nn.tuple(1), &[2, 4, 1, 5, 2]);
    assert_eq!(nn.tuple(0), &[1, 6, 0, 0, 0]);
    let s2 = 0.5_f64.sqrt() * 0.2;
    assert!((ip.tuple(0)[0] - s2).abs() < 1e-15 && (ip.tuple(0)[1] - (1.0 - s2)).abs() < 1e-15);
    assert_eq!(ip.tuple(1)[0], 0.0);
    assert!((ip.tuple(1)[1] - 0.8).abs() < 1e-15);
    assert!((ip.tuple(2)[0] - (1.0 - 0.499_999_999_9_f64.sqrt())).abs() < 1e-15);
    assert_eq!(ip.tuple(2)[1], 0.0);
}

#[test]
fn test_sphere_intersection_triangle_3_upstream_v5() {
    // v5: Eigen isApprox (1e-12) does not see the point 7e-11 from node
    // (0.5,0.5) as "on the mesh" → element 0 gets 2 new nodes, and pending
    // points are all numbered nodes.size() = 4 (UD-410/411).
    let si = run(SphereIntersectorOptions::upstream());
    let nn = si.new_node_per_elem();
    assert_eq!(nn.tuple(1), &[2, 4, 1, 4, 2]);
    assert_eq!(nn.tuple(0), &[2, 4, 0, 4, 1]);
    assert_eq!(si.intersection_points().len(), 4);
}

#[test]
fn tangent_points_follow_the_policy() {
    // Sphere ((0.25, -0.125), 0.125²) touches edge (a,b) = (0,0)–(1,0) of
    // element 0 at (0.25, 0) (exactly: binary fractions).
    let mesh = common::read(2, "test_geometry_triangle.msh", "mesh");
    let tangent = SkSphere::try_from_f64([0.25, -0.125, 0.0], 0.125 * 0.125).unwrap();
    for (policy, expected) in [
        (TangentPolicy::Reject, 0),
        (TangentPolicy::UpstreamAccept, 1),
    ] {
        let opts = SphereIntersectorOptions {
            tangent: policy,
            ..SphereIntersectorOptions::default()
        };
        let mut si = MeshSphereIntersector::new(&mesh, ElementType::Triangle3, opts).unwrap();
        si.construct_data(&mesh, GhostType::NotGhost).unwrap();
        si.compute_mesh_query_intersection_point(&mesh, &tangent, 4)
            .unwrap();
        assert_eq!(si.new_node_per_elem().tuple(0)[0], expected, "{policy:?}");
    }
}

/// UD-412 default: with the default options an exact
/// tangent point creates no node (v4, intent). Making
/// `TangentPolicy::UpstreamAccept` the default fails this test.
#[test]
fn default_options_reject_tangent_points() {
    let d = SphereIntersectorOptions::default();
    assert_eq!(d.tangent, TangentPolicy::Reject, "UD-412 default");
    assert_eq!(
        d.proximity,
        ProximityTest::MathTolerance(1e-10),
        "UD-411 default"
    );
    assert_eq!(d.numbering, NodeNumbering::Continued, "UD-410 default");
    // Sphere ((0.25, -0.125), 0.125²) touches edge (0,0)–(1,0) of element 0
    // at (0.25, 0), exactly (binary fractions): one root of multiplicity 2.
    let mesh = common::read(2, "test_geometry_triangle.msh", "mesh");
    let tangent = SkSphere::try_from_f64([0.25, -0.125, 0.0], 0.125 * 0.125).unwrap();
    let mut si = MeshSphereIntersector::new(&mesh, ElementType::Triangle3, d).unwrap();
    si.construct_data(&mesh, GhostType::NotGhost).unwrap();
    si.compute_mesh_query_intersection_point(&mesh, &tangent, 4)
        .unwrap();
    assert_eq!(
        si.new_node_per_elem().tuple(0)[0],
        0,
        "tangent point accepted"
    );
    assert_eq!(si.intersection_points().len(), 0);
}

#[test]
fn intersection_points_lie_on_sphere_and_edges() {
    // Independent check on a structured mesh: every new point is within
    // rounding of the sphere and of its edge, and each element has at most
    // two new nodes for a sphere crossing it smoothly.
    let mesh = common::triangle_grid(12, 9, 1.0, 0.75, true);
    let mut si = MeshSphereIntersector::new(
        &mesh,
        ElementType::Triangle3,
        SphereIntersectorOptions::default(),
    )
    .unwrap();
    si.construct_data(&mesh, GhostType::NotGhost).unwrap();
    let (cx, cy, r2) = (0.47, 0.41, 0.23 * 0.23);
    let sphere = SkSphere::try_from_f64([cx, cy, 0.0], r2).unwrap();
    si.compute_mesh_query_intersection_point(
        &mesh,
        &sphere,
        i32::try_from(mesh.nb_nodes()).unwrap(),
    )
    .unwrap();
    let ip = si.intersection_points();
    assert!(ip.len() > 8);
    for p in ip.tuples() {
        let d2: Real = (p[0] - cx) * (p[0] - cx) + (p[1] - cy) * (p[1] - cy);
        assert!((d2 - r2).abs() < 1e-15, "{p:?}: {d2}");
    }
    let nn = si.new_node_per_elem();
    let nodes = mesh.nodes();
    for e in 0..nn.len() {
        let row = nn.tuple(e);
        assert!(row[0] <= 2);
        for k in 0..usize::try_from(row[0]).unwrap() {
            let node = usize::try_from(row[1 + 2 * k]).unwrap() - mesh.nb_nodes();
            let edge = usize::try_from(row[2 + 2 * k]).unwrap();
            let conn_g = mesh
                .connectivity(ElementType::Triangle3, GhostType::NotGhost)
                .unwrap();
            let conn = conn_g.tuple(e);
            let a = nodes.tuple(usize::try_from(conn[edge]).unwrap());
            let b = nodes.tuple(usize::try_from(conn[(edge + 1) % 3]).unwrap());
            let p = ip.tuple(node);
            let cross = (b[0] - a[0]) * (p[1] - a[1]) - (b[1] - a[1]) * (p[0] - a[0]);
            assert!(cross.abs() < 1e-15, "point off its edge: {cross}");
        }
    }
}
