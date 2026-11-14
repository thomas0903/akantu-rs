// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Integration test of
// the example crates/akantu/examples/heat_diffusion_static_2d (port of
// examples/c++/diffusion_model/heat_diffusion_static_2d.cc, upstream ctest
// `heat_diffusion_static_2d-test`).

mod examples_common;
#[path = "../examples/heat_diffusion_static_2d/main.rs"]
#[allow(dead_code)] // the example's `main` is not called by the test
mod heat_diffusion_static_2d;

use akantu::core::enums::SparseSolverType;
use examples_common::{files_below, oracle, scratch};

/// Independent checks of the static solution: the boundary of the square
/// stays at 100 and the hot spot at 300; the solution of the discrete
/// Laplace problem obeys the maximum principle (every temperature between
/// 100 and 300); the free nodes are in equilibrium (internal heat rate
/// zero up to the rounding of one sparse LU solve: `|r_i| ≤ 1e-9 ·
/// max |r|` where `max |r|` is the largest heat rate at a blocked node);
/// two dumps.
#[test]
fn heat_diffusion_static_2d_solution() {
    let out = scratch("heat_diffusion_static_2d");
    let model =
        heat_diffusion_static_2d::run(&heat_diffusion_static_2d::example().for_test(&out)).unwrap();
    let t = model.temperature().unwrap().as_slice().to_vec();
    let blocked = model.blocked_dofs().unwrap().as_slice().to_vec();
    let r = model.internal_heat_rate().unwrap().as_slice().to_vec();
    let rmax = r
        .iter()
        .zip(&blocked)
        .filter(|(_, b)| **b)
        .fold(0.0_f64, |m, (v, _)| m.max(v.abs()));
    assert!(rmax > 0.0);
    let (mut hot, mut cold) = (0, 0);
    for k in 0..t.len() {
        assert!((100.0..=300.0).contains(&t[k]), "T[{k}] = {}", t[k]);
        if blocked[k] {
            assert!(t[k] == 100.0 || t[k] == 300.0, "blocked T[{k}] = {}", t[k]);
            if t[k] == 300.0 {
                hot += 1;
            } else {
                cold += 1;
            }
        } else {
            assert!(
                r[k].abs() <= 1e-9 * rmax,
                "heat rate at free node {k}: {}",
                r[k]
            );
        }
    }
    assert!(hot > 0 && cold > 0);
    assert_eq!(files_below(&out).len(), 4);
}

/// Oracle variant `heat_diffusion_static_2d_eigen` (Eigen sparse solver
/// pinned): both dumps byte-identical.
#[test]
fn heat_diffusion_static_2d_eigen_matches_the_oracle() {
    let out = scratch("heat_diffusion_static_2d_eigen");
    let mut ex = heat_diffusion_static_2d::example().for_test(&out);
    ex.sparse_solver = Some(SparseSolverType::Eigen);
    heat_diffusion_static_2d::run(&ex).unwrap();
    let o = oracle::oracle("heat_diffusion_static_2d_eigen");
    assert_eq!(o.assert_files_match(&out, true, &|_| false), 4);
}

/// Oracle, upstream binary (`_auto` = MUMPS): the dump before the solve is
/// byte-identical; after the solve, the arrays that do not depend on it
/// (geometry, conductivity, blocked DOFs) are bitwise, the solved
/// temperature and heat rate are not compared (MUMPS vs SparseLU rounding;
/// the Eigen-pinned variant above compares them bitwise).
#[test]
fn heat_diffusion_static_2d_mumps_oracle_diagnostic() {
    let out = scratch("heat_diffusion_static_2d_mumps");
    heat_diffusion_static_2d::run(&heat_diffusion_static_2d::example().for_test(&out)).unwrap();
    let o = oracle::oracle("heat_diffusion_static_2d");
    let first = "paraview/heat_diffusion_static_2d-VTU/heat_diffusion_static_2d_0000.vtu";
    let solved = "paraview/heat_diffusion_static_2d-VTU/heat_diffusion_static_2d_0001.vtu";
    assert_eq!(
        o.assert_files_match(&out, false, &|f| f.ends_with("_0001.vtu")
            || f.ends_with(".pvtu")),
        1
    );
    let n0 = o.assert_vtu_arrays_match(&out, first, &|_| false);
    let n = o.assert_vtu_arrays_match(&out, solved, &|name| {
        name == "temperature" || name == "internal_heat_rate"
    });
    assert!(n0 >= 6 && n >= 4, "{n0} / {n} arrays compared");
}
