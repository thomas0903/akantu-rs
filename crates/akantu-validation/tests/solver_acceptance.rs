// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). No Akantu
// counterpart: regression tests of the harness's solver acceptance
// on the committed implicit dynamic oracle fixture. The
// stopping rule is the one of Akantu's NonLinearSolverNewtonRaphson::solve
// (src/model/common/non_linear_solver/non_linear_solver_newton_raphson.cc).

use akantu_validation::fixture::Fixture;
use akantu_validation::solver::{ConvergenceType, SolverType, StoppingCriterion};
use akantu_validation::{DiffTest, QuantityReport, Scale, ToleranceSpec};

const CASE: &str = "model/implicit_dynamic/patch_triangle_6_plastic";

struct Newton {
    iterations: Vec<i64>,
    error: Vec<f64>,
    history: Vec<Vec<f64>>,
}

fn oracle_newton(fx: &Fixture) -> Newton {
    Newton {
        iterations: fx.load_i64("newton/nb_iterations").unwrap().data,
        error: fx.load_f64("newton/error").unwrap().data,
        history: fx.load_ragged_f64("newton/residual_norm").unwrap(),
    }
}

fn out_dir() -> tempfile::TempDir {
    tempfile::tempdir().unwrap()
}

/// Relative agreement to the 6 significant digits of the table below.
fn six_digits(a: f64, b: f64) -> bool {
    (a - b).abs() <= 5e-6 * b.abs()
}

/// The fixture records the solver whose rule is checked, and the oracle's
/// own results satisfy it (they are what a faithful port reproduces).
#[test]
fn the_oracle_history_satisfies_its_recorded_stopping_rule() {
    let fx = Fixture::open_by_id(CASE).unwrap();
    let c = StoppingCriterion::from_fixture(&fx).unwrap();
    assert_eq!(c.solver_type, SolverType::NewtonRaphson);
    assert_eq!(c.convergence_type, ConvergenceType::Residual);
    assert_eq!((c.threshold, c.max_iterations), (1e-10, 50));
    let n = oracle_newton(&fx);
    let out = out_dir();
    let mut t = DiffTest::with_fixture("oh2_01::oracle_values", fx)
        .unwrap()
        .with_output_root(out.path());
    assert!(t.check_i64("newton/nb_iterations", &n.iterations));
    assert!(t.check_f64("newton/error", &n.error));
    assert!(t.check_ragged_f64("newton/residual_norm", &n.history));
    let r = t.finish().unwrap();
    let a = &r.solver_acceptance[0];
    assert!(a.passed(), "{r}");
    assert_eq!(a.steps.len(), 5);
    assert_eq!(
        a.steps.iter().map(|s| s.nb_iterations).collect::<Vec<_>>(),
        vec![1, 14, 21, 19, 21]
    );
}

/// Every model fixture that records a solver records one whose stopping
/// rule the harness can check (model_probe v3 records the type).
#[test]
fn every_recorded_model_solver_has_a_checkable_rule() {
    let root = akantu_validation::fixtures_root().join("model");
    let mut n = 0;
    let dirs = |d: &std::path::Path| {
        let mut v: Vec<_> = std::fs::read_dir(d)
            .unwrap()
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.is_dir())
            .collect();
        v.sort();
        v
    };
    for kind in dirs(&root) {
        for case in dirs(&kind) {
            let fx = Fixture::open(&case).unwrap();
            if fx.manifest().parameters.get("solver").is_some() {
                StoppingCriterion::from_fixture(&fx).unwrap_or_else(|e| panic!("{e}"));
                n += 1;
            }
        }
    }
    assert_eq!(
        n, 7,
        "4 static patch tests, 2 loaded cases, 1 implicit dynamic run"
    );
}

/// Counterexample: every final residual raised by 9e-5 N
/// (history's last entry replaced by the raised error), iteration counts
/// unchanged. Each resulting final residual exceeds 1e-10 |r_0| of its
/// step, so the port's stopping decision would be wrong at every step.
fn counterexample(n: &Newton) -> (Vec<f64>, Vec<Vec<f64>>) {
    let error: Vec<f64> = n.error.iter().map(|e| e + 9e-5).collect();
    let mut history = n.history.clone();
    for (row, e) in history.iter_mut().zip(&error) {
        *row.last_mut().unwrap() = *e;
    }
    (error, history)
}

/// Table of the counterexample: oracle final residual, solver limit
/// |r_0| * 1e-10 and accepted modified residual, per step (N), measured on
/// the aarch64 oracle (platform variant `aarch64-auto` of the case, D-073).
const TABLE_AARCH64: [(f64, f64, f64); 5] = [
    (1.05921e-9, 4.85363e-5, 9.00011e-5),
    (2.40716e-5, 8.71795e-5, 1.14072e-4),
    (6.90593e-5, 1.11730e-4, 1.59059e-4),
    (8.31825e-5, 9.58664e-5, 1.73182e-4),
    (5.73385e-5, 1.08737e-4, 1.47339e-4),
];

/// The same table for the x86_64 golden (pinned oracle,
/// `OPENBLAS_CORETYPE=Haswell`; re-measured by, the MUMPS
/// solves differ in the last bits, class X2). Iteration counts 1/14/21/19/21
/// and every "modified > limit" inequality are unchanged.
const TABLE_X86_64: [(f64, f64, f64); 5] = [
    (1.03101e-9, 4.85363e-5, 9.00010e-5),
    (2.40718e-5, 8.71795e-5, 1.14072e-4),
    (6.90593e-5, 1.11730e-4, 1.59059e-4),
    (8.31827e-5, 9.58664e-5, 1.73183e-4),
    (5.73385e-5, 1.08737e-4, 1.47339e-4),
];

/// The table of the golden values the loader selected (the
/// environment of the loaded data, not `cfg(target_arch)`:).
fn table(fx: &Fixture) -> &'static [(f64, f64, f64); 5] {
    match fx.environment().as_str() {
        "aarch64-auto" => &TABLE_AARCH64,
        "x86_64-haswell" => &TABLE_X86_64,
        env => panic!("{CASE}: no review table for the numerical environment {env}"),
    }
}

/// With the former rules — `newton/error` absolute 1e-4 N and the former
/// placeholder for the residual histories (`*`, rel 1e-8 of each step's
/// row maximum, PROPOSED 2026-11-09; tightened to rel 1e-10,
/// see `review_counterexample_is_rejected_with_the_committed_rules`) —
/// every differential check passes; solver
/// acceptance rejects every step, so the test fails.
#[test]
fn review_counterexample_is_rejected_even_with_the_former_difference_rule() {
    let fx = Fixture::open_by_id(CASE).unwrap();
    let n = oracle_newton(&fx);
    let (error, history) = counterexample(&n);
    let table = table(&fx);
    for (s, (oracle, limit, modified)) in table.iter().enumerate() {
        assert!(six_digits(n.error[s], *oracle), "step {s}");
        assert!(six_digits(n.history[s][0] * 1e-10, *limit), "step {s}");
        assert!(six_digits(error[s], *modified), "step {s}");
        assert!(error[s] > n.history[s][0] * 1e-10, "step {s}");
    }
    let out = out_dir();
    let mut t = DiffTest::with_fixture("oh2_01::counterexample_former_rule", fx)
        .unwrap()
        .with_output_root(out.path());
    let former = ToleranceSpec {
        abs: 1e-4,
        ..ToleranceSpec::exact("former newton/error rule (absolute 1e-4 N), review counterexample")
    };
    let former_history = ToleranceSpec::abs_rel(
        0.0,
        1e-8,
        "former PROPOSED rule of fixtures/model/tolerances.json for model/implicit_dynamic/* \
         (rel 1e-8 of each step's row maximum), review counterexample",
    )
    .with_scale(Scale::MaxAbsPerStep);
    assert!(t.check_i64("newton/nb_iterations", &n.iterations));
    assert!(t.check_f64_with("newton/error", &[5], &error, former));
    assert!(t.check_ragged_f64_with("newton/residual_norm", &history, former_history));
    let f = t.finish().unwrap_err();
    assert!(
        f.report.quantities.iter().all(QuantityReport::passed),
        "the differential checks pass: {f}"
    );
    let a = &f.report.solver_acceptance[0];
    assert!(!a.passed());
    assert_eq!(a.steps.len(), 5);
    for (s, (_, limit, modified)) in table.iter().enumerate() {
        let st = &a.steps[s];
        assert!(!st.passed, "step {s} must be rejected: {st}");
        assert!(six_digits(st.final_norm, *modified), "{st}");
        assert!(six_digits(st.limit, *limit), "{st}");
        assert!(
            st.problems.iter().any(|p| p.contains("did not converge")),
            "{st}"
        );
    }
    let text = f.to_string();
    assert!(text.contains("[FAIL] solver acceptance"), "{text}");
}

/// With the committed rules, the counterexample also fails the tightened
/// `newton/error` diagnostic (1e-7 N) and, since the per-step rule of the
/// residual histories was tightened to rel 1e-10 of `|r_0|`,
/// the history
/// rule: the raised final residuals of the first two steps exceed it
/// (9e-5 N > 4.85e-5 N and 8.72e-5 N, the table's limits).
/// Acceptance rejects every step independently.
#[test]
fn review_counterexample_is_rejected_with_the_committed_rules() {
    let fx = Fixture::open_by_id(CASE).unwrap();
    let n = oracle_newton(&fx);
    let (error, history) = counterexample(&n);
    let out = out_dir();
    let mut t = DiffTest::with_fixture("oh2_01::counterexample", fx)
        .unwrap()
        .with_output_root(out.path());
    assert!(t.check_i64("newton/nb_iterations", &n.iterations));
    assert!(!t.check_f64("newton/error", &error));
    assert!(!t.check_ragged_f64("newton/residual_norm", &history));
    let f = t.finish().unwrap_err();
    assert!(
        f.report.solver_acceptance[0]
            .steps
            .iter()
            .all(|s| !s.passed)
    );
    // The diagnostic rule is a difference bound of 1e-7 N, documented as
    // such (not a convergence check).
    let q = f
        .report
        .quantities
        .iter()
        .find(|q| q.quantity == "newton/error")
        .unwrap();
    assert!(
        q.tolerance.contains("1e-7") || q.tolerance.contains("1.0e-7"),
        "{}",
        q.tolerance
    );
    assert!(
        q.justification.contains("not a convergence check"),
        "{}",
        q.justification
    );
}

/// Consistency failures a port could produce while matching the oracle's
/// counts: an extra iteration whose residual already converged earlier,
/// and an error that is not the last tested residual.
#[test]
fn inconsistent_port_histories_are_rejected() {
    let fx = Fixture::open_by_id(CASE).unwrap();
    let n = oracle_newton(&fx);
    // Step 2 stops one iteration late: its penultimate residual already
    // satisfied the criterion.
    let mut history = n.history.clone();
    let mut iterations = n.iterations.clone();
    let mut error = n.error.clone();
    let last = *history[1].last().unwrap();
    history[1].push(last * 0.3);
    iterations[1] += 1;
    error[1] = last * 0.3;
    let out = out_dir();
    let mut t = DiffTest::with_fixture("oh2_01::late_stop", fx.clone())
        .unwrap()
        .with_output_root(out.path());
    assert!(!t.check_i64("newton/nb_iterations", &iterations));
    t.check_f64("newton/error", &error);
    assert!(!t.check_ragged_f64("newton/residual_norm", &history));
    let f = t.finish().unwrap_err();
    let st = &f.report.solver_acceptance[0].steps[1];
    assert!(
        st.problems.iter().any(|p| p.contains("already satisfies")),
        "{st}"
    );
    // The error is not the last tested residual (one ulp off).
    let mut error = n.error.clone();
    error[3] = f64::from_bits(error[3].to_bits() + 1);
    let mut t = DiffTest::with_fixture("oh2_01::error_mismatch", fx)
        .unwrap()
        .with_output_root(out.path());
    assert!(t.check_i64("newton/nb_iterations", &n.iterations));
    assert!(t.check_f64("newton/error", &error));
    assert!(t.check_ragged_f64("newton/residual_norm", &n.history));
    let f = t.finish().unwrap_err();
    let a = &f.report.solver_acceptance[0];
    assert_eq!(a.steps.iter().filter(|s| !s.passed).count(), 1);
    assert!(
        a.steps[3].problems[0].contains("differs from the last tested norm"),
        "{}",
        a.steps[3]
    );
}
