// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs (port of Akantu, EPFL/LSMS): differential tests of the
// element classes against the oracle fixtures fixtures/fe/element_class/*
// (written by oracle/probes/cpp/element_probe.cc from upstream
// ElementClass / GaussIntegrationElement templates).

//! `fe::element_class_<type>_matches_oracle` (FE-003…010, ELEM-001…020,
//! QUAD-001…019).
//!
//! The Rust side recomputes, with the akantu-fe kernels, exactly what the
//! probe computed and with the **same Eigen operand types** as the probe
//! (dynamic `Matrix<Real>` for `dnds`, `X`, `J`, `N`; fixed Gauss tables), so
//! that bitwise agreement is expected wherever Eigen's order is reproduced
//! (`crate::eigen`). The comparison rules come from `fixtures/fe/tolerances.json`;
//! a summary of bit-exactness per quantity is printed (`--nocapture`).

mod common;

use akantu_core::{ElementKind, ElementType, Real};
use akantu_fe::eigen::Dims;
use akantu_fe::element_class::LagrangeElement;
use akantu_fe::{gauss_rule, kernels, with_element_class};
use akantu_validation::DiffTest;

use common::{COHESIVE, REGULAR, bit_stats};

struct Ctx {
    t: DiffTest,
    summary: Vec<String>,
}

impl Ctx {
    fn check(&mut self, q: &str, actual: &[f64]) {
        if let Ok(e) = self.t.reference_f64(q)
            && e.data.len() == actual.len()
        {
            let (n, m) = bit_stats(&e.data, actual);
            self.summary.push(format!(
                "{q}: {} values, {n} not bit-identical, max {m} ulp",
                actual.len()
            ));
        }
        let shape = self
            .t
            .reference_f64(q)
            .map(|f| f.shape)
            .unwrap_or_else(|_| vec![actual.len()]);
        self.t.check_f64_shaped(q, &shape, actual);
    }
}

/// Column-major `sdim × nn` node coordinates from the row-major
/// `[node_local, spatial_dim]` fixture array (same memory layout).
fn nodes(t: &DiffTest, tag: &str) -> Option<(Vec<Real>, usize)> {
    let f = t.reference_f64(&format!("physical/{tag}/nodes")).ok()?;
    Some((f.data.clone(), f.shape[1]))
}

fn check_reference<E: LagrangeElement>(cx: &mut Ctx) {
    let nd = E::NATURAL_DIM;
    let nn = E::NB_SHAPES;
    let orders: Vec<usize> =
        cx.t.fixture()
            .manifest()
            .parameters
            .get("quadrature_orders")
            .and_then(|v| v.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|x| x.as_u64())
                    .map(|x| x as usize)
                    .collect()
            })
            .unwrap_or_default();
    assert!(!orders.is_empty(), "{:?}: no quadrature orders", E::TYPE);
    // computeD2NDS2: implemented upstream for the segment_2, triangle_3 and
    // quadrangle_4 interpolations only (cohesive types reuse them); the
    // probe records which types throw AKANTU_TO_IMPLEMENT
    let d2_oracle =
        cx.t.fixture()
            .manifest()
            .parameters
            .get("d2nds2_implemented")
            .and_then(|v| v.as_bool())
            .unwrap_or_else(|| panic!("{:?}: fixture lacks d2nds2_implemented", E::TYPE));
    let d2_port = E::compute_d2nds2(&vec![0.0; nd], &mut vec![0.0; nd * nd * nn]);
    assert_eq!(
        d2_port.is_ok(),
        d2_oracle,
        "{:?}: computeD2NDS2 support differs from upstream ({d2_port:?})",
        E::TYPE
    );
    if !d2_oracle {
        assert!(
            matches!(d2_port, Err(akantu_core::Error::NotImplemented(_))),
            "{:?}: {d2_port:?}",
            E::TYPE
        );
    }
    for order in orders {
        let rule = gauss_rule(E::TYPE, order).unwrap();
        let nq = rule.nb_points();
        let rows = nd.max(1);
        // quad_points [quad, natural_dim] row-major = column-major points
        cx.check(
            &format!("quad_points/order_{order}"),
            &rule.points.as_slice()[..rows * nq],
        );
        cx.check(&format!("quad_weights/order_{order}"), &rule.weights);
        let mut shapes = vec![0.0; nq * nn];
        let mut dnds = vec![0.0; nq * nd * nn];
        for q in 0..nq {
            let xi = rule.point(q, nd);
            E::compute_shapes(xi, &mut shapes[q * nn..(q + 1) * nn]);
            let mut d = vec![0.0; nd * nn];
            E::compute_dnds(xi, &mut d);
            // fixture: [quad, natural_dim, node_local] row-major
            for i in 0..nd {
                for j in 0..nn {
                    dnds[q * nd * nn + i * nn + j] = d[i + j * nd];
                }
            }
        }
        cx.check(&format!("shapes/order_{order}"), &shapes);
        if nd > 0 {
            cx.check(&format!("dnds/order_{order}"), &dnds);
        }
        if d2_oracle {
            // fixture: [quad, natural_dim_pair, node_local] row-major; port:
            // column-major nd² × nn
            let nr = nd * nd;
            let mut d2 = vec![0.0; nq * nr * nn];
            let mut m = vec![0.0; nr * nn];
            for q in 0..nq {
                E::compute_d2nds2(rule.point(q, nd), &mut m).unwrap();
                for r in 0..nr {
                    for j in 0..nn {
                        d2[q * nr * nn + r * nn + j] = m[r + j * nr];
                    }
                }
            }
            cx.check(&format!("d2nds2/order_{order}"), &d2);
        }
    }
    // facet connectivities (core tables, verbatim column-major upstream data)
    for (ft, fc) in E::TYPE.geometrical_type().info().facets.iter().enumerate() {
        let mut c = Vec::new();
        for f in 0..fc.nb_facets {
            for n in 0..fc.nb_nodes_per_facet {
                c.push(fc.node(f, n) as i64);
            }
        }
        cx.t.check_i64(&format!("facet_connectivity/{ft}"), &c);
    }
    if nd > 0 {
        if let Ok(pts) = cx.t.reference_f64("contains/points") {
            let inside: Vec<i64> = pts
                .data
                .chunks(nd)
                .map(|xi| i64::from(E::contains(xi)))
                .collect();
            cx.t.check_i64("contains/inside", &inside);
        }
        // reference node positions: N_j(ξ_i) = δ_ij
        if let Ok(nc) = cx.t.reference_f64("node_natural_coords") {
            cx.check("node_natural_coords", &nc.data);
            let mut n = vec![0.0; nn];
            for (i, xi) in nc.data.chunks(nd).enumerate() {
                E::compute_shapes(xi, &mut n);
                for (j, v) in n.iter().enumerate() {
                    let e = if i == j { 1.0 } else { 0.0 };
                    assert!((v - e).abs() < 1e-15, "{:?}: N_{j}(ξ_{i}) = {v}", E::TYPE);
                }
            }
        }
    }
}

/// Probe `write_physical<type>` with the probe's operand types.
fn check_physical<E: LagrangeElement>(cx: &mut Ctx, tag: &str) {
    let Some((x, sdim)) = nodes(&cx.t, tag) else {
        return;
    };
    let nd = E::NATURAL_DIM;
    let nn = E::NB_SHAPES;
    let square = sdim == nd;
    let pre = format!("physical/{tag}/");
    cx.check(&format!("{pre}nodes"), &x);
    let xd = Dims::dynamic(sdim, nn);
    let dd = Dims::dynamic(nd, nn);
    let nv = Dims::new(nn, 1, false, true); // Vector<Real>(nn)

    // default order: J, det J, x_q, dN/dx, normals
    let rule = gauss_rule(E::TYPE, E::POLYNOMIAL_DEGREE).unwrap();
    let nq = rule.nb_points();
    let mut jm = Vec::new();
    let mut det = Vec::new();
    let mut xq = Vec::new();
    let mut dndx = Vec::new();
    for q in 0..nq {
        let xi = rule.point(q, nd);
        let mut dnds = vec![0.0; nd * nn];
        E::compute_dnds(xi, &mut dnds);
        let mut j = vec![0.0; nd * sdim];
        kernels::jmat(&dnds, dd, &x, xd, &mut j).unwrap();
        for r in 0..nd {
            for c in 0..sdim {
                jm.push(j[r + c * nd]);
            }
        }
        det.push(kernels::jacobian(&j, nd, sdim).unwrap());
        let mut n = vec![0.0; nn];
        E::compute_shapes(xi, &mut n);
        let mut xx = vec![0.0; sdim];
        kernels::interpolate(&x, xd, &n, nv, &mut xx).unwrap();
        xq.extend_from_slice(&xx);
        if square {
            let mut b = vec![0.0; sdim * nn];
            kernels::shape_derivatives(&j, &dnds, dd, &mut b).unwrap();
            for r in 0..sdim {
                for c in 0..nn {
                    dndx.push(b[r + c * sdim]);
                }
            }
        }
    }
    cx.check(&format!("{pre}jmat"), &jm);
    cx.check(&format!("{pre}jacobians"), &det);
    cx.check(&format!("{pre}quad_coords"), &xq);
    if square {
        cx.check(&format!("{pre}dndx"), &dndx);
    }
    if (1..=2).contains(&nd) && sdim == nd + 1 {
        let mut normals = vec![0.0; sdim * nq];
        let pts = &rule.points.as_slice()[..nd * nq];
        kernels::normals_on_natural::<E>(pts, &x, sdim, false, &mut normals).unwrap();
        cx.check(&format!("{pre}normals"), &normals);
    }

    // integrals of monomials, every order
    let mono = cx.t.reference_i64(&format!("{pre}monomials")).unwrap();
    let orders: Vec<usize> =
        cx.t.fixture()
            .manifest()
            .parameters
            .get("quadrature_orders")
            .and_then(|v| v.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|x| x.as_u64())
                    .map(|x| x as usize)
                    .collect()
            })
            .unwrap_or_default();
    let nmono = mono.shape[0];
    for order in orders {
        let rule = gauss_rule(E::TYPE, order).unwrap();
        let mut res = vec![0.0; nmono];
        for q in 0..rule.nb_points() {
            let xi = rule.point(q, nd);
            let mut dnds = vec![0.0; nd * nn];
            E::compute_dnds(xi, &mut dnds);
            let mut j = vec![0.0; nd * sdim];
            kernels::jmat(&dnds, dd, &x, xd, &mut j).unwrap();
            let dj = kernels::jacobian(&j, nd, sdim).unwrap();
            let mut n = vec![0.0; nn];
            E::compute_shapes(xi, &mut n);
            let mut xx = vec![0.0; sdim];
            kernels::interpolate(&x, xd, &n, nv, &mut xx).unwrap();
            for (k, r) in res.iter_mut().enumerate() {
                let mut v = 1.;
                for (d, &xd) in xx.iter().enumerate().take(3) {
                    for _ in 0..mono.data[k * sdim + d] {
                        v *= xd;
                    }
                }
                *r += rule.weights[q] * dj * v;
            }
        }
        cx.check(&format!("{pre}integrals/order_{order}"), &res);
    }

    if square {
        let r = E::inradius(&x);
        cx.check(&format!("{pre}inradius"), &[r]);
        // inverse map of the order-2 Gauss points
        let rule = gauss_rule(E::TYPE, 2).unwrap();
        let mut xin = Vec::new();
        let mut xiout = Vec::new();
        let mut xiref = Vec::new();
        for q in 0..rule.nb_points() {
            let xi = rule.point(q, nd);
            let mut n = vec![0.0; nn];
            E::compute_shapes(xi, &mut n);
            let mut xx = vec![0.0; sdim];
            kernels::interpolate(&x, xd, &n, nv, &mut xx).unwrap();
            let mut back = vec![0.0; nd];
            E::inverse_map(
                &xx,
                &x,
                &mut back,
                akantu_fe::element_class::INVERSE_MAP_MAX_ITERATIONS,
                akantu_fe::element_class::INVERSE_MAP_TOLERANCE,
            )
            .unwrap();
            xin.extend_from_slice(&xx);
            xiout.extend_from_slice(&back);
            xiref.extend_from_slice(xi);
        }
        cx.check(&format!("{pre}inverse_map/x"), &xin);
        cx.check(&format!("{pre}inverse_map/xi"), &xiout);
        cx.check(&format!("{pre}inverse_map/xi_exact"), &xiref);
    }
}

fn run(ty: ElementType) -> Vec<String> {
    let name = ty.name();
    let t = DiffTest::new(
        &format!(
            "fe::element_class_{}_matches_oracle",
            name.trim_start_matches('_')
        ),
        &format!("fe/element_class/{name}"),
    )
    .unwrap();
    let mut cx = Ctx {
        t,
        summary: Vec::new(),
    };
    with_element_class!(ty, E => {
        check_reference::<E>(&mut cx);
        if ty.kind() == ElementKind::Regular {
            for tag in ["affine", "curved", "embedded"] {
                check_physical::<E>(&mut cx, tag);
            }
        }
    }, else panic!("not a Lagrange type"));
    let s = cx.summary;
    cx.t.assert_pass();
    s
}

macro_rules! element_tests {
    ($($fn:ident => $ty:ident),* $(,)?) => {$(
        #[test]
        fn $fn() {
            for line in run(ElementType::$ty) {
                eprintln!("{}: {line}", ElementType::$ty.name());
            }
        }
    )*};
}

element_tests!(
    element_class_point_1_matches_oracle => Point1,
    element_class_segment_2_matches_oracle => Segment2,
    element_class_segment_3_matches_oracle => Segment3,
    element_class_triangle_3_matches_oracle => Triangle3,
    element_class_triangle_6_matches_oracle => Triangle6,
    element_class_quadrangle_4_matches_oracle => Quadrangle4,
    element_class_quadrangle_8_matches_oracle => Quadrangle8,
    element_class_tetrahedron_4_matches_oracle => Tetrahedron4,
    element_class_tetrahedron_10_matches_oracle => Tetrahedron10,
    element_class_pentahedron_6_matches_oracle => Pentahedron6,
    element_class_pentahedron_15_matches_oracle => Pentahedron15,
    element_class_hexahedron_8_matches_oracle => Hexahedron8,
    element_class_hexahedron_20_matches_oracle => Hexahedron20,
    element_class_cohesive_1d_2_matches_oracle => Cohesive1d2,
    element_class_cohesive_2d_4_matches_oracle => Cohesive2d4,
    element_class_cohesive_2d_6_matches_oracle => Cohesive2d6,
    element_class_cohesive_3d_6_matches_oracle => Cohesive3d6,
    element_class_cohesive_3d_12_matches_oracle => Cohesive3d12,
    element_class_cohesive_3d_8_matches_oracle => Cohesive3d8,
    element_class_cohesive_3d_16_matches_oracle => Cohesive3d16,
);

#[test]
fn every_lagrange_type_is_covered() {
    assert_eq!(REGULAR.len() + COHESIVE.len(), 20);
}
