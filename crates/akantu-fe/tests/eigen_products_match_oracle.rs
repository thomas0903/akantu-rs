// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs (port of Akantu, EPFL/LSMS): bit parity of the FE
// layer's Eigen-order products (akantu_fe::eigen) with the products of the
// upstream FE kernels — src/fe_engine/shape_lagrange_inline_impl.hh
// (computeBtDB `B.transpose() * D * B`, computeNtbN `N.transpose() * b * N`),
// src/fe_engine/shape_functions_inline_impl.hh
// (gradientElementalFieldOnIntegrationPoints `u * B.transpose()`) — and of
// operands with mixed compile-time/run-time extents, against the dense product
// oracle fixture written by oracle/probes/dense/gemm_probe.cc from the real
// upstream code (Akantu aka_types.hh + Eigen 3.4.0, strict build).

//! `fe::eigen_products_match_oracle`: every record of
//! `crates/akantu-linalg/tests/fixtures/dense/dense_gemm_strict.txt` is
//! recomputed through the FE entry points ([`eigen::product_slices`] with
//! [`Dims`] describing the upstream operand expressions, as the FE kernels
//! do) and compared **bit for bit** (NaN == NaN, signed zeros exact). The
//! dense layer replays the same fixture through `akantu_core::eigen_gebp`
//! directly (`akantu-linalg/tests/dense_gemm_parity.rs`); this test pins the
//! FE description of operand extents and storage orders (`Dims` →
//! `ProductShape`) to the oracle, including the run-time `1 × 1`
//! inner-product fallbacks whose order the former FE classification got
//! wrong (it summed them from `+0`; the test checks that this order fails on
//! the fixture's tree-reduced fallback records).

use std::collections::BTreeMap;

use akantu_core::eigen_order::{Reduction, sum_from_zero};
use akantu_core::tensor::MatrixX;
use akantu_core::voigt;
use akantu_fe::eigen::{self, Dims, Route};

const FIXTURE: &str =
    include_str!("../../akantu-linalg/tests/fixtures/dense/dense_gemm_strict.txt");

fn hex(tok: &str) -> f64 {
    f64::from_bits(u64::from_str_radix(tok, 16).unwrap_or_else(|_| panic!("bad hex {tok}")))
}

/// Exact match (decision D-046: bit identity, or default quiet NaNs of
/// either sign; a looser "any two NaNs" match would hide payload
/// differences).
fn same(a: f64, b: f64) -> bool {
    akantu_validation::exact_eq(a, b)
}

/// Outcome of one record.
struct Replay {
    got: Vec<f64>,
    /// Routes of the products of the record.
    routes: Vec<Route>,
    /// For a single-product record evaluated as a tree-reduced `1 × 1`
    /// inner-product fallback: the value of the former FE order (sequential
    /// from `+0`).
    former_dot: Option<f64>,
}

/// `lhs · rhs` through the FE entry point.
fn fe_product(lhs: Dims, rhs: Dims, a: &[f64], b: &[f64], routes: &mut Vec<Route>) -> Vec<f64> {
    let mut out = vec![0.0; lhs.rows * rhs.cols];
    routes.push(eigen::product_slices(lhs, rhs, a, b, &mut out).unwrap());
    out
}

fn replay(head: &[&str], inputs: &[f64]) -> Replay {
    let dims = |i: usize| -> usize { head[i].parse().unwrap() };
    let mut routes = Vec::new();
    let mut former_dot = None;
    let got = match (head[0], head[1]) {
        ("matmul", st @ ("D" | "F")) => {
            let (r, c, k) = (dims(2), dims(3), dims(4));
            let d = |m, n| {
                if st == "F" {
                    Dims::fixed(m, n)
                } else {
                    Dims::dynamic(m, n)
                }
            };
            let (a, b) = inputs.split_at(r * c);
            fe_product(d(r, c), d(c, k), a, b, &mut routes)
        }
        ("trmatmul", st @ ("D" | "F")) => {
            // at.transpose() * b, at stored C × R
            let (c, r, k) = (dims(2), dims(3), dims(4));
            let d = |m, n| {
                if st == "F" {
                    Dims::fixed(m, n)
                } else {
                    Dims::dynamic(m, n)
                }
            };
            let (at, b) = inputs.split_at(c * r);
            fe_product(d(c, r).t(), d(c, k), at, b, &mut routes)
        }
        ("btdb4", "D") => {
            // ShapeLagrange::computeBtDB order 4: B = Voigt B of Bfull,
            // Bt_D_B = (B.transpose() * D) * B (dynamic maps / matrices),
            // as `FeEngine::compute_btdb`
            let (dim, nn) = (dims(2), dims(3));
            let ts = voigt::voigt_table(dim).unwrap().size;
            let n = dim * nn;
            let bfull = MatrixX::from_col_major(dim, nn, inputs[..dim * nn].to_vec()).unwrap();
            let d = &inputs[dim * nn..];
            let mut bv = MatrixX::zeros(ts, n);
            voigt::b_matrix_to_sym_voigt(dim, &bfull, &mut bv, nn).unwrap();
            let btd = fe_product(
                Dims::dynamic(ts, n).t(),
                Dims::dynamic(ts, ts),
                bv.as_slice(),
                d,
                &mut routes,
            );
            fe_product(
                Dims::dynamic(n, ts),
                Dims::dynamic(ts, n),
                &btd,
                bv.as_slice(),
                &mut routes,
            )
        }
        ("btdb2", "D") => {
            let (dim, nn) = (dims(2), dims(3));
            let (b, d) = inputs.split_at(dim * nn);
            let btd = fe_product(
                Dims::dynamic(dim, nn).t(),
                Dims::dynamic(dim, dim),
                b,
                d,
                &mut routes,
            );
            fe_product(
                Dims::dynamic(nn, dim),
                Dims::dynamic(dim, nn),
                &btd,
                b,
                &mut routes,
            )
        }
        ("grad", "M") => {
            // u: Map<Matrix<Dynamic, nn>>, B: Map<const Matrix<dim, nn>>,
            // nabla_u = u * B.transpose(), as the FE gradient kernel
            let (dim, nn, dof) = (dims(2), dims(3), dims(4));
            let (u, b) = inputs.split_at(dof * nn);
            fe_product(
                Dims::new(dof, nn, false, true),
                Dims::fixed(dim, nn).t(),
                u,
                b,
                &mut routes,
            )
        }
        ("ntbn", "D") => {
            // (N.transpose() * b) * N, N: 1 × S, b: 1 × 1, as
            // `FeEngine::compute_ntbn`
            let s = dims(2);
            let (n, b) = inputs.split_at(s);
            let ntb = fe_product(
                Dims::dynamic(1, s).t(),
                Dims::dynamic(1, 1),
                n,
                b,
                &mut routes,
            );
            fe_product(
                Dims::dynamic(s, 1),
                Dims::dynamic(1, s),
                &ntb,
                n,
                &mut routes,
            )
        }
        ("mixed" | "trmixed" | "mixedtr", "M") => {
            // operand extents per `code` (lhs rows, lhs cols, rhs rows, rhs
            // cols: F compile-time, D run-time), see gemm_probe.cc
            let code = head[2].as_bytes();
            let (r, k, c) = (dims(3), dims(4), dims(5));
            let f = |i: usize| code[i] == b'F';
            let (a, b) = inputs.split_at(r * k);
            let lhs = match head[0] {
                // a = at (K × R storage), lhs = at.transpose()
                "trmixed" => Dims::new(k, r, f(1), f(0)).t(),
                _ => Dims::new(r, k, f(0), f(1)),
            };
            let rhs = match head[0] {
                // b = bt (C × K storage), rhs = bt.transpose()
                "mixedtr" => Dims::new(c, k, f(3), f(2)).t(),
                _ => Dims::new(k, c, f(2), f(3)),
            };
            let out = fe_product(lhs, rhs, a, b, &mut routes);
            if routes[0] == Route::Dot(Reduction::Tree) {
                // the former FE order: Σ from +0 over the depth
                let ai = |l: usize| if lhs.transposed { a[l] } else { a[l * r] };
                let bi = |l: usize| if rhs.transposed { b[l * c] } else { b[l] };
                former_dot = Some(sum_from_zero(k, &|l| ai(l) * bi(l)));
            }
            out
        }
        ("aniso", "M") => {
            // MaterialElasticLinearAnisotropic: C (Matrix<Real>) * strain
            // (Vector<Real, voigt>)
            let v = voigt::voigt_table(dims(2)).unwrap().size;
            let (cm, strain) = inputs.split_at(v * v);
            fe_product(
                Dims::dynamic(v, v),
                Dims::fixed(v, 1),
                cm,
                strain,
                &mut routes,
            )
        }
        ("anisorot", "M") => {
            // (rotator (fixed) * Cprime (dynamic)) -> Matrix<Real, D², Dynamic>
            // temporary, then * revrotor (fixed)
            let q = dims(2) * dims(2);
            let (rot, rest) = inputs.split_at(q * q);
            let (cprime, revrot) = rest.split_at(q * q);
            let tmp = fe_product(
                Dims::fixed(q, q),
                Dims::dynamic(q, q),
                rot,
                cprime,
                &mut routes,
            );
            fe_product(
                Dims::new(q, q, true, false),
                Dims::fixed(q, q),
                &tmp,
                revrot,
                &mut routes,
            )
        }
        (op, st) => panic!("unknown record {op} {st}"),
    };
    Replay {
        got,
        routes,
        former_dot,
    }
}

fn route_key(r: &Route) -> String {
    match r {
        Route::Gebp { .. } => "gebp".into(),
        Route::Gemv { matrix, kernel } => format!("gemv {matrix:?} {kernel:?}"),
        other => format!("{other:?}"),
    }
}

#[test]
fn fe_products_match_upstream_bit_for_bit() {
    let mut per_op: BTreeMap<String, usize> = BTreeMap::new();
    let mut routes: BTreeMap<String, usize> = BTreeMap::new();
    let mut mismatches = Vec::new();
    let (mut entries, mut records) = (0usize, 0usize);
    let (mut tree_dots, mut former_dot_failures) = (0usize, 0usize);
    for (idx, line) in FIXTURE.lines().enumerate() {
        if line.starts_with('#') || line.trim().is_empty() {
            continue;
        }
        let mut parts = line.split(" | ");
        let head: Vec<&str> = parts.next().unwrap().split_whitespace().collect();
        let inputs: Vec<f64> = parts.next().unwrap().split_whitespace().map(hex).collect();
        let want: Vec<f64> = parts.next().unwrap().split_whitespace().map(hex).collect();
        *per_op
            .entry(format!("{} {}", head[0], head[1]))
            .or_default() += 1;
        records += 1;
        entries += want.len();
        let r = replay(&head, &inputs);
        for route in &r.routes {
            *routes.entry(route_key(route)).or_default() += 1;
        }
        if let Some(former) = r.former_dot {
            tree_dots += 1;
            if !same(former, want[0]) {
                former_dot_failures += 1;
            }
        }
        if r.got.len() != want.len() {
            mismatches.push(format!(
                "line {}: {}: length {} != {}",
                idx + 1,
                head.join(" "),
                r.got.len(),
                want.len()
            ));
        } else if let Some(i) = (0..want.len()).find(|&i| !same(r.got[i], want[i])) {
            mismatches.push(format!(
                "line {}: {}: entry {i}: got {:e} ({:016x}) want {:e} ({:016x})",
                idx + 1,
                head.join(" "),
                r.got[i],
                r.got[i].to_bits(),
                want[i],
                want[i].to_bits()
            ));
        }
    }
    eprintln!("records per op: {per_op:?}");
    eprintln!("routes: {routes:?}");
    eprintln!(
        "{records} records, {entries} result entries; tree-reduced 1x1 fallbacks: {tree_dots}, \
         of which the former FE order (from +0) mismatches {former_dot_failures}"
    );
    if !mismatches.is_empty() {
        for m in mismatches.iter().take(40) {
            eprintln!("MISMATCH {m}");
        }
        panic!("{} mismatching records", mismatches.len());
    }
    assert_eq!(records, 812, "fixture record count changed");
    assert_eq!(entries, 54_909, "fixture entry count changed");
    for key in [
        "Coeff(Tree)",
        "Coeff(Sequential)",
        "Dot(Tree)",
        "Dot(Sequential)",
        "Outer",
        "gebp",
        "gemv Lhs ColMajor",
        "gemv Lhs RowMajor",
        "gemv Rhs ColMajor",
        "gemv Rhs RowMajor",
    ] {
        assert!(routes.contains_key(key), "missing {key}: {routes:?}");
    }
    // 1 × 20 · 20 × 1 with a compile-time lhs depth and a run-time 1 × 1
    // result (three of the four result-extent combinations, two rhs depth
    // extents, plain and signed-zero repetitions): the signed-zero fallback.
    assert_eq!(tree_dots, 12, "tree-reduced dot fallbacks in the fixture");
    // The fixture discriminates the fallback: the former FE order (from +0) fails
    // on 6 of these 12 records.
    assert_eq!(
        former_dot_failures, 6,
        "fixture discrimination of the signed-zero fallback"
    );
}
