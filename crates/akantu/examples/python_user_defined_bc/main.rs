// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): examples/c++/solid_mechanics_model/
// boundary_conditions/python_user_defined_bc/python_user_defined_bc.cc (+
// boundary_condition.py and material.dat, copied unchanged next to this file;
// fine_mesh.geo).

//! `python_user_defined_bc` (EX-012): a Rust program that embeds CPython
//! (upstream: `pybind11::embed`, `py::scoped_interpreter`) and applies a
//! Dirichlet condition computed by a Python object: `SineBoundary` holds an
//! instance of `boundary_condition.SinBoundary(0.2, 10.)` and calls its
//! `compute(disp, coord, flags)` for every node of `Fixed_x`, with the
//! node's displacement and blocked-flag rows as **zero-copy writable NumPy
//! views** of the model's arrays (`py11_akantu::embed` of `akantu-python`, the export
//! protocol of the Python package, D-047). Then the predefined `FixedValue`
//! on `Fixed_y`, `FromTraction` on `Traction` and one ParaView dump
//! (`paraview/plate_0000.pvtu`); no solve.
//!
//! Upstream's interpreter finds `boundary_condition.py` in the program's
//! directory (pybind11 adds it to `sys.path`); the port adds the example's
//! source directory. NumPy must be importable by the embedded interpreter
//! (e.g. `PYTHONPATH=<venv>/lib/python3.X/site-packages`).
//!
//! ```text
//! PYO3_PYTHON=<python3> cargo run -p akantu --features python \
//!     --example python_user_defined_bc [-- --output-dir <dir>]
//! ```

#[path = "../common/mod.rs"]
pub mod common;

use std::process::ExitCode;

use akantu::core::enums::SpatialDirection;
use akantu::core::{Error, Real, Result};
use akantu::mesh::SharedMesh;
use akantu::model::ModelOptions;
use akantu::model::boundary_condition::{FixedValue, FromTraction};
use akantu::solid::SolidMechanicsModel;
use common::Example;
use py11_akantu::embed::pyo3::prelude::*;
use py11_akantu::embed::{PythonDirichletFunctor, native, pyo3};

/// Upstream `SineBoundary`: a Dirichlet functor delegating to
/// `boundary_condition.SinBoundary(amplitude, phase).compute`.
#[derive(Debug)]
pub struct SineBoundary {
    functor: PythonDirichletFunctor,
}

impl SineBoundary {
    /// `py::module::import("boundary_condition")`, then
    /// `SinBoundary(amplitude, phase)`.
    ///
    /// # Errors
    /// The Python import or constructor failed.
    pub fn new(py: Python<'_>, amplitude: Real, phase: Real) -> PyResult<Self> {
        let module = py.import("boundary_condition")?;
        let obj = module.getattr("SinBoundary")?.call1((amplitude, phase))?;
        Ok(Self {
            functor: PythonDirichletFunctor::new(obj.unbind(), "compute"),
        })
    }

    /// The Dirichlet functor.
    #[must_use]
    pub fn functor(&self) -> &PythonDirichletFunctor {
        &self.functor
    }
}

fn py_err(e: &PyErr) -> Error {
    Error::User(format!("Python: {e}"))
}

/// Default configuration (upstream directory and input file).
#[must_use]
pub fn example() -> Example {
    Example::new(
        "python_user_defined_bc",
        "solid_mechanics_model/boundary_conditions/python_user_defined_bc",
        "material.dat",
    )
}

/// The upstream `main` after `initialize`: starts the interpreter (a no-op
/// if it runs already), returns the model in its final state.
///
/// # Errors
/// Any error of the model, the mesh, the dumper or the Python code.
pub fn run(ex: &Example) -> Result<SolidMechanicsModel> {
    ex.create_output_dir()?;
    // py::scoped_interpreter guard{};
    pyo3::Python::initialize();
    Python::attach(|py| {
        let path = py
            .import("sys")
            .and_then(|sys| sys.getattr("path"))
            .map_err(|e| py_err(&e))?;
        let dir = ex.source_dir.to_string_lossy().into_owned();
        if !path.contains(&dir).map_err(|e| py_err(&e))? {
            path.call_method1("insert", (0, dir))
                .map_err(|e| py_err(&e))?;
        }

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
        let sin_boundary = SineBoundary::new(py, 0.2, 10.).map_err(|e| py_err(&e))?;
        let m = &mut model;
        native(py, move || {
            m.apply_dirichlet_bc(sin_boundary.functor(), "Fixed_x")
        })
        .map_err(|e| py_err(&e))?;
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
    })
}

fn main() -> ExitCode {
    common::main_with(|args| run(&example().with_args(args)?).map(|_| ()))
}
