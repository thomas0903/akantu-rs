// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). This file has no
// direct C++ counterpart: it implements the tolerance specification of the
// validation methodology (docs/NUMERICAL_VALIDATION.md).

//! Tolerance specifications and `tolerances.json` resolution.
//!
//! Tolerances are never implicit: every compared floating-point quantity
//! must resolve to a [`ToleranceSpec`] carrying a written justification.
//! Specs live in `tolerances.json` files next to the fixture (case level)
//! or in the area directory (area level), separate from the generated data
//! so that regenerating a fixture never touches reviewed tolerances.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

/// Name of tolerance files.
pub const TOLERANCE_FILE: &str = "tolerances.json";

/// What the relative tolerance is relative to.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Scale {
    /// `|expected_i|`, entry-wise (classic relative error).
    #[default]
    Reference,
    /// `max_i |expected_i|` over the whole array (norm-relative: robust for
    /// fields with zero crossings such as displacements or stresses).
    MaxAbs,
    /// `max |expected|` over entries sharing the same `component` index.
    /// The quantity must have a `component` axis (see
    /// [`ToleranceSpec::check_axes`]).
    MaxAbsPerComponent,
    /// `max |expected|` over entries sharing the same `step` index. The
    /// quantity must have a `step` axis, or be a ragged quantity whose
    /// `ragged.row_axis` is `step`.
    MaxAbsPerStep,
    /// `max |expected|` over entries sharing the same index along *every*
    /// listed axis, e.g. `["row", "col"]` gives one scale per tensor
    /// component over a `[step, row, col]` history. Every listed axis must
    /// exist on the quantity.
    MaxAbsPerGroup(Vec<String>),
    /// A fixed, documented scale.
    Value(f64),
}

impl Scale {
    /// Axes whose joint index defines the groups of a grouped scale
    /// (`None` for the ungrouped scales).
    #[must_use]
    pub fn group_axes(&self) -> Option<Vec<&str>> {
        match self {
            Scale::Reference | Scale::MaxAbs | Scale::Value(_) => None,
            Scale::MaxAbsPerComponent => Some(vec!["component"]),
            Scale::MaxAbsPerStep => Some(vec!["step"]),
            Scale::MaxAbsPerGroup(axes) => Some(axes.iter().map(String::as_str).collect()),
        }
    }
}

/// NaN handling.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum NanPolicy {
    /// NaN in the reference must be matched by NaN in the result (and only
    /// there). This is the default: NaNs are part of the oracle behaviour.
    /// Under an exact rule ([`ToleranceSpec::is_exact`]) the two NaNs must
    /// also match under [`crate::exact::exact_eq`] (identical bits, or
    /// default quiet NaNs differing only in sign: D-046); under a
    /// non-exact rule any NaN matches any NaN.
    #[default]
    MustMatch,
    /// Any NaN, even when matched, is a violation.
    Forbid,
}

/// Acceptance criterion for one floating-point quantity.
///
/// An entry with reference `e` and result `a` (both finite) passes when
///
/// ```text
/// |a - e| <= abs + rel * scale(e)      or      ulp_distance(a, e) <= ulps
/// ```
///
/// Infinite values must match exactly (same sign); NaNs follow
/// [`NanPolicy`]. An exact rule (`abs = rel = 0`, `ulps` absent or 0) means
/// bit identity (`+0` and `−0` differ: decision D-051), NaNs compared
/// under decision D-046 ([`crate::exact::exact_eq`]: identical bits, or
/// default quiet NaNs differing only in sign).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToleranceSpec {
    /// Absolute tolerance (>= 0).
    #[serde(default)]
    pub abs: f64,
    /// Relative tolerance (>= 0).
    #[serde(default)]
    pub rel: f64,
    /// Alternative criterion in units in the last place.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ulps: Option<u64>,
    /// Scale of the relative tolerance.
    #[serde(default)]
    pub scale: Scale,
    /// NaN handling.
    #[serde(default)]
    pub nan: NanPolicy,
    /// Why this bound is right (conditioning, measured baseline, ...).
    /// Mandatory and non-empty.
    pub justification: String,
    /// Measured evidence supporting the bound (e.g. "strict vs default:
    /// max 3 ulp, 2026-11-09").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evidence: Option<String>,
    /// Independent reviewer who accepted the bound.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reviewed_by: Option<String>,
}

impl ToleranceSpec {
    /// Exact (bitwise) rule: `abs = rel = 0` and no ULP allowance: bit
    /// identity (`+0` and `−0` differ, D-051), NaNs under decision D-046
    /// ([`crate::exact::exact_eq`]).
    pub fn exact(justification: impl Into<String>) -> Self {
        Self {
            abs: 0.0,
            rel: 0.0,
            ulps: None,
            scale: Scale::Reference,
            nan: NanPolicy::MustMatch,
            justification: justification.into(),
            evidence: None,
            reviewed_by: None,
        }
    }

    /// ULP-distance criterion only.
    pub fn ulps(n: u64, justification: impl Into<String>) -> Self {
        Self {
            ulps: Some(n),
            ..Self::exact(justification)
        }
    }

    /// Absolute + relative criterion.
    pub fn abs_rel(abs: f64, rel: f64, justification: impl Into<String>) -> Self {
        Self {
            abs,
            rel,
            ..Self::exact(justification)
        }
    }

    /// `true` for an exact (bitwise) rule: `abs = rel = 0` and no ULP
    /// allowance (`ulps` absent or 0). Such a rule compares NaNs with
    /// [`crate::exact::exact_eq`] (D-046).
    #[must_use]
    pub fn is_exact(&self) -> bool {
        self.abs == 0.0 && self.rel == 0.0 && self.ulps.unwrap_or(0) == 0
    }

    /// Builder: set the relative scale.
    #[must_use]
    pub fn with_scale(mut self, scale: Scale) -> Self {
        self.scale = scale;
        self
    }

    /// Builder: set the NaN policy.
    #[must_use]
    pub fn with_nan(mut self, nan: NanPolicy) -> Self {
        self.nan = nan;
        self
    }

    /// Check the spec is well formed.
    pub fn validate(&self, context: &str) -> Result<()> {
        let bad = |reason: &str| {
            Err(Error::InvalidTolerance {
                context: context.to_string(),
                reason: reason.to_string(),
            })
        };
        if !(self.abs.is_finite() && self.abs >= 0.0) {
            return bad("abs must be finite and >= 0");
        }
        if !(self.rel.is_finite() && self.rel >= 0.0) {
            return bad("rel must be finite and >= 0");
        }
        if let Scale::Value(v) = &self.scale
            && !(v.is_finite() && *v > 0.0)
        {
            return bad("a fixed scale must be finite and > 0");
        }
        if let Scale::MaxAbsPerGroup(axes) = &self.scale {
            if axes.is_empty() {
                return bad("max_abs_per_group needs at least one axis");
            }
            let mut seen = std::collections::BTreeSet::new();
            if let Some(dup) = axes.iter().find(|a| !seen.insert(a.as_str())) {
                return bad(&format!("max_abs_per_group lists axis `{dup}` twice"));
            }
        }
        if self.justification.trim().is_empty() {
            return bad("a written justification is mandatory");
        }
        if let Some(r) = &self.reviewed_by
            && is_placeholder_review(r)
        {
            return bad(&format!(
                "reviewed_by `{r}` is a placeholder, not a review: name the review that accepted \
                 the rule, or leave the field out (the rule is then a proposal)"
            ));
        }
        Ok(())
    }

    /// Check that a grouped scale can be evaluated on a quantity with the
    /// given `axes` (plus `ragged_row_axis` for ragged quantities, whose
    /// rows are delimited by offsets rather than by a dense axis).
    ///
    /// A grouping axis the quantity does not have is an error: silently
    /// falling back to a whole-array scale would weaken the bound.
    pub fn check_axes(
        &self,
        context: &str,
        axes: &[String],
        ragged_row_axis: Option<&str>,
    ) -> Result<()> {
        self.validate(context)?;
        let Some(groups) = self.scale.group_axes() else {
            return Ok(());
        };
        for g in groups {
            let present = axes.iter().any(|a| a == g) || ragged_row_axis == Some(g);
            if !present {
                return Err(Error::InvalidTolerance {
                    context: context.to_string(),
                    reason: format!(
                        "scale groups by axis `{g}` but the quantity has axes {axes:?}{}; \
                         a grouped scale on an absent axis is rejected instead of silently \
                         becoming a whole-array scale",
                        ragged_row_axis
                            .map_or_else(String::new, |r| format!(" (ragged rows along `{r}`)"))
                    ),
                });
            }
        }
        Ok(())
    }

    /// `true` when an independent reviewer accepted the rule. Rules without
    /// `reviewed_by` are proposals: reports flag them and they must not be
    /// used as evidence for a *Verified* parity claim.
    /// A placeholder (`pending`, `todo`, `tbd`, `proposed`, empty) is not
    /// a review: [`ToleranceSpec::validate`] refuses it, and it never
    /// counts here.
    #[must_use]
    pub fn is_reviewed(&self) -> bool {
        self.reviewed_by
            .as_deref()
            .is_some_and(|r| !is_placeholder_review(r))
    }

    /// Short human-readable form, e.g. `abs=1e-12 rel=1e-10(max_abs) | 4 ulp`.
    #[must_use]
    pub fn summary(&self) -> String {
        let mut parts = Vec::new();
        if self.abs > 0.0 || self.rel > 0.0 {
            let scale = match &self.scale {
                Scale::Reference => String::new(),
                Scale::MaxAbs => "(max_abs)".into(),
                Scale::MaxAbsPerComponent => "(max_abs_per_component)".into(),
                Scale::MaxAbsPerStep => "(max_abs_per_step)".into(),
                Scale::MaxAbsPerGroup(axes) => format!("(max_abs_per_group:{})", axes.join(",")),
                Scale::Value(v) => format!("(scale={v:e})"),
            };
            parts.push(format!("abs={:e} rel={:e}{scale}", self.abs, self.rel));
        }
        if let Some(u) = self.ulps {
            parts.push(format!("{u} ulp"));
        }
        if parts.is_empty() {
            "exact".into()
        } else {
            parts.join(" | ")
        }
    }
}

/// One rule of a tolerance file: a quantity pattern and its spec.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToleranceRule {
    /// Quantity name or glob pattern (`*` matches any sequence, including
    /// `/`).
    pub quantity: String,
    /// Optional restriction to some case ids (glob), for area-level files.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub case: Option<String>,
    /// The spec.
    #[serde(flatten)]
    pub spec: ToleranceSpec,
}

/// Content of a `tolerances.json` file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct ToleranceFile {
    /// Schema version (1).
    pub format_version: u32,
    /// Rules, first match wins.
    pub rules: Vec<ToleranceRule>,
}

impl ToleranceFile {
    /// Read and validate a tolerance file.
    pub fn read(path: &Path) -> Result<Self> {
        let text = std::fs::read_to_string(path).map_err(|e| Error::io(path, e))?;
        let f: ToleranceFile = serde_json::from_str(&text).map_err(|e| Error::json(path, e))?;
        if f.format_version != 1 {
            return Err(Error::InvalidTolerance {
                context: path.display().to_string(),
                reason: format!("unsupported format_version {}", f.format_version),
            });
        }
        for r in &f.rules {
            r.spec
                .validate(&format!("{} rule `{}`", path.display(), r.quantity))?;
        }
        Ok(f)
    }

    /// First rule matching `quantity` (and `case_id` when the rule is
    /// restricted).
    #[must_use]
    pub fn lookup(&self, case_id: &str, quantity: &str) -> Option<&ToleranceRule> {
        self.rules.iter().find(|r| {
            glob_match(&r.quantity, quantity)
                && r.case.as_ref().is_none_or(|c| glob_match(c, case_id))
        })
    }
}

/// A resolved tolerance, with the file it came from (for reports).
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedTolerance {
    /// The spec.
    pub spec: ToleranceSpec,
    /// File and pattern that provided it, or `code` for in-code specs.
    pub origin: String,
}

/// Resolve the tolerance of `quantity` in case `case_id`: the case-level
/// `tolerances.json` first, then each ancestor directory up to (and
/// including) the area directory under `fixtures_root`.
pub fn resolve(fixtures_root: &Path, case_id: &str, quantity: &str) -> Result<ResolvedTolerance> {
    for dir in tolerance_search_path(fixtures_root, case_id) {
        let file = dir.join(TOLERANCE_FILE);
        if file.is_file() {
            let tf = ToleranceFile::read(&file)?;
            if let Some(rule) = tf.lookup(case_id, quantity) {
                return Ok(ResolvedTolerance {
                    spec: rule.spec.clone(),
                    origin: format!("{} [{}]", file.display(), rule.quantity),
                });
            }
        }
    }
    Err(Error::MissingTolerance {
        case_id: case_id.to_string(),
        quantity: quantity.to_string(),
    })
}

/// Directories searched for tolerance files, most specific first.
#[must_use]
pub fn tolerance_search_path(fixtures_root: &Path, case_id: &str) -> Vec<PathBuf> {
    let segs: Vec<&str> = case_id.split('/').collect();
    (1..=segs.len())
        .rev()
        .map(|n| {
            segs[..n]
                .iter()
                .fold(fixtures_root.to_path_buf(), |p, s| p.join(s))
        })
        .collect()
}

/// Glob matching where `*` matches any (possibly empty) sequence and `?`
/// matches one character.
#[must_use]
pub fn glob_match(pattern: &str, text: &str) -> bool {
    let p: Vec<char> = pattern.chars().collect();
    let t: Vec<char> = text.chars().collect();
    let (mut pi, mut ti) = (0usize, 0usize);
    let (mut star, mut mark) = (None::<usize>, 0usize);
    while ti < t.len() {
        if pi < p.len() && (p[pi] == '?' || p[pi] == t[ti]) {
            pi += 1;
            ti += 1;
        } else if pi < p.len() && p[pi] == '*' {
            star = Some(pi);
            mark = ti;
            pi += 1;
        } else if let Some(s) = star {
            pi = s + 1;
            mark += 1;
            ti = mark;
        } else {
            return false;
        }
    }
    while pi < p.len() && p[pi] == '*' {
        pi += 1;
    }
    pi == p.len()
}

/// Whether a `reviewed_by` value is a placeholder instead of the name of a
/// review: empty after trimming, or (ignoring case and surrounding white
/// space) `pending`, `todo`, `tbd` or `proposed` (a rule whose
/// `"pending"` was once counted as a review).
#[must_use]
pub fn is_placeholder_review(reviewed_by: &str) -> bool {
    let t = reviewed_by.trim().to_ascii_lowercase();
    t.is_empty() || ["pending", "todo", "tbd", "proposed"].contains(&t.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn globbing() {
        assert!(glob_match("shapes/*", "shapes/order_1"));
        assert!(glob_match("*", "anything/at/all"));
        assert!(glob_match("stress/_tri*_3", "stress/_triangle_3"));
        assert!(glob_match("a?c", "abc"));
        assert!(!glob_match("a?c", "ac"));
        assert!(!glob_match("shapes/*", "dnds/order_1"));
        assert!(glob_match("exact", "exact"));
        assert!(!glob_match("exact", "exactly"));
        assert!(glob_match("*ly", "exactly"));
    }

    #[test]
    fn spec_validation_and_summary() {
        assert!(ToleranceSpec::exact("bitwise").validate("t").is_ok());
        assert!(ToleranceSpec::exact("  ").validate("t").is_err());
        assert!(
            ToleranceSpec::abs_rel(-1.0, 0.0, "x")
                .validate("t")
                .is_err()
        );
        assert!(
            ToleranceSpec::abs_rel(0.0, f64::NAN, "x")
                .validate("t")
                .is_err()
        );
        assert!(
            ToleranceSpec::abs_rel(0.0, 1e-9, "x")
                .with_scale(Scale::Value(0.0))
                .validate("t")
                .is_err()
        );
        assert_eq!(ToleranceSpec::exact("x").summary(), "exact");
        assert_eq!(ToleranceSpec::ulps(4, "x").summary(), "4 ulp");
        let s = ToleranceSpec::abs_rel(1e-12, 1e-10, "x").with_scale(Scale::MaxAbs);
        assert_eq!(s.summary(), "abs=1e-12 rel=1e-10(max_abs)");
    }

    #[test]
    fn grouped_scale_needs_its_axes() {
        // A per-component tolerance on a [step,row,col] tensor must be
        // rejected, not silently turned into a whole-array scale.
        let axes = |a: &[&str]| a.iter().map(|s| (*s).to_string()).collect::<Vec<_>>();
        let comp = ToleranceSpec::abs_rel(0.0, 1e-13, "x").with_scale(Scale::MaxAbsPerComponent);
        let e = comp
            .check_axes("stress", &axes(&["step", "row", "col"]), None)
            .unwrap_err();
        assert!(matches!(e, Error::InvalidTolerance { .. }), "{e}");
        assert!(e.to_string().contains("absent axis"), "{e}");
        comp.check_axes("u", &axes(&["node", "component"]), None)
            .unwrap();
        let step = ToleranceSpec::abs_rel(0.0, 1e-8, "x").with_scale(Scale::MaxAbsPerStep);
        assert!(step.check_axes("r", &axes(&["iteration"]), None).is_err());
        // Ragged rows along `step` satisfy a per-step scale.
        step.check_axes("r", &axes(&["iteration"]), Some("step"))
            .unwrap();
        let group = ToleranceSpec::abs_rel(0.0, 1e-13, "x")
            .with_scale(Scale::MaxAbsPerGroup(vec!["row".into(), "col".into()]));
        group
            .check_axes("stress", &axes(&["step", "row", "col"]), None)
            .unwrap();
        assert!(
            group
                .check_axes("u", &axes(&["node", "component"]), None)
                .is_err()
        );
        assert!(
            ToleranceSpec::abs_rel(0.0, 1e-13, "x")
                .with_scale(Scale::MaxAbsPerGroup(vec![]))
                .validate("t")
                .is_err()
        );
        assert!(
            ToleranceSpec::abs_rel(0.0, 1e-13, "x")
                .with_scale(Scale::MaxAbsPerGroup(vec!["row".into(), "row".into()]))
                .validate("t")
                .is_err()
        );
        assert_eq!(
            group.summary(),
            "abs=0e0 rel=1e-13(max_abs_per_group:row,col)"
        );
        // JSON form of the grouped scale.
        let json = serde_json::to_string(&group.scale).unwrap();
        assert_eq!(json, r#"{"max_abs_per_group":["row","col"]}"#);
        let back: Scale = serde_json::from_str(&json).unwrap();
        assert_eq!(back, group.scale);
        assert!(!group.is_reviewed());
    }

    /// A placeholder `reviewed_by` is refused when
    /// the file is read (and never counts as a review); a named review, or
    /// no `reviewed_by` at all (a proposal), is accepted.
    #[test]
    fn placeholder_reviews_are_refused() {
        let dir = tempfile::tempdir().unwrap();
        let file = |reviewed: Option<&str>| {
            let path = dir.path().join(TOLERANCE_FILE);
            let field = reviewed.map_or_else(String::new, |r| {
                format!(r#","reviewed_by":{}"#, serde_json::to_string(r).unwrap())
            });
            std::fs::write(
                &path,
                format!(
                    r#"{{"format_version":1,"rules":[{{"quantity":"x","justification":"exact"{field}}}]}}"#
                ),
            )
            .unwrap();
            ToleranceFile::read(&path)
        };
        for p in ["pending", "Pending", " TBD ", "todo", "PROPOSED", "", "   "] {
            let e = file(Some(p)).unwrap_err();
            assert!(matches!(e, Error::InvalidTolerance { .. }), "{p:?}: {e:?}");
            assert!(format!("{e}").contains("placeholder"), "{p:?}: {e}");
            assert!(is_placeholder_review(p), "{p:?}");
        }
        let named = file(Some("independent review during the port, November 2026")).unwrap();
        assert!(named.rules[0].spec.is_reviewed());
        let proposal = file(None).unwrap();
        assert!(!proposal.rules[0].spec.is_reviewed());
        assert!(!is_placeholder_review("pending review by the maintainers"));
    }

    #[test]
    fn file_resolution_order() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let case = root.join("fe/element_class/_triangle_3");
        std::fs::create_dir_all(&case).unwrap();
        std::fs::write(
            root.join("fe").join(TOLERANCE_FILE),
            r#"{"format_version":1,"rules":[
                {"quantity":"shapes/*","ulps":8,"justification":"area default"},
                {"quantity":"jacobians/*","case":"fe/other/*","abs":1.0,"justification":"other cases only"}
            ]}"#,
        )
        .unwrap();
        std::fs::write(
            case.join(TOLERANCE_FILE),
            r#"{"format_version":1,"rules":[
                {"quantity":"shapes/order_1","ulps":0,"justification":"exact rational values"}
            ]}"#,
        )
        .unwrap();
        let id = "fe/element_class/_triangle_3";
        let r = resolve(root, id, "shapes/order_1").unwrap();
        assert_eq!(r.spec.ulps, Some(0));
        assert!(r.origin.contains("_triangle_3"));
        let r = resolve(root, id, "shapes/order_2").unwrap();
        assert_eq!(r.spec.ulps, Some(8));
        let e = resolve(root, id, "jacobians/a").unwrap_err();
        assert!(matches!(e, Error::MissingTolerance { .. }));
        assert_eq!(
            resolve(root, "fe/other/x", "jacobians/a").unwrap().spec.abs,
            1.0
        );

        std::fs::write(
            case.join(TOLERANCE_FILE),
            r#"{"format_version":1,"rules":[{"quantity":"x","justification":""}]}"#,
        )
        .unwrap();
        assert!(matches!(
            resolve(root, id, "x").unwrap_err(),
            Error::InvalidTolerance { .. }
        ));
    }

    #[test]
    fn search_path() {
        let p = tolerance_search_path(Path::new("/f"), "a/b/c");
        assert_eq!(
            p,
            vec![
                PathBuf::from("/f/a/b/c"),
                PathBuf::from("/f/a/b"),
                PathBuf::from("/f/a")
            ]
        );
    }
}
