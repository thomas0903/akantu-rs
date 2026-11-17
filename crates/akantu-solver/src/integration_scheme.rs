// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/model/common/integration_scheme/
// {integration_scheme.hh, integration_scheme.cc, integration_scheme_1st_order.*,
// integration_scheme_2nd_order.*, generalized_trapezoidal.*, newmark-beta.*,
// pseudo_time.*}, time_step_solvers/time_step_solver_default.cc
// (getIntegrationSchemeInternal :54-128).

//! Time-integration schemes (TS-001…008, TS-013…018).
//!
//! | `IntegrationSchemeType` | Implementation |
//! |---|---|
//! | `pseudo_time` | [`PseudoTime`] |
//! | `forward_euler`, `trapezoidal_rule_1`, `backward_euler`, `generalized_trapezoidal` | [`GeneralizedTrapezoidal`] with α = 0, ½, 1, user (default 0) |
//! | `central_difference`, `fox_goodwin`, `trapezoidal_rule_2`, `linear_acceleration`, `newmark_beta` | [`NewmarkBeta`] with (α, β) = (0, ½), (⅙, ½), (½, ½), (⅓, ½), user (default (0, 0)) |
//!
//! Akantu's Newmark naming: `beta` is the standard γ (velocity weight) and
//! `alpha·beta` the standard β (displacement weight).
//!
//! Predictors, correctors and Jacobian/residual contributions keep
//! upstream's operation order statement by statement (per DOF component,
//! no contraction), so explicit and implicit histories are bit-identical
//! to the oracle's. A scheme is bound to one DOF family; its methods
//! receive the [`DofManager`] explicitly.
//!
//! Upstream dereferences a null blocked-DOF array when the family has
//! none (UD-198); the port treats a family without blocked flags as having
//! no blocked DOF.
//!
//! Deviations: the family arrays a scheme reads (values,
//! derivatives, solution, blocked flags) are length-checked before every
//! predictor/corrector (`Error::DimensionMismatch` instead of a silently
//! truncated update); [`IntegrationScheme::invalidate_jacobian`]
//! lets the time-step solver force a rebuild of the shared `J` (UD-193);
//! the generalized trapezoidal rule skips `K` when its coefficient
//! `e = αΔt` is zero (forward Euler, UD-195; upstream reads it anyway).

use akantu_core::enums::IntegrationSchemeType;
use akantu_core::exec::{Tuples, TuplesRef};
use akantu_core::{Array, Error, Id, Real, Result};
use akantu_parser::ParserType;
use akantu_parser::param::{
    ParamDescriptor, ParamInfo, ParamKind, ParamValue, ParameterAccess, ParameterRegistry,
    Parsable, table_get, table_info, table_names, table_set, table_set_from_input,
};

use crate::dof_manager::{DofManager, FamilyArray};

/// Which variable the non-linear solver solves for (upstream
/// `IntegrationScheme::SolutionType`; `temperature` and `damage` are
/// aliases of `displacement`, `temperature_rate` of `velocity`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum SolutionType {
    /// `_displacement` = `_temperature` = `_damage` (0).
    Displacement,
    /// `_velocity` = `_temperature_rate` (1).
    Velocity,
    /// `_acceleration` (2).
    Acceleration,
    /// `_not_defined` (−1): the scheme's correctors reject it.
    #[default]
    NotDefined,
}

impl SolutionType {
    /// Upstream `_temperature`.
    pub const TEMPERATURE: Self = Self::Displacement;
    /// Upstream `_damage`.
    pub const DAMAGE: Self = Self::Displacement;
    /// Upstream `_temperature_rate`.
    pub const TEMPERATURE_RATE: Self = Self::Velocity;

    /// Upstream integer value (−1, 0, 1, 2).
    #[must_use]
    pub const fn value(self) -> i32 {
        match self {
            Self::NotDefined => -1,
            Self::Displacement => 0,
            Self::Velocity => 1,
            Self::Acceleration => 2,
        }
    }
}

impl std::str::FromStr for SolutionType {
    type Err = Error;
    /// Upstream `operator>>`: `displacement`, `temperature`, `velocity`,
    /// `temperature_rate`, `acceleration`, `damage`.
    fn from_str(s: &str) -> Result<Self> {
        Ok(match s {
            "displacement" | "temperature" | "damage" => Self::Displacement,
            "velocity" | "temperature_rate" => Self::Velocity,
            "acceleration" => Self::Acceleration,
            _ => {
                return Err(Error::invalid(format!(
                    "unknown integration-scheme solution type '{s}'"
                )));
            }
        })
    }
}

/// A time-integration scheme bound to one DOF family (upstream
/// `IntegrationScheme`, parsable with section type `integration_scheme`).
pub trait IntegrationScheme: ParameterRegistry + std::fmt::Debug + Send {
    /// Kind (factory key).
    fn kind(&self) -> IntegrationSchemeType;
    /// DOF family integrated by this scheme.
    fn dof_id(&self) -> &str;
    /// Order of the ODE (0, 1 or 2).
    fn order(&self) -> usize;
    /// Matrices the scheme needs (upstream `getNeededMatrixList`).
    fn needed_matrices(&self) -> &'static [&'static str];
    /// Predictor (upstream `predictor(delta_t)`).
    fn predictor(&mut self, dofs: &mut DofManager, delta_t: Real) -> Result<()>;
    /// Corrector with the solution of the last solve (upstream
    /// `corrector(type, delta_t)`).
    fn corrector(
        &mut self,
        dofs: &mut DofManager,
        solution: SolutionType,
        delta_t: Real,
    ) -> Result<()>;
    /// Assemble `J` from `K`, `M`, `C` when one of them or the blocked set
    /// changed (upstream `assembleJacobian`).
    fn assemble_jacobian(
        &mut self,
        dofs: &mut DofManager,
        solution: SolutionType,
        delta_t: Real,
    ) -> Result<()>;
    /// Inertial/damping residual terms (upstream `assembleResidual`).
    fn assemble_residual(&mut self, dofs: &mut DofManager, is_lumped: bool) -> Result<()>;
    /// Save the DOFs and derivatives 0..=order (upstream `store`, at the
    /// start of a step).
    fn store(&mut self, dofs: &DofManager) -> Result<()>;
    /// Restore the stored values (upstream `restore`, failed step).
    fn restore(&mut self, dofs: &mut DofManager) -> Result<()>;
    /// Forget the source-matrix releases recorded at the last Jacobian
    /// assembly, so the next [`IntegrationScheme::assemble_jacobian`]
    /// rebuilds `J` (port extension, UD-193: called by the time-step solver
    /// when another solver sharing the DOF manager changed `J`). Schemes
    /// that cache nothing keep the default no-op; a scheme caching releases
    /// must override it.
    fn invalidate_jacobian(&mut self) {}
}

/// Allocate a scheme of type `ty` for `dof_id` (upstream
/// `TimeStepSolverDefault::getIntegrationSchemeInternal`): with a lumped
/// mass only `forward_euler` and `central_difference` are accepted.
pub fn new_integration_scheme(
    ty: IntegrationSchemeType,
    dof_id: &str,
    is_mass_lumped: bool,
) -> Result<Box<dyn IntegrationScheme>> {
    use IntegrationSchemeType as T;
    if is_mass_lumped && !matches!(ty, T::ForwardEuler | T::CentralDifference) {
        return Err(Error::invalid(format!(
            "This integration scheme cannot be used in lumped dynamic ({ty})"
        )));
    }
    Ok(match ty {
        T::PseudoTime => Box::new(PseudoTime::new(dof_id)),
        T::ForwardEuler => Box::new(GeneralizedTrapezoidal::with_kind(dof_id, 0.0, ty)),
        T::TrapezoidalRule1 => Box::new(GeneralizedTrapezoidal::with_kind(dof_id, 0.5, ty)),
        T::BackwardEuler => Box::new(GeneralizedTrapezoidal::with_kind(dof_id, 1.0, ty)),
        T::GeneralizedTrapezoidal => Box::new(GeneralizedTrapezoidal::with_kind(dof_id, 0.0, ty)),
        T::CentralDifference => Box::new(NewmarkBeta::with_kind(dof_id, 0.0, 1.0 / 2.0, ty)),
        T::TrapezoidalRule2 => Box::new(NewmarkBeta::with_kind(dof_id, 1.0 / 2.0, 1.0 / 2.0, ty)),
        T::FoxGoodwin => Box::new(NewmarkBeta::with_kind(dof_id, 1.0 / 6.0, 1.0 / 2.0, ty)),
        T::LinearAcceleration => Box::new(NewmarkBeta::with_kind(dof_id, 1.0 / 3.0, 1.0 / 2.0, ty)),
        T::NewmarkBeta => Box::new(NewmarkBeta::with_kind(dof_id, 0.0, 0.0, ty)),
    })
}

// ------------------------------------------------------------------ store

/// Upstream `u_store`: one array per derivative order 0..=order.
#[derive(Debug, Default, Clone)]
struct Store {
    u: Vec<Option<Array<Real>>>,
}

impl Store {
    fn store(&mut self, dofs: &DofManager, dof_id: &str, order: usize) -> Result<()> {
        self.u.resize(order + 1, None);
        for (o, slot) in self.u.iter_mut().enumerate() {
            let src = dofs.dofs_derivative(dof_id, o)?;
            match slot {
                Some(a) if a.len() == src.len() && a.nb_component() == src.nb_component() => {
                    a.as_mut_slice().copy_from_slice(src.as_slice());
                }
                _ => *slot = Some(src.clone()),
            }
        }
        Ok(())
    }

    fn restore(&self, dofs: &mut DofManager, dof_id: &str, order: usize) -> Result<()> {
        for o in 0..=order {
            let saved = self.u.get(o).and_then(Option::as_ref).ok_or_else(|| {
                Error::invalid(format!("no stored values of '{dof_id}' to restore"))
            })?;
            let mut dst = dofs.dofs_derivative_mut(dof_id, o)?;
            if dst.as_slice().len() != saved.as_slice().len() {
                return Err(Error::DimensionMismatch {
                    context: "IntegrationScheme::restore",
                    expected: dst.as_slice().len(),
                    got: saved.as_slice().len(),
                });
            }
            dst.as_mut_slice().copy_from_slice(saved.as_slice());
        }
        Ok(())
    }
}

/// Every array of family `dof_id` a scheme reads in a
/// predictor/corrector (values, the derivatives `orders`, the solution of
/// the last solve and the blocked flags) has the length of the values;
/// arrays resized through [`DofManager::family_mut`] are reported, never
/// truncated by a `zip` or a panicking copy.
fn check_family(dofs: &DofManager, dof_id: &str, orders: &[usize]) -> Result<()> {
    let f = dofs.family(dof_id)?;
    let n = f.dofs.read()?.as_slice().len();
    let mismatch = |context: &'static str, got: usize| Error::DimensionMismatch {
        context,
        expected: n,
        got,
    };
    if f.solution.as_slice().len() != n {
        return Err(mismatch(
            "integration scheme: solution of the DOF family",
            f.solution.as_slice().len(),
        ));
    }
    if let Some(b) = &f.blocked
        && b.read()?.as_slice().len() != n
    {
        return Err(mismatch(
            "integration scheme: blocked flags of the DOF family",
            b.read()?.as_slice().len(),
        ));
    }
    for o in orders {
        if let Some(d) = f.derivatives.get(o)
            && d.read()?.as_slice().len() != n
        {
            return Err(mismatch(
                "integration scheme: derivative of the DOF family",
                d.read()?.as_slice().len(),
            ));
        }
    }
    Ok(())
}

/// Blocked flags of a family (all `false` when none are registered).
fn blocked_of(dofs: &DofManager, dof_id: &str) -> Result<Vec<bool>> {
    let f = dofs.family(dof_id)?;
    Ok(match &f.blocked {
        Some(b) => b.read()?.as_slice().to_vec(),
        None => vec![false; f.dofs.read()?.as_slice().len()],
    })
}

fn release(dofs: &DofManager, id: &str) -> Result<u64> {
    dofs.matrix_release(id)
}

fn unsupported(ty: SolutionType) -> Error {
    Error::invalid(format!(
        "The corrector type : {} is not supported by this type of integration scheme",
        ty.value()
    ))
}

macro_rules! registry_impl {
    ($t:ty, $table:ident) => {
        impl ParameterRegistry for $t {
            fn get_param(&self, name: &str) -> Result<ParamValue> {
                table_get($table, self, name)
            }
            fn set_param(&mut self, name: &str, v: ParamValue) -> Result<()> {
                table_set($table, self, name, v)
            }
            fn param_names(&self) -> Vec<&'static str> {
                table_names($table)
            }
            fn param_info(&self, name: &str) -> Option<ParamInfo> {
                table_info($table, name)
            }
            fn set_param_from_input(&mut self, name: &str, v: ParamValue) -> Result<()> {
                table_set_from_input($table, self, name, v)
            }
        }
        impl Parsable for $t {
            fn section_type(&self) -> ParserType {
                ParserType::IntegrationScheme
            }
            fn parsable_id(&self) -> &str {
                &self.dof_id
            }
        }
    };
}

// ------------------------------------------------------------- pseudo time

/// Static pseudo-time (upstream `PseudoTime`, order 0, needs `K`):
/// corrector `u += δ` on free DOFs, `J = K`.
#[derive(Debug, Clone)]
pub struct PseudoTime {
    dof_id: Id,
    k_release: Option<u64>,
    store: Store,
}

impl PseudoTime {
    /// Scheme for family `dof_id`.
    #[must_use]
    pub fn new(dof_id: &str) -> Self {
        Self {
            dof_id: dof_id.into(),
            k_release: None,
            store: Store::default(),
        }
    }
}

static PSEUDO_TIME_PARAMS: &[ParamDescriptor<PseudoTime>] = &[];
registry_impl!(PseudoTime, PSEUDO_TIME_PARAMS);

impl IntegrationScheme for PseudoTime {
    fn kind(&self) -> IntegrationSchemeType {
        IntegrationSchemeType::PseudoTime
    }
    fn dof_id(&self) -> &str {
        &self.dof_id
    }
    fn order(&self) -> usize {
        0
    }
    fn needed_matrices(&self) -> &'static [&'static str] {
        &["K"]
    }
    fn predictor(&mut self, _: &mut DofManager, _: Real) -> Result<()> {
        Ok(())
    }
    fn corrector(&mut self, dofs: &mut DofManager, _: SolutionType, _: Real) -> Result<()> {
        check_family(dofs, &self.dof_id, &[])?;
        let blocked = blocked_of(dofs, &self.dof_id)?;
        let f = dofs.family_mut(&self.dof_id)?;
        let mut u = f.dofs.write()?;
        for ((u, &delta), &bld) in u
            .as_mut_slice()
            .iter_mut()
            .zip(f.solution.as_slice())
            .zip(&blocked)
        {
            if !bld {
                *u += delta;
            }
        }
        Ok(())
    }
    fn assemble_jacobian(&mut self, dofs: &mut DofManager, _: SolutionType, _: Real) -> Result<()> {
        let k_rel = release(dofs, "K")?;
        let need = Some(k_rel) != self.k_release || dofs.has_blocked_dofs_changed();
        if !need {
            return Ok(());
        }
        dofs.copy_matrix_profile("J", "K")?;
        dofs.add_matrix("J", "K", 1.0)?;
        self.k_release = Some(release(dofs, "K")?);
        Ok(())
    }
    fn assemble_residual(&mut self, _: &mut DofManager, _: bool) -> Result<()> {
        Ok(())
    }
    fn store(&mut self, dofs: &DofManager) -> Result<()> {
        self.store.store(dofs, &self.dof_id, 0)
    }
    fn restore(&mut self, dofs: &mut DofManager) -> Result<()> {
        self.store.restore(dofs, &self.dof_id, 0)
    }
    fn invalidate_jacobian(&mut self) {
        self.k_release = None;
    }
}

// --------------------------------------------------- first-order residual

/// Upstream `IntegrationScheme1stOrder::assembleResidual`: `r -= M·u̇`
/// (consistent, if `M` exists) or `r -= M_lumped ∘ u̇` (if it exists).
fn first_order_residual(dofs: &mut DofManager, dof_id: &str, is_lumped: bool) -> Result<()> {
    dofs.dofs_derivative(dof_id, 1)?;
    if is_lumped {
        if dofs.has_lumped_matrix("M") {
            dofs.assemble_lumped_mat_mul_family_to_residual(
                dof_id,
                FamilyArray::Derivative(1),
                "M",
                -1.0,
            )?;
        }
    } else if dofs.has_matrix("M") {
        dofs.assemble_mat_mul_family_to_residual(dof_id, FamilyArray::Derivative(1), "M", -1.0)?;
    }
    Ok(())
}

/// Upstream `IntegrationScheme2ndOrder::assembleResidual`: `r -= C·u̇` if
/// `C` exists, then `r -= M·ü` (consistent) or `r -= M_lumped ∘ ü`.
fn second_order_residual(dofs: &mut DofManager, dof_id: &str, is_lumped: bool) -> Result<()> {
    if dofs.has_matrix("C") {
        dofs.dofs_derivative(dof_id, 1)?;
        dofs.assemble_mat_mul_family_to_residual(dof_id, FamilyArray::Derivative(1), "C", -1.0)?;
    }
    dofs.dofs_derivative(dof_id, 2)?;
    if is_lumped {
        dofs.assemble_lumped_mat_mul_family_to_residual(
            dof_id,
            FamilyArray::Derivative(2),
            "M",
            -1.0,
        )
    } else {
        dofs.assemble_mat_mul_family_to_residual(dof_id, FamilyArray::Derivative(2), "M", -1.0)
    }
}

/// The Newmark-β predictor of the equations of one part (`[k, h, α, β, Δt]`;
/// no-alias parameters, not inlined into the part closure).
#[inline(never)]
fn newmark_predictor_part(
    [k, h, alpha, beta, delta_t]: [Real; 5],
    u: &mut [Real],
    v: &mut [Real],
    a: &mut [Real],
    blocked: &[bool],
) {
    for (((u, u_dot), u_dot_dot), &bld) in u.iter_mut().zip(v).zip(a).zip(blocked) {
        if !bld {
            let dt_a_n = delta_t * *u_dot_dot;
            *u +=
                (1.0 - k * alpha) * delta_t * *u_dot + (0.5 - h * alpha * beta) * delta_t * dt_a_n;
            *u_dot = (1.0 - k) * *u_dot + (1.0 - h * beta) * dt_a_n;
            // upstream `(1 - h) * a`; IEEE multiplication commutes exactly
            *u_dot_dot *= 1.0 - h;
        }
    }
}

/// The Newmark-β corrector of the equations of one part (`[c, d, e]`;
/// no-alias parameters).
#[inline(never)]
fn newmark_corrector_part(
    [c, d, e]: [Real; 3],
    u: &mut [Real],
    v: &mut [Real],
    a: &mut [Real],
    solution: &[Real],
    blocked: &[bool],
) {
    for ((((u, u_dot), u_dot_dot), &delta), &bld) in
        u.iter_mut().zip(v).zip(a).zip(solution).zip(blocked)
    {
        if !bld {
            *u += e * delta;
            *u_dot += d * delta;
            *u_dot_dot += c * delta;
        }
    }
}

/// Equations per part of a threaded integration-scheme update (D-123). The
/// update streams a few arrays that the step's threaded kernels (the
/// residual scatter by node ranges, the lumped solve) have just written:
/// with those kernels on threads, parts of 4,096 equations made the steps
/// of meshes with 0.1–0.35 million equations faster at 2, 4 and 8 threads
/// (8 threads: 0.70–0.91 of the step with parts of 262,144 equations; the
/// same with idle workers parked at once, so not a wake-up effect).
/// An earlier screening, when those kernels were serial, found the
/// opposite: the arrays then sat in the calling thread's caches.
pub(crate) const EQUATIONS_PER_PART: usize = 4_096;

// ------------------------------------------------- generalized trapezoidal

/// The generalized-trapezoidal predictor of one part of the equations
/// (`[α, Δt]`; no-alias parameters).
#[inline(never)]
fn trapezoidal_predictor_part(
    [alpha, delta_t]: [Real; 2],
    reset_rate: bool,
    u: &mut [Real],
    u_dot: &mut [Real],
    blocked: &[bool],
) {
    for ((u, u_dot), &bld) in u.iter_mut().zip(u_dot).zip(blocked) {
        if !bld {
            *u += (1.0 - alpha) * delta_t * *u_dot;
            if reset_rate {
                *u_dot = 0.0;
            }
        }
    }
}

/// The generalized-trapezoidal corrector of one part of the equations
/// (`[e, d]`; no-alias parameters).
#[inline(never)]
fn trapezoidal_corrector_part(
    [e, d]: [Real; 2],
    u: &mut [Real],
    u_dot: &mut [Real],
    solution: &[Real],
    blocked: &[bool],
) {
    for (((u, u_dot), &delta), &bld) in u.iter_mut().zip(u_dot).zip(solution).zip(blocked) {
        if !bld {
            *u += e * delta;
            *u_dot += d * delta;
        }
    }
}

/// Generalized trapezoidal rule (upstream `GeneralizedTrapezoidal`, first
/// order, needs `K`, `M`):
/// `u_{n+1} = u_n + (1−α)Δt u̇_n + αΔt u̇_{n+1}`. Solution types:
/// temperature (`e = 1`, `d = 1/(αΔt)`) and temperature rate
/// (`e = αΔt`, `d = 1`).
#[derive(Debug, Clone)]
pub struct GeneralizedTrapezoidal {
    dof_id: Id,
    kind: IntegrationSchemeType,
    alpha: Real,
    m_release: Option<u64>,
    k_release: Option<u64>,
    store: Store,
}

impl GeneralizedTrapezoidal {
    /// Generic scheme (`generalized_trapezoidal`) with parameter `alpha`.
    #[must_use]
    pub fn new(dof_id: &str, alpha: Real) -> Self {
        Self::with_kind(dof_id, alpha, IntegrationSchemeType::GeneralizedTrapezoidal)
    }

    fn with_kind(dof_id: &str, alpha: Real, kind: IntegrationSchemeType) -> Self {
        Self {
            dof_id: dof_id.into(),
            kind,
            alpha,
            m_release: None,
            k_release: None,
            store: Store::default(),
        }
    }

    /// α.
    #[must_use]
    pub fn alpha(&self) -> Real {
        self.alpha
    }

    /// Upstream `getTemperatureCoefficient`.
    pub fn temperature_coefficient(&self, ty: SolutionType, delta_t: Real) -> Result<Real> {
        match ty {
            SolutionType::Displacement => Ok(1.0),
            SolutionType::Velocity => Ok(self.alpha * delta_t),
            t => Err(unsupported(t)),
        }
    }

    /// Upstream `getTemperatureRateCoefficient`.
    pub fn temperature_rate_coefficient(&self, ty: SolutionType, delta_t: Real) -> Result<Real> {
        match ty {
            SolutionType::Displacement => Ok(1.0 / (self.alpha * delta_t)),
            SolutionType::Velocity => Ok(1.0),
            t => Err(unsupported(t)),
        }
    }
}

static GT_PARAMS: &[ParamDescriptor<GeneralizedTrapezoidal>] = &[ParamDescriptor::new(
    "alpha",
    ParamKind::Real,
    ParameterAccess::PARSMOD,
    "The alpha parameter",
    |s| ParamValue::Real(s.alpha),
    Some(|s, v| {
        s.alpha = v.as_real()?;
        Ok(())
    }),
)];
registry_impl!(GeneralizedTrapezoidal, GT_PARAMS);

impl IntegrationScheme for GeneralizedTrapezoidal {
    fn kind(&self) -> IntegrationSchemeType {
        self.kind
    }
    fn dof_id(&self) -> &str {
        &self.dof_id
    }
    fn order(&self) -> usize {
        1
    }
    /// `{"K", "M"}`; forward Euler (upstream `ForwardEuler` override)
    /// declares only `{"M"}` (its Jacobian `J = d·M` needs no `K` with the
    /// UD-195 fix; upstream reads `K` anyway).
    fn needed_matrices(&self) -> &'static [&'static str] {
        if self.kind == IntegrationSchemeType::ForwardEuler {
            &["M"]
        } else {
            &["K", "M"]
        }
    }
    /// Upstream predictor `u += (1 − α)Δt u̇` on the free DOFs; for
    /// `α ≠ 0` the free rates are then reset to `0` (UD-287 correction;
    /// upstream keeps them with
    /// `UpstreamCompat::generalized_trapezoidal_keeps_rate_in_predictor`).
    fn predictor(&mut self, dofs: &mut DofManager, delta_t: Real) -> Result<()> {
        check_family(dofs, &self.dof_id, &[1])?;
        let blocked = blocked_of(dofs, &self.dof_id)?;
        let alpha = self.alpha;
        let reset_rate = alpha != 0.0
            && !dofs
                .upstream_compat()
                .generalized_trapezoidal_keeps_rate_in_predictor;
        let exec = dofs.exec().clone();
        let f = dofs.family_mut(&self.dof_id)?;
        {
            // both guards scoped explicitly (D-071); the rate is written
            // too (UD-287 reset)
            let mut v = f
                .derivatives
                .get_mut(&1)
                .ok_or_else(|| Error::not_found("dof derivative", "order 1"))?
                .write()?;
            let mut u = f.dofs.write()?;
            // the equations the zipped loop visits (the shortest array);
            // every equation from its own values: parts on the DOF
            // manager's threads (D-123)
            let n = (u.as_slice().len())
                .min(v.as_slice().len())
                .min(blocked.len());
            let items = (
                Tuples::new(&mut u.as_mut_slice()[..n], 1),
                Tuples::new(&mut v.as_mut_slice()[..n], 1),
                TuplesRef::new(&blocked[..n], 1),
            );
            exec.for_each(items, EQUATIONS_PER_PART, |_, (mut u, mut v, b)| {
                trapezoidal_predictor_part(
                    [alpha, delta_t],
                    reset_rate,
                    u.as_mut_slice(),
                    v.as_mut_slice(),
                    b.as_slice(),
                );
            });
        }
        Ok(())
    }
    fn corrector(&mut self, dofs: &mut DofManager, ty: SolutionType, delta_t: Real) -> Result<()> {
        let e = self.temperature_coefficient(ty, delta_t)?;
        let d = self.temperature_rate_coefficient(ty, delta_t)?;
        check_family(dofs, &self.dof_id, &[1])?;
        let blocked = blocked_of(dofs, &self.dof_id)?;
        let exec = dofs.exec().clone();
        let f = dofs.family_mut(&self.dof_id)?;
        let crate::dof_manager::DofData {
            dofs: u,
            derivatives,
            solution,
            ..
        } = f;
        let mut v = derivatives
            .get_mut(&1)
            .ok_or_else(|| Error::not_found("dof derivative", "order 1"))?
            .write()?;
        let mut u = u.write()?;
        let sol = solution.as_slice();
        // the equations the zipped loop visits (the shortest array)
        let n = (u.as_slice().len())
            .min(v.as_slice().len())
            .min(sol.len())
            .min(blocked.len());
        let items = (
            Tuples::new(&mut u.as_mut_slice()[..n], 1),
            Tuples::new(&mut v.as_mut_slice()[..n], 1),
            TuplesRef::new(&sol[..n], 1),
            TuplesRef::new(&blocked[..n], 1),
        );
        exec.for_each(items, EQUATIONS_PER_PART, |_, (mut u, mut v, sol, b)| {
            trapezoidal_corrector_part(
                [e, d],
                u.as_mut_slice(),
                v.as_mut_slice(),
                sol.as_slice(),
                b.as_slice(),
            );
        });
        Ok(())
    }
    fn assemble_jacobian(
        &mut self,
        dofs: &mut DofManager,
        ty: SolutionType,
        delta_t: Real,
    ) -> Result<()> {
        if dofs
            .upstream_compat()
            .generalized_trapezoidal_reads_k_when_e_is_zero
        {
            // Upstream order: `K` is read even when `e == 0` (UD-195).
            let m_rel = release(dofs, "M")?;
            let k_rel = release(dofs, "K")?;
            let need = Some(m_rel) != self.m_release
                || Some(k_rel) != self.k_release
                || dofs.has_blocked_dofs_changed();
            if !need {
                return Ok(());
            }
            dofs.copy_matrix_profile("J", "K")?;
            let d = self.temperature_rate_coefficient(ty, delta_t)?;
            let e = self.temperature_coefficient(ty, delta_t)?;
            dofs.add_matrix("J", "M", d)?;
            dofs.add_matrix("J", "K", e)?;
            self.m_release = Some(release(dofs, "M")?);
            self.k_release = Some(release(dofs, "K")?);
            return Ok(());
        }
        // UD-195 fix: with `e = αΔt == 0` (forward Euler, temperature-rate
        // solution) `J = d·M`, `K` is neither read nor required — exactly
        // `NewmarkBeta`'s explicit `e == 0` skip. Otherwise the upstream
        // operations in upstream order (`J.copyProfile(K)`, `+d·M`, `+e·K`).
        let d = self.temperature_rate_coefficient(ty, delta_t)?;
        let e = self.temperature_coefficient(ty, delta_t)?;
        let with_k = e != 0.0;
        let mut need = Some(release(dofs, "M")?) != self.m_release;
        if with_k {
            need |= Some(release(dofs, "K")?) != self.k_release;
        }
        need |= dofs.has_blocked_dofs_changed();
        if !need {
            return Ok(());
        }
        dofs.copy_matrix_profile("J", if with_k { "K" } else { "M" })?;
        dofs.add_matrix("J", "M", d)?;
        self.m_release = Some(release(dofs, "M")?);
        if with_k {
            dofs.add_matrix("J", "K", e)?;
            self.k_release = Some(release(dofs, "K")?);
        }
        Ok(())
    }
    fn assemble_residual(&mut self, dofs: &mut DofManager, is_lumped: bool) -> Result<()> {
        first_order_residual(dofs, &self.dof_id, is_lumped)
    }
    fn store(&mut self, dofs: &DofManager) -> Result<()> {
        self.store.store(dofs, &self.dof_id, 1)
    }
    fn restore(&mut self, dofs: &mut DofManager) -> Result<()> {
        self.store.restore(dofs, &self.dof_id, 1)
    }
    fn invalidate_jacobian(&mut self) {
        self.m_release = None;
        self.k_release = None;
    }
}

// ------------------------------------------------------------ Newmark-beta

/// Newmark-β family (upstream `NewmarkBeta`, second order, needs `K`, `M`,
/// `C`). Predictor `ũ = u + Δt u̇ + Δt²/2 ü`, `ũ̇ = u̇ + Δt ü`, `ũ̈ = ü`;
/// corrector coefficients (acceleration / velocity / displacement solution):
/// `c = 1 | 1/(βΔt) | 1/(αβΔt²)`, `d = βΔt | 1 | 1/(αΔt)`,
/// `e = αβΔt² | αΔt | 1`; `J = e K + c M + d C` (K skipped when `e == 0`).
#[derive(Debug, Clone)]
pub struct NewmarkBeta {
    dof_id: Id,
    kind: IntegrationSchemeType,
    beta: Real,
    alpha: Real,
    /// Upstream members `k`, `h` (always 0; kept for the operation order).
    k: Real,
    h: Real,
    m_release: Option<u64>,
    k_release: Option<u64>,
    c_release: Option<u64>,
    store: Store,
}

impl NewmarkBeta {
    /// Generic scheme (`newmark_beta`) with Akantu's `alpha`, `beta`.
    #[must_use]
    pub fn new(dof_id: &str, alpha: Real, beta: Real) -> Self {
        Self::with_kind(dof_id, alpha, beta, IntegrationSchemeType::NewmarkBeta)
    }

    fn with_kind(dof_id: &str, alpha: Real, beta: Real, kind: IntegrationSchemeType) -> Self {
        Self {
            dof_id: dof_id.into(),
            kind,
            beta,
            alpha,
            k: 0.0,
            h: 0.0,
            m_release: None,
            k_release: None,
            c_release: None,
            store: Store::default(),
        }
    }

    /// Akantu α (standard β/γ).
    #[must_use]
    pub fn alpha(&self) -> Real {
        self.alpha
    }

    /// Akantu β (standard γ).
    #[must_use]
    pub fn beta(&self) -> Real {
        self.beta
    }

    /// Upstream `getAccelerationCoefficient`.
    pub fn acceleration_coefficient(&self, ty: SolutionType, delta_t: Real) -> Result<Real> {
        match ty {
            SolutionType::Acceleration => Ok(1.0),
            SolutionType::Velocity => Ok(1.0 / (self.beta * delta_t)),
            SolutionType::Displacement => Ok(1.0 / (self.alpha * self.beta * delta_t * delta_t)),
            t @ SolutionType::NotDefined => Err(unsupported(t)),
        }
    }

    /// Upstream `getVelocityCoefficient`.
    pub fn velocity_coefficient(&self, ty: SolutionType, delta_t: Real) -> Result<Real> {
        match ty {
            SolutionType::Acceleration => Ok(self.beta * delta_t),
            SolutionType::Velocity => Ok(1.0),
            SolutionType::Displacement => Ok(1.0 / (self.alpha * delta_t)),
            t @ SolutionType::NotDefined => Err(unsupported(t)),
        }
    }

    /// Upstream `getDisplacementCoefficient`.
    pub fn displacement_coefficient(&self, ty: SolutionType, delta_t: Real) -> Result<Real> {
        match ty {
            SolutionType::Acceleration => Ok(self.alpha * self.beta * delta_t * delta_t),
            SolutionType::Velocity => Ok(self.alpha * delta_t),
            SolutionType::Displacement => Ok(1.0),
            t @ SolutionType::NotDefined => Err(unsupported(t)),
        }
    }
}

static NB_PARAMS: &[ParamDescriptor<NewmarkBeta>] = &[
    ParamDescriptor::new(
        "alpha",
        ParamKind::Real,
        ParameterAccess::PARSMOD,
        "The alpha parameter",
        |s| ParamValue::Real(s.alpha),
        Some(|s, v| {
            s.alpha = v.as_real()?;
            Ok(())
        }),
    ),
    ParamDescriptor::new(
        "beta",
        ParamKind::Real,
        ParameterAccess::PARSMOD,
        "The beta parameter",
        |s| ParamValue::Real(s.beta),
        Some(|s, v| {
            s.beta = v.as_real()?;
            Ok(())
        }),
    ),
];
registry_impl!(NewmarkBeta, NB_PARAMS);

impl IntegrationScheme for NewmarkBeta {
    fn kind(&self) -> IntegrationSchemeType {
        self.kind
    }
    fn dof_id(&self) -> &str {
        &self.dof_id
    }
    fn order(&self) -> usize {
        2
    }
    /// `{"K", "M", "C"}`; central difference (upstream
    /// `CentralDifference` override) needs only `{"M", "C"}`.
    fn needed_matrices(&self) -> &'static [&'static str] {
        if self.kind == IntegrationSchemeType::CentralDifference {
            &["M", "C"]
        } else {
            &["K", "M", "C"]
        }
    }
    fn predictor(&mut self, dofs: &mut DofManager, delta_t: Real) -> Result<()> {
        check_family(dofs, &self.dof_id, &[1, 2])?;
        let blocked = blocked_of(dofs, &self.dof_id)?;
        let exec = dofs.exec().clone();
        let (k, h, alpha, beta) = (self.k, self.h, self.alpha, self.beta);
        let f = dofs.family_mut(&self.dof_id)?;
        let crate::dof_manager::DofData {
            dofs: u,
            derivatives,
            ..
        } = f;
        let mut it = derivatives.range_mut(1..=2);
        let (mut v, mut a) = match (it.next(), it.next()) {
            (Some((1, v)), Some((2, a))) => (v.write()?, a.write()?),
            _ => return Err(Error::not_found("dof derivatives", "orders 1 and 2")),
        };
        let mut u = u.write()?;
        // the equations the zipped loop visits (the shortest array)
        let n = (u.as_slice().len())
            .min(v.as_slice().len())
            .min(a.as_slice().len())
            .min(blocked.len());
        // every equation from its own values: contiguous parts on the DOF
        // manager's threads (D-123)
        let items = (
            Tuples::new(&mut u.as_mut_slice()[..n], 1),
            Tuples::new(&mut v.as_mut_slice()[..n], 1),
            Tuples::new(&mut a.as_mut_slice()[..n], 1),
            TuplesRef::new(&blocked[..n], 1),
        );
        exec.for_each(items, EQUATIONS_PER_PART, |_, (mut u, mut v, mut a, b)| {
            newmark_predictor_part(
                [k, h, alpha, beta, delta_t],
                u.as_mut_slice(),
                v.as_mut_slice(),
                a.as_mut_slice(),
                b.as_slice(),
            );
        });
        Ok(())
    }
    fn corrector(&mut self, dofs: &mut DofManager, ty: SolutionType, delta_t: Real) -> Result<()> {
        let c = self.acceleration_coefficient(ty, delta_t)?;
        let d = self.velocity_coefficient(ty, delta_t)?;
        let e = self.displacement_coefficient(ty, delta_t)?;
        check_family(dofs, &self.dof_id, &[1, 2])?;
        let blocked = blocked_of(dofs, &self.dof_id)?;
        let exec = dofs.exec().clone();
        let f = dofs.family_mut(&self.dof_id)?;
        let crate::dof_manager::DofData {
            dofs: u,
            derivatives,
            solution,
            ..
        } = f;
        let mut it = derivatives.range_mut(1..=2);
        let (mut v, mut a) = match (it.next(), it.next()) {
            (Some((1, v)), Some((2, a))) => (v.write()?, a.write()?),
            _ => return Err(Error::not_found("dof derivatives", "orders 1 and 2")),
        };
        let mut u = u.write()?;
        let sol = solution.as_slice();
        // the equations the zipped loop visits (the shortest array)
        let n = (u.as_slice().len())
            .min(v.as_slice().len())
            .min(a.as_slice().len())
            .min(sol.len())
            .min(blocked.len());
        let items = (
            Tuples::new(&mut u.as_mut_slice()[..n], 1),
            Tuples::new(&mut v.as_mut_slice()[..n], 1),
            Tuples::new(&mut a.as_mut_slice()[..n], 1),
            TuplesRef::new(&sol[..n], 1),
            TuplesRef::new(&blocked[..n], 1),
        );
        exec.for_each(
            items,
            EQUATIONS_PER_PART,
            |_, (mut u, mut v, mut a, sol, b)| {
                newmark_corrector_part(
                    [c, d, e],
                    u.as_mut_slice(),
                    v.as_mut_slice(),
                    a.as_mut_slice(),
                    sol.as_slice(),
                    b.as_slice(),
                );
            },
        );
        Ok(())
    }
    fn assemble_jacobian(
        &mut self,
        dofs: &mut DofManager,
        ty: SolutionType,
        delta_t: Real,
    ) -> Result<()> {
        let c = self.acceleration_coefficient(ty, delta_t)?;
        let e = self.displacement_coefficient(ty, delta_t)?;
        let mut need = Some(release(dofs, "M")?) != self.m_release;
        // In explicit this coefficient is exactly 0.
        if e != 0.0 {
            need |= Some(release(dofs, "K")?) != self.k_release;
        }
        if dofs.has_matrix("C") {
            need |= Some(release(dofs, "C")?) != self.c_release;
        }
        need |= dofs.has_blocked_dofs_changed();
        if !need {
            return Ok(());
        }
        dofs.copy_matrix_profile("J", "M")?;
        if e != 0.0 {
            dofs.add_matrix("J", "K", e)?;
            self.k_release = Some(release(dofs, "K")?);
        }
        dofs.add_matrix("J", "M", c)?;
        self.m_release = Some(release(dofs, "M")?);
        if dofs.has_matrix("C") {
            let d = self.velocity_coefficient(ty, delta_t)?;
            dofs.add_matrix("J", "C", d)?;
            self.c_release = Some(release(dofs, "C")?);
        }
        Ok(())
    }
    fn assemble_residual(&mut self, dofs: &mut DofManager, is_lumped: bool) -> Result<()> {
        second_order_residual(dofs, &self.dof_id, is_lumped)
    }
    fn store(&mut self, dofs: &DofManager) -> Result<()> {
        self.store.store(dofs, &self.dof_id, 2)
    }
    fn restore(&mut self, dofs: &mut DofManager) -> Result<()> {
        self.store.restore(dofs, &self.dof_id, 2)
    }
    fn invalidate_jacobian(&mut self) {
        self.m_release = None;
        self.k_release = None;
        self.c_release = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn solution_type_names_and_values() {
        assert_eq!(
            "temperature".parse::<SolutionType>().unwrap(),
            SolutionType::Displacement
        );
        assert_eq!(
            "damage".parse::<SolutionType>().unwrap(),
            SolutionType::DAMAGE
        );
        assert_eq!(
            "temperature_rate".parse::<SolutionType>().unwrap(),
            SolutionType::Velocity
        );
        assert!("jerk".parse::<SolutionType>().is_err());
        assert_eq!(SolutionType::NotDefined.value(), -1);
        assert_eq!(SolutionType::Acceleration.value(), 2);
    }

    #[test]
    fn factory_presets_and_lumped_restriction() {
        use IntegrationSchemeType as T;
        let cd = new_integration_scheme(T::CentralDifference, "u", true).unwrap();
        assert_eq!(cd.get_param("alpha").unwrap().as_real().unwrap(), 0.0);
        assert_eq!(cd.get_param("beta").unwrap().as_real().unwrap(), 0.5);
        let fg = new_integration_scheme(T::FoxGoodwin, "u", false).unwrap();
        assert_eq!(fg.get_param("alpha").unwrap().as_real().unwrap(), 1.0 / 6.0);
        let tr1 = new_integration_scheme(T::TrapezoidalRule1, "u", false).unwrap();
        assert_eq!(tr1.get_param("alpha").unwrap().as_real().unwrap(), 0.5);
        assert_eq!(tr1.needed_matrices(), &["K", "M"]);
        assert_eq!(cd.needed_matrices(), &["M", "C"]);
        let fe = new_integration_scheme(T::ForwardEuler, "u", false).unwrap();
        assert_eq!(fe.needed_matrices(), &["M"]);
        assert_eq!(fg.needed_matrices(), &["K", "M", "C"]);
        assert!(new_integration_scheme(T::TrapezoidalRule2, "u", true).is_err());
        assert!(new_integration_scheme(T::ForwardEuler, "u", true).is_ok());
        let pt = new_integration_scheme(T::PseudoTime, "u", false).unwrap();
        assert_eq!((pt.order(), pt.needed_matrices()), (0, &["K"][..]));
    }

    #[test]
    fn coefficients_follow_upstream_tables() {
        let nb = NewmarkBeta::new("u", 0.5, 0.5);
        let dt = 0.1;
        for (ty, c, d, e) in [
            (
                SolutionType::Acceleration,
                1.0,
                0.5 * dt,
                0.5 * 0.5 * dt * dt,
            ),
            (SolutionType::Velocity, 1.0 / (0.5 * dt), 1.0, 0.5 * dt),
            (
                SolutionType::Displacement,
                1.0 / (0.5 * 0.5 * dt * dt),
                1.0 / (0.5 * dt),
                1.0,
            ),
        ] {
            assert_eq!(
                nb.acceleration_coefficient(ty, dt).unwrap().to_bits(),
                c.to_bits()
            );
            assert_eq!(
                nb.velocity_coefficient(ty, dt).unwrap().to_bits(),
                d.to_bits()
            );
            assert_eq!(
                nb.displacement_coefficient(ty, dt).unwrap().to_bits(),
                e.to_bits()
            );
        }
        assert!(
            nb.acceleration_coefficient(SolutionType::NotDefined, dt)
                .is_err()
        );
        let gt = GeneralizedTrapezoidal::new("u", 0.5);
        assert!(
            gt.temperature_coefficient(SolutionType::Acceleration, dt)
                .is_err()
        );
        assert_eq!(
            gt.temperature_coefficient(SolutionType::Velocity, dt)
                .unwrap(),
            0.5 * dt
        );
    }
}
