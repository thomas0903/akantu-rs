// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): extra_packages/igfem/test/test_solid_mechanics_model_igfem/
// test_transfer_internals.cc (+ test_transfer_internals.sh: 40 MPI ranks),
// material_damage.dat, fine_mesh.msh. Built only with the feature `cgal`
// (GPL-3.0-or-later sphere intersector, decision D-042).

//! TEST-201: transfer of internals on interface updates (IGFEM-004 / 005 /
//! 007). Upstream's program: the gel-pocket list `mat_1` is
//! registered **empty** before `initFull` (every element gets the damage
//! law `mat_2`), the damage of every triangle is set to `|barycenter| /
//! (diagonal / 2)`, four pockets are created (`update("mat_1")`), and every
//! damageable integration point (`sub_material = 1`) of every IGFEM element
//! must carry `Sc = 100` (`|ΔSc| ≤ 1e-15`) and the damage of its parent
//! element (summed error `≤ 1e-14`), every other point `Sc = damage = 0`
//! (`≤ Math::tolerance`), with **156** damageable points; then two pockets
//! grow (a live interface move: the IGFEM elements of the first update are
//! replaced, their own internals transferred) and the same check must give
//! **150** points.
//!
//! The transfer copies, for every new element, the value of the replaced
//! element's first integration point above its law's threshold
//! (`extrapolateInternal`), only into the damageable points. Deviation
//! (reported): serial only (the `.sh` runs 40 ranks; UD-609, D-085).

mod common;

use akantu_core::{Element, ElementType, GhostType, Real};
use akantu_geometry_cgal::SkSphere;
use akantu_igfem::SolidMechanicsModelIgfem;
use common::model::*;

const DAT: &str = include_str!("data/upstream/smm_igfem/material_damage.dat");

fn sphere(c: [Real; 2], r: Real) -> SkSphere {
    SkSphere::try_from_f64([c[0], c[1], 0.0], r * r).unwrap()
}

/// Upstream `checkResults`: `(passed, error, counter)`.
fn check_results(model: &SolidMechanicsModelIgfem, diagonal: Real) -> (bool, Real, usize) {
    let mut error = 0.0;
    let mut counter = 0;
    let tol = akantu_core::math::DEFAULT_TOLERANCE;
    let engine = model.ctx.engine().unwrap();
    for ty in [ElementType::IgfemTriangle4, ElementType::IgfemTriangle5] {
        let n = model
            .mesh()
            .read()
            .unwrap()
            .nb_element(ty, GhostType::NotGhost);
        let nq = engine.nb_integration_points(ty).unwrap();
        for e in 0..n {
            let el = Element::try_new(ty, e, GhostType::NotGhost).unwrap();
            let bary = parent_barycenter(model, el);
            let norm = (bary[0] * bary[0] + bary[1] * bary[1]).sqrt();
            let (law, local) = law_and_local(model, el);
            let damage = real_internal(model, law, "damage", ty);
            let sc = real_internal(model, law, "Sc", ty);
            let sub = uint_internal(model, law, "sub_material", ty);
            for q in 0..nq {
                let qg = local * nq + q;
                if sub[qg] == 1 {
                    if (sc[qg] - 100.).abs() > 1e-15 {
                        return (false, error, counter);
                    }
                    error += (damage[qg] - norm / (0.5 * diagonal)).abs();
                    counter += 1;
                } else if sc[qg].abs() > tol || damage[qg].abs() > tol {
                    return (false, error, counter);
                }
            }
        }
    }
    (true, error, counter)
}

#[test]
fn test_transfer_internals() {
    let mesh = common::read_msh(2, "smm_igfem/fine_mesh.msh", "mesh");
    let mut model = model_with_extra_materials(mesh, DAT);
    // the (empty) gel-pocket list, then the selector
    model.register_geometry_object(Vec::new(), "mat_1").unwrap();
    geometry_selector(&mut model, 2);
    init_static(&mut model);
    assert_eq!(
        model.smm.material(2).unwrap().law_type(),
        "igfem_saw_tooth_damage"
    );
    // damage of every triangle = |barycenter| / (diagonal / 2)
    let diagonal = mesh_diagonal(&model);
    let n_tri = model
        .mesh()
        .read()
        .unwrap()
        .nb_element(ElementType::Triangle3, GhostType::NotGhost);
    for e in 0..n_tri {
        let el = Element::try_new(ElementType::Triangle3, e, GhostType::NotGhost).unwrap();
        let b = model.mesh().read().unwrap().barycenter(el).unwrap();
        let (law, local) = law_and_local(&model, el);
        assert_eq!(law, 0, "every element is in mat_2 before the pockets exist");
        let norm = (b[0] * b[0] + b[1] * b[1]).sqrt();
        set_real_internal(
            &mut model,
            law,
            "damage",
            ElementType::Triangle3,
            local,
            norm / (0.5 * diagonal),
        );
    }
    // the inclusions
    let pockets = vec![
        sphere([0., 0.], 0.13),
        sphere([0.5, 0.5], 0.4),
        sphere([-0.75, -0.75], 0.12),
        sphere([0.625, -0.625], 0.25),
    ];
    model.set_geometry_object(pockets.clone(), "mat_1").unwrap();
    let pass = model.update("mat_1").unwrap();
    eprintln!("first update: {pass:?}");
    let (passed, error, counter) = check_results(&model, diagonal);
    assert!(passed, "the transfer check failed");
    eprintln!("The error is: {error}; there are {counter} igfem quads with damage material");
    assert!(
        error <= 1e-14 && counter == 156,
        "{error} {counter} (expected 156)"
    );

    // grow two of the gel pockets and repeat
    let grown = vec![
        sphere([0., 0.], 0.15),
        pockets[1].clone(),
        sphere([-0.75, -0.75], 0.5),
        pockets[3].clone(),
    ];
    model.set_geometry_object(grown, "mat_1").unwrap();
    let pass = model.update("mat_1").unwrap();
    eprintln!("second update: {pass:?}");
    assert!(pass.removed_nodes > 0, "the second update is a live move");
    let (passed, error, counter) = check_results(&model, diagonal);
    assert!(passed, "the transfer check failed after the move");
    eprintln!("The error is: {error}; there are {counter} igfem quads with damage material");
    assert!(
        error <= 1e-14 && counter == 150,
        "{error} {counter} (expected 150)"
    );
}

/// The model of `test_transfer_internals` after its first update.
fn transfer_model() -> (SolidMechanicsModelIgfem, Real) {
    let mesh = common::read_msh(2, "smm_igfem/fine_mesh.msh", "mesh");
    let mut model = model_with_extra_materials(mesh, DAT);
    model.register_geometry_object(Vec::new(), "mat_1").unwrap();
    geometry_selector(&mut model, 2);
    init_static(&mut model);
    let diagonal = mesh_diagonal(&model);
    let n_tri = model
        .mesh()
        .read()
        .unwrap()
        .nb_element(ElementType::Triangle3, GhostType::NotGhost);
    for e in 0..n_tri {
        let el = Element::try_new(ElementType::Triangle3, e, GhostType::NotGhost).unwrap();
        let b = model.mesh().read().unwrap().barycenter(el).unwrap();
        let (law, local) = law_and_local(&model, el);
        let norm = (b[0] * b[0] + b[1] * b[1]).sqrt();
        set_real_internal(
            &mut model,
            law,
            "damage",
            ElementType::Triangle3,
            local,
            norm / (0.5 * diagonal),
        );
    }
    model
        .set_geometry_object(
            vec![sphere([0., 0.], 0.13), sphere([0.5, 0.5], 0.4)],
            "mat_1",
        )
        .unwrap();
    model.update("mat_1").unwrap();
    (model, diagonal)
}

/// Upstream `MaterialIGFEM::extrapolateInternal` on the IGFEM law: every
/// column receives the element's first point above the tolerance (a
/// damageable point; the elastic points before it hold 0), zeros for a name
/// that is not an internal; the copy rule itself on hand-made values.
#[test]
fn extrapolate_internal_of_an_igfem_law() {
    use akantu_igfem::material::{IgfemMaterial, extrapolate_first_point};
    use akantu_igfem::material_damage::MaterialIgfemSawToothDamage;
    let (model, diagonal) = transfer_model();
    let law = model.smm.material(2).unwrap();
    let saw = law
        .as_any()
        .unwrap()
        .downcast_ref::<MaterialIgfemSawToothDamage>()
        .unwrap();
    let ty = ElementType::IgfemTriangle5;
    let nq = model
        .ctx
        .engine()
        .unwrap()
        .nb_integration_points(ty)
        .unwrap();
    let sub = uint_internal(&model, 2, "sub_material", ty);
    let mut checked = 0;
    for e in 0..model
        .mesh()
        .read()
        .unwrap()
        .nb_element(ty, GhostType::NotGhost)
    {
        let el = Element::try_new(ty, e, GhostType::NotGhost).unwrap();
        let (_, local) = law_and_local(&model, el);
        let first_damageable = (0..nq).find(|&q| sub[local * nq + q] != 0);
        let mut out = vec![-1.0; 3];
        saw.extrapolate_internal("damage", el, &mut out).unwrap();
        let b = parent_barycenter(&model, el);
        let expect = if first_damageable.is_some() {
            (b[0] * b[0] + b[1] * b[1]).sqrt() / (0.5 * diagonal)
        } else {
            0.0
        };
        assert_eq!(out, vec![expect; 3], "element {e}");
        saw.extrapolate_internal("no such internal", el, &mut out)
            .unwrap();
        assert_eq!(out, vec![0.0; 3]);
        checked += 1;
    }
    assert!(checked > 0);
    // the copy rule: first point with a norm above the threshold
    let vals = [0.0, 0.0, 1e-20, 0.0, 3.0, 4.0];
    let mut out = vec![0.0; 4];
    extrapolate_first_point(&vals, 2, 0.0, &mut out).unwrap();
    assert_eq!(out, vec![1e-20, 0.0, 1e-20, 0.0], "regular rule: norm > 0");
    extrapolate_first_point(&vals, 2, 1e-14, &mut out).unwrap();
    assert_eq!(
        out,
        vec![3.0, 4.0, 3.0, 4.0],
        "IGFEM rule: norm > tolerance"
    );
    extrapolate_first_point(&[0.0; 4], 2, 0.0, &mut out).unwrap();
    assert_eq!(out, vec![0.0; 4], "point 0 when none qualifies");
    assert!(extrapolate_first_point(&vals, 2, 0.0, &mut [0.0; 3]).is_err());
}

/// Upstream `MaterialIGFEM::{getNbDataForElements, packElementData,
/// unpackElementData}` (`_smm_stress`, IGFEM point counts): the packed
/// size equals `nb_data`, and unpacking into a zeroed stress restores it;
/// other tags pack nothing.
#[test]
fn pack_unpack_stress_round_trip() {
    use akantu_core::enums::SynchronizationTag;
    use akantu_igfem::material::as_igfem_material;
    use akantu_solid::material::Material;
    let (mut model, _) = transfer_model();
    patch_bcs(&mut model);
    set_newton(&mut model, 2, 1e-12);
    model.smm.solve_step(None).unwrap();
    let elements: Vec<Element> = [ElementType::IgfemTriangle5, ElementType::IgfemTriangle4]
        .iter()
        .flat_map(|&ty| {
            let n = model
                .mesh()
                .read()
                .unwrap()
                .nb_element(ty, GhostType::NotGhost);
            (0..n)
                .rev()
                .map(move |e| Element::try_new(ty, e, GhostType::NotGhost).unwrap())
        })
        .collect();
    assert!(!elements.is_empty());
    let base = as_igfem_material(model.smm.material_mut(2).unwrap()).unwrap();
    let before = base
        .base()
        .stress
        .get(ElementType::IgfemTriangle5, GhostType::NotGhost)
        .unwrap()
        .as_slice()
        .to_vec();
    assert!(before.iter().any(|v| *v != 0.0));
    let mut buffer = akantu_comm::CommunicationBuffer::new();
    base.pack_element_data(&mut buffer, &elements, SynchronizationTag::SmmStress)
        .unwrap();
    let size = base
        .nb_data_for_elements(&elements, SynchronizationTag::SmmStress)
        .unwrap();
    assert_eq!(buffer.len(), size);
    let nq: usize = elements
        .iter()
        .map(|e| {
            model
                .ctx
                .engine()
                .unwrap()
                .nb_integration_points(e.ty())
                .unwrap()
        })
        .sum();
    assert_eq!(size, nq * 4 * 8);
    let base = as_igfem_material(model.smm.material_mut(2).unwrap()).unwrap();
    for ty in [ElementType::IgfemTriangle5, ElementType::IgfemTriangle4] {
        if let Ok(mut s) = base.base_mut().stress.get_mut(ty, GhostType::NotGhost) {
            s.as_mut_slice().fill(0.0);
        }
    }
    buffer.rewind();
    base.unpack_element_data(&mut buffer, &elements, SynchronizationTag::SmmStress)
        .unwrap();
    let after = base
        .base()
        .stress
        .get(ElementType::IgfemTriangle5, GhostType::NotGhost)
        .unwrap()
        .as_slice()
        .to_vec();
    assert_eq!(after, before);
    assert_eq!(
        base.nb_data_for_elements(&elements, SynchronizationTag::SmmDensity)
            .unwrap(),
        0
    );
    let mut other = akantu_comm::CommunicationBuffer::new();
    base.pack_element_data(&mut other, &elements, SynchronizationTag::SmmDensity)
        .unwrap();
    assert!(other.is_empty());
}
