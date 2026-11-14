// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): examples/c++/diffusion_model/
// heat_diffusion_dynamics_3d.cc (+ material.dat, cube.geo).

//! `heat_diffusion_dynamics_3d` (EX-007): transient heat diffusion in the
//! unit cube (`cube.msh`, quadratic tetrahedra), explicit forward Euler
//! with a lumped capacity, `Δt = 0.8 Δt_crit`, 1000 steps. Every node
//! starts at 100; the nodes within 0.1 of the centre are held at 300 (a
//! heat source). Dumps temperature, temperature rate and internal heat
//! rate every 100 steps (`paraview/heat_diffusion_cube3d_<n>.pvtu`).
//!
//! ```text
//! cargo run -p akantu --release --example heat_diffusion_dynamics_3d [-- --output-dir <dir>]
//! ```

#[path = "../common/mod.rs"]
pub mod common;

use std::process::ExitCode;

use akantu::core::enums::AnalysisMethod;
use akantu::core::{Real, Result};
use akantu::diffusion::HeatTransferModel;
use akantu::mesh::SharedMesh;
use common::{Example, cout};

/// Spatial dimension of the example (upstream `const Int spatial_dimension`).
pub const SPATIAL_DIMENSION: usize = 3;

/// Result of a run.
#[derive(Debug)]
pub struct Outcome {
    /// The model in its final state.
    pub model: HeatTransferModel,
    /// Time step used (`0.8 Δt_crit`).
    pub time_step: Real,
    /// Number of nodes held at 300 (the heat source).
    pub nb_hot_nodes: usize,
}

/// Default configuration (upstream directory and input file).
#[must_use]
pub fn example() -> Example {
    Example::new(
        "heat_diffusion_dynamics_3d",
        "diffusion_model",
        "material.dat",
    )
}

/// The upstream `main` after `initialize`.
///
/// # Errors
/// Any error of the model, the mesh or the dumper.
pub fn run(ex: &Example) -> Result<Outcome> {
    ex.create_output_dir()?;
    let mesh = ex.read_mesh(SPATIAL_DIMENSION, "cube.msh")?;
    let nodes = mesh.nodes().as_slice().to_vec();

    let mut model =
        HeatTransferModel::with_parser(SharedMesh::new(mesh), None, "heat_transfer", ex.parser()?)?;
    // initialize everything
    model.init_full(&ex.model_options(AnalysisMethod::ExplicitLumpedMass))?;

    // get and set stable time step
    let time_step = model.stable_time_step()? * 0.8;
    ex.say(format!("Stable Time Step is : {}", cout(time_step / 0.8)));
    ex.say(format!("time step is:{}", cout(time_step)));
    model.set_time_step(time_step, None)?;

    // boundary conditions
    let mut temperature = model.temperature()?.as_slice().to_vec();
    let mut boundary = model.blocked_dofs()?.as_slice().to_vec();

    let length = 1.;

    let mut nb_hot_nodes = 0;
    for (i, x) in nodes.as_chunks::<SPATIAL_DIMENSION>().0.iter().enumerate() {
        temperature[i] = 100.;

        // to insert a heat source
        let dx = x[0] - length / 2.;
        let dy = x[1] - length / 2.;
        let dz = x[2] - length / 2.;

        let d = (dx * dx + dy * dy + dz * dz).sqrt();

        if d < 0.1 {
            boundary[i] = true;
            temperature[i] = 300.;
            nb_hot_nodes += 1;
        }
    }
    model
        .temperature_mut()?
        .as_mut_slice()
        .copy_from_slice(&temperature);
    model
        .blocked_dofs_mut()?
        .as_mut_slice()
        .copy_from_slice(&boundary);

    model.set_directory(&ex.paraview_dir())?;
    model.set_base_name("heat_diffusion_cube3d")?;
    model.add_dump_field("temperature")?;
    model.add_dump_field("temperature_rate")?;
    model.add_dump_field("internal_heat_rate")?;

    // //for testing
    let max_steps = ex.steps_or(1000);

    for i in 0..max_steps {
        model.solve_step(None)?;

        if i.is_multiple_of(100) && ex.dump {
            model.dump(None, None)?;
        }

        if i.is_multiple_of(10) {
            ex.say(format!("Step {i}/{max_steps}"));
        }
    }

    Ok(Outcome {
        model,
        time_step,
        nb_hot_nodes,
    })
}

fn main() -> ExitCode {
    common::main_with(|args| run(&example().with_args(args)?).map(|_| ()))
}
