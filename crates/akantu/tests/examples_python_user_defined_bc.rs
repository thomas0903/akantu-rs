// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Integration test of
// the example crates/akantu/examples/python_user_defined_bc (port of
// examples/c++/solid_mechanics_model/boundary_conditions/
// python_user_defined_bc/python_user_defined_bc.cc, upstream ctest
// `python_user_defined_bc-test`; EX-012). Feature `python` (links libpython;
// NumPy must be importable by the embedded interpreter, docs/TESTING.md
// "Python examples").

mod examples_common;
#[path = "../examples/python_user_defined_bc/main.rs"]
#[allow(dead_code)] // the example's `main` is not called by the test
mod python_user_defined_bc;

use examples_common::{files_below, read_vtu, scratch};

/// One test function: the embedded interpreter's runtime admits one thread
/// at a time, and the harness runs test functions on parallel threads.
#[test]
fn python_user_defined_bc_runs_and_matches_the_oracle() {
    python_functor_writes_the_live_rows();
    python_user_defined_bc_matches_the_oracle();
}

/// The Python functor (`boundary_condition.SinBoundary(0.2, 10).compute`)
/// writes `u_y = −0.2 sin(10 y)` and blocks `y` on `Fixed_x` through the
/// row views (the writes reach the model's arrays: nothing is copied back);
/// `FixedValue(0, y)` blocks `y` on `Fixed_y`; no `x` DOF is blocked.
fn python_functor_writes_the_live_rows() {
    let out = scratch("python_user_defined_bc");
    let model =
        python_user_defined_bc::run(&python_user_defined_bc::example().for_test(&out)).unwrap();
    let x = {
        let m = model.physics.base.mesh.read().unwrap();
        m.nodes().as_slice().to_vec()
    };
    let blocked_g = model.blocked_dofs().unwrap();
    let blocked = blocked_g.as_slice();
    let u_g = model.displacement().unwrap();
    let u = u_g.as_slice();
    let mut nb_left = 0;
    for (n, p) in x.as_chunks::<2>().0.iter().enumerate() {
        let on_left = p[0] == 0.0;
        nb_left += usize::from(on_left);
        assert!(!blocked[2 * n], "node {n} x flag at {p:?}");
        assert_eq!(
            blocked[2 * n + 1],
            on_left || p[1] == 0.0,
            "node {n} y flag at {p:?}"
        );
        assert_eq!(u[2 * n], 0.0);
        if on_left && p[1] != 0.0 {
            // Python `math.sin` (the platform libm) on the Python side
            let uy = -0.2 * (10. * p[1]).sin();
            assert_eq!(
                u[2 * n + 1].to_bits(),
                uy.to_bits(),
                "node {n} u_y at {p:?}"
            );
        } else if !on_left {
            assert_eq!(u[2 * n + 1], 0.0);
        }
    }
    assert!(nb_left > 10, "{nb_left} nodes on Fixed_x");
    assert_eq!(
        files_below(&out),
        [
            "paraview/plate-VTU/plate_0000.vtu",
            "paraview/plate_0000.pvtu"
        ]
    );
    let vtu = read_vtu(&out.join("paraview/plate-VTU/plate_0000.vtu"));
    assert_eq!((vtu.nb_points, vtu.nb_cells), (513, 1024 - 80));
}

/// Oracle (`fixtures/examples/python_user_defined_bc`: the upstream binary,
/// three identical runs, x86_64): the dump byte for byte on x86_64 glibc;
/// elsewhere every array but the sine displacement (Python's `math.sin` is
/// the platform libm, D-043).
fn python_user_defined_bc_matches_the_oracle() {
    let out = scratch("python_user_defined_bc_oracle");
    python_user_defined_bc::run(&python_user_defined_bc::example().for_test(&out)).unwrap();
    let o = examples_common::oracle::oracle("python_user_defined_bc");
    let glibc = cfg!(all(
        target_os = "linux",
        target_env = "gnu",
        target_arch = "x86_64"
    ));
    let n = o.assert_vtu_arrays_match(&out, "paraview/plate-VTU/plate_0000.vtu", &|name| {
        name == "displacement" && !glibc
    });
    assert_eq!(n, if glibc { 7 } else { 6 });
    if glibc {
        assert_eq!(o.assert_files_match(&out, true, &|_| false), 2);
    }
}
