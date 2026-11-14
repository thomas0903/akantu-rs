// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): helpers of test/test_model/
// test_solid_mechanics_model/test_embedded_interface/*.cc (mesh and
// reinforcement-mesh construction).

//! Helpers shared by the embedded-model tests.

#![allow(dead_code)] // each test binary uses a subset

use std::path::{Path, PathBuf};

use akantu_core::enums::MeshIoType;
use akantu_core::{ElementType, GhostType, Real};
use akantu_embedded::{EmbeddedInterfaceModel, PHYSICAL_NAMES};
use akantu_mesh::{Mesh, MeshAccessor, SharedMesh};
use akantu_parser::{ParamValue, Parser};

/// Directory of the copied upstream inputs.
pub fn data(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/data")
        .join(name)
}

/// Read a Gmsh mesh of dimension `dim`.
pub fn read_mesh(path: &Path, dim: usize, id: &str) -> Mesh {
    let mut mesh = Mesh::new(dim, id).unwrap();
    akantu_io::read_mesh(&mut mesh, path, MeshIoType::Gmsh).unwrap();
    mesh
}

/// A reinforcement (primitive) mesh: nodes, `_segment_2` connectivity and
/// one physical name per segment (upstream tests build it by hand).
pub fn reinforcement_mesh(
    dim: usize,
    nodes: &[&[Real]],
    segments: &[[i32; 2]],
    names: &[&str],
) -> Mesh {
    assert_eq!(segments.len(), names.len());
    let mut mesh = Mesh::new(dim, "reinforcement_mesh").unwrap();
    {
        let mut acc = MeshAccessor::new(&mut mesh);
        for x in nodes {
            acc.push_node(x).unwrap();
        }
        let mut c = acc.connectivity_mut(ElementType::Segment2, GhostType::NotGhost);
        for s in segments {
            c.try_push_tuple(s).unwrap();
        }
    }
    let arr = mesh
        .data_mut()
        .strings_mut_or_insert(PHYSICAL_NAMES)
        .unwrap()
        .alloc(
            0,
            1,
            ElementType::Segment2,
            GhostType::NotGhost,
            String::new(),
        )
        .unwrap();
    for n in names {
        arr.try_push_tuple(&[(*n).to_owned()]).unwrap();
    }
    mesh
}

/// Model on `mesh` / `primitive` with the input text `dat`.
pub fn model(mesh: Mesh, primitive: Mesh, dat: &str) -> EmbeddedInterfaceModel {
    let parser = Parser::parse_str(dat, "material.dat").unwrap();
    EmbeddedInterfaceModel::with_parser(
        SharedMesh::new(mesh),
        primitive,
        None,
        "embedded_interface_model",
        parser,
    )
    .unwrap()
}

/// Upstream `solver.set("max_iterations", n)` & co. on the default
/// non-linear solver.
pub fn set_newton(model: &mut EmbeddedInterfaceModel, max_iterations: i64, threshold: f64) {
    let nls = model.smm.solver.non_linear_solver_mut(None).unwrap();
    nls.set_param("max_iterations", ParamValue::Int(max_iterations))
        .unwrap();
    nls.set_param("threshold", ParamValue::Real(threshold))
        .unwrap();
    nls.set_param("convergence_type", ParamValue::Enum("residual".into()))
        .unwrap();
}

/// Node coordinates of the background mesh.
pub fn nodes(model: &EmbeddedInterfaceModel) -> Vec<Vec<Real>> {
    let m = model.smm.physics.base.mesh.read().unwrap();
    m.nodes().tuples().map(<[Real]>::to_vec).collect()
}

/// Upstream `Math::are_float_equal(x, y)` with tolerance `tol`
/// (`|x − y| ≤ tol · max(|x|, |y|, 1)`).
pub fn are_float_equal(x: Real, y: Real, tol: Real) -> bool {
    akantu_core::math::are_float_equal(x, y, tol)
}

/// Background mesh of `nx × ny` cells over `[0, lx] × [0, ly]`, two
/// counter-clockwise linear triangles per cell (diagonal from the lower
/// left corner), every triangle named `name` in `physical_names`.
pub fn grid_triangles(nx: usize, ny: usize, lx: Real, ly: Real, name: &str) -> Mesh {
    let mut mesh = Mesh::new(2, "mesh").unwrap();
    let id = |i: usize, j: usize| i32::try_from(j * (nx + 1) + i).unwrap();
    let mut n_el = 0;
    {
        let mut acc = MeshAccessor::new(&mut mesh);
        for j in 0..=ny {
            for i in 0..=nx {
                acc.push_node(&[lx * i as Real / nx as Real, ly * j as Real / ny as Real])
                    .unwrap();
            }
        }
        let mut c = acc.connectivity_mut(ElementType::Triangle3, GhostType::NotGhost);
        for j in 0..ny {
            for i in 0..nx {
                c.try_push_tuple(&[id(i, j), id(i + 1, j), id(i + 1, j + 1)])
                    .unwrap();
                c.try_push_tuple(&[id(i, j), id(i + 1, j + 1), id(i, j + 1)])
                    .unwrap();
                n_el += 2;
            }
        }
    }
    name_elements(&mut mesh, ElementType::Triangle3, n_el, name);
    mesh
}

/// Background mesh of `n³` cubes over `[0, l]³`, six positively oriented
/// linear tetrahedra per cube (Kuhn subdivision along the main diagonal,
/// conforming), named `name`.
pub fn grid_tetrahedra(n: usize, l: Real, name: &str) -> Mesh {
    let mut mesh = Mesh::new(3, "mesh").unwrap();
    let id = |i: usize, j: usize, k: usize| i32::try_from((k * (n + 1) + j) * (n + 1) + i).unwrap();
    let mut coords = Vec::new();
    let mut n_el = 0;
    {
        let mut acc = MeshAccessor::new(&mut mesh);
        for k in 0..=n {
            for j in 0..=n {
                for i in 0..=n {
                    let x = [
                        l * i as Real / n as Real,
                        l * j as Real / n as Real,
                        l * k as Real / n as Real,
                    ];
                    acc.push_node(&x).unwrap();
                    coords.push(x);
                }
            }
        }
        let perms = [
            [0, 1, 2],
            [0, 2, 1],
            [1, 0, 2],
            [1, 2, 0],
            [2, 0, 1],
            [2, 1, 0],
        ];
        let mut c = acc.connectivity_mut(ElementType::Tetrahedron4, GhostType::NotGhost);
        for k in 0..n {
            for j in 0..n {
                for i in 0..n {
                    for p in perms {
                        let mut b = [i, j, k];
                        let mut tet = vec![id(b[0], b[1], b[2])];
                        for axis in p {
                            b[axis] += 1;
                            tet.push(id(b[0], b[1], b[2]));
                        }
                        let x = |t: i32| coords[usize::try_from(t).unwrap()];
                        let (a, bb, cc, d) = (x(tet[0]), x(tet[1]), x(tet[2]), x(tet[3]));
                        let e1 = [bb[0] - a[0], bb[1] - a[1], bb[2] - a[2]];
                        let e2 = [cc[0] - a[0], cc[1] - a[1], cc[2] - a[2]];
                        let e3 = [d[0] - a[0], d[1] - a[1], d[2] - a[2]];
                        if det3(&[e1, e2, e3]) < 0. {
                            tet.swap(2, 3);
                        }
                        c.try_push_tuple(&tet).unwrap();
                        n_el += 1;
                    }
                }
            }
        }
    }
    name_elements(&mut mesh, ElementType::Tetrahedron4, n_el, name);
    mesh
}

/// `physical_names` = `name` on the `n` elements of `ty`.
pub fn name_elements(mesh: &mut Mesh, ty: ElementType, n: usize, name: &str) {
    mesh.data_mut()
        .strings_mut_or_insert(PHYSICAL_NAMES)
        .unwrap()
        .alloc(n, 1, ty, GhostType::NotGhost, name.to_owned())
        .unwrap();
}

/// Determinant of the 3×3 matrix with rows `m`.
pub fn det3(m: &[[Real; 3]; 3]) -> Real {
    m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1])
        - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0])
        + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0])
}

/// Barycentric coordinates of `p` in the simplex `x` (`dim + 1` vertices of
/// dimension `dim`), computed independently of the FE engine (Cramer).
pub fn barycentric(x: &[Vec<Real>], p: &[Real]) -> Vec<Real> {
    let dim = p.len();
    let mut lam = vec![0.0; dim + 1];
    if dim == 2 {
        let d =
            (x[1][0] - x[0][0]) * (x[2][1] - x[0][1]) - (x[2][0] - x[0][0]) * (x[1][1] - x[0][1]);
        let l1 =
            ((p[0] - x[0][0]) * (x[2][1] - x[0][1]) - (x[2][0] - x[0][0]) * (p[1] - x[0][1])) / d;
        let l2 =
            ((x[1][0] - x[0][0]) * (p[1] - x[0][1]) - (p[0] - x[0][0]) * (x[1][1] - x[0][1])) / d;
        lam[1] = l1;
        lam[2] = l2;
        lam[0] = 1. - l1 - l2;
    } else {
        let e = |a: usize| [x[a][0] - x[0][0], x[a][1] - x[0][1], x[a][2] - x[0][2]];
        let q = [p[0] - x[0][0], p[1] - x[0][1], p[2] - x[0][2]];
        // columns e1 e2 e3: solve [e1 e2 e3] λ = q (Cramer with rows = columns)
        let m = [e(1), e(2), e(3)];
        let d = det3(&transpose3(&m));
        for k in 0..3 {
            let mut mk = m;
            mk[k] = q;
            lam[k + 1] = det3(&transpose3(&mk)) / d;
        }
        lam[0] = 1. - lam[1] - lam[2] - lam[3];
    }
    lam
}

fn transpose3(m: &[[Real; 3]; 3]) -> [[Real; 3]; 3] {
    let mut t = [[0.0; 3]; 3];
    for i in 0..3 {
        for j in 0..3 {
            t[i][j] = m[j][i];
        }
    }
    t
}

/// Gradients of the linear shape functions of the simplex `x`: `grad[a][i]
/// = ∂λ_a/∂x_i` (finite-difference-free: the barycentric map is affine).
pub fn simplex_gradients(x: &[Vec<Real>]) -> Vec<Vec<Real>> {
    let dim = x[0].len();
    let origin = barycentric(x, &x[0]);
    let mut grad = vec![vec![0.0; dim]; dim + 1];
    for i in 0..dim {
        let mut p = x[0].clone();
        p[i] += 1.0;
        let l = barycentric(x, &p);
        for a in 0..=dim {
            grad[a][i] = l[a] - origin[a];
        }
    }
    grad
}

/// Coordinates of the nodes of element `e` of `(ty, not ghost)`.
pub fn element_coords(mesh: &Mesh, ty: ElementType, e: usize) -> Vec<Vec<Real>> {
    let conn = mesh.connectivity(ty, GhostType::NotGhost).unwrap();
    conn.get_tuple(e)
        .unwrap()
        .iter()
        .map(|&n| {
            mesh.nodes()
                .get_tuple(usize::try_from(n).unwrap())
                .unwrap()
                .to_vec()
        })
        .collect()
}

/// Interface segments `(x0, x1, name, background element)`.
pub fn interface_segments(
    model: &EmbeddedInterfaceModel,
) -> Vec<(Vec<Real>, Vec<Real>, String, akantu_core::Element)> {
    let im = model.interface_mesh();
    let n = im.nb_element(ElementType::Segment2, GhostType::NotGhost);
    let conn = im
        .connectivity(ElementType::Segment2, GhostType::NotGhost)
        .unwrap();
    let names = im
        .data()
        .strings(PHYSICAL_NAMES)
        .unwrap()
        .get(ElementType::Segment2, GhostType::NotGhost)
        .unwrap();
    let assoc = model
        .interface_associated_elements(ElementType::Segment2, GhostType::NotGhost)
        .unwrap();
    (0..n)
        .map(|e| {
            let c = conn.get_tuple(e).unwrap();
            let x = |k: usize| {
                im.nodes()
                    .get_tuple(usize::try_from(c[k]).unwrap())
                    .unwrap()
                    .to_vec()
            };
            (x(0), x(1), names.as_slice()[e].clone(), assoc[e])
        })
        .collect()
}

/// Euclidean norm of `b − a`.
pub fn dist(a: &[Real], b: &[Real]) -> Real {
    a.iter()
        .zip(b)
        .map(|(x, y)| (y - x) * (y - x))
        .sum::<Real>()
        .sqrt()
}
