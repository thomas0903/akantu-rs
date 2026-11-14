// SPDX-License-Identifier: GPL-3.0-or-later
// akantu-rs — GPL-3.0-or-later (D-042). Ported from Akantu (EPFL/LSMS):
// test/test_geometry/*.cc helpers (mesh loading, Math::are_vector_equal),
// plus structured test meshes.

#![allow(dead_code)] // each test binary uses a subset

use akantu_core::enums::MeshIoType;
use akantu_core::{ElementType, GhostType, Real};
use akantu_mesh::{Mesh, MeshAccessor};

/// Read an upstream test mesh copied to `tests/meshes`.
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

/// `Math::are_vector_equal`.
pub fn are_vector_equal(x: &[Real], y: &[Real], tol: Real) -> bool {
    x.len() == y.len()
        && x.iter()
            .zip(y)
            .all(|(&a, &b)| akantu_core::math::are_float_equal(a, b, tol))
}

/// Structured triangle mesh of `[0, lx] × [0, ly]` with `nx × ny` cells,
/// each split along its diagonal (alternating directions when `alt`).
pub fn triangle_grid(nx: usize, ny: usize, lx: Real, ly: Real, alt: bool) -> Mesh {
    let mut m = Mesh::new(2, "grid").unwrap();
    {
        let mut acc = MeshAccessor::new(&mut m);
        for j in 0..=ny {
            for i in 0..=nx {
                acc.push_node(&[lx * i as Real / nx as Real, ly * j as Real / ny as Real])
                    .unwrap();
            }
        }
        let id = |i: usize, j: usize| i32::try_from(j * (nx + 1) + i).unwrap();
        for j in 0..ny {
            for i in 0..nx {
                let (a, b, c, d) = (id(i, j), id(i + 1, j), id(i + 1, j + 1), id(i, j + 1));
                let mut conn = acc.connectivity_mut(ElementType::Triangle3, GhostType::NotGhost);
                if alt && (i + j) % 2 == 1 {
                    conn.push_tuple(&[a, b, d]);
                    conn.push_tuple(&[b, c, d]);
                } else {
                    conn.push_tuple(&[a, b, c]);
                    conn.push_tuple(&[a, c, d]);
                }
            }
        }
    }
    m
}

/// Structured tetrahedral mesh of the unit cube (`n³` cubes, 6 tetrahedra
/// each, Kuhn subdivision).
pub fn tet_grid(n: usize) -> Mesh {
    let mut m = Mesh::new(3, "tets").unwrap();
    {
        let mut acc = MeshAccessor::new(&mut m);
        let h = 1.0 / n as Real;
        for k in 0..=n {
            for j in 0..=n {
                for i in 0..=n {
                    acc.push_node(&[i as Real * h, j as Real * h, k as Real * h])
                        .unwrap();
                }
            }
        }
        let id =
            |i: usize, j: usize, k: usize| i32::try_from((k * (n + 1) + j) * (n + 1) + i).unwrap();
        for k in 0..n {
            for j in 0..n {
                for i in 0..n {
                    let v = |di: usize, dj: usize, dk: usize| id(i + di, j + dj, k + dk);
                    let (p0, p7) = (v(0, 0, 0), v(1, 1, 1));
                    let paths = [
                        [v(1, 0, 0), v(1, 1, 0)],
                        [v(1, 0, 0), v(1, 0, 1)],
                        [v(0, 1, 0), v(1, 1, 0)],
                        [v(0, 1, 0), v(0, 1, 1)],
                        [v(0, 0, 1), v(1, 0, 1)],
                        [v(0, 0, 1), v(0, 1, 1)],
                    ];
                    let mut conn =
                        acc.connectivity_mut(ElementType::Tetrahedron4, GhostType::NotGhost);
                    for [a, b] in paths {
                        conn.push_tuple(&[p0, a, b, p7]);
                    }
                }
            }
        }
    }
    m
}
