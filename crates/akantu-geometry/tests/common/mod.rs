// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): test/test_geometry/*.cc (mesh loading and
// Math::are_vector_equal checks shared by the geometry test ports).

#![allow(dead_code)] // each test binary uses a subset

use akantu_core::Real;
use akantu_core::enums::MeshIoType;
use akantu_mesh::Mesh;

/// Read one of the upstream test meshes copied to `tests/meshes`.
pub fn read(dim: usize, name: &str, id: &str) -> Mesh {
    let path = format!("{}/tests/meshes/{name}", env!("CARGO_MANIFEST_DIR"));
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"));
    let mut m = Mesh::new(dim, id).unwrap();
    akantu_io::read_mesh_from_str(
        &mut m,
        name,
        &text,
        MeshIoType::Gmsh,
        &akantu_io::ReadOptions::default(),
    )
    .unwrap();
    m
}

/// Upstream `Math::are_vector_equal(n, x, y)` with tolerance `tol`.
pub fn are_vector_equal(x: &[Real], y: &[Real], tol: Real) -> bool {
    x.len() == y.len()
        && x.iter()
            .zip(y)
            .all(|(&a, &b)| akantu_core::math::are_float_equal(a, b, tol))
}
