// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). No C++
// counterpart: a guard on the repository's Markdown documents.

//! Structural guard on every Markdown document of the repository (outside
//! `fixtures/`).
//!
//! A lost or extra code fence inverts code and prose for the rest of a file,
//! and a merge conflict resolved by committing both sides leaves its markers
//! behind. Both are cheap to detect:
//!
//! * code fences are balanced: an even number of fence lines, no fence line
//!   with an info string (a language tag) where a closing fence belongs, no
//!   block left open at the end of the file;
//! * no merge conflict marker outside code blocks.
//!
//! `AKANTU_DOC_GUARD_ROOT=<dir>` reads the documents from `<dir>` instead of
//! the workspace, to try the guard on another version of the tree.

mod repo_files;

use std::path::PathBuf;

use akantu_validation::fixture::workspace_root;

/// Markdown files of the repository that the guard reads, relative to `root`.
fn documents(root: &std::path::Path) -> Vec<String> {
    repo_files::repository_files(root)
        .into_iter()
        .filter(|rel| rel.ends_with(".md") && !rel.starts_with("fixtures/"))
        .collect()
}

/// An open code fence: its character, its length and the line it was opened on.
struct OpenFence {
    ch: char,
    len: usize,
    line: usize,
}

/// A fence line: up to three spaces, then three or more backticks or tildes,
/// then the info string (possibly empty).
fn fence_of(line: &str) -> Option<(char, usize, &str)> {
    let stripped = line.trim_start_matches(' ');
    if line.len() - stripped.len() > 3 {
        return None;
    }
    let ch = stripped.chars().next()?;
    if ch != '`' && ch != '~' {
        return None;
    }
    let len = stripped.chars().take_while(|&c| c == ch).count();
    if len < 3 {
        return None;
    }
    let info = &stripped[len..];
    // A backtick fence's info string cannot contain a backtick (that is
    // inline code such as ```x```).
    if ch == '`' && info.contains('`') {
        return None;
    }
    Some((ch, len, info))
}

/// Everything wrong with `text` (an empty list when the document is sound).
fn problems(name: &str, text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut open: Option<OpenFence> = None;
    let mut fence_lines = 0usize;
    for (i, line) in text.lines().enumerate() {
        let n = i + 1;
        if let Some((ch, len, info)) = fence_of(line) {
            match &open {
                None => {
                    fence_lines += 1;
                    open = Some(OpenFence { ch, len, line: n });
                }
                // a shorter fence, or another character, inside a block is content
                Some(o) if ch != o.ch || len < o.len => {}
                Some(_) if info.trim().is_empty() => {
                    fence_lines += 1;
                    open = None;
                }
                Some(o) => {
                    fence_lines += 1;
                    out.push(format!(
                        "{name}:{n}: fence line `{}` with an info string while the block \
                         opened on line {} is open (an opening fence was lost or a \
                         closing one is missing)",
                        line.trim(),
                        o.line
                    ));
                }
            }
            continue;
        }
        if open.is_some() {
            continue;
        }
        if line.starts_with("<<<<<<<") || line.starts_with(">>>>>>>") || line == "=======" {
            out.push(format!("{name}:{n}: merge conflict marker `{line}`"));
        }
    }
    if let Some(o) = open {
        out.push(format!(
            "{name}: the code block opened on line {} is never closed",
            o.line
        ));
    }
    if !fence_lines.is_multiple_of(2) {
        out.push(format!("{name}: odd number of fence lines ({fence_lines})"));
    }
    out
}

fn guard_root() -> PathBuf {
    std::env::var_os("AKANTU_DOC_GUARD_ROOT").map_or_else(workspace_root, PathBuf::from)
}

#[test]
fn markdown_documents_have_balanced_fences_and_no_conflict_markers() {
    let root = guard_root();
    let docs = documents(&root);
    assert!(
        docs.iter().any(|d| d == "README.md"),
        "no README.md among the documents of {}",
        root.display()
    );
    let mut all = Vec::new();
    for rel in &docs {
        let path = root.join(rel);
        let text = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
        all.extend(problems(rel, &text));
    }
    assert!(
        all.is_empty(),
        "document integrity problems:\n{}",
        all.join("\n")
    );
}

#[test]
fn a_sound_document_passes() {
    let doc =
        "# T\n\n## A\n\n```sh\ncargo test\n```\n\n## B\n\n~~~\nx\n~~~\n\nA `` ```x``` `` span.\n";
    assert_eq!(problems("doc", doc), Vec::<String>::new());
    // longer closing fence, nested shorter fence inside
    let nested = "## A\n````md\n```sh\nx\n```\n````\n";
    assert_eq!(problems("doc", nested), Vec::<String>::new());
}

#[test]
fn the_union_merge_signature_is_caught() {
    // Two sections joined line by line: the second block's heading and its
    // opening fence are gone, so the next section's tagged opening fence is
    // read as a closing one and prose and code are inverted from there on.
    let lost_opening =
        "## A\n\ntext\n\ncargo test -p b\n```\n\n## C\n\n```sh\ncargo test -p c\n```\n";
    let p = problems("doc", lost_opening);
    assert!(
        p.iter().any(|m| m.contains("info string")),
        "a language-tagged fence used as a closing fence: {p:?}"
    );
    assert!(p.iter().any(|m| m.contains("odd number")), "{p:?}");
}

#[test]
fn unclosed_block_and_conflict_markers_are_caught() {
    let p = problems("doc", "## A\n```sh\nx\n");
    assert!(p.iter().any(|m| m.contains("never closed")), "{p:?}");
    let p = problems("doc", "## A\n<<<<<<< ours\nx\n=======\ny\n>>>>>>> theirs\n");
    assert_eq!(
        p.iter().filter(|m| m.contains("conflict marker")).count(),
        3,
        "{p:?}"
    );
    // a heading repeated in another section and a `## ` line in a block are fine
    assert_eq!(
        problems("doc", "## A\ntext\n## B\n## A\n```sh\n## comment\n```\n"),
        Vec::<String>::new()
    );
    // conflict-marker-like lines inside a code block are content
    assert_eq!(
        problems("doc", "```text\n=======\n```\n"),
        Vec::<String>::new()
    );
}
