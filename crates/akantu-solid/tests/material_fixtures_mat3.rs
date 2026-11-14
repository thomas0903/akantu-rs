// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Differential tests of
// the viscoelastic, anisotropic-damage and phase-field materials
// against the golden material-point fixtures of
// oracle/probes/cpp/material_probe.cc (probe v4; src/model/solid_mechanics/
// materials/material_viscoelastic/*, material_damage/
// material_anisotropic_damage*, material_damage/material_phasefield*,
// src/model/phase_field/energy_split*).

//! Every case is replayed exactly as the probe ran it: the law is created
//! from the case's `material.dat` (factory by keyword, upstream id),
//! optional initial internals are written after `initMaterial`, then per
//! step `beforeSolveStep`, `setTimeStep(dt_k)`, prescribed internals,
//! the **recorded** `grad_u`, `computeStress` (repeated when the case asks),
//! optional `setToSteadyState`, `hasMatrixChanged("K"/"M")`, the tangent,
//! `getEnergy(...)`, density, internals, `afterSolveStep(true)` and the
//! post-step records. Quantities are compared with the fixture tolerances
//! (`DiffTest`) and for bit identity; the exceptions (a defect of the
//! reference or a corrected default) are listed per test.

mod common;
#[path = "common/timed.rs"]
mod timed;

use akantu_core::{Array, Element, ElementType, Error, Real};
use akantu_model::constitutive_law::{InternalFieldMut, InternalFieldRef, LawPhase};
use akantu_solid::Material;
use akantu_solid::material::LawOptions;
use akantu_validation::{DiffTest, Fixture, fixtures_root};
use common::{
    NG, SimplexModel, bitwise_equal_count, col_major_to_rows, materials_from_dat_with,
    materials_from_dat_with_factory_option, simplex,
};
use timed::TimedModel;

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

/// How a quantity is compared besides the fixture tolerance.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Bitwise {
    /// Every value bit-identical.
    Bits,
    /// D-046 / D-051 exact identity (`akantu_validation::exact_eq`): the
    /// bits, the sign of zero included, a default NaN of either sign
    /// matching (the cases' default NaNs, D-046 (4), are `0x7ff8…` in the
    /// aarch64 goldens and `0xfff8…` on x86_64): `anisotropic_damage`,
    /// (formerly zeros were counted by value).
    Exact,
}

/// Options of one replay.
#[derive(Clone, Copy)]
struct Replay {
    /// `LawOptions::upstream()` (set before `initMaterial`) or the defaults.
    upstream: bool,
    /// Quantities not compared (expected to differ, documented per test).
    /// `<quantity>@<case id>` restricts an entry to one case.
    skip: &'static [&'static str],
    /// Comparison of the compared quantities.
    bitwise: Bitwise,
}

/// Per-quantity bit-identity counts.
#[derive(Debug, Default)]
struct Bits {
    compared: usize,
    identical: usize,
}

fn tuple_of(law: &dyn Material, name: &str, ty: ElementType) -> Option<Vec<Real>> {
    let mut found = None;
    law.for_each_internal(&mut |nm, f| {
        if nm == name
            && let InternalFieldRef::Real(f) = f
            && let Ok(a) = f.get(ty, NG)
            && !a.is_empty()
        {
            found = Some(a.tuple(0).to_vec());
        }
    });
    found
}

fn write_internal(law: &mut dyn Material, name: &str, values: &[Real]) {
    let mut done = false;
    law.for_each_internal_mut(&mut |nm, f| {
        if nm == name
            && let InternalFieldMut::Real(f) = f
        {
            let blocks: Vec<_> = f
                .values()
                .as_map()
                .iter_all()
                .map(|(t, g, _)| (t, g))
                .collect();
            for (t, g) in blocks {
                let mut a = f.get_mut(t, g)?;
                if a.nb_component() != values.len() {
                    return Err(Error::invalid("component count"));
                }
                for c in a.as_mut_slice().chunks_exact_mut(values.len()) {
                    c.copy_from_slice(values);
                }
            }
            done = true;
        }
        Ok(())
    })
    .unwrap();
    assert!(done, "internal {name} not found");
}

/// Replay one fixture; returns bit-identity counts per compared quantity.
#[allow(clippy::too_many_lines)] // one sequential replay of the probe's loop
fn replay(fx: &Fixture, opts: Replay) -> Vec<(String, Bits)> {
    let p = &fx.manifest().parameters;
    let dim = usize::try_from(p["spatial_dimension"].as_i64().unwrap()).unwrap();
    let ty = simplex(dim);
    let model = SimplexModel::from_msh(dim, &fx.input_path("mesh").unwrap());
    let dat = std::fs::read(fx.input_path("material").unwrap()).unwrap();
    let options = opts.upstream.then(LawOptions::upstream);
    // probe v5 `--factory-option` (allocator options an input file cannot
    // spell, e.g. anisotropic_damage `mazars-drucker-prager`)
    let mut handler = match p.get("factory_option").and_then(serde_json::Value::as_str) {
        Some(option) => {
            materials_from_dat_with_factory_option(&model, &dat, options, option).unwrap()
        }
        None => materials_from_dat_with(&model, &dat, options).unwrap(),
    };
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

    let mut t = DiffTest::new(
        &format!("solid::material_fixtures_mat3::{}", fx.case_id()),
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
    let post_internals = names("post_step/internal/*", "post_step/internal/");
    let energies: Vec<String> = names("energy/*", "energy/")
        .into_iter()
        .filter(|e| e != "potential")
        .collect();
    let post_energies = names("post_step/energy/*", "post_step/energy/");
    let inits = names("input/internal_init/*", "input/internal_init/");
    let sets = names("input/internal_set/*", "input/internal_set/");
    // fixture quantity name -> upstream internal name (names with spaces)
    let internal_name = |q: &str| -> String {
        p.get("internal_names")
            .and_then(|m| m.get(q))
            .and_then(serde_json::Value::as_str)
            .unwrap_or(q)
            .to_owned()
    };
    let has_rho = t.fixture().quantity("rho").is_ok();
    let has_changed = t.fixture().quantity("matrix_changed/K").is_ok();
    let repeat = p
        .get("repeat_stress")
        .and_then(serde_json::Value::as_i64)
        .unwrap_or(1);
    let steady = p.get("steady_state_at").and_then(serde_json::Value::as_i64);
    let time_steps = t
        .fixture()
        .quantity("input/time_step")
        .is_ok()
        .then(|| t.reference_f64("input/time_step").unwrap().data);

    let mut timed = TimedModel {
        inner: &model,
        dt: 0.0,
    };
    for name in &inits {
        let v = t
            .reference_f64(&format!("input/internal_init/{name}"))
            .unwrap()
            .data;
        write_internal(law, &internal_name(name), &v);
    }
    let set_values: Vec<(String, Vec<Real>, usize)> = sets
        .iter()
        .map(|name| {
            let f = t
                .reference_f64(&format!("input/internal_set/{name}"))
                .unwrap();
            let nc = f.shape[1];
            (internal_name(name), f.data, nc)
        })
        .collect();

    let grad_u = fx.load_f64("grad_u").unwrap();
    let nsteps = grad_u.shape[0];
    let n = dim * dim;
    let v = dim * (dim + 1) / 2;
    let mut stress = Vec::new();
    let mut tangent = Vec::new();
    let mut energy = Vec::new();
    let mut extra = vec![Vec::new(); energies.len()];
    let mut post_e = vec![Vec::new(); post_energies.len()];
    let mut internal_values = vec![Vec::new(); internals.len()];
    let mut post_values = vec![Vec::new(); post_internals.len()];
    let mut rho = Vec::new();
    let (mut changed_k, mut changed_m) = (Vec::new(), Vec::new());
    for k in 0..nsteps {
        law.before_solve_step(&timed.ctx(LawPhase::BeforeSolveStep))
            .unwrap();
        if let Some(dt) = &time_steps {
            timed.dt = dt[k];
        }
        for (name, values, nc) in &set_values {
            write_internal(law, name, &values[k * nc..(k + 1) * nc]);
        }
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
        for _ in 0..repeat {
            law.compute_stress(&timed.ctx(LawPhase::Stress), ty, NG)
                .unwrap();
        }
        if steady == Some(i64::try_from(k).unwrap()) {
            law.set_to_steady_state(&timed.ctx(LawPhase::Stress), ty, NG)
                .unwrap();
        }
        if has_changed {
            let ctx = timed.ctx(LawPhase::Tangent);
            changed_k.push(if law.has_matrix_changed_in(&ctx, "K") {
                1.
            } else {
                0.
            });
            changed_m.push(if law.has_matrix_changed_in(&ctx, "M") {
                1.
            } else {
                0.
            });
        }
        let s = law.base().stress.get(ty, NG).unwrap().as_slice().to_vec();
        stress.extend(col_major_to_rows(&s, dim));
        let mut tm = Array::zeros(1, v * v);
        law.compute_tangent_moduli(&timed.ctx(LawPhase::Tangent), ty, NG, &mut tm)
            .unwrap();
        tangent.extend(col_major_to_rows(tm.as_slice(), v));
        let ctx = timed.ctx(LawPhase::Energy);
        energy.push(law.energy_of(&ctx, "potential").unwrap());
        for (i, e) in energies.iter().enumerate() {
            extra[i].push(law.energy_of(&ctx, e).unwrap());
        }
        if has_rho {
            let mut r = [0.0];
            law.rho_on_quadrature_points(Element::new(ty, 0, NG), &mut r);
            rho.push(r[0]);
        }
        for (i, name) in internals.iter().enumerate() {
            let name = internal_name(name);
            internal_values[i].extend(
                tuple_of(law, &name, ty).unwrap_or_else(|| panic!("internal {name} missing")),
            );
        }
        law.after_solve_step(&timed.ctx(LawPhase::AfterSolveStep), true)
            .unwrap();
        for (i, name) in post_internals.iter().enumerate() {
            let name = internal_name(name);
            post_values[i].extend(
                tuple_of(law, &name, ty).unwrap_or_else(|| panic!("internal {name} missing")),
            );
        }
        let ctx = timed.ctx(LawPhase::Energy);
        for (i, e) in post_energies.iter().enumerate() {
            post_e[i].push(law.energy_of(&ctx, e).unwrap());
        }
    }

    // Error-path fixtures (upstream threw at step `error_step`): compare the
    // completed steps only. The port does not reproduce errors caused by
    // upstream's uninitialised data (UD-031).
    let nrec = p
        .get("error_step")
        .and_then(serde_json::Value::as_i64)
        .map_or(nsteps, |e| usize::try_from(e).unwrap());
    let mut bits = Vec::new();
    let mut check = |t: &mut DiffTest, q: &str, actual: &[f64], per_step: usize| {
        if opts.skip.iter().any(|s| {
            let (pat, case) = s.split_once('@').map_or((*s, None), |(p, c)| (p, Some(c)));
            case.is_none_or(|c| c == fx.case_id())
                && (pat == q || (pat.ends_with('*') && q.starts_with(&pat[..pat.len() - 1])))
        }) {
            return;
        }
        let actual = &actual[..(nrec * per_step).min(actual.len())];
        t.check_f64(q, actual);
        let reference = t.reference_f64(q).unwrap();
        let identical = match opts.bitwise {
            Bitwise::Bits => bitwise_equal_count(&reference.data, actual),
            Bitwise::Exact => reference
                .data
                .iter()
                .zip(actual)
                // the same identity as the comparator's exact rule
                // (`t.check_f64` above): D-051 sign-strict, D-046 NaNs
                .filter(|(a, b)| akantu_validation::exact_eq(**a, **b))
                .count(),
        };
        if identical != reference.data.len()
            && let Some((i, (a, b))) = reference
                .data
                .iter()
                .zip(actual)
                .enumerate()
                .find(|(_, (a, b))| a.to_bits() != b.to_bits())
        {
            eprintln!(
                "{} {q}: first difference at {i}: oracle {a:e} ({:#x}), port {b:e} ({:#x})",
                fx.case_id(),
                a.to_bits(),
                b.to_bits()
            );
        }
        bits.push((
            q.to_owned(),
            Bits {
                compared: reference.data.len(),
                identical,
            },
        ));
    };
    check(&mut t, "stress", &stress, n);
    check(&mut t, "tangent", &tangent, v * v);
    if t.fixture().quantity("energy/potential").is_ok() {
        check(&mut t, "energy/potential", &energy, 1);
    }
    for (e, vals) in energies.iter().zip(&extra) {
        check(&mut t, &format!("energy/{e}"), vals, 1);
    }
    for (e, vals) in post_energies.iter().zip(&post_e) {
        check(&mut t, &format!("post_step/energy/{e}"), vals, 1);
    }
    for (name, vals) in internals.iter().zip(&internal_values) {
        let nc = vals.len() / nsteps.max(1);
        check(&mut t, &format!("internal/{name}"), vals, nc);
    }
    for (name, vals) in post_internals.iter().zip(&post_values) {
        let nc = vals.len() / nsteps.max(1);
        check(&mut t, &format!("post_step/internal/{name}"), vals, nc);
    }
    if has_rho {
        check(&mut t, "rho", &rho, 1);
    }
    if has_changed {
        check(&mut t, "matrix_changed/K", &changed_k, 1);
        check(&mut t, "matrix_changed/M", &changed_m, 1);
    }
    if t.expects_error() {
        // Upstream threw at `error_step` (anisotropic damage: "complex
        // eigenvectors" of a tensor built from uninitialised memory,
        // UD-031); the port has no such error. The completed steps were
        // compared above and their identity is asserted by the caller,
        // which is stricter than the fixture tolerance.
        eprintln!(
            "{}: upstream error at step {nrec} not reproduced (UD-031); {nrec} steps compared",
            fx.case_id()
        );
    } else {
        t.assert_pass();
    }
    bits
}

/// Replays every case of `law`; asserts tolerance compliance and identity
/// (bits or values) of every compared value. Returns the value count.
fn replay_all(law: &str, opts: Replay) -> usize {
    let cases = cases(law);
    assert!(!cases.is_empty(), "no {law} fixture found");
    let mut total = 0;
    for fx in &cases {
        for (q, b) in replay(fx, opts) {
            assert_eq!(
                b.identical,
                b.compared,
                "{} {q}: {} of {} values identical",
                fx.case_id(),
                b.identical,
                b.compared
            );
            total += b.compared;
        }
    }
    total
}

/// Replays the cases of `law` whose id satisfies `select`.
fn replay_selected(law: &str, opts: Replay, select: impl Fn(&str) -> bool) -> usize {
    let mut total = 0;
    for fx in cases(law).iter().filter(|f| select(f.case_id())) {
        for (q, b) in replay(fx, opts) {
            assert_eq!(
                b.identical,
                b.compared,
                "{} {q}: {} of {} values identical",
                fx.case_id(),
                b.identical,
                b.compared
            );
            total += b.compared;
        }
    }
    total
}

const UPSTREAM: Replay = Replay {
    upstream: true,
    skip: &[],
    bitwise: Bitwise::Bits,
};

// --- sls_deviatoric ---------------------------------------------------------

/// Upstream behaviour (UD-029 matrix-product dissipation, UD-063 history
/// advanced at every evaluation): every quantity bit-identical, including
/// three evaluations per step.
#[test]
fn sls_deviatoric_material_points_match_oracle_bitwise() {
    let n = replay_all("sls_deviatoric", UPSTREAM);
    eprintln!("sls_deviatoric (upstream mode): {n} values bit-identical");
}

/// Corrected defaults: with one evaluation per step the committed-state
/// trial (UD-063) reproduces upstream's stresses and state bit for bit; only
/// the dissipated energy (UD-029 contraction) and the potential energy
/// (two-spring free energy, verified by
/// `mv04_*` in `review_v34_regressions.rs`) differ. The repeated-
/// evaluation case differs by design (upstream advances the history three
/// times per step).
#[test]
fn sls_deviatoric_corrected_defaults_single_evaluation_bitwise() {
    let opts = Replay {
        upstream: false,
        skip: &[
            "internal/dissipated_energy",
            "energy/dissipated",
            "energy/dissipated_sls_deviatoric",
            "energy/potential",
        ],
        bitwise: Bitwise::Bits,
    };
    let n = replay_selected("sls_deviatoric", opts, |id| !id.ends_with("repeat_stress"));
    assert!(n > 0);
    eprintln!("sls_deviatoric (defaults, dissipation excluded): {n} values bit-identical");
}

// --- viscoelastic_maxwell -----------------------------------------------------

/// Upstream behaviour (UD-029: branch stresses never updated; engineering
/// shear in `epsilon_v`; plane-stress compliance in 2-D; potential energy
/// read from the stored internal): every quantity bit-identical, including
/// non-zero initial branch stresses, the `exp(−Δt/λ) == 1` branch and the
/// thermal loading of UD-327 (`*_thermal`, x86_64 cases,
/// the parent's thermal stress added to the
/// instantaneous-modulus stress, `sigma_th`/`epsilon_th` recorded).
#[test]
fn viscoelastic_maxwell_material_points_match_oracle_bitwise() {
    let n = replay_all("viscoelastic_maxwell", UPSTREAM);
    let thermal = replay_selected("viscoelastic_maxwell", UPSTREAM, |id| {
        id.ends_with("_thermal")
    });
    assert!(thermal > 0, "no thermal viscoelastic_maxwell case");
    eprintln!(
        "viscoelastic_maxwell (upstream mode): {n} values bit-identical ({thermal} in the thermal cases)"
    );
}

/// Corrected recurrence: the tangent and the stiffness flags do not depend
/// on the branch state and stay bit-identical; stresses, energies and
/// branch internals follow the recurrence (closed-form tests in
/// `test_material_viscoelastic.rs`). The thermal cases are refused by
/// default (UD-327, `maxwell_thermal_cases_are_refused_by_default`).
#[test]
fn viscoelastic_maxwell_corrected_defaults_tangent_bitwise() {
    let opts = Replay {
        upstream: false,
        skip: &["stress", "energy/*", "post_step/*", "internal/*"],
        bitwise: Bitwise::Bits,
    };
    let n = replay_selected("viscoelastic_maxwell", opts, |id| !id.ends_with("_thermal"));
    assert!(n > 0);
    eprintln!("viscoelastic_maxwell (defaults, tangent and flags): {n} values bit-identical");
}

/// UD-327: the corrected default refuses the
/// recorded thermal loading (`alpha ≠ 0`) at `initMaterial`, which the
/// upstream mode reproduces bit for bit above.
#[test]
fn maxwell_thermal_cases_are_refused_by_default() {
    let thermal: Vec<_> = cases("viscoelastic_maxwell")
        .into_iter()
        .filter(|f| f.case_id().ends_with("_thermal"))
        .collect();
    assert_eq!(thermal.len(), 3, "the three thermal cases");
    for fx in &thermal {
        let dim = usize::try_from(
            fx.manifest().parameters["spatial_dimension"]
                .as_i64()
                .unwrap(),
        )
        .unwrap();
        let model = SimplexModel::from_msh(dim, &fx.input_path("mesh").unwrap());
        let dat = std::fs::read(fx.input_path("material").unwrap()).unwrap();
        match materials_from_dat_with(&model, &dat, None) {
            Err(Error::Unsupported(m)) => assert!(m.contains("alpha"), "{}: {m}", fx.case_id()),
            Err(e) => panic!("{}: unexpected error {e}", fx.case_id()),
            Ok(_) => panic!("{}: thermal loading accepted by default", fx.case_id()),
        }
    }
}

// --- phasefield ---------------------------------------------------------------

#[test]
fn phasefield_no_split_material_points_match_oracle_bitwise() {
    let n = replay_all("phasefield_no_split", UPSTREAM);
    eprintln!("phasefield no_split (upstream mode): {n} values bit-identical");
}

#[test]
fn phasefield_volumetric_deviatoric_material_points_match_oracle_bitwise() {
    let n = replay_all("phasefield_volumetric_deviatoric", UPSTREAM);
    eprintln!("phasefield volumetric_deviatoric (upstream mode): {n} values bit-identical");
}

/// Corrected defaults without temperature change: bit-identical except the
/// 1-D volumetric–deviatoric tangent (consistent in compression with
/// damage) and, with finite deformation, the dissipation update (PK2
/// history and Green-strain increment, `LawOptions::finite_strain_work`,
/// UD-158/UD-159).
#[test]
fn phasefield_corrected_defaults_without_thermal_bitwise() {
    let all = Replay {
        upstream: false,
        skip: &[],
        bitwise: Bitwise::Bits,
    };
    let plain = |id: &str| !id.ends_with("_thermal") && !id.ends_with("_finite_deformation");
    let mut n = replay_selected("phasefield_no_split", all, plain);
    n += replay_selected("phasefield_volumetric_deviatoric", all, |id| {
        plain(id) && !id.contains("/1d_")
    });
    let one_d = Replay {
        upstream: false,
        skip: &["tangent"],
        bitwise: Bitwise::Bits,
    };
    n += replay_selected("phasefield_volumetric_deviatoric", one_d, |id| {
        id.contains("/1d_")
    });
    let finite = Replay {
        upstream: false,
        skip: &[
            "energy/dissipated",
            "post_step/*",
            "internal/damage_dissipated_energy",
            "internal/integral_of_sigma",
        ],
        bitwise: Bitwise::Bits,
    };
    for law in ["phasefield_no_split", "phasefield_volumetric_deviatoric"] {
        n += replay_selected(law, finite, |id| id.ends_with("_finite_deformation"));
    }
    eprintln!("phasefield (defaults, no thermal): {n} values bit-identical");
}

/// Corrected defaults with a temperature change in 2-D (plane strain keeps
/// the `(1 + ν)` eigenstrain, UD-038 only concerns 3-D): stresses and
/// internals other than the energy bookkeeping are bit-identical (the
/// accumulated work `integral of sigma` and the dissipation include the
/// reversible thermal work by default, verified
/// analytically by `mv03_*` in `test_material_phasefield.rs`); the energy
/// follows UD-109 (stored energy of the split), and the
/// volumetric–deviatoric tangent tests the sign of the thermal-corrected
/// trace (it differs from upstream at the steps where `tr ε > 0` but
/// `tr(ε − ε_th I) ≤ 0`).
#[test]
fn phasefield_corrected_defaults_thermal_2d() {
    let no_split = Replay {
        upstream: false,
        skip: &[
            "energy/*",
            "post_step/*",
            "internal/damage_dissipated_energy",
            "internal/integral_of_sigma",
        ],
        bitwise: Bitwise::Bits,
    };
    let vol_dev = Replay {
        upstream: false,
        skip: &[
            "energy/*",
            "post_step/*",
            "internal/damage_dissipated_energy",
            "internal/integral_of_sigma",
            "tangent",
        ],
        bitwise: Bitwise::Bits,
    };
    let n = replay_selected("phasefield_no_split", no_split, |id| {
        id.ends_with("2d_composed_thermal")
    }) + replay_selected("phasefield_volumetric_deviatoric", vol_dev, |id| {
        id.ends_with("2d_composed_thermal")
    });
    assert!(n > 0);
    eprintln!("phasefield (defaults, 2-D thermal): {n} values bit-identical");
}

// --- anisotropic_damage ---------------------------------------------------------

/// Upstream code (UD-031): the stress reads the never-written
/// `elastic_stress` (σ ≡ ±0), `trace_damage` is never written in its
/// current slot (0), the equivalent strain is √Σ⟨ε_i⟩₊. In the three
/// unpatched-oracle cases `damage` depends on upstream's uninitialised
/// `A_diag` entries (not recorded; it differed between two identical oracle
/// runs, and 3-D paths throw at a data-dependent step, so they are not
/// probed); so does the tangent of `2d_uniaxial` (see
/// `anisotropic_damage_equivalent_strain_and_tangent_bitwise`; compared here
/// against the committed run, which equals the zero-initialised variant).
/// Every other value is compared bit for bit, the sign of the ±0 stress and
/// potential-energy records included (D-051;
/// the port reproduces the committed run's signs; zeros were counted by
/// value before; a regeneration of the unpatched oracle may flip them, their
/// defined values are compared in `anisotropic_damage_variant_ud031_bitwise`),
/// default NaNs under D-046 (those of `1d_uniaxial` and `2d_uniaxial`,
/// D-046 (4): see the cases' manifest notes).
#[test]
fn anisotropic_damage_material_points_match_oracle_where_defined() {
    let opts = Replay {
        upstream: true,
        skip: &["internal/damage"],
        bitwise: Bitwise::Exact,
    };
    let n = replay_all("anisotropic_damage_mazars", opts);
    eprintln!(
        "anisotropic_damage (upstream mode, damage excluded): {n} values exact (D-051 / D-046)"
    );
}

/// The goldens of the
/// UD-031 zero-initialisation **variant** of the oracle
/// (`oracle/patches/variants/ud031-anisotropic-damage-zero-init.patch`,
/// `fixtures/material/anisotropic_damage_mazars/variant_ud031/*`, x86_64,
/// 3 identical runs) are the defined behaviour the port's upstream mode
/// implements (`A_diag` off-diagonal entries zero): every quantity is
/// bit-identical — `damage`, the tangent of the broken state (incl. the
/// default NaNs, D-046), the `±0` stress and energy (sign-strict, D-051),
/// the 3-D paths (which the unpatched oracle could not run) and the
/// `mazars-drucker-prager` option through the C++ factory (UD-058, MAT-029).
///
/// The variant build also carries the UD-814 patch
/// (`oracle/patches/variants/ud814-eig-noconvergence-zero-init.patch`:
/// zero eigenpairs after a non-converging `EigenSolver` in
/// `MatrixBase::eig`), labelled under
/// `oracle_variant.additional_patches`; `reference.patches` lists both
/// patched files.
#[test]
fn anisotropic_damage_variant_ud031_bitwise() {
    let sha_of = |rel: &str| -> String {
        let patch = std::fs::read(fixtures_root().join("..").join(rel)).unwrap();
        <sha2::Sha256 as sha2::Digest>::digest(&patch)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect()
    };
    let sha = sha_of("oracle/patches/variants/ud031-anisotropic-damage-zero-init.patch");
    let ud814 = "oracle/patches/variants/ud814-eig-noconvergence-zero-init.patch";
    let sha814 = sha_of(ud814);
    let cases = cases("anisotropic_damage_mazars/variant_ud031");
    assert_eq!(cases.len(), 7, "the seven UD-031 variant cases");
    for fx in &cases {
        let v = &fx.manifest().parameters["oracle_variant"];
        assert_eq!(v["defect"], "UD-031", "{}", fx.case_id());
        assert_eq!(
            v["patch"],
            "oracle/patches/variants/ud031-anisotropic-damage-zero-init.patch",
            "{}",
            fx.case_id()
        );
        assert_eq!(
            v["sha256"].as_str(),
            Some(sha.as_str()),
            "{}: patch hash",
            fx.case_id()
        );
        let extra = &v["additional_patches"];
        assert_eq!(extra.as_array().map(Vec::len), Some(1), "{}", fx.case_id());
        assert_eq!(extra[0]["defect"], "UD-814", "{}", fx.case_id());
        assert_eq!(extra[0]["patch"], ud814, "{}", fx.case_id());
        assert_eq!(
            extra[0]["sha256"].as_str(),
            Some(sha814.as_str()),
            "{}: UD-814 patch hash",
            fx.case_id()
        );
        for file in [
            "material_damage/material_anisotropic_damage_tmpl.hh",
            "src/common/aka_types.hh",
        ] {
            let patched = fx
                .manifest()
                .reference
                .patches
                .iter()
                .any(|p| p.contains(file));
            assert!(
                patched,
                "{}: reference.patches must list the patched {file}",
                fx.case_id()
            );
        }
        assert!(fx.quantity("internal/damage").is_ok(), "{}", fx.case_id());
    }
    let n = replay_all("anisotropic_damage_mazars/variant_ud031", UPSTREAM);
    let dp = replay_selected("anisotropic_damage_mazars/variant_ud031", UPSTREAM, |id| {
        id.ends_with("_drucker_prager")
    });
    assert!(dp > 0);
    eprintln!(
        "anisotropic_damage UD-031 variant (upstream mode): {n} values bit-identical ({dp} with mazars-drucker-prager)"
    );
}

/// The equivalent strain and the tangent of the upstream mode are also
/// bit-identical, except the tangent of `2d_uniaxial`:
/// that case reaches
/// the broken state, whose tension branch rewrites the material-wide `E`,
/// `ν` (`material_anisotropic_damage_tmpl.hh:229-238`) when `tr D' > Dc`,
/// and `D` reads upstream's uninitialised `A_diag` entries (UD-031), so the
/// elastic tangent inherits them: three identical x86_64 oracle runs gave
/// two variants (90 of 180 finite values differ from step index 2). Like
/// `damage`, it is not compared bitwise against the unpatched oracle; its
/// defined value is compared bitwise against the zero-initialised variant
/// (`anisotropic_damage_variant_ud031_bitwise`). Its NaNs are
/// documented in the case's manifest notes (D-046 (4)). The tangents of
/// `1d_uniaxial` and `2d_composed` were byte-identical over the three runs
/// and stay bitwise.
#[test]
fn anisotropic_damage_equivalent_strain_and_tangent_bitwise() {
    let opts = Replay {
        upstream: true,
        skip: &[
            "internal/damage",
            "stress",
            "energy/potential",
            "internal/elastic_stress",
            "internal/trace_damage",
            "tangent@material/anisotropic_damage_mazars/2d_uniaxial",
        ],
        bitwise: Bitwise::Bits,
    };
    let n = replay_all("anisotropic_damage_mazars", opts);
    eprintln!("anisotropic_damage (equivalent strain, tangent): {n} values bit-identical");
}
