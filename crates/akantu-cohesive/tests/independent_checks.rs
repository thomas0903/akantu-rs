// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Independent checks
// of the cohesive model (not oracle comparisons): closed-form
// traction-separation curves per law, unloading to the origin, dissipated
// energy = G_c × area, energy conservation of a free explicit run after
// insertion, mesh invariants under full insertion (adjacency contract,
// coincident doubled nodes, fragment counts).

mod common;

use akantu_cohesive::{FragmentManager, SolidMechanicsModelCohesive};
use akantu_core::enums::AnalysisMethod;
use akantu_core::{ElementType, GhostType, Real};
use akantu_mesh::utils::{ELEMENT_TO_SUBELEMENT, SUBELEMENT_TO_ELEMENT};
use akantu_mesh::{Mesh, MeshAccessor, SharedMesh};
use akantu_model::ModelOptions;
use akantu_parser::Parser;
use akantu_validation::reduce::NanMinMax;

/// Two unit quadrangles stacked along y (interface at y = 1).
fn two_quads() -> SharedMesh {
    let mut m = Mesh::new(2, "two_quads").unwrap();
    {
        let mut acc = MeshAccessor::new(&mut m);
        for p in [[0., 0.], [1., 0.], [1., 1.], [0., 1.], [0., 2.], [1., 2.]] {
            acc.push_node(&p).unwrap();
        }
        let mut c = acc.connectivity_mut(ElementType::Quadrangle4, GhostType::NotGhost);
        c.try_push_tuple(&[0, 1, 2, 3]).unwrap();
        c.try_push_tuple(&[3, 2, 5, 4]).unwrap();
    }
    SharedMesh::new(m)
}

fn model(dat: &str, extrinsic: bool) -> SolidMechanicsModelCohesive {
    let parser = Parser::parse_str(dat, "material.dat").unwrap();
    let mut m = SolidMechanicsModelCohesive::with_parser(
        two_quads(),
        None,
        "solid_mechanics_model_cohesive",
        parser,
    )
    .unwrap();
    m.ext.inserter.set_limit(1, 0.9, 1.1).unwrap();
    m.init_full(
        &ModelOptions::new(AnalysisMethod::ExplicitLumpedMass),
        extrinsic,
    )
    .unwrap();
    m
}

/// Prescribe a rigid opening `(dx, dy)` of the top element and return the
/// traction (both components) of cohesive quadrature point 0.
fn traction_at(m: &mut SolidMechanicsModelCohesive, dx: Real, dy: Real) -> [Real; 2] {
    let conn = m
        .smm
        .physics
        .base
        .mesh
        .read()
        .unwrap()
        .connectivity(ElementType::Quadrangle4, GhostType::NotGhost)
        .unwrap()
        .tuple(1)
        .to_vec();
    {
        // the doubled interface nodes belong to the top element: its nodes
        // move rigidly, every other node stays
        let mut u_g = m.smm.displacement_mut().unwrap();
        let u = u_g.as_mut_slice();
        u.fill(0.);
        for n in conn {
            u[2 * n as usize] = dx;
            u[2 * n as usize + 1] = dy;
        }
    }
    m.assemble_internal_forces().unwrap();
    let mat = m.cohesive_material("insertion").unwrap();
    let t = mat
        .array("tractions", ElementType::Cohesive2d4, GhostType::NotGhost)
        .unwrap();
    [t.as_slice()[0], t.as_slice()[1]]
}

const BULK: &str =
    "  material elastic [\n    name = body\n    rho = 1\n    E = 1e6\n    nu = 0.\n  ]\n";

/// Closed-form bound of the traction checks below: the
/// tractions are a few operations on O(σc) quantities — the bilinear and
/// linear-family laws ≤ 6 roundings (δ from the interpolated opening, one
/// division by δc or δ_max, one product), the exponential 5 plus `exp` —,
/// so their error is a few ulp of σc. The linear family's worst value
/// (1.42e-15 σ_eff, ≈ 8 ulp) comes from the insertion stress instead: the FE
/// stress interpolated to the facet (a sum over the element's quadrature
/// points) and the effective-stress formula. Measured
/// ≤ 1.42e-15 σ on macOS arm64 and three x86_64 hosts (NUMERICAL_VALIDATION
/// gap fixes); 1e-13 σc is ≥ 70× that and detects a relative formula
/// error of 1e-13 (the former 1e-9 bound would hide e.g. a 1e-10 error in
/// δc,eff).
const CLOSED_FORM: Real = 1e-13;

/// `|value − exact| ≤ CLOSED_FORM · scale`, recording the worst ratio.
fn check_closed_form(what: &str, value: Real, exact: Real, scale: Real, worst: &mut Real) {
    let r = (value - exact).abs() / scale;
    *worst = worst.nan_max(r);
    assert!(
        r <= CLOSED_FORM,
        "{what}: {value:e} vs {exact:e} ({r:e} of the scale {scale:e})"
    );
}

#[test]
fn bilinear_traction_separation_closed_form() {
    let _g = common::lock();
    let (sc, gc, d0) = (10., 0.5, 0.01);
    let dat = format!(
        "{BULK}material cohesive_bilinear [\n  name = insertion\n  beta = 0\n  G_c = {gc}\n  sigma_c = {sc}\n  delta_0 = {d0}\n]\n"
    );
    let mut m = model(&dat, false);
    assert_eq!(
        m.smm
            .physics
            .base
            .mesh
            .read()
            .unwrap()
            .nb_element(ElementType::Cohesive2d4, GhostType::NotGhost),
        1
    );
    let dc = 2. * gc / sc;
    let mut worst: Real = 0.;
    for k in 1..60 {
        let d = dc * 1.1 * Real::from(k) / 60.;
        // |T_y|: the traction acts along the facet normal (−y here)
        let t = traction_at(&mut m, 0., d)[1].abs();
        let exact = if d <= d0 {
            sc * d / d0
        } else if d <= dc {
            sc * (dc - d) / (dc - d0)
        } else {
            0.
        };
        check_closed_form(&format!("δ = {d}"), t, exact, sc, &mut worst);
    }
    eprintln!("bilinear closed form: worst {worst:.2e} σc");
}

#[test]
fn exponential_traction_separation_closed_form() {
    let _g = common::lock();
    let (sc, dc) = (2., 0.5);
    let dat = format!(
        "{BULK}material cohesive_exponential [\n  name = insertion\n  sigma_c = {sc}\n  delta_c = {dc}\n]\n"
    );
    let mut m = model(&dat, false);
    let mut worst: Real = 0.;
    for k in 1..40 {
        let d = 3. * dc * Real::from(k) / 40.;
        let t = traction_at(&mut m, 0., d)[1].abs();
        // monotonic loading: δ_max = δ (history of the previous evaluation)
        let exact = std::f64::consts::E * sc * d / dc * (-d / dc).exp();
        check_closed_form(&format!("δ = {d}"), t, exact, sc, &mut worst);
        // commit δ_max as a converged step would
        m.smm
            .with_material(
                1,
                akantu_model::constitutive_law::LawPhase::AfterSolveStep,
                |law, _| law.save_previous_state(),
            )
            .unwrap();
    }
    eprintln!("exponential closed form: worst {worst:.2e} σc");
}

/// One converged explicit step with every DOF blocked at the rigid opening
/// `(dx, dy)` of the top element (tractions, energies and the committed
/// maximal opening as after a real step; UD-245 lifecycle).
fn blocked_step(m: &mut SolidMechanicsModelCohesive, dx: Real, dy: Real) {
    let conn = m
        .smm
        .physics
        .base
        .mesh
        .read()
        .unwrap()
        .connectivity(ElementType::Quadrangle4, GhostType::NotGhost)
        .unwrap()
        .tuple(1)
        .to_vec();
    {
        let mut u_g = m.smm.displacement_mut().unwrap();
        let u = u_g.as_mut_slice();
        u.fill(0.);
        for n in conn {
            u[2 * n as usize] = dx;
            u[2 * n as usize + 1] = dy;
        }
    }
    m.smm.blocked_dofs_mut().unwrap().fill(true);
    m.solve_step(None).unwrap();
}

/// `cohesive_exponential` (upstream material_cohesive_exponential.cc
/// `computeTraction`, with the committed maximal opening of UD-245/D-052)
/// over load → unload → contact → reload → failure, mode I, both contact
/// penalties. Closed forms per converged step, with K(δ_max) = e σc
/// e^{−δ_max/δc}/δc:
/// * opening δ > 0: T = K(δ_max)·δ, δ_max = max of the converged openings
///   (on the envelope while loading, the secant to the origin below δ_max);
/// * contact δ < 0: T = δ e σc e^{−δ/δc}/δc (`exponential_penalty`) or
///   `contact_tangent`·e σc δ/δc, the cohesive part zero and δ_max kept;
/// * reversible energy ½ T δ, total energy the trapezoid sum
///   Σ ½ (δ_k − δ_{k−1})(T_k + T_{k−1}) (upstream `updateEnergies`),
///   dissipated = total − reversible: unchanged along the secant and over a
///   contact excursion that returns through the same points, growing only on
///   the envelope; at the end of the loading to 6 δc within the derived
///   trapezoid error of the exact work e σc δc[1 − (1 + X)e^{−X}] − ½ T δ.
///
/// Tolerances (PROPOSED): tractions and the reversible energy
/// 1e-13·σc / 1e-13·σc δc — five operations including `exp` (the port's
/// glibc `exp` against the platform's `f64::exp`, ≤ 1 ulp each): a few ulp;
/// total and dissipated energy 1e-13·σc δc — a sum of ≤ 150 terms, each
/// rounded (≤ 150 ε ≈ 3.3e-14 of the sum); `δ_max` bitwise (`max` of the
/// openings the law saw).
#[test]
fn exponential_load_unload_contact_reload_closed_form() {
    let _g = common::lock();
    let (sc, dc) = (2., 0.5);
    let e = std::f64::consts::E;
    let gamma = e * sc * dc;
    let h = 0.025;
    // openings as multiples of h (identical floats on the way back)
    let mut path: Vec<i32> = Vec::new();
    let go = |p: &mut Vec<i32>, to: i32| {
        let from = *p.last().unwrap_or(&0);
        let s = if to > from { 1 } else { -1 };
        let mut i = from;
        while i != to {
            i += s;
            p.push(i);
        }
    };
    for to in [30, 10, 0, -4, 0, 10, 30, 120] {
        go(&mut path, to);
    }
    for (penalty, contact) in [(true, 1.0), (false, 0.5)] {
        let dat = format!(
            "{BULK}material cohesive_exponential [\n  name = insertion\n  sigma_c = {sc}\n  delta_c = {dc}\n  exponential_penalty = {penalty}\n  contact_tangent = {contact}\n]\n"
        );
        let mut m = model(&dat, false);
        m.set_time_step(1e-3, None).unwrap();
        let mut dmax: Real = 0.;
        let (mut prev_d, mut prev_t, mut total) = (0.0, 0.0, 0.0);
        let mut dissipated = Vec::new();
        for &i in &path {
            let target = Real::from(i) * h;
            blocked_step(&mut m, 0., target);
            let mat = m.cohesive_material("insertion").unwrap();
            let get = |n: &str| {
                mat.array(n, ElementType::Cohesive2d4, GhostType::NotGhost)
                    .unwrap()
                    .as_slice()
                    .to_vec()
            };
            let op = get("opening");
            let tr = get("tractions");
            let nrm = get("normal");
            // signed normal opening and traction (n·Δ, n·T)
            let d = op[0] * nrm[0] + op[1] * nrm[1];
            let t = tr[0] * nrm[0] + tr[1] * nrm[1];
            assert!((d - target).abs() <= 1e-15, "opening {d} vs {target}");
            let tol = akantu_core::math::DEFAULT_TOLERANCE;
            let expected = if d < 0. && d.abs() > tol {
                if penalty {
                    d * e * sc * (-d / dc).exp() / dc
                } else {
                    contact * e * sc * d / dc
                }
            } else if d.abs() < tol {
                0.
            } else {
                dmax = dmax.max(d);
                e * sc * (-dmax / dc).exp() / dc * d
            };
            let ctx = format!("penalty {penalty}, δ = {d}");
            assert!(
                (t - expected).abs() <= 1e-13 * sc,
                "{ctx}: T {t:e} vs {expected:e}"
            );
            assert!(
                tr[0].abs() <= 1e-13 * sc,
                "{ctx}: tangential traction {}",
                tr[0]
            );
            assert_eq!(
                get("delta max")[0].to_bits(),
                dmax.to_bits(),
                "{ctx}: δ_max"
            );
            total += 0.5 * (d - prev_d) * (expected + prev_t);
            let rev = 0.5 * expected * d;
            let er = m.energy("reversible").unwrap();
            let ed = m.energy("dissipated").unwrap();
            assert!(
                (er - rev).abs() <= 1e-13 * gamma,
                "{ctx}: reversible {er:e} vs {rev:e}"
            );
            assert!(
                (ed - (total - rev)).abs() <= 1e-13 * gamma,
                "{ctx}: dissipated {ed:e} vs {:e}",
                total - rev
            );
            dissipated.push((i, ed));
            prev_d = d;
            prev_t = expected;
        }
        // dissipation grows only on the envelope (i above every earlier i),
        // is unchanged along the secant and, with the linear contact
        // penalty (stored energy ½ T δ exactly, trapezoid exact), in
        // contact; the exponential penalty's stored energy is not ½ T δ, but
        // the excursion returns through the same points and cancels
        let mut top = 0;
        let mut before_contact = None;
        for w in dissipated.windows(2) {
            let ((ia, a), (i, b)) = (w[0], w[1]);
            if i > top {
                top = i;
                assert!(b > a, "penalty {penalty}: loading at {i}: {b:e} ≤ {a:e}");
            } else if i < 0 || (ia < 0 && penalty) {
                if ia == 0 {
                    before_contact = Some(a);
                }
                if !penalty {
                    assert!(
                        (b - a).abs() <= 1e-13 * gamma,
                        "linear contact at {i}h: {a:e} → {b:e}"
                    );
                }
            } else {
                assert!(
                    (b - a).abs() <= 1e-13 * gamma,
                    "penalty {penalty}: δ = {i}h below δ_max: dissipation {a:e} → {b:e}"
                );
            }
            if i == 0 && ia < 0 {
                let a0 = before_contact.expect("contact entered from 0");
                assert!(
                    (b - a0).abs() <= 1e-13 * gamma,
                    "after the contact excursion: {a0:e} → {b:e}"
                );
            }
        }
        // the end of the loading: the trapezoid rule of the work against the
        // exact integral; f = K δ e^{−δ/δc} has |f''| ≤ 2 e σc/δc², so the
        // trapezoid error over [0, D] with step h is ≤ D h² /12 · 2 e σc/δc²
        let big_d = 120. * h;
        let x = big_d / dc;
        let t_end = e * sc * x * (-x).exp();
        let exact = gamma * (1. - (1. + x) * (-x).exp()) - 0.5 * t_end * big_d;
        let bound = big_d * h * h / 12. * 2. * e * sc / (dc * dc);
        let ed = dissipated.last().unwrap().1;
        assert!(
            (ed - exact).abs() <= bound,
            "penalty {penalty}: dissipated {ed:e} vs exact {exact:e} (trapezoid bound {bound:e})"
        );
        eprintln!(
            "exponential (penalty {penalty}): dissipated at 6 δc {ed:.9e}, exact {exact:.9e}, |Δ| = {:.3e} (bound {bound:.3e}); Γ = {gamma:.9e}",
            (ed - exact).abs()
        );
    }
}

/// `cohesive_exponential` in mixed mode (β = 0.5): δ = √(β²Δt² + Δn²),
/// T = K(δ_max)(β² Δt t + Δn n) on opening; in contact (Δn < 0) δ = β|Δt|,
/// the coupled traction K(δ_max) β² Δt t of the tangential opening only
/// plus the normal penalty, δ_max = max(committed, β|Δt|). Same tolerances
/// as above.
#[test]
fn exponential_mixed_mode_and_contact_with_sliding_closed_form() {
    let _g = common::lock();
    let (sc, dc, beta) = (2., 0.5, 0.5);
    let e = std::f64::consts::E;
    let dat = format!(
        "{BULK}material cohesive_exponential [\n  name = insertion\n  sigma_c = {sc}\n  delta_c = {dc}\n  beta = {beta}\n]\n"
    );
    let mut m = model(&dat, false);
    m.set_time_step(1e-3, None).unwrap();
    let mut dmax: Real = 0.;
    // (Δt, Δn): loading, unloading, contact with sliding below and above
    // the committed δ_max, reloading past it
    let path: Vec<(Real, Real)> = vec![
        (0.1, 0.1),
        (0.2, 0.2),
        (0.3, 0.3),
        (0.15, 0.15),
        (0.0, 0.05),
        (0.2, -0.05),
        (0.6, -0.1),
        (1.2, -0.1),
        (0.3, 0.2),
        (0.6, 0.5),
        (0.9, 0.9),
    ];
    for (dt, dn) in path {
        blocked_step(&mut m, dt, dn);
        let mat = m.cohesive_material("insertion").unwrap();
        let get = |n: &str| {
            mat.array(n, ElementType::Cohesive2d4, GhostType::NotGhost)
                .unwrap()
                .as_slice()
                .to_vec()
        };
        let (op, tr, nrm) = (get("opening"), get("tractions"), get("normal"));
        let an = op[0] * nrm[0] + op[1] * nrm[1];
        // tangential component along t = (n_y, −n_x)
        let at = op[0] * nrm[1] - op[1] * nrm[0];
        let tn = tr[0] * nrm[0] + tr[1] * nrm[1];
        let tt = tr[0] * nrm[1] - tr[1] * nrm[0];
        let ctx = format!("(Δt, Δn) = ({dt}, {dn})");
        assert!(
            (an - dn).abs() <= 1e-15 && (at.abs() - dt).abs() <= 1e-15,
            "{ctx}: opening"
        );
        let tol = akantu_core::math::DEFAULT_TOLERANCE;
        let (exp_t, exp_n) = if an < 0. && an.abs() > tol {
            let delta = (at * at).sqrt() * beta;
            let coupled = if delta.abs() < tol {
                0.
            } else {
                dmax = dmax.max(delta);
                e * sc * (-dmax / dc).exp() / dc * beta * beta * at
            };
            (coupled, an * e * sc * (-an / dc).exp() / dc)
        } else {
            let delta = (beta * beta * at * at + an * an).sqrt();
            dmax = dmax.max(delta);
            let k = e * sc * (-dmax / dc).exp() / dc;
            (k * beta * beta * at, k * an)
        };
        assert!(
            (tt - exp_t).abs() <= 1e-13 * sc,
            "{ctx}: T_t {tt:e} vs {exp_t:e}"
        );
        assert!(
            (tn - exp_n).abs() <= 1e-13 * sc,
            "{ctx}: T_n {tn:e} vs {exp_n:e}"
        );
        let dm = get("delta max")[0];
        assert!(
            (dm - dmax).abs() <= 4. * Real::EPSILON * dmax,
            "{ctx}: δ_max {dm:e} vs {dmax:e}"
        );
    }
}

/// Extrinsic linear-family laws: insert by a uniform tensile state above
/// `σc`, then open the interface; `T(δ) = σ_eff (1 − δ/δ_c,eff)`.
fn extrinsic_envelope(law: &str, extra: &str) {
    let sc = 10.;
    let gc = 0.5;
    let dat = format!(
        "{BULK}material {law} [\n  name = insertion\n  beta = 0\n  G_c = {gc}\n  sigma_c = {sc}\n{extra}]\n"
    );
    let mut m = model(&dat, true);
    // uniform strain ε_yy with E ε = 2 σc: facet stress 2 σc > σc
    {
        let x = m
            .smm
            .physics
            .base
            .mesh
            .read()
            .unwrap()
            .nodes()
            .as_slice()
            .to_vec();
        let mut u_g = m.smm.displacement_mut().unwrap();
        let u = u_g.as_mut_slice();
        for n in 0..x.len() / 2 {
            u[2 * n + 1] = 2. * sc / 1e6 * x[2 * n + 1];
        }
    }
    m.assemble_internal_forces().unwrap();
    let inserted = m.check_cohesive_stress().unwrap();
    assert_eq!(inserted, 1, "{law}: one interface facet");
    let (seff, dceff) = {
        let mat = m.cohesive_material("insertion").unwrap();
        let s = mat
            .array("sigma_c_eff", ElementType::Cohesive2d4, GhostType::NotGhost)
            .unwrap()
            .as_slice()[0];
        let d = mat
            .array("delta_c_eff", ElementType::Cohesive2d4, GhostType::NotGhost)
            .unwrap()
            .as_slice()[0];
        (s, d)
    };
    let mut worst: Real = 0.;
    // the insertion stress is the FE stress of the imposed uniform strain
    // (one product E·ε and the facet interpolation: a few ulp)
    check_closed_form(&format!("{law}: σ_eff"), seff, 2. * sc, sc, &mut worst);
    assert!(
        (dceff - 2. * gc / seff).abs() < 1e-15,
        "{law}: δc_eff = {dceff}"
    );
    let mut dmax: Real = 0.;
    // insertion state (δ = 0): the traction is the insertion stress
    let t0 = traction_at(&mut m, 0., 0.)[1].abs();
    check_closed_form(
        &format!("{law}: insertion traction"),
        t0,
        seff,
        seff,
        &mut worst,
    );
    m.smm
        .with_material(
            1,
            akantu_model::constitutive_law::LawPhase::AfterSolveStep,
            |law, _| law.save_previous_state(),
        )
        .unwrap();
    for k in 1..50 {
        let d = dceff * 1.05 * Real::from(k) / 50.;
        let t = traction_at(&mut m, 0., d)[1].abs();
        dmax = dmax.nan_max(d);
        let exact = if d < dceff {
            seff * (1. - d / dceff)
        } else {
            0.
        };
        check_closed_form(&format!("{law}: δ = {d}"), t, exact, seff, &mut worst);
        m.smm
            .with_material(
                1,
                akantu_model::constitutive_law::LawPhase::AfterSolveStep,
                |law, _| law.save_previous_state(),
            )
            .unwrap();
        if k == 20 && law == "cohesive_linear" {
            // unloading/reloading: secant to the origin
            for f in [0.75, 0.5, 0.25] {
                let tu = traction_at(&mut m, 0., f * dmax)[1].abs();
                let ex = seff * (1. - dmax / dceff) * f;
                check_closed_form(&format!("unloading {f}"), tu, ex, seff, &mut worst);
            }
            let _ = traction_at(&mut m, 0., dmax);
        }
    }
    eprintln!("{law} closed form: worst {worst:.2e} σ_eff");
}

#[test]
fn linear_extrinsic_envelope_and_unloading() {
    let _g = common::lock();
    extrinsic_envelope("cohesive_linear", "");
}

#[test]
fn fatigue_monotonic_follows_the_linear_envelope() {
    let _g = common::lock();
    extrinsic_envelope(
        "cohesive_linear_fatigue",
        "  delta_f = 1\n  fatigue_ratio = 1\n",
    );
}

/// State of one quadrature point of `cohesive_linear_fatigue` in mode I
/// (β = 0: δ = Δn ≥ 0), transcribed independently from upstream
/// `MaterialCohesiveLinearFatigue::computeTraction`
/// (material_cohesive_linear_fatigue.cc:150-279, Nguyen–Repetto–Ortiz–
/// Radovitzky 2001, eqs. (2)–(4)) as a scalar recurrence.
#[derive(Clone, Copy, Debug, Default)]
struct Nrort {
    delta_prec: Real,
    k_plus: Real,
    k_minus: Real,
    t_1d: Real,
    delta_max: Real,
    damage: Real,
    normal_regime: bool,
    traction: Real,
    switches: i32,
    delta_dot_prec: Real,
}

impl Nrort {
    /// One evaluation at the opening `delta` (σc,eff `sc`, δc,eff `dc`,
    /// `delta_f`, `fatigue_ratio`, insertion traction `t_ins`, tolerance
    /// `tol` = `Math::getTolerance()`).
    #[allow(clippy::too_many_arguments)] // the law's scalar parameters
    fn step(
        &mut self,
        delta: Real,
        sc: Real,
        dc: Real,
        delta_f: Real,
        ratio: Real,
        t_ins: Real,
        tol: Real,
    ) {
        let eq = |a: Real, b: Real| (a - b).abs() <= tol;
        self.delta_max = delta.max(self.delta_max);
        self.damage = (self.delta_max / dc).min(1.);
        let delta_dot = delta - self.delta_prec;
        if (delta_dot > 0. && self.delta_dot_prec <= 0.)
            || (delta_dot < 0. && self.delta_dot_prec >= 0.)
        {
            self.switches += 1;
        }
        self.delta_dot_prec = delta_dot;
        if eq(self.damage, 1.) {
            self.traction = 0.;
        } else if eq(self.damage, 0.) {
            self.traction = t_ins;
            self.t_1d = sc;
        } else if delta <= tol {
            self.traction = 0.;
        } else if delta_dot.abs() > tol {
            if delta_dot > 0. {
                if !self.normal_regime {
                    // eq. (4): reloading stiffness degrades with the increment
                    self.k_plus *= 1. - delta_dot / delta_f;
                    // eq. (2)
                    self.t_1d += self.k_plus * delta_dot;
                    let max_traction = sc * (1. - delta / dc);
                    let exceeded = self.t_1d > max_traction;
                    if exceeded {
                        self.t_1d = max_traction;
                    }
                    if self.delta_max > ratio * dc {
                        // switch to the linear law, δ_max reset
                        self.delta_max = sc / (self.t_1d / delta + sc / dc);
                        self.damage = (self.delta_max / dc).min(1.);
                        self.k_minus = sc / self.delta_max * (1. - self.damage);
                        self.normal_regime = true;
                    } else {
                        // eq. (3)
                        self.k_minus = self.t_1d / delta;
                        if exceeded {
                            self.k_plus = self.k_minus;
                        }
                    }
                } else {
                    self.k_minus = sc / self.delta_max * (1. - self.damage);
                }
            } else if !self.normal_regime {
                // eq. (4) on unloading
                self.k_plus += (self.k_plus - self.k_minus) * delta_dot / delta_f;
                self.t_1d = self.k_minus * delta;
            }
            self.traction = delta * self.k_minus;
        }
        self.delta_prec = delta;
    }
}

/// `cohesive_linear_fatigue` with the fatigue effect active (fatigue ratio
/// 0.5, δf = 3 δc,eff, `count_switches`): load–unload–reload cycles below
/// the envelope, the switch to the linear law past `fatigue_ratio · δc`,
/// failure. Every evaluation is compared bitwise with the scalar recurrence
/// transcribed from upstream's source (no oracle exists: upstream raises for
/// the 2-D/3-D cohesive types, UD-221), plus the properties of the law:
/// the reloading stiffness K⁺ degrades during every cycle, unloading
/// follows the secant K⁻ to the origin, reloading stays below the envelope
/// σc(1 − δ/δc) and below the unloading branch (dissipative cycles), the
/// regime switch happens once, and `switches` counts the sign changes of
/// the opening rate.
#[test]
fn fatigue_cycles_follow_the_nrort_recurrence() {
    let _g = common::lock();
    let (sc, gc) = (10., 0.5);
    let seff = 2. * sc;
    let dceff = 2. * gc / seff;
    // upstream initMaterial requires δf ≥ δc of the base strength
    // (2 G_c/σc = 2 δc,eff here)
    let delta_f = 1.5 * (2. * gc / sc);
    let ratio = 0.5;
    let dat = format!(
        "{BULK}material cohesive_linear_fatigue [\n  name = insertion\n  beta = 0\n  G_c = {gc}\n  sigma_c = {sc}\n  delta_f = {delta_f}\n  fatigue_ratio = {ratio}\n  count_switches = true\n]\n"
    );
    let mut m = model(&dat, true);
    {
        let x = m
            .smm
            .physics
            .base
            .mesh
            .read()
            .unwrap()
            .nodes()
            .as_slice()
            .to_vec();
        let mut u_g = m.smm.displacement_mut().unwrap();
        let u = u_g.as_mut_slice();
        for n in 0..x.len() / 2 {
            u[2 * n + 1] = 2. * sc / 1e6 * x[2 * n + 1];
        }
    }
    m.assemble_internal_forces().unwrap();
    assert_eq!(m.check_cohesive_stress().unwrap(), 1);
    let ty = ElementType::Cohesive2d4;
    let g = GhostType::NotGhost;
    let tol = akantu_core::math::DEFAULT_TOLERANCE;
    let (sce, dce) = {
        let mat = m.cohesive_material("insertion").unwrap();
        (
            mat.array("sigma_c_eff", ty, g).unwrap().as_slice()[0],
            mat.array("delta_c_eff", ty, g).unwrap().as_slice()[0],
        )
    };
    // insertion at the facet stress 2σc (FE round-off, as in
    // `extrinsic_envelope`); the recurrence below uses the law's values
    assert!((sce - seff).abs() < 1e-12 * seff && (dce - dceff).abs() < 1e-12 * dceff);
    // openings in units of δc,eff: insertion, loading, two cycles below the
    // fatigue threshold (one repeated evaluation: |δ̇| ≤ tol), loading past
    // fatigue_ratio·δc (switch), a cycle of the linear law, failure
    let mut path: Vec<Real> = vec![0.];
    let ramp = |p: &mut Vec<Real>, to: Real, n: usize| {
        let from = *p.last().unwrap();
        for k in 1..=n {
            p.push(from + (to - from) * Real::from(k as u32) / Real::from(n as u32));
        }
    };
    ramp(&mut path, 0.30, 6);
    ramp(&mut path, 0.10, 4);
    ramp(&mut path, 0.30, 4);
    path.push(*path.last().unwrap());
    ramp(&mut path, 0.12, 3);
    ramp(&mut path, 0.45, 6);
    // five cycles between 0.15 and 0.45: K⁺ degrades by ≈ (1 − Δ/δf) per
    // cycle, the peak traction falls below the envelope
    for _ in 0..5 {
        ramp(&mut path, 0.15, 2);
        ramp(&mut path, 0.45, 2);
    }
    // one step past fatigue_ratio·δc below the envelope: the switch resets
    // δ_max to σc/(T_1d/δ + σc/δc) — the point of the envelope on the secant
    // K⁻ = T_1d/δ — which is past δ (= δ only on the envelope)
    ramp(&mut path, 0.51, 1);
    let switch_step = path.len() - 1;
    ramp(&mut path, 0.72, 4);
    ramp(&mut path, 0.20, 4);
    ramp(&mut path, 0.76, 4);
    ramp(&mut path, 1.10, 4);
    let mut expect = Nrort::default();
    let mut history: Vec<(Real, Nrort)> = Vec::new();
    let mut openings: Vec<Real> = Vec::new();
    for &f in &path {
        let t = traction_at(&mut m, 0., f * dceff)[1].abs();
        let mat = m.cohesive_material("insertion").unwrap();
        let get = |n: &str| mat.array(n, ty, g).unwrap().as_slice()[0];
        // the law sees the interpolated opening (rigid motion of the top
        // element: equal to the prescribed one up to the interpolation)
        let delta = mat.array("opening", ty, g).unwrap().as_slice()[1].abs();
        let t_ins = {
            let s_g = mat.array("insertion_stress", ty, g).unwrap();
            let s = s_g.as_slice();
            s[1].abs()
        };
        expect.step(delta, sce, dce, delta_f, ratio, t_ins, tol);
        let ctx = format!("δ = {f} δc ({delta:e})");
        assert_eq!(
            t.to_bits(),
            expect.traction.to_bits(),
            "{ctx}: traction {t:e} vs {:e}",
            expect.traction
        );
        assert_eq!(
            get("K_plus").to_bits(),
            expect.k_plus.to_bits(),
            "{ctx}: K+"
        );
        assert_eq!(
            get("K_minus").to_bits(),
            expect.k_minus.to_bits(),
            "{ctx}: K-"
        );
        assert_eq!(get("T_1d").to_bits(), expect.t_1d.to_bits(), "{ctx}: T_1d");
        assert_eq!(
            get("delta max").to_bits(),
            expect.delta_max.to_bits(),
            "{ctx}: δ_max"
        );
        assert_eq!(
            get("damage").to_bits(),
            expect.damage.to_bits(),
            "{ctx}: damage"
        );
        assert_eq!(
            get("delta_prec").to_bits(),
            delta.to_bits(),
            "{ctx}: δ_prec"
        );
        assert_eq!(
            get("delta_dot_prec").to_bits(),
            expect.delta_dot_prec.to_bits(),
            "{ctx}: δ̇_prec"
        );
        let akantu_cohesive::laws::CohesiveLaw::LinearFatigue(fl) = &mat.law else {
            panic!("fatigue law");
        };
        assert_eq!(
            fl.switches(ty, g).unwrap()[0],
            expect.switches,
            "{ctx}: switches"
        );
        history.push((f, expect));
        openings.push(delta);
    }
    // the fatigue effect is active: K⁺ degrades during each reloading
    // before the switch (0 < K⁺ strictly decreasing over the reloading steps)
    let reload1 = &history[11..15];
    for w in reload1.windows(2) {
        assert!(
            w[1].1.k_plus < w[0].1.k_plus && w[1].1.k_plus > 0.,
            "K+ degrades: {w:?}"
        );
    }
    // reloading stays below the unloading branch at the same opening
    // (unloading 0.30 → 0.10 at K⁻ = T(0.30)/0.30; reloading at 0.20)
    let unload_at = |f: Real| {
        history[7..11]
            .iter()
            .find(|(x, _)| (x - f).abs() < 1e-12)
            .unwrap()
            .1
    };
    let reload_at = |f: Real| {
        history[11..15]
            .iter()
            .find(|(x, _)| (x - f).abs() < 1e-12)
            .unwrap()
            .1
    };
    assert!(
        reload_at(0.20).traction < unload_at(0.20).traction,
        "dissipative cycle"
    );
    // the second peak (0.30 again) is below the first: fatigue softening
    assert!(
        history[14].1.traction < history[6].1.traction,
        "softening over a cycle"
    );
    // the unloading follows the secant to the origin: T/δ constant
    for (f, s) in &history[7..11] {
        assert_eq!(
            s.k_minus.to_bits(),
            history[6].1.k_minus.to_bits(),
            "unloading secant at {f}"
        );
    }
    // the envelope σc(1 − δ/δc) bounds every evaluation (a clamped T_1d is
    // the envelope value itself; T = δ·(T_1d/δ) adds at most a few ulp)
    for ((f, s), d) in history.iter().zip(&openings) {
        let envelope = (sce * (1. - d / dce)).max(0.);
        assert!(
            s.traction <= envelope + 4. * Real::EPSILON * sce,
            "envelope at {f}: {:e} > {envelope:e}",
            s.traction
        );
    }
    // one switch to the linear regime (δ_max > fatigue_ratio·δc), kept;
    // it happens below the envelope, so δ_max is reset past δ
    let first_normal = history.iter().position(|(_, s)| s.normal_regime).unwrap();
    assert_eq!(first_normal, switch_step);
    assert!(history[first_normal].0 > ratio && history[first_normal - 1].0 <= ratio);
    assert!(history[first_normal..].iter().all(|(_, s)| s.normal_regime));
    let at_switch = history[switch_step].1;
    assert!(
        at_switch.delta_max > openings[switch_step] * (1. + 1e-3),
        "δ_max reset {:e} past δ {:e}",
        at_switch.delta_max,
        openings[switch_step]
    );
    // the cycles degrade: each peak at 0.45 is lower than the previous one
    let peaks: Vec<Real> = history
        .iter()
        .filter(|(f, _)| (f - 0.45).abs() < 1e-12)
        .map(|(_, s)| s.traction)
        .collect();
    assert_eq!(peaks.len(), 6);
    for w in peaks.windows(2) {
        assert!(
            w[1] < w[0],
            "fatigue: peak traction {:e} after {:e}",
            w[1],
            w[0]
        );
    }
    assert_eq!(history.last().unwrap().1.traction, 0.0, "failure past δc");
    assert_eq!(history.last().unwrap().1.damage, 1.0);
    // switches: the first loading (δ̇ > 0 after δ̇_prec = 0), then every
    // change of direction: down, up, (repeat: δ̇ = 0, no switch, δ̇_prec = 0)
    // down, up, five cycles (10), down, up — 17
    assert_eq!(history.last().unwrap().1.switches, 17);
}

/// Parameters of one `cohesive_linear_fatigue` evaluation:
/// the law's scalars and its member `delta_f`, which
/// `progressive_delta_f` overwrites with `δ_max` of each point before use.
#[derive(Clone, Copy, Debug)]
struct FatigueLaw {
    beta2_kappa2: Real,
    beta2_kappa: Real,
    penalty: Real,
    contact_after_breaking: bool,
    progressive_delta_f: bool,
    fatigue_ratio: Real,
    delta_f: Real,
    tol: Real,
}

/// State of one quadrature point of `cohesive_linear_fatigue` in 2-D, with
/// the tangential opening, penetration and `progressive_delta_f`: the
/// vector form of [`Nrort`], transcribed independently from upstream
/// `MaterialCohesiveLinearFatigue::computeTraction`
/// (material_cohesive_linear_fatigue.cc:150-285), in upstream's operation
/// order (2-D: `dot` and `norm` sum the two products left to right).
#[derive(Clone, Copy, Debug, Default)]
struct NrortQp {
    delta_prec: Real,
    k_plus: Real,
    k_minus: Real,
    t_1d: Real,
    delta_max: Real,
    damage: Real,
    normal_regime: bool,
    traction: [Real; 2],
    contact_traction: [Real; 2],
    contact_opening: [Real; 2],
    /// The `opening` internal after the evaluation (on penetration upstream
    /// replaces it by its tangential part).
    opening: [Real; 2],
    switches: i32,
    delta_dot_prec: Real,
}

impl FatigueLaw {
    /// One evaluation at the opening `op` (normal `n`, insertion traction
    /// `ins`, σc,eff `sc`, δc,eff `dc`).
    fn eval(
        &mut self,
        q: &mut NrortQp,
        op: [Real; 2],
        n: [Real; 2],
        ins: [Real; 2],
        sc: Real,
        dc: Real,
    ) {
        let tol = self.tol;
        let eq = |a: Real, b: Real| (a - b).abs() <= tol;
        let max = |a: Real, b: Real| if a < b { b } else { a };
        let min = |a: Real, b: Real| if b < a { b } else { a };
        let non = op[0] * n[0] + op[1] * n[1];
        let mut no = [n[0] * non, n[1] * non];
        let to = [op[0] - no[0], op[1] - no[1]];
        let tn = (to[0] * to[0] + to[1] * to[1]).sqrt();
        let mut delta = tn * tn * self.beta2_kappa2;
        let mut penetration = non < -tol;
        if !self.contact_after_breaking && eq(q.damage, 1.) {
            penetration = false;
        }
        q.opening = op;
        if penetration {
            q.contact_traction = [no[0] * self.penalty, no[1] * self.penalty];
            q.contact_opening = no;
            q.opening = to;
            no = [0., 0.];
        } else {
            delta += non * non;
            q.contact_traction = [0., 0.];
            q.contact_opening = [0., 0.];
        }
        delta = delta.sqrt();
        q.delta_max = max(delta, q.delta_max);
        q.damage = min(q.delta_max / dc, 1.);
        let delta_dot = delta - q.delta_prec;
        if (delta_dot > 0. && q.delta_dot_prec <= 0.) || (delta_dot < 0. && q.delta_dot_prec >= 0.)
        {
            q.switches += 1;
        }
        q.delta_dot_prec = delta_dot;
        if self.progressive_delta_f {
            self.delta_f = q.delta_max;
        }
        if eq(q.damage, 1.) {
            q.traction = [0., 0.];
        } else if eq(q.damage, 0.) {
            q.traction = if penetration { [0., 0.] } else { ins };
            q.t_1d = sc;
        } else if delta <= tol {
            q.traction = [0., 0.];
        } else if delta_dot.abs() > tol {
            if delta_dot > 0. {
                if !q.normal_regime {
                    q.k_plus *= 1. - delta_dot / self.delta_f;
                    q.t_1d += q.k_plus * delta_dot;
                    let max_traction = sc * (1. - delta / dc);
                    let exceeded = q.t_1d > max_traction;
                    if exceeded {
                        q.t_1d = max_traction;
                    }
                    if q.delta_max > self.fatigue_ratio * dc {
                        q.delta_max = sc / (q.t_1d / delta + sc / dc);
                        q.damage = min(q.delta_max / dc, 1.);
                        q.k_minus = sc / q.delta_max * (1. - q.damage);
                        q.normal_regime = true;
                    } else {
                        q.k_minus = q.t_1d / delta;
                        if exceeded {
                            q.k_plus = q.k_minus;
                        }
                    }
                } else {
                    q.k_minus = sc / q.delta_max * (1. - q.damage);
                }
            } else if !q.normal_regime {
                q.k_plus += (q.k_plus - q.k_minus) * delta_dot / self.delta_f;
                q.t_1d = q.k_minus * delta;
            }
            for i in 0..2 {
                q.traction[i] = (to[i] * self.beta2_kappa + no[i]) * q.k_minus;
            }
        }
        q.delta_prec = delta;
    }
}

/// One recorded evaluation of [`fatigue_path`].
#[derive(Clone, Copy, Debug)]
struct FatigueStep {
    /// Prescribed `(Δt, Δn)` in units of δc,eff.
    at: (Real, Real),
    /// Normal opening seen by the law.
    an: Real,
    q: NrortQp,
    delta_f: Real,
}

/// Insert one `cohesive_linear_fatigue` element (law parameters `extra`,
/// `beta`, `kappa`) by a uniform tension of 2σc, then prescribe the rigid
/// openings `(Δt, Δn)·δc,eff` of the top element along `path` and compare
/// every internal of every quadrature point after each evaluation bitwise
/// with [`FatigueLaw::eval`]: tractions, contact tractions and openings, the
/// opening, K⁺, K⁻, T_1d, δ_max, damage, δ_prec, the normal-regime flag,
/// the law's `delta_f` and, with `count_switches`, `switches` and `δ̇_prec`.
fn fatigue_path(extra: &str, beta: Real, kappa: Real, path: &[(Real, Real)]) -> Vec<FatigueStep> {
    use akantu_cohesive::laws::CohesiveLaw;
    let (sc, gc, penalty) = (10., 0.5, 1e3);
    let dat = format!(
        "{BULK}material cohesive_linear_fatigue [\n  name = insertion\n  beta = {beta}\n  kappa = {kappa}\n  G_c = {gc}\n  sigma_c = {sc}\n  penalty = {penalty}\n{extra}]\n"
    );
    let inserted = || {
        let mut m = model(&dat, true);
        {
            let x = m
                .smm
                .physics
                .base
                .mesh
                .read()
                .unwrap()
                .nodes()
                .as_slice()
                .to_vec();
            let mut u_g = m.smm.displacement_mut().unwrap();
            let u = u_g.as_mut_slice();
            for n in 0..x.len() / 2 {
                u[2 * n + 1] = 2. * sc / 1e6 * x[2 * n + 1];
            }
        }
        m.assemble_internal_forces().unwrap();
        assert_eq!(m.check_cohesive_stress().unwrap(), 1);
        m
    };
    let ty = ElementType::Cohesive2d4;
    let g = GhostType::NotGhost;
    // orientation: Δn = opening·n for an upward motion of the top element
    // (a throwaway model; the first evaluation computes the normals)
    let up = {
        let mut m0 = inserted();
        let _ = traction_at(&mut m0, 0., 0.);
        let _ = traction_at(&mut m0, 0., 1e-6);
        let mat = m0.cohesive_material("insertion").unwrap();
        let get = |n: &str| mat.array(n, ty, g).unwrap().as_slice().to_vec();
        let (op, n) = (get("opening"), get("normal"));
        (op[0] * n[0] + op[1] * n[1]).signum()
    };
    let mut m = inserted();
    assert_eq!(path[0], (0., 0.), "the path starts at the insertion state");
    let _ = traction_at(&mut m, 0., 0.);
    let (law, sce, dce, nrm, ins, nq) = {
        let mat = m.cohesive_material("insertion").unwrap();
        let get = |n: &str| mat.array(n, ty, g).unwrap().as_slice().to_vec();
        let CohesiveLaw::LinearFatigue(fl) = &mat.law else {
            panic!("fatigue law");
        };
        let sce = get("sigma_c_eff");
        let law = FatigueLaw {
            beta2_kappa2: beta * beta / kappa / kappa,
            beta2_kappa: beta * beta / kappa,
            penalty,
            contact_after_breaking: extra.contains("contact_after_breaking = true"),
            progressive_delta_f: extra.contains("progressive_delta_f = true"),
            fatigue_ratio: fl.fatigue_ratio,
            delta_f: fl.delta_f,
            tol: akantu_core::math::DEFAULT_TOLERANCE,
        };
        let nq = sce.len();
        (
            law,
            sce,
            get("delta_c_eff"),
            get("normal"),
            get("insertion_stress"),
            nq,
        )
    };
    let mut law = law;
    // the facet at y = 1 has the normal (0, ±1) exactly: the normal and
    // tangential parts of the opening are exact; Δn = up·u_y of the top
    for q in 0..nq {
        assert!(
            nrm[2 * q] == 0.0 && nrm[2 * q + 1].abs() == 1.0 && nrm[2 * q + 1] == nrm[1],
            "normal {nrm:?}"
        );
    }
    let s = up;
    let count_switches = extra.contains("count_switches = true");
    let mut state = vec![NrortQp::default(); nq];
    let mut history = Vec::new();
    for (k, &(dt, dn)) in path.iter().enumerate() {
        if k > 0 {
            let _ = traction_at(&mut m, dt * dce[0], s * dn * dce[0]);
        }
        let mat = m.cohesive_material("insertion").unwrap();
        let get = |n: &str| mat.array(n, ty, g).unwrap().as_slice().to_vec();
        let CohesiveLaw::LinearFatigue(fl) = &mat.law else {
            panic!("fatigue law");
        };
        let (op_after, co_after) = (get("opening"), get("contact_opening"));
        let (tr, ct) = (get("tractions"), get("contact_tractions"));
        let (kp, km, t1) = (get("K_plus"), get("K_minus"), get("T_1d"));
        let (dm, da, dp) = (get("delta max"), get("damage"), get("delta_prec"));
        let nreg = fl.normal_regime.get(ty, g).unwrap().as_slice().to_vec();
        let mut an = 0.;
        for q in 0..nq {
            let r = 2 * q..2 * q + 2;
            // the law's input: on penetration the stored opening is the
            // tangential part and the contact opening the normal part
            // (exact sum for n = (0, ±1))
            let op = [
                op_after[2 * q] + co_after[2 * q],
                op_after[2 * q + 1] + co_after[2 * q + 1],
            ];
            let n = [nrm[2 * q], nrm[2 * q + 1]];
            law.eval(
                &mut state[q],
                op,
                n,
                [ins[2 * q], ins[2 * q + 1]],
                sce[q],
                dce[q],
            );
            let e = state[q];
            let ctx = format!("(Δt, Δn) = ({dt}, {dn}) δc, point {q}");
            let bits = |what: &str, ours: &[Real], theirs: &[Real]| {
                assert_eq!(
                    ours.iter().map(|v| v.to_bits()).collect::<Vec<_>>(),
                    theirs.iter().map(|v| v.to_bits()).collect::<Vec<_>>(),
                    "{ctx}: {what} {ours:?} vs upstream transcription {theirs:?}"
                );
            };
            bits("traction", &tr[r.clone()], &e.traction);
            bits("contact traction", &ct[r.clone()], &e.contact_traction);
            bits("contact opening", &co_after[r.clone()], &e.contact_opening);
            bits("opening", &op_after[r.clone()], &e.opening);
            bits("K+", &[kp[q]], &[e.k_plus]);
            bits("K-", &[km[q]], &[e.k_minus]);
            bits("T_1d", &[t1[q]], &[e.t_1d]);
            bits("δ_max", &[dm[q]], &[e.delta_max]);
            bits("damage", &[da[q]], &[e.damage]);
            bits("δ_prec", &[dp[q]], &[e.delta_prec]);
            assert_eq!(nreg[q], e.normal_regime, "{ctx}: normal regime");
            if count_switches {
                bits("δ̇_prec", &[get("delta_dot_prec")[q]], &[e.delta_dot_prec]);
                assert_eq!(
                    fl.switches(ty, g).unwrap()[q],
                    e.switches,
                    "{ctx}: switches"
                );
            }
            an = op[0] * n[0] + op[1] * n[1];
        }
        assert_eq!(
            fl.delta_f.to_bits(),
            law.delta_f.to_bits(),
            "({dt}, {dn}): delta_f {} vs {}",
            fl.delta_f,
            law.delta_f
        );
        history.push(FatigueStep {
            at: (dt, dn),
            an,
            q: state[0],
            delta_f: law.delta_f,
        });
    }
    history
}

/// `cohesive_linear_fatigue` with `progressive_delta_f`
/// (δf follows δ_max), mixed mode (β = 0.8, κ = 2: δ and the traction with
/// the tangential opening, β²/κ² ≠ β²/κ), contact with sliding before
/// failure (contact traction, the opening replaced by its tangential part),
/// cycles, the switch to the linear law, failure and compression after
/// failure without `contact_after_breaking` (no contact: the broken element
/// carries nothing). Every evaluation bitwise against the transcription of
/// upstream's source (no oracle: UD-221), plus closed-form properties.
#[test]
fn fatigue_progressive_delta_f_mixed_mode_and_contact_follow_upstream() {
    let _g = common::lock();
    let mut path: Vec<(Real, Real)> = vec![(0., 0.)];
    // mixed loading, unloading
    for k in 1..=4 {
        let x = 0.05 * Real::from(k);
        path.push((x, x));
    }
    path.extend([(0.1, 0.1), (0.05, 0.02)]);
    // contact with sliding
    path.extend([(0.1, -0.02), (0.2, -0.03), (0.25, -0.03), (0.15, -0.01)]);
    // reopening, reloading, cycles below the fatigue threshold
    path.extend([(0.2, 0.1), (0.25, 0.25), (0.3, 0.3)]);
    for _ in 0..3 {
        path.extend([(0.15, 0.15), (0.3, 0.3)]);
    }
    // past fatigue_ratio·δc (δ = 1.077 x for Δt = Δn = x), failure
    path.extend([(0.5, 0.5), (0.6, 0.6), (0.8, 0.8), (1.0, 1.0)]);
    // compression after failure (contact_after_breaking false)
    path.extend([(0.5, -0.05), (0.2, -0.1)]);
    let h = fatigue_path(
        "  progressive_delta_f = true
  count_switches = true
  fatigue_ratio = 0.6
",
        0.8,
        2.,
        &path,
    );
    let (b2k2, b2k): (Real, Real) = (0.8 * 0.8 / 4., 0.8 * 0.8 / 2.);
    assert!((b2k2 - b2k).abs() > 0.1, "β²/κ² ≠ β²/κ");
    // progressive_delta_f: δf is δ_max at every evaluation, except at the
    // switch to the linear law, which resets δ_max after δf was taken (to
    // the envelope point of the secant, past δ)
    let switch = h.iter().position(|st| st.q.normal_regime).unwrap();
    for (k, st) in h.iter().enumerate() {
        if k == switch {
            assert!(
                st.delta_f < st.q.delta_max,
                "{:?}: δ_max reset past δf",
                st.at
            );
        } else {
            assert_eq!(
                st.delta_f.to_bits(),
                st.q.delta_max.to_bits(),
                "{:?}",
                st.at
            );
        }
    }
    // contact before failure: only the penalty acts normally, the traction
    // is tangential (T·n = 0) and the opening holds the tangential part
    let contact: Vec<&FatigueStep> = h
        .iter()
        .filter(|st| st.an < 0. && st.q.damage < 1.)
        .collect();
    assert_eq!(contact.len(), 4, "contact evaluations before failure");
    for st in &contact {
        let (co, ct) = (st.q.contact_opening, st.q.contact_traction);
        assert!(
            co[0] == 0. && co[1].abs() == st.an.abs(),
            "{:?}: contact opening",
            st.at
        );
        assert!(ct[0] == 0. && ct[1] == co[1] * 1e3, "{:?}: penalty", st.at);
        assert_eq!(st.q.opening[1], 0., "{:?}: normal part removed", st.at);
        assert_eq!(st.q.traction[1], 0., "{:?}: traction tangential", st.at);
        assert!(st.q.traction[0] != 0., "{:?}: sliding traction", st.at);
    }
    // the fatigue regime until δ_max passes fatigue_ratio·δc at (0.6, 0.6)
    // (δ = √(β²/κ² + 1)·0.6 δc = 0.646 δc), then the linear law; failure at
    // (1, 1)
    let first_normal = h.iter().position(|st| st.q.normal_regime).unwrap();
    assert_eq!(h[first_normal].at, (0.6, 0.6));
    assert!(h[first_normal..].iter().all(|st| st.q.normal_regime));
    assert_eq!(h[h.len() - 3].at, (1.0, 1.0));
    assert_eq!(h[h.len() - 3].q.damage, 1.);
    // compression after failure without contact_after_breaking: nothing acts
    for st in &h[h.len() - 2..] {
        assert!(st.an < 0.);
        assert_eq!(st.q.traction, [0., 0.]);
        assert_eq!(st.q.contact_traction, [0., 0.]);
    }
}
/// `cohesive_linear_fatigue` entering contact right
/// after the insertion — the just-inserted branch in contact (damage 0:
/// zero traction instead of the insertion stress, T_1d = σc,eff, only the
/// penalty acts) —, then sliding in contact (the first K⁺ is 0: the law
/// starts on the envelope), mixed-mode cycles (β = 0.5, κ = 1), failure,
/// and contact after breaking (`contact_after_breaking = true`: the broken
/// element still carries the penalty, nothing else). Bitwise against the
/// transcription of upstream's source, plus closed-form properties.
#[test]
fn fatigue_just_inserted_contact_and_contact_after_breaking_follow_upstream() {
    let _g = common::lock();
    let mut path: Vec<(Real, Real)> = vec![(0., 0.)];
    // just inserted, in contact, no tangential opening (δ = 0: damage 0)
    path.extend([(0., -0.02), (0., -0.04)]);
    // sliding in contact (damage > 0), then reopening and cycles
    path.extend([(0.05, -0.02), (0.1, -0.02), (0.1, 0.1), (0.3, 0.3)]);
    path.extend([(0.1, 0.05), (0.2, -0.05), (0.3, 0.3), (0.4, 0.4)]);
    path.extend([(0.2, 0.2), (0.45, 0.45)]);
    // failure, then contact after breaking
    path.extend([
        (0.8, 0.8),
        (1.2, 1.2),
        (0.5, -0.05),
        (0.3, -0.1),
        (0.6, 0.2),
    ]);
    let h = fatigue_path(
        "  contact_after_breaking = true\n  delta_f = 0.2\n  fatigue_ratio = 0.8\n",
        0.5,
        1.,
        &path,
    );
    // just inserted in contact: zero traction, T_1d = σc,eff, the penalty
    for st in &h[1..3] {
        assert!(st.an < 0.);
        assert_eq!(st.q.damage, 0., "{:?}", st.at);
        assert_eq!(
            st.q.traction,
            [0., 0.],
            "{:?}: no insertion stress in contact",
            st.at
        );
        assert_eq!(
            st.q.t_1d.to_bits(),
            h[0].q.t_1d.to_bits(),
            "{:?}: T_1d = σc,eff",
            st.at
        );
        let (co, ct) = (st.q.contact_opening, st.q.contact_traction);
        assert!(
            co[0] == 0. && co[1].abs() == st.an.abs() && ct[1] == co[1] * 1e3,
            "{:?}",
            st.at
        );
    }
    // at the insertion state (no opening) the traction is the insertion
    // stress (σc,eff along the normal)
    assert!(h[0].q.traction[1].abs() > 0. && h[0].q.traction[0] == 0.);
    // sliding in contact: the first loading starts with K⁺ = 0, so T_1d is
    // clamped to the envelope and K⁺ = K⁻ (the reset of the article)
    let first = &h[3];
    assert!(first.q.damage > 0. && first.an < 0.);
    assert_eq!(first.q.k_plus.to_bits(), first.q.k_minus.to_bits());
    assert_eq!(
        first.q.traction[1], 0.,
        "tangential traction only in contact"
    );
    // failure, then contact after breaking: the penalty acts, the traction
    // is zero
    let broken: Vec<&FatigueStep> = h.iter().filter(|st| st.q.damage == 1.).collect();
    assert!(broken.len() >= 4);
    let in_contact: Vec<&&FatigueStep> = broken.iter().filter(|st| st.an < 0.).collect();
    assert_eq!(in_contact.len(), 2);
    for st in in_contact {
        assert_eq!(st.q.traction, [0., 0.], "{:?}", st.at);
        assert_eq!(
            st.q.contact_traction[1],
            st.q.contact_opening[1] * 1e3,
            "{:?}",
            st.at
        );
        assert!(
            st.q.contact_traction[1] != 0.,
            "{:?}: contact after breaking",
            st.at
        );
    }
    // delta_f is not progressive here: the given value throughout
    assert!(h.iter().all(|st| st.delta_f == h[0].delta_f));
}

#[test]
fn uncoupled_mode_i_follows_the_linear_envelope() {
    let _g = common::lock();
    extrinsic_envelope("cohesive_linear_uncoupled", "  roughness = 1\n");
}

#[test]
fn friction_without_penetration_is_the_linear_law() {
    let _g = common::lock();
    extrinsic_envelope(
        "cohesive_linear_friction",
        "  mu = 0.5\n  penalty = 1e6\n  penalty_for_friction = 1e5\n",
    );
}

#[test]
fn mesh_invariants_under_full_insertion() {
    let _g = common::lock();
    for (file, dim, ct) in [
        ("_cohesive_2d_4_triangle_3.msh", 2, ElementType::Cohesive2d4),
        (
            "_cohesive_2d_6_quadrangle_8.msh",
            2,
            ElementType::Cohesive2d6,
        ),
        (
            "_cohesive_3d_6_tetrahedron_4.msh",
            3,
            ElementType::Cohesive3d6,
        ),
    ] {
        let mesh = common::read_mesh(&common::cohesive_mesh(file), dim);
        let dat = "material elastic [\n name = body\n rho = 1\n E = 1\n nu = 0.2\n]\nmaterial cohesive_bilinear [\n name = c\n sigma_c = 1\n G_c = 1\n delta_0 = 0.1\n]\n";
        let parser = Parser::parse_str(dat, "m.dat").unwrap();
        let mut model =
            SolidMechanicsModelCohesive::with_parser(mesh.clone(), None, "smmc", parser).unwrap();
        model
            .init_full(
                &ModelOptions::new(AnalysisMethod::ExplicitLumpedMass),
                false,
            )
            .unwrap();
        let m = mesh.read().unwrap();
        m.check_consistency().unwrap();
        akantu_mesh::utils::check_facet_mesh_consistency(&m).unwrap();
        let fm = m.mesh_facets().unwrap();
        // adjacency contract: one row per element of every adjacency block
        let e2s = fm.data().element_lists(ELEMENT_TO_SUBELEMENT).unwrap();
        for (ty, g, rows) in e2s.as_map().iter_all() {
            assert_eq!(
                rows.len(),
                fm.nb_element(ty, g),
                "{file}: element_to_subelement {ty}"
            );
        }
        let s2e = fm.data().element_map(SUBELEMENT_TO_ELEMENT).unwrap();
        for (ty, g, rows) in s2e.as_map().iter_all() {
            let n = if ty.spatial_dimension() == dim {
                m.nb_element(ty, g)
            } else {
                fm.nb_element(ty, g)
            };
            assert_eq!(rows.len(), n, "{file}: subelement_to_element {ty}");
        }
        // every interior facet opened: each bulk element is its own island,
        // the two sides of every cohesive element coincide
        let ncoh = m.nb_element(ct, GhostType::NotGhost);
        assert!(ncoh > 0);
        let conn = m.connectivity(ct, GhostType::NotGhost).unwrap();
        let half = ct.nb_nodes_per_element() / 2;
        for row in conn.as_slice().chunks_exact(2 * half) {
            for k in 0..half {
                let (a, b) = (row[k] as usize, row[k + half] as usize);
                assert_ne!(a, b);
                assert_eq!(m.nodes().tuple(a), m.nodes().tuple(b));
            }
        }
        drop(conn);
        drop(m);
        let mut frag = FragmentManager::new(&model).unwrap();
        frag.build_fragments(&mut model, 1.).unwrap();
        assert_eq!(
            frag.nb_fragment(),
            1,
            "{file}: undamaged cohesive elements connect"
        );
        // every cohesive element broken: one fragment per bulk element
        frag.build_fragments(&mut model, 0.).unwrap();
        let nbulk: usize = {
            let m = mesh.read().unwrap();
            m.element_types(
                akantu_core::TypeFilter::new()
                    .dim(dim)
                    .kind(akantu_core::ElementKind::Regular),
            )
            .map(|t| m.nb_element(t, GhostType::NotGhost))
            .sum()
        };
        assert_eq!(frag.nb_fragment(), nbulk, "{file}: fully broken");
        let total: Real = (0..frag.nb_fragment()).map(|k| frag.mass[k * dim]).sum();
        let mut f1 = FragmentManager::new(&model).unwrap();
        f1.build_fragments(&mut model, 1.).unwrap();
        assert!(
            (total - f1.mass[0]).abs() <= 1e-12 * f1.mass[0],
            "{file}: mass conservation"
        );
    }
}

#[test]
fn energy_is_conserved_in_a_free_run_after_insertion() {
    let _g = common::lock();
    let mut f = common::Fixture::create(
        "_cohesive_2d_4_triangle_3.msh",
        2,
        ElementType::Cohesive2d4,
        common::MATERIAL_0,
        true,
    );
    let strain = f.mode_i_strain();
    let init: Vec<Real> = strain.iter().map(|s| s * (1. - 1e-5)).collect();
    f.set_initial_condition(&init);
    let inc: Vec<Real> = strain.iter().map(|s| s * 2e-2).collect();
    let n = f.nb_steps;
    f.nb_steps = n / 2;
    f.steps(&inc, |_, _, _| {});
    assert!(f.nb_cohesive() > 0);
    // free run: boundary values frozen, no external work
    let mut energies = Vec::new();
    let mut shadow = Vec::new();
    let mut inserted = 0;
    let dt = f.model.smm.time_step(None).unwrap();
    for _ in 0..400 {
        inserted += f.model.check_cohesive_stress().unwrap();
        f.model.solve_step(None).unwrap();
        let e: Real = ["kinetic", "potential", "dissipated", "reversible"]
            .iter()
            .map(|id| f.model.energy(id).unwrap())
            .sum();
        energies.push(e);
        // central-difference shadow term (Δt²/8) aᵀ M a (lumped M)
        let a_g = f.model.smm.acceleration().unwrap();
        let a = a_g.as_slice();
        let m_g = f.model.smm.mass().unwrap();
        let m = m_g.as_slice();
        let s: Real = a.iter().zip(m).map(|(a, m)| m * a * a).sum::<Real>() * dt * dt / 8.;
        shadow.push(s);
    }
    let e0 = energies[0];
    // NaN-propagating reductions: a NaN energy at
    // any step makes the three maxima NaN and fails the assertions below.
    let drift = energies
        .iter()
        .map(|e| (e - e0).abs())
        .fold(0.0, NanMinMax::nan_max)
        / e0;
    let smax = shadow.iter().copied().fold(0.0, NanMinMax::nan_max) / e0;
    let corrected = energies
        .iter()
        .zip(&shadow)
        .map(|(e, s)| ((e - s) - (e0 - shadow[0])).abs())
        .fold(0.0, NanMinMax::nan_max)
        / e0;
    eprintln!(
        "free run: relative drift {drift:.3e}; max (Δt²/8)aᵀMa / E0 {smax:.3e}; drift of E − (Δt²/8)aᵀMa {corrected:.3e}; insertions during the free run {inserted}"
    );
    // Derived bound (replaces the former 5e-3). Lumped
    // central differences, M a_n = −f(u_n) on the free DOFs (blocked DOFs
    // do no work): with the internal work W_n measured by the trapezoid rule
    // of the same forces — the bulk ½uᵀKu (linear, exact) and the cohesive
    // total energy Σ ½(T_k + T_{k+1})·(δ_{k+1} − δ_k) (`updateEnergies`;
    // f_coh is linear in T and δ linear in u) — the quantity
    // I_n = ½|v_n|²_M − (Δt²/8)|a_n|²_M + W_n is conserved exactly
    // (v_n = v_{n+½} − Δt a_n/2; the identity holds for any f). The
    // measured energy E = I + (Δt²/8)|a|²_M therefore drifts by at most
    // max_n (Δt²/8)|a_n|²_M + (Δt²/8)|a_0|²_M, and I is conserved up to
    // round-off: 400 steps of sums of O(100) terms, ≤ 400·ε·O(10) ≈ 1e-12
    // relative (measured 5.7e-16, drift 3.3e-6 = the shadow term). The
    // identity holds while no element is inserted (an insertion starts the
    // new element's trapezoid from a zero previous traction): the scenario
    // inserts none during the free run (asserted).
    assert_eq!(inserted, 0, "the free run inserts no element");
    assert!(
        corrected <= 1e-12,
        "E − (Δt²/8)aᵀMa not conserved: {corrected:e}"
    );
    let bound = smax + shadow[0] / e0 + 1e-12;
    assert!(
        drift <= bound,
        "relative energy drift {drift:e} > {bound:e}"
    );
}
