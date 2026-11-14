// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu's JOSS scalability test (EPFL/LSMS):
// performance-testing-cohesive @ 483bace8 (cohesive_extrinsic.cc), checked
// against the C++ oracle runs of the akantu-rs harness
// benchmarks/cohesive-perf/ (fixtures/perf_cohesive/*_eigen).

//! Differential test of the Rust cohesive scalability driver against the
//! strict C++ oracle.
//!
//! The reference cases `fixtures/perf_cohesive/{coh,ela}_np1_eigen` are the
//! upstream benchmark on the frozen h = 0.12 cube (4,824 nodes, 23,156
//! tetrahedra, 1,500 steps, 17-digit energies) with the static solve done
//! by Eigen's SparseLU (`sparse_solver_type = eigen`), which the port's
//! `eigen` backend translates bit for bit; the upstream-default cases
//! `coh_np1` / `ela_np1` used MUMPS (`_auto`), see the fixture README and
//! `eigen_variant.json` for the recorded C++ Eigen-vs-MUMPS differences.
//! The comparison is **bitwise**: every energy of every compared step as
//! the 17-significant-digit text the oracle wrote (round-trip exact), the
//! insertion steps and counts, Δt, and at full length every value of
//! `summary.json` and `summary_rank0.json` except the timings and the
//! `fill_nodes_to_elements` statistics of the C++ interposer.
//!
//! Cost: the h = 0.12 static solve alone takes ≈ 85 s unoptimised, so the
//! replays are `#[ignore]`d in unoptimised builds (reported as ignored, not
//! as passed) and run in the release tier (`cargo test
//! --release -p akantu-bench`; in an unoptimised build `-- --ignored`).
//! Steps: the cohesive case runs all 1,500 steps (insertion starts at step
//! 710), the elastic case `PERF_ELA_STEPS` (default 200) unless
//! `PERF_ALL=1`; `PERF_STEPS=<n>` caps both.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use akantu_bench::output::g;
use akantu_bench::{Config, RunReport, run};
use akantu_comm::Communicator;
use serde_json::Value;

fn fixture_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/perf_cohesive")
}

fn env_steps(name: &str) -> Option<usize> {
    std::env::var(name).ok().and_then(|v| v.trim().parse().ok())
}

/// Number of steps to replay for a case whose fixture has `full` steps.
fn steps_for(full: usize, default: usize) -> usize {
    let mut n = if std::env::var("PERF_ALL").is_ok() {
        full
    } else {
        default
    };
    if let Some(cap) = env_steps("PERF_STEPS") {
        n = n.min(cap);
    }
    n.min(full)
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// Run the case `name` for `steps` steps; returns the report and the output
/// directory.
fn run_case(name: &str, steps: usize) -> (RunReport, PathBuf) {
    let dir = fixture_root().join(name);
    let out = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("perf_cohesive_{name}"));
    let _ = std::fs::remove_dir_all(&out);
    std::fs::create_dir_all(&out).unwrap();
    let mut cfg = Config::from_file(&dir.join("input.dat"), &dir, &out).unwrap();
    cfg.max_steps = steps;
    cfg.verbose = false;
    let report = run(&Communicator::serial(), &cfg).unwrap();
    (report, out)
}

/// The fixture's `energies.csv` rows (text fields) by step.
fn fixture_energies(name: &str) -> Vec<Vec<String>> {
    read(&fixture_root().join(name).join("energies.csv"))
        .lines()
        .skip(1)
        .map(|l| l.split(", ").map(str::to_owned).collect())
        .collect()
}

const COLUMNS: [&str; 5] = ["ed", "ep", "ek", "ew", "et"];

/// Bitwise comparison of the energies of steps `0..=steps` (17-digit text,
/// round-trip exact). Returns the number of compared values.
fn compare_energies(name: &str, report: &RunReport, steps: usize) -> usize {
    let expected = fixture_energies(name);
    assert!(expected.len() > steps, "{name}: fixture has fewer steps");
    assert_eq!(report.energies.len(), steps + 1, "{name}: energy rows");
    let mut compared = 0;
    for (row, (step, values)) in expected.iter().zip(&report.energies) {
        assert_eq!(row[0], step.to_string(), "{name}: step numbering");
        for (k, v) in values.iter().enumerate() {
            let got = g(*v, 17);
            let want = &row[k + 1];
            if &got != want {
                let w: f64 = want.parse().unwrap();
                let rel = (v - w).abs() / v.abs().max(w.abs());
                panic!(
                    "{name}: step {step} {}: got {got}, oracle {want} (rel {rel:.3e}); first \
                     difference",
                    COLUMNS[k]
                );
            }
            compared += 1;
        }
    }
    compared
}

/// The fixture's insertion steps up to `steps` against the run's.
fn compare_insertions(name: &str, report: &RunReport, steps: usize) -> usize {
    let text = read(&fixture_root().join(name).join("insertions.csv"));
    let expected: Vec<(usize, usize)> = text
        .lines()
        .skip(1)
        .map(|l| {
            let f: Vec<usize> = l.split(", ").map(|x| x.parse().unwrap()).collect();
            (f[0], f[1])
        })
        .filter(|&(s, _)| s <= steps)
        .collect();
    assert_eq!(
        report.insertions, expected,
        "{name}: insertion steps/counts"
    );
    expected.len()
}

/// A JSON file. serde_json parses floats correctly rounded because the
/// workspace enables its `float_roundtrip` feature (the
/// default parser read the port's `1.6271440317223231e-07` one ULP off),
/// and it keeps the sign of `-0` (parsed as `-0.0`):
/// `serde_json_reads_numbers_correctly_rounded` pins both.
fn json(path: &Path) -> Value {
    serde_json::from_str(&read(path)).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn num(v: &Value) -> f64 {
    v.as_f64().unwrap_or_else(|| panic!("not a number: {v:?}"))
}

/// Keys of the C++ `summary.json` compared at full length (all of them:
/// the C++ file has no timings; the port appends its own keys).
const SUMMARY_KEYS: [&str; 20] = [
    "psize",
    "mesh",
    "max_steps",
    "cohesive_insertion",
    "blocked_node",
    "time_step",
    "nb_global_nodes_start",
    "nb_global_nodes_end",
    "nb_owned_nodes_sum",
    "nb_regular_elements_dim3",
    "nb_cohesive_elements",
    "nb_insertion_steps",
    "first_insertion_step",
    "energy_dissipated",
    "energy_potential",
    "energy_kinetic",
    "displacement_l2_norm",
    "displacement_max_abs",
    "displacement_sum",
    "fill_nodes_to_elements",
];

/// Exact JSON equality, numbers compared as `f64` bit patterns (the oracle
/// file holds shortest round-trip forms, the port `%.17g`; sign-of-zero
/// strict), object keys in any order.
fn same(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(_), Value::Number(_)) => num(a).to_bits() == num(b).to_bits(),
        (Value::Array(x), Value::Array(y)) => {
            x.len() == y.len() && x.iter().zip(y).all(|(p, q)| same(p, q))
        }
        (Value::Object(x), Value::Object(y)) => {
            x.len() == y.len() && x.iter().all(|(k, v)| y.get(k).is_some_and(|w| same(v, w)))
        }
        _ => a == b,
    }
}

/// Full-length comparison of `summary.json` and `summary_rank0.json`.
fn compare_summaries(name: &str, out: &Path) -> usize {
    let want = json(&fixture_root().join(name).join("summary.json"));
    let got = json(&out.join("summary.json"));
    let missing = Value::Null;
    let mut n = 0;
    for key in SUMMARY_KEYS {
        let g = got.get(key).unwrap_or(&missing);
        if key == "fill_nodes_to_elements" {
            // C++ interposer mode (upstream/joss); the port keeps no
            // node→element map (mode "none")
            assert_eq!(g, &Value::from("none"), "{name}: {key}");
            continue;
        }
        let w = want.get(key).unwrap_or(&missing);
        assert!(
            same(g, w),
            "{name}: summary.json {key}: got {g:?}, oracle {w:?}"
        );
        n += 1;
    }
    let want = json(&fixture_root().join(name).join("summary_rank0.json"));
    let got = json(&out.join("summary_rank0.json"));
    let Value::Object(w) = &want else {
        panic!("{name}: summary_rank0.json is not an object");
    };
    let keys: BTreeMap<_, _> = w.iter().map(|(k, v)| (k.as_str(), v)).collect();
    for (key, value) in keys {
        if key == "fill_nodes_to_elements" {
            continue;
        }
        let g = got.get(key);
        assert!(
            g.is_some_and(|v| same(v, value)),
            "{name}: summary_rank0.json {key}: got {g:?}, oracle {value:?}"
        );
        n += 1;
    }
    n
}

/// Δt against the oracle (bitwise), at any length.
fn compare_time_step(name: &str, report: &RunReport) {
    let want = num(json(&fixture_root().join(name).join("summary.json"))
        .get("time_step")
        .expect("time_step"));
    assert_eq!(
        report.summary.time_step.to_bits(),
        want.to_bits(),
        "{name}: time step {} vs oracle {want}",
        report.summary.time_step
    );
}

fn replay(name: &str, default_steps: usize) {
    let full: usize = match json(&fixture_root().join(name).join("summary.json"))
        .get("max_steps")
        .and_then(Value::as_u64)
    {
        Some(n) => usize::try_from(n).expect("max_steps"),
        None => panic!("{name}: max_steps missing"),
    };
    let steps = steps_for(full, default_steps);
    let (report, out) = run_case(name, steps);
    compare_time_step(name, &report);
    let energies = compare_energies(name, &report, steps);
    let insertions = compare_insertions(name, &report, steps);
    let summary = if steps == full {
        compare_summaries(name, &out)
    } else {
        0
    };
    eprintln!(
        "{name}: {steps}/{full} steps bitwise: {energies} energies, {insertions} insertion steps, \
         {summary} summary values, Δt"
    );
}

/// `coh_np1_eigen`: cohesive insertion on (404 cohesive elements, 294
/// insertion steps from step 710), every step.
#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "release tier: cargo test --release -p akantu-bench"
)]
fn coh_np1_eigen_bitwise() {
    replay("coh_np1_eigen", 1500);
}

/// `ela_np1_eigen`: the same model without insertion.
#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "release tier: cargo test --release -p akantu-bench"
)]
fn ela_np1_eigen_bitwise() {
    replay("ela_np1_eigen", env_steps("PERF_ELA_STEPS").unwrap_or(200));
}

/// serde_json with `float_roundtrip` reads floats correctly rounded and
/// keeps the sign of zero, and the bitwise comparison sees one ULP (fast
/// tier). Without the feature the first assertion fails (`…4482` instead of
/// `…4481`).
#[test]
fn serde_json_reads_numbers_correctly_rounded() {
    for t in [
        "1.6271440317223231e-07",
        "1.627144031722323e-07",
        "-0",
        "-0.0",
        "4.9406564584124654e-324",
        "2.2250738585072009e-308",
        "501943.5234829415",
    ] {
        let v: Value = serde_json::from_str(t).unwrap();
        let want: f64 = t.parse().unwrap();
        assert_eq!(num(&v).to_bits(), want.to_bits(), "{t}");
    }
    let v: Value = serde_json::from_str(
        r#"{"a": [1.6271440317223231e-07, -1, 0.5, -0], "b": {"c": true, "d": "x"}, "e": []}"#,
    )
    .unwrap();
    let w: Value = serde_json::from_str(
        r#"{"e": [], "b": {"d": "x", "c": true}, "a": [1.627144031722323e-07, -1.0, 5e-1, -0.0]}"#,
    )
    .unwrap();
    assert!(same(&v, &w));
    let ulp: Value = serde_json::from_str(
        r#"{"e": [], "b": {"d": "x", "c": true}, "a": [1.6271440317223233e-07, -1, 0.5, -0]}"#,
    )
    .unwrap();
    assert!(!same(&v, &ulp), "one ULP apart");
    let plus_zero: Value = serde_json::from_str(
        r#"{"e": [], "b": {"d": "x", "c": true}, "a": [1.6271440317223231e-07, -1, 0.5, 0]}"#,
    )
    .unwrap();
    assert!(!same(&v, &plus_zero), "sign of zero");
}

/// `rust_static_solve = false` (benchmark-only timing mode) marks every
/// output so that no consumer takes it for an upstream run:
/// no `static_solve` phase (`static_solve_skipped` instead), the
/// marker line first in `energies.csv` and `insertions.csv`, and
/// `"rust_static_solve": false` in both summaries. Runs in every build (the
/// elastic solid model, one step, no static solve).
#[test]
fn no_static_solve_runs_are_marked_in_every_output() {
    let dir = fixture_root().join("ela_np1_eigen");
    let out = Path::new(env!("CARGO_TARGET_TMPDIR")).join("perf_cohesive_ela_no_static");
    let _ = std::fs::remove_dir_all(&out);
    std::fs::create_dir_all(&out).unwrap();
    let mut cfg = Config::from_file(&dir.join("input.dat"), &dir, &out).unwrap();
    cfg.max_steps = 1;
    cfg.verbose = false;
    cfg.static_solve = false;
    cfg.model = akantu_bench::ModelChoice::Solid;
    let report = run(&Communicator::serial(), &cfg).unwrap();
    assert!(report.chrono.get("static_solve").is_none());
    assert_eq!(
        report.chrono.get("static_solve_skipped").map(|(_, n)| n),
        Some(1)
    );
    let chrono = read(&out.join("chrono.csv"));
    assert!(chrono.contains("\"static_solve_skipped\""), "{chrono}");
    assert!(!chrono.contains("\"static_solve\""), "{chrono}");
    for file in ["energies.csv", "insertions.csv"] {
        let text = read(&out.join(file));
        assert_eq!(
            text.lines().next(),
            Some(akantu_bench::driver::NO_STATIC_SOLVE_MARKER),
            "{file}"
        );
    }
    for file in ["summary.json", "summary_rank0.json"] {
        assert_eq!(
            json(&out.join(file)).get("rust_static_solve"),
            Some(&Value::Bool(false)),
            "{file}"
        );
    }
}

/// Parallel readiness (scope item 4): the elastic-only variant on
/// two ranks of the local world, with the C++ harness's frozen Scotch
/// partition of `coh_np2_global` and `blocked_node = global` (the 1-rank
/// problem), runs end to end with the cohesive model (`rust_model = auto`;
/// the cohesive model accepts distributed meshes; earlier the driver fell
/// back to the solid model). Exact checks: the global node and element counts, the owned-node
/// sum and that every rank holds the blocked node it should. The energies
/// against the 1-rank oracle are printed only (diagnostic: the parallel
/// assembly sums in another order; no 2-rank rule exists: the README's
/// earlier P = 2 band was withdrawn).
#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "release tier: cargo test --release -p akantu-bench"
)]
fn ela_np2_global_local_world_runs() {
    use akantu_bench::{BlockedNode, PartitionChoice};
    use std::time::Duration;
    const CASE: &str = "ela_np2_global_local";
    let steps = env_steps("PERF_NP2_STEPS").unwrap_or(100).min(1500);
    let dir = fixture_root().join("ela_np1_eigen");
    let out = Path::new(env!("CARGO_TARGET_TMPDIR")).join("perf_cohesive_ela_np2_global");
    let _ = std::fs::remove_dir_all(&out);
    std::fs::create_dir_all(&out).unwrap();
    let mut cfg = Config::from_file(&dir.join("input.dat"), &dir, &out).unwrap();
    cfg.max_steps = steps;
    cfg.verbose = false;
    cfg.blocked_node = BlockedNode::Global;
    cfg.partitioner =
        PartitionChoice::File(fixture_root().join("coh_np2_global").join("partition.csv"));
    let reports = akantu_comm::run_local(2, Duration::from_secs(3600), |c| run(&c, &cfg));
    let reports: Vec<RunReport> = reports.into_iter().map(Result::unwrap).collect();
    let s = &reports[0].summary;
    assert_eq!(s.psize, 2);
    assert_eq!(s.model, "cohesive");
    assert_eq!(
        (s.nb_global_nodes_start, s.nb_global_nodes_end),
        (4824, 4824)
    );
    assert_eq!(s.nb_owned_nodes_sum, 4824);
    assert_eq!(s.nb_regular_elements_dim3, 23156);
    let blocked: usize = reports
        .iter()
        .map(|r| r.rank_summary.blocked_y_nodes.len())
        .sum();
    assert!(
        blocked >= 1,
        "the node of global id 3 is blocked on its ranks"
    );
    for r in &reports {
        for b in &r.rank_summary.blocked_y_nodes {
            assert_eq!(b.global, 3);
            assert_eq!(b.x, [1.0, -1.0, -1.0]);
        }
    }
    let expected = fixture_energies("ela_np1_eigen");
    let mut max_rel = [0.0_f64; 5];
    for (row, (_, values)) in expected.iter().zip(&reports[0].energies) {
        for (k, v) in values.iter().enumerate() {
            let w: f64 = row[k + 1].parse().unwrap();
            let scale = v.abs().max(w.abs());
            if scale > 0.0 {
                max_rel[k] = max_rel[k].max((v - w).abs() / scale);
            }
        }
    }
    let want_dt = num(
        json(&fixture_root().join("ela_np1_eigen").join("summary.json"))
            .get("time_step")
            .expect("time_step"),
    );
    eprintln!(
        "{CASE}: {steps} steps on 2 local ranks; max rel. diff. vs ela_np1_eigen (diagnostic): \
         ed {:.2e}, ep {:.2e}, ek {:.2e}, ew {:.2e}, et {:.2e}; Δt rel {:.2e}",
        max_rel[0],
        max_rel[1],
        max_rel[2],
        max_rel[3],
        max_rel[4],
        (s.time_step - want_dt).abs() / want_dt
    );
}
