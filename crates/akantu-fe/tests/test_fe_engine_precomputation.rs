// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): test/test_fe_engine/test_fe_engine_precomputation.cc
// and its reference implementation test/test_fe_engine/py_engine/py_engine.py
// (monomial bases of the shape functions).

//! `TestFEMPyFixture.Precompute`: the precomputed shapes `N`, physical
//! derivatives `B` and `det J · w` of every element of the upstream test
//! meshes agree (`1e-10`, upstream tolerance) with an **independent**
//! reference implementation: shape functions obtained by inverting the
//! Vandermonde matrix of a monomial basis at the reference nodes (as
//! upstream's `py_engine`), derivatives by differentiating the monomials,
//! `J`, `J⁻¹`, `det J` with a generic dense LU (`MatrixX`).

mod common;

use akantu_core::tensor::MatrixX;
use akantu_core::{ElementKind, ElementType, GhostType};
use akantu_fe::{FeEngine, shape::ShapeFamily};
use akantu_validation::Fixture;
use akantu_validation::reduce::NanMinMax;

/// Monomial exponents of the interpolation space of `ty` (py_engine
/// `MONOMES`; prisms: linear / quadratic serendipity prism spaces).
fn monomials(ty: ElementType) -> Vec<[u32; 3]> {
    let m = |v: &[[u32; 3]]| v.to_vec();
    use ElementType as E;
    match ty {
        E::Segment2 => m(&[[0, 0, 0], [1, 0, 0]]),
        E::Segment3 => m(&[[0, 0, 0], [1, 0, 0], [2, 0, 0]]),
        E::Triangle3 => m(&[[0, 0, 0], [1, 0, 0], [0, 1, 0]]),
        E::Triangle6 => m(&[
            [0, 0, 0],
            [1, 0, 0],
            [0, 1, 0],
            [2, 0, 0],
            [1, 1, 0],
            [0, 2, 0],
        ]),
        E::Quadrangle4 => m(&[[0, 0, 0], [1, 0, 0], [1, 1, 0], [0, 1, 0]]),
        E::Quadrangle8 => m(&[
            [0, 0, 0],
            [1, 0, 0],
            [1, 1, 0],
            [0, 1, 0],
            [2, 0, 0],
            [2, 1, 0],
            [1, 2, 0],
            [0, 2, 0],
        ]),
        E::Tetrahedron4 => m(&[[0, 0, 0], [1, 0, 0], [0, 1, 0], [0, 0, 1]]),
        E::Tetrahedron10 => m(&[
            [0, 0, 0],
            [1, 0, 0],
            [0, 1, 0],
            [0, 0, 1],
            [2, 0, 0],
            [1, 1, 0],
            [0, 2, 0],
            [0, 1, 1],
            [0, 0, 2],
            [1, 0, 1],
        ]),
        E::Hexahedron8 => m(&[
            [0, 0, 0],
            [1, 0, 0],
            [0, 1, 0],
            [0, 0, 1],
            [1, 1, 0],
            [1, 0, 1],
            [0, 1, 1],
            [1, 1, 1],
        ]),
        E::Hexahedron20 => m(&[
            [0, 0, 0],
            [1, 0, 0],
            [0, 1, 0],
            [0, 0, 1],
            [1, 1, 0],
            [1, 0, 1],
            [0, 1, 1],
            [1, 1, 1],
            [2, 0, 0],
            [0, 2, 0],
            [0, 0, 2],
            [2, 1, 0],
            [2, 0, 1],
            [2, 1, 1],
            [1, 2, 0],
            [0, 2, 1],
            [1, 2, 1],
            [1, 0, 2],
            [0, 1, 2],
            [1, 1, 2],
        ]),
        E::Pentahedron6 => m(&[
            [0, 0, 0],
            [1, 0, 0],
            [0, 1, 0],
            [0, 0, 1],
            [1, 1, 0],
            [1, 0, 1],
        ]),
        E::Pentahedron15 => m(&[
            [0, 0, 0],
            [1, 0, 0],
            [0, 1, 0],
            [0, 0, 1],
            [2, 0, 0],
            [1, 1, 0],
            [1, 0, 1],
            [0, 2, 0],
            [0, 1, 1],
            [0, 0, 2],
            [2, 1, 0],
            [2, 0, 1],
            [1, 2, 0],
            [1, 1, 1],
            [1, 0, 2],
        ]),
        _ => unreachable!(),
    }
}

fn mono(e: &[u32; 3], x: &[f64]) -> f64 {
    (0..x.len()).map(|d| x[d].powi(e[d] as i32)).product()
}

fn dmono(e: &[u32; 3], x: &[f64], i: usize) -> f64 {
    if e[i] == 0 {
        return 0.0;
    }
    (0..x.len())
        .map(|d| {
            if d == i {
                f64::from(e[d]) * x[d].powi(e[d] as i32 - 1)
            } else {
                x[d].powi(e[d] as i32)
            }
        })
        .product()
}

#[test]
fn test_fe_engine_precomputation() {
    let g = GhostType::NotGhost;
    for ty in common::test_element_types() {
        let mesh = common::fe_test_mesh(ty);
        let dim = mesh.spatial_dimension();
        let fe = FeEngine::with_precomputation(
            &mesh,
            None,
            ElementKind::Regular,
            ShapeFamily::Lagrange,
            "my_fem",
        )
        .unwrap();
        let nn = ty.nb_nodes_per_interpolation_element();
        let mons = monomials(ty);
        assert_eq!(mons.len(), nn);
        // reference node coordinates located by the oracle (N_j(ξ_i) = δ_ij)
        let fx = Fixture::open_by_id(&format!("fe/element_class/{}", ty.name())).unwrap();
        let xi_nodes = fx.load_f64("node_natural_coords").unwrap().data;
        let mut v = MatrixX::zeros(nn, nn);
        for i in 0..nn {
            for (j, e) in mons.iter().enumerate() {
                *v.at_mut(i, j) = mono(e, &xi_nodes[i * dim..(i + 1) * dim]);
            }
        }
        let c = v.inverse().unwrap(); // N_k(ξ) = Σ_j c(j, k) m_j(ξ)
        let rule = fe.quadrature_rule(ty, g).unwrap().clone();
        let nq = rule.nb_points();
        let shapes = fe.shapes(ty, g).unwrap();
        let dshapes = fe.shapes_derivatives(ty, g).unwrap();
        let jac = fe.jacobians(ty, g).unwrap();
        let conn = mesh.connectivity(ty, g).unwrap();
        let mut worst: f64 = 0.0;
        for (e, cn) in conn.tuples().enumerate() {
            for q in 0..nq {
                let xi = rule.point(q, dim);
                let n: Vec<f64> = (0..nn)
                    .map(|k| (0..nn).map(|j| c.at(j, k) * mono(&mons[j], xi)).sum())
                    .collect();
                let mut dnds = MatrixX::zeros(dim, nn);
                for i in 0..dim {
                    for k in 0..nn {
                        *dnds.at_mut(i, k) =
                            (0..nn).map(|j| c.at(j, k) * dmono(&mons[j], xi, i)).sum();
                    }
                }
                let mut xt = MatrixX::zeros(nn, dim);
                for (k, &node) in cn.iter().enumerate() {
                    for d in 0..dim {
                        *xt.at_mut(k, d) = mesh.nodes().tuple(node as usize)[d];
                    }
                }
                let j = dnds.matmul(&xt).unwrap();
                let b = j.inverse().unwrap().matmul(&dnds).unwrap();
                let jw = j.det().unwrap() * rule.weights[q];
                let t = e * nq + q;
                for (k, nk) in n.iter().enumerate() {
                    worst = worst.nan_max((nk - shapes.tuple(t)[k]).abs());
                    for i in 0..dim {
                        worst = worst.nan_max((b.at(i, k) - dshapes.tuple(t)[i + k * dim]).abs());
                    }
                }
                worst = worst.nan_max((jw - jac.tuple(t)[0]).abs());
            }
        }
        assert!(worst <= 1e-10, "{}: max deviation {worst}", ty.name());
    }
}
