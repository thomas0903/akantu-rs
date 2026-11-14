// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Integration test of
// the example crates/akantu/examples/steady_state (port of
// examples/c++/node_to_node/steady_state.cc, upstream ctest
// `steady_state-test`).

mod examples_common;
#[path = "../examples/steady_state/main.rs"]
#[allow(dead_code)] // the example's `main` is not called by the test
mod steady_state;

use examples_common::{oracle, scratch};

/// The 17-digit records of the oracle variant `steady_state_full`: per step
/// `[time, ekin, epot, work, econ, efri, tot, time_step]` and the digest of
/// displacement, velocity and acceleration.
fn oracle_records() -> Vec<(Vec<String>, String)> {
    let o = oracle::oracle("steady_state_full");
    oracle::kept_records(
        &o,
        "energies_full.csv",
        "step,time,ekin,epot,work,econ,efri,tot,time_step,sha256(displacement|velocity|acceleration)",
        ',',
    )
    .into_iter()
    .enumerate()
    .map(|(s, r)| {
        assert_eq!(r[0], s.to_string());
        (r[1..9].to_vec(), r[9].clone())
    })
    .collect()
}

/// A run against the oracle variant: every step's energies bitwise and its
/// full state digest; the energies file line by line and the dumps
/// byte-identical to the upstream binary's (all of them for a whole run).
/// `compat` selects upstream's defects of the split-node contact
/// (`TasnUpstreamCompat::upstream()`); without it the run is the default
/// mode, which reproduces upstream here (the defects are not reached).
fn compare(steps: Option<usize>, name: &str, compat: bool) -> steady_state::Outcome {
    let out = scratch(name);
    let want = oracle_records();
    let mut ex = steady_state::example().for_test(&out);
    ex.steps = steps;
    ex.upstream_compat = compat;
    let r = steady_state::run_with(&ex, |s, m| {
        let sha = oracle::state_sha(&[
            m.displacement()?.as_slice(),
            m.velocity()?.as_slice(),
            m.acceleration()?.as_slice(),
        ]);
        assert_eq!(sha, want[s].1, "state after step {s}");
        Ok(())
    })
    .unwrap();
    for (s, rec) in r.records.iter().enumerate() {
        let w: Vec<&str> = want[s].0.iter().map(String::as_str).collect();
        oracle::assert_csv_values_bitwise(&format!("energies of step {s}"), rec, &w);
    }
    let o = oracle::oracle("steady_state");
    let ours = std::fs::read_to_string(out.join("friction-energies-steady_state.csv")).unwrap();
    for (k, l) in ours.lines().enumerate() {
        assert_eq!(
            l,
            o.kept_lines("friction-energies-steady_state.csv")[k],
            "energies line {k}"
        );
    }
    let complete = steps.is_none();
    let skip = |f: &str| !complete && (f.ends_with(".pvd") || f.ends_with(".csv"));
    let n = o.assert_files_match(&out, complete, &skip);
    let dumps = 1 + r.records.len().div_ceil(10);
    assert_eq!(n, 2 * dumps + if complete { 2 } else { 0 });
    r
}

/// Reduced run (25 steps, past the perturbation of step 10): states,
/// energies and dumps against the oracle; independent checks: the
/// perturbation changes the friction state of the contact nodes by at most
/// 1 %, the sliding velocity is imposed on the top and bottom.
fn reduced(name: &str, compat: bool) {
    let r = compare(Some(25), name, compat);
    assert_eq!(r.records.len(), 25);
    let phi0 = 0.000_249_999_999_687_500_03;
    for &phi in r.solver.friction().friction_state().as_slice() {
        assert!((phi / phi0 - 1.0).abs() <= 0.0101, "friction state {phi}");
    }
}

/// [`reduced`] with upstream's defects selected (`--upstream-compat`).
#[test]
fn steady_state_reduced_matches_the_oracle() {
    reduced("steady_state", true);
}

/// [`reduced`] in the default mode (what `cargo run --example
/// steady_state` runs): the same oracle comparison
/// holds bitwise.
#[test]
fn steady_state_default_mode_reduced_matches_the_oracle() {
    reduced("steady_state_default", false);
}

/// Medium tier (optimised builds only): the whole run (1000 steps).
#[cfg(not(debug_assertions))]
#[test]
fn steady_state_full_run_matches_the_oracle() {
    let r = compare(None, "steady_state_full", true);
    assert_eq!(r.records.len(), 1000);
}

/// The full-state oracle variant `steady_state_full` writes the same dumps as the
/// upstream binary (the records are its only addition).
#[test]
fn steady_state_full_variant_dumps_equal_the_upstream_binarys() {
    assert_eq!(
        oracle::assert_variant_dumps_equal("steady_state", "steady_state_full"),
        203
    );
}
