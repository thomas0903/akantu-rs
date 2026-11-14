// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Regression tests
// `mx303_*` and `mx308_*`, the UD-762 corrected default of
// `sls_deviatoric` (src/model/solid_mechanics/materials/
// material_viscoelastic/material_standard_linear_solid_deviatoric.{hh,cc}).

//! `mx303_*` (E = 15, ν = 0.25, Eta = 10, `∇u = k·1e-3 e1⊗e1`, Δt = 0.5,
//! 5 steps):
//! `Ev = 1e-310` and `1e-320` gave a NaN dissipated energy in the default
//! mode. `mx308_*` checks the decision on `Ev > E`.

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

/// `(σ, h, D)` of point 0 after each of 5 steps of `∇u = min(k, 3)·1e-3·e1⊗e1`
/// (`Δt = 0.5`).
fn drive(dim: usize, extra: &str, options: LawOptions) -> Vec<(Vec<Real>, Vec<Real>, Real)> {
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
    for k in 1..=5 {
        let mut g = vec![0.0; dim * dim];
        g[0] = 1e-3 * (k.min(3) as Real);
        p.step(&t, &g, 1);
        out.push((
            p.stress(),
            p.internal("history_integral"),
            p.internal("dissipated_energy")[0],
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

/// An `Ev > 0` whose `γ_v = Ev/E` is subnormal or zero makes
/// `α = 1/(2μγ_v)` overflow; the default treats it as the `Ev → 0` limit
/// (no branch, `τ = ∞`): the dissipation is exactly 0 (it was NaN), and
/// the stress and the history integral are bit-identical to the `Ev = 0`
/// law (`γ_v·h` vanishes below the stress's last bit, `γ_∞ = (E − Ev)/E`
/// rounds to 1). `Ev = 1e-300` (`α` finite) keeps the regular arithmetic:
/// a finite, positive, vanishing dissipation. Upstream (compat) still gives
/// the NaN.
#[test]
fn mx303_subnormal_ev_takes_the_no_branch_limit() {
    for dim in 1..=3 {
        let none = drive(dim, "  Eta = 10\n  Ev = 0\n", LawOptions::default());
        for ev in ["1e-310", "1e-320", "5e-324"] {
            let r = drive(
                dim,
                &format!("  Eta = 10\n  Ev = {ev}\n"),
                LawOptions::default(),
            );
            for (k, ((s, h, d), (s0, h0, _))) in r.iter().zip(&none).enumerate() {
                assert_eq!(
                    d.to_bits(),
                    0.0_f64.to_bits(),
                    "{dim}-D Ev = {ev}, step {k}: D = {d}"
                );
                let bits = |v: &[Real]| v.iter().map(|x| x.to_bits()).collect::<Vec<_>>();
                assert_eq!(bits(s), bits(s0), "{dim}-D Ev = {ev}, step {k}: stress");
                assert_eq!(bits(h), bits(h0), "{dim}-D Ev = {ev}, step {k}: h");
            }
            let up = drive(
                dim,
                &format!("  Eta = 10\n  Ev = {ev}\n"),
                upstream_branch(),
            );
            assert!(
                up[4].2.is_nan(),
                "{dim}-D upstream Ev = {ev}: D = {}",
                up[4].2
            );
        }
        let regular = drive(dim, "  Eta = 10\n  Ev = 1e-300\n", LawOptions::default());
        let d = regular[4].2;
        assert!((0. ..1e-290).contains(&d), "{dim}-D Ev = 1e-300: D = {d}");
        // and bitwise the upstream arithmetic there (α finite: not degenerate)
        let up = drive(dim, "  Eta = 10\n  Ev = 1e-300\n", upstream_branch());
        assert_eq!(d.to_bits(), up[4].2.to_bits(), "{dim}-D Ev = 1e-300");
    }
}

/// `Ev > E` (negative long-term modulus `Einf = E − Ev`) is refused
/// by the corrected default at `initMaterial` and, after a parameter update,
/// by the next evaluation; `Ev = E` (no equilibrium spring) is admissible;
/// upstream (compat) accepts `Ev > E`.
#[test]
fn mx308_ev_larger_than_e_is_refused_by_default() {
    let model = one_element(3);
    match handler(&model, "  Eta = 10\n  Ev = 20\n", LawOptions::default()) {
        Err(Error::InvalidArgument(msg)) => {
            assert!(msg.contains("UD-762") && msg.contains("Einf"), "{msg}");
        }
        Err(e) => panic!("Ev = 20 > E = 15: unexpected error {e}"),
        Ok(_) => panic!("Ev = 20 > E = 15 accepted by the default"),
    }
    let equal = drive(3, "  Eta = 10\n  Ev = 15\n", LawOptions::default());
    assert!(
        equal
            .iter()
            .all(|(s, h, d)| d.is_finite() && s.iter().chain(h).all(|x| x.is_finite())),
        "Ev = E: {equal:?}"
    );
    let up = drive(3, "  Eta = 10\n  Ev = 20\n", upstream_branch());
    assert!(up[4].0.iter().all(|x| x.is_finite()));
    // a parameter update to Ev > E is caught by the next evaluation
    let t = TimedModel {
        inner: &model,
        dt: 0.5,
    };
    let mut h = handler(&model, "  Eta = 10\n  Ev = 5\n", LawOptions::default()).unwrap();
    let law = h.law_mut(0).unwrap();
    law.set_param("Ev", ParamValue::Real(20.)).unwrap();
    law.update_internal_parameters().unwrap();
    let err = law
        .compute_stress(&t.ctx(LawPhase::Stress), simplex(3), common::NG)
        .unwrap_err();
    assert!(matches!(err, Error::InvalidArgument(_)), "{err}");
}
