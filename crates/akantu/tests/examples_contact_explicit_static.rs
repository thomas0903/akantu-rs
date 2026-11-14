// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Integration test of
// the example crates/akantu/examples/contact_explicit_static (port of
// examples/c++/contact_mechanics_model/hertz/contact_explicit_static.cc,
// upstream ctest `contact_explicit_static-test`).

#[path = "../examples/contact_explicit_static/main.rs"]
#[allow(dead_code)] // the example's `main` is not called by the test
mod contact_explicit_static;
mod examples_common;

use akantu::core::enums::SparseSolverType;
use contact_explicit_static as hertz;
use examples_common::{oracle, scratch};

/// A run with the Eigen solver pinned, against the oracle variant
/// `contact_explicit_static_eigen`: after every step the number of contact
/// elements, the Newton iterations and the final error (bitwise); every
/// dump byte-identical (all of them for a whole run). `compat` selects
/// upstream's contact defects; without it the run is the default mode
/// (corrected contact model and coupler), which reproduces upstream here
/// (frictionless contact with a rigid master: the switches are inert).
fn compare(steps: Option<usize>, name: &str, compat: bool) -> hertz::Outcome {
    let out = scratch(name);
    let mut ex = hertz::example().for_test(&out);
    ex.steps = steps;
    ex.upstream_compat = compat;
    ex.sparse_solver = Some(SparseSolverType::Eigen);
    let r = hertz::run(&ex).unwrap();
    let o = oracle::oracle("contact_explicit_static_eigen");
    let want = oracle::kept_records(
        &o,
        "newton_full.csv",
        "step,nb_contact,nb_iterations,error",
        ',',
    );
    for (s, ((nb, it, err), w)) in r.newton.iter().zip(&want).enumerate() {
        assert_eq!(w[0], s.to_string());
        assert_eq!(
            (nb.to_string(), it.to_string()),
            (w[1].clone(), w[2].clone()),
            "step {s}"
        );
        oracle::assert_csv_values_bitwise(&format!("Newton error of step {s}"), &[*err], &[&w[3]]);
    }
    let complete = steps.is_none();
    let n = o.assert_files_match(&out, complete, &|f| f == "newton_full.csv");
    assert_eq!(n, 2 * r.newton.len());
    r
}

/// Reduced run (15 steps, contact from the first step on): Newton path,
/// contact elements and dumps against the oracle variant; independent
/// checks: every step converges, the contact zone grows with the load.
fn reduced(name: &str, compat: bool) {
    let r = compare(Some(15), name, compat);
    for w in r.newton.windows(2) {
        assert!(w[1].0 >= w[0].0, "the contact zone shrinks: {:?}", r.newton);
    }
    assert!(r.newton[0].0 > 0);
}

/// [`reduced`] with upstream's contact defects selected (`--upstream-compat`).
#[test]
fn contact_explicit_static_reduced_matches_the_oracle() {
    reduced("contact_explicit_static", true);
}

/// [`reduced`] in the default mode (what `cargo run --example
/// contact_explicit_static -- --solver eigen` runs):
/// the same oracle comparison holds bitwise.
#[test]
fn contact_explicit_static_default_mode_reduced_matches_the_oracle() {
    reduced("contact_explicit_static_default", false);
}

/// Medium tier (optimised builds only): the whole run (100 load steps).
#[cfg(not(debug_assertions))]
#[test]
fn contact_explicit_static_full_run_matches_the_oracle() {
    let r = compare(None, "contact_explicit_static_full", true);
    assert_eq!(r.newton.len(), 100);
    assert_eq!(r.newton[99].0, 15);
}

/// Oracle, upstream binary (`_auto` = MUMPS): the first dump's geometry
/// and the arrays that do not depend on the solve are those of the
/// Eigen-pinned run; the solved fields are compared by the variant above.
#[test]
fn contact_explicit_static_mumps_oracle_diagnostic() {
    let out = scratch("contact_explicit_static_mumps");
    let mut ex = hertz::example().for_test(&out);
    ex.steps = Some(1);
    ex.upstream_compat = true;
    hertz::run(&ex).unwrap();
    let o = oracle::oracle("contact_explicit_static");
    let rel = "paraview/contact-explicit-static-VTU/contact-explicit-static_0000.vtu";
    let solved = [
        "displacement",
        "normals",
        "contact_force",
        "internal_force",
        "gaps",
        "areas",
        "grad_u",
        "stress",
    ];
    let n = o.assert_vtu_arrays_match(&out, rel, &|name| solved.contains(&name));
    assert!(n >= 6, "{n} arrays");
}
