// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs (port of Akantu, EPFL/LSMS): ports of
// test/test_model/test_structural_mechanics_model/
// test_structural_mechanics_model_discrete_kirchhoff_triangle_18.cc (with
// the fixture test_structural_mechanics_model_fixture.hh) and of the intent
// of the unregistered test_structural_mechanics_model_kirchhoff_shell_patch_
// test_4_5_5.cc (UD-016).

//! * `TestStructDKT18.TestDisplacements` — Batoz vol. 2 p. 392 patch test
//!   (ISBN 2-86601-259-3): 4 triangles around an interior node, every DOF
//!   blocked except the interior node's `u, v, w, θx, θy`; the boundary
//!   nodes carry a linear membrane field and the bending field
//!   `w = −(x² + xy + y²)/2` (`θx = ∂w/∂y`, `θy = −∂w/∂x`); the interior
//!   node must reproduce the field (`‖err‖ < 1e-12`).
//! * `test_structural_mechanics_model_kirchhoff_shell_patch_test_4_5_5` —
//!   the unregistered upstream program (removed API `updateResidual`,
//!   `testConvergenceIncrement`, and rotations in an older convention, so
//!   its prescribed field is not a patch field for the current element; it
//!   printed the interior node and asserted nothing): its 5-node / 4-element
//!   mesh (a = 20, b = 10, interior node (15, 15)), `E = 1000`, `ν = 0.3`,
//!   `t = 1`, the pure bending field above, every DOF of the corners
//!   blocked, `θz` of the interior node blocked; the interior node must be
//!   exact.

mod structural_model_common;

use akantu_core::enums::AnalysisMethod;
use akantu_core::{ElementType, Idx, Real};
use akantu_structural::{
    StructuralMaterial, StructuralMechanicsModel, StructuralModelUpstreamCompat,
};
use structural_model_common::*;

const TYPE: ElementType = ElementType::DiscreteKirchhoffTriangle18;
const NDOF: usize = 6;

fn dkt18_fixture(compat: StructuralModelUpstreamCompat) -> StructuralMechanicsModel {
    let mesh = read_upstream_mesh("_discrete_kirchhoff_triangle_18.msh", 3);
    let mut model =
        StructuralMechanicsModel::with_compat(shared(mesh), None, TYPE.name(), compat).unwrap();
    let mat = StructuralMaterial {
        e: 1.,
        t: 1.,
        nu: 0.3,
        ..StructuralMaterial::default()
    };
    model.add_material(mat, "").unwrap();
    model.init_full_with(AnalysisMethod::Static).unwrap();
    model
        .element_material_mut(TYPE, G)
        .unwrap()
        .as_mut_slice()
        .fill(0);
    // setDirichletBCs
    let nb_nodes = model.displacement().unwrap().len();
    {
        let mut b_g = model.blocked_dofs_mut().unwrap();
        let b = b_g.as_mut_slice();
        b.fill(true);
        let c = (nb_nodes - 1) * NDOF;
        b[c..c + NDOF].copy_from_slice(&[false, false, false, false, false, true]);
    }
    {
        let mut u_g = model.displacement_mut().unwrap();
        let u = u_g.as_mut_slice();
        u.fill(0.);
        // Displacement field from Batoz Vol. 2 p. 392 with theta to beta
        // correction from discrete Kirchhoff condition (membrane + bending)
        let rows: [[Real; 6]; 3] = [
            [40., 20., -800., -20., 40., 0.],
            [50., 40., -1400., -40., 50., 0.],
            [10., 20., -200., -20., 10., 0.],
        ];
        for (k, r) in rows.iter().enumerate() {
            u[(k + 1) * NDOF..(k + 2) * NDOF].copy_from_slice(r);
        }
    }
    model
}

fn check_center(model: &mut StructuralMechanicsModel) {
    model.solve_step(None).unwrap();
    let solution = [22.5, 22.5, -337.5, -22.5, 22.5, 0.];
    let u = model.displacement().unwrap();
    let c = u.tuple(u.len() - 1);
    let err: Real = solution
        .iter()
        .zip(c)
        .map(|(s, v)| (s - v) * (s - v))
        .sum::<Real>()
        .sqrt();
    assert!(err <= 1e-12, "center node {c:?}, error {err}");
}

/// `TestStructDKT18.TestDisplacements` (corrected default; `t = 1`, so the
/// UD-007 bending rigidity is the same in both modes).
#[test]
fn test_struct_dkt18_test_displacements() {
    let mut m = dkt18_fixture(StructuralModelUpstreamCompat::default());
    check_center(&mut m);
}

/// Same in upstream mode.
#[test]
fn test_struct_dkt18_test_displacements_upstream_compat() {
    let mut m = dkt18_fixture(StructuralModelUpstreamCompat::UPSTREAM);
    check_center(&mut m);
}

/// Bending field `w = −(x² + xy + y²)/2` with the element's rotation
/// convention `θx = ∂w/∂y`, `θy = −∂w/∂x`.
fn bending_field(x: Real, y: Real) -> [Real; 6] {
    [
        0.,
        0.,
        -(x * x + x * y + y * y) / 2.,
        -(x / 2. + y),
        x + y / 2.,
        0.,
    ]
}

/// Port of the intent of `test_structural_mechanics_model_kirchhoff_shell_
/// patch_test_4_5_5.cc` (see the module docs), for several thicknesses
/// (the patch is exact whatever the bending rigidity, UD-007).
#[test]
fn test_structural_mechanics_model_kirchhoff_shell_patch_test_4_5_5() {
    let (a, b) = (20., 10.);
    let nodes = vec![
        vec![0., 0., 0.],
        vec![2. * a, 0., 0.],
        vec![0., 2. * b, 0.],
        vec![2. * a, 2. * b, 0.],
        vec![15., 15., 0.],
    ];
    let conn: Vec<Vec<Idx>> = vec![vec![1, 3, 4], vec![3, 2, 4], vec![2, 4, 0], vec![0, 1, 4]];
    for t in [1.0, 0.25] {
        let mesh = mesh_of(3, TYPE, &nodes, &conn, None);
        let mut model = StructuralMechanicsModel::new(shared(mesh), None, "shell").unwrap();
        model
            .add_material(
                StructuralMaterial {
                    e: 1000.,
                    nu: 0.3,
                    t,
                    ..StructuralMaterial::default()
                },
                "",
            )
            .unwrap();
        model.init_full_with(AnalysisMethod::Static).unwrap();
        {
            let mut u_g = model.displacement_mut().unwrap();
            let u = u_g.as_mut_slice();
            for (n, x) in nodes.iter().enumerate().take(4) {
                u[n * NDOF..(n + 1) * NDOF].copy_from_slice(&bending_field(x[0], x[1]));
            }
        }
        {
            let mut bl_g = model.blocked_dofs_mut().unwrap();
            let bl = bl_g.as_mut_slice();
            bl[..4 * NDOF].fill(true);
            bl[4 * NDOF + 5] = true;
            // membrane DOFs of the interior node are free but unloaded
        }
        model.solve_step(None).unwrap();
        let u = model.displacement().unwrap();
        let expected = bending_field(15., 15.);
        // same criterion as TestStructDKT18 (‖err‖ < 1e-12)
        let err: Real = u
            .tuple(4)
            .iter()
            .zip(expected)
            .map(|(g, w)| (g - w) * (g - w))
            .sum::<Real>()
            .sqrt();
        assert!(
            err <= 1e-12,
            "t = {t}: {:?} vs {expected:?} ({err})",
            u.tuple(4)
        );
    }
}
