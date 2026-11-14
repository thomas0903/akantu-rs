// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): examples/c++/node_to_node/steady_state.cc
// (+ ras_ss.in, ntn_test_ras.geo).

//! `steady_state` (EX-009): node-to-node (traction at split node) frictional
//! contact of an elastic slider on an elastic base, periodic in x, with
//! rate-and-state friction (revised Dieterich law, regularised ageing). The
//! slider is pressed on the base by tractions `∓1 MPa` and both are set to
//! the analytical static state and to the steady sliding velocity
//! `±v/2`; the top and bottom are then driven at `±v/2` for 1000 explicit
//! steps (`Δt = 0.01 Δt_crit`, recomputed every step as upstream does), and
//! the friction state is perturbed sinusoidally (1 %) along the interface
//! at step 10. Writes the energies of every step to
//! `friction-energies-steady_state.csv` and dumps every 10 steps
//! (`paraview/steady_state_<n>.pvtu`). Parameters from the `user
//! parameters` section of `ras_ss.in`.
//!
//! The upstream package (`traction_at_split_node_contact`) is an extra
//! package, off by default; the example uses the crate `akantu-ntn-contact`
//! (a dev-dependency of the facade, like the manual's chapter).
//! `--upstream-compat` selects every upstream defect of the package
//! (`TasnUpstreamCompat::upstream()`). They are not reached here: **the
//! default run reproduces upstream** bitwise (the oracle comparison is
//! tested in both modes).
//!
//! ```text
//! cargo run -p akantu --release --example steady_state [-- --output-dir <dir>]
//! ```

#[path = "../common/mod.rs"]
pub mod common;

use std::f64::consts::PI;
use std::process::ExitCode;

use akantu::core::enums::SpatialDirection;
use akantu::core::{Real, Result, try_to_usize};
use akantu::mesh::SharedMesh;
use akantu::model::boundary_condition::FromTraction;
use akantu::parser::SearchScope;
use akantu::parser::param::{ParamValue, ParameterRegistry};
use akantu::solid::SolidMechanicsModel;
use akantu_ntn_contact::{NtnContactSolverCallback, TasnUpstreamCompat};
use common::{Example, TextOutput, cout};

/// One line of the energies file: `[time, ekin, epot, work, econ, efri,
/// tot]`, plus the time step of the step.
pub type Record = [Real; 8];

/// Result of a run.
#[derive(Debug)]
pub struct Outcome {
    /// The model in its final state.
    pub model: SolidMechanicsModel,
    /// The contact solver callback in its final state.
    pub solver: NtnContactSolverCallback,
    /// The energies of every step.
    pub records: Vec<Record>,
}

/// Default configuration (upstream directory and input file).
#[must_use]
pub fn example() -> Example {
    Example::new("steady_state", "node_to_node", "ras_ss.in")
}

/// A real parameter value (integers converted).
fn real(v: &ParamValue) -> Result<Real> {
    v.as_real()
}

/// A non-negative integer parameter of the user section.
fn count(data: &akantu::parser::ParserSection, name: &str) -> Result<usize> {
    let v: i64 = data.get_parameter_value(name, SearchScope::Current)?;
    usize::try_from(v).map_err(|_| akantu::core::Error::invalid(format!("{name} = {v}")))
}

/// The upstream `main` (upstream ignores the command line; the port takes
/// the common options).
///
/// # Errors
/// Any error of the input file, the mesh, the model, the contact, the
/// solver or the outputs.
pub fn run(ex: &Example) -> Result<Outcome> {
    run_with(ex, |_, _| Ok(()))
}

/// [`run`] calling `on_step(s, &model)` after the solve and the energies of
/// every step `s` (before its dump).
///
/// # Errors
/// Any error of [`run`] or of `on_step`.
#[allow(clippy::too_many_lines)] // the upstream main, line by line
pub fn run_with(
    ex: &Example,
    mut on_step: impl FnMut(usize, &SolidMechanicsModel) -> Result<()>,
) -> Result<Outcome> {
    ex.create_output_dir()?;
    // TODO read this from input file
    let output_folder = "steady_state";

    let parser = ex.parser()?;
    let data = parser.user_section()?;
    let spatial_dimension = count(data, "spatial_dimension")?;
    let dump_every = count(data, "dump_every")?;
    let mut mesh = ex.read_mesh(spatial_dimension, "ntn_test_ras.msh")?;

    // no consumer is registered on the mesh yet: the events reach nobody
    let _ = akantu::mesh::periodic::make_periodic(
        &mut mesh,
        SpatialDirection::X,
        Some(("slider_left", "slider_right")),
    )?;
    let _ = akantu::mesh::periodic::make_periodic(
        &mut mesh,
        SpatialDirection::X,
        Some(("base_left", "base_right")),
    )?;
    let (lower_bounds, upper_bounds) = (mesh.lower_bounds().to_vec(), mesh.upper_bounds().to_vec());

    let mut model = SolidMechanicsModel::with_parser(
        SharedMesh::new(mesh),
        None,
        "solid_mechanics_model",
        parser.clone(),
    )?;

    let time_step_factor: Real =
        data.get_parameter_value("time_step_factor", SearchScope::Current)?;

    let normal_dir = 1;

    let compat = if ex.upstream_compat {
        TasnUpstreamCompat::upstream()
    } else {
        TasnUpstreamCompat::default()
    };
    let mut solver_ntn = NtnContactSolverCallback::new(
        &mut model,
        "slider_bottom",
        "base_top",
        normal_dir,
        time_step_factor,
        compat,
    )?;

    let (e, nu) = {
        let mat = model.material_by_name("slider")?;
        (real(&mat.get_param("E")?)?, real(&mat.get_param("nu")?)?)
    };

    let shear_vel: Real = data.get_parameter_value("shear_velocity", SearchScope::Current)?;
    let trac_top: Vec<Real> = data.get_parameter_value("top_traction", SearchScope::Current)?;
    let trac_bottom: Vec<Real> = data.get_parameter_value("bot_traction", SearchScope::Current)?;

    model.set_directory(&ex.paraview_dir())?;
    model.set_base_name(output_folder)?;
    model.add_dump_field("blocked_dofs")?;
    model.add_dump_field("mass")?;
    model.add_dump_field_vector("velocity")?;
    model.add_dump_field_vector("acceleration")?;
    model.add_dump_field_vector("displacement")?;
    model.add_dump_field_vector("internal_force")?;
    model.add_dump_field_vector("external_force")?;

    // Static analytical solution
    let fss: Real = data.get_parameter_value("fss", SearchScope::Current)?;
    let shear_modulus = e / (2. * (1. + nu));
    let normal_strain_applied = trac_top[1] / e - nu * nu * trac_top[1] / e;

    {
        let position = {
            let mesh_h = model.physics.base.mesh.clone();
            let m = mesh_h.read()?;
            m.nodes().as_slice().to_vec()
        };
        let mut displacement_g = model.displacement_mut()?;
        let displacement = displacement_g.as_mut_slice();
        let nb_nodes = position.len() / spatial_dimension;
        for n in 0..nb_nodes {
            let y = position[n * spatial_dimension + 1];
            displacement[n * spatial_dimension] = fss * -trac_top[1] / (shear_modulus) * y;
            displacement[n * spatial_dimension + 1] = normal_strain_applied * y;
        }
    }

    // Set boundary conditions for dynamic simulation
    model.apply_neumann_bc(&FromTraction::new(trac_top.clone()), "slider_top")?;
    model.apply_neumann_bc(&FromTraction::new(trac_bottom), "base_bottom")?;

    ///// Set to steady state
    let (slider_nodes, base_nodes) = {
        let mesh_h = model.physics.base.mesh.clone();
        let m = mesh_h.read()?;
        let g = m.groups();
        (
            g.element_group("slider")?.nodes().to_vec(),
            g.element_group("base")?.nodes().to_vec(),
        )
    };

    // Specify initial nodal velocity
    let dt = model.time_step(None)?;
    {
        let mut velo_g = model.velocity_mut()?;
        let velo = velo_g.as_mut_slice();
        for &n in &slider_nodes {
            velo[try_to_usize(n)? * spatial_dimension] = 0.5 * shear_vel;
        }
        for &n in &base_nodes {
            velo[try_to_usize(n)? * spatial_dimension] = -0.5 * shear_vel;
        }
    }
    {
        let mut increment_g = model.increment_mut()?;
        let increment = increment_g.as_mut_slice();
        for &n in &slider_nodes {
            increment[try_to_usize(n)? * spatial_dimension] = 0.5 * shear_vel * dt;
        }
        for &n in &base_nodes {
            increment[try_to_usize(n)? * spatial_dimension] = -0.5 * shear_vel * dt;
        }
    }

    {
        let friction = solver_ntn.friction_mut();
        let phi = real(&friction.get_param("friction_state")?)?;
        let nb_contact_nodes = friction.is_sticking().len();
        for n in 0..nb_contact_nodes {
            friction.is_sticking_mut().set_at(n, false);

            friction.update_friction_state(n, phi)?;
        }
    }
    //////

    // Time of the simulation
    let stable_time_step = model.stable_time_step()?;
    let time_step = stable_time_step * time_step_factor;
    model.set_time_step(time_step, None)?;
    let nb_steps = ex.steps_or(count(data, "nb_steps")?);

    if ex.dump {
        model.dump(None, None)?;
    }

    // upstream `std::filesystem::path(output_folder).replace_extension("csv")`
    let file_name = format!("friction-energies-{output_folder}.csv");
    let mut energies = TextOutput::new(ex.output(&file_name));

    energies.line("time,ekin,epot,work,econ,efri,tot");

    let mut einit = 0.;

    let mut records = Vec::with_capacity(nb_steps);
    for s in 0..nb_steps {
        // Apply velocity
        let position = {
            let mesh_h = model.physics.base.mesh.clone();
            let m = mesh_h.read()?;
            m.nodes().as_slice().to_vec()
        };
        let nb_nodes = position.len() / spatial_dimension;
        let top = upper_bounds[1];
        let bottom = lower_bounds[1];
        let stable_time_step = model.stable_time_step()?;
        let time_step = stable_time_step * time_step_factor;
        let disp_incr = shear_vel * time_step;
        {
            let mut velo_g = model.velocity_mut()?;
            let velo = velo_g.as_mut_slice();
            for n in 0..nb_nodes {
                let y = position[n * spatial_dimension + 1];
                if (y - top).abs() < 1e-6 {
                    velo[n * spatial_dimension] = 0.5 * shear_vel;
                }
                if (y - bottom).abs() < 1e-6 {
                    velo[n * spatial_dimension] = -0.5 * shear_vel;
                }
            }
        }

        {
            let mut blocked = model.blocked_dofs()?.as_slice().to_vec();
            {
                let mut displacement_g = model.displacement_mut()?;
                let displacement = displacement_g.as_mut_slice();
                for n in 0..nb_nodes {
                    let y = position[n * spatial_dimension + 1];
                    if (y - top).abs() < 1e-6 {
                        displacement[n * spatial_dimension] += 0.5 * disp_incr;
                        blocked[n * spatial_dimension] = true;
                    }
                    if (y - bottom).abs() < 1e-6 {
                        displacement[n * spatial_dimension] += -0.5 * disp_incr;
                        blocked[n * spatial_dimension] = true;
                    }
                }
            }
            model
                .blocked_dofs_mut()?
                .as_mut_slice()
                .copy_from_slice(&blocked);
        }

        solver_ntn.solve_step(&mut model, Some("explicit_lumped"))?;

        let ekin = model.energy("kinetic")?;
        let epot = model.energy("potential")?;
        let work = model.energy("external work new")?;
        let econ = solver_ntn.external_work();
        if s == 0 {
            einit = ekin + epot - (work + econ[0] + econ[1]);
        }
        #[allow(clippy::cast_precision_loss)] // upstream `s * time_step`
        let t = s as Real * time_step;
        let tot = ekin + epot - (work + econ[0] + econ[1]) - einit;
        records.push([t, ekin, epot, work, econ[0], econ[1], tot, time_step]);
        energies.line(format!(
            "{},{},{},{},{},{},{}",
            cout(t),
            cout(ekin),
            cout(epot),
            cout(work),
            cout(econ[0]),
            cout(econ[1]),
            cout(tot)
        ));

        on_step(s, &model)?;

        if s.is_multiple_of(dump_every) && ex.dump {
            model.dump(None, None)?;
        }

        if s == 10 {
            let left = lower_bounds[0];
            let right = upper_bounds[0];
            let contact_l = right - left;
            let amplitude = 0.01;
            let cur_pos = model.current_position()?.as_slice().to_vec();
            let slaves = solver_ntn.contact().slaves().as_slice().to_vec();

            let nb_contact_nodes = solver_ntn.nb_contact_nodes();
            let friction = solver_ntn.friction_mut();
            for (n, &slave) in slaves.iter().enumerate().take(nb_contact_nodes) {
                // upstream `cur_pos(slave)`: component 0 of the node
                let x = cur_pos[try_to_usize(slave)? * spatial_dimension];
                let value = friction.friction_state().at(n)
                    * (1. + amplitude * (2. * PI / contact_l * x).sin());

                friction.update_friction_state(n, value)?;
            }
        }
    }

    energies.close()?;
    Ok(Outcome {
        model,
        solver: solver_ntn,
        records,
    })
}

fn main() -> ExitCode {
    common::main_with(|args| run(&example().with_args(args)?).map(|_| ()))
}
