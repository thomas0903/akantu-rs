// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs tests (TASN-011 / TASN-000).
// Ported from the intent of Akantu (EPFL/LSMS):
// extra_packages/traction-at-split-node-contact/src/functions/node_filter.hh
// (`GeometryFilter`, `FilterPositionsGreaterThan`, `FilterPositionsLessThan`).
//! Node filters of the package, upstream convention preserved (UD-404):
//! `FilterPositionsGreaterThan` is `false` for nodes whose coordinate is
//! **greater** than the limit and `true` otherwise (also at the limit and for
//! NaN), `FilterPositionsLessThan` the converse; the base
//! `GeometryFilter::operator()` is `AKANTU_TO_IMPLEMENT` (`NotImplemented`).

use akantu_core::{Error, Idx, Real};
use akantu_mesh::{Mesh, MeshAccessor};
use akantu_ntn_contact::node_filter::{
    FilterPositionsGreaterThan, FilterPositionsLessThan, GeometryFilter,
};

/// Nodes of a 2-D mesh with coordinates on both sides of, and exactly at,
/// the limits used below (no elements: the filters only read positions).
const POINTS: [[Real; 2]; 7] = [
    [0.0, 0.0],   // 0
    [1.0, 0.5],   // 1: x = limit, y = limit
    [2.0, -1.0],  // 2
    [1.0, 3.0],   // 3: x = limit
    [0.5, 0.5],   // 4: y = limit
    [1.0, -2.0],  // 5: x = limit
    [-3.0, 0.75], // 6
];

fn mesh() -> Mesh {
    let mut mesh = Mesh::new(2, "filters").unwrap();
    {
        let mut acc = MeshAccessor::new(&mut mesh);
        for p in POINTS {
            acc.push_node(&p).unwrap();
        }
    }
    mesh
}

fn all<F: Fn(Idx) -> bool>(f: F) -> Vec<bool> {
    (0..POINTS.len() as Idx).map(f).collect()
}

#[test]
fn greater_than_returns_false_above_the_limit_true_otherwise() {
    let mesh = mesh();
    for (dir, limit) in [(0, 1.0), (1, 0.5), (0, -10.0), (1, 100.0)] {
        let f = FilterPositionsGreaterThan(GeometryFilter::new(&mesh, dir, limit).unwrap());
        let got = all(|n| f.call(n).unwrap());
        let want: Vec<bool> = POINTS.iter().map(|p| p[dir] <= limit).collect();
        assert_eq!(got, want, "dir {dir} limit {limit}");
    }
    // explicit table for x ≤ 1: only node 2 (x = 2) is above the limit
    let f = FilterPositionsGreaterThan(GeometryFilter::new(&mesh, 0, 1.0).unwrap());
    assert_eq!(
        all(|n| f.call(n).unwrap()),
        [true, true, false, true, true, true, true]
    );
}

#[test]
fn less_than_returns_false_below_the_limit_true_otherwise() {
    let mesh = mesh();
    for (dir, limit) in [(0, 1.0), (1, 0.5), (0, -10.0), (1, 100.0)] {
        let f = FilterPositionsLessThan(GeometryFilter::new(&mesh, dir, limit).unwrap());
        let got = all(|n| f.call(n).unwrap());
        let want: Vec<bool> = POINTS.iter().map(|p| p[dir] >= limit).collect();
        assert_eq!(got, want, "dir {dir} limit {limit}");
    }
    // explicit table for x < 1: nodes 0, 4 and 6 are below the limit
    let f = FilterPositionsLessThan(GeometryFilter::new(&mesh, 0, 1.0).unwrap());
    assert_eq!(
        all(|n| f.call(n).unwrap()),
        [false, true, true, true, false, true, false]
    );
}

/// Nodes exactly at the limit are kept (`true`) by both filters: neither
/// comparison is strict-or-equal (UD-404: the inverted convention).
#[test]
fn nodes_at_the_limit_are_true_for_both_filters() {
    let mesh = mesh();
    let g = FilterPositionsGreaterThan(GeometryFilter::new(&mesh, 0, 1.0).unwrap());
    let l = FilterPositionsLessThan(GeometryFilter::new(&mesh, 0, 1.0).unwrap());
    for n in [1, 3, 5] {
        assert!(g.call(n).unwrap() && l.call(n).unwrap(), "node {n}");
    }
}

/// Complementary sets: under the current convention ("true → keep node")
/// the two filters keep `x ≤ limit` and `x ≥ limit`; their intersection is
/// exactly the nodes at the limit.
#[test]
fn the_two_filters_keep_complementary_half_spaces() {
    let mesh = mesh();
    let limit = 0.5;
    let g = FilterPositionsGreaterThan(GeometryFilter::new(&mesh, 1, limit).unwrap());
    let l = FilterPositionsLessThan(GeometryFilter::new(&mesh, 1, limit).unwrap());
    for n in 0..POINTS.len() as Idx {
        let y = POINTS[n as usize][1];
        assert_eq!(g.call(n).unwrap(), y <= limit, "greater-than, node {n}");
        assert_eq!(l.call(n).unwrap(), y >= limit, "less-than, node {n}");
        assert_eq!(
            g.call(n).unwrap() && l.call(n).unwrap(),
            y == limit,
            "intersection, node {n}"
        );
    }
}

#[test]
fn nan_coordinates_and_limits_are_true_like_upstream() {
    let mesh = mesh();
    // `!(x > NaN)` and `!(x < NaN)` are true in C++
    let g = FilterPositionsGreaterThan(GeometryFilter::new(&mesh, 0, Real::NAN).unwrap());
    let l = FilterPositionsLessThan(GeometryFilter::new(&mesh, 0, Real::NAN).unwrap());
    for n in 0..POINTS.len() as Idx {
        assert!(g.call(n).unwrap() && l.call(n).unwrap(), "node {n}");
    }
}

#[test]
fn base_geometry_filter_call_is_not_implemented() {
    let mesh = mesh();
    let f = GeometryFilter::new(&mesh, 0, 0.0).unwrap();
    for n in [0, 3] {
        assert!(
            matches!(f.call(n), Err(Error::NotImplemented(_))),
            "node {n}"
        );
    }
}

#[test]
fn invalid_direction_or_node_is_an_error() {
    let mesh = mesh();
    assert!(matches!(
        GeometryFilter::new(&mesh, 2, 0.0),
        Err(Error::OutOfRange { .. })
    ));
    let f = FilterPositionsGreaterThan(GeometryFilter::new(&mesh, 0, 0.0).unwrap());
    assert!(matches!(f.call(-1), Err(Error::InvalidIndex { .. })));
    assert!(f.call(POINTS.len() as Idx).is_err());
}
