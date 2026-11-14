// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). No Akantu
// counterpart: behavioural tests of the `fixture-diff` baseline tool
// (symmetric comparison, true bit identity, metadata
// reporting, root validation).

use std::path::Path;
use std::process::Command;

use akantu_validation::manifest::{BuildInfo, FixtureStatus, Generator, Layout, ReferenceInfo};
use akantu_validation::writer::FixtureWriter;

const SHA: &str = "35a851780d0b9471d258ed53a003ae98aaa95ea0";

struct Spec<'a> {
    case: &'a str,
    special: Vec<f64>,
    u: Vec<f64>,
    u_shape: Vec<usize>,
    ids: Vec<i64>,
    extra: Option<&'a str>,
    config: &'a str,
    input_text: &'a str,
}

impl Default for Spec<'_> {
    fn default() -> Self {
        Self {
            case: "selftest/cli_case",
            special: vec![0.0, -1.0 / 3.0, f64::NAN, f64::INFINITY],
            u: vec![1.0, 2.0, 3.0, 4.0],
            u_shape: vec![2, 2],
            ids: vec![7, 9],
            extra: None,
            config: "strict",
            input_text: "$MeshFormat\n",
        }
    }
}

fn write_tree(root: &Path, spec: &Spec<'_>) {
    let mut w = FixtureWriter::create(
        root,
        spec.case,
        "cli test case",
        FixtureStatus::Golden,
        Generator {
            probe: "cli-test".into(),
            probe_version: "1".into(),
            command: "none".into(),
        },
        ReferenceInfo {
            sha: SHA.into(),
            describe: None,
            patches: vec![],
        },
        Some(BuildInfo {
            config: spec.config.into(),
            compiler: "synthetic".into(),
            arch: "none".into(),
            ..BuildInfo::default()
        }),
    )
    .unwrap();
    w.add_f64(
        "special",
        &[spec.special.len()],
        &spec.special,
        &["entry"],
        Layout::Table,
    )
    .unwrap();
    let axes: Vec<&str> = if spec.u_shape.len() == 2 {
        vec!["node", "component"]
    } else {
        vec!["entry"]
    };
    w.add_f64("u", &spec.u_shape, &spec.u, &axes, Layout::Nodal)
        .unwrap();
    w.add_i64(
        "ids",
        &[spec.ids.len()],
        &spec.ids,
        &["node"],
        Layout::Nodal,
    )
    .unwrap();
    if let Some(name) = spec.extra {
        w.add_f64(name, &[1], &[1.0], &["entry"], Layout::Table)
            .unwrap();
    }
    let input = root.join(format!("{}.msh", spec.case.replace('/', "_")));
    std::fs::write(&input, spec.input_text).unwrap();
    w.add_input_file("mesh", &input, Some("generated:test".into()))
        .unwrap();
    w.finish().unwrap();
}

struct Run {
    code: i32,
    out: String,
    err: String,
}

fn run(a: &Path, b: &Path, extra: &[&str]) -> Run {
    let o = Command::new(env!("CARGO_BIN_EXE_fixture-diff"))
        .arg(a)
        .arg(b)
        .args(extra)
        .output()
        .unwrap();
    Run {
        code: o.status.code().unwrap_or(-1),
        out: String::from_utf8_lossy(&o.stdout).into_owned(),
        err: String::from_utf8_lossy(&o.stderr).into_owned(),
    }
}

#[test]
fn identical_trees_measure_zero_differences() {
    let dir = tempfile::tempdir().unwrap();
    let (a, b) = (dir.path().join("a"), dir.path().join("b"));
    write_tree(&a, &Spec::default());
    write_tree(&b, &Spec::default());
    let r = run(&a, &b, &[]);
    assert_eq!(r.code, 0, "{}{}", r.out, r.err);
    assert!(
        r.out
            .contains("1 case(s), 3 quantities compared: 3 numerically equal (3 bit-identical, 0 bits-only), 0 differ numerically"),
        "{}",
        r.out
    );
    assert!(!r.out.contains("problem:"), "{}", r.out);
    let j = run(&a, &b, &["--json"]);
    let v: serde_json::Value = serde_json::from_str(&j.out).unwrap();
    assert_eq!(v["quantities_compared"], 3);
    assert_eq!(v["bit_identical"], 3);
    assert_eq!(v["problems"].as_array().unwrap().len(), 0);
}

#[test]
fn nonexistent_root_is_a_usage_error() {
    let dir = tempfile::tempdir().unwrap();
    let b = dir.path().join("b");
    write_tree(&b, &Spec::default());
    let r = run(&dir.path().join("does_not_exist"), &b, &[]);
    assert_eq!(r.code, 2, "{}{}", r.out, r.err);
    assert!(r.err.contains("not a readable directory"), "{}", r.err);
    assert!(!r.out.contains("quantities compared"), "{}", r.out);
    let r = run(&b, &dir.path().join("nope"), &[]);
    assert_eq!(r.code, 2);
    let r = run(&b, &b, &["--case", "nothing/matches/*"]);
    assert_eq!(r.code, 1, "{}", r.out);
    assert!(r.out.contains("nothing was measured"), "{}", r.out);
    // A regular file is not a fixture root.
    let file = dir.path().join("file");
    std::fs::write(&file, "x").unwrap();
    assert_eq!(run(&file, &b, &[]).code, 2);
}

#[test]
fn malformed_command_lines_are_usage_errors() {
    let dir = tempfile::tempdir().unwrap();
    let a = dir.path().join("a");
    write_tree(&a, &Spec::default());
    // `--case` without a value used to fall back silently to `*`.
    let r = run(&a, &a, &["--case"]);
    assert_eq!(r.code, 2, "{}{}", r.out, r.err);
    assert!(r.err.contains("--case needs a glob"), "{}", r.err);
    let r = run(&a, &a, &["--frobnicate"]);
    assert_eq!(r.code, 2);
    assert!(r.err.contains("unknown option"), "{}", r.err);
    let r = run(&a, &a, &[a.to_str().unwrap()]);
    assert_eq!(r.code, 2, "three roots");
    // Flags may precede the roots.
    let o = Command::new(env!("CARGO_BIN_EXE_fixture-diff"))
        .args(["--json", "--case", "selftest/*"])
        .arg(&a)
        .arg(&a)
        .output()
        .unwrap();
    assert_eq!(o.status.code(), Some(0));
    let v: serde_json::Value = serde_json::from_slice(&o.stdout).unwrap();
    assert_eq!(v["quantities_compared"], 3);
}

#[test]
fn dtype_changes_and_corrupt_data_are_problems() {
    let dir = tempfile::tempdir().unwrap();
    let (a, b) = (dir.path().join("a"), dir.path().join("b"));
    write_tree(&a, &Spec::default());
    write_tree(&b, &Spec::default());
    // Corrupt one data file of B without updating its checksum.
    let f = b.join("selftest/cli_case/data/u.npy");
    let mut bytes = std::fs::read(&f).unwrap();
    let last = bytes.len() - 1;
    bytes[last] ^= 0x01;
    std::fs::write(&f, bytes).unwrap();
    let r = run(&a, &b, &[]);
    assert_eq!(r.code, 1, "{}", r.out);
    assert!(
        r.out.contains("problem: selftest/cli_case: u:"),
        "{}",
        r.out
    );
    assert!(r.out.contains("2 quantities compared"), "{}", r.out);
    // Same values stored as a different dtype.
    let c = dir.path().join("c");
    let mut w = FixtureWriter::create(
        &c,
        "selftest/cli_case",
        "cli test case",
        FixtureStatus::Golden,
        Generator {
            probe: "cli-test".into(),
            probe_version: "1".into(),
            command: "none".into(),
        },
        ReferenceInfo {
            sha: SHA.into(),
            describe: None,
            patches: vec![],
        },
        Some(BuildInfo {
            config: "strict".into(),
            compiler: "synthetic".into(),
            arch: "none".into(),
            ..BuildInfo::default()
        }),
    )
    .unwrap();
    let s = Spec::default();
    w.add_f64("special", &[4], &s.special, &["entry"], Layout::Table)
        .unwrap();
    w.add_f64("u", &[2, 2], &s.u, &["node", "component"], Layout::Nodal)
        .unwrap();
    w.add_f64("ids", &[2], &[7.0, 9.0], &["node"], Layout::Nodal)
        .unwrap();
    let input = c.join("selftest_cli_case.msh");
    std::fs::create_dir_all(&c).unwrap();
    std::fs::write(&input, s.input_text).unwrap();
    w.add_input_file("mesh", &input, Some("generated:test".into()))
        .unwrap();
    w.finish().unwrap();
    let r = run(&a, &c, &[]);
    assert_eq!(r.code, 1, "{}", r.out);
    assert!(r.out.contains("ids: dtype i64 vs f64"), "{}", r.out);
}

#[test]
fn asymmetric_inventories_are_problems() {
    let dir = tempfile::tempdir().unwrap();
    let (a, b) = (dir.path().join("a"), dir.path().join("b"));
    write_tree(&a, &Spec::default());
    write_tree(
        &b,
        &Spec {
            extra: Some("only_in_b"),
            ..Spec::default()
        },
    );
    let r = run(&a, &b, &[]);
    assert_eq!(r.code, 1, "{}", r.out);
    assert!(r.out.contains("quantity only_in_b only in B"), "{}", r.out);
    assert!(r.out.contains("3 quantities compared"), "{}", r.out);
    // A-only quantity and B-only case.
    let (c, d) = (dir.path().join("c"), dir.path().join("d"));
    write_tree(
        &c,
        &Spec {
            extra: Some("only_in_a"),
            ..Spec::default()
        },
    );
    write_tree(&d, &Spec::default());
    write_tree(
        &d,
        &Spec {
            case: "selftest/second_case",
            ..Spec::default()
        },
    );
    let r = run(&c, &d, &[]);
    assert_eq!(r.code, 1, "{}", r.out);
    assert!(r.out.contains("quantity only_in_a only in A"), "{}", r.out);
    assert!(
        r.out.contains("selftest/second_case: case only in B"),
        "{}",
        r.out
    );
    // Missing A root content but existing directory: every case is B-only.
    std::fs::create_dir_all(dir.path().join("empty")).unwrap();
    let r = run(&dir.path().join("empty"), &d, &[]);
    assert_eq!(r.code, 1);
    assert!(r.out.contains("case only in B"), "{}", r.out);
}

#[test]
fn bit_identity_is_separate_from_numerical_equality() {
    let dir = tempfile::tempdir().unwrap();
    let (a, b) = (dir.path().join("a"), dir.path().join("b"));
    write_tree(&a, &Spec::default());
    // Flip the sign of the zero and change the NaN payload in B.
    let nan2 = f64::from_bits(f64::NAN.to_bits() ^ 0x1);
    write_tree(
        &b,
        &Spec {
            special: vec![-0.0, -1.0 / 3.0, nan2, f64::INFINITY],
            ..Spec::default()
        },
    );
    let r = run(&a, &b, &[]);
    assert_eq!(r.code, 0, "{}{}", r.out, r.err);
    assert!(
        r.out
            .contains("3 numerically equal (2 bit-identical, 1 bits-only), 0 differ numerically"),
        "{}",
        r.out
    );
    assert!(
        r.out.contains("bits-only: selftest/cli_case :: special"),
        "{}",
        r.out
    );
    let j = run(&a, &b, &["--json"]);
    let v: serde_json::Value = serde_json::from_str(&j.out).unwrap();
    assert_eq!(v["bit_identical"], 2);
    assert_eq!(v["numerically_equal"], 3);
    assert_eq!(v["bits_only_differences"][0], "selftest/cli_case::special");
}

#[test]
fn numerical_differences_are_measured_not_failed() {
    let dir = tempfile::tempdir().unwrap();
    let (a, b) = (dir.path().join("a"), dir.path().join("b"));
    write_tree(&a, &Spec::default());
    write_tree(
        &b,
        &Spec {
            u: vec![1.0, 2.0, 3.0, 4.0 + 1e-12],
            ids: vec![7, 10],
            ..Spec::default()
        },
    );
    let r = run(&a, &b, &[]);
    assert_eq!(r.code, 0, "{}", r.out);
    assert!(
        r.out
            .contains("1 bit-identical, 0 bits-only), 2 differ numerically"),
        "{}",
        r.out
    );
    let line = r.out.lines().find(|l| l.contains(" u ")).unwrap();
    assert!(
        line.contains("1.000e-12") || line.contains("8.882e-13"),
        "{line}"
    );
}

#[test]
fn structural_and_metadata_differences_are_reported() {
    let dir = tempfile::tempdir().unwrap();
    let (a, b) = (dir.path().join("a"), dir.path().join("b"));
    write_tree(&a, &Spec::default());
    write_tree(
        &b,
        &Spec {
            u: vec![1.0, 2.0, 3.0, 4.0],
            u_shape: vec![4],
            config: "default",
            input_text: "$MeshFormat\n2.2 0 8\n",
            ..Spec::default()
        },
    );
    let r = run(&a, &b, &[]);
    assert_eq!(r.code, 1, "{}", r.out);
    assert!(
        r.out
            .contains("problem: selftest/cli_case: u: shape [2, 2] vs [4]"),
        "{}",
        r.out
    );
    assert!(r.out.contains("axes"), "{}", r.out);
    assert!(
        r.out
            .contains("problem: selftest/cli_case: input `mesh` differs"),
        "{}",
        r.out
    );
    assert!(
        r.out
            .contains("metadata: selftest/cli_case: build.config \"strict\" vs \"default\""),
        "{}",
        r.out
    );
    // The quantity with a structural difference is not counted as compared.
    assert!(r.out.contains("2 quantities compared"), "{}", r.out);
    // A build-only difference is metadata, not a problem.
    let c = dir.path().join("c");
    write_tree(
        &c,
        &Spec {
            config: "default",
            ..Spec::default()
        },
    );
    let r = run(&a, &c, &[]);
    assert_eq!(r.code, 0, "{}", r.out);
    assert!(
        r.out.contains("metadata: selftest/cli_case: build.config"),
        "{}",
        r.out
    );
}
