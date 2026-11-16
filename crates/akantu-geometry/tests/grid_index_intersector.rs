// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS); compares the search
// structures under src/geometry/mesh_segment_intersector_tmpl.hh
// (computeSegments).

//! `MeshSegmentIntersector<GridIndex>` against
//! `MeshSegmentIntersector<LinearScan>`: the pieces of every query (both
//! assemblies, both orders) on structured meshes whose coordinates are not
//! exactly representable, with queries along mesh lines, in mesh planes,
//! through vertices, partly outside and random.

use akantu_core::{ElementType, GhostType, Int, Real};
use akantu_geometry::kernel::{Point3, Segment3};
use akantu_geometry::{
    GridIndex, LinearScan, MeshSegmentIntersector, PieceAssembly, SegmentIntersectorOptions,
    SegmentOrder,
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

struct Rng(u64);

impl Rng {
    fn unit(&mut self) -> Real {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        #[allow(clippy::cast_precision_loss)] // 53 random bits
        let x = (self.0.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 11) as Real;
        x / (1u64 << 53) as Real
    }
}

fn queries(mesh: &Mesh, n: usize, rng: &mut Rng) -> Vec<Segment3> {
    let dim = mesh.spatial_dimension();
    let nodes = mesh.nodes();
    let node = |k: usize| {
        let x = nodes.get_tuple(k).unwrap();
        Point3::new(x[0], x[1], if dim == 3 { x[2] } else { 0.0 })
    };
    let nb = mesh.nb_nodes();
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss
    )]
    let pick = |rng: &mut Rng| ((rng.unit() * nb as Real) as usize).min(nb - 1);
    let z = |v: Real| if dim == 3 { v } else { 0.0 };
    let mut out = Vec::new();
    for _ in 0..150 {
        let (a, b) = (node(pick(rng)), node(pick(rng)));
        out.push(Segment3::new(a, b));
        let t = 1.0 + rng.unit();
        out.push(Segment3::new(
            a,
            Point3::new(
                a.x + t * (b.x - a.x),
                a.y + t * (b.y - a.y),
                a.z + t * (b.z - a.z),
            ),
        ));
        let p = |rng: &mut Rng| {
            Point3::new(
                1.4 * rng.unit() - 0.2,
                1.4 * rng.unit() - 0.2,
                z(1.4 * rng.unit() - 0.2),
            )
        };
        out.push(Segment3::new(p(rng), p(rng)));
    }
    // lines along the mesh directions through nodes, and their pieces
    for k in (0..nb).step_by(nb / 40 + 1) {
        let a = node(k);
        for axis in 0..dim {
            let mut lo = a.to_array();
            let mut hi = a.to_array();
            lo[axis] = -0.05;
            hi[axis] = 1.05;
            out.push(Segment3::new(
                Point3::new(lo[0], lo[1], lo[2]),
                Point3::new(hi[0], hi[1], hi[2]),
            ));
            #[allow(clippy::cast_precision_loss)]
            let h = 1.0 / n as Real;
            let mut b = a.to_array();
            b[axis] += 2.5 * h;
            out.push(Segment3::new(a, Point3::new(b[0], b[1], b[2])));
        }
        // diagonals of the cells (Kuhn faces and triangle diagonals)
        let d = 0.37;
        out.push(Segment3::new(a, Point3::new(a.x + d, a.y + d, z(a.z + d))));
        out.push(Segment3::new(a, Point3::new(a.x + d, a.y + d, a.z)));
    }
    out
}

fn compare(mesh: &Mesh, ty: ElementType, n: usize) -> (usize, usize) {
    let mut rng = Rng(0x5DEE_CE66_D1CE_4E5B);
    let qs = queries(mesh, n, &mut rng);
    let mut differ = 0;
    let mut pieces = 0;
    let mut errors = 0;
    for assembly in [PieceAssembly::Tiled, PieceAssembly::Collected] {
        for order in [SegmentOrder::Lexicographic, SegmentOrder::Upstream] {
            let options = SegmentIntersectorOptions {
                assembly,
                order,
                ..SegmentIntersectorOptions::default()
            };
            let scan = MeshSegmentIntersector::<LinearScan>::new(mesh, ty, options).unwrap();
            let grid = MeshSegmentIntersector::<GridIndex>::new(mesh, ty, options).unwrap();
            for q in &qs {
                let (a, b) = match (
                    scan.compute_segments(mesh, q),
                    grid.compute_segments(mesh, q),
                ) {
                    (Ok(a), Ok(b)) => (a, b),
                    // a zero-length query meets the CGAL_error() branch of
                    // the coplanar case on its first coplanar primitive
                    (Err(x), Err(y)) => {
                        assert_eq!(format!("{x:?}"), format!("{y:?}"), "{q:?}");
                        errors += 1;
                        continue;
                    }
                    (x, y) => panic!("{q:?}: scan {x:?}, grid {y:?}"),
                };
                pieces += a.len();
                if a == b {
                    continue;
                }
                // only the collected set may differ: the grid's pieces are
                // the scan's in the scan's order, the missing ones belong to
                // elements far from the query (rounding artefacts of the
                // scan's predicates, which upstream's AABB tree does not
                // report either)
                assert_eq!(assembly, PieceAssembly::Collected, "{q:?}\n{a:?}\n{b:?}");
                let mut j = 0;
                for piece in &a {
                    if j < b.len() && *piece == b[j] {
                        j += 1;
                    } else {
                        let gap = element_gap(mesh, ty, piece.1, q);
                        // the grid's margin is at least an eighth of the
                        // mesh spacing here
                        assert!(gap > 0.125 / n as Real, "{q:?}: {piece:?} (gap {gap})");
                        differ += 1;
                    }
                }
                assert_eq!(j, b.len(), "{q:?}\n{a:?}\n{b:?}");
            }
        }
    }
    assert!(errors < qs.len());
    (pieces, differ)
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

/// Distance between an element (triangle, or the faces of a tetrahedron)
/// and a query.
fn element_gap(mesh: &Mesh, ty: ElementType, e: Int, q: &Segment3) -> Real {
    let conn = mesh.connectivity(ty, GhostType::NotGhost).unwrap();
    let dim = mesh.spatial_dimension();
    let nodes = mesh.nodes();
    let x: Vec<V> = conn
        .get_tuple(usize::try_from(e).unwrap())
        .unwrap()
        .iter()
        .map(|&node| {
            let c = nodes.get_tuple(usize::try_from(node).unwrap()).unwrap();
            [c[0], c[1], if dim == 3 { c[2] } else { 0.0 }]
        })
        .collect();
    let faces: Vec<[V; 3]> = if x.len() == 3 {
        vec![[x[0], x[1], x[2]]]
    } else {
        vec![
            [x[0], x[1], x[2]],
            [x[0], x[1], x[3]],
            [x[0], x[2], x[3]],
            [x[1], x[2], x[3]],
        ]
    };
    faces
        .iter()
        .map(|f| segment_triangle(v(q.source), v(q.target), f))
        .fold(Real::INFINITY, Real::min)
}

#[test]
fn grid_and_scan_pieces_2d() {
    for n in [10, 7] {
        let (pieces, differ) = compare(&triangle_grid(n), ElementType::Triangle3, n);
        eprintln!("2d n {n}: {pieces} pieces, {differ} far pieces of the scan's collected set");
        assert!(pieces > 10_000);
    }
}

#[test]
fn grid_and_scan_pieces_3d() {
    for n in [5, 3] {
        let (pieces, differ) = compare(&kuhn_grid(n), ElementType::Tetrahedron4, n);
        eprintln!("3d n {n}: {pieces} pieces, {differ} far pieces of the scan's collected set");
        assert!(pieces > 10_000);
    }
}
