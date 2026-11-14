// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): examples/c++/solid_mechanics_model/
// new_material/new_local_material.cc (+ material.dat, barre_trou.geo).

//! `new_local_material` (EX-023): how to write a new material — the
//! local damage law of [`local_material_damage`], implemented with the
//! [`akantu::solid::Material`] trait and registered in the model's
//! material factory under `local_damage` before `init_full`, so that the
//! input file's `material local_damage [ … ]` section instantiates it.
//!
//! A plate with a hole (quadratic triangles), `u_x = 0` on `Fixed_x`,
//! `u_y = 0` on `Fixed_y`, traction from the stress `300 I` on `Traction`,
//! explicit dynamics with `Δt = Δt_crit / 10`, 10 000 steps; prints the
//! energies every 100 steps and dumps every 1000 steps
//! (`paraview/local_material_<n>.pvtu`, with the `damage` internal when a
//! material of the model has it).
//!
//! `--upstream-compat` reproduces upstream's potential energy density
//! (UD-472, see [`local_material_damage`]).
//!
//! ```text
//! cargo run -p akantu --release --example new_local_material [-- --output-dir <dir>]
//! ```

#[path = "../common/mod.rs"]
pub mod common;
#[path = "local_material_damage.rs"]
pub mod local_material_damage;

use std::process::ExitCode;

use akantu::core::enums::{AnalysisMethod, SpatialDirection};
use akantu::core::tensor::MatrixX;
use akantu::core::{Real, Result};
use akantu::mesh::SharedMesh;
use akantu::model::boundary_condition::{FixedValue, FromStress};
use akantu::solid::SolidMechanicsModel;
use common::{Example, cout};

/// Result of a run.
#[derive(Debug)]
pub struct Outcome {
    /// The model in its final state.
    pub model: SolidMechanicsModel,
    /// Time step used (`Δt_crit / 10`).
    pub time_step: Real,
    /// `[epot, ekin]` after every step.
    pub energies: Vec<[Real; 2]>,
}

/// Default configuration (upstream directory and input file).
#[must_use]
pub fn example() -> Example {
    Example::new(
        "new_local_material",
        "solid_mechanics_model/new_material",
        "material.dat",
    )
}

/// The upstream `main` after `initialize`.
///
/// # Errors
/// Any error of the model, the mesh, the material or the dumper.
pub fn run(ex: &Example) -> Result<Outcome> {
    ex.create_output_dir()?;
    let max_steps = ex.steps_or(10_000);
    let dim = 2;

    let mesh = ex.read_mesh(dim, "barre_trou.msh")?;

    // model creation
    let mut model = SolidMechanicsModel::with_parser(
        SharedMesh::new(mesh),
        None,
        "solid_mechanics_model",
        ex.parser()?,
    )?;
    // the new material, known to the factory before the input file is read
    local_material_damage::register(&mut model.physics.factory, ex.upstream_compat)?;

    // model initialization
    model.init_full(&ex.model_options(AnalysisMethod::ExplicitLumpedMass))?;

    {
        // upstream `std::cout << model.getMaterial(0) << "\n"`
        let mat = model.material(0)?;
        let own = mat
            .as_any()
            .and_then(|a| a.downcast_ref::<local_material_damage::LocalMaterialDamage>());
        ex.say(common::law_printout(mat, &|name| {
            own.and_then(|m| m.private_param(name))
        })?);
    }

    let time_step = model.stable_time_step()? / 10.;
    model.set_time_step(time_step, None)?;

    // Dirichlet boundary conditions
    let fixed = |axis| FixedValue { value: 0.0, axis };
    model.apply_dirichlet_bc(&fixed(SpatialDirection::X), "Fixed_x")?;
    model.apply_dirichlet_bc(&fixed(SpatialDirection::Y), "Fixed_y")?;

    // Neumann boundary condition
    let mut stress = MatrixX::zeros(dim, dim);
    for i in 0..dim {
        *stress.at_mut(i, i) = 3e2;
    }
    model.apply_neumann_bc(&FromStress::new(stress), "Traction")?;

    model.set_directory(&ex.paraview_dir())?;
    model.set_base_name("local_material")?;
    for field in [
        "displacement",
        "velocity",
        "acceleration",
        "external_force",
        "internal_force",
        "grad_u",
        "stress",
    ] {
        model.add_dump_field(field)?;
    }
    // Upstream always adds `damage`. A dump field that no part of the model
    // provides is refused at registration (UD-683, upstream throws as well),
    // so it is added only if a material has that internal: the `local_damage`
    // law of `material.dat` does, a built-in `elastic` law given in another
    // input file does not.
    if has_internal(&model, "damage")? {
        model.add_dump_field("damage")?;
    }
    if ex.dump {
        model.dump(None, None)?;
    }

    let mut energies = Vec::with_capacity(max_steps);
    for s in 0..max_steps {
        model.solve_step(None)?;

        let epot = model.energy("potential")?;
        let ekin = model.energy("kinetic")?;
        energies.push([epot, ekin]);

        if s.is_multiple_of(100) {
            ex.say(format!(
                "{s} {} {} {}",
                cout(epot),
                cout(ekin),
                cout(epot + ekin)
            ));
        }

        if s.is_multiple_of(1000) && ex.dump {
            model.dump(None, None)?;
        }
    }

    Ok(Outcome {
        model,
        time_step,
        energies,
    })
}

/// Whether a material of the model has the internal field `name`.
///
/// # Errors
/// None in practice (every index below `nb_materials` is valid).
pub fn has_internal(model: &SolidMechanicsModel, name: &str) -> Result<bool> {
    for i in 0..model.nb_materials() {
        let names = model.material(i)?.internal_names_sorted();
        if names.iter().any(|n| n == name) {
            return Ok(true);
        }
    }
    Ok(false)
}

fn main() -> ExitCode {
    common::main_with(|args| run(&example().with_args(args)?).map(|_| ()))
}
