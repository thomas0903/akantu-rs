// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Test of the model
// benchmark suite (no upstream counterpart).

//! Every case of `suite_models` at size `s`, twice: it completes, prints
//! one JSON line per phase, and its digest is the same in both
//! repetitions and on every phase line (crate README, "Benchmark suite").

use std::collections::BTreeMap;
use std::process::Command;

#[test]
fn every_case_completes_and_repeats_its_digest() {
    let exe = env!("CARGO_BIN_EXE_suite_models");
    let list = Command::new(exe)
        .arg("--list")
        .output()
        .expect("suite_models --list");
    assert!(list.status.success());
    let names: Vec<String> = String::from_utf8(list.stdout)
        .unwrap()
        .lines()
        .filter_map(|l| l.split_whitespace().next().map(str::to_owned))
        .collect();
    assert!(names.len() >= 10, "cases: {names:?}");

    let out = Command::new(exe)
        .args(["--case", "all", "--size", "s", "--reps", "2"])
        .output()
        .expect("suite_models --case all");
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(
        out.status.success(),
        "suite_models failed:\n{stdout}\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    // case -> rep -> digests of its phase lines
    let mut seen: BTreeMap<String, BTreeMap<u64, Vec<String>>> = BTreeMap::new();
    // `skip: <case>: <reason>` lines go to stderr
    let skipped: Vec<String> = String::from_utf8_lossy(&out.stderr)
        .lines()
        .filter_map(|l| l.strip_prefix("skip: "))
        .map(|rest| rest.split(':').next().unwrap_or("").to_owned())
        .collect();
    for line in stdout.lines() {
        let v: serde_json::Value =
            serde_json::from_str(line).unwrap_or_else(|e| panic!("{line}: {e}"));
        assert_eq!(v["suite"], "models");
        assert_eq!(v["size"], "s");
        assert!(v["seconds"].as_f64().unwrap() >= 0.0);
        assert!(v["calls"].as_u64().unwrap() >= 1, "{line}");
        let case = v["case"].as_str().unwrap().to_owned();
        let rep = v["rep"].as_u64().unwrap();
        let digest = v["digest"].as_str().unwrap().to_owned();
        assert_eq!(digest.len(), 16, "{line}");
        seen.entry(case)
            .or_default()
            .entry(rep)
            .or_default()
            .push(digest);
    }
    for name in &names {
        if skipped.contains(name) {
            continue;
        }
        let reps = seen
            .get(name)
            .unwrap_or_else(|| panic!("no output for case {name}"));
        assert_eq!(reps.len(), 2, "{name}: repetitions {reps:?}");
        let all: Vec<&String> = reps.values().flatten().collect();
        assert!(
            all.iter().all(|d| *d == all[0]),
            "{name}: digests differ between phases or repetitions: {reps:?}"
        );
    }
}

/// The JSON lines, the status and the standard error of one run of the
/// binary (`AKANTU_*` thread variables of the test's environment removed,
/// `env` added).
fn thread_run(args: &[&str], env: &[(&str, &str)]) -> (bool, Vec<serde_json::Value>, String) {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_suite_models"));
    cmd.args(args).env("SCOTCH_PTHREAD_NUMBER", "1");
    for k in ["AKANTU_NUM_THREADS", "AKANTU_EXEC_GRAIN"] {
        cmd.env_remove(k);
    }
    for (k, v) in env {
        cmd.env(k, v);
    }
    let out = cmd.output().expect("suite_models runs");
    let lines = String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter(|l| l.starts_with('{'))
        .map(|l| serde_json::from_str(l).expect("one JSON object per line"))
        .collect();
    (
        out.status.success(),
        lines,
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

/// `--threads` sets the thread count of the run: one case at 1 and at 3
/// threads gives the same digest and records the count in force (and the
/// grain); the environment variable alone is recorded too; an explicit
/// `--threads` that differs from `AKANTU_NUM_THREADS` is an error naming
/// both.
#[test]
fn the_thread_option_sets_and_records_the_thread_count() {
    let case = "heat_static_tri6".to_owned();
    let run = |extra: &[&str], env: &[(&str, &str)]| {
        let mut a = vec!["--case", case.as_str(), "--size", "s"];
        a.extend_from_slice(extra);
        let (ok, lines, err) = thread_run(&a, env);
        assert!(ok, "{a:?}: {err}");
        assert!(!lines.is_empty(), "{a:?}: no output");
        lines
    };
    let one = run(&["--threads", "1"], &[]);
    let three = run(&["--threads", "3", "--grain", "finest"], &[]);
    let env3 = run(&[], &[("AKANTU_NUM_THREADS", "3")]);
    assert_eq!(one.len(), three.len());
    for (l1, l3) in one.iter().zip(&three) {
        assert_eq!(l1["digest"], l3["digest"], "{case}");
        assert_eq!(l1["threads"], 1);
        assert_eq!(l3["threads"], 3);
        assert_eq!(l1["grain"], "auto");
        assert_eq!(l3["grain"], "finest");
    }
    assert!(env3.iter().all(|l| l["threads"] == 3));
    let (ok, _, err) = thread_run(
        &["--case", case.as_str(), "--size", "s", "--threads", "2"],
        &[("AKANTU_NUM_THREADS", "3")],
    );
    assert!(!ok);
    assert!(
        err.contains("--threads 2") && err.contains("AKANTU_NUM_THREADS=3"),
        "{err}"
    );
}
