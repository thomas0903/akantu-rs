// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Regression tests
// of the viscoelastic, anisotropic-damage and phase-field
// materials (src/model/solid_mechanics/materials/material_viscoelastic/*,
// material_damage/material_{phasefield,damage}*, and the shared
// MaterialDamage::updateEnergies of material_damage_tmpl.hh).

//! Tests named `mvNN_*` (the anisotropic-damage case is in
//! `test_material_anisotropic_damage.rs`, the Maxwell tangent one in
//! `test_material_viscoelastic.rs`). Each checks the corrected default and
//! that the compatibility switch still shows the defect.

mod common;
#[path = "common/timed.rs"]
mod timed;

use akantu_core::Real;
use akantu_model::constitutive_law::LawPhase;
use akantu_solid::Material;
use akantu_solid::material::{FiniteStrainWork, LawOptions};
use akantu_validation::reduce::NanMinMax;
use common::{NG, SimplexModel, materials_from_dat_with, simplex};
use timed::{Point, TimedModel, col_major, one_element};

type Handler = akantu_model::ConstitutiveLawsHandler<dyn Material>;

fn handler(model: &SimplexModel, dat: &str, options: LawOptions) -> Handler {
    materials_from_dat_with(model, dat.as_bytes(), Some(options)).unwrap()
}

/// One converged step with prescribed damage (the coupler's role) and
/// temperature change: `beforeSolveStep`, internals, `∇u`, `computeStress`,
/// `afterSolveStep(true)`.
fn step(
    p: &mut Point<'_>,
    timed: &TimedModel<'_>,
    g: &[Real],
    damage: Option<Real>,
    delta_t: Option<Real>,
) {
    p.law
        .before_solve_step(&timed.ctx(LawPhase::BeforeSolveStep))
        .unwrap();
    if let Some(d) = damage {
        p.write_internal("damage", &[d]);
    }
    if let Some(t) = delta_t {
        p.write_internal("delta_T", &[t]);
    }
    p.set_grad_u(g);
    p.law
        .compute_stress(&timed.ctx(LawPhase::Stress), p.ty, NG)
        .unwrap();
    p.law
        .after_solve_step(&timed.ctx(LawPhase::AfterSolveStep), true)
        .unwrap();
}

/// `(ψ, ∫σ, e_diss)` densities of point 0 after a converged step.
fn energies(p: &Point<'_>) -> (Real, Real, Real) {
    let psi = p
        .law
        .base()
        .potential_energy
        .get(p.ty, NG)
        .unwrap()
        .as_slice()[0];
    (
        psi,
        p.internal("integral of sigma")[0],
        p.internal("damage dissipated energy")[0],
    )
}

fn pf_dat(dim: usize, split: &str, extra: &str) -> String {
    let (e, nu) = if dim == 1 { (100., 0.) } else { (210., 0.3) };
    format!(
        "material phasefield {split} [\n  name = pf\n  rho = 1\n  E = {e}\n  nu = {nu}\n{extra}]\n"
    )
}

fn timed(model: &SimplexModel) -> TimedModel<'_> {
    TimedModel {
        inner: model,
        dt: 1.,
    }
}

fn identity(dim: usize) -> Vec<Real> {
    let mut i = vec![0.0; dim * dim];
    for k in 0..dim {
        i[k * (dim + 1)] = 1.;
    }
    i
}

/// `∇u = R·(I + H) − I` for the rotation `R(θ)` about the last axis.
fn rotated(dim: usize, h: &[Real], theta: Real) -> Vec<Real> {
    let (c, s) = (theta.cos(), theta.sin());
    let mut r = identity(dim);
    r[0] = c;
    r[1] = s; // (1, 0) column-major
    r[dim] = -s; // (0, 1)
    r[dim + 1] = c;
    let f: Vec<Real> = (0..dim * dim).map(|k| identity(dim)[k] + h[k]).collect();
    let mut out = vec![0.0; dim * dim];
    for j in 0..dim {
        for i in 0..dim {
            let rf: Real = (0..dim).map(|k| r[i + k * dim] * f[k + j * dim]).sum();
            out[i + j * dim] = rf - identity(dim)[i + j * dim];
        }
    }
    out
}

// ---------------------------------------------------------------------------
// `mv02_*`: finite-strain work pairs PK2 with the Green strain
// ---------------------------------------------------------------------------

/// Undamaged 1-D `E = 100`, `F: 1 → 1.2`,
/// `finite_deformation`. `S = E·E_G = 22`, `ψ = ½S·E_G = 2.42`. Default:
/// accumulated work `½(S + 0)·ΔE_G = 2.42`, dissipation 0. The old pairing
/// with `Δ∇u` (the default before the fix with the PK2 history, and upstream's
/// whole update) gives work 2.20 and dissipation −0.22.
#[test]
fn mv02_reviewer_case_1d_undamaged_finite_strain() {
    let model = one_element(1);
    let t = timed(&model);
    let dat = pf_dat(1, "no_split", "  finite_deformation = true\n");
    for (mode, want_work) in [
        (FiniteStrainWork::Conjugate, 2.42),
        (FiniteStrainWork::UpstreamMixed, 2.2),
    ] {
        let options = LawOptions {
            finite_strain_work: mode,
            ..LawOptions::default()
        };
        let mut h = handler(&model, &dat, options);
        let mut p = Point {
            law: h.law_mut(0).unwrap(),
            ty: simplex(1),
        };
        step(&mut p, &t, &[0.2], Some(0.), None);
        let (psi, work, diss) = energies(&p);
        assert!((psi - 2.42).abs() < 1e-13, "{mode:?}: psi = {psi}");
        assert!((work - want_work).abs() < 1e-13, "{mode:?}: work = {work}");
        if mode == FiniteStrainWork::Conjugate {
            assert!(diss.abs() < 1e-13, "dissipation {diss}");
        } else {
            assert!((diss + 0.22).abs() < 1e-13, "upstream pairing: {diss}");
        }
    }
}

/// Undamaged finite-strain loading, unloading, reloading and a rigid
/// rotation of the loaded state (2-D plane strain and 3-D, both splits).
/// With `S = C:E` (St Venant–Kirchhoff kernel on the Green strain) the
/// trapezoidal `½(S_{n+1} + S_n):ΔE` equals `Δψ` exactly, so the
/// dissipation is rounding only; the rotation changes neither `E` nor `ψ`
/// (and adds no work by default), while the `Δ∇u` pairing books spurious
/// work at every step and under the rotation.
#[test]
fn mv02_finite_strain_loading_unloading_and_rotation() {
    for dim in [2, 3] {
        for split in ["no_split", "volumetric_deviatoric"] {
            let model = one_element(dim);
            let t = timed(&model);
            let dat = pf_dat(dim, split, "  finite_deformation = true\n");
            let h1: Vec<Real> = (0..dim * dim)
                .map(|k| [0.04, 0.01, -0.02, 0.015, 0.03, 0.005, -0.01, 0.02, 0.05][k])
                .collect();
            let half: Vec<Real> = h1.iter().map(|x| 0.5 * x).collect();
            let zero = vec![0.0; dim * dim];
            let path = [
                half.clone(),
                h1.clone(),
                half.clone(),
                zero.clone(),
                h1.clone(),
                rotated(dim, &h1, 0.4),
                rotated(dim, &h1, 1.1),
            ];
            for mode in [FiniteStrainWork::Conjugate, FiniteStrainWork::UpstreamMixed] {
                let options = LawOptions {
                    finite_strain_work: mode,
                    ..LawOptions::default()
                };
                let mut h = handler(&model, &dat, options);
                let mut p = Point {
                    law: h.law_mut(0).unwrap(),
                    ty: simplex(dim),
                };
                let mut psi_max = 0.0_f64;
                let mut worst = 0.0_f64;
                let mut psi_loaded = 0.0;
                for (k, g) in path.iter().enumerate() {
                    step(&mut p, &t, g, Some(0.), None);
                    let (psi, _, diss) = energies(&p);
                    psi_max = psi_max.nan_max(psi);
                    worst = worst.nan_max(diss.abs());
                    if k == 4 {
                        psi_loaded = psi;
                    }
                    if k >= 5 {
                        // rotation: ψ is frame indifferent
                        assert!((psi - psi_loaded).abs() <= 1e-12 * psi_loaded);
                    }
                }
                if mode == FiniteStrainWork::Conjugate {
                    assert!(
                        worst <= 1e-13 * psi_max,
                        "{dim}-D {split}: |e_diss| = {worst} (ψ ~ {psi_max})"
                    );
                } else {
                    assert!(
                        worst > 1e-3 * psi_max,
                        "{dim}-D {split}: the Δ∇u pairing should book spurious work"
                    );
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// `mv03_*`: reversible thermal work is not damage dissipation
// ---------------------------------------------------------------------------

/// Undamaged 1-D `E = 100`, `α = 0.01`, zero
/// displacement, `ΔT: 0 → 1`: `σ = −1`, `ψ = 0.005`. Default: the
/// reversible thermal work `−½(σ_{n+1} + σ_n)Δε_th = 0.005` is booked, the
/// dissipation is 0; upstream bookkeeping (`damage_thermal_work = false`):
/// mechanical work 0 and dissipation −0.005.
#[test]
fn mv03_reviewer_case_1d_heating() {
    let model = one_element(1);
    let t = timed(&model);
    let dat = pf_dat(1, "no_split", "  alpha = 0.01\n");
    for corrected in [true, false] {
        let options = LawOptions {
            damage_thermal_work: corrected,
            ..LawOptions::default()
        };
        let mut h = handler(&model, &dat, options);
        let mut p = Point {
            law: h.law_mut(0).unwrap(),
            ty: simplex(1),
        };
        step(&mut p, &t, &[0.], Some(0.), Some(1.));
        let (psi, work, diss) = energies(&p);
        assert!((psi - 0.005).abs() < 1e-16, "psi = {psi}");
        if corrected {
            assert!((work - 0.005).abs() < 1e-16, "work = {work}");
            assert!(diss.abs() < 1e-16, "dissipation = {diss}");
        } else {
            assert_eq!(work, 0.);
            assert!((diss + 0.005).abs() < 1e-16, "upstream: {diss}");
        }
    }
}

/// Undamaged heating/cooling cycles with simultaneous straining
/// (phase field 1-D/2-D/3-D, no split) dissipate nothing (the trapezoidal
/// work of the quadratic stored energy is exact); free thermal expansion
/// (`∇u = ε_th·I`) is stress- and energy-free. Upstream bookkeeping drifts.
#[test]
fn mv03_phasefield_undamaged_thermal_cycles() {
    for dim in 1..=3 {
        let model = one_element(dim);
        let t = timed(&model);
        let dat = pf_dat(dim, "no_split", "  alpha = 1e-3\n");
        let base: Vec<Real> = (0..dim * dim)
            .map(|k| [2e-3, 5e-4, -1e-3, 7e-4, 1.5e-3, 2e-4, -3e-4, 4e-4, 1e-3][k])
            .collect();
        let temps = [1., -0.5, 2., 0.3, -1., 0.];
        let scales = [0.5, 1., -0.3, 0.8, 0., 0.2];
        for corrected in [true, false] {
            let options = LawOptions {
                damage_thermal_work: corrected,
                ..LawOptions::default()
            };
            let mut h = handler(&model, &dat, options);
            let mut p = Point {
                law: h.law_mut(0).unwrap(),
                ty: simplex(dim),
            };
            let (mut psi_max, mut worst) = (0.0_f64, 0.0_f64);
            for (dt, sc) in temps.iter().zip(scales) {
                let g: Vec<Real> = base.iter().map(|x| x * sc).collect();
                step(&mut p, &t, &g, Some(0.), Some(*dt));
                let (psi, _, diss) = energies(&p);
                psi_max = psi_max.nan_max(psi);
                worst = worst.nan_max(diss.abs());
            }
            if corrected {
                assert!(
                    worst <= 1e-12 * psi_max,
                    "{dim}-D: |e_diss| = {worst} (ψ ~ {psi_max})"
                );
            } else {
                assert!(worst > 1e-2 * psi_max, "{dim}-D: upstream drift expected");
            }
        }
        // free expansion: ε = ε_th I (θ = epsilon_th of the law)
        let mut h = handler(&model, &dat, LawOptions::default());
        let mut p = Point {
            law: h.law_mut(0).unwrap(),
            ty: simplex(dim),
        };
        step(&mut p, &t, &vec![0.0; dim * dim], Some(0.), Some(2.));
        let th = p.internal("epsilon_th")[0];
        let g: Vec<Real> = identity(dim).iter().map(|x| x * th).collect();
        step(&mut p, &t, &g, Some(0.), Some(2.));
        let (psi, _, diss) = energies(&p);
        assert!(
            p.stress().iter().all(|s| s.abs() < 1e-12),
            "{dim}-D free expansion"
        );
        assert!(
            psi.abs() < 1e-18 && diss.abs() < 1e-15,
            "{dim}-D: {psi} {diss}"
        );
    }
}

/// Damaged thermal cycle (2-D plane strain, no split). Steps alternate
/// heating/cooling at fixed damage and strain (no dissipation) and damage
/// growth at fixed strain and temperature, whose dissipation is exactly the
/// release `(g(d_n) − g(d_{n+1}))·ψ⁺(ε − ε_th I)` with `ψ⁺ = ½λ tr² + μ ε':ε'`
/// (the work increment vanishes). The total equals the sum of releases.
#[test]
fn mv03_phasefield_damaged_thermal_cycle() {
    let model = one_element(2);
    let t = timed(&model);
    let dat = pf_dat(2, "no_split", "  alpha = 1e-3\n");
    let mut h = handler(&model, &dat, LawOptions::default());
    let mut p = Point {
        law: h.law_mut(0).unwrap(),
        ty: simplex(2),
    };
    let (e, nu) = (210.0_f64, 0.3_f64);
    let lambda = nu * e / ((1. + nu) * (1. - 2. * nu));
    let mu = e / (2. * (1. + nu));
    let g = col_major(&[&[3e-3, 1e-3], &[1e-3, -1e-3]]);
    let psi_plus = |th: Real| {
        let ep = [g[0] - th, g[1], g[2], g[3] - th];
        let tr = ep[0] + ep[3];
        0.5 * lambda * tr * tr + mu * ep.iter().map(|x| x * x).sum::<Real>()
    };
    let deg = |d: Real| (1. - d) * (1. - d);
    step(&mut p, &t, &g, Some(0.), Some(0.));
    let mut expected = 0.0;
    let mut d = 0.;
    for (k, (temp, d_new)) in [(1.5, 0.1), (-1., 0.3), (2., 0.5), (0.5, 0.6)]
        .into_iter()
        .enumerate()
    {
        let before = energies(&p).2;
        step(&mut p, &t, &g, Some(d), Some(temp));
        let after_heat = energies(&p).2;
        assert!(
            (after_heat - before).abs() < 1e-15,
            "step {k}: heating at fixed damage dissipated {}",
            after_heat - before
        );
        let th = p.internal("epsilon_th")[0];
        step(&mut p, &t, &g, Some(d_new), Some(temp));
        let release = (deg(d) - deg(d_new)) * psi_plus(th);
        expected += release;
        let now = energies(&p).2;
        assert!(release > 0.);
        assert!(
            (now - after_heat - release).abs() <= 1e-12 * release,
            "step {k}: {} vs release {release}",
            now - after_heat
        );
        d = d_new;
    }
    let total = energies(&p).2;
    assert!((total - expected).abs() <= 1e-12 * expected);
}

const MAZARS_COLD: &str = "material mazars [\n  name = concrete\n  rho = 3000\n  E = 25e9\n  nu = 0.2\n  \
                           K0 = 1\n  At = 1.0\n  Bt = 5e3\n  Ac = 0.8\n  Bc = 1391.3\n  beta = 1.\n  \
                           alpha = 1e-5\n{extra}]\n";
const MARIGO_COLD: &str = "material marigo [\n  name = marigo\n  rho = 3000\n  E = 25e9\n  nu = 0.2\n  \
                           Sd = 5e3\n  Yd = 1e12\n  alpha = 1e-5\n{extra}]\n";
const PLASTIC_MAZARS_COLD: &str = "material plastic_mazars [\n  name = pm\n  rho = 3000\n  E = 25e9\n  \
                                   nu = 0.2\n  sigma_y = 1e12\n  h = 1e9\n  K0 = 1\n  At = 1.0\n  \
                                   Bt = 5e3\n  Ac = 0.8\n  Bc = 1391.3\n  beta = 1.\n  alpha = 1e-5\n{extra}]\n";

/// `mv03_*` for the laws sharing the bookkeeping (`marigo`, `mazars`,
/// `plastic_mazars`; thresholds out of reach, so no damage or plasticity):
/// heating/cooling with straining in 1-D, plane strain (the out-of-plane
/// constraint stress enters `∂ψ/∂θ`), plane stress and 3-D dissipates
/// nothing by default; upstream bookkeeping books the thermal energy change
/// as dissipation.
#[test]
fn mv03_wpmat2_damage_laws_undamaged_thermal_cycles() {
    for (law, dat) in [
        ("mazars", MAZARS_COLD),
        ("marigo", MARIGO_COLD),
        ("plastic_mazars", PLASTIC_MAZARS_COLD),
    ] {
        for (dim, extra) in [(1, ""), (2, ""), (2, "  Plane_Stress = true\n"), (3, "")] {
            let model = one_element(dim);
            let t = timed(&model);
            let text = dat.replace("{extra}", extra);
            let base: Vec<Real> = (0..dim * dim)
                .map(|k| [2e-4, 5e-5, -1e-4, 7e-5, 1.5e-4, 2e-5, -3e-5, 4e-5, 1e-4][k])
                .collect();
            for corrected in [true, false] {
                let options = LawOptions {
                    damage_thermal_work: corrected,
                    ..LawOptions::default()
                };
                let mut h = handler(&model, &text, options);
                let mut p = Point {
                    law: h.law_mut(0).unwrap(),
                    ty: simplex(dim),
                };
                let (mut psi_max, mut worst) = (0.0_f64, 0.0_f64);
                for (temp, sc) in [(10., 0.5), (-5., 1.), (20., -0.3), (3., 0.8), (0., 0.)] {
                    let g: Vec<Real> = base.iter().map(|x| x * sc).collect();
                    step(&mut p, &t, &g, None, Some(temp));
                    assert!(p.internal("damage")[0] == 0.);
                    let (psi, _, diss) = energies(&p);
                    psi_max = psi_max.nan_max(psi);
                    worst = worst.nan_max(diss.abs());
                }
                let tag = format!("{law} {dim}-D {extra:?}");
                if corrected {
                    assert!(
                        worst <= 1e-12 * psi_max,
                        "{tag}: |e_diss| = {worst} (ψ ~ {psi_max})"
                    );
                } else {
                    assert!(worst > 1e-2 * psi_max, "{tag}: upstream drift expected");
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// `mv04_*`: SLS stored energy includes the branch spring
// ---------------------------------------------------------------------------

const SLS: &str = "material sls_deviatoric [\n  name = sls\n  rho = 1\n  E = 15\n  nu = 0.25\n  Eta = 10\n  Ev = 5\n{extra}]\n";

/// Potential-energy density of point 0 (this law's `computePotentialEnergy`).
fn potential_density(p: &mut Point<'_>, t: &TimedModel<'_>) -> Real {
    p.law
        .compute_potential_energy(&t.ctx(LawPhase::Energy), p.ty)
        .unwrap();
    p.law
        .base()
        .potential_energy
        .get(p.ty, NG)
        .unwrap()
        .as_slice()[0]
}

fn shear3(e12: Real) -> Vec<Real> {
    col_major(&[&[0., e12, 0.], &[e12, 0., 0.], &[0., 0., 0.]])
}

/// 3-D, `E = 15`, `ν = 0.25`, `Ev = 5`, `Eta = 10`,
/// `Δt = 0.5`, first pure-shear step `ε12 = ε21 = 1e-3`. Two-spring energy
/// `γ∞G·2ε12² + γ_v/(4G)·2h12²` = 1.1115203132285619e-5 (default); upstream
/// `½σ:ε` = 1.1529987610338383e-5 (the default before the fix).
#[test]
fn mv04_reviewer_case_first_shear_step() {
    let model = one_element(3);
    let t = TimedModel {
        inner: &model,
        dt: 0.5,
    };
    let dat = SLS.replace("{extra}", "");
    for (options, want) in [
        (LawOptions::default(), 1.111_520_313_228_562e-5),
        (LawOptions::upstream(), 1.152_998_761_033_838_3e-5),
    ] {
        let mut h = handler(&model, &dat, options);
        let mut p = Point {
            law: h.law_mut(0).unwrap(),
            ty: simplex(3),
        };
        p.step(&t, &shear3(1e-3), 1);
        let psi = potential_density(&mut p, &t);
        assert!((psi - want).abs() <= 1e-14 * want, "{psi} vs {want}");
        // independent closed form from the history integral
        if options.sls_energy == akantu_solid::materials::sls_deviatoric::SlsEnergy::TwoSpring {
            let (g, gamma_inf, gamma_v) = (6.0_f64, 10. / 15., 5. / 15.);
            let h12 = p.internal("history_integral")[1];
            let closed = gamma_inf * g * 2. * 1e-6 + gamma_v / (4. * g) * 2. * h12 * h12;
            assert!((psi - closed).abs() <= 1e-15 * closed);
        }
    }
}

/// Ramp the shear to `ε12 = 1e-3` over `T = 2`, unload linearly to
/// zero over `T = 2`, hold 2 (3-D). At zero strain the branch still stores
/// `γ_v/(4G)·h:h > 0` (upstream: `½σ:ε = 0`). Energy balance
/// `W − ψ − D` (`W` the trapezoidal work recomputed here, `D` the
/// rectangle-rule dissipation of the law) converges to 0 at first order in
/// `Δt` by default (rectangle-rule dissipation); with upstream's energy it
/// tends to a non-zero limit
/// (the branch energy is missing).
#[test]
fn mv04_unloading_to_zero_strain_and_energy_balance_under_refinement() {
    let model = one_element(3);
    let dat = SLS.replace("{extra}", "");
    let mut errors = [Vec::new(), Vec::new()];
    for (m, options) in [LawOptions::default(), LawOptions::upstream()]
        .into_iter()
        .enumerate()
    {
        for dt in [0.1, 0.05, 0.025, 0.0125] {
            let t = TimedModel { inner: &model, dt };
            let mut h = handler(&model, &dat, options);
            let mut p = Point {
                law: h.law_mut(0).unwrap(),
                ty: simplex(3),
            };
            let n = (2.0 / dt).round() as usize;
            let mut work = 0.0;
            let (mut eps_old, mut s_old) = (vec![0.0; 9], vec![0.0; 9]);
            for k in 1..=3 * n {
                let time = k as Real * dt;
                let e12 = if time <= 2. {
                    1e-3 * time / 2.
                } else if time <= 4. {
                    1e-3 * (4. - time) / 2.
                } else {
                    0.
                };
                let g = shear3(e12);
                p.step(&t, &g, 1);
                let s = p.stress();
                work += 0.5
                    * (0..9)
                        .map(|l| (s[l] + s_old[l]) * (g[l] - eps_old[l]))
                        .sum::<Real>();
                (eps_old, s_old) = (g, s);
            }
            let psi = potential_density(&mut p, &t);
            let diss = p.internal("dissipated_energy")[0];
            let h12 = p.internal("history_integral")[1];
            if m == 0 {
                let branch = (5. / 15.) / (4. * 6.) * 2. * h12 * h12;
                assert!(
                    h12 != 0. && (psi - branch).abs() <= 1e-15 * branch,
                    "{psi} {branch}"
                );
            } else {
                assert_eq!(psi, 0., "upstream ½σ:ε at zero strain");
            }
            errors[m].push(((work - psi - diss) / work).abs());
        }
    }
    eprintln!(
        "mv04 relative balance errors: default {:?}, upstream {:?}",
        errors[0], errors[1]
    );
    // observed 4.6e-3, 1.5e-3, 5.9e-4, 2.5e-4: halving ratios 0.34, 0.38,
    // 0.42 (pre-asymptotic, tending to the 0.5 of the first-order
    // rectangle-rule dissipation; the trapezoidal work is second order)
    for w in errors[0].windows(2) {
        let ratio = w[1] / w[0];
        assert!((0.3..0.6).contains(&ratio), "convergence: {:?}", errors[0]);
    }
    assert!(errors[0][3] < 5e-4);
    // upstream: the missing branch energy does not vanish under refinement
    assert!(
        errors[1][3] > 0.5 * errors[1][0] && errors[1][3] > 1e-2,
        "{:?}",
        errors[1]
    );
}

/// The corrected SLS states and enforces its domain (isothermal small
/// strain): `finite_deformation` and `alpha ≠ 0` are `Error::Unsupported`
/// at `computeStress`; the upstream compatibility mode accepts both (and
/// ignores the temperature, as upstream).
#[test]
fn mv04_sls_domain_is_enforced() {
    let model = one_element(2);
    let t = TimedModel {
        inner: &model,
        dt: 0.5,
    };
    for extra in [
        "  finite_deformation = true\n",
        "  alpha = 1e-5\n  delta_T = 10\n",
    ] {
        let dat = SLS.replace("{extra}", extra);
        for upstream in [false, true] {
            let options = if upstream {
                LawOptions::upstream()
            } else {
                LawOptions::default()
            };
            let mut h = handler(&model, &dat, options);
            let mut p = Point {
                law: h.law_mut(0).unwrap(),
                ty: simplex(2),
            };
            p.law
                .before_solve_step(&t.ctx(LawPhase::BeforeSolveStep))
                .unwrap();
            p.set_grad_u(&[1e-3, 0., 0., 0.]);
            let r = p
                .law
                .compute_stress(&t.ctx(LawPhase::Stress), simplex(2), NG);
            if upstream {
                assert!(r.is_ok(), "{extra}: {r:?}");
            } else {
                assert!(
                    matches!(r, Err(akantu_core::Error::Unsupported(_))),
                    "{extra}: {r:?}"
                );
            }
        }
    }
}

// ---------------------------------------------------------------------------
// `mv07_*`: test coverage (non-identity filter energies, Maxwell trials)
// ---------------------------------------------------------------------------

/// Element 0 → law 0, element 1 → law 1.
#[derive(Debug)]
struct ByElement;

impl akantu_model::constitutive_law::ConstitutiveLawSelector for ByElement {
    fn select(
        &self,
        _ctx: &akantu_model::constitutive_law::SelectorContext<'_>,
        element: akantu_core::Element,
    ) -> akantu_core::Result<Option<usize>> {
        Ok(Some(element.idx()))
    }
}

/// `mv07_*` (SLS-E): two triangles of unequal area (½ and 2), `elastic` on mesh
/// element 0 and `sls_deviatoric` on mesh element 1 (law-local element 0 ↔
/// mesh element 1, a non-identity filter). The per-element dissipated and
/// potential energies of the SLS law use the Jacobian of mesh element 1
/// (density × 2, upstream would use element 0's ½) and agree with the
/// whole-law energies; the potential density is the two-spring energy.
#[test]
fn mv07_sls_energies_with_a_non_identity_filter() {
    use akantu_parser::{AnonymousIds, Parser, ParserType};
    let model = SimplexModel::new(
        2,
        &[
            vec![0., 0.],
            vec![1., 0.],
            vec![0., 1.],
            vec![2., 0.],
            vec![0., 2.],
        ],
        &[vec![0, 1, 2], vec![0, 3, 4]],
    );
    let dat = format!(
        "material elastic [\n  name = steel\n  rho = 1\n  E = 1\n  nu = 0.3\n]\n{}",
        SLS.replace("{extra}", "")
    );
    let parsed =
        Parser::parse_bytes(dat.as_bytes(), "material.dat", AnonymousIds::Local(&mut 0)).unwrap();
    let mut h = Handler::new(
        "solid_mechanics_model",
        &model.mesh,
        2,
        ParserType::Material,
    )
    .unwrap();
    h.instantiate_laws(&common::factory(), &parsed.root, &model, false)
        .unwrap();
    h.selector = akantu_model::constitutive_law::SelectorChain {
        selectors: vec![Box::new(ByElement)],
        fallback_value: 0,
    };
    h.assign_laws_to_elements(&model.mesh, None).unwrap();
    let ctx = model.ctx(LawPhase::Init);
    for law in h.laws_mut() {
        law.init_material(&ctx).unwrap();
    }
    let tri = simplex(2);
    let law = h.law_mut(1).unwrap();
    assert_eq!(law.element_filter().get(tri, NG).unwrap().as_slice(), &[1]);
    let t = TimedModel {
        inner: &model,
        dt: 0.5,
    };
    let mut p = Point { law, ty: tri };
    for s in [0.5, 1., 1., 0.] {
        p.step(
            &t,
            &col_major(&[&[2e-4 * s, 1e-3 * s], &[1e-3 * s, -1e-4 * s]]),
            1,
        );
    }
    let area = 2.0; // |mesh element 1|, one quadrature point
    let diss_density = p.internal("dissipated_energy")[0];
    let psi_density = potential_density(&mut p, &t);
    assert!(diss_density > 0. && psi_density > 0.);
    let elem = akantu_core::Element::new(tri, 0, NG);
    let e_ctx = t.ctx(LawPhase::Energy);
    let diss_el = p.law.energy_of_element(&e_ctx, "dissipated", elem).unwrap();
    let pot_el = p.law.energy_of_element(&e_ctx, "potential", elem).unwrap();
    assert!(
        (diss_el - area * diss_density).abs() <= 1e-15 * diss_el,
        "{diss_el}"
    );
    assert!(
        (pot_el - area * psi_density).abs() <= 1e-15 * pot_el,
        "{pot_el}"
    );
    let diss_all = p.law.energy_of(&e_ctx, "dissipated").unwrap();
    let pot_all = p.law.energy_of(&e_ctx, "potential").unwrap();
    assert_eq!(diss_all.to_bits(), diss_el.to_bits());
    assert_eq!(pot_all.to_bits(), pot_el.to_bits());
    // zero total strain: only the branch spring stores energy, with the 2-D
    // branch compliance `h:B⁻¹:h = h:h + (tr h)²` (the
    // earlier version of this check used the retained-block norm `h:h`)
    let h_int = p.internal("history_integral");
    let tr = h_int[0] + h_int[3];
    let branch = (5. / 15.) / (4. * 6.) * (h_int.iter().map(|x| x * x).sum::<Real>() + tr * tr);
    let strain_energy = psi_density - branch;
    assert!(
        strain_energy.abs() <= 1e-15 * psi_density,
        "{strain_energy}"
    );
}

const MAXWELL2: &str = "material viscoelastic_maxwell [\n  name = m\n  rho = 1\n  Einf = 15e6\n  nu = 0.25\n  Eta = [40e6, 2e6]\n  Ev = [10e6, 5e6]\n]\n";

/// Snapshot of the Maxwell state after a step (stress, branch stresses and
/// strains, energies) for bitwise comparisons.
fn maxwell_state(p: &Point<'_>) -> Vec<u64> {
    let mut v = p.stress();
    for name in [
        "sigma_v",
        "epsilon_v",
        "dissipated_energy",
        "mechanical_work",
    ] {
        v.extend(p.internal(name));
    }
    v.into_iter().map(Real::to_bits).collect()
}

/// Maxwell trials, cf. UD-063: repeated residual evaluations in a
/// step, and a failed step (`afterSolveStep(false)`) followed by a retry of
/// the same step, give bit-identical states to a single evaluation of each
/// step (trial evaluations do not mutate the branch state; the failed step
/// rolls back).
#[test]
fn mv07_maxwell_repeated_trials_and_failed_step_retry() {
    for dim in [2, 3] {
        let model = one_element(dim);
        let t = TimedModel {
            inner: &model,
            dt: 0.5,
        };
        let path: Vec<Vec<Real>> = [1e-3, 2e-3, 2e-3, 5e-4]
            .iter()
            .map(|s| {
                (0..dim * dim)
                    .map(|k| s * [1., 0.3, 0.3, -0.5, 0.2, 0.1, 0.2, 0.1, 0.4][k])
                    .collect()
            })
            .collect();
        let run = |mode: u8| {
            let mut h = handler(&model, MAXWELL2, LawOptions::default());
            let mut p = Point {
                law: h.law_mut(0).unwrap(),
                ty: simplex(dim),
            };
            let mut states = Vec::new();
            for g in &path {
                match mode {
                    0 => p.step(&t, g, 1),
                    1 => p.step(&t, g, 4),
                    _ => {
                        // a failed attempt at a different strain, then retry
                        p.law
                            .before_solve_step(&t.ctx(LawPhase::BeforeSolveStep))
                            .unwrap();
                        let wrong: Vec<Real> = g.iter().map(|x| 3. * x + 1e-3).collect();
                        p.set_grad_u(&wrong);
                        p.law
                            .compute_stress(&t.ctx(LawPhase::Stress), p.ty, NG)
                            .unwrap();
                        p.law
                            .after_solve_step(&t.ctx(LawPhase::AfterSolveStep), false)
                            .unwrap();
                        p.step(&t, g, 2);
                    }
                }
                states.push(maxwell_state(&p));
            }
            states
        };
        let reference = run(0);
        assert_eq!(run(1), reference, "{dim}-D: repeated evaluations");
        assert_eq!(run(2), reference, "{dim}-D: failed step and retry");
    }
}
