// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). No Akantu
// counterpart: baseline-measurement tool of the validation methodology
// (docs/NUMERICAL_VALIDATION.md).
//
//! `fixture-diff <root_a> <root_b> [--json] [--case <glob>]`
//!
//! Compares two fixture trees (e.g. `strict` vs `default` oracle builds, or
//! a regeneration against the committed fixtures) **symmetrically**:
//!
//! * cases and quantities present on one side only, unreadable manifests,
//!   dtype/shape/axis/layout changes, input and expectation changes are
//!   *structural problems* (exit code 1);
//! * for every quantity present on both sides the tool reports the
//!   numerical difference (max absolute, relative and ULP difference,
//!   number of differing entries) **and**, separately, whether the `.npy`
//!   files are bit-identical (a `-0`/`+0` or NaN-payload change is
//!   numerically equivalent but not bit-identical);
//! * provenance/build/parameter differences are reported as metadata;
//! * a case's platform variants (`variants/<environment>/`, D-073) are not
//!   compared: both sides are read at their top level
//!   ([`Fixture::open_case_top`]), i.e. the authoritative golden values.
//!
//! Numerical differences never fail the process (measurement only); the
//! exit code is 2 for usage errors and nonexistent/unreadable roots, 1 for
//! structural problems or an empty selection, 0 otherwise.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use akantu_validation::fixture::{Fixture, MANIFEST_FILE};
use akantu_validation::manifest::{Manifest, Quantity};
use akantu_validation::npy::Dtype;
use akantu_validation::report::{Outcome, QuantityReport};
use akantu_validation::tolerance::{ResolvedTolerance, ToleranceSpec, glob_match};
use akantu_validation::{CompareOptions, compare_f64, compare_i64};

fn cases(root: &Path) -> std::io::Result<BTreeSet<String>> {
    fn walk(root: &Path, dir: &Path, out: &mut BTreeSet<String>) -> std::io::Result<()> {
        let mut paths: Vec<PathBuf> = std::fs::read_dir(dir)?
            .collect::<std::io::Result<Vec<_>>>()?
            .into_iter()
            .map(|e| e.path())
            .collect();
        paths.sort();
        for p in paths {
            if p.is_dir() {
                if p.join(MANIFEST_FILE).is_file()
                    && let Ok(rel) = p.strip_prefix(root)
                {
                    let id: Vec<String> = rel
                        .iter()
                        .map(|s| s.to_string_lossy().into_owned())
                        .collect();
                    out.insert(id.join("/"));
                }
                walk(root, &p, out)?;
            }
        }
        Ok(())
    }
    let mut out = BTreeSet::new();
    walk(root, root, &mut out)?;
    Ok(out)
}

/// Structural (comparison-relevant) differences between two quantity
/// descriptions.
fn quantity_structure_diffs(a: &Quantity, b: &Quantity) -> Vec<String> {
    let mut d = Vec::new();
    if a.dtype != b.dtype {
        d.push(format!("dtype {} vs {}", a.dtype, b.dtype));
    }
    if a.shape != b.shape {
        d.push(format!("shape {:?} vs {:?}", a.shape, b.shape));
    }
    if a.axes != b.axes {
        d.push(format!("axes {:?} vs {:?}", a.axes, b.axes));
    }
    if a.layout != b.layout {
        d.push(format!("layout {:?} vs {:?}", a.layout, b.layout));
    }
    if a.components != b.components {
        d.push(format!(
            "components {:?} vs {:?}",
            a.components, b.components
        ));
    }
    if a.ragged != b.ragged {
        d.push("ragged description differs".into());
    }
    if a.ordering != b.ordering {
        d.push("ordering keys differ".into());
    }
    if a.element_type != b.element_type || a.ghost_type != b.ghost_type {
        d.push(format!(
            "element/ghost type {:?}/{:?} vs {:?}/{:?}",
            a.element_type, a.ghost_type, b.element_type, b.ghost_type
        ));
    }
    d
}

/// Case-level metadata differences: `(structural, informational)`.
fn metadata_diffs(a: &Manifest, b: &Manifest) -> (Vec<String>, Vec<String>) {
    let mut structural = Vec::new();
    let mut info = Vec::new();
    let inputs = |m: &Manifest| -> BTreeMap<String, (Option<String>, String)> {
        m.inputs
            .iter()
            .map(|i| (i.role.clone(), (i.source.clone(), i.sha256.clone())))
            .collect()
    };
    let (ia, ib) = (inputs(a), inputs(b));
    for role in ia.keys().chain(ib.keys()).collect::<BTreeSet<_>>() {
        match (ia.get(role), ib.get(role)) {
            (Some(x), Some(y)) if x == y => {}
            (Some(x), Some(y)) => structural.push(format!(
                "input `{role}` differs: {:?} sha256 {}… vs {:?} sha256 {}…",
                x.0,
                &x.1[..12.min(x.1.len())],
                y.0,
                &y.1[..12.min(y.1.len())]
            )),
            (Some(_), None) => structural.push(format!("input `{role}` only in A")),
            (None, Some(_)) => structural.push(format!("input `{role}` only in B")),
            (None, None) => {}
        }
    }
    if a.expect != b.expect {
        structural.push(format!(
            "expectation differs: {:?}/{:?} vs {:?}/{:?}",
            a.expect.status, a.expect.error_kind, b.expect.status, b.expect.error_kind
        ));
    }
    if a.steps.as_ref().map(|s| &s.recorded) != b.steps.as_ref().map(|s| &s.recorded) {
        structural.push("recorded steps differ".into());
    }
    if a.parallel != b.parallel {
        structural.push("parallel layout differs".into());
    }
    if a.reference.sha != b.reference.sha {
        info.push(format!(
            "reference.sha {} vs {}",
            a.reference.sha, b.reference.sha
        ));
    }
    if a.reference.patches != b.reference.patches {
        info.push(format!(
            "reference.patches {:?} vs {:?}",
            a.reference.patches, b.reference.patches
        ));
    }
    match (&a.build, &b.build) {
        (Some(x), Some(y)) => {
            let fields = [
                ("build.config", &x.config, &y.config),
                ("build.compiler", &x.compiler, &y.compiler),
                ("build.cxx_flags", &x.cxx_flags, &y.cxx_flags),
                (
                    "build.probe_cxx_flags",
                    &x.probe_cxx_flags,
                    &y.probe_cxx_flags,
                ),
                ("build.build_type", &x.build_type, &y.build_type),
                ("build.arch", &x.arch, &y.arch),
                ("build.os", &x.os, &y.os),
            ];
            for (name, u, v) in fields {
                if u != v {
                    info.push(format!("{name} {u:?} vs {v:?}"));
                }
            }
            for k in x
                .libraries
                .keys()
                .chain(y.libraries.keys())
                .collect::<BTreeSet<_>>()
            {
                if x.libraries.get(k) != y.libraries.get(k) {
                    info.push(format!(
                        "build.libraries.{k} {:?} vs {:?}",
                        x.libraries.get(k),
                        y.libraries.get(k)
                    ));
                }
            }
        }
        (None, None) => {}
        _ => info.push("build info present on one side only".into()),
    }
    if a.generator.probe != b.generator.probe
        || a.generator.probe_version != b.generator.probe_version
    {
        info.push(format!(
            "generator {}@{} vs {}@{}",
            a.generator.probe,
            a.generator.probe_version,
            b.generator.probe,
            b.generator.probe_version
        ));
    }
    if a.status != b.status {
        structural.push(format!("status {:?} vs {:?}", a.status, b.status));
    }
    if a.parameters != b.parameters {
        let keys = |v: &serde_json::Value| -> BTreeSet<String> {
            v.as_object()
                .map(|o| o.keys().cloned().collect())
                .unwrap_or_default()
        };
        let changed: Vec<String> = keys(&a.parameters)
            .union(&keys(&b.parameters))
            .filter(|k| a.parameters.get(k) != b.parameters.get(k))
            .cloned()
            .collect();
        info.push(format!("parameters differ in {changed:?}"));
    }
    (structural, info)
}

#[derive(Debug, Default)]
struct Summary {
    reports: Vec<QuantityReport>,
    /// Quantities whose files are byte-identical.
    bit_identical: BTreeSet<(String, String)>,
    /// Numerically equivalent but not bit-identical (signed zero, NaN
    /// payload, header spelling).
    bits_only: Vec<(String, String)>,
    problems: Vec<String>,
    metadata: Vec<String>,
    n_cases_compared: usize,
}

fn compare_case(a_root: &Path, b_root: &Path, id: &str, out: &mut Summary) {
    let (fa, fb) = match (
        Fixture::open_case_top(a_root, id),
        Fixture::open_case_top(b_root, id),
    ) {
        (Ok(a), Ok(b)) => (a, b),
        (Err(e), Ok(_)) => {
            out.problems.push(format!("{id}: invalid in A: {e}"));
            return;
        }
        (Ok(_), Err(e)) => {
            out.problems.push(format!("{id}: invalid in B: {e}"));
            return;
        }
        (Err(ea), Err(eb)) => {
            out.problems
                .push(format!("{id}: invalid in A ({ea}) and in B ({eb})"));
            return;
        }
    };
    for (label, f) in [("A", &fa), ("B", &fb)] {
        if let Err(e) = f.check_materialized() {
            out.problems.push(format!(
                "{id}: data not materialised in {label}, nothing compared: {e}"
            ));
            return;
        }
    }
    out.n_cases_compared += 1;
    let (structural, info) = metadata_diffs(fa.manifest(), fb.manifest());
    out.problems
        .extend(structural.into_iter().map(|s| format!("{id}: {s}")));
    out.metadata
        .extend(info.into_iter().map(|s| format!("{id}: {s}")));

    let names_a: BTreeSet<&str> = fa
        .manifest()
        .quantities
        .iter()
        .map(|q| q.name.as_str())
        .collect();
    let names_b: BTreeSet<&str> = fb
        .manifest()
        .quantities
        .iter()
        .map(|q| q.name.as_str())
        .collect();
    for name in names_a.difference(&names_b) {
        out.problems
            .push(format!("{id}: quantity {name} only in A"));
    }
    for name in names_b.difference(&names_a) {
        out.problems
            .push(format!("{id}: quantity {name} only in B"));
    }
    let measure = ResolvedTolerance {
        spec: ToleranceSpec {
            reviewed_by: Some("fixture-diff measurement".into()),
            ..ToleranceSpec::exact("measurement only: every difference is reported")
        },
        origin: "fixture-diff".into(),
    };
    for name in names_a.intersection(&names_b) {
        let (Ok(qa), Ok(qb)) = (fa.quantity(name), fb.quantity(name)) else {
            continue;
        };
        let diffs = quantity_structure_diffs(qa, qb);
        if !diffs.is_empty() {
            out.problems
                .push(format!("{id}: {name}: {}", diffs.join("; ")));
            continue;
        }
        let bytes = (
            std::fs::read(fa.dir().join(&qa.file)),
            std::fs::read(fb.dir().join(&qb.file)),
        );
        let bit_identical = match bytes {
            (Ok(x), Ok(y)) => x == y,
            (Err(e), _) => {
                out.problems
                    .push(format!("{id}: {name}: cannot read A data: {e}"));
                continue;
            }
            (_, Err(e)) => {
                out.problems
                    .push(format!("{id}: {name}: cannot read B data: {e}"));
                continue;
            }
        };
        let r = if qa.parsed_dtype() == Some(Dtype::F64) {
            match (fa.load_f64(name), fb.load_f64(name)) {
                (Ok(x), Ok(y)) => compare_f64(
                    id,
                    name,
                    &x,
                    &y.shape,
                    &y.data,
                    &measure,
                    // numerical equivalence; bit identity is reported
                    // separately from the raw bytes above
                    &CompareOptions {
                        value_equality: true,
                        ..CompareOptions::default()
                    },
                ),
                (Err(e), _) | (_, Err(e)) => {
                    out.problems.push(format!("{id}: {name}: {e}"));
                    continue;
                }
            }
        } else {
            match (fa.load_i64(name), fb.load_i64(name)) {
                (Ok(x), Ok(y)) => compare_i64(id, name, &x, &y.shape, &y.data),
                (Err(e), _) | (_, Err(e)) => {
                    out.problems.push(format!("{id}: {name}: {e}"));
                    continue;
                }
            }
        };
        if r.outcome == Outcome::ShapeMismatch || r.outcome == Outcome::InvalidTolerance {
            out.problems.push(format!(
                "{id}: {name}: {}",
                r.message.clone().unwrap_or_default()
            ));
            continue;
        }
        if bit_identical {
            out.bit_identical
                .insert((id.to_string(), (*name).to_string()));
        } else if r.n_violations == 0 {
            out.bits_only.push((id.to_string(), (*name).to_string()));
        }
        out.reports.push(r);
    }
}

/// Parsed command line.
struct Args {
    a_root: PathBuf,
    b_root: PathBuf,
    json: bool,
    filter: String,
}

const USAGE: &str = "usage: fixture-diff <root_a> <root_b> [--json] [--case <glob>]";

fn parse_args(args: &[String]) -> std::result::Result<Args, String> {
    let mut roots = Vec::new();
    let mut json = false;
    let mut filter = None;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--json" => json = true,
            "--case" => match it.next() {
                Some(g) if !g.starts_with("--") => {
                    if filter.replace(g.clone()).is_some() {
                        return Err("--case given twice".into());
                    }
                }
                _ => return Err("--case needs a glob argument".into()),
            },
            flag if flag.starts_with("--") => return Err(format!("unknown option `{flag}`")),
            root => roots.push(PathBuf::from(root)),
        }
    }
    if roots.len() != 2 {
        return Err(format!("expected two fixture roots, got {}", roots.len()));
    }
    let b_root = roots.pop().unwrap_or_default();
    let a_root = roots.pop().unwrap_or_default();
    Ok(Args {
        a_root,
        b_root,
        json,
        filter: filter.unwrap_or_else(|| "*".into()),
    })
}

fn main() -> ExitCode {
    let raw: Vec<String> = std::env::args().skip(1).collect();
    if raw.iter().any(|a| a == "-h" || a == "--help") {
        println!("{USAGE}");
        return ExitCode::SUCCESS;
    }
    let Args {
        a_root,
        b_root,
        json,
        filter,
    } = match parse_args(&raw) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("fixture-diff: {e}\n{USAGE}");
            return ExitCode::from(2);
        }
    };
    for (label, root) in [("A", &a_root), ("B", &b_root)] {
        if !root.is_dir() {
            eprintln!(
                "fixture-diff: root {label} `{}` is not a readable directory",
                root.display()
            );
            return ExitCode::from(2);
        }
    }
    let (cases_a, cases_b) = match (cases(&a_root), cases(&b_root)) {
        (Ok(a), Ok(b)) => (a, b),
        (Err(e), _) => {
            eprintln!("fixture-diff: cannot walk A `{}`: {e}", a_root.display());
            return ExitCode::from(2);
        }
        (_, Err(e)) => {
            eprintln!("fixture-diff: cannot walk B `{}`: {e}", b_root.display());
            return ExitCode::from(2);
        }
    };
    let selected = |c: &&String| glob_match(&filter, c);
    let sel_a: BTreeSet<&String> = cases_a.iter().filter(selected).collect();
    let sel_b: BTreeSet<&String> = cases_b.iter().filter(selected).collect();
    let mut out = Summary::default();
    if sel_a.is_empty() && sel_b.is_empty() {
        out.problems.push(format!(
            "no case matches `{filter}` in either tree (A has {} cases, B has {}): nothing was measured",
            cases_a.len(),
            cases_b.len()
        ));
    }
    for id in sel_a.difference(&sel_b) {
        out.problems.push(format!("{id}: case only in A"));
    }
    for id in sel_b.difference(&sel_a) {
        out.problems.push(format!("{id}: case only in B"));
    }
    for id in sel_a.intersection(&sel_b) {
        compare_case(&a_root, &b_root, id, &mut out);
    }

    let n = out.reports.len();
    let numerically_equal = out.reports.iter().filter(|r| r.n_violations == 0).count();
    let bit_identical = out.bit_identical.len();
    if json {
        let v: Vec<serde_json::Value> = out
            .reports
            .iter()
            .map(|r| serde_json::to_value(r).unwrap_or(serde_json::Value::Null))
            .collect();
        let j = serde_json::json!({
            "cases_compared": out.n_cases_compared,
            "quantities_compared": n,
            "numerically_equal": numerically_equal,
            "bit_identical": bit_identical,
            "bits_only_differences": out.bits_only.iter().map(|(c, q)| format!("{c}::{q}")).collect::<Vec<_>>(),
            "quantities": v,
            "metadata": out.metadata,
            "problems": out.problems,
        });
        println!("{}", serde_json::to_string_pretty(&j).unwrap_or_default());
    } else {
        println!(
            "{:<44} {:<36} {:>8} {:>10} {:>10} {:>8}",
            "case", "quantity", "differ", "max_abs", "max_rel", "max_ulp"
        );
        for r in &out.reports {
            if r.n_violations == 0 {
                continue;
            }
            let abs = r.max_abs_err.as_ref().map_or(0.0, |e| e.abs_err);
            let rel = r.max_rel_err.as_ref().map_or(0.0, |e| e.rel_err);
            let ulp = r.max_ulp.as_ref().and_then(|e| e.ulps).unwrap_or(0);
            println!(
                "{:<44} {:<36} {:>8} {:>10.3e} {:>10.3e} {:>8}",
                r.case_id, r.quantity, r.n_violations, abs, rel, ulp
            );
        }
        for (c, q) in &out.bits_only {
            println!(
                "bits-only: {c} :: {q} (numerically equivalent, bytes differ: signed zero, NaN payload or header)"
            );
        }
        println!(
            "{} case(s), {n} quantities compared: {numerically_equal} numerically equal ({bit_identical} bit-identical, {} bits-only), {} differ numerically",
            out.n_cases_compared,
            out.bits_only.len(),
            n - numerically_equal
        );
        for m in &out.metadata {
            println!("metadata: {m}");
        }
        for p in &out.problems {
            println!("problem: {p}");
        }
    }
    if out.problems.is_empty() {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}
