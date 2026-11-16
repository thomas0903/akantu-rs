// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS); tests the margin of
// the grid search structure that replaces the exhaustive scan of the
// AABB-tree calls of src/geometry/mesh_segment_intersector_tmpl.hh.

//! The margin of `GridIndex`: a primitive that the scan reports although
//! its bounding box is disjoint from the query's by a few ulps (the
//! predicates' rounding), on the other side of a cell boundary, must still
//! be a candidate. The meshes are chosen so that the cell size is exact
//! and the touched vertices lie on cell boundaries; the queries are
//! searched over ulp offsets (deterministic generator) until enough such
//! configurations are found, for segments and rays, in 2-D and 3-D. A
//! last family puts the mesh at coordinates near 2^52, where the cut points
//! of a long query round by up to half a cell edge, beyond the quarter-cell
//! margin: the rounding term of the query growth must cover them.

use akantu_core::Real;
use akantu_geometry::factory::TrianglePrimitive;
use akantu_geometry::index::{GridIndex, LinearScan, TriangleIndex};
use akantu_geometry::kernel::{Bbox3, Point3, Ray3, Segment3, Triangle3};

/// xorshift64*: deterministic test inputs.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn unit(&mut self) -> Real {
        #[allow(clippy::cast_precision_loss)] // 53 random bits
        let x = (self.next() >> 11) as Real;
        x / (1u64 << 53) as Real
    }

    fn below(&mut self, n: usize) -> usize {
        #[allow(clippy::cast_possible_truncation)] // test sizes
        let r = (self.next() % n as u64) as usize;
        r
    }

    /// An integer in `[−k, k]`.
    fn offset(&mut self, k: usize) -> i64 {
        i64::try_from(self.below(2 * k + 1)).unwrap() - i64::try_from(k).unwrap()
    }
}

fn prim(id: usize, a: Point3, b: Point3, c: Point3) -> TrianglePrimitive {
    TrianglePrimitive {
        id: u32::try_from(id).unwrap(),
        triangle: Triangle3::new(a, b, c),
    }
}

/// `x` moved by `k` ulps (`x > 0`).
fn ulps(x: Real, k: i64) -> Real {
    if x == 0.0 {
        return Real::from_bits(k.unsigned_abs()) * if k < 0 { -1.0 } else { 1.0 };
    }
    let b = i64::try_from(x.to_bits()).unwrap();
    Real::from_bits(u64::try_from(b + k).unwrap())
}

/// `nx × ny` rectangles `a × b` from `origin`, two triangles each. With
/// `a = 2b` the grid's cell edge is exactly `b` (`√(ab/2) = b`, half the
/// mean extent `a`) and every node lies on a cell boundary.
fn rect_mesh(origin: [Real; 2], nx: usize, ny: usize, a: Real, b: Real) -> Vec<TrianglePrimitive> {
    #[allow(clippy::cast_precision_loss)] // small grids
    let at =
        |i: usize, j: usize| Point3::new(origin[0] + i as Real * a, origin[1] + j as Real * b, 0.0);
    let mut v = Vec::new();
    for j in 0..ny {
        for i in 0..nx {
            let id = v.len();
            v.push(prim(id, at(i, j), at(i + 1, j), at(i + 1, j + 1)));
            v.push(prim(id + 1, at(i, j), at(i + 1, j + 1), at(i, j + 1)));
        }
    }
    v
}

/// `n³` cubes of side `s`, six Kuhn tetrahedra each, their four faces as
/// primitives. Every face's box has the extent `s`, so the cell edge is
/// exactly `s / 2` and every node lies on a cell boundary.
fn kuhn_faces(n: usize, s: Real) -> Vec<TrianglePrimitive> {
    #[allow(clippy::cast_precision_loss)] // small grids
    let at =
        |i: usize, j: usize, k: usize| Point3::new(i as Real * s, j as Real * s, k as Real * s);
    let mut v = Vec::new();
    let mut tet = 0;
    for k in 0..n {
        for j in 0..n {
            for i in 0..n {
                let c = |b: usize| at(i + (b & 1), j + ((b >> 1) & 1), k + ((b >> 2) & 1));
                for path in [[1, 3], [1, 5], [2, 3], [2, 6], [4, 5], [4, 6]] {
                    let p = [c(0), c(path[0]), c(path[1]), c(7)];
                    for f in [[0, 1, 2], [0, 1, 3], [0, 2, 3], [1, 2, 3]] {
                        v.push(prim(tet, p[f[0]], p[f[1]], p[f[2]]));
                    }
                    tet += 1;
                }
            }
        }
    }
    v
}

/// Cell coordinate of `x` in direction `d` (the grid's rule).
fn cell(layout: &([Real; 3], Real, [usize; 3]), x: Real, d: usize) -> usize {
    let (lo, h, n) = layout;
    if n[d] == 1 {
        return 0;
    }
    #[allow(clippy::cast_precision_loss)] // n ≤ 4096
    let max = (n[d] - 1) as Real;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // clamped
    let c = ((x - lo[d]) / h).floor().clamp(0.0, max) as usize;
    c
}

/// `true` when the cells of box `a` and box `b` (exact boxes, no margin)
/// share no cell: such a primitive is a candidate only through the margin.
fn cells_disjoint(layout: &([Real; 3], Real, [usize; 3]), a: &Bbox3, b: &Bbox3) -> bool {
    (0..3).any(|d| {
        cell(layout, a.max[d], d) < cell(layout, b.min[d], d)
            || cell(layout, b.max[d], d) < cell(layout, a.min[d], d)
    })
}

fn bbox_of_points(pts: &[Point3]) -> Bbox3 {
    let mut b = Bbox3 {
        min: [Real::INFINITY; 3],
        max: [Real::NEG_INFINITY; 3],
    };
    for p in pts {
        for (d, x) in [p.x, p.y, p.z].into_iter().enumerate() {
            b.min[d] = b.min[d].min(x);
            b.max[d] = b.max[d].max(x);
        }
    }
    b
}

/// Queries ending (segments) or starting (rays, pointing away) a few ulps
/// from a vertex of a random primitive, the other end `len` away in a random
/// direction. Returns the number of witnesses: primitives reported by the
/// scan whose cells are disjoint from the query's cells (the margin alone
/// makes them candidates); every scan report must be a grid report.
fn vertex_family(
    prims: &[TrianglePrimitive],
    dim: usize,
    seed: u64,
    tries: usize,
    max_offset: usize,
) -> (usize, usize, usize) {
    let scan = LinearScan::build(prims).unwrap();
    let grid = GridIndex::build(prims).unwrap();
    let layout = grid.cell_layout().unwrap();
    let all: Vec<Point3> = prims.iter().flat_map(|t| t.triangle.v).collect();
    let mesh_box = bbox_of_points(&all);
    let mut rng = Rng(seed);
    let (mut seg_witnesses, mut ray_witnesses, mut beyond_rounding) = (0, 0, 0);
    for _ in 0..tries {
        let t = &prims[rng.below(prims.len())];
        let v = t.triangle.v[rng.below(3)];
        let mut d = [rng.unit() - 0.5, rng.unit() - 0.5, rng.unit() - 0.5];
        if dim == 2 {
            d[2] = 0.0;
        }
        let len = 0.1 + 0.8 * rng.unit();
        let q = Point3::new(
            ulps(v.x, rng.offset(4)),
            ulps(v.y, rng.offset(4)),
            if dim == 3 {
                ulps(v.z, rng.offset(4))
            } else {
                0.0
            },
        );
        let p = Point3::new(q.x - len * d[0], q.y - len * d[1], q.z - len * d[2]);
        // with large offsets: segments strictly inside the mesh's box (not
        // clipped, so only the margin covers the contacts beyond rounding)
        let inside = |x: Point3| {
            (0..dim).all(|k| {
                let c = [x.x, x.y, x.z][k];
                mesh_box.min[k] < c && c < mesh_box.max[k]
            })
        };
        if max_offset > 4 && !(inside(p) && inside(q)) {
            continue;
        }
        // segment p → q
        let s = Segment3::new(p, q);
        let (mut a, mut b) = (Vec::new(), Vec::new());
        let ra = scan.all_intersections(&s, &mut a);
        let rb = grid.all_intersections(&s, &mut b);
        assert_eq!(ra.is_ok(), rb.is_ok(), "{s:?}");
        let sb = bbox_of_points(&[p, q]);
        for (x, id) in &a {
            let tb = prims[*id as usize].triangle.bbox();
            if cells_disjoint(&layout, &sb, &tb) {
                seg_witnesses += 1;
                // farther than the rounding term of the query growth: only
                // the margin δ makes it a candidate
                let gap = (0..3)
                    .map(|d| (tb.min[d] - sb.max[d]).max(sb.min[d] - tb.max[d]))
                    .fold(0.0, Real::max);
                let scale = [p.x, p.y, p.z, q.x, q.y, q.z]
                    .iter()
                    .fold(0.0, |m: Real, x| m.max(x.abs()));
                if gap > 8.0 * Real::EPSILON * scale {
                    beyond_rounding += 1;
                }
                assert!(
                    b.iter().any(|(y, j)| j == id && y == x),
                    "{s:?}: the scan's report {x:?} on {id} is missing from the grid's {b:?}"
                );
            }
        }
        // ray from q away from the vertex (its points lie on one side)
        let r = Ray3::new(q, p);
        let (mut a, mut b) = (Vec::new(), Vec::new());
        scan.all_intersected_primitives(&r, &mut a);
        grid.all_intersected_primitives(&r, &mut b);
        let far = Point3::new(q.x - 1e3 * d[0], q.y - 1e3 * d[1], q.z - 1e3 * d[2]);
        let rbox = bbox_of_points(&[q, far]);
        for id in &a {
            if cells_disjoint(&layout, &rbox, &prims[*id as usize].triangle.bbox()) {
                ray_witnesses += 1;
                assert!(
                    b.contains(id),
                    "{r:?}: the scan's ray report {id} is missing from {b:?}"
                );
            }
        }
    }
    (seg_witnesses, ray_witnesses, beyond_rounding)
}

#[test]
fn margin_keeps_rounding_contacts_2d() {
    let prims = rect_mesh([0.0, 0.0], 8, 16, 0.125, 0.0625);
    let grid = GridIndex::build(&prims).unwrap();
    assert_eq!(grid.cell_layout().unwrap().1, 0.0625);
    let (s, r, b) = vertex_family(&prims, 2, 5, 40_000, 4);
    eprintln!("2-D: {s} segment and {r} ray witnesses");
    let (_, _, b2) = vertex_family(
        &rect_mesh([0.0, 0.0], 64, 128, 1.0 / 64.0, 1.0 / 128.0),
        2,
        6,
        40_000,
        2048,
    );
    eprintln!("2-D: {b} {b2} beyond the rounding term");
    assert!(s >= 50 && r >= 5, "{s} {r}");
}

#[test]
fn margin_keeps_rounding_contacts_3d() {
    let prims = kuhn_faces(3, 0.25);
    let grid = GridIndex::build(&prims).unwrap();
    assert_eq!(grid.cell_layout().unwrap().1, 0.125);
    let (s, r, b) = vertex_family(&prims, 3, 9, 3_000, 4);
    eprintln!("3-D: {s} segment and {r} ray witnesses, {b} beyond the rounding term");
    let (_, _, b2) = vertex_family(&kuhn_faces(6, 1.0 / 6.0), 3, 10, 3_000, 2048);
    eprintln!("3-D: {b2} beyond the rounding term (offsets up to 2048 ulps)");
    assert!(s >= 50 && r >= 20, "{s} {r}");
}

/// Coordinates near 2^52 (ulp 1) with elements of 2 × 1 ulps: the cell edge
/// is one ulp and the quarter-cell margin a quarter ulp, while the cut
/// points of a long query round by up to half an ulp. The scan's
/// predicates are exact here (small integer differences); every primitive
/// the true query crosses must be reported by the grid as well, for
/// segments and for rays (also from sources outside the mesh).
#[test]
fn margin_covers_the_rounding_of_the_cut_points() {
    let x0 = 4_503_599_627_370_496.0; // 2^52
    let prims = rect_mesh([x0, x0], 24, 48, 2.0, 1.0);
    let scan = LinearScan::build(&prims).unwrap();
    let grid = GridIndex::build(&prims).unwrap();
    let layout = grid.cell_layout().unwrap();
    assert_eq!(layout.1, 1.0);
    let mut rng = Rng(77);
    let mut checked = 0;
    for _ in 0..600 {
        // integer end points (exact), long and oblique: many cut points
        #[allow(clippy::cast_precision_loss)] // small integers
        let pt = |rng: &mut Rng| {
            Point3::new(x0 + rng.below(49) as Real, x0 + rng.below(49) as Real, 0.0)
        };
        let s = Segment3::new(pt(&mut rng), pt(&mut rng));
        let (mut a, mut b) = (Vec::new(), Vec::new());
        if scan.all_intersections(&s, &mut a).is_err() {
            continue;
        }
        grid.all_intersections(&s, &mut b).unwrap();
        checked += a.len();
        assert_eq!(a, b, "{s:?}");
        // the ray through the same points, from outside the mesh too
        let r = Ray3::new(s.source, s.target);
        let (mut a, mut b) = (Vec::new(), Vec::new());
        scan.all_intersected_primitives(&r, &mut a);
        grid.all_intersected_primitives(&r, &mut b);
        checked += a.len();
        assert_eq!(a, b, "{r:?}");
        let back = Point3::new(
            2.0 * s.source.x - s.target.x,
            2.0 * s.source.y - s.target.y,
            0.0,
        );
        let r = Ray3::new(back, s.source);
        let (mut a, mut b) = (Vec::new(), Vec::new());
        scan.all_intersected_primitives(&r, &mut a);
        grid.all_intersected_primitives(&r, &mut b);
        checked += a.len();
        assert_eq!(a, b, "{r:?}");
    }
    assert!(checked > 5_000, "{checked}");
}
