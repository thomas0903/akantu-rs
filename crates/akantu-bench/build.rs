// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Build script of the
// benchmark crate; no upstream counterpart.

//! Records the commit of the build (`AKANTU_BENCH_COMMIT`, written on every
//! line of the benchmark suite's output). Outside a git checkout, or
//! without `git`, the value is `unknown`.

use std::process::Command;

fn git(args: &[&str]) -> Option<String> {
    let out = Command::new("git").args(args).output().ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8(out.stdout).ok()?;
    let s = s.trim();
    (!s.is_empty()).then(|| s.to_string())
}

fn main() {
    println!("cargo:rerun-if-env-changed=AKANTU_BENCH_COMMIT");
    // rebuild when HEAD moves: the HEAD file, the branch ref it names and
    // the packed refs (paths resolved by git, worktrees included)
    for path in [
        git(&["rev-parse", "--git-path", "HEAD"]),
        git(&["symbolic-ref", "-q", "HEAD"])
            .and_then(|r| git(&["rev-parse", "--git-path", r.as_str()])),
        git(&["rev-parse", "--git-path", "packed-refs"]),
    ]
    .into_iter()
    .flatten()
    {
        println!("cargo:rerun-if-changed={path}");
    }
    let commit = std::env::var("AKANTU_BENCH_COMMIT")
        .ok()
        .filter(|s| !s.is_empty())
        .or_else(|| git(&["rev-parse", "--short=9", "HEAD"]))
        .unwrap_or_else(|| "unknown".to_string());
    println!("cargo:rustc-env=AKANTU_BENCH_COMMIT={commit}");
}
