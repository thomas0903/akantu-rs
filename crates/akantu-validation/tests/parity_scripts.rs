// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). No Akantu
// counterpart: keeps the parity-matrix tooling honest as part of
// `cargo test -p akantu-validation`:
//
// * `scripts/parity/stats.py --self-test` (its row parser refuses
//   malformed rows: a wrong cell count, an unknown status, a duplicate or
//   malformed row id);
// * `scripts/parity/check_cited_tests.py --self-test` (every brace
//   alternative and explicit crate of a cited test is checked);
// * `scripts/parity/check_cited_tests.py --source` (every Rust test the
//   Tests column of `docs/FEATURE_PARITY.md` cites exists in the source
//   tree, and every cited Python test file exists).
//
// They need `python3` (standard library only); where Python is genuinely
// unavailable, set AKANTU_VALIDATION_SKIP_PYTHON=1 to skip them explicitly
// (as `probe_scripts.rs`); otherwise a missing interpreter is a failure.
// The scripts only read the tree (`-B`: no bytecode is written, the gate's
// source mount is read-only).

use std::path::Path;
use std::process::Command;

/// Run `python3 -B <script> <args>` from the workspace root; `None` when
/// Python is unavailable and the skip is requested.
fn run(root: &Path, script: &Path, args: &[&str]) -> Option<(bool, String)> {
    let out = Command::new("python3")
        .arg("-B")
        .arg(script)
        .args(args)
        .current_dir(root)
        .output();
    match out {
        Ok(o) => Some((
            o.status.success(),
            format!(
                "{}{}",
                String::from_utf8_lossy(&o.stdout),
                String::from_utf8_lossy(&o.stderr)
            ),
        )),
        Err(e) if std::env::var("AKANTU_VALIDATION_SKIP_PYTHON").is_ok_and(|v| v == "1") => {
            eprintln!("python3 unavailable ({e}); skipped by AKANTU_VALIDATION_SKIP_PYTHON=1");
            None
        }
        Err(e) => {
            panic!("cannot run python3 ({e}); install it or set AKANTU_VALIDATION_SKIP_PYTHON=1")
        }
    }
}

/// `stats.py` refuses malformed rows (cell count, unknown status).
#[test]
fn parity_stats_row_parser_self_test() {
    let root = akantu_validation::fixture::workspace_root();
    let Some((ok, text)) = run(
        &root,
        &root.join("scripts/parity/stats.py"),
        &["--self-test"],
    ) else {
        return;
    };
    assert!(ok && text.contains("self-test: ok"), "{text}");
}

/// `check_cited_tests.py` checks every alternative of a `{a, b}` group and
/// an explicit crate, on a synthetic log and a synthetic source tree.
#[test]
fn cited_tests_checker_self_test() {
    let root = akantu_validation::fixture::workspace_root();
    let Some((ok, text)) = run(
        &root,
        &root.join("scripts/parity/check_cited_tests.py"),
        &["--self-test"],
    ) else {
        return;
    };
    assert!(ok && text.contains("self-test: ok"), "{text}");
}

/// Every test the Tests column of the parity matrix cites exists in the
/// source tree (Rust test functions, test binaries, Python test files).
#[test]
fn cited_tests_exist_in_the_source_tree() {
    let root = akantu_validation::fixture::workspace_root();
    let Some((ok, text)) = run(
        &root,
        &root.join("scripts/parity/check_cited_tests.py"),
        &["--source"],
    ) else {
        return;
    };
    assert!(
        ok,
        "docs/FEATURE_PARITY.md cites tests that do not exist:\n{text}"
    );
}
