// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): examples/c++/embedded_model/embedded.cc
// (+ material.dat, concrete.geo, reinforcement.geo).

//! `embedded` (EX-008): a `10 × 1` concrete beam (quadratic triangles)
//! with an embedded pre-stressed reinforcement bar along `y = 0.25`
//! (`EmbeddedInterfaceModel`, `reinforcement elastic` law, `area = 1e-4`,
//! `pre_stress = 10e6`): `XBlocked` blocked in x, `YBlocked` in y, the
//! traction `(0, −1)` on `Force`; one static Newton–Raphson iteration
//! (residual criterion `1e-6`); dumps of the concrete
//! (`paraview/concrete.pvtu`: displacement, external and internal forces,
//! stress) and of the reinforcement (`paraview/reinforcement.pvtu`).
//!
//! Upstream builds the example with the package `CGAL` (`USE_PACKAGES
//! CGAL`): it lives behind the facade feature `cgal` as well, although the
//! port's intersector uses the LGPL `GridIndex` search structure of
//! `akantu-geometry`, whose result does not depend on the traversal order
//! (D-042: no GPL-derived code is used). Upstream reads the reinforcement
//! mesh inside a `try` that ignores the exception raised because it holds
//! only segments; the port reads it without error. Upstream dumps the
//! reinforcement field `stress_embedded`, which no longer exists (removed
//! in 2018): upstream's `createElementalField` returns no field for it and
//! the `reinforcement` branch of `addDumpGroupFieldToDumper` (no `try`
//! there) adds nothing, so the reinforcement dump holds the mesh only; the
//! port does the same with `--upstream-compat` (`ReinforcementDumper`
//! registers nothing for that name), and dumps the reinforcement `stress`
//! otherwise.
//! `--upstream-compat` also selects upstream's interface pieces (the
//! collected segment set, upstream's segment order: UD-408, UD-599,
//! UD-601) and the concrete dump's 9 × 9-values-per-cell layout of the
//! padded stress (UD-110). The example's second-order concrete mesh crashes upstream's
//! `initFull` (UD-851, root cause UD-1140: the law's element filter takes
//! the `_segment_3` boundary elements of the background mesh); the port runs
//! it; the oracle comparison uses the first-order version of the mesh
//! (`--mesh-dir`, test `examples_embedded::embedded_eigen_p1_matches_the_oracle`).
//! The upstream solver is `_auto` (MUMPS in the oracle); `--solver eigen`
//! pins the Eigen sparse LU.
//!
//! ```text
//! cargo run -p akantu --features cgal --example embedded [-- --upstream-compat --solver eigen]
//! ```

#[path = "../common/mod.rs"]
pub mod common;

use std::process::ExitCode;

use akantu::core::Result;
use akantu::core::enums::{AnalysisMethod, SpatialDirection};
use akantu::embedded::{
    EmbeddedInterfaceModel, EmbeddedInterfaceModelOptions, REINFORCEMENT_DUMPER,
};
use akantu::mesh::SharedMesh;
use akantu::model::boundary_condition::{FixedValue, FromTraction};
use akantu::parser::param::ParamValue;
use common::Example;

/// Default configuration (upstream directory and input file).
#[must_use]
pub fn example() -> Example {
    Example::new("embedded", "embedded_model", "material.dat")
}

/// The reinforcement field the example dumps: upstream's `stress_embedded`
/// (unknown since 2018: upstream creates no field for it, so nothing is
/// added) with `--upstream-compat`, the reinforcement stress otherwise.
#[must_use]
pub fn reinforcement_field(ex: &Example) -> &'static str {
    if ex.upstream_compat {
        "stress_embedded"
    } else {
        "stress"
    }
}

/// The upstream `main` after `initialize`; returns the model in its final
/// state.
///
/// # Errors
/// Any error of the meshes, the model, the solver or the dumpers.
pub fn run(ex: &Example) -> Result<EmbeddedInterfaceModel> {
    ex.create_output_dir()?;
    let dim = 2;

    // Loading the concrete mesh
    let mesh = ex.read_mesh(dim, "concrete.msh")?;

    // Loading the reinforcement mesh
    let mut reinforcement_mesh = akantu::mesh::Mesh::new(dim, "reinforcement_mesh")?;

    // Exception is raised because reinforcement
    // mesh contains only segments, i.e. 1D elements
    // (upstream; the port reads the segments without error)
    akantu::io::read_mesh(
        &mut reinforcement_mesh,
        &ex.mesh_path("reinforcement.msh"),
        akantu::core::enums::MeshIoType::Auto,
    )?;

    // Model creation
    let mut model = EmbeddedInterfaceModel::with_parser(
        SharedMesh::new(mesh),
        reinforcement_mesh,
        Some(dim),
        "embedded_interface_model",
        ex.parser()?,
    )?;
    if ex.upstream_compat {
        model.compat.ignore_dump_field_errors = true;
        // upstream's interface pieces (UD-408, UD-599, UD-601)
        model.intersector.remove_near_duplicates = false;
        model.intersector.options = akantu::geometry::SegmentIntersectorOptions {
            order: akantu::geometry::SegmentOrder::Upstream,
            ..akantu::geometry::SegmentIntersectorOptions::default()
        };
    }
    let mut options = EmbeddedInterfaceModelOptions::new(AnalysisMethod::Static);
    options.base = ex.model_options(AnalysisMethod::Static);
    model.init_full(&options)?;

    // Boundary conditions
    let fixed = |axis| FixedValue { value: 0.0, axis };
    model
        .smm
        .apply_dirichlet_bc(&fixed(SpatialDirection::X), "XBlocked")?;
    model
        .smm
        .apply_dirichlet_bc(&fixed(SpatialDirection::Y), "YBlocked")?;

    let force = vec![0.0, -1.0];

    model
        .smm
        .apply_neumann_bc(&FromTraction::new(force), "Force")?;

    // Dumping the concrete
    model.smm.set_directory(&ex.paraview_dir())?;
    if ex.upstream_compat {
        // UD-110: upstream writes the padded 2-D stress as 9 × 9 values per
        // cell (invalid VTK); the compat layout reproduces that layout
        model
            .smm
            .dumpers
            .dumpable
            .dumper_as::<akantu::io::DumperParaview>("solid_mechanics_model")?
            .set_compat_layout(true);
    }
    model.smm.set_base_name("concrete")?;
    model.smm.add_dump_field_vector("displacement")?;
    model.smm.add_dump_field_vector("external_force")?;
    model.smm.add_dump_field_vector("internal_force")?;
    model.smm.add_dump_field_tensor("stress")?;

    // Dumping the reinforcement
    model.set_directory_to_dumper(REINFORCEMENT_DUMPER, &ex.paraview_dir())?;
    model.set_base_name_to_dumper(REINFORCEMENT_DUMPER, "reinforcement")?;
    model.add_dump_field_tensor_to_dumper(REINFORCEMENT_DUMPER, reinforcement_field(ex))?; // dumping stress in reinforcement

    {
        let solver = model.smm.solver.non_linear_solver_mut(None)?;
        solver.set_param("max_iterations", ParamValue::Int(1))?;
        solver.set_param("threshold", ParamValue::Real(1e-6))?;
        solver.set_param("convergence_type", ParamValue::Enum("residual".into()))?;
    }

    model.solve_step(None)?;

    // Dumping model
    if ex.dump {
        model.dump(None, None)?;
        model.dump_named(REINFORCEMENT_DUMPER, None, None)?;
    }

    Ok(model)
}

fn main() -> ExitCode {
    common::main_with(|args| run(&example().with_args(args)?).map(|_| ()))
}
