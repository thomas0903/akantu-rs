// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs (port of Akantu, EPFL/LSMS): independent checks of the
// structural model against closed-form Euler–Bernoulli beam
// solutions and rigid-body invariants. No upstream counterpart.

//! Static closed forms, through the whole model (assembly, DOF manager,
//! Newton–Raphson static solve):
//!
//! * cantilevers (2-D inclined, 3-D arbitrarily oriented with `Iy ≠ Iz`):
//!   tip forces in both bending planes, axial force, torque —
//!   `v = P L³/(3 E Iz)`, `θz = P L²/(2 E Iz)`, `w = P L³/(3 E Iy)`,
//!   `θy = −P L²/(2 E Iy)` (the DOF convention `θy = −dw/dx`), `u = N L/EA`,
//!   `θx = T L/GJ` — Hermite elements are nodally exact;
//! * line loads through `compute_forces_by_{local,global}_traction_array`:
//!   a uniform transverse load `q` (`v(x) = q x²(6L² − 4Lx + x²)/(24 EI)` at
//!   every node) and **uniform distributed moments**: `∫ m θ dx` equals the
//!   work of a tip force `m` (about z) or `−m` (about y, since `θy = −w'`),
//!   so a distributed moment `m_y` must give `w_tip = −m_y L³/(3 E Iy)` —
//!   the independent check of the corrected θy shape row (UD-293); the
//!   upstream row gives the opposite sign (checked too);
//! * rigid-body modes of the assembled `K` (2-D and 3-D frames of beams in
//!   several orientations): `K r = 0` for the translations and rotations
//!   (`u = ω × x`, `θ = ω`), and the number of zero eigenvalues.

mod structural_model_common;

use akantu_core::enums::AnalysisMethod;
use akantu_core::{Array, ElementType, Idx, Real};
use akantu_mesh::Mesh;
use akantu_structural::{
    StructuralMaterial, StructuralMechanicsModel, StructuralModelUpstreamCompat,
};
use structural_model_common::*;

fn static_model(
    mesh: Mesh,
    mat: StructuralMaterial,
    compat: StructuralModelUpstreamCompat,
) -> StructuralMechanicsModel {
    let mut m =
        StructuralMechanicsModel::with_compat(shared(mesh), None, "closed_form", compat).unwrap();
    m.add_material(mat, "").unwrap();
    m.init_full_with(AnalysisMethod::Static).unwrap();
    m
}

fn rel_close(got: Real, want: Real, tol: Real, what: &str) {
    let scale = want.abs().max(Real::MIN_POSITIVE);
    assert!(
        (got - want).abs() <= tol * scale,
        "{what}: {got} vs {want} (rel {:e})",
        (got - want).abs() / scale
    );
}

/// Nodally exact closed forms of the Hermite beams: relative tolerance of
/// a few hundred ulps (the static solve of a well-conditioned small system).
const EXACT: Real = 1e-11;

fn cross(a: [Real; 3], b: [Real; 3]) -> [Real; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn normalized(a: [Real; 3]) -> [Real; 3] {
    let n = (a[0] * a[0] + a[1] * a[1] + a[2] * a[2]).sqrt();
    [a[0] / n, a[1] / n, a[2] / n]
}

fn dot3(a: [Real; 3], b: &[Real]) -> Real {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

// ---------------------------------------------------------------------------
// 2-D cantilever
// ---------------------------------------------------------------------------

fn beam2_material() -> StructuralMaterial {
    StructuralMaterial {
        e: 2.1e11,
        a: 3e-3,
        i: 4.5e-6,
        ..StructuralMaterial::default()
    }
}

/// Inclined 2-D cantilever (angle 30°, 7 elements, length 3), clamped at
/// the origin; tip force along the local normal, tip moment, axial force.
#[test]
fn cantilever_2d_tip_loads_inclined() {
    let (len, ne) = (3.0, 7);
    let alpha: Real = 30f64.to_radians();
    let d = [alpha.cos(), alpha.sin()];
    let nrm = [-alpha.sin(), alpha.cos()];
    let mat = beam2_material();
    let mesh = line_mesh(2, ElementType::BernoulliBeam2, ne, len, &[0., 0.], &d, None);
    let mut m = static_model(mesh, mat, StructuralModelUpstreamCompat::default());
    m.blocked_dofs_mut().unwrap().as_mut_slice()[..3].fill(true);
    let (p, mm, n) = (1.5e3, 2e3, 4e4);
    {
        let mut f_guard = m.external_force_mut().unwrap();
        let f = f_guard.as_mut_slice();
        let t = ne * 3;
        f[t] = p * nrm[0] + n * d[0];
        f[t + 1] = p * nrm[1] + n * d[1];
        f[t + 2] = mm;
    }
    m.solve_step(None).unwrap();
    let u = m.displacement().unwrap().tuple(ne).to_vec();
    let (e, a, i) = (mat.e, mat.a, mat.i);
    let v_local = u[0] * nrm[0] + u[1] * nrm[1];
    let u_local = u[0] * d[0] + u[1] * d[1];
    rel_close(
        v_local,
        p * len.powi(3) / (3. * e * i) + mm * len * len / (2. * e * i),
        EXACT,
        "tip deflection",
    );
    rel_close(
        u[2],
        p * len * len / (2. * e * i) + mm * len / (e * i),
        EXACT,
        "tip rotation",
    );
    rel_close(u_local, n * len / (e * a), EXACT, "axial elongation");
}

/// Uniform transverse load `q` on a 2-D cantilever through the global
/// traction array of an inclined beam (and the local array on a straight
/// one): every node matches `v(x) = q x²(6L² − 4Lx + x²)/(24 EI)` and the
/// rotation `θ(x) = q x(3L² − 3Lx + x²)/(6 EI)`.
#[test]
fn cantilever_2d_uniform_line_load() {
    let (len, ne) = (2.5, 6);
    let mat = beam2_material();
    let q = -7e3;
    for inclined in [false, true] {
        let alpha: Real = if inclined { -50f64.to_radians() } else { 0.0 };
        let d = [alpha.cos(), alpha.sin()];
        let nrm = [-alpha.sin(), alpha.cos()];
        let mesh = line_mesh(2, ElementType::BernoulliBeam2, ne, len, &[1., 2.], &d, None);
        let mut m = static_model(mesh, mat, StructuralModelUpstreamCompat::default());
        m.blocked_dofs_mut().unwrap().as_mut_slice()[..3].fill(true);
        let nq = m
            .fe_engine()
            .nb_integration_points(ElementType::BernoulliBeam2, G)
            .unwrap();
        let ty = ElementType::BernoulliBeam2;
        if inclined {
            let t: Vec<Real> = (0..ne * nq)
                .flat_map(|_| [q * nrm[0], q * nrm[1], 0.])
                .collect();
            m.compute_forces_by_global_traction_array(&Array::from_vec(t, 3).unwrap(), ty)
                .unwrap();
        } else {
            let t: Vec<Real> = (0..ne * nq).flat_map(|_| [0., q, 0.]).collect();
            m.compute_forces_by_local_traction_array(&Array::from_vec(t, 3).unwrap(), ty)
                .unwrap();
        }
        m.solve_step(None).unwrap();
        let u = m.displacement().unwrap();
        let ei = mat.e * mat.i;
        for node in 0..=ne {
            let x = len * node as Real / ne as Real;
            let t = u.tuple(node);
            let v = t[0] * nrm[0] + t[1] * nrm[1];
            let v_exact = q * x * x * (6. * len * len - 4. * len * x + x * x) / (24. * ei);
            let th_exact = q * x * (3. * len * len - 3. * len * x + x * x) / (6. * ei);
            if node > 0 {
                rel_close(v, v_exact, EXACT, &format!("v at node {node}"));
                rel_close(t[2], th_exact, EXACT, &format!("theta at node {node}"));
            }
        }
    }
}

/// Uniform distributed moment `m` on a 2-D cantilever = tip force `m`.
#[test]
fn cantilever_2d_distributed_moment() {
    let (len, ne) = (4.0, 5);
    let mat = beam2_material();
    let mz = 900.0;
    let mesh = line_mesh(
        2,
        ElementType::BernoulliBeam2,
        ne,
        len,
        &[0., 0.],
        &[1., 0.],
        None,
    );
    let mut m = static_model(mesh, mat, StructuralModelUpstreamCompat::default());
    m.blocked_dofs_mut().unwrap().as_mut_slice()[..3].fill(true);
    let ty = ElementType::BernoulliBeam2;
    let nq = m.fe_engine().nb_integration_points(ty, G).unwrap();
    let t: Vec<Real> = (0..ne * nq).flat_map(|_| [0., 0., mz]).collect();
    m.compute_forces_by_local_traction_array(&Array::from_vec(t, 3).unwrap(), ty)
        .unwrap();
    m.solve_step(None).unwrap();
    let tip = m.displacement().unwrap().tuple(ne).to_vec();
    let ei = mat.e * mat.i;
    rel_close(
        tip[1],
        mz * len.powi(3) / (3. * ei),
        EXACT,
        "tip deflection",
    );
    rel_close(tip[2], mz * len * len / (2. * ei), EXACT, "tip rotation");
}

// ---------------------------------------------------------------------------
// 3-D cantilever
// ---------------------------------------------------------------------------

fn beam3_material() -> StructuralMaterial {
    StructuralMaterial {
        e: 7e10,
        a: 2e-3,
        iz: 3e-6,
        iy: 1.2e-6,
        gj: 9e4,
        ..StructuralMaterial::default()
    }
}

/// Local frame of a 3-D beam: `e1` = axis, `e3` = the normal
/// (`extra_normal`, the `w` direction), `e2 = e3 × e1`.
fn frame() -> ([Real; 3], [Real; 3], [Real; 3]) {
    let e1 = normalized([1., 2., 2.]);
    let e3 = normalized(cross(e1, [0., 0., 1.]));
    let e2 = cross(e3, e1);
    (e1, e2, e3)
}

fn cantilever_3d(
    compat: StructuralModelUpstreamCompat,
    ne: usize,
    len: Real,
) -> StructuralMechanicsModel {
    let (e1, _, e3) = frame();
    let mesh = line_mesh(
        3,
        ElementType::BernoulliBeam3,
        ne,
        len,
        &[0.5, -1., 2.],
        &e1,
        Some(&e3),
    );
    let mut m = static_model(mesh, beam3_material(), compat);
    m.blocked_dofs_mut().unwrap().as_mut_slice()[..6].fill(true);
    m
}

/// The element rotations are the frame `(e1, e2, e3)` (rows of `R`, local
/// = `R` · global) — the convention the closed forms below rely on.
#[test]
fn beam3_rotation_is_the_local_frame() {
    let (e1, e2, e3) = frame();
    let m = cantilever_3d(StructuralModelUpstreamCompat::default(), 3, 2.0);
    let r = m.rotations(ElementType::BernoulliBeam3, G).unwrap();
    for e in 0..3 {
        let r = r.tuple(e);
        for (row, ei) in [e1, e2, e3].iter().enumerate() {
            for c in 0..3 {
                assert!(
                    (r[row + 6 * c] - ei[c]).abs() <= 1e-15,
                    "R({row},{c}) = {} vs {}",
                    r[row + 6 * c],
                    ei[c]
                );
            }
        }
    }
}

/// Tip forces along `e2` and `e3`, axial force, torque on an oriented 3-D
/// cantilever with `Iy ≠ Iz` (both modes: point loads do not involve the
/// θy shape row).
#[test]
fn cantilever_3d_tip_loads_both_bending_planes() {
    let (e1, e2, e3) = frame();
    let (len, ne) = (2.0, 4);
    let mat = beam3_material();
    for compat in [
        StructuralModelUpstreamCompat::default(),
        StructuralModelUpstreamCompat::UPSTREAM,
    ] {
        let mut m = cantilever_3d(compat, ne, len);
        let (p2, p3, n, t) = (120., -80., 5e3, 33.);
        {
            let mut f_guard = m.external_force_mut().unwrap();
            let f = f_guard.as_mut_slice();
            let k = ne * 6;
            for c in 0..3 {
                f[k + c] = p2 * e2[c] + p3 * e3[c] + n * e1[c];
                f[k + 3 + c] = t * e1[c];
            }
        }
        m.solve_step(None).unwrap();
        let tip = m.displacement().unwrap().tuple(ne).to_vec();
        let (u, th) = (&tip[..3], &tip[3..]);
        let (e, a, iz, iy, gj) = (mat.e, mat.a, mat.iz, mat.iy, mat.gj);
        rel_close(dot3(e1, u), n * len / (e * a), EXACT, "axial");
        rel_close(
            dot3(e2, u),
            p2 * len.powi(3) / (3. * e * iz),
            EXACT,
            "v (local y)",
        );
        rel_close(
            dot3(e3, u),
            p3 * len.powi(3) / (3. * e * iy),
            EXACT,
            "w (local z)",
        );
        rel_close(dot3(e1, th), t * len / gj, EXACT, "torsion");
        rel_close(
            dot3(e3, th),
            p2 * len * len / (2. * e * iz),
            EXACT,
            "theta_z",
        );
        rel_close(
            dot3(e2, th),
            -p3 * len * len / (2. * e * iy),
            EXACT,
            "theta_y",
        );
    }
}

/// Uniform transverse loads in both bending planes through the global
/// traction array: nodal deflections `q x²(6L² − 4Lx + x²)/(24 EI)` with
/// `Iz` (local y) and `Iy` (local z).
#[test]
fn cantilever_3d_uniform_line_loads_both_planes() {
    let (e1, e2, e3) = frame();
    let (len, ne) = (3.0, 5);
    let mat = beam3_material();
    let (q2, q3) = (-450., 300.);
    let mut m = cantilever_3d(StructuralModelUpstreamCompat::default(), ne, len);
    let ty = ElementType::BernoulliBeam3;
    let nq = m.fe_engine().nb_integration_points(ty, G).unwrap();
    let t: Vec<Real> = (0..ne * nq)
        .flat_map(|_| {
            [
                q2 * e2[0] + q3 * e3[0],
                q2 * e2[1] + q3 * e3[1],
                q2 * e2[2] + q3 * e3[2],
                0.,
                0.,
                0.,
            ]
        })
        .collect();
    m.compute_forces_by_global_traction_array(&Array::from_vec(t, 6).unwrap(), ty)
        .unwrap();
    m.solve_step(None).unwrap();
    let u = m.displacement().unwrap();
    for node in 1..=ne {
        let x = len * node as Real / ne as Real;
        let shape = x * x * (6. * len * len - 4. * len * x + x * x) / 24.;
        let t = u.tuple(node);
        assert!(dot3(e1, t).abs() <= 1e-12 * shape.abs() * q2.abs() / (mat.e * mat.iy));
        rel_close(dot3(e2, t), q2 * shape / (mat.e * mat.iz), EXACT, "v");
        rel_close(dot3(e3, t), q3 * shape / (mat.e * mat.iy), EXACT, "w");
    }
}

/// Uniform distributed moments about the local axes: `m_z` ≡ tip force `m_z`
/// along `e2`; `m_y` ≡ tip force `−m_y` along `e3` (θy = −w'); torque `m_x`
/// gives `θx(x) = m_x x(2L − x)/(2 GJ)`. The corrected θy row (default)
/// satisfies the `m_y` closed form; the upstream row (UD-293) gives the
/// opposite deflection.
#[test]
fn cantilever_3d_distributed_moments_theta_y_convention() {
    let (e1, e2, e3) = frame();
    let (len, ne) = (2.0, 4);
    let mat = beam3_material();
    let (mx, my, mz) = (5., 70., -40.);
    let run = |compat: StructuralModelUpstreamCompat| {
        let mut m = cantilever_3d(compat, ne, len);
        let ty = ElementType::BernoulliBeam3;
        let nq = m.fe_engine().nb_integration_points(ty, G).unwrap();
        // local tractions: (fx, fy, fz, mx, my, mz) in the element basis
        let t: Vec<Real> = (0..ne * nq)
            .flat_map(|_| [0., 0., 0., mx, my, mz])
            .collect();
        m.compute_forces_by_local_traction_array(&Array::from_vec(t, 6).unwrap(), ty)
            .unwrap();
        m.solve_step(None).unwrap();
        m.displacement().unwrap().clone()
    };
    let (e, iz, iy, gj) = (mat.e, mat.iz, mat.iy, mat.gj);
    let u = run(StructuralModelUpstreamCompat::default());
    let tip = u.tuple(ne);
    rel_close(
        dot3(e2, &tip[..3]),
        mz * len.powi(3) / (3. * e * iz),
        EXACT,
        "v from m_z",
    );
    rel_close(
        dot3(e3, &tip[..3]),
        -my * len.powi(3) / (3. * e * iy),
        EXACT,
        "w from m_y",
    );
    rel_close(
        dot3(e2, &tip[3..]),
        my * len * len / (2. * e * iy),
        EXACT,
        "theta_y from m_y",
    );
    for node in 1..=ne {
        let x = len * node as Real / ne as Real;
        rel_close(
            dot3(e1, &u.tuple(node)[3..]),
            mx * x * (2. * len - x) / (2. * gj),
            EXACT,
            &format!("torsion at node {node} (linear elements: exact at nodes)"),
        );
    }
    let legacy = run(StructuralModelUpstreamCompat::UPSTREAM);
    let tip_l = legacy.tuple(ne);
    rel_close(
        dot3(e3, &tip_l[..3]),
        my * len.powi(3) / (3. * e * iy),
        EXACT,
        "legacy w (sign flipped)",
    );
}

// ---------------------------------------------------------------------------
// rigid-body modes of K
// ---------------------------------------------------------------------------

fn frame_2d() -> Mesh {
    let nodes = vec![
        vec![0., 0.],
        vec![1., 0.5],
        vec![2.5, 0.2],
        vec![1.2, -1.3],
        vec![3.0, 1.7],
    ];
    let conn: Vec<Vec<Idx>> = vec![vec![0, 1], vec![1, 2], vec![1, 3], vec![3, 2], vec![2, 4]];
    mesh_of(2, ElementType::BernoulliBeam2, &nodes, &conn, None)
}

fn frame_3d() -> Mesh {
    let nodes = vec![
        vec![0., 0., 0.],
        vec![1., 0.5, 0.2],
        vec![2.5, 0.2, -0.4],
        vec![1.2, -1.3, 0.9],
        vec![3.0, 1.7, 1.1],
    ];
    let conn: Vec<Vec<Idx>> = vec![vec![0, 1], vec![1, 2], vec![1, 3], vec![3, 2], vec![2, 4]];
    // normals orthogonal to each beam axis (any non-parallel vector would
    // do for the rigid modes; orthogonal unit normals keep R a rotation)
    let normals: Vec<Vec<Real>> = conn
        .iter()
        .map(|c| {
            let (a, b) = (&nodes[c[0] as usize], &nodes[c[1] as usize]);
            let d = normalized([b[0] - a[0], b[1] - a[1], b[2] - a[2]]);
            normalized(cross(d, [0.3, -0.2, 1.0])).to_vec()
        })
        .collect();
    mesh_of(
        3,
        ElementType::BernoulliBeam3,
        &nodes,
        &conn,
        Some(&normals),
    )
}

fn rigid_modes(m: &StructuralMechanicsModel, dim: usize) -> Vec<Vec<Real>> {
    let x = m.physics.base.mesh.read().unwrap().nodes().clone();
    let nn = x.len();
    let ndof = m.nb_degree_of_freedom();
    let mut modes = Vec::new();
    for t in 0..dim {
        let mut r = vec![0.0; nn * ndof];
        for n in 0..nn {
            r[n * ndof + t] = 1.;
        }
        modes.push(r);
    }
    let rotations: Vec<[Real; 3]> = if dim == 2 {
        vec![[0., 0., 1.]]
    } else {
        vec![[1., 0., 0.], [0., 1., 0.], [0., 0., 1.]]
    };
    for w in rotations {
        let mut r = vec![0.0; nn * ndof];
        for n in 0..nn {
            let p = x.tuple(n);
            let p3 = [p[0], p[1], if dim == 3 { p[2] } else { 0. }];
            let u = cross(w, p3);
            for c in 0..dim {
                r[n * ndof + c] = u[c];
            }
            if dim == 2 {
                r[n * ndof + 2] = w[2];
            } else {
                r[n * ndof + 3..n * ndof + 6].copy_from_slice(&w);
            }
        }
        modes.push(r);
    }
    modes
}

#[test]
fn rigid_body_modes_of_the_assembled_stiffness() {
    for (mesh, dim, mat) in [
        (frame_2d(), 2, beam2_material()),
        (frame_3d(), 3, beam3_material()),
    ] {
        let mut m = static_model(mesh, mat, StructuralModelUpstreamCompat::default());
        m.assemble_stiffness_matrix().unwrap();
        let k = dense(&m, "K");
        let kmax = max_abs(&k);
        for (i, r) in rigid_modes(&m, dim).iter().enumerate() {
            let kr = mat_vec(&k, r);
            let rmax = r.iter().fold(0.0_f64, |a, v| a.max(v.abs()));
            let err = kr.iter().fold(0.0_f64, |a, v| a.max(v.abs()));
            assert!(
                err <= 1e-12 * kmax * rmax,
                "{dim}-D rigid mode {i}: |K r| = {err:e} (|K| = {kmax:e})"
            );
        }
        // exactly 3 (2-D) / 6 (3-D) zero eigenvalues, the rest positive
        let ev = symmetric_eigenvalues(&k);
        let nzero = if dim == 2 { 3 } else { 6 };
        let emax = ev.last().copied().unwrap();
        assert!(
            ev[..nzero].iter().all(|v| v.abs() <= 1e-10 * emax),
            "{ev:?}"
        );
        assert!(ev[nzero] > 1e-8 * emax, "{:?}", &ev[..nzero + 1]);
    }
}
