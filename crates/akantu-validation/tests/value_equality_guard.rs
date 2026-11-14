// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). No C++
// counterpart: a guard on the differential-testing harness.

//! `CompareOptions::value_equality` turns an exact rule into a *value*
//! comparison (any NaN matches any NaN, `+0` matches `−0`). It exists for
//! `fixture-diff`, which reports bit identity separately; a differential
//! test that set it would weaken every exact rule it evaluates (D-046,
//! D-051). It is a public field (struct-update syntax in other crates needs
//! it), so this test scans every Rust source of the workspace: only the
//! comparator itself, `fixture-diff` and `verified.rs` (which sets it to
//! `false`) may name it.

use std::path::{Path, PathBuf};

use akantu_validation::fixture::workspace_root;

fn rust_sources(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        let p = e.path();
        let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if p.is_dir() {
            if name != "target" && !name.starts_with('.') {
                rust_sources(&p, out);
            }
        } else if name.ends_with(".rs") {
            out.push(p);
        }
    }
}

#[test]
fn value_equality_is_set_only_by_fixture_diff() {
    let root = workspace_root();
    let mut files = Vec::new();
    rust_sources(&root.join("crates"), &mut files);
    assert!(files.len() > 100, "only {} Rust files found", files.len());
    let allowed = [
        "crates/akantu-validation/src/compare.rs",
        "crates/akantu-validation/src/bin/fixture-diff.rs",
        "crates/akantu-validation/tests/value_equality_guard.rs",
    ];
    let mut bad = Vec::new();
    let mut seen_fixture_diff = false;
    for f in &files {
        let rel = f
            .strip_prefix(&root)
            .unwrap_or(f)
            .to_string_lossy()
            .replace('\\', "/");
        let text = std::fs::read_to_string(f).unwrap_or_default();
        for (k, line) in text.lines().enumerate() {
            if !line.contains("value_equality") {
                continue;
            }
            if rel.ends_with("bin/fixture-diff.rs") && line.contains("value_equality: true") {
                seen_fixture_diff = true;
            }
            let ok = allowed.iter().any(|a| rel == *a)
                || (rel == "crates/akantu-validation/src/verified.rs"
                    && line.contains("value_equality: false"));
            if !ok {
                bad.push(format!("{rel}:{}: {}", k + 1, line.trim()));
            }
        }
    }
    assert!(seen_fixture_diff, "fixture-diff no longer sets the field");
    assert!(
        bad.is_empty(),
        "CompareOptions::value_equality outside fixture-diff (it weakens exact rules):\n{}",
        bad.join("\n")
    );
}
