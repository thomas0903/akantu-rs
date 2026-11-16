// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Test of the set-up
// benchmark suite (no upstream counterpart).

//! Every case of `suite_setup` at size `s`, twice: each completes and its
//! digest repeats (`docs/briefs/perf/SUITE.md`, "Sizes"). The parallel
//! cases also run on two in-process ranks (`--local-ranks 2`), twice.

use std::collections::BTreeMap;
use std::process::Command;

/// `(case, phase) -> digest` of one run.
fn run(args: &[&str]) -> BTreeMap<(String, String), String> {
    let out = Command::new(env!("CARGO_BIN_EXE_suite_setup"))
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
