// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs design rules of the Python bindings (docs/ARCHITECTURE.md, "Shared storage and its aliasing rules"). Derived from Akantu (EPFL/LSMS) python/*.cc only in the
// sense that it guards the port of those bindings.

//! Static checks of the binding sources (run without Python):
//!
//! * the GIL is never released by the bindings (`Python::detach` /
//!   `allow_threads`): a native section must not let other Python threads
//!   run NumPy on exported views (ARCHITECTURE.md, "Python extension and C interface");
//! * no hand-written `unsafe` under `src/py/` (only PyO3 macro expansions;
//!   the inventoried `unsafe` lives in `foreign_view.rs` and `scoped.rs`);
//! * every binding source file starts with the SPDX header.
//!
//! These are static checks of the source tree: under Miri (whose isolation
//! forbids file-system access) they are skipped, so that the documented
//! command `cargo +nightly miri test -p akantu-python` runs the foreign-view
//! contract tests.

use std::fs;
use std::path::{Path, PathBuf};

fn sources(dir: &Path, out: &mut Vec<PathBuf>) {
    for e in fs::read_dir(dir).expect("readable source directory") {
        let p = e.expect("directory entry").path();
        if p.is_dir() {
            sources(&p, out);
        } else if p.extension().is_some_and(|x| x == "rs") {
            out.push(p);
        }
    }
}

fn code_lines(text: &str) -> impl Iterator<Item = (usize, &str)> {
    text.lines()
        .enumerate()
        .map(|(i, l)| (i + 1, l.trim_start()))
        .filter(|(_, l)| !l.starts_with("//"))
}

fn all_sources() -> Vec<PathBuf> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    sources(&root, &mut files);
    files.sort();
    assert!(files.len() > 5, "binding sources not found");
    files
}

#[test]
#[cfg_attr(
    miri,
    ignore = "static source check: reads the file system, unsupported under Miri isolation"
)]
fn bindings_never_release_the_gil() {
    let mut bad = Vec::new();
    for f in all_sources() {
        let text = fs::read_to_string(&f).expect("utf-8 source");
        for (n, l) in code_lines(&text) {
            if l.contains("allow_threads") || l.contains(".detach(") || l.contains("::detach(") {
                bad.push(format!("{}:{n}: {l}", f.display()));
            }
        }
    }
    assert!(
        bad.is_empty(),
        "GIL released in a binding:\n{}",
        bad.join("\n")
    );
}

#[test]
#[cfg_attr(
    miri,
    ignore = "static source check: reads the file system, unsupported under Miri isolation"
)]
fn no_hand_written_unsafe_under_py() {
    let mut bad = Vec::new();
    for f in all_sources() {
        if !f.components().any(|c| c.as_os_str() == "py") {
            continue;
        }
        let text = fs::read_to_string(&f).expect("utf-8 source");
        for (n, l) in code_lines(&text) {
            if l.contains("unsafe ") || l.contains("unsafe{") {
                bad.push(format!("{}:{n}: {l}", f.display()));
            }
        }
    }
    assert!(
        bad.is_empty(),
        "unsafe code under src/py:\n{}",
        bad.join("\n")
    );
}

#[test]
#[cfg_attr(
    miri,
    ignore = "static source check: reads the file system, unsupported under Miri isolation"
)]
fn binding_sources_carry_the_spdx_header() {
    for f in all_sources() {
        let text = fs::read_to_string(&f).expect("utf-8 source");
        assert!(
            text.starts_with("// SPDX-License-Identifier: LGPL-3.0-or-later"),
            "{} lacks the SPDX header",
            f.display()
        );
    }
}
