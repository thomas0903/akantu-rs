// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Regression tests
// `mx202_*` / UD-762, degenerate viscous-branch parameters of
// `sls_deviatoric` (src/model/solid_mechanics/materials/
// material_viscoelastic/material_standard_linear_solid_deviatoric.cc).

//! `mx202_*` (default options, 1-D/2-D/3-D, one step
//! at `H11 = 1e-3`) check the corrected default (finite, the limit 0 of
//! the dissipation) and that the compatibility switch still shows
//! upstream's NaN.

mod common;
#[path = "common/timed.rs"]
mod timed;

use akantu_core::{Error, Real};
use akantu_model::constitutive_law::LawPhase;
use akantu_parser::param::ParamValue;
use akantu_solid::Material;
use akantu_solid::material::LawOptions;
use akantu_solid::materials::sls_deviatoric::SlsDegenerateBranch;
use common::{SimplexModel, materials_from_dat_with, simplex};
use timed::{Point, TimedModel, one_element};

type Handler = akantu_model::ConstitutiveLawsHandler<dyn Material>;

const SLS: &str =
    "material sls_deviatoric [\n  name = sls\n  rho = 1\n  E = 15\n  nu = 0.25\n{extra}]\n";

fn handler(model: &SimplexModel, extra: &str, options: LawOptions) -> Result<Handler, Error> {
    materials_from_dat_with(
        model,
        SLS.replace("{extra}", extra).as_bytes(),
        Some(options),
    )
}

/// `(σ, h, D, ψ)` of point 0 after each of `steps` steps of `∇u = k·1e-3·e1⊗e1`
/// (load) then held, `Δt = 0.5`.
fn drive(
    dim: usize,
    extra: &str,
    options: LawOptions,
    steps: usize,
) -> Vec<(Vec<Real>, Vec<Real>, Real, Real)> {
    let model = one_element(dim);
    let t = TimedModel {
        inner: &model,
        dt: 0.5,
    };
    let mut h = handler(&model, extra, options).unwrap();
    let mut p = Point {
        law: h.law_mut(0).unwrap(),
        ty: simplex(dim),
    };
    let mut out = Vec::new();
    for k in 1..=steps {
        let mut g = vec![0.0; dim * dim];
        g[0] = 1e-3 * (k.min(3) as Real);
        p.step(&t, &g, 1);
        let psi = p.energy(&t, "potential");
        out.push((
            p.stress(),
            p.internal("history_integral"),
            p.internal("dissipated_energy")[0],
            psi,
        ));
    }
    out
}

fn upstream_branch() -> LawOptions {
    LawOptions {
        sls_degenerate_branch: SlsDegenerateBranch::Upstream,
        ..LawOptions::default()
    }
}

/// `Ev = 0` is an elastic law (`Einf = E`, no branch): the corrected
/// dissipation is exactly 0 (upstream: `a = ε_d − ∞·0 = NaN`), stress and
/// stored energy are finite and the branch carries no stress.
#[test]
fn mx202_ev_zero_dissipates_nothing() {
    for dim in 1..=3 {
        for eta in ["10", "0"] {
            let extra = format!("  Eta = {eta}\n  Ev = 0\n");
            for (k, (s, h, d, psi)) in drive(dim, &extra, LawOptions::default(), 5)
                .into_iter()
                .enumerate()
            {
                assert_eq!(d, 0.0, "{dim}-D Eta = {eta}, step {k}: D = {d}");
                assert!(
                    s.iter().all(|x| x.is_finite()),
                    "{dim}-D Eta = {eta}: {s:?}"
                );
                assert!(
                    h.iter().all(|x| x.is_finite()),
                    "{dim}-D Eta = {eta}: {h:?}"
                );
                assert!(
                    psi.is_finite() && psi > 0.,
                    "{dim}-D Eta = {eta}: ψ = {psi}"
                );
            }
        }
        // compat: upstream's NaN (Eta > 0), NaN stress for Eta = Ev = 0
        let up = drive(dim, "  Eta = 10\n  Ev = 0\n", upstream_branch(), 1);
        assert!(up[0].2.is_nan(), "{dim}-D upstream D = {}", up[0].2);
        assert!(up[0].0.iter().all(|x| x.is_finite()));
        let up0 = drive(dim, "  Eta = 0\n  Ev = 0\n", upstream_branch(), 1);
        assert!(
            up0[0].0.iter().any(|x| x.is_nan()),
            "{dim}-D: {:?}",
            up0[0].0
        );
    }
}

/// `Eta = 0` with `Ev > 0` relaxes the branch instantaneously: `h ≡ 0`, the
/// stress is the relaxed one, the corrected dissipation is its limit 0
/// (upstream: `q̇ = 0/0`); the limit is approached continuously as
/// `Eta → 0`.
#[test]
fn mx202_eta_zero_relaxes_instantaneously() {
    for dim in 1..=3 {
        let res = drive(dim, "  Eta = 0\n  Ev = 5\n", LawOptions::default(), 5);
        for (k, (s, h, d, psi)) in res.iter().enumerate() {
            assert_eq!(*d, 0.0, "{dim}-D step {k}");
            assert!(h.iter().all(|x| *x == 0.0), "{dim}-D step {k}: h = {h:?}");
            assert!(s.iter().all(|x| x.is_finite()) && psi.is_finite());
        }
        // continuity: D(Eta) decreases to 0 as Eta → 0 (Δt/τ = 1.25, 5,
        // 25; much smaller τ makes the increment rounding noise of the
        // cancellation `ε_d − αq = h/(2μ)`, `h ~ e^{−Δt/2τ}`)
        let ds: Vec<Real> = ["2", "0.5", "0.1"]
            .iter()
            .map(|eta| {
                drive(
                    dim,
                    &format!("  Eta = {eta}\n  Ev = 5\n"),
                    LawOptions::default(),
                    5,
                )[4]
                .2
            })
            .collect();
        assert!(
            ds[0] > ds[1] && ds[1] > ds[2] && ds[2] > 0.,
            "{dim}-D: {ds:?}"
        );
        // the stress converges to the relaxed one
        let relaxed = &res[4].0;
        let near = &drive(dim, "  Eta = 1e-12\n  Ev = 5\n", LawOptions::default(), 5)[4].0;
        let scale = relaxed.iter().fold(0.0_f64, |m, x| m.max(x.abs()));
        for (a, b) in relaxed.iter().zip(near) {
            assert!(
                (a - b).abs() <= 1e-12 * scale,
                "{dim}-D: {relaxed:?} vs {near:?}"
            );
        }
        // compat: upstream's NaN
        let up = drive(dim, "  Eta = 0\n  Ev = 5\n", upstream_branch(), 1);
        assert!(up[0].2.is_nan(), "{dim}-D upstream D = {}", up[0].2);
    }
}

/// `Ev → 0`: the dissipation vanishes continuously (`γ_v/τ = Ev²/(E·Eta)`).
#[test]
fn mx202_small_ev_dissipation_tends_to_zero() {
    let ds: Vec<Real> = ["1e-2", "1e-4", "1e-6"]
        .iter()
        .map(|ev| {
            drive(
                3,
                &format!("  Eta = 10\n  Ev = {ev}\n"),
                LawOptions::default(),
                5,
            )[4]
            .2
        })
        .collect();
    assert!(ds[0] > ds[1] && ds[1] > ds[2] && ds[2] > 0., "{ds:?}");
    assert!(ds[2] < 1e-6 * ds[0], "{ds:?}");
}

/// Negative or non-finite `Ev` / `Eta` are refused by the corrected default
/// at `initMaterial` and, after a parameter update, by the next evaluation;
/// upstream accepts them.
#[test]
fn mx202_negative_or_non_finite_parameters_are_refused() {
    let model = one_element(2);
    for extra in [
        "  Eta = -1\n  Ev = 5\n",
        "  Eta = 10\n  Ev = -5\n",
        "  Eta = nan\n  Ev = 5\n",
        "  Eta = 10\n  Ev = inf\n",
    ] {
        match handler(&model, extra, LawOptions::default()) {
            Err(Error::InvalidArgument(m)) => assert!(m.contains("UD-762"), "{m}"),
            Err(e) => panic!("{extra}: unexpected error {e}"),
            Ok(_) => panic!("{extra}: accepted"),
        }
        assert!(handler(&model, extra, upstream_branch()).is_ok(), "{extra}");
    }
    let t = TimedModel {
        inner: &model,
        dt: 0.5,
    };
    let mut h = handler(&model, "  Eta = 10\n  Ev = 5\n", LawOptions::default()).unwrap();
    let law = h.law_mut(0).unwrap();
    law.set_param("Ev", ParamValue::Real(-1.)).unwrap();
    law.update_internal_parameters().unwrap();
    let err = law
        .compute_stress(&t.ctx(LawPhase::Stress), simplex(2), common::NG)
        .unwrap_err();
    assert!(matches!(err, Error::InvalidArgument(_)), "{err}");
}
