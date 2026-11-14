// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): examples/c++/solid_mechanics_cohesive_model/
// cohesive_extrinsic/cohesive_extrinsic.cc (+ material.dat, triangle.msh).

//! `cohesive_extrinsic` (EX-015): extrinsic cohesive fracture of the
//! `[-1, 1]²` plate of quadratic triangles (upstream's committed
//! `triangle.msh`, copied next to this file): elastic `steel` and a
//! `cohesive_linear` law (`G_c = 100`, `σ_c = 100`), insertion restricted to
//! the facets whose barycentre lies in `0.20 ≤ y ≤ 0.30`; the outer nodes
//! are blocked in their normal direction, an initial velocity
//! `v_y = 0.5 y` stretches the plate and the top and bottom nodes move by
//! `0.5 Δt y` per step. 1000 explicit steps (`Δt = 0.05 Δt_crit`), each
//! preceded by the insertion check (`checkCohesiveStress`); ParaView dumps
//! of displacement, velocity, acceleration, internal force, stress and
//! `grad_u` every 10 steps (`paraview/extrinsic_*`). The run passes when
//! the dissipated energy is `200 √2` (four cracked diagonals of length
//! `√2 / 2`, `G_c = 100`) within 0.1 %, as upstream checks.
//!
//! ```text
//! cargo run -p akantu --release --example cohesive_extrinsic [-- --output-dir <dir>]
//! ```

#[path = "../cohesive_common/mod.rs"]
pub mod cohesive_common;
#[path = "../common/mod.rs"]
pub mod common;

use std::process::ExitCode;

use akantu::cohesive::{CohesiveUpstreamCompat, SolidMechanicsModelCohesive};
use akantu::core::enums::AnalysisMethod;
use akantu::core::{Error, Real, Result};
use akantu::mesh::SharedMesh;
use cohesive_common::{EnergyRecord, Transcript};
use common::{Example, cout};

/// Spatial dimension of the example.
pub const DIM: usize = 2;

/// Result of a run.
#[derive(Debug)]
pub struct Outcome {
    /// The model in its final state.
    pub model: SolidMechanicsModelCohesive,
    /// The mesh (with the inserted cohesive elements).
    pub mesh: SharedMesh,
    /// Time step (`0.05 Δt_crit`).
    pub time_step: Real,
    /// Cohesive elements inserted by the check of each step.
    pub inserted: Vec<usize>,
    /// Final dissipated energy (upstream `Ed`).
    pub dissipated: Real,
    /// The expected dissipated energy `200 √2` (upstream `Edt`).
    pub expected: Real,
    /// Per-step energies (only with [`Options::record_energies`]).
    pub energies: Vec<EnergyRecord>,
    /// Standard output.
    pub stdout: Vec<String>,
}

/// Run options beyond the common ones.
#[derive(Clone, Copy, Debug, Default)]
pub struct Options {
    /// Record the energies after every step (the quantities of the oracle
    /// variant `cohesive_extrinsic_full`; read-only queries).
    pub record_energies: bool,
}

/// Default configuration (upstream directory and input file; the mesh is
/// upstream's committed `triangle.msh`, copied next to this file).
#[must_use]
pub fn example() -> Example {
    let mut ex = Example::new(
        "cohesive_extrinsic",
        "solid_mechanics_cohesive_model/cohesive_extrinsic",
        "material.dat",
    );
    ex.mesh_dir = ex.source_dir.clone();
    ex
}

/// Nodes on the top and bottom sides (`|y| > 0.99`; upstream tests
/// `pos(_y) > 0.99 or pos(_y) < -0.99`).
fn on_top_or_bottom(y: Real) -> bool {
    y > 0.99 || y < -0.99
}

/// The upstream `main` after `initialize`.
///
/// # Errors
/// Any error of the model, the mesh or the outputs; the dissipated energy
/// outside `200 √2 · [0.999, 1.001]` (upstream returns −1).
pub fn run(ex: &Example) -> Result<Outcome> {
    run_with(ex, Options::default())
}

/// [`run`] with `options`.
///
/// # Errors
/// See [`run`].
pub fn run_with(ex: &Example, options: Options) -> Result<Outcome> {
    ex.create_output_dir()?;
    let mut out = Transcript::new(ex.verbose);
    let dim = DIM;
    let max_steps = ex.steps_or(1000);

    let mesh = SharedMesh::new(ex.read_mesh(dim, "triangle.msh")?);
    let mut model = SolidMechanicsModelCohesive::with_parser(
        mesh.clone(),
        None,
        "solid_mechanics_model_cohesive",
        ex.parser()?,
    )?;
    if ex.upstream_compat {
        // D-003: every upstream cohesive defect (UD-217, UD-040, …)
        model.ext.compat = CohesiveUpstreamCompat::upstream();
    }

    // model initialization
    model.init_full(&ex.model_options(AnalysisMethod::ExplicitLumpedMass), true)?;

    let time_step = model.stable_time_step()? * 0.05;
    model.set_time_step(time_step, None)?;
    out.line(format!("Time step: {}", cout(time_step)));

    // updated the insertion limits
    model.ext.inserter.set_limit(1, 0.30, 0.20)?;
    model.update_automatic_insertion()?;

    // boundary conditions
    {
        let position = mesh.read()?.nodes().as_slice().to_vec();
        let mut boundary = model.smm.blocked_dofs_mut()?;
        for (pos, boun) in position
            .chunks_exact(dim)
            .zip(boundary.as_mut_slice().chunks_exact_mut(dim))
        {
            if on_top_or_bottom(pos[1]) {
                boun[1] = true;
            }
            if pos[0] > 0.99 || pos[0] < -0.99 {
                boun[0] = true;
            }
        }
    }

    model.smm.set_directory(&ex.paraview_dir())?;
    model.smm.set_base_name("extrinsic")?;
    model.smm.add_dump_field_vector("displacement")?;
    model.smm.add_dump_field("velocity")?;
    model.smm.add_dump_field("acceleration")?;
    model.smm.add_dump_field("internal_force")?;
    model.smm.add_dump_field("stress")?;
    model.smm.add_dump_field("grad_u")?;
    if ex.dump {
        model.smm.dump(None, None)?;
    }

    // initial conditions
    let loading_rate = 0.5;
    let disp_update = loading_rate * time_step;
    {
        let position = mesh.read()?.nodes().as_slice().to_vec();
        let mut velocity = model.smm.velocity_mut()?;
        for (pos, vel) in position
            .chunks_exact(dim)
            .zip(velocity.as_mut_slice().chunks_exact_mut(dim))
        {
            vel[1] = loading_rate * pos[1];
        }
    }

    // Main loop
    let mut inserted = Vec::with_capacity(max_steps);
    let mut energies = Vec::new();
    for s in 1..=max_steps {
        // update displacement on extreme nodes (the current nodes: the
        // inserted cohesive elements double some)
        {
            let position = mesh.read()?.nodes().as_slice().to_vec();
            let mut displacement = model.smm.displacement_mut()?;
            for (pos, disp) in position
                .chunks_exact(dim)
                .zip(displacement.as_mut_slice().chunks_exact_mut(dim))
            {
                if on_top_or_bottom(pos[1]) {
                    disp[1] += disp_update * pos[1];
                }
            }
        }

        // check wether cohesive elements should be inserted
        inserted.push(model.check_cohesive_stress()?);
        model.solve_step(None)?;

        if options.record_energies {
            energies.push(EnergyRecord::of(&mut model, s)?);
        }
        if s % 10 == 0 {
            if ex.dump {
                model.smm.dump(None, None)?;
            }
            out.line(format!("passing step {s}/{max_steps}"));
        }
    }

    let dissipated = model.energy("dissipated")?;
    let expected = 200. * Real::sqrt(2.);
    out.line(format!("{} {}", cout(dissipated), cout(expected)));
    let complete = max_steps == 1000;
    if complete
        && (dissipated < expected * 0.999 || dissipated > expected * 1.001 || dissipated.is_nan())
    {
        out.line("The dissipated energy is incorrect");
        return Err(Error::invalid(format!(
            "cohesive_extrinsic: dissipated energy {dissipated} instead of {expected}"
        )));
    }
    Ok(Outcome {
        model,
        mesh,
        time_step,
        inserted,
        dissipated,
        expected,
        energies,
        stdout: out.lines,
    })
}

fn main() -> ExitCode {
    common::main_with(|args| run(&example().with_args(args)?).map(|_| ()))
}
