// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): python/py_akantu.cc and python/py_*.cc,
// python/py_aka_array.hh (numpy views), python/akantu/__init__.py.

//! # akantu-python
//!
//! PyO3 bindings producing the extension module `py11_akantu`, loaded by a
//! copy of upstream's pure-Python `akantu/__init__.py` (decision record
//! D-024), ported from upstream's `python/` directory (`py_akantu.cc`,
//! `py_*.cc`, `py_aka_array.hh`). Cargo features, off by default: `python`
//! (builds the extension; `python/pyproject.toml` enables it for maturin)
//! and `mpi` (the MPI build of the module, mesh level: `Mesh.distribute()`
//! over the MPI world; needs MPI and Scotch at build time). The
//! requirements the bindings meet:
//!
//! * Zero-copy NumPy views of exportable arrays
//!   ([`akantu::core::array_shared::SharedArray`], 2-D `(len, nb_component)`,
//!   C-ordered) for nodal fields, internals, connectivities: modifications
//!   from Python (`model.getDisplacement()[:] = …`) must reach Rust storage.
//!   Views hold export leases ([`foreign_view::ForeignView`]); accesses follow
//!   the runtime phase protocol (one `ForeignRuntime` per interpreter, the
//!   GIL as its interpreter lock, a native section per binding call, a
//!   checked foreign window per Python callback). The `unsafe` needed to
//!   dereference lease pointers is confined to one module, `foreign_view`.
//! * Same class/function/enum names as upstream (`Mesh`, `SolidMechanicsModel`,
//!   `_triangle_3`, `_not_ghost`, `SolveConvergenceCriteria.residual`, …).
//! * Python-subclassable `Material`/`MaterialCohesive`/`DiffusionLaw`,
//!   `SolverCallback`, boundary-condition functors (upstream
//!   `PYBIND11_OVERRIDE`): Rust trait objects that call back into Python;
//!   Python exceptions become `Error::User` and vice versa.
//! * `AkantuSparseMatrix` keeps working: `getIRN/getJCN` are 1-based.
//! * MPI through mpi4py when built with the `mpi` feature (the mesh level;
//!   the models on a distributed mesh are not supported from Python).
//!
//! The crate compiles to an empty library unless the `python` feature is
//! enabled, so the default workspace build needs no Python interpreter.

#![deny(unsafe_code)]
#![warn(missing_docs)]

#[allow(unsafe_code)]
// lease dereferences after the phase check (SAFETY comments, rows in UNSAFE.md)
pub mod foreign_view;
#[allow(unsafe_code)]
// phase-scoped lifetime erasure (SAFETY comments, rows in UNSAFE.md)
pub mod scoped;

#[cfg(feature = "python")]
#[allow(unsafe_code)]
// PyO3's `#[pymodule]`/`#[pyclass]` macros expand to `unsafe` FFI glue; no
// hand-written `unsafe` lives under `py/` (enforced by tests/binding_rules.rs).
mod py;

/// Embedding CPython in a Rust program (upstream `pybind11::embed`; the
/// facade's `python_user_defined_bc` example, EX-012). Safe code only: the
/// row views come from [`foreign_view`].
#[cfg(feature = "python")]
pub mod embed;

/// Name of the extension module (matches upstream).
pub const MODULE_NAME: &str = "py11_akantu";

#[cfg(test)]
mod tests {
    #[test]
    fn module_name() {
        assert_eq!(super::MODULE_NAME, "py11_akantu");
    }
}
