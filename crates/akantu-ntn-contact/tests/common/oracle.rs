// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). No Akantu
// counterpart: helpers of the differential replays against
// fixtures/model/ntn_contact/* (oracle/probes/ntn_contact,
// oracle/probes/ntn_contact_adv).

//! Differential replays through the validation harness.
//!
//! Every recorded oracle value is compared by `akantu_validation`'s
//! comparator under the rule `tolerances.json` assigns to the case (the
//! exact rule of `fixtures/model/ntn_contact`). There is no NaN or zero
//! handling here: D-046 / D-051 put the exact-rule semantics in the
//! comparator only.

use std::collections::BTreeMap;
use std::path::PathBuf;

use akantu_ntn_contact::{NtnBaseContact, NtnFriction};
use akantu_validation::{CompareOptions, DiffTest, Field, Fixture, compare_f64, fixtures_root};

/// The group of the TASN oracle cases.
pub const GROUP: &str = "model/ntn_contact";

/// Open a golden case of the group and verify its checksums.
pub fn case(name: &str) -> Fixture {
    let dir = fixtures_root().join(GROUP).join(name);
    let f = Fixture::open(&dir).unwrap_or_else(|e| panic!("{}: {e}", dir.display()));
    f.require_golden().unwrap();
    f.verify_checksums().unwrap();
    f
}

/// Every case of the group whose name starts with `prefix` (sorted).
pub fn cases_with_prefix(prefix: &str) -> Vec<String> {
    let dir = fixtures_root().join(GROUP);
    let mut out: Vec<String> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
        .filter_map(|e| e.ok())
        .filter(|e| e.path().join("manifest.json").exists())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.starts_with(prefix))
        .collect();
    out.sort();
    assert!(!out.is_empty(), "no fixture case {prefix}*");
    out
}

/// Recorded values of a quantity, for driving a replay (comparisons go
/// through [`Replay::check`]).
pub fn f64s(f: &Fixture, name: &str) -> Vec<f64> {
    f.load_f64(name)
        .unwrap_or_else(|e| panic!("{}: {name}: {e}", f.case_id()))
        .data
}

/// A numeric manifest parameter.
pub fn param_f64(f: &Fixture, key: &str) -> f64 {
    f.manifest().parameters[key]
        .as_f64()
        .unwrap_or_else(|| panic!("{}: parameter {key}", f.case_id()))
}

/// A string manifest parameter.
pub fn param_str<'a>(f: &'a Fixture, key: &str) -> &'a str {
    f.manifest().parameters[key]
        .as_str()
        .unwrap_or_else(|| panic!("{}: parameter {key}", f.case_id()))
}

/// Path of a recorded input file.
pub fn input(f: &Fixture, role: &str) -> PathBuf {
    f.input_path(role)
        .unwrap_or_else(|e| panic!("{}: input {role}: {e}", f.case_id()))
}

/// Exact equality of two finite scalars outside the fixture quantities
/// (manifest parameters): bit identity, no special values involved.
pub fn assert_same_finite(what: &str, rust: f64, oracle: f64) {
    assert!(
        rust.is_finite() && oracle.is_finite() && rust.to_bits() == oracle.to_bits(),
        "{what}: rust {rust:e} oracle {oracle:e}"
    );
}

/// One golden case under comparison: the fixture (inputs, parameters) and
/// the harness collecting the comparisons.
pub struct Replay {
    t: DiffTest,
}

impl Replay {
    /// Replay of case `name` by the Rust test `test`; returns the fixture
    /// for the inputs and parameters as well.
    pub fn new(test: &str, name: &str) -> (Fixture, Self) {
        let f = case(name);
        let t = DiffTest::with_fixture(test, f.clone())
            .unwrap_or_else(|e| panic!("{}: {e}", f.case_id()));
        (f, Self { t })
    }

    /// Compare a whole quantity (harness rule of the case).
    pub fn check(&mut self, quantity: &str, rust: &[f64]) {
        self.t.check_f64(quantity, rust);
    }

    /// Compare the first `rows` rows (first axis) of a quantity with the
    /// harness comparator and the case's rule (a run that stops early by
    /// design, e.g. the corrected mode at a UD-039 domain error).
    ///
    /// # Panics
    /// The comparison fails or the shapes do not fit.
    pub fn check_rows(&mut self, quantity: &str, rows: usize, rust: &[f64]) {
        let f = self.t.fixture();
        let full = f
            .load_f64(quantity)
            .unwrap_or_else(|e| panic!("{}: {quantity}: {e}", f.case_id()));
        assert!(
            !full.shape.is_empty() && rows <= full.shape[0],
            "{}: {quantity}: {rows} rows of {:?}",
            f.case_id(),
            full.shape
        );
        let per_row: usize = full.shape[1..].iter().product();
        let mut shape = full.shape.clone();
        shape[0] = rows;
        let expected = Field {
            shape: shape.clone(),
            data: full.data[..rows * per_row].to_vec(),
            axes: full.axes.clone(),
        };
        let tol = f
            .tolerance(quantity)
            .unwrap_or_else(|e| panic!("{}: {quantity}: {e}", f.case_id()));
        let report = compare_f64(
            f.case_id(),
            quantity,
            &expected,
            &shape,
            rust,
            &tol,
            &CompareOptions::default(),
        );
        assert!(report.passed(), "{report}");
    }

    /// Finish the case: panics with the harness report if any comparison
    /// failed (or nothing was compared).
    pub fn finish(self) {
        self.t.assert_pass();
    }
}

/// Booleans as recorded by the probes (0/1).
pub fn bools(v: &[bool]) -> Vec<f64> {
    v.iter().map(|&b| if b { 1.0 } else { 0.0 }).collect()
}

/// Per-step series recorded like the probes' `Recorder`.
#[derive(Default)]
pub struct Series(pub BTreeMap<String, Vec<f64>>);

impl Series {
    /// Append one row.
    pub fn push(&mut self, name: &str, v: &[f64]) {
        self.0
            .entry(name.to_owned())
            .or_default()
            .extend_from_slice(v);
    }

    /// Append the contact and friction state (probe `record_contact_state`).
    pub fn push_state(&mut self, prefix: &str, contact: &NtnBaseContact, friction: &NtnFriction) {
        let p = |n: &str| format!("{prefix}{n}");
        self.push(
            &p("contact_pressure"),
            contact.contact_pressure().as_slice(),
        );
        self.push(
            &p("is_in_contact"),
            &bools(contact.is_in_contact().as_slice()),
        );
        self.push(
            &p("friction_traction"),
            friction.friction_traction().as_slice(),
        );
        self.push(
            &p("frictional_strength"),
            friction.frictional_strength().as_slice(),
        );
        self.push(&p("is_sticking"), &bools(friction.is_sticking().as_slice()));
        self.push(&p("slip"), friction.slip().as_slice());
        self.push(&p("cumulative_slip"), friction.cumulative_slip().as_slice());
        self.push(
            &p("slip_velocity_norm"),
            friction.slip_velocity_norm().as_slice(),
        );
        self.push(&p("friction_state"), friction.friction_state().as_slice());
    }

    /// Compare every series whose name starts with `filter`.
    pub fn compare(&self, r: &mut Replay, filter: &str) {
        for (name, v) in &self.0 {
            if name.starts_with(filter) {
                r.check(name, v);
            }
        }
    }

    /// Compare the first `rows` rows of every series.
    pub fn compare_rows(&self, r: &mut Replay, rows: usize) {
        for (name, v) in &self.0 {
            r.check_rows(name, rows, v);
        }
    }
}
