// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): examples/c++/solid_mechanics_model/
// implicit/implicit_dynamic.cc (+ material_dynamic.dat, beam.geo).

//! `implicit_dynamic` (EX-020): a simply supported 2-D beam (`10 × 1`,
//! quadratic triangles, plane strain) under a point load `F = 5e3` at
//! mid-span, implicit Newmark (trapezoidal rule), `Δt = 1e-4` until
//! `t = 0.62`; Newton–Raphson with at most 100 iterations, threshold
//! `1e-12` on the solution increment. Writes the vertical displacement of
//! the loaded node and the Euler–Bernoulli modal solution (three modes) of
//! every step to `position.csv`; dumps every 100 steps
//! (`paraview/dynamic_<n>.pvtu`).
//!
//! The mesh is distributed (`mesh.distribute()`, a no-op on one rank) as
//! upstream. `--solver eigen` pins the sparse solver (upstream `_auto` is
//! MUMPS when available; the oracle variant `implicit_dynamic_eigen` pins
//! `_eigen` for bitwise comparisons).
//!
//! ```text
//! cargo run -p akantu --release --example implicit_dynamic [-- --output-dir <dir>]
//! ```

#[path = "../common/mod.rs"]
pub mod common;

use std::f64::consts::PI;
use std::process::ExitCode;

use akantu::core::enums::{AnalysisMethod, SpatialDirection};
use akantu::core::{Real, Result};
use akantu::mesh::SharedMesh;
use akantu::mesh::distribution::distribute_with;
use akantu::model::boundary_condition::FixedValue;
use akantu::parser::param::ParamValue;
use akantu::solid::SolidMechanicsModel;
use common::{Example, TextOutput, cout};

const BAR_LENGTH: Real = 10.;
const BAR_HEIGHT: Real = 1.;
const BAR_DEPTH: Real = 1.;
/// Point load.
pub const F: Real = 5e3;
const L: Real = BAR_LENGTH;
const I: Real = BAR_DEPTH * BAR_HEIGHT * BAR_HEIGHT * BAR_HEIGHT / 12.;
const E: Real = 12e7;
const RHO: Real = 1000.;
const M: Real = RHO * BAR_HEIGHT * BAR_DEPTH;
/// Time step.
pub const TIME_STEP: Real = 1e-4;
/// Final time.
pub const MAX_TIME: Real = 0.62;

/// Angular frequency of mode `n` of the simply supported beam.
fn w(n: u32) -> Real {
    Real::from(n * n) * PI * PI / (L * L) * (E * I / M).sqrt()
}

/// Mid-span deflection of the Euler–Bernoulli beam under a suddenly
/// applied point load (modes 1, 3 and 5; upstream `analytical_solution`).
#[must_use]
pub fn analytical_solution(time: Real) -> Real {
    2. * F * L * L * L / (PI.powf(4.) * E * I)
        * ((1. - (w(1) * time).cos())
            + (1. - (w(3) * time).cos()) / 81.
            + (1. - (w(5) * time).cos()) / 625.)
}

/// Result of a run.
#[derive(Debug)]
pub struct Outcome {
    /// The model in its final state.
    pub model: SolidMechanicsModel,
    /// Loaded node (first node of the group `traction`), if local.
    pub traction_node: Option<usize>,
    /// `(time, u_y, solution)` of every step (rows of `position.csv`).
    pub positions: Vec<(Real, Real, Real)>,
}

/// Default configuration (upstream directory and input file).
#[must_use]
pub fn example() -> Example {
    Example::new(
        "implicit_dynamic",
        "solid_mechanics_model/implicit",
        "material_dynamic.dat",
    )
}

/// The upstream `main` after `initialize`.
///
/// # Errors
/// Any error of the model, the mesh, the solver or the outputs.
pub fn run(ex: &Example) -> Result<Outcome> {
    run_with(ex, |_, _| Ok(()))
}

/// [`run`] calling `on_step(s, &model)` after the solve of every step `s`
/// (1-based, before the step's outputs), e.g. to record the state.
///
/// # Errors
/// Any error of the model, the mesh, the solver, the outputs or `on_step`.
pub fn run_with(
    ex: &Example,
    mut on_step: impl FnMut(usize, &SolidMechanicsModel) -> Result<()>,
) -> Result<Outcome> {
    ex.create_output_dir()?;
    let spatial_dimension = 2;
    let max_steps = ex.steps;

    let mut mesh = ex.read_mesh(spatial_dimension, "beam.msh")?;
    let prank = mesh.communicator().rank();
    // no mesh consumer exists yet: the distribution events have no receiver
    let _events = distribute_with(&mut mesh, None, 0)?;
    let mesh = SharedMesh::new(mesh);

    let mut model = SolidMechanicsModel::with_parser(
        mesh.clone(),
        None,
        "solid_mechanics_model",
        ex.parser()?,
    )?;

    // model initialization
    model.init_full(&ex.model_options(AnalysisMethod::ImplicitDynamic))?;
    {
        let mat = model.material_mut(0)?;
        mat.set_param("E", ParamValue::Real(E))?;
        mat.set_param("rho", ParamValue::Real(RHO))?;
    }

    // boundary conditions
    let fixed = |axis| FixedValue { value: 0.0, axis };
    model.apply_dirichlet_bc(&fixed(SpatialDirection::X), "blocked")?;
    model.apply_dirichlet_bc(&fixed(SpatialDirection::Y), "blocked")?;
    model.apply_dirichlet_bc(&fixed(SpatialDirection::Y), "roller")?;

    let traction_node = {
        let m = mesh.read()?;
        let trac_nodes = m.groups().element_group("traction")?.nodes();
        match trac_nodes.as_slice().first() {
            Some(&n) if m.is_local_or_master_node(n)? => Some(akantu::core::try_to_usize(n)?),
            _ => None,
        }
    };
    if let Some(n) = traction_node {
        model.external_force_mut()?.as_mut_slice()[n * spatial_dimension + 1] = F;
    }

    // output setup
    let mut pos = TextOutput::new(ex.output("position.csv"));
    pos.line("id,time,position,solution");

    model.set_directory(&ex.paraview_dir())?;
    model.set_base_name("dynamic")?;
    model.add_dump_field_vector("displacement")?;
    model.add_dump_field("velocity")?;
    model.add_dump_field("acceleration")?;
    model.add_dump_field("external_force")?;
    model.add_dump_field("internal_force")?;
    if ex.dump {
        model.dump(None, None)?;
    }

    model.set_time_step(TIME_STEP, None)?;

    let solver = model.solver.non_linear_solver_mut(None)?;
    solver.set_param("max_iterations", ParamValue::Int(100))?;
    solver.set_param("threshold", ParamValue::Real(1e-12))?;
    solver.set_param("convergence_type", ParamValue::Enum("solution".into()))?;

    // time loop
    let mut positions = Vec::new();
    let mut time = 0.;
    let mut s: usize = 1;
    while time < MAX_TIME && max_steps.is_none_or(|m| s <= m) {
        if prank == 0 {
            ex.print_raw(format!("{s}\r"));
        }

        model.solve_step(None)?;
        on_step(s, &model)?;

        if let Some(n) = traction_node {
            let u = model.displacement()?.as_slice()[n * spatial_dimension + 1];
            #[allow(clippy::cast_precision_loss)] // upstream `s * time_step` (Int → Real)
            let solution = analytical_solution(s as Real * TIME_STEP);
            pos.line(format!("{s},{},{},{}", cout(time), cout(u), cout(solution)));
            positions.push((time, u, solution));
        }

        if s.is_multiple_of(100) && ex.dump {
            model.dump(None, None)?;
        }
        s += 1;
        time += TIME_STEP;
    }

    ex.say("");
    pos.close()?;
    Ok(Outcome {
        model,
        traction_node,
        positions,
    })
}

fn main() -> ExitCode {
    common::main_with(|args| run(&example().with_args(args)?).map(|_| ()))
}
