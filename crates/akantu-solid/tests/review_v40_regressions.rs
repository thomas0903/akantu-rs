// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Regression tests of
// the viscoelastic laws (src/model/solid_mechanics/materials/
// material_viscoelastic/material_standard_linear_solid_deviatoric.{hh,cc},
// material_viscoelastic_maxwell.{hh,cc}).

//! Tests named `mxNN_*`. Each checks the corrected default and that
//! the compatibility switch still shows the defect.

mod common;
#[path = "common/timed.rs"]
mod timed;

use akantu_core::Real;
use akantu_model::constitutive_law::LawPhase;
use akantu_parser::param::ParamValue;
use akantu_solid::Material;
use akantu_solid::material::LawOptions;
use akantu_solid::materials::maxwell::MaxwellThermal;
use common::{NG, SimplexModel, materials_from_dat_with, simplex};
use timed::{Point, TimedModel, col_major, one_element};

type Handler = akantu_model::ConstitutiveLawsHandler<dyn Material>;

fn handler(model: &SimplexModel, dat: &str, options: LawOptions) -> Handler {
    materials_from_dat_with(model, dat.as_bytes(), Some(options)).unwrap()
}

/// Potential-energy density of point 0 (the law's `computePotentialEnergy`).
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

// ---------------------------------------------------------------------------
// `mx01_*`: SLS stored energy and dissipation in 1-D / 2-D
// ---------------------------------------------------------------------------

const SLS: &str = "material sls_deviatoric [\n  name = sls\n  rho = 1\n  E = 15\n  nu = 0.25\n  Eta = 10\n  Ev = 5\n{extra}]\n";

/// Amplitude: ramp `0 → 1` over `[0, 2]`, unload to 0 over
/// `[2, 4]`, hold 0 over `[4, 6]`.
fn ramp_unload_hold(time: Real) -> Real {
    if time <= 2. {
        time / 2.
    } else if time <= 4. {
        (4. - time) / 2.
    } else {
        0.
    }
}

/// Drives one `sls_deviatoric` point over `t ∈ [0, 6]` with `∇u(t) =
/// a(t)·shape` (`a` = [`ramp_unload_hold`], `shape` column-major, one
/// evaluation per step) and returns `(W, ψ, D)` at `t = 6`: the trapezoidal
/// work `Σ ½(σ_new + σ_old):(∇u_new − ∇u_old)` recomputed here (stress
/// symmetric, so the contraction with `∇u` is the one with `ε`), the law's
/// stored potential-energy density and its dissipated-energy density.
fn sls_balance(
    dim: usize,
    dt: Real,
    options: LawOptions,
    extra: &str,
    shape: &[Real],
) -> (Real, Real, Real) {
    let model = one_element(dim);
    let t = TimedModel { inner: &model, dt };
    let mut h = handler(&model, &SLS.replace("{extra}", extra), options);
    let mut p = Point {
        law: h.law_mut(0).unwrap(),
        ty: simplex(dim),
    };
    let n = dim * dim;
    let steps = (6.0 / dt).round() as usize;
    let (mut g_old, mut s_old) = (vec![0.0; n], vec![0.0; n]);
    let mut work = 0.0;
    for k in 1..=steps {
        let a = ramp_unload_hold(k as Real * dt);
        let g: Vec<Real> = shape.iter().map(|x| 1e-3 * a * x).collect();
        p.step(&t, &g, 1);
        let s = p.stress();
        work += 0.5
            * (0..n)
                .map(|l| (s[l] + s_old[l]) * (g[l] - g_old[l]))
                .sum::<Real>();
        (g_old, s_old) = (g, s);
    }
    let psi = potential_density(&mut p, &t);
    let diss = p.internal("dissipated_energy")[0];
    (work, psi, diss)
}

/// `H11 = 1` (column-major `dim × dim`).
fn uniaxial(dim: usize) -> Vec<Real> {
    let mut g = vec![0.0; dim * dim];
    g[0] = 1.;
    g
}

const DTS: [Real; 4] = [0.1, 0.025, 0.00625, 0.0015625];

/// Relative balance residuals `(W − ψ − D)/W` over [`DTS`].
fn residuals(dim: usize, options: LawOptions, extra: &str, shape: &[Real]) -> Vec<Real> {
    DTS.iter()
        .map(|&dt| {
            let (w, psi, d) = sls_balance(dim, dt, options, extra, shape);
            // upstream's ½σ:∇u is 0 at the final zero strain
            assert!(
                w > 0. && psi >= 0. && d > 0.,
                "{dim}-D dt {dt}: {w} {psi} {d}"
            );
            (w - psi - d) / w
        })
        .collect()
}

/// Corrected balance: the residual tends to zero at first order (rectangle
/// dissipation; the work is trapezoidal, the stored energy exact), as the
/// 3-D column (`−4.6e−3, −5.9e−4, −1.1e−4, −2.6e−5`).
fn assert_converges(label: &str, r: &[Real]) {
    eprintln!("mx01 {label}: (W − ψ − D)/W = {r:?}");
    for w in r.windows(2) {
        let ratio = (w[1] / w[0]).abs();
        assert!(ratio < 0.35, "{label}: refinement ratio {ratio} ({r:?})");
    }
    assert!(r[3].abs() < 1e-4, "{label}: {r:?}");
}

/// `E = 15`, `ν = 0.25`, `Ev = 5`, `Eta = 10`,
/// virgin state, uniaxial `H11(t) = 1e−3·a(t)` (ramp/unload/hold over
/// `[0, 6]`), one evaluation per step. Before the fix the residual tended to
/// `1/3` (1-D) and `1/6` (2-D): the retained `D × D` block misses the
/// eliminated deviatoric normal components of the branch
/// (`h:B⁻¹:h = h:h + (tr_D h)²/(3 − D)`). Corrected: first-order convergence
/// to zero in every dimension; the 3-D values are unchanged by the fix.
#[test]
fn mx01_reviewer_case_reduced_dimensions_energy_balance() {
    for dim in [1, 2, 3] {
        let r = residuals(dim, LawOptions::default(), "", &uniaxial(dim));
        assert_converges(&format!("{dim}-D uniaxial"), &r);
        if dim == 3 {
            // the 3-D column (unchanged by the fix)
            let want = [
                -0.004_572_825_5,
                -0.000_586_886_626,
                -0.000_111_999_961,
                -0.000_025_830_265,
            ];
            for (got, want) in r.iter().zip(want) {
                assert!((got - want).abs() <= 1e-11, "3-D: {got} vs {want}");
            }
        }
    }
}

/// The finest 2-D state (`Δt = 0.0015625`): the work is
/// the 3-D one, the stored energy and the dissipation now are too
/// (`ψ = 2.8810408238476269e−8`, `D = 8.6769911697088435e−7` in 3-D; the 2-D
/// law before the fix gave `2.4008673532063562e−8` and
/// `7.2308259747573191e−7`). Uniaxial strain with zero lateral strain is the
/// same state in 2-D (plane strain) and 3-D.
#[test]
fn mx01_reviewer_finest_2d_state_matches_3d() {
    let dt = DTS[3];
    let (w2, psi2, d2) = sls_balance(2, dt, LawOptions::default(), "", &uniaxial(2));
    let (w3, psi3, d3) = sls_balance(3, dt, LawOptions::default(), "", &uniaxial(3));
    for (a, b, what) in [(w2, w3, "W"), (psi2, psi3, "ψ"), (d2, d3, "D")] {
        assert!(
            (a - b).abs() <= 1e-13 * b.abs(),
            "{what}: 2-D {a} vs 3-D {b}"
        );
    }
    assert!((w3 - 8.964_863_687_292_332e-7).abs() <= 1e-12 * w3, "{w3}");
    assert!(
        (psi3 - 2.881_040_823_847_627e-8).abs() <= 1e-12 * psi3,
        "{psi3}"
    );
    assert!((d3 - 8.676_991_169_708_844e-7).abs() <= 1e-12 * d3, "{d3}");
}

/// Normal/volumetric and mixed histories in 2-D (plane strain and
/// `Plane_Stress`, where only the bulk coefficient changes: the energy is the
/// potential of the 2-D operator as written) and 1-D: the balance converges
/// to zero under refinement.
#[test]
fn mx01_normal_volumetric_and_mixed_histories_converge() {
    let biaxial = col_major(&[&[1., 0.], &[0., 1.]]);
    let mixed = col_major(&[&[1., 0.3], &[0.3, -0.4]]);
    let unequal = col_major(&[&[1., 0.], &[0., 0.25]]);
    for (label, extra, shape) in [
        ("2-D biaxial", "", &biaxial),
        ("2-D unequal normal", "", &unequal),
        ("2-D mixed", "", &mixed),
        (
            "2-D biaxial plane stress",
            "  Plane_Stress = true\n",
            &biaxial,
        ),
        ("2-D mixed plane stress", "  Plane_Stress = true\n", &mixed),
    ] {
        let r = residuals(2, LawOptions::default(), extra, shape);
        assert_converges(label, &r);
    }
    let r = residuals(1, LawOptions::default(), "", &[-1.]);
    assert_converges("1-D compression", &r);
}

/// Closed form of one step from the virgin state: `h = e^{−Δt/2τ}·s`
/// with `s = 2μ·B ε` (`B ε = ε − I tr ε/3` on the retained block), stored
/// energy `½σ∞:ε + γ_v/(4μ)·(h:h + (tr h)²/(3 − D))` and dissipation
/// increment `γ_v/(2μτ)·(h:h + (tr h)²/(3 − D))·Δt` (rectangle rule with the
/// new `h`). Equivalently: the eliminated deviatoric normal components
/// `−tr h/(3 − D)` reconstructed (1-D: `h22 = h33 = −h11/2`; 2-D:
/// `h33 = −(h11 + h22)`).
#[test]
fn mx01_one_step_closed_form_with_reconstructed_components() {
    let (e, nu, ev, eta, dt) = (15.0_f64, 0.25_f64, 5.0_f64, 10.0_f64, 0.5_f64);
    for dim in [1usize, 2] {
        let nu = if dim == 1 { 0. } else { nu }; // initMaterial: ν = 0 in 1-D
        let mu = e / (2. * (1. + nu));
        let lambda = nu * e / ((1. + nu) * (1. - 2. * nu));
        let kpa = lambda + 2. / 3. * mu;
        let (gamma_inf, gamma_v, tau) = ((e - ev) / e, ev / e, eta / ev);
        let model = one_element(dim);
        let t = TimedModel { inner: &model, dt };
        let mut hd = handler(&model, &SLS.replace("{extra}", ""), LawOptions::default());
        let mut p = Point {
            law: hd.law_mut(0).unwrap(),
            ty: simplex(dim),
        };
        let g = if dim == 1 {
            vec![1e-3]
        } else {
            col_major(&[&[1e-3, 2e-4], &[2e-4, -3e-4]])
        };
        p.step(&t, &g, 1);
        let psi = potential_density(&mut p, &t);
        let diss = p.internal("dissipated_energy")[0];
        // reconstructed 3-D tensors (uniaxial / plane strain kinematics)
        let n = dim * dim;
        let theta: Real = (0..dim).map(|i| g[i * (dim + 1)]).sum();
        let eps3 = |i: usize, j: usize| -> Real {
            if i < dim && j < dim {
                0.5 * (g[i + j * dim] + g[j + i * dim])
            } else {
                0.
            }
        };
        let e2 = (-0.5 * dt / tau).exp();
        let mut hh = 0.0; // h:h of the full 3-D deviator
        let mut eps_dev_eps = 0.0;
        for i in 0..3 {
            for j in 0..3 {
                let dev = eps3(i, j) - if i == j { theta / 3. } else { 0. };
                let h = e2 * 2. * mu * dev;
                hh += h * h;
                eps_dev_eps += dev * eps3(i, j);
            }
        }
        let psi_closed = 0.5 * gamma_inf * (kpa * theta * theta + 2. * mu * eps_dev_eps)
            + gamma_v / (4. * mu) * hh;
        let diss_closed = gamma_v / (2. * mu * tau) * hh * dt;
        assert!(
            (psi - psi_closed).abs() <= 1e-13 * psi_closed,
            "{dim}-D ψ {psi} vs {psi_closed}"
        );
        assert!(
            (diss - diss_closed).abs() <= 1e-13 * diss_closed,
            "{dim}-D D {diss} vs {diss_closed}"
        );
        // the retained-block norm alone is what the defect used
        let h_in = p.internal("history_integral");
        let retained: Real = h_in[..n].iter().map(|x| x * x).sum();
        assert!(
            retained < hh * (1. - 1e-6),
            "{dim}-D: eliminated part must be non-zero"
        );
    }
}

/// The compatibility mode keeps upstream's energy (`½σ:∇u`) and
/// matrix-product dissipation: the balance does not converge (the branch
/// energy is missing, zero at zero strain), in every dimension.
#[test]
fn mx01_upstream_compat_keeps_the_legacy_energy() {
    for dim in [1, 2, 3] {
        let r = residuals(dim, LawOptions::upstream(), "", &uniaxial(dim));
        eprintln!("mx01 upstream {dim}-D: {r:?}");
        assert!(
            r[3].abs() > 1e-2 && r[3].abs() > 0.5 * r[0].abs(),
            "{dim}-D upstream: {r:?}"
        );
    }
}

// ---------------------------------------------------------------------------
// `mx02_*`: Maxwell with thermal loading
// ---------------------------------------------------------------------------

const MAXWELL: &str = "material viscoelastic_maxwell [\n  name = m\n  rho = 1\n  Einf = 10\n  nu = 0\n  Eta = [10]\n  Ev = [5]\n{extra}]\n";

fn maxwell_options(thermal: MaxwellThermal) -> LawOptions {
    LawOptions {
        maxwell_thermal: thermal,
        ..LawOptions::default()
    }
}

/// With `alpha ≠ 0` the corrected mode refuses the law: at
/// `initMaterial` (parsed `alpha`), and — `alpha` set after initialisation —
/// at `computeStress`, `computePotentialEnergy` and `afterSolveStep` (the
/// branch update), all `Error::Unsupported`. `alpha = 0` with a temperature
/// change is accepted (no thermal stress).
#[test]
fn mx02_maxwell_thermal_loading_is_rejected_by_default() {
    for dim in [1, 2, 3] {
        let model = one_element(dim);
        let t = TimedModel {
            inner: &model,
            dt: 0.1,
        };
        let dat = MAXWELL.replace("{extra}", "  alpha = 0.01\n");
        let r = materials_from_dat_with(&model, dat.as_bytes(), Some(LawOptions::default()));
        assert!(
            matches!(r, Err(akantu_core::Error::Unsupported(_))),
            "{dim}-D init: {:?}",
            r.err()
        );

        // isothermal init, then alpha ≠ 0
        let dat = MAXWELL.replace("{extra}", "  delta_T = 1\n");
        let mut h = handler(&model, &dat, LawOptions::default());
        let mut p = Point {
            law: h.law_mut(0).unwrap(),
            ty: simplex(dim),
        };
        let g = vec![1e-3; dim * dim];
        p.step(&t, &g, 1); // alpha = 0: accepted
        p.law.set_param("alpha", ParamValue::Real(0.01)).unwrap();
        p.law
            .before_solve_step(&t.ctx(LawPhase::BeforeSolveStep))
            .unwrap();
        let r = p.law.compute_stress(&t.ctx(LawPhase::Stress), p.ty, NG);
        assert!(
            matches!(r, Err(akantu_core::Error::Unsupported(_))),
            "{dim}-D computeStress: {r:?}"
        );
        let r = p
            .law
            .compute_potential_energy(&t.ctx(LawPhase::Energy), p.ty);
        assert!(
            matches!(r, Err(akantu_core::Error::Unsupported(_))),
            "{dim}-D computePotentialEnergy: {r:?}"
        );
        let r = p
            .law
            .after_solve_step(&t.ctx(LawPhase::AfterSolveStep), true);
        assert!(
            matches!(r, Err(akantu_core::Error::Unsupported(_))),
            "{dim}-D afterSolveStep: {r:?}"
        );
        // the compatibility switch accepts it again
        p.law
            .set_options(maxwell_options(MaxwellThermal::Upstream))
            .unwrap();
        p.law
            .compute_stress(&t.ctx(LawPhase::Stress), p.ty, NG)
            .unwrap();
        p.law
            .after_solve_step(&t.ctx(LawPhase::AfterSolveStep), true)
            .unwrap();
    }
}

/// Case under the compatibility switch
/// ([`MaxwellThermal::Upstream`], branch recurrence otherwise corrected):
/// 1-D, `Einf = 10`, `Ev = [5]`, `Eta = [10]`, `alpha = 0.01`, `Δt = 0.1`.
/// Heating by `ΔT = 1` at zero strain gives `σ = −0.15` (instantaneous
/// modulus `E = 15`) with `ψ = 0` (not a potential of the stress); holding
/// 400 steps, ramping `H11: 0 → 0.01` in 100 steps and holding 400 steps
/// gives a negative dissipation and a persistent stress `σ → −0.05` at
/// nominal free expansion. The full upstream mode is also accepted.
#[test]
fn mx02_reviewer_case_upstream_thermal_semantics_in_compat() {
    let model = one_element(1);
    let t = TimedModel {
        inner: &model,
        dt: 0.1,
    };
    let dat = MAXWELL.replace("{extra}", "  alpha = 0.01\n");
    let mut h = handler(&model, &dat, maxwell_options(MaxwellThermal::Upstream));
    let mut p = Point {
        law: h.law_mut(0).unwrap(),
        ty: simplex(1),
    };
    let step = |p: &mut Point<'_>, g: Real| {
        p.law
            .before_solve_step(&t.ctx(LawPhase::BeforeSolveStep))
            .unwrap();
        p.write_internal("delta_T", &[1.]);
        p.set_grad_u(&[g]);
        p.law
            .compute_stress(&t.ctx(LawPhase::Stress), p.ty, NG)
            .unwrap();
        p.law
            .after_solve_step(&t.ctx(LawPhase::AfterSolveStep), true)
            .unwrap();
    };
    let psi_of = |p: &Point<'_>| -> Real {
        p.law
            .base()
            .potential_energy
            .get(p.ty, NG)
            .unwrap()
            .as_slice()[0]
    };
    step(&mut p, 0.);
    assert!((p.stress()[0] + 0.15).abs() <= 1e-15, "{:?}", p.stress());
    assert_eq!(psi_of(&p), 0.);
    assert_eq!(p.internal("dissipated_energy")[0], 0.);
    for _ in 0..400 {
        step(&mut p, 0.);
    }
    for k in 1..=100 {
        step(&mut p, 0.01 * Real::from(k) / 100.);
    }
    let (s, psi, d) = (
        p.stress()[0],
        psi_of(&p),
        p.internal("dissipated_energy")[0],
    );
    eprintln!("mx02 ramp end: σ = {s:e}, ψ = {psi:e}, D = {d:e}");
    assert!((s + 0.040_067_379_469_990_85).abs() <= 1e-12, "{s}");
    assert!((psi - 0.000_509_865_695_059_316).abs() <= 1e-14, "{psi}");
    assert!((d + 0.001_429_735_074_538_790_6).abs() <= 1e-14, "{d}");
    for _ in 0..400 {
        step(&mut p, 0.01);
    }
    let (s, psi, d) = (
        p.stress()[0],
        psi_of(&p),
        p.internal("dissipated_energy")[0],
    );
    eprintln!("mx02 hold end: σ = {s:e}, ψ = {psi:e}, D = {d:e}");
    assert!((s + 0.05).abs() <= 1e-9, "{s}");
    assert!((psi - 0.0005).abs() <= 1e-12, "{psi}");
    assert!((d + 0.001_419_869_379_479_474_6).abs() <= 1e-14, "{d}");
    assert!(d < 0., "the upstream semantics dissipate negatively");

    // full upstream mode: accepted (branches never updated, UD-029)
    let mut h = handler(&model, &dat, LawOptions::upstream());
    let mut p = Point {
        law: h.law_mut(0).unwrap(),
        ty: simplex(1),
    };
    step(&mut p, 0.);
    assert!((p.stress()[0] + 0.15).abs() <= 1e-15, "{:?}", p.stress());
}
