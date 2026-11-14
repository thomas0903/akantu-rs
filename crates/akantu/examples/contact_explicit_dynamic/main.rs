// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): examples/c++/contact_mechanics_model/hertz/
// contact_explicit_dynamic.cc (+ material.dat, hertz.geo).

//! `contact_explicit_dynamic` (EX-001): Hertzian contact of an elastic
//! half-disc pressed on a block (`CouplerSolidContact`, frictionless
//! `penalty_linear`, explicit lumped mass, `Δt = 0.1 Δt_crit`): the
//! `loading` edge moves down by `5e-3 / 20000` per step for 20 000 steps,
//! the velocities of the nodes in contact (positive gap) are damped by 0.99
//! after every step. Prints the step, the imposed displacement and the
//! potential, kinetic and total energies to the standard error every 1000
//! steps and dumps displacement, velocity, normals, contact, external and
//! internal forces, gaps, areas, blocked DOFs, `grad_u` and stress
//! (`paraview/contact-explicit-dynamic_<n>.pvtu`, the coupler's dumper,
//! `akantu::couplers::dump`).
//!
//! `--upstream-compat` selects every upstream defect of the contact model
//! (`ContactUpstreamCompat::all()`, e.g. UD-002: node-indexed stiffness,
//! assigned forces) and of the coupling (`SolidContactCompat::upstream()`:
//! UD-374, `per_iteration_friction_history`; UD-1291,
//! `search_in_corrector_only`, which acts on static / implicit couplings
//! only). In this set-up (frictionless contact, rigid master) the switches
//! are inert: **the default run reproduces upstream** bitwise (the oracle
//! comparison is tested in both modes).
//!
//! ```text
//! cargo run -p akantu --release --example contact_explicit_dynamic [-- --upstream-compat]
//! ```

#[path = "../common/mod.rs"]
pub mod common;

use std::process::ExitCode;

use akantu::contact::{ContactUpstreamCompat, PhysicalSurfaceSelector};
use akantu::core::enums::{AnalysisMethod, SpatialDirection};
use akantu::core::{Real, Result};
use akantu::couplers::{CouplerSolidContact, SolidContactCompat};
use akantu::mesh::SharedMesh;
use akantu::model::Model;
use akantu::model::boundary_condition::{FixedValue, IncrementValue};
use akantu::model::constitutive_law::ElementDataSelector;
use common::{Example, cout};

/// Spatial dimension of the example.
pub const SPATIAL_DIMENSION: usize = 2;

/// Result of a run.
#[derive(Debug)]
pub struct Outcome {
    /// The coupler in its final state.
    pub coupler: CouplerSolidContact,
    /// Time step used (`0.1 Δt_crit`).
    pub time_step: Real,
    /// Number of contact elements after every step.
    pub nb_contact: Vec<usize>,
    /// `[epot, ekin]` after every step.
    pub energies: Vec<[Real; 2]>,
}

/// Default configuration (upstream directory and input file).
#[must_use]
pub fn example() -> Example {
    Example::new(
        "contact_explicit_dynamic",
        "contact_mechanics_model/hertz",
        "material.dat",
    )
}

/// Upstream's damping of the velocities of the nodes in contact (`gap >
/// 0`), `vel *= damping_ratio` (akantu's `zip` stops with the gaps).
///
pub fn damp_contact_velocities(
    gaps: &[Real],
    velocity: &mut [Real],
    spatial_dimension: usize,
    damping_ratio: Real,
) {
    for (gap, vel) in gaps
        .iter()
        .zip(velocity.chunks_exact_mut(spatial_dimension))
    {
        if *gap > 0. {
            for v in vel {
                *v *= damping_ratio;
            }
        }
    }
}

/// The upstream `main` after `initialize`.
///
/// # Errors
/// Any error of the models, the mesh, the solver or the outputs.
pub fn run(ex: &Example) -> Result<Outcome> {
    run_with(ex, |_, _| Ok(()))
}

/// [`run`] calling `on_step(i, &coupler)` after every step `i` (after the
/// damping, before the outputs).
///
/// # Errors
/// Any error of the models, the mesh, the solver, the outputs or
/// `on_step`.
pub fn run_with(
    ex: &Example,
    mut on_step: impl FnMut(usize, &CouplerSolidContact) -> Result<()>,
) -> Result<Outcome> {
    ex.create_output_dir()?;
    let spatial_dimension = SPATIAL_DIMENSION;

    let time_factor = 0.1;
    let upstream_max_steps: u32 = 20000;
    let max_steps = ex.steps_or(upstream_max_steps as usize);
    let max_displacement = 5e-3;

    let mesh = ex.read_mesh(spatial_dimension, "hertz.msh")?;
    let parser = ex.parser()?;

    let mut coupler = CouplerSolidContact::with_parser(
        SharedMesh::new(mesh),
        None,
        "coupler_solid_contact",
        parser.clone(),
    )?;

    coupler
        .solid
        .set_material_selector(Box::new(ElementDataSelector::new("physical_names")));

    coupler.init_full(&ex.model_options(AnalysisMethod::ExplicitLumpedMass))?;
    if ex.upstream_compat {
        coupler
            .contact
            .set_upstream_compat(ContactUpstreamCompat::all());
        coupler.set_upstream_compat(SolidContactCompat::upstream())?;
    }

    let surface_selector = {
        let mesh = coupler.solid.mesh().read()?;
        PhysicalSurfaceSelector::new(&mesh, &parser)?
    };
    coupler
        .contact
        .set_surface_selector(Box::new(surface_selector))?;

    let fixed = |axis| FixedValue { value: 0.0, axis };
    coupler
        .solid
        .apply_dirichlet_bc(&fixed(SpatialDirection::X), "fixed")?;
    coupler
        .solid
        .apply_dirichlet_bc(&fixed(SpatialDirection::Y), "fixed")?;
    coupler
        .solid
        .apply_dirichlet_bc(&fixed(SpatialDirection::X), "loading")?;
    coupler
        .solid
        .apply_dirichlet_bc(&fixed(SpatialDirection::X), "symmetry")?;

    let mut time_step = coupler.solid.stable_time_step()?;
    time_step *= time_factor;
    ex.say(format!(
        "Time Step = {}s ({}s)",
        cout(time_step),
        cout(time_step)
    ));
    coupler.set_time_step(time_step, None)?;

    coupler.set_directory(&ex.paraview_dir())?;
    coupler.set_base_name("contact-explicit-dynamic")?;
    coupler.add_dump_field_vector("displacement")?;
    coupler.add_dump_field_vector("velocity")?;
    coupler.add_dump_field_vector("normals")?;
    coupler.add_dump_field_vector("contact_force")?;
    coupler.add_dump_field_vector("external_force")?;
    coupler.add_dump_field_vector("internal_force")?;
    coupler.add_dump_field("gaps")?;
    coupler.add_dump_field("areas")?;
    coupler.add_dump_field("blocked_dofs")?;
    coupler.add_dump_field("grad_u")?;
    coupler.add_dump_field("stress")?;

    let damping_ratio = 0.99;
    // upstream `max_displacement / max_steps` (the upstream count, also in
    // reduced runs)
    let increment = max_displacement / Real::from(upstream_max_steps);

    let mut nb_contact = Vec::with_capacity(max_steps);
    let mut energies = Vec::with_capacity(max_steps);
    for i in 0..max_steps {
        coupler.solid.apply_dirichlet_bc(
            &IncrementValue {
                value: -increment,
                axis: SpatialDirection::Y,
            },
            "loading",
        )?;

        coupler.solve_step(None)?;

        // damping velocities only along the contacting zone
        {
            let gaps = coupler.contact.gaps().as_slice().to_vec();
            let mut velocity = coupler.solid.velocity_mut()?;
            damp_contact_velocities(
                &gaps,
                velocity.as_mut_slice(),
                spatial_dimension,
                damping_ratio,
            );
        }

        let epot = coupler.solid.energy("potential")?;
        let ekin = coupler.solid.energy("kinetic")?;
        nb_contact.push(coupler.contact.contact_elements().len());
        energies.push([epot, ekin]);
        on_step(i, &coupler)?;

        // dumping energies
        if i.is_multiple_of(1000) {
            #[allow(clippy::cast_precision_loss)] // upstream `i * increment`
            let imposed = i as Real * increment;
            ex.say_err(format!(
                "{i},{},{},{},{},\n",
                cout(imposed),
                cout(epot),
                cout(ekin),
                cout(epot + ekin)
            ));
        }

        if i.is_multiple_of(1000) && ex.dump {
            coupler.dump(None, None)?;
        }
    }

    Ok(Outcome {
        coupler,
        time_step,
        nb_contact,
        energies,
    })
}

fn main() -> ExitCode {
    common::main_with(|args| run(&example().with_args(args)?).map(|_| ()))
}
