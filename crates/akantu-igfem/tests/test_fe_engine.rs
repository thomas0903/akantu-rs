// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): extra_packages/igfem/test/test_fe_engine/
// test_interpolate.cc, test_integrate.cc, test_gradient.cc,
// test_fe_engine_precomputation.cc (+ their `.verified` outputs).

//! Ports of the IGFEM FE-engine tests (TEST-193…196). Upstream asserts the
//! integration and gradient results with `3e-13` and compares the printed
//! output with the `.verified` files; both are checked here: the upstream
//! criteria verbatim, and every printed array value (6 significant digits,
//! upstream's stream precision) against the `.verified` files.

mod common;

use akantu_core::{Array, ElementType, GhostType, Real};
use akantu_igfem::IgfemFeEngine;
use common::{assert_matches_verified, generate_igfem_mesh, parse_verified, upstream};

const TYPES: [(ElementType, &str); 2] = [
    (
        ElementType::IgfemTriangle4,
        "mesh_info_igfem_triangle_4.txt",
    ),
    (
        ElementType::IgfemTriangle5,
        "mesh_info_igfem_triangle_5.txt",
    ),
];

const NB_STANDARD_NODES: usize = 9;

fn engine(ty: ElementType, file: &str) -> (akantu_mesh::Mesh, IgfemFeEngine) {
    let mesh = generate_igfem_mesh(ty, 2, &upstream(file));
    let fe = IgfemFeEngine::with_precomputation(&mesh, Some(2), "my_fem").unwrap();
    (mesh, fe)
}

/// `test_integrate.cc`: a constant `(1, 2)` on the quadrature points
/// integrates to `(1, 2)` over the unit square (`eps = 3e-13`); per-element
/// integrals as in `test_integrate.verified`.
#[test]
fn test_integrate() {
    let verified = parse_verified(&upstream("test_integrate.verified"));
    for (k, (ty, file)) in TYPES.into_iter().enumerate() {
        let (mesh, fe) = engine(ty, file);
        let nb_element = mesh.nb_element(ty, GhostType::NotGhost);
        let nq = fe.nb_integration_points(ty).unwrap() * nb_element;
        let mut val_on_quad = Array::new(2);
        for _ in 0..nq {
            val_on_quad.push_tuple(&[1.0, 2.0]);
        }
        let mut int_val_on_elem = Array::new(2);
        fe.integrate(
            &mesh,
            val_on_quad.view(),
            &mut int_val_on_elem,
            2,
            ty,
            GhostType::NotGhost,
            None,
        )
        .unwrap();
        let mut value = [0.0; 2];
        for t in int_val_on_elem.tuples() {
            value[0] += t[0];
            value[1] += t[1];
        }
        let eps = 3e-13;
        assert!((value[0] - 1.0).abs() < eps, "{ty}: |1 - {}|", value[0]);
        assert!((value[1] - 2.0).abs() < eps, "{ty}: |2 - {}|", value[1]);
        // printed arrays: val_on_quad, int_val_on_elem
        assert_matches_verified("val_on_quad", val_on_quad.as_slice(), 2, &verified[2 * k]);
        assert_matches_verified(
            "int_val_on_elem",
            int_val_on_elem.as_slice(),
            2,
            &verified[2 * k + 1],
        );
    }
}

/// `test_interpolate.cc`: a field constant on the standard nodes with zero
/// enrichments interpolates to the constant; the node coordinates with zero
/// enrichments interpolate to the quadrature-point coordinates. Upstream
/// only prints the results: every printed value is compared with
/// `test_interpolate.verified`.
#[test]
fn test_interpolate() {
    let verified = parse_verified(&upstream("test_interpolate.verified"));
    for (k, (ty, file)) in TYPES.into_iter().enumerate() {
        let (mesh, fe) = engine(ty, file);
        let mut const_val = Array::zeros(mesh.nb_nodes(), 2);
        for i in 0..NB_STANDARD_NODES {
            const_val.tuple_mut(i).copy_from_slice(&[1.0, 2.0]);
        }
        let mut val_on_quad = Array::new(2);
        fe.interpolate_on_integration_points(
            &mesh,
            const_val.view(),
            &mut val_on_quad,
            2,
            ty,
            GhostType::NotGhost,
            None,
        )
        .unwrap();
        let mut igfem_nodes = Array::new(2);
        fe.extract_values_at_standard_nodes(
            &mesh,
            mesh.nodes().view(),
            &mut igfem_nodes,
            GhostType::NotGhost,
        )
        .unwrap();
        let mut coord_on_quad = Array::new(2);
        fe.interpolate_on_integration_points(
            &mesh,
            igfem_nodes.view(),
            &mut coord_on_quad,
            2,
            ty,
            GhostType::NotGhost,
            None,
        )
        .unwrap();
        let v = &verified[4 * k..4 * k + 4];
        assert_matches_verified("const_val", const_val.as_slice(), 2, &v[0]);
        assert_matches_verified("val_on_quad", val_on_quad.as_slice(), 2, &v[1]);
        assert_matches_verified("node coordinates", mesh.nodes().as_slice(), 2, &v[2]);
        assert_matches_verified("coord_on_quad", coord_on_quad.as_slice(), 2, &v[3]);
        // the same through integration_points_coordinates
        let mut coords = Array::new(2);
        fe.integration_points_coordinates(&mesh, &mut coords, ty, GhostType::NotGhost, None)
            .unwrap();
        assert_eq!(coords.as_slice(), coord_on_quad.as_slice());
    }
}

/// `test_gradient.cc`: the gradient of a linear field `alpha · x` set on
/// the standard nodes (zero enrichments) is `alpha` at every quadrature
/// point, and the gradient of the node coordinates is the identity
/// (`eps = 3e-13`); printed arrays compared with `test_gradient.verified`.
#[test]
fn test_gradient() {
    let verified = parse_verified(&upstream("test_gradient.verified"));
    let alpha: [[Real; 3]; 2] = [[13.0, 23.0, 31.0], [11.0, 7.0, 5.0]];
    let eps = 3e-13;
    for (k, (ty, file)) in TYPES.into_iter().enumerate() {
        let (mesh, fe) = engine(ty, file);
        let dim = 2;
        let position = mesh.nodes();
        let mut const_val = Array::zeros(mesh.nb_nodes(), 2);
        for i in 0..NB_STANDARD_NODES {
            let x = position.tuple(i);
            let t = const_val.tuple_mut(i);
            for d in 0..dim {
                t[0] += alpha[0][d] * x[d];
                t[1] += alpha[1][d] * x[d];
            }
        }
        let mut grad_on_quad = Array::new(2 * dim);
        fe.gradient_on_integration_points(
            &mesh,
            const_val.view(),
            &mut grad_on_quad,
            2,
            ty,
            GhostType::NotGhost,
            None,
        )
        .unwrap();
        for g in grad_on_quad.tuples() {
            for d in 0..dim {
                // column-major 2 × dim: g(i, d) at i + 2 d
                assert!((g[2 * d] - alpha[0][d]).abs() < eps, "{ty}: {g:?}");
                assert!((g[1 + 2 * d] - alpha[1][d]).abs() < eps, "{ty}: {g:?}");
            }
        }
        let mut igfem_nodes = Array::new(dim);
        fe.extract_values_at_standard_nodes(
            &mesh,
            mesh.nodes().view(),
            &mut igfem_nodes,
            GhostType::NotGhost,
        )
        .unwrap();
        let mut grad_coord_on_quad = Array::new(dim * dim);
        fe.gradient_on_integration_points(
            &mesh,
            igfem_nodes.view(),
            &mut grad_coord_on_quad,
            dim,
            ty,
            GhostType::NotGhost,
            None,
        )
        .unwrap();
        for g in grad_coord_on_quad.tuples() {
            for i in 0..dim {
                for j in 0..dim {
                    let expected = if i == j { 1.0 } else { 0.0 };
                    assert!((g[i + dim * j] - expected).abs() < eps, "{ty}: {g:?}");
                }
            }
        }
        let v = &verified[4 * k..4 * k + 4];
        assert_matches_verified("const_val", const_val.as_slice(), 2, &v[0]);
        assert_matches_verified("grad_on_quad", grad_on_quad.as_slice(), 4, &v[1]);
        assert_matches_verified("node positions", mesh.nodes().as_slice(), 2, &v[2]);
        assert_matches_verified(
            "grad_coord_on_quad",
            grad_coord_on_quad.as_slice(),
            4,
            &v[3],
        );
    }
}

/// `test_fe_engine_precomputation.cc`: upstream prints the FE engine (mesh,
/// precomputed shapes, shape derivatives and Jacobians); every printed value
/// is compared with `test_fe_engine_precomputation.verified`.
#[test]
fn test_fe_engine_precomputation() {
    let verified = parse_verified(&upstream("test_fe_engine_precomputation.verified"));
    // per type: my_fem, mesh, coordinates, connectivities, shapes,
    // shapes_derivatives, jacobians (arrays with values only)
    let arrays: Vec<_> = verified
        .iter()
        .filter(|a| {
            a.values
                .first()
                .is_some_and(|t| !t.is_empty() && !t[0].is_empty())
        })
        .collect();
    assert_eq!(arrays.len(), 10, "{arrays:#?}");
    let mut compared = 0;
    for (k, (ty, file)) in TYPES.into_iter().enumerate() {
        let (mesh, fe) = engine(ty, file);
        let a = &arrays[5 * k..5 * k + 5];
        assert!(a[0].id.ends_with("coordinates"));
        compared += assert_matches_verified("coordinates", mesh.nodes().as_slice(), 2, a[0]);
        let conn: Vec<Real> = mesh
            .connectivity(ty, GhostType::NotGhost)
            .unwrap()
            .as_slice()
            .iter()
            .map(|&n| Real::from(n))
            .collect();
        compared += assert_matches_verified("connectivity", &conn, ty.nb_nodes_per_element(), a[1]);
        let nn = ty.nb_nodes_per_interpolation_element();
        assert!(a[2].id.contains("shapes_generic"));
        compared += assert_matches_verified(
            "shapes",
            fe.shapes(&mesh, ty, GhostType::NotGhost)
                .unwrap()
                .as_slice(),
            nn,
            a[2],
        );
        assert!(a[3].id.contains("shapes_derivatives_generic"));
        compared += assert_matches_verified(
            "shapes_derivatives",
            fe.shapes_derivatives(&mesh, ty, GhostType::NotGhost)
                .unwrap()
                .as_slice(),
            2 * nn,
            a[3],
        );
        assert!(a[4].id.contains("jacobians"));
        compared += assert_matches_verified(
            "jacobians",
            fe.jacobians(&mesh, ty, GhostType::NotGhost)
                .unwrap()
                .as_slice(),
            1,
            a[4],
        );
    }
    // 2 × (13·2 | 19·2 coordinates …): every printed value was compared
    assert!(compared > 500, "{compared}");
}
