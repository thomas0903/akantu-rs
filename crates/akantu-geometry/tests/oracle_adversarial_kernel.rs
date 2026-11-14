// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): differential test of the CGAL LGPL kernel
// calls made by src/geometry (Simple_cartesian<double>) on
// adversarial inputs, against the oracle fixture
// fixtures/geometry/adversarial/kernel_adv
// (oracle/probes/geometry/geometry_adversarial_probe.cc).

//! `intersection(Segment_3, Triangle_3)`, `do_intersect(Segment_3, Bbox_3)`,
//! `do_intersect(Ray_3, Triangle_3)` and the tetrahedron side predicates on
//! 4,000 adversarial samples (mixed magnitudes, scales `2^±500` and
//! `1e±150`, exactly coplanar and collinear inputs, slivers, nearly
//! parallel segments).
//!
//! Comparison: through the validation harness ([`DiffTest`]) with the
//! exact rule of `fixtures/geometry/tolerances.json`, plus the sign of zero
//! for every expected zero (the geometry rule is bitwise, signs of zero
//! included; the harness's exact rule treats `±0` as equal). NaN entries
//! are left entirely to the harness's exact rule: `coords` holds the
//! oracle's NaN from overflowing constructions (the case's manifest notes,
//! D-046 (4)); on x86_64 (the platform of the fixture) they are bit-identical
//! to the port's, on aarch64 they differ only in the sign bit of the default
//! NaN, which D-046's comparator accepts (`docs/NUMERICAL_VALIDATION.md`).
//! No local NaN handling.

use akantu_geometry::intersections::{
    TriangleSegmentIntersection, bbox_segment_do_intersect, ray_triangle_do_intersect,
    triangle_segment_intersection,
};
use akantu_geometry::kernel::{Point3, Ray3, Segment3, Tetrahedron3, Triangle3};
use akantu_validation::harness::DiffTest;

fn p(v: &[f64]) -> Point3 {
    Point3::new(v[0], v[1], v[2])
}

/// Every expected `±0` must be met with the same sign (explicit check; since
/// D-051 the harness's exact rule is sign-strict too).
fn assert_zero_signs(dt: &DiffTest, quantity: &str, actual: &[f64]) {
    let expected = dt.reference_f64(quantity).unwrap().data;
    assert_eq!(expected.len(), actual.len(), "{quantity}: length");
    for (i, (e, a)) in expected.iter().zip(actual).enumerate() {
        if *e == 0.0 {
            assert_eq!(e.to_bits(), a.to_bits(), "{quantity}[{i}]: sign of zero");
        }
    }
}

#[test]
fn kernel_adversarial_matches_the_oracle() {
    let mut dt = DiffTest::new(
        "kernel_adversarial_matches_the_oracle",
        "geometry/adversarial/kernel_adv",
    )
    .expect("fixture geometry/adversarial/kernel_adv");
    let fx = dt.fixture();
    let tri = fx.load_f64("tri").unwrap().data;
    let seg = fx.load_f64("seg").unwrap().data;
    let tet_d = fx.load_f64("tet_d").unwrap().data;
    let tet_x = fx.load_f64("tet_x").unwrap().data;
    let n = seg.len() / 6;
    assert_eq!(n, 4000);
    let (mut kind, mut coords, mut bbox, mut ray, mut bounded, mut boundary) = (
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
    );
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
        kind.push(k);
        coords.extend_from_slice(&c);
        bbox.push(i64::from(bbox_segment_do_intersect(&s, &t.bbox())));
        ray.push(i64::from(ray_triangle_do_intersect(
            &t,
            &Ray3::new(s.source, s.target),
        )));
        let th = Tetrahedron3::new(t.vertex(0), t.vertex(1), t.vertex(2), p(&tet_d[3 * i..]));
        let x = p(&tet_x[3 * i..]);
        bounded.push(i64::from(th.has_on_bounded_side(x)));
        boundary.push(i64::from(th.has_on_boundary(x)));
    }
    dt.check_i64("kind", &kind);
    dt.check_f64("coords", &coords);
    dt.check_i64("bbox_seg", &bbox);
    dt.check_i64("ray_tri", &ray);
    dt.check_i64("tet_bounded", &bounded);
    dt.check_i64("tet_boundary", &boundary);
    assert_zero_signs(&dt, "coords", &coords);
    let nan = coords.iter().filter(|x| x.is_nan()).count();
    eprintln!("kernel_adv: {n} samples, {nan} NaN coordinates (overflow, D-046 notes)");
    dt.assert_pass();
}
