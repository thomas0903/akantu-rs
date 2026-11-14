// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): examples/c++/contact_mechanics_model/hertz/
// cohesive_contact_explicit_dynamic.cc (+ material-cohesive.dat,
// cohesive-contact.geo).

//! `cohesive_contact_explicit_dynamic` (EX-003): an elastic bar with an
//! extrinsic cohesive interface and contact on the crack faces
//! (`CouplerSolidCohesiveContact`, explicit lumped mass, `Δt = 0.1
//! Δt_crit`): 25 000 steps pulling the `loading` and `fixed` edges apart by
//! `1e-3 / 25000` each per step, inserting cohesive elements where the
//! stress exceeds `σ_c` (`solid.checkCohesiveStress()`), then 25 000 steps
//! pushing them back (`coupler.checkCohesiveStress()`); after every step the
//! velocities of the nodes in contact (positive gap) are damped by 0.99.
//! Prints the step, the imposed displacement and the potential, kinetic and
//! total energies to the standard error every 1000 steps of each loop and
//! dumps displacement, velocity, normals, blocked DOFs, `grad_u`, stress,
//! gaps and areas (`paraview/cohesive-contact-explicit-dynamic_<n>.pvtu`,
//! the coupler's dumper, `akantu::couplers::dump`). The input's contact
//! penalty is `epsilon_n = 0` (no contact force).
//!
//! `--upstream-compat` selects every upstream defect of the cohesive model
//! (`CohesiveUpstreamCompat::upstream()`) and of the contact model
//! (`ContactUpstreamCompat::all()`: UD-002, assigned forces) and the
//! coupler's UD-374 / UD-772 / UD-1291 switches (`SolidContactCompat::all()`;
//! UD-1291 acts on static / implicit couplings only).
//! **The default run reproduces upstream** bitwise (solid state, energies,
//! element counts, text output) except the contact model's `gaps` (field
//! and dumps) from step 42 507 on: there the corrected natural projection
//! (UD-363, `projection_metric`) changes the gaps by at most 2e-14 m, which
//! no computation of this run reads (tested).
//!
//! ```text
//! cargo run -p akantu --release --example cohesive_contact_explicit_dynamic [-- --upstream-compat]
//! ```

#[path = "../common/mod.rs"]
pub mod common;

use std::process::ExitCode;

use akantu::cohesive::CohesiveUpstreamCompat;
use akantu::cohesive::selector::MeshDataMaterialCohesiveSelector;
use akantu::contact::{CohesiveSurfaceSelector, ContactUpstreamCompat};
use akantu::core::enums::{AnalysisMethod, SpatialDirection};
use akantu::core::{Real, Result};
use akantu::couplers::{CouplerSolidCohesiveContact, SolidContactCompat};
use akantu::mesh::SharedMesh;
use akantu::model::Model;
use akantu::model::boundary_condition::{FixedValue, IncrementValue};
use common::{Example, cout};

/// Spatial dimension of the example.
pub const SPATIAL_DIMENSION: usize = 2;

/// Result of a run.
#[derive(Debug)]
pub struct Outcome {
    /// The coupler in its final state (boxed: about 60 kB, too large for
    /// the stack frames of unoptimised builds).
    pub coupler: Box<CouplerSolidCohesiveContact>,
    /// Time step used (`0.1 Δt_crit`).
    pub time_step: Real,
    /// After every step (both loops): numbers of cohesive and contact
    /// elements.
    pub counts: Vec<[usize; 2]>,
    /// `[epot, ekin]` after every step (both loops).
    pub energies: Vec<[Real; 2]>,
}

/// Default configuration (upstream directory and input file).
#[must_use]
pub fn example() -> Example {
    Example::new(
        "cohesive_contact_explicit_dynamic",
        "contact_mechanics_model/hertz",
        "material-cohesive.dat",
    )
}

/// Upstream's damping of the velocities of the nodes in contact (`gap >
/// 0`), `vel *= damping_ratio` (akantu's `zip` stops with the gaps).
fn damp_contact_velocities(
    c: &mut CouplerSolidCohesiveContact,
    dim: usize,
    ratio: Real,
) -> Result<()> {
    let gaps = c.contact.gaps().as_slice().to_vec();
    let mut velocity = c.solid.smm.velocity_mut()?;
    for (gap, vel) in gaps
        .iter()
        .zip(velocity.as_mut_slice().chunks_exact_mut(dim))
    {
        if *gap > 0. {
            for v in vel {
                *v *= ratio;
            }
        }
    }
    Ok(())
}

/// The record of one step: counts and energies.
fn record(out: &mut Outcome) -> Result<[Real; 2]> {
    let c = &mut out.coupler;
    let nb_cohesive = {
        let mesh = c.solid.smm.mesh().read()?;
        let filter = akantu::core::TypeFilter::new()
            .dim(SPATIAL_DIMENSION)
            .kind(akantu::core::ElementKind::Cohesive);
        mesh.element_types(filter)
            .map(|t| mesh.nb_element(t, akantu::core::GhostType::NotGhost))
            .sum::<usize>()
    };
    out.counts
        .push([nb_cohesive, c.contact.contact_elements().len()]);
    let e = [c.solid.energy("potential")?, c.solid.energy("kinetic")?];
    out.energies.push(e);
    Ok(e)
}

/// The upstream `main` after `initialize`.
///
/// # Errors
/// Any error of the models, the mesh, the solver or the outputs.
pub fn run(ex: &Example) -> Result<Outcome> {
    run_with(ex, |_, _| Ok(()))
}

/// [`run`] calling `on_step(k, &coupler)` after every step (`k` counts the
/// steps of both loops; after the damping, before the outputs).
///
/// # Errors
/// Any error of the models, the mesh, the solver, the outputs or
/// `on_step`.
#[allow(clippy::too_many_lines)] // the upstream main, line by line
pub fn run_with(
    ex: &Example,
    mut on_step: impl FnMut(usize, &CouplerSolidCohesiveContact) -> Result<()>,
) -> Result<Outcome> {
    ex.create_output_dir()?;
    let spatial_dimension = SPATIAL_DIMENSION;

    let time_factor = 0.1;
    let upstream_max_steps: u32 = 25000;
    let max_steps = ex.steps_or(upstream_max_steps as usize);
    let max_displacement = 1e-3;

    let mesh = ex.read_mesh(spatial_dimension, "cohesive-contact.msh")?;

    let mut coupler = Box::new(CouplerSolidCohesiveContact::with_parser(
        SharedMesh::new(mesh),
        None,
        "coupler_solid_contact",
        ex.parser()?,
    )?);
    if ex.upstream_compat {
        // D-003: every upstream cohesive defect (UD-217, UD-040, …)
        coupler.solid.ext.compat = CohesiveUpstreamCompat::upstream();
    }

    let material_selector = MeshDataMaterialCohesiveSelector {
        default: coupler.solid.default_cohesive_selector(),
    };
    coupler
        .solid
        .set_material_selector(Box::new(material_selector));

    coupler
        .contact
        .set_surface_selector(Box::new(CohesiveSurfaceSelector::new()))?;

    coupler.init_full(&ex.model_options(AnalysisMethod::ExplicitLumpedMass), true)?;
    if ex.upstream_compat {
        // UD-002 (assigned forces, ½ for the deformable master)
        coupler
            .contact
            .set_upstream_compat(ContactUpstreamCompat::all());
        coupler.set_upstream_compat(SolidContactCompat::all());
    }

    coupler.apply_dirichlet_bc(
        &FixedValue {
            value: 0.0,
            axis: SpatialDirection::X,
        },
        "sides",
    )?;

    let mut time_step = coupler.solid.stable_time_step()?;
    time_step *= time_factor;
    ex.say(format!(
        "Time Step = {}s ({}s)",
        cout(time_step),
        cout(time_step)
    ));
    coupler.set_time_step(time_step, None)?;

    coupler.set_directory(&ex.paraview_dir())?;
    coupler.set_base_name("cohesive-contact-explicit-dynamic")?;
    coupler.add_dump_field_vector("displacement")?;
    coupler.add_dump_field_vector("velocity")?;
    coupler.add_dump_field_vector("normals")?;
    coupler.add_dump_field("blocked_dofs")?;
    coupler.add_dump_field("grad_u")?;
    coupler.add_dump_field("stress")?;
    coupler.add_dump_field("gaps")?;
    coupler.add_dump_field("areas")?;

    let damping_ratio = 0.99;
    // upstream `max_displacement / max_steps` (the upstream count, also in
    // reduced runs)
    let increment = max_displacement / Real::from(upstream_max_steps);

    let mut out = Outcome {
        coupler,
        time_step,
        counts: Vec::with_capacity(2 * max_steps),
        energies: Vec::with_capacity(2 * max_steps),
    };
    let up = |value| IncrementValue {
        value,
        axis: SpatialDirection::Y,
    };

    for i in 0..max_steps {
        let c = &mut out.coupler;
        c.apply_dirichlet_bc(&up(increment), "loading")?;
        c.apply_dirichlet_bc(&up(-increment), "fixed")?;

        c.solve_step(None)?;

        c.solid.check_cohesive_stress()?;
        // upstream's contact model (on the facet mesh) receives the mesh
        // events of the insertion at once; the port's coupler forwards them
        // (`coupler.check_cohesive_stress()` does both)
        c.forward_new_cohesive_elements()?;

        // damping velocities only along the contacting zone
        damp_contact_velocities(c, spatial_dimension, damping_ratio)?;

        let [epot, ekin] = record(&mut out)?;
        on_step(out.counts.len() - 1, &out.coupler)?;
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
            let c = &mut out.coupler;
            c.dump(None, None)?;
        }
    }

    for i in 0..max_steps {
        let c = &mut out.coupler;
        c.solid.smm.apply_dirichlet_bc(&up(-increment), "loading")?;
        c.solid.smm.apply_dirichlet_bc(&up(increment), "fixed")?;

        c.solve_step(None)?;

        c.check_cohesive_stress()?;

        // damping velocities only along the contacting zone
        damp_contact_velocities(c, spatial_dimension, damping_ratio)?;

        let [epot, ekin] = record(&mut out)?;
        on_step(out.counts.len() - 1, &out.coupler)?;
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
            let c = &mut out.coupler;
            c.dump(None, None)?;
        }
    }

    Ok(out)
}

fn main() -> ExitCode {
    common::main_with(|args| run(&example().with_args(args)?).map(|_| ()))
}
