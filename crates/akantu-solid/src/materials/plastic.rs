// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/model/solid_mechanics/materials/
// material_plastic/material_plastic.hh, material_plastic.cc,
// material_linear_isotropic_hardening.hh, material_linear_isotropic_hardening.cc,
// material_linear_isotropic_hardening_inline_impl.hh.

//! `MaterialPlastic` (MAT-019) and `plastic_linear_isotropic_hardening`
//! (MAT-020 small strain, MAT-021 finite deformation).
//!
//! # `MaterialPlastic<dim>` (base of J2 and Drucker–Prager)
//!
//! Parameters `h` (hardening modulus, 0) and `sigma_y` (yield stress, 0);
//! internals `iso_hardening` (1, history), `inelastic_strain` (`dim²`,
//! history), `plastic_energy` (1), `d_plastic_energy` (1)
//! ([`PlasticFields`]). Shared steps:
//!
//! * `computeStressAndInelasticStrainOnQuad`: `σ = σ_prev +
//!   MaterialElastic(Δ∇u − Δε_p) + Δσ_th` (upstream: no thermal term,
//!   UD-038) and
//!   `ε_p = ε_p,prev + Δε_p` ([`stress_and_inelastic_strain_on_quad`]);
//! * potential energy `ψ = ½ σ:(∇u − ε_p)` ([`plastic_potential_energy`]);
//! * after a converged step `w_p = ½(σ + σ_prev):(ε_p − ε_p,prev)`,
//!   `plastic_energy += w_p` ([`PlasticFields::update_energies`]);
//!   `getEnergy("plastic") = Σ ∫ plastic_energy`.
//!
//! # J2 radial return (`computeStressOnQuad`, small strain)
//!
//! Trial stress `σ_tr = MaterialElastic(Δ∇u, Δσ_th) + σ_prev`, von Mises
//! yield test `σ_eq − r − σ_y > 0`, hardening `r = r_prev + h·Δp`, final
//! stress [`stress_and_inelastic_strain_on_quad`]. By default:
//!
//! * the yield test reads the hardening committed at the last
//!   `savePreviousState` (UD-138, [`LawOptions::committed_trial_state`]),
//!   so repeated or rejected Newton trials of one step do not change later
//!   trials; upstream reads the current `iso_hardening`, which an earlier
//!   trial of the same step may have written;
//! * in 3-D the return is [`j2_small_strain_quad`] with upstream's
//!   operation order: `s = embed₃ₓ₃(σ_tr) − (I·tr σ_tr)/3`,
//!   `σ_eq = √(3/2·s:s)`, `Δp = (σ_eq − σ_y − r_prev)/(3μ + h)`,
//!   `Δε_p = ((s·3)/2·Δp)/σ_eq` when `|σ_eq| > ‖s‖·tol`
//!   (tol = `Math::getTolerance()`);
//! * in 1-D and 2-D the return is the consistent return of the represented
//!   3-D state, [`j2_reduced_quad`] (UD-137, [`J2ReducedReturn::Consistent`]):
//!   1-D `Δp = f/(E + h)`; plane strain the 3-D radial return with the
//!   out-of-plane stress `σ33` of the committed state; plane stress the
//!   Simo–Taylor projection with `σ33 = 0`. The returned stress lies on the
//!   yield surface. [`J2ReducedReturn::UpstreamTruncated`] runs
//!   [`j2_small_strain_quad`] in every dimension, as upstream: the in-plane
//!   trial embedded with `σ33 = 0` (plane strain included), the 3-D
//!   multiplier and the in-plane block of the flow.
//!
//! Tangent: the elastic tangent, as upstream (UD-079: a modified Newton
//! method; the iteration counts equal the oracle's). `plastic_mazars` runs
//! the same return on its own stress history (see `mazars.rs`, UD-139).
//! [`LawOptions::upstream`] reproduces upstream bit for bit.
//!
//! # Upstream defects
//!
//! * **UD-038** (category B): upstream's trial uses `σ_th,prev − σ_th` and
//!   the final stress drops the thermal increment;
//!   [`LawOptions::plastic_thermal_increment`] (default) uses `σ_th −
//!   σ_th,prev` in both. Without a temperature change both are
//!   bit-identical. With a thermal stress the potential energy is the
//!   stored elastic energy (UD-109 family, see [`plastic_potential_energy`]).
//! * **UD-137** (category B): the 1-D and 2-D return above, switch
//!   [`LawOptions::j2_reduced_return`].
//! * **UD-138** (category B): the committed trial state above;
//!   `committed_trial_state = false` reproduces upstream.
//! * **UD-030** (category B): with `finite_deformation`, upstream runs a
//!   finite-strain branch and then **overwrites** its result with the
//!   small-strain kernel on the raw `∇u` (keeping the hardening of the
//!   first branch for the yield test). The default returns
//!   [`Error::Unsupported`]; [`FiniteStrainPlasticity::UpstreamLegacy`]
//!   runs upstream's sequence ([`j2_finite_legacy_quad`]), bit for bit with
//!   the other switches of [`LawOptions::upstream`].
//! * **UD-079** (category C, no switch): the elastic tangent above.
//!
//! [`LawOptions::plastic_thermal_increment`]: crate::material::LawOptions::plastic_thermal_increment
//! [`LawOptions::committed_trial_state`]: crate::material::LawOptions::committed_trial_state
//! [`LawOptions::j2_reduced_return`]: crate::material::LawOptions::j2_reduced_return
//! [`LawOptions::upstream`]: crate::material::LawOptions::upstream

use std::sync::Arc;

use akantu_core::enums::MatrixType;
use akantu_core::math::{DEFAULT_TOLERANCE, are_float_equal};
use akantu_core::tensor::{Matrix, MatrixExt, SquareMatrixExt, as_matrix, as_matrix_mut};
use akantu_core::{
    Array, Element, ElementType, ElementTypeMapArray, Error, GhostType, Idx, Real, Result,
};
use akantu_model::InternalField;
use akantu_model::constitutive_law::{
    ConstitutiveLaw, InternalFieldMut, InternalFieldRef, LawContext, LawSpec, ModelServices,
};
use akantu_parser::param::{ParamDescriptor, ParameterAccess};

use super::damage::{DamageParent, integrate_field};
use super::elastic::{ElasticConstants, MaterialElastic};
use super::thermal::{ThermalEnergy, thermal_energy_correction};
use crate::material::{
    FiniteStrainPlasticity, J2ReducedReturn, LawOptions, Material, MaterialBase, MaterialFactory,
    MaterialParams,
};
use crate::material_tensor::{
    grad_u_to_f, grad_u_to_green, s_to_cauchy, tangent_stiffness_voigt_size,
};

const NG: GhostType = GhostType::NotGhost;

/// Internals added by upstream `MaterialPlastic` (registration order).
#[derive(Clone, Debug)]
pub struct PlasticFields {
    /// Isotropic hardening `r` (history).
    pub iso_hardening: InternalField<Real>,
    /// Plastic strain `ε_p` (`dim²`, column-major, history).
    pub inelastic_strain: InternalField<Real>,
    /// Accumulated plastic work density.
    pub plastic_energy: InternalField<Real>,
    /// Plastic work density of the last converged step.
    pub d_plastic_energy: InternalField<Real>,
}

impl PlasticFields {
    /// Upstream internal names, in registration order.
    pub const NAMES: [&'static str; 4] = [
        "iso_hardening",
        "inelastic_strain",
        "plastic_energy",
        "d_plastic_energy",
    ];

    /// Upstream `MaterialPlastic` constructor (history on `iso_hardening`
    /// and `inelastic_strain`).
    pub fn new(dim: usize) -> Result<Self> {
        let mut iso_hardening = InternalField::new(Self::NAMES[0], 1, 0.0)?;
        iso_hardening.initialize_history();
        let mut inelastic_strain = InternalField::new(Self::NAMES[1], dim * dim, 0.0)?;
        inelastic_strain.initialize_history();
        Ok(Self {
            iso_hardening,
            inelastic_strain,
            plastic_energy: InternalField::new(Self::NAMES[2], 1, 0.0)?,
            d_plastic_energy: InternalField::new(Self::NAMES[3], 1, 0.0)?,
        })
    }

    /// Visit the internals (registration order).
    pub fn for_each(&self, f: &mut dyn FnMut(&str, InternalFieldRef<'_>)) {
        f(Self::NAMES[0], InternalFieldRef::Real(&self.iso_hardening));
        f(
            Self::NAMES[1],
            InternalFieldRef::Real(&self.inelastic_strain),
        );
        f(Self::NAMES[2], InternalFieldRef::Real(&self.plastic_energy));
        f(
            Self::NAMES[3],
            InternalFieldRef::Real(&self.d_plastic_energy),
        );
    }

    /// Visit the internals mutably (registration order).
    pub fn for_each_mut(
        &mut self,
        f: &mut dyn FnMut(&str, InternalFieldMut<'_>) -> Result<()>,
    ) -> Result<()> {
        f(
            Self::NAMES[0],
            InternalFieldMut::Real(&mut self.iso_hardening),
        )?;
        f(
            Self::NAMES[1],
            InternalFieldMut::Real(&mut self.inelastic_strain),
        )?;
        f(
            Self::NAMES[2],
            InternalFieldMut::Real(&mut self.plastic_energy),
        )?;
        f(
            Self::NAMES[3],
            InternalFieldMut::Real(&mut self.d_plastic_energy),
        )
    }

    /// Upstream `MaterialPlastic::updateEnergies(type)` (not-ghost
    /// points): `w_p = ((σ + σ_prev):(ε_p − ε_p,prev))/2`,
    /// `plastic_energy += w_p`, `d_plastic_energy = w_p`. As upstream,
    /// `σ` is the second Piola–Kirchhoff stress with finite deformation
    /// and `σ_prev` the history of the Cauchy `stress`.
    ///
    /// # Errors
    /// Missing blocks or size mismatches.
    pub fn update_energies<const D: usize>(
        &mut self,
        base: &MaterialBase,
        ty: ElementType,
    ) -> Result<()> {
        let n = D * D;
        let sigma = base.sigma_field()?.get(ty, NG)?;
        let prev_sigma = base.stress.previous(ty, NG)?;
        let inel = self.inelastic_strain.get(ty, NG)?;
        let prev_inel = self.inelastic_strain.previous(ty, NG)?;
        let mut pe = self.plastic_energy.get_mut(ty, NG)?;
        let mut wp = self.d_plastic_energy.get_mut(ty, NG)?;
        let nq = sigma.len();
        if [
            prev_sigma.len(),
            inel.len(),
            prev_inel.len(),
            pe.len(),
            wp.len(),
        ]
        .iter()
        .any(|&l| l != nq)
        {
            return Err(Error::DimensionMismatch {
                context: "MaterialPlastic::updateEnergies",
                expected: nq,
                got: inel.len().min(pe.len()),
            });
        }
        let it = sigma
            .as_slice()
            .chunks_exact(n)
            .zip(prev_sigma.as_slice().chunks_exact(n))
            .zip(inel.as_slice().chunks_exact(n))
            .zip(prev_inel.as_slice().chunks_exact(n));
        for ((((s, ps), e), pe_), (pe, wp)) in
            it.zip(pe.as_mut_slice().iter_mut().zip(wp.as_mut_slice()))
        {
            let s: &Matrix<D, D> = as_matrix(s)?;
            let ps: &Matrix<D, D> = as_matrix(ps)?;
            let e: &Matrix<D, D> = as_matrix(e)?;
            let pe_: &Matrix<D, D> = as_matrix(pe_)?;
            let delta_strain = e.sub(pe_);
            let sigma_h = s.add(ps);
            *wp = sigma_h.double_dot(&delta_strain) / 2.;
            *pe += *wp;
        }
        Ok(())
    }

    /// Upstream `MaterialPlastic::getPlasticEnergy`.
    ///
    /// # Errors
    /// Integration errors.
    pub fn plastic_energy(&self, base: &MaterialBase, ctx: &LawContext<'_>) -> Result<Real> {
        integrate_field(base, &self.plastic_energy, ctx)
    }
}

/// Upstream `MaterialPlastic::computeStressAndInelasticStrainOnQuad`:
/// `σ = σ_prev + MaterialElastic(Δ∇u − Δε_p, σ_th = delta_sigma_th)`,
/// `ε_p = ε_p,prev + Δε_p`. Upstream passes no thermal stress
/// (`delta_sigma_th = 0`, UD-038).
#[must_use]
pub fn stress_and_inelastic_strain_on_quad<const D: usize>(
    c: &ElasticConstants<D>,
    delta_grad_u: &Matrix<D, D>,
    delta_inelastic_strain: &Matrix<D, D>,
    previous_sigma: &Matrix<D, D>,
    previous_inelastic_strain: &Matrix<D, D>,
    delta_sigma_th: Real,
) -> (Matrix<D, D>, Matrix<D, D>) {
    let delta_grad_u_elastic = delta_grad_u.sub(delta_inelastic_strain);
    let mut sigma_elastic = Matrix::<D, D>::zeros();
    c.stress_on_quad(&delta_grad_u_elastic, delta_sigma_th, &mut sigma_elastic);
    (
        previous_sigma.add(&sigma_elastic),
        previous_inelastic_strain.add(delta_inelastic_strain),
    )
}

/// Constants of the J2 return mapping.
#[derive(Clone, Copy, Debug)]
pub struct J2Constants<const D: usize> {
    /// Elastic kernel constants (`E`, `λ`, `μ`).
    pub elastic: ElasticConstants<D>,
    /// Hardening modulus `h`.
    pub h: Real,
    /// Yield stress `σ_y`.
    pub sigma_y: Real,
}

/// History of one quadrature point read by the J2 kernels.
#[derive(Clone, Copy, Debug)]
pub struct J2History<'a, const D: usize> {
    /// `previous_grad_u` (or the previous Green strain).
    pub previous_grad_u: &'a Matrix<D, D>,
    /// `previous_sigma` (history of the Cauchy `stress`).
    pub previous_sigma: &'a Matrix<D, D>,
    /// `previous_inelastic_strain`.
    pub previous_inelastic_strain: &'a Matrix<D, D>,
    /// `previous_iso_hardening`.
    pub previous_iso_hardening: Real,
}

/// Thermal stress increments of one point: the trial value and the one
/// added to the final stress (upstream: `σ_th,prev − σ_th` and `0`;
/// corrected, UD-038: `σ_th − σ_th,prev` in both).
#[derive(Clone, Copy, Debug)]
pub struct ThermalIncrements {
    /// Added to the diagonal of the trial stress.
    pub trial: Real,
    /// Added to the diagonal of the final stress.
    pub last: Real,
}

impl ThermalIncrements {
    /// The increments of a point with thermal stress `sigma_th` and its
    /// history `previous_sigma_th`.
    #[must_use]
    pub fn new(corrected: bool, sigma_th: Real, previous_sigma_th: Real) -> Self {
        if corrected {
            let d = sigma_th - previous_sigma_th;
            Self { trial: d, last: d }
        } else {
            Self {
                trial: previous_sigma_th - sigma_th,
                last: 0.,
            }
        }
    }
}

/// Upstream `MaterialLinearIsotropicHardening<dim>::computeStressOnQuad`
/// (infinitesimal deformations): returns `(σ, ε_p)` and updates `iso`
/// (the current `iso_hardening`, read by the yield test, then written).
#[must_use]
pub fn j2_small_strain_quad<const D: usize>(
    c: &J2Constants<D>,
    grad_u: &Matrix<D, D>,
    hist: &J2History<'_, D>,
    th: ThermalIncrements,
    iso: &mut Real,
) -> (Matrix<D, D>, Matrix<D, D>) {
    let grad_delta_u = grad_u.sub(hist.previous_grad_u);
    let mut sigma_tr = Matrix::<D, D>::zeros();
    c.elastic
        .stress_on_quad(&grad_delta_u, th.trial, &mut sigma_tr);
    let sigma_tr = sigma_tr.add(hist.previous_sigma);
    // full 3 × 3 deviator (σ_i3 = 0)
    let mut dev = Matrix::<3, 3>::zeros();
    for (j, col) in sigma_tr.iter().enumerate() {
        dev[j][..D].copy_from_slice(col);
    }
    let tr = sigma_tr.trace();
    for (j, col) in dev.iter_mut().enumerate() {
        for (i, v) in col.iter_mut().enumerate() {
            let id = if i == j { 1. } else { 0. };
            *v -= id * tr / 3.0;
        }
    }
    let s = dev.double_dot(&dev);
    let sigma_tr_dev_eff = (3. / 2. * s).sqrt();
    let initial_yielding = (sigma_tr_dev_eff - *iso - c.sigma_y) > 0.;
    let previous_iso = hist.previous_iso_hardening;
    let dp = if initial_yielding {
        (sigma_tr_dev_eff - c.sigma_y - previous_iso) / (3. * c.elastic.mu + c.h)
    } else {
        0.
    };
    *iso = previous_iso + c.h * dp;
    let mut delta_inelastic_strain = Matrix::<D, D>::zeros();
    if sigma_tr_dev_eff.abs() > dev.norm() * DEFAULT_TOLERANCE {
        for (j, col) in delta_inelastic_strain.iter_mut().enumerate() {
            for (i, v) in col.iter_mut().enumerate() {
                *v = dev[j][i] * 3. / 2. * dp / sigma_tr_dev_eff;
            }
        }
    }
    let delta_grad_u = grad_u.sub(hist.previous_grad_u);
    stress_and_inelastic_strain_on_quad(
        &c.elastic,
        &delta_grad_u,
        &delta_inelastic_strain,
        hist.previous_sigma,
        hist.previous_inelastic_strain,
        th.last,
    )
}

/// The consistent J2 return of a 1-D or 2-D point (UD-137;
/// [`J2ReducedReturn::Consistent`]). Same contract as
/// [`j2_small_strain_quad`]: returns `(σ, ε_p)` and writes `iso`; the yield
/// predicate compares with `r_pred` (the committed or the current
/// hardening, UD-138) and the return starts from `hist.previous_iso_hardening`.
///
/// * 1-D (uniaxial stress bar, `ν = 0`): `σ_tr = σ_prev + EΔε + Δσ_th`,
///   `Δp = (|σ_tr| − σ_y − r_prev)/(E + h)`, `Δε_p = Δp·sign σ_tr`.
/// * 2-D plane strain: the 3-D radial return of `σ_tr ⊕ σ33,tr` with
///   `σ33,tr = σ33,prev + λ tr Δε + Δσ_th` and the committed out-of-plane
///   stress `σ33,prev = λ tr ε_prev + 2μ tr ε_p,prev + σ_th,prev` (isochoric
///   flow from a virgin state: `ε_p33 = −tr ε_p`); the in-plane stress is
///   `σ_tr − 2μΔε_p` (`tr₃ Δε_p = 0`).
/// * 2-D plane stress: the Simo–Taylor projection (Simo & Taylor 1986; de
///   Souza Neto, Perić & Owen, *Computational Methods for Plasticity*, Box
///   9.4): with `a₁ = σ11 + σ22`, `a₂ = σ22 − σ11`, `a₃ = σ12` of the trial,
///   `ξ(Δγ) = a₁²/(6(1 + κΔγ)²) + (a₂²/2 + 2a₃²)/(1 + 2μΔγ)²`
///   (`κ = (2λ* + 2μ)/3 = E/(3(1 − ν))`), the consistency condition
///   `ξ/2 − (σ_y + r_prev + hΔp)²/3 = 0`, `Δp = Δγ√(2ξ/3)`, is solved by a
///   bracketed Newton iteration; `Δε_p = Δγ·s` (`s` the 3-D deviator with
///   `σ33 = 0`).
///
/// The final stress is rebuilt as upstream, `σ_prev + C:(Δε − Δε_p) +
/// Δσ_th,last` (plus `λ tr Δε_p` on the diagonal in plane strain, where
/// `tr₃ Δε_p = 0`), so elastic steps are bit-identical to upstream.
///
/// # Errors
/// [`Error::NotConverged`] if the plane-stress iteration does not converge
/// in 100 iterations; [`Error::Numerical`] for a non-finite result or a
/// yield surface without admissible stress (`σ_y + r ≤ 0`).
#[allow(clippy::too_many_lines, clippy::too_many_arguments)] // three reductions of one return mapping; upstream argument tuple plus the reduction data
pub fn j2_reduced_quad<const D: usize>(
    c: &J2Constants<D>,
    plane_stress: bool,
    grad_u: &Matrix<D, D>,
    hist: &J2History<'_, D>,
    previous_sigma_th: Real,
    th: ThermalIncrements,
    r_pred: Real,
    iso: &mut Real,
) -> Result<(Matrix<D, D>, Matrix<D, D>)> {
    let el = &c.elastic;
    let r_prev = hist.previous_iso_hardening;
    let delta_grad_u = grad_u.sub(hist.previous_grad_u);
    let mut sigma_tr = Matrix::<D, D>::zeros();
    el.stress_on_quad(&delta_grad_u, th.trial, &mut sigma_tr);
    let sigma_tr = sigma_tr.add(hist.previous_sigma);
    let mut delta_inelastic_strain = Matrix::<D, D>::zeros();
    let mut diag_extra = 0.;
    let mut dp = 0.;
    match D {
        1 => {
            let s = sigma_tr[0][0];
            if s.abs() - c.sigma_y - r_pred > 0. {
                dp = (s.abs() - c.sigma_y - r_prev) / (el.e + c.h);
                delta_inelastic_strain[0][0] = dp * s.signum();
            }
        }
        2 if !plane_stress => {
            let sigma33_prev = el.lambda * hist.previous_grad_u.trace()
                + 2. * el.mu * hist.previous_inelastic_strain.trace()
                + previous_sigma_th;
            let sigma33_tr = sigma33_prev + el.lambda * delta_grad_u.trace() + th.trial;
            let mut s3 = Matrix::<3, 3>::zeros();
            for (j, col) in sigma_tr.iter().enumerate() {
                s3[j][..D].copy_from_slice(col);
            }
            s3[2][2] = sigma33_tr;
            let tr = s3.trace();
            for (j, col) in s3.iter_mut().enumerate() {
                col[j] -= tr / 3.;
            }
            let sigma_eq = (1.5 * s3.double_dot(&s3)).sqrt();
            if sigma_eq - c.sigma_y - r_pred > 0. {
                dp = (sigma_eq - c.sigma_y - r_prev) / (3. * el.mu + c.h);
                let f = 1.5 * dp / sigma_eq;
                for (j, col) in delta_inelastic_strain.iter_mut().enumerate() {
                    for (i, v) in col.iter_mut().enumerate() {
                        *v = s3[j][i] * f;
                    }
                }
                // λ tr₃(Δε − Δε_p) = λ tr Δε: compensate the 2-D trace of Δε_p
                diag_extra = el.lambda * delta_inelastic_strain.trace();
            }
        }
        _ => {
            // plane stress (D = 2, σ33 = 0)
            let (s11, s22, s12) = (sigma_tr[0][0], sigma_tr[1][1], sigma_tr[1][0]);
            let xi_tr = 2. / 3. * (s11 * s11 - s11 * s22 + s22 * s22) + 2. * s12 * s12;
            let sigma_eq = (1.5 * xi_tr).sqrt();
            if sigma_eq - c.sigma_y - r_pred > 0. {
                let gamma = plane_stress_multiplier(c, s11, s22, s12, r_prev)?;
                let kappa = (2. * el.lambda + 2. * el.mu) / 3.;
                let f1 = 1. + kappa * gamma;
                let f2 = 1. + 2. * el.mu * gamma;
                let (a1, a2) = ((s11 + s22) / f1, (s22 - s11) / f2);
                let (n11, n22, n12) = ((a1 - a2) / 2., (a1 + a2) / 2., s12 / f2);
                let xi = 2. / 3. * (n11 * n11 - n11 * n22 + n22 * n22) + 2. * n12 * n12;
                dp = gamma * (2. / 3. * xi).sqrt();
                delta_inelastic_strain[0][0] = gamma * (2. * n11 - n22) / 3.;
                delta_inelastic_strain[1][1] = gamma * (2. * n22 - n11) / 3.;
                delta_inelastic_strain[1][0] = gamma * n12;
                delta_inelastic_strain[0][1] = gamma * n12;
            }
        }
    }
    *iso = r_prev + c.h * dp;
    let delta_elastic = delta_grad_u.sub(&delta_inelastic_strain);
    let mut sigma_el = Matrix::<D, D>::zeros();
    el.stress_on_quad(&delta_elastic, th.last + diag_extra, &mut sigma_el);
    let sigma = hist.previous_sigma.add(&sigma_el);
    let inel = hist.previous_inelastic_strain.add(&delta_inelastic_strain);
    if !sigma
        .iter()
        .flatten()
        .chain(inel.iter().flatten())
        .all(|v| v.is_finite())
        || !iso.is_finite()
    {
        return Err(Error::Numerical(
            "plastic_linear_isotropic_hardening: non-finite J2 return".into(),
        ));
    }
    Ok((sigma, inel))
}

/// The plastic multiplier `Δγ` of the plane-stress projection of
/// [`j2_reduced_quad`] for the trial `(s11, s22, s12)`: the root of
/// `F(Δγ) = ξ(Δγ)/2 − (σ_y + r_prev + hΔγ√(2ξ(Δγ)/3))²/3` (positive at 0
/// for a yielding trial), by Newton steps safeguarded with bisection on a
/// bracket found by doubling.
fn plane_stress_multiplier<const D: usize>(
    c: &J2Constants<D>,
    s11: Real,
    s22: Real,
    s12: Real,
    r_prev: Real,
) -> Result<Real> {
    let (mu, h) = (c.elastic.mu, c.h);
    let kappa = (2. * c.elastic.lambda + 2. * mu) / 3.;
    let a1sq = (s11 + s22) * (s11 + s22);
    let b = (s22 - s11) * (s22 - s11) / 2. + 2. * s12 * s12;
    let y0 = c.sigma_y + r_prev;
    // F and F' at γ
    let eval = |g: Real| -> (Real, Real) {
        let f1 = 1. + kappa * g;
        let f2 = 1. + 2. * mu * g;
        let xi = a1sq / (6. * f1 * f1) + b / (f2 * f2);
        let dxi = -a1sq * kappa / (3. * f1 * f1 * f1) - 4. * mu * b / (f2 * f2 * f2);
        let root = (2. / 3. * xi).sqrt();
        let hh = y0 + h * g * root;
        let ddp = if root > 0. {
            root + g * dxi / (3. * root)
        } else {
            0.
        };
        (xi / 2. - hh * hh / 3., dxi / 2. - 2. / 3. * hh * h * ddp)
    };
    let (f0, _) = eval(0.);
    // NaN-safe: a non-yielding (or NaN) consistency value gives no flow
    if f0.partial_cmp(&0.) != Some(std::cmp::Ordering::Greater) {
        return Ok(0.);
    }
    if y0.partial_cmp(&0.) != Some(std::cmp::Ordering::Greater) {
        return Err(Error::Numerical(
            "plastic_linear_isotropic_hardening: plane-stress return without admissible \
             stress (sigma_y + r <= 0)"
                .into(),
        ));
    }
    // bracket [lo, hi] with F(lo) > 0 > F(hi)
    let (mut lo, mut hi) = (0., 1. / (3. * mu));
    let mut n = 0;
    while eval(hi).0 > 0. {
        lo = hi;
        hi *= 2.;
        n += 1;
        if n > 200 || !hi.is_finite() {
            return Err(Error::Numerical(
                "plastic_linear_isotropic_hardening: plane-stress return not bracketed".into(),
            ));
        }
    }
    let mut g = lo;
    for _ in 0..100 {
        let (f, df) = eval(g);
        if f == 0. {
            return Ok(g);
        }
        if f > 0. {
            lo = g;
        } else {
            hi = g;
        }
        let mut next = if df < 0. { g - f / df } else { Real::NAN };
        if !(next > lo && next < hi) {
            next = 0.5 * (lo + hi);
        }
        if (next - g).abs() <= 4. * Real::EPSILON * next.abs() || hi - lo <= 4. * Real::EPSILON * hi
        {
            return Ok(next);
        }
        g = next;
    }
    Err(Error::NotConverged {
        threshold: 4. * Real::EPSILON,
        niter: 100,
        error: eval(g).0.abs(),
    })
}

/// Upstream `MaterialLinearIsotropicHardening<dim>::computeStressOnQuad`
/// with `F` (the finite-deformation branch, UD-030), called with the Green
/// strain `green` in place of `∇u` and the previous Green strain in
/// `hist.previous_grad_u`. Returns `(S, ε_p)` and writes `iso`. Only the
/// [`FiniteStrainPlasticity::UpstreamLegacy`] mode uses it (its result is
/// overwritten by [`j2_small_strain_quad`] there, as upstream).
#[must_use]
pub fn j2_finite_legacy_quad<const D: usize>(
    c: &J2Constants<D>,
    green: &Matrix<D, D>,
    f: &Matrix<D, D>,
    hist: &J2History<'_, D>,
    th: ThermalIncrements,
    iso: &mut Real,
) -> (Matrix<D, D>, Matrix<D, D>) {
    let mut dp = 0.0;
    let mut n = 0_u32;
    let grad_delta_u = green.sub(hist.previous_grad_u);
    let mut sigma_tr = Matrix::<D, D>::zeros();
    c.elastic
        .stress_on_quad(&grad_delta_u, th.trial, &mut sigma_tr);
    let sigma_tr = sigma_tr.add(hist.previous_sigma);
    let dev_dim = |m: &Matrix<D, D>| -> Matrix<D, D> {
        let tr = m.trace();
        std::array::from_fn(|j| {
            std::array::from_fn(|i| {
                let id = if i == j { 1. } else { 0. };
                m[j][i] - id * tr / 3.0
            })
        })
    };
    let sigma_tr_dev = dev_dim(&sigma_tr);
    let s = sigma_tr_dev.double_dot(&sigma_tr_dev);
    let sigma_tr_dev_eff = (3. / 2. * s).sqrt();
    let cauchy = s_to_cauchy(f, &sigma_tr, 1.0);
    let cauchy_dev = dev_dim(&cauchy);
    let cc = cauchy_dev.double_dot(&cauchy_dev);
    let cauchy_dev_eff = (3. / 2. * cc).sqrt();
    let iso_t = hist.previous_iso_hardening;
    *iso = iso_t;
    let mu3 = 3. * c.elastic.mu;
    let initial_yielding = (cauchy_dev_eff - *iso - c.sigma_y) > 0.;
    while initial_yielding && (cauchy_dev_eff - *iso - c.sigma_y).abs() > DEFAULT_TOLERANCE {
        let d_dp = (cauchy_dev_eff - mu3 * dp - *iso - c.sigma_y) / (3. * c.elastic.mu + c.h);
        dp += d_dp;
        *iso = iso_t + c.h * dp;
        n += 1;
        if d_dp < 1e-5 || n > 50 {
            break;
        }
    }
    let mut delta_inelastic_strain = Matrix::<D, D>::zeros();
    if sigma_tr_dev_eff.abs() > sigma_tr_dev.norm() * DEFAULT_TOLERANCE {
        let cauchy_dev_f = f.matmul(&cauchy_dev);
        let j = f.det();
        let mut constant = if are_float_equal(j, 0., DEFAULT_TOLERANCE) {
            0.
        } else {
            1. / j
        };
        constant *= 3. * dp / (2. * cauchy_dev_eff);
        let ft_c = f.tr_matmul(&cauchy_dev_f);
        delta_inelastic_strain = ft_c.scale(constant);
    }
    stress_and_inelastic_strain_on_quad(
        &c.elastic,
        &grad_delta_u,
        &delta_inelastic_strain,
        hist.previous_sigma,
        hist.previous_inelastic_strain,
        th.last,
    )
}

/// Upstream `MaterialPlastic::computePotentialEnergy(type)`: `ψ = ½ σ:(∇u −
/// ε_p)` per not-ghost point (`σ` = second Piola–Kirchhoff stress with
/// finite deformation). With [`ThermalEnergy::StoredElastic`] and a thermal
/// stress, the stored elastic energy `½(ε − ε_p − ε_th):C:(ε − ε_p − ε_th)`
/// = `ψ + ½ σ_th (tr(∇u − ε_p) − m αΔT)` (valid with the corrected thermal
/// increments, for which `σ = C:(ε − ε_p) + σ_th I`), times `1 − d` for a
/// damaged law (`damage`).
///
/// # Errors
/// Missing blocks or size mismatches.
pub fn plastic_potential_energy<const D: usize>(
    el: &mut MaterialElastic<D>,
    inelastic_strain: &InternalField<Real>,
    damage: Option<&InternalField<Real>>,
    ty: ElementType,
) -> Result<()> {
    let nq = el.base().grad_u.get(ty, NG)?.len();
    let density = plastic_energy_densities(el, inelastic_strain, damage, ty, 0..nq)?;
    let mut epot = el.base_mut().potential_energy.get_mut(ty, NG)?;
    if epot.len() != nq {
        return Err(Error::DimensionMismatch {
            context: "MaterialPlastic: potential_energy",
            expected: nq,
            got: epot.len(),
        });
    }
    epot.as_mut_slice().copy_from_slice(&density);
    Ok(())
}

/// The densities of [`plastic_potential_energy`] at the not-ghost points
/// `points` of `ty`, without storing them.
///
/// # Errors
/// Missing blocks, size mismatches or a range past the block.
pub fn plastic_energy_densities<const D: usize>(
    el: &MaterialElastic<D>,
    inelastic_strain: &InternalField<Real>,
    damage: Option<&InternalField<Real>>,
    ty: ElementType,
    points: std::ops::Range<usize>,
) -> Result<Vec<Real>> {
    let n = D * D;
    let stored = el.options.thermal_energy == ThermalEnergy::StoredElastic;
    let (plane_stress, alpha) = (el.plane_stress, el.thermal.alpha);
    // UD-137 (consistent plane-strain J2): the out-of-plane elastic strain
    // `ε_e33 = −ε_p33 = tr ε_p` adds `½ σ33 ε_e33` (and, with the stored
    // thermal convention, `½ σ_th tr ε_p` to the trace correction); zero
    // without plastic strain
    let out_of_plane = D == 2
        && !plane_stress
        && el.options.j2_reduced_return == J2ReducedReturn::Consistent
        // Drucker–Prager's reduced model has no out-of-plane plastic strain
        && el.base().law_type != "plastic_drucker_prager";
    let (lambda, mu) = (el.lambda(), el.mu());
    let sigma_th = el.sigma_th.get(ty, NG)?;
    let delta_t = el.delta_t.get(ty, NG)?;
    let inel = inelastic_strain.get(ty, NG)?;
    let dam = damage.map(|d| d.get(ty, NG)).transpose()?;
    let base = el.base();
    let grad_u = base.grad_u.get(ty, NG)?;
    let sigma = base.sigma_field()?.get(ty, NG)?;
    let nq = grad_u.len();
    if [sigma.len(), inel.len(), sigma_th.len(), delta_t.len()]
        .iter()
        .any(|&l| l != nq)
        || dam.as_ref().is_some_and(|d| d.len() != nq)
        || points.end > nq
    {
        return Err(Error::DimensionMismatch {
            context: "MaterialPlastic::computePotentialEnergy",
            expected: nq,
            got: sigma.len().min(inel.len()).min(points.end),
        });
    }
    let mut density = Vec::with_capacity(points.len());
    for q in points {
        let r = q * n..(q + 1) * n;
        let g: &Matrix<D, D> = as_matrix(&grad_u.as_slice()[r.clone()])?;
        let s: &Matrix<D, D> = as_matrix(&sigma.as_slice()[r.clone()])?;
        let e: &Matrix<D, D> = as_matrix(&inel.as_slice()[r])?;
        let elastic_strain = g.sub(e);
        let mut w = MaterialElastic::<D>::energy_on_quad(s, &elastic_strain);
        let th = sigma_th.as_slice()[q];
        let integrity = dam.as_ref().map_or(1., |d| 1. - d.as_slice()[q]);
        if out_of_plane {
            let tr_p = e.trace();
            if tr_p != 0. {
                let sigma33 = lambda * g.trace() + 2. * mu * tr_p + th;
                let thermal = if stored { th } else { 0. };
                w += integrity * 0.5 * tr_p * (sigma33 + thermal);
            }
        }
        if stored && th != 0. {
            w += integrity
                * thermal_energy_correction::<D>(
                    plane_stress,
                    th,
                    elastic_strain.trace(),
                    delta_t.as_slice()[q] * alpha,
                );
        }
        density.push(w);
    }
    Ok(density)
}

/// `plastic_linear_isotropic_hardening` (upstream
/// `MaterialLinearIsotropicHardening<DIM> : MaterialPlastic<DIM>`).
#[derive(Clone, Debug)]
pub struct MaterialLinearIsotropicHardening<const DIM: usize> {
    /// The elastic parent (parameters, thermal state, base internals,
    /// options).
    pub elastic: MaterialElastic<DIM>,
    /// Plastic internals.
    pub plastic: PlasticFields,
    /// Hardening modulus `h`.
    pub h: Real,
    /// Yield stress `sigma_y`.
    pub sigma_y: Real,
    /// Per-block results of the return mapping before they are committed
    /// (kept between steps: no allocation, no page faults per step).
    scratch: J2Scratch,
}

/// Scratch blocks of [`MaterialLinearIsotropicHardening::j2_block`]: the
/// new stress, hardening and plastic strain of one `(type, ghost)` block,
/// committed with [`InternalField::commit_overwrite`] (which leaves
/// allocations of the same size here for the next call).
#[derive(Clone, Debug, Default)]
struct J2Scratch {
    sigma: Array<Real>,
    iso: Array<Real>,
    inel: Array<Real>,
}

/// Size `a` to `nq` tuples of `nbc` values (contents unspecified unless
/// the caller fills them).
fn sized(a: &mut Array<Real>, nq: usize, nbc: usize) -> Result<()> {
    if a.nb_component() != nbc {
        *a = Array::new(nbc);
    }
    a.try_resize(nq, 0.0)
}

impl<const DIM: usize> MaterialLinearIsotropicHardening<DIM> {
    /// New law (upstream constructor chain) with the registered type
    /// `plastic_linear_isotropic_hardening`.
    pub fn new(spec: &LawSpec, mesh: Option<&akantu_mesh::Mesh>) -> Result<Self> {
        let mut elastic = MaterialElastic::<DIM>::new(spec, mesh)?;
        elastic.base_mut().law_type = "plastic_linear_isotropic_hardening".into();
        Ok(Self {
            elastic,
            plastic: PlasticFields::new(DIM)?,
            h: 0.,
            sigma_y: 0.,
            scratch: J2Scratch::default(),
        })
    }

    /// The constants of the return mapping.
    #[must_use]
    pub fn j2_constants(&self) -> J2Constants<DIM> {
        J2Constants {
            elastic: self.elastic.constants(),
            h: self.h,
            sigma_y: self.sigma_y,
        }
    }

    fn check_finite(&self) -> Result<()> {
        if self.elastic.base().finite_deformation
            && self.elastic.options.finite_strain_plasticity == FiniteStrainPlasticity::Unsupported
        {
            return Err(Error::Unsupported(
                "plastic_linear_isotropic_hardening with finite_deformation: no reviewed \
                 finite-strain J2 formulation (UD-030); FiniteStrainPlasticity::UpstreamLegacy \
                 reproduces upstream"
                    .into(),
            ));
        }
        Ok(())
    }

    /// The J2 loop over one block: the small-strain kernel per point, after
    /// the finite-strain branch when `finite_legacy` (UD-030 legacy).
    /// `previous_sigma` replaces the history of the `stress` internal as the
    /// committed stress of the return (the effective stress of
    /// `plastic_mazars`, UD-139); the stored stress is the returned one.
    #[allow(clippy::too_many_lines)] // one upstream loop with its options
    pub(crate) fn j2_block(
        &mut self,
        ty: ElementType,
        ghost: GhostType,
        finite_legacy: bool,
        previous_sigma: Option<Array<Real>>,
    ) -> Result<()> {
        let c = self.j2_constants();
        let options = self.elastic.options;
        let corrected = options.plastic_thermal_increment;
        let committed = options.committed_trial_state;
        let reduced = DIM < 3 && options.j2_reduced_return == J2ReducedReturn::Consistent;
        let plane_stress = self.elastic.plane_stress;
        let n = DIM * DIM;
        let Self {
            elastic,
            plastic,
            scratch,
            ..
        } = self;
        let mut green_out = Vec::new();
        {
            // the inputs read in place (current and previous values); the
            // results go to the scratch blocks, committed below
            let base = elastic.base();
            let sigma_th = elastic.sigma_th.get(ty, ghost)?;
            let prev_sigma_th = elastic.sigma_th.previous(ty, ghost)?;
            let grad_u = base.grad_u.get(ty, ghost)?;
            let prev_grad_u = base.grad_u.previous(ty, ghost)?;
            let prev_sigma = match &previous_sigma {
                Some(p) => p,
                None => base.stress.previous(ty, ghost)?,
            };
            let sigma_cur = base.sigma_field()?.get(ty, ghost)?;
            let iso_cur = plastic.iso_hardening.get(ty, ghost)?;
            let prev_iso = plastic.iso_hardening.previous(ty, ghost)?;
            let inel_cur = plastic.inelastic_strain.get(ty, ghost)?;
            let prev_inel = plastic.inelastic_strain.previous(ty, ghost)?;
            let nq = grad_u.len();
            if [
                prev_grad_u.len(),
                prev_sigma.len(),
                sigma_cur.len(),
                iso_cur.len(),
                prev_iso.len(),
                inel_cur.len(),
                prev_inel.len(),
                sigma_th.len(),
                prev_sigma_th.len(),
            ]
            .iter()
            .any(|&l| l != nq)
                || prev_sigma.nb_component() != n
            {
                return Err(Error::DimensionMismatch {
                    context: "MaterialLinearIsotropicHardening::computeStress",
                    expected: nq,
                    got: sigma_cur.len().min(inel_cur.len()).min(iso_cur.len()),
                });
            }
            // every point writes all `n` values of its stress and plastic
            // strain: the scratch needs the current values only for a block
            // of another width (never for the registered fields)
            sized(&mut scratch.sigma, nq, sigma_cur.nb_component())?;
            if sigma_cur.nb_component() != n {
                scratch.sigma.copy_from(&sigma_cur)?;
            }
            sized(&mut scratch.inel, nq, inel_cur.nb_component())?;
            if inel_cur.nb_component() != n {
                scratch.inel.copy_from(&inel_cur)?;
            }
            // the hardening is read before it is written (UD-138)
            sized(&mut scratch.iso, nq, iso_cur.nb_component())?;
            scratch.iso.copy_from(&iso_cur)?;
            let (sigma, iso, inel) = (&mut scratch.sigma, &mut scratch.iso, &mut scratch.inel);
            for q in 0..nq {
                let r = q * n..(q + 1) * n;
                let g: &Matrix<DIM, DIM> = as_matrix(&grad_u.as_slice()[r.clone()])?;
                let pg: &Matrix<DIM, DIM> = as_matrix(&prev_grad_u.as_slice()[r.clone()])?;
                let ps: &Matrix<DIM, DIM> = as_matrix(&prev_sigma.as_slice()[r.clone()])?;
                let pe: &Matrix<DIM, DIM> = as_matrix(&prev_inel.as_slice()[r.clone()])?;
                let th = ThermalIncrements::new(
                    corrected,
                    sigma_th.as_slice()[q],
                    prev_sigma_th.as_slice()[q],
                );
                let r_prev = prev_iso.as_slice()[q];
                let r_iso = &mut iso.as_mut_slice()[q];
                // UD-138: the yield predicate reads the committed hardening
                if committed {
                    *r_iso = r_prev;
                }
                let hist = J2History {
                    previous_grad_u: pg,
                    previous_sigma: ps,
                    previous_inelastic_strain: pe,
                    previous_iso_hardening: r_prev,
                };
                if finite_legacy {
                    let green = grad_u_to_green(g);
                    let prev_green = grad_u_to_green(pg);
                    let f = grad_u_to_f(g);
                    let hist_f = J2History {
                        previous_grad_u: &prev_green,
                        ..hist
                    };
                    let (s, e) = j2_finite_legacy_quad(&c, &green, &f, &hist_f, th, r_iso);
                    sigma.as_mut_slice()[r.clone()].copy_from_slice(s.as_flattened());
                    inel.as_mut_slice()[r.clone()].copy_from_slice(e.as_flattened());
                    green_out.push(green);
                }
                let (s, e) = if reduced {
                    let r_pred = *r_iso;
                    j2_reduced_quad(
                        &c,
                        plane_stress,
                        g,
                        &hist,
                        prev_sigma_th.as_slice()[q],
                        th,
                        r_pred,
                        r_iso,
                    )?
                } else {
                    j2_small_strain_quad(&c, g, &hist, th, r_iso)
                };
                let s_out: &mut Matrix<DIM, DIM> =
                    as_matrix_mut(&mut sigma.as_mut_slice()[r.clone()])?;
                *s_out = s;
                inel.as_mut_slice()[r].copy_from_slice(e.as_flattened());
            }
        }
        let base = elastic.base_mut();
        base.sigma_field_mut()?
            .commit_overwrite(ty, ghost, &mut scratch.sigma)?;
        if finite_legacy && let Some(gs) = &mut base.green_strain {
            let mut out = gs.get_mut(ty, ghost)?;
            for (o, g) in out.as_mut_slice().chunks_exact_mut(n).zip(&green_out) {
                o.copy_from_slice(g.as_flattened());
            }
        }
        plastic
            .iso_hardening
            .commit_overwrite(ty, ghost, &mut scratch.iso)?;
        plastic
            .inelastic_strain
            .commit_overwrite(ty, ghost, &mut scratch.inel)?;
        Ok(())
    }
}

impl<const DIM: usize> MaterialParams for MaterialLinearIsotropicHardening<DIM> {
    fn params_base(&self) -> &MaterialBase {
        self.elastic.base()
    }
    fn params_base_mut(&mut self) -> &mut MaterialBase {
        self.elastic.base_mut()
    }
    fn own_params(&self) -> &'static [ParamDescriptor<Self>] {
        if DIM == 2 {
            Self::PARAMS_2D
        } else {
            Self::PARAMS
        }
    }
    fn update_params(&mut self) -> Result<()> {
        MaterialParams::update_params(&mut self.elastic)
    }
}

impl<const DIM: usize> MaterialLinearIsotropicHardening<DIM> {
    const PARAMS: &'static [ParamDescriptor<Self>] = crate::elastic_parent_params!(
        Self,
        [elastic],
        [
            crate::real_param!(Self, "h", ParameterAccess::PARSMOD, "Hardening  modulus", h),
            crate::real_param!(
                Self,
                "sigma_y",
                ParameterAccess::PARSMOD,
                "Yield stress",
                sigma_y
            ),
        ]
    );
    const PARAMS_2D: &'static [ParamDescriptor<Self>] = crate::elastic_parent_params!(plane_stress Self, [elastic], [
        crate::real_param!(Self, "h", ParameterAccess::PARSMOD, "Hardening  modulus", h),
        crate::real_param!(Self, "sigma_y", ParameterAccess::PARSMOD, "Yield stress", sigma_y),
    ]);
}

crate::impl_material_registry!(MaterialLinearIsotropicHardening<DIM>, const DIM);

impl<const DIM: usize> ConstitutiveLaw for MaterialLinearIsotropicHardening<DIM> {
    fn name(&self) -> &str {
        self.elastic.name()
    }
    fn law_id(&self) -> &str {
        self.elastic.law_id()
    }
    fn law_type(&self) -> &str {
        self.elastic.law_type()
    }
    fn element_filter(&self) -> &ElementTypeMapArray<Idx> {
        self.elastic.element_filter()
    }
    fn element_filter_mut(&mut self) -> &mut ElementTypeMapArray<Idx> {
        self.elastic.element_filter_mut()
    }
    /// The base's `(dimension, kind)` restriction for the law's own
    /// internals too (upstream `registerInternal` / `InternalField` with the
    /// law's dimension and `_ek_regular`; row CL-002).
    fn internal_restriction(&self) -> Option<akantu_model::internal_field::TypeRestriction> {
        Some(Material::base(self).internal_restriction())
    }
    fn for_each_internal(&self, f: &mut dyn FnMut(&str, InternalFieldRef<'_>)) {
        self.elastic.for_each_internal(f);
        self.plastic.for_each(f);
    }
    fn for_each_internal_mut(
        &mut self,
        f: &mut dyn FnMut(&str, InternalFieldMut<'_>) -> Result<()>,
    ) -> Result<()> {
        self.elastic.for_each_internal_mut(f)?;
        self.plastic.for_each_mut(f)
    }
}

impl<const DIM: usize> Material for MaterialLinearIsotropicHardening<DIM> {
    fn base(&self) -> &MaterialBase {
        self.elastic.base()
    }
    fn base_mut(&mut self) -> &mut MaterialBase {
        self.elastic.base_mut()
    }

    /// Upstream `MaterialElastic::initMaterial`.
    fn init_material(&mut self, ctx: &LawContext<'_>) -> Result<()> {
        self.elastic.base_mut().init_history()?;
        let blocks = self.elastic.base().blocks(ctx);
        self.resize_internals(&blocks)?;
        self.update_params()?;
        self.elastic.base_mut().set_init();
        if DIM == 1 {
            self.elastic.thermal.nu = 0.;
        }
        self.update_params()
    }

    /// Upstream `MaterialLinearIsotropicHardening::computeStress`:
    /// `MaterialElastic::computeStress` (thermal refresh; its elastic stress
    /// is overwritten) then the J2 loop.
    fn compute_stress(
        &mut self,
        _ctx: &LawContext<'_>,
        ty: ElementType,
        ghost: GhostType,
    ) -> Result<()> {
        self.check_finite()?;
        self.elastic.compute_thermal(ty, ghost)?;
        let finite = self.elastic.base().finite_deformation;
        self.j2_block(ty, ghost, finite, None)
    }

    /// `compute_stress` rewrites the thermal blocks first (the stress,
    /// hardening and plastic strain are committed from scratch blocks).
    fn prepare_stress_overwrite(&mut self, ty: ElementType, ghost: GhostType) {
        if self.check_finite().is_ok() {
            self.elastic.prepare_thermal_overwrite(ty, ghost);
        }
    }

    /// Upstream: the elastic tangent per point, then
    /// `was_stiffness_assembled = true`.
    fn compute_tangent_moduli(
        &mut self,
        _ctx: &LawContext<'_>,
        ty: ElementType,
        ghost: GhostType,
        tangent: &mut Array<Real>,
    ) -> Result<()> {
        self.check_finite()?;
        let nq = self.elastic.base().grad_u.get(ty, ghost)?.len();
        let v = tangent_stiffness_voigt_size(DIM);
        crate::material::check_tangent(tangent, nq, v)?;
        for t in tangent.as_mut_slice().chunks_exact_mut(v * v) {
            self.elastic.tangent_on_quad(t);
        }
        self.elastic.set_stiffness_assembled();
        Ok(())
    }

    fn compute_potential_energy(&mut self, _ctx: &LawContext<'_>, ty: ElementType) -> Result<()> {
        plastic_potential_energy(&mut self.elastic, &self.plastic.inelastic_strain, None, ty)
    }

    fn energy_of(&mut self, ctx: &LawContext<'_>, energy_id: &str) -> Result<Real> {
        match energy_id {
            "plastic" => self.plastic.plastic_energy(self.elastic.base(), ctx),
            "potential" => self.potential_energy(ctx),
            _ => Ok(0.0),
        }
    }

    /// The law's own density `½σ:(∇u − ε_p)` (+ thermal correction) at the
    /// element's points (upstream inherits `MaterialElastic`'s `½σ:∇u`,
    /// UD-148).
    fn potential_energy_by_element(
        &self,
        ctx: &LawContext<'_>,
        element: Element,
    ) -> Result<Vec<Real>> {
        let points = super::damage::element_points(self.elastic.base(), ctx, element)?;
        plastic_energy_densities(
            &self.elastic,
            &self.plastic.inelastic_strain,
            None,
            element.ty(),
            points,
        )
    }

    fn update_energies(&mut self, _ctx: &LawContext<'_>, ty: ElementType) -> Result<()> {
        self.plastic.update_energies::<DIM>(self.elastic.base(), ty)
    }

    fn push_wave_speed(&self, element: Element) -> Result<Real> {
        self.elastic.push_wave_speed(element)
    }

    fn shear_wave_speed(&self, element: Element) -> Result<Real> {
        self.elastic.shear_wave_speed(element)
    }

    fn tangent_type(&self) -> MatrixType {
        MatrixType::Symmetric
    }

    fn has_stiffness_matrix_changed(&self) -> bool {
        self.elastic.has_stiffness_matrix_changed()
    }

    fn mark_stiffness_changed(&mut self) {
        self.elastic.mark_stiffness_changed();
    }

    fn options(&self) -> Option<&LawOptions> {
        Some(&self.elastic.options)
    }

    fn options_mut(&mut self) -> Option<&mut LawOptions> {
        Some(&mut self.elastic.options)
    }

    fn compute_all_cauchy_stresses(&mut self, ghost: GhostType) -> Result<()> {
        self.elastic.compute_all_cauchy_stresses(ghost)
    }
}

impl<const DIM: usize> DamageParent<DIM> for MaterialLinearIsotropicHardening<DIM> {
    fn elastic(&self) -> &MaterialElastic<DIM> {
        &self.elastic
    }

    fn elastic_mut(&mut self) -> &mut MaterialElastic<DIM> {
        &mut self.elastic
    }

    /// Upstream `MaterialLinearIsotropicHardening::computeStressOnQuad`
    /// (small strain) per point, as called by `MaterialMazars` — also with
    /// `finite_deformation` (upstream `plastic_mazars` never reaches the
    /// finite branch; the caller rejects that mode unless legacy).
    fn stress_kernel(&mut self, ty: ElementType, ghost: GhostType) -> Result<()> {
        self.j2_block(ty, ghost, false, None)
    }

    fn stress_kernel_from(
        &mut self,
        ty: ElementType,
        ghost: GhostType,
        previous_sigma: Option<Array<Real>>,
    ) -> Result<()> {
        self.j2_block(ty, ghost, false, previous_sigma)
    }

    fn inelastic_strain(&self) -> Option<&InternalField<Real>> {
        Some(&self.plastic.inelastic_strain)
    }

    fn parent_update_energies(&mut self, ty: ElementType) -> Result<()> {
        self.plastic.update_energies::<DIM>(self.elastic.base(), ty)
    }

    fn parent_potential_energy(
        &mut self,
        damage: &InternalField<Real>,
        ty: ElementType,
    ) -> Result<()> {
        plastic_potential_energy(
            &mut self.elastic,
            &self.plastic.inelastic_strain,
            Some(damage),
            ty,
        )
    }

    fn parent_energy_densities(
        &self,
        damage: &InternalField<Real>,
        ty: ElementType,
        points: std::ops::Range<usize>,
    ) -> Result<Vec<Real>> {
        plastic_energy_densities(
            &self.elastic,
            &self.plastic.inelastic_strain,
            Some(damage),
            ty,
            points,
        )
    }

    fn parent_energy(&self, ctx: &LawContext<'_>, energy_id: &str) -> Result<Option<Real>> {
        if energy_id == "plastic" {
            return self
                .plastic
                .plastic_energy(self.elastic.base(), ctx)
                .map(Some);
        }
        Ok(None)
    }

    fn parent_update_params(&mut self) -> Result<()> {
        MaterialParams::update_params(self)
    }
}

/// Register `plastic_linear_isotropic_hardening` (upstream
/// `instantiateMaterial<MaterialLinearIsotropicHardening>`).
pub fn register(factory: &mut MaterialFactory) -> Result<()> {
    factory.register(
        "plastic_linear_isotropic_hardening",
        Arc::new(|spec: &LawSpec, services: &dyn ModelServices| {
            let mesh = Some(services.mesh());
            akantu_core::with_dim!(spec.spatial_dimension, D => {
                Box::new(MaterialLinearIsotropicHardening::<D>::new(spec, mesh)?) as Box<dyn Material>
            })
        }),
    )
}
