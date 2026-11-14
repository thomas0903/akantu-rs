// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). The stopping rule
// checked here is the one of Akantu's
// src/model/common/non_linear_solver/non_linear_solver_newton_raphson.cc
// (`NonLinearSolverNewtonRaphson::solve` / `testConvergence`); this file
// belongs to the differential-testing harness (docs/NUMERICAL_VALIDATION.md,
// "Solver acceptance").

//! Solver acceptance: does the port's **own** nonlinear solve satisfy the
//! stopping rule of the solver recorded in the fixture?
//!
//! Differential tolerances bound the *difference* between the port and the
//! oracle; they are not upper bounds on the port's residuals, so equal
//! iteration counts plus a residual within some distance of the oracle's do
//! not show that a stopping criterion was met (adding
//! 9e-5 N to every final residual of the implicit fixture passed every
//! differential check although no step then satisfied
//! `|r_n| < 1e-10 |r_0|`). This module checks, step by step and on the
//! port's actual values only:
//!
//! * the stopping decision of Akantu's Newton–Raphson solver for the
//!   recorded `parameters.solver` (`type`, `convergence_type`, `threshold`,
//!   `max_iterations`): converged at the last iteration, not before, within
//!   the iteration limit;
//! * the mutual consistency of the iteration count, the norm history and
//!   the solver error (history length, error equal to the last tested norm).
//!
//! Akantu's rule (`non_linear_solver_newton_raphson.cc:92-224`, norms are
//! 2-norms over free DOFs, `testConvergence` returns `error < criterion`):
//!
//! * `residual`: `r_0` is tested against the **absolute** threshold first
//!   (converged with 0 iterations when `|r_0| < threshold`); otherwise the
//!   criterion becomes `|r_0| * threshold` (one binary64 product) and every
//!   iteration `k = 1..n` tests `|r_k| < |r_0| * threshold`;
//! * `solution`: every iteration `k = 1..n` tests the increment norm
//!   `|du_k| < threshold` (absolute), at least one iteration;
//! * the loop stops at the first converged test or after `max_iterations`
//!   iterations; a step that did not converge throws, so an `ok` fixture
//!   step converged.

use std::fmt;

use serde::Serialize;

use crate::error::{Error, Result};
use crate::fixture::Fixture;
use crate::report::ser_f64;

/// Non-linear solver types whose stopping rule is
/// `NonLinearSolverNewtonRaphson::solve` (they differ only in when the
/// Jacobian is reassembled). `_linear` (one solve, no convergence test),
/// `_lumped`, PETSc SNES and TAO have no rule here and are refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SolverType {
    /// `NonLinearSolverType::_newton_raphson`.
    NewtonRaphson,
    /// `NonLinearSolverType::_newton_raphson_modified`.
    NewtonRaphsonModified,
    /// `NonLinearSolverType::_newton_raphson_contact`.
    NewtonRaphsonContact,
}

/// `SolveConvergenceCriteria` of Akantu.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ConvergenceType {
    /// `_residual`: residual norm relative to the step's initial residual.
    Residual,
    /// `_solution`: absolute norm of the solution increment.
    Solution,
}

/// The stopping rule of one recorded solver.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct StoppingCriterion {
    /// Solver type.
    pub solver_type: SolverType,
    /// Convergence criterion.
    pub convergence_type: ConvergenceType,
    /// `threshold` parameter.
    #[serde(serialize_with = "ser_f64")]
    pub threshold: f64,
    /// `max_iterations` parameter.
    pub max_iterations: i64,
}

impl fmt::Display for StoppingCriterion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let rule = match self.convergence_type {
            ConvergenceType::Residual => format!(
                "|r_n| < {:e} |r_0| (or |r_0| < {:e} with 0 iterations)",
                self.threshold, self.threshold
            ),
            ConvergenceType::Solution => format!("|du_n| < {:e}", self.threshold),
        };
        write!(
            f,
            "{:?} {:?}: {rule}, at most {} iterations",
            self.solver_type, self.convergence_type, self.max_iterations
        )
    }
}

impl StoppingCriterion {
    /// Parse `parameters.solver` of a fixture (`type`, `convergence_type`,
    /// `threshold`, `max_iterations`; written by `model_probe.py` v3).
    pub fn from_parameters(case_id: &str, solver: &serde_json::Value) -> Result<Self> {
        let err = |reason: String| Error::SolverCriterion {
            case_id: case_id.to_string(),
            reason,
        };
        if !solver.is_object() {
            return Err(err(
                "parameters.solver is missing: the fixture records no solver whose stopping \
                 rule could be checked"
                    .into(),
            ));
        }
        let text = |key: &str| {
            solver[key].as_str().ok_or_else(|| {
                err(format!(
                    "parameters.solver.{key} is missing or not a string (model_probe v3 records it)"
                ))
            })
        };
        let solver_type = match text("type")?.trim_start_matches('_') {
            "newton_raphson" => SolverType::NewtonRaphson,
            "newton_raphson_modified" => SolverType::NewtonRaphsonModified,
            "newton_raphson_contact" => SolverType::NewtonRaphsonContact,
            other => {
                return Err(err(format!(
                    "no stopping rule is implemented for non-linear solver type `{other}` \
                     (only the Newton-Raphson family): its convergence cannot be checked"
                )));
            }
        };
        let convergence_type = match text("convergence_type")?.trim_start_matches('_') {
            "residual" => ConvergenceType::Residual,
            "solution" => ConvergenceType::Solution,
            other => return Err(err(format!("unknown convergence_type `{other}`"))),
        };
        let threshold = solver["threshold"]
            .as_f64()
            .ok_or_else(|| err("parameters.solver.threshold is missing or not a number".into()))?;
        if !(threshold.is_finite() && threshold > 0.0) {
            return Err(err(format!(
                "threshold {threshold:e} must be finite and positive"
            )));
        }
        let max_iterations = solver["max_iterations"].as_i64().ok_or_else(|| {
            err("parameters.solver.max_iterations is missing or not an integer".into())
        })?;
        if max_iterations < 1 {
            return Err(err(format!("max_iterations {max_iterations} must be >= 1")));
        }
        Ok(Self {
            solver_type,
            convergence_type,
            threshold,
            max_iterations,
        })
    }

    /// The stopping rule recorded in a fixture's `parameters.solver`.
    pub fn from_fixture(fixture: &Fixture) -> Result<Self> {
        Self::from_parameters(fixture.case_id(), &fixture.manifest().parameters["solver"])
    }

    /// Number of norm entries one step records: `n + 1` residual norms
    /// (`r_0` and one per iteration) or `n` increment norms.
    #[must_use]
    pub fn history_len(&self, nb_iterations: usize) -> usize {
        match self.convergence_type {
            ConvergenceType::Residual => nb_iterations + 1,
            ConvergenceType::Solution => nb_iterations,
        }
    }

    /// Check one solve step of the port.
    #[must_use]
    pub fn check_step(&self, step: usize, s: &SolverStep<'_>) -> StepAcceptance {
        let mut problems = Vec::new();
        let mut out = StepAcceptance {
            step,
            nb_iterations: s.nb_iterations,
            error: s.error,
            final_norm: f64::NAN,
            limit: f64::NAN,
            passed: false,
            problems: Vec::new(),
        };
        let Ok(n) = usize::try_from(s.nb_iterations) else {
            out.problems
                .push(format!("negative iteration count {}", s.nb_iterations));
            return out;
        };
        if s.nb_iterations > self.max_iterations {
            problems.push(format!(
                "{} iterations exceed max_iterations = {} (Akantu stops there and throws)",
                s.nb_iterations, self.max_iterations
            ));
        }
        let expected_len = self.history_len(n);
        if s.history.len() != expected_len {
            problems.push(format!(
                "{} norms recorded for {n} iterations; the {:?} criterion tests {expected_len}",
                s.history.len(),
                self.convergence_type
            ));
            out.problems = problems;
            return out;
        }
        if let Some((k, v)) = s
            .history
            .iter()
            .enumerate()
            .find(|(_, v)| !(v.is_finite() && **v >= 0.0))
        {
            problems.push(format!(
                "norm {k} is {v:?}, not a finite non-negative value"
            ));
            out.problems = problems;
            return out;
        }
        // The values Akantu's loop compares, in order: (norm, criterion).
        let tests: Vec<(f64, f64)> = match self.convergence_type {
            ConvergenceType::Residual => {
                let r0 = s.history[0];
                let mut t = vec![(r0, self.threshold)];
                // convergence_criteria_normalized = error * convergence_criteria
                let normalized = r0 * self.threshold;
                t.extend(s.history[1..].iter().map(|&r| (r, normalized)));
                t
            }
            ConvergenceType::Solution => s.history.iter().map(|&d| (d, self.threshold)).collect(),
        };
        if tests.is_empty() {
            problems.push(
                "no convergence test: the solution criterion needs at least one iteration".into(),
            );
            out.problems = problems;
            return out;
        }
        let (last, limit) = tests[tests.len() - 1];
        out.final_norm = last;
        out.limit = limit;
        for (k, &(v, c)) in tests[..tests.len() - 1].iter().enumerate() {
            if v < c {
                problems.push(format!(
                    "norm {k} = {v:e} < {c:e} already satisfies the criterion: Akantu stops \
                     there, so {n} iterations cannot be its stopping decision"
                ));
            }
        }
        if last >= limit {
            problems.push(format!(
                "final norm {last:e} >= criterion {limit:e}: the step did not converge"
            ));
        }
        if s.error.to_bits() != last.to_bits() {
            problems.push(format!(
                "solver error {:e} differs from the last tested norm {last:e} (Akantu's \
                 `error` is that norm)",
                s.error
            ));
        }
        out.passed = problems.is_empty();
        out.problems = problems;
        out
    }

    /// Check every step of the port's solve.
    #[must_use]
    pub fn check(&self, steps: &[SolverStep<'_>]) -> Vec<StepAcceptance> {
        steps
            .iter()
            .enumerate()
            .map(|(i, s)| self.check_step(i, s))
            .collect()
    }
}

/// The port's own results for one solve step.
#[derive(Debug, Clone, Copy)]
pub struct SolverStep<'a> {
    /// Iterations performed (`nb_iterations`).
    pub nb_iterations: i64,
    /// Final solver error (`error`).
    pub error: f64,
    /// Norms tested by the criterion, in order: `residual` → `|r_0|` and one
    /// per iteration (`nb_iterations + 1` entries); `solution` → one
    /// increment norm per iteration.
    pub history: &'a [f64],
}

/// Outcome of the stopping-rule check of one step.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct StepAcceptance {
    /// Step index (position along the fixture's `step` axis).
    pub step: usize,
    /// Iterations reported by the port.
    pub nb_iterations: i64,
    /// Error reported by the port.
    #[serde(serialize_with = "ser_f64")]
    pub error: f64,
    /// Last tested norm (NaN when the history could not be evaluated).
    #[serde(serialize_with = "ser_f64")]
    pub final_norm: f64,
    /// Criterion the last norm is compared with (`|r_0| * threshold` or
    /// `threshold`).
    #[serde(serialize_with = "ser_f64")]
    pub limit: f64,
    /// `true` when the step satisfies the rule and is consistent.
    pub passed: bool,
    /// Why not.
    pub problems: Vec<String>,
}

impl fmt::Display for StepAcceptance {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "step {}: {} iterations, final norm {:e} vs criterion {:e} ({})",
            self.step,
            self.nb_iterations,
            self.final_norm,
            self.limit,
            if self.passed { "accepted" } else { "REJECTED" }
        )?;
        for p in &self.problems {
            write!(f, "; {p}")?;
        }
        Ok(())
    }
}

/// Solver-acceptance result of a differential test (one solve history).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SolverAcceptance {
    /// Where the port's values came from (`checked newton/* quantities`,
    /// or the test's explicit call).
    pub source: String,
    /// The rule applied (from the fixture), when it could be determined.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub criterion: Option<StoppingCriterion>,
    /// Per-step outcomes.
    pub steps: Vec<StepAcceptance>,
    /// Problems preventing the check (no rule in the fixture, missing port
    /// values, step count mismatch).
    pub errors: Vec<String>,
}

impl SolverAcceptance {
    /// Check `steps` against the rule recorded in `fixture`.
    #[must_use]
    pub fn evaluate(fixture: &Fixture, source: &str, steps: &[SolverStep<'_>]) -> Self {
        let mut out = Self {
            source: source.to_string(),
            criterion: None,
            steps: Vec::new(),
            errors: Vec::new(),
        };
        match StoppingCriterion::from_fixture(fixture) {
            Ok(c) => {
                out.steps = c.check(steps);
                out.criterion = Some(c);
            }
            Err(e) => out.errors.push(e.to_string()),
        }
        if steps.is_empty() {
            out.errors
                .push("no solve step given: nothing to accept".into());
        }
        if let Some(n) = fixture.manifest().steps.as_ref().map(|s| s.recorded.len())
            && n != steps.len()
        {
            out.errors.push(format!(
                "{} steps given, the fixture records {n}",
                steps.len()
            ));
        }
        out
    }

    /// A check that could not run (no rule recorded, the port's values are
    /// incomplete, ...); never passes.
    #[must_use]
    pub fn unavailable(source: &str, errors: Vec<String>) -> Self {
        Self {
            source: source.to_string(),
            criterion: None,
            steps: Vec::new(),
            errors,
        }
    }

    /// `true` when the rule could be applied and every step passed.
    #[must_use]
    pub fn passed(&self) -> bool {
        self.errors.is_empty() && !self.steps.is_empty() && self.steps.iter().all(|s| s.passed)
    }
}

impl fmt::Display for SolverAcceptance {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let n_bad = self.steps.iter().filter(|s| !s.passed).count();
        write!(
            f,
            "[{}] solver acceptance ({}): ",
            if self.passed() { "PASS" } else { "FAIL" },
            self.source
        )?;
        match &self.criterion {
            Some(c) => writeln!(f, "{c}; {} steps, {n_bad} rejected", self.steps.len())?,
            None => writeln!(f, "no rule")?,
        }
        for e in &self.errors {
            writeln!(f, "    error: {e}")?;
        }
        for s in self.steps.iter().filter(|s| !s.passed) {
            writeln!(f, "    {s}")?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn residual(threshold: f64, max_iterations: i64) -> StoppingCriterion {
        StoppingCriterion {
            solver_type: SolverType::NewtonRaphson,
            convergence_type: ConvergenceType::Residual,
            threshold,
            max_iterations,
        }
    }

    fn step(n: i64, error: f64, history: &[f64]) -> SolverStep<'_> {
        SolverStep {
            nb_iterations: n,
            error,
            history,
        }
    }

    #[test]
    fn parses_the_recorded_solver() {
        let c = StoppingCriterion::from_parameters(
            "c",
            &json!({"type": "newton_raphson", "convergence_type": "residual",
                    "threshold": 1e-10, "max_iterations": 50, "normalisation": "text"}),
        )
        .unwrap();
        assert_eq!(c, residual(1e-10, 50));
        let s = StoppingCriterion::from_parameters(
            "c",
            &json!({"type": "_newton_raphson_modified", "convergence_type": "_solution",
                    "threshold": 1e-12, "max_iterations": 2}),
        )
        .unwrap();
        assert_eq!(s.solver_type, SolverType::NewtonRaphsonModified);
        assert_eq!(s.convergence_type, ConvergenceType::Solution);
        for (bad, why) in [
            (json!(null), "is missing"),
            (
                json!({"convergence_type": "residual", "threshold": 1e-10, "max_iterations": 5}),
                "type is missing",
            ),
            (
                json!({"type": "linear", "convergence_type": "residual", "threshold": 1e-10,
                       "max_iterations": 5}),
                "no stopping rule",
            ),
            (
                json!({"type": "lumped", "convergence_type": "residual", "threshold": 1e-10,
                       "max_iterations": 5}),
                "no stopping rule",
            ),
            (
                json!({"type": "newton_raphson", "convergence_type": "increment",
                       "threshold": 1e-10, "max_iterations": 5}),
                "unknown convergence_type",
            ),
            (
                json!({"type": "newton_raphson", "convergence_type": "residual",
                       "threshold": -1.0, "max_iterations": 5}),
                "finite and positive",
            ),
            (
                json!({"type": "newton_raphson", "convergence_type": "residual",
                       "max_iterations": 5}),
                "threshold is missing",
            ),
            (
                json!({"type": "newton_raphson", "convergence_type": "residual",
                       "threshold": 1e-10, "max_iterations": 0}),
                "must be >= 1",
            ),
            (
                json!({"type": "newton_raphson", "convergence_type": "residual",
                       "threshold": 1e-10, "max_iterations": 2.5}),
                "not an integer",
            ),
        ] {
            let e = StoppingCriterion::from_parameters("x/y", &bad).unwrap_err();
            assert!(e.to_string().contains(why), "{bad}: {e}");
        }
    }

    #[test]
    fn residual_rule_follows_akantu() {
        let c = residual(1e-10, 50);
        // Linear contraction, converged at the last iteration.
        let h = [1e6, 1e2, 1e-2, 5e-5];
        let ok = c.check_step(0, &step(3, 5e-5, &h));
        assert!(ok.passed, "{ok}");
        assert_eq!(ok.limit, 1e6 * 1e-10);
        // One iteration (elastic step).
        assert!(c.check_step(0, &step(1, 1e-9, &[4.8e5, 1e-9])).passed);
        // Predictor convergence: |r_0| below the absolute threshold, 0 iterations.
        let p = c.check_step(0, &step(0, 1e-11, &[1e-11]));
        assert!(p.passed, "{p}");
        assert_eq!(p.limit, 1e-10);
        // ... but a non-zero r_0 above the absolute threshold cannot stop at 0.
        let p = c.check_step(0, &step(0, 1e-3, &[1e-3]));
        assert!(
            !p.passed && p.problems[0].contains("did not converge"),
            "{p}"
        );
        // With 1+ iterations, |r_0| below the absolute threshold would have
        // stopped at the predictor.
        let p = c.check_step(0, &step(1, 0.0, &[1e-11, 0.0]));
        assert!(
            !p.passed && p.problems[0].contains("already satisfies"),
            "{p}"
        );
        // Not converged at the end.
        let r = c.check_step(0, &step(3, 2e-4, &[1e6, 1e2, 1e-2, 2e-4]));
        assert!(
            !r.passed && r.problems[0].contains("did not converge"),
            "{r}"
        );
        // Strict inequality: equal to the criterion is not converged.
        let crit = 1e6 * 1e-10;
        assert!(!c.check_step(0, &step(1, crit, &[1e6, crit])).passed);
        // An earlier iterate already converged: more iterations than Akantu.
        let r = c.check_step(0, &step(3, 1e-6, &[1e6, 1e2, 1e-5, 1e-6]));
        assert!(!r.passed && r.problems[0].contains("norm 2"), "{r}");
        // Too many iterations.
        let r = residual(1e-10, 2).check_step(0, &step(3, 5e-5, &h));
        assert!(!r.passed && r.problems[0].contains("max_iterations"), "{r}");
    }

    #[test]
    fn iterations_history_and_error_must_be_consistent() {
        let c = residual(1e-10, 50);
        let h = [1e6, 1e2, 1e-2, 5e-5];
        // History length must be nb_iterations + 1.
        let r = c.check_step(4, &step(4, 5e-5, &h));
        assert!(
            !r.passed && r.problems[0].contains("4 norms recorded for 4"),
            "{r}"
        );
        assert_eq!(r.step, 4);
        assert!(!c.check_step(0, &step(2, 5e-5, &h)).passed);
        // The error is the last tested norm, bit for bit.
        let r = c.check_step(0, &step(3, 4e-5, &h));
        assert!(
            !r.passed && r.problems[0].contains("differs from the last"),
            "{r}"
        );
        // Non-finite / negative norms.
        assert!(!c.check_step(0, &step(1, 1e-9, &[f64::NAN, 1e-9])).passed);
        assert!(!c.check_step(0, &step(1, 1e-9, &[1e6, -1e-9])).passed);
        assert!(
            !c.check_step(0, &step(1, f64::INFINITY, &[1e6, f64::INFINITY]))
                .passed
        );
        assert!(!c.check_step(0, &step(-1, 1e-9, &[1e6])).passed);
    }

    #[test]
    fn solution_rule_is_absolute_and_needs_an_iteration() {
        let c = StoppingCriterion {
            convergence_type: ConvergenceType::Solution,
            ..residual(1e-12, 2)
        };
        // Patch test: the first increment is the solution, the second ~0.
        assert!(
            c.check_step(0, &step(2, 3.8e-19, &[2.1e-3, 3.8e-19]))
                .passed
        );
        let r = c.check_step(0, &step(0, 0.0, &[]));
        assert!(
            !r.passed && r.problems[0].contains("at least one iteration"),
            "{r}"
        );
        assert!(!c.check_step(0, &step(2, 2e-12, &[2.1e-3, 2e-12])).passed);
        assert!(!c.check_step(0, &step(2, 1e-19, &[1e-13, 1e-19])).passed);
        assert!(
            !c.check_step(0, &step(3, 1e-19, &[1e-3, 1e-6, 1e-19]))
                .passed
        );
    }
}
