// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): extra_packages/igfem/test/test_solid_mechanics_model_igfem/
// test_material_igfem_iterative_stiffness_reduction.cc (+ .sh: the `standard_fem`
// and `igfem` runs), test_material_igfem_iterative_stiffness_reduction_damage_step_transfer.cc,
// test_material_igfem_iterative_stiffness_reduction_tangent_transfer.cc,
// test_material_igfem_iterative_stiffness_reduction_parallel.cc (serial half),
// test_volume_computation.cc; material_stiffness_reduction{,_2,_weibull}.dat,
// regular_mesh.msh, igfem_mesh.msh, test_damage_transfer.msh. Built only with
// the feature `cgal` (GPL-3.0-or-later sphere intersector, decision D-042).

//! TEST-205: `igfem_iterative_stiffness_reduction` (IGFEM-008, with
//! XMAT-003 as its damageable sub-material) — the five upstream programs of
//! the row, each a test here with upstream's criteria:
//!
//! * `test_material_igfem_iterative_stiffness_reduction`: a square in
//!   uniaxial tension (`u_y = 1e-3` on top), a straight interface `y = 0`
//!   (a sphere of radius `1e9` centred at `(0, 1e9)`); the `standard_fem`
//!   run (`regular_mesh.msh`, the interface along mesh edges) records, per
//!   damage step, the barycentre, damage and strength of every damageable
//!   element; the `igfem` run (`igfem_mesh.msh`, two `_igfem_triangle_4`
//!   whose sub-elements are the regular run's triangles) must match them
//!   step by step: every damageable regular element and IGFEM sub-element
//!   found by barycentre (`Math::tolerance = 1e-10`), `|Δd| ≤ 1e-12`,
//!   `|ΔSc| ≤ 1e-4`;
//! * `…_damage_step_transfer`: 15 regular damage steps, then the interface
//!   created (`moveInterface`): the damage step of IGFEM elements 7 and 5
//!   equals the step of regular elements 27 and 19 (the replaced ones), 0
//!   elsewhere and on the elastic sub-elements; after one more step,
//!   element 7 has one step more (and `d = 1 − a^{−(k+1)}`), element 5 is
//!   unchanged;
//! * `…_tangent_transfer` (Weibull strengths): the regular law's tangent is
//!   `D = Sc/(ε_u − Sc/E)` with `ε_u = 2 Gf/(Sc h)` (`Math::tolerance =
//!   1e-13`); after the interface is created, the IGFEM `ultimate_strain`
//!   and `tangent` of elements 7 and 5 equal those of regular elements 27
//!   and 19, and are 0 on the elastic sub-elements;
//! * `test_volume_computation`: `moveInterface` then a second `update` (a
//!   live move): the regular damage elements cover 0.5, the elastic ones
//!   0.25, the IGFEM elements 0.1 outside and 0.15 inside (`1e-8`);
//! * `…_parallel`, serial half: upstream writes the per-step state for a
//!   parallel run to compare (D-085: the parallel half is an approved
//!   exception); the port runs the same serial damage loop (interface of
//!   radius 0.125 at the origin) with invariant checks.
//!
//! Deviations (reported): upstream solves with
//! `_scm_newton_raphson_tangent_modified` (stiffness assembled once per
//! solve step); the port uses the tangent Newton–Raphson with the same
//! criterion (`_increment`, `1e-12`, 2 iterations): each solve is linear
//! (the damage is constant within a step), so both converge to the same
//! solution. `Math::setTolerance(1e-14)` is the IGFEM law's `tolerance`.

mod common;

use akantu_core::{Element, ElementType, GhostType, Real, UInt};
use akantu_geometry_cgal::SkSphere;
use akantu_igfem::SolidMechanicsModelIgfem;
use akantu_igfem::material_damage::MaterialIgfemIterativeStiffnessReduction;
use akantu_model::constitutive_law::LawPhase;
use common::model::*;

const DAT: &str = include_str!("data/upstream/smm_igfem/material_stiffness_reduction.dat");
const DAT_2: &str = include_str!("data/upstream/smm_igfem/material_stiffness_reduction_2.dat");
const DAT_WEIBULL: &str =
    include_str!("data/upstream/smm_igfem/material_stiffness_reduction_weibull.dat");

const VAL: Real = 1_000_000_000.0;

fn line_sphere(radius: Real) -> SkSphere {
    SkSphere::try_from_f64([0.0, VAL, 0.0], radius * radius).unwrap()
}

/// Model on `msh` (`smm_igfem/…`) with input `dat`, the geometry
/// `inclusion` registered with `spheres`, the test selector (IGFEM → 2,
/// inside → 1, else 0), `initFull` (static), IGFEM law tolerance `1e-14`.
fn setup(msh: &str, dat: &str, spheres: Vec<SkSphere>) -> SolidMechanicsModelIgfem {
    let mesh = common::read_msh(2, &format!("smm_igfem/{msh}"), "mesh");
    let mut model = model_with_extra_materials(mesh, dat);
    model
        .register_geometry_object(spheres, "inclusion")
        .unwrap();
    geometry_selector(&mut model, 2);
    init_static(&mut model);
    igfem_law(&mut model).saw.elastic.tolerance = 1e-14;
    set_newton(&mut model, 2, 1e-12);
    model
}

fn igfem_law(
    model: &mut SolidMechanicsModelIgfem,
) -> &mut MaterialIgfemIterativeStiffnessReduction {
    model
        .smm
        .material_mut(2)
        .unwrap()
        .as_any_mut()
        .unwrap()
        .downcast_mut::<MaterialIgfemIterativeStiffnessReduction>()
        .unwrap()
}

/// Upstream's boundary conditions: bottom `u_y = 0`, top `u_y = 1e-3`, left
/// `u_x = 0` (`eps = 1e-6 · height`).
fn tension_bcs(model: &mut SolidMechanicsModelIgfem) {
    let pos = model.mesh().read().unwrap().nodes().clone();
    let (mut lo, mut hi) = ([Real::INFINITY; 2], [Real::NEG_INFINITY; 2]);
    for x in pos.tuples() {
        for d in 0..2 {
            lo[d] = lo[d].min(x[d]);
            hi[d] = hi[d].max(x[d]);
        }
    }
    let (bottom, top, left) = (lo[1], hi[1], lo[0]);
    let eps = ((top - bottom) * 1e-6).abs();
    let mut boun = model.smm.blocked_dofs().unwrap().clone();
    let mut disp = model.smm.displacement().unwrap().clone();
    for n in 0..pos.len() {
        let x = pos.tuple(n);
        if (x[1] - bottom).abs() < eps {
            boun.tuple_mut(n)[1] = true;
            disp.tuple_mut(n)[1] = 0.;
        }
        if (x[1] - top).abs() < eps {
            boun.tuple_mut(n)[1] = true;
            disp.tuple_mut(n)[1] = 1e-3;
        }
        if (x[0] - left).abs() < eps {
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

/// One step of upstream's loop: solve (must converge), then damage the law
/// with the larger maximum (both on a tie); the number of damaged elements.
fn damage_step(model: &mut SolidMechanicsModelIgfem) -> UInt {
    model.smm.solve_step(None).unwrap();
    let max_reg = model
        .smm
        .with_material(0, LawPhase::AfterSolveStep, |law, _| {
            Ok(akantu_extra_materials::as_iterative_damage(law)
                .unwrap()
                .norm_max_equivalent_stress())
        })
        .unwrap();
    let max_igfem = igfem_law(model).norm_max_equivalent_stress();
    let update_reg = |model: &mut SolidMechanicsModelIgfem| {
        model
            .smm
            .with_material(0, LawPhase::AfterSolveStep, |law, ctx| {
                akantu_extra_materials::as_iterative_damage_mut(law)
                    .unwrap()
                    .update_damage(ctx)
            })
            .unwrap()
    };
    if max_reg > max_igfem {
        update_reg(model)
    } else if max_reg == max_igfem {
        update_reg(model) + igfem_law(model).update_damage().unwrap()
    } else {
        igfem_law(model).update_damage().unwrap()
    }
}

/// The per-step state of upstream's `standard_fem` run: barycentre,
/// damage, strength of every element of law 0.
fn regular_state(model: &SolidMechanicsModelIgfem) -> Vec<([Real; 2], Real, Real)> {
    let filter: Vec<i32> = model
        .smm
        .material(0)
        .unwrap()
        .element_filter()
        .get(ElementType::Triangle3, GhostType::NotGhost)
        .map(|a| a.as_slice().to_vec())
        .unwrap_or_default();
    let damage = real_internal(model, 0, "damage", ElementType::Triangle3);
    let sc = real_internal(model, 0, "Sc", ElementType::Triangle3);
    let mesh = model.mesh().read().unwrap();
    filter
        .iter()
        .enumerate()
        .map(|(e, &g)| {
            let el = Element::new(ElementType::Triangle3, g, GhostType::NotGhost);
            let b = mesh.barycenter(el).unwrap();
            ([b[0], b[1]], damage[e], sc[e])
        })
        .collect()
}

/// Upstream `checkDamageState(step, model, igfem_analysis = true)`.
fn check_igfem_state(
    model: &SolidMechanicsModelIgfem,
    reference: &[([Real; 2], Real, Real)],
) -> Result<usize, String> {
    let eq = |a: Real, b: Real| akantu_core::math::are_float_equal(a, b, 1e-10);
    let find = |b: &[Real]| {
        reference
            .iter()
            .position(|(r, _, _)| eq(b[0], r[0]) && eq(b[1], r[1]))
            .ok_or_else(|| format!("Element barycenter not found: {b:?}"))
    };
    let mut compared = 0;
    for (b, d, s) in regular_state(model) {
        let m = find(&b)?;
        if (reference[m].1 - d).abs() > 1e-12 || (reference[m].2 - s).abs() > 1e-4 {
            return Err(format!(
                "regular element at {b:?}: (d, Sc) = ({d}, {s}), standard run ({}, {})",
                reference[m].1, reference[m].2
            ));
        }
        compared += 1;
    }
    let ty = ElementType::IgfemTriangle4;
    let filter: Vec<i32> = model
        .smm
        .material(2)
        .unwrap()
        .element_filter()
        .get(ty, GhostType::NotGhost)
        .map(|a| a.as_slice().to_vec())
        .unwrap_or_default();
    let damage = real_internal(model, 2, "damage", ty);
    let sc = real_internal(model, 2, "Sc", ty);
    let sub = uint_internal(model, 2, "sub_material", ty);
    for (e, &g) in filter.iter().enumerate() {
        for s in 0..2 {
            if sub[e * 2 + s] == 0 {
                continue;
            }
            let el = Element::new(ty, g, GhostType::NotGhost);
            let b = model.sub_element_barycenter(el, s).unwrap();
            let m = find(&b)?;
            let (d, c) = (damage[e * 2 + s], sc[e * 2 + s]);
            if (reference[m].1 - d).abs() > 1e-12 || (reference[m].2 - c).abs() > 1e-4 {
                return Err(format!(
                    "IGFEM sub-element at {b:?}: (d, Sc) = ({d}, {c}), standard run ({}, {})",
                    reference[m].1, reference[m].2
                ));
            }
            compared += 1;
        }
    }
    Ok(compared)
}

#[test]
fn test_material_igfem_iterative_stiffness_reduction() {
    // standard_fem: record every step
    let mut reg = setup("regular_mesh.msh", DAT, vec![line_sphere(VAL)]);
    tension_bcs(&mut reg);
    let mut reference = Vec::new();
    loop {
        let nb = damage_step(&mut reg);
        reference.push(regular_state(&reg));
        if nb == 0 {
            break;
        }
        assert!(reference.len() < 100);
    }
    eprintln!("standard_fem: {} steps", reference.len());
    assert!(reference.len() > 1, "the standard run damages");
    // igfem: compare step by step
    let mut ig = setup("igfem_mesh.msh", DAT, vec![line_sphere(VAL)]);
    ig.update("inclusion").unwrap();
    assert_eq!(
        counts(&ig.mesh().read().unwrap())[1..],
        [2, 0],
        "two _igfem_triangle_4"
    );
    tension_bcs(&mut ig);
    let mut s = 0;
    loop {
        let nb = damage_step(&mut ig);
        let reference = reference
            .get(s)
            .unwrap_or_else(|| panic!("step {s}: the standard run has no such step"));
        let compared = check_igfem_state(&ig, reference)
            .unwrap_or_else(|e| panic!("error in the damage computation at step {s}: {e}"));
        assert_eq!(compared, 4, "four damageable (sub-)elements compared");
        s += 1;
        if nb == 0 {
            break;
        }
    }
    eprintln!("igfem: {s} steps");
    assert_eq!(s, reference.len(), "same number of damage steps");
}

/// `…_damage_step_transfer` (see the module docs). Upstream asserts that
/// IGFEM elements 7 and 5 carry the steps of regular elements 27 and 19 and
/// that the next step damages element 7 only; which regular elements the 15
/// steps damage is decided by sub-ulp ties (in this uniaxial patch every
/// equivalent stress is equal to `1e-15` relative at the first step, the
/// first damaged element depends on the solver's rounding, UD-1205). The
/// port checks the deterministic part literally — the replaced element of
/// IGFEM element 7 is regular element 27, of 5 is 19 — and the transfer for
/// **every** IGFEM element against its replaced element; `weaken` lowers
/// the strengths of elements 27 and 19 by 10 % so that non-zero steps are
/// transferred through upstream's ids.
fn damage_step_transfer_scenario(weaken: bool) {
    let mut model = setup(
        "test_damage_transfer.msh",
        DAT_2,
        vec![line_sphere(VAL - 0.6)],
    );
    if weaken {
        let sc = real_internal(&model, 0, "Sc", ElementType::Triangle3);
        for e in [27, 19] {
            set_real_internal(&mut model, 0, "Sc", ElementType::Triangle3, e, 0.9 * sc[e]);
        }
    }
    tension_bcs(&mut model);
    for _ in 0..15 {
        if damage_step(&mut model) == 0 {
            break;
        }
    }
    let steps_reg = uint_internal(&model, 0, "damage_step", ElementType::Triangle3);
    eprintln!("regular damage steps (weaken = {weaken}): {steps_reg:?}");
    if weaken {
        // Upstream's literal scenario, reproduced here: regular
        // element 19 reaches `max_reductions` (10 steps), element 27 takes 5,
        // no other regular element is damaged
        let damaged: Vec<(usize, UInt)> = steps_reg
            .iter()
            .enumerate()
            .filter(|&(_, &k)| k > 0)
            .map(|(e, &k)| (e, k))
            .collect();
        assert_eq!(igfem_law(&mut model).max_reductions, 10);
        assert_eq!(damaged, [(19, 10), (27, 5)], "{steps_reg:?}");
    }
    model.move_interface(VAL - 0.1, "").unwrap();
    let ty = ElementType::IgfemTriangle5;
    let el = |e: usize| Element::try_new(ty, e, GhostType::NotGhost).unwrap();
    let tri = |e: i32| Element::new(ElementType::Triangle3, e, GhostType::NotGhost);
    // upstream: "regular element id -> igfem element id: 27 -> 7; 19 -> 5"
    assert_eq!(model.element_map().get(&el(7)), Some(&tri(27)));
    assert_eq!(model.element_map().get(&el(5)), Some(&tri(19)));
    let nq = model
        .ctx
        .engine()
        .unwrap()
        .nb_integration_points(ty)
        .unwrap();
    let n_el = model
        .mesh()
        .read()
        .unwrap()
        .nb_element(ty, GhostType::NotGhost);
    // every regular element was in law 0 (law-local = global index)
    let old_steps: Vec<UInt> = (0..n_el)
        .map(|e| {
            let o = model.element_map()[&el(e)];
            assert_eq!(o.ty(), ElementType::Triangle3);
            steps_reg[usize::try_from(o.index()).unwrap()]
        })
        .collect();
    let steps = uint_internal(&model, 2, "damage_step", ty);
    let sub = uint_internal(&model, 2, "sub_material", ty);
    for (e, &old_step) in old_steps.iter().enumerate() {
        for q in 0..nq {
            let i = e * nq + q;
            let expect = if sub[i] == 0 { 0 } else { old_step };
            assert_eq!(
                steps[i], expect,
                "element {e} point {q} (sub-material {})",
                sub[i]
            );
        }
    }
    // one more damage event: every IGFEM element keeps its step or gains
    // one on all its damageable points, with d = 1 − a^{−k}
    let old_damage = real_internal(&model, 2, "damage", ty);
    let nb_damaged = damage_step(&mut model);
    assert!(nb_damaged > 0, "the next event damages something");
    let new_steps = uint_internal(&model, 2, "damage_step", ty);
    let new_damage = real_internal(&model, 2, "damage", ty);
    // upstream truncates reduction_constant to an integer (`UInt
    // reduction_constant = getParam<Real>(…)`; 2 here)
    let a = igfem_law(&mut model).reduction_constant;
    let max_red = igfem_law(&mut model).max_reductions;
    // The next event must damage at least one IGFEM element
    // (the check below is otherwise vacuous)
    let gained_elements: Vec<usize> = (0..n_el)
        .filter(|&e| (0..nq).any(|q| new_steps[e * nq + q] != steps[e * nq + q]))
        .collect();
    assert!(
        !gained_elements.is_empty(),
        "the next event damaged no IGFEM element"
    );
    if weaken {
        // upstream's post-move assertions, literally: IGFEM element 7
        // (← 27, k = 5) gets step k + 1 with d = 1 − a^{−(k+1)}; element 5
        // (← 19, at `max_reductions`) is unchanged; every other damageable
        // point stays at step 0 and d = 0
        assert_eq!(gained_elements, [7], "only IGFEM element 7 is damaged");
        let k27 = steps_reg[27];
        for q in 0..nq {
            for e in 0..n_el {
                let i = e * nq + q;
                if sub[i] == 0 {
                    continue;
                }
                match e {
                    7 => {
                        assert_eq!(new_steps[i], k27 + 1, "element 7 point {q}");
                        let expect = 1. - (1. / a.powi(i32::try_from(k27 + 1).unwrap()));
                        assert!(
                            akantu_core::math::are_float_equal(new_damage[i], expect, 1e-14),
                            "element 7 point {q}: damage {} vs {expect}",
                            new_damage[i]
                        );
                    }
                    5 => assert_eq!(
                        (new_steps[i], new_damage[i]),
                        (steps[i], old_damage[i]),
                        "element 5 (at max_reductions) is unchanged"
                    ),
                    _ => assert_eq!(
                        (new_steps[i], new_damage[i]),
                        (0, 0.0),
                        "element {e} point {q}"
                    ),
                }
            }
        }
    }
    for e in 0..n_el {
        let pts: Vec<usize> = (0..nq)
            .map(|q| e * nq + q)
            .filter(|&i| sub[i] != 0)
            .collect();
        for i in (0..nq).map(|q| e * nq + q).filter(|&i| sub[i] == 0) {
            assert_eq!(
                (new_steps[i], new_damage[i]),
                (0, 0.0),
                "elastic point of {e}"
            );
        }
        let gained: Vec<bool> = pts.iter().map(|&i| new_steps[i] == steps[i] + 1).collect();
        assert!(
            gained.iter().all(|&g| g) || gained.iter().all(|&g| !g),
            "element {e}: all damageable points damaged together"
        );
        for &i in &pts {
            if new_steps[i] == steps[i] {
                assert_eq!(new_damage[i], old_damage[i], "element {e}: unchanged");
            } else {
                assert_eq!(new_steps[i], steps[i] + 1, "element {e}: one step");
                if new_steps[i] < max_red {
                    let expect = 1. - (1. / a.powi(i32::try_from(new_steps[i]).unwrap()));
                    assert!(
                        akantu_core::math::are_float_equal(new_damage[i], expect, 1e-14),
                        "element {e}: damage {} vs {expect}",
                        new_damage[i]
                    );
                }
            }
        }
    }
}

#[test]
fn test_material_igfem_iterative_stiffness_reduction_damage_step_transfer() {
    damage_step_transfer_scenario(false);
}

#[test]
fn damage_step_transfer_through_upstreams_elements() {
    damage_step_transfer_scenario(true);
}

#[test]
fn test_material_igfem_iterative_stiffness_reduction_tangent_transfer() {
    let mut model = setup(
        "test_damage_transfer.msh",
        DAT_WEIBULL,
        vec![line_sphere(VAL - 0.6)],
    );
    tension_bcs(&mut model);
    let sc = real_internal(&model, 0, "Sc", ElementType::Triangle3);
    let d = real_internal(&model, 0, "tangent", ElementType::Triangle3);
    let e_mod = 12e9;
    let (gf, h) = (20., 0.25);
    for (i, (&s, &dd)) in sc.iter().zip(&d).enumerate() {
        let eps_u = 2. * gf / (s * h);
        let d_exact = s / (eps_u - (s / e_mod));
        assert!(
            akantu_core::math::are_float_equal(dd, d_exact, 1e-13),
            "error in the tangent or ultimate strain at {i}: {dd} vs {d_exact}"
        );
    }
    let eps_u = real_internal(&model, 0, "ultimate_strain", ElementType::Triangle3);
    let (d27, d19, e27, e19) = (d[27], d[19], eps_u[27], eps_u[19]);
    model.move_interface(VAL - 0.1, "").unwrap();
    let ty = ElementType::IgfemTriangle5;
    let nq = model
        .ctx
        .engine()
        .unwrap()
        .nb_integration_points(ty)
        .unwrap();
    let n_el = model
        .mesh()
        .read()
        .unwrap()
        .nb_element(ty, GhostType::NotGhost);
    let eps_i = real_internal(&model, 2, "ultimate_strain", ty);
    let d_i = real_internal(&model, 2, "tangent", ty);
    let sub = uint_internal(&model, 2, "sub_material", ty);
    let eq = |a: Real, b: Real| akantu_core::math::are_float_equal(a, b, 1e-13);
    for e in 0..n_el {
        for q in 0..nq {
            let i = e * nq + q;
            if sub[i] == 0 {
                assert!(
                    eq(eps_i[i], 0.) && eq(d_i[i], 0.),
                    "elastic sub-element {e}"
                );
            } else if e == 7 {
                assert!(eq(eps_i[i], e27) && eq(d_i[i], d27), "element 7 point {q}");
            } else if e == 5 {
                assert!(eq(eps_i[i], e19) && eq(d_i[i], d19), "element 5 point {q}");
            }
        }
    }
}

#[test]
fn test_volume_computation() {
    let mut model = setup(
        "test_damage_transfer.msh",
        DAT,
        vec![line_sphere(VAL - 0.1)],
    );
    tension_bcs(&mut model);
    model.move_interface(VAL - 0.1, "").unwrap();
    let pass = model.update("inclusion").unwrap();
    assert!(pass.removed_nodes > 0, "the second update is a live move");
    let fe = model.smm.physics.base.fems.get("").unwrap();
    let mesh = model.mesh().read().unwrap();
    let volume_of_law = |law: usize| {
        let filter: Vec<i32> = model
            .smm
            .material(law)
            .unwrap()
            .element_filter()
            .get(ElementType::Triangle3, GhostType::NotGhost)
            .map(|a| a.as_slice().to_vec())
            .unwrap_or_default();
        let nq = fe
            .nb_integration_points(ElementType::Triangle3, GhostType::NotGhost)
            .unwrap();
        let ones = akantu_core::Array::filled(filter.len() * nq, 1, 1.0);
        fe.integrate_scalar(
            ones.view(),
            ElementType::Triangle3,
            GhostType::NotGhost,
            Some(&filter),
        )
        .unwrap()
    };
    let (v0, v1) = (volume_of_law(0), volume_of_law(1));
    let ty = ElementType::IgfemTriangle5;
    let filter: Vec<i32> = model
        .smm
        .material(2)
        .unwrap()
        .element_filter()
        .get(ty, GhostType::NotGhost)
        .map(|a| a.as_slice().to_vec())
        .unwrap_or_default();
    let sub = uint_internal(&model, 2, "sub_material", ty);
    let engine = model.ctx.engine().unwrap();
    let sub_real: Vec<Real> = sub.iter().map(|&s| Real::from(s)).collect();
    let outside = engine
        .integrate_scalar(
            &mesh,
            akantu_core::Array::from_vec(sub_real, 1).unwrap().view(),
            ty,
            GhostType::NotGhost,
            Some(&filter),
        )
        .unwrap();
    let total = engine
        .integrate_scalar(
            &mesh,
            akantu_core::Array::filled(sub.len(), 1, 1.0).view(),
            ty,
            GhostType::NotGhost,
            Some(&filter),
        )
        .unwrap();
    let inside = total - outside;
    eprintln!("volumes: damage {v0}, elastic {v1}, igfem outside {outside}, inside {inside}");
    let eq = |a: Real, b: Real| akantu_core::math::are_float_equal(a, b, 1e-8);
    assert!(
        eq(v0, 0.5) && eq(v1, 0.25) && eq(outside, 0.1) && eq(inside, 0.15),
        "the test failed: {v0} {v1} {outside} {inside}"
    );
}

/// Serial half of `…_parallel` (upstream writes, per step, the barycentre,
/// damage and strength of every damageable regular element and IGFEM
/// sub-element for the parallel run to compare; the parallel half is the
/// D-085 exception): the damage loop with the interface of radius 0.125 at
/// the origin, with invariant checks instead of the file.
#[test]
fn test_material_igfem_iterative_stiffness_reduction_parallel_serial_half() {
    let mut model = setup(
        "test_damage_transfer.msh",
        DAT_2,
        vec![SkSphere::try_from_f64([0., 0., 0.], 0.125 * 0.125).unwrap()],
    );
    model.update("inclusion").unwrap();
    tension_bcs(&mut model);
    let ty = ElementType::IgfemTriangle5;
    let max_red = igfem_law(&mut model).max_reductions;
    let max_damage = igfem_law(&mut model).saw.max_damage;
    let mut steps = 0;
    loop {
        let nb = damage_step(&mut model);
        steps += 1;
        let d = real_internal(&model, 2, "damage", ty);
        let k = uint_internal(&model, 2, "damage_step", ty);
        let sub = uint_internal(&model, 2, "sub_material", ty);
        let a = igfem_law(&mut model).reduction_constant;
        for i in 0..d.len() {
            if sub[i] == 0 {
                assert_eq!((d[i], k[i]), (0.0, 0), "elastic points stay intact");
            } else if k[i] == max_red {
                assert_eq!(d[i], max_damage);
            } else {
                let expect = 1. - 1. / a.powi(i32::try_from(k[i]).unwrap());
                assert!((d[i] - expect).abs() <= 1e-15, "{} vs {expect}", d[i]);
            }
        }
        if nb == 0 {
            break;
        }
        assert!(steps < 500, "the damage loop does not terminate");
    }
    eprintln!("parallel (serial half): {steps} steps");
}
