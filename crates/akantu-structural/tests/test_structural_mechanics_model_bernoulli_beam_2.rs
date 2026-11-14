// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs (port of Akantu, EPFL/LSMS): port of
// test/test_model/test_structural_mechanics_model/
// test_structural_mechanics_model_bernoulli_beam_2.cc (with the fixture
// test_structural_mechanics_model_fixture.hh).

//! `TestStructBernoulli2.TestDisplacements`: two 2-D beams (x = 0, 10, 18),
//! two materials, clamped first node, rollers on the others, the uniform
//! load of the first span as nodal equivalents, a moment and an axial
//! force on the last node; static solve; rotations `5.6/4800`,
//! `−3.7/4800` and the axial displacement `10/18` within
//! `Math::getTolerance()`.

mod structural_model_common;

use akantu_core::ElementType;
use akantu_core::enums::AnalysisMethod;
use akantu_structural::{
    StructuralMaterial, StructuralMechanicsModel, StructuralModelUpstreamCompat,
};
use structural_model_common::*;

const TYPE: ElementType = ElementType::BernoulliBeam2;
const NDOF: usize = 3;

/// The upstream fixture: read the mesh, construct the model (id = the type
/// name), add the materials, `initFull(_static)`, assign the materials, set
/// the Dirichlet and Neumann conditions. Returns the model and the second
/// material (its `E·A/18` is the axial force).
fn fixture(
    compat: StructuralModelUpstreamCompat,
) -> (StructuralMechanicsModel, StructuralMaterial) {
    let mesh = read_upstream_mesh("_bernoulli_beam_2.msh", 2);
    let mut model =
        StructuralMechanicsModel::with_compat(shared(mesh), None, TYPE.name(), compat).unwrap();
    // addMaterials
    let mut mat = StructuralMaterial {
        e: 3e10,
        i: 0.0025,
        a: 0.01,
        ..StructuralMaterial::default()
    };
    model.add_material(mat, "").unwrap();
    mat.e = 3e10;
    mat.i = 0.00128;
    mat.a = 0.01;
    model.add_material(mat, "").unwrap();
    model.init_full_with(AnalysisMethod::Static).unwrap();
    // assignMaterials
    {
        let mut m = model.element_material_mut(TYPE, G).unwrap();
        m.as_mut_slice()[0] = 0;
        m.as_mut_slice()[1] = 1;
    }
    // setDirichletBCs
    {
        let mut b_g = model.blocked_dofs_mut().unwrap();
        let b = b_g.as_mut_slice();
        b[..NDOF].copy_from_slice(&[true, true, true]);
        b[NDOF..2 * NDOF].copy_from_slice(&[false, true, false]);
        b[2 * NDOF..3 * NDOF].copy_from_slice(&[false, true, false]);
    }
    // setNeumannBCs
    let (m, q, l) = (3600.0, 6000.0, 10.0);
    {
        let mut f_guard = model.external_force_mut().unwrap();
        let f = f_guard.as_mut_slice();
        f[2 * NDOF + 2] = -m; // moment on last node
        // as long as integration is not available
        f[1] = -q * l / 2.;
        f[2] = -q * l * l / 12.;
        f[NDOF + 1] = -q * l / 2.;
        f[NDOF + 2] = q * l * l / 12.;
        f[2 * NDOF] = mat.e * mat.a / 18.;
    }
    (model, mat)
}

fn check_displacements(model: &mut StructuralMechanicsModel) {
    model.solve_step(None).unwrap();
    let u_g = model.displacement().unwrap();
    let u = u_g.as_slice();
    let d1 = u[NDOF + 2];
    let d2 = u[2 * NDOF + 2];
    let d3 = u[NDOF];
    let tol = MATH_TOLERANCE;
    assert!((d1 - 5.6 / 4800.).abs() <= tol, "first rotation {d1}");
    assert!((d2 - -3.7 / 4800.).abs() <= tol, "second rotation {d2}");
    assert!((d3 - 10. / 18.).abs() <= tol, "axial deformation {d3}");
}

/// `TestStructBernoulli2.TestDisplacements` (corrected default).
#[test]
fn test_struct_bernoulli_2_test_displacements() {
    let (mut model, _) = fixture(StructuralModelUpstreamCompat::default());
    check_displacements(&mut model);
}

/// Same, every upstream compatibility switch on (2-D beams are not affected
/// by any of them).
#[test]
fn test_struct_bernoulli_2_test_displacements_upstream_compat() {
    let (mut model, _) = fixture(StructuralModelUpstreamCompat::UPSTREAM);
    check_displacements(&mut model);
}

/// The fixture through the default (`_explicit_lumped_mass`) analysis of
/// `bernoulli_beam_2_example.cc` is not the static solution: the example
/// calls `initFull()` without `_static` (UD-514). The static analysis with
/// the example's loads reproduces the test's values; the bending moments
/// it prints (`N_M(0, 1)`, `N_M(2, 1)`) are the generalised stresses of the
/// first quadrature point of each element, checked here against the
/// equilibrium moment of the hyperstatic beam at those points.
#[test]
fn bernoulli_beam_2_example_static_stresses() {
    let (mut model, mat) = fixture(StructuralModelUpstreamCompat::default());
    model.solve_step(None).unwrap();
    model.assemble_internal_force().unwrap();
    let s = model.stress(TYPE, G).unwrap();
    assert_eq!(s.nb_component(), 2);
    let nq = model.fe_engine().nb_integration_points(TYPE, G).unwrap();
    assert_eq!(s.len(), 2 * nq);
    // axial force N = E·A·ε is constant: E·A/18 in both elements (the
    // axial load acts on the last node, the first node is clamped)
    let n_expected = mat.e * mat.a / 18.;
    for q in 0..2 * nq {
        let n = s.tuple(q)[0];
        assert!(
            (n - n_expected).abs() <= 1e-9 * n_expected,
            "axial force at point {q}: {n} vs {n_expected}"
        );
    }
    // the internal forces balance the external ones on the free DOFs
    let fi = model.internal_force().unwrap().as_slice().to_vec();
    let fe = model.external_force().unwrap().as_slice().to_vec();
    let blocked = model.blocked_dofs().unwrap().as_slice().to_vec();
    let scale = fe.iter().fold(0.0_f64, |m, v| m.max(v.abs()));
    for k in 0..fi.len() {
        if !blocked[k] {
            assert!(
                (fi[k] + fe[k]).abs() <= 1e-10 * scale,
                "equilibrium at DOF {k}: f_int {} f_ext {}",
                fi[k],
                fe[k]
            );
        }
    }
}
