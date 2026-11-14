// SPDX-License-Identifier: LGPL-3.0-or-later
//
// --- upstream notice of e_log.c ---
//   Double-precision log(x) function.
//   Copyright (C) 2018-2025 Free Software Foundation, Inc.
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
// --- upstream notice of e_exp.c ---
//   Double-precision e^x function.
//   Copyright (C) 2018-2025 Free Software Foundation, Inc.
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
// --- upstream notice of e_pow.c ---
//   Double-precision x^y function.
//   Copyright (C) 2018-2025 Free Software Foundation, Inc.
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
// --- upstream notice of math_config.h ---
//   Configuration for double precision math routines.
//   Copyright (C) 2018-2025 Free Software Foundation, Inc.
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
// --- upstream notice of math_err.c ---
//   Double-precision math error handling.
//   Copyright (C) 2018-2025 Free Software Foundation, Inc.
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
// --- upstream notice of s_tan.c ---
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
//   You should have received a copy of the GNU Lesser General Public License
//   along with this program; if not, see <https://www.gnu.org/licenses/>.
// --- upstream notice of dla.h, utan.h ---
//   IBM Accurate Mathematical Library
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
//   You should have received a copy of the GNU Lesser General Public License
//   along with this program; if not, see <https://www.gnu.org/licenses/>.
//
// Part of akantu-rs, a port of Akantu (EPFL/LSMS). Akantu's random
// distributions (src/common/aka_random_generator.hh) call libm through the C++
// standard library. This file is a Rust translation, modified to follow the
// instruction sequence of the oracle's compiled
// /usr/lib/aarch64-linux-gnu/libm.so.6, of the GNU C Library 2.41 (Debian
// 2.41-12+deb13u4) files above, sysdeps/ieee754/dbl-64/ (header regenerated
// with `gen_libm_tables.py --libm-rs-header`). As permitted by their "version
// 2.1 of the License, or (at your option) any later version" grant, this
// translation is distributed under the GNU Lesser General Public License
// version 3 or later (see NOTICE.md and docs/DEPENDENCIES.md). The log/exp/pow
// code was contributed to glibc from the Arm Optimized Routines project.

//! Bit-exact ports of the oracle's glibc `log`, `exp`, `pow` and `tan`.
//!
//! libm results are not fixed by IEEE-754, so the distributions of
//! [`super::distributions`] must not call the host's libm (macOS libm and
//! glibc differ; glibc differs between aarch64 and x86_64). These functions
//! reproduce, operation by operation, the code GCC 14 generated for the
//! Debian trixie aarch64 glibc 2.41 (`log@@GLIBC_2.29`, `exp@@GLIBC_2.29`,
//! `pow@@GLIBC_2.29`, `tan@@GLIBC_2.17`). That build has `__FP_FAST_FMA`,
//! `TOINT_INTRINSICS` (round-half-away `frinta`/`fcvtas`) and GCC's default
//! `-ffp-contract=fast`, so many `a * b + c` of the C sources are single
//! `fmadd` instructions; every such place is written with [`f64::mul_add`]
//! (IEEE-754 `fusedMultiplyAdd`, correctly rounded on every platform) and
//! every other operation is a plain, separately rounded IEEE operation.
//! Rust never contracts on its own, so the results are platform independent.
//! The disassembly offsets are cited per block. Verification: bit equality
//! with glibc on > 10⁸ inputs per function (`tests/random_libm.rs`,
//! `oracle/probes/random/libm_vectors.cc`).
//!
//! Error handling (`errno`, floating-point exception flags) is not
//! reproduced; return values are, including signed zeros, infinities and
//! NaN-ness (NaN payloads are not significant).

use super::libm_tables::*;

#[inline]
fn asuint64(x: f64) -> u64 {
    x.to_bits()
}

#[inline]
fn asdouble(i: u64) -> f64 {
    f64::from_bits(i)
}

/// `__math_invalid(x)`: `(x - x) / (x - x)` (a quiet NaN).
#[inline]
#[allow(clippy::eq_op)] // deliberate NaN generation, as in glibc
fn math_invalid(x: f64) -> f64 {
    (x - x) / (x - x)
}

/// `__math_divzero(sign)`: `±1 / 0`.
#[inline]
fn math_divzero(sign: bool) -> f64 {
    (if sign { -1.0 } else { 1.0 }) / 0.0
}

/// `__math_oflow(sign)`: `±0x1p769 * 0x1p769` = ±inf.
#[inline]
fn math_oflow(sign: bool) -> f64 {
    const X: f64 = f64::from_bits(0x7000_0000_0000_0000); // 0x1p769
    (if sign { -X } else { X }) * X
}

/// `__math_uflow(sign)`: `±0x1p-767 * 0x1p-767` = ±0.
#[inline]
fn math_uflow(sign: bool) -> f64 {
    const X: f64 = f64::from_bits(0x1000_0000_0000_0000); // 0x1p-767
    (if sign { -X } else { X }) * X
}

/// `roundtoint` (aarch64 `frinta`: round half away from zero).
#[inline]
fn roundtoint(x: f64) -> f64 {
    x.round()
}

/// `converttoint` (aarch64 `fcvtas`, used as a 64-bit integer; |x| < 2^31 here).
#[inline]
fn converttoint(x: f64) -> u64 {
    // round-half-away, then two's complement reinterpretation (C converts
    // the signed result to uint64_t modulo 2^64).
    #[allow(clippy::cast_possible_truncation)] // |x| < 2^31 on every path
    let k = x.round() as i64;
    k.cast_unsigned()
}

const TWO_POW_52: f64 = f64::from_bits(0x4330_0000_0000_0000);
const TWO_POW_M1022: f64 = f64::from_bits(0x0010_0000_0000_0000);
const TWO_POW_1009: f64 = f64::from_bits(0x7f00_0000_0000_0000);

/* ------------------------------------------------------------------------ */
/* log                                                                       */
/* ------------------------------------------------------------------------ */

/// glibc `log` (`__log`, e_log.c; libm.so.6 `0x3be40`).
#[must_use]
pub fn log(x: f64) -> f64 {
    const OFF: u64 = 0x3fe6_0000_0000_0000;
    const LO: u64 = 0x3fee_0000_0000_0000; // asuint64(1.0 - 0x1p-4)
    const HI: u64 = 0x3ff1_0900_0000_0000; // asuint64(1.0 + 0x1.09p-4)
    let mut ix = asuint64(x);
    let top = (ix >> 48) as u32;

    if ix.wrapping_sub(LO) < HI - LO {
        // Close to 1.0 (0x3bf00..0x3bf98).
        if ix == asuint64(1.0) {
            return 0.0;
        }
        let b = &LOG_B;
        let r = x - 1.0;
        let r2 = r * r;
        let r3 = r * r2;
        // y = r3 * (B1 + r B2 + r2 B3 + r3 (B4 + r B5 + r2 B6 + r3 (B7 + r B8 + r2 B9 + r3 B10)))
        let inner3 = b[10].mul_add(r3, b[9].mul_add(r2, b[8].mul_add(r, b[7])));
        let inner2 = inner3.mul_add(r3, b[6].mul_add(r2, b[5].mul_add(r, b[4])));
        let inner1 = inner2.mul_add(r3, b[3].mul_add(r2, b[2].mul_add(r, b[1])));
        // w = r * 0x1p27; rhi = r + w - w  (both additions fused with the product)
        const P27: f64 = f64::from_bits(0x41a0_0000_0000_0000);
        let t = r.mul_add(P27, r);
        let rhi = (-r).mul_add(P27, t);
        let rlo = r - rhi;
        let rhi2 = rhi * rhi;
        // w = rhi * rhi * B0; hi = r + w; lo = r - hi + w
        let hi = rhi2.mul_add(b[0], r);
        let mut lo = rhi2.mul_add(b[0], r - hi);
        // lo += B0 * rlo * (rhi + r)
        lo = (b[0] * rlo).mul_add(rhi + r, lo);
        // y += lo; y += hi
        let y = inner1.mul_add(r3, lo);
        return hi + y;
    }
    if top.wrapping_sub(0x0010) >= 0x7ff0 - 0x0010 {
        // x < 0x1p-1022 or inf or nan (0x3bf9c..).
        if ix.wrapping_mul(2) == 0 {
            return math_divzero(true);
        }
        if ix == asuint64(f64::INFINITY) {
            return x;
        }
        if (top & 0x8000) != 0 || (top & 0x7ff0) == 0x7ff0 {
            return math_invalid(x);
        }
        // Subnormal: normalise.
        ix = asuint64(x * TWO_POW_52);
        ix = ix.wrapping_sub(52 << 52);
    }

    // Main path (0x3be74..0x3befc).
    let tmp = ix.wrapping_sub(OFF);
    let i = ((tmp >> 45) % 128) as usize;
    // allow: reinterpretation + arithmetic shift as in C; |k| <= 1100 fits i32.
    #[allow(clippy::cast_possible_wrap, clippy::cast_possible_truncation)]
    let k = ((tmp as i64) >> 52) as i32;
    let iz = ix.wrapping_sub(tmp & (0xfff << 52));
    let [invc, logc] = LOG_TAB[i];
    let z = asdouble(iz);
    let kd = f64::from(k);
    let a = &LOG_A;

    let w = LOG_LN2HI.mul_add(kd, logc); // kd * Ln2hi + logc
    let r = z.mul_add(invc, -1.0); // __builtin_fma (z, invc, -1.0)
    let hi = r + w;
    let lo = LOG_LN2LO.mul_add(kd, (w - hi) + r); // w - hi + r + kd * Ln2lo
    let r2 = r * r;
    // y = lo + r2 A0 + r r2 (A1 + r A2 + r2 (A3 + r A4)) + hi
    let p = a[4].mul_add(r, a[3]).mul_add(r2, a[2].mul_add(r, a[1]));
    let y = a[0].mul_add(r2, lo);
    let y = (r * r2).mul_add(p, y);
    y + hi
}

/* ------------------------------------------------------------------------ */
/* exp                                                                       */
/* ------------------------------------------------------------------------ */

/// `exp` special case (`specialcase` in e_exp.c; 0x38cb4..0x38d64).
fn exp_specialcase(tmp: f64, sbits: u64, ki: u64) -> f64 {
    if ki & 0x8000_0000 == 0 {
        // k > 0: the exponent of scale might have overflowed by <= 460.
        let scale = asdouble(sbits.wrapping_sub(1009 << 52));
        return tmp.mul_add(scale, scale) * TWO_POW_1009;
    }
    // k < 0: subnormal range.
    let scale = asdouble(sbits.wrapping_add(1022 << 52));
    let st = tmp * scale;
    let mut y = scale + st;
    if y < 1.0 {
        let hi = y + 1.0;
        let lo = (scale - y) + st;
        let lo = ((1.0 - hi) + y) + lo;
        y = (lo + hi) - 1.0;
        if y == 0.0 {
            return 0.0;
        }
    }
    y * TWO_POW_M1022
}

/// glibc `exp` (`__exp`, e_exp.c; libm.so.6 `0x38c00`).
#[must_use]
pub fn exp(x: f64) -> f64 {
    let mut abstop = ((asuint64(x) >> 52) & 0x7ff) as u32;
    // top12(0x1p-54) = 0x3c9, top12(512.0) = 0x408, top12(1024.0) = 0x409.
    if abstop.wrapping_sub(0x3c9) >= 0x408 - 0x3c9 {
        if abstop.wrapping_sub(0x3c9) >= 0x8000_0000 {
            // Tiny x (0x38d3c).
            return x + 1.0;
        }
        if abstop >= 0x409 {
            if asuint64(x) == asuint64(f64::NEG_INFINITY) {
                return 0.0;
            }
            if abstop >= 0x7ff {
                return x + 1.0;
            }
            return if asuint64(x) >> 63 != 0 {
                math_uflow(false)
            } else {
                math_oflow(false)
            };
        }
        // Large x is special cased below.
        abstop = 0;
    }
    // Main path (0x38c18..0x38c84).
    let z = x * EXP_INVLN2N;
    let kd = roundtoint(z);
    let ki = converttoint(z);
    let r = EXP_NEGLN2LON.mul_add(kd, EXP_NEGLN2HIN.mul_add(kd, x));
    let idx = 2 * (ki % 128) as usize;
    let top = ki << 45;
    let tail = asdouble(EXP_TAB[idx]);
    let sbits = EXP_TAB[idx + 1].wrapping_add(top);
    let r2 = r * r;
    // tmp = tail + r + r2 (C2 + r C3) + r2 r2 (C4 + r C5)
    let tmp = EXP_C3.mul_add(r, EXP_C2).mul_add(r2, r + tail);
    let tmp = (r2 * r2).mul_add(EXP_C5.mul_add(r, EXP_C4), tmp);
    if abstop == 0 {
        return exp_specialcase(tmp, sbits, ki);
    }
    let scale = asdouble(sbits);
    tmp.mul_add(scale, scale)
}

/* ------------------------------------------------------------------------ */
/* pow                                                                       */
/* ------------------------------------------------------------------------ */

/// `log_inline` of e_pow.c (0x3c03c..0x3c0e4): `log(x) = hi + lo`.
#[inline]
fn pow_log_inline(ix: u64) -> (f64, f64) {
    const OFF: u64 = 0x3fe6_9555_0000_0000;
    let tmp = ix.wrapping_sub(OFF);
    let i = ((tmp >> 45) % 128) as usize;
    // allow: reinterpretation + arithmetic shift as in C; |k| <= 1100 fits i32.
    #[allow(clippy::cast_possible_wrap, clippy::cast_possible_truncation)]
    let k = ((tmp as i64) >> 52) as i32;
    let iz = ix.wrapping_sub(tmp & (0xfff << 52));
    let z = asdouble(iz);
    let kd = f64::from(k);
    let [invc, logc, logctail] = POW_TAB[i];
    let a = &POW_A;

    let r = z.mul_add(invc, -1.0);
    let t1 = kd.mul_add(POW_LN2HI, logc);
    let t2 = t1 + r;
    let lo1 = kd.mul_add(POW_LN2LO, logctail);
    let lo2 = (t1 - t2) + r;
    let ar = r * a[0];
    let ar2 = r * ar;
    let ar3 = r * ar2;
    let hi = t2 + ar2;
    let lo3 = r.mul_add(ar, -ar2); // __builtin_fma (ar, r, -ar2)
    let lo4 = (t2 - hi) + ar2;
    // p = ar3 (A1 + r A2 + ar2 (A3 + r A4 + ar2 (A5 + r A6)))
    let q = ar2.mul_add(r.mul_add(a[6], a[5]), r.mul_add(a[4], a[3]));
    let q = ar2.mul_add(q, r.mul_add(a[2], a[1]));
    let lo = ((lo1 + lo2) + lo3) + lo4;
    let lo = ar3.mul_add(q, lo);
    let y = hi + lo;
    let tail = (hi - y) + lo;
    (y, tail)
}

/// `specialcase` of e_pow.c (0x3c270..0x3c3b0).
fn pow_specialcase(tmp: f64, sbits: u64, ki: u64) -> f64 {
    if ki & 0x8000_0000 == 0 {
        let scale = asdouble(sbits.wrapping_sub(1009 << 52));
        return tmp.mul_add(scale, scale) * TWO_POW_1009;
    }
    let sbits = sbits.wrapping_add(1022 << 52);
    let scale = asdouble(sbits);
    let st = tmp * scale;
    let mut y = scale + st;
    if y.abs() < 1.0 {
        let one = if y < 0.0 { -1.0 } else { 1.0 };
        let lo = (scale - y) + st;
        let hi = y + one;
        let lo = ((one - hi) + y) + lo;
        y = (lo + hi) - one;
        if y == 0.0 {
            y = asdouble(sbits & 0x8000_0000_0000_0000);
        }
    }
    y * TWO_POW_M1022
}

const SIGN_BIAS: u64 = 0x800 << 7;

/// `exp_inline` of e_pow.c (0x3c104..0x3c184).
#[inline]
fn pow_exp_inline(x: f64, xtail: f64, sign_bias: u64) -> f64 {
    let mut abstop = ((asuint64(x) >> 52) & 0x7ff) as u32;
    if abstop.wrapping_sub(0x3c9) >= 0x408 - 0x3c9 {
        if abstop.wrapping_sub(0x3c9) >= 0x8000_0000 {
            let one = x + 1.0;
            return if sign_bias != 0 { -one } else { one };
        }
        if abstop >= 0x409 {
            return if asuint64(x) >> 63 != 0 {
                math_uflow(sign_bias != 0)
            } else {
                math_oflow(sign_bias != 0)
            };
        }
        abstop = 0;
    }
    let z = x * EXP_INVLN2N;
    let kd = roundtoint(z);
    let ki = converttoint(z);
    let r = kd.mul_add(EXP_NEGLN2LON, kd.mul_add(EXP_NEGLN2HIN, x));
    let r = xtail + r;
    let idx = 2 * (ki % 128) as usize;
    let top = ki.wrapping_add(sign_bias) << 45;
    let tail = asdouble(EXP_TAB[idx]);
    let sbits = EXP_TAB[idx + 1].wrapping_add(top);
    let r2 = r * r;
    let tmp = r2.mul_add(r.mul_add(EXP_C3, EXP_C2), r + tail);
    let tmp = (r2 * r2).mul_add(r.mul_add(EXP_C5, EXP_C4), tmp);
    if abstop == 0 {
        return pow_specialcase(tmp, sbits, ki);
    }
    let scale = asdouble(sbits);
    tmp.mul_add(scale, scale)
}

/// `checkint`: 0 if not an integer, 1 if odd, 2 if even.
fn checkint(iy: u64) -> u32 {
    let e = ((iy >> 52) & 0x7ff) as i32;
    if e < 0x3ff {
        return 0;
    }
    if e > 0x3ff + 52 {
        return 2;
    }
    let s = (0x3ff + 52 - e) as u32;
    if iy & ((1u64 << s) - 1) != 0 {
        return 0;
    }
    if iy & (1u64 << s) != 0 {
        return 1;
    }
    2
}

/// 1 if `i` is the representation of 0, infinity or NaN.
#[inline]
fn zeroinfnan(i: u64) -> bool {
    i.wrapping_mul(2).wrapping_sub(1) >= 2 * asuint64(f64::INFINITY) - 1
}

/// `issignaling_inline`: `2 * (ix ^ 0x0008000000000000) > 2 * 0x7ff8000000000000`
/// in C `uint64_t` arithmetic, i.e. the doubling wraps modulo 2⁶⁴ (it drops
/// the sign bit), which must be explicit in Rust.
#[inline]
fn issignaling(x: f64) -> bool {
    let ix = asuint64(x);
    (ix ^ 0x0008_0000_0000_0000).wrapping_mul(2) > 2 * 0x7ff8_0000_0000_0000
}

/// glibc `pow` (`__pow`, e_pow.c; libm.so.6 `0x3c000`).
#[must_use]
pub fn pow(x: f64, y: f64) -> f64 {
    let mut sign_bias = 0u64;
    let mut ix = asuint64(x);
    let iy = asuint64(y);
    let mut topx = (ix >> 52) as u32;
    let topy = (iy >> 52) as u32;

    if topx.wrapping_sub(0x001) >= 0x7ff - 0x001
        || (topy & 0x7ff).wrapping_sub(0x3be) >= 0x43e - 0x3be
    {
        if zeroinfnan(iy) {
            if iy.wrapping_mul(2) == 0 {
                return if issignaling(x) { x + y } else { 1.0 };
            }
            if ix == asuint64(1.0) {
                return if issignaling(y) { x + y } else { 1.0 };
            }
            if ix.wrapping_mul(2) > 2 * asuint64(f64::INFINITY)
                || iy.wrapping_mul(2) > 2 * asuint64(f64::INFINITY)
            {
                return x + y;
            }
            if ix.wrapping_mul(2) == 2 * asuint64(1.0) {
                return 1.0;
            }
            if (ix.wrapping_mul(2) < 2 * asuint64(1.0)) == (iy >> 63 == 0) {
                return 0.0;
            }
            return y * y;
        }
        if zeroinfnan(ix) {
            let mut x2 = x * x;
            if ix >> 63 != 0 && checkint(iy) == 1 {
                x2 = -x2;
            }
            return if iy >> 63 != 0 { 1.0 / x2 } else { x2 };
        }
        // Here x and y are non-zero finite.
        if ix >> 63 != 0 {
            let yint = checkint(iy);
            if yint == 0 {
                return math_invalid(x);
            }
            if yint == 1 {
                sign_bias = SIGN_BIAS;
            }
            ix &= 0x7fff_ffff_ffff_ffff;
            topx &= 0x7ff;
        }
        if (topy & 0x7ff).wrapping_sub(0x3be) >= 0x43e - 0x3be {
            if ix == asuint64(1.0) {
                return 1.0;
            }
            if (topy & 0x7ff) < 0x3be {
                return if ix > asuint64(1.0) { 1.0 + y } else { 1.0 - y };
            }
            return if (ix > asuint64(1.0)) == (topy < 0x800) {
                math_oflow(false)
            } else {
                math_uflow(false)
            };
        }
        if topx == 0 {
            // Normalise subnormal x so the exponent becomes negative.
            ix = asuint64(x * TWO_POW_52);
            ix &= 0x7fff_ffff_ffff_ffff;
            ix = ix.wrapping_sub(52 << 52);
        }
    }

    let (hi, lo) = pow_log_inline(ix);
    let ehi = y * hi;
    let elo = y.mul_add(lo, y.mul_add(hi, -ehi));
    pow_exp_inline(ehi, elo, sign_bias)
}

/* ------------------------------------------------------------------------ */
/* tan                                                                       */
/* ------------------------------------------------------------------------ */

/// Shared tail of tan's reduced-argument cases VI/VII (0.787 < |x| <= 25),
/// given `a + da` = x reduced modulo pi/2 and `n` = parity of the quadrant
/// (0x40f20..0x40fb4, 0x41174..0x41218, 0x41228..0x41284).
fn tan_reduced(a: f64, da: f64, n: bool) -> f64 {
    let (ya, yya, sy) = if a < 0.0 {
        (-a, -da, -1.0)
    } else {
        (a, da, 1.0)
    };
    if ya <= TAN_GY2 {
        // (VI) |y| <= 0.0608
        let a2 = a * a;
        let mut t2 = a2.mul_add(TAN_D11, TAN_D9);
        t2 = a2.mul_add(t2, TAN_D7);
        t2 = a2.mul_add(t2, TAN_D5);
        t2 = a2.mul_add(t2, TAN_D3);
        let t2 = (a * a2).mul_add(t2, da);
        let b = a + t2;
        if !n {
            return b;
        }
        // -cot: EADD(a, t2, b, db); DIV2(1.0, 0.0, b, db, c, dc, ...)
        let db = if a.abs() > t2.abs() {
            (a - b) + t2
        } else {
            (t2 - b) + a
        };
        let c1 = 1.0 / b;
        let u = c1 * b;
        let uu = c1.mul_add(b, -u);
        let cc = (-db).mul_add(c1, ((1.0 - u) - uu) + 0.0) / b;
        let c = c1 + cc;
        let dc = (c1 - c) + cc;
        return -(dc + c);
    }
    // (VII) 0.0608 < |y| <= 0.787
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // in [0, 186)
    let i = (ya.mul_add(256.0, TAN_MFFTNHF) as i32) as usize;
    let [xi, fi, gi] = TAN_XFG[i];
    let z = (ya - xi) + yya;
    let z2 = z * z;
    let pz = (z * z2).mul_add(z2.mul_add(TAN_E1, TAN_E0), z);
    let s = (fi + gi) * pz;
    if n {
        // -cot
        let t2 = s / (pz + fi);
        (gi - t2) * -sy
    } else {
        let t2 = s / (gi - pz);
        (t2 + fi) * sy
    }
}

/// glibc `tan` (`__tan`, s_tan.c; libm.so.6 `0x40d20`) for non-finite
/// arguments and |x| <= 25.
///
/// This covers every argument the libstdc++ Cauchy distribution produces
/// (`tan(pi * u)`, u ∈ [0, 1)). The larger-argument branches of glibc
/// (`__branred` reduction for |x| > 25) are not ported: for |x| > 25 this
/// returns `None`.
#[must_use]
pub fn tan_checked(x: f64) -> Option<f64> {
    let ux = (asuint64(x) >> 32) as u32;
    if ux & 0x7ff0_0000 == 0x7ff0_0000 {
        #[allow(clippy::eq_op)] // inf/NaN -> NaN, as glibc's `x - x`
        let nan = x - x;
        return Some(nan);
    }
    let w = x.abs();
    if w <= TAN_G1 {
        // (I) |x| <= 1.259e-8
        return Some(x);
    }
    if w <= TAN_G2 {
        // (II) |x| <= 0.0608 (0x40e74..0x40ebc)
        let x2 = x * x;
        let mut t2 = x2.mul_add(TAN_D11, TAN_D9);
        t2 = x2.mul_add(t2, TAN_D7);
        t2 = x2.mul_add(t2, TAN_D5);
        t2 = x2.mul_add(t2, TAN_D3);
        return Some((x * x2).mul_add(t2, x));
    }
    if w <= TAN_G3 {
        // (III) |x| <= 0.787 (0x410f8..0x41170)
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // in [0, 186)
        let i = (w.mul_add(256.0, TAN_MFFTNHF) as i32) as usize;
        let s = if x < 0.0 { -1.0 } else { 1.0 };
        let [xi, fi, gi] = TAN_XFG[i];
        let z = w - xi;
        let z2 = z * z;
        let pz = (z * z2).mul_add(z2.mul_add(TAN_E1, TAN_E0), z);
        let t2 = ((fi + gi) * pz) / (gi - pz);
        return Some((t2 + fi) * s);
    }
    if w <= TAN_G4 {
        // 0.787 < |x| <= 25: range reduction by algorithm i (0x40ec8..0x40f1c)
        let t = x.mul_add(TAN_HPINV, TAN_TOINT);
        let xn = t - TAN_TOINT;
        let n = asuint64(t) & 1 != 0;
        let t1 = (-xn).mul_add(TAN_MP2, (-xn).mul_add(TAN_MP1, x));
        let a = (-xn).mul_add(TAN_MP3, t1);
        let da = (-xn).mul_add(TAN_MP3, t1 - a);
        return Some(tan_reduced(a, da, n));
    }
    None
}

/// glibc `tan` on the ported domain (see [`tan_checked`]). Crate-internal:
/// its only caller, the Cauchy distribution, evaluates
/// `pi * u` with `u` in `[0, 1)`, always inside the ported domain; that
/// invariant is debug-asserted. Outside callers use [`tan_checked`].
#[must_use]
pub(crate) fn tan(x: f64) -> f64 {
    tan_checked(x).unwrap_or_else(|| {
        debug_assert!(false, "tan({x}) outside the ported domain |x| <= 25");
        x.tan()
    })
}
