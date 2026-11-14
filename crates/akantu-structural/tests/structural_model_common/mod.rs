// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs (port of Akantu, EPFL/LSMS): helpers of the structural
// model tests (upstream meshes, line meshes, dense views of the assembled
// matrices, a dense symmetric eigen-solver for natural frequencies).
// Test-only code; the mesh construction mirrors
// test/test_model/test_structural_mechanics_model/
// test_structural_mechanics_model_fixture.hh and
// test_structural_mechanics_model_bernoulli_beam_dynamics.cc (readMesh).

#![allow(dead_code)]
// each test binary uses a subset of the helpers
// Dense textbook algorithms (Cholesky, Jacobi rotations, triangular solves)
// read as their formulas with index loops.
#![allow(clippy::needless_range_loop)]

use std::path::PathBuf;

use akantu_core::enums::MeshIoType;
use akantu_core::{Array, ElementType, GhostType, Idx, Real};
use akantu_fe::structural::EXTRA_NORMAL;
use akantu_mesh::{Mesh, MeshAccessor, SharedMesh};
use akantu_structural::StructuralMechanicsModel;

pub const G: GhostType = GhostType::NotGhost;

/// `Math::getTolerance()` (default `100 ε`-scaled tolerance of upstream).
pub const MATH_TOLERANCE: Real = akantu_core::math::DEFAULT_TOLERANCE;

/// Path of an upstream test mesh copied under `tests/data/upstream`.
pub fn upstream_mesh_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/data/upstream")
        .join(name)
}

/// Upstream `mesh->read(filename, _miot_gmsh_struct)`.
pub fn read_upstream_mesh(name: &str, dim: usize) -> Mesh {
    let mut mesh = Mesh::new(dim, "structural_mesh").unwrap();
    akantu_io::mesh_io::read_mesh(&mut mesh, &upstream_mesh_path(name), MeshIoType::GmshStruct)
        .unwrap();
    mesh
}

/// A mesh of one structural type from explicit nodes/connectivities, with
/// optional `extra_normal` rows.
pub fn mesh_of(
    dim: usize,
    ty: ElementType,
    nodes: &[Vec<Real>],
    conn: &[Vec<Idx>],
    normals: Option<&[Vec<Real>]>,
) -> Mesh {
    let mut mesh = Mesh::new(dim, "structural_mesh").unwrap();
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
        set_normals(&mut mesh, ty, n);
    }
    mesh
}

/// Write the `extra_normal` rows of `ty`.
pub fn set_normals(mesh: &mut Mesh, ty: ElementType, normals: &[Vec<Real>]) {
    let dim = mesh.spatial_dimension();
    let flat: Vec<Real> = normals.iter().flatten().copied().collect();
    mesh.data_mut()
        .reals_mut_or_insert(EXTRA_NORMAL)
        .unwrap()
        .as_map_mut()
        .replace(ty, G, Array::from_vec(flat, dim).unwrap().into());
}

/// Straight beam of `nb_element` elements from `origin` along `direction`
/// (unit vector) of length `length` (upstream dynamics fixture: nodes
/// `i · le` on x).
pub fn line_mesh(
    dim: usize,
    ty: ElementType,
    nb_element: usize,
    length: Real,
    origin: &[Real],
    direction: &[Real],
    normal: Option<&[Real]>,
) -> Mesh {
    let le = length / nb_element as Real;
    let nodes: Vec<Vec<Real>> = (0..=nb_element)
        .map(|i| {
            (0..dim)
                .map(|d| origin[d] + (i as Real * le) * direction[d])
                .collect()
        })
        .collect();
    let conn: Vec<Vec<Idx>> = (0..nb_element as Idx).map(|i| vec![i, i + 1]).collect();
    let normals: Option<Vec<Vec<Real>>> = normal.map(|n| vec![n.to_vec(); nb_element]);
    mesh_of(dim, ty, &nodes, &conn, normals.as_deref())
}

/// The model's sparse matrix `id` as a dense `n × n` row-major matrix
/// (global equations = node-major, DOFs innermost in serial).
pub fn dense(model: &StructuralMechanicsModel, id: &str) -> Vec<Vec<Real>> {
    let m = model.dof_manager().matrix(id).unwrap();
    let n = m.size() as usize;
    (0..n)
        .map(|i| (0..n).map(|j| m.get(i as i64, j as i64)).collect())
        .collect()
}

/// Largest absolute entry (Eigen `lpNorm<Infinity>`).
pub fn max_abs(a: &[Vec<Real>]) -> Real {
    a.iter().flatten().fold(0.0, |m: Real, v| m.max(v.abs()))
}

/// Restriction of a dense matrix to the free DOFs.
pub fn restrict(a: &[Vec<Real>], free: &[usize]) -> Vec<Vec<Real>> {
    free.iter()
        .map(|&i| free.iter().map(|&j| a[i][j]).collect())
        .collect()
}

/// Cholesky factor `L` of a symmetric positive definite matrix.
pub fn cholesky(a: &[Vec<Real>]) -> Vec<Vec<Real>> {
    let n = a.len();
    let mut l = vec![vec![0.0; n]; n];
    for i in 0..n {
        for j in 0..=i {
            let mut s = a[i][j];
            for k in 0..j {
                s -= l[i][k] * l[j][k];
            }
            if i == j {
                assert!(s > 0.0, "matrix not positive definite at {i}: {s}");
                l[i][i] = s.sqrt();
            } else {
                l[i][j] = s / l[j][j];
            }
        }
    }
    l
}

/// Eigenvalues (ascending) of a symmetric matrix by cyclic Jacobi rotations.
pub fn symmetric_eigenvalues(a: &[Vec<Real>]) -> Vec<Real> {
    let n = a.len();
    let mut m: Vec<Vec<Real>> = a.to_vec();
    for _sweep in 0..100 {
        let off: Real = (0..n)
            .flat_map(|i| (0..n).filter(move |&j| j != i).map(move |j| (i, j)))
            .map(|(i, j)| m[i][j] * m[i][j])
            .sum();
        if off.sqrt() < 1e-14 * max_abs(&m).max(Real::MIN_POSITIVE) {
            break;
        }
        for p in 0..n {
            for q in p + 1..n {
                if m[p][q].abs() < Real::MIN_POSITIVE {
                    continue;
                }
                let theta = (m[q][q] - m[p][p]) / (2.0 * m[p][q]);
                let t = theta.signum() / (theta.abs() + (theta * theta + 1.0).sqrt());
                let t = if theta == 0.0 { 1.0 } else { t };
                let c = 1.0 / (t * t + 1.0).sqrt();
                let s = t * c;
                for k in 0..n {
                    let (mkp, mkq) = (m[k][p], m[k][q]);
                    m[k][p] = c * mkp - s * mkq;
                    m[k][q] = s * mkp + c * mkq;
                }
                for k in 0..n {
                    let (mpk, mqk) = (m[p][k], m[q][k]);
                    m[p][k] = c * mpk - s * mqk;
                    m[q][k] = s * mpk + c * mqk;
                }
            }
        }
    }
    let mut ev: Vec<Real> = (0..n).map(|i| m[i][i]).collect();
    ev.sort_by(Real::total_cmp);
    ev
}

/// Generalised eigenvalues `K φ = λ M φ` (ascending) for symmetric `K` and
/// symmetric positive definite `M` (`L⁻¹ K L⁻ᵀ`, `M = L Lᵀ`).
pub fn generalized_eigenvalues(k: &[Vec<Real>], m: &[Vec<Real>]) -> Vec<Real> {
    let n = k.len();
    let l = cholesky(m);
    // Y = L⁻¹ K (forward substitution per column)
    let mut y = vec![vec![0.0; n]; n];
    for col in 0..n {
        for i in 0..n {
            let mut s = k[i][col];
            for j in 0..i {
                s -= l[i][j] * y[j][col];
            }
            y[i][col] = s / l[i][i];
        }
    }
    // C = Y L⁻ᵀ, i.e. solve L Cᵀ = Yᵀ
    let mut c = vec![vec![0.0; n]; n];
    for row in 0..n {
        for i in 0..n {
            let mut s = y[row][i];
            for j in 0..i {
                s -= l[i][j] * c[row][j];
            }
            c[row][i] = s / l[i][i];
        }
    }
    // symmetrise rounding
    for i in 0..n {
        for j in 0..i {
            let v = 0.5 * (c[i][j] + c[j][i]);
            c[i][j] = v;
            c[j][i] = v;
        }
    }
    symmetric_eigenvalues(&c)
}

/// Dense `A x`.
pub fn mat_vec(a: &[Vec<Real>], x: &[Real]) -> Vec<Real> {
    a.iter()
        .map(|row| row.iter().zip(x).map(|(a, b)| a * b).sum())
        .collect()
}

pub fn shared(mesh: Mesh) -> SharedMesh {
    SharedMesh::new(mesh)
}

/// `½ (Σ_DOF m v² + Σₙ Σ_{i≠j} J_ij ω_i ω_j)`: the kinetic energy of the
/// model's lumped mass with its nodal rotary blocks (`rotary_inertia()`,
/// D-059), from the model's arrays (definition, independent of the model's
/// own accumulation order).
pub fn lumped_kinetic_energy_reference(model: &StructuralMechanicsModel) -> Real {
    let lm = model.lumped_mass().unwrap().as_slice().to_vec();
    let v = model.velocity().unwrap().as_slice().to_vec();
    let j = model.rotary_inertia().unwrap();
    let nr2 = j.nb_component();
    let nr = if nr2 == 9 { 3 } else { 1 };
    let ndof = lm.len() / j.len();
    let mut e: Real = lm.iter().zip(&v).map(|(m, v)| m * v * v).sum();
    for n in 0..j.len() {
        let b = j.tuple(n);
        let w = &v[n * ndof + ndof - nr..(n + 1) * ndof];
        for p in 0..nr {
            for q in 0..nr {
                if p != q {
                    e += w[p] * b[p * nr + q] * w[q];
                }
            }
        }
    }
    0.5 * e
}
