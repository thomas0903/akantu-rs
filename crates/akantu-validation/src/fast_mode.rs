// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). No upstream
// counterpart: validation of the fast mode against the bitwise default
// (D-122, D-123).

//! Fast mode against the bitwise default.
//!
//! A kernel of the fast mode (`akantu_core::exec::Precision::Fast`: FMA,
//! another summation order, …) may round differently from the default path.
//! Its test runs a case twice, once per mode, and hands both results to
//! [`check_fast_mode`]: every quantity of the fast run is compared with the
//! bitwise run under the tolerance rule that the case's `tolerances.json`
//! gives (the same files, resolution and criteria as the oracle comparisons,
//! [`crate::tolerance`]; rules listed in `docs/TOLERANCE_LEDGER.md`). The
//! cases live under `fixtures/fast_mode/<area>/`, so a later fast kernel
//! adds one call in its test and one justified rule there.
//!
//! ```no_run
//! use akantu_validation::fast_mode::{ModeQuantity, check_fast_mode};
//! # let (u_bitwise, u_fast) = (vec![0.0; 6], vec![0.0; 6]);
//! let bitwise = [ModeQuantity::nodal("u", 3, u_bitwise)];
//! let fast = [ModeQuantity::nodal("u", 3, u_fast)];
//! check_fast_mode("fast_mode/solid/explicit_fma_kernels", &bitwise, &fast, true);
//! ```

use crate::array::Field;
use crate::compare::{CompareOptions, compare_f64};
use crate::fixture::fixtures_root;
use crate::report::QuantityReport;
use crate::tolerance::resolve;

/// One result quantity of a run.
#[derive(Debug, Clone, PartialEq)]
pub struct ModeQuantity {
    /// Quantity name (matched by the rules' `quantity` globs).
    pub name: String,
    /// Shape (row-major).
    pub shape: Vec<usize>,
    /// Axis names (`docs/NUMERICAL_VALIDATION.md` vocabulary).
    pub axes: Vec<String>,
    /// Values.
    pub values: Vec<f64>,
}

impl ModeQuantity {
    /// A nodal field of `nb_component` values per node (axes `node`,
    /// `component`).
    ///
    /// # Panics
    /// `values.len()` not a multiple of `nb_component`.
    #[must_use]
    pub fn nodal(name: &str, nb_component: usize, values: Vec<f64>) -> Self {
        assert!(
            nb_component > 0 && values.len().is_multiple_of(nb_component),
            "{name}: {} values are not nodes of {nb_component}",
            values.len()
        );
        Self {
            name: name.into(),
            shape: vec![values.len() / nb_component, nb_component],
            axes: vec!["node".into(), "component".into()],
            values,
        }
    }
}

/// Compare every quantity of `fast` with the same quantity of `bitwise`
/// under its tolerance rule (case `case_id`, `fixtures/<case_id>/…`); one
/// report per quantity, in order.
///
/// # Errors
/// A quantity without a rule, a missing or extra quantity, or two
/// quantities of different shapes.
pub fn compare_fast_mode(
    case_id: &str,
    bitwise: &[ModeQuantity],
    fast: &[ModeQuantity],
) -> crate::Result<Vec<QuantityReport>> {
    if bitwise.len() != fast.len() || bitwise.iter().zip(fast).any(|(b, f)| b.name != f.name) {
        return Err(crate::Error::shape(format!(
            "{case_id}: the two runs report different quantities"
        )));
    }
    let root = fixtures_root();
    let mut reports = Vec::with_capacity(bitwise.len());
    for (b, f) in bitwise.iter().zip(fast) {
        let tol = resolve(&root, case_id, &b.name)?;
        let expected = Field::new(b.shape.clone(), b.values.clone(), b.axes.clone())?;
        reports.push(compare_f64(
            case_id,
            &b.name,
            &expected,
            &f.shape,
            &f.values,
            &tol,
            &CompareOptions::default(),
        ));
    }
    Ok(reports)
}

/// [`compare_fast_mode`] for a test: panics with the failing quantities'
/// reports, or when `expect_difference` and the fast run equals the bitwise
/// run bit for bit (the fast kernel did not run).
///
/// # Panics
/// As described.
pub fn check_fast_mode(
    case_id: &str,
    bitwise: &[ModeQuantity],
    fast: &[ModeQuantity],
    expect_difference: bool,
) -> Vec<QuantityReport> {
    let reports = compare_fast_mode(case_id, bitwise, fast)
        .unwrap_or_else(|e| panic!("fast mode {case_id}: {e}"));
    let failed: Vec<String> = reports
        .iter()
        .filter(|r| !r.passed())
        .map(|r| r.to_json().unwrap_or_else(|_| format!("{r:?}")))
        .collect();
    assert!(
        failed.is_empty(),
        "fast mode {case_id}: outside tolerance:\n{}",
        failed.join("\n")
    );
    if expect_difference {
        let identical = bitwise.iter().zip(fast).all(|(b, f)| {
            b.values
                .iter()
                .zip(&f.values)
                .all(|(x, y)| x.to_bits() == y.to_bits())
        });
        assert!(
            !identical,
            "fast mode {case_id}: the fast run equals the bitwise run bit for bit \
             (the fast kernel did not run)"
        );
    }
    reports
}
