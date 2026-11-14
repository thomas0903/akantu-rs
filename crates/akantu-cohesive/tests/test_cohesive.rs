// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Port of
// test/test_model/test_solid_mechanics_model/test_cohesive/test_cohesive.cc
// (TestSMMCFixture: ExtrinsicModeI/II, IntrinsicModeI/II).

mod common;

use akantu_core::ElementType;
use common::{Fixture, MATERIAL_0};

#[test]
fn extrinsic_mode_i_triangle_3() {
    let _g = common::lock();
    let mut f = Fixture::create(
        "_cohesive_2d_4_triangle_3.msh",
        2,
        ElementType::Cohesive2d4,
        MATERIAL_0,
        true,
    );
    let strain = f.mode_i_strain();
    let init: Vec<f64> = strain.iter().map(|s| s * (1. - 1e-5)).collect();
    f.set_initial_condition(&init);
    let inc: Vec<f64> = strain.iter().map(|s| s * 2e-2).collect();
    eprintln!("nb_steps {}", f.nb_steps);
    f.steps(&inc, |s, _m, n| {
        if n > 0 {
            eprintln!("step {s}: inserted {n}");
        }
    });
    assert_eq!(f.nb_cohesive(), f.group_size);
    let g_c = common::real_param(&f.model, "insertion", "G_c");
    let edis = f.model.energy("dissipated").unwrap();
    eprintln!(
        "surface {} edis {} expected {}",
        f.surface,
        edis,
        f.surface * g_c
    );
    assert!((f.surface * g_c - edis).abs() < 5e-1);
}
