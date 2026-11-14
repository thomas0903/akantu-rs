// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Integration test of
// the example crates/akantu/examples/new_local_material (port of
// examples/c++/solid_mechanics_model/new_material/new_local_material.cc and
// local_material_damage.{hh,cc}, upstream ctest `new_local_material-test`).

mod examples_common;
#[path = "../examples/new_local_material/main.rs"]
#[allow(dead_code)] // the example's `main` is not called by the test
mod new_local_material;

use akantu::core::{ElementType, GhostType};
use akantu::mesh::Mesh;
use akantu::model::constitutive_law::LawSpec;
use akantu::parser::ParameterRegistry;
use akantu::parser::param::ParamValue;
use examples_common::{assert_close_rel, read_vtu, scratch};
use new_local_material::local_material_damage::LocalMaterialDamage;

/// A `local_damage` law with `E = 40e9`, `ν = 0.2`, `Y_d = 500`,
/// `S_d = 5000` (the example's parameters), constants computed.
fn law() -> LocalMaterialDamage {
    let mesh = Mesh::new(2, "m").unwrap();
    let spec = LawSpec {
        spatial_dimension: 2,
        option: String::new(),
        id: "m:0:local_damage".into(),
    };
    let mut m = LocalMaterialDamage::new(&spec, &mesh).unwrap();
    for (p, v) in [("E", 40e9), ("nu", 0.2), ("Yd", 500.), ("Sd", 5000.)] {
        m.set_param_from_input(p, ParamValue::Real(v)).unwrap();
    }
    m.update_lame();
    m
}

/// The constitutive kernel: elastic below the threshold, damage
/// `d = (Y − Y_d)/S_d` above it (`Y = ½ σ₀ : ∇u`), no healing when the load
/// decreases, capped at 1, stress scaled by `1 − d`; the upstream energy
/// density (UD-472) differs from `½ σ : ∇u` by `½ tr σ`.
#[test]
fn local_damage_kernel() {
    let mut m = law();
    let (lambda, mu) = m.lame();
    assert_eq!(lambda, 0.2 * 40e9 / ((1. + 0.2) * (1. - 2. * 0.2)));
    assert_eq!(mu, 40e9 / (2. * (1. + 0.2)));
    let elastic = |g: &[f64; 4]| {
        let tr = g[0] + g[3];
        [
            lambda * tr + 2. * mu * g[0],
            mu * (g[1] + g[2]),
            mu * (g[2] + g[1]),
            lambda * tr + 2. * mu * g[3],
        ]
    };
    let y_of = |g: &[f64; 4]| {
        let s = elastic(g);
        0.5 * (0..4).map(|k| s[k] * g[k]).sum::<f64>()
    };

    // small strain: Y < Y_d, no damage
    let g = [1e-8, 2e-9, -1e-9, 3e-9];
    let (mut s, mut d) = ([0.0; 4], 0.0);
    m.stress_on_quad(2, &g, &mut s, &mut d);
    assert_eq!(d, 0.0);
    let se = elastic(&g);
    for k in 0..4 {
        assert!(
            (s[k] - se[k]).abs() <= 1e-15 * se[0].abs(),
            "{s:?} vs {se:?}"
        );
    }

    // above the threshold: d = (Y − Y_d)/S_d
    let g = [2e-4, 0.0, 0.0, 1e-4];
    let y = y_of(&g);
    assert!(y > 500.);
    m.stress_on_quad(2, &g, &mut s, &mut d);
    let want = (y - 500.) / 5000.;
    assert!((d - want).abs() <= 1e-12 * want, "damage {d} vs {want}");
    let se = elastic(&g);
    assert!((s[0] - (1. - d) * se[0]).abs() <= 1e-12 * se[0].abs());

    // unloading keeps the damage
    let d_max = d;
    m.stress_on_quad(2, &[1e-5, 0.0, 0.0, 0.0], &mut s, &mut d);
    assert_eq!(d, d_max);

    // capped at 1
    m.stress_on_quad(2, &[1e-1, 0.0, 0.0, 1e-1], &mut s, &mut d);
    assert_eq!(d, 1.0);
    assert!(s.iter().all(|&v| v == 0.0));

    // UD-472: ½ σ : (∇u − I) = ½ σ : ∇u − ½ tr σ
    let g = [1e-6, 2e-7, 3e-7, -4e-7];
    let (mut s, mut d) = ([0.0; 4], 0.0);
    m.stress_on_quad(2, &g, &mut s, &mut d);
    let corrected = m.energy_on_quad(2, &g, &s);
    m.upstream_energy = true;
    let upstream = m.energy_on_quad(2, &g, &s);
    let tr = s[0] + s[3];
    assert!((upstream - (corrected - 0.5 * tr)).abs() <= 1e-12 * tr.abs());
}

/// The example registers the law in the model's factory and runs; the
/// plate stays far below the damage threshold (`Y ~ σ²/E ≪ Y_d`), so the
/// law is linear elastic: a run with the built-in `elastic` law of the
/// same constants gives the same displacements up to the different
/// operation order of the stress (`λ tr ∇u δ + μ (∇u + ∇uᵀ)` vs
/// `2μ ε + λ tr ∇u I`), i.e. ulp-level differences propagated by 20 steps
/// of a stable linear scheme. Independent check of the corrected UD-472
/// energy: below the threshold the default
/// `½ σ : ∇u` of every step equals the built-in law's potential energy
/// `½ σ : ε` (σ symmetric) with the same rule — a quadratic form of the
/// same displacements, so twice their relative difference at most; the
/// kinetic energies likewise.
#[test]
fn new_local_material_matches_elasticity_below_threshold() {
    let steps = 20;
    let out = scratch("new_local_material");
    let mut ex = new_local_material::example().for_test(&out);
    ex.steps = Some(steps);
    let r = new_local_material::run(&ex).unwrap();
    assert_eq!(r.energies.len(), steps);
    let law = r.model.material(0).unwrap();
    assert_eq!(law.base().law_type, "local_damage");
    let mut damage = None;
    law.for_each_internal(&mut |name, f| {
        if name == "damage"
            && let akantu::model::constitutive_law::InternalFieldRef::Real(f) = f
        {
            damage = Some(
                f.get(ElementType::Triangle6, GhostType::NotGhost)
                    .unwrap()
                    .as_slice()
                    .to_vec(),
            );
        }
    });
    let damage = damage.expect("damage internal");
    assert!(!damage.is_empty() && damage.iter().all(|&d| d == 0.0));

    // the same run with the built-in elastic law
    let dat = scratch("new_local_material_elastic").join("elastic.dat");
    std::fs::write(
        &dat,
        "material elastic [\n name = concrete\n rho = 3000\n E = 40e9\n nu = 0.2\n]\n",
    )
    .unwrap();
    let mut ex_el = new_local_material::example().for_test(&out.join("elastic"));
    ex_el.steps = Some(steps);
    ex_el.dump = false;
    ex_el.input_file = dat;
    let r_el = new_local_material::run(&ex_el).unwrap();
    let rel_dt = (r.time_step - r_el.time_step).abs() / r.time_step;
    assert!(
        rel_dt <= 4. * f64::EPSILON,
        "time steps {} vs {}",
        r.time_step,
        r_el.time_step
    );
    let rule = "ulp-level operation-order differences of the stress, 20 steps of a \
                stable linear explicit scheme (NUMERICAL_VALIDATION \"Examples\")";
    let e = assert_close_rel(
        "displacement (local_damage vs elastic)",
        r.model.displacement().unwrap().as_slice(),
        r_el.model.displacement().unwrap().as_slice(),
        1e-12,
        rule,
    );
    println!("local_damage vs elastic below the threshold: displacement {e:e}");
    assert_eq!(r_el.energies.len(), steps);
    for (k, what) in [(0, "potential"), (1, "kinetic")] {
        let mine: Vec<f64> = r.energies.iter().map(|e| e[k]).collect();
        let elastic: Vec<f64> = r_el.energies.iter().map(|e| e[k]).collect();
        assert!(elastic.iter().any(|&v| v > 0.0), "{what} energies");
        let d = assert_close_rel(
            &format!("{what} energy per step (corrected UD-472 default vs elastic)"),
            &mine,
            &elastic,
            1e-12,
            rule,
        );
        println!("local_damage vs elastic below the threshold: {what} energy {d:e}");
    }

    // dumps at steps 0 (initial) and s = 0 (s % 1000 == 0)
    let vtu = read_vtu(&out.join("paraview/local_material-VTU/local_material_0001.vtu"));
    for name in ["damage", "stress", "grad_u", "internal_force"] {
        assert!(vtu.arrays.contains_key(name), "{name} dumped");
    }
}

/// Oracle (`--upstream-compat`, UD-472): energies of every step bitwise
/// against the 17-digit variant `new_local_material_full`; the dumps
/// byte-identical to the upstream binary's; the standard output (the
/// material printout `std::cout << model.getMaterial(0)`, then the energy
/// line of every 100th step) equal to the upstream binary's, whole for
/// complete runs, else up to the first energy line.
fn compare_with_oracle(
    r: &new_local_material::Outcome,
    out: &std::path::Path,
    complete: bool,
    transcript: &str,
) {
    let full = examples_common::oracle::oracle("new_local_material_full");
    let lines = full.kept_lines("energy_full.csv");
    assert_eq!(lines[0], "id,epot,ekin");
    for (s, (e, l)) in r.energies.iter().zip(&lines[1..]).enumerate() {
        let c: Vec<&str> = l.split(',').collect();
        assert_eq!(c[0], s.to_string());
        examples_common::oracle::assert_csv_values_bitwise(
            &format!("energies of step {s}"),
            e,
            &c[1..3],
        );
    }
    let o = examples_common::oracle::oracle("new_local_material");
    let lines = (!complete).then_some(18 + 1 + r.energies.len().div_ceil(100));
    examples_common::oracle::assert_stdout("new_local_material", transcript, &o.stdout(), lines);
    let skip = |f: &str| !complete && f.ends_with(".pvd");
    let n = o.assert_files_match(out, complete, &skip);
    assert!(n >= 4, "{n} files");
}

/// Oracle, 20 steps: energies bitwise, the initial and the step-0 dumps
/// byte-identical.
#[test]
fn new_local_material_matches_the_oracle() {
    let out = scratch("new_local_material_oracle");
    let mut ex = new_local_material::example().for_test(&out);
    ex.steps = Some(20);
    ex.upstream_compat = true;
    let r = new_local_material::run(&ex).unwrap();
    compare_with_oracle(&r, &out, false, &ex.transcript());
}

/// Medium tier: the whole run (10 000 steps) — every energy bitwise, the 11
/// dumps byte-identical.
#[cfg(not(debug_assertions))]
#[test]
fn new_local_material_full_run_matches_the_oracle() {
    let out = scratch("new_local_material_full_oracle");
    let mut ex = new_local_material::example().for_test(&out);
    ex.upstream_compat = true;
    let r = new_local_material::run(&ex).unwrap();
    assert_eq!(r.energies.len(), 10_000);
    compare_with_oracle(&r, &out, true, &ex.transcript());
}
