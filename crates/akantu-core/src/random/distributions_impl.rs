// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a port of Akantu (EPFL/LSMS): the distributions wrapped by
// src/common/aka_random_generator.hh (RandomDistributionProxy around
// std::*_distribution<Real>). Clean-room implementation (D-030) written from
// the ISO C++ specification ([rand.dist.*]), published algorithms (Marsaglia &
// Bray 1964 polar method; Marsaglia & Tsang 2000 gamma method; textbook
// inversion formulas) and black-box experiments against the oracle toolchain;
// every operation-order choice is recorded in CLEANROOM_LOG.md.

//! The eleven standard distributions used by Akantu, with the internal
//! state, arithmetic and text serialisation of the oracle's standard library.
//!
//! All transcendental functions are the bit-exact glibc ports of
//! [`super::libm`]; square roots are IEEE-754 (correctly rounded) and every
//! other operation is a separately rounded IEEE-754 operation (the oracle's
//! `-ffp-contract=off` configuration).

use std::f64::consts::PI;

use super::engine::{UniformRandomBitGenerator, generate_canonical};
use super::fmt_c::ostream_scientific17;
use super::libm::{exp, log, pow, tan};

/// Appends `x` as the distributions' `operator<<` writes numbers
/// (`scientific`, 17 significant decimals).
fn put_real(out: &mut String, x: f64) {
    out.push_str(&ostream_scientific17(x));
}

/// Appends a separating space and `x`.
fn put_sep_real(out: &mut String, x: f64) {
    out.push(' ');
    put_real(out, x);
}

/* ------------------------------------------------------------------------ */

/// `std::uniform_real_distribution<double>(a, b)`: `u · (b − a) + a` with
/// `u = generate_canonical` (two engine calls).
#[derive(Clone, Debug, PartialEq)]
pub struct UniformRealDistribution {
    a: f64,
    b: f64,
}

impl UniformRealDistribution {
    /// `uniform_real_distribution(a, b)`.
    #[must_use]
    pub const fn new(a: f64, b: f64) -> Self {
        Self { a, b }
    }

    /// `a()`.
    #[must_use]
    pub const fn a(&self) -> f64 {
        self.a
    }

    /// `b()`.
    #[must_use]
    pub const fn b(&self) -> f64 {
        self.b
    }

    /// `reset()` (no state).
    pub fn reset(&mut self) {}

    /// One draw.
    pub fn sample<G: UniformRandomBitGenerator>(&mut self, g: &mut G) -> f64 {
        let u = generate_canonical(g);
        u * (self.b - self.a) + self.a
    }

    /// `operator<<`: `a b`.
    pub fn write_params(&self, out: &mut String) {
        put_real(out, self.a);
        put_sep_real(out, self.b);
    }
}

/* ------------------------------------------------------------------------ */

/// `std::exponential_distribution<double>(lambda)`: `−log(1 − u) / λ`.
#[derive(Clone, Debug, PartialEq)]
pub struct ExponentialDistribution {
    lambda: f64,
}

impl ExponentialDistribution {
    /// `exponential_distribution(lambda)`.
    #[must_use]
    pub const fn new(lambda: f64) -> Self {
        Self { lambda }
    }

    /// `lambda()`.
    #[must_use]
    pub const fn lambda(&self) -> f64 {
        self.lambda
    }

    /// `reset()` (no state).
    pub fn reset(&mut self) {}

    /// One draw.
    pub fn sample<G: UniformRandomBitGenerator>(&mut self, g: &mut G) -> f64 {
        let u = generate_canonical(g);
        -log(1.0 - u) / self.lambda
    }

    /// `operator<<`: `lambda`.
    pub fn write_params(&self, out: &mut String) {
        put_real(out, self.lambda);
    }
}

/* ------------------------------------------------------------------------ */

/// `std::normal_distribution<double>(mean, stddev)`: Marsaglia–Bray polar
/// method; each accepted pair yields two standard normal values, the second
/// of which is cached in the distribution (`saved`) and returned by the next
/// draw. The result is `z · stddev + mean`.
#[derive(Clone, Debug, PartialEq)]
pub struct NormalDistribution {
    mean: f64,
    stddev: f64,
    saved: f64,
    saved_available: bool,
}

impl NormalDistribution {
    /// `normal_distribution(mean, stddev)`.
    #[must_use]
    pub const fn new(mean: f64, stddev: f64) -> Self {
        Self {
            mean,
            stddev,
            saved: 0.0,
            saved_available: false,
        }
    }

    /// `normal_distribution(0, 1)`.
    #[must_use]
    pub const fn standard() -> Self {
        Self::new(0.0, 1.0)
    }

    /// `mean()`.
    #[must_use]
    pub const fn mean(&self) -> f64 {
        self.mean
    }

    /// `stddev()`.
    #[must_use]
    pub const fn stddev(&self) -> f64 {
        self.stddev
    }

    /// Whether a cached standard normal value is available for the next draw.
    #[must_use]
    pub const fn saved_available(&self) -> bool {
        self.saved_available
    }

    /// `reset()`: drops the cached value.
    pub fn reset(&mut self) {
        self.saved_available = false;
    }

    /// One standard normal value (cached value first).
    fn standard_value<G: UniformRandomBitGenerator>(&mut self, g: &mut G) -> f64 {
        if self.saved_available {
            self.saved_available = false;
            return self.saved;
        }
        loop {
            let x = 2.0 * generate_canonical(g) - 1.0;
            let y = 2.0 * generate_canonical(g) - 1.0;
            let r2 = x * x + y * y;
            if r2 > 1.0 || r2 == 0.0 {
                continue;
            }
            let mult = (-2.0 * log(r2) / r2).sqrt();
            self.saved = x * mult;
            self.saved_available = true;
            return y * mult;
        }
    }

    /// One draw.
    pub fn sample<G: UniformRandomBitGenerator>(&mut self, g: &mut G) -> f64 {
        let z = self.standard_value(g);
        z * self.stddev + self.mean
    }

    /// `operator<<`: `mean stddev flag [saved]`.
    pub fn write_params(&self, out: &mut String) {
        put_real(out, self.mean);
        put_sep_real(out, self.stddev);
        out.push_str(if self.saved_available { " 1" } else { " 0" });
        if self.saved_available {
            put_sep_real(out, self.saved);
        }
    }
}

/* ------------------------------------------------------------------------ */

/// `std::lognormal_distribution<double>(m, s)`: `exp(s · z + m)` with `z`
/// from an internal standard normal distribution (with its cache).
#[derive(Clone, Debug, PartialEq)]
pub struct LognormalDistribution {
    m: f64,
    s: f64,
    nd: NormalDistribution,
}

impl LognormalDistribution {
    /// `lognormal_distribution(m, s)`.
    #[must_use]
    pub const fn new(m: f64, s: f64) -> Self {
        Self {
            m,
            s,
            nd: NormalDistribution::standard(),
        }
    }

    /// `m()`.
    #[must_use]
    pub const fn m(&self) -> f64 {
        self.m
    }

    /// `s()`.
    #[must_use]
    pub const fn s(&self) -> f64 {
        self.s
    }

    /// `reset()`: resets the internal normal distribution.
    pub fn reset(&mut self) {
        self.nd.reset();
    }

    /// One draw.
    pub fn sample<G: UniformRandomBitGenerator>(&mut self, g: &mut G) -> f64 {
        let z = self.nd.sample(g);
        exp(self.s * z + self.m)
    }

    /// `operator<<`: `m s <normal>`.
    pub fn write_params(&self, out: &mut String) {
        put_real(out, self.m);
        put_sep_real(out, self.s);
        out.push(' ');
        self.nd.write_params(out);
    }
}

/* ------------------------------------------------------------------------ */

/// `std::gamma_distribution<double>(alpha, beta)`: Marsaglia–Tsang (2000)
/// squeeze/rejection method on an internal standard normal distribution; for
/// `alpha < 1` a Gamma(alpha + 1) value is multiplied by `u^(1/alpha)`.
#[derive(Clone, Debug, PartialEq)]
pub struct GammaDistribution {
    alpha: f64,
    beta: f64,
    malpha: f64,
    a2: f64,
    nd: NormalDistribution,
}

impl GammaDistribution {
    /// `gamma_distribution(alpha, beta)`.
    #[must_use]
    pub fn new(alpha: f64, beta: f64) -> Self {
        let malpha = if alpha < 1.0 { alpha + 1.0 } else { alpha };
        let a2 = 1.0 / (9.0 * (malpha - 1.0 / 3.0)).sqrt();
        Self {
            alpha,
            beta,
            malpha,
            a2,
            nd: NormalDistribution::standard(),
        }
    }

    /// `alpha()`.
    #[must_use]
    pub const fn alpha(&self) -> f64 {
        self.alpha
    }

    /// `beta()`.
    #[must_use]
    pub const fn beta(&self) -> f64 {
        self.beta
    }

    /// `reset()`: resets the internal normal distribution.
    pub fn reset(&mut self) {
        self.nd.reset();
    }

    /// One draw.
    #[allow(clippy::neg_cmp_op_on_partial_ord)] // NaN-sensitive loop conditions (E6)
    pub fn sample<G: UniformRandomBitGenerator>(&mut self, g: &mut G) -> f64 {
        let a1 = self.malpha - 1.0 / 3.0;
        let (mut u, mut n, mut v);
        loop {
            // Redraw the normal value while v <= 0 (a NaN v, from a NaN or
            // too negative alpha, ends the loop: E6).
            loop {
                n = self.nd.sample(g);
                v = 1.0 + self.a2 * n;
                if !(v <= 0.0) {
                    break;
                }
            }
            v = v * v * v;
            u = generate_canonical(g);
            // Squeeze, then the logarithmic test (Marsaglia & Tsang 2000);
            // accepted unless both fail. The logarithmic test accepts
            // log(u) == threshold (E5); the squeeze grouping and comparison
            // are not observable (CLEANROOM_LOG.md) and follow the paper.
            if !(u > 1.0 - 0.0331 * (n * n) * (n * n)
                && log(u) > 0.5 * n * n + a1 * (1.0 - v + log(v)))
            {
                break;
            }
        }
        // `alpha == malpha` (not `alpha >= 1`): a NaN alpha takes the
        // `alpha < 1` path, and so does no alpha with alpha + 1 == alpha (E6).
        if self.alpha == self.malpha {
            a1 * v * self.beta
        } else {
            // Extra uniform, redrawn while zero (E4).
            loop {
                u = generate_canonical(g);
                if u != 0.0 {
                    break;
                }
            }
            pow(u, 1.0 / self.alpha) * a1 * v * self.beta
        }
    }

    /// `operator<<`: `alpha beta <normal>`.
    pub fn write_params(&self, out: &mut String) {
        put_real(out, self.alpha);
        put_sep_real(out, self.beta);
        out.push(' ');
        self.nd.write_params(out);
    }
}

/* ------------------------------------------------------------------------ */

/// `std::chi_squared_distribution<double>(n)`: `2 · Gamma(n / 2, 1)`.
#[derive(Clone, Debug, PartialEq)]
pub struct ChiSquaredDistribution {
    n: f64,
    gd: GammaDistribution,
}

impl ChiSquaredDistribution {
    /// `chi_squared_distribution(n)`.
    #[must_use]
    pub fn new(n: f64) -> Self {
        Self {
            n,
            gd: GammaDistribution::new(n / 2.0, 1.0),
        }
    }

    /// `n()`.
    #[must_use]
    pub const fn n(&self) -> f64 {
        self.n
    }

    /// `reset()`: resets the internal gamma distribution.
    pub fn reset(&mut self) {
        self.gd.reset();
    }

    /// One draw.
    pub fn sample<G: UniformRandomBitGenerator>(&mut self, g: &mut G) -> f64 {
        2.0 * self.gd.sample(g)
    }

    /// `operator<<`: `n <gamma>`.
    pub fn write_params(&self, out: &mut String) {
        put_real(out, self.n);
        out.push(' ');
        self.gd.write_params(out);
    }
}

/* ------------------------------------------------------------------------ */

/// `std::cauchy_distribution<double>(a, b)`: `a + b · tan(π · u)`, with
/// `u = 0.5` rejected.
#[derive(Clone, Debug, PartialEq)]
pub struct CauchyDistribution {
    a: f64,
    b: f64,
}

impl CauchyDistribution {
    /// `cauchy_distribution(a, b)`.
    #[must_use]
    pub const fn new(a: f64, b: f64) -> Self {
        Self { a, b }
    }

    /// `a()`.
    #[must_use]
    pub const fn a(&self) -> f64 {
        self.a
    }

    /// `b()`.
    #[must_use]
    pub const fn b(&self) -> f64 {
        self.b
    }

    /// `reset()` (no state).
    pub fn reset(&mut self) {}

    /// One draw.
    pub fn sample<G: UniformRandomBitGenerator>(&mut self, g: &mut G) -> f64 {
        let mut u;
        loop {
            u = generate_canonical(g);
            if u != 0.5 {
                break;
            }
        }
        self.a + self.b * tan(PI * u)
    }

    /// `operator<<`: `a b`.
    pub fn write_params(&self, out: &mut String) {
        put_real(out, self.a);
        put_sep_real(out, self.b);
    }
}

/* ------------------------------------------------------------------------ */

/// `std::fisher_f_distribution<double>(m, n)`: `(X · n) / (Y · m)` with
/// `X ~ Gamma(m / 2, 1)` drawn before `Y ~ Gamma(n / 2, 1)`.
#[derive(Clone, Debug, PartialEq)]
pub struct FisherFDistribution {
    m: f64,
    n: f64,
    gd_x: GammaDistribution,
    gd_y: GammaDistribution,
}

impl FisherFDistribution {
    /// `fisher_f_distribution(m, n)`.
    #[must_use]
    pub fn new(m: f64, n: f64) -> Self {
        Self {
            m,
            n,
            gd_x: GammaDistribution::new(m / 2.0, 1.0),
            gd_y: GammaDistribution::new(n / 2.0, 1.0),
        }
    }

    /// `m()`.
    #[must_use]
    pub const fn m(&self) -> f64 {
        self.m
    }

    /// `n()`.
    #[must_use]
    pub const fn n(&self) -> f64 {
        self.n
    }

    /// `reset()`: resets both internal gamma distributions.
    pub fn reset(&mut self) {
        self.gd_x.reset();
        self.gd_y.reset();
    }

    /// One draw.
    pub fn sample<G: UniformRandomBitGenerator>(&mut self, g: &mut G) -> f64 {
        let x = self.gd_x.sample(g);
        let y = self.gd_y.sample(g);
        (x * self.n) / (y * self.m)
    }

    /// `operator<<`: `m n <gamma_x> <gamma_y>`.
    pub fn write_params(&self, out: &mut String) {
        put_real(out, self.m);
        put_sep_real(out, self.n);
        out.push(' ');
        self.gd_x.write_params(out);
        out.push(' ');
        self.gd_y.write_params(out);
    }
}

/* ------------------------------------------------------------------------ */

/// `std::student_t_distribution<double>(n)`: `Z · sqrt(n / G)` with
/// `Z ~ N(0, 1)` drawn before `G ~ Gamma(n / 2, 2)`.
#[derive(Clone, Debug, PartialEq)]
pub struct StudentTDistribution {
    n: f64,
    nd: NormalDistribution,
    gd: GammaDistribution,
}

impl StudentTDistribution {
    /// `student_t_distribution(n)`.
    #[must_use]
    pub fn new(n: f64) -> Self {
        Self {
            n,
            nd: NormalDistribution::standard(),
            gd: GammaDistribution::new(n / 2.0, 2.0),
        }
    }

    /// `n()`.
    #[must_use]
    pub const fn n(&self) -> f64 {
        self.n
    }

    /// `reset()`: resets the internal distributions.
    pub fn reset(&mut self) {
        self.nd.reset();
        self.gd.reset();
    }

    /// One draw.
    pub fn sample<G: UniformRandomBitGenerator>(&mut self, g: &mut G) -> f64 {
        let z = self.nd.sample(g);
        let gamma = self.gd.sample(g);
        z * (self.n / gamma).sqrt()
    }

    /// `operator<<`: `n <normal> <gamma>`.
    pub fn write_params(&self, out: &mut String) {
        put_real(out, self.n);
        out.push(' ');
        self.nd.write_params(out);
        out.push(' ');
        self.gd.write_params(out);
    }
}

/* ------------------------------------------------------------------------ */

/// `std::weibull_distribution<double>(a, b)` (a = shape, b = scale):
/// `b · (−log(1 − u))^(1/a)`.
#[derive(Clone, Debug, PartialEq)]
pub struct WeibullDistribution {
    a: f64,
    b: f64,
}

impl WeibullDistribution {
    /// `weibull_distribution(a, b)`.
    #[must_use]
    pub const fn new(a: f64, b: f64) -> Self {
        Self { a, b }
    }

    /// `a()` (shape).
    #[must_use]
    pub const fn a(&self) -> f64 {
        self.a
    }

    /// `b()` (scale).
    #[must_use]
    pub const fn b(&self) -> f64 {
        self.b
    }

    /// `reset()` (no state).
    pub fn reset(&mut self) {}

    /// One draw.
    pub fn sample<G: UniformRandomBitGenerator>(&mut self, g: &mut G) -> f64 {
        let u = generate_canonical(g);
        self.b * pow(-log(1.0 - u), 1.0 / self.a)
    }

    /// `operator<<`: `a b`.
    pub fn write_params(&self, out: &mut String) {
        put_real(out, self.a);
        put_sep_real(out, self.b);
    }
}

/* ------------------------------------------------------------------------ */

/// `std::extreme_value_distribution<double>(a, b)`:
/// `a − b · log(−log(1 − u))`.
#[derive(Clone, Debug, PartialEq)]
pub struct ExtremeValueDistribution {
    a: f64,
    b: f64,
}

impl ExtremeValueDistribution {
    /// `extreme_value_distribution(a, b)`.
    #[must_use]
    pub const fn new(a: f64, b: f64) -> Self {
        Self { a, b }
    }

    /// `a()` (location).
    #[must_use]
    pub const fn a(&self) -> f64 {
        self.a
    }

    /// `b()` (scale).
    #[must_use]
    pub const fn b(&self) -> f64 {
        self.b
    }

    /// `reset()` (no state).
    pub fn reset(&mut self) {}

    /// One draw.
    pub fn sample<G: UniformRandomBitGenerator>(&mut self, g: &mut G) -> f64 {
        let u = generate_canonical(g);
        self.a - self.b * log(-log(1.0 - u))
    }

    /// `operator<<`: `a b`.
    pub fn write_params(&self, out: &mut String) {
        put_real(out, self.a);
        put_sep_real(out, self.b);
    }
}
