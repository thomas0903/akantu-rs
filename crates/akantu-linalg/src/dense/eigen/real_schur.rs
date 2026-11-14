// SPDX-License-Identifier: MPL-2.0
//
// Rust translation of parts of Eigen 3.4.0 (https://eigen.tuxfamily.org),
// for akantu-rs (a port of Akantu, EPFL/LSMS). Source used: Debian
// libeigen3-dev 3.4.0-5, /usr/include/eigen3 in the oracle image
// akantu-oracle:dev. The complete notice of every Eigen file this file
// derives from is reproduced verbatim below (MPL-2.0 §3.4).
//
// ---- Eigen/src/Eigenvalues/HessenbergDecomposition.h
//      (HessenbergDecomposition):
//   This file is part of Eigen, a lightweight C++ template library
//   for linear algebra.
//
//   Copyright (C) 2008-2009 Gael Guennebaud <gael.guennebaud@inria.fr>
//   Copyright (C) 2010 Jitse Niesen <jitse@maths.leeds.ac.uk>
//
//   This Source Code Form is subject to the terms of the Mozilla
//   Public License v. 2.0. If a copy of the MPL was not distributed
//   with this file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// ---- Eigen/src/Eigenvalues/RealSchur.h
//      (RealSchur):
//   This file is part of Eigen, a lightweight C++ template library
//   for linear algebra.
//
//   Copyright (C) 2008 Gael Guennebaud <gael.guennebaud@inria.fr>
//   Copyright (C) 2010,2012 Jitse Niesen <jitse@maths.leeds.ac.uk>
//
//   This Source Code Form is subject to the terms of the Mozilla
//   Public License v. 2.0. If a copy of the MPL was not distributed
//   with this file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// ---- Eigen/src/Eigenvalues/EigenSolver.h
//      (EigenSolver, doComputeEigenvectors, eigenvectors()):
//   This file is part of Eigen, a lightweight C++ template library
//   for linear algebra.
//
//   Copyright (C) 2008 Gael Guennebaud <gael.guennebaud@inria.fr>
//   Copyright (C) 2010,2012 Jitse Niesen <jitse@maths.leeds.ac.uk>
//
//   This Source Code Form is subject to the terms of the Mozilla
//   Public License v. 2.0. If a copy of the MPL was not distributed
//   with this file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// ---- Eigen/src/Core/MathFunctions.h
//      (numext::maxi):
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
// ---- Eigen/src/Core/Dot.h
//      (normalize(), squaredNorm()):
//   This file is part of Eigen, a lightweight C++ template library
//   for linear algebra.
//
//   Copyright (C) 2006-2008, 2010 Benoit Jacob <jacob.benoit.1@gmail.com>
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

//! `Eigen::EigenSolver<MatrixType>` (real general matrices): Hessenberg
//! reduction, real Schur form by Francis double-shift QR iterations,
//! eigenvalue extraction in Schur order, and eigenvectors by
//! back-substitution, all in Eigen 3.4.0's evaluation order without
//! vectorisation.

use super::{
    Block, Buf, Mat, Prod, Rotation, Storage, apply_householder_left, apply_householder_right,
    householder_q, make_householder, rotate_cols, rotate_rows, seq_sum,
};
use crate::dense::complex::{Complex, ComplexDivision};

/// Outcome of a decomposition (Eigen `ComputationInfo`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Info {
    Success,
    NoConvergence,
    NumericalIssue,
}

/// Eigen `HessenbergDecomposition`: packed matrix and Householder
/// coefficients.
fn hessenberg(mut a: Mat, prod: Prod) -> (Mat, Buf) {
    let n = a.n;
    if n < 2 {
        return (a, Buf::zeros(0));
    }
    let mut hcoeffs = Buf::zeros(n - 1);
    for (i, hc) in hcoeffs.iter_mut().enumerate() {
        let rs = n - i - 1;
        let c0 = a.at(i + 1, i);
        let tail: Buf = (i + 2..n).map(|r| a.at(r, i)).collect();
        let h = make_householder(c0, &tail);
        for (k, e) in h.essential.iter().enumerate() {
            a.set(i + 2 + k, i, *e);
        }
        a.set(i + 1, i, h.beta);
        *hc = h.tau;
        let essential = h.essential;
        // A = H A
        apply_householder_left(
            &mut a,
            Block::new(i + 1, i + 1, rs, rs),
            &essential,
            h.tau,
            prod,
        );
        // A = A H'
        apply_householder_right(&mut a, Block::new(0, i + 1, n, rs), &essential, h.tau, prod);
    }
    (a, hcoeffs)
}

/// Eigen `RealSchur`: quasi-triangular `T` and (optionally) orthogonal `U`
/// with `A = U T Uᵀ`.
pub(crate) struct RealSchur {
    pub(crate) t: Mat,
    pub(crate) u: Option<Mat>,
    pub(crate) info: Info,
}

struct SchurState {
    t: Mat,
    u: Option<Mat>,
}

const EPS: f64 = f64::EPSILON;

impl SchurState {
    fn norm_of_t(&self) -> f64 {
        let n = self.t.n;
        let mut norm = 0.0;
        for j in 0..n {
            let len = n.min(j + 2);
            norm += seq_sum(len, |i| self.t.at(i, j).abs());
        }
        norm
    }

    fn find_small_subdiag_entry(&self, iu: usize, consider_as_zero: f64) -> usize {
        let mut res = iu;
        while res > 0 {
            let s = self.t.at(res - 1, res - 1).abs() + self.t.at(res, res).abs();
            let s = max(s * EPS, consider_as_zero);
            if self.t.at(res, res - 1).abs() <= s {
                break;
            }
            res -= 1;
        }
        res
    }

    fn split_off_two_rows(&mut self, iu: usize, exshift: f64) {
        let n = self.t.n;
        let t = &mut self.t;
        let p = 0.5 * (t.at(iu - 1, iu - 1) - t.at(iu, iu));
        let q = p * p + t.at(iu, iu - 1) * t.at(iu - 1, iu);
        *t.get_mut(iu, iu) += exshift;
        *t.get_mut(iu - 1, iu - 1) += exshift;
        if q >= 0.0 {
            let z = q.abs().sqrt();
            let rot = if p >= 0.0 {
                Rotation::make_givens(p + z, t.at(iu, iu - 1))
            } else {
                Rotation::make_givens(p - z, t.at(iu, iu - 1))
            };
            // rightCols(size-iu+1).applyOnTheLeft(iu-1, iu, rot.adjoint())
            rotate_rows(t, iu - 1, iu, iu - 1, n - iu + 1, rot.transpose());
            // topRows(iu+1).applyOnTheRight(iu-1, iu, rot)
            rotate_cols(t, iu - 1, iu, 0, iu + 1, rot);
            t.set(iu, iu - 1, 0.0);
            if let Some(u) = &mut self.u {
                rotate_cols(u, iu - 1, iu, 0, n, rot);
            }
        }
        if iu > 1 {
            self.t.set(iu - 1, iu - 2, 0.0);
        }
    }

    fn compute_shift(&mut self, iu: usize, iter: usize, exshift: &mut f64) -> [f64; 3] {
        let t = &mut self.t;
        let mut shift = [
            t.at(iu, iu),
            t.at(iu - 1, iu - 1),
            t.at(iu, iu - 1) * t.at(iu - 1, iu),
        ];
        // Wilkinson's original ad hoc shift
        if iter == 10 {
            *exshift += shift[0];
            for i in 0..=iu {
                *t.get_mut(i, i) -= shift[0];
            }
            let s = t.at(iu, iu - 1).abs() + t.at(iu - 1, iu - 2).abs();
            shift[0] = 0.75 * s;
            shift[1] = 0.75 * s;
            shift[2] = -0.4375 * s * s;
        }
        // MATLAB's new ad hoc shift
        if iter == 30 {
            let mut s = (shift[1] - shift[0]) / 2.0;
            s = s * s + shift[2];
            if s > 0.0 {
                s = s.sqrt();
                if shift[1] < shift[0] {
                    s = -s;
                }
                s += (shift[1] - shift[0]) / 2.0;
                s = shift[0] - shift[2] / s;
                *exshift += s;
                for i in 0..=iu {
                    *t.get_mut(i, i) -= s;
                }
                shift = [0.964; 3];
            }
        }
        shift
    }

    fn init_francis_qr_step(&self, il: usize, iu: usize, shift: &[f64; 3]) -> (usize, [f64; 3]) {
        let t = &self.t;
        let mut v = [0.0; 3];
        let mut im = iu - 2;
        loop {
            let tmm = t.at(im, im);
            let r = shift[0] - tmm;
            let s = shift[1] - tmm;
            v[0] = (r * s - shift[2]) / t.at(im + 1, im) + t.at(im, im + 1);
            v[1] = t.at(im + 1, im + 1) - tmm - r - s;
            v[2] = t.at(im + 2, im + 1);
            if im == il {
                break;
            }
            let lhs = t.at(im, im - 1) * (v[1].abs() + v[2].abs());
            let rhs = v[0] * (t.at(im - 1, im - 1).abs() + tmm.abs() + t.at(im + 1, im + 1).abs());
            if lhs.abs() < EPS * rhs {
                break;
            }
            im -= 1;
        }
        (im, v)
    }

    fn perform_francis_qr_step(&mut self, il: usize, im: usize, iu: usize, first_v: [f64; 3]) {
        let n = self.t.n;
        // compile-time-size essentials (2 or 1 entries): coefficient-based
        let fixed = Prod::SeqFirst;
        for k in im..=iu - 2 {
            let first = k == im;
            let v = if first {
                first_v
            } else {
                [
                    self.t.at(k, k - 1),
                    self.t.at(k + 1, k - 1),
                    self.t.at(k + 2, k - 1),
                ]
            };
            let h = make_householder(v[0], &v[1..]);
            if h.beta != 0.0 {
                if first && k > il {
                    let x = self.t.at(k, k - 1);
                    self.t.set(k, k - 1, -x);
                } else if !first {
                    self.t.set(k, k - 1, h.beta);
                }
                apply_householder_left(
                    &mut self.t,
                    Block::new(k, k, 3, n - k),
                    &h.essential,
                    h.tau,
                    fixed,
                );
                let rows = iu.min(k + 3) + 1;
                apply_householder_right(
                    &mut self.t,
                    Block::new(0, k, rows, 3),
                    &h.essential,
                    h.tau,
                    fixed,
                );
                if let Some(u) = &mut self.u {
                    apply_householder_right(u, Block::new(0, k, n, 3), &h.essential, h.tau, fixed);
                }
            }
        }
        let v = [self.t.at(iu - 1, iu - 2), self.t.at(iu, iu - 2)];
        let h = make_householder(v[0], &v[1..]);
        if h.beta != 0.0 {
            self.t.set(iu - 1, iu - 2, h.beta);
            apply_householder_left(
                &mut self.t,
                Block::new(iu - 1, iu - 1, 2, n - iu + 1),
                &h.essential,
                h.tau,
                fixed,
            );
            apply_householder_right(
                &mut self.t,
                Block::new(0, iu - 1, iu + 1, 2),
                &h.essential,
                h.tau,
                fixed,
            );
            if let Some(u) = &mut self.u {
                apply_householder_right(u, Block::new(0, iu - 1, n, 2), &h.essential, h.tau, fixed);
            }
        }
        // clean up pollution due to round-off errors
        for i in im + 2..=iu {
            self.t.set(i, i - 2, 0.0);
            if i > im + 2 {
                self.t.set(i, i - 3, 0.0);
            }
        }
    }
}

/// `numext::maxi` = `std::max(a, b)` = `(a < b) ? b : a`.
#[inline]
pub(crate) fn max(a: f64, b: f64) -> f64 {
    if a < b { b } else { a }
}

/// `matrix.cwiseAbs().maxCoeff()` (order irrelevant for non-NaN input).
pub(crate) fn max_abs(d: &[f64]) -> f64 {
    let mut m = d.first().map_or(0.0, |x| x.abs());
    for x in d.iter().skip(1) {
        m = max(m, x.abs());
    }
    m
}

/// Eigen `RealSchur::compute(matrix, computeU)`.
pub(crate) fn real_schur(a: &Mat, compute_u: bool, storage: Storage) -> RealSchur {
    let n = a.n;
    let max_iters = 40 * n;
    let scale = max_abs(&a.d);
    if scale < f64::MIN_POSITIVE {
        return RealSchur {
            t: Mat::zeros(n),
            u: compute_u.then(|| Mat::identity(n)),
            info: Info::Success,
        };
    }
    let prod = Prod::for_blocks(storage, n);
    let scaled = Mat {
        n,
        d: a.d.iter().map(|x| x / scale).collect(),
    };
    let (packed, hcoeffs) = hessenberg(scaled, prod);
    let u = compute_u.then(|| householder_q(&packed, &hcoeffs, prod));
    // matrixH(): zero below the sub-diagonal
    let mut t = packed;
    for j in 0..n {
        for i in j + 2..n {
            t.set(i, j, 0.0);
        }
    }
    let mut st = SchurState { t, u };

    let mut iu = n as isize - 1;
    let mut iter = 0;
    let mut total_iter = 0;
    let mut exshift = 0.0;
    let norm = st.norm_of_t();
    let consider_as_zero = max(norm * (EPS * EPS), f64::MIN_POSITIVE);
    if norm != 0.0 {
        while iu >= 0 {
            let iuu = iu as usize;
            let il = st.find_small_subdiag_entry(iuu, consider_as_zero);
            if il == iuu {
                let v = st.t.at(iuu, iuu) + exshift;
                st.t.set(iuu, iuu, v);
                if iuu > 0 {
                    st.t.set(iuu, iuu - 1, 0.0);
                }
                iu -= 1;
                iter = 0;
            } else if il + 1 == iuu {
                st.split_off_two_rows(iuu, exshift);
                iu -= 2;
                iter = 0;
            } else {
                let shift = st.compute_shift(iuu, iter, &mut exshift);
                iter += 1;
                total_iter += 1;
                if total_iter > max_iters {
                    break;
                }
                let (im, v) = st.init_francis_qr_step(il, iuu, &shift);
                st.perform_francis_qr_step(il, im, iuu, v);
            }
        }
    }
    let info = if total_iter <= max_iters {
        Info::Success
    } else {
        Info::NoConvergence
    };
    for x in st.t.d.iter_mut() {
        *x *= scale;
    }
    RealSchur {
        t: st.t,
        u: st.u,
        info,
    }
}

/// The eigenvalues of the quasi-triangular Schur form `t` into `values`
/// (`values.len() == t.n`; the loop of Eigen `EigenSolver::compute`).
/// Returns [`Info::NumericalIssue`] at the first non-finite value.
pub(crate) fn schur_eigenvalues(t: &Mat, values: &mut [Complex]) -> Info {
    let n = t.n;
    let mut i = 0;
    while i < n {
        if i == n - 1 || t.at(i + 1, i) == 0.0 {
            values[i] = Complex::new(t.at(i, i), 0.0);
            if !values[i].is_finite() {
                return Info::NumericalIssue;
            }
            i += 1;
        } else {
            let p = 0.5 * (t.at(i, i) - t.at(i + 1, i + 1));
            let mut t0 = t.at(i + 1, i);
            let mut t1 = t.at(i, i + 1);
            let maxval = max(p.abs(), max(t0.abs(), t1.abs()));
            t0 /= maxval;
            t1 /= maxval;
            let p0 = p / maxval;
            let z = maxval * (p0 * p0 + t0 * t1).abs().sqrt();
            values[i] = Complex::new(t.at(i + 1, i + 1) + p, z);
            values[i + 1] = Complex::new(t.at(i + 1, i + 1) + p, -z);
            if !(values[i].is_finite() && values[i + 1].is_finite()) {
                return Info::NumericalIssue;
            }
            i += 2;
        }
    }
    Info::Success
}

/// Eigen `EigenSolver` state after `compute`.
pub(crate) struct EigenSolver {
    pub(crate) values: Vec<Complex>,
    /// Real pseudo-eigenvectors (`m_eivec`), if requested.
    pub(crate) pseudo_vectors: Option<Mat>,
    pub(crate) info: Info,
    storage: Storage,
    /// Complex-division runtime of the eigenvector computation.
    division: ComplexDivision,
}

impl EigenSolver {
    /// Eigen `EigenSolver::compute(matrix, computeEigenvectors)`, with the
    /// complex divisions of the eigenvectors (back-substitution of complex
    /// pairs, normalisation) as `division`'s runtime does them.
    pub(crate) fn compute(
        a: &Mat,
        compute_vectors: bool,
        storage: Storage,
        division: ComplexDivision,
    ) -> Self {
        let n = a.n;
        let rs = real_schur(a, compute_vectors, storage);
        let mut out = Self {
            values: Vec::new(),
            pseudo_vectors: None,
            info: rs.info,
            storage,
            division,
        };
        if rs.info != Info::Success {
            return out;
        }
        let mut t = rs.t;
        let mut values = vec![Complex::default(); n];
        let info = schur_eigenvalues(&t, &mut values);
        if info != Info::Success {
            out.values = values;
            out.info = info;
            return out;
        }
        out.values = values;
        if compute_vectors {
            let mut eivec = rs.u.unwrap_or_else(|| Mat::identity(n));
            do_compute_eigenvectors(&mut t, &out.values, &mut eivec, storage, division);
            out.pseudo_vectors = Some(eivec);
        }
        out
    }

    /// Eigen `EigenSolver::eigenvectors()`: complex, normalised columns.
    pub(crate) fn eigenvectors(&self) -> Option<Vec<Vec<Complex>>> {
        let v = self.pseudo_vectors.as_ref()?;
        let n = v.n;
        let mut flat = vec![Complex::default(); n * n];
        eigenvectors_into(v, &self.values, self.storage, self.division, &mut flat);
        Some(
            flat.chunks_exact(n.max(1))
                .take(n)
                .map(<[Complex]>::to_vec)
                .collect(),
        )
    }
}

/// Eigen `EigenSolver::eigenvectors()` into `out` (column-major `n × n`,
/// `n = v.n`): complex, normalised columns of the real pseudo-eigenvectors
/// `v` (`m_eivec`) with eigenvalues `values`, the normalisation's complex
/// divisions as `division`'s runtime does them. Allocation-free
/// (fixed-size callers use stack buffers).
pub(crate) fn eigenvectors_into(
    v: &Mat,
    values: &[Complex],
    storage: Storage,
    division: ComplexDivision,
    out: &mut [Complex],
) {
    let n = v.n;
    debug_assert_eq!(out.len(), n * n);
    let precision = 2.0 * EPS;
    let mut j = 0;
    while j < n {
        let ev = values[j];
        if ev.im.abs() <= ev.re.abs() * precision || j + 1 == n {
            let col = &mut out[j * n..(j + 1) * n];
            for (i, c) in col.iter_mut().enumerate() {
                *c = Complex::new(v.at(i, j), 0.0);
            }
            normalize(col, storage, division);
        } else {
            let (c0, c1) = out[j * n..(j + 2) * n].split_at_mut(n);
            for i in 0..n {
                c0[i] = Complex::new(v.at(i, j), v.at(i, j + 1));
                c1[i] = Complex::new(v.at(i, j), -v.at(i, j + 1));
            }
            normalize(c0, storage, division);
            normalize(c1, storage, division);
            j += 1;
        }
        j += 1;
    }
}

/// `MatrixBase::normalize()` on a complex column (compile-time size: the
/// squared norm is fully unrolled when `8n + (n − 1) ≤ 110`), with the
/// complex divisions as `division`'s runtime does them.
fn normalize(col: &mut [Complex], storage: Storage, division: ComplexDivision) {
    let n = col.len();
    let t = |i: usize| col[i].abs2();
    let z = if storage == Storage::Fixed
        && n > 0
        && 8 * n + (n - 1) <= akantu_core::eigen_order::UNROLLING_LIMIT
    {
        akantu_core::eigen_order::redux_tree(0, n, &t, |a, b| a + b)
    } else {
        seq_sum(n, t)
    };
    if z > 0.0 {
        // `derived() /= sqrt(z)`: the real norm is converted to the complex
        // `Scalar` first, so each entry is a full complex division by
        // `(s, 0)` (libgcc `__divdc3`), not a component-wise one — this is
        // observable in the sign of zero entries.
        let s = Complex::new(z.sqrt(), 0.0);
        for c in col.iter_mut() {
            *c = c.quotient_with(s, division);
        }
    }
}

/// Eigen `EigenSolver::doComputeEigenvectors()`: back-substitution on the
/// quasi-triangular `t` (destroyed), then `eivec = U · X`.
pub(crate) fn do_compute_eigenvectors(
    t: &mut Mat,
    values: &[Complex],
    eivec: &mut Mat,
    storage: Storage,
    division: ComplexDivision,
) {
    let size = t.n;
    let mut norm = 0.0;
    for j in 0..size {
        let start = j.saturating_sub(1);
        norm += seq_sum(size - start, |k| t.at(j, start + k).abs());
    }
    if norm == 0.0 {
        return;
    }
    let dot_row_col = |t: &Mat, i: usize, col: usize, l: usize, n: usize| -> f64 {
        seq_sum(n - l + 1, |k| t.at(i, l + k) * t.at(l + k, col))
    };

    let mut n = size as isize - 1;
    while n >= 0 {
        let nn = n as usize;
        let p = values[nn].re;
        let q = values[nn].im;
        if q == 0.0 {
            // real vector
            let (mut lastr, mut lastw) = (0.0, 0.0);
            let mut l = nn;
            t.set(nn, nn, 1.0);
            for i in (0..nn).rev() {
                let w = t.at(i, i) - p;
                let r = dot_row_col(t, i, nn, l, nn);
                if values[i].im < 0.0 {
                    lastw = w;
                    lastr = r;
                } else {
                    l = i;
                    if values[i].im == 0.0 {
                        if w != 0.0 {
                            t.set(i, nn, -r / w);
                        } else {
                            t.set(i, nn, -r / (EPS * norm));
                        }
                    } else {
                        // solve real equations
                        let x = t.at(i, i + 1);
                        let y = t.at(i + 1, i);
                        let denom =
                            (values[i].re - p) * (values[i].re - p) + values[i].im * values[i].im;
                        let tt = (x * lastr - lastw * r) / denom;
                        t.set(i, nn, tt);
                        if x.abs() > lastw.abs() {
                            t.set(i + 1, nn, (-r - w * tt) / x);
                        } else {
                            t.set(i + 1, nn, (-lastr - y * tt) / lastw);
                        }
                    }
                    // overflow control
                    let tt = t.at(i, nn).abs();
                    if (EPS * tt) * tt > 1.0 {
                        for r in i..size {
                            *t.get_mut(r, nn) /= tt;
                        }
                    }
                }
            }
        } else if q < 0.0 && nn > 0 {
            // complex vector
            let (mut lastra, mut lastsa, mut lastw) = (0.0, 0.0, 0.0);
            let mut l = nn - 1;
            if t.at(nn, nn - 1).abs() > t.at(nn - 1, nn).abs() {
                t.set(nn - 1, nn - 1, q / t.at(nn, nn - 1));
                t.set(nn - 1, nn, -(t.at(nn, nn) - p) / t.at(nn, nn - 1));
            } else {
                let cc = Complex::new(0.0, -t.at(nn - 1, nn))
                    .quotient_with(Complex::new(t.at(nn - 1, nn - 1) - p, q), division);
                t.set(nn - 1, nn - 1, cc.re);
                t.set(nn - 1, nn, cc.im);
            }
            t.set(nn, nn - 1, 0.0);
            t.set(nn, nn, 1.0);
            for i in (0..nn - 1).rev() {
                let ra = dot_row_col(t, i, nn - 1, l, nn);
                let sa = dot_row_col(t, i, nn, l, nn);
                let w = t.at(i, i) - p;
                if values[i].im < 0.0 {
                    lastw = w;
                    lastra = ra;
                    lastsa = sa;
                } else {
                    l = i;
                    if values[i].im == 0.0 {
                        let cc = Complex::new(-ra, -sa).quotient_with(Complex::new(w, q), division);
                        t.set(i, nn - 1, cc.re);
                        t.set(i, nn, cc.im);
                    } else {
                        // solve complex equations
                        let x = t.at(i, i + 1);
                        let y = t.at(i + 1, i);
                        let mut vr = (values[i].re - p) * (values[i].re - p)
                            + values[i].im * values[i].im
                            - q * q;
                        let vi = (values[i].re - p) * 2.0 * q;
                        if vr == 0.0 && vi == 0.0 {
                            vr = EPS * norm * (w.abs() + q.abs() + x.abs() + y.abs() + lastw.abs());
                        }
                        let cc = Complex::new(
                            x * lastra - lastw * ra + q * sa,
                            x * lastsa - lastw * sa - q * ra,
                        )
                        .quotient_with(Complex::new(vr, vi), division);
                        t.set(i, nn - 1, cc.re);
                        t.set(i, nn, cc.im);
                        if x.abs() > (lastw.abs() + q.abs()) {
                            let a = (-ra - w * t.at(i, nn - 1) + q * t.at(i, nn)) / x;
                            let b = (-sa - w * t.at(i, nn) - q * t.at(i, nn - 1)) / x;
                            t.set(i + 1, nn - 1, a);
                            t.set(i + 1, nn, b);
                        } else {
                            let cc = Complex::new(
                                -lastra - y * t.at(i, nn - 1),
                                -lastsa - y * t.at(i, nn),
                            )
                            .quotient_with(Complex::new(lastw, q), division);
                            t.set(i + 1, nn - 1, cc.re);
                            t.set(i + 1, nn, cc.im);
                        }
                    }
                    // overflow control
                    let tt = max(t.at(i, nn - 1).abs(), t.at(i, nn).abs());
                    if (EPS * tt) * tt > 1.0 {
                        for r in i..size {
                            *t.get_mut(r, nn - 1) /= tt;
                            *t.get_mut(r, nn) /= tt;
                        }
                    }
                }
            }
            n -= 1;
        }
        // q > 0 with no conjugate partner cannot happen after the finiteness
        // check (Eigen asserts here).
        n -= 1;
    }

    // back-transform: eivec.col(j) = eivec.leftCols(j+1) * t.col(j).segment(0, j+1)
    let prod = Prod::for_blocks(storage, size);
    for j in (0..size).rev() {
        // inline buffer up to 16 rows; same values and
        // order as the former `Vec`
        let tmp: Buf = (0..size)
            .map(|r| prod.sum(j + 1, |k| eivec.at(r, k) * t.at(k, j)))
            .collect();
        for (r, &v) in tmp.iter().enumerate() {
            eivec.set(r, j, v);
        }
    }
}
