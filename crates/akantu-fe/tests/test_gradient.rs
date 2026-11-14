// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): test/test_fe_engine/test_gradient.cc

//! `TestFEMFixture.GradientPoly`: the gradient of the linear field
//! `(α₀·x, α₁·x)` is `α` at every quadrature point (upstream tolerance
//! `5e-13`); `TestFEMFixture.GradientPositions`: the gradient of the node
//! coordinates is the identity (`2e-14`).

mod common;

use akantu_core::{Array, ElementKind, GhostType};
use akantu_fe::{FeEngine, shape::ShapeFamily};
use akantu_validation::reduce::NanMinMax;

#[test]
fn test_gradient_poly() {
    let alpha = [[13.0, 23.0, 31.0], [11.0, 7.0, 5.0]];
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
        let mut val = Array::new(2);
        for x in mesh.nodes().tuples() {
            let mut c = [0.0, 0.0];
            for d in 0..dim {
                c[0] += alpha[0][d] * x[d];
                c[1] += alpha[1][d] * x[d];
            }
            val.push_tuple(&c);
        }
        let mut grad = Array::new(2 * dim);
        fe.gradient_on_integration_points(
            &mesh,
            val.view(),
            &mut grad,
            2,
            ty,
            GhostType::NotGhost,
            None,
        )
        .unwrap();
        for g in grad.tuples() {
            for d in 0..dim {
                // column-major 2 × dim: (i, d) at i + 2 d
                assert!(
                    (g[2 * d] - alpha[0][d]).abs() <= 5e-13,
                    "{}: {}",
                    ty.name(),
                    g[2 * d]
                );
                assert!((g[1 + 2 * d] - alpha[1][d]).abs() <= 5e-13, "{}", ty.name());
            }
        }
    }
}

#[test]
fn test_gradient_positions() {
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
        let mut grad = Array::new(dim * dim);
        fe.gradient_on_integration_points(
            &mesh,
            mesh.nodes().view(),
            &mut grad,
            dim,
            ty,
            GhostType::NotGhost,
            None,
        )
        .unwrap();
        assert_eq!(
            grad.len(),
            mesh.nb_element(ty, GhostType::NotGhost)
                * fe.nb_integration_points(ty, GhostType::NotGhost).unwrap()
        );
        for g in grad.tuples() {
            let mut diff: f64 = 0.0;
            for i in 0..dim {
                for j in 0..dim {
                    let id = if i == j { 1.0 } else { 0.0 };
                    diff = diff.nan_max((id - g[i + j * dim]).abs());
                }
            }
            assert!(diff <= 2e-14, "{}: {diff}", ty.name());
        }
    }
}
