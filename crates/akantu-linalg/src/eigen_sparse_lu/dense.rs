// SPDX-License-Identifier: MPL-2.0
//
// Rust translation of parts of Eigen 3.4.0 (https://eigen.tuxfamily.org),
// for akantu-rs (a port of Akantu, EPFL/LSMS). Source used: Debian
// libeigen3-dev 3.4.0-5, /usr/include/eigen3 in the oracle image
// akantu-oracle:dev. Modified: translated from C++ to Rust (akantu-rs,
// 2026), specialised to `double` without vectorisation
// (`EIGEN_DONT_VECTORIZE`, packet size 1) and without FMA contraction, the
// configuration of the strict oracle build. The complete notice of every
// Eigen file this file derives from is reproduced verbatim below
// (MPL-2.0 §3.4).
//
// ---- Eigen/src/Core/products/TriangularSolverVector.h
//      (triangular_solve_vector, column-major, OnTheLeft):
//   This file is part of Eigen, a lightweight C++ template library
//   for linear algebra.
//
//   Copyright (C) 2008-2010 Gael Guennebaud <gael.guennebaud@inria.fr>
//
//   This Source Code Form is subject to the terms of the Mozilla
//   Public License v. 2.0. If a copy of the MPL was not distributed
//   with this file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// ---- Eigen/src/Core/products/TriangularSolverMatrix.h
//      (triangular_solve_matrix, OnTheLeft, column-major):
//   This file is part of Eigen, a lightweight C++ template library
//   for linear algebra.
//
//   Copyright (C) 2009 Gael Guennebaud <gael.guennebaud@inria.fr>
//
//   This Source Code Form is subject to the terms of the Mozilla
//   Public License v. 2.0. If a copy of the MPL was not distributed
//   with this file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// ---- Eigen/src/Core/SolveTriangular.h
//      (triangular_solver_selector, triangular_solver_unroller):
//   This file is part of Eigen, a lightweight C++ template library
//   for linear algebra.
//
//   Copyright (C) 2008-2009 Gael Guennebaud <gael.guennebaud@inria.fr>
//
//   This Source Code Form is subject to the terms of the Mozilla
//   Public License v. 2.0. If a copy of the MPL was not distributed
//   with this file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// ---- Eigen/src/Core/products/GeneralMatrixVector.h
//      (general_matrix_vector_product, column-major):
//   This file is part of Eigen, a lightweight C++ template library
//   for linear algebra.
//
//   Copyright (C) 2008-2016 Gael Guennebaud <gael.guennebaud@inria.fr>
//
//   This Source Code Form is subject to the terms of the Mozilla
//   Public License v. 2.0. If a copy of the MPL was not distributed
//   with this file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// ---- Eigen/src/Core/products/GeneralBlockPanelKernel.h
//      (evaluateProductBlockingSizesHeuristic kc rule, gebp accumulation):
//   This file is part of Eigen, a lightweight C++ template library
//   for linear algebra.
//
//   Copyright (C) 2008-2009 Gael Guennebaud <gael.guennebaud@inria.fr>
//
//   This Source Code Form is subject to the terms of the Mozilla
//   Public License v. 2.0. If a copy of the MPL was not distributed
//   with this file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// ---- Eigen/src/Core/Dot.h
//      (dot):
//   This file is part of Eigen, a lightweight C++ template library
//   for linear algebra.
//
//   Copyright (C) 2006-2008, 2010 Benoit Jacob <jacob.benoit.1@gmail.com>
//
//   This Source Code Form is subject to the terms of the Mozilla
//   Public License v. 2.0. If a copy of the MPL was not distributed
//   with this file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// ---- Eigen/src/Core/Redux.h
//      (sum, linear traversal):
//   This file is part of Eigen, a lightweight C++ template library
//   for linear algebra.
//
//   Copyright (C) 2008 Gael Guennebaud <gael.guennebaud@inria.fr>
//   Copyright (C) 2006-2008 Benoit Jacob <jacob.benoit.1@gmail.com>
//
//   This Source Code Form is subject to the terms of the Mozilla
//   Public License v. 2.0. If a copy of the MPL was not distributed
//   with this file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// ---- Eigen/src/Core/GeneralProduct.h
//      (GEMV dispatch, inner-product fallback):
//   This file is part of Eigen, a lightweight C++ template library
//   for linear algebra.
//
//   Copyright (C) 2006-2008 Benoit Jacob <jacob.benoit.1@gmail.com>
//   Copyright (C) 2008-2011 Gael Guennebaud <gael.guennebaud@inria.fr>
//
//   This Source Code Form is subject to the terms of the Mozilla
//   Public License v. 2.0. If a copy of the MPL was not distributed
//   with this file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// ---- Eigen/src/SparseLU/SparseLU_gemm_kernel.h
//      (sparselu_gemm):
//   This file is part of Eigen, a lightweight C++ template library
//   for linear algebra.
//
//   Copyright (C) 2012 Gael Guennebaud <gael.guennebaud@inria.fr>
//
//   This Source Code Form is subject to the terms of the Mozilla
//   Public License v. 2.0. If a copy of the MPL was not distributed
//   with this file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! The dense kernels Eigen 3.4.0 evaluates inside `SparseLU` in the strict
//! oracle build (`-DEIGEN_DONT_VECTORIZE -ffp-contract=off`, aarch64
//! default cache sizes L1 16 KiB / L2 512 KiB), with Eigen's operation
//! order:
//!
//! * [`trsv_unit_lower`] / [`trsv_upper`]: `triangular_solve_vector`
//!   (column-major, panels of `EIGEN_TUNE_TRIANGULAR_PANEL_WIDTH` = 8,
//!   zero right-hand-side entries skipped, `general_matrix_vector_product`
//!   with `alpha = -1` below/above each panel);
//! * [`unit_lower_unrolled`]: `triangular_solver_unroller` for compile-time
//!   sizes 2 and 3;
//! * [`trsm_unit_lower`]: `triangular_solve_matrix` (small panels of
//!   `max(mr, nr)` = 4 rows, `gebp` updates with `alpha = -1`, depth
//!   blocking `kc` of `gemm_blocking_space<…, KcFactor = 4>`);
//! * [`gemv`]: `general_matrix_vector_product` column-major
//!   (`c = 0; c = c + a·x` per column block, `res = c·α + res`), with the
//!   inner-product fallback of a `1 × 1` result ([`dot`]);
//! * [`sparselu_gemm`]: `SparseLU_gemm_kernel.h` with packet size 1 and
//!   `RK = 2` (without vectorisation the NEON headers are not included, so
//!   `EIGEN_ARCH_DEFAULT_NUMBER_OF_REGISTERS` keeps its default 8 from
//!   `Core/arch/Default/Settings.h`, and `RK = registers >= 16 ? 4 : 2`).
//!
//! Matrices are column-major views `(slice, offset, leading dimension)`.

// `res = c·α + res` and `c·(−1)` are kept literally: they are Eigen's
// `pmadd(c, alpha, res)` operand order (the same bits as `res += c·α` for
// numbers; NaN propagation follows the written operand order).
#![allow(clippy::assign_op_pattern, clippy::neg_multiply)]

/// `EIGEN_TUNE_TRIANGULAR_PANEL_WIDTH` (`Core/arch/Default/Settings.h`: 8).
const TRSV_PANEL: usize = 8;
/// `max(gebp_traits::mr, gebp_traits::nr)` for scalar `double`.
const TRSM_SMALL_PANEL: usize = 4;
/// `gebp_traits<double,double>::nr` (scalar path).
const GEBP_NR: usize = 4;
/// `gebp` peeling along the depth.
const GEBP_PK: usize = 8;
/// Default L1/L2 data cache sizes of Eigen 3.4.0 on aarch64.
const L1: usize = 16 * 1024;
const L2: usize = 512 * 1024;
const SCALAR: usize = std::mem::size_of::<f64>();

/// `general_matrix_vector_product<…, ColMajor>` on one buffer `v` holding
/// both the vector `x = v[x_off..x_off+cols]` and the result
/// `res = v[r_off..r_off+rows]` (disjoint ranges); `lhs(i, j) =
/// a[a_off + i + j·lda]`.
#[allow(clippy::too_many_arguments)] // BLAS-style kernel signature
pub(crate) fn gemv_in_place(
    rows: usize,
    cols: usize,
    a: &[f64],
    a_off: usize,
    lda: usize,
    v: &mut [f64],
    x_off: usize,
    r_off: usize,
    alpha: f64,
) {
    let block = gemv_block_cols(cols, lda);
    let mut j2 = 0;
    while j2 < cols {
        let jend = (j2 + block).min(cols);
        for i in 0..rows {
            let mut c = 0.0;
            for j in j2..jend {
                c += a[a_off + i + j * lda] * v[x_off + j];
            }
            let r = &mut v[r_off + i];
            *r = c * alpha + *r;
        }
        j2 = jend;
    }
}

/// Same kernel with the vector and the result in different buffers.
#[allow(clippy::too_many_arguments)] // BLAS-style kernel signature
pub(crate) fn gemv(
    rows: usize,
    cols: usize,
    a: &[f64],
    a_off: usize,
    lda: usize,
    x: &[f64],
    x_off: usize,
    res: &mut [f64],
    r_off: usize,
    alpha: f64,
) {
    let block = gemv_block_cols(cols, lda);
    let mut j2 = 0;
    while j2 < cols {
        let jend = (j2 + block).min(cols);
        for i in 0..rows {
            let mut c = 0.0;
            for j in j2..jend {
                c += a[a_off + i + j * lda] * x[x_off + j];
            }
            let r = &mut res[r_off + i];
            *r = c * alpha + *r;
        }
        j2 = jend;
    }
}

/// `block_cols = cols < 128 ? cols : (lhsStride·sizeof(double) < 32000 ? 16 : 4)`.
fn gemv_block_cols(cols: usize, lda: usize) -> usize {
    if cols < 128 {
        cols
    } else if lda * SCALAR < 32000 {
        16
    } else {
        4
    }
}

/// `lhs.row(0).dot(rhs)` as the linear redux `((a0·b0 + a1·b1) + a2·b2) +
/// …` (0 for an empty product); used by the inner-product fallback of a
/// GEMV product whose result is `1 × 1` (`generic_product_impl<…,
/// GemvProduct>::scaleAndAddTo`: `dst += alpha · dot`).
pub(crate) fn dot(n: usize, a: impl Fn(usize) -> f64, b: impl Fn(usize) -> f64) -> f64 {
    let mut res = if n == 0 { 0.0 } else { a(0) * b(0) };
    for k in 1..n {
        res += a(k) * b(k);
    }
    res
}

/// `y += alpha · A · x` as Eigen evaluates `y.noalias() ±= A * x` for a
/// run-time `rows × cols` matrix and a compile-time column vector: the
/// inner product when `rows == 1`, the column-major GEMV otherwise.
#[allow(clippy::too_many_arguments)] // BLAS-style kernel signature
pub(crate) fn gemv_product_in_place(
    rows: usize,
    cols: usize,
    a: &[f64],
    a_off: usize,
    lda: usize,
    v: &mut [f64],
    x_off: usize,
    r_off: usize,
    alpha: f64,
) {
    if rows == 1 {
        let d = dot(cols, |k| a[a_off + k * lda], |k| v[x_off + k]);
        v[r_off] += alpha * d;
    } else {
        gemv_in_place(rows, cols, a, a_off, lda, v, x_off, r_off, alpha);
    }
}

/// Separate-buffer variant of [`gemv_product_in_place`].
#[allow(clippy::too_many_arguments)] // BLAS-style kernel signature
pub(crate) fn gemv_product(
    rows: usize,
    cols: usize,
    a: &[f64],
    a_off: usize,
    lda: usize,
    x: &[f64],
    x_off: usize,
    res: &mut [f64],
    r_off: usize,
    alpha: f64,
) {
    if rows == 1 {
        let d = dot(cols, |k| a[a_off + k * lda], |k| x[x_off + k]);
        res[r_off] += alpha * d;
    } else {
        gemv(rows, cols, a, a_off, lda, x, x_off, res, r_off, alpha);
    }
}

/// `triangular_solve_vector<…, OnTheLeft, Lower|UnitDiag, ColMajor>`:
/// solves `L x = b` in place for `x = v[x_off..x_off+n]`, `L(i, j) =
/// a[a_off + i + j·lda]` (unit diagonal, not read).
pub(crate) fn trsv_unit_lower(
    n: usize,
    a: &[f64],
    a_off: usize,
    lda: usize,
    v: &mut [f64],
    x_off: usize,
) {
    let mut pi = 0;
    while pi < n {
        let apw = (n - pi).min(TRSV_PANEL);
        for k in 0..apw {
            let i = pi + k;
            let xi = v[x_off + i];
            if xi != 0.0 {
                let r = apw - k - 1;
                let s = i + 1;
                for t in 0..r {
                    let target = &mut v[x_off + s + t];
                    *target -= xi * a[a_off + (s + t) + i * lda];
                }
            }
        }
        let end_block = pi + apw;
        let r = n - end_block;
        if r > 0 {
            gemv_in_place(
                r,
                apw,
                a,
                a_off + end_block + pi * lda,
                lda,
                v,
                x_off + pi,
                x_off + end_block,
                -1.0,
            );
        }
        pi += TRSV_PANEL;
    }
}

/// `triangular_solve_vector<…, OnTheLeft, Upper, ColMajor>` (non-unit
/// diagonal: `x_i /= U(i, i)`, skipped when `x_i == 0`).
pub(crate) fn trsv_upper(
    n: usize,
    a: &[f64],
    a_off: usize,
    lda: usize,
    v: &mut [f64],
    x_off: usize,
) {
    let mut pi = n;
    while pi > 0 {
        let apw = pi.min(TRSV_PANEL);
        let start_block = pi - apw;
        for k in 0..apw {
            let i = pi - k - 1;
            if v[x_off + i] != 0.0 {
                v[x_off + i] /= a[a_off + i + i * lda];
                let xi = v[x_off + i];
                let r = apw - k - 1;
                let s = i - r;
                for t in 0..r {
                    let target = &mut v[x_off + s + t];
                    *target -= xi * a[a_off + (s + t) + i * lda];
                }
            }
        }
        let r = start_block;
        if r > 0 {
            gemv_in_place(
                r,
                apw,
                a,
                a_off + start_block * lda,
                lda,
                v,
                x_off + start_block,
                x_off,
                -1.0,
            );
        }
        pi = pi.saturating_sub(TRSV_PANEL);
    }
}

/// `triangular_solver_unroller` for a unit lower system of compile-time
/// size 2 or 3: `x1 -= L10·x0; x2 -= (L20·x0 + L21·x1)`.
pub(crate) fn unit_lower_unrolled(
    n: usize,
    a: &[f64],
    a_off: usize,
    lda: usize,
    x: &mut [f64],
    x_off: usize,
) {
    debug_assert!(n == 2 || n == 3);
    let l = |i: usize, j: usize| a[a_off + i + j * lda];
    let s1 = l(1, 0) * x[x_off];
    x[x_off + 1] -= s1;
    if n == 3 {
        let s2 = l(2, 0) * x[x_off] + l(2, 1) * x[x_off + 1];
        x[x_off + 2] -= s2;
    }
}

/// Depth block `kc` of `gemm_blocking_space<ColMajor, double, double, …,
/// KcFactor = 4>(rows, cols, depth)` (`evaluateProductBlockingSizesHeuristic`):
/// unchanged when `max(depth, rows, cols) < 48` or `depth <= max_kc`
/// (= 96), else Eigen's balanced block size.
pub(crate) fn trsm_kc(depth: usize, rows: usize, cols: usize) -> usize {
    if depth.max(rows).max(cols) < 48 {
        return depth;
    }
    let k_div = 4 * (SCALAR + GEBP_NR * SCALAR); // KcFactor·(mr·8 + nr·8)
    let k_sub = GEBP_NR * SCALAR; // mr·nr·8
    let max_kc = (((L1 - k_sub) / k_div) & !(GEBP_PK - 1)).max(1);
    if depth > max_kc {
        if depth.is_multiple_of(max_kc) {
            max_kc
        } else {
            max_kc - GEBP_PK * ((max_kc - 1 - depth % max_kc) / (GEBP_PK * (depth / max_kc + 1)))
        }
    } else {
        depth
    }
}

/// `gebp` accumulation of one result entry over `depth` products `t(l)`
/// for a result column that the `1 × nr` micro-kernel handles (`full` =
/// column index below `(cols / nr)·nr`): even and odd `l` accumulate
/// separately up to the peeled depth, then `c + d`, then the remainder;
/// other columns accumulate sequentially. Both start from `+0`.
fn gebp_acc(depth: usize, full: bool, t: impl Fn(usize) -> f64) -> f64 {
    if full {
        let peeled = depth & !(GEBP_PK - 1);
        let (mut c, mut d) = (0.0, 0.0);
        let mut l = 0;
        while l < peeled {
            c += t(l);
            d += t(l + 1);
            l += 2;
        }
        let mut c = c + d;
        for l in peeled..depth {
            c += t(l);
        }
        c
    } else {
        let mut c = 0.0;
        for l in 0..depth {
            c += t(l);
        }
        c
    }
}

/// `triangular_solve_matrix<double, …, OnTheLeft, Lower|UnitDiag,
/// ColMajor (tri), ColMajor (other)>`: solves `L X = B` in place for the
/// `size × cols` block `X(i, j) = o[o_off + i + j·ldo]`, `L(i, j) =
/// t[t_off + i + j·ldt]`.
#[allow(clippy::too_many_arguments)] // BLAS-style kernel signature
pub(crate) fn trsm_unit_lower(
    size: usize,
    cols: usize,
    t: &[f64],
    t_off: usize,
    ldt: usize,
    o: &mut [f64],
    o_off: usize,
    ldo: usize,
) {
    if size == 0 {
        return;
    }
    let tri = |i: usize, j: usize| t[t_off + i + j * ldt];
    let kc = trsm_kc(size, size, cols);
    let subcols = if cols > 0 {
        L2 / (4 * SCALAR * ldo.max(size))
    } else {
        0
    };
    let subcols = ((subcols / GEBP_NR) * GEBP_NR).max(GEBP_NR);
    let mut k2 = 0;
    while k2 < size {
        let akc = (size - k2).min(kc);
        let mut j2 = 0;
        while j2 < cols {
            let acols = (cols - j2).min(subcols);
            let full_cols = (acols / GEBP_NR) * GEBP_NR;
            let mut k1 = 0;
            while k1 < akc {
                let apw = (akc - k1).min(TRSM_SMALL_PANEL);
                // tr solve of the small panel (unit diagonal: a = 1)
                for k in 0..apw {
                    let i = k2 + k1 + k;
                    let rs = apw - k - 1;
                    let s = i + 1;
                    for j in j2..j2 + acols {
                        let oij = &mut o[o_off + i + j * ldo];
                        *oij *= 1.0;
                        let b = *oij;
                        for i3 in 0..rs {
                            let r = &mut o[o_off + (s + i3) + j * ldo];
                            *r -= b * tri(s + i3, i);
                        }
                    }
                }
                // gebp update of the rows below the panel, inside the kc block
                let length_target = akc - k1 - apw;
                let start_block = k2 + k1;
                if length_target > 0 {
                    let start_target = k2 + k1 + apw;
                    for jj in 0..acols {
                        let j = j2 + jj;
                        for tt in 0..length_target {
                            let row = start_target + tt;
                            let c = gebp_acc(apw, jj < full_cols, |l| {
                                tri(row, start_block + l) * o[o_off + (start_block + l) + j * ldo]
                            });
                            let r = &mut o[o_off + row + j * ldo];
                            *r = c * -1.0 + *r;
                        }
                    }
                }
                k1 += TRSM_SMALL_PANEL;
            }
            j2 += subcols;
        }
        // R2 -= A21 * B (rows below the kc block)
        let full_cols = (cols / GEBP_NR) * GEBP_NR;
        for row in (k2 + kc).min(size)..size {
            for j in 0..cols {
                let c = gebp_acc(akc, j < full_cols, |l| {
                    tri(row, k2 + l) * o[o_off + (k2 + l) + j * ldo]
                });
                let r = &mut o[o_off + row + j * ldo];
                *r = c * -1.0 + *r;
            }
        }
        k2 += kc;
    }
}

/// `sparselu_gemm<double>(m, n, d, A, lda, B, ldb, C, ldc)` with packet
/// size 1 and `RK = 2`: `C += A·B` where, per entry, the depth is consumed
/// in blocks of two, `c = (c + a0·b0) + a1·b1`, and an odd last term is
/// added as `c + b·a` (the `rd` tail).
#[allow(clippy::too_many_arguments)] // BLAS-style kernel signature
pub(crate) fn sparselu_gemm(
    m: usize,
    n: usize,
    d: usize,
    a: &[f64],
    a_off: usize,
    lda: usize,
    b: &[f64],
    b_off: usize,
    ldb: usize,
    c: &mut [f64],
    c_off: usize,
    ldc: usize,
) {
    const RK: usize = 2;
    let d_end = (d / RK) * RK;
    let rd = d - d_end;
    for j in 0..n {
        for i in 0..m {
            let mut acc = c[c_off + i + j * ldc];
            let mut k = 0;
            while k < d_end {
                for l in 0..RK {
                    acc += a[a_off + i + (k + l) * lda] * b[b_off + (k + l) + j * ldb];
                }
                k += RK;
            }
            if rd > 0 {
                let term =
                    |l: usize| b[b_off + d_end + l + j * ldb] * a[a_off + i + (d_end + l) * lda];
                let mut s = term(0);
                for l in 1..rd {
                    s += term(l);
                }
                acc += s;
            }
            c[c_off + i + j * ldc] = acc;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kc_rule_with_kc_factor_4() {
        assert_eq!(trsm_kc(40, 40, 3), 40);
        assert_eq!(trsm_kc(96, 96, 4), 96);
        // 100 = 56 + 44 (same number of sweeps as with max_kc = 96)
        assert_eq!(trsm_kc(100, 100, 4), 56);
        assert_eq!(trsm_kc(128, 128, 16), 72); // 72 + 56
        for depth in 97..1000 {
            let kc = trsm_kc(depth, depth, 1);
            assert_eq!(depth / kc, depth / 96, "{depth}");
        }
    }

    #[test]
    fn triangular_solves_invert_their_systems() {
        // L unit lower 20x20 (two TRSV panels), U upper
        let n = 20;
        let mut l = vec![0.0; n * n];
        let mut u = vec![0.0; n * n];
        for j in 0..n {
            for i in 0..n {
                let v = ((i * 7 + j * 3) % 11) as f64 / 11.0 - 0.5;
                if i > j {
                    l[i + j * n] = v;
                } else {
                    u[i + j * n] = if i == j { 2.0 + v } else { v };
                }
            }
        }
        let x: Vec<f64> = (0..n).map(|i| 1.0 + i as f64).collect();
        let mut b = vec![0.0; n];
        for i in 0..n {
            b[i] = x[i];
            for j in 0..i {
                b[i] += l[i + j * n] * x[j];
            }
        }
        trsv_unit_lower(n, &l, 0, n, &mut b, 0);
        for (bi, xi) in b.iter().zip(&x) {
            assert!((bi - xi).abs() < 1e-12);
        }
        let mut bu = vec![0.0; n];
        for i in 0..n {
            for j in i..n {
                bu[i] += u[i + j * n] * x[j];
            }
        }
        trsv_upper(n, &u, 0, n, &mut bu, 0);
        for (bi, xi) in bu.iter().zip(&x) {
            assert!((bi - xi).abs() < 1e-11, "{bi} {xi}");
        }
        // trsm with two columns, size 100 (kc = 56: one GEPP block)
        let n = 100;
        let mut l = vec![0.0; n * n];
        for j in 0..n {
            for i in j + 1..n {
                l[i + j * n] = (((i * 13 + j * 5) % 17) as f64 / 17.0 - 0.5) / 8.0;
            }
        }
        let mut o = vec![0.0; 2 * n];
        for c in 0..2 {
            for i in 0..n {
                let xi = (i + c) as f64;
                let mut s = xi;
                for j in 0..i {
                    s += l[i + j * n] * (j + c) as f64;
                }
                o[i + c * n] = s;
            }
        }
        trsm_unit_lower(n, 2, &l, 0, n, &mut o, 0, n);
        for c in 0..2 {
            for i in 0..n {
                assert!((o[i + c * n] - (i + c) as f64).abs() < 1e-9);
            }
        }
    }

    #[test]
    fn gemm_tail_order() {
        // d = 5: two blocks of 2 then a tail of 1, all sequential from c
        let a: Vec<f64> = (0..5).map(|k| 1.0 + k as f64).collect();
        let b = vec![1.0; 5];
        let mut c = vec![0.5];
        sparselu_gemm(1, 1, 5, &a, 0, 1, &b, 0, 5, &mut c, 0, 1);
        assert_eq!(c[0], 0.5 + 1.0 + 2.0 + 3.0 + 4.0 + 5.0);
    }
}
