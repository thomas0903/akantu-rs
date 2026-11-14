// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Integration test of
// the example crates/akantu/examples/cohesive_extrinsic (port of
// examples/c++/solid_mechanics_cohesive_model/cohesive_extrinsic/
// cohesive_extrinsic.cc, upstream ctest `cohesive_extrinsic-test`).

#[path = "../examples/cohesive_extrinsic/main.rs"]
#[allow(dead_code)] // the example's `main` is not called by the test
mod cohesive_extrinsic;
mod examples_cohesive_common;
mod examples_common;

use akantu::core::{ElementType, GhostType};
use examples_cohesive_common::{
    assert_energies_bitwise, assert_oracle_status, assert_stdout_matches,
    assert_variant_matches_upstream,
};
use examples_common::{files_below, read_vtu, scratch};

/// Facets of the plate (quadratic triangles: `segment_3`) whose
/// barycentre has `0.20 ≤ y ≤ 0.30`, i.e. the insertion window.
fn window_facets(mesh: &akantu::mesh::Mesh) -> usize {
    let fm = mesh.mesh_facets().unwrap();
    let b = fm
        .barycenters(ElementType::Segment3, GhostType::NotGhost)
        .unwrap();
    b.as_slice()
        .as_chunks::<2>()
        .0
        .iter()
        .filter(|x| x[1] >= 0.20 && x[1] <= 0.30)
        .count()
}

/// The whole upstream run (1000 steps, 0.3 s in a debug build) with
/// independent checks: upstream's own criterion (dissipated energy
/// `200 √2 = G_c · 4 · √2/2` within 0.1 %), the crack is the four facets of
/// the insertion window, each inserted once (four `cohesive_2d_6`, nine
/// doubled nodes: 41 → 50), fully broken at the end (damage 1 at every
/// quadrature point), the time step, the standard output and the dump
/// series (101 dumps every 10 steps).
#[test]
fn cohesive_extrinsic_full_run() {
    let out = scratch("cohesive_extrinsic");
    let r = cohesive_extrinsic::run(&cohesive_extrinsic::example().for_test(&out)).unwrap();
    let rel = (r.dissipated - r.expected).abs() / r.expected;
    assert!(rel < 1e-3, "Ed = {} vs {}", r.dissipated, r.expected);

    let mesh = r.mesh.read().unwrap();
    assert_eq!(
        window_facets(&mesh),
        4 + 4,
        "the window's facets (both crack sides)"
    );
    assert_eq!(
        mesh.nb_element(ElementType::Cohesive2d6, GhostType::NotGhost),
        4
    );
    assert_eq!(r.inserted.iter().sum::<usize>(), 4);
    assert_eq!(mesh.nb_nodes(), 41 + 9);

    let law = r.model.cohesive_material("cohesive").unwrap();
    let damage = law
        .array("damage", ElementType::Cohesive2d6, GhostType::NotGhost)
        .unwrap();
    assert!(!damage.as_slice().is_empty());
    assert!(
        damage.as_slice().iter().all(|&d| d == 1.0),
        "{:?}",
        damage.as_slice()
    );

    assert_eq!(r.time_step, r.model.smm.time_step(None).unwrap());
    assert_eq!(r.stdout.len(), 1 + 100 + 1);
    assert!(r.stdout[0].starts_with("Time step: "));
    assert_eq!(r.stdout[100], "passing step 1000/1000");
    assert!(r.stdout[101].ends_with(" 282.843"), "{}", r.stdout[101]);

    let files = files_below(&out.join("paraview"));
    assert_eq!(files.len(), 2 * 101 + 1, "{files:?}");
    let last = read_vtu(&out.join("paraview/extrinsic-VTU/extrinsic_0100.vtu"));
    assert_eq!(last.nb_points, 50);
    // `addDumpFieldVector`: padded to 3 components
    let du = last.array("displacement");
    assert_eq!(du.nb_component, 3);
    let unpadded: Vec<f64> = du
        .values
        .reals()
        .as_chunks::<3>()
        .0
        .iter()
        .flat_map(|c| [c[0], c[1]])
        .collect();
    assert_eq!(unpadded, r.model.smm.displacement().unwrap().as_slice());
    assert_eq!(last.array("stress").nb_component, 4);
}

/// The oracle runs: three identical runs of the upstream binary and of the
/// Eigen-pinned variant (D-035, D-050); the variant's dumps are the
/// upstream binary's.
#[test]
fn cohesive_extrinsic_oracle_fixtures_are_consistent() {
    assert_oracle_status("cohesive_extrinsic");
    assert_oracle_status("cohesive_extrinsic_full");
    let n = assert_variant_matches_upstream(
        "cohesive_extrinsic",
        "cohesive_extrinsic_full",
        "paraview/",
    );
    assert_eq!(n, 2 * 101 + 1);
}

/// The whole run against the oracle: the 203 dump files byte-identical,
/// the standard output identical, and after every step the number of
/// cohesive elements and the dissipated, reversible, potential and kinetic
/// energies bitwise (variant `cohesive_extrinsic_full`). The energy queries
/// are read-only: the run that records them still writes the upstream
/// binary's dumps.
#[test]
fn cohesive_extrinsic_matches_the_oracle() {
    let out = scratch("cohesive_extrinsic_oracle");
    let r = cohesive_extrinsic::run_with(
        &cohesive_extrinsic::example().for_test(&out),
        cohesive_extrinsic::Options {
            record_energies: true,
        },
    )
    .unwrap();
    let o = examples_common::oracle::oracle("cohesive_extrinsic");
    let n = o.assert_files_match(&out, true, &|_| false);
    assert_eq!(n, 2 * 101 + 1);
    for k in [0, 1, 50, 100] {
        let f = format!("paraview/extrinsic-VTU/extrinsic_{k:04}.vtu");
        assert!(o.assert_vtu_arrays_match(&out, &f, &|_| false) >= 10);
    }
    assert_stdout_matches("cohesive_extrinsic", &r.stdout, &|_| false);
    let got: Vec<(usize, usize, [f64; 4])> = r
        .energies
        .iter()
        .map(|e| (e.step, e.nb_cohesive, e.energies()))
        .collect();
    assert_eq!(got.len(), 1000);
    assert_energies_bitwise("cohesive_extrinsic_full", &got);
}

/// `--upstream-compat` (every upstream cohesive defect, D-003) changes
/// nothing in this 2-D example: the 203 dump files, the standard output
/// and every step's energies are the oracle's too.
#[test]
fn cohesive_extrinsic_upstream_compat_matches_the_oracle() {
    let out = scratch("cohesive_extrinsic_compat");
    let mut ex = cohesive_extrinsic::example().for_test(&out);
    ex.upstream_compat = true;
    let r = cohesive_extrinsic::run_with(
        &ex,
        cohesive_extrinsic::Options {
            record_energies: true,
        },
    )
    .unwrap();
    let o = examples_common::oracle::oracle("cohesive_extrinsic");
    assert_eq!(o.assert_files_match(&out, true, &|_| false), 2 * 101 + 1);
    assert_stdout_matches("cohesive_extrinsic", &r.stdout, &|_| false);
    let got: Vec<(usize, usize, [f64; 4])> = r
        .energies
        .iter()
        .map(|e| (e.step, e.nb_cohesive, e.energies()))
        .collect();
    assert_eq!(got.len(), 1000);
    assert_energies_bitwise("cohesive_extrinsic_full", &got);
}

/// A reduced run (40 steps: the crack opens from step ≈ 20) is a prefix of
/// the oracle run: its dumps are the oracle's first five and its energies
/// the first 40 records.
#[test]
fn cohesive_extrinsic_reduced_run_is_a_prefix_of_the_oracle() {
    let out = scratch("cohesive_extrinsic_reduced");
    let mut ex = cohesive_extrinsic::example().for_test(&out);
    ex.steps = Some(40);
    let r = cohesive_extrinsic::run_with(
        &ex,
        cohesive_extrinsic::Options {
            record_energies: true,
        },
    )
    .unwrap();
    assert!(
        r.inserted.iter().sum::<usize>() > 0,
        "insertion before step 40"
    );
    let o = examples_common::oracle::oracle("cohesive_extrinsic");
    let n = o.assert_files_match(&out, false, &|f| f.ends_with(".pvd"));
    assert_eq!(n, 2 * 5);
    let got: Vec<(usize, usize, [f64; 4])> = r
        .energies
        .iter()
        .map(|e| (e.step, e.nb_cohesive, e.energies()))
        .collect();
    assert_energies_bitwise("cohesive_extrinsic_full", &got);
}
