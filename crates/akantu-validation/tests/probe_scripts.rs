// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). No Akantu
// counterpart: runs the unit tests of the fixture-generation scripts
// (oracle/probes/python/tests: determinism filter, finalisation, provenance,
// driver grammar) as part of
// `cargo test -p akantu-validation`.
//
// They need `python3` (>= 3.9, standard library only) and `bash`. Where
// Python is genuinely unavailable, set AKANTU_VALIDATION_SKIP_PYTHON=1 to
// skip them explicitly; otherwise a missing interpreter is a failure.

use std::process::Command;

#[test]
fn fixture_generation_script_tests() {
    let root = akantu_validation::fixture::workspace_root();
    let dir = root.join("oracle/probes/python");
    let out = Command::new("python3")
        .args([
            "-m", "unittest", "discover", "-s", "tests", "-t", "tests", "-v",
        ])
        .current_dir(&dir)
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
    assert!(out.status.success(), "script tests failed:\n{text}");
    assert!(text.contains("OK"), "{text}");
}
