// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): the evaluation order of the Eigen 3.4
// reductions used by src/common/aka_types.hh and the FE kernels built on it
// (src/fe_engine/shape_lagrange_inline_impl.hh, integrator_gauss_inline_impl.hh,
// src/model/solid_mechanics/material.cc). This file contains no Eigen code:
// it evaluates the orders described in `eigen_order` without closures or
// function pointers.

//! Straight-line versions of the Eigen-order reductions of
//! [`crate::eigen_order`] for hot loops (D-008, D-036).
//!
//! The general kernels ([`crate::eigen_order::redux_tree`],
//! [`crate::eigen_gebp::product_into`]) evaluate every term through a
//! closure and every addition through a function pointer, and select the
//! product route at every call. The functions here take the terms already
//! computed in a slice (`p[l] = fl(a_l · b_l)`, the same values the general
//! kernels compute at the leaves) and perform **the same additions on the
//! same operands in the same order**:
//!
//! | Function | Order (same bits as) |
//! |---|---|
//! | [`tree_sum`] | halving tree, [`crate::eigen_order::redux_tree`] with `+` (`Reduction::Tree.sum`) |
//! | [`seq_sum`] | sequential from the first term, [`crate::eigen_order::redux_seq`] (`Reduction::Sequential.sum`) |
//! | [`zero_sum`] | sequential from `+0`, [`crate::eigen_order::sum_from_zero`] (GEMV accumulator) |
//!
//! An empty sum is `+0` in all three, as in the general kernels. Every
//! addition keeps the general kernels' operand order (`left + right` of the
//! tree, `acc + term`), so even NaN payloads propagate identically.
//!
//! [`tree_sum`] is written as six nested levels of non-recursive,
//! always-inlined functions: for a length known at compile time (an
//! associated constant of a monomorphised element class, or a literal) the
//! compiler folds every branch and emits the tree as straight-line code;
//! lengths above [`TREE_MAX`] go to the recursive general function.

use crate::eigen_order;
use crate::types::Real;

/// Largest length [`tree_sum`] evaluates without recursion (`2⁵`; Eigen's
/// complete unrolling of products stops at 27 terms).
pub const TREE_MAX: usize = 32;

#[inline(always)]
fn tree_l0(p: &[Real], s: usize) -> Real {
    p[s]
}

#[inline(always)]
fn tree_l1(p: &[Real], s: usize, n: usize) -> Real {
    if n == 1 {
        p[s]
    } else {
        let h = n / 2;
        tree_l0(p, s) + tree_l0(p, s + h)
    }
}

#[inline(always)]
fn tree_l2(p: &[Real], s: usize, n: usize) -> Real {
    if n == 1 {
        p[s]
    } else {
        let h = n / 2;
        tree_l1(p, s, h) + tree_l1(p, s + h, n - h)
    }
}

#[inline(always)]
fn tree_l3(p: &[Real], s: usize, n: usize) -> Real {
    if n == 1 {
        p[s]
    } else {
        let h = n / 2;
        tree_l2(p, s, h) + tree_l2(p, s + h, n - h)
    }
}

#[inline(always)]
fn tree_l4(p: &[Real], s: usize, n: usize) -> Real {
    if n == 1 {
        p[s]
    } else {
        let h = n / 2;
        tree_l3(p, s, h) + tree_l3(p, s + h, n - h)
    }
}

#[inline(always)]
fn tree_l5(p: &[Real], s: usize, n: usize) -> Real {
    if n == 1 {
        p[s]
    } else {
        let h = n / 2;
        tree_l4(p, s, h) + tree_l4(p, s + h, n - h)
    }
}

/// Halving-tree sum of `p[..n]`: bitwise
/// `Reduction::Tree.sum(n, &|l| p[l])` (`+0` for `n = 0`). Level `L`
/// handles `n ≤ 2^L`, and `⌈n/2⌉ ≤ 2^(L−1)` holds for its halves, so the
/// six levels cover `n ≤` [`TREE_MAX`].
///
/// # Panics
/// If `p` is shorter than `n` (index out of bounds).
#[inline(always)]
#[must_use]
pub fn tree_sum(p: &[Real], n: usize) -> Real {
    if n == 0 {
        0.0
    } else if n <= TREE_MAX {
        tree_l5(p, 0, n)
    } else {
        eigen_order::redux_tree(0, n, &|l| p[l], |a, b| a + b)
    }
}

/// Sequential sum from the first term of `p[..n]`: bitwise
/// `Reduction::Sequential.sum(n, &|l| p[l])` (`+0` for `n = 0`).
///
/// # Panics
/// If `p` is shorter than `n`.
#[inline(always)]
#[must_use]
pub fn seq_sum(p: &[Real], n: usize) -> Real {
    if n == 0 {
        return 0.0;
    }
    let mut r = p[0];
    for &x in &p[1..n] {
        r += x; // r = r + x
    }
    r
}

/// Sequential sum from `+0` of `p[..n]` (`c = 0; c = c + p[l]`): bitwise
/// [`crate::eigen_order::sum_from_zero`].
///
/// # Panics
/// If `p` is shorter than `n`.
#[inline(always)]
#[must_use]
pub fn zero_sum(p: &[Real], n: usize) -> Real {
    let mut c = 0.0;
    for &x in &p[..n] {
        c += x; // c = c + x
    }
    c
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eigen_order::{redux_seq, redux_tree, sum_from_zero};

    /// Deterministic test values. Default mode: special values (signed zeros,
    /// infinities, subnormals, extremes), arbitrary bit patterns and finite
    /// values over a wide exponent range, where every NaN drawn is a *default*
    /// quiet NaN of either sign (see [`same`]). Payload mode
    /// ([`Values::payload_at`]): finite values only, except one draw, a NaN
    /// with a payload.
    struct Values {
        state: u64,
        draws: u64,
        nan_at: Option<u64>,
    }

    /// Bits of the default quiet NaN without the sign.
    const DEFAULT_NAN: u64 = 0x7ff8_0000_0000_0000;
    /// The NaN of the payload mode (quiet, negative, payload `0xabc`).
    const PAYLOAD_NAN: u64 = 0xfff8_0000_0000_0abc;

    /// The exact rule of D-046/D-051: identical bits, or two default quiet NaNs
    /// that differ only in the sign (`+0` and `−0` differ). When two NaNs meet
    /// in one addition, which one IEEE 754 returns is unspecified: x86 SSE
    /// returns the first operand's, and the compiler may commute the operands
    /// in either code path (Rust and LLVM leave the payload of such a NaN
    /// unspecified). Default-mode inputs therefore carry default NaNs only, and
    /// payload propagation is checked in payload mode, where a single NaN
    /// enters each case and every result bit is determined.
    fn same(a: Real, b: Real) -> bool {
        let (x, y) = (a.to_bits(), b.to_bits());
        let sign = 1u64 << 63;
        x == y || (x & !sign == DEFAULT_NAN && y & !sign == DEFAULT_NAN)
    }

    impl Values {
        fn new(seed: u64) -> Self {
            Self {
                state: seed,
                draws: 0,
                nan_at: None,
            }
        }

        fn next_u64(&mut self) -> u64 {
            // xorshift64*
            let mut x = self.state;
            x ^= x >> 12;
            x ^= x << 25;
            x ^= x >> 27;
            self.state = x;
            x.wrapping_mul(0x2545_f491_4f6c_dd1d)
        }

        /// Finite value with a moderate exponent spread (cancellations, no
        /// overflow in the kernels' few operations).
        fn finite(&mut self) -> Real {
            let r = self.next_u64();
            let m = (self.next_u64() >> 11) as Real / (1u64 << 53) as Real;
            let e = ((r >> 8) % 41) as i32 - 20;
            let s = if (r >> 40) & 1 == 1 { -1.0 } else { 1.0 };
            s * m * Real::powi(2.0, e)
        }

        fn next(&mut self) -> Real {
            const SPECIAL: [u64; 14] = [
                0x0000_0000_0000_0000, // +0
                0x8000_0000_0000_0000, // -0
                0x7ff0_0000_0000_0000, // +inf
                0xfff0_0000_0000_0000, // -inf
                0x7ff8_0000_0000_0000, // default quiet NaN (aarch64)
                0xfff8_0000_0000_0000, // default quiet NaN (x86_64)
                0x0000_0000_0000_0001, // smallest subnormal
                0x800f_ffff_ffff_ffff, // largest negative subnormal
                0x0010_0000_0000_0000, // MIN_POSITIVE
                0x7fef_ffff_ffff_ffff, // MAX
                0x3ff0_0000_0000_0000, // 1
                0xbff0_0000_0000_0000, // -1
                0x3cb0_0000_0000_0000, // 2^-52
                0x4340_0000_0000_0000, // 2^53
            ];
            self.draws += 1;
            if let Some(k) = self.nan_at {
                return if self.draws == k {
                    Real::from_bits(PAYLOAD_NAN)
                } else {
                    self.finite()
                };
            }
            let r = self.next_u64();
            let x = match r % 8 {
                0 => Real::from_bits(SPECIAL[(r >> 8) as usize % SPECIAL.len()]),
                1 => Real::from_bits(self.next_u64()), // any bit pattern
                _ => self.finite(),
            };
            if x.is_nan() {
                Real::from_bits(DEFAULT_NAN | (x.to_bits() & (1 << 63)))
            } else {
                x
            }
        }

        /// Payload mode: finite draws, the `k`-th draw from now (`k ≥ 1`) is
        /// the payload NaN.
        fn payload_at(&mut self, k: u64) {
            self.nan_at = Some(self.draws + k);
        }

        /// Back to the default mode.
        fn default_mode(&mut self) {
            self.nan_at = None;
        }
    }

    #[test]
    fn unrolled_sums_match_general_kernels_bitwise() {
        let mut v = Values::new(0x9e37_79b9_7f4a_7c15);
        let mut p = [0.0; 40];
        let mut cases = 0;
        for round in 0..6_000 {
            for x in &mut p {
                *x = v.next();
            }
            for n in 0..=40 {
                let t = |l: usize| p[l];
                let tree = if n == 0 {
                    0.0
                } else {
                    redux_tree(0, n, &t, |a, b| a + b)
                };
                let seq = if n == 0 {
                    0.0
                } else {
                    redux_seq(n, &t, |a, b| a + b)
                };
                let zero = sum_from_zero(n, &t);
                let f = tree_sum(&p, n);
                assert!(same(f, tree), "tree n={n} round={round}: {f:e} vs {tree:e}");
                let f = seq_sum(&p, n);
                assert!(same(f, seq), "seq n={n} round={round}: {f:e} vs {seq:e}");
                let f = zero_sum(&p, n);
                assert!(same(f, zero), "zero n={n} round={round}: {f:e} vs {zero:e}");
                cases += 1;
            }
        }
        assert!(cases >= 100_000, "{cases}");
    }

    /// Payload mode: one payload NaN among finite terms; every sum must
    /// propagate it with the same bits as the general kernels.
    #[test]
    fn unrolled_sums_propagate_a_payload_nan_like_the_general_kernels() {
        let mut v = Values::new(0x51ed_270b_27b6_d3a9);
        let mut p = [0.0; 40];
        let mut nan_results = 0;
        for _ in 0..2_000 {
            let k = 1 + v.next_u64() % 40;
            v.payload_at(k);
            for x in &mut p {
                *x = v.next();
            }
            v.default_mode();
            for n in 1..=40 {
                let t = |l: usize| p[l];
                let pairs = [
                    (tree_sum(&p, n), redux_tree(0, n, &t, |a, b| a + b)),
                    (seq_sum(&p, n), redux_seq(n, &t, |a, b| a + b)),
                    (zero_sum(&p, n), sum_from_zero(n, &t)),
                ];
                for (f, g) in pairs {
                    assert_eq!(f.to_bits(), g.to_bits(), "n={n} k={k}");
                    nan_results += usize::from(f.is_nan());
                }
            }
        }
        assert!(nan_results > 10_000, "{nan_results}");
    }

    #[test]
    fn literal_lengths_fold() {
        // the documented shapes: 3 terms t0 + (t1 + t2), 6 terms
        // (t0 + (t1 + t2)) + (t3 + (t4 + t5))
        let p = [1.0, 1e-16, 1e-16, 1.0, 1e-16, 1e-16];
        assert_eq!(tree_sum(&p, 3), 1.0 + (1e-16 + 1e-16));
        let six = (1.0 + (1e-16 + 1e-16)) + (1.0 + (1e-16 + 1e-16));
        assert_eq!(tree_sum(&p, 6), six);
        assert_eq!(seq_sum(&[-0.0, -0.0], 2).to_bits(), (-0.0_f64).to_bits());
        assert_eq!(zero_sum(&[-0.0, -0.0], 2).to_bits(), 0.0_f64.to_bits());
    }
}
