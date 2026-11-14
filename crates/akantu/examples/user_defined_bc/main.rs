// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): examples/c++/solid_mechanics_model/
// boundary_conditions/user_defined_bc/user_defined_bc.cc (+ material.dat,
// fine_mesh.geo).

//! `user_defined_bc` (EX-014): a user Dirichlet functor (`SineBoundary`,
//! `u_x = −A sin(φ y)` on `Fixed_x`), the predefined `FixedValue` on
//! `Fixed_y` and the predefined Neumann functor `FromTraction` on
//! `Traction`, dumped once to ParaView (`paraview/plate_0000.pvtu`); no
//! solve.
//!
//! A user functor implements [`DirichletFunctor`] (upstream: a subclass of
//! `BC::Dirichlet::DirichletFunctor` overriding `operator()`).
//!
//! ```text
//! cargo run -p akantu --example user_defined_bc [-- --output-dir <dir>]
//! ```

#[path = "../common/mod.rs"]
pub mod common;

use std::process::ExitCode;

use akantu::core::enums::SpatialDirection;
use akantu::core::{Error, Idx, Real, Result};
use akantu::mesh::SharedMesh;
use akantu::model::ModelOptions;
use akantu::model::boundary_condition::{DirichletFunctor, FixedValue, FromTraction};
use akantu::solid::SolidMechanicsModel;
use common::Example;

/// Upstream `SineBoundary`: block `axis` and impose
/// `−amplitude · sin(phase · y)`.
#[derive(Clone, Copy, Debug)]
pub struct SineBoundary {
    /// Amplitude.
    pub amplitude: Real,
    /// Phase (wave number in `y`).
    pub phase: Real,
    /// Blocked axis.
    pub axis: SpatialDirection,
}

impl DirichletFunctor for SineBoundary {
    fn apply(
        &self,
        _node: Idx,
        flags: &mut [bool],
        primal: &mut [Real],
        coord: &[Real],
    ) -> Result<()> {
        // DIRICHLET_SANITY_CHECK
        if primal.len() > flags.len() {
            return Err(Error::invalid(
                "The primal vector and flags vectors given to the boundary condition functor have different sizes!",
            ));
        }
        let i = match self.axis {
            SpatialDirection::X => 0,
            SpatialDirection::Y => 1,
            SpatialDirection::Z => 2,
        };
        let (Some(flag), Some(value), Some(&y)) =
            (flags.get_mut(i), primal.get_mut(i), coord.get(1))
        else {
            return Err(Error::invalid(
                "SineBoundary: axis or y coordinate out of range",
            ));
        };
        *flag = true;
        *value = -self.amplitude * (self.phase * y).sin();
        Ok(())
    }
}

/// Default configuration (upstream directory and input file).
#[must_use]
pub fn example() -> Example {
    Example::new(
        "user_defined_bc",
        "solid_mechanics_model/boundary_conditions/user_defined_bc",
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
    let spatial_dimension = 2;
    let mesh = ex.read_mesh(spatial_dimension, "fine_mesh.msh")?;

    let mut model = SolidMechanicsModel::with_parser(
        SharedMesh::new(mesh),
        None,
        "solid_mechanics_model",
        ex.parser()?,
    )?;

    // model initialization
    model.init_full(&ModelOptions::default())?;

    // boundary conditions
    let traction = vec![0.2, 0.2];
    model.apply_dirichlet_bc(
        &SineBoundary {
            amplitude: 0.2,
            phase: 10.,
            axis: SpatialDirection::X,
        },
        "Fixed_x",
    )?;
    model.apply_dirichlet_bc(
        &FixedValue {
            value: 0.,
            axis: SpatialDirection::Y,
        },
        "Fixed_y",
    )?;
    model.apply_neumann_bc(&FromTraction::new(traction), "Traction")?;

    // output a paraview file with the boundary conditions
    model.set_directory(&ex.paraview_dir())?;
    model.set_base_name("plate")?;
    model.add_dump_field_vector("displacement")?;
    model.add_dump_field_vector("external_force")?;
    model.add_dump_field("blocked_dofs")?;
    if ex.dump {
        model.dump(None, None)?;
    }
    Ok(model)
}

fn main() -> ExitCode {
    common::main_with(|args| run(&example().with_args(args)?).map(|_| ()))
}
