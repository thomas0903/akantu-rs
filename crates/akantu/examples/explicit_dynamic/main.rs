// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): examples/c++/solid_mechanics_model/
// explicit/explicit_dynamic.cc (+ material.dat, bar.geo).

//! `explicit_dynamic` (EX-019): a 3-D steel bar (`10 × 1 × 1`, linear
//! tetrahedra) with an initial sine × Gaussian displacement pulse, explicit
//! central differences with a lumped mass, `Δt = 0.8 Δt_crit`, 1000 steps.
//! Writes the potential and kinetic energies of every step to
//! `energy.csv` and dumps displacement, velocity, acceleration and stress
//! at every step (`paraview/explicit_dynamic_<step>.pvtu`).
//!
//! ```text
//! cargo run -p akantu --release --example explicit_dynamic [-- --output-dir <dir>]
//! ```

#[path = "../common/mod.rs"]
pub mod common;

use std::f64::consts::PI;
use std::process::ExitCode;

use akantu::core::enums::AnalysisMethod;
use akantu::core::{Real, Result};
use akantu::mesh::SharedMesh;
use akantu::solid::SolidMechanicsModel;
use common::{Example, TextOutput, cout};

/// Result of a run.
#[derive(Debug)]
pub struct Outcome {
    /// The model in its final state.
    pub model: SolidMechanicsModel,
    /// Critical time step (upstream `getStableTimeStep()`).
    pub stable_time_step: Real,
    /// Time step used (`0.8 Δt_crit`).
    pub time_step: Real,
    /// `[epot, ekin]` after every step.
    pub energies: Vec<[Real; 2]>,
}

/// Default configuration (upstream directory and input file).
#[must_use]
pub fn example() -> Example {
    Example::new(
        "explicit_dynamic",
        "solid_mechanics_model/explicit",
        "material.dat",
    )
}

/// The initial displacement `u_x` of a node at abscissa `x` (upstream
/// loop over the nodes; same operation order).
#[must_use]
pub fn pulse(x: Real) -> Real {
    let pulse_width = 2.;
    let a = 0.01;
    let x = x - 2.;
    // Sinus * Gaussian
    let l = pulse_width;
    let k = 0.1 * 2. * PI * 3. / l;
    a * (k * x).sin() * (-(k * x) * (k * x) / (l * l)).exp()
}

/// The upstream `main` after `initialize`.
///
/// # Errors
/// Any error of the model, the mesh, the solver or the outputs.
pub fn run(ex: &Example) -> Result<Outcome> {
    ex.create_output_dir()?;
    let spatial_dimension = 3;
    let time_factor = 0.8;
    let max_steps = ex.steps_or(1000);

    let mesh = ex.read_mesh(spatial_dimension, "bar.msh")?;
    let nodes = mesh.nodes().as_slice().to_vec();
    let nb_nodes = mesh.nb_nodes();

    let mut model = SolidMechanicsModel::with_parser(
        SharedMesh::new(mesh),
        None,
        "solid_mechanics_model",
        ex.parser()?,
    )?;

    // model initialization
    model.init_full(&ex.model_options(AnalysisMethod::ExplicitLumpedMass))?;

    let stable_time_step = model.stable_time_step()?;
    ex.say(format!(
        "Time Step = {}s ({}s)",
        cout(stable_time_step * time_factor),
        cout(stable_time_step)
    ));
    let time_step = stable_time_step * time_factor;
    model.set_time_step(time_step, None)?;

    // boundary and initial conditions
    {
        let mut displacement = model.displacement_mut()?;
        let u = displacement.as_mut_slice();
        for n in 0..nb_nodes {
            u[n * spatial_dimension] = pulse(nodes[n * spatial_dimension]);
        }
    }

    let mut energy = TextOutput::new(ex.output("energy.csv"));
    energy.line("id,rtime,epot,ekin,tot");

    model.set_directory(&ex.paraview_dir())?;
    model.set_base_name("explicit_dynamic")?;
    model.add_dump_field("displacement")?;
    model.add_dump_field("velocity")?;
    model.add_dump_field("acceleration")?;
    model.add_dump_field("stress")?;
    if ex.dump {
        model.dump(None, None)?;
    }

    let mut energies = Vec::with_capacity(max_steps);
    for s in 1..=max_steps {
        model.solve_step(None)?;

        let epot = model.energy("potential")?;
        let ekin = model.energy("kinetic")?;
        energies.push([epot, ekin]);

        #[allow(clippy::cast_precision_loss)] // upstream `s * time_step` (Int → Real)
        let rtime = s as Real * time_step;
        energy.line(format!(
            "{s},{},{},{},{},",
            cout(rtime),
            cout(epot),
            cout(ekin),
            cout(epot + ekin)
        ));

        if s.is_multiple_of(10) {
            ex.say(format!("passing step {s}/{max_steps}"));
        }
        if ex.dump {
            model.dump(None, None)?;
        }
    }

    energy.close()?;
    Ok(Outcome {
        model,
        stable_time_step,
        time_step,
        energies,
    })
}

fn main() -> ExitCode {
    common::main_with(|args| run(&example().with_args(args)?).map(|_| ()))
}
