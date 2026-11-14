// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Integration test of
// the example crates/akantu/examples/contact_explicit_dynamic (port of
// examples/c++/contact_mechanics_model/hertz/contact_explicit_dynamic.cc,
// upstream ctest `contact_explicit_dynamic-test`).

#[path = "../examples/contact_explicit_dynamic/main.rs"]
#[allow(dead_code)] // the example's `main` is not called by the test
mod contact_explicit_dynamic;
mod examples_common;

use std::collections::BTreeMap;

use akantu::model::Model;
use contact_explicit_dynamic as hertz;
use examples_common::{oracle, scratch};

/// Every 20th step of the oracle variant `contact_explicit_dynamic_full`:
/// step → `[nb_contact, epot, ekin, sha256(displacement|velocity|gaps)]`.
fn oracle_records() -> BTreeMap<usize, Vec<String>> {
    let o = oracle::oracle("contact_explicit_dynamic_full");
    oracle::kept_records(
        &o,
        "energy_full.csv",
        "step,nb_contact,potential,kinetic,sha256(displacement|velocity|gaps)",
        ',',
    )
    .into_iter()
    .map(|r| (r[0].parse().unwrap(), r[1..].to_vec()))
    .collect()
}

/// A run checking every 20th step against the oracle variant (contact
/// elements, energies bitwise, full state digest), with upstream's contact
/// defects selected (`compat`) or in the default mode (corrected contact
/// model and coupler: in this set-up, frictionless contact with a rigid
/// master, the switches are inert and the default reproduces upstream).
/// After every step it also checks that the `loading` edge carries the
/// imposed increments (`u_y = −(i + 1) · 5e-3 / 20000`, accumulated as the
/// boundary condition accumulates them) and returns the number of
/// compared steps.
fn run_compared(
    out: &std::path::Path,
    steps: Option<usize>,
    compat: bool,
) -> (hertz::Outcome, String, usize) {
    let want = oracle_records();
    let mut ex = hertz::example().for_test(out);
    ex.steps = steps;
    ex.upstream_compat = compat;
    let mut compared = 0;
    let increment = 5e-3 / 20000.;
    let mut imposed = 0.0;
    let mut loading: Vec<usize> = Vec::new();
    let r = hertz::run_with(&ex, |i, c| {
        if loading.is_empty() {
            let mesh = c.solid.mesh().read()?;
            let g = mesh.groups().element_group("loading")?;
            loading = g
                .nodes()
                .as_slice()
                .iter()
                .map(|&n| usize::try_from(n).unwrap())
                .collect();
            assert!(!loading.is_empty());
        }
        imposed += -increment;
        let u = c.solid.displacement()?;
        for &n in &loading {
            assert_eq!(
                u.as_slice()[2 * n + 1],
                imposed,
                "imposed displacement of node {n} at step {i}"
            );
        }
        if let Some(w) = want.get(&i) {
            let sha = oracle::state_sha(&[
                c.solid.displacement()?.as_slice(),
                c.solid.velocity()?.as_slice(),
                c.contact.gaps().as_slice(),
            ]);
            assert_eq!(
                c.contact.contact_elements().len().to_string(),
                w[0],
                "contact elements at step {i}"
            );
            assert_eq!(sha, w[3], "state at step {i}");
            compared += 1;
        }
        Ok(())
    })
    .unwrap();
    for (i, e) in r.energies.iter().enumerate() {
        if let Some(w) = want.get(&i) {
            oracle::assert_csv_values_bitwise(&format!("energies at step {i}"), e, &[&w[1], &w[2]]);
        }
    }
    (r, ex.stderr_transcript(), compared)
}

/// Reduced run (2001 steps): the state every 20 steps bitwise, the three
/// dumps byte-identical to the upstream binary's, its first energy lines
/// (standard error) and its time-step line; independent checks: the
/// half-disc comes into contact (contact elements appear, the contact
/// zone only grows while it is pushed down: the number of contact elements
/// never decreases), the imposed edge carries the increments (every step,
/// see [`run_compared`]).
fn reduced(name: &str, compat: bool) {
    let out = scratch(name);
    let (r, stderr, compared) = run_compared(&out, Some(2001), compat);
    assert_eq!(compared, 101);
    assert!(
        r.nb_contact.last().copied().unwrap_or(0) > 0,
        "no contact after 2000 steps"
    );
    for (i, w) in r.nb_contact.windows(2).enumerate() {
        assert!(
            w[1] >= w[0],
            "the contact zone shrinks at step {}: {} -> {}",
            i + 1,
            w[0],
            w[1]
        );
    }
    let o = oracle::oracle("contact_explicit_dynamic");
    let theirs = o.stderr();
    let want: Vec<&str> = theirs.lines().take(3).collect();
    assert_eq!(stderr.lines().collect::<Vec<_>>(), want);
    assert_eq!(o.assert_files_match(&out, false, &|_| false), 2 * 3);
    let first = o.stdout();
    assert!(first.starts_with(&format!(
        "Time Step = {}s",
        examples_common_cout(r.time_step)
    )));
}

/// [`reduced`] with upstream's contact defects selected (`--upstream-compat`).
#[test]
fn contact_explicit_dynamic_reduced_matches_the_oracle() {
    reduced("contact_explicit_dynamic", true);
}

/// [`reduced`] in the default mode (what `cargo run --example
/// contact_explicit_dynamic` runs): the same oracle
/// comparison holds bitwise, the switches being inert here.
#[test]
fn contact_explicit_dynamic_default_mode_reduced_matches_the_oracle() {
    reduced("contact_explicit_dynamic_default", false);
}

/// The time step as printed (`std::cout`, precision 6).
fn examples_common_cout(x: f64) -> String {
    akantu::parser::stream::format_general(x, 6)
}

/// Medium tier (optimised builds only): the whole run (20 000 steps): every
/// 20th state bitwise, the 20 dumps, the standard output and the standard
/// error identical to the upstream binary's.
#[cfg(not(debug_assertions))]
#[test]
fn contact_explicit_dynamic_full_run_matches_the_oracle() {
    let out = scratch("contact_explicit_dynamic_full");
    let (_r, stderr, compared) = run_compared(&out, None, true);
    assert_eq!(compared, 1000);
    let o = oracle::oracle("contact_explicit_dynamic");
    assert_eq!(stderr, o.stderr());
    assert_eq!(o.assert_files_match(&out, true, &|_| false), 2 * 20);
}

/// The full-state oracle variant `contact_explicit_dynamic_full` writes the same dumps as the
/// upstream binary (the records are its only addition).
#[test]
fn contact_explicit_dynamic_full_variant_dumps_equal_the_upstream_binarys() {
    assert_eq!(
        oracle::assert_variant_dumps_equal(
            "contact_explicit_dynamic",
            "contact_explicit_dynamic_full"
        ),
        40
    );
}
