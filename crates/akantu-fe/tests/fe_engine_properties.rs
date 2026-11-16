// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs (port of Akantu, EPFL/LSMS): independent mathematical and
// behavioural checks of the element classes, Gauss rules and FE engine (no
// direct upstream counterpart; complements the ported test_fe_engine tests).

//! Mathematical properties: quadrature weights sum to the reference
//! measure, Gauss rules integrate monomials exactly up to their order,
//! iso-parametric reproduction of linear fields on curved elements,
//! patch-test equilibrium of constant stresses, consistent/lumped mass
//! identities, elemental-field interpolation of linear fields, cohesive
//! reductions; engine behaviour: configurable orders, mesh events, error
//! paths.

mod common;

use akantu_core::{
    Array, Element, ElementKind, ElementType, ElementTypeMapArray, Error, GhostType, Idx, Real,
    SharedElementTypeMapArray,
};
use akantu_fe::element_class::ElementClass;
use akantu_fe::quadrature::gauss_rule;
use akantu_fe::shape::{CohesiveReduction, IntegrationOrders, ShapeFamily};
use akantu_fe::{FeEngine, with_element_class};
use akantu_mesh::{Mesh, MeshAccessor};
use akantu_validation::Fixture;

const G: GhostType = GhostType::NotGhost;

fn factorial(n: u32) -> f64 {
    (1..=n).map(f64::from).product()
}

/// Exact integral of `x^a y^b z^c` over the reference element of `ty`.
fn reference_integral(ty: ElementType, e: [u32; 3]) -> f64 {
    let seg = |k: u32| {
        if k.is_multiple_of(2) {
            2.0 / f64::from(k + 1)
        } else {
            0.0
        }
    };
    let nd = ty.natural_space_dimension();
    use akantu_core::element_type::GaussIntegrationType as Gi;
    match ty.gauss_integration_type() {
        Gi::Point => 1.0,
        Gi::Segment => (0..nd).map(|d| seg(e[d])).product(),
        Gi::Triangle => factorial(e[0]) * factorial(e[1]) / factorial(e[0] + e[1] + 2),
        Gi::Tetrahedron => {
            factorial(e[0]) * factorial(e[1]) * factorial(e[2]) / factorial(e[0] + e[1] + e[2] + 3)
        }
        Gi::Pentahedron => {
            seg(e[0]) * factorial(e[1]) * factorial(e[2]) / factorial(e[1] + e[2] + 2)
        }
        Gi::NotDefined => unreachable!(),
    }
}

fn max_order(ty: ElementType) -> usize {
    match ty.gauss_integration_type() {
        akantu_core::element_type::GaussIntegrationType::Segment => 7,
        _ => 5,
    }
}

#[test]
fn quadrature_weights_sum_to_reference_measure_and_rules_are_exact() {
    for ty in common::REGULAR.into_iter().chain(common::COHESIVE) {
        let nd = ty.natural_space_dimension();
        for order in 1..=max_order(ty) {
            let r = gauss_rule(ty, order).unwrap();
            let measure: f64 = r.weights.iter().sum();
            let exact = reference_integral(ty, [0, 0, 0]);
            assert!(
                (measure - exact).abs() <= 1e-15 * r.nb_points() as f64,
                "{} order {order}",
                ty.name()
            );
            // every monomial of total degree <= order (prisms: x-degree and
            // yz-degree <= order) is integrated exactly
            for a in 0..=order as u32 {
                for b in 0..=(if nd > 1 { order as u32 } else { 0 }) {
                    for c in 0..=(if nd > 2 { order as u32 } else { 0 }) {
                        let total = a + b + c;
                        let pent = ty.gauss_integration_type()
                            == akantu_core::element_type::GaussIntegrationType::Pentahedron;
                        let ok = if pent {
                            a <= order as u32 && b + c <= order as u32
                        } else {
                            total <= order as u32
                        };
                        if !ok {
                            continue;
                        }
                        let e = [a, b, c];
                        let num: f64 = (0..r.nb_points())
                            .map(|q| {
                                let x = r.point(q, nd);
                                r.weights[q]
                                    * (0..nd).map(|d| x[d].powi(e[d] as i32)).product::<f64>()
                            })
                            .sum();
                        let ex = reference_integral(ty, e);
                        assert!(
                            (num - ex).abs() <= 1e-14,
                            "{} order {order} monomial {e:?}: {num} vs {ex}",
                            ty.name()
                        );
                    }
                }
            }
        }
    }
}

/// Single-element mesh with the oracle's curved (iso-parametric) node
/// coordinates of `fe/element_class/<type>`.
fn curved_element(ty: ElementType) -> Mesh {
    let fx = Fixture::open_by_id(&format!("fe/element_class/{}", ty.name())).unwrap();
    let x = fx.load_f64("physical/curved/nodes").unwrap();
    let dim = x.shape[1];
    let mut mesh = Mesh::new(dim, "curved").unwrap();
    let mut acc = MeshAccessor::new(&mut mesh);
    for p in x.data.chunks(dim) {
        acc.push_node(p).unwrap();
    }
    let conn: Vec<Idx> = (0..ty.nb_nodes_per_element() as Idx).collect();
    acc.connectivity_mut(ty, G).push_tuple(&conn);
    mesh
}

#[test]
fn linear_fields_are_reproduced_on_curved_elements() {
    let a = [[0.7, -1.3, 2.1], [1.9, 0.4, -0.6]];
    for ty in common::test_element_types() {
        let mesh = curved_element(ty);
        let dim = mesh.spatial_dimension();
        let fe = FeEngine::with_precomputation(
            &mesh,
            None,
            ElementKind::Regular,
            ShapeFamily::Lagrange,
            "fe",
        )
        .unwrap();
        let mut u = Array::new(2);
        for x in mesh.nodes().tuples() {
            let v: Vec<f64> = (0..2)
                .map(|i| 0.3 + (0..dim).map(|d| a[i][d] * x[d]).sum::<f64>())
                .collect();
            u.push_tuple(&v);
        }
        let mut grad = Array::new(2 * dim);
        fe.gradient_on_integration_points(&mesh, u.view(), &mut grad, 2, ty, G, None)
            .unwrap();
        let mut uq = Array::new(2);
        fe.interpolate_on_integration_points(&mesh, u.view(), &mut uq, 2, ty, G, None)
            .unwrap();
        let mut xq = Array::new(dim);
        fe.integration_points_coordinates(&mesh, &mut xq, ty, G, None)
            .unwrap();
        for (q, g) in grad.tuples().enumerate() {
            for i in 0..2 {
                for d in 0..dim {
                    assert!(
                        (g[i + 2 * d] - a[i][d]).abs() <= 1e-13,
                        "{} grad",
                        ty.name()
                    );
                }
                let exact = 0.3 + (0..dim).map(|d| a[i][d] * xq.tuple(q)[d]).sum::<f64>();
                assert!(
                    (uq.tuple(q)[i] - exact).abs() <= 1e-14,
                    "{} interpolation",
                    ty.name()
                );
            }
        }
        // the volume of the element is the integral of 1
        let one = Array::from_vec(vec![1.0; uq.len()], 1).unwrap();
        let vol = fe.integrate_scalar(one.view(), ty, G, None).unwrap();
        assert!(vol > 0.0);
    }
}

#[test]
fn constant_stress_patch_is_in_equilibrium() {
    for ty in common::test_element_types() {
        let mut mesh = common::fe_test_mesh(ty);
        mesh.compute_bounding_box().unwrap();
        let dim = mesh.spatial_dimension();
        let fe = FeEngine::with_precomputation(
            &mesh,
            None,
            ElementKind::Regular,
            ShapeFamily::Lagrange,
            "fe",
        )
        .unwrap();
        let ne = mesh.nb_element(ty, G);
        let nq = fe.nb_integration_points(ty, G).unwrap();
        let nn = ty.nb_nodes_per_element();
        let sigma: Vec<f64> = (0..dim * dim).map(|k| 0.5 + k as f64).collect();
        let d = Array::from_vec(sigma.repeat(ne * nq), dim * dim).unwrap();
        let mut btd = Array::new(1);
        fe.compute_btd(d.view(), &mut btd, ty, G, None).unwrap();
        let mut f = Array::new(1);
        fe.integrate(btd.view(), &mut f, dim * nn, ty, G, None)
            .unwrap();
        let mut nodal = vec![0.0; mesh.nb_nodes() * dim];
        for (e, c) in mesh.connectivity(ty, G).unwrap().tuples().enumerate() {
            for (n, &node) in c.iter().enumerate() {
                for i in 0..dim {
                    nodal[node as usize * dim + i] += f.tuple(e)[n * dim + i];
                }
            }
        }
        let (lo, hi) = (&mesh.bbox().lower, &mesh.bbox().upper);
        for (node, x) in mesh.nodes().tuples().enumerate() {
            let boundary =
                (0..dim).any(|i| (x[i] - lo[i]).abs() < 1e-12 || (x[i] - hi[i]).abs() < 1e-12);
            if !boundary {
                for i in 0..dim {
                    assert!(
                        nodal[node * dim + i].abs() <= 1e-13,
                        "{}: interior node {node}: {}",
                        ty.name(),
                        nodal[node * dim + i]
                    );
                }
            }
        }
        for i in 0..dim {
            let total: f64 = (0..mesh.nb_nodes()).map(|n| nodal[n * dim + i]).sum();
            assert!(total.abs() <= 1e-12, "{}: total force {total}", ty.name());
        }
    }
}

#[test]
fn mass_matrices_are_consistent_with_lumped_masses() {
    for ty in common::test_element_types() {
        let mesh = common::fe_test_mesh(ty);
        let fe = FeEngine::with_precomputation(
            &mesh,
            None,
            ElementKind::Regular,
            ShapeFamily::Lagrange,
            "fe",
        )
        .unwrap();
        let nb_dof = 2;
        let nn = ty.nb_nodes_per_element();
        let lmat = nb_dof * nn;
        let mut rho = |m: &mut [f64], _: Element| -> akantu_core::Result<()> {
            m.fill(2.5);
            Ok(())
        };
        let mm = fe
            .field_matrix_elemental(&mesh, &mut rho, nb_dof, ty, G)
            .unwrap();
        let lumped = fe
            .lumped_field_elemental(&mesh, &mut rho, nb_dof, ty, G)
            .unwrap();
        for e in 0..mesh.nb_element(ty, G) {
            let m = mm.tuple(e);
            let l = lumped.tuple(e);
            let total_m: f64 = m.iter().sum();
            let total_l: f64 = l.iter().sum();
            assert!(
                (total_m - total_l).abs() <= 1e-13 * total_m.abs(),
                "{}: {total_m} vs {total_l}",
                ty.name()
            );
            for i in 0..lmat {
                for j in 0..lmat {
                    let (a, b) = (m[i + j * lmat], m[j + i * lmat]);
                    assert!(
                        (a - b).abs() <= 1e-15 * (1.0 + a.abs()),
                        "{}: asymmetric mass",
                        ty.name()
                    );
                    if i % nb_dof != j % nb_dof {
                        assert_eq!(a, 0.0, "{}: coupling between DOF components", ty.name());
                    }
                }
            }
            if akantu_fe::engine::LumpingScheme::of(ty) == akantu_fe::engine::LumpingScheme::RowSum
            {
                for (i, li) in l.iter().enumerate() {
                    if fe.integration_orders().mass_order(ty) == ty.polynomial_degree() * 2 {
                        let row: f64 = (0..lmat).map(|j| m[i + j * lmat]).sum();
                        // row sums equal ∫ρN_i when both rules integrate N_i exactly
                        if matches!(
                            ty,
                            ElementType::Segment2
                                | ElementType::Triangle3
                                | ElementType::Tetrahedron4
                                | ElementType::Quadrangle4
                                | ElementType::Hexahedron8
                                | ElementType::Pentahedron6
                        ) {
                            assert!(
                                (row - li).abs() <= 1e-13 * li.abs().max(1e-300),
                                "{}: row {i}: {row} vs {li}",
                                ty.name()
                            );
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn elemental_field_interpolation_reproduces_linear_fields() {
    for ty in [
        ElementType::Triangle6,
        ElementType::Quadrangle4,
        ElementType::Quadrangle8,
        ElementType::Tetrahedron10,
        ElementType::Segment3,
    ] {
        let mesh = common::fe_test_mesh(ty);
        let dim = mesh.spatial_dimension();
        let fe = FeEngine::with_precomputation(
            &mesh,
            None,
            ElementKind::Regular,
            ShapeFamily::Lagrange,
            "fe",
        )
        .unwrap();
        let ne = mesh.nb_element(ty, G);
        // interpolation points: the nodes of each element (2 per element: first two nodes)
        let mut points = ElementTypeMapArray::new("points");
        let p = points.alloc(0, dim, ty, G, 0.0).unwrap();
        for c in mesh.connectivity(ty, G).unwrap().tuples() {
            for &node in &c[..2] {
                p.push_tuple(mesh.nodes().tuple(node as usize));
            }
        }
        let mut filter = ElementTypeMapArray::new("filter");
        filter.alloc(0, 1, ty, G, 0).unwrap();
        for e in 0..ne as Idx {
            filter.get_mut(ty, G).unwrap().push_value(e);
        }
        let interp = fe
            .init_elemental_field_interpolation(&mesh, &points, Some(&filter))
            .unwrap();
        let mut xq = Array::new(dim);
        fe.integration_points_coordinates(&mesh, &mut xq, ty, G, None)
            .unwrap();
        let lin = |x: &[f64]| -> [f64; 2] {
            let s: f64 = x
                .iter()
                .enumerate()
                .map(|(d, v)| (d as f64 + 1.0) * v)
                .sum();
            [1.0 + s, -0.5 * s]
        };
        let mut field = ElementTypeMapArray::new("field");
        let f = field.alloc(0, 2, ty, G, 0.0).unwrap();
        for x in xq.tuples() {
            f.push_tuple(&lin(x));
        }
        let mut out = ElementTypeMapArray::new("out");
        fe.interpolate_elemental_field(&mesh, &interp, &field, &mut out, G, Some(&filter))
            .unwrap();
        // the exportable-field entry point (one guard per block instead of a
        // copy of the map): bitwise the same output, the same
        // error for a missing block
        let shared: SharedElementTypeMapArray<Real> = field.clone().into();
        let mut out_s = ElementTypeMapArray::new("out");
        fe.interpolate_elemental_field_shared(
            &mesh,
            &interp,
            &shared,
            &mut out_s,
            G,
            Some(&filter),
        )
        .unwrap();
        let bits = |a: &ElementTypeMapArray<Real>| -> Vec<u64> {
            let a = a.get(ty, G).unwrap();
            assert_eq!(a.nb_component(), 2);
            a.as_slice().iter().map(|x| x.to_bits()).collect()
        };
        assert_eq!(bits(&out), bits(&out_s), "{}", ty.name());
        let missing_plain = fe.interpolate_elemental_field(
            &mesh,
            &interp,
            &ElementTypeMapArray::new("field"),
            &mut ElementTypeMapArray::new("out"),
            G,
            Some(&filter),
        );
        let missing_shared = fe.interpolate_elemental_field_shared(
            &mesh,
            &interp,
            &SharedElementTypeMapArray::new("field"),
            &mut ElementTypeMapArray::new("out"),
            G,
            Some(&filter),
        );
        assert!(missing_plain.is_err());
        assert_eq!(format!("{missing_plain:?}"), format!("{missing_shared:?}"));
        let o = out.get(ty, G).unwrap();
        for (k, x) in points.get(ty, G).unwrap().tuples().enumerate() {
            let ex = lin(x);
            for (i, exi) in ex.iter().enumerate() {
                assert!(
                    (o.tuple(k)[i] - exi).abs() <= 1e-11,
                    "{}: point {k}: {} vs {exi}",
                    ty.name(),
                    o.tuple(k)[i],
                );
            }
        }
    }
    // unsupported upstream (out-of-bounds / uninitialised matrices)
    let mesh = common::fe_test_mesh(ElementType::Hexahedron8);
    let fe = FeEngine::with_precomputation(
        &mesh,
        None,
        ElementKind::Regular,
        ShapeFamily::Lagrange,
        "fe",
    )
    .unwrap();
    let points = ElementTypeMapArray::new("points");
    assert!(matches!(
        fe.init_elemental_field_interpolation(&mesh, &points, None),
        Err(Error::NotImplemented(_))
    ));
}

/// Two cohesive_2d_4 elements whose sides coincide (zero opening).
fn closed_cohesive_mesh() -> Mesh {
    let mut mesh = Mesh::new(2, "coh").unwrap();
    let mut acc = MeshAccessor::new(&mut mesh);
    for p in [
        [0.0, 0.0],
        [1.0, 0.2],
        [0.0, 0.0],
        [1.0, 0.2],
        [1.0, 0.2],
        [2.0, 0.1],
        [1.0, 0.2],
        [2.0, 0.1],
    ] {
        acc.push_node(&p).unwrap();
    }
    let mut c = acc.connectivity_mut(ElementType::Cohesive2d4, G);
    c.push_tuple(&[0, 1, 2, 3]);
    c.push_tuple(&[4, 5, 6, 7]);
    drop(c);
    mesh
}

#[test]
fn cohesive_reductions_and_normals() {
    let mesh = closed_cohesive_mesh();
    let ty = ElementType::Cohesive2d4;
    let fe = FeEngine::with_precomputation(
        &mesh,
        None,
        ElementKind::Cohesive,
        ShapeFamily::Cohesive,
        "coh",
    )
    .unwrap();
    let mut open = Array::new(2);
    fe.interpolate_cohesive(
        &mesh,
        mesh.nodes().view(),
        &mut open,
        ty,
        G,
        CohesiveReduction::Opening,
        None,
    )
    .unwrap();
    assert!(open.as_slice().iter().all(|&v| v == 0.0));
    let mut mean = Array::new(2);
    fe.interpolate_cohesive(
        &mesh,
        mesh.nodes().view(),
        &mut mean,
        ty,
        G,
        CohesiveReduction::Mean,
        None,
    )
    .unwrap();
    let mut dflt = Array::new(2);
    fe.interpolate_on_integration_points(&mesh, mesh.nodes().view(), &mut dflt, 2, ty, G, None)
        .unwrap();
    assert_eq!(mean.as_slice(), dflt.as_slice());
    // |J| w sums to the facet lengths
    let one = Array::from_vec(vec![1.0; mean.len()], 1).unwrap();
    let length = fe.integrate_scalar(one.view(), ty, G, None).unwrap();
    let l0 = (1.0f64 + 0.04).sqrt();
    let l1 = (1.0f64 + 0.01).sqrt();
    assert!((length - (l0 + l1)).abs() <= 1e-15 * 4.0);
    let mut n = Array::new(2);
    fe.compute_cohesive_normals(
        &mesh,
        mesh.nodes().view(),
        &mut n,
        ty,
        G,
        CohesiveReduction::Mean,
        None,
    )
    .unwrap();
    for v in n.tuples() {
        assert!((v[0] * v[0] + v[1] * v[1] - 1.0).abs() <= 1e-15);
    }
    // first element: tangent (1, 0.2) -> normal (-0.2, 1)/|.|
    assert!((n.tuple(0)[0] + 0.2 / l0).abs() <= 1e-15 && (n.tuple(0)[1] - 1.0 / l0).abs() <= 1e-15);
    assert!(matches!(
        fe.gradient_on_integration_points(&mesh, mesh.nodes().view(), &mut n, 2, ty, G, None),
        Err(Error::NotImplemented(_))
    ));
}

#[test]
fn integration_orders_are_configurable() {
    let mesh = common::fe_test_mesh(ElementType::Quadrangle4);
    let mut fe = FeEngine::new(
        &mesh,
        None,
        ElementKind::Regular,
        ShapeFamily::Lagrange,
        "fe",
    )
    .unwrap();
    fe.set_integration_orders(IntegrationOrders {
        per_type: vec![(ElementType::Quadrangle4, 4)],
        mass: Some(6),
    })
    .unwrap();
    fe.init_shape_functions(&mesh, G).unwrap();
    assert_eq!(
        fe.nb_integration_points(ElementType::Quadrangle4, G)
            .unwrap(),
        9
    );
    assert_eq!(
        fe.integration_orders().mass_order(ElementType::Quadrangle4),
        6
    );
    assert!(
        fe.set_integration_orders(IntegrationOrders::default())
            .is_err()
    );
    let f = IntegrationOrders::facets_cohesive();
    assert_eq!(f.order(ElementType::Segment2), 2); // cohesive_2d_4 degree
    assert_eq!(f.order(ElementType::Triangle6), 3); // cohesive_3d_12 degree
    assert_eq!(f.order(ElementType::Hexahedron8), 2); // no cohesive type: default
    assert!(gauss_rule(ElementType::Triangle3, 6).is_err());
}

#[test]
fn mesh_events_update_the_precomputed_data() {
    let ty = ElementType::Triangle3;
    let full = common::fe_test_mesh(ty);
    let ne = full.nb_element(ty, G);
    // start with half of the elements, then add the rest
    let mut mesh = Mesh::new(2, "partial").unwrap();
    {
        let mut acc = MeshAccessor::new(&mut mesh);
        for x in full.nodes().tuples() {
            acc.push_node(x).unwrap();
        }
        let mut c = acc.connectivity_mut(ty, G);
        for t in full.connectivity(ty, G).unwrap().tuples().take(ne / 2) {
            c.push_tuple(t);
        }
    }
    let mut fe = FeEngine::with_precomputation(
        &mesh,
        None,
        ElementKind::Regular,
        ShapeFamily::Lagrange,
        "fe",
    )
    .unwrap();
    {
        let mut acc = MeshAccessor::new(&mut mesh);
        let mut c = acc.connectivity_mut(ty, G);
        for t in full.connectivity(ty, G).unwrap().tuples().skip(ne / 2) {
            c.push_tuple(t);
        }
    }
    let added: Vec<Element> = (ne / 2..ne)
        .map(|e| Element::new(ty, e as Idx, G))
        .collect();
    fe.on_elements_added(&mesh, &added).unwrap();
    let reference = FeEngine::with_precomputation(
        &full,
        None,
        ElementKind::Regular,
        ShapeFamily::Lagrange,
        "fe",
    )
    .unwrap();
    assert_eq!(
        fe.jacobians(ty, G).unwrap().as_slice(),
        reference.jacobians(ty, G).unwrap().as_slice()
    );
    assert_eq!(
        fe.shapes(ty, G).unwrap().as_slice(),
        reference.shapes(ty, G).unwrap().as_slice()
    );
    assert_eq!(
        fe.shapes_derivatives(ty, G).unwrap().as_slice(),
        reference.shapes_derivatives(ty, G).unwrap().as_slice()
    );
    // remove the first element: everything shifts by one element
    let mut numbering = ElementTypeMapArray::new("numbering");
    let n = numbering.alloc(0, 1, ty, G, 0).unwrap();
    for e in 0..ne as Idx {
        n.push_value(e - 1);
    }
    fe.on_elements_removed(&numbering).unwrap();
    let nq = fe.nb_integration_points(ty, G).unwrap();
    assert_eq!(fe.jacobians(ty, G).unwrap().len(), (ne - 1) * nq);
    assert_eq!(
        fe.jacobians(ty, G).unwrap().as_slice(),
        &reference.jacobians(ty, G).unwrap().as_slice()[nq..]
    );
}

#[test]
fn error_paths() {
    // inverted element: negative jacobian (upstream IntegratorGaussException)
    let mut mesh = Mesh::new(2, "inverted").unwrap();
    {
        let mut acc = MeshAccessor::new(&mut mesh);
        for p in [[0.0, 0.0], [0.0, 1.0], [1.0, 0.0]] {
            acc.push_node(&p).unwrap();
        }
        acc.connectivity_mut(ElementType::Triangle3, G)
            .push_tuple(&[0, 1, 2]);
    }
    let r = FeEngine::with_precomputation(
        &mesh,
        None,
        ElementKind::Regular,
        ShapeFamily::Lagrange,
        "fe",
    );
    assert!(matches!(r, Err(Error::Numerical(m)) if m.contains("Negative jacobian")));
    // invalid order_d and filters
    let mesh = common::fe_test_mesh(ElementType::Triangle3);
    let fe = FeEngine::with_precomputation(
        &mesh,
        None,
        ElementKind::Regular,
        ShapeFamily::Lagrange,
        "fe",
    )
    .unwrap();
    let d = Array::from_vec(vec![0.0; 16 * 9], 9).unwrap();
    let mut out = Array::new(1);
    assert!(matches!(
        fe.compute_btdb(d.view(), &mut out, 3, ElementType::Triangle3, G, None),
        Err(Error::InvalidArgument(_))
    ));
    let u = Array::from_vec(vec![0.0; mesh.nb_nodes()], 1).unwrap();
    assert!(
        fe.interpolate_on_integration_points(
            &mesh,
            u.view(),
            &mut out,
            1,
            ElementType::Triangle3,
            G,
            Some(&[-1])
        )
        .is_err()
    );
    assert!(
        fe.interpolate_on_integration_points(
            &mesh,
            u.view(),
            &mut out,
            1,
            ElementType::Triangle3,
            G,
            Some(&[999])
        )
        .is_err()
    );
    assert!(
        fe.interpolate_on_integration_points(
            &mesh,
            u.view(),
            &mut out,
            2,
            ElementType::Triangle3,
            G,
            None
        )
        .is_err()
    );
    // a point far outside: inverse map converges (affine) but contains is false
    let el = Element::new(ElementType::Triangle3, 0, G);
    assert!(!fe.contains(&mesh, &[10.0, 10.0], el).unwrap());
    assert!(
        FeEngine::new(
            &mesh,
            None,
            ElementKind::Structural,
            ShapeFamily::Structural,
            "s"
        )
        .is_err()
    );
    let _ = with_element_class!(ElementType::Triangle3, E => E::NB_NODES, else 0);
}

/// `gradient_on_integration_points_view` against the
/// resizing entry point: bitwise the same values (fast paths `nb_dof` 1…3
/// and the general path), with and without a filter; a view of the wrong
/// shape is an error and stays untouched.
#[test]
fn gradient_view_matches_the_array_gradient_bitwise() {
    for ty in [
        ElementType::Segment3,
        ElementType::Triangle3,
        ElementType::Triangle6,
        ElementType::Quadrangle8,
        ElementType::Tetrahedron4,
        ElementType::Tetrahedron10,
        ElementType::Hexahedron8,
        ElementType::Hexahedron20,
    ] {
        let mesh = common::fe_test_mesh(ty);
        let dim = mesh.spatial_dimension();
        let fe = FeEngine::with_precomputation(
            &mesh,
            None,
            ElementKind::Regular,
            ShapeFamily::Lagrange,
            "fe",
        )
        .unwrap();
        let ne = mesh.nb_element(ty, G);
        let nq = fe.nb_integration_points(ty, G).unwrap();
        let every_other: Vec<Idx> = (0..ne as Idx).rev().step_by(2).collect();
        for nb_dof in 1..=4 {
            let u: Vec<Real> = (0..mesh.nb_nodes() * nb_dof)
                .map(|k| ((k * 7919) % 101) as Real / 13.0 - 3.0)
                .collect();
            let u = akantu_core::array::ArrayView::new(&u, nb_dof).unwrap();
            for filter in [None, Some(every_other.as_slice())] {
                let n = filter.map_or(ne, <[Idx]>::len) * nq;
                let mut a = Array::new(nb_dof * dim);
                fe.gradient_on_integration_points(&mesh, u, &mut a, nb_dof, ty, G, filter)
                    .unwrap();
                let mut v = vec![Real::NAN; n * nb_dof * dim];
                let view = akantu_core::array::ArrayViewMut::new(&mut v, nb_dof * dim).unwrap();
                fe.gradient_on_integration_points_view(&mesh, u, view, nb_dof, ty, G, filter)
                    .unwrap();
                let ab: Vec<u64> = a.as_slice().iter().map(|x| x.to_bits()).collect();
                let vb: Vec<u64> = v.iter().map(|x| x.to_bits()).collect();
                assert_eq!(ab, vb, "{} nb_dof {nb_dof}", ty.name());
            }
            // wrong shapes: errors, nothing written
            for (len, nbc) in [(ne * nq + 1, nb_dof * dim), (ne * nq, nb_dof * dim + 1)] {
                let mut w = vec![-7.0; len * nbc];
                let view = akantu_core::array::ArrayViewMut::new(&mut w, nbc).unwrap();
                let r = fe.gradient_on_integration_points_view(&mesh, u, view, nb_dof, ty, G, None);
                assert!(matches!(r, Err(Error::DimensionMismatch { .. })), "{r:?}");
                assert!(w.iter().all(|&x| x == -7.0));
            }
        }
    }
}

/// The consistent mass reuses a point matrix while the density bits repeat:
/// with densities that alternate between elements (including `+0`/`−0` and
/// a NaN with a payload), every element's matrix equals the one computed
/// with that element's density everywhere.
#[test]
fn mass_point_matrix_reuse_follows_the_density_bits() {
    let values = [
        2.5,
        2.5,
        3.0,
        -0.0,
        0.0,
        2.5,
        f64::from_bits(0xfff8_0000_0000_0abc),
        3.0,
    ];
    for ty in common::test_element_types() {
        let mesh = common::fe_test_mesh(ty);
        let fe = FeEngine::with_precomputation(
            &mesh,
            None,
            ElementKind::Regular,
            ShapeFamily::Lagrange,
            "fe",
        )
        .unwrap();
        let nb_dof = 2;
        let pick = |e: usize, d: usize| values[(e + d) % values.len()];
        let mut mixed = |m: &mut [f64], el: Element| -> akantu_core::Result<()> {
            for (i, x) in m.iter_mut().enumerate() {
                *x = pick(usize::try_from(el.index()).unwrap(), i % nb_dof);
            }
            Ok(())
        };
        let mm = fe
            .field_matrix_elemental(&mesh, &mut mixed, nb_dof, ty, G)
            .unwrap();
        for e in 0..mesh.nb_element(ty, G) {
            let mut uniform = |m: &mut [f64], _: Element| -> akantu_core::Result<()> {
                for (i, x) in m.iter_mut().enumerate() {
                    *x = pick(e, i % nb_dof);
                }
                Ok(())
            };
            let mu = fe
                .field_matrix_elemental(&mesh, &mut uniform, nb_dof, ty, G)
                .unwrap();
            let (a, b) = (mm.tuple(e), mu.tuple(e));
            for (x, y) in a.iter().zip(b) {
                assert_eq!(x.to_bits(), y.to_bits(), "{} element {e}", ty.name());
            }
        }
    }
}
