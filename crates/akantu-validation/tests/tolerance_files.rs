// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). No Akantu
// counterpart: every committed tolerance
// file is read through the harness.

//! `ToleranceFile::read` validates a file (format version, every rule's
//! spec, the refusal of placeholder reviews) only when a test resolves
//! a rule from it, so a broken rule in a file whose rules no test resolves
//! would go unnoticed. This test reads every `tolerances.json` under
//! `fixtures/` through the harness.

use std::path::{Path, PathBuf};

use akantu_validation::fixtures_root;
use akantu_validation::tolerance::ToleranceFile;

fn tolerance_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let mut entries: Vec<_> = std::fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
        .map(|e| e.unwrap().path())
        .collect();
    entries.sort();
    for p in entries {
        if p.is_dir() {
            tolerance_files(&p, out);
        } else if p.file_name().is_some_and(|n| n == "tolerances.json") {
            out.push(p);
        }
    }
}

/// Every committed `fixtures/**/tolerances.json` reads and validates
/// through `ToleranceFile::read`, and every rule that names its reviewers
/// names real ones (a placeholder is refused by the read itself).
#[test]
fn every_committed_tolerance_file_reads_through_the_harness() {
    let mut files = Vec::new();
    tolerance_files(&fixtures_root(), &mut files);
    assert!(
        files.len() >= 40,
        "only {} tolerance files found under {}",
        files.len(),
        fixtures_root().display()
    );
    let mut rules = 0;
    let mut failures = Vec::new();
    for f in &files {
        match ToleranceFile::read(f) {
            Ok(t) => rules += t.rules.len(),
            Err(e) => failures.push(format!("{}: {e}", f.display())),
        }
    }
    eprintln!("{} tolerance files, {rules} rules read", files.len());
    assert!(failures.is_empty(), "{failures:#?}");
}
