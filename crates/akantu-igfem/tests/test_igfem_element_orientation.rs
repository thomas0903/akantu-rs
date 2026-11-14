// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): extra_packages/igfem/test/test_solid_mechanics_model_igfem/
// test_igfem_element_orientation.cc (+ the `_parallel` registration, 10
// MPI ranks). Built only with the feature `cgal` (GPL-3.0-or-later sphere
// intersector, decision D-042).

//! TEST-203: a circle through two mesh nodes (radius `√(0.2² + 0.2²) +
//! √2·1e-11`) creates 2 `_igfem_triangle_4` that are enriched only for
//! compatibility with their neighbours (no material interface inside), and
//! 1 `_igfem_triangle_5`; 27 nodes. Both sub-elements of every
//! `_igfem_triangle_4` must get the `aggregate` Lamé constants (`Σ|Δλ| +
//! |Δμ| ≤ 1e-14`). Serial only (UD-609).

mod common;

use akantu_core::{ElementType, GhostType, Real};
use akantu_geometry_cgal::SkSphere;
use akantu_igfem::material::MaterialIgfemElastic;
use common::model::*;

const DAT: &str = include_str!("data/upstream/smm_igfem/material.dat");

#[test]
fn test_igfem_element_orientation() {
    let mesh = common::read_msh(2, "smm_igfem/test_igfem_element_orientation.msh", "mesh");
    let mut model = model(mesh, DAT);
    init_static(&mut model);
    let tol = 2f64.sqrt() * 1e-11;
    let radius = (0.2f64 * 0.2 + 0.2 * 0.2).sqrt() + tol;
    let sphere = SkSphere::try_from_f64([-0.2, 0.3, 0.0], radius * radius).unwrap();
    model.register_geometry_object(vec![sphere], "gel").unwrap();
    model.update("").unwrap();
    let c = counts(&model.mesh().read().unwrap());
    assert_eq!(
        (c[1], c[2]),
        (2, 1),
        "something went wrong in the interface creation"
    );
    assert_eq!(
        model.mesh().read().unwrap().nb_nodes(),
        27,
        "something went wrong in the interface creation"
    );
    let get = |name: &str, p: &str| match model
        .smm
        .material_by_name(name)
        .unwrap()
        .get_param(p)
        .unwrap()
    {
        akantu_parser::ParamValue::Real(v) => v,
        v => panic!("{v:?}"),
    };
    let (lambda_1, mu_1) = (get("aggregate", "lambda"), get("aggregate", "mu"));
    let law = model.smm.material_by_name("igfem_elastic").unwrap();
    let m = law
        .as_any()
        .unwrap()
        .downcast_ref::<MaterialIgfemElastic>()
        .unwrap();
    let lambda = m
        .lambda
        .get(ElementType::IgfemTriangle4, GhostType::NotGhost)
        .unwrap();
    let mu =
        m.mu.get(ElementType::IgfemTriangle4, GhostType::NotGhost)
            .unwrap();
    assert_eq!(lambda.len(), 2 * 2);
    let mut error: Real = 0.0;
    for (l, u) in lambda.as_slice().iter().zip(mu.as_slice()) {
        error += (lambda_1 - l).abs();
        error += (mu_1 - u).abs();
    }
    eprintln!("The error is: {error}");
    assert!(error <= 1e-14, "The test failed: {error}");
}
