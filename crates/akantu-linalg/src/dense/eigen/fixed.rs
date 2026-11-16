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
//      (EigenSolver::compute, eigenvalue extraction):
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
// ---- Eigen/src/Householder/Householder.h
//      (makeHouseholder, applyHouseholderOnTheLeft/Right):
//   This file is part of Eigen, a lightweight C++ template library
//   for linear algebra.
//
//   Copyright (C) 2010 Benoit Jacob <jacob.benoit.1@gmail.com>
//   Copyright (C) 2009 Gael Guennebaud <gael.guennebaud@inria.fr>
//
//   This Source Code Form is subject to the terms of the Mozilla
//   Public License v. 2.0. If a copy of the MPL was not distributed
//   with this file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// ---- Eigen/src/Householder/HouseholderSequence.h
//      (unblocked HouseholderSequence::evalTo):
//   This file is part of Eigen, a lightweight C++ template library
//   for linear algebra.
//
//   Copyright (C) 2009 Gael Guennebaud <gael.guennebaud@inria.fr>
//   Copyright (C) 2010 Benoit Jacob <jacob.benoit.1@gmail.com>
//
//   This Source Code Form is subject to the terms of the Mozilla
//   Public License v. 2.0. If a copy of the MPL was not distributed
//   with this file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// ---- Eigen/src/Jacobi/Jacobi.h
//      (JacobiRotation::makeGivens, apply_rotation_in_the_plane):
//   This file is part of Eigen, a lightweight C++ template library
//   for linear algebra.
//
//   Copyright (C) 2009 Benoit Jacob <jacob.benoit.1@gmail.com>
//   Copyright (C) 2009 Gael Guennebaud <gael.guennebaud@inria.fr>
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

//! `Eigen::EigenSolver<Matrix<double, N, N>>` for `1 ≤ N ≤ 4` on stack
//! arrays: the Hessenberg reduction, the Francis double-shift iteration
//! and the eigenvalue extraction of [`super::real_schur`], operation for
//! operation, without the general path's run-time-sized buffers (`Buf`,
//! `Mat`) and the moves of those buffers between the steps.
//!
//! Every function here is a transcription of its general counterpart with
//! the same name (same operands, same order, same branches); the products
//! of a compile-time `N < 8` matrix are coefficient-based
//! ([`super::Prod::SeqFirst`]). The differential test at the end of this
//! file compares `T`, `U`, the status and the eigenvalues bit for bit with
//! the general path.

use super::Rotation;
use super::real_schur::{Info, max};
use crate::dense::complex::Complex;

const EPS: f64 = f64::EPSILON;

/// Longest Householder tail of an `N ≤ 4` decomposition (Hessenberg:
/// `N − 2`; Francis step: 2).
const TAIL: usize = 2;

/// Square column-major `N × N` matrix on the stack (`m[col][row]`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct FMat<const N: usize>(pub(crate) [[f64; N]; N]);

impl<const N: usize> FMat<N> {
    const fn zeros() -> Self {
        Self([[0.0; N]; N])
    }

    fn identity() -> Self {
        let mut m = Self::zeros();
        for i in 0..N {
            m.0[i][i] = 1.0;
        }
        m
    }

    #[inline(always)]
    fn at(&self, i: usize, j: usize) -> f64 {
        self.0[j][i]
    }

    #[inline(always)]
    fn set(&mut self, i: usize, j: usize, v: f64) {
        self.0[j][i] = v;
    }

    #[inline(always)]
    fn get_mut(&mut self, i: usize, j: usize) -> &mut f64 {
        &mut self.0[j][i]
    }
}

/// [`super::Prod::SeqFirst`]`.sum`: `r = t0; r += t_k`, `0` when empty.
#[inline(always)]
fn seq_first(len: usize, t: impl Fn(usize) -> f64) -> f64 {
    if len == 0 {
        return 0.0;
    }
    let mut r = t(0);
    for k in 1..len {
        r += t(k);
    }
    r
}

/// [`super::make_householder`] (`tail.len() ≤ 2`): `(essential, tau, beta)`.
#[inline(always)]
fn make_householder(c0: f64, tail: &[f64]) -> ([f64; TAIL], f64, f64) {
    debug_assert!(tail.len() <= TAIL);
    let tail_sq_norm = seq_first(tail.len(), |k| tail[k] * tail[k]);
    let tol = f64::MIN_POSITIVE;
    let mut essential = [0.0; TAIL];
    if tail_sq_norm <= tol {
        (essential, 0.0, c0)
    } else {
        let mut beta = (c0 * c0 + tail_sq_norm).sqrt();
        if c0 >= 0.0 {
            beta = -beta;
        }
        let denom = c0 - beta;
        for (e, t) in essential.iter_mut().zip(tail) {
            *e = t / denom;
        }
        (essential, (beta - c0) / beta, beta)
    }
}

/// [`super::apply_householder_left`] with [`super::Prod::SeqFirst`] on rows
/// `r0..r0+m`, columns `c0..c0+ncols` (`essential.len() == m − 1`). The
/// general kernel computes every column's `tmp` before the updates; each
/// column's `tmp` and update touch only that column, so computing them
/// column by column gives the same values.
#[inline(always)]
fn apply_householder_left<const N: usize>(
    a: &mut FMat<N>,
    r0: usize,
    c0: usize,
    m: usize,
    ncols: usize,
    essential: &[f64],
    tau: f64,
) {
    debug_assert_eq!(essential.len() + 1, m);
    if m == 1 {
        let f = 1.0 - tau;
        for j in c0..c0 + ncols {
            *a.get_mut(r0, j) *= f;
        }
    } else if tau != 0.0 {
        for j in c0..c0 + ncols {
            let tmp = seq_first(m - 1, |k| essential[k] * a.at(r0 + 1 + k, j)) + a.at(r0, j);
            *a.get_mut(r0, j) -= tau * tmp;
            for (k, e) in essential.iter().enumerate() {
                *a.get_mut(r0 + 1 + k, j) -= (tau * e) * tmp;
            }
        }
    }
}

/// [`super::apply_householder_right`] with [`super::Prod::SeqFirst`] on rows
/// `r0..r0+nrows`, columns `c0..c0+m` (`essential.len() == m − 1`).
#[inline(always)]
fn apply_householder_right<const N: usize>(
    a: &mut FMat<N>,
    r0: usize,
    c0: usize,
    nrows: usize,
    m: usize,
    essential: &[f64],
    tau: f64,
) {
    debug_assert_eq!(essential.len() + 1, m);
    if m == 1 {
        let f = 1.0 - tau;
        for i in r0..r0 + nrows {
            *a.get_mut(i, c0) *= f;
        }
    } else if tau != 0.0 {
        for i in r0..r0 + nrows {
            let tmp = seq_first(m - 1, |k| a.at(i, c0 + 1 + k) * essential[k]) + a.at(i, c0);
            *a.get_mut(i, c0) -= tau * tmp;
            let tt = tau * tmp;
            for (k, e) in essential.iter().enumerate() {
                *a.get_mut(i, c0 + 1 + k) -= tt * e;
            }
        }
    }
}

/// [`super::rotate_rows`].
#[inline(always)]
fn rotate_rows<const N: usize>(
    a: &mut FMat<N>,
    p: usize,
    q: usize,
    c0: usize,
    ncols: usize,
    j: Rotation,
) {
    if j.c == 1.0 && j.s == 0.0 {
        return;
    }
    for col in c0..c0 + ncols {
        let (xi, yi) = (a.at(p, col), a.at(q, col));
        a.set(p, col, j.c * xi + j.s * yi);
        a.set(q, col, -j.s * xi + j.c * yi);
    }
}

/// [`super::rotate_cols`].
#[inline(always)]
fn rotate_cols<const N: usize>(
    a: &mut FMat<N>,
    p: usize,
    q: usize,
    r0: usize,
    nrows: usize,
    j: Rotation,
) {
    let jt = j.transpose();
    if jt.c == 1.0 && jt.s == 0.0 {
        return;
    }
    for row in r0..r0 + nrows {
        let (xi, yi) = (a.at(row, p), a.at(row, q));
        a.set(row, p, jt.c * xi + jt.s * yi);
        a.set(row, q, -jt.s * xi + jt.c * yi);
    }
}

/// `real_schur::hessenberg` in place: the packed matrix and the
/// Householder coefficients (`hcoeffs[..N − 1]`).
#[inline(always)]
fn hessenberg<const N: usize>(a: &mut FMat<N>, hcoeffs: &mut [f64; N]) {
    if N < 2 {
        return;
    }
    for i in 0..N - 1 {
        let rs = N - i - 1;
        let c0 = a.at(i + 1, i);
        let tl = rs - 1;
        let mut tail = [0.0; TAIL];
        for (k, t) in tail.iter_mut().enumerate().take(tl) {
            *t = a.at(i + 2 + k, i);
        }
        let (essential, tau, beta) = make_householder(c0, &tail[..tl]);
        for (k, e) in essential.iter().enumerate().take(tl) {
            a.set(i + 2 + k, i, *e);
        }
        a.set(i + 1, i, beta);
        hcoeffs[i] = tau;
        // A = H A
        apply_householder_left(a, i + 1, i + 1, rs, rs, &essential[..tl], tau);
        // A = A H'
        apply_householder_right(a, 0, i + 1, N, rs, &essential[..tl], tau);
    }
}

/// [`super::householder_q`] of the packed matrix (shift 1, length `N − 1`).
#[inline(always)]
fn householder_q<const N: usize>(packed: &FMat<N>, hcoeffs: &[f64; N]) -> FMat<N> {
    let mut dst = FMat::identity();
    if N < 2 {
        return dst;
    }
    for k in (0..N - 1).rev() {
        let corner = N - k - 1;
        let mut essential = [0.0; TAIL];
        for (e, i) in essential.iter_mut().zip(k + 2..N) {
            *e = packed.at(i, k);
        }
        apply_householder_left(
            &mut dst,
            k + 1,
            k + 1,
            corner,
            corner,
            &essential[..corner - 1],
            hcoeffs[k],
        );
    }
    dst
}

/// `cwiseAbs().maxCoeff()` over the column-major entries
/// (`real_schur::max_abs`).
#[inline(always)]
fn max_abs<const N: usize>(a: &FMat<N>) -> f64 {
    let d = a.0.as_flattened();
    let mut m = d.first().map_or(0.0, |x| x.abs());
    for x in d.iter().skip(1) {
        m = max(m, x.abs());
    }
    m
}

/// The iteration state (`real_schur::SchurState`); `U` is updated only
/// when `COMPUTE_U`.
struct State<const N: usize, const COMPUTE_U: bool> {
    t: FMat<N>,
    u: FMat<N>,
}

impl<const N: usize, const COMPUTE_U: bool> State<N, COMPUTE_U> {
    #[inline(always)]
    fn norm_of_t(&self) -> f64 {
        let mut norm = 0.0;
        for j in 0..N {
            let len = N.min(j + 2);
            norm += seq_first(len, |i| self.t.at(i, j).abs());
        }
        norm
    }

    #[inline(always)]
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

    #[inline(always)]
    fn split_off_two_rows(&mut self, iu: usize, exshift: f64) {
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
            rotate_rows(t, iu - 1, iu, iu - 1, N - iu + 1, rot.transpose());
            rotate_cols(t, iu - 1, iu, 0, iu + 1, rot);
            t.set(iu, iu - 1, 0.0);
            if COMPUTE_U {
                rotate_cols(&mut self.u, iu - 1, iu, 0, N, rot);
            }
        }
        if iu > 1 {
            self.t.set(iu - 1, iu - 2, 0.0);
        }
    }

    #[inline(always)]
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

    #[inline(always)]
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

    #[inline(always)]
    fn perform_francis_qr_step(&mut self, il: usize, im: usize, iu: usize, first_v: [f64; 3]) {
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
            let (essential, tau, beta) = make_householder(v[0], &v[1..]);
            if beta != 0.0 {
                if first && k > il {
                    let x = self.t.at(k, k - 1);
                    self.t.set(k, k - 1, -x);
                } else if !first {
                    self.t.set(k, k - 1, beta);
                }
                apply_householder_left(&mut self.t, k, k, 3, N - k, &essential, tau);
                let rows = iu.min(k + 3) + 1;
                apply_householder_right(&mut self.t, 0, k, rows, 3, &essential, tau);
                if COMPUTE_U {
                    apply_householder_right(&mut self.u, 0, k, N, 3, &essential, tau);
                }
            }
        }
        let v = [self.t.at(iu - 1, iu - 2), self.t.at(iu, iu - 2)];
        let (essential, tau, beta) = make_householder(v[0], &v[1..]);
        if beta != 0.0 {
            self.t.set(iu - 1, iu - 2, beta);
            apply_householder_left(
                &mut self.t,
                iu - 1,
                iu - 1,
                2,
                N - iu + 1,
                &essential[..1],
                tau,
            );
            apply_householder_right(&mut self.t, 0, iu - 1, iu + 1, 2, &essential[..1], tau);
            if COMPUTE_U {
                apply_householder_right(&mut self.u, 0, iu - 1, N, 2, &essential[..1], tau);
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

/// `$f` with `$i` bound to the literal equal to `$x` (`$x < 4`; otherwise
/// to `$x` itself).
macro_rules! lit {
    ($x:expr; $i:ident => $f:expr) => {
        match $x {
            0 => {
                let $i = 0;
                $f
            }
            1 => {
                let $i = 1;
                $f
            }
            2 => {
                let $i = 2;
                $f
            }
            3 => {
                let $i = 3;
                $f
            }
            other => {
                let $i = other;
                $f
            }
        }
    };
}

/// One Francis double-shift step of the active window `il..=iu`
/// (`initFrancisQRStep`, then `performFrancisQRStep`).
#[inline(always)]
fn francis<const N: usize, const COMPUTE_U: bool>(
    st: &mut State<N, COMPUTE_U>,
    il: usize,
    iu: usize,
    shift: &[f64; 3],
) {
    let (im, v) = st.init_francis_qr_step(il, iu, shift);
    lit!(im; m => st.perform_francis_qr_step(il, m, iu, v));
}

/// [`super::real_schur::real_schur`] of a compile-time `N × N` matrix
/// (`Storage::Fixed`, `1 ≤ N ≤ 4`): `(T, U, info)`; `U` is the identity
/// unless `COMPUTE_U` (the general path returns `None` there).
pub(crate) fn real_schur_fixed<const N: usize, const COMPUTE_U: bool>(
    a: &FMat<N>,
) -> (FMat<N>, FMat<N>, Info) {
    const { assert!(N >= 1 && N <= 4, "real_schur_fixed: 1 <= N <= 4") };
    let max_iters = 40 * N;
    let scale = max_abs(a);
    if scale < f64::MIN_POSITIVE {
        return (FMat::zeros(), FMat::identity(), Info::Success);
    }
    let mut packed = FMat(a.0.map(|c| c.map(|x| x / scale)));
    let mut hcoeffs = [0.0; N];
    hessenberg(&mut packed, &mut hcoeffs);
    let u = if COMPUTE_U {
        householder_q(&packed, &hcoeffs)
    } else {
        FMat::identity()
    };
    // matrixH(): zero below the sub-diagonal
    let mut t = packed;
    for j in 0..N {
        for i in j + 2..N {
            t.set(i, j, 0.0);
        }
    }
    let mut st = State::<N, COMPUTE_U> { t, u };

    let mut iu = N as isize - 1;
    let mut iter = 0;
    let mut total_iter = 0;
    let mut exshift = 0.0;
    let norm = st.norm_of_t();
    let consider_as_zero = max(norm * (EPS * EPS), f64::MIN_POSITIVE);
    if norm != 0.0 {
        while iu >= 0 {
            let iuu = iu as usize;
            // `iu < N ≤ 4`: each call below is made with literal indices
            // (`lit!`), so that the always-inlined steps unroll; the calls
            // and their arguments are those of the general loop
            let il = lit!(iuu; i => st.find_small_subdiag_entry(i, consider_as_zero));
            if il == iuu {
                let v = st.t.at(iuu, iuu) + exshift;
                st.t.set(iuu, iuu, v);
                if iuu > 0 {
                    st.t.set(iuu, iuu - 1, 0.0);
                }
                iu -= 1;
                iter = 0;
            } else if il + 1 == iuu {
                lit!(iuu; i => st.split_off_two_rows(i, exshift));
                iu -= 2;
                iter = 0;
            } else {
                let shift = lit!(iuu; i => st.compute_shift(i, iter, &mut exshift));
                iter += 1;
                total_iter += 1;
                if total_iter > max_iters {
                    break;
                }
                // `il + 2 ≤ iu ≤ 3`: (0, 2), (0, 3), (1, 3)
                match (il, iuu) {
                    (0, 2) => francis(&mut st, 0, 2, &shift),
                    (0, 3) => francis(&mut st, 0, 3, &shift),
                    (1, 3) => francis(&mut st, 1, 3, &shift),
                    _ => francis(&mut st, il, iuu, &shift),
                }
            }
        }
    }
    let info = if total_iter <= max_iters {
        Info::Success
    } else {
        Info::NoConvergence
    };
    for x in st.t.0.as_flattened_mut() {
        *x *= scale;
    }
    (st.t, st.u, info)
}

/// [`super::real_schur::schur_eigenvalues`] of a fixed-size Schur form.
pub(crate) fn schur_eigenvalues_fixed<const N: usize>(
    t: &FMat<N>,
    values: &mut [Complex; N],
) -> Info {
    let mut i = 0;
    while i < N {
        if i == N - 1 || t.at(i + 1, i) == 0.0 {
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

#[cfg(test)]
mod tests {
    use super::super::real_schur::{real_schur, schur_eigenvalues};
    use super::super::{Buf, Mat, Storage};
    use super::*;
    use akantu_core::bitwise_testing::{Values, same};

    fn same_slice(a: &[f64], b: &[f64]) -> bool {
        a.len() == b.len() && a.iter().zip(b).all(|(x, y)| same(*x, *y))
    }

    fn exact_slice(a: &[f64], b: &[f64]) -> bool {
        a.len() == b.len() && a.iter().zip(b).all(|(x, y)| x.to_bits() == y.to_bits())
    }

    /// One input through both paths; `exact` compares raw bits (payload
    /// mode), otherwise the D-046 rule.
    fn check<const N: usize>(a: &FMat<N>, exact: bool) -> Info {
        let cmp = if exact { exact_slice } else { same_slice };
        let m = Mat {
            n: N,
            d: Buf::from_slice(a.0.as_flattened()),
        };
        // without U
        let g = real_schur(&m, false, Storage::Fixed);
        let (t, u, info) = real_schur_fixed::<N, false>(a);
        assert_eq!(g.info, info, "info {a:?}");
        assert!(g.u.is_none());
        assert_eq!(u, FMat::identity());
        assert!(
            cmp(&g.t.d, t.0.as_flattened()),
            "T {a:?}\n{:?}\n{:?}",
            g.t.d,
            t
        );
        if info == Info::Success {
            let mut gv = vec![Complex::default(); N];
            let mut fv = [Complex::default(); N];
            assert_eq!(
                schur_eigenvalues(&g.t, &mut gv),
                schur_eigenvalues_fixed(&t, &mut fv)
            );
            for (x, y) in gv.iter().zip(&fv) {
                assert!(cmp(&[x.re, x.im], &[y.re, y.im]), "values {a:?}");
            }
        }
        // with U
        let g = real_schur(&m, true, Storage::Fixed);
        let (t, u, info2) = real_schur_fixed::<N, true>(a);
        assert_eq!(g.info, info2);
        assert!(cmp(&g.t.d, t.0.as_flattened()), "T(U) {a:?}");
        let gu = g.u.expect("U computed");
        assert!(cmp(&gu.d, u.0.as_flattened()), "U {a:?}");
        info
    }

    fn draw<const N: usize>(v: &mut Values, kind: usize) -> FMat<N> {
        let mut a = FMat::<N>::zeros();
        match kind {
            // arbitrary entries (special values included)
            0 => v.fill(a.0.as_flattened_mut()),
            // symmetric (Mazars' strains, the damage laws' tensors)
            1 => {
                for j in 0..N {
                    for i in j..N {
                        let x = v.draw();
                        a.0[j][i] = x;
                        a.0[i][j] = x;
                    }
                }
            }
            // symmetric with finite moderate values and repeated eigenvalues
            2 => {
                let d = v.finite();
                for j in 0..N {
                    for i in j..N {
                        let x = if i == j {
                            if v.next_u64() % 2 == 0 { d } else { v.finite() }
                        } else if v.next_u64() % 3 == 0 {
                            0.0
                        } else {
                            v.finite()
                        };
                        a.0[j][i] = x;
                        a.0[i][j] = x;
                    }
                }
            }
            // general finite (complex pairs, many iterations)
            _ => {
                for x in a.0.as_flattened_mut() {
                    *x = v.finite();
                }
            }
        }
        a
    }

    fn run<const N: usize>(seed: u64, rounds: usize) -> [usize; 3] {
        let mut v = Values::new(seed);
        let mut seen = [0usize; 3];
        for r in 0..rounds {
            let a = draw::<N>(&mut v, r % 4);
            let info = check(&a, false);
            seen[info as usize] += 1;
        }
        // payload mode: one NaN with a payload, everything else finite
        for _ in 0..rounds / 8 {
            let k = 1 + v.next_u64() % (N * N) as u64;
            v.payload_at(k);
            let mut a = FMat::<N>::zeros();
            v.fill(a.0.as_flattened_mut());
            v.default_mode();
            check(&a, true);
        }
        seen
    }

    #[test]
    fn fixed_schur_matches_the_general_path_bitwise() {
        run::<1>(0x5c4u64, 4_000);
        run::<2>(0x5c42, 8_000);
        let s3 = run::<3>(0x5c43, 40_000);
        run::<4>(0x5c44, 16_000);
        // the converged branch dominates; the others must occur
        assert!(s3[0] > 20_000 && s3[1] > 0, "{s3:?}");
    }

    /// Inputs that reach the exceptional shifts (iterations 10 and 30) and
    /// the iteration limit.
    #[test]
    fn fixed_schur_exceptional_shifts_and_no_convergence() {
        let hard: [FMat<3>; 4] = [
            // a permutation matrix: eigenvalues on the unit circle
            FMat([[0.0, 1.0, 0.0], [0.0, 0.0, 1.0], [1.0, 0.0, 0.0]]),
            FMat([[0.0, 0.0, 1.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]]),
            FMat([[f64::NAN, 1.0, 2.0], [3.0, 4.0, 5.0], [6.0, 7.0, 8.0]]),
            FMat([[1.0, f64::INFINITY, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]),
        ];
        for a in &hard {
            check(a, false);
        }
        let hard4 = FMat([
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
            [1.0, 0.0, 0.0, 0.0],
        ]);
        check(&hard4, false);
        // the arbitrary-entry draws reach NoConvergence (NaN entries)
        let mut v = Values::new(0xbad5);
        let mut nc = 0;
        for _ in 0..20_000 {
            let a = draw::<3>(&mut v, 0);
            if check(&a, false) == Info::NoConvergence {
                nc += 1;
            }
        }
        assert!(nc > 0);
    }
}
