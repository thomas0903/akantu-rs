// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): examples/c++/solid_mechanics_cohesive_model/
// cohesive_intrinsic/cohesive_intrinsic.cc (+ material.dat, triangle.msh).

//! `cohesive_intrinsic` (EX-017): intrinsic cohesive elements
//! (`cohesive_bilinear`, `σ_c = 1`, `G_c = 1`, `δ_0 = 0.1`) inserted at
//! initialisation on the facets of upstream's committed `triangle.msh`
//! (quadratic triangles, copied next to this file) whose barycentre lies in
//! `−0.26 ≤ x ≤ −0.24`. Every degree of freedom is blocked; the nodes of
//! the elements right of the crack (`x_barycentre > −0.25`) are moved by
//! `0.01` along x before the first step and after every step. 350
//! explicit steps (`Δt = 0.8 Δt_crit`) with a ParaView dump of
//! displacement, velocity, acceleration, `grad_u`, external and internal
//! forces after every step (`paraview/intrinsic_*`). The run passes when
//! the dissipated energy is `2 √2` (four cracked diagonals of length
//! `√2 / 2`, `G_c = 1`) within 0.1 %, as upstream checks.
//!
//! The bilinear law is accepted for intrinsic insertion: it initialises its
//! effective strength (UD-224 refuses only the laws without it).
//!
//! ```text
//! cargo run -p akantu --release --example cohesive_intrinsic [-- --output-dir <dir>]
//! ```

#[path = "../cohesive_common/mod.rs"]
pub mod cohesive_common;
#[path = "../common/mod.rs"]
pub mod common;

use std::process::ExitCode;

use akantu::cohesive::{CohesiveUpstreamCompat, SolidMechanicsModelCohesive};
use akantu::core::enums::AnalysisMethod;
use akantu::core::{ElementKind, Error, GhostType, Idx, Real, Result, TypeFilter};
use akantu::mesh::{Mesh, SharedMesh};
use cohesive_common::{EnergyRecord, Transcript};
use common::{Example, cout};

/// Spatial dimension of the example.
pub const DIM: usize = 2;

/// Result of a run.
#[derive(Debug)]
pub struct Outcome {
    /// The model in its final state.
    pub model: SolidMechanicsModelCohesive,
    /// The mesh (with the cohesive elements inserted at initialisation).
    pub mesh: SharedMesh,
    /// Time step (`0.8 Δt_crit`).
    pub time_step: Real,
    /// Nodes of upstream's element group `diplacement` (sic), in the
    /// group's order.
    pub moved_nodes: Vec<Idx>,
    /// Final dissipated energy (upstream `Ed`).
    pub dissipated: Real,
    /// The expected dissipated energy `2 √2` (upstream `Edt`).
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
    /// variant `cohesive_intrinsic_full`; read-only queries).
    pub record_energies: bool,
}

/// Default configuration (upstream directory and input file; the mesh is
/// upstream's committed `triangle.msh`, copied next to this file).
#[must_use]
pub fn example() -> Example {
    let mut ex = Example::new(
        "cohesive_intrinsic",
        "solid_mechanics_cohesive_model/cohesive_intrinsic",
        "material.dat",
    );
    ex.mesh_dir = ex.source_dir.clone();
    ex
}

/// Upstream's element group `diplacement`: `for_each_element(mesh, …,
/// _element_kind = _ek_regular)` (regular elements of the mesh dimension,
/// both ghost types) adds every element whose barycentre has `x > −0.25`
/// with its nodes; the node group keeps each node once, in the order of
/// first appearance (`addNode(node, check_for_duplicate = true)`).
///
/// # Errors
/// Mesh errors.
pub fn moved_nodes(mesh: &Mesh) -> Result<Vec<Idx>> {
    let mut nodes: Vec<Idx> = Vec::new();
    let mut seen = vec![false; mesh.nb_nodes()];
    for ghost in GhostType::ALL {
        let types: Vec<_> = mesh
            .element_types(TypeFilter {
                spatial_dimension: Some(mesh.spatial_dimension()),
                ghost_type: ghost,
                kind: Some(ElementKind::Regular),
            })
            .collect();
        for ty in types {
            let barycenters = mesh.barycenters(ty, ghost)?;
            let conn = mesh.connectivity(ty, ghost)?;
            let npe = conn.nb_component();
            for (e, bary) in barycenters
                .as_slice()
                .chunks_exact(mesh.spatial_dimension())
                .enumerate()
            {
                if bary[0] > -0.25 {
                    for &n in &conn.as_slice()[e * npe..(e + 1) * npe] {
                        let k = akantu::core::try_to_usize(n)?;
                        if !seen[k] {
                            seen[k] = true;
                            nodes.push(n);
                        }
                    }
                }
            }
        }
    }
    Ok(nodes)
}

/// Upstream `updateDisplacement`: `u_x += increment` on the group's nodes.
fn update_displacement(
    model: &mut SolidMechanicsModelCohesive,
    nodes: &[Idx],
    increment: Real,
) -> Result<()> {
    let mut displacement = model.smm.displacement_mut()?;
    let u = displacement.as_mut_slice();
    for &node in nodes {
        u[akantu::core::try_to_usize(node)? * DIM] += increment;
    }
    Ok(())
}

/// The upstream `main` after `initialize`.
///
/// # Errors
/// Any error of the model, the mesh or the outputs; the dissipated energy
/// outside `2 √2 · [0.999, 1.001]` (upstream returns −1).
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
    let max_steps = ex.steps_or(350);

    let mesh = SharedMesh::new(ex.read_mesh(DIM, "triangle.msh")?);
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
    // To restric the insertion to the range [-0.26, -0.24] in the x direction
    model.ext.inserter.set_limit(0, -0.26, -0.24)?;

    // model initialization (intrinsic: the cohesive elements are inserted
    // here)
    model.init_full(&ex.model_options(AnalysisMethod::ExplicitLumpedMass), false)?;

    let time_step = model.stable_time_step()? * 0.8;
    model.set_time_step(time_step, None)?;
    out.line(format!("Time step: {}", cout(time_step)));

    // boundary conditions
    model.smm.blocked_dofs_mut()?.as_mut_slice().fill(true);

    model.smm.set_directory(&ex.paraview_dir())?;
    model.smm.set_base_name("intrinsic")?;
    model.smm.add_dump_field_vector("displacement")?;
    model.smm.add_dump_field("velocity")?;
    model.smm.add_dump_field("acceleration")?;
    model.smm.add_dump_field("grad_u")?;
    model.smm.add_dump_field("external_force")?;
    model.smm.add_dump_field("internal_force")?;
    if ex.dump {
        model.smm.dump(None, None)?;
    }

    // update displacement
    let nodes = moved_nodes(&*mesh.read()?)?;
    let increment = 0.01;
    update_displacement(&mut model, &nodes, increment)?;

    // Main loop
    let mut energies = Vec::new();
    for s in 1..=max_steps {
        model.solve_step(None)?;
        if options.record_energies {
            energies.push(EnergyRecord::of(&mut model, s)?);
        }

        update_displacement(&mut model, &nodes, increment)?;
        if ex.dump {
            model.smm.dump(None, None)?;
        }
        out.line(format!("passing step {s}/{max_steps}"));
    }

    let dissipated = model.energy("dissipated")?;
    let expected = 2. * Real::sqrt(2.);
    out.line(format!("{} {}", cout(dissipated), cout(expected)));
    if max_steps == 350
        && (dissipated < expected * 0.999 || dissipated > expected * 1.001 || dissipated.is_nan())
    {
        out.line("The dissipated energy is incorrect");
        return Err(Error::invalid(format!(
            "cohesive_intrinsic: dissipated energy {dissipated} instead of {expected}"
        )));
    }
    Ok(Outcome {
        model,
        mesh,
        time_step,
        moved_nodes: nodes,
        dissipated,
        expected,
        energies,
        stdout: out.lines,
    })
}

fn main() -> ExitCode {
    common::main_with(|args| run(&example().with_args(args)?).map(|_| ()))
}
