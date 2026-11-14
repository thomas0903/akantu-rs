// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Integration test of
// the example crates/akantu/examples/implicit_dynamic (port of
// examples/c++/solid_mechanics_model/implicit/implicit_dynamic.cc, upstream
// ctest `implicit_dynamic-test`).

mod examples_common;
#[path = "../examples/implicit_dynamic/main.rs"]
#[allow(dead_code)] // the example's `main` is not called by the test
mod implicit_dynamic;

use akantu::solid::SolidMechanicsModel;
use examples_common::oracle::{OracleCase, assert_csv_values_bitwise, oracle};
use examples_common::{assert_close_rel, files_below, rel_diff, scratch};
use implicit_dynamic::F;

/// `E_kin + E_pot − F u_y` of the loaded node.
fn energy_balance(model: &mut SolidMechanicsModel, node: usize) -> (f64, f64) {
    let uy = model.displacement().unwrap().as_slice()[2 * node + 1];
    let ek = model.energy("kinetic").unwrap();
    let ep = model.energy("potential").unwrap();
    (ek + ep - F * uy, F * uy)
}

/// Independent check: the trapezoidal rule (Newmark β = ¼, γ = ½) conserves
/// `½ vᵀMv + ½ uᵀKu − Fᵀu` exactly for a linear undamped system under a
/// constant load once `M a = F − K u` holds; the scheme starts from
/// `a₀ = 0` (inconsistent with the suddenly applied load), so the first
/// step stores only half of the work: from step 1 on,
/// `E_kin + E_pot = F u − ½ F u₁` (Newton converged to 1e-12 on the
/// increment of ~1e-5 displacements: the identity holds to ~1e-12 of F u;
/// measured 1e-16).
#[test]
fn implicit_dynamic_trapezoidal_energy_identity() {
    let out = scratch("implicit_dynamic");
    let mut ex = implicit_dynamic::example().for_test(&out);
    ex.steps = Some(1);
    let mut r = implicit_dynamic::run(&ex).unwrap();
    let node = r.traction_node.expect("the loaded node is local");
    assert_eq!(r.positions.len(), 1);
    let (time, u1, solution) = r.positions[0];
    assert_eq!(time, 0.0);
    assert_eq!(solution, implicit_dynamic::analytical_solution(1e-4));
    let (b1, fu1) = energy_balance(&mut r.model, node);
    assert_eq!(fu1, F * u1);
    assert!(
        (b1 + 0.5 * fu1).abs() <= 1e-12 * fu1,
        "step 1: {b1} vs −½F u₁ = {}",
        -0.5 * fu1
    );
    for step in 2..=3 {
        r.model.solve_step(None).unwrap();
        let (b, fu) = energy_balance(&mut r.model, node);
        assert!(
            (b - b1).abs() <= 1e-12 * fu,
            "step {step}: balance {b} vs {b1}"
        );
    }

    let csv = std::fs::read_to_string(out.join("position.csv")).unwrap();
    let lines: Vec<&str> = csv.lines().collect();
    assert_eq!(lines[0], "id,time,position,solution");
    assert!(lines[1].starts_with("1,0,"), "{}", lines[1]);
    assert_eq!(
        files_below(&out.join("paraview")),
        ["dynamic-VTU/dynamic_0000.vtu", "dynamic_0000.pvtu"],
        "initial dump only (then every 100 steps)"
    );
}

/// The modal solution: zero at t = 0, positive, and its static limit is
/// the mid-span deflection of the simply supported beam,
/// `F L³ / (48 E I)` (series `2/π⁴ Σ_{odd n} 1/n⁴ ≈ 1/48` truncated at
/// n = 5: within 0.2 %).
#[test]
fn implicit_dynamic_modal_solution() {
    use implicit_dynamic::analytical_solution;
    assert_eq!(analytical_solution(0.0), 0.0);
    assert!(analytical_solution(0.01) > 0.0);
    let (l, e, i) = (10.0_f64, 12e7_f64, 1.0_f64 / 12.);
    let static_deflection = F * l * l * l / (48. * e * i);
    let p4 = std::f64::consts::PI.powi(4);
    let mean = 2. * F * l * l * l / (p4 * e * i) * (1. + 1. / 81. + 1. / 625.);
    assert!((mean - static_deflection).abs() <= 2e-3 * static_deflection);
}

/// Medium tier (optimised builds only): the full upstream run to
/// t = 0.62 with the default solver — one line of `position.csv` per step,
/// a dump every 100 steps, the energy identity still exact at the end — and
/// the diagnostic comparison with the **unmodified** upstream binary
/// (fixture `implicit_dynamic`, `_auto` = MUMPS in the oracle):
/// in every dump the arrays that do not depend on the solve
/// (mesh, external force) bitwise, `position.csv`'s step and time columns
/// identical; the solved fields (MUMPS vs the port's solver) printed only.
#[cfg(not(debug_assertions))]
#[test]
fn implicit_dynamic_full_run() {
    let out = scratch("implicit_dynamic_full");
    let mut r = implicit_dynamic::run(&implicit_dynamic::example().for_test(&out)).unwrap();
    let node = r.traction_node.unwrap();
    let n = r.positions.len();
    assert!((6200..=6201).contains(&n), "{n} steps");
    let u1 = r.positions[0].1;
    let (b, fu) = energy_balance(&mut r.model, node);
    assert!(
        (b + 0.5 * F * u1).abs() <= 1e-12 * fu.abs().max(F * u1),
        "balance {b}"
    );
    let csv = std::fs::read_to_string(out.join("position.csv")).unwrap();
    assert_eq!(csv.lines().count(), n + 1);
    // .pvtu + .vtu per dump, and the .pvd of the time step set after the
    // first dump
    assert_eq!(
        files_below(&out.join("paraview")).len(),
        2 * (1 + n / 100) + 1
    );

    let o = oracle("implicit_dynamic");
    let theirs = o.kept_lines("position.csv");
    assert_eq!(theirs.len(), n + 1, "the unmodified binary's step count");
    let mut differ = 0;
    for (mine, theirs) in csv.lines().zip(&theirs) {
        let m: Vec<&str> = mine.split(',').collect();
        let t: Vec<&str> = theirs.split(',').collect();
        assert_eq!(m[..2], t[..2], "position.csv: id, time");
        differ += usize::from(m[2] != t[2]);
    }
    println!(
        "implicit_dynamic vs the unmodified binary (MUMPS): {differ}/{n} printed positions differ"
    );
    let solved = ["displacement", "velocity", "acceleration", "internal_force"];
    let mut arrays = 0;
    let mut identical = std::collections::BTreeMap::new();
    for f in files_below(&out) {
        if f.ends_with(".vtu") {
            arrays += o.assert_vtu_arrays_match(&out, &f, &|name| solved.contains(&name));
            for (name, _, _, _, payload) in examples_common::oracle::vtu_payloads(&out.join(&f)) {
                if solved.contains(&name.as_str()) {
                    let same = o.arrays[&(f.clone(), name.clone())].sha256
                        == akantu_validation::checksum::sha256_bytes(&payload);
                    *identical.entry(name).or_insert(0usize) += usize::from(same);
                }
            }
        }
    }
    println!(
        "implicit_dynamic vs the unmodified binary (MUMPS): {arrays} solve-independent arrays \
         bitwise; dumps whose solved array is identical: {identical:?} (of {})",
        1 + n / 100
    );
    assert!(arrays >= 63 * 5, "{arrays} arrays compared");
}

/// Digest of the full state after a step: SHA-256 of the little-endian
/// bytes of displacement, velocity and acceleration (node-major),
/// concatenated in that order, as the oracle variant
/// `implicit_dynamic_eigen_state` writes it to `state_sha256.txt`.
fn state_digest(model: &SolidMechanicsModel) -> akantu::core::Result<String> {
    let mut bytes = Vec::new();
    for a in [
        model.displacement()?,
        model.velocity()?,
        model.acceleration()?,
    ] {
        for v in a.as_slice() {
            bytes.extend_from_slice(&v.to_le_bytes());
        }
    }
    Ok(akantu_validation::checksum::sha256_bytes(&bytes))
}

/// The oracle's per-step state digests (`implicit_dynamic_eigen_state`,
/// pinned oracle, D-050).
fn oracle_state_digests() -> Vec<String> {
    let o = oracle("implicit_dynamic_eigen_state");
    let lines = o.kept_lines("state_sha256.txt");
    assert_eq!(lines[0], "step\tsha256(displacement|velocity|acceleration)");
    lines[1..]
        .iter()
        .enumerate()
        .map(|(k, l)| {
            let (s, h) = l.split_once('\t').expect("`<step>\t<sha256>`");
            assert_eq!(s, (k + 1).to_string());
            h.to_owned()
        })
        .collect()
}

/// The example with the Eigen solver pinned, recording the state digest of
/// every step.
fn run_eigen(
    out: &std::path::Path,
    steps: Option<usize>,
) -> (implicit_dynamic::Outcome, Vec<String>) {
    let mut ex = implicit_dynamic::example().for_test(out);
    ex.steps = steps;
    ex.sparse_solver = Some(akantu::core::enums::SparseSolverType::Eigen);
    let mut digests = Vec::new();
    let r = implicit_dynamic::run_with(&ex, |_, m| {
        digests.push(state_digest(m)?);
        Ok(())
    })
    .unwrap();
    (r, digests)
}

/// Exact rules against the oracle variants `implicit_dynamic_eigen` and
/// `implicit_dynamic_eigen_state` (Eigen pinned, D-050 pinned oracle,
/// a run-to-run difference fixed): for every step, the step number, the time and
/// the loaded node's displacement of `position_full.csv` bitwise, the modal
/// solution bitwise on x86_64 glibc (libm `cos`/`pow`), and the **full
/// state** (u, v, a of every node) bitwise through its digest.
fn compare_with_the_oracle(o: &OracleCase, positions: &[(f64, f64, f64)], digests: &[String]) {
    let lines = o.kept_lines("position_full.csv");
    assert_eq!(lines[0], "id,time,position,solution");
    assert!(lines.len() > positions.len());
    let glibc = cfg!(all(
        target_os = "linux",
        target_env = "gnu",
        target_arch = "x86_64"
    ));
    for (s, (p, l)) in positions.iter().zip(&lines[1..]).enumerate() {
        let c: Vec<&str> = l.split(',').collect();
        assert_eq!(c[0], (s + 1).to_string());
        let what = format!("position_full.csv step {}", s + 1);
        assert_csv_values_bitwise(&what, &[p.0, p.1], &c[1..3]);
        if glibc {
            assert_csv_values_bitwise(&what, &[p.2], &[c[3]]);
        }
    }
    let want = oracle_state_digests();
    assert!(want.len() >= digests.len());
    assert_eq!(digests.len(), positions.len());
    for (k, (g, w)) in digests.iter().zip(&want).enumerate() {
        assert_eq!(
            g,
            w,
            "full state (u, v, a) after step {} differs from the oracle's",
            k + 1
        );
    }
}

/// Oracle variants `implicit_dynamic_eigen{,_state}` (Eigen pinned), 2
/// steps: positions and full state of both steps bitwise (see
/// [`compare_with_the_oracle`]), the initial dump byte-identical.
#[test]
fn implicit_dynamic_eigen_matches_the_oracle() {
    let out = scratch("implicit_dynamic_eigen_oracle");
    let (r, digests) = run_eigen(&out, Some(2));
    let o = oracle("implicit_dynamic_eigen");
    compare_with_the_oracle(&o, &r.positions, &digests);
    let n = o.assert_files_match(&out, false, &|f| !f.starts_with("paraview/"));
    assert_eq!(n, 2);
}

/// Medium tier: the whole run with Eigen pinned — all 6,201 steps exact
/// (positions and full state, see [`compare_with_the_oracle`]) and every
/// array of every dump (63) bitwise, the solved fields included.
#[cfg(not(debug_assertions))]
#[test]
fn implicit_dynamic_eigen_full_run_matches_the_oracle() {
    let out = scratch("implicit_dynamic_eigen_full_oracle");
    let (r, digests) = run_eigen(&out, None);
    let o = oracle("implicit_dynamic_eigen");
    assert_eq!(
        o.kept_lines("position_full.csv").len(),
        r.positions.len() + 1
    );
    assert_eq!(oracle_state_digests().len(), r.positions.len());
    compare_with_the_oracle(&o, &r.positions, &digests);
    let mut arrays = 0;
    for f in files_below(&out) {
        if f.ends_with(".vtu") {
            arrays += o.assert_vtu_arrays_match(&out, &f, &|_| false);
        }
    }
    assert_eq!(arrays, 63 * 9, "{arrays} arrays compared");
    assert_eq!(
        o.assert_files_match(&out, false, &|f| !f.starts_with("paraview/")),
        2 * 63 + 1
    );
}

/// A NaN never passes a tolerance rule of
/// the example tests — on either side, matched or not, even when every
/// value is NaN (the former helper folded with `f64::max`, which skips
/// NaNs); the printed diagnostic measure reports NaN as infinite.
#[test]
fn nan_never_passes_a_tolerance_rule() {
    let fails = |a: Vec<f64>, e: Vec<f64>| {
        std::panic::catch_unwind(move || {
            assert_close_rel("nan check", &a, &e, 1e-3, "test of the helper")
        })
        .is_err()
    };
    assert!(!fails(vec![1.0, 2.0], vec![1.0, 2.0 + 1e-6]));
    assert!(fails(vec![1.0, f64::NAN], vec![1.0, 2.0]));
    assert!(fails(vec![1.0, 2.0], vec![f64::NAN, 2.0]));
    assert!(fails(vec![f64::NAN; 3], vec![f64::NAN; 3]));
    assert!(fails(vec![f64::NAN; 3], vec![1.0; 3]));
    assert!(fails(vec![f64::INFINITY], vec![1.0]));
    assert!(fails(vec![1.0, 2.1], vec![1.0, 2.0]));
    assert_eq!(rel_diff(&[f64::NAN], &[f64::NAN]), f64::INFINITY);
    assert_eq!(rel_diff(&[1.0, 2.0], &[1.0, 2.0]), 0.0);
}
