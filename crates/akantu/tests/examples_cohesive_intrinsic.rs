// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Integration test of
// the example crates/akantu/examples/cohesive_intrinsic (port of
// examples/c++/solid_mechanics_cohesive_model/cohesive_intrinsic/
// cohesive_intrinsic.cc, upstream ctest `cohesive_intrinsic-test`).

#[path = "../examples/cohesive_intrinsic/main.rs"]
#[allow(dead_code)] // the example's `main` is not called by the test
mod cohesive_intrinsic;
mod examples_cohesive_common;
mod examples_common;

use akantu::core::{ElementType, GhostType};
use examples_cohesive_common::{
    assert_energies_bitwise, assert_oracle_status, assert_stdout_matches,
    assert_variant_matches_upstream,
};
use examples_common::{files_below, scratch};

/// The whole upstream run (350 steps) with independent checks: upstream's
/// criterion (dissipated energy `2 √2 = G_c · 4 · √2/2` within 0.1 %); the
/// intrinsic insertion put one `cohesive_2d_6` on each of the four facets
/// of the window `−0.26 ≤ x ≤ −0.24` (nine doubled nodes: 41 → 50) and
/// split the plate: no node moved with the right part belongs to an
/// element left of the crack; the moved nodes carry the 351 increments of
/// `0.01` (the crack opened past `δ_c = 2 G_c / σ_c = 2`, damage 1
/// everywhere); the standard output and the 351 dumps.
#[test]
fn cohesive_intrinsic_full_run() {
    let out = scratch("cohesive_intrinsic");
    let r = cohesive_intrinsic::run(&cohesive_intrinsic::example().for_test(&out)).unwrap();
    let rel = (r.dissipated - r.expected).abs() / r.expected;
    assert!(rel < 1e-3, "Ed = {} vs {}", r.dissipated, r.expected);

    let mesh = r.mesh.read().unwrap();
    assert_eq!(
        mesh.nb_element(ElementType::Cohesive2d6, GhostType::NotGhost),
        4
    );
    assert_eq!(mesh.nb_nodes(), 41 + 9);

    // the crack separates the two parts
    let moved: std::collections::BTreeSet<i64> =
        r.moved_nodes.iter().map(|&n| i64::from(n)).collect();
    assert_eq!(moved.len(), r.moved_nodes.len(), "each node once");
    let ty = ElementType::Triangle6;
    let conn = mesh.connectivity(ty, GhostType::NotGhost).unwrap();
    let bary = mesh.barycenters(ty, GhostType::NotGhost).unwrap();
    let mut left = 0;
    for (e, b) in bary.as_slice().as_chunks::<2>().0.iter().enumerate() {
        if b[0] <= -0.25 {
            left += 1;
            for &n in conn.tuple(e) {
                assert!(
                    !moved.contains(&i64::from(n)),
                    "node {n} of left element {e}"
                );
            }
        }
    }
    assert!(left > 0);

    let mut expected = 0.0;
    for _ in 0..351 {
        expected += 0.01;
    }
    let u_g = r.model.smm.displacement().unwrap();
    let u = u_g.as_slice();
    for &n in &r.moved_nodes {
        assert_eq!(
            u[usize::try_from(n).unwrap() * 2],
            expected,
            "u_x of node {n}"
        );
    }

    let law = r.model.cohesive_material("cohesive").unwrap();
    let damage = law
        .array("damage", ElementType::Cohesive2d6, GhostType::NotGhost)
        .unwrap();
    assert!(damage.as_slice().iter().all(|&d| d == 1.0));

    assert_eq!(r.stdout.len(), 1 + 350 + 1);
    assert!(r.stdout[0].starts_with("Time step: "));
    assert!(r.stdout[351].ends_with(" 2.82843"), "{}", r.stdout[351]);
    assert_eq!(files_below(&out.join("paraview")).len(), 2 * 351 + 1);
}

/// The oracle runs: three identical runs of the upstream binary and of the
/// Eigen-pinned variant (D-035, D-050); the variant's dumps are the
/// upstream binary's.
#[test]
fn cohesive_intrinsic_oracle_fixtures_are_consistent() {
    assert_oracle_status("cohesive_intrinsic");
    assert_oracle_status("cohesive_intrinsic_full");
    let n = assert_variant_matches_upstream(
        "cohesive_intrinsic",
        "cohesive_intrinsic_full",
        "paraview/",
    );
    assert_eq!(n, 2 * 351 + 1);
}

/// The whole run against the oracle: the 703 dump files byte-identical,
/// the standard output identical, the number of cohesive elements and the
/// four energies after every step bitwise (variant
/// `cohesive_intrinsic_full`).
#[test]
fn cohesive_intrinsic_matches_the_oracle() {
    let out = scratch("cohesive_intrinsic_oracle");
    let r = cohesive_intrinsic::run_with(
        &cohesive_intrinsic::example().for_test(&out),
        cohesive_intrinsic::Options {
            record_energies: true,
        },
    )
    .unwrap();
    let o = examples_common::oracle::oracle("cohesive_intrinsic");
    assert_eq!(o.assert_files_match(&out, true, &|_| false), 2 * 351 + 1);
    for k in [0, 1, 175, 350] {
        let f = format!("paraview/intrinsic-VTU/intrinsic_{k:04}.vtu");
        assert!(o.assert_vtu_arrays_match(&out, &f, &|_| false) >= 10);
    }
    assert_stdout_matches("cohesive_intrinsic", &r.stdout, &|_| false);
    let got: Vec<(usize, usize, [f64; 4])> = r
        .energies
        .iter()
        .map(|e| (e.step, e.nb_cohesive, e.energies()))
        .collect();
    assert_eq!(got.len(), 350);
    assert_energies_bitwise("cohesive_intrinsic_full", &got);
}

/// `--upstream-compat` (every upstream cohesive defect, D-003) changes
/// nothing in this example: the run is the oracle's too.
#[test]
fn cohesive_intrinsic_upstream_compat_matches_the_oracle() {
    let out = scratch("cohesive_intrinsic_compat");
    let mut ex = cohesive_intrinsic::example().for_test(&out);
    ex.upstream_compat = true;
    cohesive_intrinsic::run(&ex).unwrap();
    let o = examples_common::oracle::oracle("cohesive_intrinsic");
    assert_eq!(o.assert_files_match(&out, true, &|_| false), 2 * 351 + 1);
}
