// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Ports (same intent,
// upstream tolerances and step counts) of
// test/test_model/test_contact_mechanics_model/test_explicit_dynamic.cc and
// test_explicit_friction.cc. The solid–contact coupler is in akantu-couplers; these
// tests couple a `SolidMechanicsModel` and a `ContactMechanicsModel` through
// an intercepting solver callback that reproduces the explicit branch of
// upstream `CouplerSolidContactTemplate::assembleResidual`
// (`coupler_solid_contact_tmpl.hh:66-90`): copy the solid's current
// positions into the detector, `search()`, then
// `r = f_ext + f_int + f_contact`; predictor, corrector, mass and hooks are
// the solid's (the contact hooks are empty for penalty resolutions).

#![allow(clippy::needless_range_loop)] // step/entry indices mirror the upstream test loops

mod common;

use akantu_contact::{ContactMechanicsModel, PhysicalSurfaceSelector};
use akantu_core::enums::{AnalysisMethod, SpatialDirection};
use akantu_core::{Array, ElementType, GhostType, Real, Result};
use akantu_mesh::SharedMesh;
use akantu_model::ModelOptions;
use akantu_model::boundary_condition::FixedValue;
use akantu_model::constitutive_law::ElementDataSelector;
use akantu_parser::Parser;
use akantu_solid::SolidMechanicsModel;
use akantu_solver::callback::{Delegate, InterceptCallback};
use common::*;

/// The explicit (lumped) branch of the upstream solid–contact coupler.
struct ExplicitContactCoupling<'a> {
    contact: &'a mut ContactMechanicsModel,
    nodes: Array<Real>,
}

impl InterceptCallback for ExplicitContactCoupling<'_> {
    fn assemble_residual(&mut self, model: &mut Delegate<'_>) -> Result<()> {
        // current_positions.copy(solid->getCurrentPosition()); search()
        {
            let u = model.dofs().dofs("displacement")?;
            let p = self.contact.positions_mut();
            for ((pv, x), uv) in p
                .as_mut_slice()
                .iter_mut()
                .zip(self.nodes.as_slice())
                .zip(u.as_slice())
            {
                *pv = x + uv;
            }
        }
        self.contact.search()?;
        // solid internal forces, r += f_ext, r += f_int
        model.assemble_residual()?;
        // contact internal forces, r += f_contact
        self.contact.assemble_internal_forces()?;
        model
            .dofs()
            .assemble_to_residual("displacement", self.contact.internal_force().view(), 1.)
    }
}

/// `arrange(start, stop, step)` of the upstream tests (inclusive stop,
/// accumulated `value += step`).
fn arrange(start: Real, stop: Real, step: Real) -> Vec<Real> {
    let mut v = Vec::new();
    let mut x = start;
    while x <= stop {
        v.push(x);
        x += step;
    }
    v
}

/// Smooth ramp `d_max p³ (10 − 15 p + 6 p²)` (upstream lambda).
fn ramp(d_max: Real, p: Real) -> Real {
    0. + d_max * p.powf(3.0) * (10. - 15. * p + 6. * p.powf(2.0))
}

fn coupled(
    mesh_file: &str,
    dat: &str,
) -> (
    SharedMesh,
    SolidMechanicsModel,
    ContactMechanicsModel,
    Array<Real>,
) {
    let mesh = read_mesh(&contact_mesh_path(mesh_file), 2);
    let nodes = mesh.nodes().clone();
    let parser = Parser::parse_str(dat, "material.dat").unwrap();
    let selector = PhysicalSurfaceSelector::new(&mesh, &parser).unwrap();
    let shared = SharedMesh::new(mesh);
    let mut solid = SolidMechanicsModel::with_parser(
        shared.clone(),
        None,
        "solid_mechanics_model",
        parser.clone(),
    )
    .unwrap();
    solid.set_material_selector(Box::new(ElementDataSelector::new("physical_names")));
    solid
        .init_full(&ModelOptions::new(AnalysisMethod::ExplicitLumpedMass))
        .unwrap();
    let mut contact =
        ContactMechanicsModel::with_parser(shared.clone(), None, "contact_mechanics_model", parser)
            .unwrap();
    contact
        .init_full(AnalysisMethod::ExplicitLumpedMass)
        .unwrap();
    contact.set_surface_selector(Box::new(selector)).unwrap();
    (shared, solid, contact, nodes)
}

fn damp_velocities_in_contact(
    solid: &mut SolidMechanicsModel,
    contact: &ContactMechanicsModel,
    ratio: Real,
) {
    let gaps = contact.gaps().as_slice().to_vec();
    let mut v = solid.velocity_mut().unwrap();
    for (g, vel) in gaps.iter().zip(v.as_mut_slice().as_chunks_mut::<2>().0) {
        if *g > 0.0 {
            for c in vel {
                *c *= ratio;
            }
        }
    }
}

const MATERIAL_DAT: &str = include_str!("data/material.dat");
const MATERIAL_FRICTION_DAT: &str = include_str!("data/material-friction.dat");

/* ---------------------------------------------------------------------- */
/* test_explicit_dynamic.cc (TEST-085)                                     */
/* ---------------------------------------------------------------------- */

#[test]
fn test_explicit_dynamic() {
    let max_steps = 2000usize;
    let max_displacement = 1e-2;
    let damping_ratio = 0.99;
    let (_shared, mut solid, mut contact, nodes) = coupled("flat_on_flat.msh", MATERIAL_DAT);
    let fix_x = FixedValue {
        value: 0.0,
        axis: SpatialDirection::X,
    };
    solid.apply_dirichlet_bc(&fix_x, "upper").unwrap();
    solid.apply_dirichlet_bc(&fix_x, "lower").unwrap();
    let time_step = solid.stable_time_step().unwrap() * 0.05;
    solid.set_time_step(time_step, None).unwrap();
    let xi = arrange(0., 1., 1. / max_steps as Real);
    let displacements: Vec<Real> = xi.iter().map(|&p| ramp(max_displacement, p)).collect();
    for s in 0..max_steps {
        let load = FixedValue {
            value: -displacements[s],
            axis: SpatialDirection::Y,
        };
        let fixed = FixedValue {
            value: displacements[s],
            axis: SpatialDirection::Y,
        };
        solid.apply_dirichlet_bc(&load, "loading").unwrap();
        solid.apply_dirichlet_bc(&fixed, "fixed").unwrap();
        let mut cb = ExplicitContactCoupling {
            contact: &mut contact,
            nodes: nodes.clone(),
        };
        solid.solve_step_intercepted(&mut cb, None).unwrap();
        damp_velocities_in_contact(&mut solid, &contact, damping_ratio);
    }
    assert!(!contact.contact_elements().is_empty());
    // σ_yy of material "upper" (both bodies: "lower" is not a material name
    // and falls back to the first material, as upstream) ≈ 7e5 within 1 %
    let mat = solid.material_by_name("upper").unwrap();
    let stress = mat
        .base()
        .stress
        .get(ElementType::Quadrangle4, GhostType::NotGhost)
        .unwrap();
    let presc = 7e5;
    for st in stress.as_slice().as_chunks::<4>().0 {
        let err = (st[3].abs() - presc) / presc;
        assert!(err.abs() <= 1e-2, "stress error {err}: σ = {st:?}");
    }
}

/* ---------------------------------------------------------------------- */
/* test_explicit_friction.cc (TEST-086)                                    */
/* ---------------------------------------------------------------------- */

#[test]
fn test_explicit_friction() {
    let max_normal_steps = 2500usize;
    let max_shear_steps = 7500usize;
    let max_shear_displacement = 1e-1;
    let max_normal_displacement = 2e-2;
    let damping_ratio = 0.99;
    let (shared, mut solid, mut contact, nodes) =
        coupled("sliding-block-2D.msh", MATERIAL_FRICTION_DAT);
    solid
        .apply_dirichlet_bc(
            &FixedValue {
                value: 0.0,
                axis: SpatialDirection::X,
            },
            "lower",
        )
        .unwrap();
    solid
        .apply_dirichlet_bc(
            &FixedValue {
                value: 0.0,
                axis: SpatialDirection::Y,
            },
            "lower",
        )
        .unwrap();
    let time_step = solid.stable_time_step().unwrap() * 0.05;
    solid.set_time_step(time_step, None).unwrap();
    let shear: Vec<Real> = arrange(0., 1., 1. / max_shear_steps as Real)
        .iter()
        .map(|&p| ramp(max_shear_displacement, p))
        .collect();
    let normal: Vec<Real> = arrange(0., 1., 1. / max_normal_steps as Real)
        .iter()
        .map(|&p| ramp(max_normal_displacement, p))
        .collect();
    let nb_contact_nodes = nodes_of_physical(&shared.read().unwrap(), "contact_top").len();
    let exp_tang_traction = 0.3 * 1.4e6;
    let mut max_traction: Real = 0.0;
    let mut slipped = false;
    for s in 0..max_normal_steps + max_shear_steps {
        if s < max_normal_steps {
            let bc = FixedValue {
                value: -normal[s],
                axis: SpatialDirection::Y,
            };
            solid.apply_dirichlet_bc(&bc, "loading").unwrap();
        } else {
            let bc = FixedValue {
                value: shear[s - max_normal_steps],
                axis: SpatialDirection::X,
            };
            solid.apply_dirichlet_bc(&bc, "loading").unwrap();
        }
        let mut cb = ExplicitContactCoupling {
            contact: &mut contact,
            nodes: nodes.clone(),
        };
        solid.solve_step_intercepted(&mut cb, None).unwrap();
        damp_velocities_in_contact(&mut solid, &contact, damping_ratio);
        let sum: Real = contact.tangential_tractions().as_slice().iter().sum();
        let num_tang_traction = sum.abs() / nb_contact_nodes as Real;
        let error = (num_tang_traction - exp_tang_traction).abs() / exp_tang_traction;
        assert!(
            !(error > 1e-3 && num_tang_traction > exp_tang_traction),
            "step {s}: {error}----{num_tang_traction}"
        );
        max_traction = max_traction.max(num_tang_traction);
        slipped |= contact
            .contact_state()
            .as_slice()
            .contains(&akantu_contact::ContactState::Slip);
    }
    // independent: friction was mobilised up to sliding
    assert!(slipped, "no slip state reached");
    assert!(
        max_traction > 0.5 * exp_tang_traction,
        "max mean traction {max_traction}"
    );
}
