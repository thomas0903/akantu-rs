// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Test of the model
// benchmark suite (no upstream counterpart).

//! Every case of `suite_models` at size `s`, twice: it completes, prints
//! one JSON line per phase, and its digest is the same in both
//! repetitions and on every phase line (`docs/briefs/perf/SUITE.md`).

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
    // `skip: <case> (<reason>)` lines go to stderr
    let skipped: Vec<String> = String::from_utf8_lossy(&out.stderr)
        .lines()
        .filter_map(|l| l.strip_prefix("skip: "))
        .map(|rest| rest.split_whitespace().next().unwrap_or("").to_owned())
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
