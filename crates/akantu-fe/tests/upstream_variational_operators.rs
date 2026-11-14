// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): test/test_fe_engine/test_variational_operators.cc
// and its fixture test/test_fe_engine/test_fe_engine_fixture.hh
// (TestFEMFixture.{evaluate_field_on_support, consistent_force, stiffness_matrix}).

//! `TestFEMFixture.{evaluate_field_on_support, consistent_force,
//! stiffness_matrix}` (TEST-021), typed over upstream's `TestElementTypes`
//! (the 12 regular types without `_point_1`).
//!
//! Upstream's three tests are a prototype of expression-style variational
//! operators: every evaluation (`u.eval(u_d)`, `FieldIntegrator::integrate
//! (stress * grad, …)`, `integrate(grad.transpose() * C * grad, …)`) is
//! commented out, so each test only builds the fixture (mesh
//! `<type>.msh`, bounds, `FEEngineTemplate<IntegratorGauss, ShapeLagrange>`
//! with id `my_fem`, which precomputes both ghost types), calls
//! `fem->initShapeFunctions()` and constructs the operator objects on a
//! `Support{empty_filter, fem, dim, type, _not_ghost}`. The pass criterion
//! is that none of this throws; the port keeps exactly those steps.
//!
//! Additions (no upstream assertion; the data the prototype operators would
//! read): the `GradientOperator`'s source `getShapesDerivatives(type)` and
//! the `NodalTensorField`'s interpolation target are checked to be sized
//! for the support (`nb_element · nb_quadrature_points` tuples of
//! `dim · nb_nodes_per_element`, resp. one value per quadrature point).
//! The commented-out integrals (`∫ σ : ∇N`, `∫ ∇Nᵀ C ∇N`) are covered by
//! `fe_engine_properties::constant_stress_patch_is_in_equilibrium` and the
//! `BtD`/`BtDB` records of `fe_engine_matches_oracle`.

mod common;

use akantu_core::{Array, ElementKind, ElementType, GhostType};
use akantu_fe::{FeEngine, shape::ShapeFamily};
use akantu_mesh::Mesh;

/// The fixture of `test_fe_engine_fixture.hh` after `SetUp()` and the
/// test's `this->fem->initShapeFunctions()`.
struct Fixture {
    ty: ElementType,
    mesh: Mesh,
    fem: FeEngine,
    nb_element: usize,
    nb_quadrature_points_total: usize,
}

fn set_up(ty: ElementType) -> Fixture {
    let mut mesh = common::fe_test_mesh(ty);
    // lower = mesh->getLowerBounds(); upper = mesh->getUpperBounds();
    mesh.compute_bounding_box().unwrap();
    let bbox = mesh.bbox();
    assert!(
        (0..mesh.spatial_dimension()).all(|d| bbox.lower[d] <= bbox.upper[d]),
        "{}: empty bounding box",
        ty.name()
    );
    let g = GhostType::NotGhost;
    let nb_element = mesh.nb_element(ty, g);
    // FEM(*mesh, dim, "my_fem"): the constructor precomputes both ghost types
    let mut fem = FeEngine::with_precomputation(
        &mesh,
        Some(mesh.spatial_dimension()),
        ElementKind::Regular,
        ShapeFamily::Lagrange,
        "my_fem",
    )
    .unwrap();
    let nb_quadrature_points_total = fem.nb_integration_points(ty, g).unwrap() * nb_element;
    // this->fem->initShapeFunctions() (default ghost type: _not_ghost)
    fem.init_shape_functions(&mesh, g).unwrap();
    Fixture {
        ty,
        mesh,
        fem,
        nb_element,
        nb_quadrature_points_total,
    }
}

/// `Support{empty_filter, *this->fem, dim, type, _not_ghost}` plus the
/// shape derivatives a `GradientOperator` on it would copy.
fn gradient_operator(f: &Fixture) -> &Array<f64> {
    let g = GhostType::NotGhost;
    let dim = f.mesh.spatial_dimension();
    let b = f.fem.shapes_derivatives(f.ty, g).unwrap();
    assert_eq!(b.len(), f.nb_quadrature_points_total, "{}", f.ty.name());
    assert_eq!(
        b.nb_component(),
        dim * f.ty.nb_nodes_per_element(),
        "{}",
        f.ty.name()
    );
    b
}

#[test]
fn test_fe_engine_evaluate_field_on_support() {
    for ty in common::test_element_types() {
        let f = set_up(ty);
        // NodalTensorField u("disp", support); Array<Real> u_d;
        // (u.eval(u_d) is commented out upstream). The interpolation it
        // would perform is sized for the support:
        let g = GhostType::NotGhost;
        let disp = Array::from_vec(vec![0.0; f.mesh.nb_nodes()], 1).unwrap();
        let mut u_d = Array::new(1);
        f.fem
            .interpolate_on_integration_points(&f.mesh, disp.view(), &mut u_d, 1, ty, g, None)
            .unwrap();
        assert_eq!(u_d.len(), f.nb_quadrature_points_total, "{}", ty.name());
        assert_eq!(f.nb_element, f.mesh.nb_element(ty, g));
    }
}

#[test]
fn test_fe_engine_consistent_force() {
    for ty in common::test_element_types() {
        let f = set_up(ty);
        // GradientOperator grad{support};
        // IntegrationPointTensorField stress("stress", support);
        // (FieldIntegrator::integrate(stress * grad, support) is commented
        // out upstream)
        let _grad = gradient_operator(&f);
    }
}

#[test]
fn test_fe_engine_stiffness_matrix() {
    for ty in common::test_element_types() {
        let f = set_up(ty);
        // GradientOperator grad{support};
        // IntegrationPointTensorField elastic_tensor("C", support);
        // (integrate(grad.transpose() * elastic_tensor * grad, support) is
        // commented out upstream)
        let _grad = gradient_operator(&f);
    }
}
