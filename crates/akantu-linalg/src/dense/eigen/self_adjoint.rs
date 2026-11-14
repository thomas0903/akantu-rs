// SPDX-License-Identifier: MPL-2.0
//
// Rust translation of parts of Eigen 3.4.0 (https://eigen.tuxfamily.org),
// for akantu-rs (a port of Akantu, EPFL/LSMS). Source used: Debian
// libeigen3-dev 3.4.0-5, /usr/include/eigen3 in the oracle image
// akantu-oracle:dev. The complete notice of every Eigen file this file
// derives from is reproduced verbatim below (MPL-2.0 §3.4).
//
// ---- Eigen/src/Eigenvalues/SelfAdjointEigenSolver.h
//      (SelfAdjointEigenSolver, tridiagonal_qr_step, computeFromTridiagonal_impl):
//   This file is part of Eigen, a lightweight C++ template library
//   for linear algebra.
//
//   Copyright (C) 2008-2010 Gael Guennebaud <gael.guennebaud@inria.fr>
//   Copyright (C) 2010 Jitse Niesen <jitse@maths.leeds.ac.uk>
//
//   This Source Code Form is subject to the terms of the Mozilla
//   Public License v. 2.0. If a copy of the MPL was not distributed
//   with this file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// ---- Eigen/src/Eigenvalues/Tridiagonalization.h
//      (tridiagonalization_inplace (generic and 3x3)):
//   This file is part of Eigen, a lightweight C++ template library
//   for linear algebra.
//
//   Copyright (C) 2008 Gael Guennebaud <gael.guennebaud@inria.fr>
//   Copyright (C) 2010 Jitse Niesen <jitse@maths.leeds.ac.uk>
//
//   This Source Code Form is subject to the terms of the Mozilla
//   Public License v. 2.0. If a copy of the MPL was not distributed
//   with this file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// ---- Eigen/src/Core/products/SelfadjointMatrixVector.h
//      (selfadjoint_matrix_vector_product):
//   This file is part of Eigen, a lightweight C++ template library
//   for linear algebra.
//
//   Copyright (C) 2008-2009 Gael Guennebaud <gael.guennebaud@inria.fr>
//
//   This Source Code Form is subject to the terms of the Mozilla
//   Public License v. 2.0. If a copy of the MPL was not distributed
//   with this file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// ---- Eigen/src/Core/products/SelfadjointRank2Update.h
//      (selfadjoint_rank2_update_selector):
//   This file is part of Eigen, a lightweight C++ template library
//   for linear algebra.
//
//   Copyright (C) 2009 Gael Guennebaud <gael.guennebaud@inria.fr>
//
//   This Source Code Form is subject to the terms of the Mozilla
//   Public License v. 2.0. If a copy of the MPL was not distributed
//   with this file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// ---- Eigen/src/Core/MathFunctionsImpl.h
//      (positive_real_hypot, hypot_impl):
//   This file is part of Eigen, a lightweight C++ template library
//   for linear algebra.
//
//   Copyright (C) 2014 Pedro Gonnet (pedro.gonnet@gmail.com)
//   Copyright (C) 2016 Gael Guennebaud <gael.guennebaud@inria.fr>
//
//   This Source Code Form is subject to the terms of the Mozilla
//   Public License v. 2.0. If a copy of the MPL was not distributed
//   with this file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// ---- Eigen/src/Core/MathFunctions.h
//      (numext::mini, numext::maxi):
//   This file is part of Eigen, a lightweight C++ template library
//   for linear algebra.
//
//   Copyright (C) 2006-2010 Benoit Jacob <jacob.benoit.1@gmail.com>
//   Copyright (c) 2021, NVIDIA CORPORATION. All rights reserved.
//
//   This Source Code Form is subject to the terms of the Mozilla
//   Public License v. 2.0. If a copy of the MPL was not distributed
//   with this file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// ---- Eigen/src/Core/Dot.h
//      (dot()):
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
//      (cwiseAbs().maxCoeff(), run-time-size sum() order):
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
// ---- This file:
// Translation to Rust: Copyright (C) 2026 akantu-rs contributors.
//
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this file,
// You can obtain one at http://mozilla.org/MPL/2.0/. A copy is provided in
// LICENSES/MPL-2.0.txt. The rest of akantu-rs is LGPL-3.0-or-later;
// MPL-2.0 is file-level copyleft and compatible with it (MPL-2.0 §3.3);
// see NOTICE.md and docs/DEPENDENCIES.md.

//! `Eigen::SelfAdjointEigenSolver<MatrixType>` (real symmetric matrices,
//! only the lower triangle is read): scaling, tridiagonalisation (generic
//! Householder path with Eigen's symmetric matrix–vector and rank-2 update
//! kernels; closed form for compile-time 3×3), implicit symmetric QR with
//! Wilkinson shift, and ascending selection sort — Eigen 3.4.0 order without
//! vectorisation.

use super::real_schur::{Info, max, max_abs};
use super::{Buf, Mat, Prod, Rotation, Storage, make_householder, rotate_cols, seq_sum};

/// Eigen's `m_maxIterations` (per row).
const MAX_ITERATIONS: usize = 30;

/// Result: eigenvalues in Eigen's natural (ascending) order and, if
/// requested, the eigenvectors as columns.
pub(crate) struct SelfAdjoint {
    /// Inline up to 16 values (the per-point `3 × 3`
    /// solves allocate nothing).
    pub(crate) values: Buf,
    pub(crate) vectors: Option<Mat>,
    pub(crate) info: Info,
}

/// `selfadjoint_matrix_vector_product<double, ColMajor, Lower>::run` with
/// packet size 1: `res += alpha · A · rhs`, `A` the lower triangle of the
/// `size × size` block of `a` at `(o, o)`.
fn symv_lower(a: &Mat, o: usize, size: usize, rhs: &[f64], res: &mut [f64], alpha: f64) {
    let col = |j: usize, i: usize| a.at(o + i, o + j);
    let bound = size.saturating_sub(8) & !1usize;
    let mut j = 0;
    while j < bound {
        let t0 = alpha * rhs[j];
        let t1 = alpha * rhs[j + 1];
        let mut t2 = 0.0;
        let t3 = 0.0;
        let mut ptmp2 = 0.0;
        let mut ptmp3 = 0.0;
        res[j] += col(j, j) * t0;
        res[j + 1] += col(j + 1, j + 1) * t1;
        res[j + 1] += col(j, j + 1) * t0;
        t2 += col(j, j + 1) * rhs[j + 1];
        for i in j + 2..size {
            let (a0, a1) = (col(j, i), col(j + 1, i));
            res[i] = a0 * t0 + (a1 * t1 + res[i]);
            ptmp2 += a0 * rhs[i]; // IEEE addition is commutative: same bits as `t + acc`
            ptmp3 += a1 * rhs[i]; // IEEE addition is commutative: same bits as `t + acc`
        }
        res[j] += alpha * (t2 + ptmp2);
        res[j + 1] += alpha * (t3 + ptmp3);
        j += 2;
    }
    for j in bound..size {
        let t1 = alpha * rhs[j];
        let mut t2 = 0.0;
        res[j] += col(j, j) * t1;
        for i in j + 1..size {
            res[i] += col(j, i) * t1;
            t2 += col(j, i) * rhs[i];
        }
        res[j] += alpha * t2;
    }
}

/// Generic `tridiagonalization_inplace(matA, hCoeffs)`.
fn tridiagonalize_generic(a: &mut Mat, hcoeffs: &mut [f64]) {
    let n = a.n;
    for i in 0..n.saturating_sub(1) {
        let rs = n - i - 1;
        let c0 = a.at(i + 1, i);
        let tail: Vec<f64> = (i + 2..n).map(|r| a.at(r, i)).collect();
        let h = make_householder(c0, &tail);
        for (k, e) in h.essential.iter().enumerate() {
            a.set(i + 2 + k, i, *e);
        }
        let (tau, beta) = (h.tau, h.beta);
        a.set(i + 1, i, 1.0);
        let v: Vec<f64> = (i + 1..n).map(|r| a.at(r, i)).collect();
        // hCoeffs.tail(rs) = A_br.selfadjointView<Lower>() * (conj(h) * v)
        let mut w = vec![0.0; rs];
        symv_lower(a, i + 1, rs, &v, &mut w, tau);
        // hCoeffs.tail(rs) += (conj(h) * -0.5 * w.dot(v)) * v
        let coef = tau * -0.5 * seq_sum(rs, |k| w[k] * v[k]);
        for (wk, vk) in w.iter_mut().zip(&v) {
            *wk += coef * vk;
        }
        // A_br.selfadjointView<Lower>().rankUpdate(v, w, -1)
        for c in 0..rs {
            let (uc, wc) = (-v[c], -w[c]);
            for r in c..rs {
                *a.get_mut(i + 1 + r, i + 1 + c) += uc * w[r] + wc * v[r];
            }
        }
        a.set(i + 1, i, beta);
        hcoeffs[i..].copy_from_slice(&w);
        hcoeffs[i] = tau;
    }
}

/// `tridiagonalization_inplace_selector<MatrixType, 3, false>` (compile-time
/// 3×3 only).
fn tridiagonalize_3x3(a: &Mat, extract_q: bool) -> ([f64; 3], [f64; 2], Option<Mat>) {
    let tol = f64::MIN_POSITIVE;
    let m = |i: usize, j: usize| a.at(i, j);
    let mut diag = [m(0, 0), 0.0, 0.0];
    let mut sub = [0.0; 2];
    let v1norm2 = m(2, 0) * m(2, 0);
    let q = if v1norm2 <= tol {
        diag[1] = m(1, 1);
        diag[2] = m(2, 2);
        sub[0] = m(1, 0);
        sub[1] = m(2, 1);
        extract_q.then(|| Mat::identity(3))
    } else {
        let beta = (m(1, 0) * m(1, 0) + v1norm2).sqrt();
        let inv_beta = 1.0 / beta;
        let m01 = m(1, 0) * inv_beta;
        let m02 = m(2, 0) * inv_beta;
        let qq = 2.0 * m01 * m(2, 1) + m02 * (m(2, 2) - m(1, 1));
        diag[1] = m(1, 1) + m02 * qq;
        diag[2] = m(2, 2) - m02 * qq;
        sub[0] = beta;
        sub[1] = m(2, 1) - m01 * qq;
        extract_q.then(|| Mat {
            n: 3,
            d: Buf::from_slice(&[1.0, 0.0, 0.0, 0.0, m01, m02, 0.0, m02, -m01]),
        })
    };
    (diag, sub, q)
}

/// `numext::hypot` (`positive_real_hypot(|x|, |y|)`).
fn hypot(x: f64, y: f64) -> f64 {
    let (x, y) = (x.abs(), y.abs());
    if x.is_infinite() || y.is_infinite() {
        return f64::INFINITY;
    }
    if x.is_nan() || y.is_nan() {
        return f64::NAN;
    }
    let p = max(x, y);
    if p == 0.0 {
        return 0.0;
    }
    // numext::mini(y, x) = std::min(y, x) = (x < y) ? x : y
    let qp = (if x < y { x } else { y }) / p;
    p * (1.0 + qp * qp).sqrt()
}

/// `internal::tridiagonal_qr_step` (column-major `Q`).
fn tridiagonal_qr_step(
    diag: &mut [f64],
    subdiag: &mut [f64],
    start: usize,
    end: usize,
    q: Option<&mut Mat>,
) {
    let td = (diag[end - 1] - diag[end]) * 0.5;
    let e = subdiag[end - 1];
    let mut mu = diag[end];
    if td == 0.0 {
        mu -= e.abs();
    } else if e != 0.0 {
        let e2 = e * e;
        let h = hypot(td, e);
        let hs = if td > 0.0 { h } else { -h };
        if e2 == 0.0 {
            mu -= e / ((td + hs) / e);
        } else {
            mu -= e2 / (td + hs);
        }
    }
    let mut x = diag[start] - mu;
    let mut z = subdiag[start];
    let mut q = q;
    let mut k = start;
    while k < end && z != 0.0 {
        let rot = Rotation::make_givens(x, z);
        let (c, s) = (rot.c, rot.s);
        let sdk = s * diag[k] + c * subdiag[k];
        let dkp1 = s * subdiag[k] + c * diag[k + 1];
        diag[k] = c * (c * diag[k] - s * subdiag[k]) - s * (c * subdiag[k] - s * diag[k + 1]);
        diag[k + 1] = s * sdk + c * dkp1;
        subdiag[k] = c * sdk - s * dkp1;
        if k > start {
            subdiag[k - 1] = c * subdiag[k - 1] - s * z;
        }
        x = subdiag[k];
        if k < end - 1 {
            z = -s * subdiag[k + 1];
            subdiag[k + 1] *= c;
        }
        if let Some(q) = q.as_deref_mut() {
            let n = q.n;
            rotate_cols(q, k, k + 1, 0, n, rot);
        }
        k += 1;
    }
}

/// `internal::computeFromTridiagonal_impl`, including the final ascending
/// selection sort (first index of the minimum).
fn compute_from_tridiagonal(
    diag: &mut [f64],
    subdiag: &mut [f64],
    mut q: Option<&mut Mat>,
) -> Info {
    let n = diag.len();
    if n == 0 {
        return Info::Success;
    }
    let mut end = n - 1;
    let mut start = 0;
    let mut iter = 0;
    let consider_as_zero = f64::MIN_POSITIVE;
    let precision_inv = 1.0 / f64::EPSILON;
    while end > 0 {
        for i in start..end {
            if subdiag[i].abs() < consider_as_zero {
                subdiag[i] = 0.0;
            } else {
                let scaled = precision_inv * subdiag[i];
                if scaled * scaled <= diag[i].abs() + diag[i + 1].abs() {
                    subdiag[i] = 0.0;
                }
            }
        }
        while end > 0 && subdiag[end - 1] == 0.0 {
            end -= 1;
        }
        if end == 0 {
            break;
        }
        iter += 1;
        if iter > MAX_ITERATIONS * n {
            break;
        }
        start = end - 1;
        while start > 0 && subdiag[start - 1] != 0.0 {
            start -= 1;
        }
        tridiagonal_qr_step(diag, subdiag, start, end, q.as_deref_mut());
    }
    let info = if iter <= MAX_ITERATIONS * n {
        Info::Success
    } else {
        Info::NoConvergence
    };
    if info == Info::Success {
        for i in 0..n - 1 {
            let mut k = 0;
            let mut best = diag[i];
            for (kk, &v) in diag[i + 1..].iter().enumerate() {
                if v < best {
                    best = v;
                    k = kk + 1;
                }
            }
            if k > 0 {
                diag.swap(i, k + i);
                if let Some(q) = q.as_deref_mut() {
                    let nn = q.n;
                    for r in 0..nn {
                        q.d.swap(r + i * nn, r + (k + i) * nn);
                    }
                }
            }
        }
    }
    info
}

/// Eigen `SelfAdjointEigenSolver::compute(matrix, options)`.
pub(crate) fn self_adjoint_eigen(a: &Mat, compute_vectors: bool, storage: Storage) -> SelfAdjoint {
    let n = a.n;
    if n == 0 {
        return SelfAdjoint {
            values: Buf::zeros(0),
            vectors: compute_vectors.then(|| Mat::zeros(0)),
            info: Info::Success,
        };
    }
    if n == 1 {
        return SelfAdjoint {
            values: Buf::from_slice(&[a.at(0, 0)]),
            vectors: compute_vectors.then(|| Mat {
                n: 1,
                d: Buf::from_slice(&[1.0]),
            }),
            info: Info::Success,
        };
    }
    // mat = lower triangle; scale to [-1, 1]
    let mut mat = Mat::zeros(n);
    for j in 0..n {
        for i in j..n {
            mat.set(i, j, a.at(i, j));
        }
    }
    let mut scale = max_abs(&mat.d);
    if scale == 0.0 {
        scale = 1.0;
    }
    for j in 0..n {
        for i in j..n {
            *mat.get_mut(i, j) /= scale;
        }
    }
    // inline buffers (same values and order as the former `Vec`s)
    let (mut diag, mut subdiag, mut q): (Buf, Buf, Option<Mat>);
    if storage == Storage::Fixed && n == 3 {
        let (d, s, qq) = tridiagonalize_3x3(&mat, compute_vectors);
        diag = Buf::from_slice(&d);
        subdiag = Buf::from_slice(&s);
        q = qq;
    } else {
        let mut hcoeffs = vec![0.0; n - 1];
        tridiagonalize_generic(&mut mat, &mut hcoeffs);
        diag = (0..n).map(|i| mat.at(i, i)).collect();
        subdiag = (0..n - 1).map(|i| mat.at(i + 1, i)).collect();
        q = compute_vectors
            .then(|| super::householder_q(&mat, &hcoeffs, Prod::for_blocks(storage, n)));
    }
    let info = compute_from_tridiagonal(&mut diag, &mut subdiag, q.as_mut());
    for v in diag.iter_mut() {
        *v *= scale;
    }
    SelfAdjoint {
        values: diag,
        vectors: q,
        info,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hypot_matches_definition() {
        assert_eq!(hypot(3.0, 4.0), 5.0);
        assert_eq!(hypot(0.0, 0.0), 0.0);
        assert_eq!(hypot(-3.0, 0.0), 3.0);
        assert!(hypot(f64::NAN, f64::INFINITY).is_infinite());
    }

    #[test]
    fn symv_small_matches_dense() {
        // size 3 (< 8): only the scalar column loop
        let a = Mat {
            n: 3,
            d: Buf::from_slice(&[2.0, 1.0, 0.5, 9.0, 3.0, 0.25, 9.0, 9.0, 4.0]),
        };
        let rhs = [1.0, 2.0, 3.0];
        let mut res = [0.0; 3];
        symv_lower(&a, 0, 3, &rhs, &mut res, 1.0);
        // symmetric from the lower triangle: [[2,1,.5],[1,3,.25],[.5,.25,4]]
        assert_eq!(res, [2.0 + 2.0 + 1.5, 1.0 + 6.0 + 0.75, 0.5 + 0.5 + 12.0]);
    }
}
