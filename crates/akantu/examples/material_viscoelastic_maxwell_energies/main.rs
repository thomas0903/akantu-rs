// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): examples/c++/solid_mechanics_model/
// new_material/viscoelastic_maxwell/material_viscoelastic_maxwell_energies.cc
// (+ material_viscoelastic_maxwell.dat, material_viscoelastic_maxwell_mesh.geo).

//! `material_viscoelastic_maxwell_energies` (EX-024): a `1 mm` square of
//! generalised Maxwell material (`viscoelastic_maxwell`, one branch) under
//! a prescribed biaxial strain `ε(t)` ramped to `0.1` over `T = 10` and
//! then held until `t = 100` (`Δt = 0.1`, 1002 static Newton–Raphson
//! solves, residual criterion `1e-7`, at most 10 iterations). Prints the
//! material, the time and the iteration count of every step, writes
//! `t ε E_pot E_diss W` per step to `material_viscoelastic_maxwell_output.out`
//! and dumps every step (`paraview/dynamic_<step>.pvtu`). Returns
//! `EXIT_FAILURE` when a solve does not converge.
//!
//! `--upstream-compat` selects upstream's material defects
//! (`LawOptions::upstream()`: UD-029, the branch stresses of
//! `viscoelastic_maxwell` never change) and the Newton solver's early return
//! of UD-010 (0 iterations and no `afterSolveStep` once the first residual
//! converges: every step of the hold) and the dumper's layout of UD-110
//! (`strain` written as a zero-filled square). The upstream solver is `_auto`
//! (MUMPS in the oracle); `--solver eigen` pins the Eigen sparse LU.
//!
//! ```text
//! cargo run -p akantu --example material_viscoelastic_maxwell_energies [-- --upstream-compat]
//! ```

#[path = "../common/mod.rs"]
pub mod common;

use std::process::ExitCode;

use akantu::core::enums::AnalysisMethod;
use akantu::core::math::{DEFAULT_TOLERANCE, are_float_equal};
use akantu::core::{Error, Real, Result};
use akantu::mesh::SharedMesh;
use akantu::parser::param::ParamValue;
use akantu::solid::SolidMechanicsModel;
use akantu::solid::material::LawOptions;
use common::{Example, TextOutput, cout};

/// One line of the output file: `[t, ε, E_pot, E_diss, W]`.
pub type Record = [Real; 5];

/// Result of a run.
#[derive(Debug)]
pub struct Outcome {
    /// The model in its final state.
    pub model: SolidMechanicsModel,
    /// The data of every step.
    pub records: Vec<Record>,
    /// Newton iterations and final error of every step.
    pub newton: Vec<(i64, Real)>,
    /// Upstream's exit status: `false` when a solve did not converge
    /// (upstream returns `EXIT_FAILURE` after printing the failure).
    pub converged: bool,
}

/// Default configuration (upstream directory and input file).
#[must_use]
pub fn example() -> Example {
    Example::new(
        "material_viscoelastic_maxwell_energies",
        "solid_mechanics_model/new_material/viscoelastic_maxwell",
        "material_viscoelastic_maxwell.dat",
    )
}

/// An integer or real solver parameter as a number.
fn number(v: &ParamValue) -> Result<Real> {
    match v {
        ParamValue::Real(x) => Ok(*x),
        #[allow(clippy::cast_precision_loss)] // iteration counts
        ParamValue::Int(i) => Ok(*i as Real),
        other => Err(Error::invalid(format!("not a number: {other:?}"))),
    }
}

/// The upstream `main` after `initialize`.
///
/// # Errors
/// Any error of the model, the mesh, the solver (other than a
/// non-converged solve, which ends the run as upstream's `EXIT_FAILURE`) or
/// the outputs.
#[allow(clippy::too_many_lines)] // the upstream main, line by line
#[allow(clippy::if_same_then_else)] // upstream's branches, kept for the diff
pub fn run(ex: &Example) -> Result<Outcome> {
    ex.create_output_dir()?;
    // sim data
    let eps = 0.1;

    let dim = 2;
    let sim_time = 100.;
    let t_ramp = 10.; // upstream `T`
    let mesh = ex.read_mesh(dim, "material_viscoelastic_maxwell_mesh.msh")?;
    let coordinate = mesh.nodes().as_slice().to_vec();

    let mut model = SolidMechanicsModel::with_parser(
        SharedMesh::new(mesh),
        None,
        "solid_mechanics_model",
        ex.parser()?,
    )?;
    if ex.upstream_compat {
        model.physics.law_options = Some(LawOptions::upstream());
    }

    /* ---------------------------------------------------------------------- */
    /* Initialization                                                         */
    /* ---------------------------------------------------------------------- */
    model.init_full(&ex.model_options(AnalysisMethod::Static))?;
    ex.say(common::law_printout(model.material(0)?, &|_| None)?);

    let mut output_data = TextOutput::new(ex.output("material_viscoelastic_maxwell_output.out"));

    let time_step = 0.1;

    // Setting time step
    model.set_time_step(time_step, None)?;

    model.set_directory(&ex.paraview_dir())?;
    if ex.upstream_compat {
        // UD-110: upstream writes the matrix-valued `strain` as a
        // zero-filled 4 × 4 square per cell (invalid VTK); the compat
        // layout reproduces those bytes
        model
            .dumpers
            .dumpable
            .dumper_as::<akantu::io::DumperParaview>("solid_mechanics_model")?
            .set_compat_layout(true);
    }
    model.set_base_name("dynamic")?;
    model.add_dump_field_vector("displacement")?;
    model.add_dump_field("blocked_dofs")?;
    model.add_dump_field("external_force")?;
    model.add_dump_field("internal_force")?;
    model.add_dump_field("grad_u")?;
    model.add_dump_field("stress")?;
    model.add_dump_field("strain")?;

    // upstream `Int max_steps = sim_time / time_step + 1;` (truncation)
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let upstream_max_steps = (sim_time / time_step + 1.) as usize;
    let max_steps = ex.steps.map_or(upstream_max_steps, |s| {
        s.saturating_sub(1).min(upstream_max_steps)
    });
    let mut time = 0.;

    {
        let solver = model.solver.non_linear_solver_mut(None)?;
        solver.set_param("max_iterations", ParamValue::Int(10))?;
        solver.set_param("threshold", ParamValue::Real(1e-7))?;
        solver.set_param("convergence_type", ParamValue::Enum("residual".into()))?;
        if ex.upstream_compat {
            // UD-010: upstream returns early (0 iterations, no
            // `afterSolveStep`) when the first residual already converges,
            // as during the hold
            if let Some(nr) = solver.as_newton_raphson_mut() {
                nr.set_legacy_hooks(true);
            }
        }
    }

    let mut records = Vec::with_capacity(max_steps + 1);
    let mut newton = Vec::with_capacity(max_steps + 1);
    let mut converged = true;
    /* ---------------------------------------------------------------------- */
    /* Main loop                                                              */
    /* ---------------------------------------------------------------------- */
    for s in 0..=max_steps {
        ex.say(format!("Time Step = {}s", cout(time_step)));
        ex.say(format!("Time = {}", cout(time)));

        // impose displacement
        let epsilon = if time < t_ramp {
            eps * time / t_ramp
        } else {
            eps
        };

        {
            let mut disp_g = model.displacement_mut()?;
            let disp = disp_g.as_mut_slice();
            let mut block = Vec::with_capacity(disp.len());
            for (coord, u) in coordinate.chunks_exact(dim).zip(disp.chunks_exact_mut(dim)) {
                let eq = |a: Real, b: Real| are_float_equal(a, b, DEFAULT_TOLERANCE);
                if eq(coord[0], 0.0) {
                    u[0] = 0.;
                    u[1] = epsilon * coord[1];
                    block.push(true);
                } else if eq(coord[1], 0.0) {
                    u[0] = epsilon * coord[0];
                    u[1] = 0.;
                    block.push(true);
                } else if eq(coord[0], 0.001) {
                    // (upstream's two last branches are identical)
                    u[0] = epsilon * coord[0];
                    u[1] = epsilon * coord[1];
                    block.push(true);
                } else if eq(coord[1], 0.001) {
                    u[0] = epsilon * coord[0];
                    u[1] = epsilon * coord[1];
                    block.push(true);
                } else {
                    block.push(false);
                }
            }
            drop(disp_g);
            let mut blocked_g = model.blocked_dofs_mut()?;
            for (b, set) in blocked_g.as_mut_slice().chunks_exact_mut(dim).zip(block) {
                if set {
                    b.fill(true);
                }
            }
        }

        match model.solve_step(None) {
            Ok(()) => {}
            Err(Error::NotConverged { niter, error, .. }) => {
                ex.say(format!(
                    "Didn't converge after {niter} iterations. Error is {}",
                    cout(error)
                ));
                converged = false;
                break;
            }
            Err(e) => return Err(e),
        }

        // for debugging
        // auto int_force = model.getInternalForce();
        // auto &K = model.getDOFManager().getMatrix("K");
        // K.saveMatrix("K.mtx");

        let (nb_iter, error) = {
            let solver = model.solver.non_linear_solver(None)?;
            (
                number(&solver.get_param("nb_iterations")?)?,
                number(&solver.get_param("error")?)?,
            )
        };
        #[allow(clippy::cast_possible_truncation)] // an iteration count
        let nb_iter = nb_iter as i64;
        ex.say(format!("Converged in {nb_iter} iterations"));
        newton.push((nb_iter, error));

        if ex.dump {
            model.dump(None, None)?;
        }

        // upstream `mat.getEnergy(…)` of the only material
        let epot = model.energy("potential")?;
        let edis = model.energy("dissipated")?;
        let work = model.energy("work")?;

        // data output
        #[allow(clippy::cast_precision_loss)] // upstream `s * time_step`
        let t = s as Real * time_step;
        records.push([t, epsilon, epot, edis, work]);
        output_data.line(format!(
            "{} {} {} {} {}",
            cout(t),
            cout(epsilon),
            cout(epot),
            cout(edis),
            cout(work)
        ));
        time += time_step;
    }
    output_data.close()?;

    Ok(Outcome {
        model,
        records,
        newton,
        converged,
    })
}

fn main() -> ExitCode {
    common::main_with(|args| {
        let r = run(&example().with_args(args)?)?;
        if r.converged {
            Ok(())
        } else {
            Err(Error::invalid("a solve did not converge (EXIT_FAILURE)"))
        }
    })
}
