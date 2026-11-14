// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/geometry/mesh_segment_intersector_tmpl.hh
// and tree_type_helper.hh (the CGAL intersections called on
// `cgal::Cartesian` objects).
// Derived from CGAL 5.6.2, LGPL-3.0-or-later (CGAL's Intersections_3
// package):
//   Intersections_3/include/CGAL/Intersections_3/internal/
//     Segment_3_Triangle_3_intersection.h
//       Copyright (c) 2009 GeometryFactory (France), INRIA Sophia-Antipolis
//       (France). Author(s): Laurent Rineau, Stephane Tayeb
//     Line_3_Plane_3_intersection.h
//       Copyright (c) 2008 INRIA Sophia-Antipolis (France), ETH Zurich
//       (Switzerland).
//       Copyright (c) 2010, 2014 GeometryFactory Sarl (France).
//       Author(s): Geert-Jan Giezeman, Sylvain Pion
//     Bbox_3_Segment_3_do_intersect.h
//       Copyright (c) 2012 GeometryFactory Sarl (France).
//       Author(s): Laurent Rineau
//     Bbox_3_Ray_3_do_intersect.h
//       Copyright (c) 2008 ETH Zurich (Switzerland)
//       Copyright (c) 2008-2009 INRIA Sophia-Antipolis (France)
//       Author(s): Camille Wormser, Jane Tournois, Pierre Alliez,
//       Stephane Tayeb
//     Ray_3_Triangle_3_do_intersect.h
//       Copyright (c) 2003 INRIA Sophia-Antipolis (France).
//       Author(s): Philippe Guigue
// No GPL-licensed CGAL code is used in this file (D-042 (3), D-022).

//! Intersections of `Simple_cartesian<double>` objects used by upstream's
//! AABB-tree queries, with CGAL's case analysis and arithmetic.
//!
//! Every function follows the CGAL code path on all inputs (the oracle's
//! `-DNDEBUG` build disables CGAL preconditions, e.g. on degenerate
//! triangles); `Err` is returned only where CGAL calls `CGAL_error()`
//! (always active: it throws).

use akantu_core::{Error, Real, Result};

use crate::kernel::{
    Bbox3, Line3, Plane3, Point3, Ray3, Segment3, Sign, Triangle3, Vector3,
    collinear_are_ordered_along_line, coplanar_orientation3, orientation,
};

/// Result of `intersection(Segment_3, Triangle_3)`
/// (`boost::variant<Point_3, Segment_3>`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TriangleSegmentIntersection {
    /// A point.
    Point(Point3),
    /// A segment (coplanar overlap).
    Segment(Segment3),
}

fn cgal_error(what: &str) -> Error {
    Error::Numerical(format!("CGAL_error() in {what} (\"should not happen\")"))
}

/// `t3s3_intersection_coplanar_aux(p, q, a, b)`: intersection point of the
/// coplanar segments `[p, q]` and `[a, b]`.
fn coplanar_aux_point(p: Point3, q: Point3, a: Point3, b: Point3) -> Point3 {
    let pq = Vector3::from_points(p, q);
    let ab = Vector3::from_points(a, b);
    let pa = Vector3::from_points(p, a);
    let pa_ab = pa.cross(ab);
    let pq_ab = pq.cross(ab);
    let t = pa_ab.dot(pq_ab) / pq_ab.squared_length();
    p.translate(pq.scale(t))
}

/// `t3s3_intersection_coplanar_aux(a, b, c, p, q, negative_side)`.
fn coplanar_aux(
    a: Point3,
    b: Point3,
    c: Point3,
    p: Point3,
    q: Point3,
    negative_side: bool,
) -> Option<TriangleSegmentIntersection> {
    let bcq = coplanar_orientation3(b, c, q);
    let cap = coplanar_orientation3(c, a, p);
    if bcq == Sign::Negative || cap == Sign::Negative {
        None
    } else if bcq == Sign::Zero {
        Some(TriangleSegmentIntersection::Point(q))
    } else if cap == Sign::Zero {
        Some(TriangleSegmentIntersection::Point(p))
    } else {
        let mut p_side = p;
        if coplanar_orientation3(b, c, p) == Sign::Negative {
            p_side = coplanar_aux_point(p, q, b, c);
        }
        let mut q_side = q;
        if coplanar_orientation3(c, a, q) == Sign::Negative {
            q_side = coplanar_aux_point(p, q, c, a);
        }
        Some(TriangleSegmentIntersection::Segment(if negative_side {
            Segment3::new(p_side, q_side)
        } else {
            Segment3::new(q_side, p_side)
        }))
    }
}

/// `t3s3_intersection_collinear_aux(a, b, p, q)`.
fn collinear_aux(
    a: Point3,
    b: Point3,
    p: Point3,
    q: Point3,
) -> Option<TriangleSegmentIntersection> {
    use TriangleSegmentIntersection::{Point, Segment};
    if collinear_are_ordered_along_line(p, a, b) {
        if collinear_are_ordered_along_line(a, b, q) {
            return Some(Segment(Segment3::new(a, b)));
        }
        if collinear_are_ordered_along_line(q, a, b) {
            return if a == q { Some(Point(a)) } else { None };
        }
        return Some(Segment(Segment3::new(a, q)));
    }
    if collinear_are_ordered_along_line(p, b, q) {
        return if p == b {
            Some(Point(p))
        } else {
            Some(Segment(Segment3::new(p, b)))
        };
    }
    if collinear_are_ordered_along_line(a, b, p) {
        return if p == b { Some(Point(p)) } else { None };
    }
    Some(Segment(Segment3::new(p, q)))
}

/// `intersection_coplanar(Triangle_3, Segment_3)`.
fn intersection_coplanar(
    t: &Triangle3,
    s: &Segment3,
) -> Result<Option<TriangleSegmentIntersection>> {
    use Sign::{Negative as N, Positive as P, Zero as Z};
    use TriangleSegmentIntersection::Point;
    let (p, q) = (s.point(0), s.point(1));
    let (aa, bb, cc) = (t.vertex(0), t.vertex(1), t.vertex(2));
    let (a, b, c) = if coplanar_orientation3(aa, bb, cc) == P {
        (aa, bb, cc)
    } else {
        (aa, cc, bb)
    };
    let pqa = coplanar_orientation3(p, q, a);
    let pqb = coplanar_orientation3(p, q, b);
    let pqc = coplanar_orientation3(p, q, c);
    let inside = |x: Point3| {
        if collinear_are_ordered_along_line(p, x, q) {
            Some(Point(x))
        } else {
            None
        }
    };
    Ok(match (pqa, pqb, pqc) {
        (P, P, P) => None,
        (P, P, N) => coplanar_aux(a, b, c, p, q, true),
        (P, P, Z) => inside(c),
        (P, N, P) => coplanar_aux(c, a, b, p, q, true),
        (P, N, _) => coplanar_aux(b, c, a, q, p, false),
        (P, Z, P) => inside(b),
        (P, Z, N) => coplanar_aux(b, c, a, q, p, false),
        (P, Z, Z) => collinear_aux(b, c, p, q),
        (N, P, P) => coplanar_aux(b, c, a, p, q, true),
        (N, P, _) => coplanar_aux(c, a, b, q, p, false),
        (N, N, P) => coplanar_aux(a, b, c, q, p, false),
        (N, N, N) => None,
        (N, N, Z) => inside(c),
        (N, Z, P) => coplanar_aux(b, c, a, p, q, true),
        (N, Z, N) => inside(b),
        (N, Z, Z) => collinear_aux(c, b, p, q),
        (Z, P, P) => inside(a),
        (Z, P, N) => coplanar_aux(c, a, b, q, p, false),
        (Z, P, Z) => collinear_aux(c, a, p, q),
        (Z, N, P) => coplanar_aux(c, a, b, p, q, true),
        (Z, N, N) => inside(a),
        (Z, N, Z) => collinear_aux(a, c, p, q),
        (Z, Z, P) => collinear_aux(a, b, p, q),
        (Z, Z, N) => collinear_aux(b, a, p, q),
        (Z, Z, Z) => return Err(cgal_error("intersection_coplanar(Triangle_3, Segment_3)")),
    })
}

/// `intersection(Line_3, Plane_3)` restricted to the point case
/// (`Line_3_Plane_3_intersection.h`, Cartesian: `hw = 1`): `None` when the
/// line is parallel to (or contained in) the plane.
#[must_use]
pub fn line_plane_point(plane: &Plane3, line: &Line3) -> Option<Point3> {
    let pt = line.point;
    let dir = line.vector;
    let num = plane.a * pt.x + plane.b * pt.y + plane.c * pt.z + plane.d;
    let den = plane.a * dir.x + plane.b * dir.y + plane.c * dir.z;
    if den == 0.0 {
        return None;
    }
    Some(Point3::from_homogeneous(
        den * pt.x - num * dir.x,
        den * pt.y - num * dir.y,
        den * pt.z - num * dir.z,
        den,
    ))
}

/// `CGAL::intersection(Segment_3, Triangle_3)` in
/// `Simple_cartesian<double>` (`Segment_3_Triangle_3_intersection.h`).
///
/// # Errors
/// A `CGAL_error()` branch (segment and degenerate triangle all collinear).
pub fn triangle_segment_intersection(
    t: &Triangle3,
    s: &Segment3,
) -> Result<Option<TriangleSegmentIntersection>> {
    use Sign::{Negative as N, Positive as P, Zero as Z};
    use TriangleSegmentIntersection::Point;
    let (a, b, c) = (t.vertex(0), t.vertex(1), t.vertex(2));
    let (p, q) = (s.point(0), s.point(1));
    let abcp = orientation(a, b, c, p);
    let abcq = orientation(a, b, c, q);
    let edges_ok = |u: Point3, v: Point3| {
        orientation(u, v, a, b) != P && orientation(u, v, b, c) != P && orientation(u, v, c, a) != P
    };
    let crossing = || line_plane_point(&t.supporting_plane(), &s.supporting_line()).map(Point);
    Ok(match (abcp, abcq) {
        (P, P) | (N, N) => None,
        (P, N) => {
            if edges_ok(p, q) {
                crossing()
            } else {
                None
            }
        }
        (P, Z) => {
            if edges_ok(p, q) {
                Some(Point(q))
            } else {
                None
            }
        }
        (N, P) => {
            if edges_ok(q, p) {
                crossing()
            } else {
                None
            }
        }
        (N, Z) => {
            if edges_ok(q, p) {
                Some(Point(q))
            } else {
                None
            }
        }
        (Z, P) => {
            if edges_ok(q, p) {
                Some(Point(p))
            } else {
                None
            }
        }
        (Z, N) => {
            if edges_ok(p, q) {
                Some(Point(p))
            } else {
                None
            }
        }
        (Z, Z) => intersection_coplanar(t, s)?,
    })
}

/// `do_intersect_coplanar(Triangle_3, Ray_3)` (empty visitor).
fn ray_triangle_coplanar(t: &Triangle3, r: &Ray3) -> bool {
    use Sign::{Negative as N, Positive as P, Zero as Z};
    let (p, q) = (r.source, r.second);
    let (aa, bb, cc) = (t.vertex(0), t.vertex(1), t.vertex(2));
    let (a, b, c) = if coplanar_orientation3(aa, bb, cc) == P {
        (aa, bb, cc)
    } else {
        (aa, cc, bb)
    };
    let pqa = coplanar_orientation3(p, q, a);
    let pqb = coplanar_orientation3(p, q, b);
    let pqc = coplanar_orientation3(p, q, c);
    let ac = || coplanar_orientation3(a, c, p) != P;
    let cb = || coplanar_orientation3(c, b, p) != P;
    let ba = || coplanar_orientation3(b, a, p) != P;
    match (pqa, pqb) {
        (P, P) => pqc != P && ac(),
        (P, N | Z) => {
            if pqc == P {
                cb()
            } else {
                ac()
            }
        }
        (N, P) => ba(),
        (N, N) => pqc != N && cb(),
        (N, Z) => ba(),
        (Z, P) => ba(),
        (Z, N) => {
            if pqc == N {
                ac()
            } else {
                cb()
            }
        }
        (Z, Z) => {
            if pqc == P {
                cb()
            } else {
                ac()
            }
        }
    }
}

/// `CGAL::do_intersect(Ray_3, Triangle_3)` in `Simple_cartesian<double>`
/// (`Ray_3_Triangle_3_do_intersect.h`, empty visitor).
#[must_use]
pub fn ray_triangle_do_intersect(t: &Triangle3, r: &Ray3) -> bool {
    use Sign::{Negative as N, Positive as P, Zero as Z};
    let (a, b, c) = (t.vertex(0), t.vertex(1), t.vertex(2));
    let (p, q) = (r.source, r.second);
    let shifted = Ray3::from_vector(a, r.to_vector());
    let ray_direction = orientation(a, b, c, shifted.second);
    if ray_direction == Z {
        return orientation(a, b, c, p) == Z && ray_triangle_coplanar(t, r);
    }
    let abcp = orientation(a, b, c, p);
    let edges_ok = |u: Point3, v: Point3| {
        orientation(u, v, a, b) != P && orientation(u, v, b, c) != P && orientation(u, v, c, a) != P
    };
    match (abcp, ray_direction) {
        (P, N) | (Z, N) => edges_ok(p, q),
        (N, P) | (Z, P) => edges_ok(q, p),
        // (P, P), (N, N); `ray_direction` is never zero here.
        _ => false,
    }
}

/// `Do_intersect_bbox_segment_aux_is_greater<double, …, false>` and the
/// body of `do_intersect_bbox_segment_aux<double, double, bounded_0,
/// bounded_1, false>` (`Bbox_3_Segment_3_do_intersect.h`).
#[allow(clippy::too_many_lines, clippy::similar_names)] // faithful port of one CGAL function
fn bbox_segment_aux(p: Point3, q: Point3, bb: &Bbox3, bounded_0: bool, bounded_1: bool) -> bool {
    let (px, py, pz) = (p.x, p.y, p.z);
    let (qx, qy, qz) = (q.x, q.y, q.z);
    let [bxmin, bymin, bzmin] = bb.min;
    let [bxmax, bymax, bzmax] = bb.max;
    if (px >= bxmin && px <= bxmax && py >= bymin && py <= bymax && pz >= bzmin && pz <= bzmax)
        || (qx >= bxmin && qx <= bxmax && qy >= bymin && qy <= bymax && qz >= bzmin && qz <= bzmax)
    {
        return true;
    }
    let larger = |a: Real, b: Real| Sign::compare(a, b) == Sign::Positive;
    let smaller = |a: Real, b: Real| Sign::compare(a, b) == Sign::Negative;
    // `(CGAL::max)(0, t)` = std::max: `(0 < t) ? t : 0`.
    let max0 = |t: Real| if 0.0 < t { t } else { 0.0 };

    // x
    let (mut dmin, mut tmin, mut tmax, mut dmax);
    if qx >= px {
        if bounded_0 && larger(px, bxmax) {
            return false;
        }
        if bounded_1 && smaller(qx, bxmin) {
            return false;
        }
        if bounded_1 && bxmax > qx {
            tmax = 1.0;
            dmax = 1.0;
        } else {
            tmax = bxmax - px;
            dmax = qx - px;
        }
        tmin = bxmin - px;
        dmin = qx - px;
    } else {
        if bounded_1 && larger(qx, bxmax) {
            return false;
        }
        if bounded_0 && smaller(px, bxmin) {
            return false;
        }
        if bounded_1 && smaller(bxmin, qx) {
            tmax = 1.0;
            dmax = 1.0;
        } else {
            tmax = px - bxmin;
            dmax = px - qx;
        }
        tmin = px - bxmax;
        dmin = px - qx;
    }
    if bounded_0 {
        tmin = max0(tmin);
    }
    if px == qx && !(bounded_0 && bounded_1) && (larger(px, bxmax) || smaller(px, bxmin)) {
        return false;
    }

    // y
    let (dymin, tymin, tymax, dymax);
    if qy >= py {
        if bounded_0 && larger(py, bymax) {
            return false;
        }
        if bounded_1 && smaller(qy, bymin) {
            return false;
        }
        if bounded_1 && larger(bymax, qy) {
            tymax = 1.0;
            dymax = 1.0;
        } else {
            tymax = bymax - py;
            dymax = qy - py;
        }
        tymin = bymin - py;
        dymin = qy - py;
    } else {
        if bounded_1 && larger(qy, bymax) {
            return false;
        }
        if bounded_0 && smaller(py, bymin) {
            return false;
        }
        if bounded_1 && smaller(bymin, qy) {
            tymax = 1.0;
            dymax = 1.0;
        } else {
            tymax = py - bymin;
            dymax = py - qy;
        }
        tymin = py - bymax;
        dymin = py - qy;
    }
    let tymin = if bounded_0 { max0(tymin) } else { tymin };
    if py == qy && !(bounded_0 && bounded_1) && (py > bymax || smaller(py, bymin)) {
        return false;
    }

    // z
    let (dzmin, tzmin, tzmax, dzmax);
    if qz >= pz {
        if bounded_0 && larger(pz, bzmax) {
            return false;
        }
        if bounded_1 && smaller(qz, bzmin) {
            return false;
        }
        if bounded_1 && larger(bzmax, qz) {
            tzmax = 1.0;
            dzmax = 1.0;
        } else {
            tzmax = bzmax - pz;
            dzmax = qz - pz;
        }
        tzmin = bzmin - pz;
        dzmin = qz - pz;
    } else {
        if bounded_1 && larger(qz, bzmax) {
            return false;
        }
        if bounded_0 && smaller(pz, bzmin) {
            return false;
        }
        if bounded_1 && smaller(bzmin, qz) {
            tzmax = 1.0;
            dzmax = 1.0;
        } else {
            tzmax = pz - bzmin;
            dzmax = pz - qz;
        }
        tzmin = pz - bzmax;
        dzmin = pz - qz;
    }
    let tzmin = if bounded_0 { max0(tzmin) } else { tzmin };
    if pz == qz && !(bounded_0 && bounded_1) && (larger(pz, bzmax) || smaller(pz, bzmin)) {
        return false;
    }

    // Without static filters `is_greater(a, b)` is `a > b`, never
    // uncertain, and the overflow/underflow guards are inactive.
    if py != qy && px != qx {
        if dymax * tmin > dmin * tymax {
            return false;
        }
        if dmax * tymin > dymin * tmax {
            return false;
        }
    }
    if px == qx || (py != qy && dmin * tymin > dymin * tmin) {
        tmin = tymin;
        dmin = dymin;
    }
    if px == qx || (py != qy && dymax * tmax > dmax * tymax) {
        tmax = tymax;
        dmax = dymax;
    }
    if (px != qx || py != qy) && pz != qz {
        if dzmax * tmin > dmin * tzmax {
            return false;
        }
        if dmax * tzmin > dzmin * tmax {
            return false;
        }
    }
    true
}

/// `CGAL::do_intersect(Segment_3, Bbox_3)` (`bounded_0 = bounded_1 =
/// true`).
#[must_use]
pub fn bbox_segment_do_intersect(s: &Segment3, bb: &Bbox3) -> bool {
    bbox_segment_aux(s.source, s.target, bb, true, true)
}

/// `CGAL::do_intersect(Ray_3, Bbox_3)` (`bounded_0 = true`, `bounded_1 =
/// false`).
#[must_use]
pub fn bbox_ray_do_intersect(r: &Ray3, bb: &Bbox3) -> bool {
    bbox_segment_aux(r.source, r.second, bb, true, false)
}

#[cfg(test)]
mod tests {
    use super::TriangleSegmentIntersection::{Point, Segment};
    use super::*;

    fn p(x: Real, y: Real, z: Real) -> Point3 {
        Point3::new(x, y, z)
    }

    #[test]
    fn crossing_segment_meets_the_plane() {
        let t = Triangle3::new(p(0.0, 0.0, 0.0), p(1.0, 0.0, 0.0), p(0.0, 1.0, 0.0));
        let s = Segment3::new(p(0.25, 0.25, -1.0), p(0.25, 0.25, 1.0));
        assert_eq!(
            triangle_segment_intersection(&t, &s).unwrap(),
            Some(Point(p(0.25, 0.25, 0.0)))
        );
        let miss = Segment3::new(p(2.0, 2.0, -1.0), p(2.0, 2.0, 1.0));
        assert_eq!(triangle_segment_intersection(&t, &miss).unwrap(), None);
    }

    #[test]
    fn coplanar_segment_is_clipped() {
        // Upstream test_geometry_intersection: triangle (0,0)(1,0)(.5,.5)
        // and the line y = 0.25 → [(0.25,0.25), (0.75,0.25)].
        let t = Triangle3::new(p(0.0, 0.0, 0.0), p(1.0, 0.0, 0.0), p(0.5, 0.5, 0.0));
        let s = Segment3::new(p(0.0, 0.25, 0.0), p(1.0, 0.25, 0.0));
        match triangle_segment_intersection(&t, &s).unwrap() {
            Some(Segment(seg)) => {
                let (a, b) = (seg.min(), seg.max());
                assert_eq!(a, p(0.25, 0.25, 0.0));
                assert_eq!(b, p(0.75, 0.25, 0.0));
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn bbox_and_ray_queries() {
        let bb = Bbox3 {
            min: [0.0, 0.0, 0.0],
            max: [1.0, 1.0, 0.0],
        };
        assert!(bbox_segment_do_intersect(
            &Segment3::new(p(-1.0, 0.5, 0.0), p(2.0, 0.5, 0.0)),
            &bb
        ));
        assert!(!bbox_segment_do_intersect(
            &Segment3::new(p(-1.0, 2.0, 0.0), p(2.0, 2.0, 0.0)),
            &bb
        ));
        assert!(!bbox_segment_do_intersect(
            &Segment3::new(p(-1.0, 0.5, 0.0), p(-0.5, 0.5, 0.0)),
            &bb
        ));
        assert!(bbox_ray_do_intersect(
            &Ray3::new(p(-1.0, 0.5, 0.0), p(-0.5, 0.5, 0.0)),
            &bb
        ));
        assert!(!bbox_ray_do_intersect(
            &Ray3::new(p(-1.0, 0.5, 0.0), p(-2.0, 0.5, 0.0)),
            &bb
        ));
        let t = Triangle3::new(p(0.0, 0.0, 0.0), p(1.0, 0.0, 0.0), p(0.0, 1.0, 0.0));
        assert!(ray_triangle_do_intersect(
            &t,
            &Ray3::new(p(0.2, 0.2, 0.0), p(0.3, 0.3, 0.0))
        ));
        assert!(ray_triangle_do_intersect(
            &t,
            &Ray3::new(p(0.2, 0.2, 1.0), p(0.2, 0.2, 0.5))
        ));
        assert!(!ray_triangle_do_intersect(
            &t,
            &Ray3::new(p(0.2, 0.2, 1.0), p(0.2, 0.2, 2.0))
        ));
        assert!(!ray_triangle_do_intersect(
            &t,
            &Ray3::new(p(2.0, 2.0, 0.0), p(3.0, 3.0, 0.0))
        ));
    }
}
