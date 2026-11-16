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
                    gebp_minus(
                        apw,
                        start_block,
                        start_target..start_target + length_target,
                        j2..j2 + acols,
                        full_cols,
                        t,
                        t_off,
                        ldt,
                        o,
                        o_off,
                        ldo,
                    );
                }
                k1 += TRSM_SMALL_PANEL;
            }
            j2 += subcols;
        }
        // R2 -= A21 * B (rows below the kc block)
        let full_cols = (cols / GEBP_NR) * GEBP_NR;
        let below = (k2 + kc).min(size);
        if below < size {
            gebp_minus(
                akc,
                k2,
                below..size,
                0..cols,
                full_cols,
                t,
                t_off,
                ldt,
                o,
                o_off,
                ldo,
            );
        }
        k2 += kc;
    }
}

/// The `gebp` update `o(row, j) = c·(−1) + o(row, j)` of [`trsm_unit_lower`]
/// for `row ∈ rows`, `j ∈ cols`, with `c` = [`gebp_acc`] over the `depth`
/// products `t(row, k0 + l)·o(k0 + l, j)` (`full` for the first
/// `full_cols` columns of `cols`). The rows are independent and the
/// right-hand values `o(k0 + l, j)` lie outside `rows`: the update runs
/// over tiles of rows, each lane with [`gebp_acc`]'s sequence
/// (`tests::trsm_matches_reference_bitwise`).
#[allow(clippy::too_many_arguments)] // BLAS-style kernel signature
fn gebp_minus(
    depth: usize,
    k0: usize,
    rows: std::ops::Range<usize>,
    cols: std::ops::Range<usize>,
    full_cols: usize,
    t: &[f64],
    t_off: usize,
    ldt: usize,
    o: &mut [f64],
    o_off: usize,
    ldo: usize,
) {
    debug_assert!(rows.start >= k0 + depth || rows.end <= k0);
    const XMAX: usize = 128;
    let mut xbuf = [0.0f64; XMAX];
    let mut xheap = Vec::new();
    let x: &mut [f64] = if depth <= XMAX {
        &mut xbuf[..depth]
    } else {
        xheap.resize(depth, 0.0);
        &mut xheap
    };
    let tcol = &t[t_off..];
    for (jj, j) in cols.enumerate() {
        let full = jj < full_cols;
        x.copy_from_slice(&o[o_off + k0 + j * ldo..][..depth]);
        let ocol = &mut o[o_off + j * ldo..];
        let mut row = rows.start;
        while row + 8 <= rows.end {
            gebp_minus_tile::<8>(
                depth,
                full,
                tcol,
                row + k0 * ldt,
                ldt,
                x,
                &mut ocol[row..row + 8],
            );
            row += 8;
        }
        if row + 4 <= rows.end {
            gebp_minus_tile::<4>(
                depth,
                full,
                tcol,
                row + k0 * ldt,
                ldt,
                x,
                &mut ocol[row..row + 4],
            );
            row += 4;
        }
        while row < rows.end {
            gebp_minus_tile::<1>(
                depth,
                full,
                tcol,
                row + k0 * ldt,
                ldt,
                x,
                &mut ocol[row..=row],
            );
            row += 1;
        }
    }
}

/// `MR` rows of [`gebp_minus`]: `t(r, l) = t[t0 + r + l·ldt]`, `x(l)`.
#[inline(always)]
fn gebp_minus_tile<const MR: usize>(
    depth: usize,
    full: bool,
    t: &[f64],
    t0: usize,
    ldt: usize,
    x: &[f64],
    out: &mut [f64],
) {
    let tl = |l: usize| -> &[f64; MR] {
        t[t0 + l * ldt..][..MR]
            .try_into()
            .unwrap_or_else(|_| unreachable!())
    };
    let mut c = [0.0f64; MR];
    let mut l = 0;
    if full {
        let peeled = depth & !(GEBP_PK - 1);
        let mut dd = [0.0f64; MR];
        while l < peeled {
            let (t0v, t1v) = (tl(l), tl(l + 1));
            let (x0, x1) = (x[l], x[l + 1]);
            for r in 0..MR {
                c[r] += t0v[r] * x0;
                dd[r] += t1v[r] * x1;
            }
            l += 2;
        }
        for r in 0..MR {
            c[r] = c[r] + dd[r];
        }
    }
    while l < depth {
        let tv = tl(l);
        let xl = x[l];
        for r in 0..MR {
            c[r] += tv[r] * xl;
        }
        l += 1;
    }
    for r in 0..MR {
        out[r] = c[r] * -1.0 + out[r];
    }
}

/// `sparselu_gemm<double>(m, n, d, A, lda, B, ldb, C, ldc)` with packet
/// size 1 and `RK = 2`: `C += A·B` where, per entry, the depth is consumed
/// in blocks of two, `c = (c + a0·b0) + a1·b1`, and an odd last term is
/// added as `c + b·a` (the `rd` tail).
///
/// Each entry's sequence is therefore `c ← c + a(i,k)·b(k,j)` for `k` in
/// order, then `c ← c + b(d−1,j)·a(i,d−1)` when `d` is odd. The entries are
/// independent, so the kernel computes them in register tiles of
/// [`GEMM_MR`] rows × up to [`GEMM_NR`] columns (the tile's `k` loop runs over
/// the whole depth, `a` read column by column, contiguous), with exactly
/// that sequence per entry: every product and every sum is a separate
/// IEEE operation (Rust never contracts `a * b + c` into an FMA), so the
/// result is bitwise the one of the entry-by-entry loop
/// (`tests::gemm_matches_reference_bitwise`).
#[cfg(test)] // the callers all use `sparselu_gemm_into_zero`
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
    gemm_tiles::<false>(m, n, d, a, a_off, lda, b, b_off, ldb, c, c_off, ldc);
}

/// [`sparselu_gemm`] on a destination that the caller would have set to
/// `+0.0` first: the accumulators start at `+0.0` without reading `C`, and
/// every entry of the `m × n` block is written. Bitwise the same as
/// zero-filling `C` and calling [`sparselu_gemm`].
#[allow(clippy::too_many_arguments)] // BLAS-style kernel signature
pub(crate) fn sparselu_gemm_into_zero(
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
    gemm_tiles::<true>(m, n, d, a, a_off, lda, b, b_off, ldb, c, c_off, ldc);
}

/// Rows of a register tile of [`sparselu_gemm`].
const GEMM_MR: usize = 4;
/// Columns of a register tile of [`sparselu_gemm`].
const GEMM_NR: usize = 4;

#[allow(clippy::too_many_arguments)] // BLAS-style kernel signature
#[inline(always)]
fn gemm_tiles<const ZERO: bool>(
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
    if m == 0 || n == 0 {
        return;
    }
    let mut j = 0;
    while j < n {
        let nr = (n - j).min(GEMM_NR);
        let bj = b_off + j * ldb;
        let cj = c_off + j * ldc;
        match nr {
            4 => gemm_cols::<4, ZERO>(m, d, a, a_off, lda, b, bj, ldb, c, cj, ldc),
            3 => gemm_cols::<3, ZERO>(m, d, a, a_off, lda, b, bj, ldb, c, cj, ldc),
            2 => gemm_cols::<2, ZERO>(m, d, a, a_off, lda, b, bj, ldb, c, cj, ldc),
            _ => gemm_cols::<1, ZERO>(m, d, a, a_off, lda, b, bj, ldb, c, cj, ldc),
        }
        j += nr;
    }
}

/// All rows of `NR` columns: tiles of [`GEMM_MR`] rows, then 4, 2, 1.
#[allow(clippy::too_many_arguments)] // BLAS-style kernel signature
#[inline(always)]
fn gemm_cols<const NR: usize, const ZERO: bool>(
    m: usize,
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
    // the columns of B, each `d` long
    let bcols: [&[f64]; NR] = std::array::from_fn(|jn| &b[b_off + jn * ldb..][..d]);
    let mut i = 0;
    while i + GEMM_MR <= m {
        gemm_tile::<GEMM_MR, NR, ZERO>(d, a, a_off + i, lda, &bcols, c, c_off + i, ldc);
        i += GEMM_MR;
    }
    if i + 2 <= m {
        gemm_tile::<2, NR, ZERO>(d, a, a_off + i, lda, &bcols, c, c_off + i, ldc);
        i += 2;
    }
    if i < m {
        gemm_tile::<1, NR, ZERO>(d, a, a_off + i, lda, &bcols, c, c_off + i, ldc);
    }
}

/// One `MR × NR` tile: `c(r, jn) = a_off + r + jn·ldc`, `a(r, k) = a_off +
/// r + k·lda`.
#[allow(clippy::too_many_arguments)] // BLAS-style kernel signature
#[inline(always)]
fn gemm_tile<const MR: usize, const NR: usize, const ZERO: bool>(
    d: usize,
    a: &[f64],
    a_off: usize,
    lda: usize,
    bcols: &[&[f64]; NR],
    c: &mut [f64],
    c_off: usize,
    ldc: usize,
) {
    let d_end = d & !1;
    let mut acc = [[0.0f64; MR]; NR];
    if !ZERO {
        for (jn, accj) in acc.iter_mut().enumerate() {
            accj.copy_from_slice(&c[c_off + jn * ldc..][..MR]);
        }
    }
    for k in 0..d_end {
        let ak: &[f64; MR] = a[a_off + k * lda..][..MR]
            .try_into()
            .unwrap_or_else(|_| unreachable!());
        for (accj, bc) in acc.iter_mut().zip(bcols) {
            let bk = bc[k];
            for r in 0..MR {
                accj[r] = accj[r] + ak[r] * bk;
            }
        }
    }
    if d_end < d {
        // the `rd` tail: `c + b·a`
        let ak: &[f64; MR] = a[a_off + d_end * lda..][..MR]
            .try_into()
            .unwrap_or_else(|_| unreachable!());
        for (accj, bc) in acc.iter_mut().zip(bcols) {
            let bk = bc[d_end];
            for r in 0..MR {
                accj[r] = accj[r] + bk * ak[r];
            }
        }
    }
    for (jn, accj) in acc.iter().enumerate() {
        c[c_off + jn * ldc..][..MR].copy_from_slice(accj);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The entry-by-entry `sparselu_gemm` the tiled kernel replaced (the
    /// reference of the bitwise property test).
    #[allow(clippy::too_many_arguments)]
    fn sparselu_gemm_reference(
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
                    let term = |l: usize| {
                        b[b_off + d_end + l + j * ldb] * a[a_off + i + (d_end + l) * lda]
                    };
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

    /// A small deterministic generator (xorshift64*) of doubles that include
    /// signed zeros, infinities, NaNs, subnormals and wide exponents.
    pub(super) struct Gen(pub u64);
    impl Gen {
        pub fn next_u64(&mut self) -> u64 {
            self.0 ^= self.0 >> 12;
            self.0 ^= self.0 << 25;
            self.0 ^= self.0 >> 27;
            self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
        }
        pub fn below(&mut self, n: usize) -> usize {
            (self.next_u64() % n as u64) as usize
        }
        pub fn value(&mut self, specials: bool) -> f64 {
            let r = self.next_u64();
            if specials {
                match r % 64 {
                    0 => return 0.0,
                    1 => return -0.0,
                    2 => return f64::INFINITY,
                    3 => return f64::NEG_INFINITY,
                    4 => return f64::NAN,
                    5 => return f64::from_bits(0x7ff8_0000_0000_1234 | (r & (1 << 63))),
                    6 => return f64::from_bits(r & 0x800f_ffff_ffff_ffff), // subnormal
                    7 => return f64::MIN_POSITIVE,
                    8 => return f64::MAX,
                    9 => return f64::from_bits((r >> 1) | 1), // any finite or not
                    _ => {}
                }
            }
            let m = (r >> 11) as f64 / (1u64 << 53) as f64 - 0.5;
            m * 2f64.powi(((r >> 3) % 40) as i32 - 20)
        }
        pub fn vec(&mut self, n: usize, specials: bool) -> Vec<f64> {
            (0..n).map(|_| self.value(specials)).collect()
        }
    }

    /// Bitwise equality, any NaN equal to any NaN: Rust does not specify
    /// which NaN payload an operation with NaN operands returns (and LLVM
    /// may commute the operands of `+` and `*`), in the reference kernel as
    /// in the new one. Non-finite factors are refused by `factorize`.
    pub(super) fn same_bits(x: f64, y: f64) -> bool {
        x.to_bits() == y.to_bits() || (x.is_nan() && y.is_nan())
    }

    /// `gebp_acc` of the reference `trsm_unit_lower`.
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

    /// The row-by-row `trsm_unit_lower` that the row-tiled one replaced.
    #[allow(clippy::too_many_arguments)] // BLAS-style kernel signature
    fn trsm_unit_lower_reference(
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
                                    tri(row, start_block + l)
                                        * o[o_off + (start_block + l) + j * ldo]
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

    #[test]
    fn trsm_matches_reference_bitwise() {
        let mut g = Gen(0x0123_4567_89AB_CDEF);
        for case in 0..600 {
            let specials = case % 4 == 0;
            // sizes around the kc split (96 / 128) and the panel widths
            let size = match case % 5 {
                0 => g.below(10),
                1 => 90 + g.below(50),
                _ => g.below(70),
            };
            let cols = g.below(18);
            let ldt = size + g.below(5);
            let ldo = size + g.below(3);
            let (to, oo) = (g.below(4), g.below(4));
            let t = g.vec(to + ldt * size.max(1) + 1, specials);
            let o0 = g.vec(oo + ldo * cols.max(1) + size, specials);
            let mut o1 = o0.clone();
            let mut o2 = o0.clone();
            trsm_unit_lower_reference(size, cols, &t, to, ldt, &mut o1, oo, ldo);
            trsm_unit_lower(size, cols, &t, to, ldt, &mut o2, oo, ldo);
            for (x, y) in o1.iter().zip(&o2) {
                assert!(
                    same_bits(*x, *y),
                    "case {case}: size={size} cols={cols}: {x:e} {y:e}"
                );
            }
        }
    }

    #[test]
    fn gemm_matches_reference_bitwise() {
        let mut g = Gen(0x9E37_79B9_7F4A_7C15);
        for case in 0..4000 {
            let specials = case % 3 == 0;
            let m = g.below(40);
            let n = g.below(19);
            let d = g.below(23);
            let lda = m + g.below(4);
            let ldb = d + g.below(3);
            let ldc = m + g.below(3);
            let (ao, bo, co) = (g.below(5), g.below(5), g.below(5));
            let a = g.vec(ao + lda * d.max(1) + m, specials);
            let b = g.vec(bo + ldb * n.max(1) + d, specials);
            let c0 = g.vec(co + ldc * n.max(1) + m, specials);
            let mut c1 = c0.clone();
            let mut c2 = c0.clone();
            sparselu_gemm_reference(m, n, d, &a, ao, lda, &b, bo, ldb, &mut c1, co, ldc);
            sparselu_gemm(m, n, d, &a, ao, lda, &b, bo, ldb, &mut c2, co, ldc);
            for (x, y) in c1.iter().zip(&c2) {
                assert!(
                    same_bits(*x, *y),
                    "case {case}: m={m} n={n} d={d}: {x:e} {y:e}"
                );
            }
            // the zero-destination variant equals a zero fill + the reference
            let mut z1 = c0.clone();
            let mut z2 = c0.clone();
            for j in 0..n {
                z1[co + j * ldc..co + j * ldc + m].fill(0.0);
            }
            sparselu_gemm_reference(m, n, d, &a, ao, lda, &b, bo, ldb, &mut z1, co, ldc);
            sparselu_gemm_into_zero(m, n, d, &a, ao, lda, &b, bo, ldb, &mut z2, co, ldc);
            for j in 0..n {
                z2[co + j * ldc..co + j * ldc + m]
                    .iter()
                    .zip(&z1[co + j * ldc..co + j * ldc + m])
                    .for_each(|(x, y)| assert!(same_bits(*x, *y), "case {case}"));
            }
        }
    }

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
