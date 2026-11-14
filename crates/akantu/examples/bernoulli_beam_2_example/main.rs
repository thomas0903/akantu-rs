// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): examples/c++/structural_mechanics_model/
// bernoulli_beam_2_example.cc.

//! `bernoulli_beam_2_example` (EX-028): a two-span 2-D Bernoulli beam
//! (`x = 0, 10, 18`, mesh built through the [`MeshAccessor`]) with two
//! sections, clamped at the first node and on rollers at the others; the
//! uniform load `q = 6000` of the first span as equivalent nodal forces and
//! moments, a moment `M = −3600` and the axial force `E₂A₂/18` on the last
//! node. One solve; prints the rotations `d1`, `d2`, the axial displacement
//! `d3` and the bending moments `M1`, `M2`, and dumps displacement,
//! rotation, force and momentum (`paraview/structural_mechanics_model.pvtu`).
//!
//! **UD-514 (D-003 B).** Upstream calls `initFull()` without an analysis
//! method, i.e. `_explicit_lumped_mass`: its `solveStep()` is one explicit
//! central-difference step with the lumped mass, which is zero upstream
//! (UD-005), instead of the static solution the example prints. The port
//! runs `_static` by default; `--upstream-compat` keeps upstream's
//! `_explicit_lumped_mass`: upstream then divides by its zero lumped mass
//! and prints `-nan` five times; the port's lumped mass (`ρAL/2` per
//! translational DOF, UD-005 corrected) is zero as well here (the sections
//! have no density), and the port refuses the step with an error.
//!
//! ```text
//! cargo run -p akantu --example bernoulli_beam_2_example [-- --solver eigen]
//! ```

#[path = "../common/mod.rs"]
pub mod common;

use std::process::ExitCode;

use akantu::core::enums::AnalysisMethod;
use akantu::core::{ElementType, GhostType, Real, Result, to_idx};
use akantu::mesh::{Mesh, MeshAccessor, SharedMesh};
use akantu::structural::{StructuralMaterial, StructuralMechanicsModel};
use common::{Example, cout};

/// Element type of the example (upstream `#define TYPE _bernoulli_beam_2`).
pub const TYPE: ElementType = ElementType::BernoulliBeam2;

/// Result of a run.
#[derive(Debug)]
pub struct Outcome {
    /// The model in its final state.
    pub model: StructuralMechanicsModel,
    /// The printed values `[d1, d2, d3, M1, M2]`.
    pub values: [Real; 5],
}

/// Default configuration (upstream directory; no input file: upstream
/// calls `initialize(argc, argv)`).
#[must_use]
pub fn example() -> Example {
    Example::new("bernoulli_beam_2_example", "structural_mechanics_model", "")
}

/// The analysis method of the run: `_static` (UD-514 corrected) or, with
/// `--upstream-compat`, upstream's default `_explicit_lumped_mass`.
#[must_use]
pub fn analysis_method(ex: &Example) -> AnalysisMethod {
    if ex.upstream_compat {
        AnalysisMethod::ExplicitLumpedMass
    } else {
        AnalysisMethod::Static
    }
}

/// The upstream `main` after `initialize`.
///
/// # Errors
/// Any error of the mesh, the model, the solver or the dumper.
pub fn run(ex: &Example) -> Result<Outcome> {
    ex.create_output_dir()?;
    // Defining the mesh
    let mut beams = Mesh::new(2, "mesh")?;

    let q = 6000.;
    let l = 10.;
    let m = -3600.; // Momentum at 3

    let nb_nodes = 3;
    let nb_element = nb_nodes - 1;

    {
        let mut mesh_accessor = MeshAccessor::new(&mut beams);
        mesh_accessor.resize_nodes(nb_nodes)?;

        mesh_accessor.resize_connectivity(nb_element, TYPE, GhostType::NotGhost)?;

        {
            let mut nodes = mesh_accessor.nodes_mut();
            let x = nodes.as_mut_slice();
            x.fill(0.);
            x[2] = 10.; // nodes(1, 0)
            x[4] = 18.; // nodes(2, 0)
        }

        {
            let mut connectivity = mesh_accessor.connectivity_mut(TYPE, GhostType::NotGhost);
            let c = connectivity.as_mut_slice();
            for i in 0..nb_element {
                c[2 * i] = to_idx(i);
                c[2 * i + 1] = to_idx(i + 1);
            }
        }

        mesh_accessor.make_ready()?;
    }

    // Defining the materials
    let mut model =
        StructuralMechanicsModel::new(SharedMesh::new(beams), None, "structural_mechanics_model")?;

    let mat1 = StructuralMaterial {
        e: 3e10,
        i: 0.0025,
        a: 0.01,
        ..StructuralMaterial::default()
    };

    model.add_material(mat1, "")?;

    let mat2 = StructuralMaterial {
        e: 3e10,
        i: 0.00128,
        a: 0.01,
        ..StructuralMaterial::default()
    };

    model.add_material(mat2, "")?;

    // Defining the forces
    model.init_full(&ex.model_options(analysis_method(ex)))?;

    {
        let mut boundary = model.blocked_dofs_mut()?;
        boundary.as_mut_slice().fill(false);
    }
    model.external_force_mut()?.as_mut_slice().fill(0.);
    model.displacement_mut()?.as_mut_slice().fill(0.);

    model
        .element_material_mut(TYPE, GhostType::NotGhost)?
        .as_mut_slice()[1] = 1;

    {
        let mut forces_g = model.external_force_mut()?;
        let forces = forces_g.as_mut_slice();
        forces[1] = -q * l / 2.; // forces(0, 1)
        forces[2] = -q * l * l / 12.; // forces(0, 2)
        forces[4] = -q * l / 2.; // forces(1, 1)
        forces[5] = q * l * l / 12.; // forces(1, 2)
        forces[8] = m; // forces(2, 2)
        forces[6] = mat2.e * mat2.a / 18.; // forces(2, 0)
    }

    // Defining the boundary conditions
    {
        let mut boundary_g = model.blocked_dofs_mut()?;
        let boundary = boundary_g.as_mut_slice();
        boundary[0] = true; // (0, 0)
        boundary[1] = true; // (0, 1)
        boundary[2] = true; // (0, 2)
        boundary[4] = true; // (1, 1)
        boundary[7] = true; // (2, 1)
    }

    model.set_directory(&ex.paraview_dir())?;
    model.add_dump_field_vector("displacement")?;
    model.add_dump_field("rotation")?;
    model.add_dump_field_vector("force")?;
    model.add_dump_field("momentum")?;

    model.solve_step(None)?;

    model.assemble_residual()?;

    // Post-Processing
    let values = {
        let displacement = model.displacement()?;
        let u = displacement.as_slice();
        let n_m = model.stress(TYPE, GhostType::NotGhost)?;
        let s = n_m.as_slice();
        let ns = n_m.nb_component();
        [
            u[5],                           // displacement(1, 2)
            u[8],                           // displacement(2, 2)
            u[3],                           // displacement(1, 0)
            s[1],                           // N_M(0, 1)
            s[2 * (nb_nodes - 2) * ns + 1], // N_M(2 * (nb_nodes - 2), 1)
        ]
    };
    ex.say(format!(" d1 = {}", cout(values[0])));
    ex.say(format!(" d2 = {}", cout(values[1])));
    ex.say(format!(" d3 = {}", cout(values[2])));
    ex.say(format!(" M1 = {}", cout(values[3])));
    ex.say(format!(" M2 = {}", cout(values[4])));

    if ex.dump {
        model.dump(None, None)?;
    }

    Ok(Outcome { model, values })
}

fn main() -> ExitCode {
    common::main_with(|args| run(&example().with_args(args)?).map(|_| ()))
}
