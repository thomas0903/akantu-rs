// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS); tests the margin of
// the grid search structure that replaces the exhaustive scan of the
// AABB-tree calls of src/geometry/mesh_segment_intersector_tmpl.hh.

//! The growth of the query pieces of `GridIndex`: a primitive that the scan
//! reports although its bounding box is disjoint from the query's by a few
//! ulps (the predicates' rounding), on the other side of a cell boundary,
//! must still be a candidate. The meshes are chosen so that the cell size is
//! exact and the touched vertices lie on cell boundaries; the queries are
//! searched over ulp offsets (deterministic generator), for segments and
//! rays, in 2-D and 3-D, at the corners of the box, and with sources
//! 3·10^15 away (the rounding of the clipped end points).
//!
//! What the tests show (each change below of `index.rs` fails a test here):
//! no growth at all; no clipped-end term; skipping the walk when the box
//! lies behind a ray's source or beyond a segment's end. What they do not
//! show: the quarter-cell margin `δ` and the cut-point term
//! `8 ε max|coordinate|` are conservative. In every family searched
//! (offsets up to 2048 ulps beside vertices, queries in diagonal face planes
//! shifted by up to 10^-13, meshes at 2^52) every report of the scan lies
//! either within the cut-point rounding of the query or an element or more
//! away, and at magnitudes where the cut points round by more than `δ` the
//! mesh nodes and the cut points share the representable values, so a
//! touched primitive's box always overlaps a piece; setting either term to
//! zero fails none of these tests.

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
    // reports name the element (several primitives per element for
    // tetrahedron faces): the box of an element's primitives
    let nb_ids = prims.iter().map(|t| t.id as usize + 1).max().unwrap();
    let element_box: Vec<Bbox3> = (0..nb_ids)
        .map(|id| {
            let v: Vec<Point3> = prims
                .iter()
                .filter(|t| t.id as usize == id)
                .flat_map(|t| t.triangle.v)
                .collect();
            bbox_of_points(&v)
        })
        .collect();
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
            let tb = element_box[*id as usize];
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
            if cells_disjoint(&layout, &rbox, &element_box[*id as usize]) {
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
        &rect_mesh([0.0, 0.0], 16, 32, 1.0 / 16.0, 1.0 / 32.0),
        2,
        6,
        10_000,
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
    let (s, r, b) = vertex_family(&prims, 3, 9, 8_000, 4);
    eprintln!("3-D: {s} segment and {r} ray witnesses, {b} beyond the rounding term");
    let (_, _, b2) = vertex_family(&kuhn_faces(4, 1.0 / 6.0), 3, 10, 1_500, 2048);
    eprintln!("3-D: {b2} beyond the rounding term (offsets up to 2048 ulps)");
    assert!(s >= 30 && r >= 30, "{s} {r}");
}

/// Coordinates near 2^52 (ulp 1) with elements of 1 × 1 ulp: the cell edge
/// is √½ ulp (not aligned with the representable values) and the
/// quarter-cell margin 0.18 ulp, while the cut points of a long query, and
/// the clipped ends of a query from outside the mesh, round by up to half an
/// ulp. The scan's
/// predicates are exact here (small integer differences); every primitive
/// the true query crosses must be reported by the grid as well, for
/// segments and for rays (also from sources outside the mesh).
#[test]
fn margin_covers_the_rounding_of_the_cut_points() {
    let x0 = 4_503_599_627_370_496.0; // 2^52
    let prims = rect_mesh([x0, x0], 24, 24, 1.0, 1.0);
    let scan = LinearScan::build(&prims).unwrap();
    let grid = GridIndex::build(&prims).unwrap();
    let layout = grid.cell_layout().unwrap();
    assert_eq!(layout.1, (0.5 as Real).sqrt());
    let mut rng = Rng(77);
    let mut checked = 0;
    for _ in 0..150 {
        // integer end points (exact), long and oblique: many cut points
        #[allow(clippy::cast_precision_loss)] // small integers
        let pt = |rng: &mut Rng| {
            Point3::new(
                x0 - 10.0 + rng.below(45) as Real,
                x0 - 10.0 + rng.below(45) as Real,
                0.0,
            )
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
    assert!(checked > 1_000, "{checked}");
}

/// Segments that stop a few ulps before a corner of the mesh's box, coming
/// from outside (the box lies beyond the segment's end: only the end's
/// neighbourhood is walked), and rays that start there and point away.
#[test]
fn margin_at_the_corners_of_the_box() {
    let prims = kuhn_faces(3, 0.25);
    let scan = LinearScan::build(&prims).unwrap();
    let grid = GridIndex::build(&prims).unwrap();
    let mut rng = Rng(21);
    let mut reports = 0;
    for _ in 0..3_000 {
        let corner = [rng.below(2), rng.below(2), rng.below(2)];
        #[allow(clippy::cast_precision_loss)] // 0 or 0.75
        let v = corner.map(|c| c as Real * 0.75);
        // outward offsets of a few ulps, an inward direction
        let out = |c: usize, x: Real, k: i64| if c == 0 { ulps(x, -k) } else { ulps(x, k) };
        let q = Point3::new(
            out(corner[0], v[0], 1 + i64::try_from(rng.below(3)).unwrap()),
            out(corner[1], v[1], 1 + i64::try_from(rng.below(3)).unwrap()),
            out(corner[2], v[2], 1 + i64::try_from(rng.below(3)).unwrap()),
        );
        let inward = corner.map(|c| if c == 0 { 1.0 } else { -1.0 });
        let len = 0.1 + rng.unit();
        let d = [0, 1, 2].map(|i| inward[i] * (0.2 + rng.unit()));
        let p = Point3::new(q.x - len * d[0], q.y - len * d[1], q.z - len * d[2]);
        let s = Segment3::new(p, q);
        let (mut a, mut b) = (Vec::new(), Vec::new());
        let ra = scan.all_intersections(&s, &mut a);
        let rb = grid.all_intersections(&s, &mut b);
        assert_eq!(ra.is_ok(), rb.is_ok(), "{s:?}");
        assert_eq!(a, b, "{s:?}");
        reports += a.len();
        let r = Ray3::new(q, p);
        let (mut a, mut b) = (Vec::new(), Vec::new());
        scan.all_intersected_primitives(&r, &mut a);
        grid.all_intersected_primitives(&r, &mut b);
        assert_eq!(a, b, "{r:?}");
        reports += a.len();
    }
    eprintln!("corners: {reports} reports");
    assert!(reports > 0);
}

type V = [Real; 3];

fn sub(a: V, b: V) -> V {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn dot(a: V, b: V) -> Real {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// Distance from `x` to the segment `a`–`b`.
fn point_segment(x: V, a: V, b: V) -> Real {
    let d = sub(b, a);
    let t = (dot(sub(x, a), d) / dot(d, d)).clamp(0.0, 1.0);
    let y = [a[0] + t * d[0], a[1] + t * d[1], a[2] + t * d[2]];
    dot(sub(x, y), sub(x, y)).sqrt()
}

/// Distance from a 2-D triangle (z = 0) to the segment `a`–`b` (z = 0):
/// zero when they cross, else the smallest vertex–segment or end–edge
/// distance.
fn triangle_segment_2d(t: &Triangle3, a: V, b: V) -> Real {
    let v: Vec<V> = t.v.iter().map(|p| [p.x, p.y, 0.0]).collect();
    let orient = |p: V, q: V, r: V| (q[0] - p[0]) * (r[1] - p[1]) - (q[1] - p[1]) * (r[0] - p[0]);
    let crosses = |p: V, q: V, r: V, s: V| {
        orient(p, q, r) * orient(p, q, s) <= 0.0 && orient(r, s, p) * orient(r, s, q) <= 0.0
    };
    let inside = |x: V| {
        let o = [
            orient(v[0], v[1], x),
            orient(v[1], v[2], x),
            orient(v[2], v[0], x),
        ];
        o.iter().all(|&w| w >= 0.0) || o.iter().all(|&w| w <= 0.0)
    };
    if inside(a) || inside(b) || (0..3).any(|i| crosses(a, b, v[i], v[(i + 1) % 3])) {
        return 0.0;
    }
    let mut d = Real::INFINITY;
    for i in 0..3 {
        d = d.min(point_segment(v[i], a, b));
        d = d.min(point_segment(a, v[i], v[(i + 1) % 3]));
        d = d.min(point_segment(b, v[i], v[(i + 1) % 3]));
    }
    d
}

/// Sources 3·10^15 away from the mesh: the clipped end points of the walk
/// round by up to about 0.2, more than ten times the margin. The scan's own predicates are
/// rounding noise at that scale too; every primitive it reports within half
/// the margin of the true query must be reported by the grid.
#[test]
fn margin_covers_the_rounding_of_far_sources() {
    let prims = rect_mesh([0.0, 0.0], 16, 32, 0.25, 0.125);
    let scan = LinearScan::build(&prims).unwrap();
    let grid = GridIndex::build(&prims).unwrap();
    let delta = grid.cell_layout().unwrap().1 / 4.0;
    let mut rng = Rng(31);
    let mut near = 0;
    for _ in 0..150 {
        let q = [4.0 * rng.unit(), 4.0 * rng.unit(), 0.0];
        let ang = rng.unit() * std::f64::consts::TAU;
        let u = [ang.cos(), ang.sin(), 0.0];
        let far = 3e15;
        let f = [q[0] - far * u[0], q[1] - far * u[1], 0.0];
        let (fp, qp) = (Point3::new(f[0], f[1], 0.0), Point3::new(q[0], q[1], 0.0));
        // the true query near the mesh: from q back towards f (segment), and
        // the whole line through the mesh (ray from f through q)
        let back = [q[0] - 20.0 * u[0], q[1] - 20.0 * u[1], 0.0];
        let ahead = [q[0] + 20.0 * u[0], q[1] + 20.0 * u[1], 0.0];
        let s = Segment3::new(fp, qp);
        let (mut a, mut b) = (Vec::new(), Vec::new());
        if scan.all_intersections(&s, &mut a).is_ok() {
            grid.all_intersections(&s, &mut b).unwrap();
            for (x, id) in &a {
                let t = &prims[*id as usize].triangle;
                if triangle_segment_2d(t, back, q) <= 0.5 * delta {
                    near += 1;
                    assert!(b.iter().any(|(y, j)| j == id && y == x), "{s:?}: {id}");
                }
            }
        }
        let r = Ray3::new(fp, qp);
        let (mut a, mut b) = (Vec::new(), Vec::new());
        scan.all_intersected_primitives(&r, &mut a);
        grid.all_intersected_primitives(&r, &mut b);
        for id in &a {
            if triangle_segment_2d(&prims[*id as usize].triangle, back, ahead) <= 0.5 * delta {
                near += 1;
                assert!(b.contains(id), "{r:?}: {id}");
            }
        }
    }
    eprintln!("far sources: {near} reports near the true query");
    assert!(near > 100, "{near}");
}
