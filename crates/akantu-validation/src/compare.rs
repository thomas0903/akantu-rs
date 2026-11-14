// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). This file has no
// direct C++ counterpart: it implements field comparison for the
// differential-testing harness (docs/NUMERICAL_VALIDATION.md).

//! Entry-wise comparison of reference and Rust arrays.
//!
//! The comparison never panics on data: shape mismatches, NaNs and
//! infinities are reported as outcomes/violations so that a failing test
//! always yields a complete, actionable [`QuantityReport`].
//!
//! Grouped scales (`max_abs_per_component`, `max_abs_per_step`,
//! `max_abs_per_group`) are evaluated on the *same* groups that the
//! statistics use; a grouping axis the quantity does not have is an error
//! ([`Outcome::InvalidTolerance`]), never a silent fallback to a
//! whole-array scale. Ragged quantities (CSR offsets, e.g. Newton residual
//! histories) expose their rows as a grouping axis through
//! [`RaggedRows`].
//!
//! An exact rule ([`ToleranceSpec::is_exact`], no per-entry allowance)
//! compares every value with [`crate::exact::exact_eq`]: bit identity —
//! `+0` and `−0` do **not** match (decision D-051) — with the single
//! exception of default quiet NaNs differing only in sign (decision D-046);
//! payload and signalling NaNs and NaN against a number are violations.
//! [`CompareOptions::value_equality`] (measurement mode of `fixture-diff`)
//! compares values instead: `±0` equal, any NaN matches any NaN.

use crate::array::{Field, strides, unravel};
use crate::exact::exact_eq;
use crate::report::{
    Bucket, EntryDetail, GroupStat, Location, MAX_LISTED_VIOLATIONS, Outcome, QuantityReport,
};
use crate::tolerance::{NanPolicy, ResolvedTolerance, Scale, ToleranceSpec};

/// Labels of the error/tolerance histogram (upper bounds, `<=`).
const RATIO_EDGES: [(f64, &str); 8] = [
    (1e-3, "(0,1e-3]"),
    (1e-2, "(1e-3,1e-2]"),
    (1e-1, "(1e-2,1e-1]"),
    (1.0, "(1e-1,1]"),
    (10.0, "(1,10]"),
    (1e2, "(10,1e2]"),
    (1e4, "(1e2,1e4]"),
    (f64::INFINITY, ">1e4"),
];

/// Labels of the ULP histogram (inclusive upper bounds).
const ULP_EDGES: [(u64, &str); 7] = [
    (0, "0"),
    (1, "1"),
    (4, "2-4"),
    (16, "5-16"),
    (256, "17-256"),
    (65_536, "257-65536"),
    (u64::MAX, ">65536"),
];

/// Distance in units in the last place between two finite doubles
/// (`+0` and `-0` are 0 ulp apart). Saturates at `u64::MAX`.
#[must_use]
pub fn ulp_distance(a: f64, b: f64) -> u64 {
    fn ordered(x: f64) -> i128 {
        // Bit-cast reinterpretation, exactly what the ordering needs.
        #[allow(clippy::cast_possible_wrap)] // intended bit reinterpretation
        let i = x.to_bits() as i64;
        let o = if i < 0 { i64::MIN - i } else { i };
        i128::from(o)
    }
    let d = (ordered(a) - ordered(b)).unsigned_abs();
    u64::try_from(d).unwrap_or(u64::MAX)
}

/// Row structure of a ragged quantity: CSR offsets delimiting the rows of
/// the flat value array, and the axis name the rows stand for.
#[derive(Debug, Clone, Copy)]
pub struct RaggedRows<'a> {
    /// `offsets[0] == 0`, non-decreasing, last entry == number of values.
    pub offsets: &'a [i64],
    /// Axis the rows stand for (e.g. `step`).
    pub row_axis: &'a str,
}

/// Optional extras of a comparison.
#[derive(Debug, Clone, Default)]
pub struct CompareOptions<'a> {
    /// Additional per-entry absolute allowance, e.g. the rounding interval
    /// of a value parsed from an upstream `.verified` text file. Applies to
    /// `actual >= expected` (and to both sides unless `extra_abs_below` is
    /// given).
    pub extra_abs: Option<&'a [f64]>,
    /// Additional per-entry absolute allowance for `actual < expected`, when
    /// the allowance is asymmetric (a printed `1e+06` stands for values in
    /// `[999999.5, 1000005]`). Defaults to `extra_abs`.
    pub extra_abs_below: Option<&'a [f64]>,
    /// Labels of the `component` axis, for reports.
    pub component_labels: Option<&'a [String]>,
    /// Row structure of a ragged quantity (both arrays share it).
    pub ragged: Option<RaggedRows<'a>>,
    /// Measurement mode for tools that report bit identity separately
    /// (`fixture-diff`): an exact rule then compares *values* (`+0` equals
    /// `−0`, any NaN matches any NaN) instead of bits (D-046, D-051). Never
    /// set by a differential test: tolerance rules are always evaluated as
    /// written (guard: `tests/value_equality_guard.rs`).
    pub value_equality: bool,
}

/// Compare floating-point arrays.
///
/// `expected` carries the axis names; `actual` must have the same shape.
#[must_use]
pub fn compare_f64(
    case_id: &str,
    quantity: &str,
    expected: &Field<f64>,
    actual_shape: &[usize],
    actual: &[f64],
    tol: &ResolvedTolerance,
    opts: &CompareOptions<'_>,
) -> QuantityReport {
    let mut report = empty_report(case_id, quantity, expected, actual_shape, tol);
    if let Some(msg) = shape_problem(expected, actual_shape, actual.len(), opts) {
        report.outcome = Outcome::ShapeMismatch;
        report.message = Some(msg);
        return report;
    }
    if let Err(e) = tol.spec.check_axes(
        quantity,
        &expected.axes,
        opts.ragged.as_ref().map(|r| r.row_axis),
    ) {
        report.outcome = Outcome::InvalidTolerance;
        report.message = Some(format!("tolerance rejected: {e}"));
        return report;
    }
    let spec = &tol.spec;
    let scales = compute_scales(expected, spec, opts.ragged.as_ref());
    let mut acc = Accumulator::new(expected, spec, opts);
    let mut ulp_hist = vec![0usize; ULP_EDGES.len() + 1];

    for (i, (&e, &a)) in expected.data.iter().zip(actual).enumerate() {
        let extra = match (opts.extra_abs_below, a < e) {
            (Some(below), true) => below[i],
            _ => opts.extra_abs.map_or(0.0, |x| x[i]),
        };
        let allowed = spec.abs + spec.rel * scales[i] + extra;
        // Exact (bitwise) rule for this entry (no per-entry allowance
        // either): bit identity (D-051: signed zeros differ), NaNs under
        // decision D-046 (`crate::exact`).
        let exact_rule = spec.is_exact() && extra == 0.0 && !opts.value_equality;
        let mut detail = EntryDetail {
            location: Location {
                flat: i,
                index: Vec::new(),
            },
            expected: e,
            actual: a,
            expected_bits: Some(format!("{:#018x}", e.to_bits())),
            actual_bits: Some(format!("{:#018x}", a.to_bits())),
            abs_err: 0.0,
            rel_err: 0.0,
            allowed,
            ulps: None,
            ratio: 0.0,
        };
        let mut kind = EntryKind::Finite;
        if e.is_nan() || a.is_nan() {
            let matched = e.is_nan()
                && a.is_nan()
                && spec.nan == NanPolicy::MustMatch
                && (!exact_rule || exact_eq(e, a));
            detail.abs_err = if matched { 0.0 } else { f64::NAN };
            detail.rel_err = detail.abs_err;
            detail.ratio = if matched { 0.0 } else { f64::INFINITY };
            kind = if matched {
                EntryKind::Exact
            } else {
                EntryKind::NanMismatch
            };
            ulp_hist[ULP_EDGES.len()] += 1;
        } else if e.is_infinite() || a.is_infinite() {
            let matched = e == a;
            detail.abs_err = if matched { 0.0 } else { f64::INFINITY };
            detail.rel_err = detail.abs_err;
            detail.ratio = if matched { 0.0 } else { f64::INFINITY };
            kind = if matched {
                EntryKind::Exact
            } else {
                EntryKind::InfMismatch
            };
            ulp_hist[ULP_EDGES.len()] += 1;
        } else {
            let err = (a - e).abs();
            let ulps = ulp_distance(a, e);
            detail.abs_err = err;
            detail.rel_err = relative(err, e);
            detail.ulps = Some(ulps);
            detail.ratio = ratio(err, allowed, ulps, spec.ulps);
            // finite values: an exact rule requires bit identity, so `+0`
            // against `−0` (err 0, distinct bits) is a violation (D-051);
            // other rules, and the value-equality mode, compare values
            if exact_rule && err == 0.0 && e.to_bits() != a.to_bits() {
                detail.ratio = f64::INFINITY;
            } else if err == 0.0 {
                kind = EntryKind::Exact;
            }
            let b = ULP_EDGES
                .iter()
                .position(|(edge, _)| ulps <= *edge)
                .unwrap_or(ULP_EDGES.len() - 1);
            ulp_hist[b] += 1;
        }
        acc.add(i, &detail, kind, e == 0.0);
    }
    acc.finish(&mut report);
    report.ulp_histogram = ULP_EDGES
        .iter()
        .map(|(_, l)| (*l).to_string())
        .chain(std::iter::once("non-finite".to_string()))
        .zip(ulp_hist)
        .map(|(label, count)| Bucket { label, count })
        .collect();
    report
}

/// Compare integer arrays exactly (connectivities, ids, iteration counts).
#[must_use]
pub fn compare_i64(
    case_id: &str,
    quantity: &str,
    expected: &Field<i64>,
    actual_shape: &[usize],
    actual: &[i64],
) -> QuantityReport {
    let tol = ResolvedTolerance {
        spec: ToleranceSpec {
            reviewed_by: Some("integer data is compared exactly (no rule needed)".into()),
            ..ToleranceSpec::exact("integer data is compared exactly")
        },
        origin: "integer quantity".into(),
    };
    let as_f = Field {
        shape: expected.shape.clone(),
        data: Vec::new(),
        axes: expected.axes.clone(),
    };
    let mut report = empty_report(case_id, quantity, &as_f, actual_shape, &tol);
    if expected.shape != actual_shape || actual.len() != expected.data.len() {
        report.outcome = Outcome::ShapeMismatch;
        report.message = Some(format!(
            "expected shape {:?} ({} entries), got shape {:?} ({} entries)",
            expected.shape,
            expected.data.len(),
            actual_shape,
            actual.len()
        ));
        return report;
    }
    let mut acc = Accumulator::new(&as_f, &tol.spec, &CompareOptions::default());
    for (i, (&e, &a)) in expected.data.iter().zip(actual).enumerate() {
        let diff = (i128::from(a) - i128::from(e)).unsigned_abs();
        #[allow(clippy::cast_precision_loss)] // only used for display/statistics
        let err = diff as f64;
        #[allow(clippy::cast_precision_loss)] // ids/counts are far below 2^53
        let (ef, af) = (e as f64, a as f64);
        let detail = EntryDetail {
            location: Location {
                flat: i,
                index: Vec::new(),
            },
            expected: ef,
            actual: af,
            expected_bits: None,
            actual_bits: None,
            abs_err: err,
            rel_err: relative(err, ef),
            allowed: 0.0,
            ulps: None,
            ratio: if diff == 0 { 0.0 } else { f64::INFINITY },
        };
        let kind = if diff == 0 {
            EntryKind::Exact
        } else {
            EntryKind::Finite
        };
        acc.add(i, &detail, kind, e == 0);
    }
    acc.finish(&mut report);
    report
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EntryKind {
    Exact,
    Finite,
    NanMismatch,
    InfMismatch,
}

fn relative(err: f64, e: f64) -> f64 {
    if err == 0.0 {
        0.0
    } else if e == 0.0 {
        f64::INFINITY
    } else {
        err / e.abs()
    }
}

/// Fraction of the tolerance used; `<= 1` means the entry passes.
fn ratio(err: f64, allowed: f64, ulps: u64, max_ulps: Option<u64>) -> f64 {
    let r_abs = if err == 0.0 {
        0.0
    } else if allowed > 0.0 {
        err / allowed
    } else {
        f64::INFINITY
    };
    match max_ulps {
        None => r_abs,
        Some(m) => {
            #[allow(clippy::cast_precision_loss)] // ratio is a diagnostic
            let r_ulp = if ulps == 0 {
                0.0
            } else if m == 0 {
                f64::INFINITY
            } else {
                ulps as f64 / m as f64
            };
            r_abs.min(r_ulp)
        }
    }
}

/// Validate CSR offsets for `n` entries; returns the per-entry row index.
fn ragged_rows(offsets: &[i64], n: usize) -> std::result::Result<Vec<usize>, String> {
    if offsets.first() != Some(&0) {
        return Err("ragged offsets must start at 0".into());
    }
    let mut rows = Vec::with_capacity(n);
    for (r, w) in offsets.windows(2).enumerate() {
        let (a, b) = (
            usize::try_from(w[0]).map_err(|_| "negative ragged offset".to_string())?,
            usize::try_from(w[1]).map_err(|_| "negative ragged offset".to_string())?,
        );
        if a > b || b > n {
            return Err(format!("invalid ragged offsets {a}..{b} for {n} values"));
        }
        rows.extend(std::iter::repeat_n(r, b - a));
    }
    if rows.len() != n {
        return Err(format!(
            "ragged offsets cover {} values, the array has {n}",
            rows.len()
        ));
    }
    Ok(rows)
}

fn shape_problem(
    expected: &Field<f64>,
    actual_shape: &[usize],
    actual_len: usize,
    opts: &CompareOptions<'_>,
) -> Option<String> {
    if expected.shape != actual_shape || expected.data.len() != actual_len {
        return Some(format!(
            "expected shape {:?} ({} entries), got shape {:?} ({} entries)",
            expected.shape,
            expected.data.len(),
            actual_shape,
            actual_len
        ));
    }
    for (name, extra) in [
        ("extra_abs", opts.extra_abs),
        ("extra_abs_below", opts.extra_abs_below),
    ] {
        if let Some(x) = extra
            && x.len() != actual_len
        {
            return Some(format!(
                "{name} has {} entries, expected {actual_len}",
                x.len()
            ));
        }
        if let Some(x) = extra
            && x.iter().any(|v| !(v.is_finite() && *v >= 0.0))
        {
            return Some(format!("{name} must be finite and >= 0"));
        }
    }
    if let Some(r) = &opts.ragged {
        if let Err(e) = ragged_rows(r.offsets, actual_len) {
            return Some(e);
        }
        if expected.axes.iter().any(|a| a == r.row_axis) {
            return Some(format!(
                "ragged row axis `{}` is also a dense axis of the quantity",
                r.row_axis
            ));
        }
    }
    None
}

fn empty_report(
    case_id: &str,
    quantity: &str,
    expected: &Field<f64>,
    actual_shape: &[usize],
    tol: &ResolvedTolerance,
) -> QuantityReport {
    QuantityReport {
        case_id: case_id.to_string(),
        quantity: quantity.to_string(),
        outcome: Outcome::Pass,
        expected_shape: expected.shape.clone(),
        actual_shape: actual_shape.to_vec(),
        axes: expected.axes.clone(),
        tolerance: tol.spec.summary(),
        tolerance_origin: tol.origin.clone(),
        justification: tol.spec.justification.clone(),
        tolerance_reviewed: tol.spec.is_reviewed(),
        n_compared: 0,
        n_violations: 0,
        n_exact: 0,
        n_nan_mismatch: 0,
        n_inf_mismatch: 0,
        n_zero_reference: 0,
        max_abs_err: None,
        max_rel_err: None,
        worst: None,
        max_ulp: None,
        violations: Vec::new(),
        ratio_histogram: Vec::new(),
        ulp_histogram: Vec::new(),
        per_component: Vec::new(),
        per_step: Vec::new(),
        per_group: Vec::new(),
        first_failing_step: None,
        message: None,
    }
}

/// Per-entry index along one grouping axis: a dense axis of the field or
/// the rows of a ragged quantity.
#[derive(Debug, Clone)]
enum AxisMap {
    Dense { stride: usize, len: usize },
    Ragged { rows: Vec<usize>, len: usize },
}

impl AxisMap {
    fn at(&self, i: usize) -> usize {
        match self {
            AxisMap::Dense { stride, len } => (i / stride) % len,
            AxisMap::Ragged { rows, .. } => rows[i],
        }
    }

    fn len(&self) -> usize {
        match self {
            AxisMap::Dense { len, .. } | AxisMap::Ragged { len, .. } => *len,
        }
    }
}

/// Joint grouping of entries by their index along several axes.
#[derive(Debug, Clone)]
struct Grouping {
    axes: Vec<String>,
    maps: Vec<AxisMap>,
    n_groups: usize,
}

impl Grouping {
    /// Build the grouping for `axes`; `None` when an axis is unavailable
    /// (callers reject that earlier through [`ToleranceSpec::check_axes`]).
    fn new(expected: &Field<f64>, axes: &[&str], ragged: Option<&RaggedRows<'_>>) -> Option<Self> {
        let st = strides(&expected.shape);
        let mut maps = Vec::with_capacity(axes.len());
        for &name in axes {
            if let Some(ax) = expected.axis(name) {
                maps.push(AxisMap::Dense {
                    stride: st[ax],
                    len: expected.shape[ax],
                });
            } else {
                let r = ragged.filter(|r| r.row_axis == name)?;
                let rows = ragged_rows(r.offsets, expected.data.len()).ok()?;
                maps.push(AxisMap::Ragged {
                    rows,
                    len: r.offsets.len().saturating_sub(1),
                });
            }
        }
        let n_groups = maps.iter().map(AxisMap::len).product();
        Some(Self {
            axes: axes.iter().map(|a| (*a).to_string()).collect(),
            maps,
            n_groups,
        })
    }

    /// Mixed-radix group index of entry `i` (first axis outermost).
    fn group_of(&self, i: usize) -> usize {
        self.maps.iter().fold(0usize, |g, m| g * m.len() + m.at(i))
    }

    /// Multi-index of group `g`, same order as `axes`.
    fn indices(&self, mut g: usize) -> Vec<usize> {
        let mut idx = vec![0usize; self.maps.len()];
        for (k, m) in self.maps.iter().enumerate().rev() {
            let d = m.len().max(1);
            idx[k] = g % d;
            g /= d;
        }
        idx
    }

    fn stats(&self, labels: Option<&[String]>) -> Vec<GroupStat> {
        let axis = self.axes.join(",");
        (0..self.n_groups)
            .map(|g| {
                let idx = self.indices(g);
                let label = if self.axes.len() == 1 {
                    labels.and_then(|l| l.get(idx[0]).cloned())
                } else {
                    Some(
                        self.axes
                            .iter()
                            .zip(&idx)
                            .map(|(a, i)| format!("{a}={i}"))
                            .collect::<Vec<_>>()
                            .join(", "),
                    )
                };
                GroupStat {
                    axis: axis.clone(),
                    index: g,
                    label,
                    n: 0,
                    n_violations: 0,
                    max_abs_err: 0.0,
                    max_ratio: 0.0,
                }
            })
            .collect()
    }
}

/// Per-entry scale of the relative tolerance. The spec has already been
/// checked against the axes, so every grouping axis exists.
fn compute_scales(
    expected: &Field<f64>,
    spec: &ToleranceSpec,
    ragged: Option<&RaggedRows<'_>>,
) -> Vec<f64> {
    let finite_abs = |x: f64| if x.is_finite() { x.abs() } else { 0.0 };
    let n = expected.data.len();
    match &spec.scale {
        Scale::Reference => expected.data.iter().map(|&x| finite_abs(x)).collect(),
        Scale::Value(v) => vec![*v; n],
        Scale::MaxAbs => {
            let m = expected
                .data
                .iter()
                .fold(0.0f64, |m, &x| m.max(finite_abs(x)));
            vec![m; n]
        }
        grouped => {
            let axes = grouped.group_axes().unwrap_or_default();
            let Some(g) = Grouping::new(expected, &axes, ragged) else {
                // Unreachable after check_axes; a conservative zero scale
                // (pure absolute tolerance) is the safe fallback.
                return vec![0.0; n];
            };
            let mut maxes = vec![0.0f64; g.n_groups];
            for (i, &x) in expected.data.iter().enumerate() {
                let k = g.group_of(i);
                maxes[k] = maxes[k].max(finite_abs(x));
            }
            (0..n).map(|i| maxes[g.group_of(i)]).collect()
        }
    }
}

/// Running statistics shared by the f64 and i64 comparisons.
struct Accumulator {
    shape: Vec<usize>,
    axes: Vec<String>,
    ragged: Option<(String, Vec<usize>, Vec<i64>)>,
    groupings: Vec<(Grouping, Vec<GroupStat>)>,
    /// Position in `groupings` of the component / step / acceptance groups.
    comp_slot: Option<usize>,
    step_slot: Option<usize>,
    group_slot: Option<usize>,
    ratio_hist: Vec<usize>,
    n: usize,
    n_viol: usize,
    n_exact: usize,
    n_nan: usize,
    n_inf: usize,
    n_zero: usize,
    max_abs: Option<EntryDetail>,
    max_rel: Option<EntryDetail>,
    worst: Option<EntryDetail>,
    max_ulp: Option<EntryDetail>,
    listed: Vec<EntryDetail>,
}

impl Accumulator {
    fn new(expected: &Field<f64>, spec: &ToleranceSpec, opts: &CompareOptions<'_>) -> Self {
        let ragged = opts.ragged.as_ref();
        let mut groupings = Vec::new();
        let mut push = |axes: &[&str], labels: Option<&[String]>| -> Option<usize> {
            let g = Grouping::new(expected, axes, ragged)?;
            let stats = g.stats(labels);
            groupings.push((g, stats));
            Some(groupings.len() - 1)
        };
        let comp_slot = push(&["component"], opts.component_labels);
        let step_slot = push(&["step"], None);
        let group_slot = match spec.scale.group_axes() {
            Some(axes) if axes != ["component"] && axes != ["step"] => push(&axes, None),
            _ => None,
        };
        let ragged_info = ragged.and_then(|r| {
            ragged_rows(r.offsets, expected.data.len())
                .ok()
                .map(|rows| (r.row_axis.to_string(), rows, r.offsets.to_vec()))
        });
        Self {
            shape: expected.shape.clone(),
            axes: expected.axes.clone(),
            ragged: ragged_info,
            groupings,
            comp_slot,
            step_slot,
            group_slot,
            ratio_hist: vec![0; RATIO_EDGES.len() + 2],
            n: 0,
            n_viol: 0,
            n_exact: 0,
            n_nan: 0,
            n_inf: 0,
            n_zero: 0,
            max_abs: None,
            max_rel: None,
            worst: None,
            max_ulp: None,
            listed: Vec::new(),
        }
    }

    fn locate(&self, flat: usize) -> Location {
        let mut index: Vec<(String, usize)> = Vec::new();
        if let Some((row_axis, rows, offsets)) = &self.ragged {
            let row = rows[flat];
            index.push((row_axis.clone(), row));
            if self.shape.len() == 1 {
                let start = usize::try_from(offsets[row]).unwrap_or(0);
                index.push((self.axes[0].clone(), flat - start));
                return Location { flat, index };
            }
        }
        index.extend(self.axes.iter().cloned().zip(unravel(&self.shape, flat)));
        Location { flat, index }
    }

    fn add(&mut self, i: usize, d: &EntryDetail, kind: EntryKind, zero_ref: bool) {
        self.n += 1;
        if zero_ref {
            self.n_zero += 1;
        }
        // NaN ratios count as violations.
        let violated = !matches!(
            d.ratio.partial_cmp(&1.0),
            Some(std::cmp::Ordering::Less | std::cmp::Ordering::Equal)
        );
        match kind {
            EntryKind::Exact => self.n_exact += 1,
            EntryKind::NanMismatch => self.n_nan += 1,
            EntryKind::InfMismatch => self.n_inf += 1,
            EntryKind::Finite => {}
        }
        // Histogram of error/tolerance.
        let bucket = match kind {
            EntryKind::NanMismatch | EntryKind::InfMismatch => RATIO_EDGES.len() + 1,
            // a signed-zero mismatch under an exact rule (D-051) has
            // abs_err 0 but is a violation (ratio ∞): not the exact bucket
            _ if d.abs_err == 0.0 && !violated => 0,
            _ => {
                1 + RATIO_EDGES
                    .iter()
                    .position(|(edge, _)| d.ratio <= *edge)
                    .unwrap_or(RATIO_EDGES.len() - 1)
            }
        };
        self.ratio_hist[bucket] += 1;

        let need_detail = violated
            || better(&self.max_abs, d.abs_err, |x| x.abs_err)
            || (d.expected != 0.0 && better(&self.max_rel, d.rel_err, |x| x.rel_err))
            || better(&self.worst, d.ratio, |x| x.ratio)
            || d.ulps.is_some_and(|u| {
                self.max_ulp
                    .as_ref()
                    .is_none_or(|m| m.ulps.unwrap_or(0) < u)
            });
        if need_detail {
            let mut full = d.clone();
            full.location = self.locate(i);
            if better(&self.max_abs, d.abs_err, |x| x.abs_err) {
                self.max_abs = Some(full.clone());
            }
            if d.expected != 0.0 && better(&self.max_rel, d.rel_err, |x| x.rel_err) {
                self.max_rel = Some(full.clone());
            }
            if better(&self.worst, d.ratio, |x| x.ratio) {
                self.worst = Some(full.clone());
            }
            if d.ulps.is_some_and(|u| {
                self.max_ulp
                    .as_ref()
                    .is_none_or(|m| m.ulps.unwrap_or(0) < u)
            }) {
                self.max_ulp = Some(full.clone());
            }
            if violated {
                self.n_viol += 1;
                if self.listed.len() < MAX_LISTED_VIOLATIONS {
                    self.listed.push(full);
                }
            }
        }
        let abs_for_stats = if d.abs_err.is_nan() {
            f64::INFINITY
        } else {
            d.abs_err
        };
        for (grouping, stats) in &mut self.groupings {
            let g = &mut stats[grouping.group_of(i)];
            g.n += 1;
            g.n_violations += usize::from(violated);
            g.max_abs_err = g.max_abs_err.max(abs_for_stats);
            g.max_ratio = g.max_ratio.max(d.ratio);
        }
    }

    fn finish(mut self, report: &mut QuantityReport) {
        report.n_compared = self.n;
        report.n_violations = self.n_viol;
        report.n_exact = self.n_exact;
        report.n_nan_mismatch = self.n_nan;
        report.n_inf_mismatch = self.n_inf;
        report.n_zero_reference = self.n_zero;
        report.max_abs_err = self.max_abs;
        report.max_rel_err = self.max_rel;
        report.worst = self.worst;
        report.max_ulp = self.max_ulp;
        report.violations = self.listed;
        let labels = std::iter::once("exact")
            .chain(RATIO_EDGES.iter().map(|(_, l)| *l))
            .chain(std::iter::once("nan/inf mismatch"));
        report.ratio_histogram = labels
            .zip(self.ratio_hist)
            .map(|(l, count)| Bucket {
                label: l.to_string(),
                count,
            })
            .collect();
        let mut take = |slot: Option<usize>| -> Vec<GroupStat> {
            slot.map_or_else(Vec::new, |s| std::mem::take(&mut self.groupings[s].1))
        };
        report.per_component = take(self.comp_slot);
        report.per_step = take(self.step_slot);
        report.per_group = take(self.group_slot);
        report.first_failing_step = report
            .per_step
            .iter()
            .find(|g| g.n_violations > 0)
            .map(|g| g.index);
        report.outcome = if self.n_viol == 0 {
            Outcome::Pass
        } else {
            Outcome::Fail
        };
    }
}

/// `true` if `value` beats the current extreme (NaN counts as +inf).
fn better(cur: &Option<EntryDetail>, value: f64, get: impl Fn(&EntryDetail) -> f64) -> bool {
    let v = if value.is_nan() { f64::INFINITY } else { value };
    match cur {
        None => v > 0.0,
        Some(c) => {
            let cv = get(c);
            let cv = if cv.is_nan() { f64::INFINITY } else { cv };
            v > cv
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn field(shape: &[usize], data: Vec<f64>, axes: &[&str]) -> Field<f64> {
        Field::new(
            shape.to_vec(),
            data,
            axes.iter().map(|s| (*s).to_string()).collect(),
        )
        .unwrap()
    }

    fn tol(spec: ToleranceSpec) -> ResolvedTolerance {
        ResolvedTolerance {
            spec,
            origin: "test".into(),
        }
    }

    #[test]
    fn ulp_distance_basics() {
        assert_eq!(ulp_distance(1.0, 1.0), 0);
        assert_eq!(ulp_distance(0.0, -0.0), 0);
        assert_eq!(ulp_distance(1.0, f64::from_bits(1.0f64.to_bits() + 1)), 1);
        assert_eq!(ulp_distance(f64::from_bits(1), -f64::from_bits(1)), 2);
        assert_eq!(
            ulp_distance(-1.0, f64::from_bits((-1.0f64).to_bits() + 3)),
            3
        );
        assert!(ulp_distance(f64::MAX, -f64::MAX) > 1 << 62);
    }

    #[test]
    fn exact_pass_and_negative_zero() {
        let e = field(&[3], vec![1.0, 0.0, -2.5], &["entry"]);
        let r = compare_f64(
            "c",
            "q",
            &e,
            &[3],
            &[1.0, 0.0, -2.5],
            &tol(ToleranceSpec::exact("bitwise")),
            &CompareOptions::default(),
        );
        assert!(r.passed(), "{r}");
        assert_eq!(r.n_exact, 3);
        assert_eq!(r.n_zero_reference, 1);
        assert!(r.worst.is_none());
        assert!(!r.tolerance_reviewed);
        assert!(r.one_line().contains("PROPOSED"), "{}", r.one_line());
        // D-051: under an exact rule `+0` and `−0` do not match (bit
        // identity), in either direction and with `ulps: 0` too
        for (want, got) in [(0.0, -0.0), (-0.0, 0.0)] {
            for spec in [
                ToleranceSpec::exact("bitwise"),
                ToleranceSpec {
                    ulps: Some(0),
                    ..ToleranceSpec::exact("bitwise")
                },
            ] {
                let e = field(&[3], vec![1.0, want, -2.5], &["entry"]);
                let r = compare_f64(
                    "c",
                    "q",
                    &e,
                    &[3],
                    &[1.0, got, -2.5],
                    &tol(spec),
                    &CompareOptions::default(),
                );
                assert!(!r.passed(), "{want:?} vs {got:?}: {r}");
                assert_eq!(r.n_violations, 1, "{r}");
                assert_eq!(r.n_exact, 2, "{r}");
                // the error/tolerance histogram does not count it as exact
                assert_eq!(r.ratio_histogram[0].count, 2, "{r}");
            }
        }
        // value-equality mode (fixture-diff): values compared, ±0 equal
        let r = compare_f64(
            "c",
            "q",
            &e,
            &[3],
            &[1.0, -0.0, -2.5],
            &tol(ToleranceSpec::exact("bitwise")),
            &CompareOptions {
                value_equality: true,
                ..CompareOptions::default()
            },
        );
        assert!(r.passed(), "{r}");
        assert_eq!(r.n_exact, 3);
        // non-exact rules, and an exact rule with a per-entry allowance,
        // compare values: ±0 equal
        let r = compare_f64(
            "c",
            "q",
            &e,
            &[3],
            &[1.0, -0.0, -2.5],
            &tol(ToleranceSpec::abs_rel(0.0, 1e-12, "rel")),
            &CompareOptions::default(),
        );
        assert!(r.passed(), "{r}");
        let extra = [0.0, 1e-300, 0.0];
        let r = compare_f64(
            "c",
            "q",
            &e,
            &[3],
            &[1.0, -0.0, -2.5],
            &tol(ToleranceSpec::exact("bitwise")),
            &CompareOptions {
                extra_abs: Some(&extra),
                ..CompareOptions::default()
            },
        );
        assert!(r.passed(), "{r}");
    }

    /// Decision D-046 in the comparator: under an exact rule a default NaN
    /// of either sign matches (the x86_64 pattern against an aarch64
    /// golden); NaNs with a payload, signalling NaNs and NaN against a
    /// number do not.
    #[test]
    fn exact_rule_default_nan_convention_d046() {
        let pos = f64::from_bits(crate::exact::DEFAULT_NAN_BITS);
        let neg = f64::from_bits(crate::exact::NEGATIVE_DEFAULT_NAN_BITS);
        let payload = f64::from_bits(0x7ff8_0000_0000_0001);
        let signalling = f64::from_bits(0x7ff0_0000_0000_0001);
        let exact = tol(ToleranceSpec::exact("bitwise"));
        let run = |e: Vec<f64>, a: &[f64], t: &ResolvedTolerance| {
            let n = e.len();
            compare_f64(
                "c",
                "q",
                &field(&[n], e, &["entry"]),
                &[n],
                a,
                t,
                &CompareOptions::default(),
            )
        };
        // aarch64 golden, x86_64 result (the default-NaN sign pattern)
        let r = run(vec![1.0, pos, pos], &[1.0, neg, pos], &exact);
        assert!(r.passed(), "{r}");
        assert_eq!(r.n_exact, 3);
        let r = run(vec![neg], &[pos], &exact);
        assert!(r.passed(), "{r}");
        // not default NaNs: violations
        for (e, a) in [
            (pos, payload),
            (payload, neg),
            (pos, signalling),
            (signalling, pos),
            (pos, 0.0),
            (1.0, neg),
        ] {
            let r = run(vec![e], &[a], &exact);
            assert_eq!(
                r.n_violations,
                1,
                "{:#x} vs {:#x}: {r}",
                e.to_bits(),
                a.to_bits()
            );
            assert_eq!(r.n_nan_mismatch, 1);
        }
        // identical payload NaNs are bitwise identical: match
        assert!(run(vec![payload], &[payload], &exact).passed());
        // a non-exact rule keeps class matching of NaNs (docs/NUMERICAL_VALIDATION.md)
        let loose = tol(ToleranceSpec::abs_rel(0.0, 1e-12, "rel"));
        assert!(run(vec![pos], &[payload], &loose).passed());
        // `nan: forbid` rejects even matching default NaNs
        let forbid = tol(ToleranceSpec::exact("x").with_nan(NanPolicy::Forbid));
        assert_eq!(run(vec![pos], &[neg], &forbid).n_violations, 1);
        // an `ulps: 0` rule is exact too
        assert!(ToleranceSpec::ulps(0, "x").is_exact());
        assert!(!ToleranceSpec::ulps(1, "x").is_exact());
        assert!(run(vec![pos], &[neg], &tol(ToleranceSpec::ulps(0, "x"))).passed());
    }

    #[test]
    fn relative_tolerance_and_zero_reference() {
        let e = field(&[4], vec![1.0, 0.0, 1e-300, -4.0], &["entry"]);
        let a = [1.0 + 1e-11, 1e-20, 1e-300 * (1.0 + 1e-11), -4.0];
        // Pure relative: the zero reference with a tiny non-zero result fails.
        let r = compare_f64(
            "c",
            "q",
            &e,
            &[4],
            &a,
            &tol(ToleranceSpec::abs_rel(0.0, 1e-10, "rel")),
            &CompareOptions::default(),
        );
        assert_eq!(r.outcome, Outcome::Fail);
        assert_eq!(r.n_violations, 1);
        assert_eq!(r.violations[0].location.flat, 1);
        assert!(r.violations[0].rel_err.is_infinite());
        // max_rel ignores the zero reference.
        assert!(r.max_rel_err.as_ref().unwrap().rel_err < 1e-10);
        // An absolute floor fixes it.
        let r = compare_f64(
            "c",
            "q",
            &e,
            &[4],
            &a,
            &tol(ToleranceSpec::abs_rel(1e-15, 1e-10, "rel + floor")),
            &CompareOptions::default(),
        );
        assert!(r.passed(), "{r}");
    }

    #[test]
    fn max_abs_scale_handles_zero_crossings() {
        let e = field(&[3], vec![1.0, 1e-17, -1.0], &["node"]);
        let a = [1.0, 2e-17, -1.0];
        let pointwise = compare_f64(
            "c",
            "q",
            &e,
            &[3],
            &a,
            &tol(ToleranceSpec::abs_rel(0.0, 1e-12, "x")),
            &CompareOptions::default(),
        );
        assert!(!pointwise.passed());
        let normwise = compare_f64(
            "c",
            "q",
            &e,
            &[3],
            &a,
            &tol(ToleranceSpec::abs_rel(0.0, 1e-12, "x").with_scale(Scale::MaxAbs)),
            &CompareOptions::default(),
        );
        assert!(normwise.passed(), "{normwise}");
    }

    #[test]
    fn per_component_scale() {
        // component 0 is O(1e6), component 1 is O(1).
        let e = field(&[2, 2], vec![1e6, 1.0, -1e6, 2.0], &["node", "component"]);
        let a = [1e6 + 1e-4, 1.0 + 1e-4, -1e6, 2.0];
        let spec = ToleranceSpec::abs_rel(0.0, 1e-9, "x").with_scale(Scale::MaxAbsPerComponent);
        let r = compare_f64(
            "c",
            "q",
            &e,
            &[2, 2],
            &a,
            &tol(spec),
            &CompareOptions::default(),
        );
        assert_eq!(r.n_violations, 1);
        assert_eq!(r.violations[0].location.to_string(), "node=0, component=1");
        assert_eq!(r.per_component.len(), 2);
        assert_eq!(r.per_component[0].n_violations, 0);
        assert_eq!(r.per_component[1].n_violations, 1);
        assert!(
            r.per_group.is_empty(),
            "component groups are not duplicated"
        );
    }

    /// Counterexample: a per-component tolerance on a
    /// `[step, row, col]` tensor history used to fall back to a whole-array
    /// scale and accept a 5 % error on the yy component.
    #[test]
    fn absent_grouping_axis_is_rejected_not_substituted() {
        let e = field(
            &[1, 2, 2],
            vec![1e12, 0.0, 0.0, 1.0],
            &["step", "row", "col"],
        );
        let spec = ToleranceSpec::abs_rel(0.0, 1e-13, "component bound")
            .with_scale(Scale::MaxAbsPerComponent);
        let r = compare_f64(
            "review",
            "stress",
            &e,
            &e.shape,
            &[1e12, 0.0, 0.0, 1.05],
            &tol(spec),
            &CompareOptions::default(),
        );
        assert!(!r.passed());
        assert_eq!(r.outcome, Outcome::InvalidTolerance);
        assert!(
            r.message.as_deref().unwrap_or("").contains("absent axis"),
            "{r}"
        );
        // Same for a per-step scale on a quantity without a step axis.
        let e1 = field(&[2], vec![1.0, 1.0], &["iteration"]);
        let r = compare_f64(
            "review",
            "r",
            &e1,
            &[2],
            &[1.0, 1.0],
            &tol(ToleranceSpec::abs_rel(0.0, 1e-8, "x").with_scale(Scale::MaxAbsPerStep)),
            &CompareOptions::default(),
        );
        assert_eq!(r.outcome, Outcome::InvalidTolerance);
        assert!(r.one_line().starts_with("[BADTOL]"), "{}", r.one_line());
    }

    /// Explicit tensor-component groups. The yy component (O(1))
    /// gets its own scale, so a 5 % error on it fails even next to an O(1e12)
    /// xx component; highly unequal magnitudes and an exactly-zero component
    /// are covered.
    #[test]
    fn tensor_component_groups() {
        let e = field(
            &[2, 2, 2],
            vec![1e12, 0.0, 0.0, 1.0, 2e12, 0.0, 0.0, 1.0],
            &["step", "row", "col"],
        );
        let spec = ToleranceSpec::abs_rel(0.0, 1e-13, "component bound")
            .with_scale(Scale::MaxAbsPerGroup(vec!["row".into(), "col".into()]));
        let bad = [1e12, 0.0, 0.0, 1.05, 2e12, 0.0, 0.0, 1.0];
        let r = compare_f64(
            "c",
            "stress",
            &e,
            &e.shape,
            &bad,
            &tol(spec.clone()),
            &CompareOptions::default(),
        );
        assert_eq!(r.outcome, Outcome::Fail, "{r}");
        assert_eq!(r.n_violations, 1);
        assert_eq!(r.violations[0].location.to_string(), "step=0, row=1, col=1");
        // Statistics use the same groups as acceptance: 4 tensor components.
        assert_eq!(r.per_group.len(), 4);
        assert_eq!(r.per_group[3].label.as_deref(), Some("row=1, col=1"));
        assert_eq!(r.per_group[3].n_violations, 1);
        assert_eq!(r.per_group[3].n, 2);
        assert_eq!(r.per_group[0].n_violations, 0);
        assert_eq!(r.first_failing_step, Some(0));
        assert!(r.to_string().contains("per group:"), "{r}");
        // A zero component: any non-zero result there violates (scale 0).
        let zero_bad = [1e12, 1e-300, 0.0, 1.0, 2e12, 0.0, 0.0, 1.0];
        let r = compare_f64(
            "c",
            "stress",
            &e,
            &e.shape,
            &zero_bad,
            &tol(spec.clone()),
            &CompareOptions::default(),
        );
        assert_eq!(r.n_violations, 1);
        assert_eq!(r.violations[0].location.to_string(), "step=0, row=0, col=1");
        // Within the bound of each component: passes.
        let ok = [1e12 + 0.05, 0.0, 0.0, 1.0 + 1e-13, 2e12, 0.0, 0.0, 1.0];
        let r = compare_f64(
            "c",
            "stress",
            &e,
            &e.shape,
            &ok,
            &tol(spec),
            &CompareOptions::default(),
        );
        assert!(r.passed(), "{r}");
    }

    /// Ragged per-step history (CSR rows) grouped by step: scale,
    /// per-step statistics and first failing step.
    #[test]
    fn ragged_rows_group_by_step() {
        // 3 steps with 2, 0 and 3 residual assemblies.
        let e = field(&[5], vec![1e3, 1.0, 1e-2, 1e-6, 1e-12], &["iteration"]);
        let offsets = [0i64, 2, 2, 5];
        let opts = CompareOptions {
            ragged: Some(RaggedRows {
                offsets: &offsets,
                row_axis: "step",
            }),
            ..CompareOptions::default()
        };
        let spec = ToleranceSpec::abs_rel(0.0, 1e-8, "x").with_scale(Scale::MaxAbsPerStep);
        // Step 2 has max 1e-2 (allowed 1e-10); a 1e-11 error on its last
        // entry is 0.1 x tol.
        let ok = [1e3, 1.0, 1e-2, 1e-6, 1e-12 + 1e-11];
        let r = compare_f64(
            "c",
            "newton/residual_norm",
            &e,
            &[5],
            &ok,
            &tol(spec.clone()),
            &opts,
        );
        assert!(r.passed(), "{r}");
        assert_eq!(r.per_step.len(), 3);
        assert_eq!(r.per_step[1].n, 0);
        assert_eq!(r.per_step[2].n, 3);
        // Whole-array fallback would have allowed 1e3 * 1e-8 = 1e-5 here.
        let bad = [1e3, 1.0, 1e-2, 1e-6, 1e-12 + 1e-6];
        let r = compare_f64(
            "c",
            "newton/residual_norm",
            &e,
            &[5],
            &bad,
            &tol(spec.clone()),
            &opts,
        );
        assert_eq!(r.outcome, Outcome::Fail, "{r}");
        assert_eq!(r.first_failing_step, Some(2));
        assert_eq!(r.violations[0].location.to_string(), "step=2, iteration=2");
        assert_eq!(r.per_step[2].n_violations, 1);
        // Without the ragged structure the per-step scale is an error.
        let r = compare_f64(
            "c",
            "newton/residual_norm",
            &e,
            &[5],
            &ok,
            &tol(spec.clone()),
            &CompareOptions::default(),
        );
        assert_eq!(r.outcome, Outcome::InvalidTolerance);
        // Invalid offsets are a shape problem, not a panic.
        let bad_offsets = [0i64, 2, 7];
        let r = compare_f64(
            "c",
            "newton/residual_norm",
            &e,
            &[5],
            &ok,
            &tol(spec),
            &CompareOptions {
                ragged: Some(RaggedRows {
                    offsets: &bad_offsets,
                    row_axis: "step",
                }),
                ..CompareOptions::default()
            },
        );
        assert_eq!(r.outcome, Outcome::ShapeMismatch);
    }

    #[test]
    fn ulp_criterion() {
        let x = 0.1f64;
        let e = field(&[2], vec![x, x], &["entry"]);
        let a = [
            f64::from_bits(x.to_bits() + 2),
            f64::from_bits(x.to_bits() + 5),
        ];
        let r = compare_f64(
            "c",
            "q",
            &e,
            &[2],
            &a,
            &tol(ToleranceSpec::ulps(4, "x")),
            &CompareOptions::default(),
        );
        assert_eq!(r.n_violations, 1);
        assert_eq!(r.max_ulp.as_ref().unwrap().ulps, Some(5));
        assert!((r.worst.as_ref().unwrap().ratio - 1.25).abs() < 1e-12);
        let h: Vec<_> = r
            .ulp_histogram
            .iter()
            .map(|b| (b.label.as_str(), b.count))
            .collect();
        assert!(h.contains(&("2-4", 1)) && h.contains(&("5-16", 1)), "{h:?}");
        // 0 ulp allowed but abs allows it: either criterion passes.
        let spec = ToleranceSpec {
            abs: 1e-15,
            ..ToleranceSpec::ulps(0, "x")
        };
        let r = compare_f64(
            "c",
            "q",
            &e,
            &[2],
            &a,
            &tol(spec),
            &CompareOptions::default(),
        );
        assert!(r.passed(), "{r}");
    }

    #[test]
    fn nan_and_inf_handling() {
        let e = field(
            &[5],
            vec![f64::NAN, 1.0, f64::INFINITY, f64::INFINITY, f64::NAN],
            &["entry"],
        );
        let a = [f64::NAN, f64::NAN, f64::INFINITY, f64::NEG_INFINITY, 0.0];
        let r = compare_f64(
            "c",
            "q",
            &e,
            &[5],
            &a,
            &tol(ToleranceSpec::abs_rel(1.0, 1.0, "loose")),
            &CompareOptions::default(),
        );
        assert_eq!(r.n_nan_mismatch, 2);
        assert_eq!(r.n_inf_mismatch, 1);
        assert_eq!(r.n_violations, 3);
        assert_eq!(r.n_exact, 2);
        let json = r.to_json().unwrap();
        assert!(json.contains("\"NaN\""), "{json}");
        let forbid = compare_f64(
            "c",
            "q",
            &field(&[1], vec![f64::NAN], &["entry"]),
            &[1],
            &[f64::NAN],
            &tol(ToleranceSpec::exact("x").with_nan(NanPolicy::Forbid)),
            &CompareOptions::default(),
        );
        assert_eq!(forbid.n_violations, 1);
    }

    #[test]
    fn shape_mismatch_is_reported() {
        let e = field(&[2, 3], vec![0.0; 6], &["node", "component"]);
        let r = compare_f64(
            "c",
            "q",
            &e,
            &[3, 2],
            &[0.0; 6],
            &tol(ToleranceSpec::exact("x")),
            &CompareOptions::default(),
        );
        assert_eq!(r.outcome, Outcome::ShapeMismatch);
        assert!(r.message.as_ref().unwrap().contains("[3, 2]"));
        let r = compare_f64(
            "c",
            "q",
            &e,
            &[2, 3],
            &[0.0; 5],
            &tol(ToleranceSpec::exact("x")),
            &CompareOptions::default(),
        );
        assert_eq!(r.outcome, Outcome::ShapeMismatch);
        let r = compare_f64(
            "c",
            "q",
            &e,
            &[2, 3],
            &[0.0; 6],
            &tol(ToleranceSpec::exact("")),
            &CompareOptions::default(),
        );
        assert_eq!(r.outcome, Outcome::InvalidTolerance);
        assert!(r.message.unwrap().contains("justification"));
    }

    #[test]
    fn per_step_first_failure_and_listing_cap() {
        let steps = 5;
        let n = 30;
        let e = field(&[steps, n], vec![1.0; steps * n], &["step", "node"]);
        let mut a = vec![1.0; steps * n];
        for v in a.iter_mut().skip(3 * n) {
            *v = 1.1;
        }
        let r = compare_f64(
            "c",
            "q",
            &e,
            &[steps, n],
            &a,
            &tol(ToleranceSpec::abs_rel(0.0, 1e-3, "x")),
            &CompareOptions::default(),
        );
        assert_eq!(r.first_failing_step, Some(3));
        assert_eq!(r.n_violations, 2 * n);
        assert_eq!(r.violations.len(), MAX_LISTED_VIOLATIONS);
        assert_eq!(r.per_step[2].n_violations, 0);
        assert_eq!(r.per_step[4].n_violations, n);
        let text = r.to_string();
        assert!(text.contains("first failing step index: 3"), "{text}");
        assert!(text.contains("violations (first 20 of 60)"), "{text}");
    }

    #[test]
    fn extra_abs_allowance() {
        let e = field(&[2], vec![2.464762e-02, 1.0], &["entry"]);
        let a = [0.024_647_623_1, 1.000_000_4];
        let extra = [0.5e-8, 0.5e-6];
        let r = compare_f64(
            "c",
            "q",
            &e,
            &[2],
            &a,
            &tol(ToleranceSpec::exact("printed precision only")),
            &CompareOptions {
                extra_abs: Some(&extra),
                ..CompareOptions::default()
            },
        );
        assert!(r.passed(), "{r}");
    }

    /// Asymmetric allowance: below and above the reference use different
    /// extras (decade boundary of a printed value).
    #[test]
    fn asymmetric_extra_allowance() {
        let e = field(&[1], vec![1e6], &["entry"]);
        let above = [5.0];
        let below = [0.5];
        let opts = CompareOptions {
            extra_abs: Some(&above),
            extra_abs_below: Some(&below),
            ..CompareOptions::default()
        };
        let t = tol(ToleranceSpec::exact("printed precision only"));
        assert!(compare_f64("c", "q", &e, &[1], &[1e6 + 4.9], &t, &opts).passed());
        assert!(compare_f64("c", "q", &e, &[1], &[1e6 - 0.49], &t, &opts).passed());
        assert!(!compare_f64("c", "q", &e, &[1], &[1e6 - 4.9], &t, &opts).passed());
        let bad = [f64::NAN];
        let r = compare_f64(
            "c",
            "q",
            &e,
            &[1],
            &[1e6],
            &t,
            &CompareOptions {
                extra_abs: Some(&bad),
                ..CompareOptions::default()
            },
        );
        assert_eq!(r.outcome, Outcome::ShapeMismatch);
    }

    #[test]
    fn integers_exact() {
        let e = Field::new(
            vec![2, 3],
            vec![0, 1, 2, 2, 3, 0],
            vec!["element".into(), "node_local".into()],
        )
        .unwrap();
        let r = compare_i64("c", "conn", &e, &[2, 3], &[0, 1, 2, 2, 3, 0]);
        assert!(r.passed());
        assert!(
            r.tolerance_reviewed,
            "integer comparisons are not proposals"
        );
        let r = compare_i64("c", "conn", &e, &[2, 3], &[0, 1, 2, 2, 0, 3]);
        assert_eq!(r.n_violations, 2);
        assert_eq!(
            r.violations[0].location.to_string(),
            "element=1, node_local=1"
        );
        let r = compare_i64("c", "conn", &e, &[3, 2], &[0; 6]);
        assert_eq!(r.outcome, Outcome::ShapeMismatch);
    }

    #[test]
    fn ratio_histogram_counts() {
        let e = field(&[4], vec![1.0; 4], &["entry"]);
        let a = [1.0, 1.0 + 5e-13, 1.0 + 5e-11, 1.0 + 5e-9];
        let r = compare_f64(
            "c",
            "q",
            &e,
            &[4],
            &a,
            &tol(ToleranceSpec::abs_rel(1e-10, 0.0, "x")),
            &CompareOptions::default(),
        );
        let h: Vec<(String, usize)> = r
            .ratio_histogram
            .iter()
            .map(|b| (b.label.clone(), b.count))
            .collect();
        let get = |l: &str| h.iter().find(|(x, _)| x == l).map(|(_, c)| *c).unwrap();
        assert_eq!(get("exact"), 1);
        assert_eq!(get("(1e-3,1e-2]"), 1);
        assert_eq!(get("(1e-1,1]"), 1);
        assert_eq!(get("(10,1e2]"), 1);
        assert_eq!(r.n_violations, 1);
    }
}
