// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Independent checks of
// the inelastic materials (src/model/solid_mechanics/materials/
// material_damage/material_{damage,marigo,mazars}*, material_plastic/*):
// physical invariants that hold whatever the oracle says.

//! * damage monotone and bounded, dissipated energy non-negative
//!   (`marigo`, `mazars`, `plastic_mazars`) along cyclic histories;
//! * elastic (secant) unloading of the damage laws: `σ = (1 − d)·C:ε`, the
//!   tangent is the exact derivative there, the dissipated energy stays
//!   constant;
//! * J2: return mapping on the yield surface (3-D), elastic unloading,
//!   non-negative plastic work, exact elastic tangent in the elastic range;
//! * Drucker–Prager: final stress on the yield surface (no radial return),
//!   elastic unloading, the upstream apex projection (documented overshoot,
//!   compat mode);
//! * UD-038: free thermal expansion is stress-free and constrained heating
//!   gives `σ_th·I` (corrected defaults), upstream behaviour in compat mode;
//! * UD-030: finite-strain J2 is unsupported by default;
//! * upstream parameter / internal names, stiffness-change flags,
//!   `set_options`.

mod common;
mod inelastic_common;

use akantu_core::Real;
use akantu_parser::ParamValue;
use akantu_solid::material::{FiniteStrainDamage, FiniteStrainPlasticity, LawOptions};
use inelastic_common::{Point, Rng, max_abs, max_abs_diff};

const MARIGO: &str = "material marigo [\n  name = marigo\n  rho = 3000\n  E = 25e9\n  nu = 0.2\n  \
                      Sd = 5e3\n  Yd = 50\n{extra}]\n";
const MAZARS: &str = "material mazars [\n  name = concrete\n  rho = 3000\n  E = 25e9\n  nu = 0.2\n  \
                      K0 = 1e-4\n  At = 1.0\n  Bt = 5e3\n  Ac = 0.8\n  Bc = 1391.3\n  beta = 1.\n{extra}]\n";
const PLASTIC_MAZARS: &str = "material plastic_mazars [\n  name = pm\n  rho = 3000\n  E = 25e9\n  \
                              nu = 0.2\n  sigma_y = 3e7\n  h = 2e9\n  K0 = 1e-4\n  At = 1.0\n  Bt = 5e3\n  \
                              Ac = 0.8\n  Bc = 1391.3\n  beta = 1.\n{extra}]\n";
const PLASTIC: &str = "material plastic_linear_isotropic_hardening [\n  name = plastic\n  rho = 7800\n  \
                       E = 2.1e11\n  nu = 0.3\n  sigma_y = 3e8\n  h = 1e9\n{extra}]\n";
const DP: &str = "material plastic_drucker_prager [\n  name = dp\n  rho = 2500\n  E = 3e10\n  nu = 0.2\n  \
                  phi = 30\n  fc = 3e7\n{extra}]\n";

fn dat(template: &str, extra: &str) -> String {
    template.replace("{extra}", extra)
}

/// A random `dim × dim` direction (row-major).
fn direction(dim: usize, rng: &mut Rng) -> Vec<Real> {
    (0..dim * dim).map(|_| rng.sym()).collect()
}

/// `s·base` for a load / reverse / reload scale history.
fn cyclic(base: &[Real], peak: Real, n: usize) -> Vec<Vec<Real>> {
    let mut s = Vec::new();
    #[allow(clippy::cast_precision_loss)]
    let nf = n as Real;
    for i in 1..=n {
        s.push(peak * i as Real / nf);
    }
    for i in 1..=2 * n {
        s.push(peak - 2. * peak * i as Real / (2. * nf));
    }
    for i in 1..=2 * n {
        s.push(-peak + 2.5 * peak * i as Real / (2. * nf));
    }
    s.iter()
        .map(|&k| base.iter().map(|b| b * k).collect())
        .collect()
}

/// Isotropic elastic stress `λ tr ε I + 2μ ε` (row-major, dim ≥ 2) or `E ε`.
fn elastic(dim: usize, e: Real, nu: Real, plane_stress: bool, g: &[Real]) -> Vec<Real> {
    if dim == 1 {
        return vec![e * g[0]];
    }
    let mu = e / (2. * (1. + nu));
    let lambda = if dim == 2 && plane_stress {
        nu * e / ((1. + nu) * (1. - nu))
    } else {
        nu * e / ((1. + nu) * (1. - 2. * nu))
    };
    let tr: Real = (0..dim).map(|i| g[i * dim + i]).sum();
    (0..dim * dim)
        .map(|q| {
            let (i, j) = (q / dim, q % dim);
            mu * (g[i * dim + j] + g[j * dim + i]) + if i == j { lambda * tr } else { 0. }
        })
        .collect()
}

#[test]
fn damage_monotone_bounded_and_dissipation_nonnegative() {
    let mut rng = Rng(42);
    for (name, template, peak) in [
        ("marigo", MARIGO, 2e-3),
        ("mazars", MAZARS, 2e-3),
        ("plastic_mazars", PLASTIC_MAZARS, 3e-3),
    ] {
        for dim in [1, 2, 3] {
            for extra in ["", "  Plane_Stress = true\n"] {
                if dim != 2 && !extra.is_empty() {
                    continue;
                }
                let mut p = Point::new(dim, &dat(template, extra), LawOptions::default());
                let base = direction(dim, &mut rng);
                let mut d_prev = 0.;
                let mut damaged = false;
                for g in cyclic(&base, peak, 8) {
                    p.step(&g).unwrap();
                    let d = p.scalar("damage");
                    assert!(
                        d >= d_prev,
                        "{name} {dim}D: damage decreased {d_prev} -> {d}"
                    );
                    assert!((0. ..=1.).contains(&d), "{name} {dim}D: damage {d}");
                    d_prev = d;
                    damaged |= d > 0.;
                    let ediss = p.scalar("damage dissipated energy");
                    let ints = p.scalar("integral of sigma");
                    assert!(
                        ediss >= -1e-9 * ints.abs().max(1.),
                        "{name} {dim}D: negative dissipation {ediss}"
                    );
                    if name == "plastic_mazars" {
                        assert!(p.scalar("plastic_energy") >= -1e-6, "{name}: plastic work");
                    }
                }
                assert!(damaged, "{name} {dim}D{extra}: history must damage");
            }
        }
    }
}

/// After damaging, unloading at frozen damage is secant elastic:
/// `σ = (1 − d)·C:ε`, the damage tangent `(1 − d)·C` is its exact
/// derivative, and the dissipated energy stays constant (trapezoidal work
/// increments of a linear response are exact).
#[test]
fn damage_unloading_is_secant_elastic_with_constant_dissipation() {
    // peaks keep 0 < d < 1 (Marigo saturates at Y ≥ Yd + Sd)
    for (template, peak) in [(MARIGO, 4e-4), (MAZARS, 5e-4)] {
        let dim = 3;
        let mut p = Point::new(dim, &dat(template, ""), LawOptions::default());
        let base = [1., 0.2, -0.1, 0.2, -0.3, 0.05, -0.1, 0.05, 0.4];
        for k in 1..=10 {
            let s = peak * f64::from(k) / 10.;
            p.step(&base.map(|b| b * s)).unwrap();
        }
        let d = p.scalar("damage");
        assert!(d > 0.1 && d < 1.);
        let ediss0 = p.scalar("damage dissipated energy");
        for k in (5..10).rev() {
            let s = peak * f64::from(k) / 10.;
            let g = base.map(|b| b * s);
            p.step(&g).unwrap();
            assert_eq!(p.scalar("damage"), d, "damage frozen while unloading");
            let want: Vec<Real> = elastic(dim, 25e9, 0.2, false, &g)
                .iter()
                .map(|x| (1. - d) * x)
                .collect();
            let sigma = p.stress();
            assert!(max_abs_diff(&sigma, &want) <= 1e-12 * max_abs(&want));
            let ediss = p.scalar("damage dissipated energy");
            assert!(
                (ediss - ediss0).abs() <= 1e-9 * ediss0.abs(),
                "{ediss} vs {ediss0}"
            );
        }
        // tangent = (1 − d) C, the exact derivative of the unloading branch
        let t = p.tangent();
        let mut fd = vec![0.; 36];
        let g0 = base.map(|b| b * peak * 0.5);
        let voigt = [(0, 0), (1, 1), (2, 2), (1, 2), (0, 2), (0, 1)];
        for (n, &(k, l)) in voigt.iter().enumerate() {
            let h = 1e-7;
            let mut gp = g0;
            let mut gm = g0;
            // engineering shear: d(2ε_kl) = h
            if k == l {
                gp[k * 3 + l] += h;
                gm[k * 3 + l] -= h;
            } else {
                gp[k * 3 + l] += h / 2.;
                gp[l * 3 + k] += h / 2.;
                gm[k * 3 + l] -= h / 2.;
                gm[l * 3 + k] -= h / 2.;
            }
            p.eval(&gp).unwrap();
            let sp = p.stress();
            p.eval(&gm).unwrap();
            let sm = p.stress();
            for (m, &(i, j)) in voigt.iter().enumerate() {
                fd[m * 6 + n] = (sp[i * 3 + j] - sm[i * 3 + j]) / (2. * h);
            }
        }
        assert!(max_abs_diff(&t, &fd) <= 1e-6 * max_abs(&t), "tangent vs FD");
    }
}

/// Von Mises equivalent stress of a 3-D stress (row-major).
fn von_mises(s: &[Real]) -> Real {
    let tr = s[0] + s[4] + s[8];
    let dev: Vec<Real> = (0..9)
        .map(|k| s[k] - if k % 4 == 0 { tr / 3. } else { 0. })
        .collect();
    (1.5 * dev.iter().map(|x| x * x).sum::<Real>()).sqrt()
}

#[test]
fn j2_return_mapping_unloading_and_plastic_work() {
    let mut p = Point::new(3, &dat(PLASTIC, ""), LawOptions::default());
    let base = [1., 0.3, -0.2, 0.3, -0.5, 0.1, -0.2, 0.1, 0.2];
    let peak = 5e-3;
    let mut yielded = 0;
    let mut pe_prev = 0.;
    for k in 1..=20 {
        let g = base.map(|b| b * peak * f64::from(k) / 20.);
        p.step(&g).unwrap();
        let r = p.scalar("iso_hardening");
        let s = p.stress();
        let pe = p.scalar("plastic_energy");
        assert!(p.scalar("d_plastic_energy") >= 0., "plastic work increment");
        assert!(pe >= pe_prev);
        pe_prev = pe;
        if r > 0. {
            // on the yield surface: σ_eq = σ_y + r
            let veq = von_mises(&s);
            assert!(
                (veq - (3e8 + r)).abs() <= 1e-11 * veq,
                "{veq} vs {}",
                3e8 + r
            );
            yielded += 1;
        }
    }
    assert!(yielded > 5);
    // elastic unloading: Δσ = C:Δε, no plastic flow
    let ep0 = p.internal("inelastic_strain");
    let r0 = p.scalar("iso_hardening");
    let s0 = p.stress();
    let g_last = base.map(|b| b * peak);
    let g = base.map(|b| b * peak * 0.97);
    p.step(&g).unwrap();
    assert_eq!(p.internal("inelastic_strain"), ep0);
    assert_eq!(p.scalar("iso_hardening"), r0);
    let dg: Vec<Real> = g.iter().zip(&g_last).map(|(a, b)| a - b).collect();
    let ds = elastic(3, 2.1e11, 0.3, false, &dg);
    let got: Vec<Real> = p.stress().iter().zip(&s0).map(|(a, b)| a - b).collect();
    assert!(max_abs_diff(&got, &ds) <= 1e-9 * max_abs(&ds));
    // the (upstream, UD-079) tangent is the elastic one
    let t = p.tangent();
    let (e, nu): (Real, Real) = (2.1e11, 0.3);
    let lambda = nu * e / ((1. + nu) * (1. - 2. * nu));
    let mu = e / (2. * (1. + nu));
    assert_eq!(t[0], lambda + 2. * mu);
    assert_eq!(t[1], lambda);
    assert_eq!(t[3 * 6 + 3], mu);
}

/// UD-137 compat (`J2ReducedReturn::UpstreamTruncated`): upstream's 1-D
/// radial return uses the 3-D formula `Δp = (σ_eq − σ_y − r_prev)/(3μ + h)`
/// with `μ = E/2` (ν = 0) but the 1-D elastic law `σ = σ_tr − E·Δε_p`, so a
/// plastic step ends at `σ = σ_tr − E·Δp` — not on the yield surface
/// `σ_y + r`. Checked step by step on a monotone and a reversed history. The
/// corrected default (`E + h`) is checked in `review_v32_regressions.rs`.
#[test]
fn j2_uniaxial_1d_upstream_return_formula() {
    let mut p = Point::new(1, &dat(PLASTIC, ""), LawOptions::upstream());
    let (e, sy, h): (Real, Real, Real) = (2.1e11, 3e8, 1e9);
    let mu = e / 2.;
    let mut eps_prev = 0.;
    let mut sigma_prev = 0.;
    let mut r_prev = 0.;
    let mut yielded = 0;
    let mut path: Vec<Real> = (1..=50).map(|k| 5e-3 * f64::from(k) / 50.).collect();
    path.extend((1..=100).map(|k| 5e-3 - 1e-2 * f64::from(k) / 100.));
    for eps in path {
        p.step(&[eps]).unwrap();
        let sigma_tr = sigma_prev + e * (eps - eps_prev);
        let f = sigma_tr.abs() - sy - r_prev;
        let s = p.stress()[0];
        let r = p.scalar("iso_hardening");
        if f > 0. {
            let dp = (sigma_tr.abs() - sy - r_prev) / (3. * mu + h);
            let want = sigma_tr - e * dp * sigma_tr.signum();
            assert!((s - want).abs() <= 1e-9 * want.abs(), "{s} vs {want}");
            assert!((r - (r_prev + h * dp)).abs() <= 1e-9 * r.max(1.));
            // not on the yield surface (the defect)
            assert!(s.abs() > sy + r);
            yielded += 1;
        } else {
            assert!((s - sigma_tr).abs() <= 1e-9 * sigma_tr.abs().max(1.));
        }
        eps_prev = eps;
        sigma_prev = s;
        r_prev = r;
    }
    assert!(yielded > 20);
}

/// Drucker–Prager yield function `√(3 J2) + α tr σ − k` (deviator with
/// `tr/dim`).
fn dp_f(dim: usize, alpha: Real, k: Real, s: &[Real]) -> Real {
    let tr: Real = (0..dim).map(|i| s[i * dim + i]).sum();
    #[allow(clippy::cast_precision_loss)]
    let dimf = dim as Real;
    let j2: Real = 0.5
        * (0..dim * dim)
            .map(|q| {
                let v = s[q] - if q % (dim + 1) == 0 { tr / dimf } else { 0. };
                v * v
            })
            .sum::<Real>();
    (3. * j2).sqrt() + alpha * tr - k
}

#[test]
fn drucker_prager_projection_and_unloading() {
    let phi: Real = 30_f64.to_radians();
    let alpha = 6. * phi.sin() / (3. - phi.sin());
    let c = 3e7 * (1. - phi.sin()) / (2. * phi.cos());
    let k = 6. * c * phi.cos() / (3. - phi.sin());
    for dim in [2, 3] {
        let mut p = Point::new(
            dim,
            &dat(DP, "  radial_return = false\n"),
            LawOptions::default(),
        );
        let mut base: Vec<Real> = vec![0.; dim * dim];
        let mut rng = Rng(7);
        // shear-dominated (symmetric), compressive mean strain
        for i in 0..dim {
            base[i * dim + i] = -0.3;
            for j in i + 1..dim {
                let v = if rng.sym() > 0. { 1. } else { -1. };
                base[i * dim + j] = v;
                base[j * dim + i] = v;
            }
        }
        let mut plastic = 0;
        for s in 1..=20 {
            let g: Vec<Real> = base.iter().map(|b| b * 3e-3 * f64::from(s) / 20.).collect();
            p.step(&g).unwrap();
            let ep = p.internal("inelastic_strain");
            if max_abs(&ep) > 0. {
                let f = dp_f(dim, alpha, k, &p.stress());
                assert!(f.abs() <= 1e-6 * k, "{dim}D step {s}: f = {f}");
                plastic += 1;
            }
        }
        assert!(plastic > 3, "{dim}D: the history must yield");
        // unloading is elastic
        let ep0 = p.internal("inelastic_strain");
        let s0 = p.stress();
        let g0: Vec<Real> = base.iter().map(|b| b * 3e-3).collect();
        let g1: Vec<Real> = base.iter().map(|b| b * 2.9e-3).collect();
        p.step(&g1).unwrap();
        assert_eq!(p.internal("inelastic_strain"), ep0);
        let dg: Vec<Real> = g1.iter().zip(&g0).map(|(a, b)| a - b).collect();
        let want = elastic(dim, 3e10, 0.2, false, &dg);
        let got: Vec<Real> = p.stress().iter().zip(&s0).map(|(a, b)| a - b).collect();
        assert!(max_abs_diff(&got, &want) <= 1e-8 * max_abs(&want));
    }
}

/// UD-145 compat (`DruckerPragerReturn::UpstreamIteration`): upstream's
/// apex projection (hydrostatic tension beyond `k/α`): the trace correction
/// `−(α tr σ − k)/α · I` changes `α tr σ − k` by `−dim·e`, so the iteration
/// stops after one step with the trace overshot to `tr σ = tr σ_tr −
/// dim·(α tr σ_tr − k)/α`. The corrected default returns to the apex
/// `σ = k/(dim·α)·I` (`review_v32_regressions.rs`).
#[test]
fn drucker_prager_apex_projection_matches_upstream_formula() {
    let phi: Real = 30_f64.to_radians();
    let alpha = 6. * phi.sin() / (3. - phi.sin());
    let c = 3e7 * (1. - phi.sin()) / (2. * phi.cos());
    let k = 6. * c * phi.cos() / (3. - phi.sin());
    for dim in [2usize, 3] {
        let mut p = Point::new(dim, &dat(DP, ""), LawOptions::upstream());
        let eps = 1e-3;
        let g: Vec<Real> = (0..dim * dim)
            .map(|q| if q % (dim + 1) == 0 { eps } else { 0. })
            .collect();
        p.step(&g).unwrap();
        let s = p.stress();
        let tr: Real = (0..dim).map(|i| s[i * dim + i]).sum();
        let tr_trial: Real = elastic(dim, 3e10, 0.2, false, &g)
            .iter()
            .enumerate()
            .filter(|(q, _)| q % (dim + 1) == 0)
            .map(|(_, v)| v)
            .sum();
        assert!(tr_trial > k / alpha);
        #[allow(clippy::cast_precision_loss)]
        let want = tr_trial - dim as Real * (alpha * tr_trial - k) / alpha;
        assert!(
            (tr - want).abs() <= 1e-9 * tr_trial,
            "{dim}D: tr {tr} vs {want}"
        );
    }
}

/// UD-038, corrected defaults vs upstream: free thermal expansion (3-D,
/// `∇u = αΔT·I`) and constrained heating (`∇u = 0`).
#[test]
fn ud038_thermal_state_of_inelastic_laws() {
    let (alpha, dt): (Real, Real) = (1e-5, 80.);
    let th = format!("  alpha = {alpha}\n  delta_T = {dt}\n");
    let free = [alpha * dt, 0., 0., 0., alpha * dt, 0., 0., 0., alpha * dt];
    let zero = [0.; 9];
    // damage laws: refreshed σ_th → free expansion stress-free and undamaged
    for template in [MARIGO, MAZARS] {
        let mut p = Point::new(3, &dat(template, &th), LawOptions::default());
        p.step(&free).unwrap();
        let s = p.stress();
        assert!(max_abs(&s) <= 1e-9 * 25e9 * alpha * dt, "default: {s:?}");
        let mut u = Point::new(3, &dat(template, &th), LawOptions::upstream());
        u.step(&free).unwrap();
        // upstream: σ_th stale (0) → the free expansion is loaded like a
        // mechanical strain and damages the material
        assert_eq!(u.scalar("sigma_th"), 0.);
        assert!(u.scalar("damage") > 0.5, "upstream: spurious damage");
    }
    // Marigo: free expansion has zero stored energy ⇒ Y = 0 ⇒ no damage
    let mut p = Point::new(3, &dat(MARIGO, &th), LawOptions::default());
    p.step(&free).unwrap();
    assert!(p.scalar("Y").abs() <= 1e-6);
    assert_eq!(p.scalar("damage"), 0.);
    assert!(p.potential_density().abs() <= 1e-6);
    // plastic laws
    for template in [PLASTIC, DP] {
        let e = if template == PLASTIC { 2.1e11 } else { 3e10 };
        let nu = if template == PLASTIC { 0.3 } else { 0.2 };
        let sigma_th = -e / (1. - 2. * nu) * dt * alpha;
        let mut p = Point::new(3, &dat(template, &th), LawOptions::default());
        p.step(&free).unwrap();
        assert!(
            max_abs(&p.stress()) <= 1e-6 * sigma_th.abs(),
            "free expansion"
        );
        assert!(p.potential_density().abs() <= 1e-9 * (sigma_th * alpha * dt).abs());
        let mut p = Point::new(3, &dat(template, &th), LawOptions::default());
        p.step(&zero).unwrap();
        let want = [sigma_th, 0., 0., 0., sigma_th, 0., 0., 0., sigma_th];
        assert!(
            max_abs_diff(&p.stress(), &want) <= 1e-12 * sigma_th.abs(),
            "constrained"
        );
        assert_eq!(max_abs(&p.internal("inelastic_strain")), 0.);
        // stored energy of the constrained state: (9/2) K (αΔT)²
        let kb = e / (3. * (1. - 2. * nu));
        let w = 4.5 * kb * (alpha * dt).powi(2);
        assert!((p.potential_density() - w).abs() <= 1e-9 * w);
    }
    // upstream J2: trial with the wrong sign, final stress without thermal
    let mut u = Point::new(3, &dat(PLASTIC, &th), LawOptions::upstream());
    u.step(&zero).unwrap();
    assert_eq!(max_abs(&u.stress()), 0.);
    let mut u = Point::new(3, &dat(DP, &th), LawOptions::upstream());
    u.step(&free).unwrap();
    assert!(
        max_abs(&u.stress()) > 1e5,
        "upstream DP misses the thermal increment"
    );
}

#[test]
fn ud030_finite_strain_plasticity_is_unsupported_by_default() {
    for dim in [1, 2, 3] {
        let g = vec![1e-3; dim * dim];
        for template in [PLASTIC, PLASTIC_MAZARS] {
            let d = dat(template, "  finite_deformation = true\n");
            let mut p = Point::new(dim, &d, LawOptions::default());
            let e = p.step(&g).unwrap_err();
            assert!(matches!(e, akantu_core::Error::Unsupported(_)), "{e}");
            let o = LawOptions {
                finite_strain_plasticity: FiniteStrainPlasticity::UpstreamLegacy,
                finite_strain_damage: FiniteStrainDamage::UpstreamLegacy,
                ..LawOptions::default()
            };
            let mut p = Point::new(dim, &d, o);
            p.step(&g).unwrap();
            assert!(p.stress().iter().all(|x| x.is_finite()));
        }
    }
}

/// Upstream parameter names (with kinds/access) and internal names.
#[test]
fn upstream_parameter_and_internal_names() {
    use akantu_parser::param::{ParamKind, ParameterAccess};
    let cases: [(&str, &str, &[&str], &[&str]); 5] = [
        (
            MARIGO,
            "marigo",
            &[
                "E",
                "nu",
                "alpha",
                "delta_T",
                "lambda",
                "mu",
                "kapa",
                "Sd",
                "epsilon_c",
                "Yc limit",
                "damage_in_y",
                "Yd",
            ],
            &[
                "damage",
                "damage dissipated energy",
                "integral of sigma",
                "Y",
                "Yd",
                "sigma_th",
            ],
        ),
        (
            MAZARS,
            "mazars",
            &["E", "nu", "K0", "At", "Ac", "Bc", "Bt", "beta"],
            &[
                "damage",
                "damage dissipated energy",
                "integral of sigma",
                "K0",
                "Ehat",
            ],
        ),
        (
            PLASTIC_MAZARS,
            "plastic_mazars",
            &[
                "E", "nu", "h", "sigma_y", "K0", "At", "Ac", "Bc", "Bt", "beta",
            ],
            &[
                "iso_hardening",
                "inelastic_strain",
                "plastic_energy",
                "d_plastic_energy",
                "damage",
                "K0",
            ],
        ),
        (
            PLASTIC,
            "plastic_linear_isotropic_hardening",
            &["E", "nu", "alpha", "delta_T", "h", "sigma_y"],
            &[
                "iso_hardening",
                "inelastic_strain",
                "plastic_energy",
                "d_plastic_energy",
            ],
        ),
        (
            DP,
            "plastic_drucker_prager",
            &["E", "nu", "h", "sigma_y", "phi", "fc", "radial_return"],
            &[
                "iso_hardening",
                "inelastic_strain",
                "plastic_energy",
                "d_plastic_energy",
            ],
        ),
    ];
    for (template, law_type, params, internals) in cases {
        for dim in [1, 2, 3] {
            let mut p = Point::new(dim, &dat(template, ""), LawOptions::default());
            let law = p.law();
            assert_eq!(law.law_type(), law_type);
            let names = law.param_names();
            for n in params {
                assert!(
                    names.contains(n),
                    "{law_type}: parameter {n} missing ({names:?})"
                );
            }
            assert_eq!(names.contains(&"Plane_Stress"), dim == 2);
            let mut seen = Vec::new();
            law.for_each_internal(&mut |n, _| seen.push(n.to_owned()));
            for n in internals {
                assert!(
                    seen.iter().any(|s| s == n),
                    "{law_type}: internal {n} missing"
                );
            }
        }
    }
    let mut p = Point::new(3, &dat(MARIGO, ""), LawOptions::default());
    let law = p.law();
    assert_eq!(law.param_info("Yd").unwrap().kind, ParamKind::Random);
    assert!(
        !law.param_info("epsilon_c")
            .unwrap()
            .access
            .contains(ParameterAccess::READABLE)
    );
    assert_eq!(law.get_param("Sd").unwrap(), ParamValue::Real(5e3));
    // Yc = ε_c·E·ε_c/2 and the internal flag follow a parameter update
    let mut p = Point::new(
        3,
        &dat(MARIGO, "  epsilon_c = 1e-4\n"),
        LawOptions::default(),
    );
    assert_eq!(
        p.law().get_param("Yc limit").ok(),
        None,
        "internal parameter not readable"
    );
}

#[test]
fn stiffness_flags_and_options() {
    // damage laws: always changed
    let mut p = Point::new(2, &dat(MAZARS, ""), LawOptions::default());
    let _ = p.tangent();
    assert!(p.law().has_stiffness_matrix_changed());
    // J2: assembled until a parameter update
    let mut p = Point::new(2, &dat(PLASTIC, ""), LawOptions::default());
    assert!(p.law().has_stiffness_matrix_changed());
    let _ = p.tangent();
    assert!(!p.law().has_stiffness_matrix_changed());
    p.law().set_param("E", ParamValue::Real(2e11)).unwrap();
    assert!(p.law().has_stiffness_matrix_changed());
    let _ = p.tangent();
    p.law().set_options(LawOptions::upstream()).unwrap();
    assert!(p.law().has_stiffness_matrix_changed());
    assert_eq!(*p.law().options().unwrap(), LawOptions::upstream());
    // wave speeds are the elastic ones
    let mut p = Point::new(3, &dat(PLASTIC, ""), LawOptions::default());
    let (e, nu, rho): (Real, Real, Real) = (2.1e11, 0.3, 7800.);
    let lambda = nu * e / ((1. + nu) * (1. - 2. * nu));
    let mu = e / (2. * (1. + nu));
    assert_eq!(p.celerity(), ((lambda + 2. * mu) / rho).sqrt());
}

/// `plastic_mazars`: the equivalent strain uses `∇u − ε_p`, and the stored
/// gradient is `(∇u − ε_p) + ε_p` (bit for bit as upstream).
#[test]
fn plastic_mazars_uses_the_elastic_strain() {
    // yield before damage: K0 above the yield strain, strong hardening
    let d = dat(PLASTIC_MAZARS, "")
        .replace("  K0 = 1e-4\n", "  K0 = 1.3e-3\n")
        .replace("  h = 2e9\n", "  h = 1e10\n");
    let mut p = Point::new(3, &d, LawOptions::default());
    let base = [1., 0.3, -0.2, 0.3, -0.5, 0.1, -0.2, 0.1, 0.2];
    let peak = 6e-3;
    for k in 1..=20 {
        p.step(&base.map(|b| b * peak * f64::from(k) / 20.))
            .unwrap();
    }
    assert!(p.scalar("damage") > 0., "damage after yield");
    assert!(p.scalar("iso_hardening") > 0.);
    let ep = p.internal("inelastic_strain");
    assert!(max_abs(&ep) > 0.);
    let g = base.map(|b| b * peak);
    let ehat = p.scalar("Ehat");
    // principal strains of ε(∇u − ε_p) (symmetric; ε_p is symmetric)
    let e: Vec<Real> = (0..9)
        .map(|q| {
            let (i, j) = (q / 3, q % 3);
            0.5 * (g[i * 3 + j] + g[j * 3 + i]) - ep[j * 3 + i]
        })
        .collect();
    let m = akantu_core::tensor::MatrixX::from_col_major(3, 3, e).unwrap();
    let w = akantu_linalg::dense::eigh(&m, akantu_linalg::dense::Storage::Dynamic).unwrap();
    let want = w.iter().map(|x| x.max(0.).powi(2)).sum::<Real>().sqrt();
    assert!((ehat - want).abs() <= 1e-12 * want, "{ehat} vs {want}");
    // stored gradient (g − ε_p) + ε_p, column-major comparison
    let stored = p
        .law()
        .base()
        .grad_u
        .get(common::simplex(3), common::NG)
        .unwrap()
        .tuple(0)
        .to_vec();
    for c in 0..3 {
        for r in 0..3 {
            assert_eq!(
                stored[r + 3 * c],
                (g[r * 3 + c] - ep[r + 3 * c]) + ep[r + 3 * c]
            );
        }
    }
}

/// Per-element potential energy (upstream `getPotentialEnergy(element)`,
/// UD-032 handling): the law's own density at the element's
/// points, so it equals the whole-field energy on a one-element mesh.
#[test]
fn per_element_potential_energy_equals_whole_field() {
    let th = "  alpha = 1e-5\n  delta_T = 80\n";
    for (template, extra) in [
        (MARIGO, ""),
        (MAZARS, th),
        (PLASTIC_MAZARS, th),
        (PLASTIC, th),
        (DP, ""),
    ] {
        for dim in [1usize, 2, 3] {
            if template == DP && dim == 1 {
                continue; // 1-D Drucker–Prager is degenerate upstream
            }
            let mut p = Point::new(dim, &dat(template, extra), LawOptions::default());
            // compressive mean strain (a tensile apex state with a deviator
            // makes upstream's apex iteration diverge, see
            // `drucker_prager_apex_with_deviator_does_not_converge`)
            let sign = if template == DP { -1. } else { 1. };
            let g: Vec<Real> = (0..dim * dim)
                .map(|k| sign * 1e-3 * (k as Real + 1.) * if k % 2 == 0 { 1. } else { -0.5 })
                .collect();
            p.step(&g).unwrap();
            let whole = p.energy("potential");
            let ty = common::simplex(dim);
            let model = &p.model;
            let law = p.handler.law_mut(0).unwrap();
            let el = law
                .potential_energy_of_element(
                    &model.ctx(akantu_model::constitutive_law::LawPhase::Energy),
                    akantu_core::Element::new(ty, 0, common::NG),
                )
                .unwrap();
            assert_eq!(el.to_bits(), whole.to_bits(), "{dim}D {}", law.law_type());
        }
    }
}

/// UD-144 (category B): upstream's apex projection
/// (`tr σ ≥ k/α`) iterates `σ ← σ − J2(σ)·dev(σ)⁻¹` with no iteration limit;
/// for a tensile state with a non-zero deviator the iteration does not
/// reach `J2 ≤ 1e-10` and upstream loops forever. In compat mode the port
/// stops (1000 iterations or a non-finite iterate) with `Error::Numerical`;
/// the corrected default returns to the apex `k/(3α)·I`.
#[test]
fn drucker_prager_apex_with_deviator_does_not_converge() {
    let g = [1e-3, -5e-4, 1.5e-3, -5e-4, 2e-3, -1e-3, 1.5e-3, -1e-3, 3e-3];
    let mut p = Point::new(3, &dat(DP, ""), LawOptions::upstream());
    let e = p.step(&g).unwrap_err();
    assert!(
        matches!(&e, akantu_core::Error::Numerical(m) if m.contains("apex")),
        "{e}"
    );
    let mut p = Point::new(3, &dat(DP, ""), LawOptions::default());
    p.step(&g).unwrap();
    let phi: Real = 30_f64.to_radians();
    let alpha = 6. * phi.sin() / (3. - phi.sin());
    let c = 3e7 * (1. - phi.sin()) / (2. * phi.cos());
    let k = 6. * c * phi.cos() / (3. - phi.sin());
    let s = p.stress();
    for (q, v) in s.iter().enumerate() {
        let want = if q % 4 == 0 { k / (3. * alpha) } else { 0. };
        assert!((v - want).abs() <= 1e-9 * k, "{s:?}");
    }
}
