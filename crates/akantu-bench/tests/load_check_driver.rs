// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). The
// driver's per-copy check of the loads (`rust_load_check_*`) on the
// benchmark of performance-testing-cohesive @ 483bace8
// (cohesive_extrinsic.cc), checked against the C++ oracle run
// fixtures/perf_cohesive/coh_np1_eigen.

//! The per-copy check of the loads in the benchmark loop
//! (`akantu_bench::load_check`, input keys `rust_load_check_every` /
//! `rust_load_check_steps`), serial, on the frozen h = 0.12 cube for the
//! oracle case's 1,500 steps, a check at every step:
//!
//! 1. **upstream's placement** (the oracle replay's settings): the energies
//!    and insertions with the checks are the C++ oracle's, bit for bit (the
//!    check disturbs nothing), and the check fails from the first step with
//!    a doubled loaded node on, never before (the negative control);
//! 2. **the corrected placement** (`rust_doubled_node_load =
//!    follows_elements`): every check passes, with doubled loaded nodes and
//!    moved records (not vacuous), and `energies.csv` / `insertions.csv`
//!    are byte-identical to a run without the keys, whose `summary.json`
//!    has none of the check's keys.
//!
//! Cost: three 1,500-step runs with the h = 0.12 static solve (≈ 85 s each
//! unoptimised), so the tests run in the release tier (`cargo test
//! --release -p akantu-bench`), as the oracle replays of
//! `perf_cohesive_fixture`.

use std::path::{Path, PathBuf};

use akantu_bench::config::DoubledNodeLoad;
use akantu_bench::load_check::LoadCheckConfig;
use akantu_bench::output::g;
use akantu_bench::{Config, RunReport, run};
use akantu_comm::Communicator;

fn fixture_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/perf_cohesive")
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn run_case(tag: &str, placement: DoubledNodeLoad, every: Option<&str>) -> (RunReport, PathBuf) {
    let dir = fixture_root().join("coh_np1_eigen");
    let out = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("load_check_driver_{tag}"));
    let _ = std::fs::remove_dir_all(&out);
    std::fs::create_dir_all(&out).unwrap();
    let mut cfg = Config::from_file(&dir.join("input.dat"), &dir, &out).unwrap();
    cfg.verbose = false;
    cfg.doubled_node_load = placement;
    cfg.load_check = LoadCheckConfig::parse(every.map(str::to_owned), None).unwrap();
    let report = run(&Communicator::serial(), &cfg).unwrap();
    (report, out)
}

/// One row of `load_check.csv`: (step, doubled loaded nodes, records moved,
/// max rel, verdict, copies differing from the re-application).
type Row = (usize, u64, u64, f64, String, u64);

/// `load_check.csv` as [`Row`]s, checking the header's column positions.
fn global_rows(out: &Path) -> Vec<Row> {
    let text = read(&out.join("load_check.csv"));
    let mut lines = text.lines();
    let header: Vec<&str> = lines.next().unwrap().split(", ").collect();
    let col = |name: &str| header.iter().position(|h| *h == name).unwrap();
    let (cs, cd, cm, cr, cv, cc) = (
        col("step"),
        col("doubled_loaded"),
        col("moved_contributions_sum"),
        col("max_rel_diff"),
        col("verdict"),
        col("differing_copies"),
    );
    lines
        .map(|l| {
            let f: Vec<&str> = l.split(", ").collect();
            assert_eq!(f.len(), header.len(), "{l}");
            (
                f[cs].parse().unwrap(),
                f[cd].parse().unwrap(),
                f[cm].parse().unwrap(),
                f[cr].parse().unwrap(),
                f[cv].to_owned(),
                f[cc].parse().unwrap(),
            )
        })
        .collect()
}

/// Upstream's placement with a check at every step: the oracle's energies
/// and insertions bit for bit, and the check fails exactly from the first
/// doubled loaded node on.
#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "release tier: cargo test --release -p akantu-bench"
)]
fn with_upstreams_placement_the_check_fails_from_the_first_doubled_loaded_node() {
    let (report, out) = run_case("upstream", DoubledNodeLoad::Compat, Some("1"));
    assert_eq!(report.summary.doubled_node_load, "upstream");
    // the C++ oracle's energies (17-digit text) and insertions, every step
    let fixture = fixture_root().join("coh_np1_eigen");
    let want: Vec<String> = read(&fixture.join("energies.csv"))
        .lines()
        .skip(1)
        .map(str::to_owned)
        .collect();
    assert_eq!(report.energies.len(), want.len(), "energy rows");
    for ((step, v), w) in report.energies.iter().zip(&want) {
        let got = format!(
            "{step}, {}, {}, {}, {}, {}",
            g(v[0], 17),
            g(v[1], 17),
            g(v[2], 17),
            g(v[3], 17),
            g(v[4], 17)
        );
        assert_eq!(&got, w, "step {step}: the checks changed the run");
    }
    assert_eq!(
        read(&out.join("insertions.csv")),
        read(&fixture.join("insertions.csv")),
        "insertions"
    );
    let rows = global_rows(&out);
    assert_eq!(rows.len(), 1501, "a check at steps 0..=1500");
    let first_loaded = rows
        .iter()
        .find(|r| r.1 > 0)
        .map(|r| r.0)
        .expect("the run doubles a loaded node");
    for (step, loaded, moved, rel, verdict, differing) in &rows {
        assert_eq!(*moved, 0, "step {step}: upstream's placement moves nothing");
        if *step < first_loaded {
            assert_eq!(*loaded, 0);
            assert!(
                *rel == 0.0 && *differing == 0 && verdict == "pass",
                "step {step}: {rel:e}"
            );
        }
    }
    let at = &rows[first_loaded];
    assert!(
        at.4 == "FAIL" && at.3 > 1e-3,
        "the first doubled loaded node at step {first_loaded} is misplaced: {at:?}"
    );
    let lc = report.summary.load_check.expect("summary");
    assert_eq!(lc.first_failed_step, Some(first_loaded));
    assert_eq!(lc.first_doubled_loaded_step, Some(first_loaded));
    // Serially every insertion that doubles a node copies
    // the facet rows into the boundary rows
    assert_eq!(lc.row_facet_mismatches_after_doubling, Some(0));
    eprintln!(
        "upstream's placement: first doubled loaded node at step {first_loaded} ({} nodes), \
         check {:.3e} there; {} of {} checks fail; largest {:.3e} at step {:?}",
        at.1, at.3, lc.failed_checks, lc.checks, lc.max_rel, lc.max_rel_step
    );
}

/// The corrected placement passes every check (not vacuously), and the
/// checks change no output of the run.
#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "release tier: cargo test --release -p akantu-bench"
)]
fn with_the_corrected_placement_every_check_passes_and_changes_nothing() {
    let (checked, out) = run_case("follows", DoubledNodeLoad::FollowsElements, Some("1"));
    let (plain, out_plain) = run_case("follows_plain", DoubledNodeLoad::FollowsElements, None);
    for f in ["energies.csv", "insertions.csv"] {
        assert_eq!(
            read(&out.join(f)),
            read(&out_plain.join(f)),
            "{f}: the checks changed the run"
        );
    }
    assert!(plain.summary.load_check.is_none());
    let plain_json = read(&out_plain.join("summary.json"));
    assert!(!plain_json.contains("rust_load_check"), "{plain_json}");
    assert!(!out_plain.join("load_check.csv").exists());
    let json = read(&out.join("summary.json"));
    assert!(
        json.contains("\"rust_load_check_every\": \"1\",")
            && json.contains("\"verdict\": \"pass\""),
        "{json}"
    );
    let rows = global_rows(&out);
    assert_eq!(rows.len(), 1501);
    assert!(rows.iter().all(|r| r.4 == "pass"), "every check passes");
    let last = rows.last().unwrap();
    assert!(last.1 > 0 && last.2 > 0, "not vacuous: {last:?}");
    let lc = checked.summary.load_check.expect("summary");
    assert_eq!((lc.failed_checks, lc.checks), (0, 1501));
    assert!(lc.max_rel <= akantu_bench::load_check::RTOL);
    // No failed check, and before the first doubled loaded
    // node the external force is its re-application bit for bit
    assert_eq!(lc.first_failed_step, None);
    let first_loaded = lc.first_doubled_loaded_step.expect("a doubled loaded node");
    for (step, loaded, moved, rel, _, differing) in &rows {
        if *step < first_loaded {
            assert!(
                *loaded == 0 && *moved == 0 && *rel == 0.0 && *differing == 0,
                "step {step}: not bitwise before the first doubled loaded node"
            );
        }
    }
    assert_eq!(lc.row_facet_mismatches_after_doubling, Some(0));
    eprintln!(
        "corrected placement: first doubled loaded node at step {:?}; at step 1500 {} doubled \
         loaded nodes, {} records moved; largest difference {:.3e} at step {:?}",
        lc.first_doubled_loaded_step, last.1, last.2, lc.max_rel, lc.max_rel_step
    );
}
