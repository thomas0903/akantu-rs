// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/geometry/mesh_geom_common.hh (the
// exact number type `CGAL::Quotient<CGAL::MP_Float>` of `cgal::Spherical`).
// Derived from CGAL 5.6.2, LGPL-3.0-or-later (CGAL's Number_types package):
//   Number_types/include/CGAL/MP_Float.h
//     Copyright (c) 2001-2007  INRIA Sophia-Antipolis (France).
//     Author(s): Sylvain Pion
//   Number_types/include/CGAL/MP_Float_impl.h
//     Copyright (c) 2001-2006  INRIA Sophia-Antipolis (France).
//     Author(s): Sylvain Pion
// No GPL-licensed CGAL code is used in this file (D-042 (3), D-022).

//! `MP_Float`: an exact multiprecision floating-point integer in base
//! `2^16` with **signed** 16-bit limbs (CGAL 5.6.2).
//!
//! A value is `Σ v[i] · 2^(16·(exp + i))`, each limb in `[-2^15, 2^15)`.
//! After `canonicalize` (no zero leading or trailing limb) the
//! representation of a value is unique (signed digits form a complete
//! residue system modulo `2^16`), and addition, subtraction and
//! multiplication are exact, so every result is canonical. The conversion
//! to `double` ([`MpFloat::to_double_exp`]) follows CGAL's limb-by-limb
//! accumulation exactly, which is what makes `to_double` of
//! `Quotient<MP_Float>` reproducible bit for bit.

use std::cmp::Ordering;

use akantu_core::{Error, Result};

/// `log_limb` (bits per limb).
const LOG_LIMB: i64 = 16;
/// `base = 1 << log_limb`.
const BASE: i32 = 1 << 16;
/// `limbs_per_double = 2 + 53 / log_limb`.
const LIMBS_PER_DOUBLE: usize = 2 + 53 / 16;
/// CGAL asserts `|exp| < 2^30 · 2^23` after a multiplication.
const MAX_EXP: i64 = 1 << 53;

/// CGAL `MP_Float` (exact; see the module documentation).
#[derive(Clone, Debug, Default)]
pub struct MpFloat {
    /// Limbs, least significant first (`std::vector<short>`).
    v: Vec<i16>,
    /// Exponent in limbs (CGAL's `exponent_type` is a `double` holding an
    /// integer; an `i64` represents every value CGAL accepts exactly).
    exp: i64,
}

impl PartialEq for MpFloat {
    /// CGAL: `(a.v == b.v) && (a.v.empty() || a.exp == b.exp)`.
    fn eq(&self, other: &Self) -> bool {
        self.v == other.v && (self.v.is_empty() || self.exp == other.exp)
    }
}

impl Eq for MpFloat {}

impl PartialOrd for MpFloat {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for MpFloat {
    fn cmp(&self, other: &Self) -> Ordering {
        compare(self, other)
    }
}

/// CGAL `MP_Float::split`: `low = (short)(l & 0xffff)`,
/// `high = (short)((l - low) >> 16)`.
#[inline]
fn split(l: i32) -> (i16, i16) {
    // Truncation to 16 bits is the documented two's-complement conversion
    // of the C++ code (modular in GCC).
    #[allow(clippy::cast_possible_truncation)] // intended 16-bit wrap
    let low = (l & 0x0000_ffff) as i16;
    #[allow(clippy::cast_possible_truncation)]
    // (l - low) is a multiple of 2^16 whose quotient fits
    let high = ((l - i32::from(low)) >> 16) as i16;
    (high, low)
}

/// CGAL `INTERN_MP_FLOAT::compare`.
fn compare(a: &MpFloat, b: &MpFloat) -> Ordering {
    if a.is_zero() {
        return b.sign().reverse();
    }
    if b.is_zero() {
        return a.sign();
    }
    let hi = a.max_exp().max(b.max_exp()) - 1;
    let lo = a.min_exp().min(b.min_exp());
    let mut i = hi;
    while i >= lo {
        let (x, y) = (a.of_exp(i), b.of_exp(i));
        if x > y {
            return Ordering::Greater;
        }
        if x < y {
            return Ordering::Less;
        }
        i -= 1;
    }
    Ordering::Equal
}

/// CGAL `my_nearbyint` (round half to even) for `|d| < 2^31`.
fn my_nearbyint(d: f64) -> i32 {
    // `int(d)` truncates towards zero; callers guarantee the range.
    #[allow(clippy::cast_possible_truncation)] // |d| < 32770 here
    let mut z = d as i32;
    let frac = d - f64::from(z);
    if frac > 0.5 {
        z += 1;
    } else if frac < -0.5 {
        z -= 1;
    } else if frac == 0.5 && (z & 1) != 0 {
        z += 1;
    } else if frac == -0.5 && (z & 1) != 0 {
        z -= 1;
    }
    z
}

/// `std::ldexp(1.0, e)` (exact power of two, correctly rounded at the
/// ends of the range: `0` below `2^-1074`, `+inf` above `2^1023`).
#[must_use]
pub fn ldexp_one(e: i32) -> f64 {
    if e > 1023 {
        f64::INFINITY
    } else if e >= -1022 {
        // Biased exponent in [1, 2046].
        let biased = u64::try_from(e + 1023).unwrap_or(0);
        f64::from_bits(biased << 52)
    } else if e >= -1074 {
        // Subnormal 2^e = 2^-1074 · 2^(e + 1074).
        let shift = u64::try_from(e + 1074).unwrap_or(0);
        f64::from_bits(1u64 << shift)
    } else {
        // 2^-1075 is a tie between 0 and the smallest subnormal: rounds to
        // even (0); smaller values round to 0 as well.
        0.0
    }
}

impl MpFloat {
    /// Zero (`MP_Float()`).
    #[must_use]
    pub fn zero() -> Self {
        Self::default()
    }

    /// CGAL `MP_Float(int)` (`limb2` constructor).
    #[must_use]
    pub fn from_i32(i: i32) -> Self {
        let (high, low) = split(i);
        let mut r = Self {
            v: vec![low, high],
            exp: 0,
        };
        r.canonicalize();
        r
    }

    /// CGAL `MP_Float(double)` (`construct_from_builtin_fp_type`), exact.
    ///
    /// # Errors
    /// Non-finite input (a CGAL precondition), and the inputs for which the
    /// C++ conversion overflows a 16-bit limb: the double
    /// `trunc_max = 65536·32767/65535 = 32767.49999237049…` and its
    /// `2^(16k)` multiples (after scaling, `d` equals `trunc_max`, the
    /// leading limb rounds to 32768, which CGAL stores as −32768: a wrong
    /// value, then undefined behaviour downstream).
    ///
    /// As in CGAL, very large inputs convert exactly but do not convert
    /// back: [`to_double`](Self::to_double) of `MP_Float(d)` is `±inf` for
    /// `|d| ≳ 32767.5·2^992`, because CGAL's `ldexp(1, 16·max_exp)`
    /// overflows (reproduced, not corrected).
    pub fn try_from_f64(d: f64) -> Result<Self> {
        let mut r = Self::zero();
        if d == 0.0 {
            return Ok(r);
        }
        if !d.is_finite() {
            return Err(Error::InvalidArgument(format!(
                "MP_Float: non-finite input {d}"
            )));
        }
        let base = f64::from(BASE);
        // trunc_max = double(base)*(base/2-1)/double(base-1),
        // trunc_min = double(-base)*(base/2)/double(base-1).
        let trunc_max = base * f64::from(BASE / 2 - 1) / f64::from(BASE - 1);
        let trunc_min = f64::from(-BASE) * f64::from(BASE / 2) / f64::from(BASE - 1);
        let mut d = d;
        while d < trunc_min || d > trunc_max {
            r.exp += 1;
            d /= base;
        }
        while d >= trunc_min / base && d <= trunc_max / base {
            r.exp -= 1;
            d *= base;
        }
        let mut orig = d;
        let mut sum = 0.0;
        let half = f64::from(BASE / 2 - 1) / f64::from(BASE - 1);
        loop {
            let mut rr = my_nearbyint(d);
            if d - f64::from(rr) >= half {
                rr += 1;
            }
            let limb = i16::try_from(rr).map_err(|_| {
                Error::Unsupported(format!(
                    "MP_Float(double): limb overflow converting {d} (CGAL would wrap)"
                ))
            })?;
            r.v.push(limb);
            sum += f64::from(limb);
            d = orig - sum;
            if d == 0.0 {
                break;
            }
            sum *= base;
            orig *= base;
            d *= base;
            r.exp -= 1;
        }
        r.v.reverse();
        Ok(r)
    }

    fn remove_leading_zeros(&mut self) {
        while self.v.last() == Some(&0) {
            self.v.pop();
        }
    }

    fn remove_trailing_zeros(&mut self) {
        if self.v.first().is_none_or(|&l| l != 0) {
            return;
        }
        let n = self.v.iter().take_while(|&&l| l == 0).count();
        self.exp += i64::try_from(n).unwrap_or(i64::MAX);
        self.v.drain(..n);
    }

    fn canonicalize(&mut self) {
        self.remove_leading_zeros();
        self.remove_trailing_zeros();
    }

    /// `true` for zero.
    #[must_use]
    pub fn is_zero(&self) -> bool {
        self.v.is_empty()
    }

    /// Sign as an ordering against zero.
    #[must_use]
    pub fn sign(&self) -> Ordering {
        match self.v.last() {
            None => Ordering::Equal,
            Some(&l) if l > 0 => Ordering::Greater,
            Some(_) => Ordering::Less,
        }
    }

    /// Exponent (in limbs) of the least significant limb.
    #[must_use]
    pub fn min_exp(&self) -> i64 {
        self.exp
    }

    /// Exponent (in limbs) one past the most significant limb.
    #[must_use]
    pub fn max_exp(&self) -> i64 {
        i64::try_from(self.v.len()).unwrap_or(i64::MAX) + self.exp
    }

    /// Limb of weight `2^(16 i)` (zero outside the stored range).
    #[must_use]
    pub fn of_exp(&self, i: i64) -> i16 {
        if i < self.exp || i >= self.max_exp() {
            return 0;
        }
        usize::try_from(i - self.exp).map_or(0, |k| self.v[k])
    }

    /// Number of limbs.
    #[must_use]
    pub fn size(&self) -> usize {
        self.v.len()
    }

    /// Limbs, least significant first.
    #[must_use]
    pub fn limbs(&self) -> &[i16] {
        &self.v
    }

    /// CGAL `simplify_quotient` step on a numerator: shift the exponent.
    pub(crate) fn shift_exp(&mut self, delta: i64) {
        self.exp -= delta;
    }

    /// CGAL `Add_Sub` (`b` non-zero).
    fn add_sub(a: &Self, b: &Self, sub: bool) -> Self {
        debug_assert!(!b.is_zero());
        let (min_exp, max_exp) = if a.is_zero() {
            (b.min_exp(), b.max_exp())
        } else {
            (a.min_exp().min(b.min_exp()), a.max_exp().max(b.max_exp()))
        };
        let n = usize::try_from(max_exp - min_exp).unwrap_or(0);
        let mut r = Self {
            v: vec![0; n + 1],
            exp: min_exp,
        };
        for (i, ie) in (min_exp..max_exp).enumerate() {
            let (x, y) = (i32::from(a.of_exp(ie)), i32::from(b.of_exp(ie)));
            let op = if sub { x - y } else { x + y };
            let tmp = i32::from(r.v[i]) + op;
            let (high, low) = split(tmp);
            r.v[i + 1] = high;
            r.v[i] = low;
        }
        r.canonicalize();
        r
    }

    /// `a + b` (exact).
    #[must_use]
    pub fn add(&self, b: &Self) -> Self {
        if self.is_zero() {
            return b.clone();
        }
        if b.is_zero() {
            return self.clone();
        }
        Self::add_sub(self, b, false)
    }

    /// `a - b` (exact).
    #[must_use]
    pub fn sub(&self, b: &Self) -> Self {
        if b.is_zero() {
            return self.clone();
        }
        Self::add_sub(self, b, true)
    }

    /// `-a` (CGAL: `MP_Float() - a`).
    #[must_use]
    pub fn neg(&self) -> Self {
        Self::zero().sub(self)
    }

    /// `a * b` (exact).
    ///
    /// # Errors
    /// Exponent overflow (a CGAL assertion).
    pub fn mul(&self, b: &Self) -> Result<Self> {
        if self.is_zero() || b.is_zero() {
            return Ok(Self::zero());
        }
        let exp = self.exp + b.exp;
        if exp.abs() >= MAX_EXP {
            return Err(Error::Numerical(
                "Exponent overflow in MP_Float multiplication".into(),
            ));
        }
        let mut r = Self {
            v: vec![0; self.v.len() + b.v.len()],
            exp,
        };
        for (i, &ai) in self.v.iter().enumerate() {
            let mut carry: i16 = 0;
            for (j, &bj) in b.v.iter().enumerate() {
                let tmp = i32::from(carry) + i32::from(r.v[i + j]) + i32::from(ai) * i32::from(bj);
                let (high, low) = split(tmp);
                carry = high;
                r.v[i + j] = low;
            }
            r.v[i + b.v.len()] = carry;
        }
        r.canonicalize();
        Ok(r)
    }

    /// CGAL `to_double_exp`: `(d, e)` with value ≈ `d · 2^e`, `d`
    /// accumulated from the (at most 5) most significant limbs, highest
    /// first: `d += 2^(-16 k) · limb`.
    ///
    /// # Errors
    /// Exponent overflow (a CGAL assertion: `|16 · max_exp| < 2^31`).
    pub fn to_double_exp(&self) -> Result<(f64, i32)> {
        if self.is_zero() {
            return Ok((0.0, 0));
        }
        let exp = self.max_exp();
        let steps = LIMBS_PER_DOUBLE.min(self.v.len());
        let d_exp_1 = ldexp_one(-16);
        let mut d_exp = 1.0;
        let mut d = 0.0;
        let mut i = exp - 1;
        let stop = exp - 1 - i64::try_from(steps).unwrap_or(0);
        while i > stop {
            d_exp *= d_exp_1;
            d += d_exp * f64::from(self.of_exp(i));
            i -= 1;
        }
        let e = exp
            .checked_mul(LOG_LIMB)
            .and_then(|e| i32::try_from(e).ok())
            .ok_or_else(|| Error::Numerical("Exponent overflow in MP_Float to_double".into()))?;
        Ok((d, e))
    }

    /// CGAL `INTERN_MP_FLOAT::to_double(MP_Float)`: `d · ldexp(1, e)`.
    ///
    /// # Errors
    /// See [`MpFloat::to_double_exp`].
    pub fn to_double(&self) -> Result<f64> {
        let (d, e) = self.to_double_exp()?;
        Ok(d * ldexp_one(e))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mp(d: f64) -> MpFloat {
        MpFloat::try_from_f64(d).unwrap()
    }

    #[test]
    fn construction_is_exact_and_canonical() {
        for &d in &[
            1.0,
            -1.0,
            0.5,
            0.1,
            -0.3,
            1e300,
            -1e-300,
            65536.0,
            32767.0,
            32768.0,
            -32768.0,
            -32769.0,
            123_456_789.123,
            f64::MIN_POSITIVE,
            5e-324,
        ] {
            let m = mp(d);
            assert_eq!(m.to_double().unwrap(), d, "{d}");
            assert_ne!(*m.v.last().unwrap(), 0);
            assert_ne!(m.v[0], 0);
        }
        assert!(mp(0.0).is_zero());
    }

    #[test]
    fn integer_constructor_matches_double_constructor() {
        for i in [
            -70000,
            -32769,
            -32768,
            -1,
            1,
            2,
            32767,
            32768,
            65535,
            65536,
            1 << 30,
        ] {
            assert_eq!(MpFloat::from_i32(i), mp(f64::from(i)), "{i}");
        }
        assert!(MpFloat::from_i32(0).is_zero());
    }

    #[test]
    fn arithmetic_is_exact() {
        let a = mp(0.1);
        let b = mp(0.3);
        let s = a.add(&b);
        // 0.1 + 0.3 exactly (not the rounded double sum).
        assert_eq!(s.sub(&b), a);
        assert_eq!(s.sub(&a), b);
        let p = a.mul(&b).unwrap();
        assert_eq!(p.sign(), Ordering::Greater);
        assert_eq!(a.neg().add(&a), MpFloat::zero());
        assert_eq!(mp(-32768.0).neg(), mp(32768.0));
        assert!(mp(3.0) > mp(2.5));
        assert!(mp(-3.0) < mp(-2.5));
        assert_eq!(mp(6.0), mp(2.0).mul(&mp(3.0)).unwrap());
        assert_eq!(mp(1.5).mul(&mp(-1.5)).unwrap(), mp(-2.25));
    }

    #[test]
    fn ldexp_one_is_exact() {
        assert_eq!(ldexp_one(0), 1.0);
        assert_eq!(ldexp_one(-16), 1.0 / 65536.0);
        assert_eq!(ldexp_one(1023), f64::MAX / (2.0 - f64::EPSILON));
        assert_eq!(ldexp_one(1024), f64::INFINITY);
        assert_eq!(ldexp_one(-1074), 5e-324);
        assert_eq!(ldexp_one(-1075), 0.0);
        assert_eq!(ldexp_one(-1022), f64::MIN_POSITIVE);
    }
}
