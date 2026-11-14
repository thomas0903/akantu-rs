// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Differential tests of
// the inelastic materials against the golden material-point
// fixtures produced by oracle/probes/cpp/material_probe.cc
// (src/model/solid_mechanics/materials/material_damage/material_{damage,
// marigo,mazars}*, material_plastic/*, material_finite_deformation/*).

//! For every case the law is created exactly as upstream (`material.dat`
//! parsed, factory by keyword, id `solid_mechanics_model:0:<type>`,
//! `initMaterial`; global `seed` of the input file → `RandomGenerator`
//! seed, as upstream `initialize`) on the probe's one-element mesh, then
//! the probe's steps are replayed with the probe's input gradient (the FE
//! gradient of `u = H·X`, recomputed with `akantu_fe`): `beforeSolveStep`,
//! `computeStress` (the stored `grad_u` is compared with the record),
//! `computeTangentModuli`,
//! `getEnergy("potential")`, requested internals, `afterSolveStep(true)`,
//! then the post-step records (the damage laws'
//! `damage dissipated energy` / `integral of sigma` internals and
//! `getEnergy("dissipated")` after `afterSolveStep(true)`, where upstream's
//! `MaterialDamage::updateEnergies` runs). Error-path fixtures (upstream exceptions, e.g. UD-026 Neo-Hookean) are
//! replayed until the error, which must match the recorded kind/message.
//!
//! Every quantity is compared with the fixture tolerances (`DiffTest`) and
//! for bit identity; the replay asserts bit identity where the kernels
//! replicate upstream's operation order (D-008) — every law.

mod common;

use akantu_validation::reduce::NanMinMax;
use std::sync::Mutex;

use akantu_core::random::{IdxSlot, RandomGenerator, initial_seed};
use akantu_core::{Array, ElementKind};
use akantu_fe::FeEngine;
use akantu_fe::shape::ShapeFamily;
use akantu_model::constitutive_law::{InternalFieldRef, LawPhase};
use akantu_solid::material::LawOptions;
use akantu_validation::{DiffTest, Fixture, fixtures_root};
use common::{
    NG, SimplexModel, bitwise_equal_count, col_major_to_rows, materials_from_dat_with, simplex,
};

/// The global random engine is process-wide: replays run one at a time.
static ENGINE: Mutex<()> = Mutex::new(());

/// Material-point cases under `fixtures/material/<law>`.
fn cases(law: &str) -> Vec<Fixture> {
    let dir = fixtures_root().join("material").join(law);
    let Ok(rd) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut v: Vec<_> = rd
        .filter_map(|e| e.ok())
        .filter(|e| e.path().join("manifest.json").exists())
        .map(|e| Fixture::open(&e.path()).unwrap())
        .collect();
    v.sort_by(|a, b| a.case_id().cmp(b.case_id()));
    v
}

/// Options of one replay.
#[derive(Clone, Copy)]
struct Replay {
    /// Upstream-defect switches set before `initMaterial`.
    options: LawOptions,
    /// Quantities not compared.
    skip: &'static [&'static str],
    /// Require bit identity of every compared value.
    bitwise: bool,
    /// Cases replayed (by case id).
    include: fn(&str) -> bool,
}

fn all_cases(_: &str) -> bool {
    true
}

/// Cases whose recorded behaviour the corrected defaults keep: no
/// temperature change (UD-038/UD-109) and no finite-strain plasticity
/// (UD-030, unsupported by default). Later corrections narrow this per
/// law ([`j2_default_cases`], [`dp_default_cases`]; `plastic_mazars`: see
/// `plastic_mazars_corrected_defaults_match_until_damage`); the remaining
/// recorded cases are covered by the analytical regressions of
/// `review_v32_regressions.rs`.
fn default_cases(id: &str) -> bool {
    !id.contains("thermal") && !id.contains("finite_deformation")
}

/// J2: only the 3-D cases (UD-137 corrects every yielding 1-D / 2-D case;
/// with one evaluation per step the committed-state predicate of UD-138
/// coincides with upstream's).
fn j2_default_cases(id: &str) -> bool {
    default_cases(id) && id.contains("/3d_")
}

/// Drucker–Prager: the cases that never yield (the compression cases). The
/// recorded shear cases end **outside** the yield surface (upstream's
/// regular return does not converge, UD-146) and the hydrostatic cases
/// overshoot the apex (UD-145); the closed-form default corrects both.
fn dp_default_cases(id: &str) -> bool {
    default_cases(id) && id.contains("compression")
}

/// Replay in upstream compatibility mode, bitwise.
const UPSTREAM: Replay = Replay {
    options: LawOptions::upstream(),
    skip: &[],
    bitwise: true,
    include: all_cases,
};

/// Replay on the cases the corrected defaults must reproduce (the options
/// are set to `LawOptions::default()` by every user; `default()` is not
/// `const`).
const DEFAULTS: Replay = Replay {
    options: LawOptions::upstream(),
    skip: &[],
    bitwise: true,
    include: default_cases,
};

/// Bit-identity summary of one quantity.
#[derive(Debug, Default)]
struct Bits {
    compared: usize,
    identical: usize,
    max_rel: f64,
}

/// Upstream kind of a port error, as the probe records it.
fn error_kind(e: &akantu_core::Error) -> &'static str {
    match e {
        akantu_core::Error::NotImplemented(_) => "NotImplementedException",
        _ => "Exception",
    }
}

/// The probe's input gradient: `u(n, r) = Σ_c H(r, c)·X(n, c)` (sequential
/// from 0, as `material_probe.cc`), then upstream's FE gradient
/// (`akantu_fe::FeEngine::gradient_on_integration_points`, bit-identical to
/// the oracle). Column-major `dim × dim`. The recorded `grad_u` is
/// the gradient stored by the law **after** the step, which `plastic_mazars`
/// rewrites as `(∇u − ε_p) + ε_p` — so the replay feeds this gradient and
/// compares the stored one with the record.
fn fe_gradient(model: &SimplexModel, fe: &FeEngine, h: &[f64], dim: usize) -> Vec<f64> {
    let x = model.mesh.nodes();
    let nn = x.len();
    let mut u = Array::zeros(nn, dim);
    for n in 0..nn {
        for r in 0..dim {
            let mut v = 0.;
            for c in 0..dim {
                v += h[r * dim + c] * x.tuple(n)[c];
            }
            u.as_mut_slice()[n * dim + r] = v;
        }
    }
    let mut g = Array::new(dim * dim);
    fe.gradient_on_integration_points(&model.mesh, u.view(), &mut g, dim, simplex(dim), NG, None)
        .unwrap();
    g.tuple(0).to_vec()
}

/// Replay one fixture; returns the bit-identity counts per quantity.
fn replay(fx: &Fixture, opts: Replay) -> Vec<(String, Bits)> {
    let _guard = ENGINE
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let p = &fx.manifest().parameters;
    let dim = usize::try_from(p["spatial_dimension"].as_i64().unwrap()).unwrap();
    let ty = simplex(dim);
    let model = SimplexModel::from_msh(dim, &fx.input_path("mesh").unwrap());
    let dat = std::fs::read(fx.input_path("material").unwrap()).unwrap();
    // upstream `initialize`: AKA_SEED, --aka_seed, then the input's `seed`
    let parsed = akantu_parser::Parser::parse_bytes(
        &dat,
        "material.dat",
        akantu_parser::AnonymousIds::Local(&mut 0),
    )
    .unwrap();
    if let Some(s) = parsed.root.parameter("seed") {
        RandomGenerator::<IdxSlot>::seed(initial_seed(None, None, Some(s.to_int::<i64>()), 0, 0));
    }
    let mut handler = materials_from_dat_with(&model, &dat, Some(opts.options)).unwrap();
    let law = handler.law_mut(0).unwrap();
    assert_eq!(
        law.law_id(),
        p["material_id"].as_str().unwrap(),
        "{}",
        fx.case_id()
    );
    assert_eq!(
        law.name(),
        p["material_name"].as_str().unwrap(),
        "{}",
        fx.case_id()
    );

    let grad_u = fx.load_f64("input/grad_u_prescribed").unwrap();
    let recorded = fx.load_f64("grad_u").unwrap();
    let nsteps = grad_u.shape[0];
    let nrec = recorded.shape[0];
    let n = dim * dim;
    let v = dim * (dim + 1) / 2;
    let mut t = DiffTest::new(
        &format!("solid::inelastic_material_fixtures::{}", fx.case_id()),
        fx.case_id(),
    )
    .unwrap();
    let names = |pat: &str, prefix: &str| -> Vec<String> {
        t.fixture()
            .quantity_names(pat)
            .into_iter()
            .map(|s| s.trim_start_matches(prefix).to_owned())
            .collect()
    };
    let internals = names("internal/*", "internal/");
    // post-step records (probe v4 `--post-internal` / `--post-energy`)
    let post_internals = names("post_step/internal/*", "post_step/internal/");
    let post_energies = names("post_step/energy/*", "post_step/energy/");
    // fixture quantity name -> upstream internal name (names with spaces)
    let internal_name = |q: &str| -> String {
        p.get("internal_names")
            .and_then(|m| m.get(q))
            .and_then(serde_json::Value::as_str)
            .unwrap_or(q)
            .to_owned()
    };
    let has_tangent = t.fixture().quantity("tangent").is_ok();
    let has_energy = t.fixture().quantity("energy/potential").is_ok();
    let speed_names = ["celerity", "wave_speed/push", "wave_speed/shear"];
    let speed_present: Vec<bool> = speed_names
        .iter()
        .map(|q| t.fixture().quantity(q).is_ok())
        .collect();
    let mut speeds = vec![Vec::new(); 3];
    let mut stress = Vec::new();
    let mut tangent = Vec::new();
    let mut energy = Vec::new();
    let mut internal_values = vec![Vec::new(); internals.len()];
    let mut post_values = vec![Vec::new(); post_internals.len()];
    let mut post_energy_values = vec![Vec::new(); post_energies.len()];
    let services = &model;
    let mut error = None;
    let fe = FeEngine::with_precomputation(
        &model.mesh,
        None,
        ElementKind::Regular,
        ShapeFamily::Lagrange,
        "probe_fe",
    )
    .unwrap();
    let mut stored_grad_u = Vec::new();
    for k in 0..nsteps.min(nrec + 1) {
        let ctx = services.ctx(LawPhase::BeforeSolveStep);
        law.before_solve_step(&ctx).unwrap();
        let col = fe_gradient(&model, &fe, &grad_u.data[k * n..(k + 1) * n], dim);
        law.base_mut()
            .grad_u
            .get_mut(ty, NG)
            .unwrap()
            .as_mut_slice()
            .copy_from_slice(&col);
        let ctx = services.ctx(LawPhase::Stress);
        if let Err(e) = law.compute_stress(&ctx, ty, NG) {
            error = Some((k, e));
            break;
        }
        assert!(
            k < nrec,
            "{}: step {k} succeeded but was not recorded",
            fx.case_id()
        );
        stored_grad_u.extend(col_major_to_rows(
            law.base().grad_u.get(ty, NG).unwrap().as_slice(),
            dim,
        ));
        let s = law.base().stress.get(ty, NG).unwrap().as_slice().to_vec();
        stress.extend(col_major_to_rows(&s, dim));
        if has_tangent {
            let mut tm = Array::zeros(1, v * v);
            let ctx = services.ctx(LawPhase::Tangent);
            law.compute_tangent_moduli(&ctx, ty, NG, &mut tm).unwrap();
            tangent.extend(col_major_to_rows(tm.as_slice(), v));
        }
        if has_energy {
            let ctx = services.ctx(LawPhase::Energy);
            energy.push(law.energy_of(&ctx, "potential").unwrap());
        }
        let el = akantu_core::Element::new(ty, 0, NG);
        for (q, present) in speed_present.iter().enumerate() {
            if *present {
                speeds[q].push(match q {
                    0 => law.celerity(el).unwrap(),
                    1 => law.push_wave_speed(el).unwrap(),
                    _ => law.shear_wave_speed(el).unwrap(),
                });
            }
        }
        for (i, name) in internals.iter().enumerate() {
            internal_values[i].extend(tuple0(&*law, &internal_name(name), ty));
        }
        let ctx = services.ctx(LawPhase::AfterSolveStep);
        law.after_solve_step(&ctx, true).unwrap();
        for (i, name) in post_internals.iter().enumerate() {
            post_values[i].extend(tuple0(&*law, &internal_name(name), ty));
        }
        for (i, e) in post_energies.iter().enumerate() {
            let ctx = services.ctx(LawPhase::Energy);
            post_energy_values[i].push(law.energy_of(&ctx, e).unwrap());
        }
    }
    if t.expects_error() {
        let (k, e) = error.unwrap_or_else(|| panic!("{}: expected an error", fx.case_id()));
        assert_eq!(
            Some(k as i64),
            p["error_step"].as_i64(),
            "{}: error step",
            fx.case_id()
        );
        t.observe_error(error_kind(&e), &e.to_string());
    } else if let Some((k, e)) = error {
        panic!("{}: step {k} failed: {e}", fx.case_id());
    }

    let mut bits = Vec::new();
    let mut check = |t: &mut DiffTest, q: &str, actual: &[f64]| {
        if opts.skip.contains(&q) {
            return;
        }
        t.check_f64(q, actual);
        let reference = t.reference_f64(q).unwrap();
        let scale = reference
            .data
            .iter()
            .fold(0.0_f64, |m, x| m.nan_max(x.abs()));
        let max_rel = reference
            .data
            .iter()
            .zip(actual)
            .map(|(r, a)| (r - a).abs() / if scale > 0. { scale } else { 1. })
            .fold(0.0_f64, NanMinMax::nan_max);
        bits.push((
            q.to_owned(),
            Bits {
                compared: reference.data.len(),
                identical: bitwise_equal_count(&reference.data, actual),
                max_rel,
            },
        ));
    };
    check(&mut t, "grad_u", &stored_grad_u);
    check(&mut t, "stress", &stress);
    if has_tangent {
        check(&mut t, "tangent", &tangent);
    }
    if has_energy {
        check(&mut t, "energy/potential", &energy);
    }
    for (q, present) in speed_present.iter().enumerate() {
        if *present {
            check(&mut t, speed_names[q], &speeds[q]);
        }
    }
    for (name, values) in internals.iter().zip(&internal_values) {
        check(&mut t, &format!("internal/{name}"), values);
    }
    for (name, values) in post_internals.iter().zip(&post_values) {
        check(&mut t, &format!("post_step/internal/{name}"), values);
    }
    for (e, values) in post_energies.iter().zip(&post_energy_values) {
        check(&mut t, &format!("post_step/energy/{e}"), values);
    }
    t.assert_pass();
    bits
}

/// Tuple 0 (the probe's single point) of the real internal `name` of `law`.
fn tuple0(law: &dyn akantu_solid::Material, name: &str, ty: akantu_core::ElementType) -> Vec<f64> {
    let mut found = None;
    law.for_each_internal(&mut |nm, f| {
        if nm == name
            && let InternalFieldRef::Real(f) = f
        {
            found = Some(f.get(ty, NG).unwrap().tuple(0).to_vec());
        }
    });
    found.unwrap_or_else(|| panic!("internal {name} missing"))
}

/// Replays every case of `law`; asserts tolerance compliance (DiffTest)
/// and, with `opts.bitwise`, bit identity of every value. Returns the
/// number of values compared and the largest scaled difference.
fn replay_all(law: &str, opts: Replay) -> (usize, usize, f64) {
    let cases: Vec<Fixture> = cases(law)
        .into_iter()
        .filter(|f| (opts.include)(f.case_id()))
        .collect();
    assert!(!cases.is_empty(), "no {law} fixture found");
    let (mut total, mut identical, mut max_rel) = (0, 0, 0.0_f64);
    for fx in &cases {
        for (q, b) in replay(fx, opts) {
            if opts.bitwise {
                assert_eq!(
                    b.identical,
                    b.compared,
                    "{} {q}: {} of {} values bit-identical (max scaled difference {:e})",
                    fx.case_id(),
                    b.identical,
                    b.compared,
                    b.max_rel
                );
            }
            total += b.compared;
            identical += b.identical;
            max_rel = max_rel.nan_max(b.max_rel);
        }
    }
    eprintln!(
        "{law}: {} cases, {identical}/{total} values bit-identical, max scaled difference {max_rel:e}",
        cases.len()
    );
    (total, identical, max_rel)
}

#[test]
fn marigo_material_points_match_oracle_bitwise() {
    replay_all("marigo", UPSTREAM);
}

/// Without a temperature change the corrected defaults coincide with
/// upstream bit for bit (UD-038 refresh of a zero thermal stress, UD-109
/// correction of a zero thermal stress).
#[test]
fn marigo_material_points_corrected_defaults_bitwise() {
    replay_all(
        "marigo",
        Replay {
            options: LawOptions::default(),
            ..DEFAULTS
        },
    );
}

#[test]
fn plastic_linear_isotropic_hardening_material_points_match_oracle_bitwise() {
    replay_all("plastic_linear_isotropic_hardening", UPSTREAM);
}

/// Without a temperature change the corrected thermal increments (UD-038)
/// are `+0`, as upstream's, and the 3-D return is upstream's: the defaults
/// coincide bit for bit on the 3-D cases.
#[test]
fn plastic_linear_isotropic_hardening_corrected_defaults_bitwise() {
    replay_all(
        "plastic_linear_isotropic_hardening",
        Replay {
            options: LawOptions::default(),
            include: j2_default_cases,
            ..DEFAULTS
        },
    );
}

#[test]
fn mazars_material_points_match_oracle_bitwise() {
    replay_all("mazars", UPSTREAM);
}

/// Without a temperature change the corrected defaults coincide with
/// upstream bit for bit.
#[test]
fn mazars_material_points_corrected_defaults_bitwise() {
    replay_all(
        "mazars",
        Replay {
            options: LawOptions::default(),
            ..DEFAULTS
        },
    );
}

/// UD-026: upstream's `neohookean` aborts at the first `computeStress`
/// (`NotImplementedException`); the compat mode reproduces the recorded
/// error (kind, message, step 0). The documented law (default) is
/// validated analytically (`neohookean_properties.rs`) and against the
/// oracle's own kernels (`material/neohookean_kernel/*`).
#[test]
fn neohookean_upstream_error_matches_oracle() {
    replay_all("neohookean", UPSTREAM);
}

#[test]
fn plastic_mazars_material_points_match_oracle_bitwise() {
    replay_all("plastic_mazars", UPSTREAM);
}

/// UD-139: every recorded `plastic_mazars` case damages, after which the
/// corrected effective-stress history differs from upstream's damaged one by
/// design (`review_v32_regressions.rs::mi04_*` checks the corrected
/// history). With only that switch reverted, every other corrected default
/// reproduces the 3-D records bit for bit.
#[test]
fn plastic_mazars_corrected_defaults_but_effective_stress_bitwise() {
    replay_all(
        "plastic_mazars",
        Replay {
            options: LawOptions {
                plastic_mazars_effective_stress: false,
                ..LawOptions::default()
            },
            include: |id| default_cases(id) && id.contains("/3d_"),
            ..DEFAULTS
        },
    );
}

#[test]
fn plastic_drucker_prager_material_points_match_oracle_bitwise() {
    replay_all("plastic_drucker_prager", UPSTREAM);
}

#[test]
fn plastic_drucker_prager_material_points_corrected_defaults_bitwise() {
    replay_all(
        "plastic_drucker_prager",
        Replay {
            options: LawOptions::default(),
            include: dp_default_cases,
            ..DEFAULTS
        },
    );
}

/// UD-038 cases with the corrected defaults: the stress, the thermal stress
/// and everything downstream differ from the unpatched oracle by design
/// (analytical checks in `inelastic_properties.rs`); here only the
/// temperature-independent quantities are compared: the gradient-driven
/// thermal internals of the laws that refresh them upstream too.
#[test]
fn thermal_cases_corrected_defaults_unchanged_quantities() {
    for law in [
        "plastic_linear_isotropic_hardening",
        "plastic_drucker_prager",
    ] {
        replay_all(
            law,
            Replay {
                options: LawOptions::default(),
                skip: &[
                    "stress",
                    "energy/potential",
                    "internal/inelastic_strain",
                    "internal/iso_hardening",
                    "internal/plastic_energy",
                    "internal/d_plastic_energy",
                    "internal/epsilon_th",
                ],
                bitwise: true,
                include: |id| id.contains("thermal"),
            },
        );
    }
}

// ---------------------------------------------------------------------------
// x86_64 twins with post-step records
// ---------------------------------------------------------------------------

/// Laws whose committed (aarch64) cases have an x86_64 twin
/// `fixtures/material/<law>/x86_64/<case>`: the same case generated on the
/// pinned x86_64 oracle (D-043, D-050) with the post-step records.
const TWIN_LAWS: [&str; 5] = [
    "marigo",
    "mazars",
    "plastic_mazars",
    "plastic_linear_isotropic_hardening",
    "plastic_drucker_prager",
];

/// `MaterialDamage::updateEnergies` records.
const DAMAGE_POST: [&str; 3] = [
    "post_step/internal/damage_dissipated_energy",
    "post_step/internal/integral_of_sigma",
    "post_step/energy/dissipated",
];

/// `MaterialPlastic::updateEnergies` records.
const PLASTIC_POST: [&str; 3] = [
    "post_step/internal/plastic_energy",
    "post_step/internal/d_plastic_energy",
    "post_step/energy/plastic",
];

/// The post-step quantities every twin of `law` must carry.
fn required_post(law: &str) -> Vec<&'static str> {
    let mut v = Vec::new();
    if matches!(law, "marigo" | "mazars" | "plastic_mazars") {
        v.extend(DAMAGE_POST);
    }
    if law.starts_with("plastic_") {
        v.extend(PLASTIC_POST);
    }
    v
}

/// Replays the twins of `law` like [`replay_all`] and asserts that every
/// required post-step quantity was compared (bitwise with `opts.bitwise`)
/// in every replayed case: a twin without them fails instead of silently
/// comparing nothing.
fn replay_twins(law: &str, opts: Replay) -> usize {
    let twins = format!("{law}/x86_64");
    let required = required_post(law);
    let mut post = 0;
    for fx in cases(&twins).iter().filter(|f| (opts.include)(f.case_id())) {
        let bits = replay(fx, opts);
        for q in &required {
            if opts.skip.contains(q) {
                continue;
            }
            let b = bits
                .iter()
                .find(|(name, _)| name == q)
                .map(|(_, b)| b)
                .unwrap_or_else(|| panic!("{}: post-step record {q} not compared", fx.case_id()));
            assert!(b.compared > 0, "{} {q}: empty", fx.case_id());
            post += b.compared;
        }
    }
    let (total, identical, max_rel) = replay_all(&twins, opts);
    if opts.bitwise {
        assert_eq!(identical, total);
    }
    eprintln!("{twins}: {post} post-step values among {total} (max scaled difference {max_rel:e})");
    total
}

/// Every case of the five laws has its x86_64 twin, every twin carries the
/// post-step records of its law, was generated on x86_64 and has the same
/// inputs as its aarch64 golden.
#[test]
fn x86_64_twins_cover_every_case_and_carry_the_post_step_records() {
    let mut twins = 0;
    for law in TWIN_LAWS {
        let golden: Vec<String> = cases(law)
            .iter()
            .map(|f| f.case_id().rsplit('/').next().unwrap().to_owned())
            .collect();
        let x86: Vec<Fixture> = cases(&format!("{law}/x86_64"));
        let names: Vec<String> = x86
            .iter()
            .map(|f| f.case_id().rsplit('/').next().unwrap().to_owned())
            .collect();
        assert_eq!(
            names, golden,
            "{law}: twins must mirror the committed cases"
        );
        for fx in &x86 {
            let m = fx.manifest();
            assert_eq!(
                m.build.as_ref().map(|b| b.arch.as_str()),
                Some("x86_64"),
                "{}",
                fx.case_id()
            );
            for q in required_post(law) {
                assert!(
                    fx.quantity(q).is_ok(),
                    "{}: missing post-step record {q}",
                    fx.case_id()
                );
            }
            twins += 1;
        }
    }
    // plastic_linear_isotropic_hardening: 17 + the twins of the two
    // finite-return cases added later (x86_64 goldens)
    assert_eq!(twins, 16 + 14 + 11 + 19 + 10);
}

/// The x86_64 twins reproduce the aarch64 goldens: every quantity the two
/// manifests share is byte-identical (no platform difference in these laws:
/// glibc `exp`/`pow`/`log` are ported, the strict oracle has no FMA), and the
/// inputs are the same files.
#[test]
fn x86_64_twins_equal_the_aarch64_goldens_where_both_record() {
    let mut shared = 0;
    for law in TWIN_LAWS {
        for fx in cases(&format!("{law}/x86_64")) {
            let name = fx.case_id().rsplit('/').next().unwrap();
            let golden = Fixture::open_by_id(&format!("material/{law}/{name}")).unwrap();
            for role in ["material", "grad_u", "mesh"] {
                assert_eq!(
                    std::fs::read(fx.input_path(role).unwrap()).unwrap(),
                    std::fs::read(golden.input_path(role).unwrap()).unwrap(),
                    "{} input {role}",
                    fx.case_id()
                );
            }
            for q in golden.manifest().quantities.iter().map(|q| q.name.clone()) {
                let a = fx.load_f64(&q).unwrap();
                let b = golden.load_f64(&q).unwrap();
                assert_eq!(a.shape, b.shape, "{} {q}", fx.case_id());
                assert_eq!(
                    bitwise_equal_count(&a.data, &b.data),
                    b.data.len(),
                    "{} {q}",
                    fx.case_id()
                );
                shared += 1;
            }
        }
    }
    // 601 over the 68 twins, + 2 × 11 for the twins of the
    // two finite-return cases added later (x86_64 goldens)
    assert_eq!(shared, 623, "shared quantities of the 70 twins");
}

/// Upstream-compat replay of every twin, bitwise, post-step records
/// included (marigo, mazars, plastic Mazars: `damage dissipated energy`,
/// `integral of sigma`, `getEnergy("dissipated")`; J2, Drucker–Prager,
/// plastic Mazars: `plastic_energy`, `d_plastic_energy` after
/// `afterSolveStep(true)` and `getEnergy("plastic")`).
#[test]
fn x86_64_twins_match_oracle_bitwise_with_post_step_records() {
    for law in TWIN_LAWS {
        assert!(replay_twins(law, UPSTREAM) > 0, "{law}");
    }
}

/// The corrected defaults on the twins, with the case selections of the
/// aarch64 default replays above: every compared value bitwise, post-step
/// records included.
#[test]
fn x86_64_twins_corrected_defaults_bitwise_with_post_step_records() {
    let defaults = Replay {
        options: LawOptions::default(),
        ..DEFAULTS
    };
    replay_twins("marigo", defaults);
    replay_twins("mazars", defaults);
    replay_twins(
        "plastic_linear_isotropic_hardening",
        Replay {
            include: j2_default_cases,
            ..defaults
        },
    );
    replay_twins(
        "plastic_drucker_prager",
        Replay {
            include: dp_default_cases,
            ..defaults
        },
    );
    replay_twins(
        "plastic_mazars",
        Replay {
            options: LawOptions {
                plastic_mazars_effective_stress: false,
                ..LawOptions::default()
            },
            include: |id| default_cases(id) && id.contains("/3d_"),
            ..DEFAULTS
        },
    );
}
