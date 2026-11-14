// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Integration test of
// the example crates/akantu/examples/cohesive_extrinsic_ig_tg (port of
// examples/c++/solid_mechanics_cohesive_model/cohesive_extrinsic_ig_tg/
// cohesive_extrinsic_ig_tg.cc, upstream ctest `cohesive_extrinsic_ig_tg-test`).

#[path = "../examples/cohesive_extrinsic_ig_tg/main.rs"]
#[allow(dead_code)] // the example's `main` is not called by the test
mod cohesive_extrinsic_ig_tg;
mod examples_cohesive_common;
mod examples_common;

use akantu::core::{ElementType, GhostType};
use examples_cohesive_common::{
    assert_energies_bitwise, assert_oracle_status, assert_stdout_matches,
    assert_variant_matches_upstream,
};
use examples_common::{files_below, read_vtu, scratch};

/// The whole upstream run (1000 steps) with independent checks: the bulk
/// laws follow the physical names (`btop` above `y = 0`, `bbottom` below;
/// also in the dumped material index), every cohesive element on the grain
/// boundary `y = 0` took the transgranular law and every other one the
/// intergranular law (the rules selector), the nodes of the side groups
/// are blocked in their normal direction (interior nodes free) and moved by
/// the velocity functor (velocity `±0.1`, displacement = 1000 increments of
/// `0.1 Δt`), the standard output and the 102 dumps.
#[test]
fn cohesive_extrinsic_ig_tg_full_run() {
    let out = scratch("cohesive_extrinsic_ig_tg");
    let r =
        cohesive_extrinsic_ig_tg::run(&cohesive_extrinsic_ig_tg::example().for_test(&out)).unwrap();
    let smm = &r.model.smm;
    let (btop, bbottom) = (
        smm.material_index("btop").unwrap(),
        smm.material_index("bbottom").unwrap(),
    );
    let (tg, ig) = (
        smm.material_index("tg_cohesive").unwrap(),
        smm.material_index("ig_cohesive").unwrap(),
    );
    let mesh = r.mesh.read().unwrap();
    let nodes_g = mesh.nodes();
    let nodes = nodes_g.as_slice();

    // bulk laws by physical name
    let ty = ElementType::Triangle3;
    let laws = smm
        .material_by_element()
        .get(ty, GhostType::NotGhost)
        .unwrap()
        .as_slice()
        .to_vec();
    let bary = mesh.barycenters(ty, GhostType::NotGhost).unwrap();
    for (e, b) in bary.as_slice().as_chunks::<2>().0.iter().enumerate() {
        let want = if b[1] > 0.0 { btop } else { bbottom };
        assert_eq!(usize::try_from(laws[e]).unwrap(), want, "element {e}");
    }

    // cohesive laws by the rules
    let coh = ElementType::Cohesive2d4;
    let nb_coh = mesh.nb_element(coh, GhostType::NotGhost);
    assert!(nb_coh > 0, "cracks");
    assert_eq!(r.inserted.iter().sum::<usize>(), nb_coh);
    let claws = smm
        .material_by_element()
        .get(coh, GhostType::NotGhost)
        .unwrap()
        .as_slice()
        .to_vec();
    let conn = mesh.connectivity(coh, GhostType::NotGhost).unwrap();
    let (mut n_tg, mut n_ig) = (0, 0);
    assert_eq!(claws.len(), nb_coh);
    for (e, &law) in claws.iter().enumerate() {
        let c = conn.tuple(e);
        let y = |k: usize| nodes[usize::try_from(c[k]).unwrap() * 2 + 1];
        let on_boundary = y(0) == 0.0 && y(1) == 0.0;
        let law = usize::try_from(law).unwrap();
        if on_boundary {
            assert_eq!(law, tg, "cohesive element {e} on the grain boundary");
            n_tg += 1;
        } else {
            assert_eq!(law, ig, "cohesive element {e} inside a grain");
            n_ig += 1;
        }
    }
    assert!(n_ig > 0, "intergranular cracks ({n_tg} transgranular)");

    // boundary conditions: the nodes of each side's group (upstream
    // rebuilds a group's nodes from its elements after every insertion, so
    // a doubled boundary node stays in the group of the side elements
    // attached to it) are blocked normally, at velocity ±0.1, and moved by
    // the functor at every step (a copy inherits the original's history)
    let blocked_g = smm.blocked_dofs().unwrap();
    let blocked = blocked_g.as_slice();
    let v_g = smm.velocity().unwrap();
    let v = v_g.as_slice();
    let u_g = smm.displacement().unwrap();
    let u = u_g.as_slice();
    let mut step = 0.0;
    for _ in 0..1000 {
        step += 0.1 * r.time_step;
    }
    let mut on_sides = std::collections::BTreeSet::new();
    for (group, axis) in [("top", 1), ("bottom", 1), ("left", 0), ("right", 0)] {
        let g = mesh.groups().element_group(group).unwrap();
        assert!(g.nb_nodes() > 0);
        for &n in g.nodes().as_slice() {
            let n = usize::try_from(n).unwrap();
            on_sides.insert(n);
            let sign = if nodes[2 * n + axis] < 0.0 { -1.0 } else { 1.0 };
            assert!(blocked[2 * n + axis], "{group}: node {n}");
            assert_eq!(v[2 * n + axis], sign * 0.1, "{group}: velocity of node {n}");
            assert_eq!(
                u[2 * n + axis],
                sign * step,
                "{group}: displacement of node {n}"
            );
        }
    }
    for (n, x) in nodes.as_chunks::<2>().0.iter().enumerate() {
        if x[0].abs() != 1.0 && x[1].abs() != 1.0 {
            assert!(!on_sides.contains(&n));
            assert!(!blocked[2 * n] && !blocked[2 * n + 1], "interior node {n}");
        }
    }
    // UD-633 (upstream semantics, reproduced): a side node doubled by a
    // crack that reaches the side keeps its blocked flag but may leave the
    // side's group; it is then no longer moved (frozen where it was)
    let mut orphans = Vec::new();
    for (group, axis) in [("top", 1), ("bottom", 1), ("left", 0), ("right", 0)] {
        let g = mesh.groups().element_group(group).unwrap();
        let members: std::collections::BTreeSet<usize> = g
            .nodes()
            .iter()
            .map(|&n| usize::try_from(n).unwrap())
            .collect();
        let other = 1 - axis;
        for (n, x) in nodes.as_chunks::<2>().0.iter().enumerate() {
            let sign = if group == "top" || group == "right" {
                1.0
            } else {
                -1.0
            };
            if x[axis] == sign && x[other].abs() < 1.0 && !members.contains(&n) {
                assert!(blocked[2 * n + axis]);
                assert!(u[2 * n + axis].abs() < step, "{group}: node {n} is frozen");
                orphans.push((group, n));
            }
        }
    }
    assert!(!orphans.is_empty(), "the cracks reach the sides");

    assert_eq!(r.stdout.len(), 1 + 100);
    assert!(r.stdout[0].starts_with("Time step: "));
    assert_eq!(files_below(&out.join("paraview")).len(), 2 * 102 + 1);
    let last = read_vtu(&out.join("paraview/extrinsic-VTU/extrinsic_0101.vtu"));
    let mi = last.array("material_index").values.ints().to_vec();
    assert_eq!(mi.len(), laws.len());
    for (m, l) in mi.iter().zip(&laws) {
        assert_eq!(*m, i64::from(*l));
    }
}

/// The oracle runs: three identical runs of the upstream binary and of the
/// Eigen-pinned variant (D-035, D-050); the variant's dumps are the
/// upstream binary's.
#[test]
fn cohesive_extrinsic_ig_tg_oracle_fixtures_are_consistent() {
    assert_oracle_status("cohesive_extrinsic_ig_tg");
    assert_oracle_status("cohesive_extrinsic_ig_tg_full");
    let n = assert_variant_matches_upstream(
        "cohesive_extrinsic_ig_tg",
        "cohesive_extrinsic_ig_tg_full",
        "paraview/",
    );
    assert_eq!(n, 2 * 102 + 1);
}

/// The whole run against the oracle: the 205 dump files byte-identical,
/// the standard output identical, the number of cohesive elements and the
/// four energies after every step bitwise (variant
/// `cohesive_extrinsic_ig_tg_full`).
#[test]
fn cohesive_extrinsic_ig_tg_matches_the_oracle() {
    let out = scratch("cohesive_extrinsic_ig_tg_oracle");
    let r = cohesive_extrinsic_ig_tg::run_with(
        &cohesive_extrinsic_ig_tg::example().for_test(&out),
        cohesive_extrinsic_ig_tg::Options {
            record_energies: true,
            ..Default::default()
        },
    )
    .unwrap();
    let o = examples_common::oracle::oracle("cohesive_extrinsic_ig_tg");
    assert_eq!(o.assert_files_match(&out, true, &|_| false), 2 * 102 + 1);
    for k in [0, 1, 2, 50, 101] {
        let f = format!("paraview/extrinsic-VTU/extrinsic_{k:04}.vtu");
        assert!(o.assert_vtu_arrays_match(&out, &f, &|_| false) >= 11);
    }
    assert_stdout_matches("cohesive_extrinsic_ig_tg", &r.stdout, &|_| false);
    let got: Vec<(usize, usize, [f64; 4])> = r
        .energies
        .iter()
        .map(|e| (e.step, e.nb_cohesive, e.energies()))
        .collect();
    assert_eq!(got.len(), 1000);
    assert_energies_bitwise("cohesive_extrinsic_ig_tg_full", &got);
}

/// `--upstream-compat` (every upstream cohesive defect, D-003) changes
/// nothing in this example: the run is the oracle's too.
#[test]
fn cohesive_extrinsic_ig_tg_upstream_compat_matches_the_oracle() {
    let out = scratch("cohesive_extrinsic_ig_tg_compat");
    let mut ex = cohesive_extrinsic_ig_tg::example().for_test(&out);
    ex.upstream_compat = true;
    cohesive_extrinsic_ig_tg::run(&ex).unwrap();
    let o = examples_common::oracle::oracle("cohesive_extrinsic_ig_tg");
    assert_eq!(o.assert_files_match(&out, true, &|_| false), 2 * 102 + 1);
}

/// The functor's velocity write matters. With the
/// initial velocity `0.05 x` (oracle variant `cohesive_extrinsic_ig_tg_vel`)
/// the blocked side DOFs start at `±0.05`, and only the write gives them
/// the prescribed `±0.1` (the integration scheme skips blocked DOFs):
/// independently, the side nodes are at `±0.05` in the dump before the
/// loop and the side groups' nodes at `±0.1` after it; against the oracle,
/// the 205 dump files (velocity arrays included) byte-identical, the
/// standard output identical, the number of cohesive elements and the four
/// energies (kinetic included) after every step bitwise.
#[test]
fn cohesive_extrinsic_ig_tg_velocity_write_matches_the_oracle() {
    let case = "cohesive_extrinsic_ig_tg_vel";
    let out = scratch(case);
    let r = cohesive_extrinsic_ig_tg::run_with(
        &cohesive_extrinsic_ig_tg::example().for_test(&out),
        cohesive_extrinsic_ig_tg::Options {
            record_energies: true,
            initial_velocity_rate: Some(0.05),
        },
    )
    .unwrap();
    let mesh = r.mesh.read().unwrap();
    let nodes_g = mesh.nodes();
    let nodes = nodes_g.as_slice();

    // before the loop: the side nodes move at ±0.05 normally
    let initial = read_vtu(&out.join("paraview/extrinsic-VTU/extrinsic_0001.vtu"));
    let v0 = initial.array("velocity");
    let nc = v0.nb_component;
    let v0 = v0.values.reals();
    let mut nb_sides = 0;
    for n in 0..initial.nb_points {
        for axis in 0..2 {
            let x = nodes[2 * n + axis];
            if x.abs() == 1.0 {
                assert_eq!(v0[nc * n + axis], 0.05 * x, "initial velocity of node {n}");
                nb_sides += 1;
            }
        }
    }
    assert!(nb_sides > 0);
    // after the loop: the functor's ±0.1 on every node of the side groups
    let v_g = r.model.smm.velocity().unwrap();
    let v = v_g.as_slice();
    for (group, axis) in [("top", 1), ("bottom", 1), ("left", 0), ("right", 0)] {
        for &n in mesh
            .groups()
            .element_group(group)
            .unwrap()
            .nodes()
            .as_slice()
        {
            let n = usize::try_from(n).unwrap();
            let sign = if nodes[2 * n + axis] < 0.0 { -1.0 } else { 1.0 };
            assert_eq!(v[2 * n + axis], sign * 0.1, "{group}: velocity of node {n}");
        }
    }
    // the guards borrow the mesh lock and the model: end them first
    // (explicit guard `Drop`, D-071 (2))
    drop(v_g);
    drop(nodes_g);
    drop(mesh);

    // against the oracle variant
    assert_oracle_status(case);
    let o = examples_common::oracle::oracle(case);
    // every output of the variant but its own energy record
    let energy_record = |f: &str| f == "energy_full.csv";
    assert_eq!(
        o.assert_files_match(&out, true, &energy_record),
        2 * 102 + 1
    );
    for k in [0, 1, 2, 50, 101] {
        let f = format!("paraview/extrinsic-VTU/extrinsic_{k:04}.vtu");
        assert!(o.assert_vtu_arrays_match(&out, &f, &|_| false) >= 11);
    }
    assert_stdout_matches(case, &r.stdout, &|_| false);
    let got: Vec<(usize, usize, [f64; 4])> = r
        .energies
        .iter()
        .map(|e| (e.step, e.nb_cohesive, e.energies()))
        .collect();
    assert_eq!(got.len(), 1000);
    assert_energies_bitwise(case, &got);
}
