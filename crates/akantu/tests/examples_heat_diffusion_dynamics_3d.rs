// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Integration test of
// the example crates/akantu/examples/heat_diffusion_dynamics_3d (port of
// examples/c++/diffusion_model/heat_diffusion_dynamics_3d.cc, upstream ctest
// `heat_diffusion_dynamics_3d-test`).

mod examples_common;
#[path = "../examples/heat_diffusion_dynamics_3d/main.rs"]
#[allow(dead_code)] // the example's `main` is not called by the test
mod heat_diffusion_dynamics_3d;

use examples_common::{files_below, oracle, scratch};

/// Independent checks on a reduced run (101 steps): the heat source holds
/// the nodes within 0.1 of the centre at 300 (the lumped quadratic
/// tetrahedra oscillate around it: no discrete maximum principle), the mean
/// temperature rises; the
/// first two dumps and the first output lines equal the upstream run's.
#[test]
fn heat_diffusion_dynamics_3d_reduced_run_matches_the_oracle() {
    let out = scratch("heat_diffusion_dynamics_3d");
    let mut ex = heat_diffusion_dynamics_3d::example().for_test(&out);
    ex.steps = Some(101);
    let r = heat_diffusion_dynamics_3d::run(&ex).unwrap();
    assert!(r.nb_hot_nodes > 0);
    let t = r.model.temperature().unwrap().as_slice().to_vec();
    let blocked = r.model.blocked_dofs().unwrap().as_slice().to_vec();
    assert_eq!(blocked.iter().filter(|&&b| b).count(), r.nb_hot_nodes);
    for (k, (&v, &b)) in t.iter().zip(&blocked).enumerate() {
        // no bound on the free nodes: the lumped forward Euler step on
        // quadratic tetrahedra oscillates around the source (no discrete
        // maximum principle; measured minimum 98.898 after 100 steps)
        assert!(v.is_finite(), "T[{k}] = {v}");
        if b {
            assert_eq!(v, 300.0, "blocked node {k}");
        }
    }
    #[allow(clippy::cast_precision_loss)]
    let (mean, mean0) = (
        t.iter().sum::<f64>() / t.len() as f64,
        (100.0 * (t.len() - r.nb_hot_nodes) as f64 + 300.0 * r.nb_hot_nodes as f64)
            / t.len() as f64,
    );
    assert!(mean > mean0, "mean temperature {mean} vs initial {mean0}");
    let o = oracle::oracle("heat_diffusion_dynamics_3d");
    // the time-step lines (the step lines name the reduced count)
    oracle::assert_stdout(
        "heat_diffusion_dynamics_3d",
        &ex.transcript(),
        &o.stdout(),
        Some(2),
    );
    let n = o.assert_files_match(&out, false, &|f| f.ends_with(".pvd"));
    assert_eq!(n, 2 * 2);
    assert_eq!(files_below(&out.join("paraview")).len(), 2 * 2 + 1);
}

/// Medium tier (optimised builds only): the whole upstream run (1000
/// steps, 10 dumps and the `.pvd`) byte-identical, and the whole standard
/// output.
#[cfg(not(debug_assertions))]
#[test]
fn heat_diffusion_dynamics_3d_full_run_matches_the_oracle() {
    let out = scratch("heat_diffusion_dynamics_3d_full");
    let ex = heat_diffusion_dynamics_3d::example().for_test(&out);
    heat_diffusion_dynamics_3d::run(&ex).unwrap();
    let o = oracle::oracle("heat_diffusion_dynamics_3d");
    oracle::assert_stdout(
        "heat_diffusion_dynamics_3d",
        &ex.transcript(),
        &o.stdout(),
        None,
    );
    assert_eq!(o.assert_files_match(&out, true, &|_| false), 2 * 10 + 1);
}
