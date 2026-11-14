// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs qualification test: the
// pure-Rust faer LU backend against the bit-identical Eigen SparseLU
// translation on the non-linear histories of fixtures/solver_model/*
// (upstream toy of test/test_model/test_common/test_model_solver).

//! faer qualification on non-linear histories (`docs/ARCHITECTURE.md`,
//! docs/NUMERICAL_VALIDATION.md, faer qualification): every
//! Newton–Raphson case of the solver-model oracle is replayed with `faer`
//! and with `eigen` (bit-identical to the oracle). Convergence criteria
//! are unchanged; iteration counts, convergence flags and non-convergence
//! exceptions must be identical; errors and states may differ by the
//! backward-error class of the two LU factorisations (bounded below).
//!
//! Scope: these are the 1-D bar histories of the toy
//! model (tridiagonal systems, `κ₂ ≤ ~4·10³`); they qualify faer for these
//! histories only. A 2-D/3-D implicit history is required before
//! faer is recommended for production implicit runs.

mod common;

use akantu_validation::Fixture;
use akantu_validation::reduce::NanMinMax;

use akantu_core::enums::SparseSolverType;
use common::drivers::{run_dynamic, run_nl_static};
use common::oracle::{cases, param_str};

fn rel(a: &[f64], b: &[f64]) -> f64 {
    let scale = b
        .iter()
        .fold(0.0f64, |m, v| m.nan_max(v.abs()))
        .max(f64::MIN_POSITIVE);
    a.iter()
        .zip(b)
        .fold(0.0f64, |m, (x, y)| m.nan_max((x - y).abs() / scale))
}

/// Largest |Δerror| relative to the threshold each test compared against
/// (`threshold`: the Eigen run's normalised threshold per step — the
/// absolute 1e-10 for the solution criterion, `error₀ · 1e-10` for the
/// residual criterion): the errors after convergence
/// are round-off-level norms, so their relative difference is not
/// meaningful.
fn derr(a: &[f64], b: &[f64], threshold: &[f64]) -> f64 {
    assert_eq!(b.len(), threshold.len());
    a.iter()
        .zip(b)
        .zip(threshold)
        .fold(0.0f64, |m, ((x, y), t)| m.nan_max((x - y).abs() / t))
}

fn newton_dynamic(f: &Fixture) -> bool {
    f.manifest()
        .parameters
        .get("driver")
        .and_then(|d| d.as_str())
        == Some("dynamic")
        && param_str(f, "non_linear_solver_type") == "newton_raphson"
        && f.manifest().expect.status == akantu_validation::manifest::ExpectedStatus::Ok
}

#[test]
fn faer_histories_match_eigen() {
    let mut rows = Vec::new();
    let mut worst_state = 0.0f64;
    let mut worst_err = 0.0f64;
    let mut newton_cases = 0;
    for f in cases() {
        let cid = f.case_id().trim_start_matches("solver_model/").to_owned();
        let (iters, ok, d_err, d_state) = if newton_dynamic(&f) {
            newton_cases += 1;
            let e = run_dynamic(&f, SparseSolverType::Eigen);
            let fa = run_dynamic(&f, SparseSolverType::Faer);
            let ok = e.n_iter == fa.n_iter && e.converged == fa.converged && e.threw == fa.threw;
            let d_state = rel(&fa.u_snap, &e.u_snap)
                .nan_max(rel(&fa.v_snap, &e.v_snap))
                .nan_max(rel(&fa.a_snap, &e.a_snap))
                .nan_max(rel(&fa.energies, &e.energies));
            (
                e.n_iter.iter().sum::<i64>().to_string(),
                ok,
                derr(&fa.error, &e.error, &e.thresholds),
                d_state,
            )
        } else if cid.starts_with("nl_") && !cid.contains("dynamic") {
            let e = run_nl_static(&f, SparseSolverType::Eigen, true);
            let fa = run_nl_static(&f, SparseSolverType::Faer, true);
            let ok = e.n_iter == fa.n_iter && e.converged == fa.converged && e.threw == fa.threw;
            // the linear solver iterates nothing (one solve per step, no
            // error/iteration bookkeeping upstream, UD-196)
            let iters = if cid == "nl_linear" {
                format!("— (linear: {} solves)", e.threw.len())
            } else {
                newton_cases += 1;
                e.n_iter.iter().sum::<i64>().to_string()
            };
            (
                iters,
                ok,
                if e.thresholds.is_empty() {
                    0.0
                } else {
                    derr(&fa.error, &e.error, &e.thresholds)
                },
                rel(&fa.u, &e.u).nan_max(rel(&fa.r, &e.r)),
            )
        } else {
            continue;
        };
        assert!(ok, "{cid}: faer changes iteration counts / convergence");
        worst_state = worst_state.nan_max(d_state);
        worst_err = worst_err.nan_max(d_err);
        rows.push(format!(
            "| {cid} | {iters} | identical | {d_err:.2e} | {d_state:.2e} |"
        ));
    }
    println!(
        "| case | Newton iterations (total) | counts/flags | max |Δerror| / threshold | max rel Δstate |"
    );
    println!("|---|---|---|---|---|");
    for r in &rows {
        println!("{r}");
    }
    println!(
        "{newton_cases} Newton cases + nl_linear; scope: 1-D bar histories \
         only (tridiagonal systems, κ₂ ≤ ~4·10³); not a qualification for 2-D/3-D implicit runs"
    );
    assert!(rows.len() >= 15, "{} cases", rows.len());
    // bound: the two factorisations are backward stable on these
    // well-conditioned (cond₂ ≤ ~10⁴) systems; the state difference is the
    // accumulated LU round-off over ≤ 400 steps.
    assert!(
        worst_state < 1e-9,
        "max relative state difference {worst_state:e}"
    );
    assert!(
        worst_err < 1e-3,
        "error differences reach {worst_err:e} of the threshold"
    );
}
