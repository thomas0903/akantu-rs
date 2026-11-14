// SPDX-License-Identifier: LGPL-3.0-or-later
// Licence guard of akantu-embedded (D-042): not a
// port of an upstream test.

//! The `embedded` feature stays in the default, LGPL-3.0-or-later build:
//! `akantu-embedded` must never depend on the GPL-3.0-or-later crate
//! `akantu-geometry-cgal`, in any form (inline, table, renamed `package =`)
//! or dependency kind (normal, build, dev), and the umbrella's `embedded`
//! feature must not enable `cgal`.
//!
//! Checked on the **resolved** graph (`cargo tree`, offline) for
//! `akantu-embedded` with all features and for the umbrella
//! with its default features and with `embedded` alone, plus a scan of the
//! manifest for any mention of the GPL crate.

use std::path::Path;
use std::process::Command;

const GPL_CRATE: &str = "akantu-geometry-cgal";

fn workspace_root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("workspace root")
}

fn cargo_tree(args: &[&str]) -> String {
    let out = Command::new(env!("CARGO"))
        .current_dir(workspace_root())
        .args([
            "tree",
            "--offline",
            "--prefix",
            "none",
            "-e",
            "normal,build,dev",
        ])
        .args(args)
        .output()
        .expect("run cargo tree");
    let stderr = String::from_utf8_lossy(&out.stderr);
    // `--offline`: every package of the feature set must be in the local
    // registry; the CI tiers run `cargo fetch --locked` first
    // (docs/TESTING.md). The check is not skipped.
    let hint = if stderr.contains("--offline was specified") {
        "\nhint: the cargo registry lacks a package of Cargo.lock; run `cargo fetch --locked` once"
    } else {
        ""
    };
    assert!(
        out.status.success(),
        "cargo tree {args:?} failed: {stderr}{hint}"
    );
    String::from_utf8(out.stdout).expect("utf-8")
}

#[test]
fn embedded_never_depends_on_the_gpl_crate() {
    let tree = cargo_tree(&["-p", "akantu-embedded", "--all-features"]);
    assert!(
        tree.contains("akantu-geometry v"),
        "LGPL geometry expected:\n{tree}"
    );
    assert!(
        !tree.contains(GPL_CRATE),
        "akantu-embedded pulls {GPL_CRATE}:\n{tree}"
    );
}

#[test]
fn umbrella_default_and_embedded_builds_stay_lgpl() {
    let default = cargo_tree(&["-p", "akantu"]);
    assert!(
        default.contains("akantu-embedded v"),
        "embedded in the default build"
    );
    assert!(
        !default.contains(GPL_CRATE),
        "default build pulls {GPL_CRATE}"
    );
    let embedded = cargo_tree(&[
        "-p",
        "akantu",
        "--no-default-features",
        "--features",
        "embedded",
    ]);
    assert!(embedded.contains("akantu-embedded v"));
    assert!(
        !embedded.contains(GPL_CRATE),
        "feature `embedded` pulls {GPL_CRATE}"
    );
}

#[test]
fn manifest_does_not_mention_the_gpl_crate() {
    let manifest =
        std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml"))
            .expect("manifest");
    // the only allowed mention is the comment forbidding it
    for line in manifest.lines() {
        let code = line.split('#').next().unwrap_or("");
        assert!(
            !code.contains(GPL_CRATE),
            "manifest line mentions {GPL_CRATE}: {line}"
        );
    }
    let license = manifest
        .lines()
        .find(|l| l.trim_start().starts_with("license"))
        .expect("license line");
    assert!(license.contains("workspace"), "{license}");
}
