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
use akantu_geometry::intersections::TriangleSegmentIntersection;
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
fn tri_mesh(n: usize, h: Real, jitter: Real, rng: &mut Rng) -> (Vec<TrianglePrimitive>, Vec<Point3>) {
    let mut nodes = Vec::new();
    for j in 0..=n {
        for i in 0..=n {
            #[allow(clippy::cast_precision_loss)] // small grids
            let (x, y) = (i as Real * h, j as Real * h);
            let inner = i > 0 && j > 0 && i < n && j < n;
            let (dx, dy) = if inner {
                (jitter * (rng.unit() - 0.5) * h, jitter * (rng.unit() - 0.5) * h)
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
fn tet_faces(n: usize, h: Real, jitter: Real, rng: &mut Rng) -> (Vec<TrianglePrimitive>, Vec<Point3>) {
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

/// Distance between the bounding boxes of a primitive and of two points
/// (largest per-coordinate gap).
fn box_gap(p: &TrianglePrimitive, a: Point3, b: Point3) -> Real {
    let t = p.triangle.bbox();
    let q = Segment3::new(a, b).bbox();
    (0..3)
        .map(|d| (t.min[d] - q.max[d]).max(q.min[d] - t.max[d]))
        .fold(0.0, Real::max)
}

/// The grid's reports are the scan's reports in the scan's order, except
/// reports of primitives farther than the margin from the query (rounding
/// artefacts of degenerate configurations, counted in the result).
fn compare<T: std::fmt::Debug, K: PartialEq>(
    scan: &[T],
    grid: &[T],
    key: impl Fn(&T) -> K,
    prim_of: impl Fn(&T) -> usize,
    far: impl Fn(usize) -> bool,
    what: &str,
) -> usize {
    let mut j = 0;
    let mut dropped = 0;
    for s in scan {
        if j < grid.len() && key(s) == key(&grid[j]) {
            j += 1;
        } else {
            assert!(far(prim_of(s)), "{what}: report {s:?} missing from the grid");
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
    let pos = |id: u32| prims.iter().position(|p| p.id == id).unwrap();
    let (mut hits, mut rays, mut dropped) = (0, 0, 0);
    for q in qs {
        let mut a = Vec::new();
        let mut b = Vec::new();
        let ra = scan.all_intersections(q, &mut a);
        let rb = grid.all_intersections(q, &mut b);
        assert_eq!(ra.is_ok(), rb.is_ok(), "{q:?}");
        // a primitive is far when every primitive with its id is
        let far = |id: u32, a: Point3, b: Point3| {
            prims.iter().filter(|p| p.id == id).all(|p| box_gap(p, a, b) > delta)
        };
        dropped += compare(&a, &b, report_key, |x| pos(x.1), |i| far(prims[i].id, q.source, q.target), &format!("{q:?}"));
        hits += a.len();
        for r in [Ray3::new(q.source, q.target), Ray3::new(q.target, q.source)] {
            let mut a = Vec::new();
            let mut b = Vec::new();
            scan.all_intersected_primitives(&r, &mut a);
            grid.all_intersected_primitives(&r, &mut b);
            // far from the ray: farther than the margin from every point of
            // it inside the mesh box (checked as: not hit by the ray's
            // segment grown by the margin; conservative test below)
            let far_ray = |id: u32| {
                prims.iter().filter(|p| p.id == id).all(|p| ray_gap(p, &r) > delta)
            };
            dropped += compare(&a, &b, |&id| id, |&id| pos(id), |i| far_ray(prims[i].id), &format!("{r:?}"));
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

/// Distance from a primitive's bounding box to the ray (sampled finely
/// along the ray; a lower bound up to the sampling step, which is far below
/// the margins of these meshes).
fn ray_gap(p: &TrianglePrimitive, r: &Ray3) -> Real {
    let t = p.triangle.bbox();
    let d = [r.second.x - r.source.x, r.second.y - r.source.y, r.second.z - r.source.z];
    let s = [r.source.x, r.source.y, r.source.z];
    let mut best = Real::INFINITY;
    for k in 0..20_000 {
        #[allow(clippy::cast_precision_loss)] // sample count
        let u = k as Real * 1e-3;
        let x = [s[0] + u * d[0], s[1] + u * d[1], s[2] + u * d[2]];
        let g = (0..3)
            .map(|i| (t.min[i] - x[i]).max(x[i] - t.max[i]))
            .fold(0.0, Real::max);
        best = best.min(g);
    }
    best
}

#[test]
fn grid_index_reports_the_scan_on_triangles() {
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
    for (n, h, jitter) in [(12, 0.1, 0.0), (17, 0.1, 0.3), (10, 0.25, 0.0), (9, 1e5, 0.2)] {
        let (prims, nodes) = tri_mesh(n, h, jitter, &mut rng);
        #[allow(clippy::cast_precision_loss)] // small grids
        let len = n as Real * h;
        let qs = queries(&nodes, 2, len, h, &mut rng);
        let (hits, rays, dropped) = check(&prims, &qs);
        eprintln!("n {n} h {h} jitter {jitter}: {hits} hits, {rays} ray hits, {dropped} far reports of the scan");
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
        eprintln!("n {n} h {h} jitter {jitter}: {hits} hits, {rays} ray hits, {dropped} far reports of the scan");
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
    check(&prims, &[q, Segment3::new(nodes[0], Point3::new(Real::INFINITY, 0.1, 0.0))]);
    prims[5].triangle.v[1].x = Real::NAN;
    assert!(!GridIndex::build(&prims).unwrap().has_grid());
}
