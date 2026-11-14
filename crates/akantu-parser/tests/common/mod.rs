// SPDX-License-Identifier: LGPL-3.0-or-later
// Test helpers for the akantu-parser integration tests (akantu-rs).

#![allow(dead_code)] // each integration test uses a subset of the helpers

use std::path::{Path, PathBuf};

use akantu_parser::{AnonymousIds, FIRST_ANONYMOUS_ID, Parser};

/// `crates/akantu-parser/tests/fixtures`.
pub fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

/// Upstream input files copied from the reference tree (relative paths
/// `test/…`, `examples/…`, `extra_packages/…`), sorted.
pub fn upstream_inputs() -> Vec<String> {
    let root = fixtures_dir();
    let mut out = Vec::new();
    for top in ["test", "examples", "extra_packages"] {
        walk(&root.join(top), &mut out);
    }
    let mut rel: Vec<String> = out
        .into_iter()
        .map(|p| {
            p.strip_prefix(&root)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/")
        })
        .collect();
    rel.sort();
    rel
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            walk(&p, out);
        } else if p.extension().is_some_and(|e| e == "dat" || e == "in") {
            out.push(p);
        }
    }
}

/// Parse a fixture as a fresh upstream process would (anonymous section
/// counter starting at 12), with the file name as given to upstream
/// `Parser::parse` (its base name: the tests run in the file's directory).
pub fn parse_fixture(rel: &str) -> akantu_core::Result<Parser> {
    let bytes = std::fs::read(fixtures_dir().join(rel)).unwrap();
    let name = Path::new(rel).file_name().unwrap().to_string_lossy();
    let mut c = FIRST_ANONYMOUS_ID;
    Parser::parse_bytes(&bytes, &name, AnonymousIds::Local(&mut c))
}

/// Parse a string as a fresh upstream process would.
pub fn parse_text(text: &str) -> akantu_core::Result<Parser> {
    let mut c = FIRST_ANONYMOUS_ID;
    Parser::parse_str_with(text, "input.dat", AnonymousIds::Local(&mut c))
}
