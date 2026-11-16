// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs test support (not ported from Akantu): input values for the
// bitwise differential tests of the fixed-size Eigen-order kernels against
// the general kernels of `eigen_order` / `eigen_gebp`.

//! Test inputs and the comparison rule for bitwise kernel tests (D-046,
//! D-051).
//!
//! Public (hidden from the documentation) so that the kernel tests of other
//! crates (`akantu-fe`, `akantu-structural`) use the same values and rule.
//! Not for library use.

use crate::types::Real;

/// Bits of the default quiet NaN without the sign.
pub const DEFAULT_NAN: u64 = 0x7ff8_0000_0000_0000;
/// The NaN of the payload mode (quiet, negative, payload `0xabc`).
pub const PAYLOAD_NAN: u64 = 0xfff8_0000_0000_0abc;

/// The comparison rule of D-046/D-051: identical bits, or two default quiet
/// NaNs that differ only in the sign (`+0` and `−0` differ). When two NaNs
/// meet in one addition, which one IEEE 754 returns is unspecified (x86 SSE
/// returns the first operand's; the compiler may commute the operands of an
/// addition in either code path). Default-mode inputs therefore carry
/// default NaNs only, and payload propagation is checked in payload mode,
/// where a single NaN enters each case. For a kernel that never combines
/// that NaN with a negated copy of itself, every result bit is then
/// determined (compare those results with [`Real::to_bits`]); for one that
/// can (an iterative algorithm that negates its inputs, e.g. a Householder
/// update `a −= (τ·e)·t` where `τ` and `e` derive from the same NaN with
/// opposite signs), use [`same_payload_sign_free`].
#[must_use]
pub fn same(a: Real, b: Real) -> bool {
    let (x, y) = (a.to_bits(), b.to_bits());
    let sign = 1u64 << 63;
    x == y || (x & !sign == DEFAULT_NAN && y & !sign == DEFAULT_NAN)
}

/// The payload-mode rule for kernels that can combine the input NaN with a
/// negated copy of itself (D-046): identical bits, or two NaNs with the
/// same payload and quiet bit that differ only in the sign. When both
/// operands of an operation are NaNs carrying the same payload with
/// opposite signs, which one IEEE 754 returns is unspecified and the
/// compiler may commute a multiplication or an addition, so the sign of
/// the result is free; the payload must survive exactly.
#[must_use]
pub fn same_payload_sign_free(a: Real, b: Real) -> bool {
    let (x, y) = (a.to_bits(), b.to_bits());
    x == y || (a.is_nan() && b.is_nan() && (x ^ y) == 1u64 << 63)
}

/// Deterministic test values. Default mode: special values (signed zeros,
/// infinities, default NaNs of both signs, subnormals, extremes, values
/// that cancel), arbitrary bit patterns and finite values over a wide
/// exponent range. Payload mode ([`Values::payload_at`]): finite values,
/// except one draw, a NaN with a payload.
#[derive(Clone, Debug)]
pub struct Values {
    state: u64,
    draws: u64,
    nan_at: Option<u64>,
    last: Real,
}

impl Values {
    /// A generator with the given seed (non-zero).
    #[must_use]
    pub fn new(seed: u64) -> Self {
        Self {
            state: seed | 1,
            draws: 0,
            nan_at: None,
            last: 1.0,
        }
    }

    /// Next raw 64-bit draw (xorshift64*).
    pub fn next_u64(&mut self) -> u64 {
        let mut x = self.state;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.state = x;
        x.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }

    /// Finite value with a moderate exponent spread (cancellations, no
    /// overflow in a few operations).
    pub fn finite(&mut self) -> Real {
        let r = self.next_u64();
        let m = (self.next_u64() >> 11) as Real / (1u64 << 53) as Real;
        let e = ((r >> 8) % 41) as i32 - 20;
        let s = if (r >> 40) & 1 == 1 { -1.0 } else { 1.0 };
        self.last = s * m * Real::powi(2.0, e);
        self.last
    }

    /// Next value (see the type's documentation).
    pub fn draw(&mut self) -> Real {
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
            // exact cancellation partner of the last finite value drawn
            2 => -self.last,
            _ => self.finite(),
        };
        if x.is_nan() {
            Real::from_bits(DEFAULT_NAN | (x.to_bits() & (1 << 63)))
        } else {
            x
        }
    }

    /// Fill `out` with [`Values::draw`].
    pub fn fill(&mut self, out: &mut [Real]) {
        for x in out {
            *x = self.draw();
        }
    }

    /// Payload mode: finite draws, the `k`-th draw from now (`k ≥ 1`) is the
    /// payload NaN.
    pub fn payload_at(&mut self, k: u64) {
        self.nan_at = Some(self.draws + k);
    }

    /// Back to the default mode.
    pub fn default_mode(&mut self) {
        self.nan_at = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn payload_sign_free_rule() {
        let p = Real::from_bits(PAYLOAD_NAN);
        let q = Real::from_bits(PAYLOAD_NAN & !(1 << 63));
        assert!(same_payload_sign_free(p, q));
        assert!(same_payload_sign_free(1.5, 1.5));
        // another payload, a default NaN, NaN against a number, ±0
        assert!(!same_payload_sign_free(p, Real::from_bits(PAYLOAD_NAN + 1)));
        assert!(!same_payload_sign_free(p, Real::from_bits(DEFAULT_NAN)));
        assert!(!same_payload_sign_free(p, 1.0));
        assert!(!same_payload_sign_free(0.0, -0.0));
        assert!(!same_payload_sign_free(1.0, -1.0));
    }
}
