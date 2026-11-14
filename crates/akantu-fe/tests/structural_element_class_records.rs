// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/fe_engine/element_classes/
// element_class_bernoulli_beam_inline_impl.hh and
// element_class_kirchhoff_shell_inline_impl.hh (computeShapes,
// computeDNDS), against the records of oracle/probes/cpp/element_probe.cc
// (`structural/*` of fixtures/fe/element_class/<structural type>).

//! The `structural/dnds` and `structural/shapes`
//! records of the element-class fixtures of `_bernoulli_beam_2`,
//! `_bernoulli_beam_3` and `_discrete_kirchhoff_triangle_18` had no
//! consumer (their rule was an unmeasured relative 2e-14). They are
//! replayed here through `akantu_fe::structural::StructuralElement`, as the
//! probe computes them: `computeDNDS(xi, X, dnds)` with the recorded node
//! coordinates `X` (`structural/nodes`, an input echo) and, for the beams,
//! `computeShapes(xi, x_local = [0, 1.7], N)` with upstream's θy row of
//! the 3-D beam (UD-293, `StructuralUpstreamCompat::UPSTREAM`), at the
//! oracle's own points of the default order (`GaussIntegrationElement<type,
//! polynomial_degree>`). The rule is exact: the same kernels replay
//! `fixtures/fe/structural` bitwise.

mod common;

use akantu_core::ElementType;
use akantu_fe::structural::{Coords, StructuralElement, StructuralUpstreamCompat};
use akantu_fe::with_structural_element;
use akantu_validation::DiffTest;

use common::bit_stats;

/// The probe's row-major order of a `rows × cols` column-major matrix.
fn row_major(m: &[f64], rows: usize, cols: usize, out: &mut Vec<f64>) {
    for r in 0..rows {
        for c in 0..cols {
            out.push(m[r + rows * c]);
        }
    }
}

fn run(ty: ElementType) {
    let name = ty.name();
    let mut t = DiffTest::new(
        &format!(
            "fe::structural_records_{}_match_oracle",
            name.trim_start_matches('_')
        ),
        &format!("fe/element_class/{name}"),
    )
    .unwrap();
    let x = t.reference_f64("structural/nodes").unwrap();
    let (nn, sdim) = (x.shape[0], x.shape[1]);
    let dnds_ref = t.reference_f64("structural/dnds").unwrap();
    let (nq, nd, ncol) = (dnds_ref.shape[0], dnds_ref.shape[1], dnds_ref.shape[2]);
    let order = t
        .fixture()
        .manifest()
        .parameters
        .get("default_quadrature_order")
        .and_then(serde_json::Value::as_u64)
        .expect("default_quadrature_order");
    let pts = t
        .reference_f64(&format!("quad_points/order_{order}"))
        .unwrap();
    assert_eq!(pts.shape, vec![nq, nd], "{name}: the probe's points");
    let mut summary = Vec::new();
    with_structural_element!(ty, E => {
        assert_eq!(
            (E::NB_ELEMENT_DOFS / E::NB_DOF_PER_NODE, E::DNDS_COLUMNS),
            (nn, ncol),
            "{name}"
        );
        // the recorded `X` ([node, dim] row-major) is the probe's
        // column-major sdim × nn matrix
        let coords = Coords::new(&x.data, sdim, nn).unwrap();
        let local = [0.0, 1.7];
        let local = Coords::new(&local, 1, 2).unwrap();
        let (rows, cols) = (E::NB_DOF_PER_NODE, E::NB_ELEMENT_DOFS);
        let mut dnds = Vec::with_capacity(nq * nd * ncol);
        let mut shapes = Vec::new();
        for q in 0..nq {
            let xi = &pts.data[q * nd..(q + 1) * nd];
            let mut d = vec![0.0; nd * ncol];
            E::compute_dnds(xi, coords, &mut d).unwrap();
            row_major(&d, nd, ncol, &mut dnds);
            if E::HAS_SHAPES {
                let mut n = vec![0.0; rows * cols];
                E::compute_shapes_with(xi, local, StructuralUpstreamCompat::UPSTREAM, &mut n)
                    .unwrap();
                row_major(&n, rows, cols, &mut shapes);
            }
        }
        let (k, m) = bit_stats(&dnds_ref.data, &dnds);
        summary.push(format!("dnds: {} values, {k} not bit-identical, max {m} ulp", dnds.len()));
        t.check_f64_shaped("structural/dnds", &dnds_ref.shape, &dnds);
        if E::HAS_SHAPES {
            let s = t.reference_f64("structural/shapes").unwrap();
            let (k, m) = bit_stats(&s.data, &shapes);
            summary.push(format!(
                "shapes: {} values, {k} not bit-identical, max {m} ulp",
                shapes.len()
            ));
            t.check_f64_shaped("structural/shapes", &s.shape, &shapes);
        } else {
            assert!(t.reference_f64("structural/shapes").is_err(), "{name}: no shapes upstream");
        }
    }, else panic!("{name}: not a structural type"));
    for line in summary {
        eprintln!("{name}: {line}");
    }
    t.assert_pass();
}

#[test]
fn structural_records_bernoulli_beam_2_match_oracle() {
    run(ElementType::BernoulliBeam2);
}

#[test]
fn structural_records_bernoulli_beam_3_match_oracle() {
    run(ElementType::BernoulliBeam3);
}

#[test]
fn structural_records_discrete_kirchhoff_triangle_18_match_oracle() {
    run(ElementType::DiscreteKirchhoffTriangle18);
}
