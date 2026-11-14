// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): test/test_model/test_solid_mechanics_model/
// test_materials/test_finite_def_materials.cc (typed gtests ComputeStress,
// EnergyDensity, ComputeTangentModuli, ComputeCelerity for
// MaterialNeohookean<1,2,3> — all DISABLED upstream, their bodies
// AKANTU_TO_IMPLEMENT, UD-026) and the Neo-Hookean section of
// doc/dev-doc/manual/constitutive-laws.rst.

//! The intent of the four upstream tests, written against the documented
//! law (UD-026/UD-057, `akantu_solid::materials::neohookean`):
//!
//! * ComputeStress: closed-form `S` for stretches (all dimensions, plane
//!   strain), `S33 = 0` in plane stress, zero stress under a rigid rotation,
//!   frame indifference;
//! * EnergyDensity: `P = F·S = ∂Ψ/∂F` by central differences (plane stress:
//!   on the reduced energy with `C33` re-solved), `Ψ(I) = 0`;
//! * ComputeTangentModuli: `dS = ℂ : dE` by central differences (plane
//!   stress: the condensed tangent), symmetry, small-strain limit;
//! * ComputeCelerity: `√((λ + 2μ)/ρ)`.

mod common;
mod inelastic_common;

use akantu_core::Real;
use akantu_core::tensor::Matrix;
use akantu_solid::material::{LawOptions, NeohookeanMode};
use akantu_solid::materials::neohookean::{
    neohookean_energy, neohookean_stress, neohookean_tangent, neohookean_tangent_plane_stress,
    solve_c33,
};
use inelastic_common::{Point, Rng, matmul, max_abs, max_abs_diff, random_rotation, transpose};

const E: Real = 1e6;
const NU: Real = 0.45;

fn lame(dim: usize) -> (Real, Real) {
    let nu = if dim == 1 { 0. } else { NU };
    (nu * E / ((1. + nu) * (1. - 2. * nu)), E / (2. * (1. + nu)))
}

fn mat<const D: usize>(rows: &[Real]) -> Matrix<D, D> {
    std::array::from_fn(|c| std::array::from_fn(|r| rows[r * D + c]))
}

fn rows<const D: usize>(m: &Matrix<D, D>) -> Vec<Real> {
    (0..D * D).map(|k| m[k % D][k / D]).collect()
}

fn dat(dim: usize, plane_stress: bool) -> String {
    format!(
        "material neohookean [\n  name = rubber\n  rho = 1000\n  E = {E}\n  nu = {NU}\n{}]\n",
        if dim == 2 && plane_stress {
            "  Plane_Stress = true\n"
        } else {
            ""
        }
    )
}

/// `S` of the reduced plane-stress law at `grad_u` (C33 solved).
fn s_plane_stress(lambda: Real, mu: Real, g: &Matrix<2, 2>) -> (Matrix<2, 2>, Real) {
    let c33 = solve_c33(lambda, mu, g, 1.).unwrap();
    (neohookean_stress(lambda, mu, g, c33).unwrap(), c33)
}

#[test]
fn neohookean_compute_stress_stretches() {
    let l1: Real = 1.3;
    // 3-D: F = diag(l1, 1, 1)
    let (lambda, mu) = lame(3);
    let s = neohookean_stress::<3>(
        lambda,
        mu,
        &mat(&[l1 - 1., 0., 0., 0., 0., 0., 0., 0., 0.]),
        1.,
    )
    .unwrap();
    let s11 = mu + (lambda * l1.ln() - mu) / (l1 * l1);
    let s22 = lambda * l1.ln();
    let want = [s11, 0., 0., 0., s22, 0., 0., 0., s22];
    assert!(max_abs_diff(&rows(&s), &want) <= 1e-14 * max_abs(&want));
    // 2-D plane strain: same in-plane components
    let (lambda, mu) = lame(2);
    let s = neohookean_stress::<2>(lambda, mu, &mat(&[l1 - 1., 0., 0., 0.]), 1.).unwrap();
    assert!(max_abs_diff(&rows(&s), &[s11, 0., 0., s22]) <= 1e-14 * s11.abs().max(s22.abs()));
    // 1-D (ν = 0 ⇒ λ = 0): S = μ(1 − 1/l1²)
    let (lambda, mu) = lame(1);
    let s = neohookean_stress::<1>(lambda, mu, &mat(&[l1 - 1.]), 1.).unwrap();
    assert!((s[0][0] - mu * (1. - 1. / (l1 * l1))).abs() <= 1e-14 * mu);
}

#[test]
fn neohookean_plane_stress_s33_vanishes() {
    let (lambda, mu) = lame(2);
    let mut rng = Rng(11);
    for _ in 0..200 {
        let g = mat::<2>(&[
            0.3 * rng.sym(),
            0.3 * rng.sym(),
            0.3 * rng.sym(),
            0.3 * rng.sym(),
        ]);
        let x = solve_c33(lambda, mu, &g, 0.5 + rng.next()).unwrap();
        let f = [1. + g[0][0], g[1][0], g[0][1], 1. + g[1][1]]; // row-major F
        let det_f = f[0] * f[3] - f[1] * f[2];
        // S33 = μ + (λ ln J − μ)/C33 with J = det F₂ √C33
        let s33 = mu + (lambda * (det_f * x.sqrt()).ln() - mu) / x;
        assert!(s33.abs() <= 1e-9 * mu, "S33 = {s33}");
        assert!(x > 0.);
    }
}

#[test]
fn neohookean_zero_stress_under_rotation_and_frame_indifference() {
    let mut rng = Rng(3);
    for _ in 0..20 {
        // 3-D
        let (lambda, mu) = lame(3);
        let q = random_rotation(3, &mut rng);
        let g: Vec<Real> = (0..9)
            .map(|k| q[k] - if k % 4 == 0 { 1. } else { 0. })
            .collect();
        let s = neohookean_stress::<3>(lambda, mu, &mat(&g), 1.).unwrap();
        assert!(max_abs(&rows(&s)) <= 1e-9 * mu, "rigid rotation: {s:?}");
        // S(QF) = S(F)
        let f0: Vec<Real> = (0..9).map(|_| 0.2 * rng.sym()).collect();
        let f: Vec<Real> = (0..9)
            .map(|k| f0[k] + if k % 4 == 0 { 1. } else { 0. })
            .collect();
        let qf = matmul(&q, &f, 3);
        let g_f: Vec<Real> = (0..9)
            .map(|k| f[k] - if k % 4 == 0 { 1. } else { 0. })
            .collect();
        let g_qf: Vec<Real> = (0..9)
            .map(|k| qf[k] - if k % 4 == 0 { 1. } else { 0. })
            .collect();
        let s1 = rows(&neohookean_stress::<3>(lambda, mu, &mat(&g_f), 1.).unwrap());
        let s2 = rows(&neohookean_stress::<3>(lambda, mu, &mat(&g_qf), 1.).unwrap());
        assert!(max_abs_diff(&s1, &s2) <= 1e-9 * max_abs(&s1).max(mu));
        // Cauchy stress σ(QF) = Q σ(F) Qᵀ
        let cauchy = |f: &[Real], s: &[Real]| {
            let j = det3(f);
            let fs = matmul(f, s, 3);
            matmul(&fs, &transpose(f, 3), 3)
                .iter()
                .map(|x| x / j)
                .collect::<Vec<_>>()
        };
        let c1 = cauchy(&f, &s1);
        let c2 = cauchy(&qf, &s2);
        let rot = matmul(&matmul(&q, &c1, 3), &transpose(&q, 3), 3);
        assert!(max_abs_diff(&rot, &c2) <= 1e-9 * max_abs(&c1).max(mu));
        // 2-D (plane strain and plane stress)
        let (lambda, mu) = lame(2);
        let q2 = random_rotation(2, &mut rng);
        let g2: Vec<Real> = (0..4)
            .map(|k| q2[k] - if k % 3 == 0 { 1. } else { 0. })
            .collect();
        let s = neohookean_stress::<2>(lambda, mu, &mat(&g2), 1.).unwrap();
        assert!(max_abs(&rows(&s)) <= 1e-9 * mu);
        let (s, c33) = s_plane_stress(lambda, mu, &mat(&g2));
        assert!(max_abs(&rows(&s)) <= 1e-9 * mu && (c33 - 1.).abs() <= 1e-12);
    }
}

fn det3(f: &[Real]) -> Real {
    f[0] * (f[4] * f[8] - f[5] * f[7]) - f[1] * (f[3] * f[8] - f[5] * f[6])
        + f[2] * (f[3] * f[7] - f[4] * f[6])
}

/// `P = F S` against `∂Ψ/∂F` (central differences, step `h`).
fn check_energy<const D: usize>(lambda: Real, mu: Real, g: &[Real], plane_stress: bool) {
    let psi = |g: &[Real]| -> Real {
        let gm = mat::<D>(g);
        let c33 = if plane_stress {
            solve_c33(lambda, mu, &mat::<2>(g), 1.).unwrap()
        } else {
            1.
        };
        neohookean_energy::<D>(lambda, mu, &gm, c33).unwrap()
    };
    let c33 = if plane_stress {
        solve_c33(lambda, mu, &mat::<2>(g), 1.).unwrap()
    } else {
        1.
    };
    let s = rows(&neohookean_stress::<D>(lambda, mu, &mat::<D>(g), c33).unwrap());
    let f: Vec<Real> = (0..D * D)
        .map(|k| g[k] + if k % (D + 1) == 0 { 1. } else { 0. })
        .collect();
    let p = matmul(&f, &s, D);
    let h = 1e-6;
    let mut fd = vec![0.; D * D];
    for k in 0..D * D {
        let (mut gp, mut gm) = (g.to_vec(), g.to_vec());
        gp[k] += h;
        gm[k] -= h;
        fd[k] = (psi(&gp) - psi(&gm)) / (2. * h);
    }
    let scale = max_abs(&p).max(mu * 1e-3);
    assert!(
        max_abs_diff(&p, &fd) <= 2e-7 * scale,
        "P = {p:?}, dPsi/dF = {fd:?}"
    );
}

#[test]
fn neohookean_energy_density_is_the_potential_of_the_stress() {
    let mut rng = Rng(5);
    for _ in 0..20 {
        let (lambda, mu) = lame(3);
        let g: Vec<Real> = (0..9).map(|_| 0.15 * rng.sym()).collect();
        check_energy::<3>(lambda, mu, &g, false);
        let (lambda, mu) = lame(2);
        let g: Vec<Real> = (0..4).map(|_| 0.15 * rng.sym()).collect();
        check_energy::<2>(lambda, mu, &g, false);
        check_energy::<2>(lambda, mu, &g, true);
        let (lambda, mu) = lame(1);
        check_energy::<1>(lambda, mu, &[0.3 * rng.sym()], false);
    }
    // stress-free and energy-free reference state
    for dim in [1, 2, 3] {
        let (lambda, mu) = lame(dim);
        let e = match dim {
            1 => neohookean_energy::<1>(lambda, mu, &mat(&[0.]), 1.),
            2 => neohookean_energy::<2>(lambda, mu, &mat(&[0.; 4]), 1.),
            _ => neohookean_energy::<3>(lambda, mu, &mat(&[0.; 9]), 1.),
        }
        .unwrap();
        assert_eq!(e, 0.);
    }
}

/// `dS_voigt = ℂ · dE_voigt` (engineering shears) by central differences.
fn check_tangent<const D: usize>(
    lambda: Real,
    mu: Real,
    g: &[Real],
    dg: &[Real],
    plane_stress: bool,
) {
    let v = D * (D + 1) / 2;
    let voigt = |m: &[Real], factor: Real| -> Vec<Real> {
        match D {
            1 => vec![m[0]],
            2 => vec![m[0], m[3], factor * m[1]],
            _ => vec![
                m[0],
                m[4],
                m[8],
                factor * m[5],
                factor * m[2],
                factor * m[1],
            ],
        }
    };
    let s_of = |g: &[Real]| -> Vec<Real> {
        if plane_stress {
            let (s, _) = s_plane_stress(lambda, mu, &mat::<2>(g));
            rows(&s)
        } else {
            rows(&neohookean_stress::<D>(lambda, mu, &mat::<D>(g), 1.).unwrap())
        }
    };
    let e_of = |g: &[Real]| -> Vec<Real> {
        let f: Vec<Real> = (0..D * D)
            .map(|k| g[k] + if k % (D + 1) == 0 { 1. } else { 0. })
            .collect();
        let c = matmul(&transpose(&f, D), &f, D);
        (0..D * D)
            .map(|k| 0.5 * (c[k] - if k % (D + 1) == 0 { 1. } else { 0. }))
            .collect()
    };
    let h = 1e-6;
    let gp: Vec<Real> = g.iter().zip(dg).map(|(a, b)| a + h * b).collect();
    let gm: Vec<Real> = g.iter().zip(dg).map(|(a, b)| a - h * b).collect();
    let ds: Vec<Real> = voigt(&s_of(&gp), 1.)
        .iter()
        .zip(voigt(&s_of(&gm), 1.))
        .map(|(a, b)| (a - b) / (2. * h))
        .collect();
    let de: Vec<Real> = voigt(&e_of(&gp), 2.)
        .iter()
        .zip(voigt(&e_of(&gm), 2.))
        .map(|(a, b)| (a - b) / (2. * h))
        .collect();
    let mut t = vec![0.; v * v];
    let c33 = if plane_stress {
        solve_c33(lambda, mu, &mat::<2>(g), 1.).unwrap()
    } else {
        1.
    };
    if plane_stress {
        neohookean_tangent_plane_stress(lambda, mu, &mat::<2>(g), c33, &mut t).unwrap();
    } else {
        neohookean_tangent::<D>(lambda, mu, &mat::<D>(g), c33, &mut t).unwrap();
    }
    // t is column-major
    let pred: Vec<Real> = (0..v)
        .map(|m| (0..v).map(|n| t[m + n * v] * de[n]).sum())
        .collect();
    let scale = max_abs(&ds).max(mu * 1e-3);
    assert!(
        max_abs_diff(&pred, &ds) <= 1e-6 * scale,
        "C:dE = {pred:?}, dS = {ds:?}"
    );
    // major symmetry
    for m in 0..v {
        for n in 0..v {
            assert!((t[m + n * v] - t[n + m * v]).abs() <= 1e-12 * max_abs(&t));
        }
    }
}

#[test]
fn neohookean_tangent_is_the_derivative_of_the_stress() {
    let mut rng = Rng(9);
    for _ in 0..20 {
        let (lambda, mu) = lame(3);
        let g: Vec<Real> = (0..9).map(|_| 0.2 * rng.sym()).collect();
        let dg: Vec<Real> = (0..9).map(|_| rng.sym()).collect();
        check_tangent::<3>(lambda, mu, &g, &dg, false);
        let (lambda, mu) = lame(2);
        let g: Vec<Real> = (0..4).map(|_| 0.2 * rng.sym()).collect();
        let dg: Vec<Real> = (0..4).map(|_| rng.sym()).collect();
        check_tangent::<2>(lambda, mu, &g, &dg, false);
        check_tangent::<2>(lambda, mu, &g, &dg, true);
        let (lambda, mu) = lame(1);
        check_tangent::<1>(lambda, mu, &[0.3 * rng.sym()], &[1.], false);
    }
}

/// Small-strain limit: at `F = I` the tangent is the isotropic elastic one
/// (plane stress: with `λ* = 2λμ/(λ + 2μ) = νE/(1 − ν²)`), and
/// `S(δG) = C:ε(δG) + O(δ²)`.
#[test]
fn neohookean_small_strain_limit_is_linear_elasticity() {
    let (lambda, mu) = lame(3);
    let mut t = vec![0.; 36];
    neohookean_tangent::<3>(lambda, mu, &mat(&[0.; 9]), 1., &mut t).unwrap();
    for m in 0..6 {
        for n in 0..6 {
            let want = match (m < 3, n < 3) {
                (true, true) if m == n => lambda + 2. * mu,
                (true, true) => lambda,
                (false, false) if m == n => mu,
                _ => 0.,
            };
            assert!((t[m + 6 * n] - want).abs() <= 1e-9 * mu, "({m}, {n})");
        }
    }
    let (lambda, mu) = lame(2);
    let mut t = vec![0.; 9];
    neohookean_tangent_plane_stress(lambda, mu, &mat(&[0.; 4]), 1., &mut t).unwrap();
    let lps = NU * E / (1. - NU * NU);
    let want = [lps + 2. * mu, lps, 0., lps, lps + 2. * mu, 0., 0., 0., mu];
    assert!(max_abs_diff(&t, &want) <= 1e-9 * E);
    // S(δG) ≈ λ tr ε I + 2μ ε, error O(δ²)
    let (lambda, mu) = lame(3);
    let g0 = [0.3, -0.2, 0.5, 0.1, -0.4, 0.2, 0.7, 0.3, 0.1];
    for (k, delta) in [1e-3, 1e-4, 1e-5].iter().enumerate() {
        let g: Vec<Real> = g0.iter().map(|x| x * delta).collect();
        let s = rows(&neohookean_stress::<3>(lambda, mu, &mat(&g), 1.).unwrap());
        let tr = g[0] + g[4] + g[8];
        let lin: Vec<Real> = (0..9)
            .map(|q| {
                let (i, j) = (q / 3, q % 3);
                let eps = 0.5 * (g[i * 3 + j] + g[j * 3 + i]);
                2. * mu * eps + if i == j { lambda * tr } else { 0. }
            })
            .collect();
        let err = max_abs_diff(&s, &lin) / max_abs(&lin);
        assert!(
            err <= 20. * delta,
            "δ = {delta}: relative error {err} (k = {k})"
        );
    }
}

#[test]
fn neohookean_compute_celerity_and_material_paths() {
    for dim in [1, 2, 3] {
        for plane_stress in [false, true] {
            if plane_stress && dim != 2 {
                continue;
            }
            let mut p = Point::new(dim, &dat(dim, plane_stress), LawOptions::default());
            let (lambda, mu) = lame(dim);
            let c = p.celerity();
            let want = ((lambda + 2. * mu) / 1000.).sqrt();
            assert!((c - want).abs() <= 1e-14 * want);
            // a step through the material equals the kernel
            let g: Vec<Real> = (0..dim * dim).map(|k| 0.05 * (k as Real + 1.)).collect();
            p.step(&g).unwrap();
            let s = p.stress();
            let c33 = if plane_stress {
                p.scalar("third_axis_deformation")
            } else {
                1.
            };
            let want = match dim {
                1 => rows(&neohookean_stress::<1>(lambda, mu, &mat(&g), c33).unwrap()),
                2 => rows(&neohookean_stress::<2>(lambda, mu, &mat(&g), c33).unwrap()),
                _ => rows(&neohookean_stress::<3>(lambda, mu, &mat(&g), c33).unwrap()),
            };
            assert_eq!(s, want);
            // tangent and energy exist and are finite
            assert!(p.tangent().iter().all(|x| x.is_finite()));
            assert!(p.potential_density().is_finite());
        }
    }
}

/// UD-026: upstream mode reproduces the aborts.
#[test]
fn neohookean_upstream_mode_is_not_implemented() {
    let o = LawOptions {
        neohookean: NeohookeanMode::UpstreamNotImplemented,
        ..LawOptions::default()
    };
    let mut p = Point::new(3, &dat(3, false), o);
    let e = p.step(&[0.01; 9]).unwrap_err();
    assert!(matches!(e, akantu_core::Error::NotImplemented(_)), "{e}");
    assert!(
        e.to_string()
            .contains("computeStress : not implemented yet !")
    );
}
