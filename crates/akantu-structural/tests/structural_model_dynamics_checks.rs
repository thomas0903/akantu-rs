// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs (port of Akantu, EPFL/LSMS): independent dynamic checks
// of the structural model: natural frequencies vs
// Euler–Bernoulli / Saint-Venant torsion, energy balance of the implicit
// trapezoidal rule, explicit central-difference runs (lumped and
// consistent mass), lumped-mass invariants (UD-005 / UD-059). No upstream
// counterpart.

//! * Natural frequencies of a simply supported slender beam (axial DOFs
//!   blocked): consistent mass converges to `ω_n = (nπ/L)² √(EI/ρA)` at
//!   `O(h⁴)`, the HRZ lumped mass (UD-059 proposal) at `O(h²)`; 3-D: both
//!   bending planes (`Iy ≠ Iz`) and the torsion of a clamped–free shaft,
//!   `ω₁ = π/(2L) √(GJ/(ρ (Iy + Iz)))` with the corrected torsional inertia
//!   (UD-507).
//! * Energy balance of `_implicit_dynamic` (trapezoidal rule, exactly
//!   energy-conserving for linear systems): `E_kin + E_pot = F·u` for a
//!   constant force applied from rest.
//! * `_explicit_lumped_mass`: free vibration of the first mode, quarter
//!   period and bounded energy.
//! * `_explicit_consistent_mass`: the model reproduces the textbook
//!   central-difference recurrence computed densely from its own `K`, `M`.
//! * Lumped mass: total translational mass, HRZ values, rigid-translation
//!   kinetic energy, invariance of the nodal rotational-inertia trace under
//!   a rigid rotation of a 3-D frame.

mod structural_model_common;

use akantu_core::enums::AnalysisMethod;
use akantu_core::{ElementType, Error, Idx, Real};
use akantu_mesh::Mesh;
use akantu_parser::param::ParamValue;
use akantu_structural::{
    StructuralMaterial, StructuralMechanicsModel, StructuralModelUpstreamCompat,
};
use structural_model_common::*;

use std::f64::consts::PI;

/// Slender 2-D beam: `L = 10`, `A = 0.01`, `I = 1e-5` (`L/r ≈ 316`).
fn slender_material() -> StructuralMaterial {
    StructuralMaterial {
        e: 1.0,
        rho: 1.0,
        a: 0.01,
        i: 1e-5,
        iz: 1e-5,
        iy: 4e-6,
        gj: 3e-6,
        ..StructuralMaterial::default()
    }
}

const LEN: Real = 10.0;

fn model_of(
    mesh: Mesh,
    mat: StructuralMaterial,
    method: AnalysisMethod,
    compat: StructuralModelUpstreamCompat,
) -> StructuralMechanicsModel {
    let mut m = StructuralMechanicsModel::with_compat(shared(mesh), None, "dyn", compat).unwrap();
    m.add_material(mat, "").unwrap();
    m.init_full_with(method).unwrap();
    m
}

/// Simply supported 2-D beam with every axial DOF blocked.
fn ss_beam_2d(ne: usize, method: AnalysisMethod) -> StructuralMechanicsModel {
    let mesh = line_mesh(
        2,
        ElementType::BernoulliBeam2,
        ne,
        LEN,
        &[0., 0.],
        &[1., 0.],
        None,
    );
    let mut m = model_of(
        mesh,
        slender_material(),
        method,
        StructuralModelUpstreamCompat::default(),
    );
    {
        let mut b_g = m.blocked_dofs_mut().unwrap();
        let b = b_g.as_mut_slice();
        for n in 0..=ne {
            b[n * 3] = true;
        }
        b[1] = true;
        b[ne * 3 + 1] = true;
    }
    m
}

fn free_dofs(m: &StructuralMechanicsModel) -> Vec<usize> {
    m.blocked_dofs()
        .unwrap()
        .as_slice()
        .iter()
        .enumerate()
        .filter(|(_, b)| !**b)
        .map(|(i, _)| i)
        .collect()
}

fn bending_omega(n: usize, ei: Real, rho_a: Real) -> Real {
    (n as Real * PI / LEN).powi(2) * (ei / rho_a).sqrt()
}

/// Frequencies (ascending) of the free DOFs with the consistent mass.
fn consistent_frequencies(m: &mut StructuralMechanicsModel) -> Vec<Real> {
    m.assemble_matrix("K").unwrap();
    m.assemble_matrix("M").unwrap();
    let free = free_dofs(m);
    let k = restrict(&dense(m, "K"), &free);
    let mm = restrict(&dense(m, "M"), &free);
    generalized_eigenvalues(&k, &mm)
        .into_iter()
        .map(Real::sqrt)
        .collect()
}

/// Frequencies (ascending) of the free DOFs with the lumped mass.
fn lumped_frequencies(m: &mut StructuralMechanicsModel) -> Vec<Real> {
    m.assemble_matrix("K").unwrap();
    m.assemble_lumped_mass_matrix().unwrap();
    let free = free_dofs(m);
    let k = restrict(&dense(m, "K"), &free);
    let lm = m.lumped_mass().unwrap().as_slice().to_vec();
    let d: Vec<Real> = free.iter().map(|&i| lm[i]).collect();
    assert!(
        d.iter().all(|v| *v > 0.0),
        "lumped mass not positive: {d:?}"
    );
    let n = free.len();
    let mut a = vec![vec![0.0; n]; n];
    for i in 0..n {
        for j in 0..n {
            a[i][j] = k[i][j] / (d[i].sqrt() * d[j].sqrt());
        }
    }
    symmetric_eigenvalues(&a)
        .into_iter()
        .map(Real::sqrt)
        .collect()
}

#[test]
fn natural_frequencies_consistent_mass_beam_2d() {
    let mat = slender_material();
    let mut prev: Option<Real> = None;
    for ne in [4, 8, 16] {
        let mut m = ss_beam_2d(ne, AnalysisMethod::Static);
        let w = consistent_frequencies(&mut m);
        let h = LEN / ne as Real;
        for n in 1..=3 {
            let exact = bending_omega(n, mat.e * mat.i, mat.rho * mat.a);
            let err = (w[n - 1] - exact) / exact;
            // consistent Hermite mass: upper bound with leading error term
            // (k h)⁴/1440, k = nπ/L (bound with margin for higher terms)
            let kh = n as Real * PI / LEN * h;
            assert!(
                err > -1e-12,
                "ne {ne}, mode {n}: {} below {exact}",
                w[n - 1]
            );
            assert!(
                err < kh.powi(4) / 1000.,
                "ne {ne}, mode {n}: rel error {err:e}"
            );
        }
        let e1 = (w[0] - bending_omega(1, mat.e * mat.i, mat.rho * mat.a)).abs();
        eprintln!(
            "consistent mass, {ne} elements: rel errors ω1..3 = {:?}",
            (1..=3)
                .map(|n| {
                    let ex = bending_omega(n, mat.e * mat.i, mat.rho * mat.a);
                    (w[n - 1] - ex) / ex
                })
                .collect::<Vec<_>>()
        );
        if let Some(p) = prev {
            assert!(p / e1 > 12.0, "O(h^4) convergence: ratio {}", p / e1);
        }
        prev = Some(e1);
    }
}

#[test]
fn natural_frequencies_lumped_mass_beam_2d() {
    let mat = slender_material();
    let mut prev: Option<Real> = None;
    for ne in [8, 16, 32] {
        let mut m = ss_beam_2d(ne, AnalysisMethod::ExplicitLumpedMass);
        let w = lumped_frequencies(&mut m);
        let exact = bending_omega(1, mat.e * mat.i, mat.rho * mat.a);
        let err = (w[0] - exact).abs() / exact;
        eprintln!("lumped mass, {ne} elements: rel error ω1 = {err:e}");
        if ne == 32 {
            assert!(err < 1e-3, "lumped ω1 rel error {err:e}");
            for n in 2..=3 {
                let ex = bending_omega(n, mat.e * mat.i, mat.rho * mat.a);
                assert!(((w[n - 1] - ex) / ex).abs() < 1e-2, "mode {n}");
            }
        }
        if let Some(p) = prev {
            // O(h²): ratio ≈ 4 per refinement
            assert!(p / err > 3.5, "O(h^2) convergence: ratio {}", p / err);
        }
        prev = Some(err);
    }
}

/// 3-D beam along x, normal z, simply supported in both planes; axial and
/// torsion DOFs blocked: the spectrum is the two bending families
/// (`E Iz` in the x–y plane, `E Iy` in x–z).
#[test]
fn natural_frequencies_beam_3d_both_bending_planes() {
    let ne = 16;
    let mat = slender_material();
    let mesh = line_mesh(
        3,
        ElementType::BernoulliBeam3,
        ne,
        LEN,
        &[0., 0., 0.],
        &[1., 0., 0.],
        Some(&[0., 0., 1.]),
    );
    let mut m = model_of(
        mesh,
        mat,
        AnalysisMethod::Static,
        StructuralModelUpstreamCompat::default(),
    );
    {
        let mut b_g = m.blocked_dofs_mut().unwrap();
        let b = b_g.as_mut_slice();
        for n in 0..=ne {
            b[n * 6] = true;
            b[n * 6 + 3] = true;
        }
        for n in [0, ne] {
            b[n * 6 + 1] = true;
            b[n * 6 + 2] = true;
        }
    }
    let w = consistent_frequencies(&mut m);
    let wz = bending_omega(1, mat.e * mat.iz, mat.rho * mat.a);
    let wy = bending_omega(1, mat.e * mat.iy, mat.rho * mat.a);
    // Iy < Iz: the lowest is the x–z family
    assert!(((w[0] - wy) / wy).abs() < 1e-5, "{} vs {wy}", w[0]);
    let found = w.iter().any(|x| ((x - wz) / wz).abs() < 1e-5);
    assert!(found, "ω(Iz) = {wz} not in {:?}", &w[..6]);
}

/// Torsion of a clamped–free shaft: every DOF blocked except `θx`, `θx(0)
/// = 0`. Corrected default (UD-507): `ω₁ → π/(2L) √(GJ/(ρ (Iy + Iz)))`;
/// upstream mode: `√(GJ/(ρA))` instead.
#[test]
fn torsional_frequency_uses_the_polar_moment() {
    let ne = 40;
    let mat = slender_material();
    let ip = mat.iy + mat.iz;
    for (compat, inertia) in [
        (StructuralModelUpstreamCompat::default(), mat.rho * ip),
        (
            StructuralModelUpstreamCompat {
                beam3_torsional_mass_rho_a: true,
                ..StructuralModelUpstreamCompat::default()
            },
            mat.rho * mat.a,
        ),
    ] {
        let mesh = line_mesh(
            3,
            ElementType::BernoulliBeam3,
            ne,
            LEN,
            &[0., 0., 0.],
            &[1., 0., 0.],
            Some(&[0., 0., 1.]),
        );
        let mut m = model_of(mesh, mat, AnalysisMethod::Static, compat);
        {
            let mut b_g = m.blocked_dofs_mut().unwrap();
            let b = b_g.as_mut_slice();
            b.fill(true);
            for n in 1..=ne {
                b[n * 6 + 3] = false;
            }
        }
        let w = consistent_frequencies(&mut m);
        let exact = PI / (2. * LEN) * (mat.gj / inertia).sqrt();
        let err = (w[0] - exact) / exact;
        eprintln!(
            "torsion (switch {}): rel error ω1 = {err:e}",
            compat.beam3_torsional_mass_rho_a
        );
        // linear elements with consistent mass: upper bound, O(h²)
        assert!(err > 0.0 && err < 2e-4, "ω1 {} vs {exact} ({err:e})", w[0]);
    }
}

/// Energy balance of the trapezoidal rule (`_implicit_dynamic`): for a
/// constant force `F` from rest, `E_kin + E_pot = F·u` at every step.
#[test]
fn implicit_dynamics_energy_balance() {
    let ne = 40;
    let (len, f) = (2.0, 1e4);
    let mat = StructuralMaterial {
        e: 1e9,
        rho: 10.,
        i: 1.,
        a: 1.,
        ..StructuralMaterial::default()
    };
    let mesh = line_mesh(
        2,
        ElementType::BernoulliBeam2,
        ne,
        len,
        &[0., 0.],
        &[1., 0.],
        None,
    );
    let mut m = model_of(
        mesh,
        mat,
        AnalysisMethod::ImplicitDynamic,
        StructuralModelUpstreamCompat::default(),
    );
    {
        let mut b_g = m.blocked_dofs_mut().unwrap();
        let b = b_g.as_mut_slice();
        b[0] = true;
        b[1] = true;
        b[ne * 3 + 1] = true;
    }
    let mid = ne / 2;
    m.external_force_mut().unwrap().as_mut_slice()[mid * 3 + 1] = f;
    // consistent initial acceleration M a₀ = F (the trapezoidal rule
    // conserves the energy exactly only from a consistent state; the
    // upstream fixture starts from a₀ = 0, whose missing half impulse makes
    // the balance converge like 1/n instead)
    {
        m.assemble_matrix("M").unwrap();
        let free = free_dofs(&m);
        let mm = restrict(&dense(&m, "M"), &free);
        let l = cholesky(&mm);
        let b: Vec<Real> = free
            .iter()
            .map(|&i| if i == mid * 3 + 1 { f } else { 0. })
            .collect();
        let n = b.len();
        let mut y = vec![0.0; n];
        for i in 0..n {
            let mut s = b[i];
            for j in 0..i {
                s -= l[i][j] * y[j];
            }
            y[i] = s / l[i][i];
        }
        let mut x = vec![0.0; n];
        for i in (0..n).rev() {
            let mut s = y[i];
            for j in i + 1..n {
                s -= l[j][i] * x[j];
            }
            x[i] = s / l[i][i];
        }
        let mut acc_g = m.acceleration_mut().unwrap();
        let acc = acc_g.as_mut_slice();
        for (k, &i) in free.iter().enumerate() {
            acc[i] = x[k];
        }
    }
    m.set_time_step(1e-6, None).unwrap();
    {
        let nls = m.non_linear_solver_mut(None).unwrap();
        nls.set_param("threshold", ParamValue::Real(1e-8)).unwrap();
        nls.set_param("convergence_type", ParamValue::Enum("solution".into()))
            .unwrap();
    }
    let mut max_rel: Real = 0.;
    for _ in 0..300 {
        m.solve_step(None).unwrap();
        let ekin = m.energy("kinetic").unwrap();
        let epot = m.energy("potential").unwrap();
        let work = f * m.displacement().unwrap().as_slice()[mid * 3 + 1];
        assert!(ekin >= 0. && epot >= 0.);
        max_rel = max_rel.max((ekin + epot - work).abs() / work.abs());
    }
    eprintln!("trapezoidal energy balance: max rel error {max_rel:e}");
    // exact conservation in exact arithmetic; rounding of the solves and of
    // the K·u, M·v products over 300 steps
    assert!(max_rel < 1e-9, "energy balance error {max_rel:e}");
}

/// `_explicit_lumped_mass` (lumped central difference): the first mode of
/// a simply supported beam, started from the Euler–Bernoulli mode shape at
/// rest, crosses zero at a quarter of the lumped system's period, and the
/// total energy stays bounded.
#[test]
fn explicit_lumped_free_vibration() {
    let ne = 20;
    let mat = slender_material();
    let mut m = ss_beam_2d(ne, AnalysisMethod::ExplicitLumpedMass);
    let w = lumped_frequencies(&mut m);
    let (w1, wmax) = (w[0], *w.last().unwrap());
    let dt = 0.5 * 2.0 / wmax;
    m.set_time_step(dt, None).unwrap();
    {
        let mut u_g = m.displacement_mut().unwrap();
        let u = u_g.as_mut_slice();
        for n in 0..=ne {
            let x = LEN * n as Real / ne as Real;
            u[n * 3 + 1] = (PI * x / LEN).sin() * 1e-3;
            u[n * 3 + 2] = (PI / LEN) * (PI * x / LEN).cos() * 1e-3;
        }
    }
    let mid = ne / 2;
    let e0 = m.energy("potential").unwrap();
    let mut t = 0.0;
    let mut prev = m.displacement().unwrap().as_slice()[mid * 3 + 1];
    let mut crossing = None;
    let mut max_dev: Real = 0.;
    for _ in 0..200_000 {
        m.solve_step(None).unwrap();
        t += dt;
        let d = m.displacement().unwrap().as_slice()[mid * 3 + 1];
        let e = m.energy("kinetic").unwrap() + m.energy("potential").unwrap();
        max_dev = max_dev.max((e - e0).abs() / e0);
        if d <= 0.0 {
            // linear interpolation of the crossing time
            crossing = Some(t - dt * d / (d - prev));
            break;
        }
        prev = d;
    }
    let tc = crossing.expect("no zero crossing");
    let quarter = PI / (2. * w1);
    eprintln!(
        "explicit lumped: crossing rel error {:e}, energy deviation {max_dev:e}, dt ω_max = {}",
        (tc - quarter) / quarter,
        dt * wmax
    );
    // measured 3.6e-4 (the Euler–Bernoulli shape is not exactly the lumped
    // first mode; deterministic): bound with a 5x margin
    assert!(
        ((tc - quarter) / quarter).abs() < 2e-3,
        "crossing {tc} vs {quarter}"
    );
    // the exact first lumped mode would conserve energy to O((ω1 Δt)²); the
    // Euler–Bernoulli shape excites higher modes slightly (measured 3.3e-7)
    assert!(max_dev < 1e-5, "energy deviation {max_dev:e}");
    let exact = bending_omega(1, mat.e * mat.i, mat.rho * mat.a);
    assert!(((w1 - exact) / exact).abs() < 5e-3);
}

/// `_explicit_consistent_mass`: the model's steps equal the dense
/// central-difference recurrence `u⁺ = u + Δt v + Δt²/2 a`,
/// `M a⁺ = f − K u⁺`, `v⁺ = v + Δt/2 (a + a⁺)` on the free DOFs.
#[test]
fn explicit_consistent_matches_dense_central_difference() {
    let ne = 4;
    let mut m = ss_beam_2d(ne, AnalysisMethod::ExplicitConsistentMass);
    m.assemble_matrix("K").unwrap();
    m.assemble_matrix("M").unwrap();
    let free = free_dofs(&m);
    let k = restrict(&dense(&m, "K"), &free);
    let mm = restrict(&dense(&m, "M"), &free);
    let l = cholesky(&mm);
    let solve_m = |b: &[Real]| -> Vec<Real> {
        let n = b.len();
        let mut y = vec![0.0; n];
        for i in 0..n {
            let mut s = b[i];
            for j in 0..i {
                s -= l[i][j] * y[j];
            }
            y[i] = s / l[i][i];
        }
        let mut x = vec![0.0; n];
        for i in (0..n).rev() {
            let mut s = y[i];
            for j in i + 1..n {
                s -= l[j][i] * x[j];
            }
            x[i] = s / l[i][i];
        }
        x
    };
    let w = lumped_frequencies(&mut ss_beam_2d(ne, AnalysisMethod::ExplicitLumpedMass));
    let dt = 0.1 / w.last().unwrap();
    m.set_time_step(dt, None).unwrap();
    let mid = ne / 2;
    let fmag = 1e-4;
    m.external_force_mut().unwrap().as_mut_slice()[mid * 3 + 1] = fmag;
    let f: Vec<Real> = free
        .iter()
        .map(|&i| if i == mid * 3 + 1 { fmag } else { 0. })
        .collect();
    let n = free.len();
    let (mut u, mut v, mut a) = (vec![0.0; n], vec![0.0; n], vec![0.0; n]);
    for step in 0..50 {
        m.solve_step(None).unwrap();
        for i in 0..n {
            u[i] += dt * v[i] + dt * dt / 2. * a[i];
        }
        let ku = mat_vec(&k, &u);
        let r: Vec<Real> = (0..n).map(|i| f[i] - ku[i]).collect();
        let a_new = solve_m(&r);
        for i in 0..n {
            v[i] += dt / 2. * (a[i] + a_new[i]);
        }
        a = a_new;
        let um = m.displacement().unwrap().as_slice().to_vec();
        let scale = u.iter().fold(0.0_f64, |s, x| s.max(x.abs()));
        for (j, &g) in free.iter().enumerate() {
            assert!(
                (um[g] - u[j]).abs() <= 1e-9 * scale,
                "step {step}, DOF {g}: {} vs {}",
                um[g],
                u[j]
            );
        }
    }
}

// ---------------------------------------------------------------------------
// lumped mass (UD-005 / UD-059)
// ---------------------------------------------------------------------------

#[test]
fn lumped_mass_values_and_totals_2d() {
    let nodes = vec![vec![0., 0.], vec![3., 4.], vec![3., 0.]];
    let conn: Vec<Vec<Idx>> = vec![vec![0, 1], vec![1, 2]];
    let mesh = mesh_of(2, ElementType::BernoulliBeam2, &nodes, &conn, None);
    let mat = StructuralMaterial {
        e: 1.,
        rho: 5.,
        a: 3.,
        i: 1.,
        ..StructuralMaterial::default()
    };
    let mut m = model_of(
        mesh,
        mat,
        AnalysisMethod::ExplicitLumpedMass,
        StructuralModelUpstreamCompat::default(),
    );
    m.assemble_lumped_mass_matrix().unwrap();
    let lm = m.lumped_mass().unwrap();
    let (m1, m2) = (5. * 3. * 5., 5. * 3. * 4.); // ρ A L
    let expect = [
        [m1 / 2., m1 / 2., m1 * 25. / 78.],
        [
            m1 / 2. + m2 / 2.,
            m1 / 2. + m2 / 2.,
            m1 * 25. / 78. + m2 * 16. / 78.,
        ],
        [m2 / 2., m2 / 2., m2 * 16. / 78.],
    ];
    for (n, row) in expect.iter().enumerate() {
        for (c, want) in row.iter().enumerate() {
            let got = lm.tuple(n)[c];
            assert!(
                (got - want).abs() <= 1e-13 * want,
                "node {n} dof {c}: {got} vs {want}"
            );
        }
    }
    // the same values are in the DOF manager's lumped M
    let d = m.dof_manager().lumped_matrix("M").unwrap();
    assert_eq!(d.values.as_slice(), lm.as_slice());
}

/// Rigid translation: `E_kin = ½ (Σ ρ A L) |v|²`; the trace of the nodal
/// rotational inertia is invariant under a rigid rotation of a 3-D frame.
#[test]
fn lumped_mass_rigid_translation_and_rotation_invariance_3d() {
    let nodes = vec![
        vec![0., 0., 0.],
        vec![1., 0.5, 0.2],
        vec![2.5, 0.2, -0.4],
        vec![1.2, -1.3, 0.9],
    ];
    let conn: Vec<Vec<Idx>> = vec![vec![0, 1], vec![1, 2], vec![1, 3]];
    let mat = StructuralMaterial {
        e: 1.,
        rho: 2.,
        a: 0.3,
        iz: 0.01,
        iy: 0.02,
        gj: 1.,
        ..StructuralMaterial::default()
    };
    let normal_of = |a: &[Real], b: &[Real]| -> Vec<Real> {
        let d = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
        let h = [0.3, -0.2, 1.0];
        let c = [
            d[1] * h[2] - d[2] * h[1],
            d[2] * h[0] - d[0] * h[2],
            d[0] * h[1] - d[1] * h[0],
        ];
        let n = (c[0] * c[0] + c[1] * c[1] + c[2] * c[2]).sqrt();
        vec![c[0] / n, c[1] / n, c[2] / n]
    };
    // rigid rotation Q (about (1,1,1)/√3 by 50°)
    let th: Real = 50f64.to_radians();
    let k = [1. / 3f64.sqrt(); 3];
    let (c, s) = (th.cos(), th.sin());
    let q = |v: &[Real]| -> Vec<Real> {
        let kv = k[0] * v[0] + k[1] * v[1] + k[2] * v[2];
        let kxv = [
            k[1] * v[2] - k[2] * v[1],
            k[2] * v[0] - k[0] * v[2],
            k[0] * v[1] - k[1] * v[0],
        ];
        (0..3)
            .map(|i| v[i] * c + kxv[i] * s + k[i] * kv * (1. - c))
            .collect()
    };
    let traces = |nodes: &[Vec<Real>], normals: &[Vec<Real>]| -> (Vec<Real>, Real) {
        let mesh = mesh_of(3, ElementType::BernoulliBeam3, nodes, &conn, Some(normals));
        let mut m = model_of(
            mesh,
            mat,
            AnalysisMethod::ExplicitLumpedMass,
            StructuralModelUpstreamCompat::default(),
        );
        let v = [0.7, -0.4, 1.1];
        {
            let mut vel_g = m.velocity_mut().unwrap();
            let vel = vel_g.as_mut_slice();
            for n in 0..nodes.len() {
                vel[n * 6..n * 6 + 3].copy_from_slice(&v);
            }
        }
        // the lumped branch needs the lumped M (before it, the kinetic
        // energy uses the consistent mass, as upstream: UD-510)
        m.assemble_lumped_mass_matrix().unwrap();
        let ekin = m.kinetic_energy().unwrap();
        let lm = m.lumped_mass().unwrap();
        let tr = (0..nodes.len())
            .map(|n| lm.tuple(n)[3..].iter().sum())
            .collect();
        let total: Real = (0..nodes.len()).map(|n| lm.tuple(n)[0]).sum();
        let v2 = v.iter().map(|x| x * x).sum::<Real>();
        assert!((ekin - 0.5 * total * v2).abs() <= 1e-13 * ekin, "{ekin}");
        (tr, total)
    };
    let normals: Vec<Vec<Real>> = conn
        .iter()
        .map(|cn| normal_of(&nodes[cn[0] as usize], &nodes[cn[1] as usize]))
        .collect();
    let (t0, total) = traces(&nodes, &normals);
    let length = |a: &[Real], b: &[Real]| {
        ((b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2) + (b[2] - a[2]).powi(2)).sqrt()
    };
    let expected_total: Real = conn
        .iter()
        .map(|cn| mat.rho * mat.a * length(&nodes[cn[0] as usize], &nodes[cn[1] as usize]))
        .sum();
    assert!((total - expected_total).abs() <= 1e-13 * total);
    let rn: Vec<Vec<Real>> = nodes.iter().map(|x| q(x)).collect();
    let rnorm: Vec<Vec<Real>> = normals.iter().map(|x| q(x)).collect();
    let (t1, _) = traces(&rn, &rnorm);
    for (a, b) in t0.iter().zip(&t1) {
        assert!((a - b).abs() <= 1e-12 * a, "trace {a} vs {b}");
    }
}

#[test]
fn lumped_mass_of_shells_is_not_implemented() {
    let nodes = vec![vec![0., 0., 0.], vec![1., 0., 0.], vec![0., 1., 0.]];
    let conn: Vec<Vec<Idx>> = vec![vec![0, 1, 2]];
    let mesh = mesh_of(
        3,
        ElementType::DiscreteKirchhoffTriangle18,
        &nodes,
        &conn,
        None,
    );
    let mut m = model_of(
        mesh,
        StructuralMaterial {
            e: 1.,
            t: 0.1,
            nu: 0.3,
            rho: 1.,
            ..StructuralMaterial::default()
        },
        AnalysisMethod::Static,
        StructuralModelUpstreamCompat::default(),
    );
    assert!(matches!(
        m.assemble_lumped_mass_matrix(),
        Err(Error::NotImplemented(_))
    ));
    // consistent mass: no DKT shape functions upstream (UD-053)
    assert!(m.assemble_mass_matrix().is_err());
}
