// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs regression tests: the
// state kept by a `Parser` across member `parse` calls, as upstream
// `Parser::parse` (src/io/parser/parser_input_files.cc), and the anonymous
// section counter at its boundary.

use std::path::PathBuf;

use akantu_parser::{AnonymousIds, Parser, SearchScope};

fn temp_file(name: &str, text: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("akantu-rs-{}-{name}", std::process::id()));
    std::fs::write(&p, text).unwrap();
    p
}

#[test]
fn member_parse_keeps_the_permissive_flag_and_clears_before_opening() {
    let first = temp_file("first.dat", "permissive_parser = true\na = 1\n");
    let second = temp_file("second.dat", "a = 2\n");
    let mut p = Parser::default();
    p.parse(&first).unwrap();
    assert!(p.is_permissive());
    // The flag changes only when a file sets it (upstream static flag).
    p.parse(&second).unwrap();
    assert!(p.is_permissive());
    let a: f64 = p
        .root
        .get_parameter_value("a", SearchScope::Current)
        .unwrap();
    assert_eq!(a, 2.0);
    assert_eq!(p.last_parsed_file(), second.to_string_lossy());
    // Upstream calls clean() before opening the file, so a missing
    // file leaves an empty tree (and the flag and last file unchanged).
    let absent = std::env::temp_dir().join(format!("akantu-rs-{}-absent.dat", std::process::id()));
    assert!(p.parse(&absent).is_err());
    assert!(!p.root.has_parameter("a", SearchScope::Current));
    assert!(p.root.parameters.is_empty() && p.root.sub_sections.is_empty());
    assert!(p.is_permissive());
    assert_eq!(p.last_parsed_file(), second.to_string_lossy());
    // A parse error keeps the partial tree, the flag and the last file.
    let bad = temp_file(
        "bad.dat",
        "permissive_parser = false\nb = 1\nmaterial m [\n c = 2\n",
    );
    assert!(p.parse(&bad).is_err());
    assert!(p.root.has_parameter("b", SearchScope::Current));
    assert_eq!(p.root.sub_sections[0].parameter("c").unwrap().value, "2");
    assert!(p.is_permissive());
    assert_eq!(p.last_parsed_file(), second.to_string_lossy());
    for f in [first, second, bad] {
        std::fs::remove_file(f).unwrap();
    }
}

#[test]
fn caller_counter_wraps_like_size_t() {
    // `usize::MAX` is followed by 0 (upstream `size_t` increment),
    // without a panic.
    let mut id = usize::MAX;
    let p = Parser::parse_str_with(
        "user [ ]\nuser named [ ]\nuser [ ]\n",
        "x.dat",
        AnonymousIds::Local(&mut id),
    )
    .unwrap();
    let names: Vec<&str> = p
        .root
        .sub_sections
        .iter()
        .map(|s| s.name.as_str())
        .collect();
    assert_eq!(
        names,
        [
            format!("anonymous_{}", usize::MAX).as_str(),
            "named",
            "anonymous_1"
        ]
    );
    assert_eq!(id, 2);
}
