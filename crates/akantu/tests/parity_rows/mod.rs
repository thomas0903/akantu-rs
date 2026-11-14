// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Parity rows
// DOC-019, DOC-021: generated sections of the user manual that come
// from the parity matrix (`docs/FEATURE_PARITY.md`) and from the timing data
// of upstream's JOSS paper, and the check of the test references of the
// JOSS claims page. Upstream counterparts: joss/paper.md (the claims),
// joss/results/plot.py and joss/results/timmings_*_gcc_jed_v5.0.4.csv (the
// figure "Time to solution with and without cohesive insertion"),
// doc/dev-doc/tutorials.rst (the external tutorials pointer).
//
// Generated sections (`<!-- BEGIN GENERATED: <id> -->`, rewritten with
// `AKANTU_BLESS_MANUAL=1 cargo test -p akantu --test manual`):
//
// * `parity-cited <page>`: the status of every parity row the page cites
//   outside its generated sections, in the matrix's order;
// * `parity-area <AREA>`: the status counts of an area and its rows that
//   are not `Verified`;
// * `joss-figure-data`: median, minimum and maximum time to solution per
//   core count of the two series upstream's `plot.py` draws.
//
// Ids starting with `parity-` are blanked by the availability guard
// (`availability::units`): their status names are not statements.
//
// Two checks of the JOSS claims page: `claim_row_violations` (a row cited by a claim table is
// *Verified*, an approved exception, or named with its status in the claim's
// Notes cell) and `check_test_refs` (a named Rust function is a `#[test]`;
// when it is feature-gated, the reference's line names the feature).

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

/// The statuses of the matrix, in the order of `scripts/parity/stats.py`.
pub const STATUSES: [&str; 7] = [
    "Not started",
    "In progress",
    "Implemented, unverified",
    "Verified",
    "Blocked",
    "Exception (approved)",
    "Superseded",
];

/// One row of the parity matrix.
#[derive(Clone, Debug)]
pub struct ParityRow {
    pub area: String,
    pub feature: String,
    pub status: String,
    /// Position in the matrix (row order of the file).
    pub order: usize,
}

pub fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn read(p: &Path) -> String {
    std::fs::read_to_string(p).unwrap_or_else(|e| panic!("{}: {e}", p.display()))
}

/// Cells of a GFM table row: every pipe not preceded by a backslash
/// separates cells (as `scripts/parity/stats.py::split_row`).
fn split_row(line: &str) -> Vec<String> {
    let t = line.trim();
    let mut cells = Vec::new();
    let mut cur = String::new();
    let mut prev = '\0';
    for c in t.chars() {
        if c == '|' && prev != '\\' {
            cells.push(std::mem::take(&mut cur));
        } else {
            cur.push(c);
        }
        prev = c;
    }
    cells.push(cur);
    // drop what precedes the first pipe and follows the last one
    if cells.len() >= 2 {
        cells.remove(0);
        cells.pop();
    }
    cells.into_iter().map(|c| c.trim().to_string()).collect()
}

/// A parity row id: `AREA-digits`.
fn row_id(cell: &str) -> Option<(&str, &str)> {
    let (area, num) = cell.split_once('-')?;
    (!area.is_empty()
        && area.chars().all(|c| c.is_ascii_uppercase())
        && !num.is_empty()
        && num.chars().all(|c| c.is_ascii_digit()))
    .then_some((area, num))
}

/// The rows of a matrix text. A row whose cell count differs from its
/// table's header or whose status is not one of [`STATUSES`] is an error
/// (the same rule as `stats.py`).
pub fn matrix_from(text: &str) -> Result<BTreeMap<String, ParityRow>, String> {
    let mut rows = BTreeMap::new();
    let mut header: Option<(usize, usize, usize, usize)> = None; // width, status, feature, files
    for (n, line) in text.lines().enumerate() {
        let t = line.trim_start();
        if !t.starts_with('|') {
            continue;
        }
        let cells = split_row(t);
        if cells.first().is_some_and(|c| c == "ID") {
            let find = |name: &str| cells.iter().position(|c| c == name);
            header = match (find("Status"), find("Feature")) {
                (Some(s), Some(f)) => Some((
                    cells.len(),
                    s,
                    f,
                    find("Upstream files")
                        .or_else(|| find("Original files / symbols"))
                        .unwrap_or(f),
                )),
                _ => None,
            };
            continue;
        }
        let Some((width, status, feature, files)) = header else {
            continue;
        };
        let Some((area, _)) = cells.first().and_then(|c| row_id(c)) else {
            continue;
        };
        if cells.len() != width {
            return Err(format!(
                "line {}: {} has {} cells, its table {width}",
                n + 1,
                cells[0],
                cells.len()
            ));
        }
        if !STATUSES.contains(&cells[status].as_str()) {
            return Err(format!(
                "line {}: {} has the status {:?}",
                n + 1,
                cells[0],
                cells[status]
            ));
        }
        let feature = if cells[feature].is_empty() {
            cells[files].clone()
        } else {
            cells[feature].clone()
        };
        let order = rows.len();
        if rows
            .insert(
                cells[0].clone(),
                ParityRow {
                    area: area.to_string(),
                    feature,
                    status: cells[status].clone(),
                    order,
                },
            )
            .is_some()
        {
            return Err(format!("line {}: duplicate row {}", n + 1, cells[0]));
        }
    }
    if rows.is_empty() {
        return Err("no parity rows found".into());
    }
    Ok(rows)
}

/// The committed matrix.
pub fn matrix() -> BTreeMap<String, ParityRow> {
    matrix_from(&read(&root().join("docs/FEATURE_PARITY.md")))
        .unwrap_or_else(|e| panic!("docs/FEATURE_PARITY.md: {e}"))
}

const BEGIN: &str = "<!-- BEGIN GENERATED: ";
const END: &str = "<!-- END GENERATED: ";

/// The text without the bodies of its generated sections.
pub fn strip_generated(text: &str) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(b) = rest.find(BEGIN) {
        out.push_str(&rest[..b]);
        match rest[b..].find(END) {
            Some(e) => rest = &rest[b + e + END.len()..],
            None => return out,
        }
    }
    out.push_str(rest);
    out
}

/// The parity row ids a text cites (`COH-008`, `TASN-016`, …), in order of
/// first appearance: a word `AREA-digits` whose `AREA` is an area of the
/// matrix. Ids of other registers (`UD-890`, `D-085`, `B-009`) are not
/// parity rows and are skipped.
pub fn cited_ids(text: &str, areas: &BTreeSet<String>) -> Vec<String> {
    let is_word = |c: char| c.is_ascii_alphanumeric() || c == '_' || c == '-';
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let mut out: Vec<String> = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let (s, c) = chars[i];
        let starts_word = i == 0 || !is_word(chars[i - 1].1);
        if !(starts_word && c.is_ascii_uppercase()) {
            i += 1;
            continue;
        }
        let mut j = i;
        while j < chars.len() && is_word(chars[j].1) {
            j += 1;
        }
        let e = chars.get(j).map_or(text.len(), |&(p, _)| p);
        let word = &text[s..e];
        if let Some((area, _)) = row_id(word)
            && areas.contains(area)
            && !out.iter().any(|o| o == word)
        {
            out.push(word.to_string());
        }
        i = j.max(i + 1);
    }
    out
}

/// `| Row | Feature | Status |` table of the given ids, in matrix order.
fn status_table(ids: &[String], m: &BTreeMap<String, ParityRow>) -> String {
    let mut ids: Vec<&String> = ids.iter().collect();
    ids.sort_by_key(|id| m[*id].order);
    let mut o = String::from("| Row | Feature | Status |\n|---|---|---|\n");
    for id in ids {
        let r = &m[id];
        writeln!(o, "| `{id}` | {} | {} |", r.feature, r.status).unwrap();
    }
    o
}

/// Body of `parity-cited <page>`: the rows the page cites outside its
/// generated sections. A cited id of a matrix area that is not a row of
/// the matrix is an error.
pub fn cited_status(page_text: &str, m: &BTreeMap<String, ParityRow>) -> Result<String, String> {
    let areas: BTreeSet<String> = m.values().map(|r| r.area.clone()).collect();
    let ids = cited_ids(&strip_generated(page_text), &areas);
    let missing: Vec<&String> = ids.iter().filter(|id| !m.contains_key(*id)).collect();
    if !missing.is_empty() {
        return Err(format!(
            "cites rows the parity matrix does not have: {missing:?}"
        ));
    }
    if ids.is_empty() {
        return Err("cites no parity row".into());
    }
    Ok(status_table(&ids, m))
}

/// Body of `parity-area <AREA>`: status counts and the rows that are not
/// `Verified`.
pub fn area_status(area: &str, m: &BTreeMap<String, ParityRow>) -> Result<String, String> {
    let rows: Vec<(&String, &ParityRow)> = m.iter().filter(|(_, r)| r.area == area).collect();
    if rows.is_empty() {
        return Err(format!("no rows in area {area}"));
    }
    let mut o = format!("| Status | {area} rows |\n|---|---:|\n");
    for s in STATUSES {
        let n = rows.iter().filter(|(_, r)| r.status == s).count();
        if n > 0 {
            writeln!(o, "| {s} | {n} |").unwrap();
        }
    }
    let open: Vec<String> = rows
        .iter()
        .filter(|(_, r)| r.status != "Verified")
        .map(|(id, _)| (*id).clone())
        .collect();
    if !open.is_empty() {
        o.push_str("\nThe rows that are not *Verified*:\n\n");
        o.push_str(&status_table(&open, m));
    }
    Ok(o)
}

/* ---- the JOSS figure's data ------------------------------------------ */

/// Directory of the published timing CSVs (verbatim copies of upstream's
/// `joss/results/timmings_*.csv`, `benchmarks/cohesive-perf/README.md`).
pub fn jed_dir() -> PathBuf {
    root().join("benchmarks/cohesive-perf/results/published-jed")
}

/// Time to solution per core count of one CSV: the totals of the
/// `solve_step` and (cohesive series) `check_cohesive_stress`
/// chronometers, which must both have run 5,000 times.
pub fn jed_series(text: &str, cohesive: bool) -> Result<BTreeMap<u32, Vec<f64>>, String> {
    let mut lines = text.lines().filter(|l| !l.trim().is_empty());
    let header: Vec<String> = lines
        .next()
        .ok_or("empty file")?
        .split(',')
        .map(|c| c.trim().trim_matches('"').to_string())
        .collect();
    let col = |name: &str| {
        header
            .iter()
            .position(|c| c == name)
            .ok_or(format!("no column {name:?}"))
    };
    let psize = col("psize")?;
    let mut parts = vec![(col("solve_step")?, col("solve_step nb_rep")?)];
    if cohesive {
        parts.push((
            col("check_cohesive_stress")?,
            col("check_cohesive_stress nb_rep")?,
        ));
    }
    let mut out: BTreeMap<u32, Vec<f64>> = BTreeMap::new();
    for line in lines {
        let v: Vec<f64> = line
            .split(',')
            .map(|c| c.trim().parse::<f64>())
            .collect::<Result<_, _>>()
            .map_err(|e| format!("{line:?}: {e}"))?;
        if v.len() != header.len() {
            return Err(format!(
                "{line:?}: {} cells, header {}",
                v.len(),
                header.len()
            ));
        }
        let mut tts = 0.0;
        for &(c, rep) in &parts {
            if v[rep] != 5000.0 {
                return Err(format!("{line:?}: {} repetitions, not 5000", v[rep]));
            }
            tts += v[c];
        }
        let p = v[psize];
        if p < 1.0 || p.fract() != 0.0 {
            return Err(format!("{line:?}: core count {p}"));
        }
        out.entry(p as u32).or_default().push(tts);
    }
    Ok(out)
}

/// Median as pandas computes it (mean of the two middle values for an even
/// count).
fn median(v: &[f64]) -> f64 {
    let mut s = v.to_vec();
    s.sort_by(f64::total_cmp);
    let n = s.len();
    if n % 2 == 1 {
        s[n / 2]
    } else {
        (s[n / 2 - 1] + s[n / 2]) / 2.0
    }
}

/// Body of `joss-figure-data`: the two v5.0.4 series of the figure.
pub fn jed_table() -> Result<String, String> {
    let load = |name: &str, cohesive: bool| {
        jed_series(&read(&jed_dir().join(name)), cohesive).map_err(|e| format!("{name}: {e}"))
    };
    let elastic = load("timmings_elastic_gcc_jed_v5.0.4.csv", false)?;
    let cohesive = load("timmings_cohesive_gcc_jed_v5.0.4.csv", true)?;
    let cores: BTreeSet<u32> = elastic.keys().chain(cohesive.keys()).copied().collect();
    let first = *cores.iter().next().ok_or("no data")?;
    let mut o = String::from(
        "| Cores | No insertion: runs | No insertion: TTS s, median [min, max] | No insertion: speed-up | Insertion: runs | Insertion: TTS s, median [min, max] | Insertion: speed-up |\n|---:|---:|---|---:|---:|---|---:|\n",
    );
    let cell = |s: &BTreeMap<u32, Vec<f64>>, p: u32| -> String {
        let Some(v) = s.get(&p) else {
            return "— | — | —".into();
        };
        let lo = v.iter().copied().fold(f64::INFINITY, f64::min);
        let hi = v.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        let up = s.get(&first).map_or("—".to_string(), |f| {
            format!("{:.1}", median(f) / median(v))
        });
        format!("{} | {:.1} [{lo:.1}, {hi:.1}] | {up}", v.len(), median(v))
    };
    for p in cores {
        writeln!(
            o,
            "| {p} | {} | {} |",
            cell(&elastic, p),
            cell(&cohesive, p)
        )
        .unwrap();
    }
    Ok(o)
}

/* ---- test references ---------------------------------------------------- */

/// The inline code spans of a text outside fenced blocks, each with the
/// line (a table row, a paragraph line) it stands on.
fn code_spans(text: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut in_fence = false;
    for line in text.lines() {
        if line.trim_start().starts_with("```") {
            in_fence = !in_fence;
            continue;
        }
        if in_fence {
            continue;
        }
        let mut parts = line.split('`');
        parts.next();
        while let (Some(span), Some(_)) = (parts.next(), parts.next()) {
            out.push((span.to_string(), line.to_string()));
        }
    }
    out
}

/// The attribute lines directly above line `at` of a Rust file (attributes
/// and comments, no blank line between), trimmed.
fn attributes_above<'a>(lines: &[&'a str], at: usize) -> Vec<&'a str> {
    let mut out = Vec::new();
    let mut k = at;
    while k > 0 {
        let t = lines[k - 1].trim();
        if t.starts_with("#[") || t.starts_with("#![") {
            out.push(t);
        } else if !(t.starts_with("//") && !t.starts_with("//!")) {
            break;
        }
        k -= 1;
    }
    out
}

/// The features a `cfg` attribute (or `cfg_attr`) text names:
/// `feature = "x"` pairs.
fn cfg_features(attr: &str) -> Vec<String> {
    let mut out = Vec::new();
    if !attr.contains("cfg") {
        return out;
    }
    let mut rest = attr;
    while let Some(p) = rest.find("feature") {
        rest = &rest[p + "feature".len()..];
        let r = rest.trim_start();
        if let Some(r) = r.strip_prefix('=')
            && let Some(r) = r.trim_start().strip_prefix('"')
            && let Some(e) = r.find('"')
        {
            out.push(r[..e].to_string());
        }
    }
    out
}

/// The line index of `fn name(` (a definition: only visibility, `async`,
/// `unsafe` and `const` precede it on its line) in a Rust file.
fn rust_fn_line(lines: &[&str], name: &str) -> Option<usize> {
    let pat = format!("fn {name}(");
    lines.iter().position(|l| {
        l.trim_start().find(&pat).is_some_and(|p| {
            l.trim_start()[..p].split_whitespace().all(|w| {
                matches!(w, "pub" | "async" | "unsafe" | "const")
                    || (w.starts_with("pub(") && w.ends_with(')'))
            })
        })
    })
}

/// Checks every test reference of a text: a code span
/// `crates/…/x.rs[::name]` or `python/…/x.py[::name]` must name an existing
/// file of the repository and, with `::name`, a function of it (`def name(`
/// in Python; in Rust a definition `fn name(` that carries `#[test]` among
/// the attributes directly above it, a helper that is not a
/// test is refused). A Rust test that is feature-gated (`#[cfg(feature =
/// "x")]` on the function or `#![cfg(feature = "x")]` on the file) does not
/// run in the default gate: the line of the reference must then name the
/// feature in backticks. Returns the number of references and the
/// violations.
pub fn check_test_refs(text: &str, root: &Path) -> (usize, Vec<String>) {
    let mut n = 0;
    let mut v = Vec::new();
    for (span, line) in code_spans(text) {
        if !(span.starts_with("crates/") || span.starts_with("python/")) {
            continue;
        }
        let (path, name) = match span.split_once("::") {
            Some((p, f)) => (p, Some(f)),
            None => (span.as_str(), None),
        };
        let rust = path.ends_with(".rs");
        if !rust && !path.ends_with(".py") {
            continue;
        }
        n += 1;
        let file = root.join(path);
        let Ok(body) = std::fs::read_to_string(&file) else {
            v.push(format!("`{span}`: no file {path}"));
            continue;
        };
        let Some(name) = name else { continue };
        let valid = name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
        if !rust {
            if !(valid && body.contains(&format!("def {name}("))) {
                v.push(format!("`{span}`: {path} defines no `def {name}(`"));
            }
            continue;
        }
        let lines: Vec<&str> = body.lines().collect();
        let Some(at) = valid.then(|| rust_fn_line(&lines, name)).flatten() else {
            v.push(format!("`{span}`: {path} defines no `fn {name}(`"));
            continue;
        };
        let attrs = attributes_above(&lines, at);
        if !attrs.iter().any(|a| a.starts_with("#[test]")) {
            v.push(format!(
                "`{span}`: `fn {name}` of {path} is not a `#[test]`"
            ));
            continue;
        }
        let mut features: Vec<String> = attrs.iter().flat_map(|a| cfg_features(a)).collect();
        features.extend(
            lines
                .iter()
                .take_while(|l| {
                    l.trim().is_empty() || l.trim().starts_with("//") || l.trim().starts_with("#![")
                })
                .filter(|l| l.trim().starts_with("#!["))
                .flat_map(|l| cfg_features(l)),
        );
        for f in features {
            if !line.contains(&format!("`{f}`")) {
                v.push(format!(
                    "`{span}`: the test is gated by feature `{f}` (not run by the default gate); \
                     name it next to the reference"
                ));
            }
        }
    }
    (n, v)
}

/* ---- the claim tables --------------------------------------------------- */

/// The headings of the claim tables of `appendix/joss.md`.
pub const CLAIM_SECTIONS: [&str; 2] = ["## Features", "## High-performance design"];

/// Every parity row cited in the "Parity rows" cell of a claim
/// table (the second cell of a row whose table has four cells) is
/// *Verified* or an approved exception, or the claim's Notes cell (the last
/// cell) names the row together with its status text. A downgrade of a
/// cited row then fails until the prose says so. Returns the number of
/// claim rows read and the violations. Rows that the page cites as "not
/// measured" are outside the claim tables and outside the rule.
pub fn claim_row_violations(page: &str, m: &BTreeMap<String, ParityRow>) -> (usize, Vec<String>) {
    let areas: BTreeSet<String> = m.values().map(|r| r.area.clone()).collect();
    let mut rows = 0;
    let mut v = Vec::new();
    for heading in CLAIM_SECTIONS {
        let Some(b) = page.lines().position(|l| l == heading) else {
            v.push(format!("no section {heading:?}"));
            continue;
        };
        for line in page
            .lines()
            .skip(b + 1)
            .take_while(|l| !l.starts_with("## "))
        {
            if !line.starts_with('|') {
                continue;
            }
            let cells = split_row(line);
            if cells.len() != 4
                || cells[0].starts_with("Claim of the paper")
                || cells[0].starts_with("---")
            {
                continue;
            }
            rows += 1;
            for id in cited_ids(&cells[1], &areas) {
                let Some(r) = m.get(&id) else {
                    v.push(format!("{heading}: `{id}` is not a row of the matrix"));
                    continue;
                };
                let named = cells[3].contains(&id) && cells[3].contains(&r.status);
                if r.status != "Verified" && r.status != "Exception (approved)" && !named {
                    v.push(format!(
                        "{heading}: claim {:?} cites {id}, whose status is {:?}: say so in the \
                         Notes cell (name the row and its status)",
                        cells[0], r.status
                    ));
                }
            }
        }
    }
    (rows, v)
}

/// The body of a generated section this module owns, or `None` for an id
/// of another generator.
pub fn generate(id: &str, src: &Path) -> Option<String> {
    let result = if let Some(page) = id.strip_prefix("parity-cited ") {
        cited_status(&read(&src.join(page)), &matrix())
    } else if let Some(area) = id.strip_prefix("parity-area ") {
        area_status(area, &matrix())
    } else if id == "joss-figure-data" {
        jed_table()
    } else {
        return None;
    };
    Some(result.unwrap_or_else(|e| panic!("generated section {id}: {e}")))
}
