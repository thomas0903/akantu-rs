// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Test support for the
// ports of test/test_model/test_phase_field_model/* and the phase-field
// mathematical checks.

#![allow(dead_code)] // shared by several test binaries, each uses a subset

use std::path::{Path, PathBuf};

use akantu_core::enums::{AnalysisMethod, MeshIoType};
use akantu_core::{ElementType, GhostType, Real};
use akantu_mesh::{Mesh, SharedMesh};
use akantu_model::ModelOptions;
use akantu_parser::Parser;
use akantu_parser::param::ParamValue;
use akantu_phase_field::PhaseFieldModel;

/// `crates/akantu-phase-field/tests/data/test_phase_field_model`.
pub fn data_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/data/test_phase_field_model")
}

/// Read a Gmsh mesh.
pub fn read_mesh(path: &Path, dim: usize) -> Mesh {
    let mut mesh = Mesh::new(dim, "mesh").unwrap();
    akantu_io::read_mesh(&mut mesh, path, MeshIoType::Gmsh)
        .unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    mesh
}

/// Phase-field model on the upstream mesh `msh` with the upstream material
/// file `dat` (both in [`data_dir`]).
pub fn model_from_files(msh: &str, dat: &str, dim: usize) -> PhaseFieldModel {
    let mesh = read_mesh(&data_dir().join(msh), dim);
    let parser = Parser::parse_file(data_dir().join(dat)).unwrap();
    PhaseFieldModel::with_parser(SharedMesh::new(mesh), None, "phase_field_model", parser).unwrap()
}

/// Phase-field model on `mesh` with the material text `dat`.
pub fn model_with(mesh: Mesh, dat: &str) -> PhaseFieldModel {
    let parser = Parser::parse_str(dat, "material.dat").unwrap();
    PhaseFieldModel::with_parser(SharedMesh::new(mesh), None, "phase_field_model", parser).unwrap()
}

/// `initFull(_analysis_method = _static)`.
pub fn init_static(model: &mut PhaseFieldModel) {
    model
        .init_full(&ModelOptions::new(AnalysisMethod::Static))
        .unwrap();
}

/// Set the Newton–Raphson parameters of the default solver (upstream
/// `solver.set("max_iterations", …)` etc.).
pub fn set_newton(
    model: &mut PhaseFieldModel,
    max_iterations: i64,
    threshold: f64,
    criterion: &str,
) {
    let nls = model.solver.non_linear_solver_mut(None).unwrap();
    nls.set_param("max_iterations", ParamValue::Int(max_iterations))
        .unwrap();
    nls.set_param("threshold", ParamValue::Real(threshold))
        .unwrap();
    nls.set_param("convergence_type", ParamValue::Enum(criterion.into()))
        .unwrap();
}

/// Write the same strain (column-major `dim × dim`) at every quadrature
/// point of every law (what the solid coupler writes from a homogeneous
/// displacement gradient).
pub fn set_uniform_strain(model: &mut PhaseFieldModel, strain: &[Real]) {
    let dim = model.spatial_dimension();
    assert_eq!(strain.len(), dim * dim);
    for i in 0..model.nb_phase_fields() {
        let law = model.phase_field_mut(i).unwrap();
        let types = law.base().filter_types(GhostType::NotGhost);
        for ty in types {
            let mut s = law
                .base_mut()
                .strain
                .get_mut(ty, GhostType::NotGhost)
                .unwrap();
            for t in s.as_mut_slice().chunks_exact_mut(dim * dim) {
                t.copy_from_slice(strain);
            }
        }
    }
}

/// Uniaxial strain `ε_yy = e` (all other components zero) in `dim`
/// dimensions (`ε_xx = e` in 1-D).
pub fn uniaxial(dim: usize, e: Real) -> Vec<Real> {
    let mut s = vec![0.0; dim * dim];
    let k = if dim == 1 { 0 } else { 1 };
    s[k + k * dim] = e;
    s
}

/// Law `i`'s `damage` internal of `(ty, not ghost)`.
pub fn law_damage(model: &PhaseFieldModel, i: usize, ty: ElementType) -> Vec<Real> {
    model
        .phase_field(i)
        .unwrap()
        .base()
        .damage
        .get(ty, GhostType::NotGhost)
        .unwrap()
        .as_slice()
        .to_vec()
}

/// Relative error `|a − b| / |b|` (NaN for `b = 0`, as upstream divides).
pub fn rel(a: Real, b: Real) -> Real {
    (a - b).abs() / b.abs()
}

/// Upstream `const Real E = phasefield.getParam("E")` etc.
pub fn param(model: &PhaseFieldModel, law: usize, name: &str) -> Real {
    model
        .phase_field(law)
        .unwrap()
        .get_param(name)
        .unwrap()
        .as_real()
        .unwrap()
}
