// SPDX-License-Identifier: MPL-2.0
//
// Rust translation of parts of Eigen 3.4.0 (https://eigen.tuxfamily.org),
// for akantu-rs (a port of Akantu, EPFL/LSMS). Source used: Debian
// libeigen3-dev 3.4.0-5, /usr/include/eigen3 in the oracle image
// akantu-oracle:dev. The complete notice of every Eigen file this file
// derives from is reproduced verbatim below (MPL-2.0 §3.4).
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
// ---- Eigen/src/Core/GeneralProduct.h
//      (product-type selection (EIGEN_CACHEFRIENDLY_PRODUCT_THRESHOLD)):
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
// ---- Eigen/src/Core/ProductEvaluators.h
//      (coefficient-based product evaluation order):
//   This file is part of Eigen, a lightweight C++ template library
//   for linear algebra.
//
//   Copyright (C) 2006-2008 Benoit Jacob <jacob.benoit.1@gmail.com>
//   Copyright (C) 2008-2010 Gael Guennebaud <gael.guennebaud@inria.fr>
//   Copyright (C) 2011 Jitse Niesen <jitse@maths.leeds.ac.uk>
//
//   This Source Code Form is subject to the terms of the Mozilla
//   Public License v. 2.0. If a copy of the MPL was not distributed
//   with this file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// ---- Eigen/src/Core/products/GeneralMatrixVector.h
//      (scalar GEMV accumulation order):
//   This file is part of Eigen, a lightweight C++ template library
//   for linear algebra.
//
//   Copyright (C) 2008-2016 Gael Guennebaud <gael.guennebaud@inria.fr>
//
//   This Source Code Form is subject to the terms of the Mozilla
//   Public License v. 2.0. If a copy of the MPL was not distributed
//   with this file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// ---- Eigen/src/Core/Redux.h
//      (run-time-size sum() order (redux_impl, NoUnrolling)):
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
//      (squaredNorm()):
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

//! Translation of Eigen 3.4.0's eigenvalue solvers with the evaluation order
//! of a build without vectorisation (`-DEIGEN_DONT_VECTORIZE`, the oracle's
//! `strict` configuration): the Householder / Givens kernels shared by
//! [`real_schur`] (`EigenSolver`) and [`self_adjoint`]
//! (`SelfAdjointEigenSolver`).
//!
//! Matrices are square, column-major (`Mat`). Where Eigen's result depends
//! on whether the matrix type has a compile-time size, the caller passes a
//! [`Storage`].

pub(crate) mod fixed;
pub(crate) mod real_schur;
pub(crate) mod self_adjoint;

/// Whether the upstream matrix type has a compile-time size
/// (`Matrix<Real, n, n>`) or a run-time size (`Matrix<Real>`, dynamic
/// `Map`). Eigen's expression evaluation (and hence the rounding of some
/// intermediate sums) differs between the two.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Storage {
    /// Compile-time size (`Eigen::Matrix<double, n, n>`).
    Fixed,
    /// Run-time size (`Eigen::MatrixXd`, `Map<MatrixXd>`).
    Dynamic,
}

/// Summation order of a product with a run-time inner dimension.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Prod {
    /// Coefficient-based product: `r = t0; r += t_k` (small compile-time
    /// operand sizes).
    SeqFirst,
    /// GEMV kernel: `acc = 0; acc = t_k + acc` (dynamic, or compile-time
    /// sizes `≥ 8`).
    FromZero,
}

impl Prod {
    /// Order used by Eigen for products of blocks of an `n × n` matrix of
    /// the given storage with a run-time inner dimension.
    pub(crate) fn for_blocks(storage: Storage, n: usize) -> Self {
        match storage {
            Storage::Fixed if n < akantu_core::eigen_order::CACHEFRIENDLY_PRODUCT_THRESHOLD => {
                Prod::SeqFirst
            }
            _ => Prod::FromZero,
        }
    }

    /// Sum `t(0..len)` in this order.
    #[inline]
    pub(crate) fn sum(self, len: usize, t: impl Fn(usize) -> f64) -> f64 {
        match self {
            Prod::SeqFirst => {
                if len == 0 {
                    return 0.0;
                }
                let mut r = t(0);
                for k in 1..len {
                    r += t(k);
                }
                r
            }
            Prod::FromZero => {
                let mut acc = 0.0;
                for k in 0..len {
                    acc += t(k); // IEEE addition is commutative: same bits as `t + acc`
                }
                acc
            }
        }
    }
}

/// Run-time-size reduction `sum()`: sequential from the first term, `0` when
/// empty (Eigen `redux_impl<…, NoUnrolling>`).
#[inline]
pub(crate) fn seq_sum(len: usize, t: impl Fn(usize) -> f64) -> f64 {
    Prod::SeqFirst.sum(len, t)
}

/// Inline capacity of [`Buf`]: every buffer of a `4 × 4` (or smaller)
/// matrix decomposition fits without heap allocation
/// (the per-point `3 × 3` eigenvalues of Mazars must not allocate).
const BUF_INLINE: usize = 16;

/// A `f64` buffer stored inline up to [`BUF_INLINE`] values and on the heap
/// beyond; behaves as a slice (`Deref`). Only storage changes: every value
/// and operation order is the one of the former `Vec<f64>`.
#[derive(Clone)]
pub(crate) struct Buf {
    len: usize,
    inline: [f64; BUF_INLINE],
    heap: Vec<f64>,
}

impl Buf {
    pub(crate) fn zeros(len: usize) -> Self {
        let mut b = Self {
            len: 0,
            inline: [0.0; BUF_INLINE],
            heap: Vec::new(),
        };
        if len > BUF_INLINE {
            b.heap = vec![0.0; len];
        }
        b.len = len;
        b
    }

    pub(crate) fn from_slice(v: &[f64]) -> Self {
        let mut b = Self::zeros(v.len());
        b.copy_from_slice(v);
        b
    }

    fn push(&mut self, v: f64) {
        if self.len < BUF_INLINE {
            self.inline[self.len] = v;
        } else {
            if self.len == BUF_INLINE {
                self.heap = self.inline.to_vec();
            }
            self.heap.push(v);
        }
        self.len += 1;
    }
}

impl std::ops::Deref for Buf {
    type Target = [f64];
    fn deref(&self) -> &[f64] {
        if self.len <= BUF_INLINE {
            &self.inline[..self.len]
        } else {
            &self.heap
        }
    }
}

impl std::ops::DerefMut for Buf {
    fn deref_mut(&mut self) -> &mut [f64] {
        if self.len <= BUF_INLINE {
            &mut self.inline[..self.len]
        } else {
            &mut self.heap
        }
    }
}

impl FromIterator<f64> for Buf {
    fn from_iter<I: IntoIterator<Item = f64>>(iter: I) -> Self {
        let mut b = Self::zeros(0);
        for v in iter {
            b.push(v);
        }
        b
    }
}

impl PartialEq for Buf {
    fn eq(&self, other: &Self) -> bool {
        **self == **other
    }
}

impl std::fmt::Debug for Buf {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_list().entries(self.iter()).finish()
    }
}

/// Square column-major matrix.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Mat {
    pub(crate) n: usize,
    pub(crate) d: Buf,
}

impl Mat {
    pub(crate) fn zeros(n: usize) -> Self {
        Self {
            n,
            d: Buf::zeros(n * n),
        }
    }

    pub(crate) fn identity(n: usize) -> Self {
        let mut m = Self::zeros(n);
        for i in 0..n {
            m.d[i + i * n] = 1.0;
        }
        m
    }

    #[inline(always)]
    pub(crate) fn at(&self, i: usize, j: usize) -> f64 {
        self.d[i + j * self.n]
    }

    #[inline(always)]
    pub(crate) fn set(&mut self, i: usize, j: usize, v: f64) {
        self.d[i + j * self.n] = v;
    }

    #[inline(always)]
    pub(crate) fn get_mut(&mut self, i: usize, j: usize) -> &mut f64 {
        &mut self.d[i + j * self.n]
    }
}

/// Result of `makeHouseholder`: `essential` (the tail of the Householder
/// vector `v = (1, essential)`), `tau`, `beta`.
pub(crate) struct Householder {
    pub(crate) essential: Buf,
    pub(crate) tau: f64,
    pub(crate) beta: f64,
}

/// Eigen `MatrixBase::makeHouseholder` for the real vector `(c0, tail)`.
/// `tail.squaredNorm()` uses the run-time order (sequential); every
/// compile-time tail translated here has at most 2 entries, for which the
/// unrolled order is identical.
pub(crate) fn make_householder(c0: f64, tail: &[f64]) -> Householder {
    let tail_sq_norm = if tail.is_empty() {
        0.0
    } else {
        seq_sum(tail.len(), |k| tail[k] * tail[k])
    };
    let tol = f64::MIN_POSITIVE;
    if tail_sq_norm <= tol {
        // the imaginary part of c0 is 0 for real scalars
        Householder {
            essential: Buf::zeros(tail.len()),
            tau: 0.0,
            beta: c0,
        }
    } else {
        let mut beta = (c0 * c0 + tail_sq_norm).sqrt();
        if c0 >= 0.0 {
            beta = -beta;
        }
        let denom = c0 - beta;
        Householder {
            essential: tail.iter().map(|t| t / denom).collect(),
            tau: (beta - c0) / beta,
            beta,
        }
    }
}

/// A rectangular block `rows r0..r0+rows`, `cols c0..c0+cols`.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Block {
    pub(crate) r0: usize,
    pub(crate) c0: usize,
    pub(crate) rows: usize,
    pub(crate) cols: usize,
}

impl Block {
    pub(crate) const fn new(r0: usize, c0: usize, rows: usize, cols: usize) -> Self {
        Self { r0, c0, rows, cols }
    }
}

/// Eigen `applyHouseholderOnTheLeft(essential, tau)` on the block `blk` of
/// `a` (`blk.rows = essential.len() + 1`).
pub(crate) fn apply_householder_left(
    a: &mut Mat,
    blk: Block,
    essential: &[f64],
    tau: f64,
    prod: Prod,
) {
    let Block {
        r0,
        c0,
        rows: m,
        cols: ncols,
    } = blk;
    debug_assert_eq!(essential.len() + 1, m);
    if m == 1 {
        let f = 1.0 - tau;
        for j in c0..c0 + ncols {
            *a.get_mut(r0, j) *= f;
        }
    } else if tau != 0.0 {
        // tmp = essential^T * bottom; tmp += row0; row0 -= tau*tmp;
        // bottom -= (tau*essential) * tmp
        let tmp: Buf = (c0..c0 + ncols)
            .map(|j| prod.sum(m - 1, |k| essential[k] * a.at(r0 + 1 + k, j)) + a.at(r0, j))
            .collect();
        let te: Buf = essential.iter().map(|e| tau * e).collect();
        for (jj, j) in (c0..c0 + ncols).enumerate() {
            *a.get_mut(r0, j) -= tau * tmp[jj];
            for (k, t) in te.iter().enumerate() {
                *a.get_mut(r0 + 1 + k, j) -= t * tmp[jj];
            }
        }
    }
}

/// Eigen `applyHouseholderOnTheRight(essential, tau)` on the block `blk`
/// of `a` (`blk.cols = essential.len() + 1`).
pub(crate) fn apply_householder_right(
    a: &mut Mat,
    blk: Block,
    essential: &[f64],
    tau: f64,
    prod: Prod,
) {
    let Block {
        r0,
        c0,
        rows: nrows,
        cols: m,
    } = blk;
    debug_assert_eq!(essential.len() + 1, m);
    if m == 1 {
        let f = 1.0 - tau;
        for i in r0..r0 + nrows {
            *a.get_mut(i, c0) *= f;
        }
    } else if tau != 0.0 {
        // tmp = right * essential; tmp += col0; col0 -= tau*tmp;
        // right -= (tau*tmp) * essential^T
        for i in r0..r0 + nrows {
            let tmp = prod.sum(m - 1, |k| a.at(i, c0 + 1 + k) * essential[k]) + a.at(i, c0);
            *a.get_mut(i, c0) -= tau * tmp;
            let tt = tau * tmp;
            for (k, e) in essential.iter().enumerate() {
                *a.get_mut(i, c0 + 1 + k) -= tt * e;
            }
        }
    }
}

/// Real Givens / Jacobi rotation `(c, s)` (Eigen `JacobiRotation<double>`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Rotation {
    pub(crate) c: f64,
    pub(crate) s: f64,
}

impl Rotation {
    /// Eigen `JacobiRotation::makeGivens(p, q)` (real case).
    pub(crate) fn make_givens(p: f64, q: f64) -> Self {
        if q == 0.0 {
            Self {
                c: if p < 0.0 { -1.0 } else { 1.0 },
                s: 0.0,
            }
        } else if p == 0.0 {
            Self {
                c: 0.0,
                s: if q < 0.0 { 1.0 } else { -1.0 },
            }
        } else if p.abs() > q.abs() {
            let t = q / p;
            let mut u = (1.0 + t * t).sqrt();
            if p < 0.0 {
                u = -u;
            }
            let c = 1.0 / u;
            Self { c, s: -t * c }
        } else {
            let t = p / q;
            let mut u = (1.0 + t * t).sqrt();
            if q < 0.0 {
                u = -u;
            }
            let s = -1.0 / u;
            Self { c: -t * s, s }
        }
    }

    /// `transpose()` / `adjoint()` of a real rotation: `(c, −s)`.
    pub(crate) fn transpose(self) -> Self {
        Self {
            c: self.c,
            s: -self.s,
        }
    }
}

/// Eigen `internal::apply_rotation_in_the_plane(x, y, j)` (scalar path,
/// `x' = c·x + s·y`, `y' = −s·x + c·y`, skipped for the identity) applied as
/// `applyOnTheLeft(p, q, j)` restricted to columns `c0..c0+ncols`: rotates
/// rows `p` and `q`.
pub(crate) fn rotate_rows(a: &mut Mat, p: usize, q: usize, c0: usize, ncols: usize, j: Rotation) {
    if j.c == 1.0 && j.s == 0.0 {
        return;
    }
    for col in c0..c0 + ncols {
        let (xi, yi) = (a.at(p, col), a.at(q, col));
        a.set(p, col, j.c * xi + j.s * yi);
        a.set(q, col, -j.s * xi + j.c * yi);
    }
}

/// `applyOnTheRight(p, q, j)` restricted to rows `r0..r0+nrows`: rotates
/// columns `p` and `q` with `j.transpose()`.
pub(crate) fn rotate_cols(a: &mut Mat, p: usize, q: usize, r0: usize, nrows: usize, j: Rotation) {
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

/// Eigen `HouseholderSequence::evalTo` (unblocked, `length ≤ 48`) of the
/// sequence stored below the sub-diagonal of `packed` (shift 1, length
/// `n − 1`): `dst = H_0 H_1 … H_{n−2}` accumulated from the identity.
/// `in_place` reproduces the aliasing branch used when the destination is
/// the packed matrix itself (same values).
pub(crate) fn householder_q(packed: &Mat, hcoeffs: &[f64], prod: Prod) -> Mat {
    let n = packed.n;
    let mut dst = Mat::identity(n);
    if n < 2 {
        return dst;
    }
    let vecs = n - 1;
    for k in (0..vecs).rev() {
        let corner = n - k - 1;
        let essential: Buf = (k + 2..n).map(|i| packed.at(i, k)).collect();
        apply_householder_left(
            &mut dst,
            Block::new(k + 1, k + 1, corner, corner),
            &essential,
            hcoeffs[k],
            prod,
        );
    }
    dst
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn givens_zeroes_second_component() {
        for (p, q) in [
            (3.0, 4.0),
            (-3.0, 4.0),
            (4.0, -3.0),
            (0.0, 2.0),
            (2.0, 0.0),
            (-1.0, 0.0),
        ] {
            let g = Rotation::make_givens(p, q);
            // G^* (p, q)^T = (r, 0): applyOnTheLeft with the adjoint
            let gt = g.transpose();
            let y = -gt.s * p + gt.c * q;
            assert!(y.abs() < 1e-15, "{p} {q}");
            assert!((g.c * g.c + g.s * g.s - 1.0).abs() < 1e-15);
        }
    }

    #[test]
    fn householder_annihilates_tail() {
        let v = [3.0, 1.0, 2.0];
        let h = make_householder(v[0], &v[1..]);
        // H v = beta e0 with H = I - tau (1, ess)(1, ess)^T
        let w = [1.0, h.essential[0], h.essential[1]];
        let wv: f64 = w.iter().zip(&v).map(|(a, b)| a * b).sum();
        for i in 0..3 {
            let hv = v[i] - h.tau * w[i] * wv;
            let e = if i == 0 { h.beta } else { 0.0 };
            assert!((hv - e).abs() < 1e-14);
        }
        let z = make_householder(5.0, &[0.0, 0.0]);
        assert_eq!((z.tau, z.beta), (0.0, 5.0));
    }

    #[test]
    fn prod_orders() {
        let t = |k: usize| [-0.0, -0.0][k];
        assert!(Prod::SeqFirst.sum(2, t).is_sign_negative());
        assert!(Prod::FromZero.sum(2, t).is_sign_positive());
        assert_eq!(Prod::for_blocks(Storage::Fixed, 3), Prod::SeqFirst);
        assert_eq!(Prod::for_blocks(Storage::Fixed, 8), Prod::FromZero);
        assert_eq!(Prod::for_blocks(Storage::Dynamic, 3), Prod::FromZero);
    }
}
