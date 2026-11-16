// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Not ported from
// upstream: a faster route to the digits that the `printf`-style output of
// src/io/dumper and src/io/mesh_io (through `cfmt`) already prints.

//! The first `n ≤ 17` significant decimal digits of an `f64`, correctly
//! rounded (ties to even), as `{:.(n-1)e}` produces them.
//!
//! `x = m · 2^e` is multiplied by a 128-bit truncation of `10^q`
//! (`10^q = (M + δ) · 2^E`, `0 ≤ δ < 1`, `2^127 ≤ M < 2^128`), so that the
//! exact product `m · M · 2^(e+E)` is below `y = x · 10^q` by less than
//! `y · 2^-127` (less than `2^-70` once the integer part is known to be
//! below `10^17 < 2^57`, where the rounding is decided). The integer
//! part and the top 64 bits of the fraction of the computed product decide
//! the rounding unless the fraction is within two units of `2^-64` of one
//! half; those cases (exact ties included) return `None` and the caller
//! formats with `core::fmt`, so the digits are always the ones `core::fmt`
//! gives. A fraction near 0 or 1 needs no care: the true value then rounds
//! to the same integer whichever side of it the computed value falls.
//!
//! The table of `10^q`, `q ∈ [Q_MIN, Q_MAX]`, is computed exactly once on
//! first use with a small big-integer routine (repeated `× 10` and `÷ 10`
//! of a 1,344-bit number).

use std::sync::OnceLock;

/// Smallest and largest power of ten in the table (`q = n − 1 − X` for
/// decimal exponents `X ∈ [−324, 308]` and `n ∈ [1, 17]`, with margin).
const Q_MIN: i32 = -312;
const Q_MAX: i32 = 345;

/// `(M, E)` with `10^q = (M + δ) · 2^E`, `0 ≤ δ < 1`, `M` normalised.
type Pow10 = (u128, i32);

/// Little-endian 32-bit limbs.
type Big = Vec<u32>;

fn big_mul_small(a: &mut Big, k: u32) {
    let mut carry = 0u64;
    for limb in a.iter_mut() {
        let v = u64::from(*limb) * u64::from(k) + carry;
        *limb = v as u32;
        carry = v >> 32;
    }
    if carry != 0 {
        a.push(carry as u32);
    }
}

fn big_div_small(a: &mut Big, k: u32) {
    let mut rem = 0u64;
    for limb in a.iter_mut().rev() {
        let v = (rem << 32) | u64::from(*limb);
        *limb = (v / u64::from(k)) as u32;
        rem = v % u64::from(k);
    }
    while a.last() == Some(&0) {
        a.pop();
    }
}

fn big_bits(a: &Big) -> u32 {
    match a.last() {
        None => 0,
        Some(&top) => 32 * (a.len() as u32 - 1) + (32 - top.leading_zeros()),
    }
}

/// `floor(a / 2^shift)` truncated to its low 128 bits (`shift` may be
/// negative: a left shift).
fn big_top128(a: &Big, shift: i64) -> u128 {
    let mut r: u128 = 0;
    for bit in (0..128i64).rev() {
        let src = bit + shift;
        let set = if src < 0 {
            false
        } else {
            let (limb, b) = ((src / 32) as usize, (src % 32) as u32);
            a.get(limb).is_some_and(|l| (l >> b) & 1 == 1)
        };
        r = (r << 1) | u128::from(set);
    }
    r
}

fn table() -> &'static [Pow10] {
    static TABLE: OnceLock<Box<[Pow10]>> = OnceLock::new();
    TABLE.get_or_init(|| {
        let n = (Q_MAX - Q_MIN + 1) as usize;
        let mut t = vec![(0u128, 0i32); n];
        // q >= 0: 10^q exactly.
        let mut b: Big = vec![1];
        for q in 0..=Q_MAX {
            let l = i64::from(big_bits(&b));
            let shift = l - 128;
            t[(q - Q_MIN) as usize] = (big_top128(&b, shift), shift as i32);
            big_mul_small(&mut b, 10);
        }
        // q < 0: floor(2^S / 10^-q) by repeated floor division (exact:
        // floor(floor(a / b) / c) = floor(a / (b c))).
        const S: u32 = 1344;
        let mut a: Big = vec![0; (S / 32) as usize];
        a.push(1);
        for q in (Q_MIN..0).rev() {
            big_div_small(&mut a, 10);
            let l = i64::from(big_bits(&a));
            let shift = l - 128;
            t[(q - Q_MIN) as usize] = (big_top128(&a, shift), (shift - i64::from(S)) as i32);
        }
        t.into_boxed_slice()
    })
}

/// `10^k` for `k ≤ 19`.
const POW10: [u64; 20] = {
    let mut p = [1u64; 20];
    let mut i = 1;
    while i < 20 {
        p[i] = p[i - 1] * 10;
        i += 1;
    }
    p
};

/// `floor(x · 10^q)` and the top 64 bits of its fraction for `x = m · 2^e`
/// (`m` normalised to 53 bits), or `None` if the integer part does not fit
/// 64 bits.
fn scaled(m: u64, e: i32, q: i32) -> Option<(u64, u64)> {
    let (mm, ee) = table()[(q - Q_MIN) as usize];
    // P = m · M (181 bits) as three 64-bit limbs.
    let lo = u128::from(m) * (mm & u128::from(u64::MAX));
    let hi = u128::from(m) * (mm >> 64);
    let p0 = lo as u64;
    let mid = (lo >> 64) + (hi & u128::from(u64::MAX));
    let p1 = mid as u64;
    let p2 = ((hi >> 64) + (mid >> 64)) as u64;
    // y = P · 2^(e + E): `sh` fractional bits.
    let sh = -(e + ee);
    if !(64..=191).contains(&sh) {
        return None;
    }
    let sh = sh as u32;
    let p = [p0, p1, p2];
    // bit i of P
    let word = |i: u32| -> u64 {
        // 64 bits of P starting at bit i (i may exceed 192: zeros)
        let (w, b) = ((i / 64) as usize, i % 64);
        let lo = p.get(w).copied().unwrap_or(0);
        let hi = p.get(w + 1).copied().unwrap_or(0);
        if b == 0 {
            lo
        } else {
            (lo >> b) | (hi << (64 - b))
        }
    };
    // integer part: bits [sh, 192) (at most 64 bits when sh >= 128)
    if sh < 128 && word(sh + 64) != 0 {
        return None;
    }
    let int = word(sh);
    // top 64 bits of the fraction: bits [sh - 64, sh)
    let frac = word(sh - 64);
    Some((int, frac))
}

/// `floor(log10(2^e2))` for `|e2| ≤ 1650`.
fn log10_pow2(e2: i32) -> i32 {
    (e2 * 78913) >> 18
}

/// The `n` significant digits (`1 ≤ n ≤ 17`) of the finite, non-zero `x`
/// rounded to nearest, ties to even, and the decimal exponent of the
/// first digit: `|x| ≈ digits · 10^(exp − n + 1)`. `None` when the value
/// is too close to a rounding tie to decide here (the caller then uses
/// `core::fmt`).
pub(crate) fn digits(x: f64, n: usize) -> Option<(u64, i32)> {
    if !(1..=17).contains(&n) || !x.is_finite() || x == 0.0 {
        return None;
    }
    let bits = x.to_bits();
    let frac_bits = bits & ((1u64 << 52) - 1);
    let biased = ((bits >> 52) & 0x7FF) as i32;
    let (mut m, mut e) = if biased == 0 {
        (frac_bits, -1074)
    } else {
        (frac_bits | (1u64 << 52), biased - 1075)
    };
    // normalise subnormals to 53 bits
    let lz = m.leading_zeros() as i32 - 11;
    m <<= lz;
    e -= lz;
    let n_i = n as i32;
    // decimal exponent of x: X_est or X_est + 1
    let mut x10 = log10_pow2(e + 52);
    for _ in 0..3 {
        let q = n_i - 1 - x10;
        if !(Q_MIN..=Q_MAX).contains(&q) {
            return None;
        }
        let (int, frac) = scaled(m, e, q)?;
        if int >= POW10[n] {
            x10 += 1;
            continue;
        }
        if int < POW10[n - 1] {
            x10 -= 1;
            continue;
        }
        const HALF: u64 = 1 << 63;
        let up = if frac > HALF {
            true
        } else if frac < HALF - 1 {
            false
        } else {
            return None;
        };
        let d = int + u64::from(up);
        return Some(if d == POW10[n] {
            (POW10[n - 1], x10 + 1)
        } else {
            (d, x10)
        });
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The digits and exponent of `{:.(n-1)e}`.
    fn reference(x: f64, n: usize) -> (u64, i32) {
        let s = format!("{:.*e}", n - 1, x.abs());
        let (m, e) = s.split_once('e').unwrap();
        let d: String = m.chars().filter(char::is_ascii_digit).collect();
        (d.parse().unwrap(), e.parse().unwrap())
    }

    fn check(x: f64, fallbacks: &mut usize) {
        for n in 1..=17 {
            match digits(x, n) {
                Some(got) => assert_eq!(
                    got,
                    reference(x, n),
                    "x = {x:e} ({:#x}), n = {n}",
                    x.to_bits()
                ),
                None => *fallbacks += 1,
            }
        }
    }

    #[test]
    fn table_is_exact_where_it_can_be() {
        let t = table();
        for q in 0..=38 {
            let (m, e) = t[(q - Q_MIN) as usize];
            let p = 10u128.pow(q as u32);
            // 10^q = M · 2^E exactly for q ≤ 38
            if e >= 0 {
                assert_eq!(m << e, p);
            } else {
                assert_eq!(m >> -e, p);
                assert_eq!(m & ((1u128 << -e) - 1), 0);
            }
        }
        for &(m, _) in t {
            assert!(m >> 127 == 1, "normalised");
        }
    }

    /// Little-endian 64-bit limbs; only what the next test needs
    /// (independent of the table's own big-integer code).
    mod big {
        use std::cmp::Ordering;

        pub type N = Vec<u64>;

        pub fn from_u128(v: u128) -> N {
            norm(vec![v as u64, (v >> 64) as u64])
        }

        fn norm(mut a: N) -> N {
            while a.last() == Some(&0) {
                a.pop();
            }
            a
        }

        pub fn mul_small(a: &N, k: u64) -> N {
            let mut out = Vec::with_capacity(a.len() + 1);
            let mut carry = 0u128;
            for &l in a {
                let v = u128::from(l) * u128::from(k) + carry;
                out.push(v as u64);
                carry = v >> 64;
            }
            out.push(carry as u64);
            norm(out)
        }

        pub fn pow10(p: u32) -> N {
            let mut a = vec![1];
            for _ in 0..p {
                a = mul_small(&a, 10);
            }
            a
        }

        /// `a · 2^s`.
        pub fn shl(a: &N, s: u32) -> N {
            let (w, b) = ((s / 64) as usize, s % 64);
            let mut out = vec![0u64; w];
            let mut carry = 0u64;
            for &l in a {
                out.push(if b == 0 { l } else { (l << b) | carry });
                carry = if b == 0 { 0 } else { l >> (64 - b) };
            }
            out.push(carry);
            norm(out)
        }

        /// `a · b` (schoolbook).
        pub fn mul(a: &N, b: &N) -> N {
            let mut out = vec![0u64; a.len() + b.len() + 1];
            for (i, &x) in a.iter().enumerate() {
                let mut carry = 0u128;
                for (j, &y) in b.iter().enumerate() {
                    let v = u128::from(out[i + j]) + u128::from(x) * u128::from(y) + carry;
                    out[i + j] = v as u64;
                    carry = v >> 64;
                }
                let mut k = i + b.len();
                while carry != 0 {
                    let v = u128::from(out[k]) + carry;
                    out[k] = v as u64;
                    carry = v >> 64;
                    k += 1;
                }
            }
            norm(out)
        }

        pub fn cmp(a: &N, b: &N) -> Ordering {
            a.len()
                .cmp(&b.len())
                .then_with(|| a.iter().rev().cmp(b.iter().rev()))
        }
    }

    /// Every entry `(M, E)` of the table satisfies its definition
    /// `M · 2^E ≤ 10^q < (M + 1) · 2^E` with `2^127 ≤ M < 2^128`, checked
    /// in exact integer arithmetic (multiplications only, unlike the
    /// table's construction by repeated division).
    #[test]
    fn every_table_entry_brackets_its_power_of_ten() {
        use std::cmp::Ordering::{Greater, Less};
        let t = table();
        assert_eq!(t.len(), (Q_MAX - Q_MIN + 1) as usize);
        for q in Q_MIN..=Q_MAX {
            let (m, e) = t[(q - Q_MIN) as usize];
            assert!(m >> 127 == 1, "q = {q}: not normalised");
            let lo = big::from_u128(m);
            let hi = big::from_u128(m + 1); // m < 2^128 - 1 for powers of ten
            // Compare M · 2^E and (M + 1) · 2^E with 10^q, both sides
            // scaled to integers.
            let (lhs_lo, lhs_hi, rhs) = match (q >= 0, e >= 0) {
                // M · 2^E ≤ 10^q < (M + 1) · 2^E
                (true, true) => (
                    big::shl(&lo, e as u32),
                    big::shl(&hi, e as u32),
                    big::pow10(q as u32),
                ),
                // M ≤ 10^q · 2^-E < M + 1
                (true, false) => (lo, hi, big::shl(&big::pow10(q as u32), (-e) as u32)),
                // M · 10^-q ≤ 2^-E < (M + 1) · 10^-q
                (false, false) => {
                    let p = big::pow10((-q) as u32);
                    (
                        big::mul(&lo, &p),
                        big::mul(&hi, &p),
                        big::shl(&vec![1], (-e) as u32),
                    )
                }
                (false, true) => panic!("q = {q}: E = {e} cannot be non-negative"),
            };
            assert!(
                big::cmp(&lhs_lo, &rhs) != Greater,
                "q = {q}: M · 2^E > 10^q"
            );
            assert!(
                big::cmp(&rhs, &lhs_hi) == Less,
                "q = {q}: 10^q ≥ (M + 1) · 2^E"
            );
        }
    }

    #[test]
    fn digits_match_core_fmt() {
        let mut fallbacks = 0;
        let mut s = 0x9E37_79B9_7F4A_7C15_u64;
        let mut rng = || {
            s ^= s >> 12;
            s ^= s << 25;
            s ^= s >> 27;
            s.wrapping_mul(0x2545_F491_4F6C_DD1D)
        };
        let specials = [
            f64::MIN_POSITIVE,
            f64::from_bits(1),
            f64::from_bits(2),
            f64::from_bits(0x000F_FFFF_FFFF_FFFF),
            f64::MAX,
            f64::EPSILON,
            0.5,
            2.5,
            0.125,
            1.5,
            9.5,
            99.5,
            0.25,
            1e23,
            1e22,
            5e-324,
            1.7976931348623157e308,
            2.2250738585072014e-308,
        ];
        for &x in &specials {
            check(x, &mut fallbacks);
            check(-x, &mut fallbacks);
        }
        let mut total = 0;
        for _ in 0..30_000 {
            // every exponent, subnormals
            let x = f64::from_bits(rng() & !(1 << 63));
            if x.is_finite() && x != 0.0 {
                check(x, &mut fallbacks);
                total += 17;
            }
            // values near powers of ten and short decimals
            let k = (rng() % 600) as i32 - 300;
            let p = 10f64.powi(k);
            for x in [p, p.next_up(), p.next_down()] {
                if x.is_finite() && x > 0.0 {
                    check(x, &mut fallbacks);
                    total += 17;
                }
            }
            #[allow(clippy::cast_precision_loss)] // small integers
            let short = (rng() % 1_000_000 + 1) as f64 / 1000.0;
            check(short, &mut fallbacks);
            check(short * p, &mut fallbacks);
            total += 34;
        }
        // the fast path decides almost every case
        assert!(fallbacks * 100 < total, "{fallbacks} fallbacks in {total}");
    }

    #[test]
    fn ties_are_left_to_core_fmt() {
        // exact ties at the requested digit
        assert_eq!(digits(2.5, 1), None);
        assert_eq!(digits(0.125, 2), None);
        assert_eq!(digits(1.5, 1), None);
        // not ties
        assert_eq!(digits(2.5, 2), Some((25, 0)));
        assert_eq!(digits(9.96, 2), Some((10, 1)));
    }
}
