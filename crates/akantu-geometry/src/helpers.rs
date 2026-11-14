// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/geometry/geom_helper_functions.hh
// (comparePoints for Cartesian points, compareSegments, compareSegmentPairs,
// segmentPairsLess, IsSameSegment). The Circular_arc_point_3 overload of
// comparePoints lives in akantu-geometry-cgal (it needs the spherical kernel).

//! Fuzzy and exact comparisons of points and segments (GEOM-009).
//!
//! * [`compare_points`] / [`compare_segments`]: upstream's fuzzy equality
//!   (`Math::are_float_equal` per coordinate with the global
//!   `Math::tolerance`, passed explicitly here; the default is
//!   [`akantu_core::math::DEFAULT_TOLERANCE`]). Segments compare equal in
//!   either orientation.
//! * [`segment_pairs_less`]: upstream's `segmentPairsLess` exactly — it
//!   casts `CGAL::compare_lexicographically` to `bool`, so it is `true`
//!   whenever the segments' `min()` or `max()` end points differ: **not** a
//!   strict weak ordering (UD-408). Only meaningful together with the
//!   libstdc++ set model [`crate::cxx::RbSet`].
//! * [`segment_pair_order`]: the corrected, strict lexicographic order on
//!   `(min(), max())` (the port's default ordering, UD-408).

use std::cmp::Ordering;

use akantu_core::Real;
use akantu_core::math::are_float_equal;

use crate::kernel::{Point3, Segment3, Sign, compare_xyz};

/// `comparePoints(a, b)` for Cartesian points: every coordinate
/// `Math::are_float_equal` with `tolerance`.
#[must_use]
pub fn compare_points(a: Point3, b: Point3, tolerance: Real) -> bool {
    are_float_equal(a.x, b.x, tolerance)
        && are_float_equal(a.y, b.y, tolerance)
        && are_float_equal(a.z, b.z, tolerance)
}

/// `compareSegments(a, b)`: same end points in the same or the opposite
/// orientation (fuzzy, see [`compare_points`]).
#[must_use]
pub fn compare_segments(a: &Segment3, b: &Segment3, tolerance: Real) -> bool {
    (compare_points(a.source, b.source, tolerance) && compare_points(a.target, b.target, tolerance))
        || (compare_points(a.source, b.target, tolerance)
            && compare_points(a.target, b.source, tolerance))
}

/// `compareSegmentPairs(a, b)`: [`compare_segments`] on the segments.
#[must_use]
pub fn compare_segment_pairs<T>(a: &(Segment3, T), b: &(Segment3, T), tolerance: Real) -> bool {
    compare_segments(&a.0, &b.0, tolerance)
}

/// Upstream `segmentPairsLess` (UD-408): `bool(compare_lexicographically(
/// a.min, b.min)) or bool(compare_lexicographically(a.max, b.max))`, i.e.
/// `true` iff the segments differ as unordered pairs of exact points.
#[must_use]
pub fn segment_pairs_less<T>(a: &(Segment3, T), b: &(Segment3, T)) -> bool {
    compare_xyz(a.0.min(), b.0.min()) != Sign::Zero
        || compare_xyz(a.0.max(), b.0.max()) != Sign::Zero
}

/// Corrected order (UD-408 default): lexicographic on `(min(), max())`
/// with CGAL's exact `compare_lexicographically`.
#[must_use]
pub fn segment_pair_order(a: &Segment3, b: &Segment3) -> Ordering {
    match compare_xyz(a.min(), b.min()) {
        Sign::Zero => compare_xyz(a.max(), b.max()).to_ordering(),
        s => s.to_ordering(),
    }
}

/// Upstream predicate `IsSameSegment` (fuzzy, either orientation).
#[derive(Clone, Copy, Debug)]
pub struct IsSameSegment {
    segment: Segment3,
    tolerance: Real,
}

impl IsSameSegment {
    /// Predicate for `segment` with the `Math::tolerance` to use.
    #[must_use]
    pub fn new(segment: Segment3, tolerance: Real) -> Self {
        Self { segment, tolerance }
    }

    /// `operator()(pair)`.
    #[must_use]
    pub fn test<T>(&self, pair: &(Segment3, T)) -> bool {
        compare_segments(&self.segment, &pair.0, self.tolerance)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use akantu_core::math::DEFAULT_TOLERANCE;

    #[test]
    fn segments_compare_in_both_orientations() {
        let a = Point3::new(0.0, 1.0, 0.0);
        let b = Point3::new(0.0, 1.0, 1.0);
        let s1 = Segment3::new(a, b);
        let s2 = Segment3::new(b, a);
        assert!(compare_segments(&s1, &s2, DEFAULT_TOLERANCE));
        assert!(IsSameSegment::new(s1, DEFAULT_TOLERANCE).test(&(s2, 0)));
        assert!(!segment_pairs_less(&(s1, 1), &(s2, 2)));
        assert_eq!(segment_pair_order(&s1, &s2), Ordering::Equal);
        let s3 = Segment3::new(a, Point3::new(0.0, 2.0, 0.0));
        assert!(segment_pairs_less(&(s1, 1), &(s3, 2)));
        assert!(
            segment_pairs_less(&(s3, 2), &(s1, 1)),
            "not antisymmetric (UD-408)"
        );
    }
}
