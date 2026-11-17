// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/model/solid_mechanics/materials/
// material_damage/material_damage.hh, material_damage_tmpl.hh.

//! `MaterialDamage<dim, Parent>` (MAT-010): the isotropic-damage base of
//! `marigo`, `mazars` and `plastic_mazars` (and of the phase-field
//! material).
//!
//! Upstream adds to its parent (`MaterialElastic` or, for
//! `plastic_mazars`, `MaterialLinearIsotropicHardening`):
//!
//! * internals `damage` (1, **history**), `damage dissipated energy` (1)
//!   and `integral of sigma` (1) — [`DamageFields`];
//! * `computeTangentModuli`: the parent's tangent per quadrature point,
//!   then `tangent.array() *= (1 − d)` with the **current** damage
//!   ([`DamageFields::scale_tangent`]); `hasStiffnessMatrixChanged()` is
//!   always `true`;
//! * `updateEnergies(type)` after a converged step: the parent's update,
//!   then the law's `computePotentialEnergy(type)`, then per quadrature
//!   point `Δint = (σ + σ_prev) : (∇u − ∇u_prev) / 2`, `∫σ += Δint`,
//!   `e_diss = ∫σ − ψ` ([`DamageFields::update_energies`]);
//! * `getEnergy("dissipated")` = `Σ_types ∫ e_diss` over the not-ghost
//!   filter, anything else the parent's energy.
//!
//! The port keeps this structure with two corrections of the energy
//! update, on by default ([`DamageWork`]):
//!
//! * with `finite_deformation`, upstream's `"sigma"` is the second
//!   Piola–Kirchhoff stress while `"previous_sigma"` is the history of the
//!   Cauchy `stress` internal (`Material::getArguments`), paired with
//!   `Δ∇u`, which is not conjugate to `S`. The default
//!   ([`FiniteStrainWork::Conjugate`], UD-158, UD-159) uses
//!   `½(S_{n+1} + S_n):ΔE_Green`; [`FiniteStrainWork::UpstreamMixed`]
//!   ([`LawOptions::finite_strain_work`]) reproduces upstream. Small strain
//!   is unaffected;
//! * the reversible thermal work `∫ ∂ψ/∂θ dθ` is added to `∫σ`, so heating
//!   an undamaged body dissipates nothing (UD-160,
//!   [`LawOptions::damage_thermal_work`], active with
//!   [`ThermalEnergy::StoredElastic`]); `false` keeps upstream's `∫σ:dε − ψ`.
//!
//! # Thermal state (UD-038, UD-109)
//!
//! Upstream's damage laws call only the point kernels of their parents, so
//! the thermal stress is never refreshed (UD-038, category B).
//! [`LawOptions::damage_thermal_refresh`] (default `true`) runs the thermal
//! pre-step first. With a thermal stress, the stored energy of the damaged
//! solid `(1 − d)·½(ε − ε_th):C:(ε − ε_th)` is upstream's `½ σ:∇u` plus
//! `(1 − d)·½ σ_th (tr ∇u − m αΔT)` (the UD-109 correction of
//! [`super::thermal`] scaled by the integrity); it is used for the potential
//! energy (and, for `marigo`, the energy release rate `Y`) when
//! [`LawOptions::thermal_energy`] is [`ThermalEnergy::StoredElastic`].
//! Without a thermal stress both conventions are bit-identical to upstream.
//!
//! [`LawOptions::damage_thermal_refresh`]: crate::material::LawOptions::damage_thermal_refresh
//! [`LawOptions::thermal_energy`]: crate::material::LawOptions::thermal_energy
//! [`LawOptions::finite_strain_work`]: crate::material::LawOptions::finite_strain_work
//! [`LawOptions::damage_thermal_work`]: crate::material::LawOptions::damage_thermal_work

use akantu_core::exec::{Exec, Tuples, TuplesRef};
use akantu_core::tensor::{Matrix, MatrixExt, as_matrix};
use akantu_core::{ElementType, Error, GhostType, Real, Result};
use akantu_model::InternalField;
use akantu_model::constitutive_law::{InternalFieldMut, InternalFieldRef, LawContext};

use super::elastic::MaterialElastic;
use super::thermal::{ThermalEnergy, thermal_energy_correction};
use crate::material::{Material, MaterialBase};
use crate::material_tensor::grad_u_to_green;

const NG: GhostType = GhostType::NotGhost;

/// C++ `std::max(a, b)` = `(a < b) ? b : a`: returns `a` when the operands
/// compare equal (signed zeros) or either is NaN, unlike [`f64::max`].
#[inline]
#[must_use]
pub fn std_max(a: Real, b: Real) -> Real {
    if a < b { b } else { a }
}

/// C++ `std::min(a, b)` = `(b < a) ? b : a`.
#[inline]
#[must_use]
pub fn std_min(a: Real, b: Real) -> Real {
    if b < a { b } else { a }
}

/// Work-conjugate strain measure of the finite-strain energy update of
/// `MaterialDamage::updateEnergies` (UD-158, UD-159).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FiniteStrainWork {
    /// Default: the second Piola–Kirchhoff stress `S` (current and its own
    /// history) paired with the Green–Lagrange strain increment,
    /// `½(S_{n+1} + S_n):(E_{n+1} − E_n)` (`S` is conjugate to `E`).
    #[default]
    Conjugate,
    /// Upstream (`Material::getArguments`, `material_damage_tmpl.hh`):
    /// `½(S_{n+1} + σ_n):(∇u_{n+1} − ∇u_n)` with `σ_n` the history of the
    /// Cauchy `stress` internal, which these laws never write.
    UpstreamMixed,
}

/// Energy bookkeeping of one `MaterialDamage::updateEnergies` call.
#[derive(Clone, Copy, Debug, Default)]
pub struct DamageWork<'a> {
    /// Strain measure and previous stress with `finite_deformation`
    /// (small strain: always `½(σ + σ_prev):Δ∇u`, upstream's arithmetic).
    pub finite_strain: FiniteStrainWork,
    /// Per not-ghost point, the reversible thermal work of the step
    /// `Δw_th ≈ ∫ ∂ψ/∂θ dθ` (trapezoidal), added to `∫σ` so that the
    /// dissipated energy `∫σ − ψ` does not absorb the change of the stored
    /// energy caused by the temperature (UD-160); `None`
    /// keeps upstream's mechanical work only.
    pub thermal: Option<&'a [Real]>,
}

impl<'a> DamageWork<'a> {
    /// The bookkeeping selected by `options` with the thermal increments
    /// `thermal` (computed by the law when
    /// [`thermal_work_enabled`] holds).
    #[must_use]
    pub fn new(options: &crate::material::LawOptions, thermal: Option<&'a [Real]>) -> Self {
        Self {
            finite_strain: options.finite_strain_work,
            thermal,
        }
    }
}

/// Whether the reversible thermal work enters the damage bookkeeping:
/// `LawOptions::damage_thermal_work` and a potential energy that
/// is the stored energy ([`ThermalEnergy::StoredElastic`]); with upstream's
/// `½σ:∇u` the thermal and dissipative parts cannot be separated and the
/// upstream bookkeeping is kept.
#[must_use]
pub fn thermal_work_enabled(options: &crate::material::LawOptions) -> bool {
    options.damage_thermal_work && options.thermal_energy == ThermalEnergy::StoredElastic
}

/// `c` of `σ_th = −c·αΔT` (upstream `MaterialThermal`): `E` (1-D),
/// `E/(1 − ν)` (plane stress), `E/(1 − 2ν)` (plane strain, 3-D); `C:ε_th =
/// −σ_th·I` holds along the `m` directions of free expansion
/// ([`super::thermal`]).
#[must_use]
pub fn thermal_modulus<const D: usize>(e: Real, nu: Real, plane_stress: bool) -> Real {
    match D {
        1 => e,
        2 if plane_stress => e / (1. - nu),
        _ => e / (1. - 2. * nu),
    }
}

/// Reversible thermal work increments of a damage law on an
/// isotropic elastic (or J2 plastic) parent, per not-ghost point of `ty`.
///
/// The stored energy is `ψ = (1 − d)·½(ε_e − θ I_m):C:(ε_e − θ I_m)` with
/// `θ = αΔT = −σ_th/c` ([`thermal_modulus`]) and `ε_e` the elastic strain
/// of the represented 3-D state, so `∂ψ/∂θ = −tr₃σ`, the trace of the 3-D
/// stress (`tr σ` in 1-D, plane stress and 3-D; plane strain adds
/// `σ33 = (1 − d)(λ tr₃ε_e + 2μ ε_e33 + σ_th)`). The increment of the step
/// is the trapezoidal `½(∂ψ/∂θ|_{n+1} + ∂ψ/∂θ|_n)(θ_{n+1} − θ_n)`, exact for
/// the quadratic `ψ` at fixed damage and plastic strain (so a thermoelastic
/// cycle without damage dissipates nothing). Points whose `σ_th` did not
/// change give exactly 0 (bitwise neutral).
///
/// `inelastic` is the parent's plastic strain with `out_of_plane` (the
/// plane-strain `ε_p33 = −tr ε_p` of the consistent J2 return, UD-137).
///
/// # Errors
/// Missing blocks or size mismatches.
pub fn elastic_family_thermal_work<const D: usize>(
    el: &MaterialElastic<D>,
    damage: &InternalField<Real>,
    inelastic: Option<(&InternalField<Real>, bool)>,
    ty: ElementType,
) -> Result<Vec<Real>> {
    use akantu_core::tensor::SquareMatrixExt;
    let n = D * D;
    let plane_strain = D == 2 && !el.plane_stress;
    let c = thermal_modulus::<D>(el.thermal.e, el.thermal.nu, el.plane_stress);
    let (lambda, mu) = (el.lambda(), el.mu());
    let base = el.base();
    let finite = base.finite_deformation;
    let sth = (el.sigma_th.get(ty, NG)?, el.sigma_th.previous(ty, NG)?);
    let dam = (damage.get(ty, NG)?, damage.previous(ty, NG)?);
    let gu = (base.grad_u.get(ty, NG)?, base.grad_u.previous(ty, NG)?);
    let sig = (
        base.sigma_field()?.get(ty, NG)?,
        base.sigma_field()?.previous(ty, NG)?,
    );
    let inel = match inelastic {
        Some((f, oop)) => Some((f.get(ty, NG)?, f.previous(ty, NG)?, oop)),
        None => None,
    };
    let nq = sth.0.len();
    if [
        sth.1.len(),
        dam.0.len(),
        dam.1.len(),
        gu.0.len(),
        gu.1.len(),
    ]
    .iter()
    .chain(&[sig.0.len(), sig.1.len()])
    .any(|&l| l != nq)
        || inel
            .as_ref()
            .is_some_and(|(a, b, _)| a.len() != nq || b.len() != nq)
    {
        return Err(Error::DimensionMismatch {
            context: "MaterialDamage: thermal work",
            expected: nq,
            got: gu.0.len().min(sig.0.len()).min(dam.0.len()),
        });
    }
    let mut out = vec![0.0; nq];
    for (q, w) in out.iter_mut().enumerate() {
        let (th_new, th_old) = (sth.0.as_slice()[q], sth.1.as_slice()[q]);
        if th_new == th_old || c == 0. {
            continue;
        }
        let r = q * n..(q + 1) * n;
        let trace3 = |stress: &[Real], g: &[Real], d: Real, th: Real, p: Option<&[Real]>| {
            let s: &Matrix<D, D> = as_matrix(stress)?;
            let mut t = s.trace();
            if plane_strain {
                let g: &Matrix<D, D> = as_matrix(g)?;
                let strain = if finite { grad_u_to_green(g) } else { *g };
                let (tr_e3, e33) = match (p, inel.as_ref().map(|x| x.2)) {
                    (Some(p), Some(oop)) => {
                        let tp = as_matrix::<D, D>(p)?.trace();
                        let e33 = if oop { tp } else { 0. };
                        (strain.trace() - tp + e33, e33)
                    }
                    _ => (strain.trace(), 0.),
                };
                t += (1. - d) * (lambda * tr_e3 + 2. * mu * e33 + th);
            }
            Ok::<Real, Error>(t)
        };
        let t_new = trace3(
            &sig.0.as_slice()[r.clone()],
            &gu.0.as_slice()[r.clone()],
            dam.0.as_slice()[q],
            th_new,
            inel.as_ref().map(|x| &x.0.as_slice()[r.clone()]),
        )?;
        let t_old = trace3(
            &sig.1.as_slice()[r.clone()],
            &gu.1.as_slice()[r.clone()],
            dam.1.as_slice()[q],
            th_old,
            inel.as_ref().map(|x| &x.1.as_slice()[r.clone()]),
        )?;
        // ½(φ_new + φ_old)·Δθ with φ = −tr₃σ and Δθ = −Δσ_th/c
        *w = 0.5 * (t_new + t_old) * ((th_new - th_old) / c);
    }
    Ok(out)
}

/// Reversible thermal work increments of a law whose stored energy
/// is a function of the thermal-corrected strain `ε − θ·I_D` (`θ` the
/// internal `epsilon_th`, `phasefield`): `∂ψ/∂θ = −tr σ`, increment
/// `−½(tr σ_{n+1} + tr σ_n)(θ_{n+1} − θ_n)`; 0 where `θ` did not change.
///
/// # Errors
/// Missing blocks or size mismatches.
pub fn shifted_strain_thermal_work<const D: usize>(
    base: &MaterialBase,
    epsilon_th: &InternalField<Real>,
    ty: ElementType,
) -> Result<Vec<Real>> {
    use akantu_core::tensor::SquareMatrixExt;
    let n = D * D;
    let th = (epsilon_th.get(ty, NG)?, epsilon_th.previous(ty, NG)?);
    let sig = (
        base.sigma_field()?.get(ty, NG)?,
        base.sigma_field()?.previous(ty, NG)?,
    );
    let nq = th.0.len();
    if th.1.len() != nq || sig.0.len() != nq || sig.1.len() != nq {
        return Err(Error::DimensionMismatch {
            context: "MaterialDamage: thermal work",
            expected: nq,
            got: sig.0.len().min(th.1.len()),
        });
    }
    let mut out = vec![0.0; nq];
    for (q, w) in out.iter_mut().enumerate() {
        let (t_new, t_old) = (th.0.as_slice()[q], th.1.as_slice()[q]);
        if t_new == t_old {
            continue;
        }
        let r = q * n..(q + 1) * n;
        let s_new: &Matrix<D, D> = as_matrix(&sig.0.as_slice()[r.clone()])?;
        let s_old: &Matrix<D, D> = as_matrix(&sig.1.as_slice()[r])?;
        *w = -0.5 * (s_new.trace() + s_old.trace()) * (t_new - t_old);
    }
    Ok(out)
}

/// Internals added by upstream `MaterialDamage` (registration order).
#[derive(Clone, Debug)]
pub struct DamageFields {
    /// Damage `d ∈ [0, 1]` (with history).
    pub damage: InternalField<Real>,
    /// Dissipated energy density (`damage dissipated energy`).
    pub dissipated_energy: InternalField<Real>,
    /// `∫σ:dε` accumulated over converged steps (`integral of sigma`).
    pub int_sigma: InternalField<Real>,
}

impl DamageFields {
    /// Upstream internal names, in registration order.
    pub const NAMES: [&'static str; 3] =
        ["damage", "damage dissipated energy", "integral of sigma"];

    /// Upstream `MaterialDamage` constructor: the three internals, history
    /// on `damage`.
    pub fn new() -> Result<Self> {
        let mut damage = InternalField::new(Self::NAMES[0], 1, 0.0)?;
        damage.initialize_history();
        Ok(Self {
            damage,
            dissipated_energy: InternalField::new(Self::NAMES[1], 1, 0.0)?,
            int_sigma: InternalField::new(Self::NAMES[2], 1, 0.0)?,
        })
    }

    /// Visit the internals (registration order).
    pub fn for_each(&self, f: &mut dyn FnMut(&str, InternalFieldRef<'_>)) {
        f(Self::NAMES[0], InternalFieldRef::Real(&self.damage));
        f(
            Self::NAMES[1],
            InternalFieldRef::Real(&self.dissipated_energy),
        );
        f(Self::NAMES[2], InternalFieldRef::Real(&self.int_sigma));
    }

    /// Visit the internals mutably (registration order).
    pub fn for_each_mut(
        &mut self,
        f: &mut dyn FnMut(&str, InternalFieldMut<'_>) -> Result<()>,
    ) -> Result<()> {
        f(Self::NAMES[0], InternalFieldMut::Real(&mut self.damage))?;
        f(
            Self::NAMES[1],
            InternalFieldMut::Real(&mut self.dissipated_energy),
        )?;
        f(Self::NAMES[2], InternalFieldMut::Real(&mut self.int_sigma))
    }

    /// Upstream `MaterialDamage::computeTangentModuliOnQuad`:
    /// `tangent.array() *= (1 − d)` (the factor is evaluated once).
    #[inline]
    pub fn scale_tangent(tangent: &mut [Real], damage: Real) {
        let f = 1. - damage;
        for t in tangent {
            *t *= f;
        }
    }

    /// Upstream `MaterialDamage::computeTangentModuli` for one block:
    /// `parent_tangent` fills the parent's tangent of one point, then the
    /// damage scaling.
    ///
    /// # Errors
    /// Size mismatches between `tangent` and the damage block.
    pub fn tangent(
        &self,
        ty: ElementType,
        ghost: GhostType,
        voigt: usize,
        tangent: &mut akantu_core::Array<Real>,
        parent_tangent: &dyn Fn(&mut [Real]),
    ) -> Result<()> {
        let d = self.damage.get(ty, ghost)?;
        crate::material::check_tangent(tangent, d.len(), voigt)?;
        for (t, &dam) in tangent
            .as_mut_slice()
            .chunks_exact_mut(voigt * voigt)
            .zip(d.as_slice())
        {
            parent_tangent(t);
            Self::scale_tangent(t, dam);
        }
        Ok(())
    }

    /// The part of upstream `MaterialDamage::updateEnergies(type)` after
    /// the parent's update and the law's `computePotentialEnergy(type)`:
    /// per not-ghost quadrature point
    /// `Δint = ((σ + σ_prev) : Δε) / 2 [+ Δw_th]`, `∫σ += Δint`,
    /// `e_diss = ∫σ − ψ` (see [`DamageWork`] for the strain measure, the
    /// previous stress and the reversible thermal work `Δw_th`). Shared by
    /// every `MaterialDamage` law (`marigo`, `mazars`, `plastic_mazars`,
    /// `phasefield`).
    ///
    /// # Errors
    /// Missing blocks or size mismatches.
    ///
    /// Every point from its own values: parts on `exec`'s threads.
    pub fn update_energies<const D: usize>(
        &mut self,
        exec: &akantu_core::exec::Exec,
        base: &MaterialBase,
        ty: ElementType,
        work: DamageWork<'_>,
    ) -> Result<()> {
        let n = D * D;
        let finite = base.finite_deformation;
        let conjugate = finite && work.finite_strain == FiniteStrainWork::Conjugate;
        let grad_u = base.grad_u.get(ty, NG)?;
        let prev_grad_u = base.grad_u.previous(ty, NG)?;
        let sigma = base.sigma_field()?.get(ty, NG)?;
        // upstream `previous_sigma` is the history of the Cauchy `stress`
        // even with finite deformation (`Material::getArguments`)
        let prev_sigma = if finite && !conjugate {
            base.stress.previous(ty, NG)?
        } else {
            base.sigma_field()?.previous(ty, NG)?
        };
        let epot = base.potential_energy.get(ty, NG)?;
        let mut edis = self.dissipated_energy.get_mut(ty, NG)?;
        let mut ints = self.int_sigma.get_mut(ty, NG)?;
        let nq = grad_u.len();
        if [
            prev_grad_u.len(),
            sigma.len(),
            prev_sigma.len(),
            epot.len(),
            edis.len(),
            ints.len(),
        ]
        .iter()
        .any(|&l| l != nq)
            || work.thermal.is_some_and(|t| t.len() != nq)
        {
            return Err(Error::DimensionMismatch {
                context: "MaterialDamage::updateEnergies",
                expected: nq,
                got: sigma.len().min(edis.len()).min(ints.len()),
            });
        }
        if [&grad_u, &sigma].iter().any(|a| a.nb_component() != n)
            || prev_grad_u.nb_component() != n
            || prev_sigma.nb_component() != n
        {
            return Err(Error::DimensionMismatch {
                context: "MaterialDamage::updateEnergies: components",
                expected: n,
                got: sigma.nb_component(),
            });
        }
        let items = (
            (
                TuplesRef::new(grad_u.as_slice(), n),
                TuplesRef::new(prev_grad_u.as_slice(), n),
                TuplesRef::new(sigma.as_slice(), n),
                TuplesRef::new(prev_sigma.as_slice(), n),
            ),
            TuplesRef::new(epot.as_slice(), 1),
            Tuples::new(edis.as_mut_slice(), 1),
            Tuples::new(ints.as_mut_slice(), 1),
        );
        exec.try_for_each(
            items,
            2048,
            |first, ((g, pg, s, ps), ep, mut ed, mut is)| {
                let it = g
                    .as_slice()
                    .chunks_exact(n)
                    .zip(pg.as_slice().chunks_exact(n))
                    .zip(s.as_slice().chunks_exact(n))
                    .zip(ps.as_slice().chunks_exact(n));
                for (k, (((((g, pg), s), ps), &ep), (ed, is))) in it
                    .zip(ep.as_slice())
                    .zip(ed.as_mut_slice().iter_mut().zip(is.as_mut_slice()))
                    .enumerate()
                {
                    energy_on_quad::<D>(
                        conjugate,
                        work.thermal.map(|t| t[first + k]),
                        g,
                        pg,
                        s,
                        ps,
                        ep,
                        ed,
                        is,
                    )?;
                }
                Ok(())
            },
        )
    }
}

/// [`DamageFields::update_energies`] of one point: `∫σ += ½(σ + σ_prev):Δ`
/// (plus the thermal work `thermal` when not zero), `e_diss = ∫σ − ψ`.
#[allow(clippy::too_many_arguments)] // one point's values
#[inline]
fn energy_on_quad<const D: usize>(
    conjugate: bool,
    thermal: Option<Real>,
    g: &[Real],
    pg: &[Real],
    s: &[Real],
    ps: &[Real],
    ep: Real,
    ed: &mut Real,
    is: &mut Real,
) -> Result<()> {
    let g: &Matrix<D, D> = as_matrix(g)?;
    let pg: &Matrix<D, D> = as_matrix(pg)?;
    let s: &Matrix<D, D> = as_matrix(s)?;
    let ps: &Matrix<D, D> = as_matrix(ps)?;
    // conjugate pair of the second Piola–Kirchhoff stress: the
    // Green strain; upstream and small strain:
    // the displacement gradient (σ symmetric: = Δε : σ)
    let delta = if conjugate {
        grad_u_to_green(g).sub(&grad_u_to_green(pg))
    } else {
        g.sub(pg)
    };
    let sigma_h = s.add(ps);
    let dint = sigma_h.double_dot(&delta) / 2.;
    *is += dint;
    if let Some(w) = thermal
        && w != 0.
    {
        *is += w;
    }
    *ed = *is - ep;
    Ok(())
}

impl DamageFields {
    /// Upstream `MaterialDamage::getDissipatedEnergy`: `Σ_types ∫ e_diss`
    /// over the not-ghost filter blocks of the law's dimension.
    ///
    /// # Errors
    /// Integration errors of the model services.
    pub fn dissipated_energy(&self, base: &MaterialBase, ctx: &LawContext<'_>) -> Result<Real> {
        integrate_field(base, &self.dissipated_energy, ctx)
    }
}

/// `Σ_types ∫ field` over the not-ghost filter blocks of the law's
/// dimension (upstream `fem.integrate(field(type), type, _not_ghost,
/// filter)` summed from `0.`; empty blocks add nothing, as upstream's
/// `+0` never changes a sum that starts at `+0`).
///
/// # Errors
/// Missing blocks or integration errors.
pub fn integrate_field(
    base: &MaterialBase,
    field: &InternalField<Real>,
    ctx: &LawContext<'_>,
) -> Result<Real> {
    let mut sum = 0.;
    for ty in base.filter_types(NG) {
        let f = base.element_filter.get(ty, NG)?;
        if f.is_empty() {
            continue;
        }
        let v = field.get(ty, NG)?;
        if v.is_empty() {
            continue;
        }
        sum += ctx
            .services
            .integrate_scalar(v.as_slice(), ty, NG, f.as_slice())?;
    }
    Ok(sum)
}

/// The energy densities of the not-ghost points `points` of `ty` of a
/// damage law on an elastic parent (see [`elastic_damage_potential_energy`]).
///
/// # Errors
/// Missing blocks, size mismatches or a range past the block.
pub fn elastic_damage_energy_densities<const D: usize>(
    el: &MaterialElastic<D>,
    damage: &InternalField<Real>,
    ty: ElementType,
    points: std::ops::Range<usize>,
) -> Result<Vec<Real>> {
    elastic_damage_energy_densities_with(Exec::serial_ref(), el, damage, ty, points)
}

/// [`elastic_damage_energy_densities`] with the points split over
/// `exec`'s threads (every density from its own point's values).
///
/// # Errors
/// As [`elastic_damage_energy_densities`].
pub fn elastic_damage_energy_densities_with<const D: usize>(
    exec: &Exec,
    el: &MaterialElastic<D>,
    damage: &InternalField<Real>,
    ty: ElementType,
    points: std::ops::Range<usize>,
) -> Result<Vec<Real>> {
    let n = D * D;
    let stored = el.options.thermal_energy == ThermalEnergy::StoredElastic;
    let (plane_stress, alpha) = (el.plane_stress, el.thermal.alpha);
    let sigma_th = el.sigma_th.get(ty, NG)?;
    let delta_t = el.delta_t.get(ty, NG)?;
    let dam = damage.get(ty, NG)?;
    let base = el.base();
    let finite = base.finite_deformation;
    let grad_u = base.grad_u.get(ty, NG)?;
    let sigma = base.sigma_field()?.get(ty, NG)?;
    let nq = grad_u.len();
    if [sigma.len(), sigma_th.len(), delta_t.len(), dam.len()]
        .iter()
        .any(|&l| l != nq)
        || points.end > nq
    {
        return Err(Error::DimensionMismatch {
            context: "MaterialDamage: computePotentialEnergy",
            expected: nq,
            got: sigma.len().min(dam.len()).min(points.end),
        });
    }
    let point = |q: usize| -> Result<Real> {
        let g: &Matrix<D, D> = as_matrix(&grad_u.as_slice()[q * n..(q + 1) * n])?;
        let s: &Matrix<D, D> = as_matrix(&sigma.as_slice()[q * n..(q + 1) * n])?;
        let strain = if finite { grad_u_to_green(g) } else { *g };
        let mut w = MaterialElastic::<D>::energy_on_quad(s, &strain);
        let th = sigma_th.as_slice()[q];
        if stored && th != 0. {
            use akantu_core::tensor::SquareMatrixExt;
            w += (1. - dam.as_slice()[q])
                * thermal_energy_correction::<D>(
                    plane_stress,
                    th,
                    strain.trace(),
                    delta_t.as_slice()[q] * alpha,
                );
        }
        Ok(w)
    };
    let start = points.start;
    let mut density = vec![0.0; points.len()];
    exec.try_for_each(Tuples::new(&mut density, 1), 4096, |first, mut d| {
        for (k, w) in d.as_mut_slice().iter_mut().enumerate() {
            *w = point(start + first + k)?;
        }
        Ok(())
    })?;
    Ok(density)
}

/// Upstream `MaterialElastic::computePotentialEnergy(type)` as called by a
/// damage law: `ψ = ½ σ:∇u` (`½ S:E` with finite deformation) per
/// not-ghost quadrature point; with [`ThermalEnergy::StoredElastic`] and a
/// thermal stress, plus `(1 − d)·½ σ_th (tr ∇u − m αΔT)` (module docs).
///
/// # Errors
/// Missing blocks or size mismatches.
pub fn elastic_damage_potential_energy<const D: usize>(
    el: &mut MaterialElastic<D>,
    damage: &InternalField<Real>,
    ty: ElementType,
) -> Result<()> {
    elastic_damage_potential_energy_with(Exec::serial_ref(), el, damage, ty)
}

/// [`elastic_damage_potential_energy`] with the densities computed on
/// `exec`'s threads.
///
/// # Errors
/// As [`elastic_damage_potential_energy`].
pub fn elastic_damage_potential_energy_with<const D: usize>(
    exec: &Exec,
    el: &mut MaterialElastic<D>,
    damage: &InternalField<Real>,
    ty: ElementType,
) -> Result<()> {
    let nq = el.base().grad_u.get(ty, NG)?.len();
    let density = elastic_damage_energy_densities_with(exec, el, damage, ty, 0..nq)?;
    let mut epot = el.base_mut().potential_energy.get_mut(ty, NG)?;
    if epot.len() != nq {
        return Err(Error::DimensionMismatch {
            context: "MaterialDamage: potential_energy",
            expected: nq,
            got: epot.len(),
        });
    }
    epot.as_mut_slice().copy_from_slice(&density);
    Ok(())
}

/// Energy densities at the points of one law-local not-ghost element
/// (upstream `computePotentialEnergyByElement`, here with the law's own
/// density so that the per-element and whole-field energies agree, UD-032).
///
/// # Errors
/// [`Error::InvalidIndex`] for an element outside the filter; see
/// `densities`.
pub fn element_points(
    base: &MaterialBase,
    ctx: &LawContext<'_>,
    element: akantu_core::Element,
) -> Result<std::ops::Range<usize>> {
    let ty = element.ty();
    let nq = ctx.services.nb_integration_points(ty, NG);
    let nb = base.element_filter.get(ty, NG)?.len();
    if element.idx() >= nb {
        return Err(Error::InvalidIndex {
            what: "law-local element",
            value: i64::from(element.index()),
        });
    }
    Ok(element.idx() * nq..(element.idx() + 1) * nq)
}

/// The parent of a damage law (`MaterialDamage<dim, Parent>`): upstream's
/// `MaterialElastic<dim>` (`mazars`) or
/// `MaterialLinearIsotropicHardening<dim>` (`plastic_mazars`). The damage
/// law owns the parent and calls its **point kernels** only (upstream
/// `Parent::computeStressOnQuad`), never its bulk `computeStress`.
pub trait DamageParent<const DIM: usize>: Material + Clone {
    /// The embedded `MaterialElastic<DIM>` (thermal parameters and state,
    /// Lamé constants, options).
    fn elastic(&self) -> &MaterialElastic<DIM>;

    /// Mutable [`DamageParent::elastic`].
    fn elastic_mut(&mut self) -> &mut MaterialElastic<DIM>;

    /// Upstream `Parent::computeStressOnQuad` for every quadrature point of
    /// the block `(ty, ghost)` (into `stress`, or `piola_kirchhoff_2` with
    /// finite deformation), without the parent's bulk pre-steps. The
    /// points may split over `exec`'s threads (the calling law's
    /// [`crate::material::points_exec`]).
    ///
    /// # Errors
    /// Missing blocks, size mismatches, or an unsupported configuration.
    fn stress_kernel(
        &mut self,
        exec: &akantu_core::exec::Exec,
        ty: ElementType,
        ghost: GhostType,
    ) -> Result<()>;

    /// [`DamageParent::stress_kernel`] with `previous_sigma` (when `Some`)
    /// as the committed stress of an incremental parent (the effective
    /// stress history of `plastic_mazars`, UD-139); the elastic parent is
    /// total and ignores it.
    ///
    /// # Errors
    /// As [`DamageParent::stress_kernel`].
    fn stress_kernel_from(
        &mut self,
        exec: &akantu_core::exec::Exec,
        ty: ElementType,
        ghost: GhostType,
        previous_sigma: Option<akantu_core::Array<Real>>,
    ) -> Result<()> {
        let _ = previous_sigma;
        self.stress_kernel(exec, ty, ghost)
    }

    /// The parent's inelastic strain (`inelastic_strain`, plastic parents).
    fn inelastic_strain(&self) -> Option<&InternalField<Real>> {
        None
    }

    /// Upstream `Parent::computeTangentModuliOnQuad` (both parents: the
    /// isotropic elastic tangent).
    fn tangent_on_quad(&self, out: &mut [Real]) {
        self.elastic().tangent_on_quad(out);
    }

    /// Upstream `Parent::updateEnergies(type)` (elastic: nothing; plastic:
    /// the plastic work).
    ///
    /// # Errors
    /// Missing blocks or size mismatches.
    fn parent_update_energies(
        &mut self,
        exec: &akantu_core::exec::Exec,
        ty: ElementType,
    ) -> Result<()> {
        let _ = (exec, ty);
        Ok(())
    }

    /// The `computePotentialEnergy(type)` the damage law inherits
    /// (elastic: [`elastic_damage_potential_energy`]; plastic:
    /// `MaterialPlastic::computePotentialEnergy`), with the current damage
    /// for the thermal correction.
    ///
    /// # Errors
    /// Missing blocks or size mismatches.
    /// The densities may split over `exec`'s threads (the calling law's
    /// [`crate::material::points_exec`]).
    fn parent_potential_energy(
        &mut self,
        exec: &Exec,
        damage: &InternalField<Real>,
        ty: ElementType,
    ) -> Result<()>;

    /// The densities of [`DamageParent::parent_potential_energy`] at the
    /// not-ghost points `points` of `ty`, without storing them.
    ///
    /// # Errors
    /// Missing blocks, size mismatches or a range past the block.
    fn parent_energy_densities(
        &self,
        damage: &InternalField<Real>,
        ty: ElementType,
        points: std::ops::Range<usize>,
    ) -> Result<Vec<Real>>;

    /// Upstream `Parent::getEnergy(id)` for every `id` the parent knows
    /// besides `"potential"` (plastic: `"plastic"`); `None` otherwise.
    ///
    /// # Errors
    /// Integration errors.
    fn parent_energy(&self, ctx: &LawContext<'_>, energy_id: &str) -> Result<Option<Real>> {
        let _ = (ctx, energy_id);
        Ok(None)
    }

    /// Reversible thermal work increments of the step
    /// ([`elastic_family_thermal_work`]) with the parent's plastic strain.
    ///
    /// # Errors
    /// Missing blocks or size mismatches.
    fn thermal_work(&self, damage: &InternalField<Real>, ty: ElementType) -> Result<Vec<Real>> {
        let el = self.elastic();
        let out_of_plane =
            el.options.j2_reduced_return == crate::material::J2ReducedReturn::Consistent;
        elastic_family_thermal_work(
            el,
            damage,
            self.inelastic_strain().map(|f| (f, out_of_plane)),
            ty,
        )
    }

    /// Upstream `Parent::updateInternalParameters()`.
    ///
    /// # Errors
    /// The parent's update error.
    fn parent_update_params(&mut self) -> Result<()>;
}

impl<const DIM: usize> DamageParent<DIM> for MaterialElastic<DIM> {
    fn elastic(&self) -> &MaterialElastic<DIM> {
        self
    }

    fn elastic_mut(&mut self) -> &mut MaterialElastic<DIM> {
        self
    }

    /// Upstream `MaterialElastic<dim>::computeStressOnQuad(args)` with the
    /// raw `grad_u` and the (possibly stale, UD-038) `sigma_th` of the
    /// arguments — also with `finite_deformation`, where the result is
    /// stored as the second Piola–Kirchhoff stress (upstream: the damage
    /// laws bypass `MaterialElastic::computeStress`, which would use the
    /// Green strain).
    /// Every point from its own gradient and thermal stress: parts on
    /// `exec`'s threads.
    fn stress_kernel(
        &mut self,
        exec: &akantu_core::exec::Exec,
        ty: ElementType,
        ghost: GhostType,
    ) -> Result<()> {
        let constants = self.constants();
        let n = DIM * DIM;
        let sigma_th = self.sigma_th.get(ty, ghost)?.clone();
        let base = self.base_mut();
        let grad_u = base.grad_u.get(ty, ghost)?.clone();
        let mut sigma = base.sigma_field_mut()?.get_mut(ty, ghost)?;
        if grad_u.len() != sigma.len() || sigma_th.len() != sigma.len() {
            return Err(Error::DimensionMismatch {
                context: "MaterialElastic::computeStressOnQuad",
                expected: grad_u.len(),
                got: sigma.len(),
            });
        }
        if grad_u.nb_component() != n || sigma.nb_component() != n {
            return Err(Error::DimensionMismatch {
                context: "MaterialElastic::computeStressOnQuad: components",
                expected: n,
                got: sigma.nb_component(),
            });
        }
        let items = (
            akantu_core::exec::TuplesRef::new(grad_u.as_slice(), n),
            akantu_core::exec::Tuples::new(sigma.as_mut_slice(), n),
            akantu_core::exec::TuplesRef::new(sigma_th.as_slice(), 1),
        );
        exec.try_for_each(items, 2048, |_, (g, mut s, th)| {
            for ((g, s), &th) in g
                .as_slice()
                .chunks_exact(n)
                .zip(s.as_mut_slice().chunks_exact_mut(n))
                .zip(th.as_slice())
            {
                let g: &Matrix<DIM, DIM> = as_matrix(g)?;
                let s: &mut Matrix<DIM, DIM> = akantu_core::tensor::as_matrix_mut(s)?;
                constants.stress_on_quad(g, th, s);
            }
            Ok(())
        })
    }

    fn parent_potential_energy(
        &mut self,
        exec: &Exec,
        damage: &InternalField<Real>,
        ty: ElementType,
    ) -> Result<()> {
        elastic_damage_potential_energy_with(exec, self, damage, ty)
    }

    fn parent_energy_densities(
        &self,
        damage: &InternalField<Real>,
        ty: ElementType,
        points: std::ops::Range<usize>,
    ) -> Result<Vec<Real>> {
        elastic_damage_energy_densities(self, damage, ty, points)
    }

    fn parent_update_params(&mut self) -> Result<()> {
        crate::material::MaterialParams::update_params(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn std_max_min_follow_cpp_semantics() {
        assert_eq!(std_max(1., 2.), 2.);
        assert_eq!(std_max(2., 1.), 2.);
        assert!(std_max(-0., 0.).is_sign_negative()); // returns a
        assert!(std_max(0., -0.).is_sign_positive());
        assert!(std_max(f64::NAN, 1.).is_nan());
        assert_eq!(std_max(1., f64::NAN), 1.);
        assert_eq!(std_min(1., 2.), 1.);
        assert!(std_min(-0., 0.).is_sign_negative());
        assert!(std_min(f64::NAN, 1.).is_nan());
        assert_eq!(std_min(1., f64::NAN), 1.);
    }

    #[test]
    fn tangent_scaling() {
        let mut t = [2., 4., -6.];
        DamageFields::scale_tangent(&mut t, 0.25);
        assert_eq!(t, [1.5, 3., -4.5]);
    }
}
