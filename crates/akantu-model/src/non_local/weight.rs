// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/model/common/non_local_toolbox/
// base_weight_function.{hh,cc}, base_weight_function_inline_impl.hh
// (BaseWeightFunction, `update_rate`, NonLocalNeighborhoodFactory,
// INSTANTIATE_NL_NEIGHBORHOOD).

//! Weight functions of the non-local averaging (NL-004) and the factory of
//! weight-function types (upstream `NonLocalNeighborhoodFactory`, keyed by
//! the option of the `non_local <name> <type> [ … ]` section).
//!
//! The damage-dependent weight functions (`damage_wf`, `remove_wf`, …) are
//! solid-mechanics laws upstream and live in `akantu-solid`; they are
//! registered into the [`WeightFunctionFactory`] of the model that owns the
//! manager.

use std::collections::BTreeMap;
use std::sync::Arc;

use akantu_core::{Error, Int, Real, Result};
use akantu_parser::param::{
    ParamDescriptor, ParamInfo, ParamKind, ParamValue, ParameterAccess, table_get, table_info,
    table_names, table_set, table_set_from_input,
};
use akantu_parser::{ParameterRegistry, Parsable, ParserType};

use super::point::QuadPoint;

/// Manager-level flattened fields a weight function reads (upstream
/// `NonLocalManager::registerWeightFunctionInternal`, e.g. `"damage"`):
/// field name → one value per integration-point **slot** of the manager's
/// layout (owned and ghost blocks of every element type; a point's slot is
/// [`PointRef::slot`]).
pub type WeightInternals = BTreeMap<String, Vec<Real>>;

/// A point as seen by a weight function: its identity and its slot in the
/// manager's per-point layout (index into the [`WeightInternals`] values).
#[derive(Clone, Copy, Debug)]
pub struct PointRef<'a> {
    /// The point.
    pub point: &'a QuadPoint,
    /// Slot in the manager's layout.
    pub slot: usize,
}

/// Arithmetic of a weight function whose upstream definition is undefined
/// on part of its domain (UD-557: the float → `Int` conversion of
/// `damage_wf`'s exponent). Functions without such a part ignore it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum WeightArithmetic {
    /// Corrected default: upstream's arithmetic wherever it is defined,
    /// extended to the whole domain with finite weights (see the function's
    /// documentation, e.g. `akantu_solid`'s `DamagedWeightFunction`).
    #[default]
    Corrected,
    /// Upstream compiled for the authoritative oracle platform (x86_64,
    /// D-043): an out-of-range or NaN float → `Int` conversion gives
    /// `INT_MIN` (`cvttsd2si`), `Int` sums wrap; bitwise to the x86_64
    /// oracle, including its infinite weights and NaN averages.
    UpstreamX86_64,
}

/// A weight function (upstream `BaseWeightFunction` and its subclasses).
///
/// Parsable from the `weight_function weight_parameter [ … ]` sub-section
/// of the neighbourhood's `non_local` section (section type
/// [`ParserType::WeightFunction`]).
pub trait WeightFunction: Parsable + std::fmt::Debug + Send {
    /// Upstream type name (`"base"`, `"damaged"`, `"remove_damaged"`, …).
    fn type_name(&self) -> &str;

    /// Upstream `getUpdateRate()` (parameter `update_rate`, default 1).
    fn update_rate(&self) -> Int;

    /// Non-local radius.
    fn radius(&self) -> Real;

    /// Upstream `setRadius` (also `R² = R·R`).
    fn set_radius(&mut self, radius: Real);

    /// Manager-level internals this function reads (upstream `init()` →
    /// `registerWeightFunctionInternal(name)`), flattened from the laws
    /// before every weight update.
    fn internal_names(&self) -> Vec<String> {
        Vec::new()
    }

    /// Refresh from the manager-level internals (owned and ghost values)
    /// before the weights are computed.
    ///
    /// # Errors
    /// A required internal is missing.
    fn update_internals(&mut self, internals: &WeightInternals) -> Result<()> {
        let _ = internals;
        Ok(())
    }

    /// Upstream `operator()(r, q1, q2)`: the weight of `q2` seen from `q1`
    /// at distance `r`.
    fn weight(&self, r: Real, q1: PointRef<'_>, q2: PointRef<'_>) -> Real;

    /// Select the arithmetic of an upstream-undefined part of the domain
    /// ([`WeightArithmetic`]); the manager applies
    /// [`super::NonLocalOptions::weight_arithmetic`] to every weight
    /// function it creates. Default: ignored (no such part).
    fn set_arithmetic(&mut self, arithmetic: WeightArithmetic) {
        let _ = arithmetic;
    }

    /// Whether upstream's class packs its internals for `_mnl_weight` (its
    /// `getNbData` / `packData` / `unpackData`): only
    /// `RemoveDamagedWeightFunction` does; `DamagedWeightFunction` and the
    /// unregistered damage-rate / stress-based ones do not (UD-1103). Read
    /// only with [`super::NonLocalOptions::weight_exchange`] =
    /// [`super::WeightExchange::Upstream`]. Default: `true`.
    fn upstream_packs_weight_data(&self) -> bool {
        true
    }

    /// `true` if [`WeightFunction::weight`] depends only on the distance
    /// and on the radius given by [`WeightFunction::set_radius`] (no
    /// internal, no other parameter, no state): the neighbourhood then
    /// keeps the weights of its last computation while their inputs are
    /// unchanged instead of recomputing the same values. Default `false`
    /// (always recomputed, as upstream).
    fn weights_depend_only_on_distance(&self) -> bool {
        false
    }
}

/// Constructor of a weight-function type.
pub type WeightFunctionConstructor = Arc<dyn Fn() -> Box<dyn WeightFunction> + Send + Sync>;

/// Registry of weight-function types (upstream
/// `NonLocalNeighborhoodFactory`): `base_wf` is always registered; the
/// solid-mechanics model adds `damage_wf` and `remove_wf`.
#[derive(Clone)]
pub struct WeightFunctionFactory {
    ctors: BTreeMap<String, WeightFunctionConstructor>,
}

impl std::fmt::Debug for WeightFunctionFactory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WeightFunctionFactory")
            .field("types", &self.ctors.keys().collect::<Vec<_>>())
            .finish()
    }
}

impl Default for WeightFunctionFactory {
    fn default() -> Self {
        let mut f = Self {
            ctors: BTreeMap::new(),
        };
        f.register(
            "base_wf",
            Arc::new(|| Box::new(BaseWeightFunction::default()) as Box<dyn WeightFunction>),
        );
        f
    }
}

impl WeightFunctionFactory {
    /// Register (or replace) a type (upstream `INSTANTIATE_NL_NEIGHBORHOOD`).
    pub fn register(&mut self, wf_type: &str, ctor: WeightFunctionConstructor) {
        self.ctors.insert(wf_type.to_owned(), ctor);
    }

    /// `true` if `wf_type` is registered.
    #[must_use]
    pub fn contains(&self, wf_type: &str) -> bool {
        self.ctors.contains_key(wf_type)
    }

    /// Registered type names, sorted.
    #[must_use]
    pub fn types(&self) -> Vec<&str> {
        self.ctors.keys().map(String::as_str).collect()
    }

    /// New weight function of type `wf_type`.
    ///
    /// # Errors
    /// [`Error::NotFound`] for an unregistered type (upstream: factory
    /// exception).
    pub fn create(&self, wf_type: &str) -> Result<Box<dyn WeightFunction>> {
        self.ctors
            .get(wf_type)
            .map(|c| c())
            .ok_or_else(|| Error::not_found("non-local weight function type", wf_type))
    }
}

/// Common state of upstream's weight functions: radius, `R²`, update rate.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WeightFunctionBase {
    /// Radius `R`.
    pub r: Real,
    /// `R²`.
    pub r2: Real,
    /// `update_rate` (upstream `Int`, default 1, parsable and modifiable).
    pub update_rate: Int,
}

impl Default for WeightFunctionBase {
    fn default() -> Self {
        // upstream leaves R/R2 uninitialised until setRadius (called by
        // every computeWeights before the first use)
        Self {
            r: 0.,
            r2: 0.,
            update_rate: 1,
        }
    }
}

impl WeightFunctionBase {
    /// Upstream `setRadius`.
    pub fn set_radius(&mut self, radius: Real) {
        self.r = radius;
        self.r2 = self.r * self.r;
    }
}

/// `base_wf` (NL-004, upstream `BaseWeightFunction::operator()`):
/// `W(r) = (1 − r²/R²)²` for `r ≤ R`, else 0 (`alpha = 1 − r·r/R2`,
/// `w = alpha·alpha`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BaseWeightFunction {
    /// Radius and update rate.
    pub base: WeightFunctionBase,
}

/// `W(r) = (1 − r²/R²)²` on `r ≤ R` (upstream operand order).
#[inline]
#[must_use]
pub fn base_weight(r: Real, radius: Real, r2: Real) -> Real {
    let mut w = 0.;
    if r <= radius {
        let alpha = 1. - r * r / r2;
        w = alpha * alpha;
    }
    w
}

impl BaseWeightFunction {
    const PARAMS: &'static [ParamDescriptor<Self>] = &[ParamDescriptor::new(
        "update_rate",
        ParamKind::Int,
        ParameterAccess::PARSMOD,
        "Update frequency",
        |s| ParamValue::Int(i64::from(s.base.update_rate)),
        Some(set_update_rate::<Self>),
    )];
}

/// Access to the [`WeightFunctionBase`] of a weight function (shared
/// parameter setters).
pub trait HasWeightBase {
    /// Common state.
    fn wf_base_mut(&mut self) -> &mut WeightFunctionBase;
}

impl HasWeightBase for BaseWeightFunction {
    fn wf_base_mut(&mut self) -> &mut WeightFunctionBase {
        &mut self.base
    }
}

/// Setter of `update_rate` (upstream `Int`: values outside `i32` are refused).
///
/// # Errors
/// A value that is not an integer or does not fit an `Int`.
pub fn set_update_rate<S: HasWeightBase>(s: &mut S, v: ParamValue) -> Result<()> {
    match v {
        ParamValue::Int(i) => {
            s.wf_base_mut().update_rate = Int::try_from(i)
                .map_err(|_| Error::invalid(format!("update_rate {i} does not fit an Int")))?;
            Ok(())
        }
        other => Err(Error::invalid(format!(
            "update_rate expects an integer, got {other:?}"
        ))),
    }
}

impl ParameterRegistry for BaseWeightFunction {
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

impl Parsable for BaseWeightFunction {
    fn section_type(&self) -> ParserType {
        ParserType::WeightFunction
    }
    fn parsable_id(&self) -> &str {
        "weight_function:base"
    }
}

impl WeightFunction for BaseWeightFunction {
    fn type_name(&self) -> &str {
        "base"
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
    fn weight(&self, r: Real, _q1: PointRef<'_>, _q2: PointRef<'_>) -> Real {
        base_weight(r, self.base.r, self.base.r2)
    }
    /// `base_weight(r, R, R²)` with `R` and `R²` from `set_radius`.
    fn weights_depend_only_on_distance(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base_weight_formula_and_parameters() {
        let mut w = BaseWeightFunction::default();
        assert_eq!(w.update_rate(), 1);
        w.set_radius(0.5);
        let q = QuadPoint::new(
            akantu_core::ElementType::Triangle3,
            akantu_core::GhostType::NotGhost,
            0,
            0,
            1,
        );
        let p = PointRef { point: &q, slot: 0 };
        assert_eq!(w.weight(0., p, p), 1.);
        let alpha = 1. - 0.25 * 0.25 / 0.25;
        assert_eq!(w.weight(0.25, p, p), alpha * alpha);
        assert_eq!(w.weight(0.5, p, p), 0.);
        assert_eq!(w.weight(0.5000001, p, p), 0.);
        w.set_param_from_input("update_rate", ParamValue::Int(10))
            .unwrap();
        assert_eq!(w.update_rate(), 10);
        assert!(
            w.set_param_from_input("radius", ParamValue::Real(1.))
                .is_err()
        );
        let f = WeightFunctionFactory::default();
        assert_eq!(f.types(), ["base_wf"]);
        assert!(f.create("remove_wf").is_err());
    }
}
