// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): extra_packages/traction-at-split-node-contact/
// src/ntn_contact/friction_laws/{ntn_friclaw_coulomb.hh,
// ntn_friclaw_coulomb_tmpl.hh, ntn_friclaw_linear_slip_weakening.hh,
// ntn_friclaw_linear_slip_weakening_tmpl.hh,
// ntn_friclaw_linear_slip_weakening_no_healing.hh,
// ntn_friclaw_linear_slip_weakening_no_healing_tmpl.hh,
// ntn_friclaw_linear_cohesive.hh, ntn_friclaw_linear_cohesive_tmpl.hh,
// ntn_friclaw_rate_and_state.hh, ntn_friclaw_rate_and_state_tmpl.hh,
// ntn_friclaw_rate_and_state_flashheating.hh} and
// friction_regularisations/{ntn_fricreg_no_regularisation,
// ntn_fricreg_rubin_ampuero, ntn_fricreg_simplified_prakash_clifton}.{hh,cc}.

//! Friction laws and regularisations (TASN-012…037).
//!
//! Upstream composes them as C++ templates, `Law<Regularisation>` (every
//! law × every regularisation is instantiated by the factory, for both the
//! node-to-node and the rigid-flat friction). Here both are enums held by
//! [`super::NtnFriction`], which reproduces the virtual-call chain of the
//! template stack (law coefficient → Coulomb strength → regularisation).
//!
//! Every expression keeps the upstream evaluation order. `std::pow(x, 2)`
//! is evaluated as `x * x`: GCC folds `pow(x, 2.0)` into `x * x` (an exact
//! transformation it applies without fast-math), which is what the oracle
//! executes; the R&S oracle fixtures (`oracle/probes/ntn_contact`) check it.

use akantu_core::Real;

/// The natural logarithm of the oracle's libm: a pure-Rust port of glibc's
/// `log` (bit-exact with glibc 2.41 on every platform), so that the
/// rate-and-state replays are bitwise on macOS as well as on Linux
/// (the host `f64::ln` differs from glibc in the last bit for some
/// arguments). Upstream calls `log` of `<cmath>` on `double`.
#[inline]
fn ln(x: Real) -> Real {
    akantu_core::random::glibc_libm::log(x)
}

/// Rate-and-state friction coefficient (upstream `*FrictionCoefficient`
/// structs).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RsCoefficient {
    /// Dieterich: `μ₀ + a ln(V/V*) + b ln(θ/θ*)` (singular at `V = 0`).
    Dieterich,
    /// Revised Dieterich: `μ₀ + a ln(1 + V/V*) + b ln(θ/θ*) μ₀`.
    RevisedDieterich,
    /// Regularised Dieterich: `μ₀ + a ln(1 + V/V*) + b ln(1 + θ/θ*) μ₀`.
    RegularizedDieterich,
    /// N-shaped: `(1 + b ln(1 + θ/θ*)) (μ₀/√(1 + (V/V*)²) + a ln(V/V*))`
    /// (singular at `V = 0`).
    NShaped,
    /// Revised N-shaped: `(1 + b ln(1 + θ/θ*)) (μ₀/√(1 + (V*/V)²) +
    /// a ln(1 + V/V*))`.
    RevisedNShaped,
}

impl RsCoefficient {
    /// Every coefficient, in upstream registration order.
    pub const ALL: [RsCoefficient; 5] = [
        Self::Dieterich,
        Self::RevisedDieterich,
        Self::RegularizedDieterich,
        Self::NShaped,
        Self::RevisedNShaped,
    ];

    /// Keyword in the factory names (`rate_and_state:<name>:<state>`).
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Dieterich => "dieterich",
            Self::RevisedDieterich => "revised_dieterich",
            Self::RegularizedDieterich => "regularized_dieterich",
            Self::NShaped => "n_shaped",
            Self::RevisedNShaped => "revised_n_shaped",
        }
    }

    /// Whether the coefficient is singular (`ln 0`) at zero slip velocity.
    #[must_use]
    pub fn singular_at_rest(self) -> bool {
        matches!(self, Self::Dieterich | Self::NShaped)
    }

    /// The coefficient for the arguments of one node.
    #[must_use]
    pub fn evaluate(self, a: &RsArgs) -> Real {
        let (mu_0, aa, b, v, v_star, phi, phi_star) = (
            a.mu_0,
            a.a,
            a.b,
            a.slip_velocity,
            a.v_star,
            a.state,
            a.phi_star,
        );
        match self {
            Self::Dieterich => mu_0 + aa * ln(v / v_star) + b * ln(phi / phi_star),
            Self::RevisedDieterich => {
                mu_0 + aa * ln(1.0 + v / v_star) + b * ln(phi / phi_star) * mu_0
            }
            Self::RegularizedDieterich => {
                mu_0 + aa * ln(1.0 + v / v_star) + b * ln(1.0 + phi / phi_star) * mu_0
            }
            Self::NShaped => {
                let r = v / v_star;
                (1.0 + b * ln(1.0 + phi / phi_star))
                    * (mu_0 / (1.0 + r * r).sqrt() + aa * ln(v / v_star))
            }
            Self::RevisedNShaped => {
                let r = v_star / v;
                (1.0 + b * ln(1.0 + phi / phi_star))
                    * (mu_0 / (1.0 + r * r).sqrt() + aa * ln(1.0 + v / v_star))
            }
        }
    }
}

/// Rate-and-state evolution law of the state variable (upstream
/// `*StateFunction` structs; explicit Euler increments, applied before the
/// coefficient and only on slipping nodes).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RsState {
    /// Ageing: `Δθ = (1 − θV/d_c) Δt`.
    Ageing,
    /// Slip: `Δθ = −θV/d_c ln(θV/d_c) Δt` (singular at `V = 0`).
    Slip,
    /// Regularised ageing: `Δθ = (1 − θ√(V² + V₀²)/d_c) Δt`.
    AgeingRegularized,
}

impl RsState {
    /// Every state law, in upstream registration order.
    pub const ALL: [RsState; 3] = [Self::Ageing, Self::Slip, Self::AgeingRegularized];

    /// Keyword in the factory names.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Ageing => "ageing",
            Self::Slip => "slip",
            Self::AgeingRegularized => "ageing_regularized",
        }
    }

    /// The state increment for the arguments of one node.
    #[must_use]
    pub fn increment(self, a: &RsArgs) -> Real {
        let (phi, v, d_c, dt) = (a.state, a.slip_velocity, a.d_c, a.time_step);
        match self {
            Self::Ageing => (1.0 - phi * v / d_c) * dt,
            Self::Slip => (-phi * v / d_c * ln(phi * v / d_c)) * dt,
            Self::AgeingRegularized => {
                let v0 = a.v0;
                (1.0 - phi * (v * v + v0 * v0).sqrt() / d_c) * dt
            }
        }
    }
}

/// Arguments of the rate-and-state functions at one node (upstream
/// `getArguments()` tuple).
#[derive(Clone, Copy, Debug, Default)]
pub struct RsArgs {
    /// `mu_0`.
    pub mu_0: Real,
    /// `a`.
    pub a: Real,
    /// `b`.
    pub b: Real,
    /// `v_star`.
    pub v_star: Real,
    /// `phi_star`.
    pub phi_star: Real,
    /// `d_c`.
    pub d_c: Real,
    /// `v0`.
    pub v0: Real,
    /// `slip_velocity_norm`.
    pub slip_velocity: Real,
    /// `friction_state` θ.
    pub state: Real,
    /// `time_step`.
    pub time_step: Real,
}

/// Flash-heating rate-and-state law (upstream `FlashHeatingRaSFunctions`):
/// returns `(state, mu)` of a slipping node.
#[must_use]
pub fn flash_heating_slip(a: &RsArgs, mu_w: Real, v_w: Real) -> (Real, Real) {
    if a.slip_velocity < a.v_star {
        let state = a.d_c / a.v_star;
        // upstream evaluates log(v_star / v_star) literally (NaN for v_star = 0
        // or ±∞), kept for bitwise parity
        #[allow(clippy::eq_op)]
        let mu = a.mu_0 + a.a * ln(a.v_star / a.v_star) + a.b * ln(state / a.phi_star);
        (state, mu)
    } else {
        let state = a.state + (1.0 - a.state * a.slip_velocity / a.d_c) * a.time_step;
        let mu_rs = a.mu_0 + a.a * ln(a.slip_velocity / a.v_star) + a.b * ln(state / a.phi_star);
        let mu = mu_w + (mu_rs - mu_w) / (1.0 + a.d_c / (state * v_w));
        (state, mu)
    }
}

/// Friction law (upstream template `FrictionLaw`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FrictionLaw {
    /// `coulomb`: strength `μ p`.
    Coulomb,
    /// `linear_slip_weakening`: `μ` from the current slip (reset on
    /// sticking), `μ_s` while sticking.
    LinearSlipWeakening,
    /// `linear_slip_weakening_no_healing`: `μ` from the cumulative slip.
    LinearSlipWeakeningNoHealing,
    /// `linear_cohesive`: strength `max(τ_c − (τ_c − τ_r)²/(2G_c) s, τ_r)`
    /// from the cumulative slip.
    LinearCohesive,
    /// `rate_and_state:<coefficient>:<state>` (and the legacy
    /// `rate_and_state_n_shaped_*` keys).
    RateAndState {
        /// Coefficient function.
        coefficient: RsCoefficient,
        /// State evolution law.
        state: RsState,
    },
    /// `rate_and_state:flashheating`.
    RateAndStateFlashHeating,
}

impl FrictionLaw {
    /// Upstream class name (for printing).
    #[must_use]
    pub fn class_name(self) -> &'static str {
        match self {
            Self::Coulomb => "NTNFricLawCoulomb",
            Self::LinearSlipWeakening => "NTNFricLawLinearSlipWeakening",
            Self::LinearSlipWeakeningNoHealing => "NTNFricLawLinearSlipWeakeningNoHealing",
            Self::LinearCohesive => "NTNFricLawLinearCohesive",
            Self::RateAndState { .. } => "NTNFricLawRateAndState",
            Self::RateAndStateFlashHeating => "NTNFricLawRateAndStateFlashHeating",
        }
    }

    /// Whether the law derives from the Coulomb law (has `mu` and computes
    /// the strength as `μ p`).
    #[must_use]
    pub fn is_coulomb_family(self) -> bool {
        !matches!(self, Self::LinearCohesive)
    }

    /// Whether the law is a rate-and-state law.
    #[must_use]
    pub fn is_rate_and_state(self) -> bool {
        matches!(
            self,
            Self::RateAndState { .. } | Self::RateAndStateFlashHeating
        )
    }
}

/// Friction regularisation (upstream template `Regularisation`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Regularisation {
    /// `no_regularisation`: the frictional pressure is `‖p‖` in contact.
    NoRegularisation,
    /// `rubin_ampuero`: the frictional pressure relaxes towards `‖p‖`
    /// (`p ← (p + α‖p‖)/(1 + α)`, `α = Δt/t*`, unregularised when
    /// `t* = 0`).
    RubinAmpuero,
    /// `simplified_prakash_clifton`: the strength itself relaxes
    /// (`s ← (s + α s_law)/(1 + α)`).
    SimplifiedPrakashClifton,
}

impl Regularisation {
    /// Factory option name.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::NoRegularisation => "no_regularisation",
            Self::RubinAmpuero => "rubin_ampuero",
            Self::SimplifiedPrakashClifton => "simplified_prakash_clifton",
        }
    }

    /// Upstream class name (for printing).
    #[must_use]
    pub fn class_name(self) -> &'static str {
        match self {
            Self::NoRegularisation => "NTNFricRegNoRegularisation",
            Self::RubinAmpuero => "NTNFricRegRubinAmpuero",
            Self::SimplifiedPrakashClifton => "NTNFricRegSimplifiedPrakashClifton",
        }
    }

    /// Whether the regularisation has a `t_star` parameter.
    #[must_use]
    pub fn has_t_star(self) -> bool {
        !matches!(self, Self::NoRegularisation)
    }
}

/// Upstream `std::max(a, b)` (returns `a` unless `a < b`; NaN-propagation
/// as upstream).
#[inline]
#[must_use]
pub fn cpp_max(a: Real, b: Real) -> Real {
    if a < b { b } else { a }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(v: Real, state: Real) -> RsArgs {
        RsArgs {
            mu_0: 0.28,
            a: 0.005,
            b: 0.075,
            v_star: 1e-7,
            phi_star: 0.00033,
            d_c: 5e-7,
            v0: 1e-7,
            slip_velocity: v,
            state,
            time_step: 1e-8,
        }
    }

    /// Every `ln` of the rate-and-state laws is the
    /// oracle's glibc `log` (pure-Rust port), not the host libm. At
    /// `x = 0x3fe916959f0b7a44` glibc 2.41 returns `0xbfcf25aea18ec178`
    /// (recorded; the port's own tests check it against glibc) while the
    /// macOS 27 libm returns `0xbfcf25aea18ec177`.
    #[test]
    fn rate_and_state_logs_are_the_glibc_log() {
        let x = f64::from_bits(0x3fe9_1695_9f0b_7a44);
        let glibc_log_x = f64::from_bits(0xbfcf_25ae_a18e_c178);
        assert_eq!(
            akantu_core::random::glibc_libm::log(x).to_bits(),
            glibc_log_x.to_bits()
        );
        let mut a = args(1.0, x);
        a.mu_0 = 0.0;
        a.b = 1.0;
        a.v_star = 1.0;
        a.phi_star = 1.0;
        // Dieterich: 0 + a ln(V/V*) + b ln(θ/θ*) = a·ln 1 + ln x
        let mu = RsCoefficient::Dieterich.evaluate(&a);
        assert_eq!(mu.to_bits(), (0.0 + 0.005 * 0.0 + glibc_log_x).to_bits());
        // slip state law: −(θV/d_c) ln(θV/d_c) Δt with θV/d_c = x
        a.d_c = 1.0;
        a.time_step = 1.0;
        let dtheta = RsState::Slip.increment(&a);
        assert_eq!(
            dtheta.to_bits(),
            (-x * 1.0 / 1.0 * glibc_log_x * 1.0).to_bits()
        );
    }

    #[test]
    fn steady_state_of_the_ageing_law() {
        // steady state θ_ss = d_c / V: no state change
        let v = 2e-3;
        let a = args(v, 5e-7 / v);
        assert!(RsState::Ageing.increment(&a).abs() < 1e-24);
        // slip law: θV/d_c = 1 → ln 1 = 0
        assert_eq!(RsState::Slip.increment(&a), 0.0);
    }

    #[test]
    fn dieterich_steady_state_matches_the_test_data() {
        // test_ntn_friction data: fss of dieterich_ageing at V = 2e-3 with
        // θ = d_c/V is mu_0 + (a − b) ln(V/V*) + b ln(d_c/(V* θ*))
        let v = 2e-3;
        let a = args(v, 5e-7 / v);
        let mu = RsCoefficient::Dieterich.evaluate(&a);
        let closed = 0.28 + 0.005 * ln(v / 1e-7_f64) + 0.075 * ln((5e-7 / v) / 0.00033_f64);
        assert_eq!(mu, closed);
        // singularities at rest
        assert_eq!(
            RsCoefficient::Dieterich.evaluate(&args(0.0, 1.0)),
            Real::NEG_INFINITY
        );
        assert!(
            RsCoefficient::NShaped
                .evaluate(&args(0.0, 1.0))
                .is_infinite()
        );
        assert!(
            RsCoefficient::RevisedNShaped
                .evaluate(&args(0.0, 1.0))
                .is_finite()
        );
        assert!(
            RsCoefficient::RevisedDieterich
                .evaluate(&args(0.0, 1.0))
                .is_finite()
        );
        assert!(
            RsCoefficient::RegularizedDieterich
                .evaluate(&args(0.0, 1.0))
                .is_finite()
        );
        assert!(RsState::Slip.increment(&args(0.0, 1.0)).is_nan());
    }

    #[test]
    fn flash_heating_branches() {
        let a = args(5e-8, 1.0);
        let (s, mu) = flash_heating_slip(&a, 0.1, 0.2);
        assert_eq!(s, 5e-7 / 1e-7);
        assert_eq!(mu, 0.28 + 0.005 * 0.0 + 0.075 * ln(5.0 / 0.00033_f64));
        let a = args(1e-3, 1e-4);
        let (s, mu) = flash_heating_slip(&a, 0.1, 0.2);
        assert_eq!(s, 1e-4 + (1.0 - 1e-4 * 1e-3 / 5e-7) * 1e-8);
        let mu_rs = 0.28 + 0.005 * ln(1e-3 / 1e-7_f64) + 0.075 * ln(s / 0.00033);
        assert_eq!(mu, 0.1 + (mu_rs - 0.1) / (1.0 + 5e-7 / (s * 0.2)));
    }

    #[test]
    fn cpp_max_semantics() {
        assert_eq!(cpp_max(1.0, 2.0), 2.0);
        assert!(cpp_max(Real::NAN, 2.0).is_nan());
        assert_eq!(cpp_max(2.0, Real::NAN), 2.0);
    }
}
