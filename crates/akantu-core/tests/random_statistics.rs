// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs (port of Akantu, EPFL/LSMS): statistical sanity checks of
// the distributions behind src/common/aka_random_generator.hh, independent of
// the bitwise oracle fixtures.

//! Statistical sanity tests of the random module.
//!
//! Each test draws N = 200 000 values with a fixed seed (deterministic) and
//! checks, against the analytical distribution:
//!
//! * the sample mean within 5 standard errors (σ/√N);
//! * the sample variance within 5 standard errors of the variance estimator,
//!   σ²·√((κ − 1)/N) with κ the kurtosis;
//! * where the CDF is elementary, the Kolmogorov–Smirnov distance
//!   D_N < 1.95/√N (asymptotic two-sided level α ≈ 0.001).
//!
//! A correct sampler fails such a check with probability ≲ 10⁻³ per check
//! for a random seed; with the fixed seeds used here the outcome is
//! deterministic. These checks would catch wrong parameterisations (e.g.
//! shape/scale swaps, rate vs mean) that the bitwise fixtures cannot explain.

use akantu_validation::reduce::NanMinMax;
use std::f64::consts::PI;

use akantu_core::random::distributions::*;
use akantu_core::random::{
    Distribution, MinstdRand0, RandomDistributionType, RandomParameter, generate_canonical,
};

const N: usize = 200_000;

fn draw(d: &mut Distribution, seed: u64) -> Vec<f64> {
    let mut e = MinstdRand0::with_seed(seed);
    (0..N).map(|_| d.sample(&mut e)).collect()
}

#[allow(clippy::cast_precision_loss)] // sample sizes <= N = 200 000 < 2^53: exact
fn moments(v: &[f64]) -> (f64, f64) {
    let n = v.len() as f64;
    let mean = v.iter().sum::<f64>() / n;
    let var = v.iter().map(|x| (x - mean) * (x - mean)).sum::<f64>() / (n - 1.0);
    (mean, var)
}

#[allow(clippy::cast_precision_loss)] // sample sizes <= N = 200 000 < 2^53: exact
fn check_moments(name: &str, v: &[f64], mean: f64, var: f64, kurtosis: f64) {
    let n = v.len() as f64;
    let (m, s2) = moments(v);
    let se_mean = (var / n).sqrt();
    assert!(
        (m - mean).abs() < 5.0 * se_mean,
        "{name}: mean {m} vs {mean} (5 SE = {})",
        5.0 * se_mean
    );
    let se_var = var * ((kurtosis - 1.0) / n).sqrt();
    assert!(
        (s2 - var).abs() < 5.0 * se_var,
        "{name}: variance {s2} vs {var} (5 SE = {})",
        5.0 * se_var
    );
}

#[allow(clippy::cast_precision_loss)] // indices and sizes <= N = 200 000 < 2^53: exact
fn check_ks(name: &str, v: &[f64], cdf: impl Fn(f64) -> f64) {
    // a NaN sample would sort to an end and could pass the KS distance
    // through a clamped CDF (`x.max(0.0)`); the distance itself propagates
    // NaN
    assert!(v.iter().all(|x| x.is_finite()), "{name}: non-finite sample");
    let mut s = v.to_vec();
    s.sort_by(f64::total_cmp);
    let n = s.len() as f64;
    let mut d: f64 = 0.0;
    for (i, &x) in s.iter().enumerate() {
        let f = cdf(x);
        d = d
            .nan_max((f - i as f64 / n).abs())
            .nan_max(((i + 1) as f64 / n - f).abs());
    }
    let limit = 1.95 / n.sqrt();
    assert!(d < limit, "{name}: KS distance {d} >= {limit}");
}

#[test]
fn canonical_is_uniform() {
    let mut e = MinstdRand0::with_seed(11);
    let v: Vec<f64> = (0..N).map(|_| generate_canonical(&mut e)).collect();
    check_moments("canonical", &v, 0.5, 1.0 / 12.0, 1.8);
    check_ks("canonical", &v, |x| x.clamp(0.0, 1.0));
    assert!(v.iter().all(|&u| (0.0..1.0).contains(&u)));
}

#[test]
fn uniform() {
    let (a, b) = (-2.5, 7.25);
    let mut d = Distribution::Uniform(UniformRealDistribution::new(a, b));
    let v = draw(&mut d, 1);
    check_moments("uniform", &v, (a + b) / 2.0, (b - a) * (b - a) / 12.0, 1.8);
    check_ks("uniform", &v, |x| ((x - a) / (b - a)).clamp(0.0, 1.0));
}

#[test]
fn exponential() {
    let l = 3.7;
    let mut d = Distribution::Exponential(ExponentialDistribution::new(l));
    let v = draw(&mut d, 2);
    check_moments("exponential", &v, 1.0 / l, 1.0 / (l * l), 9.0);
    check_ks("exponential", &v, |x| 1.0 - (-l * x.max(0.0)).exp());
}

#[test]
fn normal() {
    let (mu, sigma) = (10.0, 0.5);
    let mut d = Distribution::Normal(NormalDistribution::new(mu, sigma));
    let v = draw(&mut d, 3);
    check_moments("normal", &v, mu, sigma * sigma, 3.0);
    // P(|X - mu| < sigma) = erf(1/sqrt 2) = 0.682689492137...
    #[allow(clippy::cast_precision_loss)] // counts <= N = 200 000 < 2^53: exact
    let inside = v.iter().filter(|&&x| (x - mu).abs() < sigma).count() as f64 / N as f64;
    let p = 0.682_689_492_137_086;
    #[allow(clippy::cast_precision_loss)] // N = 200 000 < 2^53: exact
    let se = (p * (1.0 - p) / N as f64).sqrt();
    assert!(
        (inside - p).abs() < 5.0 * se,
        "normal: 1-sigma fraction {inside}"
    );
}

#[test]
fn lognormal_has_normal_logarithm() {
    let (m, s) = (1.5, 0.25);
    let mut d = Distribution::Lognormal(LognormalDistribution::new(m, s));
    let v: Vec<f64> = draw(&mut d, 4).into_iter().map(f64::ln).collect();
    check_moments("ln(lognormal)", &v, m, s * s, 3.0);
}

#[test]
fn gamma() {
    for (alpha, beta, seed) in [(2.5, 1.5, 5), (0.5, 2.0, 6), (9.0, 0.5, 7)] {
        let mut d = Distribution::Gamma(GammaDistribution::new(alpha, beta));
        let v = draw(&mut d, seed);
        check_moments(
            &format!("gamma({alpha}, {beta})"),
            &v,
            alpha * beta,
            alpha * beta * beta,
            3.0 + 6.0 / alpha,
        );
    }
}

#[test]
fn weibull_shape_scale() {
    // std order: a = shape k, b = scale lambda.
    let (k, lambda) = (2.0, 3.5);
    let mut d = Distribution::Weibull(WeibullDistribution::new(k, lambda));
    let v = draw(&mut d, 8);
    check_ks("weibull", &v, |x| {
        1.0 - (-(x.max(0.0) / lambda).powf(k)).exp()
    });
    // Akantu's make_random_parameter takes [scale, shape].
    let mut p = RandomParameter::make(0.0, RandomDistributionType::Weibull, lambda, k);
    let mut e = MinstdRand0::with_seed(8);
    let w: Vec<f64> = (0..N).map(|_| p.sample(&mut e)).collect();
    assert_eq!(v, w);
}

#[test]
fn extreme_value() {
    let (a, b) = (-1.5, 2.5);
    let mut d = Distribution::ExtremeValue(ExtremeValueDistribution::new(a, b));
    let v = draw(&mut d, 9);
    let euler_gamma = 0.577_215_664_901_532_9;
    check_moments(
        "extreme_value",
        &v,
        a + b * euler_gamma,
        PI * PI * b * b / 6.0,
        5.4,
    );
    check_ks("extreme_value", &v, |x| (-(-(x - a) / b).exp()).exp());
}

#[test]
fn chi_squared() {
    for (n, seed) in [(1.0, 10), (10.0, 11)] {
        let mut d = Distribution::ChiSquared(ChiSquaredDistribution::new(n));
        let v = draw(&mut d, seed);
        check_moments(&format!("chi_squared({n})"), &v, n, 2.0 * n, 3.0 + 12.0 / n);
    }
}

#[test]
fn cauchy() {
    let (a, b) = (2.0, 0.5);
    let mut d = Distribution::Cauchy(CauchyDistribution::new(a, b));
    let v = draw(&mut d, 12);
    check_ks("cauchy", &v, |x| 0.5 + ((x - a) / b).atan() / PI);
}

#[test]
fn fisher_f() {
    let (m, n) = (5.0, 20.0);
    let mut d = Distribution::FisherF(FisherFDistribution::new(m, n));
    let v = draw(&mut d, 13);
    let mean = n / (n - 2.0);
    let var = 2.0 * n * n * (m + n - 2.0) / (m * (n - 2.0) * (n - 2.0) * (n - 4.0));
    let excess = 12.0 * (m * (5.0 * n - 22.0) * (m + n - 2.0) + (n - 4.0) * (n - 2.0) * (n - 2.0))
        / (m * (n - 6.0) * (n - 8.0) * (m + n - 2.0));
    check_moments("fisher_f", &v, mean, var, 3.0 + excess);
}

#[test]
fn student_t() {
    let nu = 10.0;
    let mut d = Distribution::StudentT(StudentTDistribution::new(nu));
    let v = draw(&mut d, 14);
    check_moments(
        "student_t",
        &v,
        0.0,
        nu / (nu - 2.0),
        3.0 + 6.0 / (nu - 4.0),
    );
}

#[test]
fn default_parameter_is_constant_but_consumes_the_engine() {
    let mut p = RandomParameter::new(94e3);
    let mut e = MinstdRand0::with_seed(15);
    let mut out = vec![0.0; 10];
    p.set_values_with(&mut e, &mut out);
    assert!(out.iter().all(|&x| x.to_bits() == 94e3_f64.to_bits()));
    let mut f = MinstdRand0::with_seed(15);
    f.discard(20);
    assert_eq!(e, f);
}
