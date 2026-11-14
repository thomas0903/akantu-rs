// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): test/test_model/test_solid_mechanics_model/
// test_embedded_interface/test_embedded_element_matrix.cc (inputs
// triangle.msh, embedded_element.dat).

//! Port of upstream `test_embedded_element_matrix` (TEST-125): one
//! reinforcement bar at height `h = 0.4` across the unit right triangle
//! `(0,0), (1,0), (0,1)` (bulk law `null` with `E = 0`, reinforcement
//! `E = A = 1`). The interface mesh must have one `_segment_2` in a 2-D
//! space; `solveStep` meets a singular matrix (upstream catches
//! `SingularMatrixException` and fails on any other exception); the
//! assembled stiffness must satisfy `K(0,0) = K(2,2) = 1 − h` and
//! `K(0,2) = K(2,0) = h − 1` (`Math::are_float_equal`, tolerance 1e-8): the
//! bar spans `x ∈ [0, 1 − h]` and couples the x displacements of nodes 0
//! and 1 through `∂N/∂x = (−1, 1, 0)`.

mod common;

use akantu_core::enums::AnalysisMethod;
use akantu_core::{ElementType, Error, GhostType};
use akantu_embedded::EmbeddedInterfaceModelOptions;

#[test]
fn test_embedded_element_matrix() {
    const DIM: usize = 2;
    let height = 0.4;
    let mesh = common::read_mesh(&common::data("triangle.msh"), DIM, "mesh");
    let reinforcement = common::reinforcement_mesh(
        DIM,
        &[&[0., height], &[1., height]],
        &[[0, 1]],
        &["reinforcement"],
    );
    let dat = std::fs::read_to_string(common::data("embedded_element.dat")).unwrap();
    let mut model = common::model(mesh, reinforcement, &dat);
    model
        .init_full(&EmbeddedInterfaceModelOptions::new(AnalysisMethod::Static))
        .unwrap();

    assert_eq!(
        model
            .interface_mesh()
            .nb_element(ElementType::Segment2, GhostType::NotGhost),
        1
    );
    assert_eq!(model.interface_mesh().spatial_dimension(), 2);

    // matrix should be singular
    match model.solve_step(None) {
        Ok(()) => {}
        Err(Error::LinearSolver(msg)) if msg.contains("singular") => {
            eprintln!("Matrix is singular, relax, everything is fine :)");
        }
        Err(e) => panic!("Unexpected error: {e}"),
    }

    let k = model.smm.dof_manager().matrix("K").unwrap();
    let tol = 1e-8; // Math::setTolerance(1e-8)
    assert!(common::are_float_equal(k.get(0, 0), 1. - height, tol));
    assert!(common::are_float_equal(k.get(0, 2), height - 1., tol));
    assert!(common::are_float_equal(k.get(2, 0), height - 1., tol));
    assert!(common::are_float_equal(k.get(2, 2), 1. - height, tol));
}
