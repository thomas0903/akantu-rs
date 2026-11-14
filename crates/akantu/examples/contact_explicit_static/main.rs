// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): examples/c++/contact_mechanics_model/hertz/
// contact_explicit_static.cc (+ material.dat, hertz.geo).

//! `contact_explicit_static` (EX-002): the Hertz setup of
//! `contact_explicit_dynamic` solved **statically** (`CouplerSolidContact`,
//! Newton–Raphson with the contact search in every corrector and the
//! contact stiffness in `K`): 100 load increments of `1e-4` on the
//! `loading` edge, a dump after each (`paraview/contact-explicit-static_<n>.pvtu`,
//! the coupler's dumper, `akantu::couplers::dump`).
//!
//! `--upstream-compat` selects every upstream defect of the contact model
//! (`ContactUpstreamCompat::all()`: UD-002, UD-363…365, UD-368 upstream
//! normal moduli, which drive the Newton path) and of the coupling
//! (`SolidContactCompat::upstream()`: UD-374, `per_iteration_friction_history`;
//! UD-1291, `search_in_corrector_only`). The upstream solver is `_auto`
//! (MUMPS in the oracle); `--solver eigen` pins the Eigen sparse LU. In
//! this set-up (frictionless contact, rigid master, load increments on the
//! `loading` edge only, which no contact node belongs to) the switches are
//! inert: **the default run reproduces upstream** bitwise, Newton path
//! included (the oracle comparison is tested in both modes with `--solver
//! eigen`). For UD-1291 this is because the predictor
//! moves no contact node, so the corrected default's search before the
//! first residual finds exactly the previous step's last search.
//!
//! ```text
//! cargo run -p akantu --release --example contact_explicit_static [-- --upstream-compat --solver eigen]
//! ```

#[path = "../common/mod.rs"]
pub mod common;

use std::process::ExitCode;

use akantu::contact::{ContactUpstreamCompat, PhysicalSurfaceSelector};
use akantu::core::enums::{AnalysisMethod, SpatialDirection};
use akantu::core::{Real, Result};
use akantu::couplers::{CouplerSolidContact, SolidContactCompat};
use akantu::mesh::SharedMesh;
use akantu::model::Model;
use akantu::model::boundary_condition::{FixedValue, IncrementValue};
use akantu::model::constitutive_law::ElementDataSelector;
use akantu::parser::param::ParamValue;
use common::Example;

/// Spatial dimension of the example.
pub const SPATIAL_DIMENSION: usize = 2;

/// Result of a run.
#[derive(Debug)]
pub struct Outcome {
    /// The coupler in its final state.
    pub coupler: CouplerSolidContact,
    /// After every step: number of contact elements, Newton iterations and
    /// error.
    pub newton: Vec<(usize, i64, Real)>,
}

/// Default configuration (upstream directory and input file).
#[must_use]
pub fn example() -> Example {
    Example::new(
        "contact_explicit_static",
        "contact_mechanics_model/hertz",
        "material.dat",
    )
}

/// The Newton iteration count and error of the solid's solver.
fn newton_state(coupler: &CouplerSolidContact) -> Result<(i64, Real)> {
    let nls = coupler.solid.solver.non_linear_solver(None)?;
    let n = match nls.get_param("nb_iterations")? {
        ParamValue::Int(n) => n,
        other => return Err(akantu::core::Error::invalid(format!("{other:?}"))),
    };
    let e = match nls.get_param("error")? {
        ParamValue::Real(e) => e,
        other => return Err(akantu::core::Error::invalid(format!("{other:?}"))),
    };
    Ok((n, e))
}

/// The upstream `main` after `initialize`.
///
/// # Errors
/// Any error of the models, the mesh, the solver or the outputs.
pub fn run(ex: &Example) -> Result<Outcome> {
    ex.create_output_dir()?;
    let spatial_dimension = SPATIAL_DIMENSION;

    let mesh = ex.read_mesh(spatial_dimension, "hertz.msh")?;
    let parser = ex.parser()?;

    let mut coupler = CouplerSolidContact::with_parser(
        SharedMesh::new(mesh),
        None,
        "coupler_solid_contact",
        parser.clone(),
    )?;

    coupler
        .solid
        .set_material_selector(Box::new(ElementDataSelector::new("physical_names")));

    coupler.init_full(&ex.model_options(AnalysisMethod::Static))?;
    if ex.upstream_compat {
        coupler
            .contact
            .set_upstream_compat(ContactUpstreamCompat::all());
        coupler.set_upstream_compat(SolidContactCompat::upstream())?;
    }

    let surface_selector = {
        let mesh = coupler.solid.mesh().read()?;
        PhysicalSurfaceSelector::new(&mesh, &parser)?
    };
    coupler
        .contact
        .set_surface_selector(Box::new(surface_selector))?;

    let fixed = |axis| FixedValue { value: 0.0, axis };
    coupler.apply_dirichlet_bc(&fixed(SpatialDirection::X), "fixed")?;
    coupler.apply_dirichlet_bc(&fixed(SpatialDirection::Y), "fixed")?;
    coupler.apply_dirichlet_bc(&fixed(SpatialDirection::X), "loading")?;
    coupler.apply_dirichlet_bc(&fixed(SpatialDirection::X), "symmetry")?;

    coupler.set_directory(&ex.paraview_dir())?;
    coupler.set_base_name("contact-explicit-static")?;
    coupler.add_dump_field_vector("displacement")?;
    coupler.add_dump_field_vector("normals")?;
    coupler.add_dump_field_vector("contact_force")?;
    coupler.add_dump_field_vector("external_force")?;
    coupler.add_dump_field_vector("internal_force")?;
    coupler.add_dump_field("gaps")?;
    coupler.add_dump_field("areas")?;
    coupler.add_dump_field("blocked_dofs")?;
    coupler.add_dump_field("grad_u")?;
    coupler.add_dump_field("stress")?;

    let max_steps = ex.steps_or(100);

    let mut newton = Vec::with_capacity(max_steps);
    for _ in 0..max_steps {
        let increment = 1e-4;
        coupler.apply_dirichlet_bc(
            &IncrementValue {
                value: -increment,
                axis: SpatialDirection::Y,
            },
            "loading",
        )?;

        coupler.solve_step(None)?;
        let (n, e) = newton_state(&coupler)?;
        newton.push((coupler.contact.contact_elements().len(), n, e));
        if ex.dump {
            coupler.dump(None, None)?;
        }
    }

    Ok(Outcome { coupler, newton })
}

fn main() -> ExitCode {
    common::main_with(|args| run(&example().with_args(args)?).map(|_| ()))
}
