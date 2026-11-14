// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): examples/c++/solid_mechanics_model/io/
// parser/example_parser.cc (+ input_file.dat, swiss_cheese.geo).

//! `example_parser` (EX-027): user parameters read from the `user
//! parameters [ … ]` section of the input file — spatial dimension, mesh
//! file, group names, Newton–Raphson settings and a stress matrix written
//! as an expression of another parameter — drive a static solve of a
//! "Swiss cheese" plate: the outer crust is clamped, the holes are loaded
//! by the stress (`FromStress`). Dumps the displacement
//! (`paraview/swiss_cheese_0000.pvtu`).
//!
//! `--solver eigen` pins the sparse solver (upstream `_auto` is MUMPS when
//! available; the oracle variant `example_parser_eigen` pins `_eigen`).
//!
//! ```text
//! cargo run -p akantu --example example_parser [-- --output-dir <dir>]
//! ```

#[path = "../common/mod.rs"]
pub mod common;

use std::process::ExitCode;

use akantu::core::enums::{AnalysisMethod, SpatialDirection};
use akantu::core::tensor::MatrixX;
use akantu::core::{Real, Result};
use akantu::mesh::SharedMesh;
use akantu::model::boundary_condition::{FixedValue, FromStress};
use akantu::parser::SearchScope;
use akantu::parser::param::ParamValue;
use akantu::solid::SolidMechanicsModel;
use common::Example;

/// The user parameters of the input file.
#[derive(Clone, Debug)]
pub struct UserParameters {
    /// `spatial_dimension`.
    pub spatial_dimension: u32,
    /// `mesh_file`.
    pub mesh_file: String,
    /// `max_nb_iterations`.
    pub max_iter: i64,
    /// `precision`.
    pub precision: Real,
    /// `stress` (`[[lactostatic_p, 0], [0, lactostatic_p]]`).
    pub eigen_stress: MatrixX,
    /// `outter_crust`.
    pub outer_crust: String,
    /// `inner_holes`.
    pub inner_holes: String,
}

/// Result of a run.
#[derive(Debug)]
pub struct Outcome {
    /// The model in its final state.
    pub model: SolidMechanicsModel,
    /// The user parameters read.
    pub user: UserParameters,
}

/// Default configuration (upstream directory and input file).
#[must_use]
pub fn example() -> Example {
    Example::new(
        "example_parser",
        "solid_mechanics_model/io/parser",
        "input_file.dat",
    )
}

/// The upstream `main` after `initialize`.
///
/// # Errors
/// Any error of the parser, the model, the mesh, the solver or the dumper.
pub fn run(ex: &Example) -> Result<Outcome> {
    ex.create_output_dir()?;
    let parser = ex.parser()?;

    // Get the user ParserSection.
    let usersect = parser.user_section()?;
    let cur = SearchScope::Current;

    // getParameterValue() extracts the data associated to a parameter name
    // and converts it into the requested type.
    let user = UserParameters {
        spatial_dimension: usersect.get_parameter_value("spatial_dimension", cur)?,
        mesh_file: usersect.get_parameter_value("mesh_file", cur)?,
        // getParameter() with the destination type known from the variable
        max_iter: usersect.get_parameter("max_nb_iterations", cur)?.get()?,
        precision: usersect.get_parameter("precision", cur)?.get()?,
        // Following NumPy convention, data can be interpreted as vectors or
        // matrices.
        eigen_stress: usersect.get_parameter("stress", cur)?.get()?,
        outer_crust: usersect.get_parameter_value("outter_crust", cur)?,
        inner_holes: usersect.get_parameter_value("inner_holes", cur)?,
    };
    let mesh = ex.read_mesh(user.spatial_dimension as usize, &user.mesh_file)?;

    let mut model = SolidMechanicsModel::with_parser(
        SharedMesh::new(mesh),
        None,
        "solid_mechanics_model",
        parser.clone(),
    )?;
    model.init_full(&ex.model_options(AnalysisMethod::Static))?;

    let fixed = |axis| FixedValue { value: 0.0, axis };
    model.apply_dirichlet_bc(&fixed(SpatialDirection::X), &user.outer_crust)?;
    model.apply_dirichlet_bc(&fixed(SpatialDirection::Y), &user.outer_crust)?;
    model.apply_neumann_bc(
        &FromStress::new(user.eigen_stress.clone()),
        &user.inner_holes,
    )?;

    model.set_directory(&ex.paraview_dir())?;
    model.set_base_name("swiss_cheese")?;
    model.add_dump_field_vector("displacement")?;

    let solver = model.solver.non_linear_solver_mut(None)?;
    solver.set_param("max_iterations", ParamValue::Int(user.max_iter))?;
    solver.set_param("threshold", ParamValue::Real(user.precision))?;

    model.solve_step(None)?;

    if ex.dump {
        model.dump(None, None)?;
    }
    Ok(Outcome { model, user })
}

fn main() -> ExitCode {
    common::main_with(|args| run(&example().with_args(args)?).map(|_| ()))
}
