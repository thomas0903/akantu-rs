// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Availability guard
// of the user manual: no upstream
// counterpart (upstream's manual, doc/dev-doc/manual, has no such check).
//
// The single source is `docs/manual/availability.md` (format and rules at
// its top). This module parses it, reads the facts it is checked against
// from the workspace (facade modules, features, placeholder crates), splits
// the manual and the README into sentences, checks every availability
// statement against the table and generates the availability sections.
// Everything works on in-memory copies, so that `tests/manual.rs` can run
// wording tricks and other mutations as negative cases.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/* ---------------------------------------------------------------------- */
/* The table                                                               */
/* ---------------------------------------------------------------------- */

/// Status of a table row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    /// In every default build.
    Available,
    /// Behind a non-default Cargo feature.
    OptIn,
    /// Being ported or not started.
    NotYetAvailable,
    /// Deliberately not ported.
    NotPorted,
    /// The waiting-list page itself.
    WaitingList,
}

impl Status {
    fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "available" => Self::Available,
            "opt-in" => Self::OptIn,
            "not-yet-available" => Self::NotYetAvailable,
            "not-ported" => Self::NotPorted,
            "waiting-list" => Self::WaitingList,
            _ => return None,
        })
    }

    /// Part of a build (default or opt-in).
    pub fn is_in_release(self) -> bool {
        matches!(self, Self::Available | Self::OptIn)
    }

    /// May be named by a "not available" statement.
    fn may_be_negated(self) -> bool {
        matches!(
            self,
            Self::NotYetAvailable | Self::NotPorted | Self::WaitingList
        )
    }
}

/// One row of the availability table.
#[derive(Clone, Debug)]
pub struct Row {
    pub id: String,
    pub subject: String,
    pub status: Status,
    /// Facade module (`cohesive` for `akantu::cohesive`).
    pub module: Option<String>,
    /// Crate directory name (`akantu-cohesive`).
    pub krate: Option<String>,
    /// Facade feature (`cohesive`) or `crate/feature`.
    pub feature: Option<String>,
    pub upstream: Option<String>,
    pub waiting_for: Option<String>,
    /// Page relative to `docs/manual/src`, possibly with an anchor.
    pub page: Option<String>,
    pub pinned_by: Option<String>,
    pub names: Vec<String>,
}

impl Row {
    /// Page without its anchor.
    pub fn page_file(&self) -> Option<&str> {
        self.page
            .as_deref()
            .map(|p| p.split('#').next().unwrap_or(p))
    }

    /// Label in short lists (the README): the first name, else the subject.
    pub fn label(&self) -> &str {
        self.names
            .first()
            .map_or(self.subject.as_str(), |s| s.as_str())
    }

    /// Feature name as written in the manual (`ntn-contact`, `mumps`).
    pub fn feature_short(&self) -> Option<&str> {
        self.feature
            .as_deref()
            .map(|f| f.split_once('/').map_or(f, |(_, x)| x))
    }

    /// Crate of a `crate/feature` (`None` for a facade feature).
    pub fn feature_crate(&self) -> Option<&str> {
        self.feature
            .as_deref()
            .and_then(|f| f.split_once('/').map(|(c, _)| c))
    }
}

fn cell(s: &str) -> Option<String> {
    let s = s.trim();
    if s.is_empty() || s == "—" {
        None
    } else {
        Some(s.to_string())
    }
}

fn unquote(s: &str) -> String {
    s.trim().trim_matches('`').to_string()
}

/// Parse the table of `availability.md`. Errors are reported, not panicked
/// on, so that negative cases can feed broken tables.
pub fn parse_table(text: &str) -> Result<Vec<Row>, Vec<String>> {
    let mut rows = Vec::new();
    let mut errors = Vec::new();
    let mut header_seen = false;
    for (i, line) in text.lines().enumerate() {
        let t = line.trim();
        if !t.starts_with('|') {
            continue;
        }
        let cells: Vec<&str> = t.trim_matches('|').split('|').collect();
        if !header_seen {
            header_seen = true;
            let want = [
                "Id",
                "Subject",
                "Status",
                "Module",
                "Crate",
                "Feature",
                "Upstream",
                "Waiting for",
                "Page",
                "Pinned by",
                "Names",
            ];
            let got: Vec<&str> = cells.iter().map(|c| c.trim()).collect();
            if got != want {
                errors.push(format!("availability.md: header {got:?}, want {want:?}"));
                return Err(errors);
            }
            continue;
        }
        if cells.iter().all(|c| c.trim().chars().all(|ch| ch == '-')) {
            continue;
        }
        if cells.len() != 11 {
            errors.push(format!(
                "availability.md:{}: {} cells, want 11",
                i + 1,
                cells.len()
            ));
            continue;
        }
        let Some(status) = Status::parse(cells[2].trim()) else {
            errors.push(format!(
                "availability.md:{}: unknown status {}",
                i + 1,
                cells[2].trim()
            ));
            continue;
        };
        let module = cell(cells[3]).map(|m| {
            let m = unquote(&m);
            m.strip_prefix("akantu::").unwrap_or(&m).to_string()
        });
        rows.push(Row {
            id: cells[0].trim().to_string(),
            subject: cells[1].trim().to_string(),
            status,
            module,
            krate: cell(cells[4]).map(|c| unquote(&c)),
            feature: cell(cells[5]).map(|c| unquote(&c)),
            upstream: cell(cells[6]),
            waiting_for: cell(cells[7]),
            page: cell(cells[8]),
            pinned_by: cell(cells[9]),
            names: cell(cells[10])
                .map(|n| n.split(", ").map(|s| s.trim().to_string()).collect())
                .unwrap_or_default(),
        });
    }
    if !header_seen {
        errors.push("availability.md: no table".into());
    }
    if errors.is_empty() {
        Ok(rows)
    } else {
        Err(errors)
    }
}

/* ---------------------------------------------------------------------- */
/* The workspace                                                           */
/* ---------------------------------------------------------------------- */

/// Facts the table is checked against.
#[derive(Clone, Debug, Default)]
pub struct Workspace {
    /// Facade module → (crate directory, cfg feature or `None`).
    pub modules: BTreeMap<String, (String, Option<String>)>,
    /// Crate directory → placeholder?
    pub placeholder: BTreeMap<String, bool>,
    /// Crate directories under `crates/`.
    pub crates: BTreeSet<String>,
    /// Crate directory → (features, default features); the facade is
    /// `akantu`.
    pub features: BTreeMap<String, (BTreeSet<String>, BTreeSet<String>)>,
}

pub fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn read(p: &Path) -> String {
    std::fs::read_to_string(p).unwrap_or_else(|e| panic!("{}: {e}", p.display()))
}

/// `[features]` of a Cargo manifest: names and the `default` list.
fn manifest_features(text: &str) -> (BTreeSet<String>, BTreeSet<String>) {
    let mut names = BTreeSet::new();
    let mut default = BTreeSet::new();
    let mut in_features = false;
    for line in text.lines() {
        let t = line.trim();
        if t.starts_with('[') {
            in_features = t == "[features]";
            continue;
        }
        if !in_features || t.starts_with('#') || t.is_empty() {
            continue;
        }
        let Some((name, value)) = t.split_once('=') else {
            continue;
        };
        let name = name.trim().to_string();
        if name == "default" {
            for v in value.split(',') {
                let v = v.trim().trim_matches(|c| c == '[' || c == ']' || c == ' ');
                let v = v.trim_matches('"');
                if !v.is_empty() {
                    default.insert(v.to_string());
                }
            }
        } else {
            names.insert(name);
        }
    }
    (names, default)
}

/// A placeholder crate: `src/` holds only `lib.rs`, which declares no
/// module, re-exports nothing, has no `impl`, and whose public functions
/// (if any) are `todo!` (the shape of the reserved crates of the workspace,
/// e.g. `crates/akantu-diffusion/src/lib.rs`: one unit struct).
pub fn is_placeholder(crate_dir: &Path) -> bool {
    let src = crate_dir.join("src");
    let files: Vec<PathBuf> = std::fs::read_dir(&src)
        .unwrap_or_else(|e| panic!("{}: {e}", src.display()))
        .map(|e| e.unwrap().path())
        .collect();
    if files.len() != 1 || !files[0].ends_with("lib.rs") {
        return false;
    }
    let code: Vec<String> = read(&files[0])
        .lines()
        .map(str::trim)
        .filter(|l| !l.starts_with("//") && !l.is_empty())
        .map(String::from)
        .collect();
    let has = |p: &str| {
        code.iter()
            .any(|l| l.starts_with(p) || l.contains(&format!(" {p}")))
    };
    if has("mod ") || has("pub mod ") || has("pub use ") || has("impl") {
        return false;
    }
    let text = code.join("\n");
    !text.contains("pub fn ") || text.contains("todo!(")
}

impl Workspace {
    /// Read the facts from disk.
    pub fn load() -> Self {
        let root = repo_root();
        let mut ws = Workspace::default();
        for e in std::fs::read_dir(root.join("crates")).unwrap() {
            let p = e.unwrap().path();
            if p.join("Cargo.toml").exists() {
                let name = p.file_name().unwrap().to_string_lossy().to_string();
                ws.features.insert(
                    name.clone(),
                    manifest_features(&read(&p.join("Cargo.toml"))),
                );
                ws.crates.insert(name);
            }
        }
        // facade modules: `#[cfg(feature = "x")]` + `pub use akantu_y as z;`
        let lib = read(&root.join("crates/akantu/src/lib.rs"));
        let mut cfg: Option<String> = None;
        for line in lib.lines() {
            let t = line.trim();
            if let Some(rest) = t.strip_prefix("#[cfg(feature = \"") {
                cfg = rest.split('"').next().map(String::from);
                continue;
            }
            if let Some(rest) = t.strip_prefix("pub use akantu_")
                && let Some((krate, module)) = rest.trim_end_matches(';').split_once(" as ")
            {
                let dir = format!("akantu-{}", krate.trim().replace('_', "-"));
                ws.placeholder
                    .insert(dir.clone(), is_placeholder(&root.join("crates").join(&dir)));
                ws.modules
                    .insert(module.trim().to_string(), (dir, cfg.take()));
                continue;
            }
            if !t.is_empty() && !t.starts_with("//") {
                cfg = None;
            }
        }
        ws
    }
}

/* ---------------------------------------------------------------------- */
/* Documents, units, sentences                                             */
/* ---------------------------------------------------------------------- */

/// A Markdown document: path relative to `docs/manual/src` (manual pages)
/// or `README.md` (the repository front page, `readme = true`).
#[derive(Clone, Debug)]
pub struct Doc {
    pub path: String,
    pub readme: bool,
    pub text: String,
}

/// The manual's pages and the README, from disk.
pub fn load_docs() -> Vec<Doc> {
    fn walk(dir: &Path, base: &Path, out: &mut Vec<Doc>) {
        let mut entries: Vec<_> = std::fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().path())
            .collect();
        entries.sort();
        for p in entries {
            if p.is_dir() {
                walk(&p, base, out);
            } else if p.extension().is_some_and(|e| e == "md") {
                let rel = p
                    .strip_prefix(base)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/");
                out.push(Doc {
                    path: rel,
                    readme: false,
                    text: read(&p),
                });
            }
        }
    }
    let root = repo_root();
    let src = root.join("docs/manual/src");
    let mut out = Vec::new();
    walk(&src, &src, &mut out);
    out.push(Doc {
        path: "README.md".into(),
        readme: true,
        text: read(&root.join("README.md")),
    });
    out
}

/// A unit of text checked on its own: a sentence of a paragraph or of a
/// list item, a heading, or a whole table row.
#[derive(Clone, Debug)]
pub struct Unit {
    pub line: usize,
    pub text: String,
    /// Table row the unit belongs to (its cells are units of one row).
    pub row: Option<usize>,
}

/// Blank out HTML comments (keeping the line structure).
fn strip_comments(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(b) = rest.find("<!--") {
        out.push_str(&rest[..b]);
        let after = &rest[b..];
        let e = after.find("-->").map_or(after.len(), |e| e + 3);
        out.extend(
            after[..e]
                .chars()
                .map(|c| if c == '\n' { '\n' } else { ' ' }),
        );
        rest = &after[e..];
    }
    out.push_str(rest);
    out
}

const ABBREVIATIONS: [&str; 9] = [
    "e.g", "i.e", "etc", "vs", "cf", "resp", "approx", "incl", "no",
];

/// Split a paragraph into sentences: at `.`, `!` or `?` followed by a
/// space and a character that is not a lower-case letter, outside code
/// spans and link targets, not after a common abbreviation.
pub fn sentences(par: &str) -> Vec<String> {
    let chars: Vec<char> = par.chars().collect();
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut in_code = false;
    let mut in_target = 0usize;
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        cur.push(c);
        if c == '`' {
            in_code = !in_code;
        } else if !in_code {
            if c == '(' && i > 0 && chars[i - 1] == ']' {
                in_target = 1;
            } else if in_target > 0 && c == '(' {
                in_target += 1;
            } else if in_target > 0 && c == ')' {
                in_target -= 1;
            } else if in_target == 0
                && matches!(c, '.' | '!' | '?')
                && i + 1 < chars.len()
                && chars[i + 1].is_whitespace()
            {
                let next = chars[i + 1..].iter().find(|c| !c.is_whitespace());
                let lower_next = next.is_some_and(|c| c.is_lowercase());
                let word: String = cur
                    .trim_end_matches(['.', '!', '?'])
                    .rsplit(|c: char| c.is_whitespace() || c == '(')
                    .next()
                    .unwrap_or("")
                    .to_ascii_lowercase();
                if !lower_next && !ABBREVIATIONS.contains(&word.as_str()) {
                    out.push(std::mem::take(&mut cur));
                }
            }
        }
        i += 1;
    }
    if !cur.trim().is_empty() {
        out.push(cur);
    }
    out.into_iter()
        .map(|s| s.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|s| !s.is_empty())
        .collect()
}

/// Blank out the generated parity blocks: the README's parity-statistics
/// block (generated by `scripts/parity/stats.py` and checked by
/// `readme_status_matches_parity_stats`) and the manual's row-status tables
/// (`parity-cited …`, `parity-area …`, generated from
/// `docs/FEATURE_PARITY.md` by `tests/parity_rows`); their status names,
/// e.g. *Not started*, are not statements.
fn strip_parity_stats(text: &str) -> String {
    let b = "<!-- BEGIN GENERATED: parity-";
    let e = "<!-- END GENERATED: parity-";
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(s) = rest.find(b) {
        let Some(len) = rest[s..].find(e) else { break };
        let blank: String = rest[s..s + len]
            .chars()
            .map(|c| if c == '\n' { '\n' } else { ' ' })
            .collect();
        out.push_str(&rest[..s]);
        out.push_str(&blank);
        rest = &rest[s + len..];
    }
    out.push_str(rest);
    out
}

/// The units of a document: headings, the sentences of every cell of a
/// table row (grouped by row), and the sentences of paragraphs and list
/// items; code fences, HTML comments and `{{#include}}` lines are skipped.
pub fn units(text: &str) -> Vec<Unit> {
    let text = strip_comments(&strip_parity_stats(text));
    let mut out = Vec::new();
    let mut block = String::new();
    let mut block_line = 0;
    let mut in_fence = false;
    let flush = |block: &mut String, line: usize, out: &mut Vec<Unit>| {
        if !block.trim().is_empty() {
            for s in sentences(block) {
                out.push(Unit {
                    line,
                    text: s,
                    row: None,
                });
            }
        }
        block.clear();
    };
    for (i, line) in text.lines().enumerate() {
        let t = line.trim();
        if t.starts_with("```") {
            flush(&mut block, block_line, &mut out);
            in_fence = !in_fence;
            continue;
        }
        if in_fence {
            continue;
        }
        if t.is_empty() || t.starts_with("{{#") {
            flush(&mut block, block_line, &mut out);
            continue;
        }
        if t.starts_with('#') {
            flush(&mut block, block_line, &mut out);
            out.push(Unit {
                line: i + 1,
                text: t.trim_start_matches('#').trim().to_string(),
                row: None,
            });
            continue;
        }
        if t.starts_with('|') {
            flush(&mut block, block_line, &mut out);
            let inner = t.trim_matches('|');
            if !inner
                .split('|')
                .all(|c| c.trim().chars().all(|ch| ch == '-' || ch == ':'))
            {
                for c in inner.split('|') {
                    for s in sentences(c.trim()) {
                        out.push(Unit {
                            line: i + 1,
                            text: s,
                            row: Some(i),
                        });
                    }
                }
            }
            continue;
        }
        let item = t.starts_with("* ")
            || t.starts_with("- ")
            || t.split_once(". ")
                .is_some_and(|(n, _)| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()));
        if item {
            flush(&mut block, block_line, &mut out);
        }
        if block.is_empty() {
            block_line = i + 1;
        }
        block.push_str(t);
        block.push(' ');
    }
    flush(&mut block, block_line, &mut out);
    out
}

/* ---------------------------------------------------------------------- */
/* Statements                                                              */
/* ---------------------------------------------------------------------- */

fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '-'
}

/// Byte ranges of `needle` in `hay` (both already lower-cased if the
/// match is case-insensitive) with word boundaries on both sides.
fn word_matches(hay: &str, needle: &str) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    if needle.is_empty() {
        return out;
    }
    let mut from = 0;
    while let Some(p) = hay[from..].find(needle) {
        let s = from + p;
        let e = s + needle.len();
        let before = hay[..s].chars().next_back();
        let after = hay[e..].chars().next();
        let first = needle.chars().next().unwrap();
        let last = needle.chars().next_back().unwrap();
        let ok_before = !is_word(first) || before.is_none_or(|c| !is_word(c));
        let ok_after = !is_word(last) || after.is_none_or(|c| !is_word(c));
        if ok_before && ok_after {
            out.push((s, e));
        }
        from = s + needle.len().max(1);
    }
    out
}

/// Is the unit a statement that something is **not** available?
pub fn is_negative(unit: &str) -> bool {
    let l = unit.to_ascii_lowercase();
    const PHRASES: [&str; 30] = [
        "unavailable",
        "placeholder",
        "placeholders",
        "stub",
        "stubs",
        "waiting list",
        "waits for",
        "wait for",
        "waiting for",
        "being ported",
        "to be ported",
        "being written",
        "to be written",
        "to be merged",
        "not ported",
        "never ported",
        "not been ported",
        "planned",
        "under development",
        "in development",
        "work in progress",
        "not started",
        "later release",
        "future release",
        "next release",
        "upcoming release",
        "coming release",
        "unmerged",
        "not merged",
        "not been merged",
    ];
    if PHRASES.iter().any(|p| !word_matches(&l, p).is_empty()) {
        return true;
    }
    if l.contains("`feat/") || l.contains("`review/") || l.contains("`integration/") {
        return true;
    }
    // a work-package name `WP-…`
    if unit.match_indices("WP-").any(|(i, _)| {
        unit[i + 3..].starts_with(|c: char| c.is_ascii_uppercase() || c.is_ascii_digit())
    }) {
        return true;
    }
    // "not … available / part of / in this release / on main / ported …"
    const AFTER_NOT: [&str; 13] = [
        "available",
        "ported",
        "merged",
        "released",
        "part of",
        "in this release",
        "in the release",
        "on main",
        "on `main`",
        "shipped",
        "included",
        "provided",
        "started",
    ];
    for (s, e) in word_matches(&l, "not") {
        let _ = s;
        let window: String = l[e..].chars().take(40).collect();
        if AFTER_NOT
            .iter()
            .any(|p| !word_matches(&window, p).is_empty())
        {
            return true;
        }
    }
    // a negation followed by "yet" ("not … yet", "no … yet", "never … yet")
    for neg in ["not", "no", "never", "nothing", "cannot"] {
        for (_, e) in word_matches(&l, neg) {
            let window: String = l[e..].chars().take(60).collect();
            if !word_matches(&window, "yet").is_empty() {
                return true;
            }
        }
    }
    if l.contains("n't") && !word_matches(&l, "yet").is_empty() {
        return true;
    }
    // "absent / missing / excluded / omitted … release"
    for w in ["absent", "missing", "excluded", "omitted"] {
        for (_, e) in word_matches(&l, w) {
            let window: String = l[e..].chars().take(60).collect();
            if !word_matches(&window, "release").is_empty() {
                return true;
            }
        }
    }
    // "on a branch", "on other branches" (not "the `main` branch")
    for (s, _) in word_matches(&l, "branch")
        .into_iter()
        .chain(word_matches(&l, "branches"))
    {
        let before: Vec<&str> = l[..s].split_whitespace().rev().take(3).collect();
        if !before.iter().any(|w| w.trim_matches('`') == "main") {
            return true;
        }
    }
    false
}

/// Is the unit a statement that something **is** available?
pub fn is_positive(unit: &str) -> bool {
    let l = unit.to_ascii_lowercase();
    [
        "available",
        "part of this release",
        "in this release",
        "part of the release",
        "on `main`",
        "on main",
    ]
    .iter()
    .any(|p| !word_matches(&l, p).is_empty())
}

/// A negative statement about the build (an opt-in subject may be named).
fn is_build_conditional(unit: &str) -> bool {
    let l = unit.to_ascii_lowercase();
    let build = [
        "in this build",
        "without the feature",
        "without the cargo feature",
        "without it",
        "unless the feature",
        "needs a build with",
        "need a build with",
        "not enabled",
        "not compiled",
        "off by default",
        "disabled",
    ]
    .iter()
    .any(|p| l.contains(p));
    let release = [
        "release",
        "placeholder",
        "ported",
        "branch",
        "waiting",
        "planned",
        "stub",
        "`feat/",
    ]
    .iter()
    .any(|p| l.contains(p))
        || unit.contains("WP-");
    build && !release
}

/// Resolve a link target of `doc` to a page relative to `docs/manual/src`
/// (with its anchor), or `None` for an external or non-manual target.
fn resolve_link(doc: &Doc, target: &str) -> Option<String> {
    if target.starts_with("http://") || target.starts_with("https://") {
        return None;
    }
    let (path, anchor) = match target.split_once('#') {
        Some((p, a)) => (p, Some(a)),
        None => (target, None),
    };
    let base: Vec<&str> = if doc.readme {
        vec!["..", "..", ".."] // README is at the repository root
    } else {
        let mut d: Vec<&str> = doc.path.split('/').collect();
        d.pop();
        d
    };
    let mut parts: Vec<&str> = base;
    if path.is_empty() {
        parts = doc.path.split('/').collect();
    } else {
        for p in path.split('/') {
            match p {
                "." => {}
                ".." => {
                    if parts.last().is_some_and(|l| *l != "..") {
                        parts.pop();
                    } else {
                        parts.push("..");
                    }
                }
                x => parts.push(x),
            }
        }
    }
    // README: `docs/manual/src/x.md` resolves through `../../../docs/manual/src`
    let joined = parts.join("/");
    let joined = joined
        .strip_prefix("../../../docs/manual/src/")
        .map(String::from)
        .unwrap_or(joined);
    if joined.starts_with("..") || !joined.ends_with(".md") {
        return None;
    }
    Some(match anchor {
        Some(a) => format!("{joined}#{a}"),
        None => joined,
    })
}

/// Inline link targets `[text](target)` of a unit, with the byte range of
/// the whole link.
fn unit_links(unit: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(p) = unit[from..].find("](") {
        let s = from + p + 2;
        match unit[s..].find(')') {
            Some(e) => {
                out.push(unit[s..s + e].to_string());
                from = s + e;
            }
            None => break,
        }
    }
    out
}

/// The rows a unit names (indices into `rows`).
pub fn named_rows(rows: &[Row], doc: &Doc, unit: &str) -> BTreeSet<usize> {
    // plain names never match inside code spans (identifiers, literals)
    let mut in_code = false;
    let lower: String = unit
        .chars()
        .map(|c| {
            if c == '`' {
                in_code = !in_code;
                ' '
            } else if in_code {
                if c.is_ascii() { ' ' } else { c }
            } else {
                c.to_ascii_lowercase()
            }
        })
        .collect();
    // (start, end, row) candidates of text and code aliases
    let mut cands: Vec<(usize, usize, usize)> = Vec::new();
    for (r, row) in rows.iter().enumerate() {
        if let Some(m) = &row.module {
            let path = format!("akantu::{m}");
            for (s, e) in word_matches(unit, &path) {
                // `akantu::x` and its items `akantu::x::…`, not `akantu::x_y`
                cands.push((s, e, r));
            }
        }
        if let Some(c) = &row.krate {
            for (s, e) in word_matches(unit, c) {
                // not inside a path (`crates/akantu-x/tests/…`)
                if !unit[..s].ends_with('/') {
                    cands.push((s, e, r));
                }
            }
        }
        if let Some(f) = row.feature_short() {
            for (s, e) in word_matches(unit, &format!("`{f}`")) {
                cands.push((s, e, r));
            }
        }
        if let Some(f) = &row.feature
            && f.contains('/')
        {
            for (s, e) in word_matches(unit, &format!("`{f}`")) {
                cands.push((s, e, r));
            }
        }
        for n in &row.names {
            if n.starts_with('`') {
                for (s, e) in word_matches(unit, n) {
                    cands.push((s, e, r));
                }
                // `akantu::runtime::initialize` also matches inside a longer path
            } else {
                for (s, e) in word_matches(&lower, &n.to_ascii_lowercase()) {
                    cands.push((s, e, r));
                }
            }
        }
    }
    // enumerations sharing one head noun ("the contact, diffusion and
    // structural models"): each item is matched as "<item> model"
    for (word_start, word_end, item) in enumeration_items(&lower) {
        for (r, row) in rows.iter().enumerate() {
            let hit = row.names.iter().any(|n| {
                let n = n.to_ascii_lowercase();
                n == format!("{item} model") || n == format!("{item} models")
            });
            if hit {
                cands.push((word_start, word_end, r));
            }
        }
    }
    // longest match wins; overlapping shorter matches are dropped
    cands.sort_by(|a, b| (b.1 - b.0).cmp(&(a.1 - a.0)).then(a.0.cmp(&b.0)));
    let mut taken: Vec<(usize, usize)> = Vec::new();
    let mut named = BTreeSet::new();
    for (s, e, r) in cands {
        if taken.iter().all(|&(ts, te)| e <= ts || s >= te) {
            taken.push((s, e));
            named.insert(r);
        }
    }
    // page links: the row whose page (with anchor) matches exactly, else
    // the row of the page without an anchor
    for target in unit_links(unit) {
        if target.starts_with('#') {
            continue; // a section of the same page, not a subject
        }
        let Some(resolved) = resolve_link(doc, &target) else {
            continue;
        };
        let file = resolved.split('#').next().unwrap_or(&resolved).to_string();
        if let Some(r) = rows
            .iter()
            .position(|row| row.page.as_deref() == Some(resolved.as_str()))
        {
            named.insert(r);
        } else if let Some(r) = rows
            .iter()
            .position(|row| row.page.as_deref() == Some(file.as_str()))
        {
            named.insert(r);
        }
    }
    named
}

/// The items of enumerations with a plural head noun `models`
/// ("a, b and c models", "a and b models"): (start, end, item) of each
/// item before the last one, whose own match is the plain "c models"
/// name. `lower` is the lower-cased unit with code spans blanked.
fn enumeration_items(lower: &str) -> Vec<(usize, usize, String)> {
    let mut out = Vec::new();
    for (head, _) in word_matches(lower, "models") {
        // walk back over "w1, w2, … and wn " before the head noun
        let mut end = head;
        let mut items: Vec<(usize, usize)> = Vec::new();
        loop {
            let before = &lower[..end];
            let trimmed = before.trim_end_matches(' ');
            let word_start = trimmed
                .char_indices()
                .rev()
                .take_while(|(_, c)| is_word(*c))
                .last()
                .map(|(i, _)| i);
            let Some(ws) = word_start else { break };
            items.push((ws, trimmed.len()));
            let rest = &lower[..ws];
            if let Some(r) = rest.strip_suffix(", and ") {
                end = r.len();
            } else if let Some(r) = rest.strip_suffix(" and ") {
                end = r.len();
            } else if let Some(r) = rest.strip_suffix(", ") {
                end = r.len();
            } else {
                break;
            }
        }
        if items.len() > 1 {
            for &(s, e) in &items {
                out.push((s, e, lower[s..e].to_string()));
            }
        }
    }
    out
}

/// Work packages (`WP-…`) and branches (`` `feat/…` ``) a unit names.
fn work_refs(unit: &str) -> Vec<String> {
    let mut out = Vec::new();
    for (i, _) in unit.match_indices("WP-") {
        let tail: String = unit[i..]
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '-')
            .collect();
        let tail = tail.trim_end_matches('-').to_string();
        if tail.len() > 3 {
            out.push(tail);
        }
    }
    for prefix in ["`feat/", "`review/", "`integration/"] {
        for (i, _) in unit.match_indices(prefix) {
            if let Some(e) = unit[i + 1..].find('`') {
                out.push(unit[i..i + 2 + e].to_string());
            }
        }
    }
    out
}

/// Does the cell mention the work reference as a whole token?
fn mentions(cell: &str, wref: &str) -> bool {
    if wref.starts_with('`') {
        return cell.contains(wref);
    }
    cell.match_indices(wref).any(|(i, _)| {
        cell[i + wref.len()..]
            .chars()
            .next()
            .is_none_or(|c| !(c.is_ascii_alphanumeric() || c == '-'))
    })
}

/// Check every unit of every document against the table.
pub fn check_statements(rows: &[Row], docs: &[Doc]) -> Vec<String> {
    let mut v = Vec::new();
    for doc in docs {
        if doc.path == "SUMMARY.md" {
            continue; // checked structurally
        }
        // rows whose page is this document
        let hosted: Vec<usize> = rows
            .iter()
            .enumerate()
            .filter(|(_, r)| !doc.readme && r.page_file() == Some(doc.path.as_str()))
            .map(|(i, _)| i)
            .collect();
        let all = units(&doc.text);
        let named_all: Vec<BTreeSet<usize>> =
            all.iter().map(|u| named_rows(rows, doc, &u.text)).collect();
        for (k, unit) in all.iter().enumerate() {
            let named = named_all[k].clone();
            let where_ = format!("{}:{}", doc.path, unit.line);
            if is_negative(&unit.text) {
                // a cell that names nothing speaks of what its row names,
                // else of the page's subjects
                let row_context: BTreeSet<usize> = match unit.row {
                    Some(r) => all
                        .iter()
                        .zip(&named_all)
                        .filter(|(u, _)| u.row == Some(r))
                        .flat_map(|(_, n)| n.iter().copied())
                        .collect(),
                    None => BTreeSet::new(),
                };
                let subjects: Vec<usize> = if !named.is_empty() {
                    named.iter().copied().collect()
                } else if !row_context.is_empty() {
                    row_context.into_iter().collect()
                } else {
                    hosted.clone()
                };
                if subjects.is_empty() {
                    v.push(format!(
                        "{where_}: a 'not available' statement names no subject of the availability table: {}",
                        unit.text
                    ));
                    continue;
                }
                let build = is_build_conditional(&unit.text);
                for &r in &subjects {
                    let row = &rows[r];
                    let ok = row.status.may_be_negated() || (row.status == Status::OptIn && build);
                    if !ok {
                        v.push(format!(
                            "{where_}: a 'not available' statement {} '{}' ({:?} in the availability table): {}",
                            if named.is_empty() { "is about" } else { "names" },
                            row.id,
                            row.status,
                            unit.text
                        ));
                    }
                }
                for wref in work_refs(&unit.text) {
                    let known = subjects.iter().any(|&r| {
                        rows[r]
                            .waiting_for
                            .as_deref()
                            .is_some_and(|w| mentions(w, &wref))
                    });
                    if !known {
                        v.push(format!(
                            "{where_}: {wref} is not what the availability table says its subjects wait for: {}",
                            unit.text
                        ));
                    }
                }
            } else if is_positive(&unit.text) {
                for &r in &named {
                    let row = &rows[r];
                    if matches!(row.status, Status::NotYetAvailable | Status::NotPorted) {
                        v.push(format!(
                            "{where_}: an 'available' statement names '{}' ({:?} in the availability table): {}",
                            row.id, row.status, unit.text
                        ));
                    }
                }
            }
        }
    }
    v
}

/* ---------------------------------------------------------------------- */
/* Table and declarations against the workspace                            */
/* ---------------------------------------------------------------------- */

/// Page declarations `<!-- availability: available akantu::x -->`:
/// (page, available?, module).
pub fn declarations(docs: &[Doc]) -> (Vec<(String, bool, String)>, Vec<String>) {
    let mut out = Vec::new();
    let mut errors = Vec::new();
    for doc in docs.iter().filter(|d| !d.readme) {
        for line in doc.text.lines() {
            let Some(rest) = line.trim().strip_prefix("<!-- availability: ") else {
                continue;
            };
            let rest = rest.trim_end_matches("-->").trim();
            let Some((state, module)) = rest.split_once(' ') else {
                errors.push(format!("{}: bad availability line {line}", doc.path));
                continue;
            };
            let available = match state {
                "available" => true,
                "not-yet-available" => false,
                other => {
                    errors.push(format!("{}: unknown availability {other}", doc.path));
                    continue;
                }
            };
            match module.strip_prefix("akantu::") {
                Some(m) => out.push((doc.path.clone(), available, m.to_string())),
                None => errors.push(format!("{}: {module} is not akantu::…", doc.path)),
            }
        }
    }
    (out, errors)
}

/// The test functions named by *Pinned by* exist.
fn pinned_test_exists(spec: &str) -> Result<(), String> {
    let (file, test) = spec
        .split_once("::")
        .ok_or_else(|| format!("pinned test {spec}: not file.rs::test"))?;
    let root = repo_root();
    let path = if file.contains('/') {
        root.join(file)
    } else {
        root.join("crates/akantu/tests").join(file)
    };
    let text = std::fs::read_to_string(&path).map_err(|e| format!("pinned test {spec}: {e}"))?;
    if text.contains(&format!("fn {test}(")) {
        Ok(())
    } else {
        Err(format!(
            "pinned test {spec}: no fn {test} in {}",
            path.display()
        ))
    }
}

/// Every check of the table against the workspace and the manual's
/// structure (declarations, SUMMARY, status blocks).
pub fn check_table(rows: &[Row], ws: &Workspace, docs: &[Doc]) -> Vec<String> {
    let mut v = Vec::new();
    let pages: BTreeSet<&str> = docs
        .iter()
        .filter(|d| !d.readme)
        .map(|d| d.path.as_str())
        .collect();
    let mut ids = BTreeSet::new();
    for row in rows {
        let r = &row.id;
        if !ids.insert(r.clone()) {
            v.push(format!("availability table: duplicate id {r}"));
        }
        match (row.status, &row.waiting_for) {
            (Status::NotYetAvailable, None) => {
                v.push(format!(
                    "availability table: {r} is not yet available but says nothing in 'Waiting for'"
                ));
            }
            (Status::NotYetAvailable, Some(_)) => {}
            (_, Some(w)) => v.push(format!(
                "availability table: {r} is {:?} but waits for {w}",
                row.status
            )),
            (_, None) => {}
        }
        if let Some(p) = row.page_file()
            && !pages.contains(p)
        {
            v.push(format!("availability table: {r}: page {p} does not exist"));
        }
        if let Some(c) = &row.krate
            && !ws.crates.contains(c)
        {
            v.push(format!("availability table: {r}: crate {c} does not exist"));
        }
        if let Some(m) = &row.module {
            match ws.modules.get(m) {
                None => v.push(format!(
                    "availability table: {r}: akantu::{m} is not a facade module"
                )),
                Some((dir, cfg)) => {
                    if row.krate.as_deref() != Some(dir.as_str()) {
                        v.push(format!(
                            "availability table: {r}: akantu::{m} is the crate {dir}"
                        ));
                    }
                    if row.feature.as_deref() != cfg.as_deref() {
                        v.push(format!(
                            "availability table: {r}: akantu::{m} is behind the feature {cfg:?}, the table says {:?}",
                            row.feature
                        ));
                    }
                    let placeholder = ws.placeholder.get(dir).copied().unwrap_or(false);
                    let default = cfg.as_ref().is_none_or(|f| {
                        ws.features
                            .get("akantu")
                            .is_some_and(|(_, d)| d.contains(f))
                    });
                    let want = if placeholder {
                        Status::NotYetAvailable
                    } else if default {
                        Status::Available
                    } else {
                        Status::OptIn
                    };
                    if row.status != want {
                        v.push(format!(
                            "availability table: {r}: akantu::{m} is {want:?} in the workspace (crate {dir} {}, feature {cfg:?} {}), the table says {:?}",
                            if placeholder { "is a placeholder" } else { "is implemented" },
                            if default { "on by default" } else { "off by default" },
                            row.status
                        ));
                    }
                }
            }
        }
        if let Some(f) = &row.feature {
            let (krate, name) = f.split_once('/').unwrap_or(("akantu", f));
            match ws.features.get(krate) {
                Some((names, default)) => {
                    if !names.contains(name) {
                        v.push(format!(
                            "availability table: {r}: {krate} has no feature {name}"
                        ));
                    } else if row.module.is_none() && krate != "akantu" {
                        // a crate feature: opt-in iff not a default of that crate
                        let opt_in = !default.contains(name);
                        if opt_in != (row.status == Status::OptIn) && row.status.is_in_release() {
                            v.push(format!(
                                "availability table: {r}: feature {f} is {} by default, the table says {:?}",
                                if opt_in { "off" } else { "on" },
                                row.status
                            ));
                        }
                    }
                }
                None => v.push(format!("availability table: {r}: no crate {krate}")),
            }
        }
        if let Some(p) = &row.pinned_by
            && let Err(e) = pinned_test_exists(p)
        {
            v.push(format!("availability table: {r}: {e}"));
        }
    }
    // every facade module has a row, every facade feature appears
    for m in ws.modules.keys() {
        let n = rows
            .iter()
            .filter(|r| r.module.as_deref() == Some(m))
            .count();
        if n != 1 {
            v.push(format!(
                "availability table: akantu::{m} has {n} rows, want 1"
            ));
        }
    }
    if let Some((features, _)) = ws.features.get("akantu") {
        for f in features {
            if !rows
                .iter()
                .any(|r| r.feature.as_deref() == Some(f.as_str()))
            {
                v.push(format!(
                    "availability table: the facade feature {f} is in no row"
                ));
            }
        }
    }
    // declarations agree with the crates and the table; every module is declared
    let (decls, errors) = declarations(docs);
    v.extend(errors);
    for (page, available, module) in &decls {
        match rows
            .iter()
            .find(|r| r.module.as_deref() == Some(module.as_str()))
        {
            None => v.push(format!(
                "{page}: declares akantu::{module}, which has no row in the availability table"
            )),
            Some(row) => {
                if row.status.is_in_release() != *available {
                    v.push(format!(
                        "{page}: declares akantu::{module} {} but the availability table says {:?}",
                        if *available {
                            "available"
                        } else {
                            "not yet available"
                        },
                        row.status
                    ));
                }
            }
        }
        if let Some((dir, _)) = ws.modules.get(module) {
            let p = ws.placeholder.get(dir).copied().unwrap_or(false);
            if *available == p {
                v.push(format!(
                    "{page}: declares akantu::{module} {} but its crate is {}",
                    if *available {
                        "available"
                    } else {
                        "not yet available"
                    },
                    if p { "a placeholder" } else { "implemented" }
                ));
            }
        }
    }
    for (m, (dir, _)) in &ws.modules {
        let want = !ws.placeholder.get(dir).copied().unwrap_or(false);
        if !decls.iter().any(|(_, a, d)| d == m && *a == want) {
            v.push(format!(
                "no page declares akantu::{m} {} (add `<!-- availability: {} akantu::{m} -->`)",
                if want {
                    "available"
                } else {
                    "not yet available"
                },
                if want {
                    "available"
                } else {
                    "not-yet-available"
                }
            ));
        }
    }
    // status blocks on the model pages
    for row in rows {
        if let Some(p) = row.page_file()
            && p.starts_with("models/")
            && row.status != Status::WaitingList
        {
            let marker = format!("<!-- BEGIN GENERATED: availability-status {} -->", row.id);
            if let Some(doc) = docs.iter().find(|d| d.path == p)
                && !doc.text.contains(&marker)
            {
                v.push(format!("{p}: no status block `{marker}`"));
            }
        }
    }
    v.extend(check_summary(rows, docs));
    v
}

/// SUMMARY.md: the part *Models not yet available* lists exactly the
/// waiting list and the pages of the rows that are not yet available;
/// the pages of rows in the release are listed outside it.
fn check_summary(rows: &[Row], docs: &[Doc]) -> Vec<String> {
    let mut v = Vec::new();
    let Some(summary) = docs.iter().find(|d| d.path == "SUMMARY.md") else {
        return vec!["SUMMARY.md missing".into()];
    };
    let mut part = String::new();
    let mut inside: BTreeSet<String> = BTreeSet::new();
    let mut outside: BTreeSet<String> = BTreeSet::new();
    for line in summary.text.lines() {
        let t = line.trim();
        if let Some(h) = t.strip_prefix("# ") {
            part = h.trim().to_string();
            continue;
        }
        for target in unit_links(t) {
            let target = target.split('#').next().unwrap_or(&target).to_string();
            if part == "Models not yet available" {
                inside.insert(target);
            } else {
                outside.insert(target);
            }
        }
    }
    let mut want: BTreeSet<String> = BTreeSet::new();
    for row in rows {
        let Some(p) = row.page_file() else { continue };
        match row.status {
            Status::NotYetAvailable | Status::WaitingList => {
                want.insert(p.to_string());
            }
            Status::Available | Status::OptIn => {
                if inside.contains(p) {
                    v.push(format!(
                        "SUMMARY.md: {p} ({}, {:?}) is listed under 'Models not yet available'",
                        row.id, row.status
                    ));
                }
                if !outside.contains(p) {
                    v.push(format!(
                        "SUMMARY.md: {p} ({}) is not listed in the book",
                        row.id
                    ));
                }
            }
            Status::NotPorted => {}
        }
    }
    for p in want.difference(&inside) {
        v.push(format!(
            "SUMMARY.md: {p} is not listed under 'Models not yet available'"
        ));
    }
    for p in inside.difference(&want) {
        if !rows
            .iter()
            .any(|r| r.page_file() == Some(p.as_str()) && r.status.is_in_release())
        {
            v.push(format!(
                "SUMMARY.md: {p} is listed under 'Models not yet available' but no row that is not yet available has it as its page"
            ));
        }
    }
    v
}

/* ---------------------------------------------------------------------- */
/* Generated sections                                                      */
/* ---------------------------------------------------------------------- */

/// Relative link from document `from` to manual page `page`.
fn rel(from: &Doc, page: &str) -> String {
    if from.readme {
        return format!("docs/manual/src/{page}");
    }
    let dir = from.path.rsplit_once('/').map_or("", |(d, _)| d);
    if !dir.is_empty()
        && let Some(rest) = page.strip_prefix(&format!("{dir}/"))
    {
        return rest.to_string();
    }
    let depth = from.path.matches('/').count();
    format!("{}{page}", "../".repeat(depth))
}

/// First heading of a page.
fn title(docs: &[Doc], page: &str) -> String {
    let file = page.split('#').next().unwrap_or(page);
    docs.iter()
        .find(|d| d.path == file)
        .and_then(|d| d.text.lines().find_map(|l| l.strip_prefix("# ")))
        .map_or_else(|| file.to_string(), |t| t.trim().to_string())
}

fn capitalise(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().chain(c).collect(),
        None => String::new(),
    }
}

fn join_and(items: &[String]) -> String {
    match items.len() {
        0 => String::new(),
        1 => items[0].clone(),
        n => format!("{} and {}", items[..n - 1].join(", "), items[n - 1]),
    }
}

/// Content of the availability section `id` in document `doc`, or `None`
/// if `id` is not an availability section.
pub fn generate(id: &str, rows: &[Row], docs: &[Doc], doc: &Doc) -> Option<String> {
    let mut o = String::new();
    match id {
        "availability-modules" => {
            o.push_str("| Module | Crate | Feature | Content |\n|---|---|---|---|\n");
            for row in rows
                .iter()
                .filter(|r| r.module.is_some() && r.status.is_in_release())
            {
                let feature = match (&row.feature, row.status) {
                    (None, _) => "—".to_string(),
                    (Some(f), Status::OptIn) => format!("`{f}` (off by default)"),
                    (Some(f), _) => format!("`{f}`"),
                };
                let mut content = capitalise(&row.subject);
                if let Some(p) = &row.page {
                    content.push_str(&format!(" ([{}]({}))", title(docs, p), rel(doc, p)));
                }
                o.push_str(&format!(
                    "| `akantu::{}` | `{}` | {feature} | {content} |\n",
                    row.module.as_deref().unwrap_or(""),
                    row.krate.as_deref().unwrap_or("—"),
                ));
            }
        }
        "availability-placeholders" => {
            let features: Vec<String> = rows
                .iter()
                .filter(|r| r.module.is_some() && r.status == Status::NotYetAvailable)
                .filter_map(|r| r.feature.as_ref().map(|f| format!("`{f}`")))
                .collect();
            // no sentence at all when no facade feature is a placeholder
            if !features.is_empty() {
                o.push_str(&format!(
                    "The features {} exist, but the crates behind them are placeholders in this release ([Models not yet available]({})).\n",
                    join_and(&features),
                    rel(doc, "models/not-yet-available.md")
                ));
            }
        }
        "availability-waiting-list" => {
            o.push_str(
                "| Feature | Upstream manual | Facade module | Why it is not available |\n|---|---|---|---|\n",
            );
            for row in rows.iter().filter(|r| r.status == Status::NotYetAvailable) {
                let subject = match &row.page {
                    Some(p) => format!("[{}]({})", capitalise(&row.subject), rel(doc, p)),
                    None => capitalise(&row.subject),
                };
                let module = row.module.as_ref().map_or_else(
                    || "—".to_string(),
                    |m| format!("`akantu::{m}` (placeholder crate)"),
                );
                o.push_str(&format!(
                    "| {subject} | {} | {module} | {} |\n",
                    row.upstream.as_deref().unwrap_or("—"),
                    row.waiting_for.as_deref().unwrap_or("—")
                ));
            }
        }
        "availability-readme" => {
            let opt: Vec<String> = rows
                .iter()
                .filter(|r| r.status == Status::OptIn)
                .map(|r| format!("{} (`{}`)", r.label(), r.feature.as_deref().unwrap_or("—")))
                .collect();
            let nya: Vec<String> = rows
                .iter()
                .filter(|r| r.status == Status::NotYetAvailable)
                .map(|r| r.label().to_string())
                .collect();
            o.push_str(&format!(
                "Behind a Cargo feature that is off by default: {}.\n\n",
                join_and(&opt)
            ));
            o.push_str(&format!(
                "Not in this release, and none of them is being ported: {} (the reason for each: [models not yet available]({})).\n",
                join_and(&nya),
                rel(doc, "models/not-yet-available.md")
            ));
        }
        _ => {
            let rid = id.strip_prefix("availability-status ")?;
            let Some(row) = rows.iter().find(|r| r.id == rid) else {
                return Some(format!("unknown availability row {rid}\n"));
            };
            let module = row
                .module
                .as_ref()
                .map(|m| format!("facade module `akantu::{m}`"));
            let krate = row.krate.as_ref().map(|c| format!("crate `{c}`"));
            match row.status {
                Status::Available => {
                    let mut parts: Vec<String> = module.into_iter().chain(krate).collect();
                    if let Some(f) = &row.feature {
                        parts.push(format!("feature `{f}`, on by default"));
                    }
                    o.push_str(&format!(
                        "**Available in this release** ({}).\n",
                        parts.join(", ")
                    ));
                }
                Status::OptIn => {
                    let f = row.feature_short().unwrap_or("—");
                    let of = row
                        .feature_crate()
                        .map(|c| format!(" of `{c}`"))
                        .unwrap_or_default();
                    let parts: Vec<String> = module.into_iter().chain(krate).collect();
                    o.push_str(&format!(
                        "**Available behind the Cargo feature `{f}`{of}**, off by default ({}).\n",
                        parts.join(", ")
                    ));
                }
                Status::NotYetAvailable => {
                    o.push_str(&format!(
                        "**Not yet available in this release:** {} waits for {}",
                        row.subject,
                        row.waiting_for.as_deref().unwrap_or("—")
                    ));
                    if let Some(c) = &row.krate {
                        o.push_str(&format!("; the crate `{c}` is a placeholder"));
                    }
                    o.push_str(".\n");
                }
                Status::NotPorted | Status::WaitingList => {
                    o.push_str(&format!("**Not ported:** {}.\n", row.subject));
                }
            }
        }
    }
    Some(o)
}

/// The ids of the availability sections that must be used somewhere.
pub const REQUIRED_SECTIONS: [(&str, &str); 4] = [
    ("getting-started.md", "availability-modules"),
    ("getting-started.md", "availability-placeholders"),
    ("models/not-yet-available.md", "availability-waiting-list"),
    ("README.md", "availability-readme"),
];

const BEGIN: &str = "<!-- BEGIN GENERATED: ";
const END: &str = "<!-- END GENERATED: ";

/// Rewrite every availability section of `doc` (returns the new text and
/// the ids found).
pub fn regenerate(rows: &[Row], docs: &[Doc], doc: &Doc) -> (String, Vec<String>) {
    let text = &doc.text;
    let mut out = String::new();
    let mut rest = text.as_str();
    let mut found = Vec::new();
    while let Some(b) = rest.find(BEGIN) {
        let after = &rest[b + BEGIN.len()..];
        let Some(id_end) = after.find(" -->") else {
            break;
        };
        let id = &after[..id_end];
        let body_start = b + BEGIN.len() + id_end + " -->\n".len();
        let end_marker = format!("{END}{id} -->");
        let Some(e) = rest.get(body_start..).and_then(|s| s.find(&end_marker)) else {
            // a block of another generator whose end marker does not repeat
            // the id (the README's parity-stats block): skip its BEGIN line
            let skip = (b + BEGIN.len() + id_end + " -->".len()).min(rest.len());
            out.push_str(&rest[..skip]);
            rest = &rest[skip..];
            continue;
        };
        let e = e + body_start;
        out.push_str(&rest[..body_start]);
        match generate(id, rows, docs, doc) {
            Some(want) => {
                out.push_str(&want);
                found.push(id.to_string());
            }
            None => out.push_str(&rest[body_start..e]),
        }
        out.push_str(&end_marker);
        rest = &rest[e + end_marker.len()..];
    }
    out.push_str(rest);
    (out, found)
}

/// Stale or missing availability sections.
pub fn check_sections(rows: &[Row], docs: &[Doc]) -> Vec<String> {
    let mut v = Vec::new();
    for doc in docs {
        let (new, found) = regenerate(rows, docs, doc);
        if new != doc.text {
            v.push(format!(
                "{}: stale availability section (run with AKANTU_BLESS_MANUAL=1)",
                doc.path
            ));
        }
        for (page, id) in REQUIRED_SECTIONS {
            if doc.path == page && !found.iter().any(|f| f == id) {
                v.push(format!("{page}: no generated section {id}"));
            }
        }
    }
    v
}

/// Every check: table, workspace, structure, sections, statements.
pub fn check_all(table: &str, ws: &Workspace, docs: &[Doc]) -> Vec<String> {
    let rows = match parse_table(table) {
        Ok(r) => r,
        Err(e) => return e,
    };
    let mut v = check_table(&rows, ws, docs);
    v.extend(check_sections(&rows, docs));
    v.extend(check_statements(&rows, docs));
    v
}

pub fn table_path() -> PathBuf {
    repo_root().join("docs/manual/availability.md")
}
