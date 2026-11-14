// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/common/aka_random_generator.hh
// (RandomDistributionType, RandomDistributionTypeHelper, RandomDistribution,
// RandomDistributionProxy, RandomParameter, make_random_parameter).

//! `RandomDistributionType`, type-erased distributions and `RandomParameter`.

use std::fmt;
use std::str::FromStr;

use super::distributions_impl::{
    CauchyDistribution, ChiSquaredDistribution, ExponentialDistribution, ExtremeValueDistribution,
    FisherFDistribution, GammaDistribution, LognormalDistribution, NormalDistribution,
    StudentTDistribution, UniformRealDistribution, WeibullDistribution,
};
use super::engine::{MinstdRand0, UniformRandomBitGenerator};
use super::fmt_c::ostream_default;
use super::global::{IdxSlot, RandomGenerator};
use crate::error::{Error, Result};

/// Upstream `RandomDistributionType` (`AKANTU_RANDOM_DISTRIBUTION_TYPES`),
/// in upstream order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RandomDistributionType {
    /// `_uniform`
    Uniform,
    /// `_exponential`
    Exponential,
    /// `_gamma`
    Gamma,
    /// `_weibull`
    Weibull,
    /// `_extreme_value`
    ExtremeValue,
    /// `_normal`
    Normal,
    /// `_lognormal`
    Lognormal,
    /// `_chi_squared`
    ChiSquared,
    /// `_cauchy`
    Cauchy,
    /// `_fisher_f`
    FisherF,
    /// `_student_t`
    StudentT,
    /// `_not_defined`
    NotDefined,
}

impl RandomDistributionType {
    /// All variants in upstream order.
    pub const ALL: [Self; 12] = [
        Self::Uniform,
        Self::Exponential,
        Self::Gamma,
        Self::Weibull,
        Self::ExtremeValue,
        Self::Normal,
        Self::Lognormal,
        Self::ChiSquared,
        Self::Cauchy,
        Self::FisherF,
        Self::StudentT,
        Self::NotDefined,
    ];

    /// Upstream `std::to_string(RandomDistributionType)` (`"uniform"`, …,
    /// `"not_defined"`); also the keyword of the `.dat` grammar.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Uniform => "uniform",
            Self::Exponential => "exponential",
            Self::Gamma => "gamma",
            Self::Weibull => "weibull",
            Self::ExtremeValue => "extreme_value",
            Self::Normal => "normal",
            Self::Lognormal => "lognormal",
            Self::ChiSquared => "chi_squared",
            Self::Cauchy => "cauchy",
            Self::FisherF => "fisher_f",
            Self::StudentT => "student_t",
            Self::NotDefined => "not_defined",
        }
    }
}

impl fmt::Display for RandomDistributionType {
    /// Upstream `operator<<(RandomDistributionType)`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

impl FromStr for RandomDistributionType {
    type Err = Error;

    /// Exact lookup of one token, the upstream name (`"uniform"`, …,
    /// `"not_defined"`), like upstream's
    /// `AKANTU_CLASS_ENUM_INPUT_STREAM(RandomDistributionType, …)`
    /// (`aka_random_generator.hh:43`): a class enum's keys are the bare
    /// names, so `"_uniform"` is rejected;
    /// stream tokenisation (whitespace) is the caller's job
    /// (the parser hands over the bare word).
    fn from_str(s: &str) -> Result<Self> {
        Self::ALL
            .into_iter()
            .find(|t| t.name() == s)
            .ok_or_else(|| {
                Error::InvalidArgument(format!("unknown random distribution type '{s}'"))
            })
    }
}

/// A distribution with its state (upstream `RandomDistributionProxy<Real, D>`
/// around a `std::*_distribution<Real>`). `Clone` copies the state, like the
/// C++ copy constructor used by `make_unique()`.
#[derive(Clone, Debug, PartialEq)]
pub enum Distribution {
    /// `std::uniform_real_distribution`
    Uniform(UniformRealDistribution),
    /// `std::exponential_distribution`
    Exponential(ExponentialDistribution),
    /// `std::gamma_distribution`
    Gamma(GammaDistribution),
    /// `std::weibull_distribution`
    Weibull(WeibullDistribution),
    /// `std::extreme_value_distribution`
    ExtremeValue(ExtremeValueDistribution),
    /// `std::normal_distribution`
    Normal(NormalDistribution),
    /// `std::lognormal_distribution`
    Lognormal(LognormalDistribution),
    /// `std::chi_squared_distribution`
    ChiSquared(ChiSquaredDistribution),
    /// `std::cauchy_distribution`
    Cauchy(CauchyDistribution),
    /// `std::fisher_f_distribution`
    FisherF(FisherFDistribution),
    /// `std::student_t_distribution`
    StudentT(StudentTDistribution),
}

macro_rules! each_distribution {
    ($self:expr, $d:ident => $body:expr) => {
        match $self {
            Distribution::Uniform($d) => $body,
            Distribution::Exponential($d) => $body,
            Distribution::Gamma($d) => $body,
            Distribution::Weibull($d) => $body,
            Distribution::ExtremeValue($d) => $body,
            Distribution::Normal($d) => $body,
            Distribution::Lognormal($d) => $body,
            Distribution::ChiSquared($d) => $body,
            Distribution::Cauchy($d) => $body,
            Distribution::FisherF($d) => $body,
            Distribution::StudentT($d) => $body,
        }
    };
}

impl Distribution {
    /// Upstream `RandomDistributionTypeHelper<Real, D>::value`.
    #[must_use]
    pub const fn distribution_type(&self) -> RandomDistributionType {
        match self {
            Self::Uniform(_) => RandomDistributionType::Uniform,
            Self::Exponential(_) => RandomDistributionType::Exponential,
            Self::Gamma(_) => RandomDistributionType::Gamma,
            Self::Weibull(_) => RandomDistributionType::Weibull,
            Self::ExtremeValue(_) => RandomDistributionType::ExtremeValue,
            Self::Normal(_) => RandomDistributionType::Normal,
            Self::Lognormal(_) => RandomDistributionType::Lognormal,
            Self::ChiSquared(_) => RandomDistributionType::ChiSquared,
            Self::Cauchy(_) => RandomDistributionType::Cauchy,
            Self::FisherF(_) => RandomDistributionType::FisherF,
            Self::StudentT(_) => RandomDistributionType::StudentT,
        }
    }

    /// Draw one value (`distribution(generator)`).
    pub fn sample<G: UniformRandomBitGenerator>(&mut self, g: &mut G) -> f64 {
        each_distribution!(self, d => d.sample(g))
    }

    /// `reset()` (drops cached normal values).
    pub fn reset(&mut self) {
        each_distribution!(self, d => d.reset());
    }

    /// The text the oracle's `operator<<` writes for this distribution
    /// (parameters and internal state, `scientific`, 17 digits).
    #[must_use]
    pub fn params_string(&self) -> String {
        let mut out = String::new();
        each_distribution!(self, d => d.write_params(&mut out));
        out
    }

    /// Checks exactly the preconditions the oracle's standard library
    /// asserts when built with `_GLIBCXX_ASSERTIONS` (determined by
    /// black-box experiment, `random/CLEANROOM_LOG.md` E3): uniform
    /// `a <= b`; exponential `lambda > 0`; normal `stddev > 0`; gamma
    /// `alpha > 0`; chi-squared and Student-t `n / 2 > 0`; Fisher-F
    /// `m / 2 > 0` and `n / 2 > 0` (the halved values are the shapes of the
    /// nested gamma distributions, so e.g. `n` = the smallest subnormal
    /// fails). Lognormal, Weibull, extreme-value and Cauchy parameters, the
    /// normal mean and the gamma `beta` are not checked. The oracle is built
    /// without `_GLIBCXX_ASSERTIONS`, so upstream silently samples such
    /// parameters (producing NaN/inf or meaningless values), and so does
    /// this port; this check is available to callers that want to reject
    /// them.
    ///
    /// # Errors
    /// [`Error::InvalidArgument`] naming the violated precondition.
    #[allow(clippy::neg_cmp_op_on_partial_ord)] // NaN parameters must fail the checks
    pub fn check_parameters(&self) -> Result<()> {
        let fail = |what: &str| -> Result<()> {
            Err(Error::InvalidArgument(format!(
                "{} distribution: precondition {what} violated ({})",
                self.distribution_type(),
                self.params_string()
            )))
        };
        match self {
            Self::Uniform(d) if !(d.a() <= d.b()) => fail("a <= b"),
            Self::Exponential(d) if !(d.lambda() > 0.0) => fail("lambda > 0"),
            Self::Normal(d) if !(d.stddev() > 0.0) => fail("stddev > 0"),
            Self::Gamma(d) if !(d.alpha() > 0.0) => fail("alpha > 0"),
            Self::ChiSquared(d) if !(d.n() / 2.0 > 0.0) => fail("n / 2 > 0"),
            Self::StudentT(d) if !(d.n() / 2.0 > 0.0) => fail("n / 2 > 0"),
            Self::FisherF(d) if !(d.m() / 2.0 > 0.0) => fail("m / 2 > 0"),
            Self::FisherF(d) if !(d.n() / 2.0 > 0.0) => fail("n / 2 > 0"),
            _ => Ok(()),
        }
    }
}

impl fmt::Display for Distribution {
    /// Upstream `RandomDistributionProxy::printself`: `<type> [ <params> ]`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} [ {} ]",
            self.distribution_type(),
            self.params_string()
        )
    }
}

/// Upstream `RandomParameter<Real>`: `base_value + distribution(generator)`.
///
/// `Clone` copies the distribution *with its state* (upstream copy
/// constructor / assignment via `make_unique()`).
#[derive(Clone, Debug, PartialEq)]
pub struct RandomParameter {
    base_value: f64,
    distribution: Distribution,
}

impl RandomParameter {
    /// `RandomParameter(base_value)`: distribution `uniform [0, 0]`.
    ///
    /// Note: drawing from it still consumes two engine calls per value.
    #[must_use]
    pub const fn new(base_value: f64) -> Self {
        Self {
            base_value,
            distribution: Distribution::Uniform(UniformRealDistribution::new(0.0, 0.0)),
        }
    }

    /// `RandomParameter(base_value, dist)`.
    #[must_use]
    pub const fn with_distribution(base_value: f64, distribution: Distribution) -> Self {
        Self {
            base_value,
            distribution,
        }
    }

    /// Upstream `make_random_parameter(base, type, a, b)` (missing
    /// parameters are `T()` = 0.0, as when the parser passes fewer values).
    ///
    /// Parameter mapping (upstream order, `.dat` syntax `base type [a, b]`):
    /// `uniform(a, b)`, `exponential(a)`, `gamma(a, b)`,
    /// **`weibull` → `std::weibull_distribution(b, a)`** (a = scale λ,
    /// b = shape k), `extreme_value(a, b)`, `normal(a, b)`, `lognormal(a, b)`,
    /// `chi_squared(a)`, `cauchy(a, b)`, `fisher_f(a, b)`, `student_t(a)`;
    /// `not_defined` → `uniform(0, 0)`.
    #[must_use]
    pub fn make(base: f64, ty: RandomDistributionType, a: f64, b: f64) -> Self {
        use RandomDistributionType as T;
        let d = match ty {
            T::NotDefined => Distribution::Uniform(UniformRealDistribution::new(0.0, 0.0)),
            T::Uniform => Distribution::Uniform(UniformRealDistribution::new(a, b)),
            T::Exponential => Distribution::Exponential(ExponentialDistribution::new(a)),
            T::Gamma => Distribution::Gamma(GammaDistribution::new(a, b)),
            T::Weibull => Distribution::Weibull(WeibullDistribution::new(b, a)),
            T::ExtremeValue => Distribution::ExtremeValue(ExtremeValueDistribution::new(a, b)),
            T::Normal => Distribution::Normal(NormalDistribution::new(a, b)),
            T::Lognormal => Distribution::Lognormal(LognormalDistribution::new(a, b)),
            T::ChiSquared => Distribution::ChiSquared(ChiSquaredDistribution::new(a)),
            T::Cauchy => Distribution::Cauchy(CauchyDistribution::new(a, b)),
            T::FisherF => Distribution::FisherF(FisherFDistribution::new(a, b)),
            T::StudentT => Distribution::StudentT(StudentTDistribution::new(a)),
        };
        Self::with_distribution(base, d)
    }

    /// `make_random_parameter` from a parameter list as produced by the
    /// parser (`Parser::parseRandomParameter`): 0, 1 or 2 values are passed
    /// on, any other count behaves like 0 values.
    #[must_use]
    pub fn from_parsed(base: f64, ty: RandomDistributionType, params: &[f64]) -> Self {
        match *params {
            [a] => Self::make(base, ty, a, 0.0),
            [a, b] => Self::make(base, ty, a, b),
            _ => Self::make(base, ty, 0.0, 0.0),
        }
    }

    /// `getBaseValue()`.
    #[must_use]
    pub const fn base_value(&self) -> f64 {
        self.base_value
    }

    /// `setBaseValue()`.
    pub fn set_base_value(&mut self, value: f64) {
        self.base_value = value;
    }

    /// The distribution type (upstream member `type`).
    #[must_use]
    pub const fn distribution_type(&self) -> RandomDistributionType {
        self.distribution.distribution_type()
    }

    /// The distribution, with its current state.
    #[must_use]
    pub const fn distribution(&self) -> &Distribution {
        &self.distribution
    }

    /// One draw: `base_value + distribution(g)`.
    pub fn sample<G: UniformRandomBitGenerator>(&mut self, g: &mut G) -> f64 {
        self.base_value + self.distribution.sample(g)
    }

    /// Upstream `setValues(it, end)` with an explicit engine: fills `out` in
    /// index order, one draw per element.
    pub fn set_values_with<G: UniformRandomBitGenerator>(&mut self, g: &mut G, out: &mut [f64]) {
        for v in out {
            *v = self.sample(g);
        }
    }

    /// Upstream `setValues(it, end)`: draws from the process-global
    /// `RandomGenerator<Idx>` engine.
    pub fn set_values(&mut self, out: &mut [f64]) {
        RandomGenerator::<IdxSlot>::with_engine(|e: &mut MinstdRand0| self.set_values_with(e, out));
    }
}

impl Default for RandomParameter {
    /// `RandomParameter{T()}` (base 0, `uniform [0, 0]`), the member
    /// initialiser of `RandomInternalField`.
    fn default() -> Self {
        Self::new(0.0)
    }
}

impl fmt::Display for RandomParameter {
    /// Upstream `printself`: `<base> + <type> [ <params> ]`, e.g.
    /// `94000 + uniform [ 0.00000000000000000e+00 9.40000000000000000e+03 ]`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} + {}",
            ostream_default(self.base_value),
            self.distribution
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Upstream's class-enum
    /// `operator>>` (`AKANTU_CLASS_ENUM_INPUT_STREAM`) looks the bare names
    /// up exactly; `_uniform` and decorated spellings are rejected.
    #[test]
    fn distribution_type_parsing_is_exact_like_upstream() {
        assert_eq!(RandomDistributionType::ALL.len(), 12);
        for t in RandomDistributionType::ALL {
            assert_eq!(t.name().parse::<RandomDistributionType>().unwrap(), t);
            assert_eq!(t.to_string(), t.name());
            for bad in [
                format!("_{}", t.name()),
                format!(" {}", t.name()),
                format!("{} ", t.name()),
                t.name().to_uppercase(),
            ] {
                assert!(
                    bad.parse::<RandomDistributionType>().is_err(),
                    "`{bad}` accepted"
                );
            }
        }
        assert!("".parse::<RandomDistributionType>().is_err());
    }
}
