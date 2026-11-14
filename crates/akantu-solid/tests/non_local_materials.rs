// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): independent checks of
// src/model/solid_mechanics/materials/material_damage/material_*_non_local*,
// weight_functions/*, and src/model/common/non_local_toolbox/
// neighborhoods_criterion_evaluation/neighborhood_max_criterion.*.

//! Independent checks of the non-local laws (MAT-012/014/016), the weight
//! functions (NL-005…008), the model integration (UD-027, UD-561) and the
//! max criterion (NL-009):
//!
//! * under a uniform strain the non-local average reproduces the local
//!   variable (weights sum to one), so every non-local law follows its local
//!   counterpart along a loading/unloading history;
//! * weight-function formulas at hand-computed values;
//! * `average_on_damage` switches the averaged variable;
//! * UD-027 default vs compat registration; UD-561 default vs compat.

use akantu_core::enums::{AnalysisMethod, MeshIoType};
use akantu_core::{ElementType, GhostType, Real};
use akantu_io::read_mesh;
use akantu_mesh::{Mesh, SharedMesh};
use akantu_model::ModelOptions;
use akantu_model::constitutive_law::InternalFieldRef;
use akantu_model::non_local::{
    NonLocalOptions, PointRef, QuadPoint, WeightFunction, WeightFunctionFactory, WeightInternals,
};
use akantu_parser::Parser;
use akantu_solid::SolidMechanicsModel;
use akantu_solid::materials::non_local::{
    DamagedWeightFunction, RemoveDamagedWeightFunction, RemoveDamagedWithDamageRateWeightFunction,
    StressBasedWeightFunction, register_weight_functions,
};
use akantu_solver::SolverCallback;
use akantu_validation::reduce::nan_max;

const QUAD: ElementType = ElementType::Quadrangle4;
const NG: GhostType = GhostType::NotGhost;

fn plate() -> Mesh {
    let mut mesh = Mesh::new(2, "mesh").unwrap();
    let p = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../akantu-model/tests/data/upstream/non_local/plate.msh");
    read_mesh(&mut mesh, &p, MeshIoType::Gmsh).unwrap();
    mesh
}

fn model(dat: &str) -> SolidMechanicsModel {
    let parser = Parser::parse_str(dat, "nl.dat").unwrap();
    let mut model = SolidMechanicsModel::with_parser(
        SharedMesh::new(plate()),
        None,
        "solid_mechanics_model",
        parser,
    )
    .unwrap();
    model
        .init_full(&ModelOptions::new(AnalysisMethod::ExplicitLumpedMass))
        .unwrap();
    model
}

fn internal(model: &SolidMechanicsModel, m: usize, name: &str) -> Vec<Real> {
    let mut out = None;
    model.material(m).unwrap().for_each_internal(&mut |n, f| {
        if n == name
            && let InternalFieldRef::Real(f) = f
        {
            out = Some(f.get(QUAD, NG).unwrap().as_slice().to_vec());
        }
    });
    out.unwrap_or_else(|| panic!("internal {name}"))
}

/// One committed step under the uniform strain `eps` (eigen gradient
/// `−eps`, no displacement).
fn step(model: &mut SolidMechanicsModel, eps: [Real; 4]) {
    for m in 0..model.nb_materials() {
        let law = model.material_mut(m).unwrap();
        for g in law
            .base_mut()
            .eigen_grad_u
            .get_mut(QUAD, NG)
            .unwrap()
            .as_mut_slice()
            .as_chunks_mut::<4>()
            .0
        {
            *g = eps.map(|e| -e);
        }
    }
    model
        .physics
        .before_solve_step(&mut model.solver.dof_manager)
        .unwrap();
    model.assemble_internal_forces().unwrap();
    model
        .physics
        .after_solve_step(&mut model.solver.dof_manager, true)
        .unwrap();
}

/// Largest relative difference; NaN-propagating (main's
/// rule): a NaN anywhere makes it NaN and the `<=` checks fail.
fn max_rel(a: &[Real], b: &[Real]) -> Real {
    assert_eq!(a.len(), b.len(), "max_rel: lengths");
    a.iter()
        .zip(b)
        .map(|(x, y)| {
            let s = nan_max(x.abs(), y.abs());
            if s == 0. { 0. } else { (x - y).abs() / s }
        })
        .fold(0., nan_max)
}

/// The non-local law follows its local counterpart under a uniform strain
/// history: the averaged variable equals the local one up to the rounding
/// of `Σ_k w_k = 1` (≤ (n + 1)·ε for n ≤ 64 weights) and so do damage and
/// stress (the damage laws are Lipschitz in the averaged variable with a
/// moderate constant at these strains; bound 1e-12 relative, measured
/// values printed).
#[test]
fn non_local_laws_follow_their_local_law_under_uniform_strain() {
    let history: [[Real; 4]; 6] = [
        [1e-4, 2e-5, 2e-5, -3e-5],
        [2e-4, 4e-5, 4e-5, -6e-5],
        [3.5e-4, 5e-5, 5e-5, -1e-4],
        [2e-4, 3e-5, 3e-5, -5e-5],
        [5e-4, 6e-5, 6e-5, -1.5e-4],
        [7e-4, 8e-5, 8e-5, -2e-4],
    ];
    let elastic = "  rho = 1\n  E = 1e9\n  nu = 0.2\n";
    let cases = [
        ("mazars", "mazars_non_local", "  K0 = 1e-4\n", "Ehat"),
        (
            "mazars",
            "mazars_non_local",
            "  K0 = 1e-4\n  average_on_damage = true\n",
            "damage",
        ),
        (
            "plastic_mazars",
            "plastic_mazars_non_local",
            "  K0 = 1e-4\n  sigma_y = 8e4\n  h = 1e8\n",
            "Ehat",
        ),
        ("marigo", "marigo_non_local", "  Yd = 2\n  Sd = 50\n", "Y"),
    ];
    for (local, nl, params, var) in cases {
        let local_params = params.replace("  average_on_damage = true\n", "");
        let mut ml = model(&format!(
            "material {local} [\n  name = body\n{elastic}{local_params}]\n"
        ));
        let mut mn = model(&format!(
            "material {nl} [\n  name = body\n{elastic}{params}]\nnon_local body base_wf [\n  radius = 0.6\n]\n"
        ));
        assert!(!ml.is_non_local() && mn.is_non_local());
        let nl_var = if nl == "marigo_non_local" {
            "Y non local"
        } else {
            "non_local_variable"
        };
        let mut worst: Real = 0.;
        for eps in history {
            step(&mut ml, eps);
            step(&mut mn, eps);
            let (vl, vn) = (internal(&mn, 0, var), internal(&mn, 0, nl_var));
            let r_var = max_rel(&vl, &vn);
            assert!(
                r_var <= 65. * Real::EPSILON,
                "{nl}: average of a constant {r_var:e}"
            );
            let r_d = max_rel(&internal(&ml, 0, "damage"), &internal(&mn, 0, "damage"));
            let r_s = max_rel(&internal(&ml, 0, "stress"), &internal(&mn, 0, "stress"));
            worst = nan_max(nan_max(worst, r_d), r_s);
            assert!(
                r_d <= 1e-12 && r_s <= 1e-12,
                "{nl} ({var}): damage {r_d:e}, stress {r_s:e}"
            );
        }
        let dmax = internal(&mn, 0, "damage").iter().copied().fold(0., nan_max);
        assert!(
            dmax > 0.05,
            "{nl}: the history damages the material ({dmax})"
        );
        eprintln!(
            "{nl} averaging {var}: max relative difference to {local}: {worst:e} (final damage {dmax})"
        );
    }
}

/// `average_on_damage` (default false) selects the averaged variable.
#[test]
fn average_on_damage_selects_the_variable() {
    let m = model(
        "material mazars_non_local [\n  name = body\n]\nnon_local body base_wf [\n  radius = 0.6\n]\n",
    );
    let law = m.material(0).unwrap();
    assert_eq!(
        law.get_param("average_on_damage").unwrap(),
        akantu_parser::ParamValue::Bool(false)
    );
    let nl = law.as_non_local().unwrap();
    assert_eq!(nl.non_local_variables()[0].local, "Ehat");
    let m = model(
        "material mazars_non_local [\n  name = body\n  average_on_damage = true\n]\nnon_local body base_wf [\n  radius = 0.6\n]\n",
    );
    let nl = m.material(0).unwrap().as_non_local().unwrap();
    assert_eq!(nl.non_local_variables()[0].local, "damage");
    assert_eq!(nl.neighborhood_name(), "body");
}

/// UD-027: the default registers the real non-local Marigo law; the compat
/// switch allocates upstream's local law (no manager, `non_local` section
/// ignored).
#[test]
fn ud027_marigo_non_local_registration() {
    let dat = "material marigo_non_local [\n  name = body\n  Yd = 2\n]\nnon_local body remove_wf [\n  radius = 0.6\n]\n";
    let m = model(dat);
    assert_eq!(m.material(0).unwrap().law_type(), "marigo_non_local");
    assert!(m.is_non_local());
    let parser = Parser::parse_str(dat, "nl.dat").unwrap();
    let mut m = SolidMechanicsModel::with_parser(
        SharedMesh::new(plate()),
        None,
        "solid_mechanics_model",
        parser,
    )
    .unwrap();
    akantu_solid::materials::non_local::register_upstream_marigo_non_local(&mut m.physics.factory);
    m.init_full(&ModelOptions::new(AnalysisMethod::ExplicitLumpedMass))
        .unwrap();
    assert_eq!(m.material(0).unwrap().law_type(), "marigo");
    assert!(!m.is_non_local());
}

/// UD-561: two non-local laws with different averaged variables; upstream
/// asks every non-local law for every averaged internal and throws; the
/// default copies each average to the laws that own it.
#[test]
fn ud561_mixed_non_local_laws() {
    let dat = "material mazars_non_local [\n  name = mat_1\n  K0 = 1e-4\n  E = 1e9\n]\n\
               material marigo_non_local [\n  name = mat_2\n  Yd = 2\n  E = 1e9\n]\n\
               non_local mat_1 base_wf [\n  radius = 0.6\n]\nnon_local mat_2 base_wf [\n  radius = 0.6\n]\n";
    for (options, ok) in [
        (NonLocalOptions::default(), true),
        (NonLocalOptions::upstream(), false),
    ] {
        let parser = Parser::parse_str(dat, "nl.dat").unwrap();
        let mut m = SolidMechanicsModel::with_parser(
            SharedMesh::new(plate()),
            None,
            "solid_mechanics_model",
            parser,
        )
        .unwrap();
        m.physics.non_local_options = options;
        m.set_material_selector(Box::new(
            akantu_model::constitutive_law::ElementDataSelector::new("physical_names"),
        ));
        m.init_full(&ModelOptions::new(AnalysisMethod::ExplicitLumpedMass))
            .unwrap();
        let r = m.assemble_internal_forces();
        assert_eq!(r.is_ok(), ok, "{options:?}: {r:?}");
        if !ok {
            assert!(
                r.unwrap_err()
                    .to_string()
                    .contains("does not contain an internal")
            );
        }
    }
}

fn pts() -> (QuadPoint, QuadPoint) {
    (
        QuadPoint::new(QUAD, NG, 0, 0, 4),
        QuadPoint::new(QUAD, NG, 0, 1, 4),
    )
}

/// Weight functions at hand-computed values (upstream formulas as coded).
#[test]
fn weight_function_formulas() {
    let (a, b) = pts();
    let (pa, pb) = (
        PointRef { point: &a, slot: 0 },
        PointRef { point: &b, slot: 1 },
    );
    let mut internals = WeightInternals::new();
    internals.insert("damage".into(), vec![0., 0.5]);
    // remove_wf: same point → 1; damaged below the limit → base weight;
    // at the limit → 0
    let mut rw = RemoveDamagedWeightFunction::default();
    rw.set_radius(2.);
    rw.damage_limit = 0.5;
    rw.update_internals(&internals).unwrap();
    assert_eq!(rw.weight(0.7, pa, pa), 1.);
    assert_eq!(rw.weight(1., pb, pa), (1. - 1. / 4.) * (1. - 1. / 4.));
    assert_eq!(
        rw.weight(1., pa, pb),
        0.,
        "D = 0.5 is not below 0.5·(1 − tol)"
    );
    rw.damage_limit = 0.5 / (1. - 2. * akantu_core::math::DEFAULT_TOLERANCE);
    assert_eq!(rw.weight(1., pa, pb), 0.75 * 0.75);
    assert_eq!(rw.weight(3., pb, pa), 0., "outside the radius: max(0, ·)");
    // damage_wf: D = 0 → exponent b = 2: W = (1 − r²/R⁴)²
    let mut dw = DamagedWeightFunction::default();
    dw.set_radius(2.);
    dw.update_internals(&internals).unwrap();
    let r4: Real = 16.;
    assert_eq!(dw.weight(1., pb, pa), (1. - 1. / r4) * (1. - 1. / r4));
    // D = 0.5: R_t = (R²/2)² = 4, e = 2 ln 0.51 / ln(1 − 0.49·4/16)
    let e = (2. * akantu_core::glibc_libm::log(0.51))
        / akantu_core::glibc_libm::log(1.0 - 0.49 * 4. / 16.);
    #[allow(clippy::cast_possible_truncation)] // small positive exponent
    let fl = e.floor() as i32;
    let b_exp = Real::from(fl + fl % 2);
    assert_eq!(
        dw.weight(1., pa, pb),
        akantu_core::glibc_libm::pow(1. - 1. / r4, b_exp)
    );
    // 2·ln 0.51 / ln 0.8775 = 10.30… → b = 10 + (10 mod 2) = 10
    assert_eq!(b_exp, 10.);
    // D = 1: R_t = 0 < tol → 0.001·R_init
    assert_eq!(
        DamagedWeightFunction::weight_for_damage(1., 4., 1.),
        DamagedWeightFunction::weight_for_damage(1., 4., 1. - 1e-300)
    );
    // remove with damage rate: unregistered, max(0, 1 − r²/R²)² (upstream
    // clamps: remove_damaged_with_damage_rate_weight_function_inline_impl.hh
    // :51; the oracle sweep is
    // non_local_fixtures.rs::weight_rate_sweep_matches_the_oracle)
    let mut rr = RemoveDamagedWithDamageRateWeightFunction::default();
    rr.set_radius(2.);
    let mut ri = WeightInternals::new();
    ri.insert("damage-rate".into(), vec![0., 0.2]);
    rr.update_internals(&ri).unwrap();
    assert_eq!(rr.weight(3., pa, pb), 0., "outside the radius: max(0, ·)");
    assert_eq!(rr.weight(2., pa, pb), 0.);
    assert_eq!(rr.weight(1., pa, pb), 0.75 * 0.75);
    assert_eq!(rr.weight(3., pa, pa), 1., "same point");
    rr.damage_limit = 0.1;
    assert_eq!(rr.weight(1., pa, pb), 0.);
    // stress based: stub
    let sw = StressBasedWeightFunction::default();
    assert_eq!(sw.weight(0., pa, pb), 0.);
    // registration: damage_wf and remove_wf only
    let mut f = WeightFunctionFactory::default();
    register_weight_functions(&mut f);
    assert_eq!(f.types(), ["base_wf", "damage_wf", "remove_wf"]);
}

/// NL-009 (evident intent, UD-563): the owned points whose criterion is the
/// largest within the radius.
/// Independent model of upstream's `damage_wf` exponent: `expb` from the
/// operands as coded, then `Int expb_floor = floor(expb)` and `expb_floor +
/// expb_floor % 2` in `i64` (so the `i32` overflow is visible). `None` where
/// the `i32` conversion or sum is undefined (NaN, out of range, sum ≥ 2³¹).
fn upstream_defined_exponent(r2: Real, d: Real) -> (Real, Option<Real>) {
    let mut radius_init = r2;
    let mut radius_t = radius_init * (1. - d);
    radius_init *= radius_init;
    radius_t *= radius_t;
    if radius_t < akantu_core::math::DEFAULT_TOLERANCE {
        radius_t = 0.001 * radius_init;
    }
    let expb = (2. * akantu_core::glibc_libm::log(0.51))
        / akantu_core::glibc_libm::log(1.0 - 0.49 * radius_t / radius_init);
    let f = expb.floor();
    if !(-2_147_483_648.0..2_147_483_648.0).contains(&f) {
        return (expb, None);
    }
    #[allow(clippy::cast_possible_truncation)] // range checked, integral value
    let i = f as i64;
    let b = i + i % 2;
    if b > i64::from(i32::MAX) {
        return (expb, None);
    }
    #[allow(clippy::cast_precision_loss)] // |b| ≤ 2³¹: exact
    (expb, Some(b as Real))
}

/// The damages of the domain checks: a uniform grid of [0, 1], the UD-557
/// bands (ordinary, last in-range / first out-of-range `floor(expb)`, beyond
/// the `Int` range, `ln(1 − x) = +0`, the tolerance clamp) and log-spaced
/// `1 − D` down to 1e-20, sorted increasingly.
fn domain_damages() -> Vec<Real> {
    let mut d: Vec<Real> = (0..=1000).map(|k| Real::from(k) / 1000.).collect();
    for e in 1..=200 {
        d.push(1. - akantu_core::glibc_libm::pow(10., -Real::from(e) / 10.));
    }
    d.extend([
        Real::from_bits(0x3fef_ffb4_f9d6_acc3),
        Real::from_bits(0x3fef_ffb4_f9d6_acc4),
        1. - 3.6e-5,
        1. - 3e-5,
        1. - 8e-9,
        1. - 6e-9,
        1. - 1.06e-8,
        1. - 1.5e-11,
        1. - 1e-300,
        1.,
    ]);
    d.retain(|x| (0. ..=1.).contains(x));
    d.sort_by(Real::total_cmp);
    d.dedup();
    d
}

/// UD-557 corrected `damage_wf` (default, D-062 (3)), independent checks
/// over the whole domain 0 ≤ D ≤ 1 for radii 0.01 … 1e4 and distances
/// r ∈ {0, 1e-6·R, 0.3·R, R}:
///
/// 1. wherever upstream's `Int` arithmetic is defined, the corrected weight
///    is upstream's, bitwise (independent model above);
/// 2. the weight is finite and in [0, 1] (no NaN, no infinity);
/// 3. it is non-increasing in D below the tolerance clamp (the clamp itself
///    is upstream's formula, kept);
/// 4. limits: weight 1 at r = 0 for every D; for D → 1⁻ (before the clamp)
///    the weight at r = 0.3·R tends to 0;
/// 5. the x86_64 compat follows `cvttsd2si` (`INT_MIN` for NaN or
///    out-of-range) and is non-finite exactly where the oracle is.
#[test]
fn damage_wf_exponent_domain() {
    use akantu_model::non_local::WeightArithmetic::{Corrected, UpstreamX86_64};
    let damages = domain_damages();
    let mut n_defined = 0usize;
    let mut n_undefined = 0usize;
    for radius in [0.01, 0.5, 1., 5., 100., 1e4] {
        let r2: Real = radius * radius;
        for r in [0., 1e-6 * radius, 0.3 * radius, radius] {
            let mut previous: Option<Real> = None;
            for &d in &damages {
                let w = DamagedWeightFunction::weight_for_damage_with(r, r2, d, Corrected);
                assert!(
                    w.is_finite() && (0. ..=1.).contains(&w),
                    "R={radius} r={r} D={d:e}: corrected weight {w}"
                );
                let (expb, upstream_b) = upstream_defined_exponent(r2, d);
                let x86 = DamagedWeightFunction::weight_for_damage_with(r, r2, d, UpstreamX86_64);
                let alpha = (1. - r * r / (r2 * r2)).max(0.);
                if let Some(b) = upstream_b {
                    n_defined += 1;
                    let up = akantu_core::glibc_libm::pow(alpha, b);
                    assert_eq!(
                        w.to_bits(),
                        up.to_bits(),
                        "R={radius} r={r} D={d:e}: defined domain"
                    );
                    assert_eq!(
                        x86.to_bits(),
                        up.to_bits(),
                        "R={radius} r={r} D={d:e}: compat"
                    );
                } else {
                    n_undefined += 1;
                    // cvttsd2si: INT_MIN → b = −2³¹ (`expb` NaN, ±∞ or ≥ 2³¹)
                    let int_min = akantu_core::glibc_libm::pow(alpha, -2_147_483_648.0);
                    assert_eq!(
                        x86.to_bits(),
                        int_min.to_bits(),
                        "R={radius} D={d:e} expb={expb:e}"
                    );
                    if alpha < 1. && alpha > 1. - 1e-6 {
                        assert!(x86 > 1., "huge or infinite oracle weight");
                    } else if alpha < 1. {
                        assert_eq!(x86, Real::INFINITY);
                    }
                }
                // monotone below the tolerance clamp
                let clamped =
                    (r2 * (1. - d)) * (r2 * (1. - d)) < akantu_core::math::DEFAULT_TOLERANCE;
                if !clamped {
                    if let Some(p) = previous {
                        assert!(w <= p, "R={radius} r={r} D={d:e}: {w} > {p} (not monotone)");
                    }
                    previous = Some(w);
                }
                if r == 0. {
                    assert_eq!(w, 1.);
                }
            }
        }
        // D → 1⁻ before the clamp: the neighbour at 0.3·R gets weight 0
        let d = 1. - 1e-6;
        assert_eq!(
            DamagedWeightFunction::weight_for_damage(0.3 * radius, r2, d),
            0.,
            "R={radius}"
        );
    }
    assert!(n_defined > 0 && n_undefined > 0);
    // outside [0, 1]: finite (1 − x < 0 continues the limit: b = 0, W = 1);
    // a NaN damage stays NaN (the input's NaN)
    assert_eq!(DamagedWeightFunction::weight_for_damage(0.3, 1., -0.5), 1.);
    assert!(DamagedWeightFunction::weight_for_damage(0.3, 1., Real::NAN).is_nan());
    // the exponent of the ln(1 − x) = +0 band: −x instead of +0
    let (expb, x, _) = DamagedWeightFunction::exponent_operands(25., 1. - 1e-8);
    assert_eq!(expb, Real::NEG_INFINITY);
    let b = DamagedWeightFunction::exponent(expb, x, Corrected);
    assert!(b.is_finite() && b > 1e16, "b = {b:e}");
    assert_eq!(
        DamagedWeightFunction::exponent(Real::INFINITY, 0., Corrected),
        Real::INFINITY
    );
}

/// The table `v91_damage_wf_exponent_domain`: the x86_64 weight (oracle conversion)
/// and the port's corrected weight at r = 0.3·R.
#[test]
fn damage_wf_review_v91_table() {
    use akantu_model::non_local::WeightArithmetic::{Corrected, UpstreamX86_64};
    let inf = Real::INFINITY;
    let rows: [(Real, Real, Real); 16] = [
        (0.5, 1e-4, 0.),
        (0.5, 3.6e-5, 0.),
        (0.5, 3e-5, inf),
        (0.5, 1e-6, inf),
        (0.5, 0., 0.),
        (5., 1e-4, 0.),
        (5., 1e-6, inf),
        (5., 8e-9, inf),
        (5., 6e-9, inf),
        (5., 1e-9, Real::from_bits(0x3f0a_06fe_128a_2c17)),
        (5., 0., Real::from_bits(0x3f0a_06fe_128a_2c17)),
        (100., 1e-6, inf),
        (100., 1e-8, inf),
        (100., 1e-10, inf),
        (100., 1e-11, Real::from_bits(0x3fef_37e1_236d_7a72)),
        (100., 0., Real::from_bits(0x3fef_37e1_236d_7a72)),
    ];
    for (radius, one_minus_d, x86) in rows {
        let d = 1. - one_minus_d;
        let r2: Real = radius * radius;
        let got =
            DamagedWeightFunction::weight_for_damage_with(0.3 * radius, r2, d, UpstreamX86_64);
        assert_eq!(
            got.to_bits(),
            x86.to_bits(),
            "R={radius} 1-D={one_minus_d:e}"
        );
        let w = DamagedWeightFunction::weight_for_damage_with(0.3 * radius, r2, d, Corrected);
        assert!(w.is_finite(), "R={radius} 1-D={one_minus_d:e}");
        if x86.is_finite() {
            assert_eq!(w.to_bits(), x86.to_bits(), "defined: corrected = upstream");
        } else {
            assert_eq!(
                w, 0.,
                "R={radius} 1-D={one_minus_d:e}: limit b → +∞ at alpha < 1"
            );
        }
    }
}

#[test]
fn neighborhood_max_criterion_finds_local_maxima() {
    let mesh = plate();
    let fe = akantu_fe::FeEngine::with_precomputation(
        &mesh,
        None,
        akantu_core::ElementKind::Regular,
        akantu_fe::shape::ShapeFamily::Lagrange,
        "fe",
    )
    .unwrap();
    let parser = Parser::parse_str("neighborhood test [\n  radius = 0.5\n]\n", "n.dat").unwrap();
    let section = parser
        .root
        .sub_sections_of(akantu_parser::ParserType::Neighborhood)
        .next()
        .unwrap();
    let mc = akantu_model::non_local::max_criterion::NeighborhoodMaxCriterion::new(
        &mesh,
        &fe,
        "criterion",
        "max",
        section,
        false,
    )
    .unwrap();
    let n = mc.layout().len();
    let mut xq = akantu_core::Array::new(2);
    fe.integration_points_coordinates(&mesh, &mut xq, QUAD, NG, None)
        .unwrap();
    // a criterion peaked at (0.5, 0.5) and at (-0.5, -0.5) (two maxima)
    let crit: Vec<Real> = (0..n)
        .map(|k| {
            let x = xq.tuple(k);
            let d1 = (x[0] - 0.5).powi(2) + (x[1] - 0.5).powi(2);
            let d2 = (x[0] + 0.5).powi(2) + (x[1] + 0.5).powi(2);
            (-d1).max(-d2 - 0.01)
        })
        .collect();
    let found = mc.find_max_quads(&crit).unwrap();
    // brute force: strictly larger than every other point within R + tol,
    // ties going to the first point of the pair as in the port
    let r = 0.5 + akantu_core::math::DEFAULT_TOLERANCE;
    let brute: Vec<usize> = (0..n)
        .filter(|&k| {
            (0..n).all(|j| {
                j == k
                    || akantu_model::non_local::distance(xq.tuple(k), xq.tuple(j)) > r
                    || crit[k] > crit[j]
            })
        })
        .collect();
    let got: Vec<usize> = found
        .iter()
        .map(|q| usize::try_from(q.global_num).unwrap())
        .collect();
    assert_eq!(got, brute);
    assert!(!got.is_empty());
}
