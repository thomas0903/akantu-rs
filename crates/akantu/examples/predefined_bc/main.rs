// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): examples/c++/solid_mechanics_model/
// boundary_conditions/predefined_bc/predefined_bc.cc (+ material.dat,
// square.geo).

//! `predefined_bc` (EX-013): the predefined Dirichlet functor
//! `FixedValue` on the groups `Fixed_x` and `Fixed_y` of a square, dumped
//! once to ParaView (`paraview/plate_0000.pvtu`); no solve.
//!
//! ```text
//! cargo run -p akantu --example predefined_bc [-- --output-dir <dir>]
//! ```

#[path = "../common/mod.rs"]
pub mod common;

use std::process::ExitCode;

use akantu::core::Result;
use akantu::core::enums::SpatialDirection;
use akantu::mesh::SharedMesh;
use akantu::model::ModelOptions;
use akantu::model::boundary_condition::FixedValue;
use akantu::solid::SolidMechanicsModel;
use common::Example;

/// Default configuration (upstream directory and input file).
#[must_use]
pub fn example() -> Example {
    Example::new(
        "predefined_bc",
        "solid_mechanics_model/boundary_conditions/predefined_bc",
        "material.dat",
    )
}

/// The upstream `main` after `initialize`; returns the model in its final
/// state.
///
/// # Errors
/// Any error of the model, the mesh or the dumper.
pub fn run(ex: &Example) -> Result<SolidMechanicsModel> {
    ex.create_output_dir()?;
    let mesh = ex.read_mesh(2, "square.msh")?;

    // model initialization
    let mut model = SolidMechanicsModel::with_parser(
        SharedMesh::new(mesh),
        None,
        "solid_mechanics_model",
        ex.parser()?,
    )?;
    model.init_full(&ModelOptions::default())?;

    // Dirichlet boundary conditions
    model.apply_dirichlet_bc(
        &FixedValue {
            value: 0.0,
            axis: SpatialDirection::X,
        },
        "Fixed_x",
    )?;
    model.apply_dirichlet_bc(
        &FixedValue {
            value: 0.0,
            axis: SpatialDirection::Y,
        },
        "Fixed_y",
    )?;

    // output in a paraview file
    model.set_directory(&ex.paraview_dir())?;
    model.set_base_name("plate")?;
    model.add_dump_field_vector("displacement")?;
    model.add_dump_field("blocked_dofs")?;
    model.add_dump_field("external_force")?;
    if ex.dump {
        model.dump(None, None)?;
    }
    Ok(model)
}

fn main() -> ExitCode {
    common::main_with(|args| run(&example().with_args(args)?).map(|_| ()))
}
