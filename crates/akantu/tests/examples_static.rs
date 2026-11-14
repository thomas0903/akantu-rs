// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Integration test of
// the example crates/akantu/examples/static (port of
// examples/c++/solid_mechanics_model/static/static.cc, upstream ctest
// `static-test`).

mod examples_common;
#[path = "../examples/static/main.rs"]
#[allow(dead_code)] // the example's `main` is not called by the test
mod static_example;

use akantu::core::{ElementType, GhostType};
use akantu::parser::param::ParamValue;
use examples_common::{assert_close_rel, files_below, read_vtu, rel_diff, scratch};

/// Independent check: with `ν = 0`, `u_x = 1e-4` on x = 0.01, `u_x = 0` on
/// x = 0 and `u_y = 0` on y = 0, the exact solution is the affine field
/// `u = (x/100, 0)`, which quadratic triangles reproduce: the solved
/// displacement, `∇u = diag(0.01, 0)` and `σ = diag(0.01 E, 0)` hold to
/// rounding. The initial and the solved states are dumped.
#[test]
fn static_reproduces_the_uniform_strain() {
    let out = scratch("static");
    let model = static_example::run(&static_example::example().for_test(&out)).unwrap();

    let x = {
        let m = model.physics.base.mesh.read().unwrap();
        m.nodes().as_slice().to_vec()
    };
    let u_g = model.displacement().unwrap();
    let u = u_g.as_slice();
    let exact: Vec<f64> = x
        .as_chunks::<2>()
        .0
        .iter()
        .flat_map(|p| [p[0] / 100., 0.0])
        .collect();
    // affine field from one sparse LU solve of a well-conditioned 2-D
    // problem with 37 nodes: rounding only (a few ulps of 1e-4)
    let e = assert_close_rel(
        "displacement vs the affine solution",
        u,
        &exact,
        1e-12,
        "one sparse LU solve of a well-conditioned 37-node problem: rounding only",
    );
    println!("static: displacement vs the affine solution {e:e}");

    let law = model.material(0).unwrap();
    let young = match law.get_param("E").unwrap() {
        ParamValue::Real(v) => v,
        other => panic!("E = {other:?}"),
    };
    let ty = ElementType::Triangle6;
    let gu = law.base().grad_u.get(ty, GhostType::NotGhost).unwrap();
    let sigma = law.base().stress.get(ty, GhostType::NotGhost).unwrap();
    assert!(!gu.as_slice().is_empty());
    for (g, s) in gu
        .as_slice()
        .as_chunks::<4>()
        .0
        .iter()
        .zip(sigma.as_slice().as_chunks::<4>().0.iter())
    {
        let want_g = [0.01, 0.0, 0.0, 0.0];
        let want_s = [0.01 * young, 0.0, 0.0, 0.0];
        for k in 0..4 {
            assert!((g[k] - want_g[k]).abs() < 1e-14, "grad_u {g:?}");
            assert!(
                (s[k] - want_s[k]).abs() < 1e-12 * young * 0.01,
                "stress {s:?}"
            );
        }
    }

    assert_eq!(
        files_below(&out),
        [
            "paraview/static-VTU/static_0000.vtu",
            "paraview/static-VTU/static_0001.vtu",
            "paraview/static_0000.pvtu",
            "paraview/static_0001.pvtu"
        ]
    );
    let first = read_vtu(&out.join("paraview/static-VTU/static_0000.vtu"));
    let last = read_vtu(&out.join("paraview/static-VTU/static_0001.vtu"));
    for name in [
        "displacement",
        "external_force",
        "internal_force",
        "blocked_dofs",
        "grad_u",
        "stress",
    ] {
        assert!(last.arrays.contains_key(name), "{name} dumped");
    }
    let du = last.array("displacement").values.reals();
    for (n, v) in du.as_chunks::<3>().0.iter().enumerate() {
        assert_eq!([v[0], v[1]], [u[2 * n], u[2 * n + 1]]);
    }
    // the initial dump holds the imposed values only
    let d0 = first.array("displacement").values.reals();
    for (v, p) in d0
        .as_chunks::<3>()
        .0
        .iter()
        .zip(x.as_chunks::<2>().0.iter())
    {
        let want = if p[0] == 0.01 { 1e-4 } else { 0.0 };
        assert_eq!(v[0], want);
    }
}

/// Oracle variant `static_eigen` (Eigen sparse solver pinned): both dumps
/// byte-identical.
#[test]
fn static_eigen_matches_the_oracle() {
    let out = scratch("static_eigen_oracle");
    let mut ex = static_example::example().for_test(&out);
    ex.sparse_solver = Some(akantu::core::enums::SparseSolverType::Eigen);
    static_example::run(&ex).unwrap();
    let o = examples_common::oracle::oracle("static_eigen");
    assert_eq!(o.assert_files_match(&out, true, &|_| false), 4);
}

/// Oracle, upstream binary (`_auto` = MUMPS): the arrays that do not
/// depend on the solve are bitwise; the solved ones are diagnostic
/// (MUMPS vs SparseLU rounding; printed, not asserted beyond the affine
/// solution of `static_reproduces_the_uniform_strain`).
#[test]
fn static_mumps_oracle_diagnostic() {
    let out = scratch("static_mumps_oracle");
    static_example::run(&static_example::example().for_test(&out)).unwrap();
    let o = examples_common::oracle::oracle("static");
    let solved = ["displacement", "internal_force", "grad_u", "stress"];
    let rel = "paraview/static-VTU/static_0001.vtu";
    let n = o.assert_vtu_arrays_match(&out, rel, &|name| solved.contains(&name));
    assert!(n >= 6);
    let mine = read_vtu(&out.join(rel));
    let theirs = read_vtu(&o.kept(rel));
    for name in solved {
        let d = rel_diff(
            mine.array(name).values.reals(),
            theirs.array(name).values.reals(),
        );
        println!("static (MUMPS oracle) {name}: max relative difference {d:e}");
    }
}
