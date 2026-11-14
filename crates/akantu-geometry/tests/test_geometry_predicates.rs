// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): test/test_geometry/test_geometry_predicates.cc.

//! Upstream `test_geometry_predicates`: `compareSegments` is orientation
//! independent; sorting and `std::unique` with `segmentPairsLess` /
//! `compareSegmentPairs` keep one of two equal segments; a
//! `std::set<…, segmentPairsLess>` keeps one of them.
//!
//! The upstream test sorts with an invalid comparator (UD-408) and inserts
//! with a hint; the port checks the same intents with the corrected order
//! (default) and with the libstdc++ set model (non-hinted insertion, which
//! detects this duplicate the same way).

use akantu_core::math::DEFAULT_TOLERANCE;
use akantu_geometry::cxx::RbSet;
use akantu_geometry::helpers::{
    compare_segment_pairs, compare_segments, segment_pair_order, segment_pairs_less,
};
use akantu_geometry::kernel::{Point3, Segment3};

#[test]
fn test_geometry_predicates() {
    let a = Point3::new(0.0, 1.0, 0.0);
    let b = Point3::new(0.0, 1.0, 1.0);
    let seg1 = Segment3::new(a, b);
    let seg2 = Segment3::new(b, a);
    assert!(compare_segments(&seg1, &seg2, DEFAULT_TOLERANCE));

    // sort + unique on a list of segment pairs
    let mut pair_list = vec![(seg1, 1_u32), (seg2, 2_u32)];
    pair_list.sort_by(|x, y| segment_pair_order(&x.0, &y.0));
    pair_list.dedup_by(|x, y| compare_segment_pairs(x, y, DEFAULT_TOLERANCE));
    assert_eq!(pair_list.len(), 1);

    // insertion in a set (upstream comparator, libstdc++ set model)
    let mut pair_set = RbSet::new();
    assert!(pair_set.insert((seg1, 1_u32), segment_pairs_less));
    assert!(!pair_set.insert((seg2, 2_u32), segment_pairs_less));
    assert_eq!(pair_set.len(), 1);
}
