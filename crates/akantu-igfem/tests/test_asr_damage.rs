// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): extra_packages/igfem/test/test_solid_mechanics_model_igfem/
// test_ASR_damage.cc (registered as `test_ASR_damge`), material_ASR.dat,
// one_inclusion.msh. Built only with the feature `cgal` (GPL-3.0-or-later
// sphere intersector, decision D-042).

//! TEST-204: alkali–silica-reaction damage with an IGFEM gel pocket
//! (IGFEM-007 with XMAT-001). Upstream's program: aggregate
//! `damage_iterative` (`Sc = 26.4e6` Weibull `[6.6e6, 5]`, `prescribed_dam
//! = 0.1`, `max_damage = 0.95`), gel `elastic`, IGFEM law
//! `igfem_saw_tooth_damage` (gel / aggregate); the strength of aggregate
//! element 11 lowered to 1, a gel pocket of radius 0.1 created
//! (`update("gel")`), an eigenstrain `0.05·I` in the gel, free expansion
//! (bottom on rollers in y, left in x); then solve and damage the law with
//! the larger normalised equivalent stress (`updateDamage`) until no element
//! is damaged. Upstream's criterion is that this loop terminates
//! (`EXIT_SUCCESS`; it prints the first stresses of the IGFEM law).
//!
//! The port keeps upstream's loop and adds independent checks: every solve
//! converges; damage stays in `[0, max_damage]` on the damageable IGFEM
//! points and `0` on the gel points of the IGFEM elements; every step that
//! damages raises the damage of the elements it counts; when the loop ends,
//! both maxima are below 1 (the point at a maximum `≥ 1` would have been
//! damaged). Deviation (reported): serial only (UD-609, D-085).

mod common;

use akantu_core::tensor::MatrixX;
use akantu_core::{ElementType, GhostType, Real};
use akantu_geometry_cgal::SkSphere;
use akantu_igfem::material_damage::MaterialIgfemSawToothDamage;
use akantu_model::constitutive_law::LawPhase;
use common::model::*;

const DAT: &str = include_str!("data/upstream/smm_igfem/material_ASR.dat");

/// Upstream `applyBoundaryConditions`: displacement and blocked flags
/// reset, bottom `u_y = 0`, left `u_x = 0` (`eps = 1e-12 · height`).
fn apply_boundary_conditions(model: &mut akantu_igfem::SolidMechanicsModelIgfem) {
    let pos = model.mesh().read().unwrap().nodes().clone();
    let (mut lo, mut hi) = ([Real::INFINITY; 2], [Real::NEG_INFINITY; 2]);
    for x in pos.tuples() {
        for d in 0..2 {
            lo[d] = lo[d].min(x[d]);
            hi[d] = hi[d].max(x[d]);
        }
    }
    let (bottom, top, left) = (lo[1], hi[1], lo[0]);
    let eps = ((top - bottom) * 1e-12).abs();
    let n = pos.len();
    let mut disp = akantu_core::Array::filled(n, 2, 0.0);
    let mut boun = akantu_core::Array::filled(n, 2, false);
    for i in 0..n {
        let x = pos.tuple(i);
        if (x[1] - bottom).abs() < eps {
            boun.tuple_mut(i)[1] = true;
            disp.tuple_mut(i)[1] = 0.0;
        }
        if (x[0] - left).abs() < eps {
            boun.tuple_mut(i)[0] = true;
            disp.tuple_mut(i)[0] = 0.0;
        }
    }
    model
        .smm
        .displacement_mut()
        .unwrap()
        .copy_from(&disp)
        .unwrap();
    model
        .smm
        .blocked_dofs_mut()
        .unwrap()
        .copy_from(&boun)
        .unwrap();
}

fn igfem_law(
    model: &mut akantu_igfem::SolidMechanicsModelIgfem,
) -> &mut MaterialIgfemSawToothDamage {
    model
        .smm
        .material_mut(2)
        .unwrap()
        .as_any_mut()
        .unwrap()
        .downcast_mut::<MaterialIgfemSawToothDamage>()
        .unwrap()
}

#[test]
fn test_asr_damage() {
    let mesh = common::read_msh(2, "smm_igfem/one_inclusion.msh", "mesh");
    let mut model = model_with_extra_materials(mesh, DAT);
    model.register_geometry_object(Vec::new(), "gel").unwrap();
    geometry_selector(&mut model, 2);
    init_static(&mut model);
    // weaken one element to enforce damage there
    set_real_internal(&mut model, 0, "Sc", ElementType::Triangle3, 11, 1.0);
    // the gel pocket
    let r0: Real = 0.1;
    model
        .set_geometry_object(
            vec![SkSphere::try_from_f64([0., 0., 0.], r0 * r0).unwrap()],
            "gel",
        )
        .unwrap();
    let pass = model.update("gel").unwrap();
    eprintln!(
        "interface: {pass:?}, counts {:?}",
        counts(&model.mesh().read().unwrap())
    );
    assert!(pass.new_elements > 0);
    let mut prestrain = MatrixX::zeros(2, 2);
    for i in 0..2 {
        *prestrain.at_mut(i, i) = 0.05;
    }
    model
        .apply_eigen_grad_u(&prestrain, "gel", GhostType::NotGhost)
        .unwrap();
    apply_boundary_conditions(&mut model);
    set_newton(&mut model, 2, 1e-6);
    let max_damage_igfem = igfem_law(&mut model).max_damage;
    assert_eq!(max_damage_igfem, 0.95, "read from the aggregate");
    let mut steps = 0;
    let (mut max_agg, mut max_igfem);
    let mut damaged_total = [0usize; 2];
    loop {
        model.smm.solve_step(None).unwrap();
        max_agg = model
            .smm
            .with_material(0, LawPhase::AfterSolveStep, |law, _| {
                Ok(akantu_extra_materials::as_iterative_damage(law)
                    .unwrap()
                    .norm_max_equivalent_stress())
            })
            .unwrap();
        max_igfem = igfem_law(&mut model).norm_max_equivalent_stress();
        let nb = if max_agg > max_igfem {
            damaged_total[0] += 1;
            model
                .smm
                .with_material(0, LawPhase::AfterSolveStep, |law, ctx| {
                    akantu_extra_materials::as_iterative_damage_mut(law)
                        .unwrap()
                        .update_damage(ctx)
                })
                .unwrap()
        } else {
            let before = real_internal(&model, 2, "damage", ElementType::IgfemTriangle5);
            let nb = igfem_law(&mut model).update_damage().unwrap();
            let after = real_internal(&model, 2, "damage", ElementType::IgfemTriangle5);
            if nb > 0 {
                damaged_total[1] += 1;
                assert!(
                    after.iter().zip(&before).any(|(a, b)| a > b),
                    "a step that damages raises some damage"
                );
            }
            nb
        };
        // damage bounds on the IGFEM law
        for ty in [ElementType::IgfemTriangle4, ElementType::IgfemTriangle5] {
            if model
                .mesh()
                .read()
                .unwrap()
                .nb_element(ty, GhostType::NotGhost)
                == 0
            {
                continue;
            }
            let d = real_internal(&model, 2, "damage", ty);
            let sub = uint_internal(&model, 2, "sub_material", ty);
            for (&d, &s) in d.iter().zip(&sub) {
                if s == 0 {
                    assert_eq!(d, 0.0, "gel points are never damaged");
                } else {
                    assert!((0.0..=max_damage_igfem).contains(&d), "{d}");
                }
            }
        }
        steps += 1;
        eprintln!(
            "step {steps}: max aggregate {max_agg:.6e}, max igfem {max_igfem:.6e}, damaged elements: {nb}"
        );
        if nb == 0 {
            break;
        }
        assert!(steps < 2000, "the damage loop does not terminate");
    }
    eprintln!("damage steps by law (aggregate, igfem): {damaged_total:?}");
    assert!(max_agg < 1.0 && max_igfem < 1.0, "{max_agg} {max_igfem}");
}
