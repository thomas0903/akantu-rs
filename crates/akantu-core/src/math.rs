// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/common/aka_math.hh, aka_math_tmpl.hh,
// aka_math.cc (Math::reduce), aka_extern.cc (Math::tolerance).

//! Numerical constants and predicates shared by all crates.
//!
//! Upstream `Math::tolerance` is a *mutable global* (`Math::setTolerance`).
//! In akantu-rs it is passed explicitly where configurable; this constant is
//! its default value. Geometric predicates (`contains`, `inverseMap`
//! convergence, `are_float_equal`) must use it to reproduce upstream
//! decisions exactly.

use crate::error::{Error, Result};
use crate::types::Real;

pub use crate::bbox::intervals_intersect as intersects;

/// Default of upstream `Math::tolerance` = `1e2 * epsilon`.
pub const DEFAULT_TOLERANCE: Real = 1e2 * Real::EPSILON;

/// Upstream `Math::are_float_equal(x, y)`:
/// `|x - y| <= tolerance * max(|x|, |y|, 1)`.
#[inline]
#[must_use]
pub fn are_float_equal(x: Real, y: Real, tolerance: Real) -> bool {
    let abs_max = x.abs().max(y.abs()).max(1.0);
    (x - y).abs() <= tolerance * abs_max
}

/// Upstream `Math::are_vector_equal(n, x, y)`: [`are_float_equal`] on
/// every pair (`&=` over all entries, no short circuit — same result); only
/// the common length is compared (upstream takes an explicit `n`).
#[must_use]
pub fn are_vector_equal(x: &[Real], y: &[Real], tolerance: Real) -> bool {
    x.iter()
        .zip(y)
        .fold(true, |t, (&a, &b)| t & are_float_equal(a, b, tolerance))
}

/// Upstream `Math::is_in_range(a, x_min, x_max)`: closed interval test.
#[inline]
#[must_use]
pub fn is_in_range(a: Real, x_min: Real, x_max: Real) -> bool {
    (a >= x_min) && (a <= x_max)
}

/// Upstream `Math::pow<p>(x)` / `Math::pow(x, p)`: `((1·x)·x)·…·x`, one
/// rounding per factor from the left (not `f64::powi`, whose evaluation
/// order is unspecified).
#[inline]
#[must_use]
pub fn pow_n(x: Real, p: u32) -> Real {
    let mut r = 1.0;
    for _ in 0..p {
        r *= x;
    }
    r
}

/// Upstream `Math::kronecker(i, j)`: `1.0` if `i == j`, else `0.0`.
///
/// Upstream accepts integral types only (`std::is_integral`); here the
/// bound `Eq` refuses floating-point arguments at compile time:
///
/// ```compile_fail
/// akantu_core::math::kronecker(1.0_f64, 1.0_f64);
/// ```
#[inline]
#[must_use]
pub fn kronecker<T: Eq>(i: T, j: T) -> Real {
    if i == j { 1.0 } else { 0.0 }
}

/// Outcome of a scalar Newton–Raphson solve
/// ([`NewtonRaphson::solve_unchecked`]).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NewtonRaphsonOutcome {
    /// Last iterate.
    pub x: Real,
    /// `f(x)` at the last iterate.
    pub residual: Real,
    /// Iterations performed.
    pub iterations: usize,
    /// Upstream's acceptance test `iterations < max_iteration`
    /// (`AKANTU_DEBUG_ASSERT`, active in the oracle build): an iterate that
    /// reaches the tolerance exactly on the last allowed iteration counts as
    /// not converged (UD-268).
    pub converged: bool,
}

/// Scalar Newton–Raphson solver (upstream `Math::NewtonRaphson<Real>`,
/// CORE-027): `x ← x − f(x)/f'(x)` while `|f(x)| > tolerance` and fewer
/// than `max_iteration` iterations were made.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NewtonRaphson {
    /// Absolute tolerance on `|f(x)|`.
    pub tolerance: Real,
    /// Maximum number of iterations.
    pub max_iteration: usize,
}

impl NewtonRaphson {
    /// Solver with the given tolerance and iteration limit.
    #[must_use]
    pub fn new(tolerance: Real, max_iteration: usize) -> Self {
        Self {
            tolerance,
            max_iteration,
        }
    }

    /// Iterate from `x0` exactly as upstream `solve` (same operations and
    /// stopping test; a NaN residual stops the loop as upstream's
    /// `std::abs(f) > tol` does) and report the outcome without judging it
    /// (the compat path of UD-268: a NaN or infinite root is reported with
    /// `converged = true` when the iteration limit was not reached, as
    /// upstream returns it).
    pub fn solve_unchecked(
        &self,
        f: impl Fn(Real) -> Real,
        f_prime: impl Fn(Real) -> Real,
        x0: Real,
    ) -> NewtonRaphsonOutcome {
        let mut x = x0;
        let mut f_x = f(x);
        let mut iter = 0usize;
        while f_x.abs() > self.tolerance && iter < self.max_iteration {
            x -= f_x / f_prime(x);
            f_x = f(x);
            iter += 1;
        }
        NewtonRaphsonOutcome {
            x,
            residual: f_x,
            iterations: iter,
            converged: iter < self.max_iteration,
        }
    }

    /// Upstream `solve(functor, x0)` with its assertion active (as in the
    /// oracle build): the root, or an error when the iteration limit was
    /// reached or the root is not a finite number.
    ///
    /// UD-268 (c), D-003 B: upstream's loop also stops
    /// when the residual becomes NaN (`std::abs(f) > tol` is false) and
    /// returns that NaN — or an infinite iterate whose residual passes the
    /// test — as a converged root (e.g. f = x² − 2 from x0 = 0: f′(0) = 0,
    /// x = +∞, then NaN). The default refuses a non-finite residual or
    /// iterate; [`NewtonRaphson::solve_unchecked`] is the compat path that
    /// reports upstream's outcome unjudged.
    ///
    /// # Errors
    /// [`Error::NotConverged`] if `iterations == max_iteration` (upstream's
    /// assertion); [`Error::Numerical`] if the last iterate or its residual
    /// is NaN or infinite.
    pub fn solve(
        &self,
        f: impl Fn(Real) -> Real,
        f_prime: impl Fn(Real) -> Real,
        x0: Real,
    ) -> Result<Real> {
        let o = self.solve_unchecked(f, f_prime, x0);
        if !o.converged {
            Err(Error::NotConverged {
                threshold: self.tolerance,
                niter: o.iterations,
                error: o.residual.abs(),
            })
        } else if !(o.x.is_finite() && o.residual.is_finite()) {
            Err(Error::Numerical(format!(
                "Newton-Raphson: non-finite root after {} iteration(s) (x = {}, f(x) = {}; \
                 upstream returns it as converged, UD-268)",
                o.iterations, o.x, o.residual
            )))
        } else {
            Ok(o.x)
        }
    }
}

/// Below this length [`sort_total`] uses the comparison sort.
const RADIX_MIN_LEN: usize = 512;
/// Bits per digit of [`sort_total`]'s radix passes (6 passes for 64 bits).
const RADIX_BITS: u32 = 11;

/// Order-preserving key of `f64::total_cmp`: unsigned comparison of the
/// keys is `total_cmp` of the values (negative values have every bit
/// flipped, non-negative ones the sign bit).
#[inline]
fn total_key(x: f64) -> u64 {
    let b = x.to_bits();
    if b >> 63 == 1 { !b } else { b | (1 << 63) }
}

/// Sort ascending by `f64::total_cmp`. Values with equal keys have equal
/// bits, so the result is the same array as any correct sort by
/// `total_cmp` (`sort_unstable_by(f64::total_cmp)` for short slices); long
/// slices use a least-significant-digit radix sort of the
/// [`total_key`]s (passes on which every key has the same digit are
/// skipped).
pub fn sort_total(values: &mut [f64]) {
    let n = values.len();
    if n < RADIX_MIN_LEN {
        values.sort_unstable_by(f64::total_cmp);
        return;
    }
    const BUCKETS: usize = 1 << RADIX_BITS;
    const PASSES: usize = 64usize.div_ceil(RADIX_BITS as usize);
    let mask = (BUCKETS - 1) as u64;
    let mut keys: Vec<u64> = values.iter().map(|&x| total_key(x)).collect();
    // one histogram per pass, from a single read of the keys
    let mut hist = vec![[0usize; BUCKETS]; PASSES];
    for &k in &keys {
        for (p, h) in hist.iter_mut().enumerate() {
            h[((k >> (p as u32 * RADIX_BITS)) & mask) as usize] += 1;
        }
    }
    let mut tmp = vec![0u64; n];
    for (p, h) in hist.iter_mut().enumerate() {
        if h.contains(&n) {
            continue; // every key has the same digit: the pass keeps the order
        }
        let mut sum = 0usize;
        for c in h.iter_mut() {
            let here = *c;
            *c = sum;
            sum += here;
        }
        let shift = p as u32 * RADIX_BITS;
        for &k in &keys {
            let d = ((k >> shift) & mask) as usize;
            tmp[h[d]] = k;
            h[d] += 1;
        }
        std::mem::swap(&mut keys, &mut tmp);
    }
    for (v, &k) in values.iter_mut().zip(&keys) {
        // inverse of total_key
        *v = f64::from_bits(if k >> 63 == 1 { k & !(1 << 63) } else { !k });
    }
}

/// Upstream `Math::reduce` (`aka_math.cc`): deterministic pairwise sum used
/// for scalar integrals (`IntegratorGauss::integrate` → `Real`).
///
/// Exactly as upstream, it **sorts `values` ascending in place**, then
/// repeatedly folds an odd remainder into the second-to-last value and sums
/// consecutive pairs into the first half, until one value remains. Returns
/// `0.0` for an empty slice.
///
/// Sorting uses `f64::total_cmp`; upstream uses `operator<` with
/// `std::sort`. They agree on all finite non-zero values; the relative
/// placement of `-0.0`/`+0.0` (and NaN, which is undefined behaviour
/// upstream) may differ, which can only change the sign of an exactly-zero
/// result.
pub fn reduce(values: &mut [Real]) -> Real {
    let mut n = values.len();
    if n == 0 {
        return 0.0;
    }
    let mut half = n >> 1;
    sort_total(values);
    while half != 0 {
        if n - 2 * half != 0 {
            values[n - 2] += values[n - 1];
        }
        for i in 0..half {
            values[i] = values[2 * i] + values[2 * i + 1];
        }
        n = half;
        half >>= 1;
    }
    values[0]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reduce_matches_upstream_algorithm() {
        assert_eq!(reduce(&mut []), 0.0);
        assert_eq!(reduce(&mut [4.0]), 4.0);
        // n = 3: sort → [1, 2, 3]; fold: [1, 5]; pair: 6
        let mut v = [3.0, 1.0, 2.0];
        assert_eq!(reduce(&mut v), 6.0);
        // The pairwise order is observable in floating point:
        // sorted [1.5e-16, 1.5e-16, 1, 1] → 3e-16 + 2 rounds up to 2 + ulp(2),
        // whereas naive left-to-right summation of the input gives exactly 2.
        let mut w = [1.0, 1.5e-16, 1.0, 1.5e-16];
        let r = reduce(&mut w);
        assert_eq!(r, 2.0 + 2.0 * f64::EPSILON);
        assert_eq!(((1.0 + 1.5e-16) + 1.0) + 1.5e-16, 2.0);
        // n = 6: [a+b, c+d, e+f] → fold → [a+b, (c+d)+(e+f)] → sum
        let mut x = [6.0, 5.0, 4.0, 3.0, 2.0, 1.0];
        assert_eq!(reduce(&mut x), (1.0 + 2.0) + ((3.0 + 4.0) + (5.0 + 6.0)));
    }

    fn special_or_random(rng: &mut u64) -> f64 {
        // xorshift64
        *rng ^= *rng << 13;
        *rng ^= *rng >> 7;
        *rng ^= *rng << 17;
        let r = *rng;
        match r % 16 {
            0 => 0.0,
            1 => -0.0,
            2 => f64::INFINITY,
            3 => f64::NEG_INFINITY,
            4 => f64::from_bits(0x7ff8_0000_0000_0000 | (r >> 20)), // NaNs, both signs below
            5 => -f64::from_bits(0x7ff0_0000_0000_0001 | (r >> 24)),
            6 => f64::from_bits(r >> 13), // subnormals and small
            7 => -f64::from_bits(r >> 12),
            8 => f64::MIN_POSITIVE,
            9 => ((r >> 40) % 7) as f64, // many equal values
            _ => f64::from_bits(r),      // any bit pattern
        }
    }

    /// The radix sort gives the bits of the comparison sort, on random
    /// bit patterns with signed zeros, infinities, NaNs (both signs,
    /// several payloads), subnormals and repeated values, around the
    /// switch length; and `reduce` gives the same bits.
    #[test]
    fn sort_total_matches_the_comparison_sort_bitwise() {
        let mut rng = 0x9e37_79b9_7f4a_7c15_u64;
        for &n in &[0usize, 1, 2, 511, 512, 513, 1000, 4096, 20_000] {
            for _ in 0..4 {
                let v: Vec<f64> = (0..n).map(|_| special_or_random(&mut rng)).collect();
                let mut a = v.clone();
                a.sort_unstable_by(f64::total_cmp);
                let mut b = v.clone();
                sort_total(&mut b);
                assert!(
                    a.iter().zip(&b).all(|(x, y)| x.to_bits() == y.to_bits()),
                    "n = {n}"
                );
                // narrow positive data (the radix passes skip digits)
                let w: Vec<f64> = (0..n)
                    .map(|_| 1.0 + (special_or_random(&mut rng).to_bits() >> 40) as f64 * 1e-9)
                    .collect();
                let mut a = w.clone();
                a.sort_unstable_by(f64::total_cmp);
                let mut b = w.clone();
                sort_total(&mut b);
                assert!(a.iter().zip(&b).all(|(x, y)| x.to_bits() == y.to_bits()));
                // reduce: the comparison-sorted reference algorithm
                let mut c = w.clone();
                let mut r = w.clone();
                r.sort_unstable_by(f64::total_cmp);
                let reference = {
                    let mut n = r.len();
                    let mut half = n >> 1;
                    while half != 0 {
                        if n - 2 * half != 0 {
                            r[n - 2] += r[n - 1];
                        }
                        for i in 0..half {
                            r[i] = r[2 * i] + r[2 * i + 1];
                        }
                        n = half;
                        half >>= 1;
                    }
                    r.first().copied().unwrap_or(0.0)
                };
                assert_eq!(reduce(&mut c).to_bits(), reference.to_bits());
            }
        }
    }

    #[test]
    fn scalar_helpers() {
        assert!(are_vector_equal(
            &[1.0, 2.0],
            &[1.0 + f64::EPSILON, 2.0],
            DEFAULT_TOLERANCE
        ));
        assert!(!are_vector_equal(
            &[1.0, 2.0],
            &[1.0, 2.1],
            DEFAULT_TOLERANCE
        ));
        assert!(is_in_range(1.0, 1.0, 2.0) && !is_in_range(2.5, 1.0, 2.0));
        assert!(intersects(0.0, 1.0, 1.0, 2.0) && !intersects(0.0, 1.0, 1.5, 2.0));
        assert_eq!(pow_n(3.0, 0), 1.0);
        assert_eq!(pow_n(1.1, 3), (1.1 * 1.1) * 1.1);
        assert_eq!((kronecker(2, 2), kronecker(1, 2)), (1.0, 0.0));
    }

    #[test]
    fn newton_raphson_follows_upstream() {
        let nr = NewtonRaphson::new(1e-12, 50);
        let f = |x: f64| x * x - 2.0;
        let df = |x: f64| 2.0 * x;
        let o = nr.solve_unchecked(f, df, 1.0);
        // same iterates as the upstream loop written out
        let (mut x, mut fx, mut it) = (1.0_f64, f(1.0), 0);
        while fx.abs() > 1e-12 && it < 50 {
            x -= fx / df(x);
            fx = f(x);
            it += 1;
        }
        assert_eq!((o.x, o.iterations, o.residual), (x, it, fx));
        assert_eq!(nr.solve(f, df, 1.0).unwrap(), x);
        // limit reached: error (upstream assertion)
        assert!(NewtonRaphson::new(0.0, 3).solve(f, df, 1.0).is_err());
        // converging exactly on the last allowed iteration is rejected too
        let n = o.iterations;
        assert!(
            !NewtonRaphson::new(1e-12, n)
                .solve_unchecked(f, df, 1.0)
                .converged
        );
        // NaN residual stops the loop, reported as converged (as upstream)
        let nan = NewtonRaphson::new(1e-12, 5).solve_unchecked(|_| f64::NAN, df, 1.0);
        assert!(nan.converged && nan.iterations == 0);
    }

    /// Upstream's loop stops on a NaN
    /// residual (`std::abs(f) > tol` is false) and returns the NaN as a
    /// converged root. f = x² − 2 from x0 = 0: f'(0) = 0, x = +∞, then NaN.
    /// The default `solve` refuses a non-finite residual or iterate (D-003
    /// B); `solve_unchecked` keeps upstream's outcome (compat path).
    #[test]
    fn solve_refuses_a_non_finite_root() {
        let nr = NewtonRaphson::new(1e-12, 50);
        let f = |x: f64| x * x - 2.0;
        let df = |x: f64| 2.0 * x;
        assert!(nr.solve(f, df, 0.0).is_err());
        let raw = nr.solve_unchecked(f, df, 0.0);
        assert!(raw.converged && raw.x.is_nan() && raw.residual.is_nan());
        assert_eq!(raw.iterations, 2);
        // a NaN start and a NaN residual are refused too
        assert!(nr.solve(f, df, f64::NAN).is_err());
        assert!(nr.solve(|_| f64::NAN, df, 1.0).is_err());
        // an infinite iterate with a finite residual: f = 1/x from 1e308,
        // f' = −1/x² = −0, x = +∞, f(+∞) = 0 ≤ tol = 0 — upstream accepts
        // +∞ as the root; refused
        let (g, dg) = (|x: f64| 1.0 / x, |x: f64| -1.0 / (x * x));
        let inf = NewtonRaphson::new(0.0, 5).solve_unchecked(g, dg, 1e308);
        assert!(inf.converged && inf.x == f64::INFINITY && inf.residual == 0.0);
        assert!(NewtonRaphson::new(0.0, 5).solve(g, dg, 1e308).is_err());
        // finite roots are unchanged
        assert_eq!(
            nr.solve(f, df, 1.0).unwrap(),
            nr.solve_unchecked(f, df, 1.0).x
        );
    }

    #[test]
    fn default_tolerance() {
        assert_eq!(DEFAULT_TOLERANCE, 100.0 * f64::EPSILON);
        assert!(are_float_equal(1.0, 1.0 + f64::EPSILON, DEFAULT_TOLERANCE));
        assert!(!are_float_equal(1.0, 1.0 + 1e-10, DEFAULT_TOLERANCE));
        // relative for large magnitudes
        assert!(are_float_equal(
            1e20,
            1e20 * (1.0 + 50.0 * f64::EPSILON),
            DEFAULT_TOLERANCE
        ));
        // absolute (scale 1) for small magnitudes
        assert!(are_float_equal(0.0, 1e-15, DEFAULT_TOLERANCE));
    }
}
