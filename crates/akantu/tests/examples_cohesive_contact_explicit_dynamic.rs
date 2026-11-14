// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Integration test of
// the example crates/akantu/examples/cohesive_contact_explicit_dynamic (port
// of examples/c++/contact_mechanics_model/hertz/
// cohesive_contact_explicit_dynamic.cc, upstream ctest
// `cohesive_contact_explicit_dynamic-test`).

#[path = "../examples/cohesive_contact_explicit_dynamic/main.rs"]
#[allow(dead_code)] // the example's `main` is not called by the test
mod cohesive_contact_explicit_dynamic;
mod examples_common;

use std::collections::BTreeMap;

use cohesive_contact_explicit_dynamic as cc;
use examples_common::{oracle, scratch};

/// Every 50th step of the oracle variant
/// `cohesive_contact_explicit_dynamic_full`: step → `[nb_cohesive,
/// nb_contact, epot, ekin, sha256(displacement|velocity|gaps)]`.
fn oracle_records() -> BTreeMap<usize, Vec<String>> {
    let o = oracle::oracle("cohesive_contact_explicit_dynamic_full");
    oracle::kept_records(
        &o,
        "energy_full.csv",
        "step,nb_cohesive,nb_contact,potential,kinetic,sha256(displacement|velocity|gaps)",
        ',',
    )
    .into_iter()
    .map(|r| (r[0].parse().unwrap(), r[1..].to_vec()))
    .collect()
}

/// The whole upstream run (2 × 25 000 steps on a 15-node mesh), upstream's
/// defects selected, against the oracle: every 50th step the numbers of
/// cohesive and contact elements, the energies bitwise and the full state
/// digest; the 50 dumps byte-identical; the standard output and error
/// identical. Independent checks: cohesive elements are inserted while
/// the bar is pulled apart (two, on the `interface`), and contact
/// elements appear only on the crack faces after the insertion.
#[test]
fn cohesive_contact_explicit_dynamic_matches_the_oracle() {
    on_big_stack(|| run_and_compare("cohesive_contact_explicit_dynamic", true));
}

/// The whole run in the default mode (what `cargo run --example
/// cohesive_contact_explicit_dynamic` runs: corrected cohesive model,
/// contact model and coupler) next to the run with
/// upstream's defects (compared with the oracle by the test above). The
/// default reproduces upstream's solid state, energies, element counts and
/// text output bitwise; the one departure is the contact model's `gaps`
/// output from step 42 507 on, a rounding-level change (≤ 2e-14 m) of the
/// natural projection by the corrected metric (UD-363; the run with every
/// switch but `projection_metric` is the default run bitwise, measured),
/// which no computation reads here: displacement and
/// velocity stay those of the upstream run at every step. Checked: every
/// 50th step the counts and energies bitwise against the oracle (and its
/// state digest up to step 42 500), the standard output and error
/// identical, at every step displacement and velocity bitwise those of the
/// compat run, the gaps bitwise those of the compat run before step
/// 42 507 and different at it, within [`GAPS_DEPARTURE_BOUND`] of the compat
/// run's from there on.
#[test]
fn cohesive_contact_explicit_dynamic_default_mode_matches_the_oracle() {
    on_big_stack(default_mode);
}

/// First step at which the default mode's `gaps` depart from upstream's
/// (UD-363, see the test above).
const FIRST_GAPS_DEPARTURE: usize = 42_507;

/// Regression guard (NUMERICAL_VALIDATION, recorded as a guard): bound of
/// `max |Δgap|` (m)
/// between the default and the compat run from step 42 507 on; measured
/// 1.8e-14 m, rounded up. A trip after a legitimate change is re-measured
/// and re-set with its evidence, never widened in place.
const GAPS_DEPARTURE_BOUND: f64 = 2.0e-14;

/// Per step: digest of displacement and velocity, the gaps and the
/// oracle's state digest (displacement, velocity, gaps).
type States = Vec<(String, Vec<f64>, String)>;

fn record_states(compat: bool, name: &str) -> (cc::Outcome, String, String, States) {
    let out = scratch(name);
    let mut ex = cc::example().for_test(&out);
    ex.upstream_compat = compat;
    ex.dump = false;
    let mut states = Vec::new();
    let r = cc::run_with(&ex, |_, c| {
        let (u, v) = (c.solid.smm.displacement()?, c.solid.smm.velocity()?);
        let gaps = c.contact.gaps().as_slice().to_vec();
        states.push((
            oracle::state_sha(&[u.as_slice(), v.as_slice()]),
            gaps.to_vec(),
            oracle::state_sha(&[u.as_slice(), v.as_slice(), &gaps]),
        ));
        Ok(())
    })
    .unwrap();
    (r, ex.transcript(), ex.stderr_transcript(), states)
}

fn default_mode() {
    let want = oracle_records();
    let (_, _, _, upstream) =
        record_states(true, "cohesive_contact_explicit_dynamic_compat_states");
    let (r, stdout, stderr, states) =
        record_states(false, "cohesive_contact_explicit_dynamic_default");
    assert_eq!(states.len(), upstream.len());
    let mut compared = 0;
    for (k, ((counts, e), (sha, gaps, full))) in
        r.counts.iter().zip(&r.energies).zip(&states).enumerate()
    {
        assert_eq!(sha, &upstream[k].0, "displacement and velocity at step {k}");
        if k < FIRST_GAPS_DEPARTURE {
            assert_eq!(gaps, &upstream[k].1, "gaps at step {k}");
        }
        if let Some(w) = want.get(&k) {
            assert_eq!(
                [counts[0].to_string(), counts[1].to_string()],
                [w[0].clone(), w[1].clone()],
                "cohesive / contact elements at step {k}"
            );
            oracle::assert_csv_values_bitwise(&format!("energies at step {k}"), e, &[&w[2], &w[3]]);
            if k < FIRST_GAPS_DEPARTURE {
                assert_eq!(full, &w[4], "state at step {k}");
            }
            compared += 1;
        }
    }
    assert_eq!(compared, 1000);
    assert_ne!(
        states[FIRST_GAPS_DEPARTURE].1, upstream[FIRST_GAPS_DEPARTURE].1,
        "UD-363: the corrected projection departs at step {FIRST_GAPS_DEPARTURE}"
    );
    // regression guard: after the departure the gaps
    // stay within the measured level of the compat run's, max |Δgap| =
    // 1.8e-14 m (gaps of about 1.4e-6 m); bound 2e-14 m, the measured value
    // rounded up
    let mut worst = 0.0_f64;
    for k in FIRST_GAPS_DEPARTURE..states.len() {
        assert_eq!(states[k].1.len(), upstream[k].1.len(), "gaps at step {k}");
        for (a, b) in states[k].1.iter().zip(&upstream[k].1) {
            worst = worst.max((a - b).abs());
        }
    }
    assert!(
        worst <= GAPS_DEPARTURE_BOUND,
        "max |Δgap| after step {FIRST_GAPS_DEPARTURE} = {worst:e} m (guard {GAPS_DEPARTURE_BOUND:e} m)"
    );
    let o = oracle::oracle("cohesive_contact_explicit_dynamic");
    oracle::assert_stdout("cohesive_contact", &stdout, &o.stdout(), None);
    assert_eq!(stderr, o.stderr());
}

/// Runs `f` on a thread with a 16 MiB stack: unoptimised builds with
/// `akantu-core/export-all` (gate tier `exportall`) need slightly more than
/// the test threads' 2 MiB of stack on the coupled insertion path
/// (measured: 3 MiB suffice); the example's own main thread has 8 MiB.
fn on_big_stack(f: impl FnOnce() + Send + 'static) {
    std::thread::Builder::new()
        .stack_size(16 << 20)
        .spawn(f)
        .unwrap()
        .join()
        .unwrap();
}

fn run_and_compare(name: &str, compat: bool) {
    let out = scratch(name);
    let want = oracle_records();
    let mut ex = cc::example().for_test(&out);
    ex.upstream_compat = compat;
    let mut compared = 0;
    let r = cc::run_with(&ex, |k, c| {
        if let Some(w) = want.get(&k) {
            let sha = oracle::state_sha(&[
                c.solid.smm.displacement()?.as_slice(),
                c.solid.smm.velocity()?.as_slice(),
                c.contact.gaps().as_slice(),
            ]);
            assert_eq!(sha, w[4], "state at step {k}");
            compared += 1;
        }
        Ok(())
    })
    .unwrap();
    assert_eq!(compared, 1000);
    for (k, (counts, e)) in r.counts.iter().zip(&r.energies).enumerate() {
        if let Some(w) = want.get(&k) {
            assert_eq!(
                [counts[0].to_string(), counts[1].to_string()],
                [w[0].clone(), w[1].clone()],
                "cohesive / contact elements at step {k}"
            );
            oracle::assert_csv_values_bitwise(&format!("energies at step {k}"), e, &[&w[2], &w[3]]);
        }
    }
    let first_cohesive = r.counts.iter().position(|c| c[0] > 0).unwrap();
    let first_contact = r.counts.iter().position(|c| c[1] > 0).unwrap();
    assert!(
        first_contact > first_cohesive,
        "{first_cohesive} {first_contact}"
    );
    assert_eq!(r.counts.last().unwrap()[0], 2);
    let o = oracle::oracle("cohesive_contact_explicit_dynamic");
    oracle::assert_stdout("cohesive_contact", &ex.transcript(), &o.stdout(), None);
    assert_eq!(ex.stderr_transcript(), o.stderr());
    // UD-850 (C): after the insertion upstream dumps the contact model's
    // never-resized `blocked_dofs` (15 values for 18 points, an invalid
    // VTU); the port's has 18 (`false` for the new nodes): those files are
    // compared array by array, `blocked_dofs` through upstream's 15 values
    let short = |f: &str| {
        o.arrays
            .get(&(f.to_owned(), "blocked_dofs".to_owned()))
            .is_some_and(|a| a.nb_values == 15)
            && o.arrays
                .get(&(f.to_owned(), "positions".to_owned()))
                .is_some_and(|a| a.nb_values == 54)
    };
    let vtus: Vec<String> = o.digests.keys().filter(|f| short(f)).cloned().collect();
    assert_eq!(vtus.len(), 31, "{vtus:?}");
    let n = o.assert_files_match(&out, true, &|f| {
        vtus.iter().any(|v| v == f)
            || vtus.iter().any(|v| {
                let stem = v.rsplit('/').next().unwrap().trim_end_matches(".vtu");
                f.ends_with(&format!("{stem}.pvtu"))
            })
    });
    assert_eq!(n, 2 * (50 - 31));
    for v in &vtus {
        assert_eq!(
            o.assert_vtu_arrays_match(&out, v, &|name| name == "blocked_dofs"),
            11
        );
        let mine = oracle::vtu_payloads(&out.join(v));
        let (_, _, _, nvalues, payload) = mine.iter().find(|a| a.0 == "blocked_dofs").unwrap();
        assert_eq!(*nvalues, 18);
        assert!(
            payload[15..].iter().all(|&b| b == 0),
            "new nodes not blocked"
        );
        let want = &o.arrays[&(v.clone(), "blocked_dofs".to_owned())];
        assert_eq!(
            akantu_validation::checksum::sha256_bytes(&payload[..15]),
            want.sha256,
            "{v}: upstream's 15 blocked_dofs values"
        );
    }
}

/// The full-state oracle variant `cohesive_contact_explicit_dynamic_full` writes the same dumps as the
/// upstream binary (the records are its only addition).
#[test]
fn cohesive_contact_explicit_dynamic_full_variant_dumps_equal_the_upstream_binarys() {
    assert_eq!(
        oracle::assert_variant_dumps_equal(
            "cohesive_contact_explicit_dynamic",
            "cohesive_contact_explicit_dynamic_full"
        ),
        100
    );
}
