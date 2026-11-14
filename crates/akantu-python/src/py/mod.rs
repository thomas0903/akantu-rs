// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): python/py_akantu.cc (PYBIND11_MODULE,
// register_all), python/py_aka_common.cc (register_initialize,
// register_functions), python/py_aka_error.cc (register_error).

//! The `py11_akantu` extension module (feature `python`).

pub(crate) mod arrays;
pub(crate) mod bc;
pub(crate) mod cohesive;
pub(crate) mod common;
pub(crate) mod dumpable;
pub(crate) mod enums;
pub(crate) mod errors;
pub(crate) mod etma;
pub(crate) mod exports;
pub(crate) mod fe;
pub(crate) mod handle;
pub(crate) mod laws;
pub(crate) mod mesh;
pub(crate) mod model;
pub(crate) mod model_ops;
pub(crate) mod parser;
pub(crate) mod pf_laws;
pub(crate) mod physics;
pub(crate) mod runtime;
pub(crate) mod solid;
pub(crate) mod solver;
pub(crate) mod test_common;

use pyo3::prelude::*;

/// `PYBIND11_MODULE(py11_akantu, mod)`.
#[pymodule(gil_used = true)]
fn py11_akantu(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.setattr("__doc__", "Akantu python interface")?;
    errors::register(m)?;
    // `register_all` order.
    enums::register(m)?;
    common::register(m)?;
    parser::register(m)?;
    solver::register_solvers(m)?;
    m.add_class::<dumpable::PyDumpable>()?;
    mesh::register_group_manager(m)?;
    mesh::register_mesh(m)?;
    common::register_element(m)?;
    fe::register(m)?;
    solver::register_integration_schemes(m)?;
    solver::register_dof_manager(m)?;
    model::register_solvers(m)?;
    bc::register(m)?;
    model::register(m)?;
    laws::register_selectors(m)?;
    cohesive::register_selectors(m)?;
    laws::register_internal_handler(m)?;
    physics::register_heat_transfer(m)?;
    solid::register(m)?;
    laws::register_material_base(m)?;
    solid::register_material(m)?;
    cohesive::register_materials(m)?;
    cohesive::register(m)?;
    physics::register_structural(m)?;
    physics::register_contact(m)?;
    physics::register_phase_field(m)?;
    test_common::register(m)?;
    common::register_tail(m)?;
    // pybind11's module defines no `__all__` (a star import takes every public
    // name, which upstream's `akantu/__init__.py` relies on when it copies
    // `dir(py11_akantu)`); PyO3's `PyModule::add` builds one, and a name
    // renamed after registration would stay in it.
    if m.hasattr("__all__")? {
        m.delattr("__all__")?;
    }
    Ok(())
}
