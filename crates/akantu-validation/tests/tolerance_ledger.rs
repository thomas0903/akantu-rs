// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). No Akantu
// counterpart: the tolerance ledger
// `docs/TOLERANCE_LEDGER.md` stays current.

//! `docs/TOLERANCE_LEDGER.md` lists every rule of every
//! `fixtures/**/tolerances.json` with the review that accepted it. It is
//! written by `scripts/gold/tolerance_ledger.py`; these tests fail when a
//! tolerance file changes without the ledger being regenerated:
//!
//! * the script's `--check` (needs `python3`, standard library only; where
//!   Python is genuinely unavailable, `AKANTU_VALIDATION_SKIP_PYTHON=1`
//!   skips that test explicitly, as in `parity_scripts.rs`);
//! * independently of Python, every rule read by the harness
//!   (`ToleranceFile::read`) has its row in the ledger, in order, with the
//!   status `ToleranceSpec::is_reviewed` gives, with its case pattern and
//!   with the criterion the harness reads (exact exactly when
//!   `ToleranceSpec::is_exact`; the `abs`, `rel`, scale, ULP allowance and
//!   NaN policy as numbers), and the counts of the ledger's marker line are
//!   the harness's counts. `row_check_catches_a_changed_rule` mutates a
//!   rule in each of these respects and requires the row check to fail
//!   (a value change used to pass without Python).
//!
//! `tolerance_files.rs` (every file reads and validates) stays as it is.

use std::path::{Path, PathBuf};
use std::process::Command;

use akantu_validation::fixture::workspace_root;
use akantu_validation::tolerance::{NanPolicy, Scale, ToleranceFile, ToleranceRule};

const REGENERATE: &str = "re-run `python3 scripts/gold/tolerance_ledger.py` and commit \
     docs/TOLERANCE_LEDGER.md with the tolerance file that changed";

/// Every `tolerances.json` under `dir`, sorted by path relative to the
/// workspace root (the script's order).
fn tolerance_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap_or_else(|e| panic!("{}: {e}", dir.display())) {
        let p = entry.unwrap().path();
        if p.is_dir() {
            tolerance_files(&p, out);
        } else if p.file_name().is_some_and(|n| n == "tolerances.json") {
            out.push(p);
        }
    }
}

/// The script reports that the committed ledger equals a fresh rendering.
#[test]
fn tolerance_ledger_is_current() {
    let root = workspace_root();
    let out = Command::new("python3")
        .arg("-B")
        .arg(root.join("scripts/gold/tolerance_ledger.py"))
        .arg("--check")
        .current_dir(&root)
        .output();
    let out = match out {
        Ok(o) => o,
        Err(e) if std::env::var("AKANTU_VALIDATION_SKIP_PYTHON").is_ok_and(|v| v == "1") => {
            eprintln!("python3 unavailable ({e}); skipped by AKANTU_VALIDATION_SKIP_PYTHON=1");
            return;
        }
        Err(e) => {
            panic!("cannot run python3 ({e}); install it or set AKANTU_VALIDATION_SKIP_PYTHON=1")
        }
    };
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        out.status.success() && text.contains("tolerance ledger: current"),
        "{REGENERATE}:\n{text}"
    );
}

/// A string as `code()` of the script writes it into a table cell: white
/// space collapsed, `|` escaped, in a code span.
fn code_cell(text: &str) -> String {
    let cell = text
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .replace('|', "\\|");
    if cell.contains('`') {
        format!("``{cell}``")
    } else {
        format!("`{cell}`")
    }
}

/// `token` is the number `v`, written in the ledger as in the tolerance file.
/// The harness reads the file with serde_json, whose float parser is not
/// always correctly rounded: a few units in the last place of slack, far
/// below any change of a rule's value.
fn same_number(token: &str, v: f64) -> bool {
    token
        .parse::<f64>()
        .is_ok_and(|p| p == v || (p - v).abs() <= 8.0 * f64::EPSILON * p.abs().max(v.abs()))
}

/// The number written after the token `key` in the criterion cell.
fn number_after<'a>(tokens: &[&'a str], key: &str) -> Option<&'a str> {
    let at = tokens.iter().position(|t| *t == key)?;
    tokens.get(at + 1).copied()
}

/// What the criterion cell must say for `rule` (the script's `criterion`).
fn criterion_problems(rule: &ToleranceRule, criterion: &str) -> Vec<String> {
    let spec = &rule.spec;
    let mut p = Vec::new();
    if criterion.starts_with("exact") != spec.is_exact() {
        p.push(format!(
            "criterion `{criterion}` says exact = {}, the harness says {}",
            criterion.starts_with("exact"),
            spec.is_exact()
        ));
    }
    if (criterion.ends_with("; NaN forbidden")) != (spec.nan == NanPolicy::Forbid) {
        p.push(format!("criterion `{criterion}`: NaN policy differs"));
    }
    let tokens: Vec<&str> = criterion.split_whitespace().collect();
    match number_after(&tokens, "abs") {
        Some(t) if spec.abs != 0.0 && same_number(t, spec.abs) => {}
        None if spec.abs == 0.0 => {}
        _ => p.push(format!("criterion `{criterion}`: abs is {:e}", spec.abs)),
    }
    match number_after(&tokens, "rel") {
        Some(t) if spec.rel != 0.0 && same_number(t, spec.rel) => {
            let after = criterion.split_once(" × ").map_or("", |(_, rest)| rest);
            let ok = if let Scale::Value(v) = &spec.scale {
                // `fixed <v>`, the number as in the tolerance file.
                after.starts_with("fixed ")
                    && number_after(&tokens, "fixed").is_some_and(|t| same_number(t, *v))
            } else {
                let scale = match &spec.scale {
                    Scale::Reference => "\\|e_i\\|".to_string(),
                    Scale::MaxAbs => "max\\|e\\|".to_string(),
                    Scale::MaxAbsPerComponent => "max\\|e\\| per component".to_string(),
                    Scale::MaxAbsPerStep => "max\\|e\\| per step".to_string(),
                    Scale::MaxAbsPerGroup(axes) => format!("max\\|e\\| per ({})", axes.join(", ")),
                    Scale::Value(_) => unreachable!(),
                };
                // The scale is followed by the end of the cell, ` or ≤ N ulp`
                // or `; NaN forbidden`, so `max|e|` is not `max|e| per step`.
                after
                    .strip_prefix(scale.as_str())
                    .is_some_and(|r| r.is_empty() || r.starts_with(" or ") || r.starts_with(';'))
            };
            if !ok {
                p.push(format!(
                    "criterion `{criterion}`: scale is {:?}",
                    spec.scale
                ));
            }
        }
        None if spec.rel == 0.0 => {}
        _ => p.push(format!("criterion `{criterion}`: rel is {:e}", spec.rel)),
    }
    match (
        spec.ulps.unwrap_or(0),
        tokens.iter().position(|t| *t == "ulp"),
    ) {
        (0, None) => {}
        (u, Some(at)) if at > 0 && tokens[at - 1] == u.to_string() => {}
        (u, _) => p.push(format!("criterion `{criterion}`: ULP allowance is {u}")),
    }
    p
}

/// The problems of the ledger row `row` for rule `[i]` of `rel` (the `n`-th
/// rule overall): file, index, case pattern, quantity, criterion, status.
fn row_problems(rel: &str, n: usize, i: usize, rule: &ToleranceRule, row: &str) -> Vec<String> {
    let reviewed = rule.spec.is_reviewed();
    let case = rule
        .case
        .as_deref()
        .map_or_else(|| "—".to_string(), code_cell);
    let head = format!(
        "| {n} | `{rel}` | {i} | {case} | {} | ",
        code_cell(&rule.quantity)
    );
    let Some(rest) = row.strip_prefix(&head) else {
        return vec![format!(
            "row {n} should start `{head}` ({rel} [{i}]) but is: {row}"
        )];
    };
    let status = if reviewed {
        " | accepted | "
    } else {
        " | **PROPOSED** | "
    };
    let Some((criterion, _)) = rest.split_once(status) else {
        return vec![format!(
            "row {n} ({rel} [{i}]) should have the status{status}but is: {row}"
        )];
    };
    criterion_problems(rule, criterion)
}

/// The rows of the ledger's "All rules" table.
fn ledger_rows(ledger: &str) -> Vec<&str> {
    let all_rules = ledger
        .split_once("\n## All rules\n")
        .unwrap_or_else(|| panic!("no `## All rules` section; {REGENERATE}"))
        .1;
    all_rules
        .lines()
        .filter(|l| l.starts_with("| ") && l.as_bytes().get(2).is_some_and(u8::is_ascii_digit))
        .collect()
}

/// Every `tolerances.json` of the workspace, read by the harness, sorted as
/// the script sorts them: (path relative to the root, the file).
fn harness_files(root: &Path) -> Vec<(String, ToleranceFile)> {
    let mut files = Vec::new();
    tolerance_files(&root.join("fixtures"), &mut files);
    files.sort_by_key(|p| p.strip_prefix(root).unwrap().to_string_lossy().into_owned());
    files
        .iter()
        .map(|f| {
            let rel = f
                .strip_prefix(root)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            (
                rel,
                ToleranceFile::read(f).unwrap_or_else(|e| panic!("{e}")),
            )
        })
        .collect()
}

/// Without Python: every rule the harness reads has its ledger row (file,
/// index, case pattern, quantity, criterion, status) in order, and the marker
/// counts agree.
#[test]
fn tolerance_ledger_rows_match_the_harness() {
    let root = workspace_root();
    let ledger_path = root.join("docs/TOLERANCE_LEDGER.md");
    let ledger = std::fs::read_to_string(&ledger_path)
        .unwrap_or_else(|e| panic!("{}: {e}; {REGENERATE}", ledger_path.display()));

    let files = harness_files(&root);
    assert!(
        files.len() >= 40,
        "only {} tolerance files found",
        files.len()
    );
    let rows = ledger_rows(&ledger);

    let (mut n, mut accepted) = (0usize, 0usize);
    let mut problems = Vec::new();
    for (rel, tf) in &files {
        for (i, rule) in tf.rules.iter().enumerate() {
            n += 1;
            accepted += usize::from(rule.spec.is_reviewed());
            let Some(row) = rows.get(n - 1) else {
                problems.push(format!("no row {n} for {rel} [{i}]"));
                continue;
            };
            problems.extend(row_problems(rel, n, i, rule, row));
        }
    }
    if rows.len() != n {
        problems.push(format!(
            "the ledger has {} rule rows, the harness reads {n} rules",
            rows.len()
        ));
    }
    let marker = format!(
        "<!-- ledger: files={} rules={n} accepted={accepted} proposed={} -->",
        files.len(),
        n - accepted
    );
    if !ledger.contains(&marker) {
        problems.push(format!("marker line `{marker}` missing"));
    }
    assert!(problems.is_empty(), "{REGENERATE}:\n{problems:#?}");
    eprintln!(
        "{} files, {n} rules: {accepted} accepted, {} PROPOSED",
        files.len(),
        n - accepted
    );
}

/// The row check itself: it passes on the committed rows and fails when a
/// rule changes in any respect the ledger shows (mutations that the
/// Python-free test used not to see: a value, a case pattern, the review
/// status, and the rest of the criterion).
#[test]
fn row_check_catches_a_changed_rule() {
    let root = workspace_root();
    let ledger = std::fs::read_to_string(root.join("docs/TOLERANCE_LEDGER.md"))
        .unwrap_or_else(|e| panic!("docs/TOLERANCE_LEDGER.md: {e}; {REGENERATE}"));
    let rows = ledger_rows(&ledger);
    let files = harness_files(&root);

    // (n, file, index, rule, row) of the first rule that `pick` selects.
    let find = |pick: &dyn Fn(&ToleranceRule) -> bool| {
        let mut n = 0usize;
        for (rel, tf) in &files {
            for (i, rule) in tf.rules.iter().enumerate() {
                n += 1;
                if pick(rule) {
                    return (n, rel.clone(), i, rule.clone(), rows[n - 1]);
                }
            }
        }
        panic!("no rule selected");
    };
    let check = |what: &str,
                 found: &(usize, String, usize, ToleranceRule, &str),
                 mutate: &dyn Fn(&mut ToleranceRule)| {
        let (n, rel, i, rule, row) = found;
        assert!(
            row_problems(rel, *n, *i, rule, row).is_empty(),
            "unmutated rule has problems: {:?}",
            row_problems(rel, *n, *i, rule, row)
        );
        let mut m = rule.clone();
        mutate(&mut m);
        assert!(
            !row_problems(rel, *n, *i, &m, row).is_empty(),
            "the row check does not see: {what} ({rel} [{i}])"
        );
    };

    let rel_rule =
        find(&|r| r.spec.rel != 0.0 && r.spec.abs == 0.0 && r.spec.scale == Scale::MaxAbs);
    check("rel 1e-12 -> 1e-11", &rel_rule, &|r| r.spec.rel *= 10.0);
    check("rel 1e-12 -> 0 (the rule becomes exact)", &rel_rule, &|r| {
        r.spec.rel = 0.0
    });
    check("a scale change", &rel_rule, &|r| {
        r.spec.scale = Scale::Reference
    });
    check("an abs allowance added", &rel_rule, &|r| r.spec.abs = 1e-30);
    check("an ULP allowance added", &rel_rule, &|r| {
        r.spec.ulps = Some(4)
    });
    check("NaN forbidden", &rel_rule, &|r| {
        r.spec.nan = NanPolicy::Forbid
    });
    check("a case pattern added", &rel_rule, &|r| {
        r.case = Some("*".into())
    });
    check("the quantity", &rel_rule, &|r| r.quantity.push('x'));

    let exact_rule = find(&|r| r.spec.is_exact() && r.case.is_none());
    check("an exact rule gets a tolerance", &exact_rule, &|r| {
        r.spec.rel = 1e-15
    });
    check("an exact rule gets a ULP allowance", &exact_rule, &|r| {
        r.spec.ulps = Some(1)
    });
    check("a case pattern added to an exact rule", &exact_rule, &|r| {
        r.case = Some("x/*".into())
    });

    let cased = find(&|r| r.case.is_some());
    check("a case pattern changed", &cased, &|r| {
        r.case = r.case.as_ref().map(|c| format!("{c}x"));
    });
    check("a case pattern removed", &cased, &|r| r.case = None);

    // The status: a placeholder review is PROPOSED.
    let reviewed = find(&|r| r.spec.is_reviewed());
    check("reviewed_by removed", &reviewed, &|r| {
        r.spec.reviewed_by = None
    });
}
