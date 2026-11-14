// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Regression tests of
// the inelastic materials (src/model/solid_mechanics/materials/
// material_damage/material_{marigo,mazars}*, material_plastic/*,
// material_finite_deformation/material_neohookean*).

//! One test per defect, named `miNN_*`. Each test checks the corrected
//! default **and** that the upstream-compat option (`LawOptions::upstream()`
//! or the single switch) still shows the defect: the behaviour of the
//! version before the fix, whose defaults were upstream's for every switch
//! involved (so every default assertion below fails there).

mod common;
mod inelastic_common;

use akantu_core::Real;
use akantu_core::tensor::Matrix;
use akantu_model::constitutive_law::LawPhase;
use akantu_solid::material::{
    DruckerPragerReturn, FiniteStrainDamage, J2ReducedReturn, LawOptions,
};
use akantu_solid::materials::drucker_prager::{
    DpConstants, dp_closest_point_projection, dp_return_closed_form, dp_stress_on_quad,
};
use akantu_solid::materials::elastic::ElasticConstants;
use akantu_validation::reduce::NanMinMax;
use inelastic_common::{Point, max_abs, max_abs_diff};

const PLASTIC: &str = "material plastic_linear_isotropic_hardening [\n  name = plastic\n  rho = 7800\n  \
                       E = 2.1e11\n  nu = 0.3\n  sigma_y = 3e8\n  h = 1e9\n{extra}]\n";
const MAZARS: &str = "material mazars [\n  name = concrete\n  rho = 3000\n  E = 25e9\n  nu = 0.2\n  \
                      K0 = 1e-4\n  At = 1.0\n  Bt = 5e3\n  Ac = 0.8\n  Bc = 1391.3\n  beta = 1.\n{extra}]\n";
const MARIGO: &str = "material marigo [\n  name = marigo\n  rho = 3000\n  E = 25e9\n  nu = 0.2\n  \
                      Sd = 5e3\n  Yd = 50\n{extra}]\n";
const PLASTIC_MAZARS: &str = "material plastic_mazars [\n  name = pm\n  rho = 3000\n  E = 25e9\n  \
                              nu = 0.2\n  sigma_y = 1e12\n  h = 1e9\n  K0 = 1e-4\n  At = 1.0\n  Bt = 5e3\n  \
                              Ac = 0.8\n  Bc = 1391.3\n  beta = 1.\n{extra}]\n";
const DP: &str = "material plastic_drucker_prager [\n  name = dp\n  rho = 3000\n  E = 25e9\n  nu = 0.2\n  \
                  phi = 30\n  fc = 1e7\n{extra}]\n";

fn dat(template: &str, extra: &str) -> String {
    template.replace("{extra}", extra)
}

/// One solve step with several trial evaluations (Newton iterates / line
/// search) before convergence.
fn trial_step(p: &mut Point, trials: &[&[Real]]) {
    {
        let model = &p.model;
        let law = p.handler.law_mut(0).unwrap();
        law.before_solve_step(&model.ctx(LawPhase::BeforeSolveStep))
            .unwrap();
    }
    for t in trials {
        p.eval(t).unwrap();
    }
    let model = &p.model;
    let law = p.handler.law_mut(0).unwrap();
    law.after_solve_step(&model.ctx(LawPhase::AfterSolveStep), true)
        .unwrap();
}

/// Von Mises equivalent stress of a row-major `dim × dim` stress with
/// `σ_i3 = 0`.
fn von_mises(s: &[Real]) -> Real {
    let dim = (s.len() as f64).sqrt() as usize;
    let mut m = [[0.; 3]; 3];
    for i in 0..dim {
        for j in 0..dim {
            m[i][j] = s[i * dim + j];
        }
    }
    let tr = m[0][0] + m[1][1] + m[2][2];
    let mut ss = 0.;
    for (i, row) in m.iter().enumerate() {
        for (j, v) in row.iter().enumerate() {
            let d = v - if i == j { tr / 3. } else { 0. };
            ss += d * d;
        }
    }
    (1.5 * ss).sqrt()
}

// ---------------------------------------------------------------- ---

/// UD-137: 1-D J2, one step to `ε = 0.005` (`E = 2.1e11`, `σ_y =
/// 3e8`, `h = 1e9`): the default returns onto the yield surface
/// (`Δp = (Eε − σ_y)/(E + h)`); upstream returns 551582278.48 Pa with the
/// yield radius `σ_y + r` = 302373417.72 Pa.
#[test]
fn mi01_j2_1d_return_is_on_the_yield_surface() {
    let (e, sy, h) = (2.1e11, 3e8, 1e9);
    let mut p = Point::new(1, &dat(PLASTIC, ""), LawOptions::default());
    p.step(&[0.005]).unwrap();
    let s = p.stress()[0];
    let r = p.scalar("iso_hardening");
    let dp = (e * 0.005 - sy) / (e + h);
    assert!((r - h * dp).abs() <= 1e-12 * r);
    assert!((s - (sy + r)).abs() <= 1e-12 * s, "{s} vs {}", sy + r);
    assert!((p.internal("inelastic_strain")[0] - dp).abs() <= 1e-15);
    let mut u = Point::new(1, &dat(PLASTIC, ""), LawOptions::upstream());
    u.step(&[0.005]).unwrap();
    let (su, ru) = (u.stress()[0], u.scalar("iso_hardening"));
    assert!((su - 551_582_278.481_012_7).abs() <= 1e-6, "{su}");
    assert!((ru - 2_373_417.721_518_987_3).abs() <= 1e-6, "{ru}");
    assert!(
        su - (sy + ru) > 2.4e8,
        "upstream: far outside the yield radius"
    );
}

/// UD-137: 2-D plane stress, `ν = 0.3`, `H = diag(0.005, 0)`: the
/// default (Simo–Taylor projection) ends on the von Mises surface of the
/// `σ33 = 0` state with a normal flow; upstream's equivalent stress is
/// 470669505.23 Pa for the radius 302982071.33 Pa. Checked along a
/// non-proportional cyclic history too.
#[test]
fn mi01_j2_plane_stress_return_is_on_the_yield_surface() {
    let ps = "  Plane_Stress = true\n";
    let sy = 3e8;
    let mut p = Point::new(2, &dat(PLASTIC, ps), LawOptions::default());
    p.step(&[0.005, 0., 0., 0.]).unwrap();
    let s = p.stress();
    let r = p.scalar("iso_hardening");
    assert!((von_mises(&s) - sy - r).abs() <= 1e-9 * sy, "{s:?} r {r}");
    let mut u = Point::new(2, &dat(PLASTIC, ps), LawOptions::upstream());
    u.step(&[0.005, 0., 0., 0.]).unwrap();
    let su = u.stress();
    assert!((su[0] - 542_187_005.235_031_1).abs() <= 1e-5);
    assert!((su[3] - 303_569_981.377_122_46).abs() <= 1e-5);
    assert!((von_mises(&su) - 470_669_505.233_685_14).abs() <= 1e-5);

    // non-proportional cyclic path: yield residual and normality of every
    // plastic increment (Δε_p ∝ deviator of the returned stress)
    let mut p = Point::new(2, &dat(PLASTIC, ps), LawOptions::default());
    let mut ep_prev = p.internal("inelastic_strain");
    let mut plastic = 0;
    for k in 0..60 {
        let t = f64::from(k) / 10.;
        let g = [
            4e-3 * t.sin(),
            3e-3 * t.cos(),
            2e-3 * (0.7 * t).sin(),
            -1e-3 * t.cos(),
        ];
        p.step(&g).unwrap();
        let ep = p.internal("inelastic_strain");
        let dep: Vec<Real> = ep.iter().zip(&ep_prev).map(|(a, b)| a - b).collect();
        if max_abs(&dep) > 0. {
            plastic += 1;
            let s = p.stress();
            let r = p.scalar("iso_hardening");
            assert!((von_mises(&s) - sy - r).abs() <= 1e-9 * sy, "step {k}");
            // deviator with σ33 = 0 (column-major internal, symmetric)
            let tr = s[0] + s[3];
            let dev = [s[0] - tr / 3., s[1], s[2], s[3] - tr / 3.];
            let ratio = dep[0] / dev[0];
            assert!(ratio > 0.);
            for q in 0..4 {
                assert!(
                    (dep[q] - ratio * dev[q]).abs() <= 1e-9 * max_abs(&dep),
                    "step {k}"
                );
            }
        }
        ep_prev = ep;
    }
    assert!(plastic > 10);
}

/// UD-137: 2-D plane strain equals the accepted 3-D return driven by
/// the embedded gradient (`ε_i3 = 0`), including the out-of-plane stress
/// reconstructed from the committed state, along a non-proportional cyclic
/// history; upstream's 2-D return (deviator with `σ33 = 0`) does not.
#[test]
fn mi01_j2_plane_strain_matches_the_3d_return() {
    let mut p2 = Point::new(2, &dat(PLASTIC, ""), LawOptions::default());
    let mut u2 = Point::new(2, &dat(PLASTIC, ""), LawOptions::upstream());
    let mut p3 = Point::new(3, &dat(PLASTIC, ""), LawOptions::default());
    let (lambda, mu) = {
        let (e, nu): (Real, Real) = (2.1e11, 0.3);
        (nu * e / ((1. + nu) * (1. - 2. * nu)), e / (2. * (1. + nu)))
    };
    let mut worst_upstream: Real = 0.;
    for k in 0..60 {
        let t = f64::from(k) / 8.;
        let g2 = [
            5e-3 * t.sin(),
            2e-3 * t.cos(),
            1e-3 * (0.3 * t).sin(),
            -3e-3 * (0.5 * t).cos(),
        ];
        let g3 = [g2[0], g2[1], 0., g2[2], g2[3], 0., 0., 0., 0.];
        p2.step(&g2).unwrap();
        u2.step(&g2).unwrap();
        p3.step(&g3).unwrap();
        let (s2, s3) = (p2.stress(), p3.stress());
        let s3_in = [s3[0], s3[1], s3[3], s3[4]];
        let scale = max_abs(&s3);
        assert!(
            max_abs_diff(&s2, &s3_in) <= 1e-9 * scale,
            "step {k}: {s2:?} {s3:?}"
        );
        let r2 = p2.scalar("iso_hardening");
        assert!((r2 - p3.scalar("iso_hardening")).abs() <= 1e-9 * scale);
        // σ33 = λ tr ε + 2μ tr ε_p (isochoric flow)
        let ep = p2.internal("inelastic_strain");
        let s33 = lambda * (g2[0] + g2[3]) + 2. * mu * (ep[0] + ep[3]);
        assert!(
            (s33 - s3[8]).abs() <= 1e-8 * scale,
            "step {k}: σ33 {s33} vs {}",
            s3[8]
        );
        worst_upstream = worst_upstream.nan_max(max_abs_diff(&u2.stress(), &s3_in) / scale);
    }
    assert!(
        worst_upstream > 1e-2,
        "upstream plane strain differs from 3-D"
    );
}

// ---------------------------------------------------------------- ---

/// UD-138: trial evaluations within one solve step do not influence
/// later trials. Scalar case (`E = 25e9`, `σ_y = 3e8`, `h = 1e11`,
/// 1-D): trials 0.1, 0.015, 0.015. Default: every 0.015 trial equals the
/// direct evaluation from the committed state; upstream returns
/// 375000000 Pa (elastic: the high trial's hardening enters the yield test)
/// and then 361363636.36 Pa.
#[test]
fn mi03_j2_trials_are_reconstructed_from_the_committed_state() {
    let d = "material plastic_linear_isotropic_hardening [\n  name = p\n  rho = 1\n  E = 25e9\n  \
             nu = 0.2\n  sigma_y = 3e8\n  h = 1e11\n]\n";
    let mut direct = Point::new(1, d, LawOptions::default());
    direct.step(&[0.015]).unwrap();
    let want = direct.stress()[0];
    let mut p = Point::new(1, d, LawOptions::default());
    for trial in [&[0.1][..], &[0.015], &[0.015]] {
        {
            let model = &p.model;
            let law = p.handler.law_mut(0).unwrap();
            if trial[0] == 0.1 {
                law.before_solve_step(&model.ctx(LawPhase::BeforeSolveStep))
                    .unwrap();
            }
        }
        p.eval(trial).unwrap();
        if trial[0] == 0.015 {
            assert_eq!(p.stress()[0].to_bits(), want.to_bits());
        }
    }
    let mut u = Point::new(1, d, LawOptions::upstream());
    {
        let model = &u.model;
        let law = u.handler.law_mut(0).unwrap();
        law.before_solve_step(&model.ctx(LawPhase::BeforeSolveStep))
            .unwrap();
    }
    u.eval(&[0.1]).unwrap();
    u.eval(&[0.015]).unwrap();
    assert_eq!(u.stress()[0], 375_000_000.);
    u.eval(&[0.015]).unwrap();
    assert!((u.stress()[0] - 361_363_636.363_636_4).abs() <= 1e-6);

    // 2-D and 3-D: a Newton-like sequence then the converged state equals a
    // single evaluation from the same committed history (bitwise)
    for dim in [2usize, 3] {
        let base: Vec<Real> = (0..dim * dim)
            .map(|q| if q % (dim + 1) == 0 { 1. } else { 0.3 })
            .collect();
        let at = |x: Real| -> Vec<Real> { base.iter().map(|b| b * x).collect() };
        let mut a = Point::new(dim, &dat(PLASTIC, ""), LawOptions::default());
        let mut b = Point::new(dim, &dat(PLASTIC, ""), LawOptions::default());
        for x in [2e-3, 4e-3] {
            a.step(&at(x)).unwrap();
            b.step(&at(x)).unwrap();
        }
        trial_step(&mut a, &[&at(2e-2), &at(1e-3), &at(5e-3), &at(5e-3)]);
        b.step(&at(5e-3)).unwrap();
        assert_eq!(a.stress(), b.stress(), "{dim}D");
        assert_eq!(
            a.internal("inelastic_strain"),
            b.internal("inelastic_strain")
        );
        assert_eq!(a.scalar("iso_hardening"), b.scalar("iso_hardening"));
    }
}

/// UD-138: Mazars and Marigo, 1-D trials 0.0005 then
/// 0.00001 in one step. Default: the low trial is the virgin answer
/// 250000 Pa (`d = 0`); upstream keeps the high trial's damage
/// (Mazars `d = 0.8646647167633873`, 33833.82 Pa; Marigo `d = 0.615`,
/// 96250 Pa). A rejected step restores the committed damage.
#[test]
fn mi03_damage_trials_do_not_keep_discarded_damage() {
    for (template, d_up, s_up) in [
        (MAZARS, 0.864_664_716_763_387_3, 33_833.820_809_153_18),
        (MARIGO, 0.615, 96_250.),
    ] {
        let mut p = Point::new(1, &dat(template, ""), LawOptions::default());
        trial_step(&mut p, &[&[5e-4], &[1e-5]]);
        assert_eq!(p.scalar("damage"), 0.);
        assert!((p.stress()[0] - 250_000.).abs() <= 1e-9);
        let mut u = Point::new(1, &dat(template, ""), LawOptions::upstream());
        trial_step(&mut u, &[&[5e-4], &[1e-5]]);
        assert!(
            (u.scalar("damage") - d_up).abs() <= 1e-15,
            "{}",
            u.scalar("damage")
        );
        assert!((u.stress()[0] - s_up).abs() <= 1e-6, "{}", u.stress()[0]);
    }
    // rejected step (afterSolveStep(false)) restores the committed damage
    let mut p = Point::new(1, &dat(MAZARS, ""), LawOptions::default());
    p.step(&[2e-4]).unwrap();
    let d0 = p.scalar("damage");
    {
        let model = &p.model;
        let law = p.handler.law_mut(0).unwrap();
        law.before_solve_step(&model.ctx(LawPhase::BeforeSolveStep))
            .unwrap();
    }
    p.eval(&[5e-3]).unwrap();
    assert!(p.scalar("damage") > d0);
    {
        let model = &p.model;
        let law = p.handler.law_mut(0).unwrap();
        law.after_solve_step(&model.ctx(LawPhase::AfterSolveStep), false)
            .unwrap();
    }
    assert_eq!(p.scalar("damage"), d0);
}

/// UD-138: Marigo's `damage_in_y` uses the committed damage, so a
/// repeated trial gives the same answer (upstream reads the current trial
/// damage and its answer drifts from one evaluation to the next).
#[test]
fn mi03_marigo_damage_in_y_uses_the_committed_damage() {
    let extra = "  damage_in_y = true\n";
    let mut p = Point::new(1, &dat(MARIGO, extra), LawOptions::default());
    p.step(&[2e-4]).unwrap();
    trial_step(&mut p, &[&[4e-4], &[4e-4], &[4e-4]]);
    let a = (p.scalar("damage"), p.stress()[0]);
    let mut q = Point::new(1, &dat(MARIGO, extra), LawOptions::default());
    q.step(&[2e-4]).unwrap();
    q.step(&[4e-4]).unwrap();
    assert_eq!(a, (q.scalar("damage"), q.stress()[0]));
    let mut u = Point::new(1, &dat(MARIGO, extra), LawOptions::upstream());
    u.step(&[2e-4]).unwrap();
    trial_step(&mut u, &[&[4e-4], &[4e-4], &[4e-4]]);
    let mut v = Point::new(1, &dat(MARIGO, extra), LawOptions::upstream());
    v.step(&[2e-4]).unwrap();
    v.step(&[4e-4]).unwrap();
    assert_ne!(u.scalar("damage"), v.scalar("damage"));
}

// ---------------------------------------------------------------- ---

/// UD-139: `plastic_mazars` with inactive plasticity held at a fixed
/// strain 0.0005 (1-D): the default stress stays
/// `(1 − d)Eε` = 1691691.04 Pa at constant damage; upstream decays
/// 1.692 → 0.229 → 0.031 → 0.004 MPa.
#[test]
fn mi04_plastic_mazars_stress_does_not_decay_at_fixed_strain() {
    let mut p = Point::new(1, &dat(PLASTIC_MAZARS, ""), LawOptions::default());
    let mut u = Point::new(1, &dat(PLASTIC_MAZARS, ""), LawOptions::upstream());
    let want_up = [
        1_691_691.040_457_658_7,
        228_945.486_109_177_3,
        30_984.402_208_329_488,
        4_193.282_848_781_399_5,
    ];
    for w in want_up {
        p.step(&[5e-4]).unwrap();
        u.step(&[5e-4]).unwrap();
        assert!((p.scalar("damage") - 0.864_664_716_763_387_3).abs() <= 1e-15);
        assert_eq!(p.internal("inelastic_strain")[0], 0.);
        assert!((p.stress()[0] - 1_691_691.040_457_658_7).abs() <= 1e-6);
        assert!((u.stress()[0] - w).abs() <= 1e-6 * w, "{}", u.stress()[0]);
    }
    // unloading / reloading at constant damage: secant elastic
    let d = p.scalar("damage");
    for e in [2e-4, 0., 3e-4, 5e-4] {
        p.step(&[e]).unwrap();
        assert!((p.stress()[0] - (1. - d) * 25e9 * e).abs() <= 1e-6);
    }
}

/// UD-139: with active plasticity the effective stress of the
/// default `plastic_mazars` is the stress of the J2 law on the same history
/// and the stored stress is `(1 − d)·σ_eff`, through complete damage
/// (`d = 1`: zero stress, finite state, no division by `1 − d`).
#[test]
fn mi04_plastic_mazars_effective_stress_is_the_j2_stress() {
    let pm = dat(PLASTIC_MAZARS, "")
        .replace("sigma_y = 1e12", "sigma_y = 2e7")
        .replace("K0 = 1e-4", "K0 = 3e-4");
    let j2 = "material plastic_linear_isotropic_hardening [\n  name = p\n  rho = 3000\n  E = 25e9\n  \
              nu = 0.2\n  sigma_y = 2e7\n  h = 1e9\n]\n";
    let mut p = Point::new(3, &pm, LawOptions::default());
    let mut q = Point::new(3, j2, LawOptions::default());
    let base = [1., 0.3, -0.2, 0.3, -0.4, 0.1, -0.2, 0.1, 0.2];
    let mut saturated = false;
    // a cyclic plastic-damage history, then hydrostatic tension large enough
    // for `d = 1` exactly (`α_t = 1`, `exp(−Bt(ε̂ − K0))` underflows), then
    // further loading at complete damage
    let mut path: Vec<[Real; 9]> = (0..40)
        .map(|k| {
            let x = 4e-3 * (f64::from(k) / 6.).sin() + 1e-4 * f64::from(k);
            base.map(|b| b * x)
        })
        .collect();
    for x in [0.3, 0.35, 0.2] {
        path.push([x, 0., 0., 0., x, 0., 0., 0., x]);
    }
    for (k, g) in path.into_iter().enumerate() {
        p.step(&g).unwrap();
        q.step(&g).unwrap();
        let eff = p.internal("effective_stress");
        let s_j2 = q.stress();
        let eff_rows = inelastic_common::rows(&eff, 3);
        // not bitwise: plastic_mazars stores the gradient as upstream's
        // `(∇u − ε_p) + ε_p` round trip, so its next increment Δ∇u differs
        // from the J2 law's by a few ulps of the gradient (relative 1e-16);
        // the stress differs by that increment times E: ≤ 1e-12 relative
        let scale = max_abs(&s_j2).max(1.);
        assert!(max_abs_diff(&eff_rows, &s_j2) <= 1e-12 * scale, "step {k}");
        let d = p.scalar("damage");
        let want: Vec<Real> = eff_rows.iter().map(|s| s * (1. - d)).collect();
        assert_eq!(p.stress(), want, "step {k}");
        saturated |= d == 1.;
        assert!(p.stress().iter().all(|v| v.is_finite()));
    }
    assert!(saturated, "the history must reach complete damage");
}

// ---------------------------------------------------------------- ---

/// UD-140: free thermal expansion does not damage Mazars
/// (`α = 1.2e-5`, `ΔT = ±50`, `∇u = αΔT·I`; 1-D, 2-D plane stress, 3-D);
/// upstream's total-strain driver gives `d = 0.9908696607076611` (3-D
/// heating) at zero stress.
#[test]
fn mi05_mazars_free_heating_does_not_damage() {
    for dt in [50., -50.] {
        let th = format!("  alpha = 1.2e-5\n  delta_T = {dt}\n");
        let free = 1.2e-5 * dt;
        for (dim, extra) in [(1usize, ""), (2, "  Plane_Stress = true\n"), (3, "")] {
            let g: Vec<Real> = (0..dim * dim)
                .map(|q| if q % (dim + 1) == 0 { free } else { 0. })
                .collect();
            for template in [MAZARS, PLASTIC_MAZARS] {
                let d = dat(template, &format!("{th}{extra}"));
                let mut p = Point::new(dim, &d, LawOptions::default());
                p.step(&g).unwrap();
                assert_eq!(p.scalar("damage"), 0., "{dim}D ΔT {dt}");
                assert!(max_abs(&p.stress()) <= 1e-3, "{:?}", p.stress());
            }
        }
    }
    let th = "  alpha = 1.2e-5\n  delta_T = 50\n";
    let g = [6e-4, 0., 0., 0., 6e-4, 0., 0., 0., 6e-4];
    let o = LawOptions {
        mazars_mechanical_strain: false,
        ..LawOptions::default()
    };
    let mut u = Point::new(3, &dat(MAZARS, th), o);
    u.step(&g).unwrap();
    assert!((u.scalar("damage") - 0.990_869_660_707_661_1).abs() <= 1e-15);
}

// ---------------------------------------------------------------- ---

/// UD-141: Mazars in 2-D plane stress damages like the 3-D law on
/// the represented strain (`ε33 = −ν/(1 − ν)(ε11 + ε22)`): biaxial
/// compression `H = diag(−0.001, −0.001)` (`ε33 = 0.0005 > K0`) and a mixed
/// tension/compression state; upstream (`ε33 = 0`, plane-stress `λ`)
/// predicts no damage in biaxial compression.
#[test]
fn mi06_mazars_plane_stress_embeds_the_out_of_plane_strain() {
    let ps = "  Plane_Stress = true\n";
    for (e11, e22) in [(-1e-3, -1e-3), (2e-4, -1.2e-3)] {
        let e33 = -0.25 * (e11 + e22);
        let mut p2 = Point::new(2, &dat(MAZARS, ps), LawOptions::default());
        p2.step(&[e11, 0., 0., e22]).unwrap();
        let mut p3 = Point::new(3, &dat(MAZARS, ""), LawOptions::default());
        p3.step(&[e11, 0., 0., 0., e22, 0., 0., 0., e33]).unwrap();
        assert!(p3.scalar("damage") > 0.);
        assert_eq!(p2.scalar("damage"), p3.scalar("damage"), "({e11}, {e22})");
        assert_eq!(p2.scalar("Ehat"), p3.scalar("Ehat"));
    }
    let mut u = Point::new(2, &dat(MAZARS, ps), LawOptions::upstream());
    u.step(&[-1e-3, 0., 0., -1e-3]).unwrap();
    assert_eq!(u.scalar("damage"), 0.);
}

// ---------------------------------------------------------------- ---

/// UD-142: finite-deformation damage is rejected by default; the
/// legacy option reproduces upstream's raw-gradient law, which is not frame
/// indifferent (0.1 rad rigid rotation: `S11 = S22 = −173466483.40` Pa with
/// damage suppressed).
#[test]
fn mi07_finite_deformation_damage_is_rejected_by_default() {
    let (c, s) = (0.1_f64.cos(), 0.1_f64.sin());
    let h = [c - 1., -s, s, c - 1.];
    for (template, extra) in [
        (MARIGO, "  Yd = 1e20\n"),
        (MAZARS, "  K0 = 100\n"),
        (PLASTIC_MAZARS, "  K0 = 100\n"),
    ] {
        let d = dat(template, &format!("  finite_deformation = true\n{extra}"))
            .replace("  Yd = 50\n", "")
            .replace("  K0 = 1e-4\n", "");
        let mut p = Point::new(2, &d, LawOptions::default());
        let e = p.step(&h).unwrap_err();
        assert!(matches!(e, akantu_core::Error::Unsupported(_)), "{e}");
        if template == PLASTIC_MAZARS {
            continue;
        }
        let o = LawOptions {
            finite_strain_damage: FiniteStrainDamage::UpstreamLegacy,
            ..LawOptions::default()
        };
        let mut u = Point::new(2, &d, o);
        u.step(&h).unwrap();
        let st = u.stress();
        assert!((st[0] + 173_466_483.401_881_25).abs() <= 1e-4, "{st:?}");
        assert!((st[3] + 173_466_483.401_881_25).abs() <= 1e-4);
    }
}

// ---------------------------------------------------------------- ---

fn dp_constants<const D: usize>(radial_return: bool) -> DpConstants<D> {
    let (e, nu): (Real, Real) = (25e9, 0.2);
    let lambda = nu * e / ((1. + nu) * (1. - 2. * nu));
    DpConstants {
        elastic: ElasticConstants {
            e,
            lambda,
            mu: e / (2. * (1. + nu)),
        },
        alpha: 1.2,
        k: 6e6,
        radial_return,
    }
}

/// 3-D isotropic Voigt elastic tangent (engineering shears).
fn de3(c: &DpConstants<3>) -> Matrix<6, 6> {
    let (l, m) = (c.elastic.lambda, c.elastic.mu);
    std::array::from_fn(|j| {
        std::array::from_fn(|i| {
            if i < 3 && j < 3 {
                l + if i == j { 2. * m } else { 0. }
            } else if i == j {
                m
            } else {
                0.
            }
        })
    })
}

fn dp_f<const D: usize>(c: &DpConstants<D>, s: &Matrix<D, D>) -> Real {
    #[allow(clippy::cast_precision_loss)]
    let dd = D as Real;
    let tr: Real = (0..D).map(|i| s[i][i]).sum();
    let mut j2 = 0.;
    for (j, col) in s.iter().enumerate() {
        for (i, v) in col.iter().enumerate() {
            let d = v - if i == j { tr / dd } else { 0. };
            j2 += 0.5 * d * d;
        }
    }
    (3. * j2).sqrt() + c.alpha * tr - c.k
}

/// UD-144: the singular-deviator trial `diag(3e7, 1e7,
/// 2e7)` (deviator `diag(1e7, −1e7, 0)`, singular) — upstream's kernel
/// returned `Ok` with six NaN plastic strains before the fix; the compat
/// kernel now fails with `Error::Numerical`, and the default returns onto
/// the cone.
#[test]
fn mi02_dp_singular_deviator_is_an_error_not_nan() {
    let c = dp_constants::<3>(false);
    let mut st = Matrix::<3, 3>::default();
    st[0][0] = 3e7;
    st[1][1] = 1e7;
    st[2][2] = 2e7;
    let e = dp_closest_point_projection::<3, 6>(&c, &de3(&c), &st).unwrap_err();
    assert!(matches!(e, akantu_core::Error::Numerical(_)), "{e}");
    // the closest point is on the cone (`q_tr − 3μΔλ = 5 MPa ≥ 0`), not the
    // apex upstream heads for (`tr σ ≥ k/α`): σ = σ_tr − C:Δε_p is on the
    // surface with the trial's deviator direction
    let dep = dp_return_closed_form(&c, &st).unwrap();
    let (l, m) = (c.elastic.lambda, c.elastic.mu);
    let tr: Real = (0..3).map(|i| dep[i][i]).sum();
    let s: Matrix<3, 3> = std::array::from_fn(|j| {
        std::array::from_fn(|i| st[j][i] - 2. * m * dep[j][i] - if i == j { l * tr } else { 0. })
    });
    assert!(dp_f(&c, &s).abs() <= 1e-8, "f = {}", dp_f(&c, &s));
    let mean = (s[0][0] + s[1][1] + s[2][2]) / 3.;
    let dev = [s[0][0] - mean, s[1][1] - mean, s[2][2] - mean];
    let ratio = dev[0] / 1e7;
    assert!(ratio > 0. && ratio < 1.);
    assert!(
        (dev[1] + ratio * 1e7).abs() <= 1e-6 && dev[2].abs() <= 1e-6,
        "{dev:?}"
    );
    for (j, col) in s.iter().enumerate() {
        for (i, v) in col.iter().enumerate() {
            if i != j {
                assert_eq!(*v, 0.);
            }
        }
    }
}

/// UD-145: hydrostatic tension `H = 0.002·I` (3-D, `φ = 30°`,
/// `fc = 1e7`, no radial return): the default stress is the apex
/// `k/(3α)` = 1.6666667 MPa per diagonal; upstream's trace correction
/// overshoots to −161666666.67 Pa.
#[test]
fn mi02_dp_hydrostatic_tension_returns_to_the_apex() {
    let d = dat(DP, "  radial_return = false\n");
    let g = [2e-3, 0., 0., 0., 2e-3, 0., 0., 0., 2e-3];
    let mut p = Point::new(3, &d, LawOptions::default());
    p.step(&g).unwrap();
    let apex = 6e6 / (3. * 1.2);
    for (q, s) in p.stress().iter().enumerate() {
        let want = if q % 4 == 0 { apex } else { 0. };
        assert!((s - want).abs() <= 1e-7 * apex, "{q}: {s}");
    }
    let mut u = Point::new(3, &d, LawOptions::upstream());
    u.step(&g).unwrap();
    assert!((u.stress()[0] + 161_666_666.666_666_7).abs() <= 1e-3 * 1.6e8);
}

/// UD-146: the closed-form default is the closest-point projection:
/// on random yielding trials (plane strain, plane stress, 3-D; associated
/// and radial), the returned stress is on the yield surface; the associated
/// return satisfies the variational inequality `(σ_tr − σ):C⁻¹:(τ − σ) ≤ 0`
/// for admissible `τ` (here: trial-scaled points of the surface/interior
/// and the apex); the radial return keeps the pressure and the deviator
/// direction.
#[test]
fn mi02_dp_closed_form_is_the_closest_point_projection() {
    #[allow(clippy::needless_range_loop)] // symmetric (i, j) fills and contractions
    fn check<const D: usize, const S: usize>(plane_stress: bool) {
        let mut rng = inelastic_common::Rng(11);
        for radial in [false, true] {
            let mut c = dp_constants::<D>(radial);
            if plane_stress {
                let (e, nu): (Real, Real) = (25e9, 0.2);
                c.elastic.lambda = nu * e / ((1. + nu) * (1. - nu));
            }
            let (l, m) = (c.elastic.lambda, c.elastic.mu);
            let ce = |x: &Matrix<D, D>| -> Matrix<D, D> {
                // C : x (D-dim isotropic)
                let tr: Real = (0..D).map(|i| x[i][i]).sum();
                std::array::from_fn(|j| {
                    std::array::from_fn(|i| 2. * m * x[j][i] + if i == j { l * tr } else { 0. })
                })
            };
            let mut tested = 0;
            for _ in 0..400 {
                let mut st: Matrix<D, D> = [[0.0; D]; D];
                for j in 0..D {
                    for i in 0..=j {
                        let v = 3e7 * rng.sym();
                        st[j][i] = v;
                        st[i][j] = v;
                    }
                }
                if dp_f(&c, &st) <= 0. {
                    continue;
                }
                tested += 1;
                let dep = dp_return_closed_form(&c, &st).unwrap();
                let cd = ce(&dep);
                let s: Matrix<D, D> =
                    std::array::from_fn(|j| std::array::from_fn(|i| st[j][i] - cd[j][i]));
                assert!(dp_f(&c, &s).abs() <= 1e-9 * 3e7, "f = {}", dp_f(&c, &s));
                let tr = |x: &Matrix<D, D>| -> Real { (0..D).map(|i| x[i][i]).sum() };
                if radial {
                    assert!((tr(&s) - tr(&st)).abs() <= 1e-9 * 3e7 || tr(&st) * 1.2 - 6e6 >= 0.);
                } else {
                    // variational inequality with C⁻¹:(σ_tr − σ) = Δε_p
                    #[allow(clippy::cast_precision_loss)]
                    let apex: Matrix<D, D> = std::array::from_fn(|j| {
                        std::array::from_fn(|i| if i == j { 6e6 / (1.2 * D as Real) } else { 0. })
                    });
                    for tau in [apex, s.map(|col| col.map(|v| 0.5 * v + 0.5 * 0.))] {
                        let mut tau = tau;
                        if dp_f(&c, &tau) > 0. {
                            tau = apex;
                        }
                        let mut vi = 0.;
                        for j in 0..D {
                            for i in 0..D {
                                vi += dep[j][i] * (tau[j][i] - s[j][i]);
                            }
                        }
                        assert!(vi <= 1e-9 * 3e7 * max_abs(dep.as_flattened()), "VI {vi}");
                    }
                }
            }
            assert!(tested > 50);
        }
    }
    check::<2, 3>(false);
    check::<2, 3>(true);
    check::<3, 6>(false);
}

/// UD-146: the recorded upstream shear fixture
/// (`material/plastic_drucker_prager/2d_shear`, `φ = 30°`, `fc = 3e7`:
/// `α = 1.2`, `k = 1.8e7`) ends **outside** the yield surface after a
/// "converged" return in 10 of 20 recorded steps (e.g. step 18: `τ =
/// −12.06 MPa`, `f = √3|τ| − k = 2.89e6` Pa) while others undershoot; the
/// default returns onto it (`|τ| = k/√3`) at every plastic step.
#[test]
fn mi02_dp_upstream_shear_record_violates_the_yield_condition() {
    use akantu_validation::{Fixture, fixtures_root};
    let fx =
        Fixture::open(&fixtures_root().join("material/plastic_drucker_prager/2d_shear")).unwrap();
    let st = fx.load_f64("stress").unwrap();
    let k = 1.8e7;
    // f of every recorded step (pure shear: tr σ = 0 up to 1e-8 Pa)
    let outside = (0..st.shape[0])
        .filter(|&i| {
            let r = &st.data[i * 4..i * 4 + 4];
            3_f64.sqrt() * r[1].abs() + 1.2 * (r[0] + r[3]) - k > 1e6
        })
        .count();
    assert!(outside >= 8, "{outside} recorded steps outside the surface");
    let dat_s = std::fs::read_to_string(fx.input_path("material").unwrap()).unwrap();
    let g = fx.load_f64("input/grad_u_prescribed").unwrap();
    let mut p = Point::new(2, &dat_s, LawOptions::default());
    let mut ep_prev = p.internal("inelastic_strain");
    for step in 0..g.shape[0] {
        p.step(&g.data[step * 4..step * 4 + 4]).unwrap();
        let ep = p.internal("inelastic_strain");
        let plastic = ep != ep_prev;
        ep_prev = ep;
        if plastic {
            let s = p.stress();
            let f = 3_f64.sqrt() * s[1].abs() + 1.2 * (s[0] + s[3]) - k;
            assert!(f.abs() <= 1e-8 * k, "step {step}: f {f}");
        }
    }
}

/// UD-146: 1-D Drucker–Prager (`J2 = 0`): physical `φ = 30°`
/// tension returns to `k/α` = 5 MPa in both modes; `φ = −30°` in compression
/// yields to `k/α` = −15 MPa by default, while upstream's singular gradient
/// silently returned the unchanged elastic −50 MPa before the fix (compat:
/// `Error::Numerical`).
#[test]
fn mi02_dp_1d_returns() {
    let mut p = Point::new(
        1,
        &dat(DP, "  radial_return = false\n"),
        LawOptions::default(),
    );
    p.step(&[2e-3]).unwrap();
    assert!((p.stress()[0] - 5e6).abs() <= 1e-6);
    assert!((p.internal("inelastic_strain")[0] - 1.8e-3).abs() <= 1e-15);
    let neg = dat(DP, "  radial_return = false\n").replace("phi = 30", "phi = -30");
    let mut p = Point::new(1, &neg, LawOptions::default());
    p.step(&[-2e-3]).unwrap();
    assert!((p.stress()[0] + 1.5e7).abs() <= 1e-6, "{:?}", p.stress());
    let mut u = Point::new(1, &neg, LawOptions::upstream());
    let e = u.step(&[-2e-3]).unwrap_err();
    assert!(matches!(e, akantu_core::Error::Numerical(_)), "{e}");
}

// ---------------------------------------------------------------- ---

/// Public kernels validate their buffers instead of panicking
/// (`neohookean_tangent::<2>(…, &mut [])` panicked before the fix).
#[test]
fn mi08_public_kernels_reject_malformed_buffers() {
    use akantu_solid::materials::neohookean::{
        neohookean_tangent, neohookean_tangent_plane_stress,
    };
    let g = Matrix::<2, 2>::default();
    let r = std::panic::catch_unwind(|| {
        let mut empty: [Real; 0] = [];
        neohookean_tangent::<2>(1e9, 1e9, &g, 1., &mut empty)
    });
    assert!(matches!(
        r,
        Ok(Err(akantu_core::Error::DimensionMismatch { .. }))
    ));
    let mut short = [0.; 4];
    assert!(neohookean_tangent_plane_stress(1e9, 1e9, &g, 1., &mut short).is_err());
    // Drucker–Prager `(D, S)`: invalid combinations do not compile (const
    // assertion of `akantu_core::voigt`, `compile_fail` doctest of
    // `dp_closest_point_projection`); valid ones still run
    let c = dp_constants::<3>(false);
    let z = Matrix::<3, 3>::default();
    let r = dp_stress_on_quad::<3, 6>(
        &c,
        &de3(&c),
        &z,
        &z,
        &z,
        &z,
        (0., 0.),
        DruckerPragerReturn::ClosedForm,
    );
    assert!(r.is_ok());
}

// ---------------------------------------------------------- switches -----

/// The compat switches of these tests are part of `LawOptions::upstream()`
/// and off in `LawOptions::default()`.
#[test]
fn v32_switches_default_and_upstream() {
    let d = LawOptions::default();
    let u = LawOptions::upstream();
    assert_eq!(d.j2_reduced_return, J2ReducedReturn::Consistent);
    assert_eq!(u.j2_reduced_return, J2ReducedReturn::UpstreamTruncated);
    assert!(d.committed_trial_state && !u.committed_trial_state);
    assert!(d.plastic_mazars_effective_stress && !u.plastic_mazars_effective_stress);
    assert!(d.mazars_mechanical_strain && !u.mazars_mechanical_strain);
    assert!(d.mazars_out_of_plane_strain && !u.mazars_out_of_plane_strain);
    assert_eq!(d.finite_strain_damage, FiniteStrainDamage::Unsupported);
    assert_eq!(u.finite_strain_damage, FiniteStrainDamage::UpstreamLegacy);
    assert_eq!(d.drucker_prager_return, DruckerPragerReturn::ClosedForm);
    assert_eq!(
        u.drucker_prager_return,
        DruckerPragerReturn::UpstreamIteration
    );
}
