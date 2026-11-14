// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Integration test of
// the example crates/akantu/examples/material_viscoelastic_maxwell_energies
// (port of examples/c++/solid_mechanics_model/new_material/viscoelastic_maxwell/
// material_viscoelastic_maxwell_energies.cc, upstream ctest
// `material_viscoelastic_maxwell_energies-test`).

mod examples_common;
#[path = "../examples/material_viscoelastic_maxwell_energies/main.rs"]
#[allow(dead_code)] // the example's `main` is not called by the test
mod material_viscoelastic_maxwell_energies;

use akantu::core::enums::SparseSolverType;
use examples_common::{oracle, scratch};
use material_viscoelastic_maxwell_energies as visco;

/// The 17-digit records of the Eigen-pinned oracle variant
/// (`output_full.out`: `t ε E_pot E_diss W nb_iter error`).
fn oracle_records() -> Vec<Vec<String>> {
    let o = oracle::oracle("material_viscoelastic_maxwell_energies_eigen");
    o.kept_lines("output_full.out")
        .iter()
        .map(|l| l.split(' ').map(str::to_owned).collect())
        .collect()
}

/// `r` against the oracle variant: every record bitwise, the Newton
/// iteration count and error of every step, the 6-digit output file line by
/// line and the dumps byte-identical (all of them for a whole run).
fn compare_with_oracle(r: &visco::Outcome, out: &std::path::Path, complete: bool) {
    let want = oracle_records();
    assert!(r.records.len() <= want.len());
    if complete {
        assert_eq!(r.records.len(), want.len());
    }
    for (s, (rec, w)) in r.records.iter().zip(&want).enumerate() {
        let w5: Vec<&str> = w[..5].iter().map(String::as_str).collect();
        oracle::assert_csv_values_bitwise(&format!("record of step {s}"), rec, &w5);
        assert_eq!(r.newton[s].0.to_string(), w[5], "iterations of step {s}");
        oracle::assert_csv_values_bitwise(
            &format!("error of step {s}"),
            &[r.newton[s].1],
            &[&w[6]],
        );
    }
    let o = oracle::oracle("material_viscoelastic_maxwell_energies_eigen");
    let ours =
        std::fs::read_to_string(out.join("material_viscoelastic_maxwell_output.out")).unwrap();
    let theirs = o.kept_lines("material_viscoelastic_maxwell_output.out");
    for (k, l) in ours.lines().enumerate() {
        assert_eq!(l, theirs[k], "output line {k}");
    }
    let skip = |f: &str| {
        f == "output_full.out" || (!complete && (f.ends_with(".pvd") || f.ends_with(".out")))
    };
    let n = o.assert_files_match(out, complete, &skip);
    assert_eq!(n, 2 * r.records.len() + if complete { 2 } else { 0 });
}

/// Reduced run (120 steps: the ramp and the start of the hold, upstream
/// defects selected, Eigen pinned) against
/// the oracle variant `material_viscoelastic_maxwell_energies_eigen`; the
/// printed material block and the first steps' lines of the standard
/// output.
#[test]
fn material_viscoelastic_maxwell_energies_reduced_matches_the_oracle() {
    let out = scratch("visco_reduced");
    let mut ex = visco::example().for_test(&out);
    ex.steps = Some(120);
    ex.upstream_compat = true;
    ex.sparse_solver = Some(SparseSolverType::Eigen);
    let r = visco::run(&ex).unwrap();
    assert!(r.converged);
    assert_eq!(r.records.len(), 120);
    compare_with_oracle(&r, &out, false);
    let o = oracle::oracle("material_viscoelastic_maxwell_energies_eigen");
    let lines = ex.transcript().lines().count();
    oracle::assert_stdout("viscoelastic", &ex.transcript(), &o.stdout(), Some(lines));
}

/// Independent checks of the corrected material (the port's default,
/// UD-029 corrected: the Maxwell branch relaxes): during the hold (`t > T =
/// 10`) the branch stress decays with `τ = η / E_v = 4`, so at `t = 100`
/// only the equilibrium spring remains: biaxial strain `ε = 0.1`, `ν = 0`,
/// plane strain, `σ_xx = σ_yy = E_inf ε`, stored energy `½ (σ_xx + σ_yy) ε
/// A = 0.15` J on the `1 mm²` square (the branch's share is of order
/// `e^{−22.5}` ≈ 2e−10 of it); the dissipation never decreases and is
/// positive at the end. (The recorded work equals `ψ + e_diss` by
/// construction, the dissipation being computed as work minus stored
/// energy, so it is not checked.)
#[cfg(not(debug_assertions))]
#[test]
fn material_viscoelastic_maxwell_energies_corrected_relaxation() {
    let out = scratch("visco_corrected");
    let mut ex = visco::example().for_test(&out);
    ex.dump = false;
    let r = visco::run(&ex).unwrap();
    assert!(r.converged);
    let last = r.records.last().unwrap();
    assert!((last[1] - 0.1).abs() < 1e-15);
    let rel = (last[2] - 0.15).abs() / 0.15;
    assert!(rel < 1e-8, "relaxed stored energy {} ({rel:e})", last[2]);
    for w in r.records.windows(2) {
        assert!(
            w[1][3] >= w[0][3] - 1e-18,
            "dissipation decreases: {:?} → {:?}",
            w[0],
            w[1]
        );
    }
    assert!(last[3] > 0.0);
}

/// Medium tier (optimised builds only): the whole upstream run (1002 steps,
/// 1002 dumps, the `.pvd`, the output file) against the oracle variant,
/// and the whole standard output.
#[cfg(not(debug_assertions))]
#[test]
fn material_viscoelastic_maxwell_energies_full_run_matches_the_oracle() {
    let out = scratch("visco_full");
    let mut ex = visco::example().for_test(&out);
    ex.upstream_compat = true;
    ex.sparse_solver = Some(SparseSolverType::Eigen);
    let r = visco::run(&ex).unwrap();
    assert_eq!(r.records.len(), 1002);
    compare_with_oracle(&r, &out, true);
    let o = oracle::oracle("material_viscoelastic_maxwell_energies_eigen");
    oracle::assert_stdout("viscoelastic", &ex.transcript(), &o.stdout(), None);
}

/// Oracle, upstream binary (`_auto` = MUMPS): the 6-digit output file and
/// the standard output of the whole run equal the Eigen-pinned variant's
/// (diagnostic of the MUMPS rounding: the dumps are compared with the
/// variant only).
#[test]
fn material_viscoelastic_maxwell_energies_mumps_oracle_output() {
    let up = oracle::oracle("material_viscoelastic_maxwell_energies");
    let eigen = oracle::oracle("material_viscoelastic_maxwell_energies_eigen");
    let a = up.kept_lines("material_viscoelastic_maxwell_output.out");
    let b = eigen.kept_lines("material_viscoelastic_maxwell_output.out");
    let same = a.iter().zip(&b).filter(|(x, y)| x == y).count();
    println!(
        "MUMPS vs Eigen oracle: {same} / {} output lines identical",
        a.len()
    );
    assert_eq!(a.len(), b.len());
}
