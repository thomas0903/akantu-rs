// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): examples/c++/contact_mechanics_model/
// friction/bloc_friction.cc (+ material.dat, bloc.geo).

//! `bloc_friction` (EX-004): an elastic block on a rigid foundation with
//! Coulomb friction (`CouplerSolidContact`, `penalty_linear` with `mu =
//! 0.3`, explicit lumped mass, `Δt = 0.1 Δt_crit`, materials chosen by the
//! `physical_names` mesh data): 10 000 steps pressing the `loading` edge
//! down by `1/10000` per step, 10 000 steps sliding it by `2/10000` per
//! step, 10 000 steps without load; after every step the velocities of the
//! nodes in contact (positive gap) are damped by 0.99. Dumps displacement,
//! velocity, normals, tangents, contact, external and internal forces,
//! areas, stress and blocked DOFs initially and every 100 steps of each
//! phase (`paraview/bloc_friction_<n>.pvtu`, the coupler's dumper,
//! `akantu::couplers::dump`).
//!
//! `--upstream-compat` selects every upstream defect of the contact model
//! (`ContactUpstreamCompat::all()`) and the coupler's UD-374 switch, and
//! reproduces upstream's output. **The default run departs from upstream
//! at step 1911** (the first contact is at step 1846), through one switch
//! only: **UD-365** (`trial_slip_on_previous_element`). When a slave node
//! in contact passes from one master facet to the next (first at step 1911:
//! node 67, over the foundation node at x = 6), upstream evaluates the
//! current natural projection on the **previous** facet's nodes, i.e. it
//! sees a slip of about one facet length (0.2 m) instead of the node's
//! actual 2e-6 m, and the friction jumps to the Coulomb limit in the
//! reversed direction; the corrected default measures the slip on the
//! current facet. Every other switch is inert here. The default mode is
//! tested past the first contact with independent checks (Coulomb bound,
//! return map with the node's own slip).
//!
//! ```text
//! cargo run -p akantu --release --example bloc_friction [-- --upstream-compat]
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

/// Result of a run.
#[derive(Debug)]
pub struct Outcome {
    /// The coupler in its final state.
    pub coupler: CouplerSolidContact,
    /// Time step used (`0.1 Δt_crit`).
    pub time_step: Real,
    /// After every step (the three phases numbered on): number of contact
    /// elements.
    pub nb_contact: Vec<usize>,
    /// `[epot, ekin]` after every step.
    pub energies: Vec<[Real; 2]>,
}

/// Default configuration (upstream directory and input file).
#[must_use]
pub fn example() -> Example {
    Example::new(
        "bloc_friction",
        "contact_mechanics_model/friction",
        "material.dat",
    )
}

/// The steps of each of the three phases (upstream 10 000, bounded by
/// `--steps`, which counts the steps of all three phases).
fn phase_steps(ex: &Example, phase: usize) -> usize {
    let upstream = 10000;
    ex.steps.map_or(upstream, |s| {
        s.saturating_sub(phase * upstream).min(upstream)
    })
}

/// One step of a phase: optional loading increment, solve, damping of the
/// velocities in contact, record; a progress line and a dump every 100
/// steps.
fn step(
    ex: &Example,
    out: &mut Outcome,
    s: usize,
    load: Option<IncrementValue>,
    on_step: &mut dyn FnMut(usize, &CouplerSolidContact) -> Result<()>,
) -> Result<()> {
    let c = &mut out.coupler;
    if let Some(inc) = load {
        c.solid.apply_dirichlet_bc(&inc, "loading")?;
    }

    c.solve_step(None)?;

    // damping velocities only along the contacting zone
    {
        let gaps = c.contact.gaps().as_slice().to_vec();
        let mut velocity = c.solid.velocity_mut()?;
        for (gap, vel) in gaps
            .iter()
            .zip(velocity.as_mut_slice().as_chunks_mut::<2>().0.iter_mut())
        {
            if *gap > 0. {
                for v in vel {
                    *v *= 0.99;
                }
            }
        }
    }
    out.nb_contact.push(c.contact.contact_elements().len());
    out.energies
        .push([c.solid.energy("potential")?, c.solid.energy("kinetic")?]);
    on_step(out.nb_contact.len() - 1, c)?;

    if s.is_multiple_of(100) {
        ex.print_raw(format!("Step {s}\t\r"));
        if ex.dump {
            c.dump(None, None)?;
        }
    }
    Ok(())
}

/// The upstream `main` after `initialize`.
///
/// # Errors
/// Any error of the models, the mesh, the solver or the outputs.
pub fn run(ex: &Example) -> Result<Outcome> {
    run_with(ex, |_, _| Ok(()))
}

/// The upstream-compatibility switches of a run.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Switches {
    /// Switches of the contact model.
    pub contact: ContactUpstreamCompat,
    /// Switches of the coupler.
    pub coupler: SolidContactCompat,
}

impl Switches {
    /// The switches `--upstream-compat` selects: every upstream defect of
    /// the contact model (`ContactUpstreamCompat::all()`) and the coupler's
    /// (`SolidContactCompat::upstream()`: UD-374, and UD-1291, which acts on
    /// static / implicit couplings only); by default none (corrected
    /// behaviour).
    #[must_use]
    pub fn of(ex: &Example) -> Self {
        if ex.upstream_compat {
            Self {
                contact: ContactUpstreamCompat::all(),
                coupler: SolidContactCompat::upstream(),
            }
        } else {
            Self::default()
        }
    }
}

/// [`run`] calling `on_step(k, &coupler)` after every step (`k` counts the
/// steps of the three phases; after the damping, before the outputs).
///
/// # Errors
/// Any error of the models, the mesh, the solver, the outputs or
/// `on_step`.
pub fn run_with(
    ex: &Example,
    on_step: impl FnMut(usize, &CouplerSolidContact) -> Result<()>,
) -> Result<Outcome> {
    run_switched(ex, Switches::of(ex), on_step)
}

/// [`run_with`] with the given switches instead of those of
/// `ex.upstream_compat` (the tests attribute a departure to one switch).
///
/// # Errors
/// As [`run_with`].
pub fn run_switched(
    ex: &Example,
    switches: Switches,
    mut on_step: impl FnMut(usize, &CouplerSolidContact) -> Result<()>,
) -> Result<Outcome> {
    ex.create_output_dir()?;
    // Create the mesh>
    // Read the mesh
    let mesh = ex.read_mesh(2, "bloc.msh")?; // Dimension 2
    let parser = ex.parser()?;

    // Create the model
    let mut coupler = CouplerSolidContact::with_parser(
        SharedMesh::new(mesh),
        None,
        "coupler_solid_contact",
        parser.clone(),
    )?;

    // Initialize each model
    coupler
        .solid
        .set_material_selector(Box::new(ElementDataSelector::new("physical_names")));

    // Initialize the coupler
    coupler.init_full(&ex.model_options(AnalysisMethod::ExplicitLumpedMass))?;
    if switches != Switches::default() {
        coupler.contact.set_upstream_compat(switches.contact);
        coupler.set_upstream_compat(switches.coupler)?;
    }
    let time_step = coupler.solid.stable_time_step()? * 0.1;
    coupler.set_time_step(time_step, None)?;
    ex.say(format!("Time step: {}", cout(time_step)));

    // Setup the contact
    let surface_selector = {
        let mesh = coupler.solid.mesh().read()?;
        PhysicalSurfaceSelector::new(&mesh, &parser)?
    };
    coupler
        .contact
        .set_surface_selector(Box::new(surface_selector))?;

    // Configuration of the dumper
    coupler.set_directory(&ex.paraview_dir())?;
    coupler.set_base_name("bloc_friction")?;
    coupler.add_dump_field_vector("displacement")?;
    coupler.add_dump_field_vector("velocity")?;
    coupler.add_dump_field_vector("normals")?;
    coupler.add_dump_field_vector("tangents")?;
    coupler.add_dump_field_vector("contact_force")?;
    coupler.add_dump_field_vector("external_force")?;
    coupler.add_dump_field_vector("internal_force")?;
    coupler.add_dump_field("areas")?;
    coupler.add_dump_field("stress")?;
    coupler.add_dump_field("blocked_dofs")?;

    // Add the boundary conditions
    let fixed = |axis| FixedValue { value: 0.0, axis };
    coupler
        .solid
        .apply_dirichlet_bc(&fixed(SpatialDirection::X), "XFixed")?;
    coupler
        .solid
        .apply_dirichlet_bc(&fixed(SpatialDirection::Y), "YFixed")?;
    coupler
        .solid
        .apply_dirichlet_bc(&fixed(SpatialDirection::X), "loading")?;
    coupler
        .solid
        .apply_dirichlet_bc(&fixed(SpatialDirection::Y), "loading")?;

    // Dump the initial state
    if ex.dump {
        coupler.dump(None, None)?;
    }

    let mut out = Outcome {
        coupler,
        time_step,
        nb_contact: Vec::new(),
        energies: Vec::new(),
    };
    let inc = |value, axis| IncrementValue { value, axis };

    // First loop : compression of the bloc against the wall
    for s in 0..phase_steps(ex, 0) {
        let load = inc(-1.0 / 10000., SpatialDirection::Y);
        step(ex, &mut out, s, Some(load), &mut on_step)?;
    }

    ex.say("Compression done !");

    // Second loop : sliding of the bloc against the wall
    for s in 0..phase_steps(ex, 1) {
        let load = inc(2.0 / 10000., SpatialDirection::X);
        step(ex, &mut out, s, Some(load), &mut on_step)?;
    }

    ex.say("Sliding done !");

    // Third loop : stabilization of the system (no external force)
    for s in 0..phase_steps(ex, 2) {
        step(ex, &mut out, s, None, &mut on_step)?;
    }

    Ok(out)
}

fn main() -> ExitCode {
    common::main_with(|args| run(&example().with_args(args)?).map(|_| ()))
}
