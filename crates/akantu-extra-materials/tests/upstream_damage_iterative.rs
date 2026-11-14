// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): extra_packages/extra-materials/test/
// test_material_damage/{test_material_damage_iterative.cc,
// test_material_iterative_stiffness_reduction.cc, material.dat,
// material_stiffness_reduction.dat, plate.msh, two_elements.msh,
// test_material_iterative_stiffness_reduction.verified}.

//! Ports of the upstream `damage_iterative` tests (TEST-184, TEST-185 serial
//! part, TEST-188). The package never compiled at the reference commit:
//! these are the upstream assertions, with their tolerances,
//! checked against the port (D-007).

mod common;

use akantu_core::enums::SpatialDirection;
use akantu_core::math::are_float_equal;
use akantu_core::{ElementType, GhostType, Real};
use akantu_extra_materials::{MaterialDamageIterative, as_iterative_damage};
use akantu_model::boundary_condition::FixedValue;
use common::{data, format_g, init_static, model_with, read_mesh, with_iterative, with_law};

const T3: ElementType = ElementType::Triangle3;
const NG: GhostType = GhostType::NotGhost;

fn bcs(model: &mut akantu_solid::SolidMechanicsModel) {
    for (value, axis, group) in [
        (0., SpatialDirection::X, "left"),
        (0., SpatialDirection::Y, "bottom"),
        (2., SpatialDirection::Y, "top"),
    ] {
        model
            .apply_dirichlet_bc(&FixedValue { value, axis }, group)
            .unwrap();
    }
}

/// Upstream `test_material_damage_iterative` (serial; the `_parallel`
/// variant runs the same source on 2 ranks with the counts multiplied by
/// the number of ranks).
#[test]
fn test_material_damage_iterative() {
    // Math::setTolerance(1e-13)
    let tolerance = 1e-13;
    let dat = std::fs::read_to_string(data("test_material_damage/material.dat")).unwrap();
    let mesh = read_mesh(&data("test_material_damage/plate.msh"), 2);
    let mut model = model_with(mesh, &dat);
    init_static(&mut model);
    bcs(&mut model);

    model.solve_step(None).unwrap();

    // the normalised equivalent stress of every element is 0.1
    let nb_elements = model.physics.base.mesh.read().unwrap().nb_element(T3, NG);
    {
        let law = model.material(0).unwrap();
        let eq = law
            .as_any()
            .and_then(|a| a.downcast_ref::<MaterialDamageIterative<2>>())
            .unwrap()
            .equivalent_stress
            .get(T3, NG)
            .unwrap()
            .clone();
        assert_eq!(eq.len(), nb_elements);
        for (e, &v) in eq.as_slice().iter().enumerate() {
            assert!(
                are_float_equal(v, 0.1, tolerance),
                "Error in the equivalent normalized stress: element {e}: {v}"
            );
        }
    }

    // no damage below a normalised stress of 1
    let max_eq_stress = as_iterative_damage(model.material(0).unwrap())
        .unwrap()
        .norm_max_equivalent_stress();
    let mut nb_damaged = 0;
    if max_eq_stress > 1. {
        nb_damaged = with_iterative(&mut model, 0, |m, ctx| m.update_damage(ctx));
    }
    assert_eq!(
        nb_damaged, 0,
        "Damage occured even though the normalized stress is below 1"
    );

    // weaken the material locally to cause damage
    with_law(&mut model, 0, |law, _| {
        let m = law
            .as_any_mut()
            .and_then(|a| a.downcast_mut::<MaterialDamageIterative<2>>())
            .unwrap();
        let mut sc = m.sc.get_mut(T3, NG)?;
        sc.as_mut_slice()[1] = 0.9;
        sc.as_mut_slice()[5] = 0.898;
        Ok(())
    });

    model.solve_step(None).unwrap();
    let max_eq_stress = as_iterative_damage(model.material(0).unwrap())
        .unwrap()
        .norm_max_equivalent_stress();
    let mut nb_damaged = 0;
    if max_eq_stress > 1. {
        nb_damaged = with_iterative(&mut model, 0, |m, ctx| m.update_damage(ctx));
    }
    let psize = 1;
    let nb_damaged_elements_per_proc = 2;
    assert_eq!(
        nb_damaged,
        psize * nb_damaged_elements_per_proc,
        "Error in number of damaged elements"
    );

    // damage occurred in the right elements
    let damage = model
        .material(0)
        .unwrap()
        .as_any()
        .and_then(|a| a.downcast_ref::<MaterialDamageIterative<2>>())
        .unwrap()
        .damage
        .damage
        .get(T3, NG)
        .unwrap()
        .clone();
    let mut dam_diff = 0.;
    for (e, &d) in damage.as_slice().iter().enumerate() {
        if e == 1 || e == 5 {
            dam_diff += (0.1 - d).abs();
        } else {
            dam_diff += d;
        }
    }
    assert!(dam_diff <= 1e-13, "Error in damage pattern: {dam_diff}");

    // solve to compute the stresses correctly for dumping
    model.solve_step(None).unwrap();
}

/// Upstream `test_material_iterative_stiffness_reduction`: the printed
/// history must equal the committed `.verified` output (44 lines, printed
/// with `std::setprecision(12)`).
#[test]
fn test_material_iterative_stiffness_reduction() {
    let dat = std::fs::read_to_string(data(
        "test_material_damage/material_stiffness_reduction.dat",
    ))
    .unwrap();
    let verified = std::fs::read_to_string(data(
        "test_material_damage/test_material_iterative_stiffness_reduction.verified",
    ))
    .unwrap();
    let mesh = read_mesh(&data("test_material_damage/two_elements.msh"), 2);
    let mut model = model_with(mesh, &dat);
    init_static(&mut model);
    bcs(&mut model);

    let e_modulus = match akantu_parser::ParameterRegistry::get_param(
        // `material.get("E")`
        model.material(0).unwrap(),
        "E",
    )
    .unwrap()
    {
        akantu_parser::ParamValue::Real(v) => v,
        other => panic!("E: {other:?}"),
    };
    let nb_elements = model.physics.base.mesh.read().unwrap().nb_element(T3, NG);
    let mut out = String::new();
    loop {
        model.solve_step(None).unwrap();
        let nb_damaged = with_iterative(&mut model, 0, |m, ctx| m.update_damage(ctx));
        let law = model
            .material(0)
            .unwrap()
            .as_any()
            .and_then(|a| {
                a.downcast_ref::<akantu_extra_materials::MaterialIterativeStiffnessReduction<2>>()
            })
            .unwrap();
        let damage = law.parent.damage.damage.get(T3, NG).unwrap();
        let sc = law.parent.sc.get(T3, NG).unwrap();
        for _ in 0..nb_elements {
            let modulus: Real = (1. - damage.as_slice()[0]) * e_modulus;
            out.push_str(&format!("the new modulus is {}\n", format_g(modulus, 12)));
            out.push_str(&format!(
                "the new strength is {}\n",
                format_g(sc.as_slice()[0], 12)
            ));
        }
        if nb_damaged == 0 {
            break;
        }
    }
    assert_eq!(out, verified);
}

#[test]
fn format_g_matches_printf() {
    assert_eq!(format_g(30000000000., 12), "30000000000");
    assert_eq!(format_g(2117237.376666, 12), "2117237.37667");
    assert_eq!(format_g(6000000., 12), "6000000");
    assert_eq!(format_g(1e-5, 12), "1e-05");
    assert_eq!(format_g(1.5e15, 12), "1.5e+15");
    assert_eq!(format_g(0.1, 12), "0.1");
}
