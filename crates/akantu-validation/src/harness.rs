// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). This file has no
// direct C++ counterpart: it implements the differential-test driver
// (docs/NUMERICAL_VALIDATION.md).

//! High-level differential-test driver.
//!
//! ```no_run
//! use akantu_validation::DiffTest;
//!
//! let mut t = DiffTest::new("fe::shapes_triangle_3", "fe/element_class/_triangle_3").unwrap();
//! let shapes: Vec<f64> = vec![/* computed by akantu-rs */];
//! t.check_f64("shapes/order_1", &shapes);
//! t.assert_pass(); // panics with an actionable report on failure
//! ```
//!
//! Each `check_*` call compares one quantity using the tolerance resolved
//! from `tolerances.json` (or an explicit, justified in-code spec) and keeps
//! going, so one run reports every failing quantity. A test must produce
//! evidence: a report with no compared quantity never passes, and an
//! error-path fixture (`expect.status = error`) passes only when the test
//! reports the error it observed through [`DiffTest::observe_error`].
//!
//! Differential accuracy and solver acceptance are separate:
//! a tolerance bounds the port's *difference* from the oracle, not
//! the port's residuals. Whenever a test checks a convergence quantity
//! ([`NEWTON_QUANTITIES`]), the report also checks the port's checked
//! values against the stopping rule recorded in the fixture
//! (`parameters.solver`, [`crate::solver`]); the test fails unless the
//! port's own iteration count, residual history and error are consistent
//! and satisfy that rule at every step. Other solver results are accepted
//! through [`DiffTest::check_solver_acceptance`].
//!
//! On failure, [`DiffTest::finish`] writes under `$AKANTU_VALIDATION_OUT`
//! (default `<workspace>/target/akantu-validation/<test>/`):
//!
//! * `report.txt` / `report.json` — the full comparison report;
//! * `actual/…` — a `rust_snapshot` fixture holding every Rust array that was
//!   checked (loadable with numpy or [`crate::Fixture::open`]);
//! * `regression/…` — a `pending_oracle` case with the inputs recorded via
//!   [`DiffTest::record_input_file`] / [`DiffTest::record_input_array`], to
//!   be promoted to a golden regression fixture.

use std::fmt;
use std::path::{Path, PathBuf};

use crate::array::Field;
use crate::compare::{CompareOptions, RaggedRows, compare_f64, compare_i64};
use crate::error::{Error, Result};
use crate::fixture::Fixture;
use crate::manifest::{ExpectedStatus, FixtureStatus, Generator, Layout, ReferenceInfo};
use crate::npy::{NpyArray, NpyData};
use crate::regression::{InputArray, RegressionCase, persist_regression_case};
use crate::report::{CaseReport, ErrorObservation, QuantityReport};
use crate::solver::{SolverAcceptance, SolverStep, StoppingCriterion};
use crate::tolerance::{ResolvedTolerance, ToleranceSpec};
use crate::writer::{FixtureWriter, sanitize};

/// Environment variable overriding the output directory of reports.
pub const OUT_DIR_ENV: &str = "AKANTU_VALIDATION_OUT";
/// When set to `1`, reports are written for passing tests too.
pub const WRITE_ALL_ENV: &str = "AKANTU_VALIDATION_WRITE_ALL";

/// Convergence quantities of a nonlinear solve (fixture format, docs/NUMERICAL_VALIDATION.md):
/// iteration counts per step, final solver error per step and the ragged
/// history of the tested norms. Checking any of them makes the report run
/// solver acceptance on the port's checked values.
pub const NEWTON_QUANTITIES: [&str; 3] = [
    "newton/nb_iterations",
    "newton/error",
    "newton/residual_norm",
];

/// Root of report output.
#[must_use]
pub fn output_root() -> PathBuf {
    if let Some(d) = std::env::var_os(OUT_DIR_ENV) {
        return PathBuf::from(d);
    }
    if let Some(t) = std::env::var_os("CARGO_TARGET_DIR") {
        return PathBuf::from(t).join("akantu-validation");
    }
    crate::fixture::workspace_root()
        .join("target")
        .join("akantu-validation")
}

/// A Rust array that was checked (kept to persist on failure).
#[derive(Debug, Clone)]
struct Checked {
    name: String,
    array: NpyArray,
    axes: Vec<String>,
    layout: Layout,
    /// Row lengths given by the test for a ragged check.
    rows: Option<Vec<usize>>,
}

/// Differential test of akantu-rs results against one golden fixture.
#[derive(Debug)]
pub struct DiffTest {
    test: String,
    fixture: Fixture,
    reports: Vec<QuantityReport>,
    errors: Vec<String>,
    error_check: Option<ErrorObservation>,
    checked: Vec<Checked>,
    /// Names of every quantity a `check_*` call was made for.
    attempted: Vec<String>,
    acceptance: Vec<SolverAcceptance>,
    inputs: RegressionCase,
    out_root: PathBuf,
}

/// A failed differential test: the report and where artefacts were written.
#[derive(Debug)]
pub struct Failure {
    /// The full report.
    pub report: CaseReport,
    /// Directory holding `report.*`, `actual/` and `regression/`.
    pub artifacts: Option<PathBuf>,
    /// Problem while writing artefacts, if any.
    pub write_error: Option<String>,
}

impl fmt::Display for Failure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.report)?;
        if let Some(a) = &self.artifacts {
            writeln!(
                f,
                "artefacts (report, Rust arrays, regression inputs): {}",
                a.display()
            )?;
        }
        if let Some(e) = &self.write_error {
            writeln!(f, "could not write artefacts: {e}")?;
        }
        Ok(())
    }
}

impl std::error::Error for Failure {}

/// Last path segment of a `::`-qualified type name (`akantu::debug::X` → `X`).
fn strip_namespace(kind: &str) -> &str {
    kind.trim().rsplit("::").next().unwrap_or(kind).trim()
}

impl DiffTest {
    /// Open golden fixture `case_id` (under [`crate::fixtures_root`]) for the
    /// Rust test `test`.
    pub fn new(test: &str, case_id: &str) -> Result<Self> {
        Self::with_fixture(test, Fixture::open_by_id(case_id)?)
    }

    /// Use an already opened fixture (must be golden and materialised).
    pub fn with_fixture(test: &str, fixture: Fixture) -> Result<Self> {
        fixture.require_golden()?;
        fixture.check_materialized()?;
        Ok(Self {
            test: test.to_string(),
            inputs: RegressionCase {
                case_id: format!(
                    "regression/{}/{}",
                    fixture.manifest().area(),
                    sanitize(test)
                ),
                description: format!(
                    "inputs of failing test `{test}` (fixture {})",
                    fixture.case_id()
                ),
                origin: format!("test `{test}`, fixture `{}`", fixture.case_id()),
                ..RegressionCase::default()
            },
            fixture,
            reports: Vec::new(),
            errors: Vec::new(),
            error_check: None,
            checked: Vec::new(),
            attempted: Vec::new(),
            acceptance: Vec::new(),
            out_root: output_root(),
        })
    }

    /// Redirect artefact output (tests of the harness itself).
    #[must_use]
    pub fn with_output_root(mut self, dir: &Path) -> Self {
        self.out_root = dir.to_path_buf();
        self
    }

    /// The fixture.
    #[must_use]
    pub fn fixture(&self) -> &Fixture {
        &self.fixture
    }

    /// `true` for an error-path fixture (`expect.status = error`): the test
    /// must call [`DiffTest::observe_error`] with the error it got.
    #[must_use]
    pub fn expects_error(&self) -> bool {
        self.fixture.manifest().expect.status == ExpectedStatus::Error
    }

    /// Load a reference array (e.g. an input such as node coordinates).
    pub fn reference_f64(&self, quantity: &str) -> Result<Field<f64>> {
        self.fixture.load_f64(quantity)
    }

    /// Load a reference integer array.
    pub fn reference_i64(&self, quantity: &str) -> Result<Field<i64>> {
        self.fixture.load_i64(quantity)
    }

    /// Load a reference ragged quantity as rows.
    pub fn reference_ragged_f64(&self, quantity: &str) -> Result<Vec<Vec<f64>>> {
        self.fixture.load_ragged_f64(quantity)
    }

    /// Compare `actual` (same shape as the reference) using the tolerance
    /// from `tolerances.json`. Returns `true` if it passed.
    pub fn check_f64(&mut self, quantity: &str, actual: &[f64]) -> bool {
        let shape = self
            .fixture
            .quantity(quantity)
            .map(|q| q.shape.clone())
            .unwrap_or_default();
        let shape = if shape.iter().product::<usize>() == actual.len() {
            shape
        } else {
            vec![actual.len()]
        };
        self.check_f64_shaped(quantity, &shape, actual)
    }

    /// Compare `actual` with an explicit shape using the tolerance from
    /// `tolerances.json`.
    pub fn check_f64_shaped(&mut self, quantity: &str, shape: &[usize], actual: &[f64]) -> bool {
        self.attempt(quantity);
        if let Err(e) = self.fixture.quantity(quantity) {
            self.keep_f64(quantity, shape, actual);
            self.errors.push(e.to_string());
            return false;
        }
        match self.fixture.tolerance(quantity) {
            Ok(tol) => self.run_f64(quantity, shape, actual, &tol),
            Err(e) => {
                self.keep_f64(quantity, shape, actual);
                self.errors.push(e.to_string());
                false
            }
        }
    }

    /// Compare a ragged quantity (e.g. a per-step Newton residual history)
    /// given as rows. The row structure must match the reference offsets;
    /// the tolerance comes from `tolerances.json` and may group by the
    /// ragged row axis (`max_abs_per_step`).
    pub fn check_ragged_f64(&mut self, quantity: &str, rows: &[Vec<f64>]) -> bool {
        self.ragged_f64(quantity, rows, None)
    }

    /// [`DiffTest::check_ragged_f64`] with an in-code tolerance (must carry a
    /// justification; use only when the bound is intrinsic to the test, as
    /// for [`DiffTest::check_f64_with`], e.g. a regression test pinned to a
    /// former rule).
    pub fn check_ragged_f64_with(
        &mut self,
        quantity: &str,
        rows: &[Vec<f64>],
        spec: ToleranceSpec,
    ) -> bool {
        self.ragged_f64(quantity, rows, Some(spec))
    }

    fn ragged_f64(
        &mut self,
        quantity: &str,
        rows: &[Vec<f64>],
        spec: Option<ToleranceSpec>,
    ) -> bool {
        self.attempt(quantity);
        let flat: Vec<f64> = rows.iter().flatten().copied().collect();
        let lengths: Vec<usize> = rows.iter().map(Vec::len).collect();
        let ok = self.compare_ragged(quantity, &flat, &lengths, spec);
        if let Some(c) = self.checked.iter_mut().find(|c| c.name == quantity) {
            c.rows = Some(lengths);
        }
        ok
    }

    fn compare_ragged(
        &mut self,
        quantity: &str,
        flat: &[f64],
        lengths: &[usize],
        spec: Option<ToleranceSpec>,
    ) -> bool {
        let expected_rows = match self.fixture.load_ragged_f64(quantity) {
            Ok(r) => r,
            Err(e) => {
                self.keep_f64(quantity, &[flat.len()], flat);
                self.errors.push(e.to_string());
                return false;
            }
        };
        let expected_lengths: Vec<usize> = expected_rows.iter().map(Vec::len).collect();
        if lengths != expected_lengths {
            self.keep_f64(quantity, &[flat.len()], flat);
            self.errors.push(format!(
                "ragged quantity `{quantity}`: row lengths {lengths:?} differ from the reference {expected_lengths:?}"
            ));
            return false;
        }
        let tol = match spec {
            Some(spec) => Ok(ResolvedTolerance {
                spec,
                origin: format!("in-code spec of test `{}`", self.test),
            }),
            None => self.fixture.tolerance(quantity),
        };
        match tol {
            Ok(tol) => self.run_f64(quantity, &[flat.len()], flat, &tol),
            Err(e) => {
                self.keep_f64(quantity, &[flat.len()], flat);
                self.errors.push(e.to_string());
                false
            }
        }
    }

    /// Compare with an in-code tolerance (must carry a justification; use
    /// only when the bound is intrinsic to the test, e.g. an analytical
    /// identity).
    pub fn check_f64_with(
        &mut self,
        quantity: &str,
        shape: &[usize],
        actual: &[f64],
        spec: ToleranceSpec,
    ) -> bool {
        self.attempt(quantity);
        let tol = ResolvedTolerance {
            spec,
            origin: format!("in-code spec of test `{}`", self.test),
        };
        self.run_f64(quantity, shape, actual, &tol)
    }

    /// Compare integer data exactly.
    pub fn check_i64(&mut self, quantity: &str, actual: &[i64]) -> bool {
        self.attempt(quantity);
        let shape = self
            .fixture
            .quantity(quantity)
            .map(|q| q.shape.clone())
            .unwrap_or_default();
        let shape = if shape.iter().product::<usize>() == actual.len() {
            shape
        } else {
            vec![actual.len()]
        };
        self.keep(
            quantity,
            NpyArray {
                shape: shape.clone(),
                data: NpyData::I64(actual.to_vec()),
            },
        );
        match self.fixture.load_i64(quantity) {
            Ok(expected) => {
                let r = compare_i64(self.fixture.case_id(), quantity, &expected, &shape, actual);
                let ok = r.passed();
                self.reports.push(r);
                ok
            }
            Err(e) => {
                self.errors.push(e.to_string());
                false
            }
        }
    }

    /// Report the error the Rust code produced and check it against the
    /// fixture's `expect` (`error_kind` compared without namespaces,
    /// `message_contains` as a substring). Returns `true` when it matches.
    ///
    /// For an ordinary fixture (`expect.status = ok`) any observed error is
    /// a failure. Calling this twice keeps the first observation and records
    /// a second one as an error.
    pub fn observe_error(&mut self, kind: &str, message: &str) -> bool {
        let expect = &self.fixture.manifest().expect;
        let mut problems = Vec::new();
        if expect.status != ExpectedStatus::Error {
            problems.push("the fixture expects a successful run (expect.status = ok)".into());
        }
        if let Some(k) = &expect.error_kind
            && strip_namespace(k) != strip_namespace(kind)
        {
            problems.push(format!(
                "error kind `{}` does not match expected `{}`",
                strip_namespace(kind),
                strip_namespace(k)
            ));
        }
        if let Some(m) = &expect.message_contains
            && !message.contains(m.as_str())
        {
            problems.push(format!("message does not contain {m:?}"));
        }
        let obs = ErrorObservation {
            expected_status: match expect.status {
                ExpectedStatus::Ok => "ok".into(),
                ExpectedStatus::Error => "error".into(),
            },
            expected_kind: expect.error_kind.clone(),
            expected_message: expect.message_contains.clone(),
            observed_kind: kind.to_string(),
            observed_message: message.to_string(),
            passed: problems.is_empty(),
            problems,
        };
        let ok = obs.passed;
        if self.error_check.is_some() {
            self.errors
                .push(format!("a second error was observed: {obs}"));
        } else {
            self.error_check = Some(obs);
        }
        ok
    }

    /// Check the port's own solve against the stopping rule recorded in the
    /// fixture (`parameters.solver`): one entry per step along the fixture's
    /// `step` axis. Use it for solver results that are not checked as
    /// [`NEWTON_QUANTITIES`] (those are accepted automatically). Returns
    /// `true` when every step satisfies the rule and is consistent.
    pub fn check_solver_acceptance(&mut self, steps: &[SolverStep<'_>]) -> bool {
        let a = SolverAcceptance::evaluate(&self.fixture, "explicit check by the test", steps);
        let ok = a.passed();
        self.acceptance.push(a);
        ok
    }

    /// Solver acceptance of the port's checked convergence quantities
    /// (`None` when the test checked none of [`NEWTON_QUANTITIES`]).
    fn newton_acceptance(&self) -> Option<SolverAcceptance> {
        const SOURCE: &str = "the port's checked newton/nb_iterations, newton/error and \
                              newton/residual_norm";
        if !NEWTON_QUANTITIES
            .iter()
            .any(|q| self.attempted.iter().any(|a| a == q))
        {
            return None;
        }
        let get = |name: &str| self.checked.iter().find(|c| c.name == name);
        let criterion = StoppingCriterion::from_fixture(&self.fixture);
        let (Ok(criterion), Some(iters), Some(error), Some(history)) = (
            &criterion,
            get(NEWTON_QUANTITIES[0]),
            get(NEWTON_QUANTITIES[1]),
            get(NEWTON_QUANTITIES[2]),
        ) else {
            let mut errors = Vec::new();
            if let Err(e) = &criterion {
                errors.push(e.to_string());
            }
            let missing: Vec<&str> = NEWTON_QUANTITIES
                .iter()
                .copied()
                .filter(|q| get(q).is_none())
                .collect();
            if !missing.is_empty() {
                errors.push(format!(
                    "convergence quantities were checked without {missing:?}: solver acceptance \
                     needs the port's iteration counts, errors and residual histories"
                ));
            }
            return Some(SolverAcceptance::unavailable(SOURCE, errors));
        };
        let rows = history.rows.as_ref();
        let (NpyData::I64(iters), NpyData::F64(error), NpyData::F64(history)) =
            (&iters.array.data, &error.array.data, &history.array.data)
        else {
            return Some(SolverAcceptance::unavailable(
                SOURCE,
                vec!["unexpected dtypes of the checked convergence quantities".into()],
            ));
        };
        if iters.len() != error.len() {
            return Some(SolverAcceptance::unavailable(
                SOURCE,
                vec![format!(
                    "{} iteration counts but {} errors checked",
                    iters.len(),
                    error.len()
                )],
            ));
        }
        // Rows: as given by the test (ragged check) or, for a flat check,
        // as implied by the port's own iteration counts.
        let lengths: Vec<usize> = match rows {
            Some(r) if r.len() == iters.len() => r.clone(),
            Some(r) => {
                return Some(SolverAcceptance::unavailable(
                    SOURCE,
                    vec![format!(
                        "{} residual-history rows for {} steps",
                        r.len(),
                        iters.len()
                    )],
                ));
            }
            None => iters
                .iter()
                .map(|&n| usize::try_from(n).map_or(0, |n| criterion.history_len(n)))
                .collect(),
        };
        if lengths.iter().sum::<usize>() != history.len() {
            return Some(SolverAcceptance::unavailable(
                SOURCE,
                vec![format!(
                    "{} residual norms checked, but the iteration counts imply {} (rows {lengths:?})",
                    history.len(),
                    lengths.iter().sum::<usize>()
                )],
            ));
        }
        let mut steps = Vec::with_capacity(iters.len());
        let mut start = 0;
        for ((&n, &e), &len) in iters.iter().zip(error).zip(&lengths) {
            steps.push(SolverStep {
                nb_iterations: n,
                error: e,
                history: &history[start..start + len],
            });
            start += len;
        }
        Some(SolverAcceptance::evaluate(&self.fixture, SOURCE, &steps))
    }

    /// Record an input file to persist as a regression case on failure.
    pub fn record_input_file(&mut self, role: &str, path: &Path) {
        self.inputs
            .input_files
            .push((role.to_string(), path.to_path_buf()));
    }

    /// Record an input array to persist as a regression case on failure.
    pub fn record_input_array(&mut self, name: &str, shape: &[usize], data: &[f64], axes: &[&str]) {
        self.inputs.input_arrays.push(InputArray {
            name: name.to_string(),
            array: NpyArray {
                shape: shape.to_vec(),
                data: NpyData::F64(data.to_vec()),
            },
            axes: axes.iter().map(|a| (*a).to_string()).collect(),
            layout: Layout::Table,
        });
    }

    /// Record free-form parameters for the regression case.
    pub fn record_parameters(&mut self, probe: &str, parameters: serde_json::Value) {
        self.inputs.probe = probe.to_string();
        self.inputs.parameters = parameters;
    }

    /// Current report.
    #[must_use]
    pub fn report(&self) -> CaseReport {
        CaseReport {
            test: self.test.clone(),
            case_id: self.fixture.case_id().to_string(),
            fixture_dir: self.fixture.dir().display().to_string(),
            build_config: self.fixture.manifest().build.as_ref().map_or_else(
                || "?".into(),
                |b| format!("{} ({}, {})", b.config, b.compiler, b.arch),
            ),
            expects_error: self.expects_error(),
            error_check: self.error_check.clone(),
            quantities: self.reports.clone(),
            solver_acceptance: self
                .acceptance
                .iter()
                .cloned()
                .chain(self.newton_acceptance())
                .collect(),
            errors: self.errors.clone(),
        }
    }

    /// Finish: `Ok(report)` if everything passed, otherwise write the
    /// artefacts and return a [`Failure`]. A test that checked nothing (and
    /// observed no expected error) fails: there is no evidence to pass on.
    pub fn finish(self) -> std::result::Result<CaseReport, Box<Failure>> {
        let report = self.report();
        let write_all = std::env::var(WRITE_ALL_ENV).is_ok_and(|v| v == "1");
        if report.passed() && !write_all {
            return Ok(report);
        }
        let dir = self.out_root.join(sanitize(&self.test));
        let written = self.write_artifacts(&dir, &report);
        if report.passed() {
            return Ok(report);
        }
        let (artifacts, write_error) = match written {
            Ok(()) => (Some(dir), None),
            Err(e) => (None, Some(e.to_string())),
        };
        Err(Box::new(Failure {
            report,
            artifacts,
            write_error,
        }))
    }

    /// Finish and panic with the full report on failure.
    ///
    /// # Panics
    /// When any quantity failed or could not be compared.
    pub fn assert_pass(self) {
        if let Err(f) = self.finish() {
            panic!("{f}");
        }
    }

    fn run_f64(
        &mut self,
        quantity: &str,
        shape: &[usize],
        actual: &[f64],
        tol: &ResolvedTolerance,
    ) -> bool {
        self.keep_f64(quantity, shape, actual);
        let q = match self.fixture.quantity(quantity) {
            Ok(q) => q.clone(),
            Err(e) => {
                self.errors.push(e.to_string());
                return false;
            }
        };
        // Ragged quantities expose their rows as a grouping axis.
        let ragged = match (&q.ragged, q.layout) {
            (Some(r), Layout::RaggedValues) => match self.fixture.load_i64(&r.offsets) {
                Ok(o) => Some((o.data, r.row_axis.clone())),
                Err(e) => {
                    self.errors.push(e.to_string());
                    return false;
                }
            },
            _ => None,
        };
        match self.fixture.load_f64(quantity) {
            Ok(expected) => {
                let r = compare_f64(
                    self.fixture.case_id(),
                    quantity,
                    &expected,
                    shape,
                    actual,
                    tol,
                    &CompareOptions {
                        component_labels: q.components.as_deref(),
                        ragged: ragged
                            .as_ref()
                            .map(|(offsets, row_axis)| RaggedRows { offsets, row_axis }),
                        ..CompareOptions::default()
                    },
                );
                let ok = r.passed();
                self.reports.push(r);
                ok
            }
            Err(e) => {
                self.errors.push(e.to_string());
                false
            }
        }
    }

    fn attempt(&mut self, quantity: &str) {
        if !self.attempted.iter().any(|a| a == quantity) {
            self.attempted.push(quantity.to_string());
        }
    }

    fn keep_f64(&mut self, quantity: &str, shape: &[usize], actual: &[f64]) {
        let shape = if shape.iter().product::<usize>() == actual.len() {
            shape.to_vec()
        } else {
            vec![actual.len()]
        };
        self.keep(
            quantity,
            NpyArray {
                shape,
                data: NpyData::F64(actual.to_vec()),
            },
        );
    }

    fn keep(&mut self, quantity: &str, array: NpyArray) {
        let (axes, layout) = self.fixture.quantity(quantity).map_or_else(
            |_| (vec!["entry".to_string()], Layout::Table),
            |q| (q.axes.clone(), q.layout),
        );
        // A result whose rank disagrees with the fixture is kept flattened.
        let (array, axes) = if axes.len() == array.shape.len() {
            (array, axes)
        } else {
            let n = array.data.len();
            (
                NpyArray {
                    shape: vec![n],
                    data: array.data,
                },
                vec!["entry".to_string()],
            )
        };
        self.checked.retain(|c| c.name != quantity);
        self.checked.push(Checked {
            name: quantity.to_string(),
            array,
            axes,
            layout,
            rows: None,
        });
    }

    fn write_artifacts(&self, dir: &Path, report: &CaseReport) -> Result<()> {
        if dir.exists() {
            // Only our own previous output for this test lives here.
            std::fs::remove_dir_all(dir).map_err(|e| Error::io(dir, e))?;
        }
        report.write(dir)?;
        let mut w = FixtureWriter::create(
            dir,
            &format!("actual/{}", sanitize(&self.test)),
            &format!(
                "akantu-rs values of test `{}` (fixture {})",
                self.test,
                self.fixture.case_id()
            ),
            FixtureStatus::RustSnapshot,
            Generator {
                probe: "akantu-validation::DiffTest".into(),
                probe_version: env!("CARGO_PKG_VERSION").into(),
                command: format!("cargo test (test `{}`)", self.test),
            },
            ReferenceInfo {
                sha: self.fixture.manifest().reference.sha.clone(),
                describe: None,
                patches: Vec::new(),
            },
            None,
        )?;
        for c in &self.checked {
            let axes: Vec<&str> = c.axes.iter().map(String::as_str).collect();
            let q = w.add_array(&c.name, &c.array, &axes, c.layout)?;
            if c.layout == Layout::RaggedValues {
                // A snapshot of ragged values is kept flat; without its
                // offsets the manifest cannot describe it as ragged.
                q.layout = Layout::Table;
            }
        }
        w.finish()?;
        if !self.inputs.input_files.is_empty() || !self.inputs.input_arrays.is_empty() {
            persist_regression_case(dir, &self.inputs)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::{BuildInfo, Expectation};
    use crate::tolerance::TOLERANCE_FILE;

    fn golden_fixture(root: &Path) -> Fixture {
        golden_fixture_with(root, "test/harness/case", None)
    }

    fn golden_fixture_with(root: &Path, case_id: &str, expect: Option<Expectation>) -> Fixture {
        let mut w = FixtureWriter::create(
            root,
            case_id,
            "synthetic golden case",
            FixtureStatus::Golden,
            Generator {
                probe: "unit-test".into(),
                probe_version: "1".into(),
                command: "none".into(),
            },
            ReferenceInfo {
                sha: crate::regression::REFERENCE_SHA.into(),
                describe: None,
                patches: Vec::new(),
            },
            Some(BuildInfo {
                config: "strict".into(),
                compiler: "synthetic".into(),
                arch: "none".into(),
                ..BuildInfo::default()
            }),
        )
        .unwrap();
        w.add_f64(
            "u",
            &[2, 3, 2],
            &[0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.1, 0.0, 0.0, 1.1],
            &["step", "node", "component"],
            Layout::Nodal,
        )
        .unwrap()
        .components = Some(vec!["x".into(), "y".into()]);
        w.add_i64(
            "conn",
            &[1, 3],
            &[0, 1, 2],
            &["element", "node_local"],
            Layout::Elemental,
        )
        .unwrap();
        w.add_f64(
            "empty",
            &[0, 3],
            &[],
            &["entry", "component"],
            Layout::Table,
        )
        .unwrap();
        w.add_i64(
            "residual/offsets",
            &[3],
            &[0, 2, 3],
            &["entry"],
            Layout::RaggedOffsets,
        )
        .unwrap();
        w.add_f64(
            "residual",
            &[3],
            &[1.0, 1e-3, 5.0],
            &["iteration"],
            Layout::RaggedValues,
        )
        .unwrap()
        .ragged = Some(crate::manifest::Ragged {
            offsets: "residual/offsets".into(),
            row_axis: "step".into(),
        });
        w.manifest_mut().steps = Some(crate::manifest::Steps {
            recorded: vec![0, 10],
            dt: Some(0.1),
            time_quantity: None,
        });
        if let Some(e) = expect {
            w.manifest_mut().expect = e;
        }
        let dir = w.finish().unwrap();
        std::fs::write(
            root.join("test").join(TOLERANCE_FILE),
            r#"{"format_version":1,"rules":[
                {"quantity":"u","abs":1e-14,"rel":1e-12,"scale":"max_abs","justification":"synthetic"},
                {"quantity":"empty","justification":"synthetic"},
                {"quantity":"residual","rel":1e-6,"scale":"max_abs_per_step","justification":"synthetic"}
            ]}"#,
        )
        .unwrap();
        Fixture::open(&dir).unwrap()
    }

    #[test]
    fn passing_test() {
        let dir = tempfile::tempdir().unwrap();
        let fx = golden_fixture(dir.path());
        let out = dir.path().join("out");
        let mut t = DiffTest::with_fixture("harness::pass", fx)
            .unwrap()
            .with_output_root(&out);
        let mut u = t.reference_f64("u").unwrap().data;
        u[2] += 1e-15;
        assert!(t.check_f64("u", &u));
        assert!(t.check_i64("conn", &[0, 1, 2]));
        let r = t.finish().unwrap();
        assert!(r.passed());
        assert_eq!(r.quantities.len(), 2);
        assert!(!out.exists(), "no artefacts for passing tests");
        assert_eq!(r.n_proposed_tolerances(), 1);
        assert!(r.to_string().contains("PROPOSED"), "{r}");
    }

    #[test]
    fn failing_test_writes_artifacts() {
        let dir = tempfile::tempdir().unwrap();
        let fx = golden_fixture(dir.path());
        let out = dir.path().join("out");
        let mut t = DiffTest::with_fixture("harness::fail", fx)
            .unwrap()
            .with_output_root(&out);
        let mut u = t.reference_f64("u").unwrap().data;
        u[9] = 0.5; // step 1, node 1, component y
        assert!(!t.check_f64("u", &u));
        assert!(!t.check_i64("conn", &[0, 2, 1]));
        assert!(!t.check_f64("not_there", &[1.0]));
        assert!(!t.check_f64("u", &[1.0, 2.0])); // shape mismatch
        t.record_input_array("strain", &[1, 2], &[0.1, 0.2], &["step", "component"]);
        t.record_parameters("material_probe", serde_json::json!({"E": 1.0}));
        let f = t.finish().unwrap_err();
        let text = f.to_string();
        assert!(text.contains("FAILED"), "{text}");
        assert!(text.contains("has no quantity named `not_there`"), "{text}");
        assert!(text.contains("step=1, node=1, component=1"), "{text}");
        assert!(text.contains("first failing step index: 1"), "{text}");
        let art = f.artifacts.clone().unwrap();
        assert!(art.join("report.json").is_file());
        assert!(art.join("report.txt").is_file());
        let snap = Fixture::open(&art.join("actual/harness__fail")).unwrap();
        assert_eq!(snap.manifest().status, FixtureStatus::RustSnapshot);
        // The last check of `u` (wrong shape) is what was kept.
        assert_eq!(snap.load_f64("u").unwrap().data, vec![1.0, 2.0]);
        let reg = Fixture::open(&art.join("regression/test/harness__fail")).unwrap();
        assert_eq!(reg.manifest().status, FixtureStatus::PendingOracle);
        assert_eq!(reg.manifest().parameters["E"], 1.0);
        let json: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(art.join("report.json")).unwrap())
                .unwrap();
        assert_eq!(json["case_id"], "test/harness/case");
        assert_eq!(json["quantities"].as_array().unwrap().len(), 3);
    }

    #[test]
    fn missing_tolerance_is_an_error_not_a_default() {
        let dir = tempfile::tempdir().unwrap();
        let fx = golden_fixture(dir.path());
        std::fs::remove_file(dir.path().join("test").join(TOLERANCE_FILE)).unwrap();
        let mut t = DiffTest::with_fixture("harness::notol", fx)
            .unwrap()
            .with_output_root(&dir.path().join("out"));
        let u = t.reference_f64("u").unwrap().data;
        assert!(!t.check_f64("u", &u));
        let f = t.finish().unwrap_err();
        assert!(f.to_string().contains("no tolerance defined"), "{f}");
        // In-code spec works without a file.
        let fx = Fixture::open(&dir.path().join("test/harness/case")).unwrap();
        let mut t = DiffTest::with_fixture("harness::code", fx).unwrap();
        assert!(t.check_f64_with("u", &[2, 3, 2], &u, ToleranceSpec::exact("same data")));
        assert!(t.finish().is_ok());
    }

    #[test]
    fn non_golden_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let mut w = FixtureWriter::create(
            dir.path(),
            "test/x/y",
            "snapshot",
            FixtureStatus::RustSnapshot,
            Generator {
                probe: "t".into(),
                probe_version: "1".into(),
                command: String::new(),
            },
            ReferenceInfo::default(),
            None,
        )
        .unwrap();
        w.add_f64("a", &[1], &[1.0], &["entry"], Layout::Table)
            .unwrap();
        let d = w.finish().unwrap();
        assert!(DiffTest::with_fixture("t", Fixture::open(&d).unwrap()).is_err());
    }

    /// A `DiffTest` that checks nothing has no evidence and must not
    /// pass; a legitimate empty-array shape comparison is evidence.
    #[test]
    fn empty_completion_is_refused_but_empty_arrays_are_fine() {
        let dir = tempfile::tempdir().unwrap();
        let fx = golden_fixture(dir.path());
        let out = dir.path().join("out");
        let t = DiffTest::with_fixture("harness::nothing", fx.clone())
            .unwrap()
            .with_output_root(&out);
        let f = t.finish().unwrap_err();
        assert!(!f.report.passed());
        assert!(f.to_string().contains("no quantity was checked"), "{f}");
        let mut t = DiffTest::with_fixture("harness::empty", fx)
            .unwrap()
            .with_output_root(&out);
        assert!(t.check_f64("empty", &[]));
        let r = t.finish().unwrap();
        assert!(r.passed());
        assert_eq!(r.quantities[0].n_compared, 0);
        assert_eq!(r.quantities[0].expected_shape, vec![0, 3]);
    }

    /// Error-path fixtures need the error to be observed and matched.
    #[test]
    fn expected_errors_must_be_observed() {
        let dir = tempfile::tempdir().unwrap();
        let expect = Expectation {
            status: ExpectedStatus::Error,
            exit_code: 0,
            error_kind: Some("akantu::debug::NotImplementedException".into()),
            message_contains: Some("not implemented".into()),
        };
        let fx = golden_fixture_with(dir.path(), "test/harness/err", Some(expect));
        let out = dir.path().join("out");
        // Checking an (empty) quantity without observing the error: fail.
        let mut t = DiffTest::with_fixture("harness::err_unobserved", fx.clone())
            .unwrap()
            .with_output_root(&out);
        assert!(t.expects_error());
        assert!(t.check_f64("empty", &[]));
        let f = t.finish().unwrap_err();
        assert!(f.to_string().contains("did not observe one"), "{f}");
        // Wrong exception class.
        let mut t = DiffTest::with_fixture("harness::err_kind", fx.clone())
            .unwrap()
            .with_output_root(&out);
        assert!(!t.observe_error("Unsupported", "computeStress: not implemented yet"));
        let f = t.finish().unwrap_err();
        assert!(f.to_string().contains("does not match expected"), "{f}");
        // Wrong message.
        let mut t = DiffTest::with_fixture("harness::err_msg", fx.clone())
            .unwrap()
            .with_output_root(&out);
        assert!(!t.observe_error("NotImplementedException", "something else"));
        assert!(t.finish().is_err());
        // Matching observation: passes without any quantity.
        let mut t = DiffTest::with_fixture("harness::err_ok", fx)
            .unwrap()
            .with_output_root(&out);
        assert!(t.observe_error(
            "akantu_core::Error::NotImplementedException",
            "computeStress : not implemented yet !"
        ));
        let r = t.finish().unwrap();
        assert!(r.passed());
        assert!(r.error_check.as_ref().unwrap().passed);
        let text = r.to_string();
        assert!(text.contains("observed error"), "{text}");
        let json = r.to_json().unwrap();
        assert!(json.contains("\"error_check\""), "{json}");
        // An ordinary fixture that errors unexpectedly fails even when a
        // quantity passed.
        let ok_fx = golden_fixture(dir.path());
        let mut t = DiffTest::with_fixture("harness::unexpected_err", ok_fx)
            .unwrap()
            .with_output_root(&out);
        let u = t.reference_f64("u").unwrap().data;
        assert!(t.check_f64("u", &u));
        assert!(!t.observe_error("Io", "disk on fire"));
        let f = t.finish().unwrap_err();
        assert!(f.to_string().contains("expects a successful run"), "{f}");
    }

    /// Synthetic Newton history: step 0 elastic (1 iteration), step 1
    /// contracting (3 iterations), Akantu residual criterion 1e-10.
    fn newton_fixture(root: &Path, case_id: &str, solver: serde_json::Value) -> Fixture {
        let mut w = FixtureWriter::create(
            root,
            case_id,
            "synthetic Newton history",
            FixtureStatus::Golden,
            Generator {
                probe: "unit-test".into(),
                probe_version: "1".into(),
                command: "none".into(),
            },
            ReferenceInfo {
                sha: crate::regression::REFERENCE_SHA.into(),
                describe: None,
                patches: Vec::new(),
            },
            Some(BuildInfo {
                config: "strict".into(),
                compiler: "synthetic".into(),
                arch: "none".into(),
                ..BuildInfo::default()
            }),
        )
        .unwrap();
        w.add_i64(
            "newton/nb_iterations",
            &[2],
            &[1, 3],
            &["step"],
            Layout::Scalar,
        )
        .unwrap();
        w.add_f64(
            "newton/error",
            &[2],
            &[1e-9, 5e-5],
            &["step"],
            Layout::Scalar,
        )
        .unwrap();
        w.add_i64(
            "newton/residual_norm/offsets",
            &[3],
            &[0, 2, 6],
            &["entry"],
            Layout::RaggedOffsets,
        )
        .unwrap();
        w.add_f64(
            "newton/residual_norm",
            &[6],
            &[4.8e5, 1e-9, 1e6, 1e2, 1e-2, 5e-5],
            &["iteration"],
            Layout::RaggedValues,
        )
        .unwrap()
        .ragged = Some(crate::manifest::Ragged {
            offsets: "newton/residual_norm/offsets".into(),
            row_axis: "step".into(),
        });
        w.add_f64("u", &[2], &[1.0, 2.0], &["step"], Layout::Scalar)
            .unwrap();
        w.manifest_mut().steps = Some(crate::manifest::Steps {
            recorded: vec![1, 2],
            dt: Some(0.1),
            time_quantity: None,
        });
        w.manifest_mut().parameters = serde_json::json!({ "solver": solver });
        let dir = w.finish().unwrap();
        std::fs::write(
            root.join(case_id.split('/').next().unwrap()).join(TOLERANCE_FILE),
            r#"{"format_version":1,"rules":[
                {"quantity":"newton/error","abs":1e-7,"justification":"synthetic diagnostic"},
                {"quantity":"newton/residual_norm","rel":1e-8,"scale":"max_abs_per_step","justification":"synthetic"},
                {"quantity":"u","justification":"synthetic"}
            ]}"#,
        )
        .unwrap();
        Fixture::open(&dir).unwrap()
    }

    fn newton_rule() -> serde_json::Value {
        serde_json::json!({"type": "newton_raphson", "convergence_type": "residual",
                           "threshold": 1e-10, "max_iterations": 50})
    }

    /// Convergence quantities trigger solver acceptance on the
    /// port's own values; differential agreement cannot replace it.
    #[test]
    fn solver_acceptance_checks_the_ports_own_stopping_decision() {
        let dir = tempfile::tempdir().unwrap();
        let fx = newton_fixture(dir.path(), "newton/case", newton_rule());
        let out = dir.path().join("out");
        // The oracle's own values are accepted.
        let mut t = DiffTest::with_fixture("harness::newton_ok", fx.clone())
            .unwrap()
            .with_output_root(&out);
        let rows = t.reference_ragged_f64("newton/residual_norm").unwrap();
        assert!(t.check_i64("newton/nb_iterations", &[1, 3]));
        assert!(t.check_f64("newton/error", &[1e-9, 5e-5]));
        assert!(t.check_ragged_f64("newton/residual_norm", &rows));
        let r = t.finish().unwrap();
        assert_eq!(r.solver_acceptance.len(), 1);
        assert!(r.solver_acceptance[0].passed(), "{r}");
        assert_eq!(r.solver_acceptance[0].steps[1].limit, 1e6 * 1e-10);

        // Final residuals raised above |r_0| 1e-10 with the iteration
        // counts unchanged, checked with a wide in-code difference bound
        // (a constructed counterexample): every differential check passes,
        // acceptance rejects both steps.
        let mut t = DiffTest::with_fixture("harness::newton_bad", fx.clone())
            .unwrap()
            .with_output_root(&out);
        let errs = [1e-9 + 9e-5, 5e-5 + 9e-5];
        let mut bad = rows.clone();
        bad[0][1] = errs[0];
        bad[1][3] = errs[1];
        let wide = ToleranceSpec {
            abs: 1e-4,
            ..ToleranceSpec::exact("review counterexample: the former 1e-4 N rule")
        };
        assert!(t.check_i64("newton/nb_iterations", &[1, 3]));
        assert!(t.check_f64_with("newton/error", &[2], &errs, wide.clone()));
        assert!(t.check_f64_with("newton/residual_norm", &[6], &bad.concat(), wide));
        let f = t.finish().unwrap_err();
        assert!(f.report.quantities.iter().all(QuantityReport::passed));
        let a = &f.report.solver_acceptance[0];
        assert!(!a.passed());
        assert!(a.steps.iter().all(|s| !s.passed), "{a}");
        let text = f.to_string();
        assert!(text.contains("[FAIL] solver acceptance"), "{text}");
        assert!(text.contains("did not converge"), "{text}");

        // A test that checks only the error cannot skip acceptance.
        let mut t = DiffTest::with_fixture("harness::newton_partial", fx.clone())
            .unwrap()
            .with_output_root(&out);
        assert!(t.check_f64("newton/error", &[1e-9, 5e-5]));
        let f = t.finish().unwrap_err();
        assert!(
            f.to_string().contains("needs the port's iteration counts"),
            "{f}"
        );

        // Inconsistent port values: the error is not the last residual.
        let mut t = DiffTest::with_fixture("harness::newton_inconsistent", fx.clone())
            .unwrap()
            .with_output_root(&out);
        assert!(t.check_i64("newton/nb_iterations", &[1, 3]));
        assert!(t.check_f64("newton/error", &[1e-9, 5e-5 + 1e-12]));
        assert!(t.check_f64("newton/residual_norm", &rows.concat()));
        let f = t.finish().unwrap_err();
        assert!(
            f.to_string().contains("differs from the last tested norm"),
            "{f}"
        );

        // A flat history check is split by the port's iteration counts: an
        // extra iteration that the reference does not have is caught by
        // the exact count *and* by the history length.
        let mut t = DiffTest::with_fixture("harness::newton_counts", fx.clone())
            .unwrap()
            .with_output_root(&out);
        assert!(!t.check_i64("newton/nb_iterations", &[1, 4]));
        assert!(t.check_f64("newton/error", &[1e-9, 5e-5]));
        assert!(t.check_f64("newton/residual_norm", &rows.concat()));
        let f = t.finish().unwrap_err();
        assert!(
            f.to_string().contains("the iteration counts imply 7"),
            "{f}"
        );
    }

    #[test]
    fn solver_acceptance_needs_a_recorded_rule() {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("out");
        let fx = newton_fixture(dir.path(), "norule/case", serde_json::Value::Null);
        let mut t = DiffTest::with_fixture("harness::norule", fx)
            .unwrap()
            .with_output_root(&out);
        assert!(t.check_i64("newton/nb_iterations", &[1, 3]));
        let f = t.finish().unwrap_err();
        assert!(
            f.to_string().contains("parameters.solver is missing"),
            "{f}"
        );
        let lumped = serde_json::json!({"type": "lumped", "convergence_type": "residual",
                                         "threshold": 1e-10, "max_iterations": 1});
        let fx = newton_fixture(dir.path(), "lumped/case", lumped);
        let mut t = DiffTest::with_fixture("harness::lumped", fx)
            .unwrap()
            .with_output_root(&out);
        assert!(t.check_i64("newton/nb_iterations", &[1, 3]));
        let f = t.finish().unwrap_err();
        assert!(f.to_string().contains("no stopping rule"), "{f}");
    }

    /// Solver results that are not fixture quantities are accepted through
    /// the explicit call; quantities without solver meaning do not trigger
    /// acceptance.
    #[test]
    fn explicit_solver_acceptance() {
        let dir = tempfile::tempdir().unwrap();
        let fx = newton_fixture(dir.path(), "explicit/case", newton_rule());
        let out = dir.path().join("out");
        let mut t = DiffTest::with_fixture("harness::plain", fx.clone())
            .unwrap()
            .with_output_root(&out);
        assert!(t.check_f64("u", &[1.0, 2.0]));
        assert!(t.finish().unwrap().solver_acceptance.is_empty());
        let mut t = DiffTest::with_fixture("harness::explicit", fx.clone())
            .unwrap()
            .with_output_root(&out);
        let h0 = [4.8e5, 1e-9];
        let h1 = [1e6, 1e2, 1e-2, 5e-5];
        let steps = [
            SolverStep {
                nb_iterations: 1,
                error: 1e-9,
                history: &h0,
            },
            SolverStep {
                nb_iterations: 3,
                error: 5e-5,
                history: &h1,
            },
        ];
        assert!(t.check_solver_acceptance(&steps));
        let r = t.finish().unwrap();
        assert!(r.passed(), "acceptance alone is evidence: {r}");
        // Wrong number of steps for the fixture.
        let mut t = DiffTest::with_fixture("harness::explicit_steps", fx)
            .unwrap()
            .with_output_root(&out);
        assert!(!t.check_solver_acceptance(&steps[..1]));
        let f = t.finish().unwrap_err();
        assert!(
            f.to_string()
                .contains("1 steps given, the fixture records 2"),
            "{f}"
        );
    }

    /// Ragged quantities are compared per step through the harness.
    #[test]
    fn ragged_history_through_the_harness() {
        let dir = tempfile::tempdir().unwrap();
        let fx = golden_fixture(dir.path());
        let out = dir.path().join("out");
        let mut t = DiffTest::with_fixture("harness::ragged", fx.clone())
            .unwrap()
            .with_output_root(&out);
        let rows = t.reference_ragged_f64("residual").unwrap();
        assert_eq!(rows, vec![vec![1.0, 1e-3], vec![5.0]]);
        // Step 0 scale is 1.0: a 1e-7 error on its small entry is fine; the
        // same error on a whole-array basis would also pass, so use 1e-5
        // (0.2 x tol of step 1's scale 5, but 10 x tol of step 0).
        let ok = vec![vec![1.0, 1e-3 + 1e-7], vec![5.0 + 1e-6]];
        assert!(t.check_ragged_f64("residual", &ok));
        let bad = vec![vec![1.0, 1e-3 + 1e-5], vec![5.0]];
        assert!(!t.check_ragged_f64("residual", &bad));
        assert!(!t.check_ragged_f64("residual", &[vec![1.0], vec![1e-3, 5.0]]));
        let f = t.finish().unwrap_err();
        let text = f.to_string();
        assert!(text.contains("row lengths"), "{text}");
        assert!(text.contains("step=0, iteration=1"), "{text}");
        // Flat check of the same quantity also uses the reference rows.
        let mut t = DiffTest::with_fixture("harness::ragged_flat", fx)
            .unwrap()
            .with_output_root(&out);
        assert!(t.check_f64("residual", &[1.0, 1e-3, 5.0]));
        let r = t.finish().unwrap();
        assert_eq!(r.quantities[0].per_step.len(), 2);
    }
}
