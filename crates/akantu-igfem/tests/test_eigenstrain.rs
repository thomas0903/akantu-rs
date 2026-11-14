// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): extra_packages/igfem/test/test_solid_mechanics_model_igfem/
// test_eigenstrain.cc (material_damage.dat, fine_mesh.msh). Built only with
// the feature `cgal` (GPL-3.0-or-later sphere intersector, decision D-042).

//! TEST-202: four gel pockets on `fine_mesh.msh` with upstream's
//! `material_damage.dat` (`damage_iterative` `mat_2`, `elastic` `mat_1`,
//! `igfem_saw_tooth_damage` with sub-materials `mat_1` / `mat_2`); no
//! `_igfem_triangle_4` expected; `applyEigenGradU(0.07·I, "mat_1")` must set
//! the eigen gradient of every point of the regular law `mat_1`, of no point
//! of `mat_2`, and of exactly the IGFEM points whose sub-material is
//! `mat_1` (upstream criterion: summed Frobenius error `≤ Math::tolerance`).
//!
//! As upstream, the (empty) pocket list is registered before `initFull`
//! (every element in `mat_2`) and filled afterwards; the update's
//! `reassignMaterial` moves the regular elements inside the pockets to
//! `mat_1`. The damage laws exist (IGFEM-007, XMAT-001):
//! the earlier adaptation to `elastic` / `igfem_elastic` is gone.

mod common;

use akantu_core::tensor::MatrixX;
use akantu_core::{ElementType, GhostType, Real};
use akantu_geometry_cgal::SkSphere;
use common::model::*;

const DAT: &str = include_str!("data/upstream/smm_igfem/material_damage.dat");

fn frob(m: &[Real]) -> Real {
    m.iter().map(|v| v * v).sum::<Real>().sqrt()
}

#[test]
fn test_eigenstrain() {
    let mesh = common::read_msh(2, "smm_igfem/fine_mesh.msh", "mesh");
    let mut model = model_with_extra_materials(mesh, DAT);
    model.register_geometry_object(Vec::new(), "mat_1").unwrap();
    geometry_selector(&mut model, 2);
    init_static(&mut model);
    let pockets = vec![
        SkSphere::try_from_f64([0., 0., 0.], 0.13 * 0.13).unwrap(),
        SkSphere::try_from_f64([0.5, 0.5, 0.], 0.4 * 0.4).unwrap(),
        SkSphere::try_from_f64([-0.75, -0.75, 0.], 0.12 * 0.12).unwrap(),
        SkSphere::try_from_f64([0.625, -0.625, 0.], 0.25 * 0.25).unwrap(),
    ];
    model.set_geometry_object(pockets, "mat_1").unwrap();
    model.update("mat_1").unwrap();
    let c = counts(&model.mesh().read().unwrap());
    eprintln!("counts {c:?}");
    assert_eq!(c[1], 0, "something went wrong in the interface creation");
    assert!(c[2] > 0);
    let mut prestrain = MatrixX::zeros(2, 2);
    for i in 0..2 {
        *prestrain.at_mut(i, i) = 0.07;
    }
    model
        .apply_eigen_grad_u(&prestrain, "mat_1", GhostType::NotGhost)
        .unwrap();
    let p = prestrain.as_slice().to_vec();
    let diff = |m: &[Real]| frob(&[p[0] - m[0], p[1] - m[1], p[2] - m[2], p[3] - m[3]]);
    let mut error = 0.0;
    let mut counted = [0usize; 3];
    let e0 = model
        .smm
        .material(0)
        .unwrap()
        .base()
        .eigen_grad_u
        .get(ElementType::Triangle3, GhostType::NotGhost)
        .unwrap();
    for m in e0.as_slice().as_chunks::<4>().0 {
        error += frob(m);
        counted[0] += 1;
    }
    let e1 = model
        .smm
        .material(1)
        .unwrap()
        .base()
        .eigen_grad_u
        .get(ElementType::Triangle3, GhostType::NotGhost)
        .unwrap();
    for m in e1.as_slice().as_chunks::<4>().0 {
        error += diff(m);
        counted[1] += 1;
    }
    let law = model.smm.material(2).unwrap();
    assert_eq!(law.law_type(), "igfem_saw_tooth_damage");
    let e2 = law
        .base()
        .eigen_grad_u
        .get(ElementType::IgfemTriangle5, GhostType::NotGhost)
        .unwrap();
    let sub = akantu_igfem::material::as_igfem(law)
        .unwrap()
        .igfem_base()
        .sub_material
        .get(ElementType::IgfemTriangle5, GhostType::NotGhost)
        .unwrap();
    for (m, &s) in e2.as_slice().as_chunks::<4>().0.iter().zip(sub.as_slice()) {
        error += if s == 0 { diff(m) } else { frob(m) };
        counted[2] += 1;
    }
    eprintln!("The error in the prestrain is: {error} (points {counted:?})");
    assert!(counted.iter().all(|&n| n > 0), "{counted:?}");
    assert!(
        error.abs() <= akantu_core::math::DEFAULT_TOLERANCE,
        "The test failed: {error}"
    );
}
