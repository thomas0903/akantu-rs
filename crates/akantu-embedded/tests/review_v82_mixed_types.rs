// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): material_reinforcement_tmpl.hh
// (assembleInternalForcesInterface / assembleStiffnessMatrixInterface per
// (interface type, background type) group; UD-590), checked independently.
// The residual test comes from an independent check; the stiffness and
// random-field test was added with the fixes.

//! At rest (`u = 0`) the residual of a straight pre-stressed bar piece in
//! element `e` is `f(a) = −A σ_p t (N_a(x_out) − N_a(x_in))` (exact
//! integration of `∂N_a/∂s`, linear for `_triangle_6`); summed over the
//! pieces of one primitive segment inside one connected square it
//! telescopes to the shape values at the segment's ends in that square.
//! The reference uses only the bar geometry and the shape functions
//! (no interface mesh), on a mesh of `_triangle_3` (left unit square) and
//! `_triangle_6` (right unit square, separate nodes), with two laws so
//! that law positions differ from interface element ids. It checks the
//! per-group integration/assembly of UD-590 that the mixed patch test does
//! not exercise.

mod common;

use akantu_core::Real;
use akantu_core::enums::AnalysisMethod;
use akantu_core::{ElementType, GhostType};
use akantu_embedded::{EmbeddedInterfaceModelOptions, INTERFACE_FE_ENGINE};
use akantu_mesh::{Mesh, MeshAccessor};
use akantu_model::constitutive_law::LawPhase;

/// `(name, E, A, σ_p, polyline)` of a reinforcement law.
type LawSpec = (&'static str, Real, Real, Real, Vec<[Real; 2]>);

fn mixed_mesh() -> Mesh {
    let mut mesh = Mesh::new(2, "mesh").unwrap();
    let mut t3: Vec<[i32; 3]> = Vec::new();
    let mut t6: Vec<[i32; 6]> = Vec::new();
    {
        let mut acc = MeshAccessor::new(&mut mesh);
        let n = 3usize;
        let id3 = |i: usize, j: usize| i32::try_from(j * (n + 1) + i).unwrap();
        for j in 0..=n {
            for i in 0..=n {
                acc.push_node(&[i as Real / n as Real, j as Real / n as Real])
                    .unwrap();
            }
        }
        for j in 0..n {
            for i in 0..n {
                t3.push([id3(i, j), id3(i + 1, j), id3(i + 1, j + 1)]);
                t3.push([id3(i, j), id3(i + 1, j + 1), id3(i, j + 1)]);
            }
        }
        let base = (n + 1) * (n + 1);
        let g = 5usize;
        let id6 = |i: usize, j: usize| i32::try_from(base + j * g + i).unwrap();
        for j in 0..g {
            for i in 0..g {
                acc.push_node(&[1.0 + i as Real / 4.0, j as Real / 4.0])
                    .unwrap();
            }
        }
        for j in 0..2 {
            for i in 0..2 {
                let (a, b) = (2 * i, 2 * j);
                // corners (a,b) (a+2,b) (a+2,b+2); mids 01, 12, 20
                t6.push([
                    id6(a, b),
                    id6(a + 2, b),
                    id6(a + 2, b + 2),
                    id6(a + 1, b),
                    id6(a + 2, b + 1),
                    id6(a + 1, b + 1),
                ]);
                t6.push([
                    id6(a, b),
                    id6(a + 2, b + 2),
                    id6(a, b + 2),
                    id6(a + 1, b + 1),
                    id6(a + 1, b + 2),
                    id6(a, b + 1),
                ]);
            }
        }
        let mut c = acc.connectivity_mut(ElementType::Triangle3, GhostType::NotGhost);
        for t in &t3 {
            c.try_push_tuple(t).unwrap();
        }
        drop(c);
        let mut c = acc.connectivity_mut(ElementType::Triangle6, GhostType::NotGhost);
        for t in &t6 {
            c.try_push_tuple(t).unwrap();
        }
    }
    common::name_elements(&mut mesh, ElementType::Triangle3, t3.len(), "concrete");
    common::name_elements(&mut mesh, ElementType::Triangle6, t6.len(), "concrete");
    mesh
}

/// Barycentric coordinates of `p` in the triangle of corners `x`.
fn bary(x: &[[Real; 2]; 3], p: [Real; 2]) -> [Real; 3] {
    let d = (x[1][0] - x[0][0]) * (x[2][1] - x[0][1]) - (x[2][0] - x[0][0]) * (x[1][1] - x[0][1]);
    let l1 = ((p[0] - x[0][0]) * (x[2][1] - x[0][1]) - (x[2][0] - x[0][0]) * (p[1] - x[0][1])) / d;
    let l2 = ((x[1][0] - x[0][0]) * (p[1] - x[0][1]) - (p[0] - x[0][0]) * (x[1][1] - x[0][1])) / d;
    [1.0 - l1 - l2, l1, l2]
}

/// Shape values at `p` of the nodes of the element of `ty` that contains
/// `p` (largest smallest barycentric coordinate): `(node, N)` pairs.
fn shapes_at(mesh: &Mesh, ty: ElementType, p: [Real; 2]) -> Vec<(usize, Real)> {
    let conn = mesh.connectivity(ty, GhostType::NotGhost).unwrap();
    let nodes_g = mesh.nodes();
    let xy = |n: i32| {
        let t = nodes_g.get_tuple(usize::try_from(n).unwrap()).unwrap();
        [t[0], t[1]]
    };
    let mut best: Option<(Real, Vec<i32>)> = None;
    for c in conn.tuples() {
        let x = [xy(c[0]), xy(c[1]), xy(c[2])];
        let mn = bary(&x, p).into_iter().fold(Real::INFINITY, Real::min);
        if best.as_ref().is_none_or(|(b, _)| mn > *b) {
            best = Some((mn, c.to_vec()));
        }
    }
    let (mn, c) = best.unwrap();
    assert!(mn >= -1e-12, "point outside the {ty} part");
    let x = [xy(c[0]), xy(c[1]), xy(c[2])];
    let l = bary(&x, p);
    let node = |k: usize| usize::try_from(c[k]).unwrap();
    if ty == ElementType::Triangle3 {
        (0..3).map(|k| (node(k), l[k])).collect()
    } else {
        // mid nodes 3, 4, 5 between corners (0,1), (1,2), (2,0)
        let mut out: Vec<(usize, Real)> = (0..3)
            .map(|k| (node(k), l[k] * (2.0 * l[k] - 1.0)))
            .collect();
        for (m, (i, j)) in [(3, (0, 1)), (4, (1, 2)), (5, (2, 0))] {
            let xm = xy(c[m]);
            let mid = [0.5 * (x[i][0] + x[j][0]), 0.5 * (x[i][1] + x[j][1])];
            assert!((xm[0] - mid[0]).abs() < 1e-14 && (xm[1] - mid[1]).abs() < 1e-14);
            out.push((node(m), 4.0 * l[i] * l[j]));
        }
        out
    }
}

#[test]
fn v82_mixed_types_prestress_residual() {
    // (name, E, A, σ_p, polyline); every primitive segment crosses x = 1 at
    // most once
    let laws: Vec<LawSpec> = vec![
        ("bar_a", 2.0e4, 0.01, 3.5, vec![[0.13, 0.21], [1.87, 0.77]]),
        (
            "bar_b",
            1.5e4,
            0.02,
            -1.25,
            vec![[0.31, 0.93], [1.42, 0.08]],
        ),
        (
            "bar_c",
            2.5e4,
            0.015,
            7.0,
            vec![[1.9, 0.6], [0.2, 0.35], [1.6, 0.1]],
        ),
    ];
    let mut nodes: Vec<Vec<Real>> = Vec::new();
    let mut segs = Vec::new();
    let mut names = Vec::new();
    // interleave the laws' segments so that positions ≠ element ids
    for k in 0..2 {
        for (name, _, _, _, pts) in &laws {
            if k + 1 < pts.len() {
                let first = nodes.len();
                nodes.push(pts[k].to_vec());
                nodes.push(pts[k + 1].to_vec());
                segs.push([
                    i32::try_from(first).unwrap(),
                    i32::try_from(first + 1).unwrap(),
                ]);
                names.push(*name);
            }
        }
    }
    let refs: Vec<&[Real]> = nodes.iter().map(Vec::as_slice).collect();
    let primitive = common::reinforcement_mesh(2, &refs, &segs, &names);
    let mut dat = String::from("material elastic [\n name = concrete\n E = 1e3\n nu = 0.25\n]\n");
    for (name, e, a, sp, _) in &laws {
        dat += &format!(
            "material reinforcement elastic [\n name = {name}\n E = {e:e}\n area = {a:e}\n pre_stress = {sp:e}\n]\n"
        );
    }
    let mesh = mixed_mesh();
    let mut model = common::model(mesh, primitive, &dat);
    model
        .init_full(&EmbeddedInterfaceModelOptions::new(AnalysisMethod::Static))
        .unwrap();
    model.assemble_internal_forces().unwrap();
    let f = model.smm.internal_force().unwrap().as_slice().to_vec();
    let mesh = model.smm.physics.base.mesh.read().unwrap();
    let mut f_ref = vec![0.0; f.len()];
    for (_, _, a, sp, pts) in &laws {
        for w in pts.windows(2) {
            let (p, q) = (w[0], w[1]);
            let l = ((q[0] - p[0]).powi(2) + (q[1] - p[1]).powi(2)).sqrt();
            let t = [(q[0] - p[0]) / l, (q[1] - p[1]) / l];
            // split at x = 1 into the T3 part (x ≤ 1) and the T6 part
            let s = (1.0 - p[0]) / (q[0] - p[0]);
            let xc = [1.0, p[1] + s * (q[1] - p[1])];
            let parts: Vec<([Real; 2], [Real; 2])> = if (0.0..=1.0).contains(&s) {
                vec![(p, xc), (xc, q)]
            } else {
                vec![(p, q)]
            };
            for (x_in, x_out) in parts {
                let xm = 0.5 * (x_in[0] + x_out[0]);
                let ty = if xm < 1.0 {
                    ElementType::Triangle3
                } else {
                    ElementType::Triangle6
                };
                for (sign, x) in [(1.0, x_out), (-1.0, x_in)] {
                    for (node, n) in shapes_at(&mesh, ty, x) {
                        for i in 0..2 {
                            f_ref[node * 2 + i] -= a * sp * t[i] * sign * n;
                        }
                    }
                }
            }
        }
    }
    let scale = laws.iter().fold(0.0_f64, |m, w| m.max((w.2 * w.3).abs()));
    let mut worst: Real = 0.0;
    for (k, (x, y)) in f.iter().zip(&f_ref).enumerate() {
        worst = worst.max((x - y).abs() / scale);
        assert!((x - y).abs() <= 1e-12 * scale, "f_int[{k}] = {x} vs {y}");
    }
    eprintln!("mixed-type pre-stress residual: max |f − f_ref| / max|A σ_p| = {worst:.2e}");
}

/// Corner coordinates and connectivity of every element of `ty`.
fn elements(mesh: &Mesh, ty: ElementType) -> Vec<(Vec<usize>, [[Real; 2]; 3])> {
    let conn = mesh.connectivity(ty, GhostType::NotGhost).unwrap();
    let nodes_g = mesh.nodes();
    conn.tuples()
        .map(|c| {
            let nodes: Vec<usize> = c.iter().map(|&n| usize::try_from(n).unwrap()).collect();
            let xy = |k: usize| {
                let t = nodes_g.get_tuple(nodes[k]).unwrap();
                [t[0], t[1]]
            };
            (nodes.clone(), [xy(0), xy(1), xy(2)])
        })
        .collect()
}

/// Gradients of the barycentric coordinates of the triangle `x`.
fn bary_grads(x: &[[Real; 2]; 3]) -> [[Real; 2]; 3] {
    let d = (x[1][0] - x[0][0]) * (x[2][1] - x[0][1]) - (x[2][0] - x[0][0]) * (x[1][1] - x[0][1]);
    let g1 = [(x[2][1] - x[0][1]) / d, -(x[2][0] - x[0][0]) / d];
    let g2 = [-(x[1][1] - x[0][1]) / d, (x[1][0] - x[0][0]) / d];
    [[-(g1[0] + g2[0]), -(g1[1] + g2[1])], g1, g2]
}

/// Shape-function gradients at `p` of a `_triangle_3` / straight-sided
/// `_triangle_6` with corners `x` (node order of the connectivity).
fn shape_grads(ty: ElementType, x: &[[Real; 2]; 3], p: [Real; 2]) -> Vec<[Real; 2]> {
    let g = bary_grads(x);
    if ty == ElementType::Triangle3 {
        return g.to_vec();
    }
    let l = bary(x, p);
    let mut out: Vec<[Real; 2]> = (0..3)
        .map(|k| [(4.0 * l[k] - 1.0) * g[k][0], (4.0 * l[k] - 1.0) * g[k][1]])
        .collect();
    for (i, j) in [(0, 1), (1, 2), (2, 0)] {
        out.push([
            4.0 * (l[i] * g[j][0] + l[j] * g[i][0]),
            4.0 * (l[i] * g[j][1] + l[j] * g[i][1]),
        ]);
    }
    out
}

/// `em04_*` (mutation M11 and its stiffness analogue): the stiffness matrix and
/// the residual under a random displacement field, with three pre-stressed
/// laws whose segments are interleaved, on the `_triangle_3` + `_triangle_6`
/// background, against a reference built from the bar geometry only: every
/// primitive segment is split at the parameters where it enters or leaves
/// an element (barycentric coordinates, affine along the bar), each
/// sub-interval is integrated in the element containing its midpoint with
/// the element's shape-function gradients at the interface quadrature points
/// (one Gauss point per `_segment_2`, as upstream: the midpoint rule is the
/// discretisation's — not exact for the quadratic stiffness integrand of a
/// `_triangle_6` piece, which an exact integral misses by ≈ 40 % here; the
/// residual at rest, linear along a piece, is exact). The port's residual
/// and stiffness are also checked against each other (`f(u) − f(0) = −K u`).
/// Bulk `E = 0`, so `K` and `f_int` are the reinforcement's.
#[test]
fn em04_mixed_types_stiffness_and_residual_closed_form() {
    let laws: Vec<LawSpec> = vec![
        ("bar_a", 2.0e4, 0.01, 3.5, vec![[0.13, 0.21], [1.87, 0.77]]),
        (
            "bar_b",
            1.5e4,
            0.02,
            -1.25,
            vec![[0.31, 0.93], [1.42, 0.08]],
        ),
        (
            "bar_c",
            2.5e4,
            0.015,
            7.0,
            vec![[1.9, 0.6], [0.2, 0.35], [1.6, 0.1]],
        ),
    ];
    let mut nodes: Vec<Vec<Real>> = Vec::new();
    let mut segs = Vec::new();
    let mut names = Vec::new();
    for k in 0..2 {
        for (name, _, _, _, pts) in &laws {
            if k + 1 < pts.len() {
                let first = nodes.len();
                nodes.push(pts[k].to_vec());
                nodes.push(pts[k + 1].to_vec());
                segs.push([
                    i32::try_from(first).unwrap(),
                    i32::try_from(first + 1).unwrap(),
                ]);
                names.push(*name);
            }
        }
    }
    let refs: Vec<&[Real]> = nodes.iter().map(Vec::as_slice).collect();
    let primitive = common::reinforcement_mesh(2, &refs, &segs, &names);
    let mut dat = String::from("material elastic [\n name = concrete\n E = 0\n nu = 0\n]\n");
    for (name, e, a, sp, _) in &laws {
        dat += &format!(
            "material reinforcement elastic [\n name = {name}\n E = {e:e}\n area = {a:e}\n pre_stress = {sp:e}\n]\n"
        );
    }
    let mut model = common::model(mixed_mesh(), primitive, &dat);
    model
        .init_full(&EmbeddedInterfaceModelOptions::new(AnalysisMethod::Static))
        .unwrap();
    // random displacement field (64-bit LCG)
    let ndof = model.smm.displacement().unwrap().as_slice().len();
    let mut state: u64 = 17;
    let u: Vec<Real> = (0..ndof)
        .map(|_| {
            state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            1e-3 * (((state >> 11) as Real) / ((1u64 << 53) as Real) * 2.0 - 1.0)
        })
        .collect();
    model
        .smm
        .displacement_mut()
        .unwrap()
        .as_mut_slice()
        .copy_from_slice(&u);
    model.assemble_internal_forces().unwrap();
    let f = model.smm.internal_force().unwrap().as_slice().to_vec();
    model.assemble_stiffness_matrix(true).unwrap();
    let kk = model.smm.dof_manager().matrix("K").unwrap();
    let k: Vec<Vec<Real>> = (0..ndof)
        .map(|i| {
            (0..ndof)
                .map(|j| kk.get(i64::try_from(i).unwrap(), i64::try_from(j).unwrap()))
                .collect()
        })
        .collect();
    let energies: Vec<Real> = laws
        .iter()
        .map(|(name, ..)| {
            let idx = model.smm.material_index(name).unwrap();
            model
                .smm
                .with_material(idx, LawPhase::Energy, |m, ctx| {
                    m.energy_of(ctx, "potential")
                })
                .unwrap()
        })
        .collect();

    let mesh = model.smm.physics.base.mesh.read().unwrap();
    let all: Vec<(ElementType, Vec<usize>, [[Real; 2]; 3])> =
        [ElementType::Triangle3, ElementType::Triangle6]
            .into_iter()
            .flat_map(|ty| {
                elements(&mesh, ty)
                    .into_iter()
                    .map(move |(n, x)| (ty, n, x))
            })
            .collect();
    let mut f_ref = vec![0.0; ndof];
    let mut k_ref = vec![vec![0.0; ndof]; ndof];
    // the discretisation's quadrature along a piece: the interface engine's
    // Gauss points (one for `_segment_2`, as upstream: polynomial degree 1),
    // as fractions of the piece with equal weights
    let nq = model
        .fe_engine(INTERFACE_FE_ENGINE)
        .unwrap()
        .nb_integration_points(ElementType::Segment2, GhostType::NotGhost)
        .unwrap();
    let gauss: Vec<Real> = match nq {
        1 => vec![0.5],
        2 => vec![
            0.5 * (1.0 - 1.0 / 3.0_f64.sqrt()),
            0.5 * (1.0 + 1.0 / 3.0_f64.sqrt()),
        ],
        n => panic!("unexpected number of interface quadrature points {n}"),
    };
    let mut energy_errors = Vec::new();
    for (law, (_, e_mod, area, sp, pts)) in laws.iter().enumerate() {
        let mut w_ref = 0.0;
        for w in pts.windows(2) {
            let (p, q) = (w[0], w[1]);
            let len = ((q[0] - p[0]).powi(2) + (q[1] - p[1]).powi(2)).sqrt();
            let t = [(q[0] - p[0]) / len, (q[1] - p[1]) / len];
            let mut breaks = vec![0.0, 1.0];
            for (_, _, x) in &all {
                let (lp, lq) = (bary(x, p), bary(x, q));
                let (mut lo, mut hi) = (0.0_f64, 1.0_f64);
                for a in 0..3 {
                    let d = lq[a] - lp[a];
                    if d > 0.0 {
                        lo = lo.max(-lp[a] / d);
                    } else if d < 0.0 {
                        hi = hi.min(-lp[a] / d);
                    } else if lp[a] < 0.0 {
                        hi = -1.0;
                    }
                }
                if hi - lo > 1e-13 {
                    breaks.push(lo);
                    breaks.push(hi);
                }
            }
            breaks.sort_by(Real::total_cmp);
            breaks.dedup_by(|a, b| (*a - *b).abs() <= 1e-12);
            for s in breaks.windows(2) {
                if s[1] - s[0] <= 1e-12 {
                    continue;
                }
                let at = |r: Real| [p[0] + r * (q[0] - p[0]), p[1] + r * (q[1] - p[1])];
                let mid = at(0.5 * (s[0] + s[1]));
                let (mn, el) = all
                    .iter()
                    .map(|(ty, n, x)| {
                        (
                            bary(x, mid).into_iter().fold(Real::INFINITY, Real::min),
                            (ty, n, x),
                        )
                    })
                    .max_by(|a, b| a.0.total_cmp(&b.0))
                    .unwrap();
                assert!(mn >= -1e-10, "piece outside the mesh");
                let (ty, n, x) = el;
                let piece = (s[1] - s[0]) * len;
                for &g in &gauss {
                    let xg = at(s[0] + g * (s[1] - s[0]));
                    let grads = shape_grads(*ty, x, xg);
                    let ga: Vec<Real> = grads.iter().map(|d| t[0] * d[0] + t[1] * d[1]).collect();
                    let eps: Real = n
                        .iter()
                        .zip(&ga)
                        .map(|(&node, &gi)| gi * (t[0] * u[node * 2] + t[1] * u[node * 2 + 1]))
                        .sum();
                    let sigma = e_mod * eps + sp;
                    let wq = piece / nq as Real;
                    w_ref += wq * area * 0.5 * sigma * eps;
                    for (a, &na) in n.iter().enumerate() {
                        for i in 0..2 {
                            f_ref[na * 2 + i] -= wq * area * sigma * ga[a] * t[i];
                            for (b, &nb) in n.iter().enumerate() {
                                for j in 0..2 {
                                    k_ref[na * 2 + i][nb * 2 + j] +=
                                        wq * area * e_mod * ga[a] * ga[b] * t[i] * t[j];
                                }
                            }
                        }
                    }
                }
            }
        }
        let rel = (energies[law] - w_ref).abs() / w_ref.abs().max(1e-300);
        eprintln!(
            "{}: energy {:.15e} vs {:.15e} (rel {rel:.2e})",
            laws[law].0, energies[law], w_ref
        );
        energy_errors.push(rel);
    }
    let fmax = f_ref.iter().fold(0.0_f64, |m, x| m.max(x.abs()));
    let kmax = k_ref.iter().flatten().fold(0.0_f64, |m, x| m.max(x.abs()));
    let (mut wf, mut wk): (Real, Real) = (0.0, 0.0);
    for r in 0..ndof {
        wf = wf.max((f[r] - f_ref[r]).abs() / fmax);
        for c in 0..ndof {
            wk = wk.max((k[r][c] - k_ref[r][c]).abs() / kmax);
        }
    }
    eprintln!("mixed types, three laws: max rel f {wf:.2e}, K {wk:.2e}");
    // the port's residual and stiffness are consistent: f(u) − f(0) = −K u
    let f0: Vec<Real> = {
        let refs: Vec<&[Real]> = nodes.iter().map(Vec::as_slice).collect();
        let mut m0 = common::model(
            mixed_mesh(),
            common::reinforcement_mesh(2, &refs, &segs, &names),
            &dat,
        );
        m0.init_full(&EmbeddedInterfaceModelOptions::new(AnalysisMethod::Static))
            .unwrap();
        m0.assemble_internal_forces().unwrap();
        m0.smm.internal_force().unwrap().as_slice().to_vec()
    };
    let consistency = (0..ndof).fold(0.0_f64, |m, r| {
        let ku: Real = (0..ndof).map(|c| k[r][c] * u[c]).sum();
        m.max((f[r] - f0[r] + ku).abs() / fmax)
    });
    assert!(consistency <= 1e-12, "f(u) − f(0) + K u: {consistency:e}");
    assert!(wf <= 1e-12, "f_int {wf:e}");
    assert!(wk <= 1e-12, "K {wk:e}");
    assert!(
        energy_errors.iter().all(|&e| e <= 1e-12),
        "energies {energy_errors:?}"
    );
}
