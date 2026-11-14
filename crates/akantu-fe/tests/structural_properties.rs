// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs (port of Akantu, EPFL/LSMS): independent mathematical
// checks of the structural element kernels (no upstream counterpart).

//! Independent checks: Hermite interpolation of cubic
//! fields through `N·T` on inclined / oriented beams, rigid-body modes in
//! the stiffness null space (beam 2-D: 3, beam 3-D: 6, DKT: 6) and the
//! exact null-space dimension of the element stiffness (3 / 6 / 9 = 6
//! rigid + 3 drilling for DKT), constant
//! strain / curvature patch tests, analytical beam stiffness under
//! rotation, rotation invariance (DKT, 3-D beam), total translational
//! consistent mass `ρAL`, and the error paths (no panic on malformed input).
//! Tolerances are relative to the matrix / field magnitude and stated per
//! check (rounding of short closed-form expressions: ≤ 1e-12).

mod structural_common;

use akantu_core::{Array, ElementType, Error, GhostType, Idx, Real};
use akantu_fe::structural::kernels::{
    DktBendingStiffness, tangent_bernoulli_beam_2, tangent_bernoulli_beam_3,
    tangent_discrete_kirchhoff_triangle_18,
};
use akantu_fe::structural::{EXTRA_NORMAL, StructuralFeEngine, StructuralUpstreamCompat};
use akantu_validation::reduce::NanMinMax;
use structural_common::{
    G, assemble, block_diag, congruence, element_stiffness, engine, matvec, max_abs, mesh,
    mesh_stiffness,
};

type V3 = [Real; 3];

fn cross(a: V3, b: V3) -> V3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn unit(a: V3) -> V3 {
    let l = (a[0] * a[0] + a[1] * a[1] + a[2] * a[2]).sqrt();
    [a[0] / l, a[1] / l, a[2] / l]
}

fn dot(a: V3, b: V3) -> Real {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// Rotation about `axis` by `angle` (Rodrigues), row-major rows.
fn rotation(axis: V3, angle: Real) -> [V3; 3] {
    let k = unit(axis);
    let (c, s) = (angle.cos(), angle.sin());
    let mut r = [[0.0; 3]; 3];
    for (i, row) in r.iter_mut().enumerate() {
        for (j, v) in row.iter_mut().enumerate() {
            let delta = if i == j { 1. } else { 0. };
            let kx = [[0., -k[2], k[1]], [k[2], 0., -k[0]], [-k[1], k[0], 0.]][i][j];
            *v = c * delta + s * kx + (1. - c) * k[i] * k[j];
        }
    }
    r
}

fn apply(r: &[V3; 3], x: V3) -> V3 {
    [dot(r[0], x), dot(r[1], x), dot(r[2], x)]
}

// ---------------------------------------------------------------------------
// Hermite interpolation of cubic fields
// ---------------------------------------------------------------------------

/// Local fields along a straight beam: axial / torsion linear, transverse
/// cubic. Returns values and slopes at arc length `s`.
fn cubic(c: [Real; 4], s: Real) -> (Real, Real) {
    (
        c[0] + c[1] * s + c[2] * s * s + c[3] * s * s * s,
        c[1] + 2. * c[2] * s + 3. * c[3] * s * s,
    )
}

/// `N·T · u_e` reproduces cubic transverse and linear axial fields given in
/// the local basis of an inclined 2-D beam chain (all elements collinear).
#[test]
fn hermite_reproduces_cubic_fields_on_an_inclined_beam_2() {
    let ty = ElementType::BernoulliBeam2;
    let (theta, x0) = (0.6_f64, [0.3, -0.2]);
    let e1 = [theta.cos(), theta.sin()];
    let e2 = [-theta.sin(), theta.cos()];
    let s_nodes = [0.0, 0.7, 1.9, 2.4, 3.6];
    let nodes: Vec<Vec<Real>> = s_nodes
        .iter()
        .map(|&s| vec![x0[0] + s * e1[0], x0[1] + s * e1[1]])
        .collect();
    let conn: Vec<Vec<Idx>> = (0..4).map(|i| vec![i, i + 1]).collect();
    let m = mesh(2, ty, &nodes, &conn, None);
    let fe = engine(&m);
    let (ca, cv) = ([0.1, -0.35, 0., 0.], [0.2, 0.15, -0.4, 0.07]);
    let mut u = Vec::new();
    for &s in &s_nodes {
        let (ul, _) = cubic(ca, s);
        let (vl, dv) = cubic(cv, s);
        u.extend([ul * e1[0] + vl * e2[0], ul * e1[1] + vl * e2[1], dv]);
    }
    let u = Array::from_vec(u, 3).unwrap();
    let mut uq = Array::new(1);
    fe.interpolate_on_integration_points(&m, u.view(), &mut uq, 3, ty, G, None)
        .unwrap();
    let rule = fe.quadrature_rule(ty, G).unwrap();
    let nq = rule.nb_points();
    for (e, c) in conn.iter().enumerate() {
        let (sa, sb) = (s_nodes[c[0] as usize], s_nodes[c[1] as usize]);
        for q in 0..nq {
            let xi = rule.point(q, 1)[0];
            let s = sa + (xi + 1.) * (sb - sa) / 2.;
            let got = &uq.as_slice()[(e * nq + q) * 3..(e * nq + q + 1) * 3];
            let (ul, _) = cubic(ca, s);
            let (vl, dv) = cubic(cv, s);
            for (g, w) in got.iter().zip([ul, vl, dv]) {
                assert!((g - w).abs() < 1e-14, "element {e}, point {q}: {got:?}");
            }
        }
    }
}

/// Oriented 3-D beam: axial / torsion linear, both transverse fields cubic.
/// With the corrected default (UD-293) every row reproduces the local
/// fields, θy included (`θy = −dw/ds`, the DOF convention of the `w` row and
/// of `B`); in legacy mode upstream's θy row evaluates to `+dw/ds`.
#[test]
fn hermite_reproduces_cubic_fields_on_an_oriented_beam_3() {
    hermite_cubic_beam_3(StructuralUpstreamCompat::default());
    hermite_cubic_beam_3(StructuralUpstreamCompat::UPSTREAM);
}

fn hermite_cubic_beam_3(compat: StructuralUpstreamCompat) {
    let ty = ElementType::BernoulliBeam3;
    let x0 = [0.2, -0.1, 0.4];
    let e1 = unit([1.2, 0.8, -0.6]);
    let n = unit(cross(e1, [0.3, -0.8, 0.5]));
    let s_nodes = [0.0, 1.1, 1.7];
    let nodes: Vec<Vec<Real>> = s_nodes
        .iter()
        .map(|&s| (0..3).map(|i| x0[i] + s * e1[i]).collect())
        .collect();
    let conn: Vec<Vec<Idx>> = vec![vec![0, 1], vec![1, 2]];
    let normals = vec![n.to_vec(); 2];
    let m = mesh(3, ty, &nodes, &conn, Some(&normals));
    let fe = StructuralFeEngine::with_precomputation_compat(&m, None, "cubic", compat).unwrap();
    // local basis of upstream's rotation: rows x, -(x × n), n
    let r = fe.rotations(&m, ty, G).unwrap().as_slice()[..36].to_vec();
    let row = |i: usize| [r[i], r[i + 6], r[i + 12]];
    let (b1, b2, b3) = (row(0), row(1), row(2));
    let (ca, cv, cw, ct) = (
        [0.05, 0.2, 0., 0.],
        [0.1, -0.3, 0.25, -0.08],
        [-0.2, 0.12, 0.05, 0.11],
        [0.3, -0.15, 0., 0.],
    );
    let mut u = Vec::new();
    for &s in &s_nodes {
        let (ul, _) = cubic(ca, s);
        let (vl, dv) = cubic(cv, s);
        let (wl, dw) = cubic(cw, s);
        let (tx, _) = cubic(ct, s);
        let disp_l = [ul, vl, wl];
        let rot_l = [tx, -dw, dv];
        for (vals, offset) in [(disp_l, 0), (rot_l, 3)] {
            let _ = offset;
            for i in 0..3 {
                u.push(vals[0] * b1[i] + vals[1] * b2[i] + vals[2] * b3[i]);
            }
        }
    }
    let u = Array::from_vec(u, 6).unwrap();
    let mut uq = Array::new(1);
    fe.interpolate_on_integration_points(&m, u.view(), &mut uq, 6, ty, G, None)
        .unwrap();
    let rule = fe.quadrature_rule(ty, G).unwrap();
    let nq = rule.nb_points();
    for (e, c) in conn.iter().enumerate() {
        let (sa, sb) = (s_nodes[c[0] as usize], s_nodes[c[1] as usize]);
        for q in 0..nq {
            let xi = rule.point(q, 1)[0];
            let s = sa + (xi + 1.) * (sb - sa) / 2.;
            let got = &uq.as_slice()[(e * nq + q) * 6..(e * nq + q + 1) * 6];
            let (ul, _) = cubic(ca, s);
            let (vl, dv) = cubic(cv, s);
            let (wl, dw) = cubic(cw, s);
            let (tx, _) = cubic(ct, s);
            let theta_y = if compat.beam3_theta_y_legacy { dw } else { -dw };
            let want = [ul, vl, wl, tx, theta_y, dv];
            for (g, w) in got.iter().zip(want) {
                assert!(
                    (g - w).abs() < 1e-13,
                    "element {e}, point {q}: {got:?} vs {want:?}"
                );
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Meshes used by several checks
// ---------------------------------------------------------------------------

fn beam_2_chain() -> (Vec<Vec<Real>>, Vec<Vec<Idx>>) {
    (
        vec![
            vec![0.1, -0.3],
            vec![1.37, 0.41],
            vec![0.93, 2.2],
            vec![-1.15, 1.7],
            vec![-0.4, -2.05],
        ],
        vec![vec![0, 1], vec![1, 2], vec![3, 2], vec![3, 4]],
    )
}

/// Nodes, connectivity and normals of a 3-D beam frame.
type Frame = (Vec<Vec<Real>>, Vec<Vec<Idx>>, Vec<Vec<Real>>);

fn beam_3_frame() -> Frame {
    let nodes = vec![
        vec![0.2, -0.1, 0.3],
        vec![1.4, 0.7, -0.5],
        vec![0.6, 2.1, 1.3],
        vec![-1.2, 0.4, 2.2],
    ];
    let conn: Vec<Vec<Idx>> = vec![vec![0, 1], vec![1, 2], vec![2, 3], vec![3, 0]];
    let helpers = [[0., 0., 1.], [1., 0., 0.], [0.3, -0.8, 0.5], [0., 1., 0.]];
    let normals = conn
        .iter()
        .zip(helpers)
        .map(|(c, h)| {
            let (a, b) = (&nodes[c[0] as usize], &nodes[c[1] as usize]);
            unit(cross([b[0] - a[0], b[1] - a[1], b[2] - a[2]], h)).to_vec()
        })
        .collect();
    (nodes, conn, normals)
}

fn dkt_mesh_3d() -> (Vec<Vec<Real>>, Vec<Vec<Idx>>) {
    (
        vec![
            vec![0.1, 0.05, 0.2],
            vec![1.3, -0.2, 0.45],
            vec![0.4, 0.9, -0.3],
            vec![1.6, 1.1, 0.8],
            vec![-0.7, 0.6, 0.1],
            vec![0.9, 2.0, 0.35],
        ],
        vec![
            vec![0, 1, 2],
            vec![1, 3, 2],
            vec![4, 0, 2],
            vec![2, 3, 5],
            vec![4, 2, 5],
        ],
    )
}

fn dkt_tangents(n: usize) -> Vec<Vec<Real>> {
    (0..n)
        .map(|e| {
            tangent_discrete_kirchhoff_triangle_18(
                2.1e3 * (1. + e as Real),
                0.2 + 0.05 * e as Real,
                0.05 + 0.03 * e as Real,
                DktBendingStiffness::Corrected,
            )
            .to_vec()
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Rigid-body modes
// ---------------------------------------------------------------------------

/// `‖K u‖ ≤ 1e-12 ‖K‖ ‖u‖` for every rigid mode `u`, and positive energy for
/// a non-rigid mode.
fn check_null_space(k: &[Real], modes: &[Vec<Real>], deformation: &[Real]) {
    let kmax = max_abs(k);
    for (i, u) in modes.iter().enumerate() {
        let r = matvec(k, u);
        assert!(
            max_abs(&r) <= 1e-12 * kmax * max_abs(u),
            "rigid mode {i}: residual {}",
            max_abs(&r)
        );
    }
    let ku = matvec(k, deformation);
    let energy: Real = ku.iter().zip(deformation).map(|(a, b)| a * b).sum();
    assert!(energy > 1e-6 * kmax * max_abs(deformation).powi(2));
}

/// Eigenvalues of a symmetric column-major `n × n` matrix (cyclic Jacobi,
/// converged when the off-diagonal sum of squares is
/// below `1e-300`).
fn jacobi_eigenvalues(a: &[Real], n: usize) -> Vec<Real> {
    let mut m = a.to_vec();
    for _sweep in 0..100 {
        let off: Real = (0..n)
            .flat_map(|i| (0..n).map(move |j| (i, j)))
            .filter(|(i, j)| i != j)
            .map(|(i, j)| m[i + j * n] * m[i + j * n])
            .sum();
        if off < 1e-300 {
            break;
        }
        for p in 0..n {
            for q in p + 1..n {
                let apq = m[p + q * n];
                if apq == 0. {
                    continue;
                }
                let theta = (m[q + q * n] - m[p + p * n]) / (2. * apq);
                let t = if theta == 0. {
                    1.
                } else {
                    theta.signum() / (theta.abs() + (theta * theta + 1.).sqrt())
                };
                let c = 1. / (t * t + 1.).sqrt();
                let s = t * c;
                for k in 0..n {
                    let (akp, akq) = (m[k + p * n], m[k + q * n]);
                    m[k + p * n] = c * akp - s * akq;
                    m[k + q * n] = s * akp + c * akq;
                }
                for k in 0..n {
                    let (apk, aqk) = (m[p + k * n], m[q + k * n]);
                    m[p + k * n] = c * apk - s * aqk;
                    m[q + k * n] = s * apk + c * aqk;
                }
            }
        }
    }
    (0..n).map(|i| m[i + i * n]).collect()
}

/// One null-space case: type, spatial dimension, nodes, `extra_normal`
/// rows, tangent, expected null-space dimension.
type NullSpaceCase = (
    ElementType,
    usize,
    Vec<Vec<Real>>,
    Option<Vec<Vec<Real>>>,
    Vec<Real>,
    usize,
);

/// The exact null-space dimension of
/// one element stiffness on generic oriented geometry is 3 (beam 2-D), 6
/// (beam 3-D) and 9 (DKT: 6 rigid modes + the 3 local drilling DOFs, which
/// have no stiffness, as upstream). Zero eigenvalues: `|λ|/max|λ| < 1e-12`
/// (measured ≤ 1e-16); the first nonzero one must be clearly
/// separated, `|λ|/max|λ| > 1e-6` (measured 0.067, 0.050, 8.1e-4).
#[test]
fn stiffness_null_space_dimensions() {
    let s = |v: [Real; 3]| {
        let n = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
        [v[0] / n, v[1] / n, v[2] / n]
    };
    // beam 3-D along (1.1, 0.9, 0.4) with a unit normal orthogonal to it
    let axis = s([1.1, 0.9, 0.4]);
    let normal = s(cross(axis, [0.3, -0.8, 0.5]));
    let beam3_nodes = vec![
        vec![0.2, -0.1, 0.3],
        vec![
            0.2 + axis[0] * 1.5,
            -0.1 + axis[1] * 1.5,
            0.3 + axis[2] * 1.5,
        ],
    ];
    let cases: Vec<NullSpaceCase> = vec![
        (
            ElementType::BernoulliBeam2,
            2,
            vec![vec![0.2, -0.1], vec![1.3, 0.8]],
            None,
            tangent_bernoulli_beam_2(2., 3., 5.).to_vec(),
            3,
        ),
        (
            ElementType::BernoulliBeam3,
            3,
            beam3_nodes,
            Some(vec![normal.to_vec()]),
            tangent_bernoulli_beam_3(2., 3., 5., 7., 11.).to_vec(),
            6,
        ),
        (
            ElementType::DiscreteKirchhoffTriangle18,
            3,
            vec![
                vec![0.2, -0.1, 0.3],
                vec![1.3, 0.8, 0.3],
                vec![0.1, 0.5, 1.1],
            ],
            None,
            tangent_discrete_kirchhoff_triangle_18(2., 0.3, 0.2, DktBendingStiffness::Corrected)
                .to_vec(),
            9,
        ),
    ];
    for (ty, dim, nodes, normals, d, expect) in cases {
        let conn: Vec<Vec<Idx>> = vec![(0..ty.nb_nodes_per_element() as Idx).collect()];
        let m = mesh(dim, ty, &nodes, &conn, normals.as_deref());
        let k = mesh_stiffness(&m, ty, &[d]).remove(0);
        let nc = (k.len() as f64).sqrt() as usize;
        assert_eq!(nc * nc, k.len());
        let ev = jacobi_eigenvalues(&k, nc);
        let max = ev.iter().fold(0.0 as Real, |a, b| a.nan_max(b.abs()));
        let mut rel: Vec<Real> = ev.iter().map(|v| v.abs() / max).collect();
        rel.sort_by(Real::total_cmp);
        let zeros = rel.iter().filter(|v| **v < 1e-12).count();
        assert_eq!(zeros, expect, "{ty:?}: {rel:?}");
        assert!(
            rel[expect] > 1e-6,
            "{ty:?}: no gap after the null space: {rel:?}"
        );
        // the matrix is positive semi-definite: no clearly negative eigenvalue
        assert!(ev.iter().all(|v| *v > -1e-12 * max), "{ty:?}: {ev:?}");
    }
}

#[test]
fn rigid_body_modes_beam_2() {
    let ty = ElementType::BernoulliBeam2;
    let (nodes, conn) = beam_2_chain();
    let m = mesh(2, ty, &nodes, &conn, None);
    let fe = engine(&m);
    let d: Vec<Vec<Real>> = (0..conn.len())
        .map(|e| {
            tangent_bernoulli_beam_2(1e3 * (1. + e as Real), 0.4, 0.02 * (1. + e as Real)).to_vec()
        })
        .collect();
    let k = assemble(nodes.len(), 3, &conn, &element_stiffness(&fe, &m, ty, &d));
    let x0 = [0.3, 0.2];
    let modes: Vec<Vec<Real>> = vec![
        nodes.iter().flat_map(|_| [1., 0., 0.]).collect(),
        nodes.iter().flat_map(|_| [0., 1., 0.]).collect(),
        nodes
            .iter()
            .flat_map(|x| [-(x[1] - x0[1]), x[0] - x0[0], 1.])
            .collect(),
    ];
    let deformation: Vec<Real> = (0..nodes.len() * 3)
        .map(|i| (i as Real * 0.37).sin())
        .collect();
    check_null_space(&k, &modes, &deformation);
}

fn rigid_modes_3d(nodes: &[Vec<Real>]) -> Vec<Vec<Real>> {
    let x0 = [0.1, -0.2, 0.3];
    let mut modes = Vec::new();
    for d in 0..3 {
        let mut t = [0.0; 6];
        t[d] = 1.;
        modes.push(nodes.iter().flat_map(|_| t).collect());
    }
    for d in 0..3 {
        let mut w = [0.0; 3];
        w[d] = 1.;
        modes.push(
            nodes
                .iter()
                .flat_map(|x| {
                    let u = cross(w, [x[0] - x0[0], x[1] - x0[1], x[2] - x0[2]]);
                    [u[0], u[1], u[2], w[0], w[1], w[2]]
                })
                .collect(),
        );
    }
    modes
}

#[test]
fn rigid_body_modes_beam_3() {
    let ty = ElementType::BernoulliBeam3;
    let (nodes, conn, normals) = beam_3_frame();
    let m = mesh(3, ty, &nodes, &conn, Some(&normals));
    let fe = engine(&m);
    let d: Vec<Vec<Real>> = (0..conn.len())
        .map(|e| {
            let f = 1. + e as Real;
            tangent_bernoulli_beam_3(2e3 * f, 0.5, 0.02 * f, 0.03 * f, 7. * f).to_vec()
        })
        .collect();
    let k = assemble(nodes.len(), 6, &conn, &element_stiffness(&fe, &m, ty, &d));
    let deformation: Vec<Real> = (0..nodes.len() * 6)
        .map(|i| (i as Real * 0.41).cos())
        .collect();
    check_null_space(&k, &rigid_modes_3d(&nodes), &deformation);
}

#[test]
fn rigid_body_modes_dkt() {
    let ty = ElementType::DiscreteKirchhoffTriangle18;
    let (nodes, conn) = dkt_mesh_3d();
    let m = mesh(3, ty, &nodes, &conn, None);
    let fe = engine(&m);
    let k = assemble(
        nodes.len(),
        6,
        &conn,
        &element_stiffness(&fe, &m, ty, &dkt_tangents(conn.len())),
    );
    let deformation: Vec<Real> = (0..nodes.len() * 6)
        .map(|i| (i as Real * 0.29).sin())
        .collect();
    check_null_space(&k, &rigid_modes_3d(&nodes), &deformation);
}

// ---------------------------------------------------------------------------
// Constant strain / curvature patch tests
// ---------------------------------------------------------------------------

/// Straight inclined beam chain with `u_l = ε s`, `v_l = κ s²/2`: the
/// generalised strains are `(ε, −κ)` at every quadrature point.
#[test]
fn constant_strain_and_curvature_beam_2() {
    let ty = ElementType::BernoulliBeam2;
    let theta = -2.2_f64;
    let (e1, e2) = ([theta.cos(), theta.sin()], [-theta.sin(), theta.cos()]);
    let s_nodes = [0.0, 0.45, 1.3, 1.75, 3.1];
    let nodes: Vec<Vec<Real>> = s_nodes
        .iter()
        .map(|&s| vec![s * e1[0], s * e1[1]])
        .collect();
    let conn: Vec<Vec<Idx>> = (0..4).map(|i| vec![i, i + 1]).collect();
    let m = mesh(2, ty, &nodes, &conn, None);
    let fe = engine(&m);
    let (eps, kappa) = (1.5e-3, -0.25);
    let u: Vec<Real> = s_nodes
        .iter()
        .flat_map(|&s| {
            let (ul, vl) = (eps * s, kappa * s * s / 2.);
            [ul * e1[0] + vl * e2[0], ul * e1[1] + vl * e2[1], kappa * s]
        })
        .collect();
    let u = Array::from_vec(u, 3).unwrap();
    let mut strain = Array::new(1);
    fe.gradient_on_integration_points(&m, u.view(), &mut strain, 3, ty, G, None)
        .unwrap();
    for s in strain.as_slice().chunks(2) {
        assert!(
            (s[0] - eps).abs() < 1e-15 && (s[1] + kappa).abs() < 1e-14,
            "{s:?}"
        );
    }
}

/// Straight oriented 3-D beam: `u = ε s`, `v = κz s²/2`, `w = κy s²/2`,
/// torsion `φ = τ s`: strains `(ε, −κz, −κy, τ)` everywhere.
#[test]
fn constant_strain_and_curvature_beam_3() {
    let ty = ElementType::BernoulliBeam3;
    let e1 = unit([-0.4, 1.1, 0.7]);
    let n = unit(cross(e1, [1., 0.2, 0.]));
    let s_nodes = [0.0, 0.9, 1.4, 2.6];
    let nodes: Vec<Vec<Real>> = s_nodes
        .iter()
        .map(|&s| (0..3).map(|i| 1.0 + s * e1[i]).collect())
        .collect();
    let conn: Vec<Vec<Idx>> = (0..3).map(|i| vec![i, i + 1]).collect();
    let normals = vec![n.to_vec(); 3];
    let m = mesh(3, ty, &nodes, &conn, Some(&normals));
    let fe = engine(&m);
    let r = fe.rotations(&m, ty, G).unwrap().as_slice()[..36].to_vec();
    let row = |i: usize| [r[i], r[i + 6], r[i + 12]];
    let b = [row(0), row(1), row(2)];
    let (eps, kz, ky, tau) = (2e-3, 0.3, -0.45, 0.12);
    let u: Vec<Real> = s_nodes
        .iter()
        .flat_map(|&s| {
            let disp = [eps * s, kz * s * s / 2., ky * s * s / 2.];
            let rot = [tau * s, -ky * s, kz * s];
            let g = |l: V3| -> V3 {
                [0, 1, 2].map(|i| l[0] * b[0][i] + l[1] * b[1][i] + l[2] * b[2][i])
            };
            let (dg, rg) = (g(disp), g(rot));
            [dg[0], dg[1], dg[2], rg[0], rg[1], rg[2]]
        })
        .collect();
    let u = Array::from_vec(u, 6).unwrap();
    let mut strain = Array::new(1);
    fe.gradient_on_integration_points(&m, u.view(), &mut strain, 6, ty, G, None)
        .unwrap();
    for s in strain.as_slice().chunks(4) {
        for (g, w) in s.iter().zip([eps, -kz, -ky, tau]) {
            assert!((g - w).abs() < 1e-13, "{s:?}");
        }
    }
}

/// Planar distorted DKT patch (counter-clockwise triangles in z = 0):
/// isotropic membrane `u = ε (x, y)` and bending `w = −κ (x² + y²)/2` with
/// `θx = ∂w/∂y`, `θy = −∂w/∂x` (Batoz convention of the upstream patch
/// test) give the constant generalised strains `(ε, ε, 0, κ, κ, 0)` at
/// every quadrature point (invariant under the in-plane element bases).
#[test]
fn constant_strain_and_curvature_dkt() {
    let ty = ElementType::DiscreteKirchhoffTriangle18;
    let nodes = vec![
        vec![0., 0., 0.],
        vec![1.3, -0.1, 0.],
        vec![2.1, 0.4, 0.],
        vec![0.2, 1.1, 0.],
        vec![1.15, 0.85, 0.],
        vec![1.9, 1.6, 0.],
    ];
    let conn: Vec<Vec<Idx>> = vec![
        vec![0, 1, 4],
        vec![1, 2, 4],
        vec![0, 4, 3],
        vec![4, 2, 5],
        vec![3, 4, 5],
    ];
    let m = mesh(3, ty, &nodes, &conn, None);
    let fe = engine(&m);
    let (eps, kappa) = (1e-3, 0.7);
    let u: Vec<Real> = nodes
        .iter()
        .flat_map(|x| {
            let w = -kappa * (x[0] * x[0] + x[1] * x[1]) / 2.;
            [eps * x[0], eps * x[1], w, -kappa * x[1], kappa * x[0], 0.]
        })
        .collect();
    let u = Array::from_vec(u, 6).unwrap();
    let mut strain = Array::new(1);
    fe.gradient_on_integration_points(&m, u.view(), &mut strain, 6, ty, G, None)
        .unwrap();
    for s in strain.as_slice().chunks(6) {
        for (g, w) in s.iter().zip([eps, eps, 0., kappa, kappa, 0.]) {
            assert!((g - w).abs() < 1e-12, "{s:?}");
        }
    }
}

// ---------------------------------------------------------------------------
// Analytical stiffness and rotation invariance
// ---------------------------------------------------------------------------

/// Inclined 2-D beam: `K = Tᵀ K_local T` with the classical cantilever
/// entries `EA/l`, `12EI/l³`, `6EI/l²`, `4EI/l`, `2EI/l`.
#[test]
fn analytical_stiffness_of_an_inclined_beam_2() {
    let ty = ElementType::BernoulliBeam2;
    let (theta, l, e, a, i) = (2.4_f64, 1.7, 2e5, 0.03, 4e-4);
    let nodes = vec![
        vec![0.5, -0.2],
        vec![0.5 + l * theta.cos(), -0.2 + l * theta.sin()],
    ];
    let conn: Vec<Vec<Idx>> = vec![vec![0, 1]];
    let m = mesh(2, ty, &nodes, &conn, None);
    let fe = engine(&m);
    let k = &element_stiffness(&fe, &m, ty, &[tangent_bernoulli_beam_2(e, a, i).to_vec()])[0];
    let (l2, l3) = (l * l, l * l * l);
    let kl = [
        [e * a / l, 0., 0., -e * a / l, 0., 0.],
        [
            0.,
            12. * e * i / l3,
            6. * e * i / l2,
            0.,
            -12. * e * i / l3,
            6. * e * i / l2,
        ],
        [
            0.,
            6. * e * i / l2,
            4. * e * i / l,
            0.,
            -6. * e * i / l2,
            2. * e * i / l,
        ],
        [-e * a / l, 0., 0., e * a / l, 0., 0.],
        [
            0.,
            -12. * e * i / l3,
            -6. * e * i / l2,
            0.,
            12. * e * i / l3,
            -6. * e * i / l2,
        ],
        [
            0.,
            6. * e * i / l2,
            2. * e * i / l,
            0.,
            -6. * e * i / l2,
            4. * e * i / l,
        ],
    ];
    let kl: Vec<Real> = (0..36).map(|x| kl[x % 6][x / 6]).collect();
    let (c, s) = (theta.cos(), theta.sin());
    let t = block_diag(&[c, -s, 0., s, c, 0., 0., 0., 1.], 3, 2);
    let expect = congruence(&t, &kl, 6);
    let kmax = max_abs(&expect);
    for (g, w) in k.iter().zip(&expect) {
        assert!((g - w).abs() <= 1e-12 * kmax, "{g} vs {w}");
    }
}

/// DKT and 3-D beam stiffness are frame indifferent: rotating the mesh
/// (and the beam normals) by `Q` gives `Q_blk K Q_blkᵀ`.
#[test]
fn rotation_invariance_dkt_and_beam_3() {
    let q = rotation([0.3, -1.1, 0.6], 0.83);
    let rot_nodes = |nodes: &[Vec<Real>]| -> Vec<Vec<Real>> {
        nodes
            .iter()
            .map(|x| apply(&q, [x[0], x[1], x[2]]).to_vec())
            .collect()
    };
    // Q_blk (column-major) acting on (u, θ) of every node
    let qblk = |nn: usize| -> Vec<Real> {
        let mut r6 = [0.0; 36];
        for i in 0..3 {
            for j in 0..3 {
                r6[i + 6 * j] = q[i][j];
                r6[(i + 3) + 6 * (j + 3)] = q[i][j];
            }
        }
        block_diag(&r6, 6, nn)
    };
    let check = |k: &[Real], kr: &[Real], n: usize| {
        // kr = Qb K Qbᵀ  <=>  Qbᵀ kr Qb = K
        let back = congruence(&qblk(n / 6), kr, n);
        let kmax = max_abs(k);
        for (g, w) in back.iter().zip(k) {
            assert!((g - w).abs() <= 1e-12 * kmax, "{g} vs {w}");
        }
    };
    // DKT
    let ty = ElementType::DiscreteKirchhoffTriangle18;
    let (nodes, conn) = dkt_mesh_3d();
    let d = dkt_tangents(conn.len());
    let k = assemble(
        nodes.len(),
        6,
        &conn,
        &mesh_stiffness(&mesh(3, ty, &nodes, &conn, None), ty, &d),
    );
    let rn = rot_nodes(&nodes);
    let kr = assemble(
        nodes.len(),
        6,
        &conn,
        &mesh_stiffness(&mesh(3, ty, &rn, &conn, None), ty, &d),
    );
    check(&k, &kr, nodes.len() * 6);
    // beam 3
    let ty = ElementType::BernoulliBeam3;
    let (nodes, conn, normals) = beam_3_frame();
    let d: Vec<Vec<Real>> =
        vec![tangent_bernoulli_beam_3(3e3, 0.7, 0.05, 0.02, 9.).to_vec(); conn.len()];
    let k = assemble(
        nodes.len(),
        6,
        &conn,
        &mesh_stiffness(&mesh(3, ty, &nodes, &conn, Some(&normals)), ty, &d),
    );
    let rn = rot_nodes(&nodes);
    let rnorm: Vec<Vec<Real>> = normals
        .iter()
        .map(|x| apply(&q, [x[0], x[1], x[2]]).to_vec())
        .collect();
    let kr = assemble(
        nodes.len(),
        6,
        &conn,
        &mesh_stiffness(&mesh(3, ty, &rn, &conn, Some(&rnorm)), ty, &d),
    );
    check(&k, &kr, nodes.len() * 6);
}

// ---------------------------------------------------------------------------
// Consistent mass: total translational mass
// ---------------------------------------------------------------------------

/// `tᵀ M t = ρ A L |t|²` for a uniform translation `t` (INT-002 check
/// "translational mass sums ρAL"), 2-D chain and 3-D frame.
#[test]
fn consistent_mass_translational_total() {
    for ty in [ElementType::BernoulliBeam2, ElementType::BernoulliBeam3] {
        let (nodes, conn, normals, ndof, ns) = if ty == ElementType::BernoulliBeam2 {
            let (n, c) = beam_2_chain();
            (n, c, None, 3, 2)
        } else {
            let (n, c, nm) = beam_3_frame();
            (n, c, Some(nm), 6, 4)
        };
        let m = mesh(
            ty.spatial_dimension(),
            ty,
            &nodes,
            &conn,
            normals.as_deref(),
        );
        let fe = engine(&m);
        let nqm = akantu_fe::gauss_rule(ty, fe.integration_orders().mass_order(ty))
            .unwrap()
            .nb_points();
        let rho_a: Vec<Real> = (0..conn.len()).map(|e| 2.5 + e as Real).collect();
        let field: Vec<Real> = rho_a
            .iter()
            .flat_map(|&v| std::iter::repeat_n(v, nqm * ns))
            .collect();
        let field = Array::from_vec(field, ns).unwrap();
        let mut me = Array::new(1);
        fe.consistent_mass(&m, field.view(), &mut me, ty, G)
            .unwrap();
        let mats: Vec<Vec<Real>> = me
            .as_slice()
            .chunks(me.nb_component())
            .map(<[Real]>::to_vec)
            .collect();
        let mass = assemble(nodes.len(), ndof, &conn, &mats);
        let total: Real = conn
            .iter()
            .zip(&rho_a)
            .map(|(c, ra)| {
                let (a, b) = (&nodes[c[0] as usize], &nodes[c[1] as usize]);
                ra * a
                    .iter()
                    .zip(b)
                    .map(|(x, y)| (x - y) * (x - y))
                    .sum::<Real>()
                    .sqrt()
            })
            .sum();
        let sdim = ty.spatial_dimension();
        for d in 0..sdim {
            let t: Vec<Real> = (0..nodes.len() * ndof)
                .map(|i| if i % ndof == d { 1. } else { 0. })
                .collect();
            let mt = matvec(&mass, &t);
            let e: Real = mt.iter().zip(&t).map(|(a, b)| a * b).sum();
            assert!(
                (e - total).abs() <= 1e-12 * total,
                "{ty:?} direction {d}: {e} vs {total}"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Error paths (no panic on malformed input)
// ---------------------------------------------------------------------------

#[test]
fn malformed_inputs_are_errors() {
    // beam 3 without extra_normal
    let ty = ElementType::BernoulliBeam3;
    let nodes = vec![vec![0., 0., 0.], vec![1., 2., 3.]];
    let conn: Vec<Vec<Idx>> = vec![vec![0, 1]];
    let m = mesh(3, ty, &nodes, &conn, None);
    assert!(StructuralFeEngine::with_precomputation(&m, None, "e").is_err());
    // extra_normal with the wrong number of components
    let mut m2 = mesh(3, ty, &nodes, &conn, None);
    m2.data_mut()
        .reals_mut_or_insert(EXTRA_NORMAL)
        .unwrap()
        .as_map_mut()
        .replace(
            ty,
            GhostType::NotGhost,
            Array::from_vec(vec![0., 0.], 2).unwrap().into(),
        );
    assert!(matches!(
        StructuralFeEngine::with_precomputation(&m2, None, "e"),
        Err(Error::DimensionMismatch { .. })
    ));
    // beam 2 in a 3-D mesh: the engine handles dimension-3 types only, a
    // beam-2 engine on a 3-D mesh is refused
    let m3 = mesh(3, ElementType::BernoulliBeam2, &nodes, &conn, None);
    assert!(StructuralFeEngine::with_precomputation(&m3, Some(2), "e").is_err());
    // DKT: operations needing N are refused
    let tyd = ElementType::DiscreteKirchhoffTriangle18;
    let md = mesh(
        3,
        tyd,
        &[vec![0., 0., 0.], vec![1., 0., 0.], vec![0., 1., 0.]],
        &[vec![0, 1, 2]],
        None,
    );
    let fe = engine(&md);
    let nq = fe.nb_integration_points(tyd, G).unwrap();
    let u = Array::from_vec(vec![0.0; 18], 6).unwrap();
    let mut out = Array::new(1);
    assert!(matches!(
        fe.interpolate_on_integration_points(&md, u.view(), &mut out, 6, tyd, G, None),
        Err(Error::NotImplemented(_))
    ));
    let b = Array::from_vec(vec![1.0; 6 * nq], 6).unwrap();
    assert!(matches!(
        fe.compute_ntb(&md, b.view(), &mut out, tyd, G, None),
        Err(Error::NotImplemented(_))
    ));
    assert!(matches!(
        fe.compute_ntbn(&md, b.view(), &mut out, tyd, G, None),
        Err(Error::NotImplemented(_))
    ));
    assert!(matches!(
        fe.consistent_mass(&md, b.view(), &mut out, tyd, G),
        Err(Error::NotImplemented(_))
    ));
    // wrong DOF count, wrong component count, filter out of range
    assert!(
        fe.gradient_on_integration_points(&md, u.view(), &mut out, 3, tyd, G, None)
            .is_err()
    );
    let sig = Array::from_vec(vec![0.0; 5 * nq], 5).unwrap();
    assert!(
        fe.compute_btd(&md, sig.view(), &mut out, tyd, G, None)
            .is_err()
    );
    let sig = Array::from_vec(vec![0.0; 6 * nq], 6).unwrap();
    assert!(
        fe.compute_btd(&md, sig.view(), &mut out, tyd, G, Some(&[3]))
            .is_err()
    );
    // not a structural type
    assert!(
        fe.compute_btd(&md, sig.view(), &mut out, ElementType::Triangle3, G, None)
            .is_err()
    );
    // degenerate (zero-length) beam: refused, no panic, no NaN cache
    // (more cases in review_v55_regressions.rs)
    let mz = mesh(
        2,
        ElementType::BernoulliBeam2,
        &[vec![1., 1.], vec![1., 1.]],
        &[vec![0, 1]],
        None,
    );
    assert!(matches!(
        StructuralFeEngine::with_precomputation(&mz, None, "e"),
        Err(Error::Numerical(_))
    ));
}
