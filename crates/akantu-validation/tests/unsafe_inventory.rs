// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). No Akantu
// counterpart: the per-crate `UNSAFE.md` inventories cannot go stale.
//
// * `scripts/unsafe_inventory.py --check --strict-safety`: every `unsafe`
//   block and `unsafe impl` has its own adjacent `// SAFETY:` comment
//   (AGENTS.md, "Rules for numerical code"), and every `unsafe` block, `unsafe fn`,
//   `unsafe impl`, `unsafe trait`, `extern` block (and the items it
//   declares), unsafe attribute and foreign-ABI function of every crate
//   under `crates/` is listed exactly once, at its current line, in that
//   crate's `UNSAFE.md`; no row names a site that is gone. On a failure the
//   script prints the missing rows (`| ? | <line> | <site> |`) and the rows
//   to delete; a pure line drift is fixed with
//   `python3 scripts/unsafe_inventory.py --update-lines`
//   (docs/TESTING.md). Every row is also pinned to a
//   fingerprint of its site's code in `<crate>/UNSAFE.fingerprints`
//   (comments and whitespace ignored), so a block inserted
//   before another of the same item, or two swapped blocks, fail the check
//   until the rows are reviewed and re-pinned (`--refresh ID`);
//   `--update-lines` moves a row only while its fingerprint matches.
// * `scripts/unsafe_inventory.py --self-test`: the scanner on a synthetic
//   source (comments, strings, raw strings, char literals, lifetimes,
//   function-pointer types, nested items), the row parser round trip, the
//   `SAFETY:` comment rule (`SAFETYNET` or the word in prose do not
//   count) and the fingerprints (stable under comment and spacing
//   edits, changed by a token or literal edit, an insertion before a block
//   and a swap of two blocks).
//
// They need `python3` (standard library only); where Python is genuinely
// unavailable, set AKANTU_VALIDATION_SKIP_PYTHON=1 to skip them explicitly
// (as `parity_scripts.rs`); otherwise a missing interpreter is a failure.
// The script only reads the tree (`-B`: no bytecode is written, the gate's
// source mount is read-only).

use std::path::Path;
use std::process::Command;

/// Run `python3 -B scripts/unsafe_inventory.py <args>` from the workspace
/// root; `None` when Python is unavailable and the skip is requested.
fn run(root: &Path, args: &[&str]) -> Option<(bool, String)> {
    let out = Command::new("python3")
        .arg("-B")
        .arg(root.join("scripts/unsafe_inventory.py"))
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

/// Every `unsafe` site of the workspace is in its crate's `UNSAFE.md`, at
/// its current line, every row names an existing site, and every block /
/// impl carries its own `SAFETY:` comment.
#[test]
fn every_unsafe_site_is_in_its_crate_inventory() {
    let root = akantu_validation::fixture::workspace_root();
    let Some((ok, text)) = run(&root, &["--check", "--strict-safety"]) else {
        return;
    };
    eprintln!("{}", text.trim());
    assert!(ok, "scripts/unsafe_inventory.py --check failed:\n{text}");
    assert!(
        text.contains("unsafe inventory: PASS"),
        "no PASS verdict:\n{text}"
    );
}

/// The scanner and the row parser pass their own tests.
#[test]
fn unsafe_inventory_scanner_self_test() {
    let root = akantu_validation::fixture::workspace_root();
    let Some((ok, text)) = run(&root, &["--self-test"]) else {
        return;
    };
    assert!(ok && text.contains("self-test: PASS"), "{text}");
}
