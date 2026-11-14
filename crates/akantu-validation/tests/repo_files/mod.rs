// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). No C++
// counterpart: the list of the repository's files for the guards that read
// documents and text files (doc_integrity_guard.rs, public_tree_guard.rs).

//! The files of the repository, relative to its root, with `/` separators.
//!
//! The tracked files when `git ls-files` works in the root (a clone);
//! otherwise (a source archive, no git) every file below the root except
//! build output, tool state and virtual environments.

use std::path::Path;
use std::process::Command;

/// Directory names that are never part of the tree (build output, tool state).
const SKIP_DIRS: [&str; 7] = [
    ".git",
    "target",
    ".venv",
    "__pycache__",
    "node_modules",
    "scratch",
    ".oracle-build",
];

/// Every file of the repository at `root`, sorted.
pub fn repository_files(root: &Path) -> Vec<String> {
    let git = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["ls-files", "-z", "--cached"])
        .output();
    if let Ok(out) = git
        && out.status.success()
        && !out.stdout.is_empty()
    {
        let mut files: Vec<String> = out
            .stdout
            .split(|&b| b == 0)
            .filter(|s| !s.is_empty())
            .map(|s| String::from_utf8_lossy(s).into_owned())
            // a file deleted in the work tree but not yet committed
            .filter(|rel| root.join(rel).is_file())
            .collect();
        files.sort();
        return files;
    }
    let mut files = Vec::new();
    walk(root, root, &mut files);
    files.sort();
    files
}

fn walk(root: &Path, dir: &Path, out: &mut Vec<String>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        if kind.is_dir() {
            if SKIP_DIRS.contains(&name.as_str()) || name.starts_with("target-") {
                continue;
            }
            walk(root, &path, out);
        } else if kind.is_file()
            && let Ok(rel) = path.strip_prefix(root)
        {
            out.push(rel.to_string_lossy().replace('\\', "/"));
        }
    }
}
