// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Integration test of
// the example crates/akantu/examples/bernoulli_beam_2_example (port of
// examples/c++/structural_mechanics_model/bernoulli_beam_2_example.cc,
// upstream ctest `bernoulli_beam_2_example-test`).

#[path = "../examples/bernoulli_beam_2_example/main.rs"]
#[allow(dead_code)] // the example's `main` is not called by the test
mod bernoulli_beam_2_example;
mod examples_common;

use akantu::core::GhostType;
use akantu::core::enums::SparseSolverType;
use bernoulli_beam_2_example as beam;
use examples_common::{assert_close_rel, oracle, scratch};

/// The static solution of the two-span beam (the port's default, UD-514
/// corrected) against its closed form (`TestStructBernoulli2`): rotations
/// `5.6/4800` and `−3.7/4800`, axial displacement `10/18`; the axial force
/// `N = E₂A₂/18` at every quadrature point.
#[test]
fn bernoulli_beam_2_example_static_solution() {
    let out = scratch("beam_static");
    let r = beam::run(&beam::example().for_test(&out)).unwrap();
    let e = assert_close_rel(
        "d1 d2 d3",
        &r.values[..3],
        &[5.6 / 4800., -3.7 / 4800., 10. / 18.],
        1e-12,
        "one direct solve of a 6-DOF well-conditioned system: rounding only",
    );
    println!("beam: d1 d2 d3 vs the closed form {e:e}");
    let s = r.model.stress(beam::TYPE, GhostType::NotGhost).unwrap();
    let n = 3e10 * 0.01 / 18.;
    for q in 0..s.len() {
        assert!((s.tuple(q)[0] - n).abs() <= 1e-12 * n, "N at point {q}");
    }
}

/// Oracle variant `bernoulli_beam_2_example_static` (`_static`, Eigen
/// sparse solver pinned): the printed values, the five values and the
/// displacement, stress and internal force arrays bitwise (`values_full.txt`,
/// 17 digits), the dump byte-identical.
#[test]
fn bernoulli_beam_2_example_static_matches_the_oracle() {
    let out = scratch("beam_static_eigen");
    let mut ex = beam::example().for_test(&out);
    ex.sparse_solver = Some(SparseSolverType::Eigen);
    let r = beam::run(&ex).unwrap();
    let o = oracle::oracle("bernoulli_beam_2_example_static");
    oracle::assert_stdout(
        "bernoulli_beam_2_example",
        &ex.transcript(),
        &o.stdout(),
        None,
    );
    let full = o.kept_lines("values_full.txt");
    let field = |name: &str| -> Vec<&str> {
        full.iter()
            .filter_map(|l| l.strip_prefix(name).and_then(|v| v.strip_prefix(' ')))
            .collect()
    };
    let five: Vec<&str> = ["d1", "d2", "d3", "M1", "M2"]
        .iter()
        .map(|n| field(n)[0])
        .collect();
    oracle::assert_csv_values_bitwise("d1 d2 d3 M1 M2", &r.values, &five);
    let u = r.model.displacement().unwrap().as_slice().to_vec();
    oracle::assert_csv_values_bitwise("displacement", &u, &field("displacement"));
    let s = r
        .model
        .stress(beam::TYPE, GhostType::NotGhost)
        .unwrap()
        .as_slice()
        .to_vec();
    oracle::assert_csv_values_bitwise("stress", &s, &field("stress"));
    let f = r.model.internal_force().unwrap().as_slice().to_vec();
    oracle::assert_csv_values_bitwise("internal_force", &f, &field("internal_force"));
    assert_eq!(
        o.assert_files_match(&out, true, &|f| f == "values_full.txt"),
        2
    );
}

/// The upstream binary (UD-514: `initFull()` = `_explicit_lumped_mass`, one
/// explicit step with the zero lumped mass of UD-005): every printed value
/// is `-nan` (`0/0`), the dump holds NaNs. `--upstream-compat` keeps that
/// analysis; the port's corrected lumped mass is `ρAL/2`, zero here too (the
/// sections have no density), and the port refuses to divide by it: the run
/// stops with an error naming the zero lumped mass (category A refusal), so
/// nothing of upstream's explicit step is compared.
#[test]
fn bernoulli_beam_2_example_upstream_explicit_step_is_refused() {
    let out = scratch("beam_explicit");
    let mut ex = beam::example().for_test(&out);
    ex.upstream_compat = true;
    let e = beam::run(&ex).unwrap_err();
    assert!(e.to_string().contains("lumped mass"), "{e}");
    let o = oracle::oracle("bernoulli_beam_2_example");
    assert_eq!(
        o.stdout(),
        " d1 = -nan\n d2 = -nan\n d3 = -nan\n M1 = -nan\n M2 = -nan\n"
    );
}
