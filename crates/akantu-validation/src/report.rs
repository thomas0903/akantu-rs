// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). This file has no
// direct C++ counterpart: it implements the comparison reports of the
// differential-testing harness (docs/NUMERICAL_VALIDATION.md).

//! Structured comparison reports with human-readable and JSON output.

use std::fmt;
use std::path::Path;

use serde::{Serialize, Serializer};

use crate::error::{Error, Result};
use crate::solver::SolverAcceptance;

/// Maximum number of individual violations listed in a report.
pub const MAX_LISTED_VIOLATIONS: usize = 20;

/// Serialise an `f64` as a JSON number when finite, otherwise as the
/// strings `"NaN"`, `"inf"` or `"-inf"` (JSON has no non-finite numbers).
pub fn ser_f64<S: Serializer>(x: &f64, s: S) -> std::result::Result<S::Ok, S::Error> {
    if x.is_finite() {
        s.serialize_f64(*x)
    } else if x.is_nan() {
        s.serialize_str("NaN")
    } else if *x > 0.0 {
        s.serialize_str("inf")
    } else {
        s.serialize_str("-inf")
    }
}

/// Position of an entry: flat row-major offset plus the named multi-index.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Location {
    /// Flat row-major offset.
    pub flat: usize,
    /// `(axis name, index)` pairs, outermost first.
    pub index: Vec<(String, usize)>,
}

impl fmt::Display for Location {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.index.is_empty() {
            return write!(f, "scalar");
        }
        let parts: Vec<String> = self.index.iter().map(|(a, i)| format!("{a}={i}")).collect();
        write!(f, "{}", parts.join(", "))
    }
}

/// Details of one compared entry (a violation or an extreme).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct EntryDetail {
    /// Where.
    pub location: Location,
    /// Reference value.
    #[serde(serialize_with = "ser_f64")]
    pub expected: f64,
    /// Rust value.
    #[serde(serialize_with = "ser_f64")]
    pub actual: f64,
    /// Bit pattern of the reference value (floating-point quantities).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected_bits: Option<String>,
    /// Bit pattern of the Rust value (floating-point quantities).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actual_bits: Option<String>,
    /// `|actual - expected|`.
    #[serde(serialize_with = "ser_f64")]
    pub abs_err: f64,
    /// `abs_err / |expected|` (infinite when the reference is zero and the
    /// error is not).
    #[serde(serialize_with = "ser_f64")]
    pub rel_err: f64,
    /// Allowed absolute error at this entry (`abs + rel*scale + extra`).
    #[serde(serialize_with = "ser_f64")]
    pub allowed: f64,
    /// ULP distance (floating-point quantities with finite values).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ulps: Option<u64>,
    /// Fraction of the tolerance used (`<= 1` passes).
    #[serde(serialize_with = "ser_f64")]
    pub ratio: f64,
}

impl fmt::Display for EntryDetail {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}: expected {:?} got {:?} (abs {:.3e}, rel {:.3e}",
            self.location, self.expected, self.actual, self.abs_err, self.rel_err
        )?;
        if let Some(u) = self.ulps {
            write!(f, ", {u} ulp")?;
        }
        write!(f, ", {:.3} x tolerance)", self.ratio)?;
        if let (Some(e), Some(a)) = (&self.expected_bits, &self.actual_bits)
            && self.ulps.is_some_and(|u| u <= 16)
        {
            write!(f, " [bits {e} vs {a}]")?;
        }
        Ok(())
    }
}

/// One histogram bucket.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Bucket {
    /// Bucket label (e.g. `(1e-3,1e-2]`).
    pub label: String,
    /// Number of entries.
    pub count: usize,
}

/// Statistics of the entries sharing one index along an axis.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct GroupStat {
    /// Axis name (`component`, `step`).
    pub axis: String,
    /// Index along that axis.
    pub index: usize,
    /// Label of the index (component name), if known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// Number of entries in the group.
    pub n: usize,
    /// Number of violations in the group.
    pub n_violations: usize,
    /// Largest absolute error in the group.
    #[serde(serialize_with = "ser_f64")]
    pub max_abs_err: f64,
    /// Largest fraction of tolerance used in the group.
    #[serde(serialize_with = "ser_f64")]
    pub max_ratio: f64,
}

/// Overall outcome of one quantity comparison.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    /// Every entry within tolerance.
    Pass,
    /// At least one violation.
    Fail,
    /// The arrays cannot be compared entry-wise.
    ShapeMismatch,
    /// The tolerance cannot be applied to this quantity (malformed rule, or
    /// a grouped scale on an axis the quantity does not have).
    InvalidTolerance,
}

/// Report of the comparison of one quantity.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct QuantityReport {
    /// Fixture case id.
    pub case_id: String,
    /// Quantity name.
    pub quantity: String,
    /// Outcome.
    pub outcome: Outcome,
    /// Reference shape.
    pub expected_shape: Vec<usize>,
    /// Shape of the Rust result.
    pub actual_shape: Vec<usize>,
    /// Axis names (from the fixture).
    pub axes: Vec<String>,
    /// Tolerance summary (`exact`, `4 ulp`, ...).
    pub tolerance: String,
    /// Where the tolerance came from.
    pub tolerance_origin: String,
    /// Justification of the tolerance.
    pub justification: String,
    /// `true` when the rule carries an independent `reviewed_by`; proposals
    /// (`false`) are flagged in reports and are not evidence for a
    /// *Verified* parity claim.
    pub tolerance_reviewed: bool,
    /// Number of compared entries.
    pub n_compared: usize,
    /// Number of entries outside tolerance (including NaN/Inf mismatches).
    pub n_violations: usize,
    /// Entries with bit-identical (or both-zero) values.
    pub n_exact: usize,
    /// NaN present on one side only (or forbidden NaNs).
    pub n_nan_mismatch: usize,
    /// Infinity mismatches.
    pub n_inf_mismatch: usize,
    /// Entries whose reference is exactly zero (relative error undefined).
    pub n_zero_reference: usize,
    /// Entry with the largest absolute error.
    pub max_abs_err: Option<EntryDetail>,
    /// Entry with the largest relative error (non-zero references only).
    pub max_rel_err: Option<EntryDetail>,
    /// Entry using the largest fraction of the tolerance.
    pub worst: Option<EntryDetail>,
    /// Entry with the largest ULP distance.
    pub max_ulp: Option<EntryDetail>,
    /// First violations in storage order (at most
    /// [`MAX_LISTED_VIOLATIONS`]).
    pub violations: Vec<EntryDetail>,
    /// Histogram of `error / allowed`.
    pub ratio_histogram: Vec<Bucket>,
    /// Histogram of ULP distances (floating-point quantities).
    pub ulp_histogram: Vec<Bucket>,
    /// Per-component statistics (when there is a `component` axis).
    pub per_component: Vec<GroupStat>,
    /// Per-step statistics (when there is a `step` axis, or ragged rows
    /// along `step`).
    pub per_step: Vec<GroupStat>,
    /// Statistics per acceptance group when the tolerance scale groups by
    /// axes other than `component` / `step` alone (e.g. tensor components
    /// `row, col`); same groups as the acceptance criterion.
    pub per_group: Vec<GroupStat>,
    /// First step index containing a violation.
    pub first_failing_step: Option<usize>,
    /// Extra information (shape mismatch explanation, notes).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

impl QuantityReport {
    /// `true` when the quantity passed.
    #[must_use]
    pub fn passed(&self) -> bool {
        self.outcome == Outcome::Pass
    }

    /// Serialise to pretty JSON.
    pub fn to_json(&self) -> Result<String> {
        serde_json::to_string_pretty(self).map_err(|e| Error::json("", e))
    }

    /// One-line summary.
    #[must_use]
    pub fn one_line(&self) -> String {
        let tag = match self.outcome {
            Outcome::Pass => "PASS",
            Outcome::Fail => "FAIL",
            Outcome::ShapeMismatch => "SHAPE",
            Outcome::InvalidTolerance => "BADTOL",
        };
        let worst = self.worst.as_ref().map_or_else(String::new, |w| {
            format!(", worst {:.3} x tol at {}", w.ratio, w.location)
        });
        let status = if self.tolerance_reviewed {
            ""
        } else {
            " PROPOSED"
        };
        format!(
            "[{tag}] {} :: {} ({} entries, {} violations, tol {}{status}{worst})",
            self.case_id, self.quantity, self.n_compared, self.n_violations, self.tolerance
        )
    }
}

impl fmt::Display for QuantityReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "{}", self.one_line())?;
        writeln!(f, "  shape {:?} axes {:?}", self.expected_shape, self.axes)?;
        writeln!(
            f,
            "  tolerance: {} (from {})",
            self.tolerance, self.tolerance_origin
        )?;
        writeln!(f, "  justification: {}", self.justification)?;
        if !self.tolerance_reviewed {
            writeln!(
                f,
                "  tolerance status: PROPOSED (no independent reviewer recorded; not usable as Verified evidence)"
            )?;
        }
        if let Some(m) = &self.message {
            writeln!(f, "  note: {m}")?;
        }
        if matches!(
            self.outcome,
            Outcome::ShapeMismatch | Outcome::InvalidTolerance
        ) {
            writeln!(f, "  actual shape {:?}", self.actual_shape)?;
            return Ok(());
        }
        writeln!(
            f,
            "  {} exact, {} NaN mismatches, {} Inf mismatches, {} zero references",
            self.n_exact, self.n_nan_mismatch, self.n_inf_mismatch, self.n_zero_reference
        )?;
        for (label, e) in [
            ("max abs err", &self.max_abs_err),
            ("max rel err", &self.max_rel_err),
            ("max ulp    ", &self.max_ulp),
            ("worst      ", &self.worst),
        ] {
            if let Some(e) = e {
                writeln!(f, "  {label}: {e}")?;
            }
        }
        write_hist(f, "error/tolerance", &self.ratio_histogram)?;
        write_hist(f, "ulp distance", &self.ulp_histogram)?;
        for (name, groups) in [
            ("component", &self.per_component),
            ("step", &self.per_step),
            ("group", &self.per_group),
        ] {
            let failing: Vec<&GroupStat> = groups.iter().filter(|g| g.n_violations > 0).collect();
            if groups.is_empty() {
                continue;
            }
            if name != "step" || !failing.is_empty() {
                let shown: Vec<&GroupStat> = if name != "step" {
                    groups.iter().collect()
                } else {
                    failing.iter().take(10).copied().collect()
                };
                let parts: Vec<String> = shown
                    .iter()
                    .map(|g| {
                        format!(
                            "{}{}: max_abs {:.3e} ratio {:.3} viol {}",
                            g.label.clone().unwrap_or_else(|| g.index.to_string()),
                            if g.label.is_some() {
                                format!("[{}]", g.index)
                            } else {
                                String::new()
                            },
                            g.max_abs_err,
                            g.max_ratio,
                            g.n_violations
                        )
                    })
                    .collect();
                writeln!(f, "  per {name}: {}", parts.join("; "))?;
            }
        }
        if let Some(s) = self.first_failing_step {
            writeln!(f, "  first failing step index: {s}")?;
        }
        if !self.violations.is_empty() {
            writeln!(
                f,
                "  violations (first {} of {}):",
                self.violations.len(),
                self.n_violations
            )?;
            for v in &self.violations {
                writeln!(f, "    {v}")?;
            }
        }
        Ok(())
    }
}

fn write_hist(f: &mut fmt::Formatter<'_>, title: &str, h: &[Bucket]) -> fmt::Result {
    if h.iter().all(|b| b.count == 0) {
        return Ok(());
    }
    let parts: Vec<String> = h
        .iter()
        .filter(|b| b.count > 0)
        .map(|b| format!("{}:{}", b.label, b.count))
        .collect();
    writeln!(f, "  {title} histogram: {}", parts.join("  "))
}

/// Outcome of checking an observed error against the fixture's `expect`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ErrorObservation {
    /// `expect.status` of the fixture (`ok` or `error`).
    pub expected_status: String,
    /// `expect.error_kind`, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected_kind: Option<String>,
    /// `expect.message_contains`, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected_message: Option<String>,
    /// Error kind reported by the Rust code (namespace stripped on both
    /// sides for the comparison).
    pub observed_kind: String,
    /// Error message reported by the Rust code.
    pub observed_message: String,
    /// Whether the observation matches the expectation.
    pub passed: bool,
    /// Why it did not match.
    pub problems: Vec<String>,
}

impl fmt::Display for ErrorObservation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "[{}] observed error `{}`: {:?} (expected status {}{}{})",
            if self.passed { "PASS" } else { "FAIL" },
            self.observed_kind,
            self.observed_message,
            self.expected_status,
            self.expected_kind
                .as_deref()
                .map_or_else(String::new, |k| format!(", kind `{k}`")),
            self.expected_message
                .as_deref()
                .map_or_else(String::new, |m| format!(", message containing {m:?}")),
        )?;
        for p in &self.problems {
            write!(f, "; {p}")?;
        }
        Ok(())
    }
}

/// Report of a whole differential test (several quantities of one case).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CaseReport {
    /// Name of the Rust test.
    pub test: String,
    /// Fixture case id.
    pub case_id: String,
    /// Fixture directory.
    pub fixture_dir: String,
    /// Oracle build configuration of the fixture.
    pub build_config: String,
    /// `true` when the fixture is an error-path fixture (`expect.status =
    /// error`): the test must observe the error ([`CaseReport::error_check`]).
    pub expects_error: bool,
    /// Result of the expected-error check, when the test observed an error.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_check: Option<ErrorObservation>,
    /// Per-quantity reports.
    pub quantities: Vec<QuantityReport>,
    /// Solver acceptance: the port's own solve checked against the stopping
    /// rule recorded in the fixture. Present
    /// whenever the test checked a convergence quantity
    /// (`newton/nb_iterations`, `newton/error`, `newton/residual_norm`) or
    /// called [`crate::DiffTest::check_solver_acceptance`].
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub solver_acceptance: Vec<SolverAcceptance>,
    /// Errors that prevented a comparison (missing tolerance, unknown
    /// quantity, ...).
    pub errors: Vec<String>,
}

impl CaseReport {
    /// `true` when the test produced evidence and all of it is positive:
    /// no error, every compared quantity passed, and
    ///
    /// * for an ordinary fixture: at least one quantity (or solver
    ///   acceptance) was checked and no unexpected error was observed;
    /// * for an error-path fixture: the expected error was observed and
    ///   matched (`error_check`);
    /// * every solver-acceptance check passed: differential agreement of
    ///   solver diagnostics never substitutes for the port's own stopping
    ///   decision.
    ///
    /// A report without any check never passes.
    #[must_use]
    pub fn passed(&self) -> bool {
        if !self.errors.is_empty()
            || !self.quantities.iter().all(QuantityReport::passed)
            || !self.solver_acceptance.iter().all(SolverAcceptance::passed)
        {
            return false;
        }
        match (&self.error_check, self.expects_error) {
            (Some(e), _) => e.passed,
            (None, true) => false,
            (None, false) => !self.quantities.is_empty() || !self.solver_acceptance.is_empty(),
        }
    }

    /// Why the report does not pass, for a report without errors or failing
    /// quantities (missing evidence); `None` when it passes.
    #[must_use]
    pub fn missing_evidence(&self) -> Option<String> {
        match (&self.error_check, self.expects_error) {
            (Some(e), _) if !e.passed => Some(format!("error observation failed: {e}")),
            (None, true) => Some(
                "the fixture expects an error (expect.status = error) but the test did not \
                 observe one (call DiffTest::observe_error)"
                    .into(),
            ),
            (None, false) if self.quantities.is_empty() && self.solver_acceptance.is_empty() => {
                Some("no quantity was checked: a report without evidence cannot pass".into())
            }
            _ => None,
        }
    }

    /// Number of compared quantities whose tolerance rule is still a
    /// proposal (no `reviewed_by`).
    #[must_use]
    pub fn n_proposed_tolerances(&self) -> usize {
        self.quantities
            .iter()
            .filter(|q| !q.tolerance_reviewed)
            .count()
    }

    /// Serialise to pretty JSON.
    pub fn to_json(&self) -> Result<String> {
        let mut s = serde_json::to_string_pretty(self).map_err(|e| Error::json("", e))?;
        s.push('\n');
        Ok(s)
    }

    /// Write `report.json` and `report.txt` into `dir`.
    pub fn write(&self, dir: &Path) -> Result<()> {
        std::fs::create_dir_all(dir).map_err(|e| Error::io(dir, e))?;
        let json = dir.join("report.json");
        std::fs::write(&json, self.to_json()?).map_err(|e| Error::io(&json, e))?;
        let txt = dir.join("report.txt");
        std::fs::write(&txt, self.to_string()).map_err(|e| Error::io(&txt, e))?;
        Ok(())
    }
}

impl fmt::Display for CaseReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let n_fail = self.quantities.iter().filter(|q| !q.passed()).count();
        writeln!(
            f,
            "differential test `{}` against fixture `{}` ({}; oracle build `{}`): {}",
            self.test,
            self.case_id,
            self.fixture_dir,
            self.build_config,
            if self.passed() { "PASSED" } else { "FAILED" }
        )?;
        writeln!(
            f,
            "  {} quantities compared, {} failed, {} errors",
            self.quantities.len(),
            n_fail,
            self.errors.len()
        )?;
        for e in &self.errors {
            writeln!(f, "  error: {e}")?;
        }
        if let Some(m) = self.missing_evidence() {
            writeln!(f, "  error: {m}")?;
        } else if let Some(e) = &self.error_check {
            writeln!(f, "  {e}")?;
        }
        for a in &self.solver_acceptance {
            write!(f, "  {a}")?;
        }
        let proposed = self.n_proposed_tolerances();
        if proposed > 0 {
            writeln!(
                f,
                "  {proposed} quantity(ies) compared with PROPOSED tolerances (not independently reviewed): \
                 this report is not evidence for a Verified parity claim"
            )?;
        }
        for q in &self.quantities {
            if q.passed() {
                writeln!(f, "  {}", q.one_line())?;
            }
        }
        for q in self.quantities.iter().filter(|q| !q.passed()) {
            write!(f, "{q}")?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn non_finite_json() {
        #[derive(Serialize)]
        struct T {
            #[serde(serialize_with = "ser_f64")]
            a: f64,
            #[serde(serialize_with = "ser_f64")]
            b: f64,
            #[serde(serialize_with = "ser_f64")]
            c: f64,
            #[serde(serialize_with = "ser_f64")]
            d: f64,
        }
        let s = serde_json::to_string(&T {
            a: f64::NAN,
            b: f64::INFINITY,
            c: f64::NEG_INFINITY,
            d: 0.5,
        })
        .unwrap();
        assert_eq!(s, r#"{"a":"NaN","b":"inf","c":"-inf","d":0.5}"#);
    }

    #[test]
    fn location_display() {
        let l = Location {
            flat: 7,
            index: vec![("step".into(), 1), ("node".into(), 3)],
        };
        assert_eq!(l.to_string(), "step=1, node=3");
        let s = Location {
            flat: 0,
            index: vec![],
        };
        assert_eq!(s.to_string(), "scalar");
    }
}
