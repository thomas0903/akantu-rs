// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): examples/c++/solid_mechanics_model/static/
// static.cc (+ material.dat, square.geo).

//! `static` (EX-021): static plane problem on a square of quadratic
//! triangles, `u_x = 0` on `Fixed_x`, `u_y = 0` on `Fixed_y`, `u_x = 1e-4`
//! on `Displacement`; Newton–Raphson limited to 2 iterations, threshold
//! `2e-4` on the solution increment. Dumps the initial and the solved
//! state (`paraview/static_000{0,1}.pvtu`).
//!
//! The sparse solver is the default one (upstream `_auto`: MUMPS when
//! available, else Eigen's SparseLU; the port: Eigen's SparseLU unless the
//! `mumps` backend is registered). `--solver eigen` pins it (the oracle
//! variant `static_eigen` pins `_eigen` for bitwise comparisons).
//!
//! ```text
//! cargo run -p akantu --example static [-- --output-dir <dir>]
//! ```

#[path = "../common/mod.rs"]
pub mod common;

use std::process::ExitCode;

use akantu::core::Result;
use akantu::core::enums::{AnalysisMethod, SpatialDirection};
use akantu::mesh::SharedMesh;
use akantu::model::boundary_condition::FixedValue;
use akantu::parser::param::ParamValue;
use akantu::solid::SolidMechanicsModel;
use common::Example;

/// Default configuration (upstream directory and input file).
#[must_use]
pub fn example() -> Example {
    Example::new("static", "solid_mechanics_model/static", "material.dat")
}

/// The upstream `main` after `initialize`; returns the model in its final
/// state.
///
/// # Errors
/// Any error of the model, the mesh, the solver or the dumper.
pub fn run(ex: &Example) -> Result<SolidMechanicsModel> {
    ex.create_output_dir()?;
    let spatial_dimension = 2;
    let mesh = ex.read_mesh(spatial_dimension, "square.msh")?;

    let mut model = SolidMechanicsModel::with_parser(
        SharedMesh::new(mesh),
        None,
        "solid_mechanics_model",
        ex.parser()?,
    )?;

    // model initialization
    model.init_full(&ex.model_options(AnalysisMethod::Static))?;

    // configure dumper
    model.set_directory(&ex.paraview_dir())?;
    model.set_base_name("static")?;
    model.add_dump_field_vector("displacement")?;
    model.add_dump_field("external_force")?;
    model.add_dump_field("internal_force")?;
    model.add_dump_field("blocked_dofs")?;
    model.add_dump_field("grad_u")?;
    model.add_dump_field("stress")?;

    // Dirichlet boundary conditions
    let fixed = |value, axis| FixedValue { value, axis };
    model.apply_dirichlet_bc(&fixed(0.0, SpatialDirection::X), "Fixed_x")?;
    model.apply_dirichlet_bc(&fixed(0.0, SpatialDirection::Y), "Fixed_y")?;
    model.apply_dirichlet_bc(&fixed(0.0001, SpatialDirection::X), "Displacement")?;
    if ex.dump {
        model.dump(None, None)?;
    }

    // set the limits for the Newton-Raphson solver
    let solver = model.solver.non_linear_solver_mut(None)?;
    solver.set_param("max_iterations", ParamValue::Int(2))?;
    solver.set_param("threshold", ParamValue::Real(2e-4))?;
    solver.set_param("convergence_type", ParamValue::Enum("solution".into()))?;

    // static solve
    model.solve_step(None)?;
    if ex.dump {
        model.dump(None, None)?;
    }
    Ok(model)
}

fn main() -> ExitCode {
    common::main_with(|args| run(&example().with_args(args)?).map(|_| ()))
}
