// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Integration test of
// the example crates/akantu/examples/user_defined_bc (port of
// examples/c++/solid_mechanics_model/boundary_conditions/user_defined_bc/
// user_defined_bc.cc, upstream ctest `user_defined_bc-test`).

mod examples_common;
#[path = "../examples/user_defined_bc/main.rs"]
#[allow(dead_code)] // the example's `main` is not called by the test
mod user_defined_bc;

use akantu::core::enums::SpatialDirection;
use akantu::model::boundary_condition::DirichletFunctor;
use examples_common::{files_below, read_vtu, scratch};
use user_defined_bc::SineBoundary;

/// The user functor blocks its axis and imposes `−A sin(φ y)`; it checks
/// the sizes like upstream's `DIRICHLET_SANITY_CHECK`.
#[test]
fn sine_boundary_functor() {
    let f = SineBoundary {
        amplitude: 0.2,
        phase: 10.,
        axis: SpatialDirection::X,
    };
    let mut flags = [false, false];
    let mut primal = [1.0, 2.0];
    f.apply(0, &mut flags, &mut primal, &[0.5, 0.25]).unwrap();
    assert_eq!(flags, [true, false]);
    assert_eq!(primal, [-0.2 * (10.0_f64 * 0.25).sin(), 2.0]);
    // primal longer than flags
    assert!(
        f.apply(0, &mut [false], &mut [0.0, 0.0], &[0.0, 0.0])
            .is_err()
    );
    // axis out of range
    let z = SineBoundary {
        axis: SpatialDirection::Z,
        ..f
    };
    assert!(z.apply(0, &mut flags, &mut primal, &[0.0, 0.0]).is_err());
}

/// The example applies the sine displacement on `Fixed_x` (x = 0), blocks
/// `u_y` on `Fixed_y` (y = 0) and distributes the traction `(0.2, 0.2)`
/// over `Traction` (x = 2, length 2): every loaded node is on x = 2 and the
/// nodal forces add up to `0.4` per component.
#[test]
fn user_defined_bc_applies_the_three_conditions() {
    let out = scratch("user_defined_bc");
    let model = user_defined_bc::run(&user_defined_bc::example().for_test(&out)).unwrap();

    let x = {
        let m = model.physics.base.mesh.read().unwrap();
        m.nodes().as_slice().to_vec()
    };
    let blocked_g = model.blocked_dofs().unwrap();
    let blocked = blocked_g.as_slice();
    let u_g = model.displacement().unwrap();
    let u = u_g.as_slice();
    let f_g = model.external_force().unwrap();
    let f = f_g.as_slice();
    let mut total = [0.0, 0.0];
    for (n, p) in x.as_chunks::<2>().0.iter().enumerate() {
        let on_left = p[0] == 0.0;
        assert_eq!(blocked[2 * n], on_left, "node {n} x flag at {p:?}");
        assert_eq!(blocked[2 * n + 1], p[1] == 0.0, "node {n} y flag at {p:?}");
        let ux = if on_left {
            -0.2 * (10. * p[1]).sin()
        } else {
            0.0
        };
        assert_eq!(u[2 * n].to_bits(), ux.to_bits(), "node {n} u_x at {p:?}");
        assert_eq!(u[2 * n + 1], 0.0);
        if p[0] != 2.0 {
            assert_eq!((f[2 * n], f[2 * n + 1]), (0.0, 0.0), "node {n} f at {p:?}");
        }
        total[0] += f[2 * n];
        total[1] += f[2 * n + 1];
    }
    for t in total {
        // sum of ~20 segment contributions t·l/2 (l = 0.1): rounding only
        assert!((t - 0.4).abs() <= 1e-14, "total force {t}");
    }

    assert_eq!(
        files_below(&out),
        [
            "paraview/plate-VTU/plate_0000.vtu",
            "paraview/plate_0000.pvtu"
        ]
    );
    let vtu = read_vtu(&out.join("paraview/plate-VTU/plate_0000.vtu"));
    // triangles of the frozen fine_mesh.msh (1024 elements, 80 of them segments)
    assert_eq!((vtu.nb_points, vtu.nb_cells), (513, 1024 - 80));
    let du = vtu.array("displacement");
    assert_eq!(du.nb_component, 3);
    for (n, v) in du.values.reals().as_chunks::<3>().0.iter().enumerate() {
        assert_eq!([v[0], v[1], v[2]], [u[2 * n], u[2 * n + 1], 0.0]);
    }
    let df = vtu.array("external_force");
    assert_eq!(
        df.nb_component, 3,
        "addDumpFieldVector pads to 3 components"
    );
}

/// Oracle (`fixtures/examples/user_defined_bc`, upstream binary): every
/// array of the dump but the sine displacement is bitwise on every
/// platform; the displacement (libm `sin`) and the files byte for byte on
/// x86_64 glibc, the authoritative platform (D-043; macOS is an enumerated
/// libm exception).
#[test]
fn user_defined_bc_matches_the_oracle() {
    let out = scratch("user_defined_bc_oracle");
    user_defined_bc::run(&user_defined_bc::example().for_test(&out)).unwrap();
    let o = examples_common::oracle::oracle("user_defined_bc");
    let glibc = cfg!(all(
        target_os = "linux",
        target_env = "gnu",
        target_arch = "x86_64"
    ));
    let n = o.assert_vtu_arrays_match(&out, "paraview/plate-VTU/plate_0000.vtu", &|name| {
        name == "displacement" && !glibc
    });
    // positions, connectivity, offsets, types, blocked_dofs, external_force
    // (+ displacement on x86_64 glibc)
    assert_eq!(n, if glibc { 7 } else { 6 });
    if glibc {
        assert_eq!(o.assert_files_match(&out, true, &|_| false), 2);
    }
}
