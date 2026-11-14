// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): test/test_fe_engine/test_fe_engine_gauss_integration.cc
// (polynomial coefficients copied from the upstream test).

//! `TestGaussIntegrationFixture.ArbitraryOrder`: for every regular type and
//! polynomial degree `0..=5`, integrating `p_d(x_d)` (degree-`k` truncation
//! of upstream's fixed polynomials) over the unit test meshes with the
//! Gauss rule of order `max(1, k)` (`IntegratorGauss::integrate<type,
//! order>`, Jacobians computed on the fly, `Math::reduce`) equals the exact
//! integral (upstream tolerance `5e-14`).

mod common;

use akantu_core::{Array, ElementKind, GhostType};
use akantu_fe::{FeEngine, gauss_rule, shape::ShapeFamily};

const POLYS: [[f64; 6]; 3] = [
    [
        0.40062394, 0.13703225, 0.51731446, 0.87830084, 0.5410543, 0.71842292,
    ],
    [
        0.41861835, 0.11080576, 0.49874043, 0.49077504, 0.85073835, 0.66259755,
    ],
    [
        0.92620845, 0.7503478, 0.62962232, 0.31662719, 0.64069644, 0.30878135,
    ],
];

fn eval(c: &[f64; 6], degree: usize, x: f64) -> f64 {
    (0..=degree).fold(0.0, |r, k| r + c[k] * x.powi(k as i32))
}

fn primitive(c: &[f64; 6], degree: usize, x: f64) -> f64 {
    (0..=degree).fold(0.0, |r, k| {
        r + c[k] / (k as f64 + 1.0) * x.powi(k as i32 + 1)
    })
}

#[test]
fn test_fe_engine_gauss_integration() {
    let g = GhostType::NotGhost;
    for ty in common::test_element_types() {
        let mut mesh = common::fe_test_mesh(ty);
        mesh.compute_bounding_box().unwrap();
        let (lower, upper) = (mesh.bbox().lower.clone(), mesh.bbox().upper.clone());
        let dim = mesh.spatial_dimension();
        let fe = FeEngine::with_precomputation(
            &mesh,
            None,
            ElementKind::Regular,
            ShapeFamily::Lagrange,
            "my_fem",
        )
        .unwrap();
        for degree in 0..=5usize {
            let order = degree.max(1);
            let rule = gauss_rule(ty, order).unwrap();
            let nq = rule.nb_points();
            // integration point positions with the order-n shapes
            let mut shapes = Array::new(1);
            fe.compute_shapes_at_natural(ty, &rule.points, &mut shapes, None)
                .unwrap();
            let conn = mesh.connectivity(ty, g).unwrap();
            let mut pos = Vec::new();
            for c in conn.tuples() {
                for q in 0..nq {
                    let n = shapes.tuple(q);
                    for d in 0..dim {
                        pos.push(
                            c.iter()
                                .zip(n)
                                .map(|(&node, &nk)| nk * mesh.nodes().tuple(node as usize)[d])
                                .sum::<f64>(),
                        );
                    }
                }
            }
            for d in 0..dim {
                let poly = &POLYS[d];
                let f: Vec<f64> = pos.chunks(dim).map(|x| eval(poly, degree, x[d])).collect();
                let f = Array::from_vec(f, 1).unwrap();
                let res = fe
                    .integrate_scalar_with_order(&mesh, f.view(), ty, g, order)
                    .unwrap();
                let mut expect =
                    primitive(poly, degree, upper[d]) - primitive(poly, degree, lower[d]);
                for o in 0..dim {
                    if o != d {
                        expect *= upper[d] - lower[d];
                    }
                }
                assert!(
                    (expect - res).abs() <= 5e-14,
                    "{}:{degree}: d={d} expect {expect} got {res}",
                    ty.name()
                );
            }
        }
    }
}
