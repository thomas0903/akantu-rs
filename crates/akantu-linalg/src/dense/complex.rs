// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs: complex arithmetic used by the dense eigen-solvers (std::complex
// division as used by Eigen 3.4.0 in Akantu's aka_types.hh, EPFL/LSMS).
//
// CLEAN-ROOM implementation (D-030 and its amendment).
// `Complex::quotient` reproduces the oracle's `std::complex<double>` division
// (g++ 14.2.0, Debian trixie, aarch64) and, as `ComplexDivision::Unfused`,
// the x86_64 oracle's (D-043). It was derived ONLY from:
//   * R. L. Smith, "Algorithm 116: Complex division", CACM 5(8), 1962;
//   * M. Baudin, R. L. Smith, "A robust complex division in Scilab", 2012
//     (the d·(b/c) numerators used when the ratio underflows);
//   * ISO/IEC 9899:2011 Annex G, G.5.1 (special-value recovery, EXAMPLE 2);
//   * IEEE 754-2008 (fused multiply-add semantics);
//   * black-box experiments: small C++ programs dividing std::complex<double>
//     values in the `akantu-oracle:dev` image, observing outputs bit for bit
//     (oracle/probes/dense/complex_cleanroom/).
// No GCC/libgcc/libstdc++ source was read and no compiled library code was
// disassembled or otherwise inspected. Provenance of every choice:
// crates/akantu-linalg/src/dense/COMPLEX_CLEANROOM_LOG.md.

//! Minimal complex numbers with the oracle's division semantics.
//!
//! [`Complex::quotient`] reproduces, bit for bit, the results of the oracle
//! runtime's `std::complex<double>` division (fixtures
//! `dense_strict.txt` `cdiv` records and `complex_div_cleanroom.txt`). The
//! observed algorithm (established by black-box experiments only, see
//! `COMPLEX_CLEANROOM_LOG.md`) is:
//!
//! 1. **Range scaling** of all four operands by a power of two (with
//!    `CD = max(|c|, |d|)`, `AB = max(|a|, |b|)`, a NaN `c` or `a` propagating
//!    into the maximum): `CD >= DBL_MAX/2` → `×1/2`; else `CD < 2^-52` →
//!    `×2^52`; else, if `|a| < DBL_MIN` or `|b| < DBL_MIN` (zero included)
//!    and `AB, CD < 0x1.fffffffffffffp+970` (= `DBL_MAX·2^-53`) → `×2^52`.
//! 2. **Smith's algorithm** with the ratio `r = c/d` when `|c| < |d|`, else
//!    `r = d/c`; the denominator and both numerators are single-rounding
//!    fused multiply-adds and the quotients are true divisions. When
//!    `|r| <= DBL_MIN` the numerators take the Baudin & Smith form
//!    (e.g. `fma(d, b/c, a)` instead of `fma(b, r, a)`).
//! 3. **Annex G recovery** (ISO C11 G.5.1, EXAMPLE 2) of infinities and
//!    zeros when both parts are NaN, evaluated on the scaled operands.
//!
//! **Two runtimes ([`ComplexDivision`], D-043).** The algorithm above is
//! Debian's aarch64 runtime, whose oracle fixtures are authoritative for
//! merges until the x86_64 corpus exists (D-043(5)); it is the default and
//! what `/` does. Debian's x86_64 runtime (the authoritative platform of
//! D-043(1)) performs the same steps with every fused multiply-add replaced
//! by a rounded product followed by a rounded sum
//! ([`ComplexDivision::Unfused`]; black-box experiment E28 on the x86_64
//! oracle and the fixture `complex_div_cleanroom_x86_64.txt`, see
//! `COMPLEX_CLEANROOM_LOG.md` §8). The two differ in the last bits of some
//! quotients, and through Eigen's eigenvector back-substitution in many
//! entries of complex eigenvectors (`dense::eig_vectors_complex_with`).
//!
//! The fused multiply-adds require [`f64::mul_add`]: a documented,
//! evidence-backed exception to the no-`mul_add` rule (AGENTS.md, "Rules for numerical code"), since
//! the reference semantics *are* fused here (black-box experiment E1: the
//! fused variant matches 2 000 000 of 2 000 000 cases, every unfused variant
//! fails on at least 7 302). `mul_add` is correctly rounded on every target,
//! so the results do not depend on the host. NaN sign bits and payloads are
//! not reproduced (neither the C++ nor the Rust language specifies them);
//! NaN-ness is.

/// A complex number `re + i·im`.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Complex {
    /// Real part.
    pub re: f64,
    /// Imaginary part.
    pub im: f64,
}

/// Smallest positive normal double (`DBL_MIN = 2^-1022`): the switch to the
/// Baudin & Smith numerators (`|r| <= DBL_MIN`, E3c) and the small-numerator
/// trigger (`|a| < DBL_MIN`, E16/E17).
const MIN_NORMAL: f64 = f64::MIN_POSITIVE;
/// `DBL_MAX / 2` (exact): denominators with `max(|c|, |d|)` at or above are
/// halved together with the numerator (E19).
const HALF_MAX: f64 = f64::MAX / 2.0;
/// `2^-52`: denominators with `max(|c|, |d|)` strictly below are scaled up
/// (E6).
const TINY_DENOMINATOR: f64 = f64::EPSILON;
/// `2^52`: the up-scaling factor (E6, E7, E13).
const UP_SCALE: f64 = 1.0 / f64::EPSILON;
/// `0x1.fffffffffffffp+970` (= `DBL_MAX·2^-53`, the predecessor of `2^971`):
/// no small-numerator up-scaling at or above this magnitude of `max(|a|, |b|)`
/// or `max(|c|, |d|)`, so that `2·max(|c|, |d|)·2^52` (the largest scaled
/// denominator) cannot overflow. The x86_64 runtime up-scales at
/// `0x1.ffffffffffffep+970` and not at `0x1.fffffffffffffp+970`, for both
/// maxima (black-box bisections, log §9:
/// with `2^971`, [`ComplexDivision::Unfused`] differed in the sign of a zero
/// part on 996 of 24,261,021 records, all at that magnitude). For the aarch64
/// runtime any guard in `[2^60, 2^971]` gives identical results (E18, E20),
/// so both variants share it.
const UP_SCALE_GUARD: f64 = f64::from_bits(0x7c9f_ffff_ffff_ffff);

/// `max(|x|, |y|)` with a NaN `x` propagating (`|x| < |y| ? |y| : |x|`, E23).
fn abs_max(x: f64, y: f64) -> f64 {
    let (x, y) = (x.abs(), y.abs());
    if x < y { y } else { x }
}

/// Which oracle runtime's `std::complex<double>` division
/// [`Complex::quotient_with`] reproduces (module docs, D-043).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum ComplexDivision {
    /// Debian's aarch64 runtime: Smith's denominator and numerators are
    /// single-rounding fused multiply-adds. The default (the aarch64 oracle
    /// fixtures are authoritative for merges until the x86_64 corpus
    /// exists, D-043(5)); what `/` and [`Complex::quotient`] do.
    #[default]
    Fused,
    /// Debian's x86_64 runtime: the same steps with each fused multiply-add
    /// `x·y + z` evaluated as a rounded product followed by a rounded sum
    /// (black-box experiment E28).
    Unfused,
}

/// `x·y + z` as the runtime evaluates it.
fn mad(division: ComplexDivision, x: f64, y: f64, z: f64) -> f64 {
    match division {
        ComplexDivision::Fused => x.mul_add(y, z),
        ComplexDivision::Unfused => x * y + z,
    }
}

/// Smith's algorithm on (already scaled) operands: denominator and
/// numerators as multiply-adds (fused or not), true divisions, Baudin &
/// Smith numerators when `|r| <= DBL_MIN`.
#[allow(clippy::many_single_char_names)] // a, b, c, d, r: the textbook names
fn smith(a: f64, b: f64, c: f64, d: f64, f: ComplexDivision) -> (f64, f64) {
    if c.abs() < d.abs() {
        let r = c / d;
        let den = mad(f, c, r, d);
        if r.abs() > MIN_NORMAL {
            (mad(f, a, r, b) / den, mad(f, b, r, -a) / den)
        } else {
            (mad(f, c, a / d, b) / den, mad(f, c, b / d, -a) / den)
        }
    } else {
        let r = d / c;
        let den = mad(f, d, r, c);
        if r.abs() > MIN_NORMAL {
            (mad(f, b, r, a) / den, mad(f, -a, r, b) / den)
        } else {
            (mad(f, d, b / c, a) / den, mad(f, -d, a / c, b) / den)
        }
    }
}

/// `1` for an infinity, `0` otherwise, with the sign of `x` (Annex G).
fn unit_if_infinite(x: f64) -> f64 {
    (if x.is_infinite() { 1.0_f64 } else { 0.0 }).copysign(x)
}

impl Complex {
    /// `re + i·im`.
    #[must_use]
    pub const fn new(re: f64, im: f64) -> Self {
        Self { re, im }
    }

    /// Both parts finite (`numext::isfinite` on a complex).
    #[must_use]
    pub fn is_finite(self) -> bool {
        self.re.is_finite() && self.im.is_finite()
    }

    /// `|z|² = re·re + im·im` (Eigen `numext::abs2`).
    #[must_use]
    pub fn abs2(self) -> f64 {
        self.re * self.re + self.im * self.im
    }

    /// Division by a real (`std::complex<double> / double`: both parts
    /// divided).
    #[must_use]
    pub fn div_real(self, s: f64) -> Self {
        Self::new(self.re / s, self.im / s)
    }

    /// Complex division with the aarch64 oracle runtime's semantics
    /// ([`ComplexDivision::Fused`], see module docs); also available as the
    /// `/` operator.
    #[must_use]
    pub fn quotient(self, other: Complex) -> Self {
        self.quotient_with(other, ComplexDivision::Fused)
    }

    /// Complex division with the semantics of the given oracle runtime
    /// (module docs).
    #[must_use]
    #[allow(clippy::many_single_char_names)] // a, b, c, d: the textbook names
    pub fn quotient_with(self, other: Complex, division: ComplexDivision) -> Self {
        let (mut a, mut b, mut c, mut d) = (self.re, self.im, other.re, other.im);
        let ab = abs_max(a, b);
        let cd = abs_max(c, d);
        let scale = if cd >= HALF_MAX {
            Some(0.5)
        } else if cd < TINY_DENOMINATOR
            || ((a.abs() < MIN_NORMAL || b.abs() < MIN_NORMAL)
                && ab < UP_SCALE_GUARD
                && cd < UP_SCALE_GUARD)
        {
            Some(UP_SCALE)
        } else {
            None
        };
        if let Some(s) = scale {
            a *= s;
            b *= s;
            c *= s;
            d *= s;
        }
        let (x, y) = smith(a, b, c, d, division);
        if !(x.is_nan() && y.is_nan()) {
            return Self::new(x, y);
        }
        // ISO C11 Annex G.5.1 (EXAMPLE 2): recover infinities and zeros that
        // computed as NaN + iNaN, on the scaled operands (E22, E23, E25).
        if c == 0.0 && d == 0.0 && (!a.is_nan() || !b.is_nan()) {
            let inf = f64::INFINITY.copysign(c);
            Self::new(inf * a, inf * b)
        } else if (a.is_infinite() || b.is_infinite()) && c.is_finite() && d.is_finite() {
            let (a, b) = (unit_if_infinite(a), unit_if_infinite(b));
            Self::new(
                f64::INFINITY * (a * c + b * d),
                f64::INFINITY * (b * c - a * d),
            )
        } else if (c.is_infinite() || d.is_infinite()) && a.is_finite() && b.is_finite() {
            let (c, d) = (unit_if_infinite(c), unit_if_infinite(d));
            Self::new(0.0 * (a * c + b * d), 0.0 * (b * c - a * d))
        } else {
            Self::new(x, y)
        }
    }
}

impl std::ops::Div for Complex {
    type Output = Complex;

    /// [`Complex::quotient`].
    fn div(self, rhs: Complex) -> Complex {
        self.quotient(rhs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_cases() {
        let z = Complex::new(1.0, 2.0) / Complex::new(3.0, 4.0);
        assert!((z.re - 0.44).abs() < 1e-16 && (z.im - 0.08).abs() < 1e-16);
        assert_eq!(
            (Complex::new(4.0, 2.0) / Complex::new(2.0, 0.0)),
            Complex::new(2.0, 1.0)
        );
        assert_eq!(
            (Complex::new(0.0, 2.0) / Complex::new(0.0, 2.0)),
            Complex::new(1.0, 0.0)
        );
    }

    #[test]
    fn special_values() {
        let z = Complex::new(1.0, 1.0) / Complex::new(0.0, 0.0);
        assert!(z.re.is_infinite() && z.im.is_infinite());
        let z = Complex::new(1.0, 1.0) / Complex::new(f64::INFINITY, 0.0);
        assert_eq!((z.re, z.im), (0.0, 0.0));
        let z = Complex::new(f64::INFINITY, 0.0) / Complex::new(1.0, 1.0);
        assert!(z.re.is_infinite());
        // tiny and huge operands are rescaled, not flushed
        let z = Complex::new(1e-300, 1e-300) / Complex::new(1e-300, 1e-300);
        assert_eq!(z, Complex::new(1.0, 0.0));
        let z = Complex::new(1e300, 1e300) / Complex::new(1e308, 1e308);
        assert!((z.re - 1e-8).abs() < 1e-22);
    }

    #[test]
    fn constants() {
        assert_eq!(HALF_MAX.to_bits(), 0x7fdf_ffff_ffff_ffff);
        assert_eq!(TINY_DENOMINATOR, 2f64.powi(-52));
        assert_eq!(UP_SCALE, 2f64.powi(52));
        // DBL_MAX · 2^-53, the predecessor of 2^971 (log §9)
        assert_eq!(UP_SCALE_GUARD, f64::MAX * 2f64.powi(-53));
        assert_eq!(UP_SCALE_GUARD.to_bits() + 1, 2f64.powi(971).to_bits());
        assert_eq!(MIN_NORMAL, 2f64.powi(-1022));
    }

    #[test]
    fn tie_takes_the_d_over_c_branch() {
        // E0: (1 + i)/(1 - i) = +0 + i (the r = c/d branch would give -0).
        let z = Complex::new(1.0, 1.0) / Complex::new(1.0, -1.0);
        assert_eq!((z.re.to_bits(), z.im.to_bits()), (0, 1f64.to_bits()));
    }
}
