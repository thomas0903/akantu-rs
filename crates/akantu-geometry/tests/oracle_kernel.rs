// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): differential test of the CGAL LGPL kernel
// calls made by src/geometry (Simple_cartesian<double>), against the oracle
// fixture fixtures/geometry/kernel/triangle_segment (oracle/probes/geometry).

//! Bitwise comparison with the CGAL-enabled oracle (x86_64, strict) of
//! `intersection(Segment_3, Triangle_3)` (type and coordinates, including
//! coplanar, collinear and degenerate configurations), `do_intersect(Segment_3,
//! Bbox_3)`, `do_intersect(Ray_3, Triangle_3)` and the tetrahedron side
//! predicates. Every value must match exactly (`f64::to_bits`): the port
//! evaluates CGAL's formulas in CGAL's order.

use akantu_geometry::intersections::{
    TriangleSegmentIntersection, bbox_segment_do_intersect, ray_triangle_do_intersect,
    triangle_segment_intersection,
};
use akantu_geometry::kernel::{Point3, Ray3, Segment3, Tetrahedron3, Triangle3};
use akantu_validation::Fixture;

fn p(v: &[f64]) -> Point3 {
    Point3::new(v[0], v[1], v[2])
}

#[test]
fn kernel_triangle_segment_matches_the_oracle() {
    let fx = Fixture::open_by_id("geometry/kernel/triangle_segment").expect("fixture");
    fx.require_golden().unwrap();
    let tri = fx.load_f64("input/triangles").unwrap().data;
    let seg = fx.load_f64("input/segments").unwrap().data;
    let tet = fx.load_f64("input/tetrahedra").unwrap().data;
    let pts = fx.load_f64("input/points").unwrap().data;
    let kind = fx.load_i64("intersection/kind").unwrap().data;
    let coords = fx.load_f64("intersection/coords").unwrap().data;
    let bbox = fx.load_i64("bbox_segment").unwrap().data;
    let ray = fx.load_i64("ray_triangle").unwrap().data;
    let bounded = fx.load_i64("tetrahedron/bounded").unwrap().data;
    let boundary = fx.load_i64("tetrahedron/boundary").unwrap().data;
    let n = kind.len();
    assert!(n >= 1000);
    let mut counts = [0usize; 4];
    for i in 0..n {
        let t = Triangle3::new(p(&tri[9 * i..]), p(&tri[9 * i + 3..]), p(&tri[9 * i + 6..]));
        let s = Segment3::new(p(&seg[6 * i..]), p(&seg[6 * i + 3..]));
        let (k, c): (i64, [f64; 6]) = match triangle_segment_intersection(&t, &s) {
            Err(_) => (-1, [0.0; 6]),
            Ok(None) => (0, [0.0; 6]),
            Ok(Some(TriangleSegmentIntersection::Point(q))) => (1, [q.x, q.y, q.z, 0.0, 0.0, 0.0]),
            Ok(Some(TriangleSegmentIntersection::Segment(g))) => (
                2,
                [
                    g.source.x, g.source.y, g.source.z, g.target.x, g.target.y, g.target.z,
                ],
            ),
        };
        assert_eq!(k, kind[i], "sample {i}: kind ({t:?}, {s:?})");
        for j in 0..6 {
            // D-046 (bit identity, or default NaNs of either sign)
            assert!(
                akantu_validation::exact_eq(coords[6 * i + j], c[j]),
                "sample {i} coord {j}: {} vs {}",
                c[j],
                coords[6 * i + j]
            );
        }
        counts[usize::try_from(k + 1).unwrap()] += 1;
        assert_eq!(
            i64::from(bbox_segment_do_intersect(&s, &t.bbox())),
            bbox[i],
            "sample {i}: bbox"
        );
        assert_eq!(
            i64::from(ray_triangle_do_intersect(
                &t,
                &Ray3::new(s.source, s.target)
            )),
            ray[i],
            "sample {i}: ray"
        );
        let th = Tetrahedron3::new(
            p(&tet[12 * i..]),
            p(&tet[12 * i + 3..]),
            p(&tet[12 * i + 6..]),
            p(&tet[12 * i + 9..]),
        );
        let x = p(&pts[3 * i..]);
        assert_eq!(
            i64::from(th.has_on_bounded_side(x)),
            bounded[i],
            "sample {i}: bounded"
        );
        assert_eq!(
            i64::from(th.has_on_boundary(x)),
            boundary[i],
            "sample {i}: boundary"
        );
    }
    // The fixture exercises every result type.
    assert!(
        counts[1] > 0 && counts[2] > 0 && counts[3] > 0,
        "{counts:?}"
    );
}
