// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Tests of the user
// manual (`docs/manual`): every input-file snippet parses, every
// generated table is identical to what the code produces, and every link
// resolves. Upstream counterparts of the generated content:
// doc/dev-doc/manual/appendix/elements.rst (element appendix),
// doc/dev-doc/manual/basic_types.rst (element tables),
// doc/dev-doc/manual/io.rst (solver and section keywords).
//
// Regenerate the generated sections after a code change with
// `AKANTU_BLESS_MANUAL=1 cargo test -p akantu --test manual`.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use akantu::core::element_type::ElementKind;
use akantu::core::enums::{
    AnalysisMethod, IntegrationSchemeType, NonLinearSolverType, SolveConvergenceCriteria,
    SparseSolverType, TimeStepSolverType,
};
use akantu::core::{ElementType, Real};
use akantu::fe::element_class::LagrangeElement;
use akantu::fe::quadrature::default_rule;
use akantu::fe::with_element_class;
use akantu::parser::param::ParameterAccess;
use akantu::parser::{Parser, ParserType};

mod availability;
mod parity_rows;

fn manual_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/manual")
}

fn src_dir() -> PathBuf {
    manual_dir().join("src")
}

/// Every Markdown file of the manual (sorted).
fn markdown_files() -> Vec<PathBuf> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        let mut entries: Vec<_> = std::fs::read_dir(dir)
            .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
            .map(|e| e.unwrap().path())
            .collect();
        entries.sort();
        for p in entries {
            if p.is_dir() {
                walk(&p, out);
            } else if p.extension().is_some_and(|e| e == "md") {
                out.push(p);
            }
        }
    }
    let mut out = Vec::new();
    walk(&src_dir(), &mut out);
    out
}

fn read(p: &Path) -> String {
    std::fs::read_to_string(p).unwrap_or_else(|e| panic!("{}: {e}", p.display()))
}

/// A fenced code block: info string, first line number, content.
struct Fence {
    info: String,
    line: usize,
    body: String,
}

fn fences(text: &str) -> Vec<Fence> {
    let mut out = Vec::new();
    let mut open: Option<Fence> = None;
    for (i, line) in text.lines().enumerate() {
        let t = line.trim_start();
        if let Some(rest) = t.strip_prefix("```") {
            match open.take() {
                None => {
                    open = Some(Fence {
                        info: rest.trim().to_string(),
                        line: i + 1,
                        body: String::new(),
                    });
                }
                Some(f) => out.push(f),
            }
        } else if let Some(f) = open.as_mut() {
            f.body.push_str(line);
            f.body.push('\n');
        }
    }
    assert!(open.is_none(), "unterminated code fence");
    out
}

/* ---------------------------------------------------------------------- */
/* Input-file snippets                                                     */
/* ---------------------------------------------------------------------- */

/// Every ```` ```dat ```` block of the manual parses with the port's input
/// parser; blocks tagged ```` ```dat,fails ```` must be rejected. Every file
/// of `src/inputs/` parses too.
#[test]
fn manual_input_snippets_parse() {
    let mut checked = 0;
    for file in markdown_files() {
        for f in fences(&read(&file)) {
            let mut tags = f.info.split(',').map(str::trim);
            if tags.next() != Some("dat") {
                continue;
            }
            let should_fail = tags.any(|t| t == "fails");
            let name = format!("{}:{}", file.display(), f.line);
            let body = expand_includes(&f.body, file.parent().unwrap());
            let r = Parser::parse_str(&body, &name);
            match (r, should_fail) {
                (Ok(_), false) | (Err(_), true) => {}
                (Ok(_), true) => panic!("{name}: snippet tagged `fails` parsed"),
                (Err(e), false) => panic!("{name}: {e}"),
            }
            checked += 1;
        }
    }
    for entry in std::fs::read_dir(src_dir().join("inputs")).unwrap() {
        let p = entry.unwrap().path();
        if p.extension().is_some_and(|e| e == "dat") {
            Parser::parse_file(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display()));
            checked += 1;
        }
    }
    // sanity check of the scanner (the manual has 9 `dat` blocks and 1 input file)
    assert!(checked >= 10, "only {checked} input snippets found");
}

/// Expand mdBook whole-file includes (`{{#include path}}`, relative to the
/// chapter) in a code block.
fn expand_includes(body: &str, dir: &Path) -> String {
    let mut out = String::new();
    for line in body.lines() {
        let t = line.trim();
        if let Some(path) = t
            .strip_prefix("{{#include ")
            .and_then(|r| r.strip_suffix("}}"))
        {
            out.push_str(&read(&dir.join(path.trim())));
        } else {
            out.push_str(line);
            out.push('\n');
        }
    }
    out
}

/* ---------------------------------------------------------------------- */
/* Links                                                                   */
/* ---------------------------------------------------------------------- */

/// mdBook heading anchor: lower case, alphanumerics, `-` and `_` kept,
/// spaces to `-`, everything else dropped.
fn anchor_of(heading: &str) -> String {
    let mut s = String::new();
    for c in heading.trim().chars() {
        if c.is_alphanumeric() || c == '-' || c == '_' {
            s.extend(c.to_lowercase());
        } else if c == ' ' {
            s.push('-');
        }
    }
    s
}

fn anchors(text: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let mut seen: BTreeMap<String, usize> = BTreeMap::new();
    let mut in_fence = false;
    for line in text.lines() {
        if line.trim_start().starts_with("```") {
            in_fence = !in_fence;
            continue;
        }
        if in_fence {
            continue;
        }
        if let Some(h) = line.strip_prefix('#') {
            let h = h.trim_start_matches('#');
            if h.starts_with(' ') {
                let a = anchor_of(&h.replace('`', ""));
                let n = seen.entry(a.clone()).or_insert(0);
                out.insert(if *n == 0 {
                    a.clone()
                } else {
                    format!("{a}-{n}")
                });
                *n += 1;
            }
        }
    }
    out
}

/// Inline links `[text](target)` outside code fences and code spans.
fn links(text: &str) -> Vec<String> {
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
        // drop code spans
        let mut clean = String::new();
        let mut in_code = false;
        for c in line.chars() {
            if c == '`' {
                in_code = !in_code;
            } else if !in_code {
                clean.push(c);
            }
        }
        let b = clean.as_bytes();
        let mut i = 0;
        while i + 1 < b.len() {
            if b[i] == b']'
                && b[i + 1] == b'('
                && let Some(end) = clean[i + 2..].find(')')
            {
                out.push(clean[i + 2..i + 2 + end].to_string());
                i += 2 + end;
                continue;
            }
            i += 1;
        }
    }
    out
}

/// Every relative link of the manual (and of the repository README) points
/// to an existing file and, for Markdown targets, an existing heading
/// anchor; every chapter is listed in `SUMMARY.md` and every `SUMMARY.md`
/// entry exists.
#[test]
fn manual_links_resolve() {
    let mut files = markdown_files();
    files.push(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../README.md"));
    let mut checked = 0;
    for file in &files {
        let dir = file.parent().unwrap();
        for target in links(&read(file)) {
            if target.starts_with("http://") || target.starts_with("https://") {
                continue;
            }
            let (path, anchor) = match target.split_once('#') {
                Some((p, a)) => (p, Some(a)),
                None => (target.as_str(), None),
            };
            let resolved = if path.is_empty() {
                file.clone()
            } else {
                dir.join(path)
            };
            assert!(
                resolved.exists(),
                "{}: broken link {target}",
                file.display()
            );
            if let Some(a) = anchor
                && resolved.extension().is_some_and(|e| e == "md")
            {
                let have = anchors(&read(&resolved));
                assert!(
                    have.contains(a),
                    "{}: no anchor #{a} in {} (have {have:?})",
                    file.display(),
                    resolved.display()
                );
            }
            checked += 1;
        }
    }
    assert!(checked > 20, "only {checked} links found");

    let summary = read(&src_dir().join("SUMMARY.md"));
    let listed: BTreeSet<PathBuf> = links(&summary)
        .into_iter()
        .map(|l| src_dir().join(l))
        .collect();
    for l in &listed {
        assert!(l.exists(), "SUMMARY.md lists missing {}", l.display());
    }
    for f in markdown_files() {
        if f.file_name().is_some_and(|n| n == "SUMMARY.md") {
            continue;
        }
        assert!(listed.contains(&f), "{} is not in SUMMARY.md", f.display());
    }
}

/* ---------------------------------------------------------------------- */
/* Doc-test carriers                                                       */
/* ---------------------------------------------------------------------- */

/// Would rustdoc compile and run a fenced block with this info string? Empty
/// info strings and `rust` are Rust; so is a string made only of rustdoc
/// attributes (`ignore`, `no_run`, `should_panic`, `compile_fail`, …); any
/// other word (`dat`, `sh`, `toml`, `text`) makes it a non-Rust block.
fn is_rust_block(info: &str) -> bool {
    let tokens: Vec<&str> = info
        .split(|c: char| c == ',' || c.is_whitespace())
        .filter(|t| !t.is_empty())
        .collect();
    tokens.iter().all(|t| {
        matches!(
            *t,
            "rust"
                | "ignore"
                | "no_run"
                | "should_panic"
                | "compile_fail"
                | "test_harness"
                | "standalone_crate"
        ) || t.starts_with("edition")
            || t.starts_with("ignore-")
    })
}

/// Every Markdown file with a Rust code block (manual chapters and the
/// README) is included by `src/manual.rs`, so that its blocks really run as
/// doc-tests, and every file `src/manual.rs` includes
/// exists.
#[test]
fn manual_rust_blocks_are_doc_tests() {
    let carrier = read(&Path::new(env!("CARGO_MANIFEST_DIR")).join("src/manual.rs"));
    let mut included = BTreeSet::new();
    for (i, _) in carrier.match_indices("=> \"") {
        let rest = &carrier[i + 4..];
        let name = &rest[..rest.find('"').expect("unterminated chapter name")];
        let path = src_dir().join(name);
        assert!(path.exists(), "src/manual.rs includes missing {name}");
        included.insert(path);
    }
    assert!(
        carrier.contains("include_str!(\"../../../README.md\")"),
        "src/manual.rs no longer includes the README"
    );
    let readme = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../README.md");
    included.insert(readme.clone());
    let mut files = markdown_files();
    files.push(readme);
    let mut with_rust = 0;
    for file in files {
        let blocks: Vec<usize> = fences(&read(&file))
            .into_iter()
            .filter(|f| is_rust_block(&f.info))
            .map(|f| f.line)
            .collect();
        if blocks.is_empty() {
            continue;
        }
        with_rust += 1;
        assert!(
            included.contains(&file),
            "{} has Rust code blocks (lines {blocks:?}) but is not included by \
             crates/akantu/src/manual.rs: they would never run",
            file.display()
        );
    }
    assert!(
        with_rust >= 10,
        "only {with_rust} files with Rust blocks found"
    );
    assert!(is_rust_block("") && is_rust_block("rust,no_run") && is_rust_block("ignore"));
    assert!(!is_rust_block("dat") && !is_rust_block("dat,fails") && !is_rust_block("sh"));
}

/* ---------------------------------------------------------------------- */
/* README status                                                           */
/* ---------------------------------------------------------------------- */

/// The README's parity numbers (a generated block) are those printed by
/// `python3 scripts/parity/stats.py --readme` for the committed
/// `docs/FEATURE_PARITY.md`: a status change in the
/// matrix without `stats.py --update` (which rewrites the block) fails
/// here instead of leaving stale numbers on the front page.
#[test]
fn readme_status_matches_parity_stats() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let out = std::process::Command::new("python3")
        .arg(root.join("scripts/parity/stats.py"))
        .arg("--readme")
        .output()
        .expect("python3 is needed to run scripts/parity/stats.py");
    assert!(
        out.status.success(),
        "stats.py failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let want = String::from_utf8(out.stdout).unwrap();
    let readme = read(&root.join("README.md"));
    let begin = "<!-- BEGIN GENERATED: parity-stats (scripts/parity/stats.py --update) -->\n";
    let end = "<!-- END GENERATED: parity-stats -->";
    let b = readme.find(begin).expect("README: no parity-stats block") + begin.len();
    let e = readme[b..]
        .find(end)
        .expect("README: unterminated parity-stats block")
        + b;
    assert_eq!(
        &readme[b..e],
        want,
        "README parity numbers are stale: run `python3 scripts/parity/stats.py --update`"
    );
}

/* ---------------------------------------------------------------------- */
/* Generated sections                                                      */
/* ---------------------------------------------------------------------- */

const BEGIN: &str = "<!-- BEGIN GENERATED: ";
const END: &str = "<!-- END GENERATED: ";

/// Content of the generated section `id`.
fn generate(id: &str) -> String {
    match id {
        "element-summary" => element_summary(),
        "element-appendix-regular" => element_appendix(ElementKind::Regular),
        "element-appendix-cohesive" => element_appendix(ElementKind::Cohesive),
        "section-types" => section_types(),
        "solver-keywords" => solver_keywords(),
        "elastic-parameters" => material_parameters(),
        "aka-options" => aka_options(),
        "registered-laws" => registered_laws(),
        "random-parameters" => random_parameters(),
        "phasefield-parameters" => phasefield_parameters(),
        "ntn-friction-laws" => ntn_friction_laws(),
        // `parity-cited <page>`, `parity-area <AREA>`,
        // `joss-figure-data` (tests/parity_rows/mod.rs)
        other => parity_rows::generate(other, &src_dir())
            .unwrap_or_else(|| panic!("unknown generated section {other}")),
    }
}

/// Every `<!-- BEGIN GENERATED: id -->` … `<!-- END GENERATED: id -->`
/// block equals what the code generates (set `AKANTU_BLESS_MANUAL=1` to
/// rewrite the blocks).
#[test]
fn manual_generated_sections_are_current() {
    let bless = std::env::var_os("AKANTU_BLESS_MANUAL").is_some();
    let mut found = BTreeSet::new();
    let mut stale = Vec::new();
    for file in markdown_files() {
        let text = read(&file);
        let mut out = String::new();
        let mut rest = text.as_str();
        while let Some(b) = rest.find(BEGIN) {
            let after = &rest[b + BEGIN.len()..];
            let id_end = after.find(" -->").expect("unterminated BEGIN marker");
            let id = &after[..id_end];
            let body_start = b + BEGIN.len() + id_end + " -->\n".len();
            let end_marker = format!("{END}{id} -->");
            let e = rest[body_start..]
                .find(&end_marker)
                .unwrap_or_else(|| panic!("{}: no end marker for {id}", file.display()))
                + body_start;
            // availability sections: `manual_availability_matches_the_workspace`
            let want = if id.starts_with("availability-") {
                rest[body_start..e].to_string()
            } else {
                generate(id)
            };
            if rest[body_start..e] != want {
                stale.push(format!("{}: {id}", file.display()));
            }
            out.push_str(&rest[..body_start]);
            out.push_str(&want);
            out.push_str(&end_marker);
            rest = &rest[e + end_marker.len()..];
            found.insert(id.to_string());
        }
        out.push_str(rest);
        if bless && out != text {
            std::fs::write(&file, out).unwrap();
        }
    }
    for id in [
        "element-summary",
        "element-appendix-regular",
        "element-appendix-cohesive",
        "section-types",
        "solver-keywords",
        "elastic-parameters",
        "aka-options",
        "registered-laws",
        "random-parameters",
        "phasefield-parameters",
        "ntn-friction-laws",
        "parity-cited appendix/joss.md",
        "parity-area PY",
        "joss-figure-data",
    ] {
        assert!(found.contains(id), "generated section {id} is not used");
    }
    assert!(
        bless || stale.is_empty(),
        "stale generated sections (run with AKANTU_BLESS_MANUAL=1): {stale:?}"
    );
}

/* ---- the JOSS claims page (DOC-019) and the tutorials pointer (DOC-021) -- */

/// `appendix/joss.md` (DOC-019): every parity row the page
/// cites is a row of `docs/FEATURE_PARITY.md` (the page's status table,
/// `parity-cited appendix/joss.md`, is generated from it and checked by
/// [`manual_generated_sections_are_current`]), every test it names exists
/// (`fn name(` in the named file), and the page covers each claim group of
/// upstream's `joss/paper.md`.
#[test]
fn joss_claims_page_cites_existing_rows_and_tests() {
    let page = read(&src_dir().join("appendix/joss.md"));
    let m = parity_rows::matrix();
    let table =
        parity_rows::cited_status(&page, &m).unwrap_or_else(|e| panic!("appendix/joss.md: {e}"));
    // the rows of the paper's claims (features, parallel insertion, contact,
    // the scaling study and its data)
    for id in [
        "COH-004", "NL-001", "MAT-020", "SMM-023", "CONT-001", "TASN-016", "CPL-003", "PART-001",
        "MPI-016", "COH-008", "EX-018", "EX-049", "EX-051", "GOLD-013", "INT-009", "DOC-019",
        "CONT-019",
    ] {
        assert!(
            table.contains(&format!("| `{id}` |")),
            "appendix/joss.md does not cite {id}"
        );
    }
    let (n, v) = parity_rows::check_test_refs(&page, &parity_rows::root());
    assert!(v.is_empty(), "appendix/joss.md: {v:#?}");
    assert!(n >= 20, "appendix/joss.md names only {n} tests");
    // A row cited by a claim table is Verified, an approved
    // exception, or named with its status in the claim's Notes cell
    let (claims, v) = parity_rows::claim_row_violations(&page, &m);
    assert!(v.is_empty(), "appendix/joss.md: {v:#?}");
    assert!(
        claims >= 9,
        "appendix/joss.md: only {claims} claim rows read"
    );
    for heading in [
        "## Features",
        "## High-performance design",
        "## Scaling analysis",
        "### The figure's data",
        "### Upstream's multi-rank runs of this benchmark (UD-890)",
        "### What the port has measured",
        "### The full-size run",
        "## Status of the cited rows",
    ] {
        assert!(
            page.lines().any(|l| l == heading),
            "appendix/joss.md: no heading {heading:?}"
        );
    }
    // the External tutorials section of the Python chapter (DOC-021) shows
    // the availability row and the generated status of the Python rows
    let py = read(&src_dir().join("models/python.md"));
    let section = &py[py
        .find("## External tutorials (DOC-021)")
        .expect("models/python.md: no External tutorials section")..];
    for marker in [
        "<!-- BEGIN GENERATED: availability-status tutorials -->",
        "<!-- BEGIN GENERATED: parity-area PY -->",
        "https://gitlab.com/akantu/tutorials",
    ] {
        assert!(section.contains(marker), "External tutorials: no {marker}");
    }
}

/// The guards of `tests/parity_rows` catch what they are meant to catch:
/// a cited row id that the matrix does not have, a test reference to a
/// missing file or function, a matrix row with a shifted cell or an
/// unknown status, and a timing row that did not run 5,000 steps; ids of
/// other registers (`UD-890`, `D-085`) and generated sections are not
/// citations.
#[test]
fn parity_rows_guards_catch_mutations() {
    let m = parity_rows::matrix();
    assert!(m.len() > 1000 && m["DOC-019"].area == "DOC");
    // citations
    assert!(parity_rows::cited_status("rows `COH-008` and `COH-999`", &m).is_err());
    assert!(parity_rows::cited_status("only UD-890, D-085 and WP-DOC-JOSS", &m).is_err());
    let t = parity_rows::cited_status(
        "COH-008 (UD-890)\n<!-- BEGIN GENERATED: x -->\n| `MAT-001` |\n<!-- END GENERATED: x -->\n",
        &m,
    )
    .unwrap();
    assert!(t.contains("`COH-008`") && !t.contains("MAT-001"));
    // test references
    let root = parity_rows::root();
    let (n, v) = parity_rows::check_test_refs(
        "`crates/akantu/tests/manual.rs::parity_rows_guards_catch_mutations` and `python/tests/test_parser.py`",
        &root,
    );
    assert_eq!((n, v.len()), (2, 0), "{v:?}");
    for bad in [
        "`crates/akantu/tests/manual.rs::no_such_test`",
        "`crates/akantu/tests/no_such_file.rs`",
        "`crates/akantu/tests/manual.rs::parity_rows_guards_catch_mutations(`",
    ] {
        let (n, v) = parity_rows::check_test_refs(bad, &root);
        assert_eq!((n, v.len()), (1, 1), "{bad} not caught");
    }
    // Mutation g2d: a function that is not a `#[test]` and a
    // feature-gated test whose line does not name the feature are refused
    let tmp = Path::new(env!("CARGO_TARGET_TMPDIR")).join("parity_rows_guard");
    let tests_dir = tmp.join("crates/x/tests");
    std::fs::create_dir_all(&tests_dir).unwrap();
    std::fs::write(
        tests_dir.join("t.rs"),
        "fn helper() {}\n\n#[test]\nfn plain() {}\n\n#[cfg(feature = \"gated\")]\n#[test]\nfn gated() {}\n\n/// doc\n#[test]\n// comment\npub fn documented() {}\n",
    )
    .unwrap();
    std::fs::write(
        tests_dir.join("u.rs"),
        "#![cfg(feature = \"filegated\")]\n\n#[test]\nfn whole_file() {}\n",
    )
    .unwrap();
    let refs = |text: &str| parity_rows::check_test_refs(text, &tmp);
    assert_eq!(
        refs("`crates/x/tests/t.rs::plain` `crates/x/tests/t.rs::documented`"),
        (2, vec![])
    );
    for (bad, what) in [
        ("`crates/x/tests/t.rs::helper`", "not a `#[test]`"),
        ("`crates/x/tests/t.rs::gated`", "gated by feature `gated`"),
        (
            "`crates/x/tests/t.rs::gated` (feature `other`)",
            "gated by feature `gated`",
        ),
        (
            "`crates/x/tests/u.rs::whole_file`",
            "gated by feature `filegated`",
        ),
    ] {
        let (n, v) = refs(bad);
        assert!(
            n == 1 && v.len() == 1 && v[0].contains(what),
            "{bad}: {v:?}"
        );
    }
    assert_eq!(
        refs("`crates/x/tests/t.rs::gated` (feature `gated`)")
            .1
            .len(),
        0
    );
    let page = read(&src_dir().join("appendix/joss.md"));
    let (n, v) = parity_rows::check_test_refs(
        &page.replace(
            "linear_extrinsic_envelope_and_unloading`",
            "extrinsic_envelope`", // a helper of independent_checks.rs
        ),
        &root,
    );
    assert!(
        n >= 20 && v.len() == 1 && v[0].contains("is not a `#[test]`"),
        "{v:?}"
    );
    // Mutation g1e: the page's claim tables against a matrix in
    // which a cited row is downgraded
    let mut downgraded = m.clone();
    downgraded.get_mut("CMAT-005").unwrap().status = "Not started".into();
    let (_, v) = parity_rows::claim_row_violations(&page, &downgraded);
    // CMAT-005 is cited by two claim rows (cohesive laws; friction)
    assert!(
        v.len() == 2
            && v.iter()
                .all(|e| e.contains("CMAT-005") && e.contains("Not started")),
        "{v:?}"
    );
    let table = "## Features\n\n| Claim of the paper | Parity rows | Tests in the port | Notes |\n|---|---|---|---|\n";
    let page2 = format!(
        "{table}| c | CMAT-005 | t | n |\n\n## High-performance design\n\n| Claim of the paper | Parity rows | Tests in the port | Notes |\n"
    );
    let mut m2 = m.clone();
    for status in ["Verified", "Exception (approved)"] {
        m2.get_mut("CMAT-005").unwrap().status = status.into();
        assert_eq!(parity_rows::claim_row_violations(&page2, &m2), (1, vec![]));
    }
    m2.get_mut("CMAT-005").unwrap().status = "In progress".into();
    assert_eq!(parity_rows::claim_row_violations(&page2, &m2).1.len(), 1);
    // ... unless the Notes cell names the row with its status
    let named = page2.replace("| t | n |", "| t | CMAT-005 is In progress |");
    assert_eq!(parity_rows::claim_row_violations(&named, &m2), (1, vec![]));
    // an unknown row and a missing section are violations too
    let unknown = page2.replace("CMAT-005", "CMAT-999");
    assert_eq!(parity_rows::claim_row_violations(&unknown, &m2).1.len(), 1);
    assert!(
        !parity_rows::claim_row_violations("## Features\n", &m2)
            .1
            .is_empty()
    );
    // matrix rows
    let head = "| ID | Feature | Status |\n|---|---|---|\n";
    assert!(parity_rows::matrix_from(&format!("{head}| AB-001 | f | Verified |\n")).is_ok());
    assert!(parity_rows::matrix_from(&format!("{head}| AB-001 | f | g | Verified |\n")).is_err());
    assert!(parity_rows::matrix_from(&format!("{head}| AB-001 | f | Done |\n")).is_err());
    // timing rows
    let csv = "\"psize\", \"solve_step\", \"solve_step nb_rep\"\n1, 10.0, 5000\n1, 12.0, 5000\n2, 6.0, 5000\n";
    let s = parity_rows::jed_series(csv, false).unwrap();
    assert_eq!(s[&1], vec![10.0, 12.0]);
    assert!(parity_rows::jed_series(&csv.replace("6.0, 5000", "6.0, 50"), false).is_err());
    assert!(parity_rows::jed_series(csv, true).is_err()); // no check column
}

/* ---- element appendix --------------------------------------------------- */

/// Reference-element node coordinates and shape functions of an
/// interpolation, as printed in the appendix. The formulas are evaluated by
/// [`eval`] and compared with the port's kernels
/// ([`shape_formulas_match_the_kernels`]); the coordinates are checked with
/// the Kronecker property `N_j(ξ_i) = δ_ij`.
struct Interp {
    nodes: Vec<Vec<Real>>,
    shapes: Vec<String>,
}

fn s(v: &[&str]) -> Vec<String> {
    v.iter().map(|x| (*x).to_string()).collect()
}

fn hexa_corner(sx: f64, sy: f64, sz: f64, serendipity: bool) -> String {
    let sg = |v: f64| if v > 0.0 { '+' } else { '-' };
    let base = format!("(1 {} ξ)·(1 {} η)·(1 {} ζ)", sg(sx), sg(sy), sg(sz));
    if serendipity {
        format!("{base}·(-2 {} ξ {} η {} ζ)/8", sg(sx), sg(sy), sg(sz))
    } else {
        format!("{base}/8")
    }
}

const HEX_CORNERS: [[f64; 3]; 8] = [
    [-1., -1., -1.],
    [1., -1., -1.],
    [1., 1., -1.],
    [-1., 1., -1.],
    [-1., -1., 1.],
    [1., -1., 1.],
    [1., 1., 1.],
    [-1., 1., 1.],
];

/// Interpolation data of the Lagrange family used by `ty` (cohesive types
/// use the family of their facet).
fn interp(ty: ElementType) -> Interp {
    use ElementType as E;
    let p1 = |v: &[f64]| v.to_vec();
    match ty {
        E::Point1 | E::Cohesive1d2 => Interp {
            nodes: vec![vec![]],
            shapes: s(&["1"]),
        },
        E::Segment2 | E::Cohesive2d4 => Interp {
            nodes: vec![p1(&[-1.]), p1(&[1.])],
            shapes: s(&["(1 - ξ)/2", "(1 + ξ)/2"]),
        },
        E::Segment3 | E::Cohesive2d6 => Interp {
            nodes: vec![p1(&[-1.]), p1(&[1.]), p1(&[0.])],
            shapes: s(&["ξ·(ξ - 1)/2", "ξ·(ξ + 1)/2", "1 - ξ²"]),
        },
        E::Triangle3 | E::Cohesive3d6 => Interp {
            nodes: vec![p1(&[0., 0.]), p1(&[1., 0.]), p1(&[0., 1.])],
            shapes: s(&["1 - ξ - η", "ξ", "η"]),
        },
        E::Triangle6 | E::Cohesive3d12 => Interp {
            nodes: vec![
                p1(&[0., 0.]),
                p1(&[1., 0.]),
                p1(&[0., 1.]),
                p1(&[0.5, 0.]),
                p1(&[0.5, 0.5]),
                p1(&[0., 0.5]),
            ],
            shapes: s(&[
                "(1 - ξ - η)·(1 - 2·ξ - 2·η)",
                "ξ·(2·ξ - 1)",
                "η·(2·η - 1)",
                "4·ξ·(1 - ξ - η)",
                "4·ξ·η",
                "4·η·(1 - ξ - η)",
            ]),
        },
        E::Quadrangle4 | E::Cohesive3d8 => Interp {
            nodes: vec![
                p1(&[-1., -1.]),
                p1(&[1., -1.]),
                p1(&[1., 1.]),
                p1(&[-1., 1.]),
            ],
            shapes: s(&[
                "(1 - ξ)·(1 - η)/4",
                "(1 + ξ)·(1 - η)/4",
                "(1 + ξ)·(1 + η)/4",
                "(1 - ξ)·(1 + η)/4",
            ]),
        },
        E::Quadrangle8 | E::Cohesive3d16 => Interp {
            nodes: vec![
                p1(&[-1., -1.]),
                p1(&[1., -1.]),
                p1(&[1., 1.]),
                p1(&[-1., 1.]),
                p1(&[0., -1.]),
                p1(&[1., 0.]),
                p1(&[0., 1.]),
                p1(&[-1., 0.]),
            ],
            shapes: s(&[
                "(1 - ξ)·(1 - η)·(-1 - ξ - η)/4",
                "(1 + ξ)·(1 - η)·(-1 + ξ - η)/4",
                "(1 + ξ)·(1 + η)·(-1 + ξ + η)/4",
                "(1 - ξ)·(1 + η)·(-1 - ξ + η)/4",
                "(1 - ξ²)·(1 - η)/2",
                "(1 + ξ)·(1 - η²)/2",
                "(1 - ξ²)·(1 + η)/2",
                "(1 - ξ)·(1 - η²)/2",
            ]),
        },
        E::Tetrahedron4 => Interp {
            nodes: vec![
                p1(&[0., 0., 0.]),
                p1(&[0., 1., 0.]),
                p1(&[0., 0., 1.]),
                p1(&[1., 0., 0.]),
            ],
            shapes: s(&["1 - ξ - η - ζ", "η", "ζ", "ξ"]),
        },
        E::Tetrahedron10 => Interp {
            nodes: vec![
                p1(&[0., 0., 0.]),
                p1(&[1., 0., 0.]),
                p1(&[0., 1., 0.]),
                p1(&[0., 0., 1.]),
                p1(&[0.5, 0., 0.]),
                p1(&[0.5, 0.5, 0.]),
                p1(&[0., 0.5, 0.]),
                p1(&[0., 0., 0.5]),
                p1(&[0.5, 0., 0.5]),
                p1(&[0., 0.5, 0.5]),
            ],
            shapes: s(&[
                "(1 - ξ - η - ζ)·(1 - 2·ξ - 2·η - 2·ζ)",
                "ξ·(2·ξ - 1)",
                "η·(2·η - 1)",
                "ζ·(2·ζ - 1)",
                "4·ξ·(1 - ξ - η - ζ)",
                "4·ξ·η",
                "4·η·(1 - ξ - η - ζ)",
                "4·ζ·(1 - ξ - η - ζ)",
                "4·ξ·ζ",
                "4·η·ζ",
            ]),
        },
        E::Pentahedron6 => Interp {
            nodes: vec![
                p1(&[-1., 1., 0.]),
                p1(&[-1., 0., 1.]),
                p1(&[-1., 0., 0.]),
                p1(&[1., 1., 0.]),
                p1(&[1., 0., 1.]),
                p1(&[1., 0., 0.]),
            ],
            shapes: s(&[
                "η·(1 - ξ)/2",
                "ζ·(1 - ξ)/2",
                "(1 - η - ζ)·(1 - ξ)/2",
                "η·(1 + ξ)/2",
                "ζ·(1 + ξ)/2",
                "(1 - η - ζ)·(1 + ξ)/2",
            ]),
        },
        E::Pentahedron15 => Interp {
            nodes: vec![
                p1(&[-1., 1., 0.]),
                p1(&[-1., 0., 1.]),
                p1(&[-1., 0., 0.]),
                p1(&[1., 1., 0.]),
                p1(&[1., 0., 1.]),
                p1(&[1., 0., 0.]),
                p1(&[-1., 0.5, 0.5]),
                p1(&[-1., 0., 0.5]),
                p1(&[-1., 0.5, 0.]),
                p1(&[0., 1., 0.]),
                p1(&[0., 0., 1.]),
                p1(&[0., 0., 0.]),
                p1(&[1., 0.5, 0.5]),
                p1(&[1., 0., 0.5]),
                p1(&[1., 0.5, 0.]),
            ],
            shapes: s(&[
                "η·(1 - ξ)·(2·η - 2 - ξ)/2",
                "ζ·(1 - ξ)·(2·ζ - 2 - ξ)/2",
                "(ξ - 1)·(1 - η - ζ)·(ξ + 2·η + 2·ζ)/2",
                "η·(1 + ξ)·(2·η - 2 + ξ)/2",
                "ζ·(1 + ξ)·(2·ζ - 2 + ξ)/2",
                "(-ξ - 1)·(1 - η - ζ)·(-ξ + 2·η + 2·ζ)/2",
                "2·η·ζ·(1 - ξ)",
                "2·ζ·(1 - η - ζ)·(1 - ξ)",
                "2·η·(1 - ξ)·(1 - η - ζ)",
                "η·(1 - ξ²)",
                "ζ·(1 - ξ²)",
                "(1 - η - ζ)·(1 - ξ²)",
                "2·η·ζ·(1 + ξ)",
                "2·ζ·(1 - η - ζ)·(1 + ξ)",
                "2·η·(1 - η - ζ)·(1 + ξ)",
            ]),
        },
        E::Hexahedron8 => Interp {
            nodes: HEX_CORNERS.iter().map(|c| c.to_vec()).collect(),
            shapes: HEX_CORNERS
                .iter()
                .map(|c| hexa_corner(c[0], c[1], c[2], false))
                .collect(),
        },
        E::Hexahedron20 => {
            let mut nodes: Vec<Vec<Real>> = HEX_CORNERS.iter().map(|c| c.to_vec()).collect();
            let mut shapes: Vec<String> = HEX_CORNERS
                .iter()
                .map(|c| hexa_corner(c[0], c[1], c[2], true))
                .collect();
            let mid: [([f64; 3], &str); 12] = [
                ([0., -1., -1.], "(1 - ξ²)·(1 - η)·(1 - ζ)/4"),
                ([1., 0., -1.], "(1 - η²)·(1 + ξ)·(1 - ζ)/4"),
                ([0., 1., -1.], "(1 - ξ²)·(1 + η)·(1 - ζ)/4"),
                ([-1., 0., -1.], "(1 - η²)·(1 - ξ)·(1 - ζ)/4"),
                ([-1., -1., 0.], "(1 - ζ²)·(1 - ξ)·(1 - η)/4"),
                ([1., -1., 0.], "(1 - ζ²)·(1 + ξ)·(1 - η)/4"),
                ([1., 1., 0.], "(1 - ζ²)·(1 + ξ)·(1 + η)/4"),
                ([-1., 1., 0.], "(1 - ζ²)·(1 - ξ)·(1 + η)/4"),
                ([0., -1., 1.], "(1 - ξ²)·(1 - η)·(1 + ζ)/4"),
                ([1., 0., 1.], "(1 - η²)·(1 + ξ)·(1 + ζ)/4"),
                ([0., 1., 1.], "(1 - ξ²)·(1 + η)·(1 + ζ)/4"),
                ([-1., 0., 1.], "(1 - η²)·(1 - ξ)·(1 + ζ)/4"),
            ];
            for (c, f) in mid {
                nodes.push(c.to_vec());
                shapes.push(f.to_string());
            }
            Interp { nodes, shapes }
        }
        other => panic!("no Lagrange interpolation for {other}"),
    }
}

/// Evaluate a shape-function formula of the appendix at `xi` (variables
/// `ξ`, `η`, `ζ`; operators `+ - · /`, postfix `²`, parentheses).
fn eval(formula: &str, xi: &[Real]) -> Real {
    struct P<'a> {
        c: Vec<char>,
        i: usize,
        xi: &'a [Real],
    }
    impl P<'_> {
        fn peek(&mut self) -> Option<char> {
            while self.c.get(self.i) == Some(&' ') {
                self.i += 1;
            }
            self.c.get(self.i).copied()
        }
        fn expr(&mut self) -> Real {
            let mut v = self.term();
            while let Some(op @ ('+' | '-')) = self.peek() {
                self.i += 1;
                let r = self.term();
                v = if op == '+' { v + r } else { v - r };
            }
            v
        }
        fn term(&mut self) -> Real {
            let mut v = self.unary();
            while let Some(op @ ('·' | '/')) = self.peek() {
                self.i += 1;
                let r = self.unary();
                v = if op == '·' { v * r } else { v / r };
            }
            v
        }
        fn unary(&mut self) -> Real {
            if self.peek() == Some('-') {
                self.i += 1;
                return -self.unary();
            }
            let mut v = self.primary();
            while self.peek() == Some('²') {
                self.i += 1;
                v *= v;
            }
            v
        }
        fn primary(&mut self) -> Real {
            match self.peek() {
                Some('(') => {
                    self.i += 1;
                    let v = self.expr();
                    assert_eq!(self.peek(), Some(')'), "missing )");
                    self.i += 1;
                    v
                }
                Some(c @ ('ξ' | 'η' | 'ζ')) => {
                    self.i += 1;
                    self.xi[match c {
                        'ξ' => 0,
                        'η' => 1,
                        _ => 2,
                    }]
                }
                Some(c) if c.is_ascii_digit() => {
                    let start = self.i;
                    while self
                        .c
                        .get(self.i)
                        .is_some_and(|c| c.is_ascii_digit() || *c == '.')
                    {
                        self.i += 1;
                    }
                    self.c[start..self.i]
                        .iter()
                        .collect::<String>()
                        .parse()
                        .unwrap()
                }
                other => panic!("unexpected {other:?} in {:?}", self.c),
            }
        }
    }
    let mut p = P {
        c: formula.chars().collect(),
        i: 0,
        xi,
    };
    let v = p.expr();
    assert_eq!(p.peek(), None, "trailing input in {formula}");
    v
}

fn kernel_shapes(ty: ElementType, xi: &[Real]) -> Vec<Real> {
    let mut n = vec![0.0; ty.nb_nodes_per_interpolation_element()];
    with_element_class!(ty, E => E::compute_shapes(xi, &mut n), else panic!("no kernel for {ty}"));
    n
}

fn lagrange_types(kind: ElementKind) -> Vec<ElementType> {
    ElementType::of_kind(kind)
        .filter(|t| matches!(t.kind(), ElementKind::Regular | ElementKind::Cohesive))
        .collect()
}

/// The appendix's node coordinates satisfy `N_j(ξ_i) = δ_ij` with the
/// port's kernels, and its shape-function formulas agree with the kernels
/// at the nodes, the quadrature points and 50 points of `[-1, 1]^d`.
///
/// Tolerance 1e-13 (absolute): both sides evaluate the same polynomial
/// (|N| ≤ 2 on the sample box, at most 5 factors) in a different operation
/// order, so they differ by a few ulps (≤ ~1e-14); a transcription error
/// changes values by O(0.1). This checks the documentation, not the
/// kernels (those are verified bitwise against the oracle, FE rows).
#[test]
fn shape_formulas_match_the_kernels() {
    let mut rng: u64 = 0x2545_f491_4f6c_dd1d;
    let mut next = || {
        rng ^= rng << 13;
        rng ^= rng >> 7;
        rng ^= rng << 17;
        (rng >> 11) as f64 / (1u64 << 53) as f64 * 2.0 - 1.0
    };
    for kind in [ElementKind::Regular, ElementKind::Cohesive] {
        for ty in lagrange_types(kind) {
            let it = interp(ty);
            let nd = ty.natural_space_dimension();
            assert_eq!(
                it.shapes.len(),
                ty.nb_nodes_per_interpolation_element(),
                "{ty}"
            );
            for (i, x) in it.nodes.iter().enumerate() {
                assert_eq!(x.len(), nd, "{ty}");
                let n = kernel_shapes(ty, x);
                for (j, v) in n.iter().enumerate() {
                    let delta = if i == j { 1.0 } else { 0.0 };
                    assert!((v - delta).abs() <= 1e-15, "{ty}: N_{j}(node {i}) = {v}");
                }
            }
            let mut points: Vec<Vec<Real>> = it.nodes.clone();
            let rule = default_rule(ty).unwrap();
            for q in 0..rule.nb_points() {
                points.push(rule.point(q, nd).to_vec());
            }
            for _ in 0..50 {
                points.push((0..nd).map(|_| next()).collect());
            }
            for x in &points {
                let n = kernel_shapes(ty, x);
                for (j, f) in it.shapes.iter().enumerate() {
                    let v = eval(f, x);
                    assert!(
                        (v - n[j]).abs() <= 1e-13,
                        "{ty}: N_{j} = {f} gives {v}, kernel {} at {x:?}",
                        n[j]
                    );
                }
            }
        }
    }
}

fn fmt_point(x: &[Real]) -> String {
    if x.is_empty() {
        return "—".into();
    }
    let v: Vec<String> = x.iter().map(|v| format!("{v}")).collect();
    format!("({})", v.join(", "))
}

fn type_link(ty: ElementType) -> String {
    format!("[`{}`](#{})", ty.name(), ty.name().trim_start_matches('_'))
}

fn element_summary() -> String {
    let mut o = String::new();
    o.push_str(
        "| Type | Kind | Dim. | Nodes | Shape functions | Default order | Quadrature points | Facet types |\n\
         |---|---|---|---|---|---|---|---|\n",
    );
    for ty in ElementType::ALL.iter().copied() {
        let kind = ty.kind();
        let name = match kind {
            ElementKind::Regular | ElementKind::Cohesive => type_link(ty),
            _ => format!("`{}`", ty.name()),
        };
        let nq = match kind {
            // the structural engine integrates with the same default rule
            // (`IntegrationOrders::default`, order = polynomial degree)
            ElementKind::Regular | ElementKind::Cohesive | ElementKind::Structural => {
                default_rule(ty)
                    .map(|r| r.nb_points().to_string())
                    .unwrap_or_else(|_| "—".into())
            }
            // the IGFEM engine integrates per sub-element (models/igfem.md)
            ElementKind::Igfem => "set by the IGFEM engine".into(),
            _ => "not in this release".into(),
        };
        let facets = ty
            .facet_types()
            .map(|f| {
                f.iter()
                    .map(|t| format!("`{}`", t.name()))
                    .collect::<Vec<_>>()
                    .join(", ")
            })
            .unwrap_or_else(|| "—".into());
        writeln!(
            o,
            "| {name} | {} | {} | {} | {} | {} | {nq} | {facets} |",
            kind_word(kind),
            ty.spatial_dimension(),
            ty.nb_nodes_per_element(),
            ty.nb_nodes_per_interpolation_element(),
            ty.polynomial_degree(),
        )
        .unwrap();
    }
    o
}

fn kind_word(kind: ElementKind) -> &'static str {
    match kind {
        ElementKind::Regular => "regular",
        ElementKind::Cohesive => "cohesive",
        ElementKind::Structural => "structural",
        ElementKind::Igfem => "IGFEM",
        ElementKind::NotDefined => "—",
    }
}

fn coord_header(nd: usize) -> &'static str {
    match nd {
        0 => "—",
        1 => "ξ",
        2 => "(ξ, η)",
        _ => "(ξ, η, ζ)",
    }
}

fn element_appendix(kind: ElementKind) -> String {
    let mut o = String::new();
    for ty in lagrange_types(kind) {
        let nd = ty.natural_space_dimension();
        let it = interp(ty);
        writeln!(o, "### {}\n", ty.name().trim_start_matches('_')).unwrap();
        writeln!(o, "| Property | Value |\n|---|---|").unwrap();
        writeln!(
            o,
            "| Rust name | `ElementType::{ty:?}` (upstream `{}`) |",
            ty.name()
        )
        .unwrap();
        writeln!(
            o,
            "| Spatial / natural dimension | {} / {nd} |",
            ty.spatial_dimension()
        )
        .unwrap();
        writeln!(
            o,
            "| Nodes / shape functions | {} / {} |",
            ty.nb_nodes_per_element(),
            ty.nb_nodes_per_interpolation_element()
        )
        .unwrap();
        writeln!(o, "| Interpolation | `{}` |", ty.interpolation_type()).unwrap();
        writeln!(
            o,
            "| Default integration order | {} |",
            ty.polynomial_degree()
        )
        .unwrap();
        if let Some(p1) = ty.p1_type() {
            writeln!(o, "| Linear (P1) type | `{}` |", p1.name()).unwrap();
        }
        if let Some(c) = ty.cohesive_type() {
            writeln!(o, "| Cohesive type on its facets | `{}` |", c.name()).unwrap();
        }
        o.push('\n');
        if kind == ElementKind::Cohesive {
            let half = ty.nb_nodes_per_element() / 2;
            writeln!(
                o,
                "| Nodes (side 1, side 2) | {} | N |\n|---|---|---|",
                coord_header(nd)
            )
            .unwrap();
            for (i, (x, f)) in it.nodes.iter().zip(&it.shapes).enumerate() {
                writeln!(o, "| {i}, {} | {} | `{f}` |", i + half, fmt_point(x)).unwrap();
            }
        } else {
            writeln!(o, "| Node | {} | N |\n|---|---|---|", coord_header(nd)).unwrap();
            for (i, (x, f)) in it.nodes.iter().zip(&it.shapes).enumerate() {
                writeln!(o, "| {i} | {} | `{f}` |", fmt_point(x)).unwrap();
            }
        }
        o.push('\n');
        if let Some(facets) = ty.facet_types() {
            let mut rows = Vec::new();
            for (t, ft) in facets.iter().enumerate() {
                let fc = ty.facet_connectivity(t).unwrap();
                for f in 0..fc.nb_facets {
                    let nodes: Vec<String> = fc.facet(f).map(|n| n.to_string()).collect();
                    rows.push(format!("| `{}` | {} |", ft.name(), nodes.join(", ")));
                }
            }
            if !rows.is_empty() {
                writeln!(o, "| Facet type | Local nodes |\n|---|---|").unwrap();
                for r in rows {
                    writeln!(o, "{r}").unwrap();
                }
                o.push('\n');
            }
        }
        let rule = default_rule(ty).unwrap();
        let total: Real = rule.weights.iter().sum();
        writeln!(
            o,
            "Default quadrature: {} point(s), weights sum to {total}.\n",
            rule.nb_points()
        )
        .unwrap();
        writeln!(
            o,
            "| Point | {} | Weight |\n|---|---|---|",
            coord_header(nd)
        )
        .unwrap();
        for q in 0..rule.nb_points() {
            writeln!(
                o,
                "| {q} | {} | {} |",
                fmt_point(rule.point(q, nd)),
                rule.weights[q]
            )
            .unwrap();
        }
        o.push('\n');
    }
    o
}

/* ---- keyword lists ------------------------------------------------------ */

fn section_types() -> String {
    let names: Vec<String> = ParserType::ALL
        .iter()
        .filter(|t| **t != ParserType::NotDefined)
        .map(|t| format!("`{}`", t.name()))
        .collect();
    format!("{}\n", names.join(", "))
}

fn solver_keywords() -> String {
    fn list<T: Copy + std::fmt::Display>(v: &[T], skip: &[&str]) -> String {
        v.iter()
            .map(|t| t.to_string())
            .filter(|t| !skip.contains(&t.as_str()))
            .map(|t| format!("`{}`", t.trim_start_matches('_')))
            .collect::<Vec<_>>()
            .join(", ")
    }
    let mut o = String::new();
    o.push_str("| Keyword position | Accepted values |\n|---|---|\n");
    writeln!(
        o,
        "| `time_step_solver <type>` | {} |",
        list(TimeStepSolverType::ALL, &["not_defined"])
    )
    .unwrap();
    writeln!(
        o,
        "| `non_linear_solver <type>` | {} |",
        list(NonLinearSolverType::ALL, &[])
    )
    .unwrap();
    writeln!(
        o,
        "| `integration_scheme <type>` | {} |",
        list(IntegrationSchemeType::ALL, &[])
    )
    .unwrap();
    writeln!(
        o,
        "| `sparse_solver_type = …` | {} |",
        list(SparseSolverType::ALL, &[])
    )
    .unwrap();
    writeln!(
        o,
        "| `convergence_type = …` | {} |",
        list(SolveConvergenceCriteria::ALL, &[])
    )
    .unwrap();
    o
}

/* ---- command-line options ----------------------------------------------- */

/// The `--help` text of a program that registered Akantu's options
/// (upstream `initialize` + `printHelp`).
fn aka_options() -> String {
    use akantu::parser::cli::{ArgumentParser, ParseFlags, ParseOutcome, register_akantu_options};
    let mut p = ArgumentParser::new();
    register_akantu_options(&mut p).unwrap();
    let mut argv = vec!["my_simulation".to_string(), "--help".to_string()];
    let outcome = p
        .parse_argv(&mut argv, ParseFlags::REMOVE_PARSED, true)
        .unwrap();
    assert_eq!(outcome, ParseOutcome::HelpRequested);
    format!("```text\n$ my_simulation --help\n{}```\n", p.help_string())
}

/* ---- material parameters ------------------------------------------------ */

const SQUARE: &str = "meshes/square.msh";
const CUBE: &str = "meshes/cube.msh";

fn material_parameters() -> String {
    use akantu::core::enums::MeshIoType;
    use akantu::mesh::{Mesh, SharedMesh};
    use akantu::model::ModelOptions;
    use akantu::solid::SolidMechanicsModel;

    let mut o = String::new();
    let elastic = "material elastic [\n name = m\n rho = 1\n E = 1\n nu = 0.25\n]\n";
    for (law, dat2, dat3) in [
        ("elastic", elastic, elastic),
        (
            "elastic_orthotropic",
            "material elastic_orthotropic [\n name = m\n rho = 1\n E1 = 1\n E2 = 1\n \
             nu12 = 0.1\n G12 = 1\n]\n",
            "material elastic_orthotropic [\n name = m\n rho = 1\n E1 = 1\n E2 = 1\n E3 = 1\n \
             nu12 = 0.1\n nu13 = 0.1\n nu23 = 0.1\n G12 = 1\n G13 = 1\n G23 = 1\n]\n",
        ),
        (
            "elastic_anisotropic",
            "material elastic_anisotropic [\n name = m\n rho = 1\n C11 = 2\n C22 = 2\n C33 = 1\n]\n",
            "material elastic_anisotropic [\n name = m\n rho = 1\n \
             C11 = 2\n C22 = 2\n C33 = 2\n C44 = 1\n C55 = 1\n C66 = 1\n]\n",
        ),
    ] {
        writeln!(o, "#### `{law}`\n").unwrap();
        o.push_str("| Parameter | Type | Access | 2-D | 3-D |\n");
        o.push_str("|---|---|---|---|---|\n");
        let mut rows: BTreeMap<&'static str, (String, String, bool, bool)> = BTreeMap::new();
        let mut order: Vec<&'static str> = Vec::new();
        for (dim, mesh_file, dat) in [(2, SQUARE, dat2), (3, CUBE, dat3)] {
            let mut mesh = Mesh::new(dim, "mesh").unwrap();
            akantu::io::read_mesh(&mut mesh, &src_dir().join(mesh_file), MeshIoType::Auto).unwrap();
            let parser = Parser::parse_str(dat, "material.dat").unwrap();
            let mut model =
                SolidMechanicsModel::with_parser(SharedMesh::new(mesh), None, "smm", parser)
                    .unwrap();
            model
                .init_full(&ModelOptions::new(AnalysisMethod::Static))
                .unwrap();
            let m = model.material(0).unwrap();
            for name in m.param_names() {
                let info = m.param_info(name).unwrap();
                let e = rows.entry(name).or_insert_with(|| {
                    order.push(name);
                    (
                        format!("{:?}", info.kind),
                        access_string(info.access),
                        false,
                        false,
                    )
                });
                if dim == 2 {
                    e.2 = true;
                } else {
                    e.3 = true;
                }
            }
        }
        let yes = |b: bool| if b { "✓" } else { "—" };
        for name in order {
            let (k, a, d2, d3) = &rows[name];
            writeln!(o, "| `{name}` | {k} | {a} | {} | {} |", yes(*d2), yes(*d3)).unwrap();
        }
        o.push('\n');
    }
    o
}

fn access_string(a: ParameterAccess) -> String {
    let mut s = String::new();
    if a.contains(ParameterAccess::READABLE) {
        s.push('r');
    } else {
        s.push('-');
    }
    if a.contains(ParameterAccess::WRITABLE) {
        s.push('w');
    } else {
        s.push('-');
    }
    if a.contains(ParameterAccess::PARSABLE) {
        s.push('p');
    } else {
        s.push('-');
    }
    s
}

/* ---------------------------------------------------------------------- */
/* Registered laws                                                         */
/* ---------------------------------------------------------------------- */

/// One registered constitutive law: model section type, law name and its
/// parameters (name, kind, access) in registration order.
struct LawInfo {
    section: &'static str,
    law: String,
    params: Vec<(
        &'static str,
        akantu::parser::param::ParamKind,
        ParameterAccess,
    )>,
}

fn read_manual_mesh(dim: usize) -> akantu::mesh::SharedMesh {
    use akantu::core::enums::MeshIoType;
    use akantu::mesh::{Mesh, SharedMesh};
    let file = if dim == 2 { SQUARE } else { CUBE };
    let mut mesh = Mesh::new(dim, "mesh").unwrap();
    akantu::io::read_mesh(&mut mesh, &src_dir().join(file), MeshIoType::Auto).unwrap();
    SharedMesh::new(mesh)
}

/// Every constitutive law the models of this release register in their
/// factories (solid mechanics, cohesive additions, contact resolutions,
/// phase field), created with `register_new_material` /
/// `register_new_resolution` / `register_new_phase_field` (empty option)
/// on the 2-D and 3-D manual meshes; parameters are the union over both
/// dimensions in first-seen order. Generated, so a law merged without a
/// manual update makes the generated sections stale.
fn registered_law_infos() -> Vec<LawInfo> {
    use akantu::cohesive::SolidMechanicsModelCohesive;
    use akantu::contact::ContactMechanicsModel;
    use akantu::diffusion::HeatTransferModel;
    use akantu::phase_field::PhaseFieldModel;
    use akantu::solid::SolidMechanicsModel;

    let mut out: Vec<LawInfo> = Vec::new();
    let mut push = |section: &'static str,
                    law: &str,
                    params: Vec<(
        &'static str,
        akantu::parser::param::ParamKind,
        ParameterAccess,
    )>| {
        if let Some(info) = out
            .iter_mut()
            .find(|i| i.section == section && i.law == law)
        {
            for p in params {
                if !info.params.iter().any(|q| q.0 == p.0) {
                    info.params.push(p);
                }
            }
        } else {
            out.push(LawInfo {
                section,
                law: law.to_string(),
                params,
            });
        }
    };
    for dim in [2, 3] {
        // solid mechanics model: the built-in materials
        let solid = SolidMechanicsModel::new(read_manual_mesh(dim), None, "smm").unwrap();
        let solid_names: Vec<String> = solid.physics.factory.names().map(String::from).collect();
        // cohesive model: the same plus the cohesive laws
        let mut cohesive =
            SolidMechanicsModelCohesive::new(read_manual_mesh(dim), None, "smmc").unwrap();
        let names: Vec<String> = cohesive
            .smm
            .physics
            .factory
            .names()
            .map(String::from)
            .collect();
        for law in &names {
            let i = cohesive
                .smm
                .register_new_material(&format!("m_{law}"), law, "")
                .unwrap_or_else(|e| panic!("{law} ({dim}-D): {e}"));
            let m = cohesive.smm.material(i).unwrap();
            let params = m
                .param_names()
                .into_iter()
                .map(|n| {
                    let info = m.param_info(n).unwrap();
                    (n, info.kind, info.access)
                })
                .collect();
            let section = if solid_names.contains(law) {
                "material"
            } else {
                "material (cohesive)"
            };
            push(section, law, params);
        }
        // contact mechanics model: the resolution factory
        let detector = "contact_detector [\n type = explicit\n master = master\n slave = slave\n \
            projection_tolerance = 1e-10\n max_iterations = 100\n extension_tolerance = 1e-5\n]\n";
        let parser = Parser::parse_str(detector, "contact.dat").unwrap();
        let mut contact =
            ContactMechanicsModel::with_parser(read_manual_mesh(dim), None, "cm", parser).unwrap();
        let names: Vec<String> = contact.factory.names().map(String::from).collect();
        for law in &names {
            let r = contact
                .register_new_resolution(&format!("r_{law}"), law, "")
                .unwrap_or_else(|e| panic!("{law} ({dim}-D): {e}"));
            let params = r
                .param_names()
                .into_iter()
                .map(|n| {
                    let info = r.param_info(n).unwrap();
                    (n, info.kind, info.access)
                })
                .collect();
            push("contact_resolution", law, params);
        }
        // phase-field model
        let mut pf = PhaseFieldModel::new(read_manual_mesh(dim), None, "pfm").unwrap();
        let names: Vec<String> = pf.physics.factory.names().map(String::from).collect();
        for law in &names {
            let i = pf
                .register_new_phase_field(&format!("p_{law}"), law, "")
                .unwrap_or_else(|e| panic!("{law} ({dim}-D): {e}"));
            let l = pf.phase_field(i).unwrap();
            let params = l
                .param_names()
                .into_iter()
                .map(|n| {
                    let info = l.param_info(n).unwrap();
                    (n, info.kind, info.access)
                })
                .collect();
            push("phasefield", law, params);
        }
        // diffusion (heat transfer) model: the diffusion laws
        let mut heat =
            HeatTransferModel::new(read_manual_mesh(dim), None, "heat_transfer").unwrap();
        let names: Vec<String> = heat.physics.factory.names().map(String::from).collect();
        for law in &names {
            let i = heat
                .register_new_law(&format!("d_{law}"), law, "")
                .unwrap_or_else(|e| panic!("{law} ({dim}-D): {e}"));
            let l = heat.law(i).unwrap();
            let params = l
                .param_names()
                .into_iter()
                .map(|n| {
                    let info = l.param_info(n).unwrap();
                    (n, info.kind, info.access)
                })
                .collect();
            push("constitutive_law", law, params);
        }
    }
    out
}

/// `registered-laws` (materials.md): the law names of every model's
/// factory.
fn registered_laws() -> String {
    let infos = registered_law_infos();
    let mut o = String::new();
    o.push_str("| Model | Section | Registered laws |\n|---|---|---|\n");
    for (model, section) in [
        ("`SolidMechanicsModel`", "material"),
        (
            "`SolidMechanicsModelCohesive` (in addition)",
            "material (cohesive)",
        ),
        ("`ContactMechanicsModel`", "contact_resolution"),
        ("`PhaseFieldModel`", "phasefield"),
        ("`HeatTransferModel`", "constitutive_law"),
    ] {
        let laws: Vec<String> = infos
            .iter()
            .filter(|i| i.section == section)
            .map(|i| format!("`{}`", i.law))
            .collect();
        let keyword = section.split(' ').next().unwrap();
        writeln!(o, "| {model} | `{keyword}` | {} |", laws.join(", ")).unwrap();
    }
    o
}

/// The crates whose models own a law factory (a `pub factory:` field):
/// exactly those whose laws [`registered_law_infos`] lists.
/// The cohesive model uses the solid mechanics model's factory;
/// split-node contact registers friction laws (`ntn-friction-laws`).
const MODEL_FACTORY_CRATES: [&str; 4] = [
    "akantu-contact",
    "akantu-diffusion",
    "akantu-phase-field",
    "akantu-solid",
];

/// A model merged with its own law factory and not added
/// to [`registered_law_infos`] (and the generated `registered-laws` table)
/// fails here, so a new model's laws cannot be missing from the manual
/// silently.
#[test]
fn registered_laws_cover_every_model_factory() {
    fn has_factory_field(dir: &Path) -> bool {
        std::fs::read_dir(dir).unwrap().any(|e| {
            let p = e.unwrap().path();
            if p.is_dir() {
                has_factory_field(&p)
            } else {
                p.extension().is_some_and(|x| x == "rs")
                    && read(&p)
                        .lines()
                        .any(|l| l.trim_start().starts_with("pub factory:"))
            }
        })
    }
    let crates = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let mut found: Vec<String> = std::fs::read_dir(&crates)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.join("src").is_dir() && has_factory_field(&p.join("src")))
        .map(|p| p.file_name().unwrap().to_string_lossy().to_string())
        .filter(|c| c != "akantu-python") // bindings, not a model
        .collect();
    found.sort();
    assert_eq!(
        found, MODEL_FACTORY_CRATES,
        "a model crate gained or lost a law factory: update registered_law_infos and MODEL_FACTORY_CRATES"
    );
    let sections: BTreeSet<&str> = registered_law_infos().iter().map(|i| i.section).collect();
    for s in [
        "material",
        "material (cohesive)",
        "contact_resolution",
        "phasefield",
        "constitutive_law",
    ] {
        assert!(sections.contains(s), "no registered law in section {s}");
    }
}

/// `random-parameters` (input-files.md): the parameters registered as
/// random (upstream `RandomParameter`) by the registered laws.
fn random_parameters() -> String {
    use akantu::parser::param::ParamKind;
    let infos = registered_law_infos();
    let mut o = String::new();
    o.push_str("| Section | Law | Random parameters |\n|---|---|---|\n");
    for i in &infos {
        let random: Vec<String> = i
            .params
            .iter()
            .filter(|p| p.1 == ParamKind::Random)
            .map(|p| format!("`{}`", p.0))
            .collect();
        if !random.is_empty() {
            let keyword = i.section.split(' ').next().unwrap();
            writeln!(o, "| `{keyword}` | `{}` | {} |", i.law, random.join(", ")).unwrap();
        }
    }
    o
}

/// `phasefield-parameters` (models/phase-field.md).
fn phasefield_parameters() -> String {
    let infos: Vec<LawInfo> = registered_law_infos()
        .into_iter()
        .filter(|i| i.section == "phasefield")
        .collect();
    let laws = ["quadratic", "linear"];
    assert_eq!(
        infos
            .iter()
            .map(|i| i.law.as_str())
            .collect::<BTreeSet<_>>(),
        laws.iter().copied().collect::<BTreeSet<_>>(),
        "phase-field laws changed: update models/phase-field.md"
    );
    let mut o = String::new();
    o.push_str("| Parameter | Type | Access | `quadratic` | `linear` |\n|---|---|---|---|---|\n");
    let mut order: Vec<(&str, String, String)> = Vec::new();
    for law in laws {
        let info = infos.iter().find(|i| i.law == law).unwrap();
        for (n, k, a) in &info.params {
            if !order.iter().any(|x| x.0 == *n) {
                order.push((n, format!("{k:?}"), access_string(*a)));
            }
        }
    }
    let yes = |b: bool| if b { "✓" } else { "—" };
    for (n, k, a) in order {
        let has = |law: &str| {
            infos
                .iter()
                .find(|i| i.law == law)
                .unwrap()
                .params
                .iter()
                .any(|p| p.0 == n)
        };
        writeln!(
            o,
            "| `{n}` | {k} | {a} | {} | {} |",
            yes(has("quadratic")),
            yes(has("linear"))
        )
        .unwrap();
    }
    o
}

/// `ntn-friction-laws` (models/ntn-contact.md): every registered friction
/// law (upstream `NTNFactory` keys) with its parsable parameters
/// (`no_regularisation`), laws with the same parameters on one row.
fn ntn_friction_laws() -> String {
    use akantu::core::enums::MeshIoType;
    use akantu::mesh::Mesh;
    use akantu::parser::ParameterRegistry;
    use akantu_ntn_contact::{NtnBaseContact, TasnUpstreamCompat, allocate_friction};
    let mut mesh = Mesh::new(2, "mesh").unwrap();
    akantu::io::read_mesh(
        &mut mesh,
        &src_dir().join("meshes/ntn_2d.msh"),
        MeshIoType::Auto,
    )
    .unwrap();
    let contact =
        NtnBaseContact::new_ntn(&mesh, 2, "contact", TasnUpstreamCompat::default()).unwrap();
    let mut rows: Vec<(Vec<&'static str>, Vec<String>)> = Vec::new();
    for law in akantu_ntn_contact::registered_friction_laws() {
        let f = allocate_friction(&contact, mesh.nb_nodes(), &law, "no_regularisation").unwrap();
        let params: Vec<&'static str> = f
            .param_names()
            .into_iter()
            .filter(|n| {
                f.param_info(n)
                    .is_some_and(|i| i.access.contains(ParameterAccess::PARSABLE))
            })
            .collect();
        match rows.iter_mut().find(|r| r.0 == params) {
            Some(r) => r.1.push(law),
            None => rows.push((params, vec![law])),
        }
    }
    let mut o = String::new();
    o.push_str("| Friction laws | Parameters |\n|---|---|\n");
    for (params, laws) in rows {
        let laws: Vec<String> = laws.iter().map(|l| format!("`{l}`")).collect();
        let params: Vec<String> = params.iter().map(|p| format!("`{p}`")).collect();
        writeln!(o, "| {} | {} |", laws.join(", "), params.join(", ")).unwrap();
    }
    o
}

/* ---------------------------------------------------------------------- */
/* Law keyword tables and internal lists (DOC-010, DOC-023)                */
/* ---------------------------------------------------------------------- */

/// Keywords of the material base (`Material` constructor), not listed in
/// the law tables (the catalogue's "Common keywords").
const BASE_KEYWORDS: [&str; 5] = [
    "name",
    "rho",
    "finite_deformation",
    "inelastic_deformation",
    "eigen_grad_u",
];

/// Keywords of the elastic parent (`PlaneStressToolbox`, `MaterialThermal`,
/// `MaterialElastic`), not listed by a table that inherits `elastic`.
const ELASTIC_KEYWORDS: [&str; 8] = [
    "Plane_Stress",
    "E",
    "nu",
    "alpha",
    "delta_T",
    "lambda",
    "mu",
    "kapa",
];

/// Internal fields every solid law has (the catalogue's common list), and
/// the thermal ones of the laws built on `elastic`; the `law-internals`
/// paragraphs list the others.
const COMMON_INTERNALS: [&str; 8] = [
    "third_axis_deformation",
    "stress",
    "grad_u",
    "eigen_grad_u",
    "potential_energy",
    "delta_T",
    "sigma_th",
    "epsilon_th",
];

/// The elastic family: described with the generated `elastic-parameters`
/// tables of materials.md, so it has no keyword table.
const ELASTIC_FAMILY: [&str; 3] = ["elastic", "elastic_orthotropic", "elastic_anisotropic"];

/// A `<!-- law-keywords: … -->` or `<!-- law-internals: … -->` marker:
/// the law, its attributes (`option=…`; `inherits=base` — the default —,
/// `inherits=elastic` or `inherits=<law>`: the keywords the table does
/// not repeat) and the lines that follow it (up to the next marker or
/// heading).
struct LawMarker {
    place: String,
    law: String,
    attrs: BTreeMap<String, String>,
    after: Vec<String>,
}

fn law_markers(kind: &str) -> Vec<LawMarker> {
    let open = format!("<!-- {kind}: ");
    let mut out = Vec::new();
    for file in markdown_files() {
        let text = read(&file);
        let lines: Vec<&str> = text.lines().collect();
        for (i, line) in lines.iter().enumerate() {
            let Some(rest) = line.trim().strip_prefix(open.as_str()) else {
                continue;
            };
            let rest = rest
                .strip_suffix("-->")
                .expect("unterminated marker")
                .trim();
            let mut words = rest.split_whitespace();
            let law = words.next().expect("marker without a law").to_string();
            let attrs = words
                .map(|w| {
                    let (k, v) = w
                        .split_once('=')
                        .unwrap_or_else(|| panic!("bad attribute {w}"));
                    (k.to_string(), v.to_string())
                })
                .collect();
            let after = lines[i + 1..]
                .iter()
                .take_while(|l| !l.starts_with("<!-- law-") && !l.starts_with('#'))
                .map(|l| (*l).to_string())
                .collect();
            out.push(LawMarker {
                place: format!("{}:{}", file.display(), i + 1),
                law,
                attrs,
                after,
            });
        }
    }
    out
}

/// The cells of a Markdown table row (outer pipes removed, trimmed).
fn table_cells(row: &str) -> Vec<String> {
    let t = row.trim();
    let t = t.strip_prefix('|').unwrap_or(t);
    let t = t.strip_suffix('|').unwrap_or(t);
    t.split('|').map(|c| c.trim().to_string()).collect()
}

/// `` `x` `` → `x`.
fn unquote(cell: &str) -> &str {
    cell.trim()
        .strip_prefix('`')
        .and_then(|c| c.strip_suffix('`'))
        .unwrap_or(cell.trim())
}

/// Every `` `…` `` span of a text.
fn code_spans(text: &str) -> Vec<String> {
    text.split('`')
        .enumerate()
        .filter(|(i, _)| i % 2 == 1)
        .map(|(_, s)| s.to_string())
        .collect()
}

/// One registered parameter of a law: kind, access, and the value read
/// back right after construction (`None` for a parameter that is not
/// readable).
#[derive(Clone)]
struct LawParam {
    kind: akantu::parser::param::ParamKind,
    access: ParameterAccess,
    value: Option<akantu::parser::ParamValue>,
}

/// The parameters of `law` (with `option`) in 2-D and 3-D (union), from
/// the factory of the cohesive model (the solid laws and the cohesive
/// laws).
fn law_params(law: &str, option: &str) -> BTreeMap<String, LawParam> {
    use akantu::cohesive::SolidMechanicsModelCohesive;
    let mut out = BTreeMap::new();
    for dim in [2, 3] {
        let mut model =
            SolidMechanicsModelCohesive::new(read_manual_mesh(dim), None, "smmc").unwrap();
        let i = model
            .smm
            .register_new_material("m", law, option)
            .unwrap_or_else(|e| panic!("{law} [{option}] ({dim}-D): {e}"));
        let m = model.smm.material(i).unwrap();
        for n in m.param_names() {
            let info = m.param_info(n).unwrap();
            out.entry(n.to_string()).or_insert(LawParam {
                kind: info.kind,
                access: info.access,
                value: m.param_print_value(n),
            });
        }
    }
    out
}

/// The default of a parameter that the law registers as parsable only
/// (upstream `_pat_parsable`: not readable through the registry), read
/// from the fields of a newly constructed law (3-D; the defaults do not
/// depend on the dimension).
fn parse_only_default(law: &str, option: &str, key: &str) -> Option<akantu::parser::ParamValue> {
    use akantu::model::constitutive_law::LawSpec;
    use akantu::parser::ParamValue::{Bool, Real};
    use akantu::solid::materials::anisotropic_damage::MaterialAnisotropicDamage;
    use akantu::solid::materials::marigo::MaterialMarigo;
    use akantu::solid::materials::mazars::{
        MaterialMazars, MaterialMazarsElastic, MaterialVonMisesMazars,
    };
    use akantu::solid::materials::non_local::{
        MaterialMarigoNonLocal, MaterialMazarsNonLocalElastic, MaterialVonMisesMazarsNonLocal,
    };
    use akantu::solid::materials::phasefield::MaterialPhaseField;
    let spec = LawSpec {
        spatial_dimension: 3,
        option: option.to_string(),
        id: "m".into(),
    };
    fn marigo(m: &MaterialMarigo<3>, key: &str) -> Option<akantu::parser::ParamValue> {
        match key {
            "epsilon_c" => Some(Real(m.epsilon_c)),
            "damage_in_y" => Some(Bool(m.damage_in_y)),
            "Yd" => Some(Real(m.yd.parameter_value())),
            _ => None,
        }
    }
    fn mazars<P>(m: &MaterialMazars<3, P>, key: &str) -> Option<akantu::parser::ParamValue> {
        let p = &m.params;
        match key {
            "K0" => Some(Real(m.k0.parameter_value())),
            "At" => Some(Real(p.at)),
            "Bt" => Some(Real(p.bt)),
            "Ac" => Some(Real(p.ac)),
            "Bc" => Some(Real(p.bc)),
            "beta" => Some(Real(p.beta)),
            _ => None,
        }
    }
    match law {
        "marigo" => marigo(&MaterialMarigo::<3>::new(&spec, None).ok()?, key),
        "marigo_non_local" => marigo(
            &MaterialMarigoNonLocal::<3>::new(&spec, None).ok()?.inner,
            key,
        ),
        "mazars" => mazars(&MaterialMazarsElastic::<3>::new(&spec, None).ok()?, key),
        "plastic_mazars" => mazars(&MaterialVonMisesMazars::<3>::new(&spec, None).ok()?, key),
        "mazars_non_local" => mazars(
            &MaterialMazarsNonLocalElastic::<3>::new(&spec, None)
                .ok()?
                .inner,
            key,
        ),
        "plastic_mazars_non_local" => mazars(
            &MaterialVonMisesMazarsNonLocal::<3>::new(&spec, None)
                .ok()?
                .inner,
            key,
        ),
        "anisotropic_damage" => {
            let m = MaterialAnisotropicDamage::<3>::new(&spec, None).ok()?;
            match key {
                "Dc" => Some(Real(m.dc)),
                "a" => Some(Real(m.a)),
                "A" => Some(Real(m.big_a)),
                "K0" => Some(Real(m.k0)),
                "k" => Some(Real(m.k)),
                _ => None,
            }
        }
        "phasefield" => match key {
            "eta" => Some(Real(MaterialPhaseField::<3>::new(&spec, None).ok()?.eta)),
            _ => None,
        },
        "cohesive_exponential" => {
            use akantu::cohesive::SolidMechanicsModelCohesive;
            use akantu::cohesive::laws::CohesiveLaw;
            let mut model =
                SolidMechanicsModelCohesive::new(read_manual_mesh(3), None, "smmc").ok()?;
            model.smm.register_new_material("m", law, option).ok()?;
            let CohesiveLaw::Exponential(e) = &model.cohesive_material("m").ok()?.law else {
                return None;
            };
            match key {
                "beta" => Some(Real(e.beta)),
                "exponential_penalty" => Some(Bool(e.exp_penalty)),
                "contact_tangent" => Some(Real(e.contact_tangent)),
                _ => None,
            }
        }
        _ => None,
    }
}

/// Does the Default cell of a keyword table state `value`? Numbers are
/// compared exactly after parsing (`0.000293` and `2.93e-4` are the same
/// value), a random parameter by its base value.
fn default_matches(cell: &str, value: &akantu::parser::ParamValue) -> bool {
    use akantu::parser::ParamValue;
    let s = unquote(cell);
    let real = |s: &str| s.parse::<f64>().ok();
    match value {
        ParamValue::Real(x) => real(s) == Some(*x),
        ParamValue::Random(r) => real(s) == Some(r.base_value),
        ParamValue::Int(i) => s.parse::<i64>().ok() == Some(*i),
        ParamValue::Bool(b) => s == if *b { "true" } else { "false" },
        ParamValue::VectorReal(v) => {
            let Some(inner) = s.strip_prefix('[').and_then(|r| r.strip_suffix(']')) else {
                return false;
            };
            let parsed: Option<Vec<f64>> = if inner.trim().is_empty() {
                Some(Vec::new())
            } else {
                inner.split(',').map(|x| real(x.trim())).collect()
            };
            parsed.as_deref() == Some(v.as_slice())
        }
        ParamValue::String(t) => s == t,
        _ => false,
    }
}

/// The violations of one keyword table (`marker`) against the law.
fn keyword_table_violations(marker: &LawMarker) -> Vec<String> {
    let mut bad = Vec::new();
    let place = &marker.place;
    let option = marker.attrs.get("option").map_or("", String::as_str);
    let inherits = marker.attrs.get("inherits").map_or("base", String::as_str);
    let rows: Vec<&String> = marker
        .after
        .iter()
        .skip_while(|l| l.trim().is_empty())
        .take_while(|l| l.trim_start().starts_with('|'))
        .collect();
    if rows.len() < 2 || table_cells(rows[0]) != ["Keyword", "Meaning", "Unit", "Default"] {
        return vec![format!(
            "{place}: law-keywords marker not followed by a | Keyword | Meaning | Unit | Default | table"
        )];
    }
    let params = law_params(&marker.law, option);
    let mut listed = BTreeSet::new();
    for row in &rows[2..] {
        let cells = table_cells(row);
        if cells.len() != 4 {
            bad.push(format!("{place}: row with {} cells: {row}", cells.len()));
            continue;
        }
        let key = unquote(&cells[0]).to_string();
        if !listed.insert(key.clone()) {
            bad.push(format!("{place}: {key} listed twice"));
        }
        let Some(p) = params.get(&key) else {
            bad.push(format!(
                "{place}: `{key}` is not a keyword of {} [{option}]",
                marker.law
            ));
            continue;
        };
        if !p.access.contains(ParameterAccess::PARSABLE) {
            bad.push(format!(
                "{place}: `{key}` of {} is not parsable",
                marker.law
            ));
        }
        let value = if p.access.contains(ParameterAccess::READABLE) {
            p.value.clone()
        } else {
            parse_only_default(&marker.law, option, &key)
        };
        match value {
            Some(v) if default_matches(&cells[3], &v) => {}
            Some(v) => bad.push(format!(
                "{place}: default of `{key}` ({:?}) is {v:?}, the table says {}",
                p.kind, cells[3]
            )),
            None => bad.push(format!(
                "{place}: no way to read the default of `{key}` of {} (add it to parse_only_default)",
                marker.law
            )),
        }
    }
    // `inherits=<law>`: the keywords of that law are not listed again
    let parent: BTreeSet<String> = match inherits {
        "base" | "elastic" => BTreeSet::new(),
        law => law_params(law, "").into_keys().collect(),
    };
    // completeness: every parsable keyword of the law that is not common
    for (key, p) in &params {
        let common = BASE_KEYWORDS.contains(&key.as_str())
            || (inherits == "elastic" && ELASTIC_KEYWORDS.contains(&key.as_str()))
            || parent.contains(key);
        if p.access.contains(ParameterAccess::PARSABLE) && !common && !listed.contains(key) {
            bad.push(format!(
                "{place}: parsable keyword `{key}` of {} [{option}] missing from the table",
                marker.law
            ));
        }
    }
    if inherits == "elastic" && !params.contains_key("E") {
        bad.push(format!("{place}: {} does not inherit elastic", marker.law));
    }
    bad
}

/// DOC-010 / DOC-023: the keyword tables of the laws
/// catalogue and of the cohesive chapter agree with the laws: every
/// keyword is registered and parsable, every parsable keyword is listed
/// (apart from the common ones), every default is the law's value; and
/// every law of the solid and cohesive factories (but the elastic family,
/// whose tables are generated) has a table.
#[test]
fn law_keyword_tables_match_the_registered_parameters() {
    // the unoptimised build of many cohesive models needs more than the
    // default 2 MiB stack of a test thread
    std::thread::Builder::new()
        .stack_size(64 << 20)
        .spawn(check_law_keyword_tables)
        .unwrap()
        .join()
        .unwrap();
}

fn check_law_keyword_tables() {
    use akantu::cohesive::SolidMechanicsModelCohesive;
    let markers = law_markers("law-keywords");
    let mut bad: Vec<String> = markers.iter().flat_map(keyword_table_violations).collect();
    let documented: BTreeSet<&str> = markers.iter().map(|m| m.law.as_str()).collect();
    let model = SolidMechanicsModelCohesive::new(read_manual_mesh(2), None, "smmc").unwrap();
    for law in model.smm.physics.factory.names() {
        if !ELASTIC_FAMILY.contains(&law) && !documented.contains(law) {
            bad.push(format!(
                "registered law {law} has no keyword table in the manual"
            ));
        }
    }
    assert!(bad.is_empty(), "keyword tables:\n{}", bad.join("\n"));
    assert!(
        markers.len() >= 20,
        "only {} keyword tables found",
        markers.len()
    );

    // the check catches a keyword the law does not register, a wrong
    // default and a missing keyword
    let m = markers
        .iter()
        .find(|m| m.law == "marigo")
        .expect("marigo table");
    for (what, edit) in [
        ("unregistered", ("| `Sd` |", "| `Sd_max` |")),
        ("wrong default", ("| `5000` |", "| `500` |")),
        ("missing", ("| `epsilon_c` |", "| `Sd` |")),
    ] {
        let after = m.after.iter().map(|l| l.replace(edit.0, edit.1)).collect();
        let mutated = LawMarker {
            place: m.place.clone(),
            law: m.law.clone(),
            attrs: m.attrs.clone(),
            after,
        };
        assert!(
            !keyword_table_violations(&mutated).is_empty(),
            "mutation `{what}` not detected"
        );
    }
}

/// A minimal valid section of every law whose internal fields the
/// manual lists (the defaults of some laws are refused by `init_full`).
fn sample_law_section(law: &str, option: &str) -> String {
    let opt = if option.is_empty() {
        String::new()
    } else {
        format!(" {option}")
    };
    let extra = match law {
        "sls_deviatoric" => " E = 3\n nu = 0.3\n Ev = 1\n Eta = 1\n",
        "viscoelastic_maxwell" => " nu = 0.3\n Ev = [1, 2]\n Eta = [1, 2]\n",
        _ => " E = 1\n nu = 0.3\n",
    };
    // a non-local law needs its neighbourhood (named after the material)
    let non_local = if law.ends_with("_non_local") {
        "non_local m base_wf [\n radius = 0.3\n]\n"
    } else {
        ""
    };
    format!("material {law}{opt} [\n name = m\n rho = 1\n{extra}]\n{non_local}")
}

/// DOC-010: the internal fields that the catalogue lists for
/// each law (the code spans of the paragraph after `<!-- law-internals:
/// law -->`) are the law's internal fields after `init_full`, apart from
/// the common ones.
#[test]
fn law_internal_lists_match_the_laws() {
    use akantu::model::ModelOptions;
    use akantu::solid::SolidMechanicsModel;
    let markers = law_markers("law-internals");
    let mut bad = Vec::new();
    for m in &markers {
        let option = m.attrs.get("option").map_or("", String::as_str);
        let paragraph: String = m
            .after
            .iter()
            .skip_while(|l| l.trim().is_empty())
            .take_while(|l| !l.trim().is_empty())
            .cloned()
            .collect::<Vec<_>>()
            .join(" ");
        let listed: BTreeSet<String> = code_spans(&paragraph).into_iter().collect();
        for dim in [2, 3] {
            let parser = Parser::parse_str(&sample_law_section(&m.law, option), "m.dat").unwrap();
            let mut model =
                SolidMechanicsModel::with_parser(read_manual_mesh(dim), None, "smm", parser)
                    .unwrap();
            model
                .init_full(&ModelOptions::new(AnalysisMethod::Static))
                .unwrap_or_else(|e| panic!("{}: init_full of {}: {e}", m.place, m.law));
            let all = model.material(0).unwrap().internal_names_sorted();
            // The fields the page says every law
            // stores are required, not only filtered out
            let required: &[&str] = if dim == 2 {
                &[
                    "stress",
                    "grad_u",
                    "eigen_grad_u",
                    "potential_energy",
                    "third_axis_deformation",
                ]
            } else {
                &["stress", "grad_u", "eigen_grad_u", "potential_energy"]
            };
            for r in required {
                if !all.iter().any(|n| n == r) {
                    bad.push(format!(
                        "{} ({dim}-D): {} has no internal `{r}` (the page says every law stores it)",
                        m.place, m.law
                    ));
                }
            }
            let have: BTreeSet<String> = all
                .into_iter()
                .filter(|n| !COMMON_INTERNALS.contains(&n.as_str()))
                .collect();
            if have != listed {
                bad.push(format!(
                    "{} ({dim}-D): {} has {have:?}, the manual lists {listed:?}",
                    m.place, m.law
                ));
            }
        }
    }
    assert!(
        markers.len() >= 12,
        "only {} internal lists found",
        markers.len()
    );
    assert!(bad.is_empty(), "internal lists:\n{}", bad.join("\n"));
}

/// DOC-010: every `material <law> [ … ]` section of the
/// manual's input-file snippets (top level, and inside `model` sections)
/// names a registered law and only keywords that the law registers as
/// parsable — the snippets cannot teach a keyword the code rejects.
#[test]
fn manual_material_snippets_use_registered_keywords() {
    use akantu::cohesive::SolidMechanicsModelCohesive;
    use akantu::parser::ParserSection;
    fn materials<'a>(s: &'a ParserSection, out: &mut Vec<&'a ParserSection>) {
        for sub in &s.sub_sections {
            if sub.section_type == ParserType::Material {
                out.push(sub);
            } else {
                materials(sub, out);
            }
        }
    }
    let registered: BTreeSet<String> =
        SolidMechanicsModelCohesive::new(read_manual_mesh(2), None, "smmc")
            .unwrap()
            .smm
            .physics
            .factory
            .names()
            .map(String::from)
            .collect();
    let mut checked = 0;
    let mut bad = Vec::new();
    for file in markdown_files() {
        for f in fences(&read(&file)) {
            let mut tags = f.info.split(',').map(str::trim);
            if tags.next() != Some("dat") || tags.any(|t| t == "fails") {
                continue;
            }
            let place = format!("{}:{}", file.display(), f.line);
            let body = expand_includes(&f.body, file.parent().unwrap());
            let parser = Parser::parse_str(&body, &place).unwrap();
            let mut sections = Vec::new();
            materials(&parser.root, &mut sections);
            for s in sections {
                if !registered.contains(&s.name) {
                    bad.push(format!("{place}: unknown law {}", s.name));
                    continue;
                }
                let params = law_params(&s.name, &s.option);
                for p in &s.parameters {
                    let key = p.name.to_string_lossy().trim().to_string();
                    match params.get(&key) {
                        Some(lp) if lp.access.contains(ParameterAccess::PARSABLE) => {}
                        _ => bad.push(format!("{place}: `{key}` is not a keyword of {}", s.name)),
                    }
                }
                checked += 1;
            }
        }
    }
    // sanity check of the scanner (the manual had 7 such sections when it was written)
    assert!(checked >= 7, "only {checked} material sections checked");
    assert!(bad.is_empty(), "material snippets:\n{}", bad.join("\n"));
}

/// Every `non_local <material> <weight function> [ … ]`
/// section of the manual's input-file snippets names a registered weight
/// function, and its keywords — `radius` and those of the
/// `weight_function weight_parameter [ … ]` sub-section (`update_rate`,
/// `damage_limit`, …) — are parsed by the port's own neighbourhood and
/// weight function with upstream's non-permissive `parseSection` (an
/// unknown keyword, e.g. `radiuss`, is an error).
#[test]
fn manual_non_local_snippets_use_registered_keywords() {
    use akantu::model::non_local::{NonLocalNeighborhood, WeightFunctionFactory};
    use akantu::parser::ParserSection;
    use akantu::parser::param::Parsable;
    fn non_locals<'a>(s: &'a ParserSection, out: &mut Vec<&'a ParserSection>) {
        for sub in &s.sub_sections {
            if sub.section_type == ParserType::NonLocal {
                out.push(sub);
            } else {
                non_locals(sub, out);
            }
        }
    }
    let mut factory = WeightFunctionFactory::default();
    akantu::solid::materials::non_local::register_weight_functions(&mut factory);
    let mut checked = 0;
    let mut bad = Vec::new();
    for file in markdown_files() {
        for f in fences(&read(&file)) {
            let mut tags = f.info.split(',').map(str::trim);
            if tags.next() != Some("dat") || tags.any(|t| t == "fails") {
                continue;
            }
            let place = format!("{}:{}", file.display(), f.line);
            let body = expand_includes(&f.body, file.parent().unwrap());
            let parser = Parser::parse_str(&body, &place).unwrap();
            let mut sections = Vec::new();
            non_locals(&parser.root, &mut sections);
            for s in sections {
                match factory.create(&s.option) {
                    Ok(w) => {
                        let mut n = NonLocalNeighborhood::new(&s.name, "manual", 2, w);
                        if let Err(e) = n.parse_section(s, false) {
                            bad.push(format!("{place}: non_local {} {}: {e}", s.name, s.option));
                        }
                    }
                    Err(e) => bad.push(format!("{place}: non_local {}: {e}", s.name)),
                }
                checked += 1;
            }
        }
    }
    // sanity check of the scanner (the non-local page's section)
    assert!(checked >= 1, "no non_local section checked");
    assert!(bad.is_empty(), "non_local snippets:\n{}", bad.join("\n"));
}

/* ---------------------------------------------------------------------- */
/* Availability guard                                                      */
/* ---------------------------------------------------------------------- */

/// Every availability statement of the manual and of the README agrees
/// with the single table `docs/manual/availability.md`, which agrees with
/// the workspace (rules: the top of that file and
/// `tests/availability/mod.rs`): table rows vs facade modules, placeholder
/// crates, facade and crate features, pages and pinned tests; page
/// declarations; `SUMMARY.md`; the generated availability sections
/// (rewritten by `AKANTU_BLESS_MANUAL=1`); every sentence, list item,
/// heading and table row that says something is or is not available.
#[test]
fn manual_availability_matches_the_workspace() {
    let table = read(&availability::table_path());
    let ws = availability::Workspace::load();
    assert!(ws.modules.len() >= 20, "facade modules: {:?}", ws.modules);
    // sanity: the detector sees implemented crates (no placeholder crate is
    // left since the non-local, IGFEM and couplers merges of 2026-11-12, so
    // "both kinds" can no longer be required; the placeholder detection is
    // exercised by the negative cases on a mutated workspace)
    assert!(ws.placeholder.values().any(|p| !*p));
    let mut docs = availability::load_docs();
    if std::env::var_os("AKANTU_BLESS_MANUAL").is_some() {
        let rows = availability::parse_table(&table).expect("availability table");
        let snapshot = docs.clone();
        for doc in &mut docs {
            let (new, _) = availability::regenerate(&rows, &snapshot, doc);
            if new != doc.text {
                let path = if doc.readme {
                    availability::repo_root().join("README.md")
                } else {
                    src_dir().join(&doc.path)
                };
                std::fs::write(path, &new).unwrap();
                doc.text = new;
            }
        }
    }
    let v = availability::check_all(&table, &ws, &docs);
    assert!(
        v.is_empty(),
        "{} availability problem(s):\n{}",
        v.len(),
        v.join("\n")
    );
}

fn append_to(docs: &mut [availability::Doc], path: &str, text: &str) {
    let d = docs
        .iter_mut()
        .find(|d| d.path == path)
        .unwrap_or_else(|| panic!("no document {path}"));
    d.text.push_str("\n\n");
    d.text.push_str(text);
    d.text.push('\n');
}

fn replace_in(docs: &mut [availability::Doc], path: &str, old: &str, new: &str) {
    let d = docs
        .iter_mut()
        .find(|d| d.path == path)
        .unwrap_or_else(|| panic!("no document {path}"));
    assert!(d.text.contains(old), "{path}: no {old:?}");
    d.text = d.text.replacen(old, new, 1);
}

type Mutation = Box<dyn Fn(&mut String, &mut availability::Workspace, &mut Vec<availability::Doc>)>;

fn doc_case(path: &'static str, text: &'static str) -> Mutation {
    Box::new(move |_, _, docs| append_to(docs, path, text))
}

/// Negative cases of the guard: each mutation alone must make
/// [`manual_availability_matches_the_workspace`]'s check fail. W1–W7 are the
/// wording tricks that once passed the guard, M1–M7
/// stale statements of an earlier merge, A/R mutations of declarations,
/// crates and the waiting
/// list, S… synonyms and further tricks, T… mutations of the table and of
/// the workspace. The positive cases must pass (no false alarm on correct
/// statements of each kind).
#[test]
fn manual_availability_guard_negative_cases() {
    let table = read(&availability::table_path());
    let ws = availability::Workspace::load();
    let docs = availability::load_docs();
    assert!(
        availability::check_all(&table, &ws, &docs).is_empty(),
        "baseline must pass (see manual_availability_matches_the_workspace)"
    );
    let cases: Vec<(&str, Mutation)> = vec![
        // The seven wording tricks
        (
            "W1 own page",
            doc_case(
                "geometry.md",
                "`akantu::geometry` is being ported and not part of this release.",
            ),
        ),
        (
            "W2 other phrasing",
            doc_case(
                "materials.md",
                "The phase-field model (`akantu::phase_field`) is not available in this release.",
            ),
        ),
        (
            "W3 model name, no path",
            doc_case("meshes.md", "The cohesive model is not yet available."),
        ),
        (
            "W4 item path",
            doc_case(
                "solid-mechanics.md",
                "`akantu::phase_field::PhaseFieldModel` is being ported.",
            ),
        ),
        (
            "W5 feature name",
            doc_case(
                "getting-started.md",
                "The feature `phase-field` is a placeholder in this release.",
            ),
        ),
        (
            "W6 link only",
            doc_case(
                "introduction.md",
                "[Phase-field fracture](models/phase-field.md) is not yet available.",
            ),
        ),
        (
            "W7 README",
            doc_case("README.md", "Not yet available: the phase-field model."),
        ),
        // The stale statements left by the merge with main
        (
            "M1 banner on the model's own page",
            doc_case(
                "models/contact.md",
                "**Not yet available in this release.**",
            ),
        ),
        // The cohesive model runs on distributed meshes now
        (
            "M1c stale distributed cohesive",
            doc_case(
                "models/cohesive.md",
                "The cohesive model on a distributed mesh is not yet available.",
            ),
        ),
        (
            "M1b placeholder on the own page",
            doc_case(
                "models/contact.md",
                "`akantu::contact` is a placeholder in this release.",
            ),
        ),
        (
            "M2 placeholder feature list",
            doc_case(
                "getting-started.md",
                "The features `contact`, `diffusion` and `couplers` exist, but the crates behind them are placeholders.",
            ),
        ),
        (
            "M3 elliptic model name",
            doc_case(
                "models/couplers.md",
                "The cohesive and phase-field models are available; the contact model is not yet available ([contact](contact.md)).",
            ),
        ),
        (
            "M4 upstream-changes row",
            doc_case(
                "upstream-changes.md",
                "| 5.0.3 | contact mechanics fixes | not yet available (contact model: [waiting list](models/not-yet-available.md)) |",
            ),
        ),
        (
            "M4b branch remark",
            doc_case(
                "upstream-changes.md",
                "| — | contact friction | contact mechanics is on the branch `feat/contact` |",
            ),
        ),
        (
            "M5 README list",
            doc_case(
                "README.md",
                "Not yet available: the contact, diffusion and structural models.",
            ),
        ),
        (
            "M5b enumeration with an unavailable item",
            doc_case(
                "README.md",
                "Not yet available: PETSc and the contact, diffusion and structural models.",
            ),
        ),
        (
            "M6 SUMMARY part",
            Box::new(|_, _, docs| {
                replace_in(
                    docs,
                    "SUMMARY.md",
                    "- [Overview](models/not-yet-available.md)\n",
                    "- [Overview](models/not-yet-available.md)\n  - [Contact mechanics](models/contact.md)\n",
                )
            }),
        ),
        (
            "M7 own page unnamed",
            doc_case("models/structural.md", "This model is still being ported."),
        ),
        // A/R (documentation and crate mutations)
        (
            "A1 stale sentence",
            doc_case("meshes.md", "`akantu::cohesive` is not yet available."),
        ),
        (
            "A2 wrong declaration",
            Box::new(|_, _, docs| {
                replace_in(
                    docs,
                    "models/couplers.md",
                    "<!-- availability: available akantu::couplers -->",
                    "<!-- availability: not-yet-available akantu::couplers -->",
                )
            }),
        ),
        (
            "A3 declaration removed",
            Box::new(|_, _, docs| {
                replace_in(
                    docs,
                    "models/phase-field.md",
                    "<!-- availability: available akantu::phase_field -->\n",
                    "",
                )
            }),
        ),
        (
            // no placeholder crate is left in the workspace: the same defect
            // is a row that still says "not yet available" for a crate that
            // holds its implementation
            "A4 placeholder gains a module",
            Box::new(|t, _, _| {
                *t = t.replacen(
                    "| available | `akantu::couplers` | `akantu-couplers` | `couplers` | `contactmechanicsmodel.rst`, `phasefieldmodel.rst` | — |",
                    "| not-yet-available | `akantu::couplers` | `akantu-couplers` | `couplers` | `contactmechanicsmodel.rst`, `phasefieldmodel.rst` | WP-CPL-FIX |",
                    1,
                )
            }),
        ),
        (
            "A5 waiting-list row",
            doc_case(
                "models/not-yet-available.md",
                "| Cohesive elements | — | `akantu::cohesive` | being ported |",
            ),
        ),
        (
            "A6 being ported",
            doc_case(
                "materials.md",
                "The `phasefield` laws of `akantu::phase_field` are being ported.",
            ),
        ),
        (
            "R1 crate becomes a placeholder",
            Box::new(|_, ws, _| {
                ws.placeholder.insert("akantu-ntn-contact".into(), true);
            }),
        ),
        (
            "R2 available module called being ported",
            doc_case(
                "parallel.md",
                "Distributed geometric queries (`akantu::geometry`) are being ported.",
            ),
        ),
        // further wording tricks and synonyms
        (
            "S1 unavailable",
            doc_case("output.md", "The structural model is unavailable."),
        ),
        (
            "S2 stub",
            doc_case("output.md", "`akantu::structural` is a stub."),
        ),
        (
            "S3 crate name",
            doc_case("output.md", "`akantu-contact` is not part of this release."),
        ),
        (
            "S4 not shipped",
            doc_case(
                "output.md",
                "The split-node contact is not shipped with the default build of this release.",
            ),
        ),
        (
            "S5 not merged yet",
            doc_case(
                "output.md",
                "The contact mechanics model has not been merged yet.",
            ),
        ),
        (
            "S6 absent from the release",
            doc_case(
                "output.md",
                "`ContactMechanicsModel` is absent from this release.",
            ),
        ),
        (
            "S7 later release",
            doc_case(
                "output.md",
                "The phase-field fracture model comes in a later release.",
            ),
        ),
        (
            "S8 planned",
            doc_case("output.md", "The cohesive model is planned."),
        ),
        (
            "S9 on a branch",
            doc_case(
                "output.md",
                "The structural mechanics model lives on another branch.",
            ),
        ),
        (
            "S10 work package",
            doc_case("output.md", "The geometry module waits for WP-GEOM."),
        ),
        (
            "S11 heading",
            doc_case("output.md", "## The cohesive model (not yet available)"),
        ),
        (
            "S12 list item",
            doc_case("output.md", "* the cohesive model: not yet available"),
        ),
        (
            "S13 cannot yet",
            doc_case("output.md", "The cohesive model cannot be used yet."),
        ),
        (
            "S14 unlisted subject",
            doc_case("materials.md", "The frobnicator is not yet available."),
        ),
        (
            "S15 positive claim of a placeholder",
            doc_case("materials.md", "The C API is available in this release."),
        ),
        (
            "S16 wrong work package",
            doc_case("materials.md", "The C API waits for WP-FOO."),
        ),
        (
            "S17 opt-in called unported",
            doc_case("mumps.md", "MUMPS is not part of this release."),
        ),
        (
            "S18 mixed sentence",
            doc_case(
                "materials.md",
                "The phase-field model and the C API are not yet available.",
            ),
        ),
        (
            "S19 table cells split",
            doc_case("materials.md", "| `akantu::cohesive` | being ported |"),
        ),
        // table and workspace mutations
        (
            "T1 wrong status",
            Box::new(|t, _, _| {
                *t = t.replacen("| contact | the contact mechanics model `ContactMechanicsModel` (one process): detection, penalty resolutions with Coulomb friction | available |", "| contact | the contact mechanics model `ContactMechanicsModel` (one process): detection, penalty resolutions with Coulomb friction | not-yet-available |", 1)
            }),
        ),
        (
            "T2 module without a row",
            Box::new(|t, _, _| {
                *t = t
                    .lines()
                    .filter(|l| !l.starts_with("| structural |"))
                    .collect::<Vec<_>>()
                    .join("\n");
            }),
        ),
        (
            "T3 waiting for missing",
            // the "waiting for" cell of the parallel-structural row
            // (the former anchor of the Python bindings left the table with
            // them; the tutorials row's cell left it with
            // D-085; the python-rest row left with the
            // Python subclasses of the diffusion and phase-field laws, PY-083
            // and PY-092 *Verified*)
            Box::new(|t, _, _| {
                let cell = "| not planned: upstream has no distributed support for it (UD-902); \
                            the port refuses |";
                assert!(t.contains(cell), "T3: anchor missing from the table");
                *t = t.replacen(cell, "| — |", 1)
            }),
        ),
        (
            "T4 pinned test missing",
            Box::new(|t, _, _| {
                *t = t.replacen(
                    "manual_claims_2.rs::doc_solver_petsc_types_are_refused",
                    "manual_claims_2.rs::no_such_test",
                    1,
                )
            }),
        ),
        (
            "T5 table changed, sections not regenerated",
            Box::new(|t, _, _| {
                *t = t.replacen(
                    "| `akantu-capi` | — | — | not planned for now: upstream has none (approved exception, D-100) |",
                    "| `akantu-capi` | — | — | planned for a later release |",
                    1,
                )
            }),
        ),
        (
            "T6 opt-in becomes a default feature",
            Box::new(|_, ws, _| {
                ws.features
                    .get_mut("akantu")
                    .unwrap()
                    .1
                    .insert("ntn-contact".into());
            }),
        ),
        (
            "T7 new facade feature",
            Box::new(|_, ws, _| {
                ws.features
                    .get_mut("akantu")
                    .unwrap()
                    .0
                    .insert("petsc".into());
            }),
        ),
        (
            "T8 new facade module",
            Box::new(|_, ws, _| {
                ws.modules.insert(
                    "python".into(),
                    ("akantu-python".into(), Some("python".into())),
                );
            }),
        ),
        (
            "T9 status block removed",
            Box::new(|_, _, docs| {
                let d = docs
                    .iter_mut()
                    .find(|d| d.path == "models/diffusion.md")
                    .unwrap();
                let b = d
                    .text
                    .find("<!-- BEGIN GENERATED: availability-status diffusion -->")
                    .unwrap();
                let e = d
                    .text
                    .find("<!-- END GENERATED: availability-status diffusion -->")
                    .unwrap();
                d.text.replace_range(
                    b..e + "<!-- END GENERATED: availability-status diffusion -->".len(),
                    "",
                );
            }),
        ),
        (
            "T10 stale generated text edited by hand",
            Box::new(|_, _, docs| {
                // the rotary-inertia package left the generated table when it
                // merged (and the DOC-010 / DOC-023 rows, the
                // external tutorials, D-085; the rest of the
                // Python interface); the waiting-for text
                // of the structural model on a distributed mesh remains
                replace_in(
                    docs,
                    "models/not-yet-available.md",
                    "upstream has no distributed support for it (UD-902)",
                    "upstream has no distributed support for it (UD-903)",
                )
            }),
        ),
    ];
    let mut report = Vec::new();
    for (name, mutate) in &cases {
        let (mut t, mut w, mut d) = (table.clone(), ws.clone(), docs.clone());
        mutate(&mut t, &mut w, &mut d);
        let v = availability::check_all(&t, &w, &d);
        assert!(!v.is_empty(), "negative case {name} passes the guard");
        report.push(format!("{name}: {}", v[0]));
    }
    println!("{}", report.join("\n"));

    // positive cases: correct statements of each kind pass
    let positives: Vec<(&str, Mutation)> = vec![
        (
            "P1 sub-feature on an available page",
            doc_case(
                "models/contact.md",
                "Contact mechanics on a distributed mesh is not part of this release.",
            ),
        ),
        (
            "P2 opt-in about the build",
            doc_case(
                "mumps.md",
                "Without the feature `mumps`, MUMPS is not available in this build.",
            ),
        ),
        (
            "P3 placeholder with its work package",
            doc_case(
                "materials.md",
                "Contact mechanics on a distributed mesh is not yet available (parity row CONT-019).",
            ),
        ),
        (
            "P4 stub page unnamed",
            doc_case("models/not-yet-available.md", "It is still being ported."),
        ),
        (
            "P5 positive claim",
            doc_case(
                "materials.md",
                "The contact mechanics model is available in this release.",
            ),
        ),
        (
            "P6 not ported by design",
            doc_case(
                "appendix/conventions.md",
                "The Emacs input mode is not ported.",
            ),
        ),
        (
            // was "the rest of the cohesive chapter" (DOC-023) until the keyword tables were added
            // wrote that chapter, then the fragment manager on a distributed
            // mesh (merged), then split-node contact on a
            // distributed mesh (available). No
            // row that stays not yet available names a work package any
            // more, so the case is a sub-feature with its reference on an
            // available page: the structural model on a distributed mesh
            // (row `parallel-structural`, UD-902)
            "P7 sub-feature with its reference on an available page",
            doc_case(
                "models/structural.md",
                "The structural model on a distributed mesh is not yet available (UD-902).",
            ),
        ),
        (
            "P8 enumeration of placeholders",
            doc_case(
                "materials.md",
                "The C API and the structural model on a distributed mesh are not yet available.",
            ),
        ),
        (
            "P9 opt-in PETSc about the build",
            doc_case(
                "petsc.md",
                "Without the feature `petsc`, the PETSc back-end is not compiled in this build.",
            ),
        ),
    ];
    for (name, mutate) in &positives {
        let (mut t, mut w, mut d) = (table.clone(), ws.clone(), docs.clone());
        mutate(&mut t, &mut w, &mut d);
        let v = availability::check_all(&t, &w, &d);
        assert!(
            v.is_empty(),
            "positive case {name} fails the guard:\n{}",
            v.join("\n")
        );
    }
}

/// The Energies table of
/// `constitutive-laws.md` against **driven** laws. Every law of the law
/// catalogue (the `law-internals` markers) and `elastic` is built as the
/// other catalogue tests do, on the 2-D manual mesh, with every DOF
/// prescribed: a uniaxial strain `ε_yy` ramped to 2e-2 in 20 explicit
/// steps (`Δt = 0.1`) and back to 0 in 20 more, with thresholds low enough
/// that the inelastic laws damage, yield or creep (the law's own keywords
/// below); the phase-field solid law, whose damage comes from the coupled
/// phase field, is given a damage of 0.5 at every point instead. Then:
/// `potential` is positive at the peak for every law ("every law"), and
/// after the cycle each of `dissipated`, `plastic` and `work` is positive
/// exactly for the laws the table names for it and zero for every other
/// law (a law that does not define an id answers 0, upstream's
/// `Material::getEnergy`). The table is parsed from the page (code spans of
/// the Laws column; "the three non-local laws" are the three
/// `*_non_local` laws of the catalogue).
#[test]
fn energies_table_matches_the_driven_laws() {
    use akantu::model::ModelOptions;
    use akantu::solid::SolidMechanicsModel;
    let page = read(&manual_dir().join("src/constitutive-laws.md"));
    let section: Vec<&str> = page
        .lines()
        .skip_while(|l| l.trim() != "## Energies")
        .skip(1)
        .take_while(|l| !l.starts_with("## "))
        .collect();
    let mut laws: Vec<String> = law_markers("law-internals")
        .into_iter()
        .map(|m| m.law)
        .collect();
    laws.push("elastic".into());
    laws.sort();
    laws.dedup();
    let non_local: Vec<String> = laws
        .iter()
        .filter(|l| l.ends_with("_non_local"))
        .cloned()
        .collect();
    assert_eq!(non_local.len(), 3, "{non_local:?}");
    let mut table: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for row in section.iter().filter(|l| l.starts_with("| `")) {
        let cells: Vec<&str> = row.split('|').map(str::trim).collect();
        let id = code_spans(cells[1]).remove(0);
        let mut named: BTreeSet<String> = code_spans(cells[2]).into_iter().collect();
        if cells[2].contains("every law") {
            named.extend(laws.iter().cloned());
        }
        if cells[2].contains("the three non-local laws") {
            named.extend(non_local.iter().cloned());
        }
        for l in &named {
            assert!(
                laws.contains(l),
                "the Energies table names {l}, not in the catalogue"
            );
        }
        table.insert(id, named);
    }
    let ids: Vec<String> = table.keys().cloned().collect();
    assert_eq!(ids, ["dissipated", "plastic", "potential", "work"]);
    // the driving keywords of each law (low thresholds, E = 1 otherwise)
    let drive = |law: &str| -> &'static str {
        match law.trim_end_matches("_non_local") {
            "marigo" => " Yd = 1e-6\n Sd = 1\n",
            "mazars" => " K0 = 1e-4\n",
            "plastic_mazars" => " K0 = 1e-4\n sigma_y = 1e-3\n h = 0.1\n",
            "plastic_linear_isotropic_hardening" => " sigma_y = 1e-3\n h = 0.1\n",
            "plastic_drucker_prager" => " phi = 30\n fc = 1e-3\n",
            _ => "",
        }
    };
    let mut bad = Vec::new();
    for law in &laws {
        let base = sample_law_section(law, "");
        let text = base.replacen(" rho = 1\n", &format!(" rho = 1\n{}", drive(law)), 1);
        let parser = Parser::parse_str(&text, "energies.dat").unwrap();
        let mut model =
            SolidMechanicsModel::with_parser(read_manual_mesh(2), None, "smm", parser).unwrap();
        if law == "anisotropic_damage" {
            // the default is refused (UD-031 / UD-058): the approved
            // specification, opt-in
            model.physics.law_options = Some(akantu::solid::material::LawOptions {
                anisotropic_damage:
                    akantu::solid::materials::anisotropic_damage::AnisotropicDamageModel::Corrected,
                ..akantu::solid::material::LawOptions::default()
            });
        }
        model
            .init_full(&ModelOptions::new(AnalysisMethod::ExplicitLumpedMass))
            .unwrap_or_else(|e| panic!("{law}: init_full: {e}"));
        model.set_time_step(0.1, None).unwrap();
        if law == "phasefield" {
            // the damage the coupled phase field would give the solid law
            let mut done = false;
            let types: Vec<ElementType> = model_types(&model);
            let m = model.material_mut(0).unwrap();
            m.for_each_internal_mut(&mut |name, f| {
                if name == "damage"
                    && let akantu::model::constitutive_law::InternalFieldMut::Real(f) = f
                {
                    for &ty in &types {
                        if let Ok(mut a) = f.get_mut(ty, akantu::core::GhostType::NotGhost) {
                            a.as_mut_slice().fill(0.5);
                            done = true;
                        }
                    }
                }
                Ok(())
            })
            .unwrap();
            assert!(done, "phasefield: no damage internal");
        }
        let x = model
            .physics
            .base
            .mesh
            .read()
            .unwrap()
            .nodes()
            .as_slice()
            .to_vec();
        {
            let mut b = model.blocked_dofs_mut().unwrap();
            b.as_mut_slice().fill(true);
        }
        let mut peak = None;
        for s in 1..=40u32 {
            let e = 2e-2 * Real::from(if s <= 20 { s } else { 40 - s }) / 20.0;
            {
                let mut u = model.displacement_mut().unwrap();
                for (u, x) in u
                    .as_mut_slice()
                    .as_chunks_mut::<2>()
                    .0
                    .iter_mut()
                    .zip(x.as_chunks::<2>().0)
                {
                    u[0] = 0.0;
                    u[1] = e * x[1];
                }
            }
            model
                .solve_step(None)
                .unwrap_or_else(|err| panic!("{law}: step {s}: {err}"));
            if s == 20 {
                peak = Some(model.energy("potential").unwrap());
            }
        }
        let peak = peak.unwrap();
        if peak.is_nan() || peak <= 0.0 {
            bad.push(format!("{law}: potential energy {peak:e} at the peak"));
        }
        for id in ["dissipated", "plastic", "work"] {
            let v = model.energy(id).unwrap();
            let named = table[id].contains(law);
            eprintln!("{law}: {id} = {v:e} (named: {named})");
            if named && (v.is_nan() || v <= 0.0) {
                bad.push(format!(
                    "{law}: `{id}` is {v:e} after the cycle, the table names it"
                ));
            }
            if !named && v != 0.0 {
                bad.push(format!(
                    "{law}: `{id}` is {v:e}, the table does not name it"
                ));
            }
        }
    }
    assert!(bad.is_empty(), "{bad:#?}");
}

/// The not-ghost element types of the mesh dimension of `model`'s mesh.
fn model_types(model: &akantu::solid::SolidMechanicsModel) -> Vec<ElementType> {
    let mesh = model.physics.base.mesh.read().unwrap();
    let dim = mesh.spatial_dimension();
    mesh.element_types(akantu::core::TypeFilter::new())
        .filter(|t| t.spatial_dimension() == dim)
        .collect()
}
