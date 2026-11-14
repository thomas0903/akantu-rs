// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs (port of Akantu, EPFL/LSMS): helpers of the structural
// element tests (mesh construction, element matrices, dense assembly and
// solve). Test-only code, no upstream counterpart.

#![allow(dead_code)] // each test binary uses a subset of the helpers

use akantu_core::{Array, ElementType, GhostType, Idx, Real};
use akantu_fe::structural::{EXTRA_NORMAL, StructuralFeEngine};
use akantu_mesh::{Mesh, MeshAccessor};
use akantu_validation::reduce::NanMinMax;

pub const G: GhostType = GhostType::NotGhost;

/// Mesh with one structural element type (`normals`: `extra_normal` rows).
pub fn mesh(
    dim: usize,
    ty: ElementType,
    nodes: &[Vec<Real>],
    conn: &[Vec<Idx>],
    normals: Option<&[Vec<Real>]>,
) -> Mesh {
    let mut mesh = Mesh::new(dim, "structural_test").unwrap();
    {
        let mut acc = MeshAccessor::new(&mut mesh);
        for x in nodes {
            acc.push_node(x).unwrap();
        }
        let mut c = acc.connectivity_mut(ty, G);
        for row in conn {
            c.push_tuple(row);
        }
    }
    if let Some(n) = normals {
        let flat: Vec<Real> = n.iter().flatten().copied().collect();
        mesh.data_mut()
            .reals_mut_or_insert(EXTRA_NORMAL)
            .unwrap()
            .as_map_mut()
            .replace(ty, G, Array::from_vec(flat, dim).unwrap().into());
    }
    mesh
}

pub fn engine(mesh: &Mesh) -> StructuralFeEngine {
    StructuralFeEngine::with_precomputation(mesh, None, "structural_test_fem").unwrap()
}

/// Per-quadrature-point tangents from one tangent per element.
pub fn tangents_per_point(per_element: &[Vec<Real>], nq: usize) -> Array<Real> {
    let n = per_element[0].len();
    let mut d = Vec::new();
    for t in per_element {
        for _ in 0..nq {
            d.extend_from_slice(t);
        }
    }
    Array::from_vec(d, n).unwrap()
}

/// Element stiffness matrices (`nc × nc` column-major per element).
pub fn element_stiffness(
    fe: &StructuralFeEngine,
    m: &Mesh,
    ty: ElementType,
    per_element: &[Vec<Real>],
) -> Vec<Vec<Real>> {
    let nq = fe.nb_integration_points(ty, G).unwrap();
    let d = tangents_per_point(per_element, nq);
    let mut k = Array::new(1);
    fe.element_stiffness(m, d.view(), &mut k, ty, G).unwrap();
    let nc2 = k.nb_component();
    k.as_slice().chunks(nc2).map(<[Real]>::to_vec).collect()
}

/// [`element_stiffness`] of a fresh engine on `m`.
pub fn mesh_stiffness(m: &Mesh, ty: ElementType, per_element: &[Vec<Real>]) -> Vec<Vec<Real>> {
    element_stiffness(&engine(m), m, ty, per_element)
}

/// Dense global matrix (`n × n` column-major, `n = nb_nodes · ndof`) from
/// element matrices in node-major local DOF order.
pub fn assemble(nb_nodes: usize, ndof: usize, conn: &[Vec<Idx>], mats: &[Vec<Real>]) -> Vec<Real> {
    let n = nb_nodes * ndof;
    let mut a = vec![0.0; n * n];
    for (c, m) in conn.iter().zip(mats) {
        let nc = c.len() * ndof;
        let g = |l: usize| c[l / ndof] as usize * ndof + l % ndof;
        for j in 0..nc {
            for i in 0..nc {
                a[g(i) + g(j) * n] += m[i + j * nc];
            }
        }
    }
    a
}

/// `y = A x` for a dense column-major `n × n` matrix.
pub fn matvec(a: &[Real], x: &[Real]) -> Vec<Real> {
    let n = x.len();
    (0..n)
        .map(|i| (0..n).map(|j| a[i + j * n] * x[j]).sum())
        .collect()
}

/// Solve `A_ff u_f = f_f − A_fc u_c` for the free DOFs (`blocked[i]`:
/// prescribed `u[i]`), Gaussian elimination with partial pivoting.
pub fn solve_blocked(a: &[Real], f: &[Real], u: &mut [Real], blocked: &[bool]) {
    let n = f.len();
    let free: Vec<usize> = (0..n).filter(|&i| !blocked[i]).collect();
    let m = free.len();
    let mut k = vec![0.0; m * m];
    let mut r = vec![0.0; m];
    for (fi, &i) in free.iter().enumerate() {
        r[fi] = f[i]
            - (0..n)
                .filter(|&j| blocked[j])
                .map(|j| a[i + j * n] * u[j])
                .sum::<Real>();
        for (fj, &j) in free.iter().enumerate() {
            k[fi + fj * m] = a[i + j * n];
        }
    }
    for col in 0..m {
        let p = (col..m)
            .max_by(|&x, &y| k[x + col * m].abs().total_cmp(&k[y + col * m].abs()))
            .unwrap();
        if p != col {
            for c in 0..m {
                k.swap(col + c * m, p + c * m);
            }
            r.swap(col, p);
        }
        let d = k[col + col * m];
        for row in col + 1..m {
            let l = k[row + col * m] / d;
            for c in col..m {
                k[row + c * m] -= l * k[col + c * m];
            }
            r[row] -= l * r[col];
        }
    }
    let mut x = vec![0.0; m];
    for row in (0..m).rev() {
        let s: Real = (row + 1..m).map(|c| k[row + c * m] * x[c]).sum();
        x[row] = (r[row] - s) / k[row + row * m];
    }
    for (fi, &i) in free.iter().enumerate() {
        u[i] = x[fi];
    }
}

/// Infinity norm of a slice.
pub fn max_abs(v: &[Real]) -> Real {
    v.iter().fold(0.0, |m, x| m.nan_max(x.abs()))
}

/// `Tᵀ K T` for column-major `n × n` matrices.
pub fn congruence(t: &[Real], k: &[Real], n: usize) -> Vec<Real> {
    let mut kt = vec![0.0; n * n];
    for j in 0..n {
        for i in 0..n {
            kt[i + j * n] = (0..n).map(|l| k[i + l * n] * t[l + j * n]).sum();
        }
    }
    let mut out = vec![0.0; n * n];
    for j in 0..n {
        for i in 0..n {
            out[i + j * n] = (0..n).map(|l| t[l + i * n] * kt[l + j * n]).sum();
        }
    }
    out
}

/// Block-diagonal `T` (`ndof · nn` square) from an `ndof × ndof` rotation.
pub fn block_diag(r: &[Real], ndof: usize, nn: usize) -> Vec<Real> {
    let nc = ndof * nn;
    let mut t = vec![0.0; nc * nc];
    for n in 0..nn {
        for j in 0..ndof {
            for i in 0..ndof {
                t[(n * ndof + i) + (n * ndof + j) * nc] = r[i + j * ndof];
            }
        }
    }
    t
}
