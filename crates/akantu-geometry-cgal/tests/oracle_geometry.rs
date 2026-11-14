// SPDX-License-Identifier: GPL-3.0-or-later
// akantu-rs — GPL-3.0-or-later (D-042). Ported from Akantu (EPFL/LSMS):
// differential tests of src/geometry (MeshGeomFactory + CGAL AABB tree,
// MeshSegmentIntersector, cgal::Spherical line-arc/sphere intersection,
// MeshSphereIntersector) against the CGAL-enabled oracle fixtures
// fixtures/geometry/* written by oracle/probes/geometry/geometry_probe.cc.

//! Bitwise differential tests (x86_64 strict oracle, D-043):
//! * `aabb/*`: primitive list and the **order** of `all_intersections` /
//!   `all_intersected_primitives` (CGAL tree build with the clean-room
//!   `std::nth_element`);
//! * `segment_intersector/*`: result meshes of upstream's
//!   `MeshSegmentIntersector` with `SegmentIntersectorOptions::upstream()`
//!   (UD-407/408 compatibility) over the CGAL-compatible tree;
//! * `spherical/line_arc_sphere`: counts, multiplicities and `to_double`
//!   of the exact intersection points;
//! * `sphere_intersector/*`: upstream (v5) semantics
//!   (`SphereIntersectorOptions::upstream()`) against the compile-fixed and
//!   zero-fill-patched class (UD-409, UD-414).

use akantu_core::{ElementType, GhostType, Real};
use akantu_geometry::factory::{MeshGeomFactory, TetrahedronPrimitives};
use akantu_geometry::index::TriangleIndex;
use akantu_geometry::intersections::TriangleSegmentIntersection;
use akantu_geometry::kernel::{Point3, Ray3, Segment3};
use akantu_geometry::{MeshSegmentIntersector, SegmentIntersectorOptions};
use akantu_geometry_cgal::spherical::{LineArc, SkPoint, SkSphere};
use akantu_geometry_cgal::{AabbTree, MeshSphereIntersector, SphereIntersectorOptions};
use akantu_mesh::{Mesh, MeshAccessor};
use akantu_validation::Fixture;

fn open(id: &str) -> Fixture {
    let fx = Fixture::open_by_id(id).unwrap_or_else(|e| panic!("{id}: {e}"));
    fx.require_golden().unwrap();
    fx
}

fn f64s(fx: &Fixture, name: &str) -> Vec<f64> {
    fx.load_f64(name)
        .unwrap_or_else(|e| panic!("{name}: {e}"))
        .data
}

fn i64s(fx: &Fixture, name: &str) -> Vec<i64> {
    fx.load_i64(name)
        .unwrap_or_else(|e| panic!("{name}: {e}"))
        .data
}

/// Exact comparison with the oracle record (D-046: bit identity, or
/// default NaNs of either sign; signed zeros differ, D-051).
fn bits_eq(a: &[f64], b: &[f64], what: &str) {
    assert_eq!(a.len(), b.len(), "{what}: length");
    for (i, (x, y)) in a.iter().zip(b).enumerate() {
        assert!(
            akantu_validation::exact_eq(*y, *x),
            "{what}[{i}]: {x} ({:#018x}) vs {y} ({:#018x})",
            x.to_bits(),
            y.to_bits()
        );
    }
}

/// Rebuild the probe's input mesh from `input/nodes` and
/// `input/connectivity`.
fn mesh_from(fx: &Fixture, ty: ElementType) -> Mesh {
    let nodes = fx.load_f64("input/nodes").unwrap();
    let conn = fx.load_i64("input/connectivity").unwrap();
    let dim = nodes.shape[1];
    let mut m = Mesh::new(dim, "mesh").unwrap();
    {
        let mut acc = MeshAccessor::new(&mut m);
        for x in nodes.data.chunks(dim) {
            acc.push_node(x).unwrap();
        }
        let k = conn.shape[1];
        let mut c = acc.connectivity_mut(ty, GhostType::NotGhost);
        for e in conn.data.chunks(k) {
            let t: Vec<i32> = e.iter().map(|&v| i32::try_from(v).unwrap()).collect();
            c.push_tuple(&t);
        }
    }
    m
}

fn segments(q: &[f64]) -> Vec<Segment3> {
    q.chunks(6)
        .map(|s| Segment3::new(Point3::new(s[0], s[1], s[2]), Point3::new(s[3], s[4], s[5])))
        .collect()
}

fn encode(i: &TriangleSegmentIntersection) -> (i64, [f64; 6]) {
    match i {
        TriangleSegmentIntersection::Point(q) => (1, [q.x, q.y, q.z, 0.0, 0.0, 0.0]),
        TriangleSegmentIntersection::Segment(g) => (
            2,
            [
                g.source.x, g.source.y, g.source.z, g.target.x, g.target.y, g.target.z,
            ],
        ),
    }
}

fn aabb_case(id: &str, ty: ElementType) {
    let fx = open(&format!("geometry/aabb/{id}"));
    let mesh = mesh_from(&fx, ty);
    let mut factory = MeshGeomFactory::new(ty, TetrahedronPrimitives::UpstreamTriangle).unwrap();
    factory.construct_data(&mesh, GhostType::NotGhost).unwrap();
    let prims = factory.primitive_list();
    let want_tri = f64s(&fx, "primitives/triangles");
    let got_tri: Vec<f64> = prims
        .iter()
        .flat_map(|p| p.triangle.v.iter().flat_map(|v| v.to_array()))
        .collect();
    bits_eq(&got_tri, &want_tri, "primitives/triangles");
    let ids: Vec<i64> = prims.iter().map(|p| i64::from(p.id)).collect();
    assert_eq!(ids, i64s(&fx, "primitives/ids"));
    let tree = AabbTree::build(prims).unwrap();
    let queries = segments(&f64s(&fx, "input/queries"));
    let off = i64s(&fx, "all_intersections/offsets");
    let want_ids = i64s(&fx, "all_intersections/ids");
    let want_kinds = i64s(&fx, "all_intersections/kinds");
    let want_coords = f64s(&fx, "all_intersections/coords");
    let roff = i64s(&fx, "rays/offsets");
    let want_rays = i64s(&fx, "rays/ids");
    let mut nonempty = 0;
    for (k, q) in queries.iter().enumerate() {
        let (a, b) = (
            usize::try_from(off[k]).unwrap(),
            usize::try_from(off[k + 1]).unwrap(),
        );
        let mut got = Vec::new();
        tree.all_intersections(q, &mut got).unwrap();
        let got_ids: Vec<i64> = got.iter().map(|e| i64::from(e.1)).collect();
        assert_eq!(
            got_ids,
            want_ids[a..b],
            "query {k}: traversal order ({q:?})"
        );
        for (j, (i, _)) in got.iter().enumerate() {
            let (kind, c) = encode(i);
            assert_eq!(kind, want_kinds[a + j], "query {k} hit {j}: kind");
            bits_eq(
                &c,
                &want_coords[6 * (a + j)..6 * (a + j + 1)],
                &format!("query {k} hit {j}"),
            );
        }
        nonempty += usize::from(b > a + 1);
        let (ra, rb) = (
            usize::try_from(roff[k]).unwrap(),
            usize::try_from(roff[k + 1]).unwrap(),
        );
        let mut rays = Vec::new();
        tree.all_intersected_primitives(&Ray3::new(q.source, q.target), &mut rays);
        let rays: Vec<i64> = rays.into_iter().map(i64::from).collect();
        assert_eq!(rays, want_rays[ra..rb], "query {k}: ray order");
    }
    assert!(nonempty > 20, "queries with several hits: {nonempty}");
}

#[test]
fn aabb_triangle_grid_matches_the_oracle() {
    aabb_case("triangle_grid", ElementType::Triangle3);
}

#[test]
fn aabb_tetrahedron_grid_matches_the_oracle() {
    aabb_case("tetrahedron_grid", ElementType::Tetrahedron4);
}

fn segment_case(id: &str, ty: ElementType) {
    let fx = open(&format!("geometry/segment_intersector/{id}"));
    let mesh = mesh_from(&fx, ty);
    let dim = ty.spatial_dimension();
    let mut result = Mesh::new(dim, "interface_mesh").unwrap();
    result
        .data_mut()
        .element_map_mut_or_insert("associated_element")
        .unwrap()
        .alloc(
            0,
            1,
            ElementType::Segment2,
            GhostType::NotGhost,
            akantu_core::Element::NULL,
        )
        .unwrap();
    result
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
    let mut si =
        MeshSegmentIntersector::<AabbTree>::new(&mesh, ty, SegmentIntersectorOptions::upstream())
            .unwrap();
    let queries = segments(&f64s(&fx, "input/queries"));
    let sizes = i64s(&fx, "input/batch_sizes");
    let after = i64s(&fx, "result/nb_elements_after_batch");
    let mut start = 0;
    for (b, &n) in sizes.iter().enumerate() {
        let n = usize::try_from(n).unwrap();
        si.set_physical_name(format!("batch_{b}"));
        si.build_result_from_query_list(&mesh, &queries[start..start + n], &mut result)
            .unwrap();
        start += n;
        assert_eq!(
            i64::try_from(result.nb_element(ElementType::Segment2, GhostType::NotGhost)).unwrap(),
            after[b],
            "elements after batch {b}"
        );
    }
    bits_eq(
        result.nodes().as_slice(),
        &f64s(&fx, "result/nodes"),
        "result/nodes",
    );
    let conn: Vec<i64> = result
        .connectivity(ElementType::Segment2, GhostType::NotGhost)
        .unwrap()
        .as_slice()
        .iter()
        .map(|&v| i64::from(v))
        .collect();
    assert_eq!(conn, i64s(&fx, "result/connectivity"));
    let assoc: Vec<i64> = result
        .data()
        .element_map("associated_element")
        .unwrap()
        .get(ElementType::Segment2, GhostType::NotGhost)
        .unwrap()
        .as_slice()
        .iter()
        .map(|e| i64::from(e.index()))
        .collect();
    assert_eq!(assoc, i64s(&fx, "result/associated_element"));
    let names: Vec<i64> = result
        .data()
        .strings("physical_names")
        .unwrap()
        .get(ElementType::Segment2, GhostType::NotGhost)
        .unwrap()
        .as_slice()
        .iter()
        .map(|s| s.trim_start_matches("batch_").parse::<i64>().unwrap())
        .collect();
    assert_eq!(names, i64s(&fx, "result/physical_names"));
}

#[test]
fn segment_intersector_triangle_test_mesh_matches_the_oracle() {
    segment_case("triangle_test_mesh", ElementType::Triangle3);
}

#[test]
fn segment_intersector_triangle_grid_matches_the_oracle() {
    segment_case("triangle_grid", ElementType::Triangle3);
}

#[test]
fn segment_intersector_tetrahedron_test_mesh_matches_the_oracle() {
    segment_case("tetrahedron_test_mesh", ElementType::Tetrahedron4);
}

#[test]
fn segment_intersector_tetrahedron_grid_matches_the_oracle() {
    segment_case("tetrahedron_grid", ElementType::Tetrahedron4);
}

#[test]
fn spherical_line_arc_sphere_matches_the_oracle() {
    let fx = open("geometry/spherical/line_arc_sphere");
    let arcs = f64s(&fx, "input/arcs");
    let spheres = f64s(&fx, "input/spheres");
    let count = i64s(&fx, "count");
    let mult = i64s(&fx, "multiplicity");
    let points = f64s(&fx, "points");
    let mut tangents = 0;
    for i in 0..count.len() {
        let a = &arcs[6 * i..6 * i + 6];
        let s = &spheres[4 * i..4 * i + 4];
        let arc = LineArc::from_points(
            &SkPoint::try_from_f64([a[0], a[1], a[2]]).unwrap(),
            &SkPoint::try_from_f64([a[3], a[4], a[5]]).unwrap(),
        )
        .unwrap();
        let sphere = SkSphere::try_from_f64([s[0], s[1], s[2]], s[3]).unwrap();
        let res = arc.intersect_sphere(&sphere).unwrap();
        assert_eq!(
            i64::try_from(res.len()).unwrap(),
            count[i],
            "sample {i}: count"
        );
        let mut got = [0.0; 6];
        let mut got_m = [0_i64; 2];
        for (k, (p, m)) in res.iter().enumerate() {
            got_m[k] = i64::from(*m);
            got[3 * k..3 * k + 3].copy_from_slice(&p.to_double().unwrap());
        }
        tangents += usize::from(got_m[0] == 2);
        assert_eq!(
            &got_m[..],
            &mult[2 * i..2 * i + 2],
            "sample {i}: multiplicities"
        );
        bits_eq(
            &got,
            &points[6 * i..6 * i + 6],
            &format!("sample {i}: points"),
        );
    }
    assert!(tangents > 10, "tangent cases: {tangents}");
}

fn sphere_case(id: &str) {
    let fx = open(&format!("geometry/sphere_intersector/{id}"));
    let mesh = mesh_from(&fx, ElementType::Triangle3);
    let spheres = f64s(&fx, "input/spheres");
    let sizes = i64s(&fx, "input/run_sizes");
    let status = i64s(&fx, "status");
    let ipo = i64s(&fx, "intersection_points/offsets");
    let ipv = f64s(&fx, "intersection_points/values");
    let nn = i64s(&fx, "new_node_per_elem");
    let ne = mesh.nb_element(ElementType::Triangle3, GhostType::NotGhost);
    let mut si = MeshSphereIntersector::new(
        &mesh,
        ElementType::Triangle3,
        SphereIntersectorOptions::upstream(),
    )
    .unwrap();
    let nb_old = i32::try_from(mesh.nb_nodes()).unwrap();
    let mut start = 0;
    for (r, &n) in sizes.iter().enumerate() {
        let n = usize::try_from(n).unwrap();
        let list: Vec<SkSphere> = spheres[4 * start..4 * (start + n)]
            .chunks(4)
            .map(|s| SkSphere::try_from_f64([s[0], s[1], s[2]], s[3]).unwrap())
            .collect();
        start += n;
        si.construct_data(&mesh, GhostType::NotGhost).unwrap();
        let res = si.compute_mesh_query_list_intersection_point(&mesh, &list, nb_old);
        assert_eq!(
            i64::from(res.is_err()),
            status[r],
            "run {r}: status ({res:?})"
        );
        let (a, b) = (
            usize::try_from(ipo[r]).unwrap(),
            usize::try_from(ipo[r + 1]).unwrap(),
        );
        bits_eq(
            si.intersection_points().as_slice(),
            &ipv[2 * a..2 * b],
            &format!("run {r}: intersection points"),
        );
        let got: Vec<i64> = si
            .new_node_per_elem()
            .as_slice()
            .iter()
            .map(|&v| i64::from(v))
            .collect();
        assert_eq!(
            got,
            nn[5 * ne * r..5 * ne * (r + 1)],
            "run {r}: new_node_per_elem"
        );
    }
    let total: Real = ipv.len() as Real;
    assert!(total > 0.0);
}

#[test]
fn sphere_intersector_triangle_test_mesh_matches_the_oracle() {
    sphere_case("triangle_test_mesh");
}

#[test]
fn sphere_intersector_triangle_grid_matches_the_oracle() {
    sphere_case("triangle_grid");
}
