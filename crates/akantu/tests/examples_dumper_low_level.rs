// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Integration test of
// the example crates/akantu/examples/dumper_low_level (port of
// examples/c++/solid_mechanics_model/io/dumper/dumper_low_level.cc and
// locomotive_tools.cc, upstream ctest `dumper_low_level-test`).

#[path = "../examples/dumper_low_level/main.rs"]
#[allow(dead_code)] // the example's `main` is not called by the test
mod dumper_low_level;
mod examples_common;

use examples_common::files_below;
use examples_common::scratch;
use examples_common::train::{STEPS, check_motion, restrict, train, vtu};

fn low_level(compat: bool) {
    let t = train();
    let out = scratch(&format!("dumper_low_level_{compat}"));
    let mut ex = dumper_low_level::example().for_test(&out);
    ex.steps = Some(STEPS);
    ex.upstream_compat = compat;
    let u_final = dumper_low_level::run(&ex).unwrap();
    let dir = out.join("paraview/dumper");
    // a field-less dump, then one dump per step, for each dumper
    assert_eq!(files_below(&dir).len(), 2 * 2 * (STEPS + 1));

    let first = vtu(&dir, "train", 0);
    assert_eq!((first.nb_points, first.nb_cells), (7841, 15244));
    assert_eq!(
        first.arrays.keys().collect::<Vec<_>>(),
        ["connectivity", "offsets", "positions", "types"]
    );
    let wheels0 = vtu(&dir, "wheels", 0);
    assert_eq!(
        (wheels0.nb_points, wheels0.nb_cells),
        (t.wheel_nodes.len(), t.wheel_elements.len())
    );

    for i in 0..STEPS {
        let train = vtu(&dir, "train", i + 1);
        let u = train.array("displacement");
        assert_eq!(u.nb_component, 3);
        let turned = check_motion(&t, i, u.values.reals(), 3, compat);
        assert_eq!(
            turned > 0,
            !compat,
            "wheel nodes checked against the rotation"
        );
        assert_eq!(train.array("colour").values.ints(), &t.colour[..]);
        let wheels = vtu(&dir, "wheels", i + 1);
        assert_eq!(
            wheels.array("displacement").values.reals(),
            &restrict(u.values.reals(), 3, &t.wheel_nodes)[..]
        );
        let wc: Vec<i64> = t.wheel_elements.iter().map(|&e| t.colour[e]).collect();
        assert_eq!(wheels.array("colour").values.ints(), &wc[..]);
        if i + 1 == STEPS {
            assert_eq!(u.values.reals(), &u_final[..]);
        }
    }
}

/// `dumper_low_level`, upstream-compatible (UD-473: the wheels do not
/// turn): structure of both dumpers, colours from the physical names,
/// translation of every node, filtered fields equal to the full ones on
/// the wheel nodes and elements.
#[test]
fn dumper_low_level_upstream_compat() {
    low_level(true);
}

/// `dumper_low_level` with the corrected rotation (UD-473 default): every
/// wheel node but the two centres follows the closed-form rotation about
/// its translated centre (`check_motion`; fails if the wheels do not turn).
#[test]
fn dumper_low_level_turning_wheels() {
    low_level(false);
}

/// Oracle (`--upstream-compat`, UD-473), 10 steps: every file written
/// byte-identical to the upstream binary's.
#[test]
fn dumper_low_level_matches_the_oracle() {
    let out = scratch("dumper_low_level_oracle");
    let mut ex = dumper_low_level::example().for_test(&out);
    ex.steps = Some(10);
    ex.upstream_compat = true;
    dumper_low_level::run(&ex).unwrap();
    let o = examples_common::oracle::oracle("dumper_low_level");
    assert_eq!(o.assert_files_match(&out, false, &|_| false), 2 * 2 * 11);
}

/// Medium tier: the whole run (100 steps, 202 dumps) byte-identical.
#[cfg(not(debug_assertions))]
#[test]
fn dumper_low_level_full_run_matches_the_oracle() {
    let out = scratch("dumper_low_level_full_oracle");
    let mut ex = dumper_low_level::example().for_test(&out);
    ex.upstream_compat = true;
    dumper_low_level::run(&ex).unwrap();
    let o = examples_common::oracle::oracle("dumper_low_level");
    assert_eq!(o.assert_files_match(&out, true, &|_| false), 2 * 2 * 101);
}
