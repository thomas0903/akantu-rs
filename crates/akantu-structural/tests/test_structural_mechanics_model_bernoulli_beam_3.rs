// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs (port of Akantu, EPFL/LSMS): port of
// test/test_model/test_structural_mechanics_model/
// test_structural_mechanics_model_bernoulli_beam_3.cc (with the fixture
// test_structural_mechanics_model_fixture.hh).

//! `TestStructBernoulli3Static.TestDisplacements`: two 3-D beams from the
//! origin to `(√2/2, ±√2/2, 0)`, normals `z`, all properties 1, nodes 2 and
//! 3 fully clamped, a vertical force `−1` on the first node; static solve;
//! `w = −5/48`, `θy = −√2/8`, `θx = θz = 0` within `Math::getTolerance()`.
//! The θy shape row (UD-293) does not enter (point loads only): both the
//! corrected default and the upstream mode are checked.

mod structural_model_common;

use akantu_core::ElementType;
use akantu_core::enums::AnalysisMethod;
use akantu_structural::{
    StructuralMaterial, StructuralMechanicsModel, StructuralModelUpstreamCompat,
};
use structural_model_common::*;

const TYPE: ElementType = ElementType::BernoulliBeam3;
const NDOF: usize = 6;

fn fixture(compat: StructuralModelUpstreamCompat) -> StructuralMechanicsModel {
    let mesh = read_upstream_mesh("_bernoulli_beam_3.msh", 3);
    let mut model =
        StructuralMechanicsModel::with_compat(shared(mesh), None, TYPE.name(), compat).unwrap();
    // setNormals (after the model constructor, before initFull, as upstream)
    model
        .set_extra_normals(TYPE, &[0., 0., 1., 0., 0., 1.])
        .unwrap();
    // addMaterials
    let mat = StructuralMaterial {
        e: 1.,
        iz: 1.,
        iy: 1.,
        a: 1.,
        gj: 1.,
        ..StructuralMaterial::default()
    };
    model.add_material(mat, "").unwrap();
    model.init_full_with(AnalysisMethod::Static).unwrap();
    // assignMaterials
    model
        .element_material_mut(TYPE, G)
        .unwrap()
        .as_mut_slice()
        .fill(0);
    // setDirichletBCs: blocking all DOFs of nodes 2 & 3
    model.blocked_dofs_mut().unwrap().as_mut_slice()[NDOF..3 * NDOF].fill(true);
    // setNeumannBCs
    {
        let mut f_guard = model.external_force_mut().unwrap();
        let f = f_guard.as_mut_slice();
        f.fill(0.);
        f[2] = -1.; // vertical force on first node
    }
    model
}

fn check(mut model: StructuralMechanicsModel) {
    model.solve_step(None).unwrap();
    let u_g = model.displacement().unwrap();
    let u = u_g.as_slice();
    let (vz, thx, thy, thz) = (u[2], u[3], u[4], u[5]);
    let tol = MATH_TOLERANCE;
    assert!((vz - -5. / 48.).abs() <= tol, "vz = {vz}");
    assert!((thy - -(2.0_f64.sqrt()) / 8.).abs() <= tol, "thy = {thy}");
    assert!(thz.abs() <= tol, "thz = {thz}");
    assert!(thx.abs() <= tol, "thx = {thx}");
}

/// `TestStructBernoulli3Static.TestDisplacements` (corrected default).
#[test]
fn test_struct_bernoulli_3_static_test_displacements() {
    check(fixture(StructuralModelUpstreamCompat::default()));
}

/// Same in upstream mode.
#[test]
fn test_struct_bernoulli_3_static_test_displacements_upstream_compat() {
    check(fixture(StructuralModelUpstreamCompat::UPSTREAM));
}

/// Normals given on the mesh before the model is constructed (the other
/// way of providing upstream's `extra_normal`) give the same solution, bit
/// for bit.
#[test]
fn normals_before_construction_are_kept() {
    let mut mesh = read_upstream_mesh("_bernoulli_beam_3.msh", 3);
    set_normals(&mut mesh, TYPE, &[vec![0., 0., 1.], vec![0., 0., 1.]]);
    let mut a = StructuralMechanicsModel::new(shared(mesh), None, "a").unwrap();
    let mut b = fixture(StructuralModelUpstreamCompat::default());
    {
        let m = &mut a;
        m.add_material(
            StructuralMaterial {
                e: 1.,
                iz: 1.,
                iy: 1.,
                a: 1.,
                gj: 1.,
                ..StructuralMaterial::default()
            },
            "",
        )
        .unwrap();
        m.init_full_with(AnalysisMethod::Static).unwrap();
        m.blocked_dofs_mut().unwrap().as_mut_slice()[NDOF..3 * NDOF].fill(true);
        m.external_force_mut().unwrap().as_mut_slice()[2] = -1.;
    }
    a.solve_step(None).unwrap();
    b.solve_step(None).unwrap();
    let (ua, ub) = (a.displacement().unwrap(), b.displacement().unwrap());
    assert!(
        ua.as_slice()
            .iter()
            .zip(ub.as_slice())
            .all(|(x, y)| x.to_bits() == y.to_bits())
    );
}

/// Without normals the 3-D beam frame is undefined: upstream reads an
/// uninitialised vector (UD-294); the port refuses at `init_full`.
#[test]
fn missing_normals_are_refused() {
    let mesh = read_upstream_mesh("_bernoulli_beam_3.msh", 3);
    let mut model = StructuralMechanicsModel::new(shared(mesh), None, "no_normals").unwrap();
    model
        .add_material(StructuralMaterial::default(), "")
        .unwrap();
    let r = model.init_full_with(AnalysisMethod::Static);
    assert!(r.is_err(), "{r:?}");
}
