// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Integration test of
// the example crates/akantu/examples/phase_field_notch (port of
// examples/c++/phase_field_model/phase_field_notch.cc, upstream ctest
// `phase_field_notch-test`).

mod examples_common;
#[path = "../examples/phase_field_notch/main.rs"]
#[allow(dead_code)] // the example's `main` is not called by the test
mod phase_field_notch;

use akantu::core::enums::SparseSolverType;
use examples_common::{oracle, scratch};

/// The oracle variant's state digest after every load step
/// (`phase_field_notch_eigen`, `state_sha256.txt`).
fn oracle_digests() -> Vec<String> {
    let o = oracle::oracle("phase_field_notch_eigen");
    let lines = o.kept_lines("state_sha256.txt");
    assert_eq!(lines[0], "step\tsha256(displacement|damage)");
    lines[1..]
        .iter()
        .enumerate()
        .map(|(k, l)| {
            let (s, h) = l.split_once('\t').expect("`<step>\t<sha256>`");
            assert_eq!(s, (k + 1).to_string());
            h.to_owned()
        })
        .collect()
}

/// A run with the Eigen solver pinned, recording the state digest of every
/// load step; damage in `[0, 1]` at every step. `compat` selects upstream's
/// defects (`--upstream-compat`); without it the run is the default mode
/// (corrected coupler, phase-field model and `phasefield` tangent), which
/// reproduces upstream here (the defects are not reached).
fn run_eigen(
    out: &std::path::Path,
    steps: Option<usize>,
    compat: bool,
) -> (phase_field_notch::Outcome, Vec<String>, String) {
    let mut ex = phase_field_notch::example().for_test(out);
    ex.steps = steps;
    ex.upstream_compat = compat;
    ex.sparse_solver = Some(SparseSolverType::Eigen);
    let mut digests = Vec::new();
    let r = phase_field_notch::run_with(&ex, |_, c| {
        let u = c.solid.displacement()?;
        let d = c.phase.damage()?;
        for &x in d.as_slice() {
            assert!((0.0..=1.0).contains(&x), "damage {x}");
        }
        digests.push(oracle::state_sha(&[u.as_slice(), d.as_slice()]));
        Ok(())
    })
    .unwrap();
    let transcript = ex.transcript();
    (r, digests, transcript)
}

/// Reduced run (40 load steps) against the oracle variant: the state of
/// every load step bitwise, the initial dump byte-identical.
#[test]
fn phase_field_notch_reduced_matches_the_oracle() {
    reduced("phase_field_notch_reduced", true);
}

/// [`phase_field_notch_reduced_matches_the_oracle`] in the default mode
/// (what `cargo run --example phase_field_notch -- --solver eigen` runs):
/// the same oracle comparison holds bitwise.
#[test]
fn phase_field_notch_default_mode_reduced_matches_the_oracle() {
    reduced("phase_field_notch_reduced_default", false);
}

fn reduced(name: &str, compat: bool) {
    let out = scratch(name);
    let (r, digests, _) = run_eigen(&out, Some(40), compat);
    assert_eq!(r.nb_load_steps, 40);
    let want = oracle_digests();
    for (s, (g, w)) in digests.iter().zip(&want).enumerate() {
        assert_eq!(g, w, "state after load step {}", s + 1);
    }
    let o = oracle::oracle("phase_field_notch_eigen");
    // the initial dump (the clusters of a reduced run are not upstream's)
    let n = o.assert_files_match(&out, false, &|f| {
        !(f.ends_with("phase_notch_0000.vtu") || f.ends_with("phase_notch_0000.pvtu"))
    });
    assert_eq!(n, 2);
}

/// Medium tier (optimised builds only): the whole run (999 load steps)
/// against the oracle variant: every state bitwise, the 10 dumps, the two
/// cluster dumps (`crack_0`, `crack_1`) byte-identical, two clusters, the
/// standard output with the wall-clock timings masked.
#[cfg(not(debug_assertions))]
#[test]
fn phase_field_notch_full_run_matches_the_oracle() {
    let out = scratch("phase_field_notch_full");
    let (r, digests, transcript) = run_eigen(&out, None, true);
    assert_eq!(r.nb_clusters, 2);
    assert_eq!(digests, oracle_digests());
    let o = oracle::oracle("phase_field_notch_eigen");
    let n = o.assert_files_match(&out, true, &|f| f == "state_sha256.txt");
    assert!(n >= 2 * 10 + 4, "{n} files");
    assert_eq!(
        oracle::mask_numbers(&transcript),
        oracle::mask_numbers(&o.stdout())
    );
}

/// Oracle, upstream binary (`_auto` = MUMPS): the dump before any solve is
/// byte-identical; the clusters (`crack_0`, `crack_1`: the elements
/// whose damage stays below 0.15 somewhere) and their dumps, which the
/// rounding of the solver does not reach, are identical to the upstream
/// run's; `Nb clusters: 2`.
#[cfg(not(debug_assertions))]
#[test]
fn phase_field_notch_mumps_oracle_clusters() {
    let out = scratch("phase_field_notch_mumps");
    let mut ex = phase_field_notch::example().for_test(&out);
    ex.upstream_compat = true;
    let r = phase_field_notch::run(&ex).unwrap();
    assert_eq!(r.nb_clusters, 2);
    let o = oracle::oracle("phase_field_notch");
    let n = o.assert_files_match(&out, false, &|f| {
        !(f.contains("crack_") || f.ends_with("_0000.vtu") || f.ends_with("_0000.pvtu"))
    });
    assert!(n >= 2, "{n} files");
    assert!(o.stdout().ends_with("Nb clusters: 2\n"));
}
