// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs (port of Akantu, EPFL/LSMS): permanent regression tests
// for the FE-layer upstream defects/quirks of docs/UPSTREAM_DEFECTS.md
// (UD-043, UD-096 … UD-101). Upstream sources:
// src/fe_engine/fe_engine_template_tmpl.hh, shape_functions.cc,
// shape_functions_inline_impl.hh, shape_cohesive_inline_impl.hh,
// shape_lagrange_inline_impl.hh, integrator_gauss_inline_impl.hh,
// element_class_tmpl.hh. No upstream test exercises these paths.

//! Each test pins the port's documented replacement (or preserved)
//! behaviour with analytical expectations; the doc comment of every test
//! names its `UD-` entry and the upstream behaviour it deviates from.

mod common;

use akantu_core::{
    Array, Element, ElementKind, ElementType, ElementTypeMapArray, Error, GhostType, Idx,
};
use akantu_fe::FeEngine;
use akantu_fe::element_class::{LagrangeElement, types};
use akantu_fe::geometry;
use akantu_fe::kernels;
use akantu_fe::shape::{CohesiveReduction, ShapeFamily};
use akantu_mesh::data::MeshDataValues;
use akantu_mesh::{Mesh, MeshAccessor};

const G: GhostType = GhostType::NotGhost;

fn regular(mesh: &Mesh, dim: Option<usize>) -> FeEngine {
    FeEngine::with_precomputation(mesh, dim, ElementKind::Regular, ShapeFamily::Lagrange, "fe")
        .unwrap()
}

fn element_lists(ty: ElementType, lists: &[Vec<Element>]) -> MeshDataValues {
    let mut m: ElementTypeMapArray<Vec<Element>> =
        ElementTypeMapArray::new("element_to_subelement");
    let a = m.alloc(0, 1, ty, G, Vec::new()).unwrap();
    for l in lists {
        a.push_value(l.clone());
    }
    MeshDataValues::ElementList(m)
}

fn seg(e: Idx) -> Element {
    Element::new(ElementType::Segment2, e, G)
}

/// **UD-043** (point normals use the element index as a node index).
/// Upstream `computeNormalsOnIntegrationPoints<_point_1>` reads
/// `coords(elem)`; the port reads the coordinate of the point's node.
/// Nodes `x = [0, 1, 2]`, segments `[0,1]`, `[1,2]`, point elements on the
/// deliberately permuted nodes `[2, 0, 1]`: outward normals `+1` (right end,
/// node 2), `−1` (left end, node 0), `1` (interior node 1, two segments).
/// Upstream would evaluate `coords(0) − coords(1) = −1` for point 0.
#[test]
fn ud043_point_normals_follow_the_connectivity() {
    let mesh = point_mesh(false);
    let mut fe = regular(&mesh, Some(0));
    fe.compute_normals_on_integration_points(&mesh, G).unwrap();
    let n = fe
        .normals_on_integration_points(ElementType::Point1, G)
        .unwrap();
    assert_eq!(n.as_slice(), &[1.0, -1.0, 1.0]);

    // malformed adjacency data is an error, not a panic: an
    // extra point whose single "segment" is a one-node element
    let bad = point_mesh(true);
    let mut fe_bad = regular(&bad, Some(0));
    assert!(matches!(
        fe_bad.compute_normals_on_integration_points(&bad, G),
        Err(Error::Mesh(_))
    ));
}

/// Nodes `x = [0, 1, 2]`, segments `[0,1]`, `[1,2]`, point elements on the
/// nodes `[2, 0, 1]` (plus, if `malformed`, a point whose adjacent element
/// is itself a point element).
fn point_mesh(malformed: bool) -> Mesh {
    let mut mesh = Mesh::new(1, "points").unwrap();
    let mut acc = MeshAccessor::new(&mut mesh);
    for x in [0.0, 1.0, 2.0] {
        acc.push_node(&[x]).unwrap();
    }
    let mut s = acc.connectivity_mut(ElementType::Segment2, G);
    s.push_tuple(&[0, 1]);
    s.push_tuple(&[1, 2]);
    drop(s);
    let mut p = acc.connectivity_mut(ElementType::Point1, G);
    for n in [2, 0, 1] {
        p.push_tuple(&[n]);
    }
    drop(p);
    let mut lists = vec![vec![seg(1)], vec![seg(0)], vec![seg(0), seg(1)]];
    if malformed {
        acc.connectivity_mut(ElementType::Point1, G)
            .push_tuple(&[0]);
        lists.push(vec![Element::new(ElementType::Point1, 0, G)]);
    }
    acc.mesh_mut().data_mut().insert_elemental(
        "element_to_subelement",
        element_lists(ElementType::Point1, &lists),
    );
    mesh
}

/// The segment3 mesh: elements on `[0, 1]` and `[2, 4]`.
fn segment3_mesh() -> Mesh {
    let mut mesh = Mesh::new(1, "seg3").unwrap();
    let mut acc = MeshAccessor::new(&mut mesh);
    for x in [0.0, 1.0, 0.5, 2.0, 4.0, 3.0] {
        acc.push_node(&[x]).unwrap();
    }
    let mut c = acc.connectivity_mut(ElementType::Segment3, G);
    c.push_tuple(&[0, 1, 2]);
    c.push_tuple(&[3, 4, 5]);
    drop(c);
    mesh
}

/// **UD-096** (elemental-field interpolation without element filter).
/// Upstream `initElementalFieldInterpolationFromIntegrationPoints` /
/// `interpolateElementalFieldFromIntegrationPoints` zip over the (empty)
/// default filter and compute nothing; the port interpolates all elements.
/// `f(x) = x` at the quadrature points, targets `0.25` and `3.5` ⇒
/// `[0.25, 3.5]` (the order-2 fit reproduces the affine field). The types
/// whose upstream matrices are out of bounds / uninitialised (hex8, penta6,
/// penta15) and hex20 (`AKANTU_TO_IMPLEMENT`) are `NotImplemented`.
#[test]
fn ud096_elemental_interpolation_without_filter() {
    let ty = ElementType::Segment3;
    let mesh = segment3_mesh();
    let fe = regular(&mesh, None);
    let mut points = ElementTypeMapArray::new("points");
    let p = points.alloc(0, 1, ty, G, 0.0).unwrap();
    p.push_value(0.25);
    p.push_value(3.5);
    let interp = fe
        .init_elemental_field_interpolation(&mesh, &points, None)
        .unwrap();
    let mut xq = Array::new(1);
    fe.integration_points_coordinates(&mesh, &mut xq, ty, G, None)
        .unwrap();
    let mut field = ElementTypeMapArray::new("field");
    let f = field.alloc(0, 1, ty, G, 0.0).unwrap();
    for &x in xq.as_slice() {
        f.push_value(x);
    }
    let mut out = ElementTypeMapArray::new("out");
    fe.interpolate_elemental_field(&mesh, &interp, &field, &mut out, G, None)
        .unwrap();
    let o = out.get(ty, G).unwrap().as_slice();
    assert!((o[0] - 0.25).abs() <= 1e-15, "{}", o[0]);
    assert!((o[1] - 3.5).abs() <= 4e-15, "{}", o[1]);

    for unsupported in [
        ElementType::Hexahedron8,
        ElementType::Hexahedron20,
        ElementType::Pentahedron6,
        ElementType::Pentahedron15,
    ] {
        let m = common::fe_test_mesh(unsupported);
        let fe = regular(&m, None);
        let points = ElementTypeMapArray::new("points");
        assert!(
            matches!(
                fe.init_elemental_field_interpolation(&m, &points, None),
                Err(Error::NotImplemented(_))
            ),
            "{}",
            unsupported.name()
        );
    }
}

/// Two `_cohesive_1d_2` elements in a 1-D mesh of three segments
/// (`[0,1]`, `[1,2]`, `[2,3]`, duplicated nodes at the cracks) with the
/// facet mesh data the 1-D cohesive normals read. The second cohesive
/// element lists its facets in reversed order (right segment first).
fn cohesive_1d_mesh() -> Mesh {
    let mut mesh = Mesh::new(1, "coh1d").unwrap();
    let mut acc = MeshAccessor::new(&mut mesh);
    for x in [0.0, 1.0, 1.0, 2.0, 2.0, 3.0] {
        acc.push_node(&[x]).unwrap();
    }
    let mut s = acc.connectivity_mut(ElementType::Segment2, G);
    s.push_tuple(&[0, 1]);
    s.push_tuple(&[2, 3]);
    s.push_tuple(&[4, 5]);
    drop(s);
    let mut c = acc.connectivity_mut(ElementType::Cohesive1d2, G);
    c.push_tuple(&[1, 2]);
    c.push_tuple(&[4, 3]);
    drop(c);
    // facet mesh: point facets f0 = node 1 (seg 0), f1 = node 2 (seg 1),
    // f2 = node 3 (seg 1), f3 = node 4 (seg 2)
    let mut facets = Mesh::new(1, "coh1d:facets").unwrap();
    {
        let mut fa = MeshAccessor::new(&mut facets);
        for x in [0.0, 1.0, 1.0, 2.0, 2.0, 3.0] {
            fa.push_node(&[x]).unwrap();
        }
        let mut p = fa.connectivity_mut(ElementType::Point1, G);
        for n in [1, 2, 3, 4] {
            p.push_tuple(&[n]);
        }
        drop(p);
        let pt = |i: Idx| Element::new(ElementType::Point1, i, G);
        let mut s2e: ElementTypeMapArray<Element> =
            ElementTypeMapArray::new("subelement_to_element");
        let a = s2e
            .alloc(0, 2, ElementType::Cohesive1d2, G, Element::NULL)
            .unwrap();
        a.push_tuple(&[pt(0), pt(1)]);
        a.push_tuple(&[pt(3), pt(2)]); // reversed adjacent-segment ordering
        let data = fa.mesh_mut().data_mut();
        data.insert_elemental("subelement_to_element", MeshDataValues::Element(s2e));
        data.insert_elemental(
            "element_to_subelement",
            element_lists(
                ElementType::Point1,
                &[vec![seg(0)], vec![seg(1)], vec![seg(1)], vec![seg(2)]],
            ),
        );
    }
    acc.set_mesh_facets(facets);
    mesh
}

/// **UD-097** (filtered 1-D cohesive normals written at the element id).
/// Upstream `ShapeLagrange<_ek_cohesive>::computeNormalsOnIntegrationPoints`
/// writes `normals_u(el)` with the filtered element id into filter-sized
/// storage (out of bounds for filter `[1]` of a two-element block); the
/// port writes at the filtered position. Normal = sign of
/// `barycentre(segment of facet 0) − barycentre(segment of facet 1)`:
/// element 0 → `0.5 − 1.5 < 0` ⇒ `−1`; element 1 (reversed facets) →
/// `2.5 − 1.5 > 0` ⇒ `+1`.
#[test]
fn ud097_filtered_1d_cohesive_normals_use_the_filtered_position() {
    let mesh = cohesive_1d_mesh();
    let ty = ElementType::Cohesive1d2;
    let fe = FeEngine::with_precomputation(
        &mesh,
        None,
        ElementKind::Cohesive,
        ShapeFamily::Cohesive,
        "coh",
    )
    .unwrap();
    assert_eq!(fe.nb_integration_points(ty, G).unwrap(), 1);
    let normals = |filter: Option<&[Idx]>| {
        let mut n = Array::new(1);
        fe.compute_cohesive_normals(
            &mesh,
            mesh.nodes().view(),
            &mut n,
            ty,
            G,
            CohesiveReduction::Mean,
            filter,
        )
        .unwrap();
        n.as_slice().to_vec()
    };
    assert_eq!(normals(None), [-1.0, 1.0]);
    assert_eq!(normals(Some(&[1])), [1.0]);
    assert_eq!(normals(Some(&[1, 0])), [1.0, -1.0]);
    assert_eq!(normals(Some(&[0])), [-1.0]);
}

/// **UD-098** (`computeBtDB` with a tensor order other than 2 or 4).
/// Upstream `ShapeLagrange::computeBtDB` has only the order-4 and order-2
/// branches and silently leaves the output untouched otherwise; the port
/// returns `Error::InvalidArgument` (and still computes orders 2 and 4).
#[test]
fn ud098_compute_btdb_rejects_other_orders() {
    let ty = ElementType::Triangle3;
    let mut mesh = Mesh::new(2, "tri").unwrap();
    {
        let mut acc = MeshAccessor::new(&mut mesh);
        for p in [[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]] {
            acc.push_node(&p).unwrap();
        }
        acc.connectivity_mut(ty, G).push_tuple(&[0, 1, 2]);
    }
    let fe = regular(&mesh, None);
    let nq = fe.nb_integration_points(ty, G).unwrap();
    let d4 = Array::from_vec(vec![1.0; nq * 9], 9).unwrap();
    let d2 = Array::from_vec(vec![1.0; nq * 4], 4).unwrap();
    for order in [0, 1, 3, 5, 8] {
        let mut out = Array::from_vec(vec![42.0; 3], 1).unwrap();
        assert!(
            matches!(
                fe.compute_btdb(d4.view(), &mut out, order, ty, G, None),
                Err(Error::InvalidArgument(_))
            ),
            "order {order}"
        );
        assert_eq!(out.as_slice(), &[42.0; 3], "order {order}: output modified");
    }
    let mut out = Array::new(1);
    fe.compute_btdb(d4.view(), &mut out, 4, ty, G, None)
        .unwrap();
    assert_eq!(out.nb_component(), 36);
    fe.compute_btdb(d2.view(), &mut out, 2, ty, G, None)
        .unwrap();
    assert_eq!(out.nb_component(), 9);
    // D = I (2 × 2), B constant: BᵀB = ((2,−1,−1),(−1,1,0),(−1,0,1))
    let id = Array::from_vec([1.0, 0.0, 0.0, 1.0].repeat(nq), 4).unwrap();
    fe.compute_btdb(id.view(), &mut out, 2, ty, G, None)
        .unwrap();
    assert_eq!(
        out.tuple(0),
        &[2.0, -1.0, -1.0, -1.0, 1.0, 0.0, -1.0, 0.0, 1.0]
    );
}

/// **UD-099** (negative Jacobian leaves partial engine state).
/// Upstream `IntegratorGauss::computeJacobiansOnIntegrationPoints` stores the
/// raw Jacobians before the sign check and throws before weighting/shape
/// completion; the port computes a type into temporaries and stores nothing
/// for it on error (types processed earlier stay stored, previously stored
/// data of the failing type is left intact).
#[test]
fn ud099_negative_jacobian_stores_nothing_for_the_type() {
    let mesh = mixed_mesh(true);
    let mut fe = FeEngine::new(
        &mesh,
        None,
        ElementKind::Regular,
        ShapeFamily::Lagrange,
        "fe",
    )
    .unwrap();
    let r = fe.init_shape_functions(&mesh, G);
    assert!(matches!(r, Err(Error::Numerical(m)) if m.contains("Negative jacobian")));
    // triangle (processed first, enum order) is stored
    assert_eq!(
        fe.jacobians(ElementType::Triangle3, G).unwrap().as_slice(),
        &[0.5]
    );
    // nothing for the quadrangle: no Jacobians, shapes, derivatives or rule
    let q = ElementType::Quadrangle4;
    assert!(fe.jacobians(q, G).is_err());
    assert!(fe.shapes(q, G).is_err());
    assert!(fe.shapes_derivatives(q, G).is_err());
    assert!(fe.integration_points(q, G).is_err());

    // previously valid data of a type is left intact when a later
    // re-initialisation fails
    let good = mixed_mesh(false);
    let mut fe = regular(&good, None);
    let jac = fe.jacobians(q, G).unwrap().as_slice().to_vec();
    let shapes = fe.shapes(q, G).unwrap().as_slice().to_vec();
    assert!(jac.iter().all(|&j| j > 0.0));
    assert!(fe.init_shape_functions(&mesh, G).is_err());
    assert_eq!(fe.jacobians(q, G).unwrap().as_slice(), jac.as_slice());
    assert_eq!(fe.shapes(q, G).unwrap().as_slice(), shapes.as_slice());
}

/// A triangle and a unit quadrangle, the latter clockwise (`det J < 0`)
/// if `inverted`.
fn mixed_mesh(inverted: bool) -> Mesh {
    let mut mesh = Mesh::new(2, "mixed").unwrap();
    let mut acc = MeshAccessor::new(&mut mesh);
    for p in [
        [0.0, 0.0],
        [1.0, 0.0],
        [0.0, 1.0],
        [2.0, 0.0],
        [3.0, 0.0],
        [3.0, 1.0],
        [2.0, 1.0],
    ] {
        acc.push_node(&p).unwrap();
    }
    acc.connectivity_mut(ElementType::Triangle3, G)
        .push_tuple(&[0, 1, 2]);
    let quad: [Idx; 4] = if inverted { [3, 6, 5, 4] } else { [3, 4, 5, 6] };
    acc.connectivity_mut(ElementType::Quadrangle4, G)
        .push_tuple(&quad);
    mesh
}

/// **UD-100** (`contains` on point/cohesive shapes with an empty natural
/// coordinate vector). Upstream `GeometricalShapeContains<_gst_point>`
/// unconditionally reads `coords(0)`; for a zero-dimensional (empty) vector
/// the port treats the coordinate as `0` ⇒ inside. For non-empty
/// coordinates of cohesive types (shape `_gst_point` upstream) the odd
/// first-coordinate comparison `ξ₀ < ε` is preserved (classification C).
#[test]
fn ud100_contains_with_empty_coordinates() {
    let tol = akantu_core::math::DEFAULT_TOLERANCE;
    use akantu_core::element_type::GeometricalShapeType as S;
    assert!(geometry::contains(S::Point, &[], tol));
    assert!(types::Point1::contains(&[]));
    assert!(types::Cohesive1d2::contains(&[]));
    // preserved upstream comparison on non-empty coordinates
    assert!(types::Cohesive2d4::contains(&[-0.5]));
    assert!(!types::Cohesive2d4::contains(&[0.5]));
    assert!(types::Cohesive3d6::contains(&[0.0, 0.7]));
    assert!(!types::Cohesive3d6::contains(&[1e-15, -0.7]));
}

/// **UD-101** (inverse map fails on exact convergence in its last allowed
/// iteration). Upstream `ElementClass::inverseMap` increments the iteration
/// counter and then tests `iterations >= max_iterations` independently of
/// the final residual; the port preserves this (classification C),
/// including the zero-iteration limit. Segment2 on `[0, 1]`, target
/// `x = 0.75`: one Newton step lands exactly on `ξ = 0.5`.
#[test]
fn ud101_inverse_map_last_iteration_semantics() {
    let mut mesh = Mesh::new(1, "seg2").unwrap();
    {
        let mut acc = MeshAccessor::new(&mut mesh);
        acc.push_node(&[0.0]).unwrap();
        acc.push_node(&[1.0]).unwrap();
        acc.connectivity_mut(ElementType::Segment2, G)
            .push_tuple(&[0, 1]);
    }
    let fe = regular(&mesh, None);
    let el = Element::new(ElementType::Segment2, 0, G);
    let mut xi = [9.0];
    // limit 1: converged exactly in the last iteration, still an error
    let r = fe.inverse_map_with(&mesh, &[0.75], el, &mut xi, 1, 1e-10);
    assert!(
        matches!(r, Err(Error::NotConverged { niter: 1, error, .. }) if error == 0.0),
        "{r:?}"
    );
    assert_eq!(xi, [0.5], "the converged iterate is left in the output");
    // limit 2: the loop exits on the residual before the limit
    fe.inverse_map_with(&mesh, &[0.75], el, &mut xi, 2, 1e-10)
        .unwrap();
    assert_eq!(xi, [0.5]);
    // limit 0: even an exact start point (x = 0.5 ⇔ ξ = 0) is an error
    let r = fe.inverse_map_with(&mesh, &[0.5], el, &mut xi, 0, 1e-10);
    assert!(
        matches!(r, Err(Error::NotConverged { niter: 0, .. })),
        "{r:?}"
    );
    assert_eq!(xi, [0.0]);
    // same semantics in the element-level kernel
    let r = kernels::inverse_map::<types::Segment2>(&[0.75], &[0.0, 1.0], &mut xi, 1, 1e-10);
    assert!(matches!(r, Err(Error::NotConverged { niter: 1, .. })));
}
