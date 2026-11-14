// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Ports (material-point
// part) of test/test_model/test_solid_mechanics_model/test_materials/
// test_material_viscoelastic/test_material_standard_linear_solid_deviatoric_
// relaxation{,_tension}.cc and test_material_viscoelastic_maxwell/
// test_material_viscoelastic_maxwell_relaxation.cc (inputs
// material_standard_linear_solid_deviatoric_relaxation.dat,
// material_viscoelastic_maxwell.dat), plus independent checks of the
// viscoelastic laws: closed-form relaxation and creep, dissipation,
// frame invariance (UD-029), committed-state semantics (UD-063), elastic
// limits.

mod common;
#[path = "common/timed.rs"]
mod timed;

use akantu_core::Real;
use akantu_model::constitutive_law::LawPhase;
use akantu_parser::ParamValue;
use akantu_solid::Material;
use akantu_solid::material::LawOptions;
use akantu_validation::reduce::NanMinMax;
use common::{NG, SimplexModel, materials_from_dat_with, simplex};
use timed::{Point, TimedModel, col_major, one_element};

type Handler = akantu_model::ConstitutiveLawsHandler<dyn Material>;

fn law(model: &SimplexModel, dat: &str, upstream: bool) -> Handler {
    materials_from_dat_with(model, dat.as_bytes(), upstream.then(LawOptions::upstream)).unwrap()
}

/// Relative error of `actual` against an analytical `solution`:
/// the actual stress must be finite at every step; the only
/// analytical zero of these histories (`t = 0`, zero strain) requires an
/// exactly zero stress instead of a relative error.
fn relative_error(actual: Real, solution: Real) -> Real {
    assert!(actual.is_finite(), "non-finite stress {actual}");
    assert!(solution.is_finite());
    if solution == 0. {
        assert_eq!(actual, 0., "stress at the analytical zero");
        return 0.;
    }
    ((actual - solution) / solution).abs()
}

fn get(law: &dyn Material, name: &str) -> Real {
    match law.get_param(name).unwrap() {
        ParamValue::Real(x) => x,
        other => panic!("{name}: {other:?}"),
    }
}

/// `material_standard_linear_solid_deviatoric_relaxation.dat` (2-D: with
/// its `Plane_Stress = 0`).
const SLS_DAT: &str = "material sls_deviatoric [\n  name = anything\n  rho = 7800\n  E = 15\n  nu = 0.25\n  Eta = 10\n  Ev = 5\n  Plane_Stress = 0\n]\n";
/// The same material without the 2-D-only `Plane_Stress` key.
const SLS_DAT_ANY: &str = "material sls_deviatoric [\n  name = anything\n  rho = 7800\n  E = 15\n  nu = 0.25\n  Eta = 10\n  Ev = 5\n]\n";

/// Port of `test_material_standard_linear_solid_deviatoric_relaxation`:
/// pure shear `u = (εy, εx)` ramped over `T = 10` then held; the shear
/// stress follows `2G0 ε/T (γ∞ t + γ τ (1 − e^{−t/τ}))` (ramp) and
/// `2G0 ε (γ∞ + γ τ/T (e^{(T−t)/τ} − e^{−t/τ}))` (hold), relative tolerance
/// 1e-7 as upstream. Upstream's time step is a tenth of the stable step of
/// its mesh; the port uses `Δt = 2e-3` (the midpoint scheme's relative error
/// on the viscous part is ≈ (Δt/τ)²/24 = 4e-8 with τ = 2). Both modes:
/// upstream (history advanced at every evaluation, no step bookkeeping
/// needed) and the committed-state default (one converged step per time
/// step).
#[test]
fn test_material_standard_linear_solid_deviatoric_relaxation() {
    for upstream in [true, false] {
        let model = one_element(2);
        let mut h = law(&model, SLS_DAT, upstream);
        let l = h.law_mut(0).unwrap();
        let (eta, ev, einf, nu) = (get(l, "Eta"), get(l, "Ev"), get(l, "Einf"), get(l, "nu"));
        assert_eq!(einf, 10.);
        let ginf = einf / (2. * (1. + nu));
        let g = ev / (2. * (1. + nu));
        let g0 = g + ginf;
        let gamma = g / g0;
        let tau = eta / ev;
        let gammainf = ginf / g0;
        let (t_ramp, eps, sim_time, dt) = (10.0, 0.001, 25.0, 2e-3);
        let timed = TimedModel { inner: &model, dt };
        let mut p = Point {
            law: l,
            ty: simplex(2),
        };
        let max_steps = (sim_time / dt) as usize;
        let tolerance = 1e-7;
        let mut worst = 0.0_f64;
        for s in 0..=max_steps {
            let time = s as Real * dt;
            let epsilon = if time < t_ramp {
                eps * time / t_ramp
            } else {
                eps
            };
            p.step(&timed, &col_major(&[&[0., epsilon], &[epsilon, 0.]]), 1);
            let solution = if time < t_ramp {
                2. * g0 * eps / t_ramp
                    * (gammainf * time + gamma * tau * (1. - (-time / tau).exp()))
            } else {
                2. * g0
                    * eps
                    * (gammainf
                        + gamma * tau / t_ramp
                            * (((t_ramp - time) / tau).exp() - (-time / tau).exp()))
            };
            let sigma = p.stress();
            assert_eq!(solution == 0., s == 0, "analytical zero only at t = 0");
            for v in [sigma[2], sigma[1]] {
                let rel = relative_error(v, solution);
                worst = worst.nan_max(rel);
                assert!(
                    rel <= tolerance,
                    "upstream={upstream} step {s}: relative error {rel}"
                );
            }
        }
        eprintln!("SLS shear relaxation (upstream={upstream}): max relative error {worst:e}");
    }
}

/// Port of `test_material_standard_linear_solid_deviatoric_relaxation_tension`:
/// `setToSteadyState` at zero displacement, then `u = εx` in both directions
/// (2-D plane strain); `σ_yy = 2μ(ε − ε_kk/3)/T (γ∞ t + γ τ (1 − e^{−t/τ})) +
/// γ∞ κ ε_kk t/T` during the ramp, relative tolerance 1e-5 as upstream.
#[test]
fn test_material_standard_linear_solid_deviatoric_relaxation_tension() {
    for upstream in [true, false] {
        let model = one_element(2);
        let mut h = law(&model, SLS_DAT, upstream);
        let l = h.law_mut(0).unwrap();
        let (eta, ev, einf, e0) = (get(l, "Eta"), get(l, "Ev"), get(l, "Einf"), get(l, "E"));
        let (kpa, mu) = (get(l, "kapa"), get(l, "mu"));
        let (gamma, gammainf, tau) = (ev / e0, einf / e0, eta / ev);
        let (t_ramp, eps, sim_time, dt) = (10.0, 0.001, 25.0, 2e-3);
        let timed = TimedModel { inner: &model, dt };
        let ty = simplex(2);
        let mut p = Point { law: l, ty };
        // assembleInternalForces at zero displacement, then setToSteadyState
        p.set_grad_u(&[0.; 4]);
        p.law
            .compute_stress(&timed.ctx(LawPhase::Stress), ty, NG)
            .unwrap();
        p.law
            .set_to_steady_state(&timed.ctx(LawPhase::Stress), ty, NG)
            .unwrap();
        let max_steps = (sim_time / dt) as usize;
        let mut worst = 0.0_f64;
        for s in 0..=max_steps {
            let time = s as Real * dt;
            let epsilon = if time < t_ramp {
                eps * time / t_ramp
            } else {
                eps
            };
            p.step(&timed, &col_major(&[&[epsilon, 0.], &[0., epsilon]]), 1);
            let epskk = 2. * eps;
            let solution = if time < t_ramp {
                2. * mu * (eps - epskk / 3.) / t_ramp
                    * (gammainf * time + gamma * tau * (1. - (-time / tau).exp()))
                    + gammainf * kpa * epskk * time / t_ramp
            } else {
                2. * mu
                    * (eps - epskk / 3.)
                    * (gammainf
                        + gamma * tau / t_ramp
                            * (((t_ramp - time) / tau).exp() - (-time / tau).exp()))
                    + gammainf * kpa * epskk
            };
            assert_eq!(solution == 0., s == 0, "analytical zero only at t = 0");
            let rel = relative_error(p.stress()[3], solution);
            worst = worst.nan_max(rel);
            assert!(
                rel <= 1e-5,
                "upstream={upstream} step {s}: relative error {rel}"
            );
        }
        eprintln!("SLS tension relaxation (upstream={upstream}): max relative error {worst:e}");
    }
}

/// `material_viscoelastic_maxwell.dat` (2-D).
const MAXWELL_DAT: &str = "material viscoelastic_maxwell [\n  name = anything\n  rho = 7800\n  Einf = 15e6\n  nu = 0.0\n  Eta = [40e6]\n  Ev = [10e6]\n  Plane_Stress = 0\n]\n";
/// The same material without `Plane_Stress`.
const MAXWELL_DAT_ANY: &str = "material viscoelastic_maxwell [\n  name = anything\n  rho = 7800\n  Einf = 15e6\n  nu = 0.0\n  Eta = [40e6]\n  Ev = [10e6]\n]\n";

/// Material-point part of `test_material_viscoelasti_maxwell_relaxation`
/// (`UNSTABLE` upstream): biaxial strain `ε = 0.1` ramped over `T = 10`
/// then held (the boundary conditions impose a homogeneous `u = εX`),
/// `Δt = 0.1`, `σ_xx = pre·ε/T (E∞ t + λ E_v (1 − e^{−t/λ}))` during the
/// ramp, relative tolerance 1e-6 as upstream. Each step evaluates the stress
/// twice (residual evaluations of a Newton step). The corrected recurrence
/// (default) passes; the upstream mode (UD-029: branch stresses never
/// updated) does not — which is why the upstream test is `UNSTABLE`.
#[test]
fn test_material_viscoelastic_maxwell_relaxation() {
    let run = |upstream: bool| -> Real {
        let model = one_element(2);
        let mut h = law(&model, MAXWELL_DAT, upstream);
        let l = h.law_mut(0).unwrap();
        let (eps, sim_time, t_ramp, dt) = (0.1, 100.0, 10.0, 0.1);
        let (einf, nu) = (get(l, "Einf"), get(l, "nu"));
        let (eta, ev) = match (l.get_param("Eta").unwrap(), l.get_param("Ev").unwrap()) {
            (ParamValue::VectorReal(a), ParamValue::VectorReal(b)) => (a[0], b[0]),
            _ => unreachable!(),
        };
        let lambda = eta / ev;
        let pre_mult = 1. / (1. + nu) / (1. - 2. * nu);
        let timed = TimedModel { inner: &model, dt };
        let mut p = Point {
            law: l,
            ty: simplex(2),
        };
        let max_steps = (sim_time / dt) as usize + 1;
        let mut time = 0.0;
        let mut worst = 0.0_f64;
        for _ in 0..=max_steps {
            let epsilon = if time < t_ramp {
                eps * time / t_ramp
            } else {
                eps
            };
            p.step(&timed, &col_major(&[&[epsilon, 0.], &[0., epsilon]]), 2);
            let solution = if time < t_ramp {
                pre_mult * eps / t_ramp
                    * (einf * time + lambda * ev * (1. - (-time / lambda).exp()))
            } else {
                pre_mult
                    * eps
                    * (einf
                        + lambda * ev / t_ramp
                            * (((t_ramp - time) / lambda).exp() - (-time / lambda).exp()))
            };
            let rel = relative_error(p.stress()[0], solution);
            worst = worst.nan_max(rel);
            time += dt;
        }
        worst
    };
    let corrected = run(false);
    assert!(
        corrected <= 1e-6,
        "corrected recurrence: max relative error {corrected}"
    );
    let upstream = run(true);
    assert!(
        upstream > 1e-2,
        "upstream mode unexpectedly matches ({upstream})"
    );
    eprintln!("Maxwell relaxation: corrected {corrected:e}, upstream {upstream:e} (UD-029)");
}

/// Closed form of the exponential integrator: strain ramped over the first
/// step `[0, Δt]` then held: branch `k` carries
/// `E_k λ_k/Δt (1 − e^{−Δt/λ_k}) e^{−(t − Δt)/λ_k} C:ε`, so
/// `σ(t) = (E∞ + Σ_k …) C:ε` at every later grid point (3-D, two branches,
/// a general symmetric strain). Bound 64 u·max|σ| (a few dozen flops per
/// entry, repeated scaling by `e_k` over ≤ 40 steps).
#[test]
fn maxwell_relaxation_matches_closed_form_3d_two_branches() {
    let dat = "material viscoelastic_maxwell [\n  name = m\n  Einf = 3e6\n  nu = 0.3\n  Eta = [4e7, 1e6]\n  Ev = [1e7, 5e6]\n]\n";
    let model = one_element(3);
    let mut h = law(&model, dat, false);
    let l = h.law_mut(0).unwrap();
    let dt = 0.25;
    let timed = TimedModel { inner: &model, dt };
    let ty = simplex(3);
    let g = col_major(&[
        &[1e-3, 2e-4, -1e-4],
        &[2e-4, -5e-4, 3e-4],
        &[-1e-4, 3e-4, 2e-4],
    ]);
    let mut p = Point { law: l, ty };
    let (einf, nu, eta, ev) = (3e6, 0.3_f64, [4e7, 1e6], [1e7, 5e6]);
    // unit isotropic stiffness acting on ε (tensor form): pre·((1−2ν)ε + ν tr ε I)
    let pre = 1. / ((1. + nu) * (1. - 2. * nu));
    let tr = g[0] + g[4] + g[8];
    let c_eps: Vec<Real> = (0..9)
        .map(|l| pre * ((1. - 2. * nu) * g[l] + if l % 4 == 0 { nu * tr } else { 0. }))
        .collect();
    let max = c_eps.iter().fold(0.0_f64, |m, x| m.nan_max(x.abs())) * (einf + ev[0] + ev[1]);
    p.step(&timed, &[0.; 9], 1);
    for s in 1..=40 {
        p.step(&timed, &g, 1);
        let t = s as Real * dt;
        let mut modulus = einf;
        for k in 0..2 {
            let lam = eta[k] / ev[k];
            modulus += ev[k] * lam / dt * (1. - (-dt / lam).exp()) * (-(t - dt) / lam).exp();
        }
        let sigma = p.stress();
        for l in 0..9 {
            let want = modulus * c_eps[l];
            assert!(
                (sigma[l] - want).abs() <= 64. * Real::EPSILON * max,
                "step {s} component {l}: {} vs {want}",
                sigma[l]
            );
        }
    }
}

/// Stress-controlled creep at a material point (secant solve of the affine
/// step map), instantaneous elastic response at `t = 0` (a first step with
/// `Δt = 1e-17`, so that `exp(−Δt/λ) == 1` and the branch takes its full
/// stiffness exactly — with a merely small `Δt` the factor `1 − e^{−Δt/λ}`
/// loses `≈ u·λ/Δt` relative accuracy, upstream's arithmetic), then
/// constant stress. Closed form of the standard
/// solid: `J(t) = 1/E∞ − E1/(E∞ E0) e^{−t/λ_r}`, `λ_r = λ E0/E∞`. Returns
/// the relative error of the strain at `t_end` for `n` steps.
fn creep_error(
    dat: &str,
    dim: usize,
    n: usize,
    t_end: Real,
    modulus: (Real, Real, Real),
    shear: bool,
) -> Real {
    let model = one_element(dim);
    let mut h = law(&model, dat, false);
    let l = h.law_mut(0).unwrap();
    let ty = simplex(dim);
    let mut p = Point { law: l, ty };
    let sigma0 = 1e3;
    let unit = |a: Real| -> Vec<Real> {
        if shear {
            col_major(&[&[0., a], &[a, 0.]])
        } else {
            vec![a]
        }
    };
    let comp = |s: &[Real]| if shear { s[2] } else { s[0] };
    let mut a = 0.0;
    let solve_step = |p: &mut Point<'_>, dt: Real, a: &mut Real| {
        let timed = TimedModel { inner: &model, dt };
        p.law
            .before_solve_step(&timed.ctx(LawPhase::BeforeSolveStep))
            .unwrap();
        let eval = |p: &mut Point<'_>, x: Real| {
            p.set_grad_u(&unit(x));
            p.law
                .compute_stress(&timed.ctx(LawPhase::Stress), ty, NG)
                .unwrap();
            comp(&p.stress())
        };
        let (x0, x1) = (*a, *a + 1e-6);
        let (s0, s1) = (eval(p, x0), eval(p, x1));
        let x = x0 + (sigma0 - s0) * (x1 - x0) / (s1 - s0);
        eval(p, x);
        *a = x;
        p.law
            .after_solve_step(&timed.ctx(LawPhase::AfterSolveStep), true)
            .unwrap();
    };
    solve_step(&mut p, 1e-17, &mut a);
    let dt = t_end / n as Real;
    for _ in 0..n {
        solve_step(&mut p, dt, &mut a);
    }
    let (e_inf, e1, lambda) = modulus;
    let e0 = e_inf + e1;
    let lr = lambda * e0 / e_inf;
    let j = 1. / e_inf - e1 / (e_inf * e0) * (-t_end / lr).exp();
    // shear: `modulus` holds the moduli `2G` acting on ε12 (τ = 2G ε12)
    let want = sigma0 * j;
    ((a - want) / want).abs()
}

/// Creep of the corrected Maxwell law (1-D, one branch) and of the SLS
/// (2-D pure shear, shear moduli `G∞ = γ∞μ`, `G1 = γ_vμ`, relaxation time
/// `η/Ev`) converge to the standard-solid closed form at second order. The
/// Maxwell recurrence integrates each branch exactly for piecewise-linear
/// strain; the SLS exponential midpoint rule is not exact (coefficient
/// `e^{−x/2}` instead of `(1 − e^{−x})/x`, `x = Δt/τ`: relative difference
/// `−x²/24`), hence second order. The creep load
/// histories are not piecewise linear in strain in either case.
#[test]
fn creep_converges_to_closed_form() {
    let mx = "material viscoelastic_maxwell [\n  name = m\n  Einf = 15e6\n  Eta = [40e6]\n  Ev = [10e6]\n]\n";
    let errs: Vec<Real> = [20, 40, 80]
        .iter()
        .map(|&n| creep_error(mx, 1, n, 10., (15e6, 10e6, 4.), false))
        .collect();
    assert!(errs[2] < 1e-4, "Maxwell creep error {errs:?}");
    assert!(
        errs[0] / errs[1] > 3.5 && errs[1] / errs[2] > 3.5,
        "Maxwell order: {errs:?}"
    );
    let (e, nu, eta, ev) = (15.0_f64, 0.25_f64, 10.0, 5.0);
    let mu = e / (2. * (1. + nu));
    let (ginf, g1) = ((e - ev) / e * mu, ev / e * mu);
    let errs: Vec<Real> = [20, 40, 80]
        .iter()
        .map(|&n| creep_error(SLS_DAT, 2, n, 10., (2. * ginf, 2. * g1, eta / ev), true))
        .collect();
    assert!(errs[2] < 1e-4, "SLS creep error {errs:?}");
    assert!(
        errs[0] / errs[1] > 3.5 && errs[1] / errs[2] > 3.5,
        "SLS order: {errs:?}"
    );
}

/// UD-029 (SLS): the corrected dissipation is frame invariant and its
/// increments are non-negative (`α |γ_v s − q|²Δt/τ`); upstream's entry sum
/// of the matrix product changes when the same loading is applied in a
/// rotated frame (combined extension/shear in two frames).
#[test]
fn ud029_sls_dissipation_frame_invariance() {
    let rot = {
        let (a, b) = (0.6_f64, -0.4_f64);
        let (ca, sa, cb, sb) = (a.cos(), a.sin(), b.cos(), b.sin());
        // R = Rz(a)·Rx(b)
        [
            [ca, -sa * cb, sa * sb],
            [sa, ca * cb, -ca * sb],
            [0., sb, cb],
        ]
    };
    let rotate = |g: &[Real]| -> Vec<Real> {
        let mut out = vec![0.0; 9];
        for i in 0..3 {
            for j in 0..3 {
                let mut s = 0.0;
                for k in 0..3 {
                    for m in 0..3 {
                        s += rot[i][k] * g[k + 3 * m] * rot[j][m];
                    }
                }
                out[i + 3 * j] = s;
            }
        }
        out
    };
    let path: Vec<Vec<Real>> = (0..30)
        .map(|k| {
            let x = (k as Real * 0.3).sin() * 1e-3;
            col_major(&[
                &[x, 0.4 * x, 0.],
                &[0.4 * x, -0.2 * x, 0.1 * x],
                &[0., 0.1 * x, 0.5 * x],
            ])
        })
        .collect();
    let dissipation = |upstream: bool, rotated: bool| -> Vec<Real> {
        let model = one_element(3);
        let mut h = law(&model, SLS_DAT_ANY, upstream);
        let l = h.law_mut(0).unwrap();
        let timed = TimedModel {
            inner: &model,
            dt: 0.5,
        };
        let mut p = Point {
            law: l,
            ty: simplex(3),
        };
        path.iter()
            .map(|g| {
                let g = if rotated { rotate(g) } else { g.clone() };
                p.step(&timed, &g, 1);
                p.internal("dissipated_energy")[0]
            })
            .collect()
    };
    let (c0, c1) = (dissipation(false, false), dissipation(false, true));
    let max = c0.iter().fold(0.0_f64, |m, x| m.nan_max(x.abs()));
    for (k, (a, b)) in c0.iter().zip(&c1).enumerate() {
        assert!(
            (a - b).abs() <= 1e-12 * max,
            "corrected, step {k}: {a} vs {b}"
        );
        if k > 0 {
            assert!(
                *a >= c0[k - 1],
                "corrected dissipation decreased at step {k}"
            );
        }
    }
    let (u0, u1) = (dissipation(true, false), dissipation(true, true));
    let rel = (u0.last().unwrap() - u1.last().unwrap()).abs() / u0.last().unwrap().abs();
    assert!(
        rel > 1e-3,
        "upstream dissipation unexpectedly frame invariant ({rel})"
    );
    eprintln!(
        "UD-029: upstream dissipation differs by {rel:e} between frames; corrected invariant"
    );
}

/// UD-063: with the committed state (default), repeated evaluations within
/// a step are idempotent and a failed step rolls back; upstream advances the
/// history at every evaluation.
#[test]
fn ud063_sls_committed_state() {
    let g1 = col_major(&[&[1e-3, 2e-4], &[2e-4, -3e-4]]);
    let g2 = col_major(&[&[2e-3, 1e-4], &[1e-4, -1e-4]]);
    let run = |upstream: bool, evaluations: usize| -> (Vec<Real>, Vec<Real>) {
        let model = one_element(2);
        let mut h = law(&model, SLS_DAT, upstream);
        let l = h.law_mut(0).unwrap();
        let timed = TimedModel {
            inner: &model,
            dt: 0.5,
        };
        let mut p = Point {
            law: l,
            ty: simplex(2),
        };
        p.step(&timed, &g1, evaluations);
        p.step(&timed, &g2, evaluations);
        (p.stress(), p.internal("history_integral"))
    };
    assert_eq!(
        run(false, 1),
        run(false, 4),
        "default: evaluations are idempotent"
    );
    assert_ne!(
        run(true, 1),
        run(true, 4),
        "upstream: history advanced per evaluation"
    );
    // rollback of a failed step
    let model = one_element(2);
    let mut h = law(&model, SLS_DAT, false);
    let l = h.law_mut(0).unwrap();
    let timed = TimedModel {
        inner: &model,
        dt: 0.5,
    };
    let mut p = Point {
        law: l,
        ty: simplex(2),
    };
    p.step(&timed, &g1, 1);
    let committed = (
        p.internal("history_integral"),
        p.internal("dissipated_energy"),
    );
    p.law
        .before_solve_step(&timed.ctx(LawPhase::BeforeSolveStep))
        .unwrap();
    p.set_grad_u(&g2);
    p.law
        .compute_stress(&timed.ctx(LawPhase::Stress), simplex(2), NG)
        .unwrap();
    p.law
        .after_solve_step(&timed.ctx(LawPhase::AfterSolveStep), false)
        .unwrap();
    assert_eq!(
        (
            p.internal("history_integral"),
            p.internal("dissipated_energy")
        ),
        committed
    );
}

/// Maxwell (corrected): dissipation `W − ψ ≥ 0` along a cyclic 2-D path with
/// two branches, and the energy balance `W = ψ + e_diss` with `ψ` the stored
/// energy of the springs (`½ε:E∞C:ε + Σ_k ½ q_k : C⁻¹ : q_k / E_k`,
/// recomputed independently from `sigma_v`); the dissipated energy converges
/// under time-step refinement.
#[test]
fn maxwell_dissipation_and_energy_balance() {
    let dat = "material viscoelastic_maxwell [\n  name = m\n  Einf = 2e6\n  nu = 0.3\n  Eta = [4e6, 1e5]\n  Ev = [1e6, 5e5]\n]\n";
    let run = |n: usize| -> Real {
        let model = one_element(2);
        let mut h = law(&model, dat, false);
        let l = h.law_mut(0).unwrap();
        let t_end = 20.0;
        let dt = t_end / n as Real;
        let timed = TimedModel { inner: &model, dt };
        let mut p = Point {
            law: l,
            ty: simplex(2),
        };
        let mut max_w = 0.0_f64;
        let mut last = 0.0;
        for k in 1..=n {
            let t = k as Real * dt;
            let x = 1e-2 * (0.5 * t).sin();
            p.step(
                &timed,
                &col_major(&[&[x, 0.3 * x], &[0.3 * x, -0.5 * x]]),
                1,
            );
            let w = p.internal("mechanical_work")[0];
            let d = p.internal("dissipated_energy")[0];
            let psi = p.internal("potential_energy")[0];
            max_w = max_w.nan_max(w.abs());
            assert!(d >= -1e-12 * max_w, "step {k}: dissipation {d} < 0");
            assert!((w - psi - d).abs() <= 1e-12 * max_w);
            // independent stored energy from the branch stresses
            let nu = 0.3_f64;
            let sv = p.internal("sigma_v");
            let g = &p
                .law
                .base()
                .grad_u
                .get(simplex(2), NG)
                .unwrap()
                .tuple(0)
                .to_vec();
            let eps = [g[0], g[3], g[1] + g[2]]; // engineering Voigt
            let pre = 1. / ((1. + nu) * (1. - 2. * nu));
            let c = [
                [pre * (1. - nu), pre * nu, 0.],
                [pre * nu, pre * (1. - nu), 0.],
                [0., 0., pre * 0.5 * (1. - 2. * nu)],
            ];
            let s_ = [
                [(1. + nu) * (1. - nu), -nu * (1. + nu), 0.],
                [-nu * (1. + nu), (1. + nu) * (1. - nu), 0.],
                [0., 0., 2. * (1. + nu)],
            ];
            let mut psi_ref = 0.0;
            for i in 0..3 {
                for j in 0..3 {
                    psi_ref += 0.5 * 2e6 * eps[i] * c[i][j] * eps[j];
                }
            }
            for (k, ek) in [1e6, 5e5].iter().enumerate() {
                let q = [sv[4 * k], sv[4 * k + 3], sv[4 * k + 2]];
                for i in 0..3 {
                    for j in 0..3 {
                        psi_ref += 0.5 * q[i] * s_[i][j] * q[j] / ek;
                    }
                }
            }
            assert!(
                (psi - psi_ref).abs() <= 1e-10 * psi_ref.abs().max(1e-300),
                "step {k}: ψ {psi} vs {psi_ref}"
            );
            last = d;
        }
        last
    };
    let (d1, d2, d3) = (run(100), run(200), run(400));
    let (e1, e2) = ((d1 - d3).abs(), (d2 - d3).abs());
    assert!(
        e2 < e1 && d3 > 0.,
        "dissipation under refinement: {d1}, {d2}, {d3}"
    );
}

/// Elastic limits. SLS: `η → 0` relaxes instantly to the elastic law with
/// `E∞` (bulk and shear); `η → ∞` keeps the instantaneous shear modulus
/// `μ(E)` with the relaxed bulk modulus `γ∞κ` (the viscosity is deviatoric
/// only). Maxwell: `η_k → ∞` is elastic with `E = E∞ + ΣE_k`, `η_k → 0`
/// elastic with `E∞`. Bounds 16 u·max|σ| (different association of the
/// same products).
#[test]
fn viscoelastic_elastic_limits() {
    let g = col_major(&[
        &[1e-3, 4e-4, -2e-4],
        &[4e-4, -6e-4, 1e-4],
        &[-2e-4, 1e-4, 3e-4],
    ]);
    let elastic = |e: Real, nu: Real| -> Vec<Real> {
        let (lambda, mu) = (nu * e / ((1. + nu) * (1. - 2. * nu)), e / (2. * (1. + nu)));
        let tr = g[0] + g[4] + g[8];
        (0..9)
            .map(|l| 2. * mu * g[l] + if l % 4 == 0 { lambda * tr } else { 0. })
            .collect()
    };
    let check = |a: &[Real], b: &[Real], what: &str| {
        let max = b.iter().fold(0.0_f64, |m, x| m.nan_max(x.abs()));
        for (x, y) in a.iter().zip(b) {
            assert!(
                (x - y).abs() <= 16. * Real::EPSILON * max,
                "{what}: {a:?} vs {b:?}"
            );
        }
    };
    let sls = |eta: Real| -> Vec<Real> {
        let dat = format!(
            "material sls_deviatoric [\n  name = s\n  E = 15\n  nu = 0.25\n  Eta = {eta:e}\n  Ev = 5\n]\n"
        );
        let model = one_element(3);
        let mut h = law(&model, &dat, false);
        let l = h.law_mut(0).unwrap();
        let timed = TimedModel {
            inner: &model,
            dt: 1.,
        };
        let mut p = Point {
            law: l,
            ty: simplex(3),
        };
        p.step(&timed, &g, 1);
        p.stress()
    };
    check(&sls(1e-300), &elastic(10., 0.25), "SLS η → 0");
    let instantaneous: Vec<Real> = {
        let (mu, kappa_inf) = (15. / 2.5, 10. / (3. * 0.5));
        let tr = g[0] + g[4] + g[8];
        (0..9)
            .map(|l| {
                2. * mu * (g[l] - if l % 4 == 0 { tr / 3. } else { 0. })
                    + if l % 4 == 0 { kappa_inf * tr } else { 0. }
            })
            .collect()
    };
    check(&sls(1e300), &instantaneous, "SLS η → ∞");
    let maxwell = |eta: Real| -> Vec<Real> {
        let dat = format!(
            "material viscoelastic_maxwell [\n  name = m\n  Einf = 10\n  nu = 0.25\n  Eta = [{eta:e}, {eta:e}]\n  Ev = [3, 2]\n]\n"
        );
        let model = one_element(3);
        let mut h = law(&model, &dat, false);
        let l = h.law_mut(0).unwrap();
        let timed = TimedModel {
            inner: &model,
            dt: 1.,
        };
        let mut p = Point {
            law: l,
            ty: simplex(3),
        };
        p.step(&timed, &[0.; 9], 1);
        p.step(&timed, &g, 1);
        p.stress()
    };
    check(&maxwell(1e300), &elastic(15., 0.25), "Maxwell η → ∞");
    check(&maxwell(1e-300), &elastic(10., 0.25), "Maxwell η → 0");
}

/// The Maxwell tangent is symmetric, equals `(E∞ + ΣE_k^eff)·C` and the
/// stiffness is flagged changed exactly when the time step changes
/// (upstream `hasStiffnessMatrixChanged`).
#[test]
fn maxwell_tangent_and_stiffness_flag() {
    let model = one_element(3);
    let mut h = law(&model, MAXWELL_DAT_ANY, false);
    let l = h.law_mut(0).unwrap();
    let mut p = Point {
        law: l,
        ty: simplex(3),
    };
    let t1 = TimedModel {
        inner: &model,
        dt: 0.5,
    };
    assert!(p.law.has_matrix_changed_in(&t1.ctx(LawPhase::Tangent), "K"));
    let tan = p.tangent(&t1);
    for i in 0..6 {
        for j in 0..6 {
            assert_eq!(tan[i + 6 * j].to_bits(), tan[j + 6 * i].to_bits());
        }
    }
    let lam = 4.0_f64;
    let e_eff = 15e6 + (1. - (-0.5 / lam).exp()) * 10e6 * lam / 0.5;
    assert!(
        (tan[0] - e_eff).abs() <= 4. * Real::EPSILON * e_eff,
        "{} vs {e_eff}",
        tan[0]
    );
    assert!(!p.law.has_matrix_changed_in(&t1.ctx(LawPhase::Tangent), "K"));
    let t2 = TimedModel {
        inner: &model,
        dt: 0.25,
    };
    assert!(p.law.has_matrix_changed_in(&t2.ctx(LawPhase::Tangent), "K"));
    p.law.set_options(LawOptions::default()).unwrap();
    assert!(
        p.law.has_matrix_changed_in(&t1.ctx(LawPhase::Tangent), "K"),
        "options change flags K"
    );
}

/// UD-157: parameter updates at a fixed time step change
/// the Maxwell tangent and must flag the stiffness changed (cleared only by
/// the next successful tangent), through the registry (`Einf`, `Ev`, `Eta`,
/// parent `nu`) and through `set_options`. 1-D case: `dt = 1`,
/// `Ev = [5]`, `Eta = [10]`, `Einf: 10 → 20` changes `K` from
/// 13.934693402873666 to 23.934693402873666. Upstream compat mode keeps
/// upstream's time-step-only query. (Before the fix the query stayed `false`.)
#[test]
fn mv05_maxwell_parameter_updates_flag_the_stiffness() {
    let dat = "material viscoelastic_maxwell [\n  name = m\n  rho = 1\n  Einf = 10\n  nu = 0.25\n  Eta = [10]\n  Ev = [5]\n]\n";
    for dim in [1, 3] {
        let model = one_element(dim);
        let timed = TimedModel {
            inner: &model,
            dt: 1.,
        };
        let ctx = || timed.ctx(LawPhase::Tangent);
        let mut h = law(&model, dat, false);
        let mut p = Point {
            law: h.law_mut(0).unwrap(),
            ty: simplex(dim),
        };
        let k0 = p.tangent(&timed)[0];
        let c00 = if dim == 1 { 1. } else { 0.75 / (1.25 * 0.5) };
        let e_eff = (1. - (-0.5_f64).exp()) * 5. * 2.;
        assert!((k0 - (10. + e_eff) * c00).abs() <= 1e-14 * k0, "{k0}");
        if dim == 1 {
            assert_eq!(k0, 13.934_693_402_873_666);
        }
        assert!(!p.law.has_matrix_changed_in(&ctx(), "K"));
        assert!(!p.law.has_stiffness_matrix_changed());
        let updates: [(&str, ParamValue); 3] = [
            ("Einf", ParamValue::Real(20.)),
            ("Ev", ParamValue::VectorReal(vec![6.])),
            ("Eta", ParamValue::VectorReal(vec![12.])),
        ];
        let mut previous = k0;
        for (name, value) in updates {
            p.law.set_param(name, value).unwrap();
            assert!(
                p.law.has_matrix_changed_in(&ctx(), "K"),
                "{dim}-D: {name} update must flag K"
            );
            assert!(p.law.has_stiffness_matrix_changed());
            let k = p.tangent(&timed)[0];
            assert_ne!(k, previous, "{dim}-D: {name} changes the tangent");
            previous = k;
            assert!(!p.law.has_matrix_changed_in(&ctx(), "K"));
        }
        if dim == 1 {
            assert_eq!(k0 + 10., 23.934_693_402_873_666);
        } else {
            p.law.set_param("nu", ParamValue::Real(0.3)).unwrap();
            assert!(p.law.has_matrix_changed_in(&ctx(), "K"), "nu update");
            let k = p.tangent(&timed)[0];
            assert_ne!(k, previous);
        }
        assert!(!p.law.has_matrix_changed_in(&ctx(), "K"));
        p.law.set_options(LawOptions::default()).unwrap();
        assert!(p.law.has_matrix_changed_in(&ctx(), "K"), "set_options");
        // a failed tangent (wrong buffer) does not clear the flag
        let mut bad = akantu_core::Array::zeros(2, 1);
        assert!(
            p.law
                .compute_tangent_moduli(&ctx(), simplex(dim), NG, &mut bad)
                .is_err()
        );
        assert!(p.law.has_matrix_changed_in(&ctx(), "K"));

        // upstream: only the time step is compared
        let mut h = law(&model, dat, true);
        let mut p = Point {
            law: h.law_mut(0).unwrap(),
            ty: simplex(dim),
        };
        let _ = p.tangent(&timed);
        p.law.set_param("Einf", ParamValue::Real(20.)).unwrap();
        assert!(!p.law.has_matrix_changed_in(&ctx(), "K"));
    }
}

/// `Eta`/`Ev` of different lengths and finite deformation are rejected
/// (upstream: debug assertions only).
#[test]
fn maxwell_rejects_inconsistent_branches() {
    let model = one_element(2);
    let bad =
        "material viscoelastic_maxwell [\n  name = m\n  Einf = 1\n  Eta = [1, 2]\n  Ev = [1]\n]\n";
    assert!(materials_from_dat_with(&model, bad.as_bytes(), None).is_err());
    let fd = "material viscoelastic_maxwell [\n  name = m\n  Einf = 1\n  Eta = [1]\n  Ev = [1]\n  finite_deformation = true\n]\n";
    assert!(materials_from_dat_with(&model, fd.as_bytes(), None).is_err());
}

/// Parameter registry: `Einf = E − Ev` is read-only and follows `E`;
/// `E` of the Maxwell law is `Einf + Σ|Ev_k|` after `initMaterial`.
#[test]
fn viscoelastic_parameters() {
    let model = one_element(2);
    let mut h = law(&model, SLS_DAT, false);
    let l = h.law_mut(0).unwrap();
    assert!(
        l.set_param("Einf", ParamValue::Real(1.)).is_err(),
        "Einf is read-only"
    );
    l.set_param("E", ParamValue::Real(20.)).unwrap();
    assert_eq!(get(l, "Einf"), 15.);
    let names = l.param_names();
    let pos = |n: &str| names.iter().position(|x| *x == n).unwrap();
    assert!(pos("E") < pos("Eta") && pos("Eta") < pos("Ev") && pos("Ev") < pos("Einf"));
    let mut h = law(
        &model,
        "material viscoelastic_maxwell [\n  name = m\n  Einf = 2\n  Eta = [1, 2]\n  Ev = [3, -4]\n]\n",
        false,
    );
    let l = h.law_mut(0).unwrap();
    assert_eq!(get(l, "E"), 9.);
    assert_eq!(get(l, "previous_dt"), 0.);
}
