// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): test/test_model/test_solid_mechanics_model/
// test_embedded_interface/test_embedded_interface_model_prestress.cc
// (inputs embedded_mesh_prestress.msh,
// embedded_mesh_prestress_reinforcement.msh, prestress.dat).

//! Port of upstream `test_embedded_interface_model_prestress` (TEST-127).
//!
//! A concrete beam `10 × 1` (10 398 linear triangles, `E = 30e9`, `ν = 0`,
//! plane stress) with a pre-stressed reinforcement along `y = 0.25` over
//! the whole length (`E = 210e9`, `A = 1e-2`, `pre_stress = 1e6`); the face
//! `x = 10` (`XBlocked`) is blocked in x, the corner `(0, 0)` (`YBlocked`)
//! in y; static solve without external load. The reactions on `XBlocked`
//! must equal the stress resultants of the transformed-section beam theory
//! (concrete: linear stress of the eccentric pre-stress force; steel:
//! `pre_stress − P/A_eq` plus bending at the bar): the sum of the absolute
//! reactions, with the reaction of the bar-end node `EndNode = (10, 0.25)`
//! split into a concrete part (interpolated from the two neighbouring face
//! nodes) and a steel part, against `∫|σ_c| dy + |σ_s| A`, relative error
//! ≤ 1e-3 (upstream `EXIT_FAILURE` otherwise).
//!
//! Upstream also builds an "analytical residual" from a Neumann condition
//! on `XBlocked` and then zeroes the external force before solving (and
//! adds the steel force to a *copy* of a nodal vector): it never enters the
//! check, so it is not reproduced.

mod common;

use akantu_core::Real;
use akantu_core::enums::{AnalysisMethod, SpatialDirection};
use akantu_embedded::EmbeddedInterfaceModelOptions;
use akantu_model::boundary_condition::FixedValue;
use akantu_parser::ParamValue;

const YG: Real = 0.483644859;
const I_EQ: Real = 0.012488874;
const A_EQ: Real = 1e-2 + 1. / 7. * 1.;

/// Upstream `StressSolution::stress(height)`: `−M/I (y − y_g) + σ_0`.
struct StressSolution {
    m: Real,
    i: Real,
    yg: Real,
    pre_stress: Real,
}

impl StressSolution {
    fn stress(&self, height: Real) -> Real {
        -self.m / self.i * (height - self.yg) + self.pre_stress
    }
}

#[test]
fn test_embedded_interface_model_prestress() {
    const DIM: usize = 2;
    let tol = 1e-6; // Math::setTolerance(1e-6)
    let mesh = common::read_mesh(&common::data("embedded_mesh_prestress.msh"), DIM, "mesh");
    let reinforcement = common::read_mesh(
        &common::data("embedded_mesh_prestress_reinforcement.msh"),
        DIM,
        "reinforcement_mesh",
    );
    let dat = std::fs::read_to_string(common::data("prestress.dat")).unwrap();
    let mut model = common::model(mesh, reinforcement, &dat);
    model
        .init_full(&EmbeddedInterfaceModelOptions::new(AnalysisMethod::Static))
        .unwrap();

    // Computation of analytical residual
    let law = model.smm.material_by_name("reinforcement").unwrap();
    let real = |v: ParamValue| match v {
        ParamValue::Real(x) => x,
        other => panic!("not a real: {other:?}"),
    };
    let steel_area = real(law.get_param("area").unwrap());
    let pre_stress = real(law.get_param("pre_stress").unwrap());
    assert_eq!((steel_area, pre_stress), (1e-2, 1e6));

    let pre_force = pre_stress * steel_area;
    let pre_moment = -pre_force * (YG - 0.25);
    let neutral_axis = YG - I_EQ / A_EQ * pre_force / pre_moment;

    let concrete_stress = StressSolution {
        m: pre_moment,
        i: 7. * I_EQ,
        yg: YG,
        pre_stress: -pre_force / (7. * A_EQ),
    };
    let steel_stress = StressSolution {
        m: pre_moment,
        i: I_EQ,
        yg: YG,
        pre_stress: pre_stress - pre_force / A_EQ,
    };

    let stress_norm = concrete_stress.stress(1.).abs() * (1. - neutral_axis) * 0.5
        + concrete_stress.stress(0.).abs() * neutral_axis * 0.5
        + steel_stress.stress(0.25).abs() * steel_area;

    model
        .smm
        .apply_dirichlet_bc(
            &FixedValue {
                value: 0.,
                axis: SpatialDirection::X,
            },
            "XBlocked",
        )
        .unwrap();
    model
        .smm
        .apply_dirichlet_bc(
            &FixedValue {
                value: 0.,
                axis: SpatialDirection::Y,
            },
            "YBlocked",
        )
        .unwrap();

    model.solve_step(None).unwrap();

    // Computation of FEM residual norm
    let (boundary_nodes, end_node) = {
        let m = model.smm.physics.base.mesh.read().unwrap();
        let g = m.groups();
        let xb = g.element_group("XBlocked").unwrap().nodes().to_vec();
        let end = g.element_group("EndNode").unwrap().nodes().as_slice()[0];
        (xb, end)
    };
    let position = common::nodes(&model);

    model.assemble_internal_forces().unwrap();
    let f_int = model.smm.internal_force().unwrap().as_slice().to_vec();
    let f_ext = model.smm.external_force().unwrap().as_slice().to_vec();
    let res = |n: usize| -> [Real; DIM] {
        [
            f_int[n * DIM] - f_ext[n * DIM],
            f_int[n * DIM + 1] - f_ext[n * DIM + 1],
        ]
    };

    let mut res_sum = 0.;
    let mut lower_node = None;
    let mut upper_node = None;
    let mut lower_dist = 1.;
    let mut upper_dist = 1.;
    for &node in &boundary_nodes {
        let node = usize::try_from(node).unwrap();
        let r = res(node);
        let pos = &position[node];
        if !common::are_float_equal(pos[1], 0.25, tol) {
            if (pos[1] - 0.25).abs() < lower_dist && pos[1] < 0.25 {
                lower_dist = (pos[1] - 0.25).abs();
                lower_node = Some(node);
            }
            if (pos[1] - 0.25).abs() < upper_dist && pos[1] > 0.25 {
                upper_dist = (pos[1] - 0.25).abs();
                upper_node = Some(node);
            }
        }
        for ri in r {
            if !common::are_float_equal(pos[1], 0.25, tol) {
                res_sum += ri.abs();
            }
        }
    }

    let upper_res = res(upper_node.unwrap());
    let lower_res = res(lower_node.unwrap());
    let end_node_res = res(usize::try_from(end_node).unwrap());
    for i in 0..DIM {
        let delta = (upper_res[i] - lower_res[i]) * (lower_dist / (upper_dist + lower_dist));
        let concrete_residual = lower_res[i] + delta;
        let steel_residual = end_node_res[i] - concrete_residual;
        res_sum += concrete_residual.abs();
        res_sum += steel_residual.abs();
    }

    let relative_error = (res_sum - stress_norm).abs() / stress_norm;
    eprintln!(
        "res_sum = {res_sum:.10e}, stress_norm = {stress_norm:.10e}, relative error = {relative_error:.3e}"
    );
    assert!(relative_error <= 1e-3, "Relative error = {relative_error}");
}
