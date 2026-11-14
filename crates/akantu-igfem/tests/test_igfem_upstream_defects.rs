// SPDX-License-Identifier: LGPL-3.0-or-later
// Tests of the corrections of upstream defects UD-1200, UD-1201 and UD-1203,
// written for akantu-rs. Derived from Akantu (EPFL/LSMS)
// extra_packages/igfem: src/mesh_igfem_spherical_growing_gel{.hh,_tmpl.hh},
// src/material_igfem_saw_tooth_damage{.cc,_inline_impl.hh}; inputs of
// test/test_solid_mechanics_model_igfem/test_transfer_internals.cc
// (material_damage.dat, fine_mesh.msh) and
// test/patch_tests/test_interface_position.cc. Built only with the feature
// `cgal` (GPL-3.0-or-later sphere intersector, decision D-042).
#![cfg(feature = "cgal")]

//! The three D-003 corrections that had no test.
//!
//! * UD-1200 (B, no compat): a pass that creates no node still removes the
//!   previous enriched nodes (upstream returns early and keeps them as
//!   orphans with DOFs and no stiffness).
//! * UD-1201 (B, switch): the maximum normalised equivalent stress of an
//!   IGFEM damage law holding `_igfem_triangle_4` **and**
//!   `_igfem_triangle_5` elements runs over both types by default;
//!   `equivalent_stress_max_over_types = false` reproduces upstream's reset
//!   before every type (the last type's maximum).
//! * UD-1203 (B, no compat): `update_damage_on_quad` takes the element of a
//!   point as `quad_index / nb_quads`, not upstream's `quad_index /
//!   nb_elements`.

mod common;

use akantu_core::{ElementType, GhostType, Real};
use akantu_geometry_cgal::SkSphere;
use akantu_igfem::SolidMechanicsModelIgfem;
use akantu_igfem::material_damage::MaterialIgfemSawToothDamage;
use common::model::*;

const DAT_POS: &str = include_str!("data/upstream/material_test_interface_position.dat");
const DAT_DAMAGE: &str = include_str!("data/upstream/smm_igfem/material_damage.dat");

fn sphere(c: [Real; 2], r: Real) -> SkSphere {
    SkSphere::try_from_f64([c[0], c[1], 0.0], r * r).unwrap()
}

fn saw_tooth(model: &mut SolidMechanicsModelIgfem) -> &mut MaterialIgfemSawToothDamage {
    model
        .smm
        .material_mut(2)
        .unwrap()
        .as_any_mut()
        .unwrap()
        .downcast_mut::<MaterialIgfemSawToothDamage>()
        .unwrap()
}

/// TEST-201's mesh and input with one pocket: the law `test_mat_igfem`
/// holds `_igfem_triangle_4` and `_igfem_triangle_5` elements.
fn mixed_type_model() -> SolidMechanicsModelIgfem {
    let mesh = common::read_msh(2, "smm_igfem/fine_mesh.msh", "mesh");
    let mut model = model_with_extra_materials(mesh, DAT_DAMAGE);
    model.register_geometry_object(Vec::new(), "mat_1").unwrap();
    geometry_selector(&mut model, 2);
    init_static(&mut model);
    // a circle through four mesh nodes: 6 `_igfem_triangle_4` (one new
    // node) and 3 `_igfem_triangle_5` (two new nodes)
    model
        .set_geometry_object(vec![sphere([0., 0.], 0.25)], "mat_1")
        .unwrap();
    model.update("mat_1").unwrap();
    model
}

/// Uniaxial tension (bottom `u_y = 0`, top `u_y = 1e-3`, left `u_x = 0`).
fn tension_bcs(model: &mut SolidMechanicsModelIgfem) {
    let pos = model.mesh().read().unwrap().nodes().clone();
    let (mut lo, mut hi) = ([Real::INFINITY; 2], [Real::NEG_INFINITY; 2]);
    for x in pos.tuples() {
        for d in 0..2 {
            lo[d] = lo[d].min(x[d]);
            hi[d] = hi[d].max(x[d]);
        }
    }
    let eps = ((hi[1] - lo[1]) * 1e-6).abs();
    let mut boun = model.smm.blocked_dofs().unwrap().clone();
    let mut disp = model.smm.displacement().unwrap().clone();
    for n in 0..pos.len() {
        let x = pos.tuple(n);
        if (x[1] - lo[1]).abs() < eps {
            boun.tuple_mut(n)[1] = true;
            disp.tuple_mut(n)[1] = 0.;
        }
        if (x[1] - hi[1]).abs() < eps {
            boun.tuple_mut(n)[1] = true;
            disp.tuple_mut(n)[1] = 1e-3;
        }
        if (x[0] - lo[0]).abs() < eps {
            boun.tuple_mut(n)[0] = true;
            disp.tuple_mut(n)[0] = 0.;
        }
    }
    model
        .smm
        .blocked_dofs_mut()
        .unwrap()
        .copy_from(&boun)
        .unwrap();
    model
        .smm
        .displacement_mut()
        .unwrap()
        .copy_from(&disp)
        .unwrap();
}

/// Largest equivalent stress over the damageable points (`sub_material ≠ 0`,
/// damage below `max_damage`) of the block `ty`: what upstream's
/// `findMaxNormalizedEquivalentStress` computes for one type.
fn block_max(model: &mut SolidMechanicsModelIgfem, ty: ElementType) -> Real {
    let max_damage = saw_tooth(model).max_damage;
    let eq = real_internal(model, 2, "equivalent_stress", ty);
    let dam = real_internal(model, 2, "damage", ty);
    let sub = uint_internal(model, 2, "sub_material", ty);
    (0..eq.len())
        .filter(|&i| sub[i] != 0 && dam[i] < max_damage)
        .map(|i| eq[i])
        .fold(0.0, Real::max)
}

/// The mixed-type model, solved in tension, with the UD-1201 switch set
/// (`None`: the law's default).
fn solved_mixed_model(max_over_types: Option<bool>) -> SolidMechanicsModelIgfem {
    let mut model = mixed_type_model();
    if let Some(flag) = max_over_types {
        saw_tooth(&mut model).equivalent_stress_max_over_types = flag;
    }
    tension_bcs(&mut model);
    set_newton(&mut model, 2, 1e-12);
    model.smm.solve_step(None).unwrap();
    model
}

/// UD-1201 (B, switch): one law, two IGFEM types whose maxima differ. By
/// default the law's maximum is over both types; with the switch off it is
/// the last type's (`_igfem_triangle_5`), upstream's reset before every
/// type.
#[test]
fn ud_1201_maximum_over_both_igfem_types_by_default() {
    let ty4 = ElementType::IgfemTriangle4;
    let ty5 = ElementType::IgfemTriangle5;
    // the circle through mesh nodes gives both types in one law
    let c = counts(&mixed_type_model().mesh().read().unwrap());
    assert!(c[1] > 0 && c[2] > 0, "both IGFEM types: {c:?}");
    // default: the maximum over both types
    let mut model = solved_mixed_model(None);
    assert!(
        saw_tooth(&mut model).equivalent_stress_max_over_types,
        "UD-1201's corrected behaviour is the default"
    );
    let (m4, m5) = (block_max(&mut model, ty4), block_max(&mut model, ty5));
    assert!(
        m4 > m5 && m5 > 0.0,
        "the types' maxima must differ: {m4:e} vs {m5:e}"
    );
    let law = saw_tooth(&mut model).norm_max_equivalent_stress();
    assert_eq!(law, m4.max(m5), "default: maximum over both types");
    assert_eq!(law, m4, "the larger maximum is the first type's");
    // compat switch: upstream's reset before every type, the last type's
    // maximum
    let mut model = solved_mixed_model(Some(false));
    let (m4, m5) = (block_max(&mut model, ty4), block_max(&mut model, ty5));
    let law = saw_tooth(&mut model).norm_max_equivalent_stress();
    assert_eq!(
        law, m5,
        "switch off: the last type's maximum ({m4:e}, {m5:e})"
    );
    assert!(law < m4, "and it is below the other type's maximum");
}

/// UD-1203 (B, no compat): the element of a point is `quad_index /
/// nb_quads`. In the `_igfem_triangle_4` block (more elements than points
/// per element) the point of element 3 damages only the damageable points
/// of element 3: `+= prescribed_dam` below `dam_threshold`, `+= max_damage`
/// from it on (upstream's `+=`, kept).
#[test]
fn ud_1203_update_damage_on_quad_damages_the_points_element() {
    let ty = ElementType::IgfemTriangle4;
    let ng = GhostType::NotGhost;
    let nq = 2; // two sub-elements, one point each
    let mut model = solved_mixed_model(None);
    let n_el = model.mesh().read().unwrap().nb_element(ty, ng);
    assert!(n_el > nq, "more elements than points per element");
    let sub = uint_internal(&model, 2, "sub_material", ty);
    assert_eq!(sub.len(), n_el * nq);
    // every point passes the test `eq ≥ (1 − dam_tolerance)·max`
    saw_tooth(&mut model).dam_tolerance = 1.0;
    let (prescribed, threshold, max_damage) = {
        let l = saw_tooth(&mut model);
        (l.prescribed_dam, l.dam_threshold, l.max_damage)
    };
    assert!(prescribed > 0.0 && threshold > 0.0);
    for (element, own_high) in [(3_usize, false), (4, true)] {
        let q = (0..nq)
            .find(|&q| sub[element * nq + q] != 0)
            .expect("a damageable point");
        let point = element * nq + q;
        let before = real_internal(&model, 2, "damage", ty);
        if own_high {
            // at the threshold the point jumps by `max_damage`
            set_real_internal(&mut model, 2, "damage", ty, point, threshold);
        }
        let before_set = real_internal(&model, 2, "damage", ty);
        let damaged = saw_tooth(&mut model)
            .update_damage_on_quad(point, ty, ng)
            .unwrap();
        let after = real_internal(&model, 2, "damage", ty);
        let damageable: usize = (0..nq).filter(|&q| sub[element * nq + q] != 0).count();
        assert_eq!(damaged as usize, damageable, "element {element}");
        let step = if own_high { max_damage } else { prescribed };
        for i in 0..after.len() {
            if i / nq == element && sub[i] != 0 {
                assert_eq!(
                    after[i],
                    before_set[i] + step,
                    "element {element} point {i}: damageable point of the point's element"
                );
            } else {
                assert_eq!(
                    after[i],
                    before_set[i],
                    "point {i} (element {}) belongs to another element or is elastic \
                     (before: {before:?})",
                    i / nq
                );
            }
        }
    }
}

/// UD-1200 (B, no compat): the pass after the interface left the mesh (the
/// domain's spheres emptied: no new node) still removes the previous
/// enriched nodes; upstream's early return would keep them as orphans with
/// DOFs and no stiffness. After the pass: no IGFEM element, only the
/// standard nodes, and TEST-192's patch test solved on the regular mesh.
#[test]
fn ud_1200_interface_leaves_the_mesh() {
    let mesh = common::read_msh(2, "test_interface_position.msh", "mesh");
    let nb_standard = mesh.nb_nodes();
    let mut model = model(mesh, DAT_POS);
    init_static(&mut model);
    model
        .register_geometry_object(vec![sphere([0., 0.], 0.25)], "inside")
        .unwrap();
    model.update("").unwrap();
    let enriched = model.mesh().read().unwrap().nb_nodes() - nb_standard;
    assert!(enriched > 0);
    model.set_geometry_object(Vec::new(), "inside").unwrap();
    let pass = model.update("").unwrap();
    let m = model.mesh().read().unwrap();
    assert_eq!(pass.new_nodes, 0);
    assert_eq!(pass.removed_nodes, enriched);
    assert_eq!(m.nb_nodes(), nb_standard, "no orphaned enriched node");
    assert_eq!(counts(&m)[1..], [0, 0]);
    drop(m);
    patch_bcs(&mut model);
    set_newton(&mut model, 2, 1e-12);
    model.smm.solve_step(None).unwrap();
    let err = l2_error(&model, &|x| 0.5 * x + 0.5);
    assert!(err <= 1e-13, "patch test after the interface left: {err:e}");
}

/// Input of TEST-192 with the law of the sub-material `inside` replaced by
/// `law` (its parameters `params`).
fn dat_with_inside(law: &str, params: &str) -> String {
    format!(
        "seed = 1\n\
         material elastic [\n name = outside\n rho = 2.7e-9\n E = 10.\n nu = 0.\n]\n\
         material {law} [\n name = inside\n rho = 2.7e-9\n {params}\n]\n\
         material igfem_elastic [\n name = igfem_elastic\n name_sub_mat_1 = inside\n \
         name_sub_mat_2 = outside\n]\n"
    )
}

/// UD-1210: an IGFEM sub-material must be an elastic law or
/// one built on it (upstream `dynamic_cast<MaterialElastic &>`). A
/// `neohookean` law (it exposes `lambda`, `mu` and `kapa` too, but is not a
/// `MaterialElastic` upstream) and a law without `kapa` (`elastic_orthotropic`) are
/// refused at `init_full`; the elastic one is accepted.
#[test]
fn igfem_sub_materials_must_be_elastic_based() {
    let run = |dat: &str| {
        let mesh = common::read_msh(2, "test_interface_position.msh", "mesh");
        let mut model = model(mesh, dat);
        model.init_full(&akantu_model::ModelOptions::new(
            akantu_core::enums::AnalysisMethod::Static,
        ))
    };
    run(&dat_with_inside("elastic", "E = 10.\n nu = 0.")).unwrap();
    for (law, params) in [
        ("neohookean", "E = 10.\n nu = 0."),
        (
            "elastic_orthotropic",
            "E1 = 10.\n E2 = 10.\n nu12 = 0.\n G12 = 5.\n n1 = [1., 0.]\n n2 = [0., 1.]",
        ),
    ] {
        let err = run(&dat_with_inside(law, params)).expect_err(law);
        let text = err.to_string();
        assert!(
            text.contains("IGFEM sub-material inside") && text.contains(law),
            "{law}: {text}"
        );
    }
}
