// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS); tests the search
// structure that replaces the exhaustive scan of the AABB-tree calls of
// src/geometry/mesh_segment_intersector_tmpl.hh.

//! `GridIndex` against `LinearScan`: the same reports (intersection objects
//! bit for bit, primitive ids) in the same order, for segments and rays, on
//! structured meshes whose coordinates are not exactly representable
//! (multiples of 0.1), on jittered meshes, and with queries chosen to be
//! degenerate: along grid lines and element edges, on face planes, through
//! vertices, inside one element, partly or wholly outside the mesh, of zero
//! length.

use akantu_core::Real;
use akantu_geometry::factory::TrianglePrimitive;
use akantu_geometry::index::{GridIndex, LinearScan, TriangleIndex};
use akantu_geometry::intersections::{
    TriangleSegmentIntersection, ray_triangle_do_intersect, triangle_segment_intersection,
};
use akantu_geometry::kernel::{Point3, Ray3, Segment3, Triangle3};

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
}

fn prim(id: usize, a: Point3, b: Point3, c: Point3) -> TrianglePrimitive {
    TrianglePrimitive {
        id: u32::try_from(id).unwrap(),
        triangle: Triangle3::new(a, b, c),
    }
}

/// `n × n` squares of side `h`, each split into two triangles (z = 0),
/// node coordinates `i · h` (`h = 0.1`: not exactly representable),
/// optionally jittered.
fn tri_mesh(
    n: usize,
    h: Real,
    jitter: Real,
    rng: &mut Rng,
) -> (Vec<TrianglePrimitive>, Vec<Point3>) {
    let mut nodes = Vec::new();
    for j in 0..=n {
        for i in 0..=n {
            #[allow(clippy::cast_precision_loss)] // small grids
            let (x, y) = (i as Real * h, j as Real * h);
            let inner = i > 0 && j > 0 && i < n && j < n;
            let (dx, dy) = if inner {
                (
                    jitter * (rng.unit() - 0.5) * h,
                    jitter * (rng.unit() - 0.5) * h,
                )
            } else {
                (0.0, 0.0)
            };
            nodes.push(Point3::new(x + dx, y + dy, 0.0));
        }
    }
    let at = |i: usize, j: usize| nodes[j * (n + 1) + i];
    let mut prims = Vec::new();
    for j in 0..n {
        for i in 0..n {
            let id = prims.len();
            prims.push(prim(id, at(i, j), at(i + 1, j), at(i + 1, j + 1)));
            prims.push(prim(id + 1, at(i, j), at(i + 1, j + 1), at(i, j + 1)));
        }
    }
    (prims, nodes)
}

/// `n³` cubes of side `h`, six tetrahedra each, the four faces of every
/// tetrahedron as primitives with the tetrahedron's id (the `Faces`
/// primitives of `_tetrahedron_4`).
fn tet_faces(
    n: usize,
    h: Real,
    jitter: Real,
    rng: &mut Rng,
) -> (Vec<TrianglePrimitive>, Vec<Point3>) {
    let mut nodes = Vec::new();
    for k in 0..=n {
        for j in 0..=n {
            for i in 0..=n {
                #[allow(clippy::cast_precision_loss)] // small grids
                let p = [i as Real * h, j as Real * h, k as Real * h];
                let inner = [i, j, k].iter().all(|&c| c > 0 && c < n);
                let mut q = p;
                if inner {
                    for x in &mut q {
                        *x += jitter * (rng.unit() - 0.5) * h;
                    }
                }
                nodes.push(Point3::new(q[0], q[1], q[2]));
            }
        }
    }
    let m = n + 1;
    let at = |i: usize, j: usize, k: usize| nodes[(k * m + j) * m + i];
    let mut prims = Vec::new();
    let mut tet = 0;
    for k in 0..n {
        for j in 0..n {
            for i in 0..n {
                let c = |b: usize| at(i + (b & 1), j + ((b >> 1) & 1), k + ((b >> 2) & 1));
                // Kuhn split along the diagonal 0–7
                for path in [[1, 3], [1, 5], [2, 3], [2, 6], [4, 5], [4, 6]] {
                    let v = [c(0), c(path[0]), c(path[1]), c(7)];
                    for f in [[0, 1, 2], [0, 1, 3], [0, 2, 3], [1, 2, 3]] {
                        prims.push(prim(tet, v[f[0]], v[f[1]], v[f[2]]));
                    }
                    tet += 1;
                }
            }
        }
    }
    (prims, nodes)
}

/// Queries over a mesh with node list `nodes` and extent `[0, len]^dim`.
fn queries(nodes: &[Point3], dim: usize, len: Real, h: Real, rng: &mut Rng) -> Vec<Segment3> {
    let mut out = Vec::new();
    let z = |v: Real| if dim == 3 { v } else { 0.0 };
    let rand_point = |rng: &mut Rng, lo: Real, hi: Real| {
        Point3::new(
            lo + (hi - lo) * rng.unit(),
            lo + (hi - lo) * rng.unit(),
            z(lo + (hi - lo) * rng.unit()),
        )
    };
    for _ in 0..150 {
        // random, partly outside
        let a = rand_point(rng, -0.2 * len, 1.2 * len);
        let b = rand_point(rng, -0.2 * len, 1.2 * len);
        out.push(Segment3::new(a, b));
        // short, mostly inside one element
        let a = rand_point(rng, 0.0, len);
        let d = 0.3 * h;
        let b = Point3::new(
            a.x + d * (rng.unit() - 0.5),
            a.y + d * (rng.unit() - 0.5),
            z(a.z + d * (rng.unit() - 0.5)),
        );
        out.push(Segment3::new(a, b));
    }
    for _ in 0..150 {
        // between two nodes (along edges, through vertices, on face
        // planes), sometimes extended beyond them
        let a = nodes[rng.below(nodes.len())];
        let b = nodes[rng.below(nodes.len())];
        out.push(Segment3::new(a, b));
        let t = 1.0 + 0.5 * rng.unit();
        let c = Point3::new(
            a.x + t * (b.x - a.x),
            a.y + t * (b.y - a.y),
            a.z + t * (b.z - a.z),
        );
        out.push(Segment3::new(a, c));
        // node to its neighbour along a grid direction and beyond
        let k = rng.below(nodes.len());
        let a = nodes[k];
        let step = 1 + rng.below(3);
        let b = nodes[(k + step).min(nodes.len() - 1)];
        out.push(Segment3::new(a, b));
        // zero length
        out.push(Segment3::new(a, a));
    }
    // axis-aligned lines at node coordinates (multiples of h, computed as
    // the mesh computes them) and slightly off them
    for j in 0..20 {
        #[allow(clippy::cast_precision_loss)] // small counts
        let y = (j % 11) as Real * h * (len / (10.0 * h)).round();
        let y = if j >= 11 { y + 1e-17 } else { y };
        let ylo = rng.unit() * len;
        out.push(Segment3::new(
            Point3::new(ylo * 0.5, y, z(y)),
            Point3::new(len * (0.5 + 0.5 * rng.unit()), y, z(y)),
        ));
        out.push(Segment3::new(
            Point3::new(y, -0.1 * len, z(ylo)),
            Point3::new(y, 1.1 * len, z(ylo)),
        ));
    }
    // wholly outside
    out.push(Segment3::new(
        Point3::new(2.0 * len, 2.0 * len, z(2.0 * len)),
        Point3::new(3.0 * len, 2.5 * len, z(2.0 * len)),
    ));
    out
}

type V = [Real; 3];

fn v(p: Point3) -> V {
    [p.x, p.y, p.z]
}

fn sub(a: V, b: V) -> V {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn dot(a: V, b: V) -> Real {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn cross(a: V, b: V) -> V {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn at(a: V, d: V, t: Real) -> V {
    [a[0] + t * d[0], a[1] + t * d[1], a[2] + t * d[2]]
}

fn dist(a: V, b: V) -> Real {
    dot(sub(a, b), sub(a, b)).sqrt()
}

/// Distance from `p` to the segment `a`–`b`.
fn point_segment(p: V, a: V, b: V) -> Real {
    let d = sub(b, a);
    let dd = dot(d, d);
    let t = if dd > 0.0 {
        (dot(sub(p, a), d) / dd).clamp(0.0, 1.0)
    } else {
        0.0
    };
    dist(p, at(a, d, t))
}

/// Distance between two segments (sampled minimisation refined on the
/// closer one: an estimate accurate far below the margins of these tests).
fn segment_segment(a: V, b: V, c: V, d: V) -> Real {
    let mut best = Real::INFINITY;
    for k in 0..=200 {
        #[allow(clippy::cast_precision_loss)] // sample count
        let t = Real::from(k) / 200.0;
        best = best.min(point_segment(at(a, sub(b, a), t), c, d));
        best = best.min(point_segment(at(c, sub(d, c), t), a, b));
    }
    best
}

/// Distance from `p` to the triangle `t`.
fn point_triangle(p: V, t: &[V; 3]) -> Real {
    let n = cross(sub(t[1], t[0]), sub(t[2], t[0]));
    let nn = dot(n, n);
    if nn > 0.0 {
        // projection inside the triangle?
        let s = dot(sub(p, t[0]), n) / nn;
        let q = at(p, n, -s);
        let inside = (0..3).all(|i| dot(cross(sub(t[(i + 1) % 3], t[i]), sub(q, t[i])), n) >= 0.0);
        if inside {
            return dist(p, q);
        }
    }
    (0..3)
        .map(|i| point_segment(p, t[i], t[(i + 1) % 3]))
        .fold(Real::INFINITY, Real::min)
}

/// Distance between the segment `a`–`b` and the triangle `t` (zero when the
/// segment crosses it).
fn segment_triangle(a: V, b: V, t: &[V; 3]) -> Real {
    let n = cross(sub(t[1], t[0]), sub(t[2], t[0]));
    let (sa, sb) = (dot(sub(a, t[0]), n), dot(sub(b, t[0]), n));
    if sa * sb < 0.0 {
        let x = at(a, sub(b, a), sa / (sa - sb));
        if point_triangle(x, t) == 0.0 {
            return 0.0;
        }
    }
    let mut best = point_triangle(a, t).min(point_triangle(b, t));
    for i in 0..3 {
        best = best.min(segment_segment(a, b, t[i], t[(i + 1) % 3]));
    }
    best
}

fn tri(p: &TrianglePrimitive) -> [V; 3] {
    [v(p.triangle.v[0]), v(p.triangle.v[1]), v(p.triangle.v[2])]
}

/// The grid's reports are the scan's reports in the scan's order, except
/// reports of primitives farther than the margin from the query (rounding
/// artefacts of degenerate configurations, counted in the result).
fn compare<T: std::fmt::Debug, K: PartialEq>(
    scan: &[T],
    grid: &[T],
    key: impl Fn(&T) -> K,
    far: impl Fn(&T) -> bool,
    what: &str,
) -> usize {
    let mut j = 0;
    let mut dropped = 0;
    for s in scan {
        if j < grid.len() && key(s) == key(&grid[j]) {
            j += 1;
        } else {
            assert!(far(s), "{what}: report {s:?} missing from the grid");
            dropped += 1;
        }
    }
    assert_eq!(j, grid.len(), "{what}: the grid reports more than the scan");
    dropped
}

fn check(prims: &[TrianglePrimitive], qs: &[Segment3]) -> (usize, usize, usize) {
    let scan = LinearScan::build(prims).unwrap();
    let grid = GridIndex::build(prims).unwrap();
    let delta = grid.margin().unwrap();
    let (mut hits, mut rays, mut dropped) = (0, 0, 0);
    for q in qs {
        let mut a = Vec::new();
        let mut b = Vec::new();
        let ra = scan.all_intersections(q, &mut a);
        let rb = grid.all_intersections(q, &mut b);
        if ra.is_err() && rb.is_ok() {
            // the scan stopped at the `CGAL_error()` branch on a primitive
            // the grid does not test: a degenerate primitive on the query's
            // line, which must be far from the query
            let first = prims
                .iter()
                .find(|p| triangle_segment_intersection(&p.triangle, q).is_err())
                .unwrap();
            let d = segment_triangle(v(q.source), v(q.target), &tri(first));
            assert!(
                d > delta,
                "{q:?}: the scan's error comes from {first:?} at {d}"
            );
            dropped += 1;
            continue;
        }
        assert_eq!(format!("{ra:?}"), format!("{rb:?}"), "{q:?}");
        if ra.is_err() {
            // both stopped at the `CGAL_error()` branch (the partial outputs
            // are discarded by every caller)
            continue;
        }
        // primitives producing a report (same element id, same
        // intersection), split by their distance to the query
        let producers = |x: &(TriangleSegmentIntersection, u32)| {
            let (mut near, mut far) = (0, 0);
            for p in prims.iter().filter(|p| p.id == x.1) {
                let produces = triangle_segment_intersection(&p.triangle, q)
                    .ok()
                    .flatten()
                    .is_some_and(|y| report_key(&(y, x.1)) == report_key(x));
                if produces {
                    if segment_triangle(v(q.source), v(q.target), &tri(p)) > delta {
                        far += 1;
                    } else {
                        near += 1;
                    }
                }
            }
            (near, far)
        };
        // a missing report is explained by a distant producer; the grid
        // keeps one report per near producer
        dropped += compare(
            &a,
            &b,
            report_key,
            |x| producers(x).1 > 0,
            &format!("{q:?}"),
        );
        for x in &a {
            let kept = b.iter().filter(|y| report_key(y) == report_key(x)).count();
            assert!(kept >= producers(x).0, "{q:?}: {x:?}");
        }
        hits += a.len();
        for r in [Ray3::new(q.source, q.target), Ray3::new(q.target, q.source)] {
            let mut a = Vec::new();
            let mut b = Vec::new();
            scan.all_intersected_primitives(&r, &mut a);
            grid.all_intersected_primitives(&r, &mut b);
            // the element's primitives hit by the ray, split by distance
            let producers = |id: u32| {
                let hit: Vec<&TrianglePrimitive> = prims
                    .iter()
                    .filter(|p| p.id == id && ray_triangle_do_intersect(&p.triangle, &r))
                    .collect();
                let near = hit.iter().filter(|p| ray_gap(p, &r) <= delta).count();
                (near, hit.len() - near)
            };
            dropped += compare(
                &a,
                &b,
                |&id| id,
                |&id| producers(id).1 > 0,
                &format!("{r:?}"),
            );
            for &id in &a {
                let kept = b.iter().filter(|&&j| j == id).count();
                assert!(kept >= producers(id).0, "{r:?}: {id}");
            }
            rays += a.len();
        }
    }
    (hits, rays, dropped)
}

/// Bit pattern of a report (intersection object and primitive id).
fn report_key(r: &(TriangleSegmentIntersection, u32)) -> (u8, [u64; 6], u32) {
    let bits = |p: Point3| [p.x.to_bits(), p.y.to_bits(), p.z.to_bits()];
    match r.0 {
        TriangleSegmentIntersection::Point(p) => {
            let b = bits(p);
            (0, [b[0], b[1], b[2], 0, 0, 0], r.1)
        }
        TriangleSegmentIntersection::Segment(s) => {
            let (a, b) = (bits(s.source), bits(s.target));
            (1, [a[0], a[1], a[2], b[0], b[1], b[2]], r.1)
        }
    }
}

/// Distance from a primitive to the ray (cut at a thousand times its
/// direction vector, far beyond these meshes).
fn ray_gap(p: &TrianglePrimitive, r: &Ray3) -> Real {
    let (s, q) = (v(r.source), v(r.second));
    segment_triangle(s, at(s, sub(q, s), 1e3), &tri(p))
}

#[test]
fn grid_index_reports_the_scan_on_triangles() {
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
    for (n, h, jitter) in [
        (12, 0.1, 0.0),
        (17, 0.1, 0.3),
        (10, 0.25, 0.0),
        (9, 1e5, 0.2),
    ] {
        let (prims, nodes) = tri_mesh(n, h, jitter, &mut rng);
        #[allow(clippy::cast_precision_loss)] // small grids
        let len = n as Real * h;
        let qs = queries(&nodes, 2, len, h, &mut rng);
        let (hits, rays, dropped) = check(&prims, &qs);
        eprintln!(
            "n {n} h {h} jitter {jitter}: {hits} hits, {rays} ray hits, {dropped} far reports of the scan"
        );
        assert!(hits > 500 && rays > 500, "{hits} {rays}");
    }
}

#[test]
fn grid_index_reports_the_scan_on_tetrahedron_faces() {
    let mut rng = Rng(0x0123_4567_89AB_CDEF);
    for (n, h, jitter) in [(4, 0.1, 0.0), (5, 0.1, 0.3), (4, 0.5, 0.0)] {
        let (prims, nodes) = tet_faces(n, h, jitter, &mut rng);
        #[allow(clippy::cast_precision_loss)] // small grids
        let len = n as Real * h;
        let qs = queries(&nodes, 3, len, h, &mut rng);
        let (hits, rays, dropped) = check(&prims, &qs);
        eprintln!(
            "n {n} h {h} jitter {jitter}: {hits} hits, {rays} ray hits, {dropped} far reports of the scan"
        );
        assert!(hits > 500 && rays > 500, "{hits} {rays}");
    }
}

#[test]
fn grid_index_small_and_non_finite_inputs_scan() {
    let mut rng = Rng(7);
    let (prims, _) = tri_mesh(3, 0.1, 0.0, &mut rng);
    assert!(!GridIndex::build(&prims).unwrap().has_grid());
    let (mut prims, nodes) = tri_mesh(8, 0.1, 0.0, &mut rng);
    assert!(GridIndex::build(&prims).unwrap().has_grid());
    // a non-finite query is a full scan: same result as the scan
    let nan = Point3::new(Real::NAN, 0.5, 0.0);
    let q = Segment3::new(nodes[3], nan);
    check(
        &prims,
        &[
            q,
            Segment3::new(nodes[0], Point3::new(Real::INFINITY, 0.1, 0.0)),
        ],
    );
    prims[5].triangle.v[1].x = Real::NAN;
    assert!(!GridIndex::build(&prims).unwrap().has_grid());
}

/// One huge element among many small ones: the grid is kept, its size
/// stays linear (the huge element is listed once per cell), and the
/// reports are the scan's. Many huge elements exceed the item bound: the
/// index scans.
#[test]
fn grid_index_huge_elements_are_bounded() {
    let mut rng = Rng(11);
    let (mut prims, nodes) = tri_mesh(40, 0.025, 0.2, &mut rng);
    let n_small = prims.len();
    let huge = |id: usize| {
        prim(
            id,
            Point3::new(-0.5, -0.5, 0.0),
            Point3::new(2.0, -0.5, 0.0),
            Point3::new(-0.5, 2.0, 0.0),
        )
    };
    prims.push(huge(n_small));
    let grid = GridIndex::build(&prims).unwrap();
    let (cells, items) = grid.grid_size().unwrap();
    // every small triangle spans at most 2 × 2 cells, the huge one all
    assert!(items <= 4 * n_small + cells, "{cells} cells, {items} items");
    assert!(cells <= 4 * prims.len(), "{cells} cells");
    let qs = queries(&nodes, 2, 1.0, 0.025, &mut rng);
    let (hits, _, _) = check(&prims, &qs[..400]);
    assert!(hits > 400);
    // many large primitives: the mean-extent rule makes the cells coarser,
    // the grid stays within its item bound
    for k in 0..n_small / 4 {
        prims[4 * k] = huge(4 * k);
    }
    let grid = GridIndex::build(&prims).unwrap();
    let (cells, items) = grid.grid_size().unwrap();
    assert!(
        items <= 64 * (prims.len() + cells),
        "{cells} cells, {items} items"
    );
    check(&prims, &qs[..100]);
    // a heavy tail (3 % of the primitives a thousand times larger than the
    // others): beyond 64 items per primitive and cell, the index scans
    let (mut prims, _) = tri_mesh(70, 1.0 / 70.0, 0.0, &mut rng);
    let n_small = prims.len();
    for k in 0..3 * n_small / 100 {
        prims.push(prim(
            n_small + k,
            Point3::new(-6.5, -6.5, 0.0),
            Point3::new(7.5, -6.5, 0.0),
            Point3::new(-6.5, 7.5, 0.0),
        ));
    }
    let grid = GridIndex::build(&prims).unwrap();
    assert!(!grid.has_grid(), "{:?}", grid.grid_size());
    check_scan_only(&prims, &qs[..50]);
}

/// Without a grid the index is the scan (same reports, same order).
fn check_scan_only(prims: &[TrianglePrimitive], qs: &[Segment3]) {
    let scan = LinearScan::build(prims).unwrap();
    let grid = GridIndex::build(prims).unwrap();
    for q in qs {
        let (mut a, mut b) = (Vec::new(), Vec::new());
        assert_eq!(
            scan.all_intersections(q, &mut a).is_ok(),
            grid.all_intersections(q, &mut b).is_ok()
        );
        assert_eq!(
            a.iter().map(report_key).collect::<Vec<_>>(),
            b.iter().map(report_key).collect::<Vec<_>>()
        );
    }
}

/// Zero-length queries (inside, outside, on the planes of faces, far away)
/// give the scan's result, its error included; degenerate rays too.
#[test]
fn grid_index_zero_length_queries_are_the_scans() {
    let mut rng = Rng(41);
    for (prims, nodes, dim) in [
        {
            let (p, n) = tet_faces(4, 0.1, 0.0, &mut rng);
            (p, n, 3)
        },
        {
            let (p, n) = tri_mesh(10, 0.1, 0.0, &mut rng);
            (p, n, 2)
        },
    ] {
        let mut points: Vec<Point3> = nodes.clone();
        for _ in 0..200 {
            let z = if dim == 3 {
                2.0 * rng.unit() - 0.5
            } else {
                0.0
            };
            points.push(Point3::new(
                2.0 * rng.unit() - 0.5,
                2.0 * rng.unit() - 0.5,
                z,
            ));
        }
        // outside the mesh on the planes of its faces (z = 0.1 k, x = y)
        for k in 0..5 {
            #[allow(clippy::cast_precision_loss)] // small counts
            let c = 0.1 * k as Real;
            points.push(Point3::new(3.0, -2.0, if dim == 3 { c } else { 0.0 }));
            points.push(Point3::new(
                5.0 + c,
                5.0 + c,
                if dim == 3 { 0.2 } else { 0.0 },
            ));
        }
        let scan = LinearScan::build(&prims).unwrap();
        let grid = GridIndex::build(&prims).unwrap();
        let mut errors = 0;
        for p in points {
            let q = Segment3::new(p, p);
            let (mut a, mut b) = (Vec::new(), Vec::new());
            let ra = scan.all_intersections(&q, &mut a);
            let rb = grid.all_intersections(&q, &mut b);
            assert_eq!(format!("{ra:?}"), format!("{rb:?}"), "{q:?}");
            assert_eq!(format!("{a:?}"), format!("{b:?}"), "{q:?}");
            errors += usize::from(ra.is_err());
            let r = Ray3::new(p, p);
            let (mut a, mut b) = (Vec::new(), Vec::new());
            scan.all_intersected_primitives(&r, &mut a);
            grid.all_intersected_primitives(&r, &mut b);
            assert_eq!(a, b, "{r:?}");
        }
        assert!(errors > 0, "dimension {dim}");
    }
}

/// Degenerate primitives (zero-area faces: repeated or collinear vertices,
/// a point) among regular ones: the grid's reports follow the rule of
/// `check` (the scan's, minus distant noise).
#[test]
fn grid_index_degenerate_primitives() {
    let mut rng = Rng(43);
    let (mut prims, nodes) = tri_mesh(12, 0.1, 0.2, &mut rng);
    let n = prims.len();
    for k in (0..n).step_by(7) {
        let t = prims[k].triangle;
        prims[k].triangle = match k % 3 {
            0 => Triangle3::new(t.v[0], t.v[0], t.v[1]),
            1 => {
                let m = Point3::new(
                    0.5 * (t.v[0].x + t.v[1].x),
                    0.5 * (t.v[0].y + t.v[1].y),
                    0.0,
                );
                Triangle3::new(t.v[0], m, t.v[1])
            }
            _ => Triangle3::new(t.v[2], t.v[2], t.v[2]),
        };
    }
    let qs = queries(&nodes, 2, 1.2, 0.1, &mut rng);
    let (hits, rays, _) = check(&prims, &qs);
    assert!(hits > 500 && rays > 500, "{hits} {rays}");
    let (mut prims3, nodes3) = tet_faces(4, 0.1, 0.3, &mut rng);
    for k in (0..prims3.len()).step_by(5) {
        let t = prims3[k].triangle;
        prims3[k].triangle = Triangle3::new(t.v[0], t.v[1], t.v[1]);
    }
    let qs = queries(&nodes3, 3, 0.4, 0.1, &mut rng);
    let (hits, rays, _) = check(&prims3, &qs);
    assert!(hits > 500 && rays > 500, "{hits} {rays}");
}
