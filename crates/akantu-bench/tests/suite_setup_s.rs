// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Test of the set-up
// benchmark suite (no upstream counterpart).

//! Every case of `suite_setup` at size `s`, twice: each completes and its
//! digest repeats (crate README, "Benchmark suite"). The parallel
//! cases also run on two in-process ranks (`--local-ranks 2`), twice.

use std::collections::BTreeMap;
use std::process::Command;

/// `(case, phase) -> digest` of one run.
fn run(args: &[&str]) -> BTreeMap<(String, String), String> {
    // Scotch's multi-threaded partition differs from run to run
    let out = Command::new(env!("CARGO_BIN_EXE_suite_setup"))
        .env("SCOTCH_PTHREAD_NUMBER", "1")
        .args(args)
        .output()
        .expect("suite_setup runs");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.success(),
        "suite_setup {args:?} failed:\n{stderr}"
    );
    let stdout = String::from_utf8(out.stdout).expect("UTF-8 output");
    let mut m = BTreeMap::new();
    for line in stdout.lines() {
        let v: serde_json::Value = serde_json::from_str(line).expect("one JSON object per line");
        let field = |k: &str| v[k].as_str().expect(k).to_owned();
        assert_eq!(field("suite"), "setup");
        assert_eq!(field("size"), "s");
        assert!(v["seconds"].as_f64().expect("seconds") >= 0.0);
        assert!(v["calls"].as_u64().expect("calls") >= 1);
        // the FNV-1a offset basis: nothing was hashed
        assert_ne!(field("digest"), "cbf29ce484222325", "{line}");
        m.insert((field("case"), field("phase")), field("digest"));
    }
    assert!(!m.is_empty(), "no output for {args:?}");
    m
}

fn cases(m: &BTreeMap<(String, String), String>) -> std::collections::BTreeSet<&str> {
    m.keys().map(|k| k.0.as_str()).collect()
}

#[test]
fn every_case_completes_and_repeats_its_digest() {
    let listed = Command::new(env!("CARGO_BIN_EXE_suite_setup"))
        .arg("--list")
        .output()
        .expect("suite_setup --list");
    let listed = String::from_utf8(listed.stdout).expect("UTF-8");
    let a = run(&["--case", "all", "--size", "s"]);
    let b = run(&["--case", "all", "--size", "s", "--reps", "2"]);
    // every listed case ran (the Scotch case only when built with it)
    for line in listed.lines() {
        let name = line.split_whitespace().next().expect("a name");
        if name == "partition_scotch_tet4" && !cfg!(feature = "scotch") {
            continue;
        }
        assert!(cases(&a).contains(name), "{name} did not run");
    }
    assert_eq!(a, b, "digests differ between runs (or with --reps 2)");
    // every phase line of a run carries the same digest
    let mut per_case: BTreeMap<&str, &str> = BTreeMap::new();
    for ((case, _), d) in &a {
        assert_eq!(*per_case.entry(case).or_insert(d), d.as_str(), "{case}");
    }
}

#[test]
fn parallel_cases_on_two_local_ranks_repeat_their_digest() {
    let list = "dist_mesh_tet4,dist_facets_tet4,ghost_nodes_tet4,ghost_elements_tet4,\
                reduce_scalars,explicit_step_dist_tet4";
    let args = ["--case", list, "--size", "s", "--local-ranks", "2"];
    let a = run(&args);
    let b = run(&args);
    assert_eq!(a, b);
    assert_eq!(cases(&a).len(), 6);
}

/// The JSON lines, the status and the standard error of one run of the
/// binary (`AKANTU_*` thread variables of the test's environment removed,
/// `env` added).
fn thread_run(args: &[&str], env: &[(&str, &str)]) -> (bool, Vec<serde_json::Value>, String) {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_suite_setup"));
    cmd.args(args).env("SCOTCH_PTHREAD_NUMBER", "1");
    for k in ["AKANTU_NUM_THREADS", "AKANTU_EXEC_GRAIN"] {
        cmd.env_remove(k);
    }
    for (k, v) in env {
        cmd.env(k, v);
    }
    let out = cmd.output().expect("suite_setup runs");
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
    // the first case of the list
    let list = Command::new(env!("CARGO_BIN_EXE_suite_setup"))
        .arg("--list")
        .output()
        .expect("--list");
    let case = String::from_utf8_lossy(&list.stdout)
        .lines()
        .find_map(|l| l.split_whitespace().next().map(str::to_owned))
        .expect("a case");
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
