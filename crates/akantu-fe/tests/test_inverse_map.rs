// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): test/test_fe_engine/test_inverse_map.cc

//! `TestFEMFixture.InverseMap`: the inverse map of every quadrature point's
//! physical position returns its natural coordinates (normalised distance
//! `≤ 3.5e-11`, upstream tolerance) and `contains` accepts the point.

mod common;

use akantu_core::{Array, Element, ElementKind, GhostType};
use akantu_fe::{FeEngine, shape::ShapeFamily};
use akantu_validation::reduce::NanMinMax;

#[test]
fn test_inverse_map() {
    for ty in common::test_element_types() {
        let mut mesh = common::fe_test_mesh(ty);
        mesh.compute_bounding_box().unwrap();
        let g = GhostType::NotGhost;
        let fe = FeEngine::with_precomputation(
            &mesh,
            None,
            ElementKind::Regular,
            ShapeFamily::Lagrange,
            "my_fem",
        )
        .unwrap();
        let dim = mesh.spatial_dimension();
        let rule = akantu_fe::quadrature::default_rule(ty).unwrap();
        let mut coords = Array::new(dim);
        fe.interpolate_on_integration_points(
            &mesh,
            mesh.nodes().view(),
            &mut coords,
            dim,
            ty,
            g,
            None,
        )
        .unwrap();
        let bbox = mesh.bbox();
        let length = (0..dim)
            .map(|d| (bbox.upper[d] - bbox.lower[d]).abs())
            .fold(0.0, NanMinMax::nan_max);
        let nq = rule.nb_points();
        let mut natural = vec![0.0; dim];
        for e in 0..mesh.nb_element(ty, g) {
            let el = Element::new(ty, e as i32, g);
            for q in 0..nq {
                let x = coords.tuple(e * nq + q);
                fe.inverse_map(&mesh, x, el, &mut natural).unwrap();
                let r = rule.point(q, dim);
                let d = r
                    .iter()
                    .zip(&natural)
                    .map(|(a, b)| (a - b) * (a - b))
                    .sum::<f64>()
                    .sqrt()
                    / length;
                assert!(d <= 3.5e-11, "{}: element {e}, point {q}: {d}", ty.name());
                assert!(
                    fe.contains(&mesh, x, el).unwrap(),
                    "{}: element {e}, point {q}",
                    ty.name()
                );
            }
        }
    }
}
