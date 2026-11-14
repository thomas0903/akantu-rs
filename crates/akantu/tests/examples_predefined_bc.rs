// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Integration test of
// the example crates/akantu/examples/predefined_bc (port of
// examples/c++/solid_mechanics_model/boundary_conditions/predefined_bc/
// predefined_bc.cc, upstream ctest `predefined_bc-test`).

mod examples_common;
#[path = "../examples/predefined_bc/main.rs"]
#[allow(dead_code)] // the example's `main` is not called by the test
mod predefined_bc;

use examples_common::{files_below, read_vtu, scratch};

/// The example runs, blocks `u_x` on `Fixed_x` (x = 0) and `u_y` on
/// `Fixed_y` (y = 0), leaves the displacement and the external force at
/// zero, and dumps exactly that state once.
#[test]
fn predefined_bc_blocks_the_fixed_sides() {
    let out = scratch("predefined_bc");
    let model = predefined_bc::run(&predefined_bc::example().for_test(&out)).unwrap();

    let x = {
        let m = model.physics.base.mesh.read().unwrap();
        m.nodes().as_slice().to_vec()
    };
    let blocked = model.blocked_dofs().unwrap().as_slice().to_vec();
    assert_eq!(x.len(), 24, "12 nodes of the frozen square.msh");
    for (n, p) in x.as_chunks::<2>().0.iter().enumerate() {
        assert_eq!(blocked[2 * n], p[0] == 0.0, "node {n} x flag at {p:?}");
        assert_eq!(blocked[2 * n + 1], p[1] == 0.0, "node {n} y flag at {p:?}");
    }
    assert!(
        model
            .displacement()
            .unwrap()
            .as_slice()
            .iter()
            .all(|&u| u == 0.0)
    );
    assert!(
        model
            .external_force()
            .unwrap()
            .as_slice()
            .iter()
            .all(|&f| f == 0.0)
    );

    assert_eq!(
        files_below(&out),
        [
            "paraview/plate-VTU/plate_0000.vtu",
            "paraview/plate_0000.pvtu"
        ]
    );
    let vtu = read_vtu(&out.join("paraview/plate-VTU/plate_0000.vtu"));
    assert_eq!((vtu.nb_points, vtu.nb_cells), (12, 14));
    let flags: Vec<i64> = blocked.iter().map(|&b| i64::from(b)).collect();
    assert_eq!(vtu.array("blocked_dofs").values.ints(), &flags[..]);
    assert_eq!(vtu.array("displacement").nb_component, 3, "padded vector");
    assert!(
        vtu.array("displacement")
            .values
            .reals()
            .iter()
            .all(|&u| u == 0.0)
    );
}

/// Oracle (`fixtures/examples/predefined_bc`, the upstream binary on the
/// frozen mesh): the dump is byte-identical.
#[test]
fn predefined_bc_matches_the_oracle() {
    let out = scratch("predefined_bc_oracle");
    predefined_bc::run(&predefined_bc::example().for_test(&out)).unwrap();
    let o = examples_common::oracle::oracle("predefined_bc");
    assert_eq!(o.assert_files_match(&out, true, &|_| false), 2);
    for f in files_below(&out) {
        examples_common::assert_same_file(&out.join(&f), &o.kept(&f));
    }
}
