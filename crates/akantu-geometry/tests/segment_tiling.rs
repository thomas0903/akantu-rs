// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/geometry/mesh_segment_intersector_tmpl.hh
// (computeSegments); regression cases for the port's interval tiling
// (UD-599, UD-601).

//! [`PieceAssembly::Tiled`]: the pieces of a query must tile the part of the
//! query inside the mesh exactly — never a duplicate, never a lost piece —
//! for every bar, in particular the adversarial ones (bars
//! through a mesh vertex at a grazing angle to a face, along edges, in
//! interior and boundary faces). The reference is independent of the
//! intersector: the meshes fill the unit square / cube, so the length of a
//! query inside the mesh is the length of the query clipped to the box
//! (Liang–Barsky), and every piece must lie in its element (barycentric
//! coordinates of its midpoint).
//!
//! The collected set (upstream's construction, [`PieceAssembly::Collected`])
//! is checked to still show its defects (UD-601), so that the
//! compatibility path stays what it claims to be.

use akantu_core::{ElementType, GhostType, Real};
use akantu_geometry::kernel::{Point3, Segment3};
use akantu_geometry::{
    LinearScan, MeshSegmentIntersector, PieceAssembly, SegmentIntersectorOptions, TilingReport,
};
use akantu_mesh::{Mesh, MeshAccessor};

/// `n³` cubes over `[0, 1]³`, six positively oriented tetrahedra per cube
/// (Kuhn subdivision), node and element order of the embedded crate's tests.
fn kuhn_grid(n: usize) -> Mesh {
    let mut mesh = Mesh::new(3, "mesh").unwrap();
    let id = |i: usize, j: usize, k: usize| i32::try_from((k * (n + 1) + j) * (n + 1) + i).unwrap();
    let mut coords = Vec::new();
    {
        let mut acc = MeshAccessor::new(&mut mesh);
        for k in 0..=n {
            for j in 0..=n {
                for i in 0..=n {
                    let x = [
                        i as Real / n as Real,
                        j as Real / n as Real,
                        k as Real / n as Real,
                    ];
                    acc.push_node(&x).unwrap();
                    coords.push(x);
                }
            }
        }
        let perms = [
            [0, 1, 2],
            [0, 2, 1],
            [1, 0, 2],
            [1, 2, 0],
            [2, 0, 1],
            [2, 1, 0],
        ];
        let mut c = acc.connectivity_mut(ElementType::Tetrahedron4, GhostType::NotGhost);
        for k in 0..n {
            for j in 0..n {
                for i in 0..n {
                    for p in perms {
                        let mut b = [i, j, k];
                        let mut tet = vec![id(b[0], b[1], b[2])];
                        for axis in p {
                            b[axis] += 1;
                            tet.push(id(b[0], b[1], b[2]));
                        }
                        let x = |t: i32| coords[usize::try_from(t).unwrap()];
                        let (a, bb, cc, d) = (x(tet[0]), x(tet[1]), x(tet[2]), x(tet[3]));
                        let e = |q: [Real; 3]| [q[0] - a[0], q[1] - a[1], q[2] - a[2]];
                        let (e1, e2, e3) = (e(bb), e(cc), e(d));
                        let det = e1[0] * (e2[1] * e3[2] - e2[2] * e3[1])
                            - e1[1] * (e2[0] * e3[2] - e2[2] * e3[0])
                            + e1[2] * (e2[0] * e3[1] - e2[1] * e3[0]);
                        if det < 0. {
                            tet.swap(2, 3);
                        }
                        c.try_push_tuple(&tet).unwrap();
                    }
                }
            }
        }
    }
    mesh
}

/// `n × n` cells over `[0, 1]²`, two linear triangles per cell (diagonal
/// from the lower left corner).
fn triangle_grid(n: usize) -> Mesh {
    let mut mesh = Mesh::new(2, "mesh").unwrap();
    let id = |i: usize, j: usize| i32::try_from(j * (n + 1) + i).unwrap();
    {
        let mut acc = MeshAccessor::new(&mut mesh);
        for j in 0..=n {
            for i in 0..=n {
                acc.push_node(&[i as Real / n as Real, j as Real / n as Real])
                    .unwrap();
            }
        }
        let mut c = acc.connectivity_mut(ElementType::Triangle3, GhostType::NotGhost);
        for j in 0..n {
            for i in 0..n {
                c.try_push_tuple(&[id(i, j), id(i + 1, j), id(i + 1, j + 1)])
                    .unwrap();
                c.try_push_tuple(&[id(i, j), id(i + 1, j + 1), id(i, j + 1)])
                    .unwrap();
            }
        }
    }
    mesh
}

fn intersector(mesh: &Mesh, ty: ElementType, assembly: PieceAssembly) -> MeshSegmentIntersector {
    MeshSegmentIntersector::<LinearScan>::new(
        mesh,
        ty,
        SegmentIntersectorOptions {
            assembly,
            ..SegmentIntersectorOptions::default()
        },
    )
    .unwrap()
}

fn sub(a: Point3, b: Point3) -> [Real; 3] {
    [a.x - b.x, a.y - b.y, a.z - b.z]
}

fn norm(v: [Real; 3]) -> Real {
    (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt()
}

/// Length of `q` inside `[−g, 1 + g]^dim` (Liang–Barsky clipping).
fn box_length(q: &Segment3, dim: usize, g: Real) -> Real {
    let d = sub(q.target, q.source);
    let a = q.source.to_array();
    let (mut t0, mut t1): (Real, Real) = (0.0, 1.0);
    for i in 0..dim {
        if d[i] == 0.0 {
            if !(-g..=1.0 + g).contains(&a[i]) {
                return 0.0;
            }
            continue;
        }
        let (u, v) = ((-g - a[i]) / d[i], (1.0 + g - a[i]) / d[i]);
        t0 = t0.max(u.min(v));
        t1 = t1.min(u.max(v));
    }
    (t1 - t0).max(0.0) * norm(d)
}

/// Length of `q` inside `[0, 1]^dim`.
fn clipped_length(q: &Segment3, dim: usize) -> Real {
    box_length(q, dim, 0.0)
}

/// Point tolerance of the intersector (upstream `Math::tolerance`) in
/// coordinates: within it of the box boundary, "inside" is a tolerance
/// decision (the meshes' coordinates are at most 1).
const BAND: Real = 1e2 * Real::EPSILON;

/// Barycentric coordinates of `p` in the simplex of element `e` (Gaussian
/// elimination with partial pivoting, independent of the crate's code).
fn barycentric(mesh: &Mesh, ty: ElementType, e: usize, p: Point3) -> Vec<Real> {
    let conn = mesh.connectivity(ty, GhostType::NotGhost).unwrap();
    let dim = mesh.spatial_dimension();
    let x: Vec<Vec<Real>> = conn
        .get_tuple(e)
        .unwrap()
        .iter()
        .take(dim + 1)
        .map(|&n| {
            mesh.nodes()
                .get_tuple(usize::try_from(n).unwrap())
                .unwrap()
                .to_vec()
        })
        .collect();
    let n = dim + 1;
    let pc = p.to_array();
    let mut m = vec![vec![0.0; n + 1]; n];
    for r in 0..dim {
        for c in 0..n {
            m[r][c] = x[c][r];
        }
        m[r][n] = pc[r];
    }
    m[dim] = vec![1.0; n + 1];
    for col in 0..n {
        let piv = (col..n)
            .max_by(|&i, &j| m[i][col].abs().total_cmp(&m[j][col].abs()))
            .unwrap();
        m.swap(col, piv);
        let pivot = m[col].clone();
        for (r, row) in m.iter_mut().enumerate() {
            if r != col {
                let f = row[col] / pivot[col];
                for (x, p) in row.iter_mut().zip(&pivot).skip(col) {
                    *x -= f * p;
                }
            }
        }
    }
    (0..n).map(|i| m[i][n] / m[i][i]).collect()
}

/// Check the tiling of the pieces of `q`: ordered along the query without
/// overlap, consecutive pieces meeting (or a gap outside the box), every
/// piece in its element, total length = clipped length (relative 1e-12).
/// Returns the relative length error.
fn check_tiling(
    mesh: &Mesh,
    ty: ElementType,
    q: &Segment3,
    pieces: &[(Segment3, i32)],
    label: &str,
) -> Real {
    let dim = mesh.spatial_dimension();
    let d = sub(q.target, q.source);
    let len = norm(d);
    let dd = d[0] * d[0] + d[1] * d[1] + d[2] * d[2];
    let param = |p: Point3| {
        let v = sub(p, q.source);
        (v[0] * d[0] + v[1] * d[1] + v[2] * d[2]) / dd
    };
    let mut iv: Vec<(Real, Real, &Segment3, i32)> = pieces
        .iter()
        .map(|(s, e)| {
            let (a, b) = (param(s.source), param(s.target));
            (a.min(b), a.max(b), s, *e)
        })
        .collect();
    iv.sort_by(|x, y| x.0.total_cmp(&y.0));
    let tol = 1e-12;
    let mut total = 0.0;
    for (k, (lo, hi, s, e)) in iv.iter().enumerate() {
        assert!(hi > lo, "{label}: degenerate piece {s:?}");
        total += norm(sub(s.target, s.source));
        if k > 0 {
            let prev_hi = iv[k - 1].1;
            assert!(
                *lo >= prev_hi - tol,
                "{label}: pieces overlap at t = {lo} < {prev_hi}"
            );
            if *lo > prev_hi + tol {
                // a gap must lie outside the box
                let tm = 0.5 * (lo + prev_hi);
                let m = [0, 1, 2].map(|i| q.source.to_array()[i] + tm * d[i]);
                assert!(
                    (0..dim).any(|i| m[i] < -tol || m[i] > 1.0 + tol),
                    "{label}: gap [{prev_hi}, {lo}] inside the mesh"
                );
            }
        }
        let mid = Point3::new(
            0.5 * (s.source.x + s.target.x),
            0.5 * (s.source.y + s.target.y),
            0.5 * (s.source.z + s.target.z),
        );
        let lam = barycentric(mesh, ty, usize::try_from(*e).unwrap(), mid);
        let min = lam.iter().copied().fold(Real::INFINITY, Real::min);
        assert!(
            min >= -1e-12,
            "{label}: piece {s:?} not in element {e} (λ_min {min})"
        );
    }
    // exact up to the point tolerance at the mesh boundary: between the
    // strict and the tolerance-grown box
    let (strict, grown) = (clipped_length(q, dim), box_length(q, dim, BAND));
    let err = if total < strict {
        (strict - total) / len
    } else {
        (total - grown).max(0.0) / len
    };
    assert!(
        err <= 1e-12,
        "{label}: length {total:.17} vs [{strict:.17}, {grown:.17}] (relative {err:.2e}); \
         pieces {iv:?}"
    );
    err
}

/// Case `grazing`: through `c` at the angle `delta` to the plane
/// z = c.z, direction (0.6, −0.8, δ).
fn grazing_3d(delta: Real, c: [Real; 3]) -> Segment3 {
    let n = (0.36 + 0.64 + delta * delta).sqrt();
    let d = [0.6 / n, -0.8 / n, delta / n];
    Segment3::new(
        Point3::new(c[0] - 0.3 * d[0], c[1] - 0.3 * d[1], c[2] - 0.3 * d[2]),
        Point3::new(c[0] + 0.35 * d[0], c[1] + 0.35 * d[1], c[2] + 0.35 * d[2]),
    )
}

const THIRD: Real = 1.0 / 3.0;

/// Grazing bars through the vertex (1/3, 2/3, 1/3)
/// and through a face point, every angle: exact length.
#[test]
fn em01_grazing_bars_tile_exactly_3d() {
    let mesh = kuhn_grid(3);
    let tiled = intersector(&mesh, ElementType::Tetrahedron4, PieceAssembly::Tiled);
    let vertex = [THIRD, 2.0 * THIRD, THIRD];
    let face_point = [0.41, 0.52, THIRD];
    for c in [vertex, face_point] {
        for delta in [1e-2, 1e-4, 1e-6, 1e-8, 1e-10, 1e-12, 0.0] {
            let q = grazing_3d(delta, c);
            let (pieces, report) = tiled.compute_segments_with_report(&mesh, &q).unwrap();
            let label = format!("δ {delta:e} at {c:?}");
            check_tiling(&mesh, ElementType::Tetrahedron4, &q, &pieces, &label);
            assert_eq!(report.outside_gaps, 0, "{label}: {report:?}");
        }
    }
}

/// UD-601: the collected set keeps near-duplicate pieces at δ = 1e-4 and
/// loses the piece in tetrahedron 80 at δ = 1e-10; the tiled assembly drops
/// the duplicates and rebuilds the piece.
#[test]
fn ud601_collected_set_defects_and_their_repair() {
    let mesh = kuhn_grid(3);
    let collected = intersector(&mesh, ElementType::Tetrahedron4, PieceAssembly::Collected);
    let tiled = intersector(&mesh, ElementType::Tetrahedron4, PieceAssembly::Tiled);
    let q = grazing_3d(1e-4, [THIRD, 2.0 * THIRD, THIRD]);
    let pieces = collected.compute_segments(&mesh, &q).unwrap();
    let total: Real = pieces
        .iter()
        .map(|(s, _)| norm(sub(s.target, s.source)))
        .sum();
    assert_eq!(pieces.len(), 12);
    assert!(total > 0.65 * 1.7, "collected length {total}");
    let (tp, r) = tiled.compute_segments_with_report(&mesh, &q).unwrap();
    assert_eq!(tp.len(), 4);
    eprintln!("δ 1e-4: {r:?}");
    assert_eq!((r.candidates, r.kept, r.gaps_filled), (12, 4, 0), "{r:?}");
    assert_eq!(r.covered + r.invalid, 8, "{r:?}");
    let q = grazing_3d(1e-10, [THIRD, 2.0 * THIRD, THIRD]);
    let pieces = collected.compute_segments(&mesh, &q).unwrap();
    assert!(pieces.iter().all(|(_, e)| *e != 80), "{pieces:?}");
    let total: Real = pieces
        .iter()
        .map(|(s, _)| norm(sub(s.target, s.source)))
        .sum();
    assert!((total - 0.4119).abs() < 1e-3, "collected length {total}");
    let (tp, r) = tiled.compute_segments_with_report(&mesh, &q).unwrap();
    assert_eq!((r.gaps_filled, r.fill_pieces), (1, 1), "{r:?}");
    assert!(tp.iter().any(|(_, e)| *e == 80));
    check_tiling(&mesh, ElementType::Tetrahedron4, &q, &tp, "δ 1e-10 tiled");
}

/// 3-D adversarial bars (main diagonal = shared edge of six
/// tetrahedra per cube, oblique in an interior grid plane, transversal
/// through a vertex, partly outside, inside one tetrahedron, along a
/// boundary edge, oblique in a boundary face, kinked polyline) and a bar in
/// the boundary face z = 0.
#[test]
fn em03_adversarial_bars_tile_exactly_3d() {
    let mesh = kuhn_grid(3);
    let tiled = intersector(&mesh, ElementType::Tetrahedron4, PieceAssembly::Tiled);
    let collected = intersector(&mesh, ElementType::Tetrahedron4, PieceAssembly::Collected);
    let p = Point3::new;
    let vtx = [THIRD, 2.0 * THIRD, THIRD];
    let dirn = [0.6, -0.48, 0.64];
    let bars = [
        ("diag_edge", p(0.0, 0.0, 0.0), p(1.0, 1.0, 1.0)),
        ("in_face", p(0.05, 0.1, THIRD), p(0.93, 0.77, THIRD)),
        (
            "thru_vertex",
            p(
                vtx[0] - 0.3 * dirn[0],
                vtx[1] - 0.3 * dirn[1],
                vtx[2] - 0.3 * dirn[2],
            ),
            p(
                vtx[0] + 0.35 * dirn[0],
                vtx[1] + 0.35 * dirn[1],
                vtx[2] + 0.35 * dirn[2],
            ),
        ),
        ("outside", p(-0.3, 0.4, 0.5), p(0.6, 0.55, 0.45)),
        ("inside", p(0.21, 0.05, 0.03), p(0.26, 0.07, 0.04)),
        ("bnd_edge", p(0.0, 0.0, 0.0), p(1.0, 0.0, 0.0)),
        ("bnd_face", p(0.0, 0.13, 0.91), p(0.0, 0.88, 0.07)),
        (
            "polyline_a",
            p(0.9, 0.1, 0.2),
            p(2.0 * THIRD, THIRD, 2.0 * THIRD),
        ),
        (
            "polyline_b",
            p(2.0 * THIRD, THIRD, 2.0 * THIRD),
            p(0.15, 0.95, 0.8),
        ),
        ("face_z0", p(0.07, 0.11, 0.0), p(0.89, 0.71, 0.0)),
    ];
    let mut ratios = Vec::new();
    for (label, a, b) in bars {
        let q = Segment3::new(a, b);
        let pieces = tiled.compute_segments(&mesh, &q).unwrap();
        check_tiling(&mesh, ElementType::Tetrahedron4, &q, &pieces, label);
        // the collected set over-counts the bars through a vertex or in a
        // face (UD-599 scope)
        let c: Real = collected
            .compute_segments(&mesh, &q)
            .unwrap()
            .iter()
            .map(|(s, _)| norm(sub(s.target, s.source)))
            .sum();
        ratios.push((label, c / clipped_length(&q, 3)));
    }
    eprintln!("collected length / exact length: {ratios:?}");
    let ratio = |l: &str| ratios.iter().find(|(x, _)| *x == l).unwrap().1;
    assert!(ratio("thru_vertex") > 1.4, "{ratios:?}");
    assert!(ratio("bnd_face") > 1.5, "{ratios:?}");
    assert!(ratio("in_face") > 1.5, "{ratios:?}");
}

/// 2-D: adversarial and grazing bars of the 4 × 4 grid.
#[test]
fn em01_em03_bars_tile_exactly_2d() {
    let mesh = triangle_grid(4);
    let tiled = intersector(&mesh, ElementType::Triangle3, PieceAssembly::Tiled);
    let p = |x: Real, y: Real| Point3::new(x, y, 0.0);
    let mut bars = vec![
        Segment3::new(p(0.0, 0.0), p(1.0, 1.0)),
        Segment3::new(p(0.0, 1.0), p(1.0, 0.0)),
        Segment3::new(p(0.1, 0.5), p(0.9, 0.5)),
        Segment3::new(p(-0.2, 0.3), p(0.55, 0.62)),
        Segment3::new(p(0.03, 0.87), p(0.97, 0.11)),
        Segment3::new(p(0.30, 0.05), p(0.33, 0.07)),
    ];
    for dir0 in [[1.0_f64, 0.0], [1.0, 1.0]] {
        for delta in [1e-2_f64, 1e-4, 1e-6, 1e-8, 1e-10, 1e-12, 0.0] {
            let e = [dir0[0] - delta * dir0[1], dir0[1] + delta * dir0[0]];
            let n = (e[0] * e[0] + e[1] * e[1]).sqrt();
            let d = [e[0] / n, e[1] / n];
            bars.push(Segment3::new(
                p(0.5 - 0.3 * d[0], 0.5 - 0.3 * d[1]),
                p(0.5 + 0.35 * d[0], 0.5 + 0.35 * d[1]),
            ));
        }
    }
    for (k, q) in bars.iter().enumerate() {
        let pieces = tiled.compute_segments(&mesh, q).unwrap();
        check_tiling(
            &mesh,
            ElementType::Triangle3,
            q,
            &pieces,
            &format!("2-D bar {k}"),
        );
    }
}

/// Deterministic generator (64-bit LCG) for the sweeps.
struct Lcg(u64);
impl Lcg {
    fn next(&mut self) -> Real {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        ((self.0 >> 11) as Real) / ((1u64 << 53) as Real)
    }
    fn below(&mut self, n: usize) -> usize {
        ((self.next() * n as Real) as usize).min(n - 1)
    }
}

/// Own sweep (3-D): bars through mesh vertices, edge points, grid-plane
/// points and generic points, along directions in a grid plane, in an
/// element face plane (x = y) or along the main diagonal (shared edges),
/// tilted out of that plane by δ from 0 to 1 rad; partly outside the mesh
/// or not.
#[test]
fn sweep_angles_and_positions_3d() {
    let n = 3;
    let mesh = kuhn_grid(n);
    let tiled = intersector(&mesh, ElementType::Tetrahedron4, PieceAssembly::Tiled);
    let mut rng = Lcg(2026);
    let grid = |k: usize| k as Real / n as Real;
    let mut worst: Real = 0.0;
    let mut stats = TilingReport::default();
    let deltas = [0.0, 1e-14, 1e-12, 1e-10, 1e-8, 1e-6, 1e-4, 1e-2, 0.3, 1.0];
    for case in 0..600 {
        let v = [rng.below(n + 1), rng.below(n + 1), rng.below(n + 1)].map(grid);
        let anchor = match case % 4 {
            0 => v,
            1 => [v[0], v[1], v[2] + rng.next() / n as Real],
            2 => [
                v[0] + rng.next() / n as Real,
                v[1] + rng.next() / n as Real,
                v[2],
            ],
            _ => [rng.next(), rng.next(), rng.next()],
        };
        let delta = deltas[rng.below(deltas.len())];
        let th = 2.0 * std::f64::consts::PI * rng.next();
        let s2 = std::f64::consts::FRAC_1_SQRT_2;
        let (base, normal) = match rng.below(4) {
            0 => ([th.cos(), th.sin(), 0.0], [0.0, 0.0, 1.0]),
            1 => ([th.cos(), 0.0, th.sin()], [0.0, 1.0, 0.0]),
            2 => ([s2 * th.cos(), s2 * th.cos(), th.sin()], [s2, -s2, 0.0]),
            _ => ([1.0, 1.0, 1.0], [1.0, -1.0, 0.0]),
        };
        let d0 = [0, 1, 2].map(|i| base[i] + delta * normal[i]);
        let dn = norm(d0);
        let d = d0.map(|x| x / dn);
        let (s0, s1) = (0.2 + 0.6 * rng.next(), 0.2 + 0.6 * rng.next());
        let q = Segment3::new(
            Point3::new(
                anchor[0] - s0 * d[0],
                anchor[1] - s0 * d[1],
                anchor[2] - s0 * d[2],
            ),
            Point3::new(
                anchor[0] + s1 * d[0],
                anchor[1] + s1 * d[1],
                anchor[2] + s1 * d[2],
            ),
        );
        let (pieces, r) = tiled.compute_segments_with_report(&mesh, &q).unwrap();
        stats.covered += r.covered;
        stats.invalid += r.invalid;
        stats.clipped += r.clipped;
        stats.gaps_filled += r.gaps_filled;
        stats.fill_pieces += r.fill_pieces;
        stats.trimmed += r.trimmed;
        if box_length(&q, 3, BAND) == 0.0 {
            assert!(pieces.is_empty(), "case {case}: pieces outside the mesh");
            continue;
        }
        let label = format!("case {case} δ {delta:e}");
        worst = worst.max(check_tiling(
            &mesh,
            ElementType::Tetrahedron4,
            &q,
            &pieces,
            &label,
        ));
    }
    eprintln!("3-D sweep: worst relative length error {worst:.2e}; {stats:?}");
}

/// Own sweep (2-D): bars through vertices, edge points and generic points
/// along the grid lines and the diagonal edges, tilted by ±δ.
#[test]
fn sweep_angles_and_positions_2d() {
    let n = 4;
    let mesh = triangle_grid(n);
    let tiled = intersector(&mesh, ElementType::Triangle3, PieceAssembly::Tiled);
    let mut rng = Lcg(11);
    let grid = |k: usize| k as Real / n as Real;
    let mut worst: Real = 0.0;
    let deltas = [0.0, 1e-14, 1e-12, 1e-10, 1e-8, 1e-6, 1e-4, 1e-2, 0.3, 1.0];
    for case in 0..800 {
        let v = [rng.below(n + 1), rng.below(n + 1)].map(grid);
        let anchor = match case % 3 {
            0 => v,
            1 => [v[0] + rng.next() / n as Real, v[1]],
            _ => [rng.next(), rng.next()],
        };
        let delta = deltas[rng.below(deltas.len())];
        let base: [Real; 2] = match rng.below(3) {
            0 => [1.0, 0.0],
            1 => [0.0, 1.0],
            _ => [1.0, 1.0],
        };
        let sgn = if rng.next() < 0.5 { 1.0 } else { -1.0 };
        let d0 = [
            base[0] - sgn * delta * base[1],
            base[1] + sgn * delta * base[0],
        ];
        let dn = (d0[0] * d0[0] + d0[1] * d0[1]).sqrt();
        let d = [d0[0] / dn, d0[1] / dn];
        let (s0, s1) = (0.1 + 0.6 * rng.next(), 0.1 + 0.6 * rng.next());
        let q = Segment3::new(
            Point3::new(anchor[0] - s0 * d[0], anchor[1] - s0 * d[1], 0.0),
            Point3::new(anchor[0] + s1 * d[0], anchor[1] + s1 * d[1], 0.0),
        );
        let pieces = tiled.compute_segments(&mesh, &q).unwrap();
        if box_length(&q, 2, BAND) == 0.0 {
            assert!(pieces.is_empty(), "case {case}: pieces outside the mesh");
            continue;
        }
        let label = format!("2-D case {case} δ {delta:e}");
        worst = worst.max(check_tiling(
            &mesh,
            ElementType::Triangle3,
            &q,
            &pieces,
            &label,
        ));
    }
    eprintln!("2-D sweep: worst relative length error {worst:.2e}");
}

/// The tiled assembly refuses upstream's flat tetrahedron triangles and
/// non-finite queries; a degenerate query has no piece.
#[test]
fn tiled_assembly_refusals() {
    let mesh = kuhn_grid(1);
    let options = SegmentIntersectorOptions {
        assembly: PieceAssembly::Tiled,
        ..SegmentIntersectorOptions::upstream()
    };
    assert!(
        MeshSegmentIntersector::<LinearScan>::new(&mesh, ElementType::Tetrahedron4, options)
            .is_err()
    );
    let tiled = intersector(&mesh, ElementType::Tetrahedron4, PieceAssembly::Tiled);
    let nan = Segment3::new(Point3::new(Real::NAN, 0.5, 0.5), Point3::new(0.5, 0.5, 0.5));
    assert!(tiled.compute_segments(&mesh, &nan).is_err());
    let point = Segment3::new(Point3::new(0.3, 0.2, 0.1), Point3::new(0.3, 0.2, 0.1));
    assert!(tiled.compute_segments(&mesh, &point).unwrap().is_empty());
}
