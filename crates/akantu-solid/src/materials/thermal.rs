// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/model/solid_mechanics/materials/
// material_thermal.hh, material_thermal.cc.

//! Thermal pre-step of the isotropic elastic law (upstream
//! `MaterialThermal<dim>`, base class of `MaterialElastic<dim>`).
//!
//! Upstream registers the parameters `E` (0), `nu` (0.5), `alpha` (0) and
//! the internal-field parameter `delta_T` (uniform temperature change, the
//! default value of the internal `delta_T`), and the internals `delta_T`,
//! `sigma_th` (history) and `epsilon_th` (history). Per quadrature point
//! (`computeStressOnQuad`, evaluated with upstream's operand order):
//!
//! | case | `epsilon_th` | `sigma_th` |
//! |---|---|---|
//! | 1-D | `ΔT·α` | `−E·ε_th` |
//! | 2-D plane stress | `ΔT·α` | `(−E/(1 − ν))·ε_th` |
//! | 2-D plane strain | `(ΔT·α)·(1 + ν)` | `((−E/(1 − 2ν))·ΔT)·α` |
//! | 3-D | `ΔT·α` (**UD-038**, corrected default) or `(ΔT·α)·(1 + ν)` ([`ThermalStrain::Upstream`]) | `((−E/(1 − 2ν))·ΔT)·α` |
//!
//! **UD-038** (category B: the 3-D eigenstrain is corrected, the valid
//! reduced plane-strain and plane-stress formulas are kept): the isotropic 3-D thermal
//! eigenstrain is `αΔT·I`; upstream stores `(1 + ν)αΔT`, which is the
//! *effective in-plane* eigenstrain of the 2-D plane-strain reduction and
//! wrong in 3-D (it feeds the phase-field strain transfer). The thermal
//! **stress** offset is correct in every case and unchanged; the elastic
//! stress uses only `sigma_th`, so the fix changes the `epsilon_th`
//! internal of 3-D laws only. [`ThermalStrain::Upstream`] reproduces the
//! reference for differential tests.
//!
//! # Thermal potential energy (UD-109)
//!
//! With `∇u` the (eigen-corrected) displacement gradient, `ε = sym ∇u` and
//! a fixed temperature change, the stress is `σ = C:ε + σ_th·I`. Upstream's
//! energy density `W = ½ σ:∇u` is **not** a potential of that stress:
//! `∂W/∂ε = C:ε + ½σ_th·I` (the linear thermal term is halved).
//! The corrected default ([`ThermalEnergy::StoredElastic`]) is the
//! stored elastic energy of the mechanical strain
//!
//! ```text
//! W = ½ (ε − ε_th) : C : (ε − ε_th)
//!   = ½ ε:C:ε + σ_th·tr ε + g(T),        g(T) = ½ ε_th:C:ε_th = −½·m·σ_th·αΔT ≥ 0
//!   = ½ σ:∇u + ½ σ_th·(tr ∇u − m·αΔT)    (form evaluated, from upstream's term)
//! ```
//!
//! with `ε_th = αΔT` along the `m` directions of free thermal expansion of
//! the 3-D stress state that each reduction represents, for which
//! `C:ε_th = −σ_th·I` holds exactly:
//!
//! | case | 3-D state | `m` | `g(T)` |
//! |---|---|---|---|
//! | 1-D | uniaxial stress bar | 1 | `½E(αΔT)²` |
//! | 2-D plane stress | `σ_i3 = 0` (ε33 free) | 2 | `E/(1 − ν)·(αΔT)²` |
//! | 2-D plane strain | `ε_i3 = 0`, energy per unit thickness including the constraint stress `σ33 = λ tr ε + σ_th` | 3 | `(9/2)K(αΔT)²` |
//! | 3-D | — | 3 | `(9/2)K(αΔT)²` |
//!
//! so `∂W/∂ε = σ` at fixed temperature, `W ≥ 0` for a positive-definite `C`,
//! and `W = 0` at the stress-free state (free expansion `∇u = αΔT·I`); in
//! plane strain the in-plane free expansion `(1 + ν)αΔT` leaves the
//! constraint stress `σ33 = −EαΔT` and `W = ½E(αΔT)²`.
//! With `finite_deformation` (St-Venant–Kirchhoff, `S = C:E + σ_th·I`) the
//! same expression is evaluated on the Green–Lagrange strain `E` and the
//! second Piola–Kirchhoff stress (`∂W/∂E = S`, energy per reference
//! volume). The `αΔT` used is `delta_T·alpha` of the quadrature point,
//! independent of the [`ThermalStrain`] convention of the `epsilon_th`
//! internal. Without a thermal stress (`σ_th = 0`) both conventions are
//! bit-identical (`½ σ:∇u`). [`ThermalEnergy::Upstream`] keeps `½ σ:∇u`
//! for differential tests.

use akantu_core::Real;

/// Potential-energy convention of `elastic` with a temperature change
/// (UD-109, category B).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ThermalEnergy {
    /// Stored elastic energy `½ (ε − ε_th):C:(ε − ε_th)`, a potential of
    /// the computed stress at fixed temperature (default, UD-109 fix).
    #[default]
    StoredElastic,
    /// Upstream: `½ σ:∇u` (compatibility, differential tests against the
    /// unpatched oracle).
    Upstream,
}

/// Number `m` of directions of free thermal expansion of the 3-D stress
/// state represented by a law of dimension `DIM` (module docs): 1 (1-D
/// bar), 2 (2-D plane stress), 3 (2-D plane strain, 3-D).
#[must_use]
pub fn thermal_expansion_directions<const DIM: usize>(plane_stress: bool) -> Real {
    match DIM {
        1 => 1.,
        2 if plane_stress => 2.,
        _ => 3.,
    }
}

/// The term `½ σ_th·(tr ∇u − m·αΔT)` that turns upstream's `½ σ:∇u` into
/// the stored elastic energy ([`ThermalEnergy::StoredElastic`]); `trace` is
/// `tr ∇u` (or `tr E` with finite deformation), `alpha_delta_t` the free
/// thermal strain `αΔT` of the quadrature point.
#[must_use]
pub fn thermal_energy_correction<const DIM: usize>(
    plane_stress: bool,
    sigma_th: Real,
    trace: Real,
    alpha_delta_t: Real,
) -> Real {
    let m = thermal_expansion_directions::<DIM>(plane_stress);
    0.5 * sigma_th * (trace - m * alpha_delta_t)
}

/// Convention for the 3-D thermal eigenstrain (UD-038).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ThermalStrain {
    /// `ε_th = αΔT` in 3-D (UD-038 fix, default).
    #[default]
    Corrected,
    /// Upstream: `ε_th = (1 + ν)αΔT` in 3-D (compatibility, differential
    /// tests against the unpatched oracle).
    Upstream,
}

/// Thermal parameters of an isotropic law.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ThermalParams {
    /// Young's modulus `E`.
    pub e: Real,
    /// Poisson's ratio `nu`.
    pub nu: Real,
    /// Thermal expansion coefficient `alpha`.
    pub alpha: Real,
}

impl Default for ThermalParams {
    /// Upstream defaults: `E = 0`, `nu = 0.5`, `alpha = 0`.
    fn default() -> Self {
        Self {
            e: 0.0,
            nu: 0.5,
            alpha: 0.0,
        }
    }
}

/// Upstream `MaterialThermal<dim>::computeStressOnQuad` (and the 2-D
/// `computeStressOnQuadPlaneStress` when `plane_stress`): returns
/// `(epsilon_th, sigma_th)` for the temperature change `delta_t`.
#[must_use]
pub fn thermal_on_quad<const DIM: usize>(
    p: &ThermalParams,
    plane_stress: bool,
    convention: ThermalStrain,
    delta_t: Real,
) -> (Real, Real) {
    match DIM {
        1 => {
            let epsilon = delta_t * p.alpha;
            (epsilon, -p.e * epsilon)
        }
        2 if plane_stress => {
            let epsilon = delta_t * p.alpha;
            (epsilon, -p.e / (1. - p.nu) * epsilon)
        }
        _ => {
            let sigma = -p.e / (1. - 2. * p.nu) * delta_t * p.alpha;
            let epsilon = if DIM == 3 && convention == ThermalStrain::Corrected {
                delta_t * p.alpha
            } else {
                delta_t * p.alpha * (1. + p.nu)
            };
            (epsilon, sigma)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Port of `test_material_thermal.cc` (`ThermalComputeStress` for
    /// `MaterialThermal<1|2|3>`, tolerance 1e-14 as upstream).
    #[test]
    fn test_material_thermal_compute_stress() {
        let p = ThermalParams {
            e: 1.,
            nu: 0.3,
            alpha: 2.,
        };
        let delta_t = 1.;
        // 3D
        let (_, s) = thermal_on_quad::<3>(&p, false, ThermalStrain::Corrected, delta_t);
        let solution = -p.e / (1. - 2. * p.nu) * p.alpha * delta_t;
        assert!((s - solution).abs() < 1e-14);
        // 2D plane strain, then plane stress
        let (_, s) = thermal_on_quad::<2>(&p, false, ThermalStrain::Corrected, delta_t);
        assert!((s - solution).abs() < 1e-14);
        let (_, s) = thermal_on_quad::<2>(&p, true, ThermalStrain::Corrected, delta_t);
        let solution = -p.e / (1. - p.nu) * p.alpha * delta_t;
        assert!((s - solution).abs() < 1e-14);
        // 1D
        let (_, s) = thermal_on_quad::<1>(&p, false, ThermalStrain::Corrected, delta_t);
        assert!((s - (-p.e * p.alpha * delta_t)).abs() < 1e-14);
    }

    /// UD-038 independent checks: in 3-D, `σ_th = −3KαΔT` is the stress of
    /// a fully constrained heated solid (`K = E/(3(1 − 2ν))`), and the
    /// corrected eigenstrain `αΔT` is the free volumetric expansion that
    /// makes the stress vanish (`3K·(tr ε_free / 3) = −σ_th`).
    #[test]
    fn ud038_free_and_constrained_expansion() {
        let p = ThermalParams {
            e: 210e9,
            nu: 0.3,
            alpha: 1.2e-5,
        };
        let dt = 80.0;
        let k = p.e / (3. * (1. - 2. * p.nu));
        let (eps, sig) = thermal_on_quad::<3>(&p, false, ThermalStrain::Corrected, dt);
        assert!((sig - (-3. * k * p.alpha * dt)).abs() <= 1e-12 * sig.abs());
        // free expansion ε = eps·I: isotropic elastic stress 3K·eps + σ_th = 0
        assert!((3. * k * eps + sig).abs() <= 1e-9 * sig.abs());
        // upstream convention differs by (1 + ν) in 3-D only
        let (eps_up, sig_up) = thermal_on_quad::<3>(&p, false, ThermalStrain::Upstream, dt);
        assert_eq!(sig_up, sig);
        assert_eq!(eps_up, dt * p.alpha * (1. + p.nu));
        // 2-D plane strain keeps the reduced (1 + ν) eigenstrain: with the
        // plane-strain in-plane stiffness (λ + μ per unit trace) it yields
        // σ_th: (λ + μ)·2·(1+ν)αΔT = 3KαΔT.
        let (eps2, _) = thermal_on_quad::<2>(&p, false, ThermalStrain::Corrected, dt);
        let lambda = p.nu * p.e / ((1. + p.nu) * (1. - 2. * p.nu));
        let mu = p.e / (2. * (1. + p.nu));
        assert!(((lambda + mu) * 2. * eps2 + sig).abs() <= 1e-9 * sig.abs());
    }
}
