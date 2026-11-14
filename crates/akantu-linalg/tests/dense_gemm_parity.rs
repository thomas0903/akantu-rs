// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): bit parity of the Eigen products used by
// src/fe_engine/shape_lagrange_inline_impl.hh (computeBtDB `B.transpose() *
// D * B` orders 4 and 2, computeNtbN `N.transpose() * b * N`),
// src/fe_engine/shape_functions_inline_impl.hh
// (gradientElementalFieldOnIntegrationPoints `u * B.transpose()`),
// src/fe_engine/element_class_structural.hh (`N * T`, `B * R`),
// src/model/solid_mechanics/materials/material_elastic_linear_anisotropic*
// (`C * voigt_strain`, `rotator * Cprime * revrotor`), plain aka_types.hh
// products at Eigen's gebp boundaries and products of operands with mixed
// compile-time/run-time extents, against fixtures produced by the real
// upstream code (oracle/probes/dense/gemm_probe.cc).

//! Evidence (D-036): every record of
//! `fixtures/dense/dense_gemm_strict.txt` is recomputed with
//! `akantu_core::eigen_gebp` (through `MatrixX`, fixed `Matrix` or the
//! mixed-size [`product_into`]) and compared **bit for bit** (NaN == NaN;
//! signed zeros exact). The test also asserts that the fixture covers the
//! routes it is meant to cover (gebp with even/odd chains, panel
//! remainders, depth blocking, row/column blocking, GEMV column blocking,
//! the signed-zero behaviour of coefficient-based vs. `+0`-started kernels,
//! and coefficient reductions whose order is decided by the **left**
//! operand's column extent while the right operand fixes the depth).

use std::collections::BTreeMap;

use akantu_core::eigen_gebp::{
    self, Extent, GemvMatrix, Order, ProductShape, Reduction, Route, gemm_kc, product_into,
};
use akantu_core::tensor::{Matrix, MatrixExt, MatrixX};
use akantu_core::voigt;

const FIXTURE: &str = include_str!("fixtures/dense/dense_gemm_strict.txt");

fn hex(tok: &str) -> f64 {
    f64::from_bits(u64::from_str_radix(tok, 16).unwrap_or_else(|_| panic!("bad hex {tok}")))
}

/// Exact match (decision D-046: bit identity, or default quiet NaNs of
/// either sign; a looser "any two NaNs" match would hide payload
/// differences).
fn same(a: f64, b: f64) -> bool {
    akantu_validation::exact_eq(a, b)
}

fn mx(r: usize, c: usize, d: &[f64]) -> MatrixX {
    MatrixX::from_col_major(r, c, d.to_vec()).unwrap()
}

/// Route tally: what the fixture exercises.
#[derive(Default, Debug)]
struct Coverage {
    routes: BTreeMap<String, usize>,
    gebp_even_odd: usize,
    gebp_panel_remainder: usize,
    gebp_depth_blocked: usize,
    gebp_row_or_col_blocking: usize,
    gemv_col_blocked: usize,
    /// coefficient reductions that are sequential although the depth is
    /// compile-time (fixed by the rhs only) and within the unrolling limit:
    /// the products whose order the left-operand rule decides
    lhs_rule: usize,
    /// products of operands with mixed compile-time/run-time extents
    mixed_extents: usize,
    negative_zero_results: usize,
    positive_zero_results: usize,
}

impl Coverage {
    fn add(&mut self, shape: &ProductShape) {
        let depth = shape.depth();
        let (m, k, n) = (shape.rows.size(), depth.size(), shape.cols.size());
        let r = eigen_gebp::route(shape);
        let exts = [shape.rows, shape.lhs_depth, shape.rhs_depth, shape.cols];
        if exts.iter().any(|e| e.is_fixed()) && exts.iter().any(|e| !e.is_fixed()) {
            self.mixed_extents += 1;
        }
        if let Route::Coeff(Reduction::Sequential) | Route::Dot(Reduction::Sequential) = r
            && depth.is_fixed()
            && Reduction::of_product(k, true) == Reduction::Tree
            && k >= 3
        {
            self.lhs_rule += 1;
        }
        let key = match r {
            Route::Gebp { kc } => {
                if k >= 8 && n >= 4 {
                    self.gebp_even_odd += 1;
                }
                if n >= 4 && n % 4 != 0 {
                    self.gebp_panel_remainder += 1;
                }
                if kc < k {
                    self.gebp_depth_blocked += 1;
                }
                if kc == k && m.max(n).max(k) >= 48 {
                    self.gebp_row_or_col_blocking += 1;
                }
                "gebp".to_string()
            }
            Route::Gemv { matrix, kernel } => {
                if kernel == Order::ColMajor && k >= 128 {
                    self.gemv_col_blocked += 1;
                }
                format!("gemv {matrix:?} {kernel:?}")
            }
            Route::Coeff(order) => format!("Coeff {order:?}"),
            Route::Dot(order) => format!("Dot {order:?}"),
            other => format!("{other:?}"),
        };
        *self.routes.entry(key).or_default() += 1;
    }

    fn zeros(&mut self, v: &[f64]) {
        for x in v {
            if *x == 0.0 {
                if x.is_sign_negative() {
                    self.negative_zero_results += 1;
                } else {
                    self.positive_zero_results += 1;
                }
            }
        }
    }
}

fn fixed_matmul<const R: usize, const C: usize, const K: usize>(d: &[f64]) -> Vec<f64> {
    let a: Matrix<R, C> = std::array::from_fn(|j| std::array::from_fn(|i| d[i + j * R]));
    let b: Matrix<C, K> = std::array::from_fn(|j| std::array::from_fn(|i| d[R * C + i + j * C]));
    a.matmul(&b).as_flat().to_vec()
}

fn fixed_trmatmul<const C: usize, const R: usize, const K: usize>(d: &[f64]) -> Vec<f64> {
    let at: Matrix<C, R> = std::array::from_fn(|j| std::array::from_fn(|i| d[i + j * C]));
    let b: Matrix<C, K> = std::array::from_fn(|j| std::array::from_fn(|i| d[C * R + i + j * C]));
    at.tr_matmul(&b).as_flat().to_vec()
}

/// Dispatch a fixed `R × C · C × K` record to the const-generic product
/// (`tr`: `trmatmul F C R K`, else `matmul F R C K`).
macro_rules! fixed_dispatch {
    ($tr:expr, $dims:expr, $d:expr; $(($r:literal, $c:literal, $k:literal)),* $(,)?) => {
        match ($tr, $dims) {
            $(
                (false, ($r, $c, $k)) => fixed_matmul::<$r, $c, $k>($d),
                (true, ($r, $c, $k)) => fixed_trmatmul::<$c, $r, $k>($d),
            )*
            (tr, s) => panic!("no fixed dispatch for tr={tr} {s:?}"),
        }
    };
}

fn fixed_product(tr: bool, r: usize, c: usize, k: usize, d: &[f64]) -> Vec<f64> {
    fixed_dispatch!(tr, (r, c, k), d;
        (5, 8, 7), (9, 8, 5), (3, 8, 13),
        (5, 9, 7), (9, 9, 5), (3, 9, 13),
        (5, 10, 7), (9, 10, 5), (3, 10, 13),
        (5, 11, 7), (9, 11, 5), (3, 11, 13),
        (5, 12, 7), (9, 12, 5), (3, 12, 13),
        (5, 13, 7), (9, 13, 5), (3, 13, 13),
        (5, 14, 7), (9, 14, 5), (3, 14, 13),
        (5, 15, 7), (9, 15, 5), (3, 15, 13),
        (5, 16, 7), (9, 16, 5), (3, 16, 13),
        (6, 12, 12), (4, 12, 12), (6, 18, 18),
        (12, 3, 12), (24, 6, 24), (9, 9, 9),
        (3, 409, 5), (3, 817, 5),
    )
}

/// One fixture record recomputed: returns (got, want).
fn recompute(head: &[&str], inputs: &[f64], cov: &mut Coverage) -> Vec<f64> {
    let dims = |i: usize| -> usize { head[i].parse().unwrap() };
    match (head[0], head[1]) {
        ("matmul", "D") => {
            let (r, c, k) = (dims(2), dims(3), dims(4));
            cov.add(&ProductShape::dynamic(r, c, k));
            let a = mx(r, c, &inputs[..r * c]);
            let b = mx(c, k, &inputs[r * c..]);
            a.matmul(&b).unwrap().as_slice().to_vec()
        }
        ("trmatmul", "D") => {
            let (c, r, k) = (dims(2), dims(3), dims(4));
            cov.add(&ProductShape::dynamic(r, c, k).lhs_transposed());
            let at = mx(c, r, &inputs[..c * r]);
            let b = mx(c, k, &inputs[c * r..]);
            at.tr_matmul(&b).unwrap().as_slice().to_vec()
        }
        ("matmul", "F") => {
            let (r, c, k) = (dims(2), dims(3), dims(4));
            cov.add(&ProductShape::fixed(r, c, k));
            fixed_product(false, r, c, k, inputs)
        }
        ("trmatmul", "F") => {
            let (c, r, k) = (dims(2), dims(3), dims(4));
            cov.add(&ProductShape::fixed(r, c, k).lhs_transposed());
            fixed_product(true, r, c, k, inputs)
        }
        ("btdb4", "D") => {
            // B = VoigtHelper<dim>::transferBMatrixToSymVoigtBMatrix(Bfull);
            // Bt_D_B = (B.transpose() * D) * B
            let (dim, nn) = (dims(2), dims(3));
            let ts = match dim {
                1 => 1,
                2 => 3,
                3 => 6,
                _ => panic!("dim {dim}"),
            };
            let bfull = mx(dim, nn, &inputs[..dim * nn]);
            let d = mx(ts, ts, &inputs[dim * nn..]);
            let mut b = MatrixX::zeros(ts, dim * nn);
            voigt::b_matrix_to_sym_voigt(dim, &bfull, &mut b, nn).unwrap();
            cov.add(&ProductShape::dynamic(dim * nn, ts, ts).lhs_transposed());
            cov.add(&ProductShape::dynamic(dim * nn, ts, dim * nn));
            let btd = b.tr_matmul(&d).unwrap();
            btd.matmul(&b).unwrap().as_slice().to_vec()
        }
        ("btdb2", "D") => {
            let (dim, nn) = (dims(2), dims(3));
            let b = mx(dim, nn, &inputs[..dim * nn]);
            let d = mx(dim, dim, &inputs[dim * nn..]);
            cov.add(&ProductShape::dynamic(nn, dim, dim).lhs_transposed());
            cov.add(&ProductShape::dynamic(nn, dim, nn));
            let btd = b.tr_matmul(&d).unwrap();
            btd.matmul(&b).unwrap().as_slice().to_vec()
        }
        ("grad", "M") => {
            // u: Map<Matrix<Dynamic, nnodes>> (dof × nnodes),
            // B: Map<const Matrix<dim, nnodes>>, nabla_u = u * B.transpose()
            let (dim, nn, dof) = (dims(2), dims(3), dims(4));
            let u = &inputs[..dof * nn];
            let bm = &inputs[dof * nn..];
            let shape = ProductShape::new(
                Extent::Dynamic(dof),
                Extent::Fixed(nn),
                Extent::Fixed(nn),
                Extent::Fixed(dim),
            )
            .rhs_transposed();
            cov.add(&shape);
            let mut out = vec![0.0; dof * dim];
            product_into(
                &shape,
                &|i, l| u[i + l * dof],
                &|l, j| bm[j + l * dim],
                &mut out,
            )
            .unwrap();
            out
        }
        ("ntbn", "D") => {
            // (N.transpose() * b) * N, N: 1 × S, b: 1 × 1 (dynamic maps)
            let s = dims(2);
            let n = mx(1, s, &inputs[..s]);
            let b = mx(1, 1, &inputs[s..s + 1]);
            cov.add(&ProductShape::dynamic(s, 1, 1).lhs_transposed());
            cov.add(&ProductShape::dynamic(s, 1, s));
            let ntb = n.tr_matmul(&b).unwrap();
            ntb.matmul(&n).unwrap().as_slice().to_vec()
        }
        ("mixed" | "trmixed" | "mixedtr", "M") => {
            // operands with the extents of `code` (lhs rows, lhs cols, rhs
            // rows, rhs cols: F compile-time, D run-time), see gemm_probe.cc
            let code = head[2].as_bytes();
            let (r, k, c) = (dims(3), dims(4), dims(5));
            let ext = |b: u8, n: usize| {
                if b == b'F' {
                    Extent::Fixed(n)
                } else {
                    Extent::Dynamic(n)
                }
            };
            let shape = ProductShape::new(
                ext(code[0], r),
                ext(code[1], k),
                ext(code[2], k),
                ext(code[3], c),
            );
            let (a, b) = inputs.split_at(r * k);
            let mut out = vec![0.0; r * c];
            let shape = match head[0] {
                "mixed" => {
                    product_into(&shape, &|i, l| a[i + l * r], &|l, j| b[l + j * k], &mut out)
                        .unwrap();
                    shape
                }
                "trmixed" => {
                    // a = at (K × R storage), lhs = at.transpose()
                    let shape = shape.lhs_transposed();
                    product_into(&shape, &|i, l| a[l + i * k], &|l, j| b[l + j * k], &mut out)
                        .unwrap();
                    shape
                }
                _ => {
                    // b = bt (C × K storage), rhs = bt.transpose()
                    let shape = shape.rhs_transposed();
                    product_into(&shape, &|i, l| a[i + l * r], &|l, j| b[j + l * c], &mut out)
                        .unwrap();
                    shape
                }
            };
            cov.add(&shape);
            out
        }
        ("aniso", "M") => {
            // MaterialElasticLinearAnisotropic::computeStressOnQuad:
            // C (Matrix<Real>, dynamic v × v) * voigt_strain (Vector<Real, v>)
            let v = match dims(2) {
                1 => 1,
                2 => 3,
                3 => 6,
                d => panic!("dim {d}"),
            };
            let (cm, strain) = inputs.split_at(v * v);
            let shape = ProductShape::new(
                Extent::Dynamic(v),
                Extent::Dynamic(v),
                Extent::Fixed(v),
                Extent::Fixed(1),
            );
            cov.add(&shape);
            let mut stress = vec![0.0; v];
            product_into(
                &shape,
                &|i, l| cm[i + l * v],
                &|l, _| strain[l],
                &mut stress,
            )
            .unwrap();
            stress
        }
        ("anisorot", "M") => {
            // updateInternalParameters: Cfull = (rotator * Cprime) * revrotor,
            // rotator / revrotor Matrix<Real, D², D²>, Cprime Matrix<Real>;
            // the temporary (rotator * Cprime) is a Matrix<Real, D², Dynamic>.
            let q = dims(2) * dims(2);
            let (rot, rest) = inputs.split_at(q * q);
            let (cprime, revrot) = rest.split_at(q * q);
            let (f, d) = (Extent::Fixed(q), Extent::Dynamic(q));
            let first = ProductShape::new(f, f, d, d);
            let second = ProductShape::new(f, d, f, f);
            cov.add(&first);
            cov.add(&second);
            let mut tmp = vec![0.0; q * q];
            product_into(
                &first,
                &|i, l| rot[i + l * q],
                &|l, j| cprime[l + j * q],
                &mut tmp,
            )
            .unwrap();
            let mut cfull = vec![0.0; q * q];
            product_into(
                &second,
                &|i, l| tmp[i + l * q],
                &|l, j| revrot[l + j * q],
                &mut cfull,
            )
            .unwrap();
            cfull
        }
        (op, st) => panic!("unknown record {op} {st}"),
    }
}

#[test]
fn gemm_routes_match_upstream_bit_for_bit() {
    let mut cov = Coverage::default();
    let mut per_op: BTreeMap<String, usize> = BTreeMap::new();
    let mut mismatches = Vec::new();
    let mut entries = 0usize;
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
        let got = recompute(&head, &inputs, &mut cov);
        cov.zeros(&want);
        entries += want.len();
        if got.len() != want.len() {
            mismatches.push(format!(
                "line {}: {}: length {} != {}",
                idx + 1,
                head.join(" "),
                got.len(),
                want.len()
            ));
        } else if let Some(i) = (0..got.len()).find(|&i| !same(got[i], want[i])) {
            mismatches.push(format!(
                "line {}: {}: entry {i}: got {:e} ({:016x}) want {:e} ({:016x})",
                idx + 1,
                head.join(" "),
                got[i],
                got[i].to_bits(),
                want[i],
                want[i].to_bits()
            ));
        }
    }
    eprintln!("records per op: {per_op:?}");
    eprintln!("{} result entries; coverage: {cov:#?}", entries);
    if !mismatches.is_empty() {
        for m in mismatches.iter().take(40) {
            eprintln!("MISMATCH {m}");
        }
        panic!("{} mismatching records", mismatches.len());
    }
    let records: usize = per_op.values().sum();
    assert_eq!(records, 812, "fixture record count changed");
    // Coverage the fixture was designed for (acceptance list).
    assert_eq!(per_op["btdb4 D"], 36); // 12 Lagrange shapes × 3 (one sparse)
    assert_eq!(per_op["btdb2 D"], 36);
    // The anisotropic material's products and the 16 F/D
    // combinations of the four operand extents per shape (17 shapes + 4
    // signed-zero repetitions + 5 deep records; 3 + 2 transposed shapes).
    assert_eq!(per_op["aniso M"], 12);
    assert_eq!(per_op["anisorot M"], 6);
    assert_eq!(per_op["mixed M"], 21 * 16 + 5);
    assert_eq!(per_op["trmixed M"], 3 * 16);
    assert_eq!(per_op["mixedtr M"], 2 * 16);
    assert!(cov.gebp_even_odd >= 50, "{cov:?}");
    assert!(cov.gebp_panel_remainder >= 50, "{cov:?}");
    // dynamic depths 409/816/817 (two shapes each) and 1000; depth 409
    // fixed with a run-time result extent (DD·FF, FF·DD: not finite)
    assert_eq!(cov.gebp_depth_blocked, 9, "{cov:?}");
    assert!(cov.gebp_row_or_col_blocking >= 5, "{cov:?}");
    // column result, dynamic depths 128/129/200/409 and fixed depth 129
    // with run-time rows (DD·FF GEMV, DF·DD GemmProduct → GEMV) (the
    // 4-column block variant for matrices with >= 4000 rows is unit-tested
    // only: eigen_gebp tests)
    assert_eq!(cov.gemv_col_blocked, 6, "{cov:?}");
    for key in [
        "Coeff Tree",
        "Coeff Sequential",
        "Dot Tree",
        "Dot Sequential",
        "Outer",
        "gemv Lhs ColMajor",
        "gemv Lhs RowMajor",
        "gemv Rhs ColMajor",
        "gemv Rhs RowMajor",
    ] {
        assert!(cov.routes.contains_key(key), "missing {key}: {cov:?}");
    }
    // The left-operand rule: coefficient reductions over a compile-time
    // depth fixed only by the right operand are sequential (54 of these
    // records fail with the prefer-fixed depth, docs/NUMERICAL_VALIDATION.md).
    assert!(cov.lhs_rule >= 50, "{cov:?}");
    assert!(cov.mixed_extents >= 400, "{cov:?}");
    // Signed zeros: coefficient-based products keep -0, gebp/GEMV give +0.
    assert!(cov.negative_zero_results > 0 && cov.positive_zero_results > 0);
}

#[test]
fn depth_blocking_of_the_fixture_shapes() {
    // The dynamic deep records split as documented; fixed ones do not.
    let blocks = |depth: usize, finite: bool| {
        let kc = gemm_kc(3, 5, depth, finite);
        let mut v = Vec::new();
        let mut k0 = 0;
        while k0 < depth {
            v.push(kc.min(depth - k0));
            k0 += kc;
        }
        v
    };
    assert_eq!(blocks(407, false), [407]);
    assert_eq!(blocks(408, false), [408]);
    assert_eq!(blocks(409, false), [208, 201]);
    assert_eq!(blocks(816, false), [408, 408]);
    assert_eq!(blocks(817, false), [280, 280, 257]);
    assert_eq!(blocks(1000, false), [336, 336, 328]);
    assert_eq!(blocks(817, true), [817]);
    // triangle_6 BtDB order 4: first product coefficient-based,
    // second gebp.
    let first = eigen_gebp::route(&ProductShape::dynamic(12, 3, 3).lhs_transposed());
    let second = eigen_gebp::route(&ProductShape::dynamic(12, 3, 12));
    assert_eq!(first, Route::Coeff(Reduction::Sequential));
    assert_eq!(second, Route::Gebp { kc: 3 });
    // NᵀbN of a 20-node element: first product a row-major GEMV (+0 start).
    assert_eq!(
        eigen_gebp::route(&ProductShape::dynamic(20, 1, 1).lhs_transposed()),
        Route::Gemv {
            matrix: GemvMatrix::Lhs,
            kernel: Order::RowMajor
        }
    );
}
