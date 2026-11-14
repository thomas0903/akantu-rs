// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/model/solid_mechanics/materials/
// weight_functions/{damaged_weight_function.{hh,cc},
// damaged_weight_function_inline_impl.hh, remove_damaged_weight_function.{hh,cc},
// remove_damaged_weight_function_inline_impl.hh,
// remove_damaged_with_damage_rate_weight_function{.hh,_inline_impl.hh},
// stress_based_weight_function.{hh,cc}, stress_based_weight_function_inline_impl.hh}.

//! Damage-dependent weight functions (NL-005…008).
//!
//! | Type | Upstream class | Registered as |
//! |---|---|---|
//! | [`DamagedWeightFunction`] | `DamagedWeightFunction` | `damage_wf` |
//! | [`RemoveDamagedWeightFunction`] | `RemoveDamagedWeightFunction` | `remove_wf` |
//! | [`RemoveDamagedWithDamageRateWeightFunction`] | `RemoveDamagedWithDamageRateWeightFunction` | not registered upstream (header only); available programmatically |
//! | [`StressBasedWeightFunction`] | `StressBasedWeightFunction` | not registered upstream; a stub returning 0 (all computations commented out upstream) |
//!
//! The damage values are the manager-level weight-function internal
//! `"damage"` (upstream `registerWeightFunctionInternal("damage")`),
//! flattened from every law with a `damage` internal before each weight
//! update; the value of `q2` (owned or ghost) is read at its slot.

use std::sync::Arc;

use akantu_core::{Error, Int, Real, Result};
use akantu_model::non_local::{
    HasWeightBase, PointRef, WeightArithmetic, WeightFunction, WeightFunctionBase,
    WeightFunctionFactory, WeightInternals, set_update_rate,
};
use akantu_parser::param::{
    ParamDescriptor, ParamInfo, ParamKind, ParamValue, ParameterAccess, table_get, table_info,
    table_names, table_set, table_set_from_input,
};
use akantu_parser::{ParameterRegistry, Parsable, ParserType};

/// Copy the named internal (one value per slot) out of the manager's
/// internals.
fn internal_values(internals: &WeightInternals, name: &str) -> Result<Vec<Real>> {
    internals
        .get(name)
        .cloned()
        .ok_or_else(|| Error::not_found("weight-function internal", name))
}

/// Upstream `Math::getTolerance()` (default `100·ε`).
const TOLERANCE: Real = akantu_core::math::DEFAULT_TOLERANCE;

macro_rules! weight_registry {
    ($ty:ty, $id:literal) => {
        impl HasWeightBase for $ty {
            fn wf_base_mut(&mut self) -> &mut WeightFunctionBase {
                &mut self.base
            }
        }
        impl ParameterRegistry for $ty {
            fn get_param(&self, name: &str) -> Result<ParamValue> {
                table_get(Self::PARAMS, self, name)
            }
            fn set_param(&mut self, name: &str, value: ParamValue) -> Result<()> {
                table_set(Self::PARAMS, self, name, value)
            }
            fn param_names(&self) -> Vec<&'static str> {
                table_names(Self::PARAMS)
            }
            fn param_info(&self, name: &str) -> Option<ParamInfo> {
                table_info(Self::PARAMS, name)
            }
            fn set_param_from_input(&mut self, name: &str, value: ParamValue) -> Result<()> {
                table_set_from_input(Self::PARAMS, self, name, value)
            }
        }
        impl Parsable for $ty {
            fn section_type(&self) -> ParserType {
                ParserType::WeightFunction
            }
            fn parsable_id(&self) -> &str {
                $id
            }
        }
    };
}

fn set_damage_limit(v: ParamValue) -> Result<Real> {
    match v {
        ParamValue::Real(x) => Ok(x),
        #[allow(clippy::cast_precision_loss)] // upstream Real parameter given as an integer
        ParamValue::Int(i) => Ok(i as Real),
        other => Err(Error::invalid(format!(
            "damage_limit expects a Real, got {other:?}"
        ))),
    }
}

/// `damage_wf` (NL-005, upstream `DamagedWeightFunction::operator()`).
///
/// Upstream, with `D = damage(q2)` and `R² = R·R`: `R_init = R²`, `R_t =
/// R_init·(1 − D)`, then `R_init ← R_init²`, `R_t ← R_t²` (`R_t = 0.001·
/// R_init` if `R_t < tolerance`); `x = 0.49·R_t/R_init`, `expb = 2 ln 0.51
/// / ln(1 − x)`, `Int expb_floor = ⌊expb⌋`, `b = expb_floor + expb_floor
/// mod 2` (`Int` arithmetic), `W = max(0, 1 − r²/R_init)^b`. The radius test
/// uses `R⁴` against `r²` (dimensionally inconsistent; reproduced as coded,
/// UD-557 formula part, category C).
///
/// **Intent.** The interaction radius of a damaged point shrinks: the
/// relative radius `ρ = R_t/R_init = (1 − D)²` sets an even exponent `b`
/// such that `W` at `r² = 0.49·ρ·R_init` equals `0.51²`, the value of the
/// undamaged bell `(1 − r²/R_init)²` at `r² = 0.49·R_init` (at `D = 0`,
/// `expb = 2` and `W` is that bell). As `D → 1⁻`, `ρ → 0⁺`, `expb ≈
/// 2.748/ρ → +∞` and `W` tends to 1 at `r = 0` and to 0 elsewhere: a
/// fully damaged point keeps only itself.
///
/// **UD-557 (conversion, category B, D-062 (3)).** `Int expb_floor =
/// ⌊expb⌋` is undefined for a NaN or out-of-range value, and the `Int` sum
/// overflows at `⌊expb⌋ = 2³¹ − 1`. This happens for ordinary damages:
/// `expb ≥ 2³¹ − 1` once `1 − D ≲ 3.58e-5`; `1 − x` rounds to 1 once
/// `x ≤ 2⁻⁵⁴` (`ln 1 = +0`, `expb = −∞`; reached when `R ≥ 3.74`, e.g. the
/// default radius 100 with `1 − D ∈ [1.5e-11, 1.06e-8]`); `1 − x < 0` for
/// `D < −0.43` (`expb = NaN`). The x86_64 oracle converts all of these to
/// `INT_MIN` (`b = −2³¹`: `+∞` or huge weights, infinite volumes, NaN
/// averages).
///
/// [`WeightArithmetic::Corrected`] (default) computes `b` in floating
/// point, never leaving the `Int` range: `f = ⌊expb⌋`, `b = f + fmod(f,
/// 2)` — identical to upstream for `⌊expb⌋ ∈ [−2³¹, 2³¹ − 2]`, and the
/// same even rounding beyond (exact: every `f64` integer ≥ 2⁵³ is even);
/// for `expb = −∞` (`ln(1 − x)` rounded to `+0`) the logarithm is replaced
/// by `−x` (within one ulp of `ln(1 − x)` for `0 < x ≤ 2⁻⁵⁴`), giving the
/// finite `expb ≥ 2.4e16` it approximates; `expb = +∞` gives `b = +∞`
/// (`W = 1` at `r = 0`, else 0, IEEE `pow`); `1 − x < 0` (finite `D <
/// −0.43` or `D > 2.43`) continues the limit `1 − x → 0⁺` (`expb → +0`,
/// `b = 0`, `W = 1`). `W` is then in `[0, 1]` for every finite `D` and
/// non-increasing in `D` on `[0, 1)` up to the tolerance clamp (which is
/// upstream's, kept); a NaN damage gives a NaN weight (the input's NaN, not
/// one created here). [`WeightArithmetic::UpstreamX86_64`] reproduces the
/// x86_64 oracle bitwise ([`DamagedWeightFunction::exponent`]).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DamagedWeightFunction {
    /// Radius and update rate.
    pub base: WeightFunctionBase,
    /// Arithmetic of the exponent on the domain where upstream's
    /// conversion is undefined (UD-557).
    pub arithmetic: WeightArithmetic,
    damage: Vec<Real>,
}

impl DamagedWeightFunction {
    const PARAMS: &'static [ParamDescriptor<Self>] = &[ParamDescriptor::new(
        "update_rate",
        ParamKind::Int,
        ParameterAccess::PARSMOD,
        "Update frequency",
        |s| ParamValue::Int(i64::from(s.base.update_rate)),
        Some(set_update_rate::<Self>),
    )];

    /// The corrected weight ([`WeightArithmetic::Corrected`]) for a damage
    /// `d` of `q2` at distance `r`, radius `R` with `R² = r2`.
    #[must_use]
    pub fn weight_for_damage(r: Real, r2: Real, d: Real) -> Real {
        Self::weight_for_damage_with(r, r2, d, WeightArithmetic::Corrected)
    }

    /// The weight for a damage `d` of `q2` at distance `r`, radius `R` with
    /// `R² = r2`, under `arithmetic` (upstream operand order).
    #[must_use]
    pub fn weight_for_damage_with(
        r: Real,
        r2: Real,
        d: Real,
        arithmetic: WeightArithmetic,
    ) -> Real {
        if d.is_nan() && arithmetic == WeightArithmetic::Corrected {
            return d;
        }
        let (expb, x, radius_init) = Self::exponent_operands(r2, d);
        let b = Self::exponent(expb, x, arithmetic);
        let alpha = super::super::damage::std_max(0., 1. - r * r / radius_init);
        akantu_core::glibc_libm::pow(alpha, b)
    }

    /// Upstream's intermediate quantities for a damage `d` and `R² = r2`:
    /// `(expb, x, R_init)` with `R_init = R⁴`, `x = 0.49·R_t/R_init` (after
    /// the tolerance clamp) and `expb = 2 ln 0.51 / ln(1 − x)` (glibc
    /// `log` port, upstream operand order).
    #[must_use]
    pub fn exponent_operands(r2: Real, d: Real) -> (Real, Real, Real) {
        let mut radius_init = r2;
        let mut radius_t = radius_init * (1. - d);
        radius_init *= radius_init;
        radius_t *= radius_t;
        if radius_t < TOLERANCE {
            radius_t = 0.001 * radius_init;
        }
        let x = 0.49 * radius_t / radius_init;
        let expb =
            (2. * akantu_core::glibc_libm::log(0.51)) / akantu_core::glibc_libm::log(1.0 - x);
        (expb, x, radius_init)
    }

    /// The exponent `b` from upstream's `expb` (and `x`, for the corrected
    /// `expb = −∞` case) under `arithmetic` (type documentation).
    ///
    /// [`WeightArithmetic::UpstreamX86_64`]: `Int expb_floor = ⌊expb⌋` as
    /// x86_64's `cvttsd2si` (a NaN or a value outside `[−2³¹, 2³¹)` gives
    /// `INT_MIN`), then `expb_floor + expb_floor % 2` in wrapping 32-bit
    /// arithmetic (`2³¹ − 1` wraps to `INT_MIN`), converted to `f64`.
    #[must_use]
    pub fn exponent(expb: Real, x: Real, arithmetic: WeightArithmetic) -> Real {
        match arithmetic {
            WeightArithmetic::UpstreamX86_64 => {
                let f = expb.floor();
                let i: Int = if (-2_147_483_648.0..2_147_483_648.0).contains(&f) {
                    #[allow(clippy::cast_possible_truncation)] // range checked, integral value
                    {
                        f as Int
                    }
                } else {
                    Int::MIN // cvttsd2si "integer indefinite" (NaN, ±∞, out of range)
                };
                Real::from(i.wrapping_add(i % 2))
            }
            WeightArithmetic::Corrected => {
                if expb.is_finite() {
                    even_floor(expb)
                } else if expb == Real::NEG_INFINITY {
                    // ln(1 − x) rounded to +0 (0 < x ≤ 2⁻⁵⁴): use −x
                    if x > 0. {
                        let e = (2. * akantu_core::glibc_libm::log(0.51)) / (-x);
                        if e.is_finite() {
                            even_floor(e)
                        } else {
                            Real::INFINITY
                        }
                    } else {
                        Real::INFINITY
                    }
                } else if expb == Real::INFINITY {
                    Real::INFINITY
                } else if x > 1. {
                    // 1 − x < 0 (ln of a negative number): limit 1 − x → 0⁺
                    0.
                } else {
                    Real::NAN // NaN operands (degenerate radius): propagated
                }
            }
        }
    }
}

/// `f + fmod(f, 2)` with `f = ⌊e⌋` for a finite `e`: upstream's
/// `expb_floor + expb_floor % 2` computed in `f64` (exact; equal to the
/// `Int` computation wherever that is defined), `+0` for a zero result.
fn even_floor(e: Real) -> Real {
    let f = e.floor();
    let b = f + f % 2.;
    if b == 0. { 0. } else { b }
}

weight_registry!(DamagedWeightFunction, "weight_function:damaged");

impl WeightFunction for DamagedWeightFunction {
    /// UD-1103: upstream's class packs nothing for `_mnl_weight`.
    fn upstream_packs_weight_data(&self) -> bool {
        false
    }
    fn type_name(&self) -> &str {
        "damaged"
    }
    fn update_rate(&self) -> Int {
        self.base.update_rate
    }
    fn radius(&self) -> Real {
        self.base.r
    }
    fn set_radius(&mut self, radius: Real) {
        self.base.set_radius(radius);
    }
    fn internal_names(&self) -> Vec<String> {
        vec!["damage".into()]
    }
    fn update_internals(&mut self, internals: &WeightInternals) -> Result<()> {
        self.damage = internal_values(internals, "damage")?;
        Ok(())
    }
    fn weight(&self, r: Real, _q1: PointRef<'_>, q2: PointRef<'_>) -> Real {
        let d = self.damage.get(q2.slot).copied().unwrap_or(Real::NAN);
        Self::weight_for_damage_with(r, self.base.r2, d, self.arithmetic)
    }
    fn set_arithmetic(&mut self, arithmetic: WeightArithmetic) {
        self.arithmetic = arithmetic;
    }
}

/// `remove_wf` (NL-006, upstream `RemoveDamagedWeightFunction::operator()`):
/// `W = 1` if `q1` and `q2` are the same point; else, with `D = damage(q2)`,
/// `W = max(0, 1 − r²/R²)²` if `D < damage_limit·(1 − tolerance)`, else 0.
/// Parameter `damage_limit` (default 1, parsable).
#[derive(Clone, Debug, PartialEq)]
pub struct RemoveDamagedWeightFunction {
    /// Radius and update rate.
    pub base: WeightFunctionBase,
    /// `damage_limit`.
    pub damage_limit: Real,
    damage: Vec<Real>,
}

impl Default for RemoveDamagedWeightFunction {
    fn default() -> Self {
        Self {
            base: WeightFunctionBase::default(),
            damage_limit: 1.,
            damage: Vec::new(),
        }
    }
}

impl RemoveDamagedWeightFunction {
    const PARAMS: &'static [ParamDescriptor<Self>] = &[
        ParamDescriptor::new(
            "update_rate",
            ParamKind::Int,
            ParameterAccess::PARSMOD,
            "Update frequency",
            |s| ParamValue::Int(i64::from(s.base.update_rate)),
            Some(set_update_rate::<Self>),
        ),
        ParamDescriptor::new(
            "damage_limit",
            ParamKind::Real,
            ParameterAccess::PARSABLE,
            "Damage Threshold",
            |s| ParamValue::Real(s.damage_limit),
            Some(|s, v| {
                s.damage_limit = set_damage_limit(v)?;
                Ok(())
            }),
        ),
    ];
}

weight_registry!(
    RemoveDamagedWeightFunction,
    "weight_function:remove_damaged"
);

impl WeightFunction for RemoveDamagedWeightFunction {
    fn type_name(&self) -> &str {
        "remove_damaged"
    }
    fn update_rate(&self) -> Int {
        self.base.update_rate
    }
    fn radius(&self) -> Real {
        self.base.r
    }
    fn set_radius(&mut self, radius: Real) {
        self.base.set_radius(radius);
    }
    fn internal_names(&self) -> Vec<String> {
        vec!["damage".into()]
    }
    fn update_internals(&mut self, internals: &WeightInternals) -> Result<()> {
        self.damage = internal_values(internals, "damage")?;
        Ok(())
    }
    fn weight(&self, r: Real, q1: PointRef<'_>, q2: PointRef<'_>) -> Real {
        if q1.point.same_point(q2.point) {
            return 1.;
        }
        let d = self.damage.get(q2.slot).copied().unwrap_or(Real::NAN);
        if d < self.damage_limit * (1. - TOLERANCE) {
            let alpha = super::super::damage::std_max(0., 1. - r * r / self.base.r2);
            return alpha * alpha;
        }
        0.
    }
}

/// `RemoveDamagedWithDamageRateWeightFunction` (NL-007; header only and
/// not registered upstream): `W = 1` for the same `global_num`; else, with
/// `D = damage-rate(q2)`, `W = max(0, 1 − (r²/R²)¹)²` if `D < damage_limit`,
/// else 0 (`remove_damaged_with_damage_rate_weight_function_inline_impl.hh:
/// 46-53`: `alpha = std::max(0., 1. - pow(r * r / R2, 1.))`, `w = pow(alpha,
/// 2.)`). Internal `"damage-rate"`, parameter `damage_limit` (default 1).
///
/// Bit pattern: the upstream code compiled with
/// the oracle's flags gives `pow(x, 1.) = x` and `pow(alpha, 2.) = alpha ·
/// alpha` on the whole sweep of `fixtures/model/non_local/weight_rate_sweep`
/// (`non_local_probe.cc` version 2), which the port computes directly.
#[derive(Clone, Debug, PartialEq)]
pub struct RemoveDamagedWithDamageRateWeightFunction {
    /// Radius and update rate.
    pub base: WeightFunctionBase,
    /// `damage_limit`.
    pub damage_limit: Real,
    damage_rate: Vec<Real>,
}

impl Default for RemoveDamagedWithDamageRateWeightFunction {
    fn default() -> Self {
        Self {
            base: WeightFunctionBase::default(),
            damage_limit: 1.,
            damage_rate: Vec::new(),
        }
    }
}

impl RemoveDamagedWithDamageRateWeightFunction {
    const PARAMS: &'static [ParamDescriptor<Self>] = &[
        ParamDescriptor::new(
            "update_rate",
            ParamKind::Int,
            ParameterAccess::PARSMOD,
            "Update frequency",
            |s| ParamValue::Int(i64::from(s.base.update_rate)),
            Some(set_update_rate::<Self>),
        ),
        ParamDescriptor::new(
            "damage_limit",
            ParamKind::Real,
            ParameterAccess::PARSABLE,
            "Damage Threshold",
            |s| ParamValue::Real(s.damage_limit),
            Some(|s, v| {
                s.damage_limit = set_damage_limit(v)?;
                Ok(())
            }),
        ),
    ];
}

weight_registry!(
    RemoveDamagedWithDamageRateWeightFunction,
    "weight_function:remove_damage_with_damage_rate"
);

impl WeightFunction for RemoveDamagedWithDamageRateWeightFunction {
    /// UD-1103: upstream's class packs nothing for `_mnl_weight`.
    fn upstream_packs_weight_data(&self) -> bool {
        false
    }
    fn type_name(&self) -> &str {
        "remove_damage_with_damage_rate"
    }
    fn update_rate(&self) -> Int {
        self.base.update_rate
    }
    fn radius(&self) -> Real {
        self.base.r
    }
    fn set_radius(&mut self, radius: Real) {
        self.base.set_radius(radius);
    }
    fn internal_names(&self) -> Vec<String> {
        vec!["damage-rate".into()]
    }
    fn update_internals(&mut self, internals: &WeightInternals) -> Result<()> {
        self.damage_rate = internal_values(internals, "damage-rate")?;
        Ok(())
    }
    fn weight(&self, r: Real, q1: PointRef<'_>, q2: PointRef<'_>) -> Real {
        if q1.point.global_num == q2.point.global_num {
            return 1.;
        }
        let d = self.damage_rate.get(q2.slot).copied().unwrap_or(Real::NAN);
        if d < self.damage_limit {
            let alpha = super::super::damage::std_max(0., 1. - r * r / self.base.r2);
            return alpha * alpha;
        }
        0.
    }
}

/// `StressBasedWeightFunction` (NL-008, Giry et al. 2011): upstream every
/// computation is commented out and `operator()` returns 0; not registered.
/// Ported as the same stub (parameter `update_rate`), so that its presence
/// is documented; a weight of 0 everywhere gives zero volumes.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct StressBasedWeightFunction {
    /// Radius and update rate.
    pub base: WeightFunctionBase,
}

impl StressBasedWeightFunction {
    const PARAMS: &'static [ParamDescriptor<Self>] = &[ParamDescriptor::new(
        "update_rate",
        ParamKind::Int,
        ParameterAccess::PARSMOD,
        "Update frequency",
        |s| ParamValue::Int(i64::from(s.base.update_rate)),
        Some(set_update_rate::<Self>),
    )];
}

weight_registry!(StressBasedWeightFunction, "weight_function:stress_based");

impl WeightFunction for StressBasedWeightFunction {
    /// UD-1103: upstream's class packs nothing for `_mnl_weight`.
    fn upstream_packs_weight_data(&self) -> bool {
        false
    }
    fn type_name(&self) -> &str {
        "stress_based"
    }
    fn update_rate(&self) -> Int {
        self.base.update_rate
    }
    fn radius(&self) -> Real {
        self.base.r
    }
    fn set_radius(&mut self, radius: Real) {
        self.base.set_radius(radius);
    }
    fn weight(&self, _r: Real, _q1: PointRef<'_>, _q2: PointRef<'_>) -> Real {
        0.
    }
}

/// Register `damage_wf` and `remove_wf` (upstream
/// `INSTANTIATE_NL_NEIGHBORHOOD(damage_wf, …)`, `(remove_wf, …)`).
pub fn register_weight_functions(factory: &mut WeightFunctionFactory) {
    factory.register(
        "damage_wf",
        Arc::new(|| Box::new(DamagedWeightFunction::default()) as Box<dyn WeightFunction>),
    );
    factory.register(
        "remove_wf",
        Arc::new(|| Box::new(RemoveDamagedWeightFunction::default()) as Box<dyn WeightFunction>),
    );
}
