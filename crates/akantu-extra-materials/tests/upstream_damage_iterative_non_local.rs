// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): extra_packages/extra-materials/test/
// test_material_damage/{test_material_damage_iterative_non_local_serial.cc,
// material_non_local.dat, plate.msh}.

//! TEST-186: upstream's `test_material_damage_iterative_non_local_serial`
//! ported by intent with its own acceptance criteria, the non-local law
//! driven by the solid model's non-local manager.
//! The package never compiled at the reference commit: the
//! criteria are upstream's assertions and constants, checked against the
//! port (D-007); no oracle run exists.
//!
//! Upstream's program, block by block (tolerance `Math::setTolerance(1e-13)`):
//!
//! 1. static solve (`newton_raphson_modified`, `_increment` criterion —
//!    the reference's `solution` — `1e-4`, at most 2 iterations; `left` x = 0, `bottom` y = 0, `top`
//!    y = 2 on the 2 × 2 plate): `Σ_q ‖∇u_nl − e_y⊗e_y‖ ≤ 10e-13`;
//! 2. node 0 (the corner (1, 1)) moved to `u_y = 2.6`, residual assembled
//!    (averages and maximum recomputed): only the gradients of elements 34
//!    and 38 (the two triangles at that corner) change (`≥ 0.1`; the others
//!    `Σ ≤ 10e-13`);
//! 3. the averaged gradient changes (`≥ 1e-6`) exactly at the points within
//!    `R + tol` (`R = 1`, the `non_local` section) of the quadrature points
//!    of elements 34 or 38, and nowhere else (`Σ ≤ 10e-13`);
//! 4. the normalised equivalent stress of those neighbourhood points is
//!    not the uniform value `0.1` (it is computed from the averaged, not the
//!    local, gradient);
//! 5. the maximum normalised equivalent stress is `0.1311267235941873`
//!    (`Math::are_float_equal`, tolerance `1e-13`).
//!
//! Block 4 is checked as intended: upstream's loop increments its point
//! counter only for the points it checks and `continue`s at counter 34, so
//! from the 35th point on it skips every point (the counter stays at 34;
//! UD-1006): its literal assertion covers points 0…33 only, which the
//! intended check over all points (except 34 and 38) contains.

mod common;

use akantu_core::enums::{AnalysisMethod, NonLinearSolverType, SpatialDirection};
use akantu_core::math::are_float_equal;
use akantu_core::{Array, ElementType, GhostType, Real};
use akantu_extra_materials::non_local::MaterialDamageIterativeNonLocal;
use akantu_extra_materials::{
    ExtraLawOptions, as_iterative_damage, register_extra_materials_with,
    register_extra_non_local_materials_with,
};
use akantu_mesh::SharedMesh;
use akantu_model::ModelOptions;
use akantu_model::boundary_condition::FixedValue;
use akantu_parser::Parser;
use akantu_solid::{Material as _, SolidMechanicsModel};
use common::{data, read_mesh, set_newton};

const T3: ElementType = ElementType::Triangle3;
const NG: GhostType = GhostType::NotGhost;

/// `‖A − e_y⊗e_y‖` (Frobenius, upstream `Matrix::norm<L_2>`).
fn diff_norm(g: &[Real; 4]) -> Real {
    let d = [g[0], g[1], g[2], g[3] - 1.];
    d.iter().map(|x| x * x).sum::<Real>().sqrt()
}

fn law(m: &SolidMechanicsModel) -> &MaterialDamageIterativeNonLocal<2> {
    m.material(0)
        .unwrap()
        .as_any()
        .unwrap()
        .downcast_ref::<MaterialDamageIterativeNonLocal<2>>()
        .unwrap()
}

/// Upstream `test_material_damage_iterative_non_local_serial` (module
/// docs).
#[test]
fn test_material_damage_iterative_non_local_serial() {
    // Math::setTolerance(1e-13)
    let tolerance = 1e-13;
    let dat = std::fs::read_to_string(data("test_material_damage/material_non_local.dat")).unwrap();
    let mesh = read_mesh(&data("test_material_damage/plate.msh"), 2);
    let parser = Parser::parse_str(&dat, "material_non_local.dat").unwrap();
    let mut model = SolidMechanicsModel::with_parser(
        SharedMesh::new(mesh),
        None,
        "solid_mechanics_model",
        parser,
    )
    .unwrap();
    register_extra_materials_with(&mut model.physics.factory, ExtraLawOptions::default()).unwrap();
    register_extra_non_local_materials_with(&mut model.physics.factory, ExtraLawOptions::default())
        .unwrap();
    // initFull(_static); solveStep<_scm_newton_raphson_tangent_modified,
    // _increment>(1e-4, error, 2): the removed criterion name `_increment` is
    // the reference's `_solution` (norm of the increment)
    let mut options = ModelOptions::new(AnalysisMethod::Static);
    options.solver_options.non_linear_solver_type =
        Some(NonLinearSolverType::NewtonRaphsonModified);
    model.init_full(&options).unwrap();
    set_newton(&mut model, 2, 1e-4, "solution");
    for (value, axis, group) in [
        (0., SpatialDirection::X, "left"),
        (0., SpatialDirection::Y, "bottom"),
        (2., SpatialDirection::Y, "top"),
    ] {
        model
            .apply_dirichlet_bc(&FixedValue { value, axis }, group)
            .unwrap();
    }
    assert_eq!(
        law(&model).local.base().law_type,
        "damage_iterative_non_local"
    );
    assert!(model.non_local_manager().is_some(), "driven by the manager");

    model.solve_step(None).expect("Did not converge");

    // ---- block 1: the averaged gradient of the uniform field
    let grad_u_nl = law(&model).grad_u_nl.get(T3, NG).unwrap().clone();
    let diff: Real = grad_u_nl
        .as_slice()
        .as_chunks::<4>()
        .0
        .iter()
        .map(diff_norm)
        .sum();
    eprintln!("block 1: Σ ‖∇u_nl − e_y⊗e_y‖ = {diff:e}");
    assert!(
        diff <= 10.0e-13,
        "Error in the non-local grad_u computation: {diff}"
    );

    // ---- block 2: change the displacement of node 0 to modify grad_u
    model.displacement_mut().unwrap().as_mut_slice()[1] = 2.6;
    // compute stresses: this averages grad_u and computes the max. eq. stress
    model.assemble_residual().unwrap();

    let grad_u = law(&model).local.base().grad_u.get(T3, NG).unwrap().clone();
    let mut diff = 0.;
    for (counter, g) in grad_u.as_slice().as_chunks::<4>().0.iter().enumerate() {
        let d = diff_norm(g);
        if counter == 34 || counter == 38 {
            assert!(
                d >= 0.1,
                "Error in the grad_u computation: element {counter}: {d}"
            );
        } else {
            diff += d;
        }
    }
    eprintln!("block 2: Σ_others ‖∇u − e_y⊗e_y‖ = {diff:e}");
    assert!(diff <= 10.0e-13, "Error in the grad_u computation: {diff}");

    // ---- block 3: the averaged gradient changes in the neighbourhoods of
    // the quadrature points of elements 34 and 38 only
    let nl_radius = 1.0; // same value as in the material file
    let grad_u_nl = law(&model).grad_u_nl.get(T3, NG).unwrap().clone();
    let coords = {
        let mesh = model.physics.base.mesh.read().unwrap();
        let fe = model.physics.base.fems.get("").unwrap();
        let mut c = Array::new(2);
        fe.integration_points_coordinates(&mesh, &mut c, T3, NG, None)
            .unwrap();
        assert_eq!(fe.nb_integration_points(T3, NG).unwrap(), 1);
        c
    };
    let nb_quads = coords.len();
    assert_eq!(grad_u_nl.len(), nb_quads);
    let dist = |a: &[Real], b: &[Real]| ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)).sqrt();
    let (q1, q2) = (coords.tuple(34).to_vec(), coords.tuple(38).to_vec());
    let in_neighbourhood = |q: usize| {
        dist(&q1, coords.tuple(q)) <= nl_radius + tolerance
            || dist(&q2, coords.tuple(q)) <= nl_radius + tolerance
    };
    let mut diff = 0.;
    let mut nb_neighbours = 0;
    let mut min_changed = Real::INFINITY;
    for (q, g) in grad_u_nl.as_slice().as_chunks::<4>().0.iter().enumerate() {
        let d = diff_norm(g);
        if in_neighbourhood(q) {
            nb_neighbours += 1;
            min_changed = min_changed.min(d);
            assert!(
                d >= 1.0e-6,
                "Error in the non-local grad_u computation: point {q}: {d}"
            );
        } else {
            diff += d;
        }
    }
    eprintln!(
        "block 3: {nb_neighbours} of {nb_quads} points changed (min {min_changed:e}), \
         Σ_others = {diff:e}"
    );
    assert!(
        diff <= 10.0e-13,
        "Error in the non-local grad_u computation: {diff}"
    );
    assert!(
        nb_neighbours > 2 && nb_neighbours < nb_quads,
        "a partial neighbourhood"
    );

    // ---- block 4: the normalised equivalent stress of the neighbourhood
    // points is based on the non-local grad_u (upstream's message: "the
    // normalized equivalent stress is most likely based on the local, not
    // the non-local grad_u")
    let eq_stress = law(&model)
        .local
        .equivalent_stress
        .get(T3, NG)
        .unwrap()
        .clone();
    let mut checked = 0;
    for (q, &eq) in eq_stress.as_slice().iter().enumerate() {
        if q == 34 || q == 38 || !in_neighbourhood(q) {
            continue;
        }
        checked += 1;
        assert!(
            !are_float_equal(eq, 0.1, tolerance),
            "point {q}: the normalized equivalent stress is most likely based on the local, \
             not the non-local grad_u!!!!"
        );
    }
    assert!(checked > 0);
    // upstream's literal loop (UD-1006) checks the points 0…33 only
    let literal = (0..34).filter(|&q| in_neighbourhood(q)).count();
    eprintln!("block 4: {checked} points checked (upstream's literal loop: {literal})");

    // ---- block 5: the maximum normalised equivalent stress
    let max_eq_stress = as_iterative_damage(model.material(0).unwrap())
        .unwrap()
        .norm_max_equivalent_stress();
    let reference: Real = 0.1311267235941873;
    eprintln!(
        "block 5: max_eq_stress = {max_eq_stress:?} (upstream {reference:?}, {} ulp)",
        max_eq_stress.to_bits().abs_diff(reference.to_bits())
    );
    assert!(
        are_float_equal(max_eq_stress, reference, tolerance),
        "the maximum equivalent stress is wrong: {max_eq_stress:.17}"
    );
    // Upstream's criterion (1e-13, about 3 600 ulps at 0.131) is
    // kept above; this pins the documented agreement: the port's double,
    // printed with upstream's 16 significant digits, is upstream's constant.
    assert_eq!(
        format!("{max_eq_stress:.15e}"),
        "1.311267235941873e-1",
        "the maximum equivalent stress no longer agrees with upstream's constant \
         to its 16 printed digits: {max_eq_stress:.17}"
    );
}
