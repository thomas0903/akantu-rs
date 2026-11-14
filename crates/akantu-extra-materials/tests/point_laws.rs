// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): independent checks of extra_packages/
// extra-materials/src/material_damage/material_damage_linear*,
// material_brittle*, material_vreepeerlings*, material_plastic/
// material_viscoplastic*, material_viscoelastic/material_stiffness_proportional*.

//! Independent checks (D-007: no oracle) of the point laws of the
//! extra-materials package, driven through `SolidMechanicsModel` on the
//! two-triangle unit square of `two_elements.msh` with prescribed nodal
//! displacements and velocities. Expected values are closed forms written
//! here from the documented formulas, not calls into the laws.
//!
//! Tolerance: stresses of an affine displacement field are compared with a
//! relative tolerance of 1e-12 (rule XMAT-T1 in
//! `docs/NUMERICAL_VALIDATION.md`, PROPOSED: the FE gradient of an affine
//! nodal field reproduces it up to rounding of the shape-function
//! derivatives, a few ulps of the entries).

mod common;

use akantu_core::enums::AnalysisMethod;
use akantu_core::{ElementType, GhostType, Real};
use akantu_extra_materials::damage_linear::MaterialDamageLinear;
use akantu_extra_materials::{ExtraLawOptions, brittle::MaterialBrittle};
use akantu_extra_materials::{
    stiffness_proportional::MaterialStiffnessProportional, viscoplastic::MaterialViscoPlastic,
    vreepeerlings::MaterialVreePeerlings,
};
use akantu_model::ModelOptions;
use akantu_model::constitutive_law::ConstitutiveLaw;
use akantu_solid::SolidMechanicsModel;
use common::{data, model_with, model_with_options, read_mesh, with_law};

const T3: ElementType = ElementType::Triangle3;
const NG: GhostType = GhostType::NotGhost;
const TOL: Real = 1e-12;

fn explicit_model(dat: &str) -> SolidMechanicsModel {
    let mesh = read_mesh(&data("test_material_damage/two_elements.msh"), 2);
    let mut model = model_with(mesh, dat);
    model
        .init_full(&ModelOptions::new(AnalysisMethod::ExplicitLumpedMass))
        .unwrap();
    model
}

fn nodes(model: &SolidMechanicsModel) -> Vec<[Real; 2]> {
    let mesh = model.physics.base.mesh.read().unwrap();
    mesh.nodes()
        .as_slice()
        .as_chunks::<2>()
        .0
        .iter()
        .map(|c| [c[0], c[1]])
        .collect()
}

/// `u = H·X` (`h` row-major `[[h00, h01], [h10, h11]]`), plus `extra` on
/// node `n` when given.
fn set_displacement(
    model: &mut SolidMechanicsModel,
    h: [[Real; 2]; 2],
    extra: Option<(usize, [Real; 2])>,
) {
    let x = nodes(model);
    let mut u = model.displacement_mut().unwrap();
    for (n, p) in x.iter().enumerate() {
        let mut v = [
            h[0][0] * p[0] + h[0][1] * p[1],
            h[1][0] * p[0] + h[1][1] * p[1],
        ];
        if let Some((m, d)) = extra
            && m == n
        {
            v[0] += d[0];
            v[1] += d[1];
        }
        u.as_mut_slice()[2 * n..2 * n + 2].copy_from_slice(&v);
    }
}

fn set_velocity(model: &mut SolidMechanicsModel, l: [[Real; 2]; 2]) {
    let x = nodes(model);
    let mut v = model.velocity_mut().unwrap();
    for (n, p) in x.iter().enumerate() {
        v.as_mut_slice()[2 * n] = l[0][0] * p[0] + l[0][1] * p[1];
        v.as_mut_slice()[2 * n + 1] = l[1][0] * p[0] + l[1][1] * p[1];
    }
}

/// Stress of material 0, per point `[s00, s10, s01, s11]` (column-major).
fn stress(model: &SolidMechanicsModel) -> Vec<[Real; 4]> {
    model
        .material(0)
        .unwrap()
        .base()
        .stress
        .get(T3, NG)
        .unwrap()
        .as_slice()
        .as_chunks::<4>()
        .0
        .iter()
        .map(|c| [c[0], c[1], c[2], c[3]])
        .collect()
}

fn close(a: Real, b: Real, tol: Real) -> bool {
    (a - b).abs() <= tol * a.abs().max(b.abs()).max(Real::MIN_POSITIVE)
}

fn assert_close(what: &str, got: Real, want: Real, tol: Real) {
    assert!(close(got, want, tol), "{what}: got {got:e}, want {want:e}");
}

/// Plane-strain Lamé constants.
fn lame(e: Real, nu: Real) -> (Real, Real) {
    (nu * e / ((1. + nu) * (1. - 2. * nu)), e / (2. * (1. + nu)))
}

fn law<T: 'static>(model: &SolidMechanicsModel) -> &T {
    model
        .material(0)
        .unwrap()
        .as_any()
        .unwrap()
        .downcast_ref::<T>()
        .unwrap()
}

fn set_extra(model: &mut SolidMechanicsModel, o: ExtraLawOptions) {
    with_law(model, 0, |l, _| {
        let any = l.as_any_mut().unwrap();
        if let Some(m) = any.downcast_mut::<MaterialDamageLinear<2>>() {
            m.extra_options = o;
        } else if let Some(m) = any.downcast_mut::<MaterialBrittle<2>>() {
            m.extra_options = o;
        } else if let Some(m) = any.downcast_mut::<MaterialVreePeerlings<2>>() {
            m.extra_options = o;
        } else if let Some(m) = any.downcast_mut::<MaterialViscoPlastic<2>>() {
            m.extra_options = o;
        } else {
            panic!("no extra options");
        }
        Ok(())
    });
}

// ---------------------------------------------------------------- damage_linear

const LINEAR: &str =
    "material damage_linear [\n name = m\n rho = 1\n E = 1e9\n nu = 0.2\n Sigc = 1e5\n Gc = 2\n]\n";

/// `damage_linear` input with `ν = 0` (uniaxial strain = uniaxial stress,
/// `σ_xx = E·ε`).
fn linear_nu0(e: Real, sigc: Real, gc: Real) -> String {
    format!(
        "material damage_linear [\n name = m\n rho = 1\n E = {e:e}\n nu = 0\n Sigc = {sigc:e}\n Gc = {gc}\n]\n"
    )
}

/// The bilinear uniaxial stress–strain curve that `Sigc`, `Gc` and `E`
/// define for a linear-softening law (written from its definition, not
/// from the damage formula): elastic up to `(ε_min, Sigc)`, the straight
/// line down to `(ε_max, 0)`, zero beyond.
fn linear_softening_curve(e: Real, sigc: Real, gc: Real, eps: Real) -> Real {
    let eps_min = sigc / e;
    let eps_max = 2. * gc / sigc + eps_min;
    if eps <= eps_min {
        e * eps
    } else if eps < eps_max {
        sigc * (eps_max - eps) / (eps_max - eps_min)
    } else {
        0.
    }
}

/// UD-780: the default `damage_linear` follows the
/// linear softening curve its parameters define. Uniaxial monotonic path
/// with `ν = 0` (grid through `ε_min`, past `ε_max`), at every step:
///
/// * `σ_xx` on the bilinear curve (absolute floor `1e-12·Sigc`, rule
///   XMAT-T1: the curve vanishes at `ε_max`);
/// * peak stress `Sigc` (attained at `ε_min`);
/// * work to failure `∫σ dε = ½·Sigc·ε_max = Gc + Sigc²/(2E)` and
///   softening-branch area `Gc` (trapezoidal rule, exact on the linear
///   pieces since `ε_min` is a grid point: relative `1e-11` covers the
///   rounding of ~500 terms);
/// * the model's own dissipated energy (`MaterialDamage::updateEnergies`
///   bookkeeping, `∫ ½(σ_n + σ_{n−1}):Δε − ψ`, over the unit square) is 0 up
///   to the peak and equals the work to failure once `d = 1`.
#[test]
fn damage_linear_default_is_linear_softening_peak_and_energy() {
    for (e, sigc, gc) in [(1e9, 1e5, 20.), (30e9, 1e5, 2.), (30e9, 3e6, 100.)] {
        let mut model = explicit_model(&linear_nu0(e, sigc, gc));
        let eps_min = sigc / e;
        let eps_max = 2. * gc / sigc + eps_min;
        let (n1, n2, n3) = (50, 400, 20);
        // (strain, on the softening branch); the segment ends are exact
        let mut path: Vec<(Real, bool)> = (1..n1)
            .map(|k| (eps_min * Real::from(k) / Real::from(n1), false))
            .collect();
        path.push((eps_min, false));
        path.extend((1..n2).map(|k| {
            let t = Real::from(k) / Real::from(n2);
            (eps_min + (eps_max - eps_min) * t, true)
        }));
        path.push((eps_max, true));
        path.extend(
            (1..=n3).map(|k| (eps_max * (1. + 0.2 * Real::from(k) / Real::from(n3)), false)),
        );
        let (mut work, mut soft, mut peak) = (0., 0., 0_f64);
        let (mut prev_eps, mut prev_sig) = (0., 0.);
        let mut worst_curve = 0_f64;
        for &(eps, softening) in &path {
            set_displacement(&mut model, [[eps, 0.], [0., 0.]], None);
            model.assemble_internal_forces().unwrap();
            let sig = stress(&model);
            for s in &sig {
                let want = linear_softening_curve(e, sigc, gc, eps);
                worst_curve = worst_curve.max((s[0] - want).abs() / sigc);
                assert!(
                    (s[0] - want).abs() <= 1e-12 * sigc,
                    "E = {e:e}, ε = {eps:e}: σ_xx = {:e}, curve {want:e}",
                    s[0]
                );
                assert!(s[1].abs() <= 1e-12 * sigc && s[3].abs() <= 1e-12 * sigc);
            }
            let s = sig[0][0];
            let dw = 0.5 * (s + prev_sig) * (eps - prev_eps);
            work += dw;
            if softening {
                soft += dw;
            }
            peak = peak.max(s);
            // the damage bookkeeping of the model: updateEnergies, then commit
            with_law(&mut model, 0, |l, ctx| {
                l.update_energies(ctx, T3)?;
                if eps <= eps_min {
                    let d = l.energy_of(ctx, "dissipated")?;
                    assert!(
                        d.abs() <= 1e-12 * sigc * eps_min,
                        "dissipated before the peak: {d:e}"
                    );
                }
                l.save_previous_state()
            });
            (prev_eps, prev_sig) = (eps, s);
        }
        assert_close("peak", peak, sigc, 1e-12);
        let w_fail = gc + sigc * sigc / (2. * e);
        assert_close("½·Sigc·ε_max", 0.5 * sigc * eps_max, w_fail, 1e-14);
        assert_close("work to failure", work, w_fail, 1e-11);
        assert_close("softening area", soft, gc, 1e-11);
        let dissipated = with_law(&mut model, 0, |l, ctx| l.energy_of(ctx, "dissipated"));
        println!(
            "E = {e:e}, Sigc = {sigc:e}, Gc = {gc}: max |σ − curve|/Sigc = {worst_curve:.2e}, \
             peak {:.2e}, work {:.2e}, softening area {:.2e}, dissipated {:.2e} (relative)",
            (peak - sigc).abs() / sigc,
            (work - w_fail).abs() / w_fail,
            (soft - gc).abs() / gc,
            (dissipated - w_fail).abs() / w_fail
        );
        assert_close("dissipated energy (model)", dissipated, w_fail, 1e-11);
        let m: &MaterialDamageLinear<2> = law(&model);
        assert!(
            m.damage
                .damage
                .get(T3, NG)
                .unwrap()
                .as_slice()
                .iter()
                .all(|&d| d == 1.)
        );
    }
}

/// UD-780 compat (`damage_linear_linear_softening = false`): upstream's
/// formula `d = (ε̂ − ε_min)/(ε_max − ε_min)·(ε̂/ε_max)`, checked on the
/// uniaxial path with `ν = 0` against the maximum of its stress curve
/// `σ(ε) = Eε·(1 − (ε − a)ε/((b − a)b))` (`a = ε_min`, `b = ε_max`), found
/// by calculus: `dσ/dε = 0` at `ε* = (2a + √(4a² + 12(b − a)b))/6`. For
/// `E = 1e9, Sigc = 1e5, Gc = 20`: `σ* = 2.1016·Sigc`.
#[test]
fn damage_linear_upstream_formula_peak_closed_form() {
    let (e, sigc, gc) = (1e9, 1e5, 20.);
    let upstream = ExtraLawOptions {
        damage_linear_linear_softening: false,
        ..ExtraLawOptions::default()
    };
    let mut model = explicit_model(&linear_nu0(e, sigc, gc));
    set_extra(&mut model, upstream);
    let (a, b) = (sigc / e, 2. * gc / sigc + sigc / e);
    let eps_star = (2. * a + (4. * a * a + 12. * (b - a) * b).sqrt()) / 6.;
    let sigma_star = e * eps_star * (1. - (eps_star - a) * eps_star / ((b - a) * b));
    assert!(
        (sigma_star / sigc - 2.1016).abs() < 1e-4,
        "{}",
        sigma_star / sigc
    );
    let mut values = Vec::new();
    for eps in [eps_star * (1. - 1e-3), eps_star, eps_star * (1. + 1e-3)] {
        set_displacement(&mut model, [[eps, 0.], [0., 0.]], None);
        model.assemble_internal_forces().unwrap();
        values.push(stress(&model)[0][0]);
    }
    assert_close("σ(ε*)", values[1], sigma_star, 1e-12);
    assert!(values[1] > values[0] && values[1] > values[2], "{values:?}");
    // the corrected law at the same strain is on the softening line
    let mut model = explicit_model(&linear_nu0(e, sigc, gc));
    set_displacement(&mut model, [[eps_star, 0.], [0., 0.]], None);
    model.assemble_internal_forces().unwrap();
    assert_close(
        "σ(ε*) default",
        stress(&model)[0][0],
        linear_softening_curve(e, sigc, gc, eps_star),
        1e-12,
    );
}

/// Default law with `ν = 0.2` (uniaxial strain): the damaged stress is
/// `(1 − d)·C:ε` with the integrity `1 − d = σ_line(ε)/(Eε)` of the
/// softening line (`ε̂ = ε`); unloading below `K` keeps `d` (no healing).
#[test]
fn damage_linear_uniaxial_strain_components_and_no_healing() {
    let mut model = explicit_model(LINEAR);
    let (e, nu) = (1e9, 0.2);
    let (lambda, mu) = lame(e, nu);
    // ε = 1.2e-4 > ε_min = 1e-4: ε̂ = ε (principal strains ε, 0, 0)
    let eps = 1.2e-4;
    set_displacement(&mut model, [[eps, 0.], [0., 0.]], None);
    model.assemble_internal_forces().unwrap();
    let integrity = linear_softening_curve(e, 1e5, 2., eps) / (e * eps);
    let d = 1. - integrity;
    for s in stress(&model) {
        assert_close("σ_xx", s[0], integrity * (lambda + 2. * mu) * eps, TOL);
        assert_close("σ_yy", s[3], integrity * lambda * eps, TOL);
        assert!(s[1].abs() < 1e-6 && s[2].abs() < 1e-6);
    }
    let m: &MaterialDamageLinear<2> = law(&model);
    for (&dq, &kq) in m
        .damage
        .damage
        .get(T3, NG)
        .unwrap()
        .as_slice()
        .iter()
        .zip(m.k.get(T3, NG).unwrap().as_slice())
    {
        assert_close("d", dq, d, 1e-11);
        assert_close("K", kq, eps, TOL);
    }
    // unloading below K: same damage, scaled elastic stress
    let eps2 = 1.1e-4;
    set_displacement(&mut model, [[eps2, 0.], [0., 0.]], None);
    model.assemble_internal_forces().unwrap();
    for s in stress(&model) {
        assert_close(
            "σ_xx (unload)",
            s[0],
            integrity * (lambda + 2. * mu) * eps2,
            1e-11,
        );
    }
}

#[test]
fn damage_linear_threshold_per_point_vs_upstream_shared_k() {
    // node 2 (0.5, 0.5) moved: the two triangles get different strains
    let run = |o: ExtraLawOptions| {
        let mut model = explicit_model(LINEAR);
        set_extra(&mut model, o);
        set_displacement(&mut model, [[1.2e-4, 0.], [0., 0.]], Some((2, [1e-5, 0.])));
        model.assemble_internal_forces().unwrap();
        let m: &MaterialDamageLinear<2> = law(&model);
        (
            m.damage.damage.get(T3, NG).unwrap().as_slice().to_vec(),
            m.k.get(T3, NG).unwrap().as_slice().to_vec(),
        )
    };
    let (d_def, k_def) = run(ExtraLawOptions::default());
    let (d_up, k_up) = run(ExtraLawOptions {
        damage_linear_threshold_per_point: false,
        ..ExtraLawOptions::default()
    });
    // default: two different thresholds and damages
    assert_ne!(k_def[0], k_def[1]);
    assert!(d_def[0] > 0. && d_def[1] > 0.);
    // upstream: only K[0] is ever read and written; point 1 compares with
    // the threshold point 0 just raised to ε̂_0
    assert_eq!(k_up[1], 1e5 / 1e9, "K[1] is never touched upstream");
    assert_eq!(d_up[0], d_def[0]);
    if k_def[1] > k_def[0] {
        assert_eq!(d_up[1], d_def[1]);
        assert_eq!(k_up[0], k_def[1]);
    } else {
        assert_eq!(d_up[1], 0.);
        assert_eq!(k_up[0], k_def[0]);
    }
}

#[test]
fn damage_linear_symmetric_strain_vs_raw_gradient() {
    // simple shear ∇u = [[0, γ], [0, 0]]: sym eigenvalues ±γ/2, raw ∇u is
    // nilpotent (eigenvalues 0)
    let gamma = 3e-4;
    let run = |o: ExtraLawOptions| {
        let mut model = explicit_model(LINEAR);
        set_extra(&mut model, o);
        set_displacement(&mut model, [[0., gamma], [0., 0.]], None);
        model.assemble_internal_forces().unwrap();
        let m: &MaterialDamageLinear<2> = law(&model);
        m.k.get(T3, NG).unwrap().as_slice()[0]
    };
    assert_close(
        "ε̂ (sym)",
        run(ExtraLawOptions::default()),
        gamma / 2.,
        1e-10,
    );
    assert_eq!(
        run(ExtraLawOptions::upstream()),
        1e5 / 1e9,
        "raw: ε̂ = 0 ≤ K"
    );
}

// -------------------------------------------------------------------- brittle

fn brittle_dat(e: Real) -> String {
    format!("material brittle [\n name = m\n rho = 1\n E = {e:e}\n nu = 0.25\n]\n")
}

#[test]
fn brittle_equivalent_strain_rate_and_low_rate_fracture() {
    let e = 30e9;
    let (lambda, mu) = lame(e, 0.25);
    // σ_xx = (λ + 2μ)ε straddles S_0 = 157e6
    let eps_crit = 157e6 / (lambda + 2. * mu);
    for (factor, broken) in [(0.99, false), (1.01, true)] {
        let mut model = explicit_model(&brittle_dat(e));
        let eps = factor * eps_crit;
        set_displacement(&mut model, [[eps, 0.], [0., 0.]], None);
        // uniaxial rate r: ε̇_eq = 2r/3 (2-D with the out-of-plane term)
        set_velocity(&mut model, [[1e3, 0.], [0., 0.]]);
        model.assemble_internal_forces().unwrap();
        let m: &MaterialBrittle<2> = law(&model);
        for d in m.damage.damage.get(T3, NG).unwrap().as_slice() {
            assert_eq!(*d, if broken { 1. } else { 0. });
        }
        for s in stress(&model) {
            let want = if broken { 0. } else { (lambda + 2. * mu) * eps };
            assert_close("σ_xx", s[0], want, TOL);
        }
    }
    let r: Real = 1234.5;
    let grad_v = [[r, 0.], [0., 0.]];
    assert_close(
        "ε̇_eq",
        MaterialBrittle::<2>::equivalent_strain_rate(&grad_v),
        2. * r / 3.,
        1e-15,
    );
}

#[test]
fn brittle_high_rate_fracture_stress_polynomial_vs_upstream_constant() {
    let model = explicit_model(&brittle_dat(30e9));
    let m: &MaterialBrittle<2> = law(&model);
    let (a, b, c, d) = (1.622e-5, -1.3274, 3.6544e4, -181.38e6);
    let poly = |x: Real| a * x * x * x + b * x * x + c * x + d;
    let r = 30e3;
    assert_close(
        "fracture stress (polynomial)",
        m.fracture_stress(r),
        poly(r),
        1e-12,
    );
    // continuity with S_0 at E_0 for the code defaults (UD-577)
    assert!(((poly(27e3) - 157e6) / 157e6).abs() < 2e-3);
    let mut up = m.clone();
    up.extra_options = ExtraLawOptions::upstream();
    assert_eq!(up.fracture_stress(r), a);
    assert_eq!(up.fracture_stress(1e3), 157e6);
}

// --------------------------------------------------------------- vreepeerlings

fn vree_dat(extra: &str) -> String {
    format!(
        "material vreepeerlings [\n name = m\n rho = 1\n E = 1e9\n nu = 0.25\n Kct = 1\n {extra}\n]\n"
    )
}

#[test]
fn vreepeerlings_equivalent_strain_rate_and_linear_softening() {
    let nu = 0.25;
    let eps = 1.5e-4 * (1. + nu); // ε̂ = ε/(1 + ν) = 1.5e-4 for k = 1
    let r = 2.;
    // the default `Crate` (1e-4, UD-578): not set in the input
    let mut model = explicit_model(&vree_dat(""));
    set_displacement(&mut model, [[eps, 0.], [0., 0.]], None);
    set_velocity(&mut model, [[r, 0.], [0., 0.]]);
    model.assemble_internal_forces().unwrap();
    let m: &MaterialVreePeerlings<2> = law(&model);
    for (&es, &esr) in m
        .equi_strain
        .get(T3, NG)
        .unwrap()
        .as_slice()
        .iter()
        .zip(m.equi_strain_rate.get(T3, NG).unwrap().as_slice())
    {
        assert_close("ε̂", es, eps / (1. + nu), 1e-12);
        assert_close("ε̂̇", esr, r / (1. + nu), 1e-12);
    }
    // κ₀ = Crate·(1 + 0^1) = 1e-4 (default Crate) = κ_oi → κ_dyn = κ_oi;
    // d = 1 − κ_oi/ε̂·(κ_c − ε̂)/(κ_c − κ_oi)
    let (k0, kc, eh) = (1e-4, 2e-4, 1.5e-4);
    let d = 1. - k0 / eh * ((kc - eh) / (kc - k0));
    for &dq in m.damage.damage.get(T3, NG).unwrap().as_slice() {
        assert_close("d", dq, d, 1e-12);
    }
    for &n in m.number_damage.get(T3, NG).unwrap().as_slice() {
        assert_eq!(n, 1.);
    }
}

/// Damage of a point at rest with `ε̂ = eh` (`k = 1`, `ν = 0.25`: `ε̂ =
/// ε/(1 + ν)` for the uniaxial strain `ε`), first point.
fn vree_damage_at(dat: &str, options: ExtraLawOptions, eh: Real) -> Real {
    let mesh = read_mesh(&data("test_material_damage/two_elements.msh"), 2);
    let mut model = model_with_options(mesh, dat, options);
    model
        .init_full(&ModelOptions::new(AnalysisMethod::ExplicitLumpedMass))
        .unwrap();
    set_displacement(&mut model, [[eh * 1.25, 0.], [0., 0.]], None);
    model.assemble_internal_forces().unwrap();
    let m: &MaterialVreePeerlings<2> = law(&model);
    m.damage.damage.get(T3, NG).unwrap().as_slice()[0]
}

/// The default `Crate` is the documented `0.0001`, so at
/// rest (`ε̂̇ = 0`, `Arate = 0`) `κ₀ = Crate ≤ Kapaoi` and the threshold is
/// `Kapaoi`: no damage below it, `d = 1 − κ_oi/ε̂·(κ_c − ε̂)/(κ_c − κ_oi)`
/// above it, for two values of `Kapaoi` (the damage of the linear
/// softening of the equivalent strain between `Kapaoi` and `Kapac`).
#[test]
fn vreepeerlings_default_crate_keeps_kapaoi_active() {
    let model = explicit_model(&vree_dat(""));
    let m: &MaterialVreePeerlings<2> = law(&model);
    assert_eq!(m.crate_, 1e-4, "documented default");
    let kc = 2e-4;
    for kapaoi in [1e-4, 1.2e-4] {
        let dat = vree_dat(&format!("Kapaoi = {kapaoi:e}"));
        let below = vree_damage_at(&dat, ExtraLawOptions::default(), 0.95 * kapaoi);
        assert_eq!(below, 0., "Kapaoi = {kapaoi:e}: damage below the threshold");
        let eh = 1.5e-4;
        let want = 1. - kapaoi / eh * ((kc - eh) / (kc - kapaoi));
        let got = vree_damage_at(&dat, ExtraLawOptions::default(), eh);
        assert_close(&format!("d (Kapaoi = {kapaoi:e})"), got, want, 1e-11);
    }
    // 2/3 for the defaults
    let got = vree_damage_at(&vree_dat(""), ExtraLawOptions::default(), 1.5e-4);
    assert_close("d (defaults)", got, 2. / 3., 1e-11);
}

/// UD-578 compat through an input file: registered with the upstream
/// switches, the key `Crate` writes `Brate` at parse time and the member
/// `Crate` is the defined value 1, so `κ₀ = 1 ≫ κ_c`, `κ_dyn = 0.99·κ_c`
/// and `Kapaoi` is inert: no damage at `ε̂ = 1.5e-4`
/// (`d₁ = 1 − 1.98/1.5·25 < 0`).
#[test]
fn vreepeerlings_crate_binding_upstream_writes_brate() {
    let eps = 1.5e-4 * 1.25;
    let dat = vree_dat("Crate = 1e-4");
    let mesh = read_mesh(&data("test_material_damage/two_elements.msh"), 2);
    let mut model = model_with_options(mesh, &dat, ExtraLawOptions::upstream());
    model
        .init_full(&ModelOptions::new(AnalysisMethod::ExplicitLumpedMass))
        .unwrap();
    let m: &MaterialVreePeerlings<2> = law(&model);
    assert_eq!((m.brate, m.crate_), (1e-4, 1.));
    set_displacement(&mut model, [[eps, 0.], [0., 0.]], None);
    // loading (the compat mode raises UD-579's error at rest)
    set_velocity(&mut model, [[2., 0.], [0., 0.]]);
    model.assemble_internal_forces().unwrap();
    let m: &MaterialVreePeerlings<2> = law(&model);
    assert!(
        m.damage
            .damage
            .get(T3, NG)
            .unwrap()
            .as_slice()
            .iter()
            .all(|&d| d == 0.)
    );
    // the default registration binds the key to Crate
    let mesh = read_mesh(&data("test_material_damage/two_elements.msh"), 2);
    let mut model = model_with_options(mesh, &dat, ExtraLawOptions::default());
    model
        .init_full(&ModelOptions::new(AnalysisMethod::ExplicitLumpedMass))
        .unwrap();
    let m: &MaterialVreePeerlings<2> = law(&model);
    assert_eq!((m.brate, m.crate_), (1., 1e-4));
}

#[test]
fn vreepeerlings_at_rest_keeps_rate_by_default_and_errors_upstream() {
    let mut model = explicit_model(&vree_dat(""));
    set_displacement(&mut model, [[1e-5, 0.], [0., 0.]], None);
    // zero velocity: I1·İ1 = 0 and J̇2 = 0
    model.assemble_internal_forces().unwrap();
    let m: &MaterialVreePeerlings<2> = law(&model);
    assert!(
        m.equi_strain_rate
            .get(T3, NG)
            .unwrap()
            .as_slice()
            .iter()
            .all(|&r| r == 0.)
    );
    set_extra(&mut model, ExtraLawOptions::upstream());
    let err = model.assemble_internal_forces().unwrap_err();
    assert!(
        err.to_string().contains("commented but has to be checked"),
        "{err}"
    );
}

#[test]
fn vreepeerlings_j2_rate_exact_vs_upstream() {
    let model = explicit_model(&vree_dat(""));
    let m: &MaterialVreePeerlings<2> = law(&model);
    // symmetric gradients: identical bits
    let g = [[1e-4, 2e-5], [2e-5, -3e-5]];
    let v = [[0.5, -0.1], [-0.1, 0.2]];
    let mut up = m.clone();
    up.extra_options = ExtraLawOptions::upstream();
    assert_eq!(m.invariants(&g, &v), up.invariants(&g, &v));
    // J̇2 = ε:ε̇ − I1·İ1/3 for symmetric tensors
    let inv = m.invariants(&g, &v);
    let want = g[0][0] * v[0][0] + g[1][1] * v[1][1] + 2. * g[1][0] * v[1][0]
        - (g[0][0] + g[1][1]) * (v[0][0] + v[1][1]) / 3.;
    assert_close("J̇2", inv.j2_rate, want, 1e-14);
    // non-symmetric: the exact rate uses the symmetric parts
    let g = [[1e-4, 0.], [4e-5, -3e-5]]; // column-major: g10 = 0, g01 = 4e-5
    let v = [[0.5, 0.3], [-0.1, 0.2]];
    assert_ne!(m.invariants(&g, &v).j2_rate, up.invariants(&g, &v).j2_rate);
    // 1-D: J2 = ε²/3, J̇2 = 2εε̇/3 (upstream 5εε̇/6)
    let spec = akantu_model::constitutive_law::LawSpec {
        spatial_dimension: 1,
        option: String::new(),
        id: "m".into(),
    };
    let m1 = MaterialVreePeerlings::<1>::new(&spec, None).unwrap();
    let inv = m1.invariants(&[[3e-4]], &[[2.]]);
    assert_close("J2 (1-D)", inv.j2, 3e-4 * 3e-4 / 3., 1e-14);
    assert_close("J̇2 (1-D)", inv.j2_rate, 2. * 3e-4 * 2. / 3., 1e-14);
    let mut u1 = m1.clone();
    u1.extra_options = ExtraLawOptions::upstream();
    assert_close(
        "J̇2 (1-D upstream)",
        u1.invariants(&[[3e-4]], &[[2.]]).j2_rate,
        5. * 3e-4 * 2. / 6.,
        1e-14,
    );
    let _ = m1.law_type();
}

/// XMAT-007: the rate-dependent threshold of
/// `computeDamageAndStressOnQuad` in closed form, from upstream's source
/// (`κ₀ = C_rate·(1 + (|ε̂̇|·A_rate)^{B_rate})·(1 − r) + r·κ_q`; `κ_dyn =
/// κ_oi` for `κ₀ ≤ κ_oi`, `κ₀` for `κ_oi < κ₀ < 0.99·κ_c`, else
/// `0.99·κ_c`; `d₁ = 1 − κ_dyn/ε̂·(κ_c − ε̂)/(κ_c − κ_dyn)` for
/// `ε̂ > κ_oi`, applied if `d₁ > d`, `d = min(d₁, 1)`, `number-damage += 1`,
/// and at `d ≥ 1` the strain and rate are stored).
#[test]
fn vreepeerlings_rate_dependent_threshold_three_branches_in_closed_form() {
    let model = explicit_model(&vree_dat(""));
    let base: &MaterialVreePeerlings<2> = law(&model);
    let mut m = base.clone();
    m.kapaoi = 1e-4;
    m.kapac = 2e-4;
    m.crate_ = 1e-4;
    m.arate = 1.;
    m.brate = 1.;
    m.kapao_randomness = 0.;
    let d1 = |kdyn: Real, eh: Real| 1. - kdyn / eh * ((2e-4 - eh) / (2e-4 - kdyn));
    let zero = (0., 0., 0.);
    // (a) κ₀ ≤ κ_oi (rate 0): κ_dyn = κ_oi
    let (d, nb, fs, fr) = m.damage_on_quad(0., 1.8e-4, 0., 0., zero);
    assert_close("d (κ_dyn = κ_oi)", d, d1(1e-4, 1.8e-4), 1e-13);
    assert_eq!((nb, fs, fr), (1., 0., 0.));
    // (b) κ_oi < κ₀ < 0.99 κ_c (rate 0.5, A = 1, B = 1: κ₀ = 1.5e-4)
    let (d, nb, ..) = m.damage_on_quad(0., 1.8e-4, 0.5, 0., zero);
    assert_close("d (κ_dyn = κ₀)", d, d1(1.5e-4, 1.8e-4), 1e-13);
    assert_close("d (κ_dyn = κ₀) = 2/3", d, 2. / 3., 1e-13);
    assert_eq!(nb, 1.);
    // the sign of the rate does not matter (|ε̂̇|)
    let (dn, ..) = m.damage_on_quad(0., 1.8e-4, -0.5, 0., zero);
    assert_eq!(dn, d);
    // (c) κ₀ ≥ 0.99 κ_c (rate 2: κ₀ = 3e-4): κ_dyn = 0.99·κ_c = 1.98e-4;
    // the unclamped κ₀ would give d₁ > 1
    let (d, ..) = m.damage_on_quad(0., 1.99e-4, 2., 0., zero);
    assert_close("d (κ_dyn = 0.99 κ_c)", d, d1(1.98e-4, 1.99e-4), 1e-12);
    assert!((d - 0.5025).abs() < 1e-3, "{d}");
    // a high rate suppresses the damage the rest state would produce
    let (d, nb, ..) = m.damage_on_quad(0., 1.8e-4, 2., 0., zero);
    assert_eq!((d, nb), (0., 0.), "d₁ < 0 is not applied");
    // B_rate = 2 (rate 0.5: κ₀ = 1e-4·(1 + 0.25) = 1.25e-4)
    m.brate = 2.;
    let (d, ..) = m.damage_on_quad(0., 1.8e-4, 0.5, 0., zero);
    assert_close("d (B_rate = 2)", d, d1(1.25e-4, 1.8e-4), 1e-13);
    m.brate = 1.;
    // A_rate scales the rate: A = 0.25, rate 2 gives the case (b)
    m.arate = 0.25;
    let (d, ..) = m.damage_on_quad(0., 1.8e-4, 2., 0., zero);
    assert_close("d (A_rate = 0.25)", d, d1(1.5e-4, 1.8e-4), 1e-13);
    m.arate = 1.;
    // Kapao_randomness r = 0.5, κ_q = 1.6e-4, rate 0:
    // κ₀ = 1e-4·1·0.5 + 0.5·1.6e-4 = 1.3e-4
    m.kapao_randomness = 0.5;
    let (d, ..) = m.damage_on_quad(0., 1.8e-4, 0., 1.6e-4, zero);
    assert_close("d (randomness)", d, d1(1.3e-4, 1.8e-4), 1e-13);
    // r = 1: κ₀ = κ_q (here 0.5e-4 ≤ κ_oi → κ_dyn = κ_oi)
    m.kapao_randomness = 1.;
    let (d, ..) = m.damage_on_quad(0., 1.8e-4, 0.5, 0.5e-4, zero);
    assert_close("d (r = 1)", d, d1(1e-4, 1.8e-4), 1e-13);
    m.kapao_randomness = 0.;

    // below κ_oi: nothing happens, nothing is counted
    let (d, nb, fs, fr) = m.damage_on_quad(0.3, 0.9e-4, 0.5, 0., (4., 7., 8.));
    assert_eq!((d, nb, fs, fr), (0.3, 4., 7., 8.));
    // d₁ ≤ d: the damage never heals and is not counted again
    let (d, nb, fs, fr) = m.damage_on_quad(0.95, 1.8e-4, 0., 0., (4., 7., 8.));
    assert_eq!((d, nb, fs, fr), (0.95, 4., 7., 8.));
    // full damage (ε̂ > κ_c): d = min(d₁, 1) = 1, the strain and the rate
    // are stored (rate 0.7: κ₀ = 1.7e-4, d₁ = 2.1333 → 1)
    assert!(d1(1.7e-4, 2.5e-4) > 1.);
    let (d, nb, fs, fr) = m.damage_on_quad(0.2, 2.5e-4, 0.7, 0., (4., 7., 8.));
    assert_eq!((d, nb, fs, fr), (1., 5., 2.5e-4, 0.7));
    // upstream's `dam1 > dam` compares the unclamped value: a point that
    // already has d = 1 is counted and stored again
    let (d, nb, fs, fr) = m.damage_on_quad(1., 2.6e-4, 0.1, 0., (5., 2.5e-4, 0.7));
    assert_eq!((d, nb, fs, fr), (1., 6., 2.6e-4, 0.1));
}

/// End to end: with `Arate = 0.25` the loading rate
/// `ε̂̇ = r/(1 + ν) = 2` of the explicit model raises the threshold to
/// `κ₀ = 1.5e-4` (case (b)): `d = 2/3` at `ε̂ = 1.8e-4`, against `8/9` at
/// rest-threshold `κ_oi` (the same strain with `Arate = 0`).
#[test]
fn vreepeerlings_arate_raises_the_threshold_under_loading() {
    let nu = 0.25;
    let eh = 1.8e-4;
    let run = |extra: &str| {
        let mut model = explicit_model(&vree_dat(extra));
        set_displacement(&mut model, [[eh * (1. + nu), 0.], [0., 0.]], None);
        set_velocity(&mut model, [[2.5, 0.], [0., 0.]]);
        model.assemble_internal_forces().unwrap();
        let m: &MaterialVreePeerlings<2> = law(&model);
        for &es in m.equi_strain.get(T3, NG).unwrap().as_slice() {
            assert_close("ε̂", es, eh, 1e-12);
        }
        for &r in m.equi_strain_rate.get(T3, NG).unwrap().as_slice() {
            assert_close("ε̂̇", r, 2., 1e-12);
        }
        m.damage.damage.get(T3, NG).unwrap().as_slice().to_vec()
    };
    for d in run("Arate = 0.25") {
        assert_close("d (Arate = 0.25)", d, 2. / 3., 1e-12);
    }
    let kc = 2e-4;
    for d in run("") {
        assert_close(
            "d (Arate = 0)",
            d,
            1. - 1e-4 / eh * ((kc - eh) / (kc - 1e-4)),
            1e-12,
        );
    }
}

/// The 3-D terms of `invariants` (`is3D` of upstream's
/// `J̇2`), closed form for a non-symmetric `∇u` and `∇v`: `I1 = tr ∇u`,
/// `J2 = ½ Σ g_ii² + ¼ Σ_{i<j}(g_ij + g_ji)² − I1²/6`, and
/// `J̇2 = Σ g_ii v_ii + 2 (g_01 v_10 + g_12 v_21 + g_20 v_02) − I1 İ1/3`
/// (UD-580 upstream mode; the exact mode pairs the symmetric parts).
#[test]
fn vreepeerlings_invariants_3d_closed_form() {
    let spec = akantu_model::constitutive_law::LawSpec {
        spatial_dimension: 3,
        option: String::new(),
        id: "m".into(),
    };
    let m = MaterialVreePeerlings::<3>::new(&spec, None).unwrap();
    let mut up = m.clone();
    up.extra_options = ExtraLawOptions::upstream();
    // row-major inputs g(i, j), v(i, j); the matrices are stored [col][row]
    let g = [
        [1.0e-4, 2.0e-5, -3.0e-5],
        [-1.0e-5, 4.0e-5, 6.0e-5],
        [5.0e-5, -2.0e-5, -7.0e-5],
    ];
    let v = [[0.5, -0.2, 0.3], [0.1, 0.25, -0.4], [-0.6, 0.7, 0.15]];
    let col_major = |a: &[[Real; 3]; 3]| {
        let mut out = [[0.; 3]; 3];
        for i in 0..3 {
            for j in 0..3 {
                out[j][i] = a[i][j];
            }
        }
        out
    };
    let (gm, vm) = (col_major(&g), col_major(&v));
    let i1 = g[0][0] + g[1][1] + g[2][2];
    let i1_rate = v[0][0] + v[1][1] + v[2][2];
    let mut j2 = 0.;
    let mut shear_sym = 0.;
    for i in 0..3 {
        j2 += 0.5 * g[i][i] * g[i][i];
        for j in (i + 1)..3 {
            j2 += 0.25 * (g[i][j] + g[j][i]) * (g[i][j] + g[j][i]);
            shear_sym += 2. * (0.5 * (g[i][j] + g[j][i])) * (0.5 * (v[i][j] + v[j][i]));
        }
    }
    j2 -= i1 * i1 / 6.;
    let diag = g[0][0] * v[0][0] + g[1][1] * v[1][1] + g[2][2] * v[2][2];
    let rate_up =
        diag + 2. * (g[0][1] * v[1][0] + g[1][2] * v[2][1] + g[2][0] * v[0][2]) - i1 * i1_rate / 3.;
    let rate_exact = diag + shear_sym - i1 * i1_rate / 3.;
    let inv = up.invariants(&gm, &vm);
    assert_close("I1", inv.i1, i1, 1e-14);
    assert_close("İ1", inv.i1_rate, i1_rate, 1e-14);
    assert_close("J2", inv.j2, j2, 1e-12);
    assert_close("J̇2 (upstream)", inv.j2_rate, rate_up, 1e-12);
    let inv = m.invariants(&gm, &vm);
    assert_close("J2 (exact mode)", inv.j2, j2, 1e-12);
    assert_close("J̇2 (exact)", inv.j2_rate, rate_exact, 1e-12);
    assert!(
        (rate_up - rate_exact).abs() > 1e-6,
        "the inputs must differ"
    );
}

// ----------------------------------------------------------- ve_stiffness_prop

#[test]
fn stiffness_proportional_kelvin_voigt_stress_and_elastic_energy() {
    let dat = "material ve_stiffness_prop [\n name = m\n rho = 1\n E = 2e9\n nu = 0.3\n Alpha = 0.01\n]\n";
    let mut model = explicit_model(dat);
    let (lambda, mu) = lame(2e9, 0.3);
    let h = [[1e-4, 3e-5], [3e-5, -2e-5]];
    let l = [[2., -1.], [-1., 0.5]];
    set_displacement(&mut model, h, None);
    set_velocity(&mut model, l);
    model.assemble_internal_forces().unwrap();
    let c = |e: [[Real; 2]; 2]| {
        let tr = e[0][0] + e[1][1];
        [
            lambda * tr + 2. * mu * e[0][0],
            2. * mu * e[1][0],
            2. * mu * e[0][1],
            lambda * tr + 2. * mu * e[1][1],
        ]
    };
    let (se, sv) = (c(h), c(l));
    for s in stress(&model) {
        for k in 0..4 {
            assert_close("σ", s[k], se[k] + 0.01 * sv[k], TOL);
        }
    }
    let m: &MaterialStiffnessProportional<2> = law(&model);
    for el in m
        .stress_elastic
        .get(T3, NG)
        .unwrap()
        .as_slice()
        .as_chunks::<4>()
        .0
        .iter()
    {
        for k in 0..4 {
            assert_close("σ_el", el[k], se[k], TOL);
        }
    }
    // potential energy density ½ σ_el : ∇u, times the square's area 1
    let w = 0.5 * (se[0] * h[0][0] + se[1] * h[1][0] + se[2] * h[0][1] + se[3] * h[1][1]);
    let epot = with_law(&mut model, 0, |l, ctx| l.potential_energy(ctx));
    assert_close("E_pot", epot, w, 1e-12);
}

// -------------------------------------------------------------- visco_plastic

const VISCO: &str = "material visco_plastic [\n name = m\n rho = 1\n E = 1e9\n nu = 0.2\n sigma_y = 1e6\n h = 1e8\n rate = 1\n edot0 = 0.1\n ts = 0.5\n]\n";

#[test]
fn viscoplastic_explicit_update_from_committed_state() {
    let mut model = explicit_model(VISCO);
    let (lambda, mu) = lame(1e9, 0.2);
    let c = |e: [[Real; 2]; 2]| {
        let tr = e[0][0] + e[1][1];
        [
            [lambda * tr + 2. * mu * e[0][0], 2. * mu * e[1][0]],
            [2. * mu * e[0][1], lambda * tr + 2. * mu * e[1][1]],
        ]
    };
    // step 1 from the zero state: σ_in = 0 → Δε_p = 0, elastic
    let h1 = [[1e-3, 0.], [0., 0.]];
    set_displacement(&mut model, h1, None);
    model.assemble_internal_forces().unwrap();
    let s1 = c(h1);
    for s in stress(&model) {
        assert_close("σ_xx (step 1)", s[0], s1[0][0], TOL);
    }
    // commit, step 2: Δε_p = ts·ε̇₀·‖σ₁‖⁰/(σ_y + 0)¹·σ₁ (m = 1)
    with_law(&mut model, 0, |l, _| l.save_previous_state());
    let h2 = [[1.5e-3, 0.], [0., 0.]];
    set_displacement(&mut model, h2, None);
    model.assemble_internal_forces().unwrap();
    let f = 0.5 * 0.1 / 1e6;
    let dep = [[f * s1[0][0], f * s1[1][0]], [f * s1[0][1], f * s1[1][1]]];
    let de = [
        [h2[0][0] - h1[0][0] - dep[0][0], -dep[0][1]],
        [-dep[1][0], h2[1][1] - h1[1][1] - dep[1][1]],
    ];
    // row-major (de[i][j] = Δε_ij) → C
    let cde = c([[de[0][0], de[1][0]], [de[0][1], de[1][1]]]);
    let dep_norm =
        (dep[0][0] * dep[0][0] + dep[1][1] * dep[1][1] + 2. * dep[0][1] * dep[0][1]).sqrt();
    for s in stress(&model) {
        assert_close("σ_xx (step 2)", s[0], s1[0][0] + cde[0][0], 1e-11);
        assert_close("σ_yy (step 2)", s[3], s1[1][1] + cde[1][1], 1e-11);
    }
    let m: &MaterialViscoPlastic<2> = law(&model);
    for &r in m.plastic.iso_hardening.get(T3, NG).unwrap().as_slice() {
        assert_close("r", r, 1e8 * dep_norm, 1e-11);
    }
    // a repeated evaluation of the same step is idempotent (UD-583 default)
    let before = stress(&model);
    model.assemble_internal_forces().unwrap();
    assert_eq!(before, stress(&model));
}

#[test]
fn viscoplastic_upstream_nan_at_zero_stress_and_first_point_previous_stress() {
    // rate = 0 (default): ‖σ‖^{-1} at σ = 0 → NaN upstream, 0 by default
    let dat = "material visco_plastic [\n name = m\n rho = 1\n E = 1e9\n nu = 0.2\n sigma_y = 1e6\n edot0 = 0.1\n ts = 0.5\n]\n";
    let mut model = explicit_model(dat);
    set_displacement(&mut model, [[1e-3, 0.], [0., 0.]], None);
    model.assemble_internal_forces().unwrap();
    assert!(stress(&model).iter().flatten().all(|x| x.is_finite()));
    let mut model = explicit_model(dat);
    set_extra(&mut model, ExtraLawOptions::upstream());
    set_displacement(&mut model, [[1e-3, 0.], [0., 0.]], None);
    model.assemble_internal_forces().unwrap();
    assert!(stress(&model).iter().flatten().all(|x| x.is_nan()));

    // UD-583: different previous stresses per point (non-affine field)
    let run = |o: ExtraLawOptions| {
        let mut model = explicit_model(VISCO);
        set_extra(&mut model, o);
        set_displacement(&mut model, [[1e-3, 0.], [0., 0.]], Some((2, [2e-4, 0.])));
        model.assemble_internal_forces().unwrap();
        with_law(&mut model, 0, |l, _| l.save_previous_state());
        let prev = stress(&model);
        model.assemble_internal_forces().unwrap();
        (prev, stress(&model))
    };
    let (prev, def) = run(ExtraLawOptions {
        viscoplastic_committed_flow: false,
        ..ExtraLawOptions::default()
    });
    assert_ne!(prev[0], prev[1]);
    let (_, up) = run(ExtraLawOptions {
        viscoplastic_committed_flow: false,
        ..ExtraLawOptions::upstream()
    });
    // same first point, the second point starts from the first point's σ_prev
    assert_eq!(def[0], up[0]);
    assert_ne!(def[1], up[1]);
}

/// UD-583 compat: upstream never advances the previous
/// stress **and** previous inelastic-strain iterators, so with the
/// upstream flow (current state, `viscoplastic_committed_flow = false`)
/// point `q` gets `ε_p[q] = ε_p,prev[0] + Δε_p[q]` with `Δε_p[q] =
/// ts·ε̇₀·‖σ[q]‖^{m−1}/(σ_y + r[q])^m·σ[q]` from its own current stress
/// and hardening (the values committed at the last step: one evaluation per
/// step). `Δε_p` is written here from the documented flow rule (`m = 2`:
/// `‖σ‖^1`, `(σ_y + r)^2`), not taken from the law.
#[test]
fn viscoplastic_compat_composes_with_the_first_point_previous_inelastic_strain() {
    let dat = "material visco_plastic [\n name = m\n rho = 1\n E = 1e9\n nu = 0.2\n sigma_y = 1e6\n h = 1e8\n rate = 2\n edot0 = 0.1\n ts = 0.5\n]\n";
    let (ts, edot0, sigma_y) = (0.5, 0.1, 1e6);
    for per_point in [true, false] {
        let mut model = explicit_model(dat);
        set_extra(
            &mut model,
            ExtraLawOptions {
                viscoplastic_committed_flow: false,
                viscoplastic_previous_state_per_point: per_point,
                ..ExtraLawOptions::upstream()
            },
        );
        // non-affine field: the two elements get different stresses and flows
        for k in 1..=3 {
            let s = Real::from(k) * 1e-3;
            set_displacement(&mut model, [[s, 0.], [0., 0.]], Some((2, [0.4 * s, 0.])));
            if k == 3 {
                break;
            }
            model.assemble_internal_forces().unwrap();
            with_law(&mut model, 0, |l, _| l.save_previous_state());
        }
        let (prev_inel, prev_sigma, prev_iso) = {
            let m: &MaterialViscoPlastic<2> = law(&model);
            (
                m.plastic
                    .inelastic_strain
                    .previous(T3, NG)
                    .unwrap()
                    .as_slice()
                    .to_vec(),
                akantu_solid::Material::base(&m.elastic)
                    .stress
                    .get(T3, NG)
                    .unwrap()
                    .as_slice()
                    .to_vec(),
                m.plastic
                    .iso_hardening
                    .get(T3, NG)
                    .unwrap()
                    .as_slice()
                    .to_vec(),
            )
        };
        assert_ne!(
            prev_inel[0..4],
            prev_inel[4..8],
            "need different committed ε_p"
        );
        model.assemble_internal_forces().unwrap();
        let m: &MaterialViscoPlastic<2> = law(&model);
        let cur = m
            .plastic
            .inelastic_strain
            .get(T3, NG)
            .unwrap()
            .as_slice()
            .to_vec();
        for q in 0..2 {
            let sig = &prev_sigma[4 * q..4 * q + 4];
            let norm = sig.iter().map(|x| x * x).sum::<Real>().sqrt();
            let f = ts * edot0 * norm / ((sigma_y + prev_iso[q]) * (sigma_y + prev_iso[q]));
            let base = if per_point { 4 * q } else { 0 };
            for i in 0..4 {
                let want = prev_inel[base + i] + f * sig[i];
                assert!(
                    (cur[4 * q + i] - want).abs() <= 1e-13 * want.abs().max(1e-30),
                    "per_point {per_point}, q {q}, component {i}: {} vs {want}",
                    cur[4 * q + i]
                );
            }
        }
    }
}

/// UD-582: the orthotropic law reports a changed
/// stiffness after every assembly by default (its damage changes between
/// solves); the compat mode reports `MaterialElastic`'s flag, `false` once
/// the tangent was assembled.
#[test]
fn orthotropic_stiffness_flag_default_and_upstream() {
    use akantu_extra_materials::orthotropic_damage::MaterialOrthotropicDamageIterative;
    let dat = "material orthotropic_damage_iterative [\n name = m\n rho = 1\n E = 1e9\n nu = 0.2\n Sc = 1e6\n]\n";
    for (options, after) in [
        (ExtraLawOptions::default(), true),
        (ExtraLawOptions::upstream(), false),
    ] {
        let mesh = read_mesh(&data("test_material_damage/two_elements.msh"), 2);
        let mut model = model_with_options(mesh, dat, options);
        model
            .init_full(&ModelOptions::new(AnalysisMethod::Static))
            .unwrap();
        let m: &MaterialOrthotropicDamageIterative<2> = law(&model);
        assert!(akantu_solid::Material::has_stiffness_matrix_changed(m));
        model.assemble_stiffness_matrix(true).unwrap();
        let m: &MaterialOrthotropicDamageIterative<2> = law(&model);
        assert_eq!(
            akantu_solid::Material::has_stiffness_matrix_changed(m),
            after
        );
    }
}

/// The brittle equivalent strain rate squares with
/// upstream's `pow(x, 2.)`, i.e. glibc's `pow` (not correctly rounded at
/// `y = 2`), bit for bit. Inputs: pure shear rates `∇v = [[0, x], [x, 0]]`
/// (`½(x + x) = x` exactly, every other term `pow(0, 2) = 0`) for the first
/// 64 values `x` of a fixed xorshift stream where `pow(x, 2.) ≠ x·x`
/// (17,008 such inputs were counted in 2·10⁷).
#[test]
fn brittle_equivalent_strain_rate_uses_glibc_pow_bitwise() {
    use akantu_core::glibc_libm::pow;
    let mut s: u64 = 0x9E37_79B9_7F4A_7C15;
    let mut next = || {
        s ^= s << 13;
        s ^= s >> 7;
        s ^= s << 17;
        s
    };
    let (mut found, mut differs_from_square) = (0, 0);
    while found < 64 {
        let m = (next() >> 12) | 0x3FF0_0000_0000_0000;
        let e = i32::try_from(next() % 81).unwrap() - 40;
        let x = f64::from_bits(m) * 2f64.powi(e);
        if pow(x, 2.).to_bits() == (x * x).to_bits() {
            continue;
        }
        found += 1;
        let grad_v = [[0., x], [x, 0.]];
        let got = MaterialBrittle::<2>::equivalent_strain_rate(&grad_v);
        // upstream order: 2-D term, then (0,0), (0,1), (1,0), (1,1)
        let sum = |sq: &dyn Fn(Real) -> Real| {
            let mut r = 0.;
            r += 2. / 3. * sq(0.);
            r += 2. / 3. * sq(0.);
            r += 2. / 3. * sq(x);
            r += 2. / 3. * sq(x);
            r += 2. / 3. * sq(0.);
            Real::sqrt(r)
        };
        let want = sum(&|v| pow(v, 2.));
        assert_eq!(
            got.to_bits(),
            want.to_bits(),
            "x = {x:e}: {got:e} vs {want:e}"
        );
        if sum(&|v| v * v).to_bits() != want.to_bits() {
            differs_from_square += 1;
        }
    }
    assert!(
        differs_from_square > 0,
        "the inputs must separate pow from x·x"
    );
}

/// Upstream asserts `prescribed_dam > 0.`; a NaN
/// `prescribed_dam` is refused by `updateDamage` as a non-positive one.
#[test]
fn damage_iterative_refuses_nan_prescribed_damage() {
    use akantu_extra_materials::MaterialDamageIterative;
    use akantu_extra_materials::orthotropic_damage::MaterialOrthotropicDamageIterative;
    for dat in [
        "material damage_iterative [\n name = m\n rho = 1\n E = 1\n nu = 0\n Sc = 10\n]\n",
        "material orthotropic_damage_iterative [\n name = m\n rho = 1\n E = 1\n nu = 0\n Sc = 10\n]\n",
    ] {
        let mesh = read_mesh(&data("test_material_damage/two_elements.msh"), 2);
        let mut model = model_with(mesh, dat);
        model
            .init_full(&ModelOptions::new(AnalysisMethod::Static))
            .unwrap();
        model.assemble_internal_forces().unwrap();
        for value in [Real::NAN, 0., -0.1] {
            let err = model
                .with_material(
                    0,
                    akantu_model::constitutive_law::LawPhase::AfterSolveStep,
                    |l, ctx| {
                        let any = l.as_any_mut().unwrap();
                        if let Some(m) = any.downcast_mut::<MaterialDamageIterative<2>>() {
                            m.prescribed_dam = value;
                        } else if let Some(m) =
                            any.downcast_mut::<MaterialOrthotropicDamageIterative<2>>()
                        {
                            m.prescribed_dam = value;
                        }
                        akantu_extra_materials::as_iterative_damage_mut(l)
                            .unwrap()
                            .update_damage(ctx)
                    },
                )
                .unwrap_err();
            assert!(
                err.to_string().contains("greater than zero"),
                "{value}: {err}"
            );
        }
    }
}
