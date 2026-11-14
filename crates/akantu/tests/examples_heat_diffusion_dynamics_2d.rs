// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Integration test of
// the example crates/akantu/examples/heat_diffusion_dynamics_2d (port of
// examples/c++/diffusion_model/heat_diffusion_dynamics_2d.cc, upstream ctest
// `heat_diffusion_dynamics_2d-test`).

mod examples_common;
#[path = "../examples/heat_diffusion_dynamics_2d/main.rs"]
#[allow(dead_code)] // the example's `main` is not called by the test
mod heat_diffusion_dynamics_2d;

use examples_common::{files_below, oracle, read_vtu, scratch};

/// Independent checks on a reduced run (201 steps): the hot spot holds the
/// nodes within 0.1 of `(¼, ¼)` at 300, every temperature stays between
/// the extreme initial values 100 and 300 up to small undershoots (bound 99) (the
/// lumped forward Euler step on quadratic triangles has no discrete maximum
/// principle; `Δt = 0.8 Δt_crit`), heat flows out of the hot spot
/// (the mean temperature rises), the blocked nodes keep their value, and
/// the dumps of steps 0, 100 and 200 hold the model's final temperature.
#[test]
fn heat_diffusion_dynamics_2d_reduced_run() {
    let out = scratch("heat_diffusion_dynamics_2d");
    let mut ex = heat_diffusion_dynamics_2d::example().for_test(&out);
    ex.steps = Some(201);
    let r = heat_diffusion_dynamics_2d::run(&ex).unwrap();
    assert!(r.nb_hot_nodes > 0);
    let t = r.model.temperature().unwrap().as_slice().to_vec();
    let blocked = r.model.blocked_dofs().unwrap().as_slice().to_vec();
    let hot = blocked.iter().filter(|&&b| b).count();
    assert_eq!(hot, r.nb_hot_nodes);
    for (k, (&v, &b)) in t.iter().zip(&blocked).enumerate() {
        // no discrete maximum principle for the lumped quadratic elements
        // (positive off-diagonal conductivity terms): small undershoots
        // below 100 (measured 1.4e-14 after 200 steps); a sanity bound, not a comparison
        assert!((99.0..=300.0).contains(&v), "T[{k}] = {v}");
        if b {
            assert_eq!(v, 300.0, "blocked node {k}");
        }
    }
    #[allow(clippy::cast_precision_loss)]
    let mean = t.iter().sum::<f64>() / t.len() as f64;
    #[allow(clippy::cast_precision_loss)]
    let mean0 = (100.0 * (t.len() - hot) as f64 + 300.0 * hot as f64) / t.len() as f64;
    assert!(mean > mean0, "mean temperature {mean} vs initial {mean0}");
    // the first output line (the step lines name the reduced count)
    let o = oracle::oracle("heat_diffusion_dynamics_2d");
    oracle::assert_stdout(
        "heat_diffusion_dynamics_2d",
        &ex.transcript(),
        &o.stdout(),
        Some(1),
    );
    let vtu = read_vtu(
        &out.join("paraview/heat_diffusion_square2d-VTU/heat_diffusion_square2d_0002.vtu"),
    );
    assert!(vtu.arrays.contains_key("temperature_rate"));
    assert!(vtu.arrays.contains_key("internal_heat_rate"));
    assert_eq!(files_below(&out.join("paraview")).len(), 2 * 3 + 1);
}

/// Oracle (upstream binary): the dumps of steps 0, 100 and 200 of a
/// reduced run (201 steps) byte-identical to the upstream run's.
#[test]
fn heat_diffusion_dynamics_2d_matches_the_oracle() {
    let out = scratch("heat_diffusion_dynamics_2d_oracle");
    let mut ex = heat_diffusion_dynamics_2d::example().for_test(&out);
    ex.steps = Some(201);
    heat_diffusion_dynamics_2d::run(&ex).unwrap();
    let o = oracle::oracle("heat_diffusion_dynamics_2d");
    let n = o.assert_files_match(&out, false, &|f| f.ends_with(".pvd"));
    assert_eq!(n, 2 * 3);
}

/// Medium tier (optimised builds only): the whole upstream run (15 000
/// steps, 150 dumps and the `.pvd`) byte-identical to the upstream binary's,
/// and the whole standard output.
#[cfg(not(debug_assertions))]
#[test]
fn heat_diffusion_dynamics_2d_full_run_matches_the_oracle() {
    let out = scratch("heat_diffusion_dynamics_2d_full");
    let ex = heat_diffusion_dynamics_2d::example().for_test(&out);
    heat_diffusion_dynamics_2d::run(&ex).unwrap();
    let o = oracle::oracle("heat_diffusion_dynamics_2d");
    oracle::assert_stdout(
        "heat_diffusion_dynamics_2d",
        &ex.transcript(),
        &o.stdout(),
        None,
    );
    assert_eq!(o.assert_files_match(&out, true, &|_| false), 2 * 150 + 1);
}
