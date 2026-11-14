// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). No C++
// counterpart: a guard on personal paths and broken links in the tree.

//! Two checks on the repository's text files:
//!
//! * no personal or scratch path and no sandbox id: `/Users/<name>` or
//!   `/home/<name>/` names a directory of one machine; `/private/tmp/`,
//!   `/var/folders/` and `/tmp/claude-…` are a session's scratch
//!   directories; `bx_` followed by eight letters or digits is the id of a
//!   rented build machine. Use a placeholder (`<akantu-src>`, `<run-dir>`) or
//!   a variable in its place. A path inside a longer one
//!   (`boost/spirit/home/x3/`) and the placeholder name `someone` are not
//!   hits; a line that must name a fixed home directory (a container user's,
//!   for example) carries the marker `public-tree-guard: allow`;
//! * every relative link of a Markdown file under `docs/` or at the root of
//!   the repository (`README.md`, `CONTRIBUTING.md`, ...) points to a file
//!   or directory of the tree (`[text](path)`, `![alt](path)` and
//!   `[label]: path`; links with a scheme, absolute paths and pure `#anchor`
//!   links are not checked; code blocks and code spans are skipped).
//!
//! Recorded outputs of runs (`fixtures/`, `oracle/results/` and the files of
//! `docs/benchmarks/` other than its `README.md` files) are checked too, with
//! two limits. Under `fixtures/` only the text metadata is read (manifests,
//! provenance notes, READMEs, scripts: `.json`, `.md`, `.txt`, `.py`, `.sh`);
//! the data files (`.npy`, meshes, `.dat` inputs, VTU, CSV) are generated
//! and checksummed and are not read. In recorded outputs the home directory
//! of the generic account `user` of the x86_64 machines that generated them
//! (`/home/user/…`) is allowed: generators wrote it into checksummed
//! manifests (`generator.command`) and provenance lines, and it names no
//! person.
//!
//! The test reports every hit. Files listed in
//! `tests/data/public_tree_guard_pending.txt` (a path, or a directory ending
//! in `/`) are exempt while their owners rewrite them; an entry without any
//! hit left fails the test, so the list only shrinks.
//!
//! `AKANTU_DOC_GUARD_ROOT=<dir>` checks the tree at `<dir>` instead of the
//! workspace.

mod repo_files;

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use akantu_validation::fixture::workspace_root;

const PENDING: &str = "tests/data/public_tree_guard_pending.txt";

/// Files never read by the guard: the data files of `fixtures/` (generated
/// and checksummed; only its text metadata is read) and the guard's own
/// sources (they spell the patterns out).
fn exempt(rel: &str) -> bool {
    (rel.starts_with("fixtures/") && !FIXTURE_TEXT.iter().any(|e| rel.ends_with(e)))
        || rel == "crates/akantu-validation/tests/public_tree_guard.rs"
        || rel == "crates/akantu-validation/tests/data/public_tree_guard_pending.txt"
}

/// Extensions of the text metadata read under `fixtures/`.
const FIXTURE_TEXT: [&str; 5] = [".json", ".md", ".txt", ".py", ".sh"];

/// Recorded outputs of runs, where the generic account's home directory
/// (`RECORDED_GENERIC_HOMES`) is allowed.
fn recorded(rel: &str) -> bool {
    rel.starts_with("fixtures/")
        || rel.starts_with("oracle/results/")
        || (rel.starts_with("docs/benchmarks/") && !rel.ends_with("README.md"))
}

/// Account names allowed in `/home/<name>/` in recorded outputs: the generic
/// account of the x86_64 machines that generated the fixtures.
const RECORDED_GENERIC_HOMES: [&str; 1] = ["user"];

/// Scratch-directory prefixes of a session (macOS temporary directories and
/// the coding agents' scratch pads).
const SCRATCH_PREFIXES: [&str; 4] = [
    "/private/tmp/",
    "/private/var/folders/",
    "/var/folders/",
    "/tmp/claude-",
];

/// Scratch paths and sandbox ids (`bx_` and eight letters or digits) in
/// `line`, unless the line carries the allow marker.
fn scratch_and_sandbox_ids(line: &str) -> Vec<String> {
    if line.contains(ALLOW_MARKER) {
        return Vec::new();
    }
    let b = line.as_bytes();
    let mut out = Vec::new();
    for prefix in SCRATCH_PREFIXES {
        let mut from = 0;
        while let Some(off) = line[from..].find(prefix) {
            let start = from + off;
            // A prefix inside a longer path (`/tmp/claude-` in
            // `/private/tmp/claude-…`) is reported once, by the longer prefix.
            let inside =
                start > 0 && (b[start - 1].is_ascii_alphanumeric() || b[start - 1] == b'_');
            if !inside {
                let end = line[start..]
                    .find(|c: char| c.is_whitespace() || matches!(c, '"' | '\'' | '`' | ')' | ','))
                    .map_or(line.len(), |e| start + e);
                out.push(line[start..end].to_owned());
            }
            from = start + prefix.len();
        }
    }
    let mut from = 0;
    while let Some(off) = line[from..].find("bx_") {
        let start = from + off;
        let id = &b[start + 3..];
        let n = id
            .iter()
            .take_while(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
            .count();
        let before_ok =
            start == 0 || !(b[start - 1].is_ascii_alphanumeric() || b[start - 1] == b'_');
        let after_ok = id
            .get(n)
            .is_none_or(|c| !(c.is_ascii_alphanumeric() || *c == b'_'));
        if n == 8 && before_ok && after_ok {
            out.push(line[start..start + 11].to_owned());
        }
        from = start + 3;
    }
    out
}

/// Text of a file, or `None` for a binary file (a NUL byte or invalid UTF-8).
fn text_of(path: &Path) -> Option<String> {
    let bytes = std::fs::read(path).ok()?;
    if bytes.contains(&0) {
        return None;
    }
    String::from_utf8(bytes).ok()
}

fn is_name_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'-' || b == b'.'
}

/// Names that only stand for "some user" in examples and tests.
const PLACEHOLDER_NAMES: [&str; 1] = ["someone"];

/// The marker of a line that may name a fixed home directory.
const ALLOW_MARKER: &str = "public-tree-guard: allow";

/// Home-directory paths in `line`: `/Users/<name>` and `/home/<name>/`, at
/// the start of a path (not preceded by a name character or a `/`).
fn home_paths(line: &str) -> Vec<String> {
    home_paths_except(line, &[])
}

/// `home_paths`, also accepting `/home/<name>/` for the names in `generic`.
fn home_paths_except(line: &str, generic: &[&str]) -> Vec<String> {
    if line.contains(ALLOW_MARKER) {
        return Vec::new();
    }
    let b = line.as_bytes();
    let mut out = Vec::new();
    for (prefix, need_slash) in [("/Users/", false), ("/home/", true)] {
        let mut from = 0;
        while let Some(off) = line[from..].find(prefix) {
            let start = from + off;
            let name_start = start + prefix.len();
            let mut end = name_start;
            while end < b.len() && is_name_byte(b[end]) {
                end += 1;
            }
            let has_name = end > name_start && b[name_start].is_ascii_alphanumeric();
            let at_path_start = start == 0
                || !(b[start - 1].is_ascii_alphanumeric()
                    || matches!(b[start - 1], b'_' | b'.' | b'/'));
            let name = &line[name_start..end];
            let placeholder = PLACEHOLDER_NAMES.contains(&name)
                || (prefix == "/home/" && generic.contains(&name));
            if has_name
                && at_path_start
                && !placeholder
                && (!need_slash || b.get(end) == Some(&b'/'))
            {
                out.push(line[start..end].to_owned());
            }
            from = name_start;
        }
    }
    out
}

/// Remove code spans (`` `...` ``) from a line, keeping its length small.
fn without_code_spans(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut rest = line;
    while let Some(i) = rest.find('`') {
        out.push_str(&rest[..i]);
        let ticks = rest[i..].bytes().take_while(|&c| c == b'`').count();
        let after = &rest[i + ticks..];
        let fence = "`".repeat(ticks);
        match after.find(&fence) {
            Some(j) => rest = &after[j + ticks..],
            None => {
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// Link targets of one Markdown line (outside code).
fn link_targets(line: &str) -> Vec<String> {
    let line = without_code_spans(line);
    let mut out = Vec::new();
    // reference definitions: `[label]: target`
    let t = line.trim_start();
    if t.starts_with('[')
        && let Some(close) = t.find("]:")
        && !t[1..close].contains('[')
    {
        if let Some(target) = t[close + 2..].split_whitespace().next() {
            out.push(target.trim_matches(['<', '>']).to_owned());
        }
        return out;
    }
    // inline links: `](target)` or `](target "title")`
    let mut from = 0;
    while let Some(off) = line[from..].find("](") {
        let start = from + off + 2;
        let mut depth = 0usize;
        let mut end = start;
        for (k, c) in line[start..].char_indices() {
            match c {
                '(' => depth += 1,
                ')' if depth == 0 => {
                    end = start + k;
                    break;
                }
                ')' => depth -= 1,
                _ => {}
            }
            end = start + k + c.len_utf8();
        }
        let inner = line[start..end].trim();
        let target = if let Some(stripped) = inner.strip_prefix('<') {
            stripped.split('>').next().unwrap_or("")
        } else {
            inner.split_whitespace().next().unwrap_or("")
        };
        if !target.is_empty() {
            out.push(target.to_owned());
        }
        from = end.max(start);
    }
    out
}

/// A relative link that the guard checks, without its anchor; `None` for
/// external, absolute and anchor-only links.
fn checked_path(target: &str) -> Option<String> {
    let lower = target.to_ascii_lowercase();
    if target.starts_with('#')
        || target.starts_with('/')
        || lower.contains("://")
        || lower.starts_with("mailto:")
        || lower.starts_with("data:")
        || lower.starts_with("tel:")
    {
        return None;
    }
    let path = target.split(['#', '?']).next().unwrap_or("");
    if path.is_empty() {
        return None;
    }
    Some(path.replace("%20", " "))
}

/// Normalise `dir/target` lexically (no file-system access); `None` when the
/// path leaves the repository.
fn resolve(dir: &str, target: &str) -> Option<String> {
    let mut parts: Vec<&str> = dir.split('/').filter(|p| !p.is_empty()).collect();
    for p in target.split('/') {
        match p {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            _ => parts.push(p),
        }
    }
    Some(parts.join("/"))
}

/// Broken relative links of a Markdown file (`rel` relative to `root`).
fn broken_links(root: &Path, rel: &str, text: &str) -> Vec<String> {
    let dir = rel.rsplit_once('/').map_or("", |(d, _)| d);
    let mut out = Vec::new();
    let mut fence: Option<(char, usize)> = None;
    for (i, line) in text.lines().enumerate() {
        let s = line.trim_start_matches(' ');
        if line.len() - s.len() <= 3
            && let Some(ch) = s.chars().next()
            && (ch == '`' || ch == '~')
        {
            let len = s.chars().take_while(|&c| c == ch).count();
            if len >= 3 {
                match fence {
                    None => fence = Some((ch, len)),
                    Some((c, l)) if c == ch && len >= l => fence = None,
                    Some(_) => {}
                }
                continue;
            }
        }
        if fence.is_some() {
            continue;
        }
        for target in link_targets(line) {
            let Some(path) = checked_path(&target) else {
                continue;
            };
            match resolve(dir, &path) {
                Some(p) if root.join(&p).exists() => {}
                Some(p) => out.push(format!(
                    "{rel}:{}: link to `{target}` ({p} does not exist)",
                    i + 1
                )),
                None => out.push(format!(
                    "{rel}:{}: link to `{target}` leaves the repository",
                    i + 1
                )),
            }
        }
    }
    out
}

/// Every hit of the tree at `root`, keyed by file.
fn hits(root: &Path) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for rel in repo_files::repository_files(root) {
        if exempt(&rel) {
            continue;
        }
        let Some(text) = text_of(&root.join(&rel)) else {
            continue;
        };
        let generic: &[&str] = if recorded(&rel) {
            &RECORDED_GENERIC_HOMES
        } else {
            &[]
        };
        for (i, line) in text.lines().enumerate() {
            for p in home_paths_except(line, generic) {
                out.push((
                    rel.clone(),
                    format!("{rel}:{}: home-directory path `{p}`", i + 1),
                ));
            }
            for p in scratch_and_sandbox_ids(line) {
                out.push((
                    rel.clone(),
                    format!("{rel}:{}: scratch path or sandbox id `{p}`", i + 1),
                ));
            }
        }
        if links_checked(&rel) {
            for m in broken_links(root, &rel, &text) {
                out.push((rel.clone(), m));
            }
        }
    }
    out
}

/// Markdown files whose relative links are checked: under `docs/` and at the
/// root of the repository.
fn links_checked(rel: &str) -> bool {
    rel.ends_with(".md") && (rel.starts_with("docs/") || !rel.contains('/'))
}

fn pending(root: &Path) -> BTreeSet<String> {
    let path = root.join("crates/akantu-validation").join(PENDING);
    std::fs::read_to_string(&path)
        .unwrap_or_default()
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(str::to_owned)
        .collect()
}

#[test]
fn no_home_paths_and_no_broken_doc_links() {
    let root = std::env::var_os("AKANTU_DOC_GUARD_ROOT").map_or_else(workspace_root, PathBuf::from);
    let pending = pending(&root);
    let all = hits(&root);
    let covers = |entry: &str, file: &str| {
        if entry.ends_with('/') {
            file.starts_with(entry)
        } else {
            file == entry
        }
    };
    let failing: Vec<&str> = all
        .iter()
        .filter(|(f, _)| !pending.iter().any(|e| covers(e, f)))
        .map(|(_, m)| m.as_str())
        .collect();
    let stale: Vec<String> = pending
        .iter()
        .filter(|e| !all.iter().any(|(f, _)| covers(e, f)))
        .map(|f| format!("{PENDING}: `{f}` has no hit any more (or is gone): remove the line"))
        .collect();
    let exempted = all.len() - failing.len();
    if exempted > 0 {
        eprintln!(
            "{exempted} hit(s) in {} pending file(s) exempted by {PENDING}",
            pending.len()
        );
    }
    assert!(
        failing.is_empty() && stale.is_empty(),
        "{} hit(s), {} stale pending entr(ies):\n{}",
        failing.len(),
        stale.len(),
        failing
            .iter()
            .map(|s| (*s).to_owned())
            .chain(stale)
            .collect::<Vec<_>>()
            .join("\n")
    );
}

#[test]
fn home_paths_are_found() {
    assert_eq!(home_paths("see /Users/alice/code/x"), ["/Users/alice"]);
    assert_eq!(home_paths("HOME=/home/bob/akantu"), ["/home/bob"]);
    assert!(home_paths("/home/ alone, /Users/ and /usr/home").is_empty());
    assert!(home_paths("a /home/x without slash").is_empty());
    assert_eq!(home_paths("/Users/a and /home/b/c").len(), 2);
    assert!(home_paths("boost/spirit/home/x3/support").is_empty());
    assert_eq!(home_paths("${SRC:-/Users/alice/akantu}"), ["/Users/alice"]);
    assert!(home_paths("dir=/home/someone/x /Users/someone/y").is_empty());
    assert!(home_paths("h=/home/akantu/.ssh # public-tree-guard: allow").is_empty());
    assert_eq!(home_paths("-v \"/home/user/akantu:/src\""), ["/home/user"]);
}

#[test]
fn scratch_paths_and_sandbox_ids_are_found() {
    assert_eq!(
        scratch_and_sandbox_ids("dir=/private/tmp/claude-501/x/scratchpad/r1 done"),
        ["/private/tmp/claude-501/x/scratchpad/r1"]
    );
    assert_eq!(
        scratch_and_sandbox_ids("/private/var/folders/6x/T/a"),
        ["/private/var/folders/6x/T/a"]
    );
    assert_eq!(
        scratch_and_sandbox_ids("\"/var/folders/6x/abc/T/x.md\""),
        ["/var/folders/6x/abc/T/x.md"]
    );
    assert_eq!(
        scratch_and_sandbox_ids("see /tmp/claude-1/a"),
        ["/tmp/claude-1/a"]
    );
    assert!(scratch_and_sandbox_ids("/tmp/pyfe.u9oZzX and /tmp/x").is_empty());
    assert_eq!(
        scratch_and_sandbox_ids("sandbox bx_6sghvcfm (default)"),
        ["bx_6sghvcfm"]
    );
    assert_eq!(scratch_and_sandbox_ids("`bx_hrsrcnks`,"), ["bx_hrsrcnks"]);
    assert!(scratch_and_sandbox_ids("let bx_values = 1; abx_12345678; bx_1234567").is_empty());
    assert!(scratch_and_sandbox_ids("bx_123456789 bx_ABCDEFGH").is_empty());
    assert!(scratch_and_sandbox_ids("bx_6sghvcfm # public-tree-guard: allow").is_empty());
}

#[test]
fn generic_homes_only_in_recorded_outputs() {
    assert_eq!(home_paths("--out /home/user/stage"), ["/home/user"]);
    assert!(home_paths_except("--out /home/user/stage", &RECORDED_GENERIC_HOMES).is_empty());
    assert_eq!(
        home_paths_except("/home/alice/x /Users/user/y", &RECORDED_GENERIC_HOMES),
        ["/Users/user", "/home/alice"]
    );
    assert!(recorded("fixtures/model/smm/x/manifest.json"));
    assert!(recorded("oracle/results/ctest-strict.json"));
    assert!(recorded("docs/benchmarks/c1-2026-11-12/state.json"));
    assert!(!recorded("docs/benchmarks/c1-2026-11-12/README.md"));
    assert!(!recorded("docs/ORACLE_BUILD.md"));
}

#[test]
fn scope_of_the_checks() {
    assert!(links_checked("README.md"));
    assert!(links_checked("docs/manual/src/intro.md"));
    assert!(!links_checked("ci/README.md"));
    assert!(!links_checked("docs/TESTING.txt"));
    assert!(!exempt("fixtures/fe/tolerances.json"));
    assert!(!exempt("fixtures/examples/static/provenance.txt"));
    assert!(!exempt("fixtures/mesh2/README.md"));
    assert!(exempt("fixtures/model/smm/patch_triangle_3/data/u.npy"));
    assert!(exempt("fixtures/meshes/gmsh-4.13.1/test/x.msh"));
    assert!(!exempt("oracle/results/known-failures.json"));
    assert!(!exempt("docs/benchmarks/c1-2026-11-12/state.json"));
    assert!(!exempt("docs/benchmarks/c1-2026-11-12/README.md"));
    assert!(!exempt("oracle/scripts/common.sh"));
}

#[test]
fn links_are_parsed() {
    assert_eq!(
        link_targets("a [x](../TESTING.md#tiers) b"),
        ["../TESTING.md#tiers"]
    );
    assert_eq!(
        link_targets("![f](figures/a.svg \"title\")"),
        ["figures/a.svg"]
    );
    assert_eq!(link_targets("[ref]: ./x.md"), ["./x.md"]);
    assert_eq!(link_targets("[a](f(1).md)"), ["f(1).md"]);
    assert!(link_targets("`[x](nope.md)` in code").is_empty());
    assert_eq!(checked_path("https://example.org/a"), None);
    assert_eq!(checked_path("#anchor"), None);
    assert_eq!(checked_path("a.md#x").as_deref(), Some("a.md"));
    assert_eq!(
        resolve("docs/manual/src", "../../TESTING.md").as_deref(),
        Some("docs/TESTING.md")
    );
    assert_eq!(resolve("docs", "../../x"), None);
}

#[test]
fn broken_links_are_reported_and_code_is_skipped() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(tmp.path().join("docs/sub")).unwrap();
    std::fs::write(tmp.path().join("docs/a.md"), "x").unwrap();
    let text = "[ok](../a.md) [bad](missing.md)\n```\n[code](nope.md)\n```\n[web](https://x.org)\n";
    let b = broken_links(tmp.path(), "docs/sub/b.md", text);
    assert_eq!(b.len(), 1, "{b:?}");
    assert!(b[0].contains("missing.md"), "{b:?}");
}
