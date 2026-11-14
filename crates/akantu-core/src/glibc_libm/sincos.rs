// SPDX-License-Identifier: LGPL-3.0-or-later
//
// --- upstream notice of s_sin.c ---
//   IBM Accurate Mathematical Library
//   written by International Business Machines Corp.
//   Copyright (C) 2001-2025 Free Software Foundation, Inc.
//
//   This program is free software; you can redistribute it and/or modify
//   it under the terms of the GNU Lesser General Public License as published by
//   the Free Software Foundation; either version 2.1 of the License, or
//   (at your option) any later version.
//
//   This program is distributed in the hope that it will be useful,
//   but WITHOUT ANY WARRANTY; without even the implied warranty of
//   MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
//   GNU Lesser General Public License for more details.
//
//   You should have received a copy of the GNU  Lesser General Public License
//   along with this program; if not, see <https://www.gnu.org/licenses/>.
// --- upstream notice of usncs.h ---
//   IBM Accurate Mathematical Library
//   Copyright (C) 2001-2025 Free Software Foundation, Inc.
//   (same LGPL-2.1-or-later grant as above)
// --- upstream notice of s_sincos.c ---
//   Compute sine and cosine of argument.
//   Copyright (C) 1997-2025 Free Software Foundation, Inc.
//   This file is part of the GNU C Library.
//
//   The GNU C Library is free software; you can redistribute it and/or
//   modify it under the terms of the GNU Lesser General Public
//   License as published by the Free Software Foundation; either
//   version 2.1 of the License, or (at your option) any later version.
//
//   The GNU C Library is distributed in the hope that it will be useful,
//   but WITHOUT ANY WARRANTY; without even the implied warranty of
//   MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the GNU
//   Lesser General Public License for more details.
//
//   You should have received a copy of the GNU Lesser General Public
//   License along with the GNU C Library; if not, see
//   <https://www.gnu.org/licenses/>.
//
// Part of akantu-rs, a port of Akantu (EPFL/LSMS). Rust translation of glibc
// 2.41 (Debian 2.41-12+deb13u4) sysdeps/ieee754/dbl-64/s_sin.c, s_sincos.c
// and usncs.h, modified to follow the instruction sequence of the oracle's
// compiled /usr/lib/aarch64-linux-gnu/libm.so.6 (`sin@@GLIBC_2.17` 0x3f760,
// `cos@@GLIBC_2.17` 0x3fee0, `sincos@@GLIBC_2.17` 0x40640). Distributed under
// LGPL-3.0-or-later via the "or (at your option) any later version" grant
// (NOTICE.md, docs/DEPENDENCIES.md).

//! glibc `sin`, `cos` and `sincos` for `|x| < 105414350` (plus infinities
//! and NaN).
//!
//! Every `a * b + c` that GCC contracted into an `fmadd`/`fmsub`/`fnmsub`
//! in the oracle's binary is an [`f64::mul_add`] here (offsets cited per
//! block); every other operation is a separately rounded IEEE operation in
//! the source's order. The round-to-nearest mode that glibc forces
//! (`SET_RESTORE_ROUND_53BIT`) is Rust's only mode. The `__branred`
//! reduction of `|x| ≥ 105414350` is not ported: the `*_checked` functions
//! return `None` there.

use super::sincos_table::SINCOSTAB;

// usncs.h (values verified in the oracle's .rodata, see the module docs of
// `crate::glibc_libm`)
const S1: f64 = f64::from_bits(0xbfc5_5555_5555_5555); // -0x1.5555555555555p-3
const S2: f64 = f64::from_bits(0x3f81_1111_1111_0ece); // 0x1.1111111110ECEp-7
const S3: f64 = f64::from_bits(0xbf2a_01a0_19db_08b8); // -0x1.A01A019DB08B8p-13
const S4: f64 = f64::from_bits(0x3ec7_1de2_7b9a_7ed9); // 0x1.71DE27B9A7ED9p-19
const S5: f64 = f64::from_bits(0xbe5a_ddff_c2fc_df59); // -0x1.ADDFFC2FCDF59p-26
const BIG: f64 = f64::from_bits(0x42c8_0000_0000_0000); // 0x1.8p45
const HP0: f64 = f64::from_bits(0x3ff9_21fb_5444_2d18); // 0x1.921FB54442D18p0
const HP1: f64 = f64::from_bits(0x3c91_a626_3314_5c07); // 0x1.1A62633145C07p-54
const MP1: f64 = f64::from_bits(0x3ff9_21fb_5800_0000); // 0x1.921FB58000000p0
const MP2: f64 = f64::from_bits(0xbe4d_de97_3c00_0000); // -0x1.DDE973C000000p-27
const PP3: f64 = f64::from_bits(0xbc8c_b3b3_9800_0000); // -0x1.CB3B398000000p-55
const PP4: f64 = f64::from_bits(0xbacd_747f_23e3_2ed7); // -0x1.d747f23e32ed7p-83
const HPINV: f64 = f64::from_bits(0x3fe4_5f30_6dc9_c883); // 0x1.45F306DC9C883p-1
const TOINT: f64 = f64::from_bits(0x4338_0000_0000_0000); // 0x1.8p52
// s_sin.c (decimal literals; the oracle's values at .rodata 0x5ce..)
const SN3: f64 = f64::from_bits(0xbfc5_5555_5555_5515); // -1.66666666666664880952546298448555E-01
const SN5: f64 = f64::from_bits(0x3f81_1110_e829_872f); // 8.33333214285722277379541354343671E-03
const CS2: f64 = 0.5; // 4.99999999999999999999950396842453E-01 rounds to 0.5
const CS4: f64 = f64::from_bits(0xbfa5_5555_5555_5535); // -4.16666666666664434524222570944589E-02
const CS6: f64 = f64::from_bits(0x3f56_c16b_edd9_e239); // 1.38888874007937613028114285595617E-03
/// `do_sin` switches to the Taylor series below this magnitude.
const TAYLOR_LIMIT: f64 = 0.126;

/// The low 32-bit word of `x` (`u.i[LOW_HALF]` on little-endian).
#[inline]
fn low_word(x: f64) -> u32 {
    #[allow(clippy::cast_possible_truncation)] // the low word, as glibc reads it
    let w = x.to_bits() as u32;
    w
}

/// `|x|`'s high word without sign (`0x7fffffff & u.i[HIGH_HALF]`).
#[inline]
fn high_word_abs(x: f64) -> u32 {
    #[allow(clippy::cast_possible_truncation)] // the high word
    let w = (x.to_bits() >> 32) as u32;
    w & 0x7fff_ffff
}

/// `SINCOS_TABLE_LOOKUP(u, sn, ssn, cs, ccs)`: `k = u.i[LOW_HALF] << 2`.
#[inline]
fn table(u: f64) -> (f64, f64, f64, f64) {
    let k = (low_word(u) << 2) as usize;
    (
        f64::from_bits(SINCOSTAB[k]),
        f64::from_bits(SINCOSTAB[k + 1]),
        f64::from_bits(SINCOSTAB[k + 2]),
        f64::from_bits(SINCOSTAB[k + 3]),
    )
}

/// `TAYLOR_SIN(x * x, x, dx)` (sin 0x3fb04 with `dx = 0`, 0x3fd58 with
/// `dx`): `POLYNOMIAL(xx)` as nested `fmadd`s, `fnmsub` for
/// `POLYNOMIAL(xx)·x − 0.5·dx`, `fmadd` for `(…)·xx + dx`, then `x + t`.
#[inline]
fn taylor_sin(x: f64, dx: f64) -> f64 {
    let xx = x * x;
    let poly = xx.mul_add(xx.mul_add(xx.mul_add(xx.mul_add(S5, S4), S3), S2), S1);
    let h = dx * 0.5;
    let t1 = poly.mul_add(x, -h);
    let t = xx.mul_add(t1, dx);
    x + t
}

/// `do_sin(x, dx)` (sin 0x3f810 / 0x3fa4c, sincos 0x406b8).
#[inline]
fn do_sin(x: f64, dx: f64) -> f64 {
    let xold = x;
    if x.abs() < TAYLOR_LIMIT {
        return taylor_sin(x, dx);
    }
    let dx = if x <= 0. { -dx } else { dx };
    let u = BIG + x.abs();
    let x = x.abs() - (u - BIG);
    let xx = x * x;
    let t_cs = xx.mul_add(CS6, CS4);
    let t_sn = xx.mul_add(SN5, SN3);
    let xxx = x * xx;
    let c0 = xx.mul_add(t_cs, CS2);
    let s = x + xxx.mul_add(t_sn, dx);
    let c = x.mul_add(dx, xx * c0);
    let (sn, ssn, cs, ccs) = table(u);
    let cor = s.mul_add(cs, (-c).mul_add(sn, s.mul_add(ccs, ssn)));
    (sn + cor).copysign(xold)
}

/// `do_cos(x, dx)` (sin 0x3f8fc, cos 0x3ff70, sincos 0x40780).
#[inline]
fn do_cos(x: f64, dx: f64) -> f64 {
    let dx = if x < 0. { -dx } else { dx };
    let u = BIG + x.abs();
    let x = (x.abs() - (u - BIG)) + dx;
    let xx = x * x;
    let t_cs = xx.mul_add(CS6, CS4);
    let t_sn = xx.mul_add(SN5, SN3);
    let xxx = x * xx;
    let c0 = xx.mul_add(t_cs, CS2);
    let s = xxx.mul_add(t_sn, x);
    let c = xx * c0;
    let (sn, ssn, cs, ccs) = table(u);
    let cor = (-s).mul_add(sn, (-c).mul_add(cs, (-s).mul_add(ssn, ccs)));
    cs + cor
}

/// `reduce_sincos` (sin 0x3f9c8, cos 0x40138, sincos 0x408a4): every
/// `y − xn·c` is a single `fmsub`.
#[inline]
fn reduce_sincos(x: f64) -> (u32, f64, f64) {
    let t = x.mul_add(HPINV, TOINT);
    let xn = t - TOINT;
    let n = low_word(t) & 3;
    let y = (-xn).mul_add(MP2, (-xn).mul_add(MP1, x));
    let t2 = (-xn).mul_add(PP3, y);
    let b = (-xn).mul_add(PP4, t2);
    let db = (-xn).mul_add(PP3, y - t2);
    let db2 = (-xn).mul_add(PP4, t2 - b);
    (n, b, db + db2)
}

/// `do_sincos(a, da, n)`.
#[inline]
fn do_sincos(a: f64, da: f64, n: u32) -> f64 {
    let r = if n & 1 != 0 {
        do_cos(a, da)
    } else {
        do_sin(a, da)
    };
    if n & 2 != 0 { -r } else { r }
}

/// glibc's `x / x` for ±∞ and NaN arguments (a quiet NaN).
#[inline]
#[allow(clippy::eq_op)] // deliberate NaN generation, as in glibc
fn nan_of(x: f64) -> f64 {
    x / x
}

/// `|x| < 105414350` bound of `reduce_sincos` (`k < 0x419921FB`).
const REDUCE_LIMIT: u32 = 0x4199_21fb;

/// glibc `sin(x)` (`__sin`); `None` for `105414350 ≤ |x| < ∞` (the
/// unported `__branred` range).
#[must_use]
pub fn sin_checked(x: f64) -> Option<f64> {
    let k = high_word_abs(x);
    Some(if k < 0x3e50_0000 {
        x
    } else if k < 0x3feb_6000 {
        do_sin(x, 0.)
    } else if k < 0x4003_68fd {
        let t = HP0 - x.abs();
        do_cos(t, HP1).copysign(x)
    } else if k < REDUCE_LIMIT {
        let (n, a, da) = reduce_sincos(x);
        do_sincos(a, da, n)
    } else if k < 0x7ff0_0000 {
        return None;
    } else {
        nan_of(x)
    })
}

/// glibc `cos(x)` (`__cos`); `None` for `105414350 ≤ |x| < ∞`.
#[must_use]
pub fn cos_checked(x: f64) -> Option<f64> {
    let k = high_word_abs(x);
    Some(if k < 0x3e40_0000 {
        1.0
    } else if k < 0x3feb_6000 {
        do_cos(x, 0.)
    } else if k < 0x4003_68fd {
        let y = HP0 - x.abs();
        let a = y + HP1;
        let da = (y - a) + HP1;
        do_sin(a, da)
    } else if k < REDUCE_LIMIT {
        let (n, a, da) = reduce_sincos(x);
        do_sincos(a, da, n + 1)
    } else if k < 0x7ff0_0000 {
        return None;
    } else {
        nan_of(x)
    })
}

/// glibc `sincos(x, &s, &c)` (`__sincos`), returned as `(s, c)`; `None`
/// for `105414350 ≤ |x| < ∞`. GCC turns a `sin(x)`/`cos(x)` pair of the
/// same argument into this call (Akantu's
/// `MaterialDruckerPrager::updateInternalParameters` does); for
/// `0.855469 ≤ |x| < 2.426265` its sine is **not** always `sin(x)`
/// (`do_cos(a, da)` on the reduced pair instead of `do_cos(hp0 − |x|,
/// hp1)`).
#[must_use]
pub fn sincos_checked(x: f64) -> Option<(f64, f64)> {
    let k = high_word_abs(x);
    if k < 0x4003_68fd {
        if k < 0x3e40_0000 {
            return Some((x, 1.0));
        }
        if k < 0x3feb_6000 {
            return Some((do_sin(x, 0.), do_cos(x, 0.)));
        }
        let y = HP0 - x.abs();
        let a = y + HP1;
        let da = (y - a) + HP1;
        return Some((do_cos(a, da).copysign(x), do_sin(a, da)));
    }
    if k < 0x7ff0_0000 {
        if k >= REDUCE_LIMIT {
            return None;
        }
        let (n, mut a, mut da) = reduce_sincos(x);
        if n == 1 || n == 2 {
            a = -a;
            da = -da;
        }
        let s = do_sin(a, da);
        let xx = do_cos(a, da);
        let c = if n & 2 != 0 { -xx } else { xx };
        return Some(if n & 1 != 0 { (c, s) } else { (s, c) });
    }
    let nan = nan_of(x);
    Some((nan, nan))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[allow(clippy::excessive_precision, clippy::approx_constant)] // the literals of s_sin.c / usncs.h
    fn constants_match_their_source_literals() {
        assert_eq!(PP4, -1.903_488_962_019_326_6e-25);
        assert_eq!(MP2, -1.390_906_756_437_715_3e-8);
        assert_eq!(HPINV, 0.636_619_772_367_581_4);
        assert_eq!(SN3, -1.666_666_666_666_648_809_525_462_984_485_55E-01);
        assert_eq!(SN5, 8.333_332_142_857_222_773_795_413_543_436_71E-03);
        assert_eq!(CS4, -4.166_666_666_666_644_345_242_225_709_445_89E-02);
        assert_eq!(CS6, 1.388_888_740_079_376_130_281_142_855_956_17E-03);
        let cs2 = 4.999_999_999_999_999_999_999_503_968_424_53E-01_f64;
        assert_eq!(CS2, cs2);
    }

    #[test]
    fn special_values() {
        assert!(sin_checked(f64::NAN).unwrap().is_nan());
        assert!(cos_checked(f64::INFINITY).unwrap().is_nan());
        assert!(sin_checked(1e9).is_none());
        assert_eq!(sin_checked(-0.0).unwrap().to_bits(), (-0.0_f64).to_bits());
        assert_eq!(cos_checked(0.0), Some(1.0));
        let (s, c) = sincos_checked(1e-30).unwrap();
        assert_eq!((s, c), (1e-30, 1.0));
    }
}
