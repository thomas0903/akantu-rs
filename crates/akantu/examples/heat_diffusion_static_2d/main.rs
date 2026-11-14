// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): examples/c++/diffusion_model/
// heat_diffusion_static_2d.cc (+ material.dat, square.geo).

//! `heat_diffusion_static_2d` (EX-006): static heat conduction in the unit
//! square (`square.msh`, quadratic triangles). The boundary of the square
//! is held at 100 and the nodes within 0.1 of `(¼, ¼)` at 300; one linear
//! solve. Dumps temperature, internal heat rate, conductivity and the
//! blocked DOFs before and after the solve
//! (`paraview/heat_diffusion_static_2d_<n>.pvtu`). The upstream solver is
//! `_auto` (MUMPS in the oracle); `--solver eigen` pins the Eigen sparse LU.
//!
//! ```text
//! cargo run -p akantu --example heat_diffusion_static_2d [-- --solver eigen]
//! ```

#[path = "../common/mod.rs"]
pub mod common;

use std::process::ExitCode;

use akantu::core::Result;
use akantu::core::enums::AnalysisMethod;
use akantu::diffusion::HeatTransferModel;
use akantu::mesh::SharedMesh;
use common::Example;

/// Spatial dimension of the example (upstream `const Int spatial_dimension`).
pub const SPATIAL_DIMENSION: usize = 2;

/// Default configuration (upstream directory and input file).
#[must_use]
pub fn example() -> Example {
    Example::new(
        "heat_diffusion_static_2d",
        "diffusion_model",
        "material.dat",
    )
}

/// The upstream `main` after `initialize`; returns the model in its final
/// state.
///
/// # Errors
/// Any error of the model, the mesh, the solver or the dumper.
pub fn run(ex: &Example) -> Result<HeatTransferModel> {
    ex.create_output_dir()?;
    // create mesh
    let mesh = ex.read_mesh(SPATIAL_DIMENSION, "square.msh")?;
    let nodes = mesh.nodes().as_slice().to_vec();

    let mut model =
        HeatTransferModel::with_parser(SharedMesh::new(mesh), None, "heat_transfer", ex.parser()?)?;
    // initialize everything
    model.init_full(&ex.model_options(AnalysisMethod::Static))?;

    // boundary conditions
    let mut temperature = model.temperature()?.as_slice().to_vec();
    let mut blocked_dofs = model.blocked_dofs()?.as_slice().to_vec();
    let length = 1.;
    for (i, x) in nodes.as_chunks::<SPATIAL_DIMENSION>().0.iter().enumerate() {
        temperature[i] = 100.;

        let dx = x[0];
        let dy = x[1];

        // upstream `Vector<Real> dX = {dx, dy}; dX.array() -= length / 4.;
        // dX.norm()`
        let d_x = [dx - length / 4., dy - length / 4.];
        let d = (d_x[0] * d_x[0] + d_x[1] * d_x[1]).sqrt();
        if d < 0.1 {
            blocked_dofs[i] = true;
            temperature[i] = 300.;
        }

        if dx.abs() < 1e-4 || dy.abs() < 1e-4 {
            blocked_dofs[i] = true;
        }
        if (dx - length).abs() < 1e-4 || (dy - length).abs() < 1e-4 {
            blocked_dofs[i] = true;
        }
    }
    model
        .temperature_mut()?
        .as_mut_slice()
        .copy_from_slice(&temperature);
    model
        .blocked_dofs_mut()?
        .as_mut_slice()
        .copy_from_slice(&blocked_dofs);

    model.set_directory(&ex.paraview_dir())?;
    model.set_base_name("heat_diffusion_static_2d")?;
    model.add_dump_field("temperature")?;
    model.add_dump_field("internal_heat_rate")?;
    model.add_dump_field("conductivity")?;
    model.add_dump_field("blocked_dofs")?;
    if ex.dump {
        model.dump(None, None)?;
    }

    model.solve_step(None)?;
    if ex.dump {
        model.dump(None, None)?;
    }

    Ok(model)
}

fn main() -> ExitCode {
    common::main_with(|args| run(&example().with_args(args)?).map(|_| ()))
}
