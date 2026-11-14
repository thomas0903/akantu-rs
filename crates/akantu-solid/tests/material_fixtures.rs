// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Differential tests of
// the elastic-family materials against the golden material-point fixtures
// produced by oracle/probes/cpp/material_probe.cc (src/model/solid_mechanics/
// material*.{hh,cc}, materials/material_elastic*, material_thermal*,
// material_elastic_linear_anisotropic*, material_elastic_orthotropic*).

//! For every case the law is created exactly as upstream
//! (`material.dat` parsed, factory by keyword, id `solid_mechanics_model:0:
//! <type>`, `initMaterial`) on the probe's one-element mesh, then the probe's
//! step is replayed with the **recorded** `grad_u`:
//! `beforeSolveStep`, `computeStress`, `computeTangentModuli`,
//! `getEnergy("potential")`, requested internals, `afterSolveStep(true)`.
//! Every quantity is compared with the fixture tolerances (`DiffTest`) **and**
//! for bit identity: the kernels replicate upstream's operation order
//! (D-008), so the elastic-family results are required to be bit-identical.

mod common;

use akantu_core::Array;
use akantu_model::constitutive_law::LawPhase;
use akantu_validation::{DiffTest, Fixture, fixtures_root};
use common::{
    NG, SimplexModel, bitwise_equal_count, col_major_to_rows, materials_from_dat_with, simplex,
};

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
#[derive(Clone, Copy, Default)]
struct Replay {
    /// Reproduce upstream defects (UD-038 thermal eigenstrain, UD-033
    /// celerity, UD-104 axes, UD-105 orthotropic update, UD-109 thermal
    /// energy): `LawOptions::upstream()`, set before `initMaterial`.
    upstream: bool,
    /// Quantities not compared (a corrected default is expected to differ
    /// from the unpatched oracle there).
    skip: &'static [&'static str],
}

/// Bit-identity summary of one case.
#[derive(Debug, Default)]
struct Bits {
    compared: usize,
    identical: usize,
}

/// Replay one fixture; returns the bit-identity counts per quantity.
fn replay(fx: &Fixture, opts: Replay) -> Vec<(String, Bits)> {
    let p = &fx.manifest().parameters;
    let dim = usize::try_from(p["spatial_dimension"].as_i64().unwrap()).unwrap();
    let ty = simplex(dim);
    let model = SimplexModel::from_msh(dim, &fx.input_path("mesh").unwrap());
    let dat = std::fs::read(fx.input_path("material").unwrap()).unwrap();
    let options = opts
        .upstream
        .then(akantu_solid::material::LawOptions::upstream);
    let mut handler = materials_from_dat_with(&model, &dat, options).unwrap();
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
    if opts.upstream
        && let Some(o) = law.options()
    {
        assert_eq!(*o, akantu_solid::material::LawOptions::upstream());
    }

    let grad_u = fx.load_f64("grad_u").unwrap();
    let nsteps = grad_u.shape[0];
    let n = dim * dim;
    let v = dim * (dim + 1) / 2;
    let mut t = DiffTest::new(
        &format!("solid::material_fixtures::{}", fx.case_id()),
        fx.case_id(),
    )
    .unwrap();
    let internals: Vec<String> = t
        .fixture()
        .quantity_names("internal/*")
        .into_iter()
        .map(|s| s.trim_start_matches("internal/").to_owned())
        .collect();

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
    let services = &model;
    for k in 0..nsteps {
        let ctx = services.ctx(LawPhase::BeforeSolveStep);
        law.before_solve_step(&ctx).unwrap();
        // the recorded FE gradient (fixture layout [step, row, col])
        let g = &grad_u.data[k * n..(k + 1) * n];
        let mut col = vec![0.0; n];
        for r in 0..dim {
            for c in 0..dim {
                col[r + c * dim] = g[r * dim + c];
            }
        }
        law.base_mut()
            .grad_u
            .get_mut(ty, NG)
            .unwrap()
            .as_mut_slice()
            .copy_from_slice(&col);
        let ctx = services.ctx(LawPhase::Stress);
        law.compute_stress(&ctx, ty, NG).unwrap();
        let s = law.base().stress.get(ty, NG).unwrap().as_slice().to_vec();
        stress.extend(col_major_to_rows(&s, dim));
        let mut tm = Array::zeros(1, v * v);
        let ctx = services.ctx(LawPhase::Tangent);
        law.compute_tangent_moduli(&ctx, ty, NG, &mut tm).unwrap();
        tangent.extend(col_major_to_rows(tm.as_slice(), v));
        let ctx = services.ctx(LawPhase::Energy);
        energy.push(law.energy_of(&ctx, "potential").unwrap());
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
            let mut found = None;
            law.for_each_internal(&mut |nm, f| {
                if nm == name
                    && let akantu_model::constitutive_law::InternalFieldRef::Real(f) = f
                {
                    found = Some(f.get(ty, NG).unwrap().tuple(0).to_vec());
                }
            });
            internal_values[i].extend(found.unwrap_or_else(|| panic!("internal {name} missing")));
        }
        let ctx = services.ctx(LawPhase::AfterSolveStep);
        law.after_solve_step(&ctx, true).unwrap();
    }

    let mut bits = Vec::new();
    let mut check = |t: &mut DiffTest, q: &str, actual: &[f64]| {
        if opts.skip.contains(&q) {
            return;
        }
        t.check_f64(q, actual);
        let reference = t.reference_f64(q).unwrap();
        bits.push((
            q.to_owned(),
            Bits {
                compared: reference.data.len(),
                identical: bitwise_equal_count(&reference.data, actual),
            },
        ));
    };
    check(&mut t, "stress", &stress);
    check(&mut t, "tangent", &tangent);
    check(&mut t, "energy/potential", &energy);
    for (q, present) in speed_present.iter().enumerate() {
        if *present {
            check(&mut t, speed_names[q], &speeds[q]);
        }
    }
    for (name, values) in internals.iter().zip(&internal_values) {
        check(&mut t, &format!("internal/{name}"), values);
    }
    t.assert_pass();
    bits
}

/// Replays every case of `law`, asserts tolerance compliance (DiffTest) and
/// bit identity of every value.
fn replay_all(law: &str, opts: Replay) -> usize {
    let cases = cases(law);
    let mut total = 0;
    for fx in &cases {
        for (q, b) in replay(fx, opts) {
            assert_eq!(
                b.identical,
                b.compared,
                "{} {q}: {} of {} values bit-identical",
                fx.case_id(),
                b.identical,
                b.compared
            );
            total += b.compared;
        }
    }
    total
}

/// Upstream behaviour (`LawOptions::upstream()`: UD-038 3-D thermal
/// eigenstrain, UD-033 celerity, UD-104 axes, UD-105 orthotropic update,
/// UD-109 thermal energy) — the unpatched oracle's.
const UPSTREAM: Replay = Replay {
    upstream: true,
    skip: &[],
};

/// The corrected defaults (`LawOptions::default()`), every quantity.
const DEFAULTS: Replay = Replay {
    upstream: false,
    skip: &[],
};

#[test]
fn elastic_material_points_match_oracle_bitwise() {
    let n = replay_all("elastic", UPSTREAM);
    assert!(n > 0, "no elastic fixture found");
    eprintln!("elastic: {n} values bit-identical");
}

#[test]
fn elastic_thermal_material_points_match_oracle_bitwise() {
    let n = replay_all("elastic_thermal", UPSTREAM);
    assert!(n > 0, "no elastic_thermal fixture found");
    eprintln!("elastic_thermal: {n} values bit-identical");
}

#[test]
fn elastic_finite_deformation_material_points_match_oracle_bitwise() {
    let n = replay_all("elastic_finite_deformation", UPSTREAM);
    assert!(n > 0, "no elastic_finite_deformation fixture found");
    eprintln!("elastic_finite_deformation: {n} values bit-identical");
}

#[test]
fn elastic_anisotropic_material_points_match_oracle_bitwise() {
    let n = replay_all("elastic_anisotropic", UPSTREAM);
    assert!(n > 0, "no elastic_anisotropic fixture found");
    eprintln!("elastic_anisotropic: {n} values bit-identical");
}

#[test]
fn elastic_orthotropic_material_points_match_oracle_bitwise() {
    let n = replay_all("elastic_orthotropic", UPSTREAM);
    assert!(n > 0, "no elastic_orthotropic fixture found");
    eprintln!("elastic_orthotropic: {n} values bit-identical");
}

/// Corrected defaults where no upstream defect applies: the isotropic
/// elastic and St-Venant–Kirchhoff cases have no thermal stress (UD-109
/// energy correction vanishes, UD-038 only concerns `epsilon_th`), so the
/// default replay is bit-identical to the unpatched oracle too.
#[test]
fn elastic_material_points_corrected_defaults_bitwise() {
    let n = replay_all("elastic", DEFAULTS) + replay_all("elastic_finite_deformation", DEFAULTS);
    assert!(n > 0);
    eprintln!("elastic + finite deformation (defaults): {n} values bit-identical");
}

/// Corrected defaults with a temperature change: stress, tangent, wave
/// speeds and the thermal internals other than `epsilon_th` (UD-038, 3-D)
/// are unchanged bit for bit; the potential energy differs by the UD-109
/// correction (tested analytically and by finite differences in
/// `review_v22_regressions.rs`).
#[test]
fn elastic_thermal_corrected_defaults_unchanged_quantities() {
    let n = replay_all(
        "elastic_thermal",
        Replay {
            upstream: false,
            skip: &["energy/potential", "internal/epsilon_th"],
        },
    );
    assert!(n > 0);
    eprintln!(
        "elastic_thermal (defaults, energy and epsilon_th excluded): {n} values bit-identical"
    );
}

/// A regression rule that only exercised the harness is not enough: the
/// regression seed `regression/material/replay_elastic_2d` is recomputed by
/// the real Rust elastic law (upstream mode) and must be bit-identical to
/// the oracle — the exact contract its tolerance rules now state.
#[test]
fn regression_replay_elastic_2d_recomputed_bitwise() {
    let dir = fixtures_root()
        .join("regression")
        .join("material")
        .join("replay_elastic_2d");
    let fx = Fixture::open(&dir).unwrap();
    let mut total = 0;
    for (q, b) in replay(&fx, UPSTREAM) {
        assert_eq!(
            b.identical, b.compared,
            "{q}: {} of {} values bit-identical",
            b.identical, b.compared
        );
        total += b.compared;
    }
    assert!(total > 0, "nothing compared");
    eprintln!("regression/material/replay_elastic_2d: {total} values bit-identical");
}
