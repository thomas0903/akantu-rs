// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs differential test of the FE engine's evaluation at real
// coordinates, ported from Akantu (EPFL/LSMS)
// src/fe_engine/shape_lagrange_inline_impl.hh:86-170 (inverseMap,
// computeShapes, computeShapeDerivatives, interpolate at real coordinates)
// and src/fe_engine/fe_engine_template.hh (the FEEngine entry points),
// against fixtures/fe/engine_real/<type> (oracle/probes/fe/fe_extra_probe_impl.hh,
// probe_real).

//! FE-024 / FE-027: `computeShapes`,
//! `computeShapeDerivatives` and `interpolate` at real points — the
//! integration-point coordinates of up to 24 elements of each upstream test
//! mesh — compared with the oracle bit for bit
//! (`fixtures/fe/engine_real/tolerances.json`, zero tolerance: the inverse
//! map and the natural-coordinate kernels replicate upstream's arithmetic,
//! as the inverse-map records of `fe/engine`). `interpolate` takes the
//! global nodal array (the port extracts the element's values itself).

mod common;

use akantu_core::{Array, Element, ElementKind, ElementType, GhostType, Idx, Real};
use akantu_fe::FeEngine;
use akantu_fe::shape::ShapeFamily;
use akantu_validation::DiffTest;

use common::bit_stats;

fn real(ty: ElementType) {
    let name = ty.name();
    let mut t = DiffTest::new(
        &format!(
            "fe::engine_real_{}_matches_oracle",
            name.trim_start_matches('_')
        ),
        &format!("fe/engine_real/{name}"),
    )
    .unwrap();
    let mesh = common::fe_test_mesh(ty);
    let dim = mesh.spatial_dimension();
    let nn = ty.nb_nodes_per_interpolation_element();
    let fe = FeEngine::with_precomputation(
        &mesh,
        None,
        ElementKind::Regular,
        ShapeFamily::Lagrange,
        "my_fem",
    )
    .unwrap();
    let elements = t.reference_i64("elements").unwrap().data;
    let points = t.reference_f64("points").unwrap();
    let nq = points.shape[1];
    assert_eq!(points.shape[2], dim);
    let field = t.reference_f64("nodal_field").unwrap();
    let nc = field.shape[1];
    let nodal = Array::from_vec(field.data.clone(), nc).unwrap();
    let mut shapes = Vec::new();
    let mut derivatives = Vec::new();
    let mut interpolated = Vec::new();
    let mut n = vec![0.0; nn];
    let mut b = vec![0.0; dim * nn];
    let mut v = vec![0.0; nc];
    for (i, &e) in elements.iter().enumerate() {
        let el = Element::new(ty, Idx::try_from(e).unwrap(), GhostType::NotGhost);
        for k in 0..nq {
            let x: &[Real] = &points.data[(i * nq + k) * dim..(i * nq + k + 1) * dim];
            fe.compute_shapes_at(&mesh, x, el, &mut n).unwrap();
            shapes.extend_from_slice(&n);
            fe.compute_shape_derivatives_at(&mesh, x, el, &mut b)
                .unwrap();
            // port: column-major sdim × nn; fixture: [d][a]
            for d in 0..dim {
                for a in 0..nn {
                    derivatives.push(b[d + a * dim]);
                }
            }
            fe.interpolate(&mesh, x, nodal.view(), el, &mut v).unwrap();
            interpolated.extend_from_slice(&v);
        }
    }
    let mut summary = Vec::new();
    for (q, got) in [
        ("shapes", &shapes),
        ("shape_derivatives", &derivatives),
        ("interpolate", &interpolated),
    ] {
        let want = t.reference_f64(q).unwrap();
        if want.data.len() == got.len() {
            let (nd, m) = bit_stats(&want.data, got);
            summary.push(format!(
                "{q}: {} values, {nd} not bit-identical, max {m} ulp",
                got.len()
            ));
        }
        t.check_f64_shaped(q, &want.shape, got);
    }
    for l in &summary {
        eprintln!("{name}: {l}");
    }
    t.assert_pass();
}

macro_rules! real_tests {
    ($($test:ident => $ty:ident),* $(,)?) => {$(
        #[test]
        fn $test() {
            real(ElementType::$ty);
        }
    )*};
}

real_tests! {
    engine_real_segment_2_matches_oracle => Segment2,
    engine_real_segment_3_matches_oracle => Segment3,
    engine_real_triangle_3_matches_oracle => Triangle3,
    engine_real_triangle_6_matches_oracle => Triangle6,
    engine_real_quadrangle_4_matches_oracle => Quadrangle4,
    engine_real_quadrangle_8_matches_oracle => Quadrangle8,
    engine_real_tetrahedron_4_matches_oracle => Tetrahedron4,
    engine_real_tetrahedron_10_matches_oracle => Tetrahedron10,
    engine_real_pentahedron_6_matches_oracle => Pentahedron6,
    engine_real_pentahedron_15_matches_oracle => Pentahedron15,
    engine_real_hexahedron_8_matches_oracle => Hexahedron8,
    engine_real_hexahedron_20_matches_oracle => Hexahedron20,
}
