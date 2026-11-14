// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): test/test_fe_engine/test_interpolate.cc

//! `TestFEMFixture.InterpolateConstant`: a constant nodal field interpolates
//! to the same constant at every quadrature point (upstream tolerance
//! `1e-14`, partition of unity of the shapes).

mod common;

use akantu_core::{Array, ElementKind, GhostType};
use akantu_fe::{FeEngine, shape::ShapeFamily};
use akantu_validation::reduce::NanMinMax;

#[test]
fn test_interpolate_constant() {
    for ty in common::test_element_types() {
        let mesh = common::fe_test_mesh(ty);
        let fe = FeEngine::with_precomputation(
            &mesh,
            None,
            ElementKind::Regular,
            ShapeFamily::Lagrange,
            "my_fem",
        )
        .unwrap();
        let value = [1.0, 2.0];
        let mut nodal = Array::new(2);
        for _ in 0..mesh.nb_nodes() {
            nodal.push_tuple(&value);
        }
        let mut uq = Array::new(2);
        fe.interpolate_on_integration_points(
            &mesh,
            nodal.view(),
            &mut uq,
            2,
            ty,
            GhostType::NotGhost,
            None,
        )
        .unwrap();
        assert_eq!(
            uq.len(),
            mesh.nb_element(ty, GhostType::NotGhost)
                * fe.nb_integration_points(ty, GhostType::NotGhost).unwrap()
        );
        for v in uq.tuples() {
            let diff = (value[0] - v[0]).abs().nan_max((value[1] - v[1]).abs());
            assert!(diff <= 1e-14, "{}: {diff}", ty.name());
        }
    }
}
