// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS) behaviour (no upstream test):
// `SolidMechanicsModel::initSolver` (src/model/solid_mechanics/
// solid_mechanics_model.cc:212-216) resets `current_position` to the mesh
// nodes without touching `current_position_release`;
// `updateCurrentPosition` (solid_mechanics_model.cc:411-424) refreshes
// only when the displacement release changed.

//! Current positions after a solver created once a displacement exists
//! (`initNewSolver` after a static solve, as the cohesive scalability
//! benchmark does before `getStableTimeStep()`):
//!
//! * no reader refreshed the positions at the current displacement release
//!   (the benchmark: no cohesive element exists during the static solve, so
//!   upstream never calls `getCurrentPosition()`): the next reader sees
//!   `X + u` — upstream and the port agree whatever the `init_solver` rule;
//! * a reader refreshed them at that release: upstream's reset to `X`
//!   sticks until the next displacement update. That is the defect UD-352
//!   (D-064: category B, corrected): with the compat switch
//!   `SmmUpstreamCompat::init_solver_resets_current_position` the port
//!   reproduces upstream (`X`), by default it keeps `X + u`.

use akantu_core::enums::AnalysisMethod;
use akantu_core::{ElementType, GhostType};
use akantu_mesh::{Mesh, MeshAccessor, SharedMesh};
use akantu_model::ModelOptions;
use akantu_parser::Parser;
use akantu_solid::SolidMechanicsModel;

const DAT: &str = "material elastic [\n  name = steel\n  rho = 1\n  E = 1\n  nu = 0.3\n]\n";

/// A statically solved tetrahedron (node 3 pulled in z), `compat` = the
/// UD-352 compat switch; returns the model and the mesh nodes.
fn solved_tet(compat: bool) -> (SolidMechanicsModel, Vec<f64>) {
    let mut m = Mesh::new(3, "tet").unwrap();
    {
        let mut a = MeshAccessor::new(&mut m);
        for x in [
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 1.0],
        ] {
            a.push_node(&x).unwrap();
        }
        a.connectivity_mut(ElementType::Tetrahedron4, GhostType::NotGhost)
            .push_tuple(&[0, 1, 2, 3]);
    }
    m.compute_bounding_box().unwrap();
    let shared = SharedMesh::new(m);
    let parser = Parser::parse_str(DAT, "material.dat").unwrap();
    let mut model = SolidMechanicsModel::with_parser(shared.clone(), None, "smm", parser).unwrap();
    model
        .physics
        .upstream_compat
        .init_solver_resets_current_position = compat;
    model
        .init_full(&ModelOptions::new(AnalysisMethod::Static))
        .unwrap();
    {
        let mut b_g = model.blocked_dofs_mut().unwrap();
        let b = b_g.as_mut_slice();
        b[..3].fill(true); // node 0
        b[4] = true; // node 1: y, z
        b[5] = true;
        b[8] = true; // node 2: z
    }
    model.external_force_mut().unwrap().as_mut_slice()[11] = 0.1; // node 3, z
    model.solve_step(None).unwrap();
    let x = shared.read().unwrap().nodes().as_slice().to_vec();
    (model, x)
}

/// No reader before `initNewSolver` (the scalability benchmark): `X + u`
/// under both rules.
#[test]
fn current_position_is_x_plus_u_after_a_new_solver_without_reader() {
    for compat in [false, true] {
        let (mut model, x) = solved_tet(compat);
        model
            .init_new_solver(AnalysisMethod::ExplicitLumpedMass)
            .unwrap();
        let u = model.displacement().unwrap().as_slice().to_vec();
        assert!(u[11] > 0.0, "the static solve moved node 3");
        let cp = model.current_position().unwrap().as_slice().to_vec();
        for i in 0..x.len() {
            assert_eq!(cp[i], x[i] + u[i], "compat {compat}, component {i}");
        }
    }
}

/// D-064 corrected default: after a reader at the current release, a new
/// solver keeps the refreshed `X + u`.
#[test]
fn ud352_default_keeps_x_plus_u_after_a_reader_at_the_current_release() {
    let (mut model, x) = solved_tet(false);
    let _ = model.current_position().unwrap();
    model
        .init_new_solver(AnalysisMethod::ExplicitLumpedMass)
        .unwrap();
    let u = model.displacement().unwrap().as_slice().to_vec();
    let cp = model.current_position().unwrap().as_slice().to_vec();
    for i in 0..x.len() {
        assert_eq!(cp[i], x[i] + u[i], "component {i}");
    }
    assert_ne!(cp, x);
}

/// Upstream (compat switch): the reset to `X` sticks after a reader at the
/// current release (UD-352).
#[test]
fn ud352_reset_to_x_sticks_after_a_reader_at_the_current_release() {
    let (mut model, x) = solved_tet(true);
    // a reader at the current displacement release (upstream: the cohesive
    // laws' `computeNormal(getCurrentPosition())` once cohesive elements
    // exist)
    let _ = model.current_position().unwrap();
    model
        .init_new_solver(AnalysisMethod::ExplicitLumpedMass)
        .unwrap();
    let u = model.displacement().unwrap().as_slice().to_vec();
    assert!(u[11] > 0.0, "the static solve moved node 3");
    let cp = model.current_position().unwrap().as_slice().to_vec();
    assert_eq!(cp, x, "upstream keeps current_position = X (UD-352)");
}
