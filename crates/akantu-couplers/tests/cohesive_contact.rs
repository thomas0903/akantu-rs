// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). The solid–cohesive–
// contact coupler (CPL-003, coupler_solid_cohesive_contact.cc) and the
// stand-alone CohesiveContactSolverCallback (CPL-004,
// cohesive_contact_solvercallback.cc) on the setup of
// examples/c++/contact_mechanics_model/hertz/cohesive_contact_explicit_dynamic.cc.

mod common;

use akantu_cohesive::selector::MeshDataMaterialCohesiveSelector;
use akantu_contact::{CohesiveSurfaceSelector, ContactUpstreamCompat};
use akantu_core::Real;
use akantu_core::enums::{AnalysisMethod, SpatialDirection};
use akantu_couplers::CouplerSolidCohesiveContact;
use akantu_mesh::SharedMesh;
use akantu_model::ModelOptions;
use akantu_model::boundary_condition::{FixedValue, IncrementValue};
use akantu_parser::Parser;
use common::*;

pub const COHESIVE_DAT: &str = include_str!("data/contact/material-cohesive.dat");

/// The example's input with the contact penalty `epsilon_n` (the example
/// uses 0, which disables the contact forces).
pub fn cohesive_dat(epsilon_n: Real) -> String {
    COHESIVE_DAT.replace("epsilon_n = 0", &format!("epsilon_n = {epsilon_n:e}"))
}

pub fn cohesive_coupler(dat: &str, compat: ContactUpstreamCompat) -> CouplerSolidCohesiveContact {
    let mesh = read_mesh(
        &frozen_mesh("examples/c++/contact_mechanics_model/hertz/cohesive-contact.msh"),
        2,
    );
    let parser = Parser::parse_str(dat, "material-cohesive.dat").unwrap();
    let mut c = CouplerSolidCohesiveContact::with_parser(
        SharedMesh::new(mesh),
        None,
        "coupler_solid_contact",
        parser,
    )
    .unwrap();
    let sel = MeshDataMaterialCohesiveSelector {
        default: c.solid.default_cohesive_selector(),
    };
    c.solid.set_material_selector(Box::new(sel));
    c.contact
        .set_surface_selector(Box::new(CohesiveSurfaceSelector::new()))
        .unwrap();
    c.init_full(&ModelOptions::new(AnalysisMethod::ExplicitLumpedMass), true)
        .unwrap();
    c.contact.set_upstream_compat(compat);
    let fix = FixedValue {
        value: 0.0,
        axis: SpatialDirection::X,
    };
    c.apply_dirichlet_bc(&fix, "sides").unwrap();
    let dt = c.stable_time_step().unwrap() * 0.1;
    c.set_time_step(dt, None).unwrap();
    c
}

/// One step of the example's loop: `±increment` on `loading` / `fixed`,
/// solve, insertion check, damping of the velocities in contact.
pub fn cohesive_step(c: &mut CouplerSolidCohesiveContact, increment: Real) -> usize {
    let up = IncrementValue {
        value: increment,
        axis: SpatialDirection::Y,
    };
    let down = IncrementValue {
        value: -increment,
        axis: SpatialDirection::Y,
    };
    c.apply_dirichlet_bc(&up, "loading").unwrap();
    c.apply_dirichlet_bc(&down, "fixed").unwrap();
    c.solve_step(None).unwrap();
    let n = c.check_cohesive_stress().unwrap();
    let gaps = c.contact.gaps().as_slice().to_vec();
    let mut v = c.solid.smm.velocity_mut().unwrap();
    for (g, vel) in gaps.iter().zip(v.as_mut_slice().as_chunks_mut::<2>().0) {
        if *g > 0.0 {
            for x in vel {
                *x *= 0.99;
            }
        }
    }
    drop(v);
    n
}

/// Largest penetration (positive gap) over the steps of the unloading phase
/// and the number of steps with contact elements.
fn run(c: &mut CouplerSolidCohesiveContact, steps: usize, inc: Real) -> (Real, usize, usize) {
    let mut max_pen: Real = 0.0;
    let mut with_contact = 0;
    let mut inserted = 0;
    for s in 0..3 * steps {
        let i = if s < steps { inc } else { -inc };
        inserted += cohesive_step(c, i);
        if !c.contact.contact_elements().is_empty() {
            with_contact += 1;
        }
        if s >= steps {
            for &g in c.contact.gaps().as_slice() {
                max_pen = max_pen.max(g);
            }
        }
    }
    (max_pen, with_contact, inserted)
}

/// CPL-003 with the corrected contact defaults: cohesive elements are
/// inserted under tension (the contact surface grows from 0 to the facets
/// of the new elements), and once the crack closes the contact penalty
/// acts: with `epsilon_n = 1e3` the largest penetration of the crack faces
/// is less than half of the one without contact forces (`epsilon_n = 0`,
/// the example's input), and contact elements exist only after insertion.
///
/// Runs on the default test thread again: with the
/// cohesive model's staged insertion flags boxed, both tests of this file
/// fit in 2 MiB in a debug build (arm64, measured: plain debug overflows at
/// 1.75 MiB and fits at 1.875 MiB; with `akantu-core/export-all` overflows
/// at 1.875 MiB and fits at 2 MiB; the x86_64 gates `check` and
/// `exportall` run them as well). With the facet FE engine boxed as well
/// every threshold is 256 KiB lower:
/// plain debug overflows at 1.5 MiB and fits at 1.625 MiB, export-all
/// overflows at 1.625 MiB and fits at 1.75 MiB (`RUST_MIN_STACK`), so the
/// default 2 MiB thread has at least 256 KiB of headroom. An overflow here
/// or in `constructor_stack.rs` points at the size of the models, not at
/// the change under test.
#[test]
fn cohesive_contact_penalty_limits_crack_face_penetration() {
    let steps = 2500;
    let inc = 1e-3 / Real::from(2500u32);
    let mut with = cohesive_coupler(&cohesive_dat(1e3), ContactUpstreamCompat::default());
    assert_eq!(with.contact.contact_detector().positions().len(), 15);
    let (pen_with, contact_steps, inserted) = run(&mut with, steps, inc);
    let mut without = cohesive_coupler(COHESIVE_DAT, ContactUpstreamCompat::default());
    let (pen_without, _, inserted0) = run(&mut without, steps, inc);
    eprintln!(
        "penetration with contact {pen_with:e}, without {pen_without:e}; {contact_steps} steps \
         with contact; {inserted} / {inserted0} inserted"
    );
    assert!(inserted > 0 && inserted0 > 0);
    assert!(contact_steps > 1000, "{contact_steps}");
    assert!(pen_without > 0.0);
    // qualitative check (soft penalty, epsilon_n = E): measured ratio 0.40
    assert!(
        pen_with < 0.5 * pen_without,
        "{pen_with:e} vs {pen_without:e}"
    );
}

/// CPL-004: `CohesiveContactSolverCallback` gives the coupler's trajectory
/// for an explicit solve (the contact's solve-step hooks it does not call
/// are empty for penalty resolutions; every matrix being symmetric and the
/// missing split residual do not matter to the lumped solver).
#[test]
fn cohesive_contact_callback_matches_coupler() {
    let dat = cohesive_dat(1e3);
    let mut a = cohesive_coupler(&dat, ContactUpstreamCompat::default());
    let mut b = cohesive_coupler(&dat, ContactUpstreamCompat::default());
    let inc = 1e-3 / Real::from(2500u32);
    for s in 0..3000 {
        let i = if s < 2500 { inc } else { -inc };
        cohesive_step(&mut a, i);
        // the same step through the stand-alone callback
        let up = IncrementValue {
            value: i,
            axis: SpatialDirection::Y,
        };
        let down = IncrementValue {
            value: -i,
            axis: SpatialDirection::Y,
        };
        b.apply_dirichlet_bc(&up, "loading").unwrap();
        b.apply_dirichlet_bc(&down, "fixed").unwrap();
        b.forward_new_cohesive_elements().unwrap();
        let contact = &mut b.contact;
        b.solid
            .smm
            .solve_step_with(None, |physics, solver, id| {
                let mut cohesive = akantu_cohesive::model::CohesiveCallback::new(physics);
                let mut cb = akantu_couplers::CohesiveContactSolverCallback::new(
                    &mut cohesive,
                    contact,
                    AnalysisMethod::ExplicitLumpedMass,
                );
                solver.solve_step(cb.as_callback(), id)
            })
            .unwrap();
        b.check_cohesive_stress().unwrap();
        let gaps = b.contact.gaps().as_slice().to_vec();
        let mut v = b.solid.smm.velocity_mut().unwrap();
        for (g, vel) in gaps.iter().zip(v.as_mut_slice().as_chunks_mut::<2>().0) {
            if *g > 0.0 {
                for x in vel {
                    *x *= 0.99;
                }
            }
        }
        drop(v);
        assert_bits(
            "callback",
            &format!("step {s} displacement"),
            b.solid.smm.displacement().unwrap().as_slice(),
            a.solid.smm.displacement().unwrap().as_slice(),
        );
    }
}
