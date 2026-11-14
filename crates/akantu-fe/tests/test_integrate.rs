// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): test/test_fe_engine/test_integrate.cc

//! `TestFEMFixture.IntegrateConstant`: interpolating the constant `(1, 2)`
//! and integrating it over the elements of the unit test meshes gives
//! `(1, 2)` (upstream tolerance `1e-14`); the scalar integral (`Math::reduce`)
//! of the first component is checked as well.

mod common;

use akantu_core::{Array, ElementKind, GhostType};
use akantu_fe::{FeEngine, shape::ShapeFamily};
use akantu_validation::reduce::NanMinMax;

#[test]
fn test_integrate_constant() {
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
        let g = GhostType::NotGhost;
        let value = [1.0, 2.0];
        let mut nodal = Array::new(2);
        for _ in 0..mesh.nb_nodes() {
            nodal.push_tuple(&value);
        }
        let mut uq = Array::new(2);
        fe.interpolate_on_integration_points(&mesh, nodal.view(), &mut uq, 2, ty, g, None)
            .unwrap();
        let mut per_element = Array::new(2);
        fe.integrate(uq.view(), &mut per_element, 2, ty, g, None)
            .unwrap();
        assert_eq!(per_element.len(), mesh.nb_element(ty, g));
        let mut sum = [0.0, 0.0];
        for v in per_element.tuples() {
            sum[0] += v[0];
            sum[1] += v[1];
        }
        let diff = (value[0] - sum[0]).abs().nan_max((value[1] - sum[1]).abs());
        assert!(diff <= 1e-14, "{}: {diff}", ty.name());
        // scalar version
        let first: Vec<f64> = uq.tuples().map(|t| t[0]).collect();
        let first = Array::from_vec(first, 1).unwrap();
        let s = fe.integrate_scalar(first.view(), ty, g, None).unwrap();
        assert!((s - 1.0).abs() <= 1e-14, "{}: {s}", ty.name());
    }
}
