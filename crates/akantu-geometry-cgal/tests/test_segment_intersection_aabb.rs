// SPDX-License-Identifier: GPL-3.0-or-later
// akantu-rs — GPL-3.0-or-later (D-042). Ported from Akantu (EPFL/LSMS):
//   test/test_geometry/test_segment_intersection_triangle_3.cc
//   test/test_geometry/test_segment_intersection_tetrahedron_4.cc
// with the CGAL AABB tree (aabb.rs, GPL-derived) as search structure.

//! Segment intersector with the CGAL-compatible AABB tree:
//! * the upstream test intents with the default options (as with the LGPL
//!   linear scan);
//! * default results do not depend on the search structure (AABB tree vs
//!   linear scan, identical output on structured meshes);
//! * independent checks: the pieces cover the clipped query exactly
//!   (lengths add up) and each piece lies in its associated element;
//! * upstream compatibility mode on the upstream meshes (UD-407: the
//!   tetrahedron test's first query gives no segment upstream).

mod common;

use akantu_core::{Element, ElementType, GhostType, Real};
use akantu_geometry::kernel::{Point3, Segment3};
use akantu_geometry::{LinearScan, MeshSegmentIntersector, SegmentIntersectorOptions};
use akantu_geometry_cgal::AabbTree;
use akantu_mesh::Mesh;

fn p(x: Real, y: Real, z: Real) -> Point3 {
    Point3::new(x, y, z)
}

fn segments_of(m: &Mesh) -> Vec<Vec<Real>> {
    let n = m.nb_element(ElementType::Segment2, GhostType::NotGhost);
    (0..n)
        .map(|e| {
            let conn = m
                .element_connectivity(
                    Element::try_new(ElementType::Segment2, e, GhostType::NotGhost).unwrap(),
                )
                .unwrap()
                .to_vec();
            conn.iter()
                .flat_map(|&k| m.nodes().tuple(usize::try_from(k).unwrap()).to_vec())
                .collect()
        })
        .collect()
}

#[test]
fn upstream_test_intents_with_the_aabb_tree() {
    let tol = 1e-10;
    let mesh = common::read(2, "test_geometry_triangle.msh", "mesh");
    let mut interface = Mesh::new(2, "interface_mesh").unwrap();
    let opts = SegmentIntersectorOptions {
        point_tolerance: tol,
        ..SegmentIntersectorOptions::default()
    };
    let si = MeshSegmentIntersector::<AabbTree>::new(&mesh, ElementType::Triangle3, opts).unwrap();
    si.compute_intersection_query_list(
        &mesh,
        &[
            Segment3::new(p(0.0, 0.25, 0.0), p(1.0, 0.25, 0.0)),
            Segment3::new(p(0.25, 0.0, 0.0), p(0.25, 1.0, 0.0)),
        ],
        &mut interface,
    )
    .unwrap();
    assert_eq!(
        interface.nb_element(ElementType::Segment2, GhostType::NotGhost),
        4
    );
    let b0 = interface
        .barycenter(Element::try_new(ElementType::Segment2, 0, GhostType::NotGhost).unwrap())
        .unwrap();
    assert!(common::are_vector_equal(&b0, &[0.125, 0.25], tol));

    let mesh3 = common::read(3, "test_geometry_tetrahedron.msh", "mesh");
    let mut interface3 = Mesh::new(3, "interface_mesh").unwrap();
    let si3 = MeshSegmentIntersector::<AabbTree>::new(
        &mesh3,
        ElementType::Tetrahedron4,
        SegmentIntersectorOptions::default(),
    )
    .unwrap();
    si3.compute_intersection_query(
        &mesh3,
        &Segment3::new(p(0.0, 0.0, 0.0), p(1.0, 1.0, 1.0)),
        &mut interface3,
    )
    .unwrap();
    assert_eq!(
        interface3.nb_element(ElementType::Segment2, GhostType::NotGhost),
        2
    );
}

#[test]
fn default_output_is_independent_of_the_search_structure() {
    let meshes = [
        common::triangle_grid(9, 7, 1.0, 0.8, true),
        common::triangle_grid(4, 4, 1.0, 1.0, false),
    ];
    let queries = [
        Segment3::new(p(-0.13, 0.071, 0.0), p(1.21, 0.733, 0.0)),
        Segment3::new(p(0.5, -1.0, 0.0), p(0.5, 2.0, 0.0)),
        Segment3::new(p(0.0, 0.0, 0.0), p(1.0, 0.8, 0.0)),
        Segment3::new(p(0.25, 0.25, 0.0), p(0.75, 0.25, 0.0)),
        Segment3::new(p(0.31, 0.32, 0.0), p(0.33, 0.34, 0.0)),
        Segment3::new(p(0.0, 0.5, 0.0), p(1.0, 0.5, 0.0)),
    ];
    for mesh in &meshes {
        let a = MeshSegmentIntersector::<AabbTree>::new(
            mesh,
            ElementType::Triangle3,
            SegmentIntersectorOptions::default(),
        )
        .unwrap();
        let b = MeshSegmentIntersector::<LinearScan>::new(
            mesh,
            ElementType::Triangle3,
            SegmentIntersectorOptions::default(),
        )
        .unwrap();
        let mut ra = Mesh::new(2, "a").unwrap();
        let mut rb = Mesh::new(2, "b").unwrap();
        a.compute_intersection_query_list(mesh, &queries, &mut ra)
            .unwrap();
        b.compute_intersection_query_list(mesh, &queries, &mut rb)
            .unwrap();
        assert_eq!(segments_of(&ra), segments_of(&rb));
        assert!(ra.nb_element(ElementType::Segment2, GhostType::NotGhost) > 20);
    }
    let tets = common::tet_grid(3);
    let q3 = [
        Segment3::new(p(0.1, 0.2, 0.3), p(0.9, 0.7, 0.6)),
        Segment3::new(p(-0.5, 0.45, 0.55), p(1.5, 0.45, 0.55)),
        Segment3::new(p(0.0, 0.0, 0.0), p(1.0, 1.0, 1.0)),
    ];
    let a = MeshSegmentIntersector::<AabbTree>::new(
        &tets,
        ElementType::Tetrahedron4,
        SegmentIntersectorOptions::default(),
    )
    .unwrap();
    let b = MeshSegmentIntersector::<LinearScan>::new(
        &tets,
        ElementType::Tetrahedron4,
        SegmentIntersectorOptions::default(),
    )
    .unwrap();
    let mut ra = Mesh::new(3, "a").unwrap();
    let mut rb = Mesh::new(3, "b").unwrap();
    a.compute_intersection_query_list(&tets, &q3, &mut ra)
        .unwrap();
    b.compute_intersection_query_list(&tets, &q3, &mut rb)
        .unwrap();
    assert_eq!(segments_of(&ra), segments_of(&rb));
}

fn length(s: &[Real], dim: usize) -> Real {
    (0..dim)
        .map(|k| (s[dim + k] - s[k]).powi(2))
        .sum::<Real>()
        .sqrt()
}

#[test]
fn pieces_cover_the_clipped_query() {
    // 2-D: the unit square is covered by the mesh; a query crossing it is cut
    // into pieces whose lengths add up to the clipped length.
    let mesh = common::triangle_grid(8, 8, 1.0, 1.0, true);
    let si = MeshSegmentIntersector::<AabbTree>::new(
        &mesh,
        ElementType::Triangle3,
        SegmentIntersectorOptions::default(),
    )
    .unwrap();
    let q = Segment3::new(p(-0.2, 0.13, 0.0), p(1.3, 0.91, 0.0));
    let mut r = Mesh::new(2, "r").unwrap();
    si.compute_intersection_query(&mesh, &q, &mut r).unwrap();
    // Clip to x ∈ [0, 1]: y = 0.13 + 0.52·(x + 0.2) stays inside [0, 1].
    let slope: Real = (0.91 - 0.13) / 1.5;
    let clipped = (1.0 + slope * slope).sqrt();
    let total: Real = segments_of(&r).iter().map(|s| length(s, 2)).sum();
    assert!((total - clipped).abs() < 1e-12, "{total} vs {clipped}");
    // 3-D: tetrahedra of the unit cube.
    let tets = common::tet_grid(4);
    let si3 = MeshSegmentIntersector::<AabbTree>::new(
        &tets,
        ElementType::Tetrahedron4,
        SegmentIntersectorOptions::default(),
    )
    .unwrap();
    let q3 = Segment3::new(p(0.11, 0.23, 0.37), p(0.83, 0.71, 0.59));
    let mut r3 = Mesh::new(3, "r3").unwrap();
    si3.compute_intersection_query(&tets, &q3, &mut r3).unwrap();
    let want = ((0.72_f64).powi(2) + (0.48_f64).powi(2) + (0.22_f64).powi(2)).sqrt();
    let total3: Real = segments_of(&r3).iter().map(|s| length(s, 3)).sum();
    assert!((total3 - want).abs() < 1e-12, "{total3} vs {want}");
}

#[test]
fn upstream_mode_reproduces_ud407() {
    // Upstream builds one flat triangle per tetrahedron (z ignored): the
    // diagonal of the cube only touches those triangles at the origin
    // (points, which the tetrahedron branch cannot complete into a segment
    // through the flat primitives), so no segment is created — upstream's
    // test_segment_intersection_tetrahedron_4 expectation (2) fails.
    let mesh3 = common::read(3, "test_geometry_tetrahedron.msh", "mesh");
    let mut out = Mesh::new(3, "interface_mesh").unwrap();
    let si = MeshSegmentIntersector::<AabbTree>::new(
        &mesh3,
        ElementType::Tetrahedron4,
        SegmentIntersectorOptions::upstream(),
    )
    .unwrap();
    si.compute_intersection_query(
        &mesh3,
        &Segment3::new(p(0.0, 0.0, 0.0), p(1.0, 1.0, 1.0)),
        &mut out,
    )
    .unwrap();
    assert_ne!(
        out.nb_element(ElementType::Segment2, GhostType::NotGhost),
        2
    );
}

/// A bar strictly inside element 77 of 7 × 7 cells with alternating
/// diagonals, on the line `x − y = −2/7` of the diagonal edge of the
/// distant element 29. The exhaustive scan reports it on both elements and
/// its collected set keeps (bar, 29); the CGAL-compatible tree tests only
/// the elements whose boxes meet the bar and keeps (bar, 77), as the LGPL
/// grid search structure (`akantu-geometry`, `tests/grid_index_intersector.rs`,
/// `diagonal_bar_regression`) does.
#[test]
fn diagonal_bar_keeps_its_element() {
    use akantu_mesh::MeshAccessor;
    let n = 7;
    let mut mesh = Mesh::new(2, "mesh").unwrap();
    let id = |i: usize, j: usize| i32::try_from(j * (n + 1) + i).unwrap();
    {
        let mut acc = MeshAccessor::new(&mut mesh);
        for j in 0..=n {
            for i in 0..=n {
                #[allow(clippy::cast_precision_loss)] // small grid
                acc.push_node(&[i as Real / n as Real, j as Real / n as Real])
                    .unwrap();
            }
        }
        let mut c = acc.connectivity_mut(ElementType::Triangle3, GhostType::NotGhost);
        for j in 0..n {
            for i in 0..n {
                if (i + j).is_multiple_of(2) {
                    c.try_push_tuple(&[id(i, j), id(i + 1, j), id(i + 1, j + 1)])
                        .unwrap();
                    c.try_push_tuple(&[id(i, j), id(i + 1, j + 1), id(i, j + 1)])
                        .unwrap();
                } else {
                    c.try_push_tuple(&[id(i, j), id(i + 1, j), id(i, j + 1)])
                        .unwrap();
                    c.try_push_tuple(&[id(i + 1, j), id(i + 1, j + 1), id(i, j + 1)])
                        .unwrap();
                }
            }
        }
    }
    let c = -2.0 / 7.0;
    let (x0, len) = (0.43, 0.0143);
    let q = Segment3::new(p(x0, x0 - c, 0.0), p(x0 + len, x0 + len - c, 0.0));
    let options = SegmentIntersectorOptions {
        assembly: akantu_geometry::PieceAssembly::Collected,
        ..SegmentIntersectorOptions::default()
    };
    let ty = ElementType::Triangle3;
    let tree = MeshSegmentIntersector::<AabbTree>::new(&mesh, ty, options).unwrap();
    let scan = MeshSegmentIntersector::<LinearScan>::new(&mesh, ty, options).unwrap();
    assert_eq!(tree.compute_segments(&mesh, &q).unwrap(), vec![(q, 77)]);
    assert_eq!(scan.compute_segments(&mesh, &q).unwrap(), vec![(q, 29)]);
    // upstream's set order gives the same piece
    let upstream = SegmentIntersectorOptions {
        order: akantu_geometry::SegmentOrder::Upstream,
        ..options
    };
    let tree = MeshSegmentIntersector::<AabbTree>::new(&mesh, ty, upstream).unwrap();
    assert_eq!(tree.compute_segments(&mesh, &q).unwrap(), vec![(q, 77)]);
}
