// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Integration test of
// the example crates/akantu/examples/dumpable_interface (port of
// examples/c++/solid_mechanics_model/io/dumper/dumpable_interface.cc and
// locomotive_tools.cc, upstream ctest `dumpable_interface-test`).

#[path = "../examples/dumpable_interface/main.rs"]
#[allow(dead_code)] // the example's `main` is not called by the test
mod dumpable_interface;
mod examples_common;

use examples_common::files_below;
use examples_common::scratch;
use examples_common::train::{STEPS, check_motion, restrict, train, vtu};

fn dumpable(compat: bool) {
    let t = train();
    let out = scratch(&format!("dumpable_interface_{compat}"));
    let mut ex = dumpable_interface::example().for_test(&out);
    ex.steps = Some(STEPS);
    ex.upstream_compat = compat;
    let u_final = dumpable_interface::run(&ex).unwrap();
    let dir = out.join("paraview/dumpable");
    assert_eq!(files_below(&dir).len(), 2 * 2 * STEPS);
    for i in 0..STEPS {
        let train = vtu(&dir, "train", i);
        assert_eq!((train.nb_points, train.nb_cells), (7841, 15244));
        // UD-474: the wheel-filtered colour is not on the train dumper
        assert_eq!(
            train.arrays.keys().collect::<Vec<_>>(),
            [
                "connectivity",
                "displacement",
                "displacement_as_paraview_vector",
                "offsets",
                "positions",
                "types"
            ]
        );
        let u = train.array("displacement");
        assert_eq!(u.nb_component, 2);
        let turned = check_motion(&t, i, u.values.reals(), 2, compat);
        assert_eq!(
            turned > 0,
            !compat,
            "wheel nodes checked against the rotation"
        );
        let padded = train.array("displacement_as_paraview_vector");
        assert_eq!(padded.nb_component, 3);
        check_motion(&t, i, padded.values.reals(), 3, compat);

        let wheels = vtu(&dir, "wheels", i);
        assert_eq!(
            (wheels.nb_points, wheels.nb_cells),
            (t.wheel_nodes.len(), t.wheel_elements.len())
        );
        assert_eq!(
            wheels.array("displacement").values.reals(),
            &restrict(u.values.reals(), 2, &t.wheel_nodes)[..]
        );
        assert_eq!(
            wheels
                .array("displacement_as_paraview_vector")
                .values
                .reals(),
            &restrict(padded.values.reals(), 3, &t.wheel_nodes)[..]
        );
        let wc: Vec<i64> = t.wheel_elements.iter().map(|&e| t.colour[e]).collect();
        assert_eq!(wheels.array("colour").values.ints(), &wc[..]);
        if i + 1 == STEPS {
            assert_eq!(u.values.reals(), &u_final[..]);
        }
    }
}

/// `dumpable_interface`, upstream-compatible motion (UD-473): the
/// registry's default dumper and the group dumper of `wheels`, external
/// fields with and without padding.
#[test]
fn dumpable_interface_upstream_compat() {
    dumpable(true);
}

/// `dumpable_interface` with the corrected rotation (UD-473 default): the
/// wheel nodes follow the closed-form rotation (`check_motion`; fails if
/// the wheels do not turn).
#[test]
fn dumpable_interface_turning_wheels() {
    dumpable(false);
}

/// Oracle (`--upstream-compat`, UD-473/474), 3 steps: every array of the
/// `train` dumps bitwise (upstream's extra wheel-filtered `color` array
/// has one value per wheel element, UD-474); the mesh arrays of the
/// `wheels` dumps bitwise, its fields equal to upstream's unfiltered
/// arrays restricted to the wheel nodes/elements (kept step-1 file); the
/// `.pvtu` files equal up to the `color` entry.
#[test]
fn dumpable_interface_matches_the_oracle() {
    use examples_common::oracle::oracle;
    let t = train();
    let out = scratch("dumpable_interface_oracle");
    let mut ex = dumpable_interface::example().for_test(&out);
    ex.steps = Some(STEPS);
    ex.upstream_compat = true;
    dumpable_interface::run(&ex).unwrap();
    let o = oracle("dumpable_interface");
    let fields = ["displacement", "colour", "displacement_as_paraview_vector"];
    for i in 0..STEPS {
        let train_rel = format!("paraview/dumpable/train-VTU/train_{i:04}.vtu");
        assert_eq!(o.assert_vtu_arrays_match(&out, &train_rel, &|_| false), 6);
        // UD-474: upstream's wheel-filtered `color` (one value per wheel
        // element) holds the wheel colours, in the filter's order
        let color = &o.arrays[&(train_rel.clone(), "color".to_owned())];
        assert_eq!(color.nb_values as usize, t.wheel_elements.len(), "UD-474");
        assert_eq!((color.ty.as_str(), color.nb_component), ("Int32", 1));
        let wheel_colours: Vec<u8> = t
            .wheel_elements
            .iter()
            .flat_map(|&e| i32::try_from(t.colour[e]).unwrap().to_le_bytes())
            .collect();
        assert_eq!(
            akantu_validation::checksum::sha256_bytes(&wheel_colours),
            color.sha256,
            "UD-474: upstream's short color array = the wheel colours"
        );
        let wheels_rel = format!("paraview/dumpable/wheels-VTU/wheels_{i:04}.vtu");
        let n = o.assert_vtu_arrays_match(&out, &wheels_rel, &|name| fields.contains(&name));
        assert_eq!(n, 4);
    }
    let rel = "paraview/dumpable/wheels-VTU/wheels_0001.vtu";
    let mine = examples_common::read_vtu(&out.join(rel));
    let theirs = examples_common::read_vtu(&o.kept(rel));
    for (name, nc) in [("displacement", 2), ("displacement_as_paraview_vector", 3)] {
        let full = theirs.array(name).values.reals();
        assert_eq!(
            full.len(),
            nc * t.x.len() / 2,
            "UD-474: upstream writes every node"
        );
        assert_eq!(
            mine.array(name).values.reals(),
            &restrict(full, nc, &t.wheel_nodes)[..]
        );
    }
    let colour = theirs.array("colour").values.ints();
    let want: Vec<i64> = t.wheel_elements.iter().map(|&e| colour[e]).collect();
    assert_eq!(mine.array("colour").values.ints(), &want[..]);
    for name in ["train", "wheels"] {
        let rel = format!("paraview/dumpable/{name}_0001.pvtu");
        let theirs: String = std::fs::read_to_string(o.kept(&rel))
            .unwrap()
            .lines()
            .filter(|l| !l.contains("Name=\"color\""))
            .map(|l| format!("{l}\n"))
            .collect();
        assert_eq!(
            std::fs::read_to_string(out.join(&rel)).unwrap(),
            theirs,
            "{rel}"
        );
    }
}
