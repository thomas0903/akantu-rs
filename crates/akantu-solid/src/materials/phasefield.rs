// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/model/solid_mechanics/materials/
// material_damage/material_phasefield.hh, material_phasefield.cc,
// material_phasefield_inline_impl.hh, material_damage.hh,
// material_damage_tmpl.hh; src/model/phase_field/energy_split.hh,
// energy_split.cc, energy_splits/no_energy_split{.hh,_inline_impl.hh},
// energy_splits/volumetric_deviatoric_split{.hh,_inline_impl.hh}.

//! `phasefield`: the solid side of the phase-field fracture coupling
//! (upstream `MaterialPhaseField<dim, EnergySplit>`, parents
//! `MaterialDamage<dim>` → `MaterialElastic<dim>`), with the energy splits
//! `no_split` (default option) and `volumetric_deviatoric`, all spatial
//! dimensions.
//!
//! The damage `d` (internal `damage`, with history) is written by the
//! solid/phase-field coupler (later work package); the law degrades the
//! stress with `g(d) = (1 − d)² + η`:
//!
//! ```text
//! (thermal pre-step: σ_th, ε_th of the parent, LawOptions::thermal_strain)
//! ε = ½(∇uᵀ + ∇u)  (Green–Lagrange strain with finite_deformation, result
//!                   stored as the second Piola–Kirchhoff stress)
//! no_split:    ε' = ε − ε_th I;  σ⁺ = (2μ)ε' + (λ tr ε') I;   σ⁻ = 0
//! vol/dev:     ε' = ε − ε_th I;  t = tr ε';  dev = ε' − (t/3) I
//!              σ⁺ = (κ⟨t⟩₊) I + (2μ) dev;  σ⁻ = (κ⟨t⟩₋) I   (κ = λ + (2/3)μ)
//! σ = g·σ⁺ + σ⁻
//! ```
//!
//! `λ`, `μ` are the split's (`EnergySplit::updateMaterialProperties(E, ν,
//! plane_stress)`, set once by `initMaterial`, like upstream; a later
//! change of `E`/`ν` does not reach the split, kept).
//! The split works on run-time-size matrices upstream (`const
//! Matrix<Real> &` arguments), so its traces are **sequential** sums (not
//! the halving tree of compile-time sizes).
//!
//! Tangent (split coefficients, Voigt): no split `g·D_iso` (`g·E` in 1-D);
//! vol/dev `M_iiii = g_h κ + g·2μ·(2/3)`, `M_iijj = g_h κ − g·2μ/3`,
//! `M_ijij = gμ` with `g_h = g` in volumetric tension, else 1.
//!
//! Energies: potential `½σ:∇u` (`½S:E`); after a converged step the
//! shared `MaterialDamage` bookkeeping of [`super::damage`]
//! (`DamageFields::update_energies`): `∫σ += ½(σ + σ_prev):Δε [+ Δw_th]`,
//! `e_diss = ∫σ − ψ` (internals `integral of sigma`,
//! `damage dissipated energy`; `getEnergy("dissipated")`). This
//! accumulated deficit is the material-point mechanical energy balance of
//! prescribed damage, not the total phase-field fracture energy (no crack
//! surface energy here). Mass with
//! `degrade_mass`: `ρ_q = min(ρ, ρ·((1 − d_q)² + η))`, and the mass matrix is
//! flagged changed.
//!
//! # Upstream defects and switches
//!
//! * `LawOptions::phasefield_tangent` (category B,
//!   tangent only — converged results are unaffected): upstream's
//!   volumetric–deviatoric tangent (a) uses `g_h·E` in 1-D although the
//!   stress gives `g_h κ + g·(4/3)μ` (they differ in compression with
//!   damage), and (b) tests the sign of the trace of the *total* strain
//!   while the stress splits on the trace of `ε − ε_th I`.
//!   [`PhaseFieldTangent::Consistent`] (default) differentiates the stress
//!   exactly; [`PhaseFieldTangent::Upstream`] reproduces upstream.
//! * `LawOptions::thermal_energy` (UD-109 extended to this law): with a
//!   thermal strain, upstream's `½σ:∇u` is not the stored energy of the
//!   split; [`ThermalEnergy::StoredElastic`] (default) uses
//!   `½σ:(∇u − ε_th I) = g ψ⁺(ε') + ψ⁻(ε')`, identical to upstream when
//!   `ε_th = 0`.
//! * `degrade_mass` with several quadrature points per element (category A,
//!   Upstream views the per-quadrature-point density
//!   vector as an `nq × nq` matrix and scales its columns (out-of-bounds
//!   writes, every point scaled by the first point's degradation); the port
//!   scales each point by its own damage. One point per element (the
//!   oracle probe) is identical.
//! * `LawOptions::finite_strain_work` (UD-158 FD-PREV + UD-159,
//!   category B): with `finite_deformation` the work update pairs
//!   the PK2 stress and its own history with the Green-strain increment
//!   ([`FiniteStrainWork::Conjugate`](super::damage::FiniteStrainWork),
//!   default); upstream pairs the current PK2 with the never-written Cauchy
//!   history and `Δ∇u`.
//! * `LawOptions::damage_thermal_work` (UD-160, category
//!   B): with the stored-energy convention the reversible thermal work
//!   `−½(tr σ_{n+1} + tr σ_n)(ε_th,n+1 − ε_th,n)` (`∂ψ/∂ε_th = −tr σ`) is
//!   added to `∫σ`, so heating without damage dissipates nothing; upstream
//!   books the thermal change of `ψ` as dissipation.
//! * Ghost synchronisation of `damage`/`grad_u` for the degraded mass
//!   (upstream `packData` with `_smm_density`) belongs to the parallel
//!   work package.

use std::sync::Arc;

use akantu_core::eigen_order as eo;
use akantu_core::enums::MatrixType;
use akantu_core::tensor::{Matrix, MatrixExt, as_matrix, as_matrix_mut};
use akantu_core::{
    Array, Element, ElementType, ElementTypeMapArray, Error, GhostType, Idx, Real, Result,
};
use akantu_model::constitutive_law::{
    ConstitutiveLaw, InternalFieldMut, InternalFieldRef, LawContext, LawSpec, ModelServices,
};
use akantu_parser::ParameterRegistry;
use akantu_parser::param::{ParamDescriptor, ParamKind, ParamValue, ParameterAccess};

use super::damage::{DamageFields, DamageWork, shifted_strain_thermal_work, thermal_work_enabled};
use super::elastic::MaterialElastic;
use super::thermal::{ThermalEnergy, thermal_on_quad};
use crate::material::{
    DerivedMaterialParams, LawOptions, Material, MaterialBase, MaterialFactory, check_tangent,
};
use crate::material_tensor::{grad_u_to_epsilon, grad_u_to_green, tangent_stiffness_voigt_size};

// The energy split (`EnergySplitKind`, `EnergySplit`) and the tangent
// switch (`PhaseFieldTangent`) have a single implementation, shared with the
// phase-field laws.
pub use akantu_model::energy_split::{EnergySplit, EnergySplitKind, PhaseFieldTangent};

/// Identity coefficient.
#[inline(always)]
fn id(i: usize, j: usize) -> Real {
    if i == j { 1.0 } else { 0.0 }
}

/// `strain.trace()` of a run-time-size matrix: sequential sum (the
/// upstream sign test of the volumetric split).
fn trace_dyn<const D: usize>(m: &Matrix<D, D>) -> Real {
    eo::sum(D, eo::READ_COST, false, &|i| m[i][i])
}

/// Phase-field material (upstream `MaterialPhaseField<DIM, Split>`).
#[derive(Clone, Debug)]
pub struct MaterialPhaseField<const DIM: usize> {
    /// The elastic parent (`MaterialElastic<DIM>` through
    /// `MaterialDamage<DIM>`).
    pub elastic: MaterialElastic<DIM>,
    /// The energy split (allocator option).
    pub split_kind: EnergySplitKind,
    /// `eta`: residual stiffness.
    pub eta: Real,
    /// `is_isotropic` (no effect on the stress).
    pub is_isotropic: bool,
    /// `degrade_mass`.
    pub degrade_mass: bool,
    split: EnergySplit,
    /// The `MaterialDamage` internals (`damage` with history, written by
    /// the coupler; `damage dissipated energy`; `integral of sigma`) and
    /// their shared energy bookkeeping.
    pub damage: DamageFields,
}

impl<const DIM: usize> MaterialPhaseField<DIM> {
    /// New law (upstream constructor chain; `spec.option` selects the
    /// split).
    pub fn new(spec: &LawSpec, mesh: Option<&akantu_mesh::Mesh>) -> Result<Self> {
        let split_kind = EnergySplitKind::from_option(&spec.option)?;
        let mut elastic = MaterialElastic::<DIM>::new(spec, mesh)?;
        elastic.base_mut().law_type = "phasefield".into();
        Ok(Self {
            elastic,
            split_kind,
            eta: 0.,
            is_isotropic: false,
            degrade_mass: false,
            split: EnergySplit {
                kind: split_kind,
                ..EnergySplit::default()
            },
            damage: DamageFields::new()?,
        })
    }

    /// The split constants (as set by `initMaterial`).
    #[must_use]
    pub fn energy_split(&self) -> &EnergySplit {
        &self.split
    }

    /// The split used by the kernels: the constants of `initMaterial` with
    /// the current allocator option [`Self::split_kind`].
    fn split_constants(&self) -> EnergySplit {
        EnergySplit {
            kind: self.split_kind,
            ..self.split
        }
    }

    /// `g(d) = (1 − d)² + η` (upstream `(1 - d) * (1 - d) + eta`).
    #[must_use]
    pub fn degradation(&self, d: Real) -> Real {
        (1. - d) * (1. - d) + self.eta
    }

    fn update(&mut self) -> Result<()> {
        ParameterRegistry::update_internal_parameters(&mut self.elastic)
    }

    /// Upstream `MaterialThermal::computeStress` for one block.
    fn compute_thermal(&mut self, ty: ElementType, ghost: GhostType) -> Result<()> {
        let e = &mut self.elastic;
        let dt = e.delta_t.get(ty, ghost)?.clone();
        let (mut eps, mut sig) = (
            e.epsilon_th.get_mut(ty, ghost)?,
            e.sigma_th.get_mut(ty, ghost)?,
        );
        if dt.len() != eps.len() || dt.len() != sig.len() {
            return Err(Error::DimensionMismatch {
                context: "MaterialThermal::computeStress",
                expected: dt.len(),
                got: eps.len().min(sig.len()),
            });
        }
        for ((d, ep), s) in dt
            .as_slice()
            .iter()
            .zip(eps.as_mut_slice())
            .zip(sig.as_mut_slice())
        {
            (*ep, *s) =
                thermal_on_quad::<DIM>(&e.thermal, e.plane_stress, e.options.thermal_strain, *d);
        }
        Ok(())
    }

    /// Strain measure fed to the split (`∇u` or the Green strain).
    fn strain_input(&self, g: &Matrix<DIM, DIM>) -> Matrix<DIM, DIM> {
        if self.elastic.base().finite_deformation {
            grad_u_to_green(g)
        } else {
            *g
        }
    }

    /// Potential energy density of one point (see the module docs).
    fn point_energy(
        &self,
        strain: &Matrix<DIM, DIM>,
        sigma: &Matrix<DIM, DIM>,
        eps_th: Real,
    ) -> Real {
        let w = MaterialElastic::<DIM>::energy_on_quad(sigma, strain);
        if self.elastic.options.thermal_energy == ThermalEnergy::Upstream || eps_th == 0.0 {
            return w;
        }
        let shifted: Matrix<DIM, DIM> =
            std::array::from_fn(|j| std::array::from_fn(|i| strain[j][i] - eps_th * id(i, j)));
        0.5 * sigma.double_dot(&shifted)
    }

    fn dissipated_energy_total(&self, ctx: &LawContext<'_>) -> Result<Real> {
        self.damage.dissipated_energy(self.elastic.base(), ctx)
    }
}

impl<const DIM: usize> MaterialPhaseField<DIM> {
    const PARAMS: &'static [ParamDescriptor<Self>] = &[
        crate::real_param!(Self, "eta", ParameterAccess::PARSABLE, "eta", eta),
        ParamDescriptor::new(
            "is_isotropic",
            ParamKind::Bool,
            ParameterAccess(ParameterAccess::PARSABLE.0 | ParameterAccess::READABLE.0),
            "Use isotropic formulation",
            |s| ParamValue::Bool(s.is_isotropic),
            Some(|s, v| {
                s.is_isotropic = v.as_bool()?;
                Ok(())
            }),
        ),
        ParamDescriptor::new(
            "degrade_mass",
            ParamKind::Bool,
            ParameterAccess(ParameterAccess::PARSABLE.0 | ParameterAccess::READABLE.0),
            "Degrade mass with damage",
            |s| ParamValue::Bool(s.degrade_mass),
            Some(|s, v| {
                s.degrade_mass = v.as_bool()?;
                Ok(())
            }),
        ),
    ];
}

impl<const DIM: usize> DerivedMaterialParams for MaterialPhaseField<DIM> {
    type Parent = MaterialElastic<DIM>;
    fn parent(&self) -> &Self::Parent {
        &self.elastic
    }
    fn parent_mut(&mut self) -> &mut Self::Parent {
        &mut self.elastic
    }
    fn derived_params(&self) -> &'static [ParamDescriptor<Self>] {
        Self::PARAMS
    }
    fn update_derived(&mut self) -> Result<()> {
        self.update()
    }
    fn derived_id(&self) -> &str {
        &self.elastic.base().id
    }
}

crate::impl_derived_material_registry!(MaterialPhaseField<DIM>, const DIM);

impl<const DIM: usize> ConstitutiveLaw for MaterialPhaseField<DIM> {
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
        self.damage.for_each(f);
    }
    fn for_each_internal_mut(
        &mut self,
        f: &mut dyn FnMut(&str, InternalFieldMut<'_>) -> Result<()>,
    ) -> Result<()> {
        self.elastic.for_each_internal_mut(f)?;
        self.damage.for_each_mut(f)
    }
}

impl<const DIM: usize> Material for MaterialPhaseField<DIM> {
    fn base(&self) -> &MaterialBase {
        self.elastic.base()
    }
    fn base_mut(&mut self) -> &mut MaterialBase {
        self.elastic.base_mut()
    }

    /// Upstream `initMaterial`: `MaterialElastic::initMaterial`, then the
    /// split's `updateMaterialProperties(E, ν, plane_stress)` (2-D) /
    /// `(E, ν, false)`.
    fn init_material(&mut self, ctx: &LawContext<'_>) -> Result<()> {
        self.base_mut().init_history()?;
        let blocks = self.base().blocks(ctx);
        self.resize_internals(&blocks)?;
        self.update()?;
        self.base_mut().set_init();
        if DIM == 1 {
            self.elastic.thermal.nu = 0.;
        }
        self.update()?;
        let plane_stress = DIM == 2 && self.elastic.plane_stress;
        self.split = EnergySplit::new(
            self.split_kind,
            self.elastic.thermal.e,
            self.elastic.thermal.nu,
            plane_stress,
        );
        Ok(())
    }

    fn compute_stress(
        &mut self,
        _ctx: &LawContext<'_>,
        ty: ElementType,
        ghost: GhostType,
    ) -> Result<()> {
        self.compute_thermal(ty, ghost)?;
        let n = DIM * DIM;
        let grad_u = self.elastic.base().grad_u.get(ty, ghost)?.clone();
        let dam = self.damage.damage.get(ty, ghost)?.clone();
        let eps_th = self.elastic.epsilon_th.get(ty, ghost)?.clone();
        let nq = grad_u.len();
        if dam.len() != nq || eps_th.len() != nq {
            return Err(Error::DimensionMismatch {
                context: "MaterialPhaseField::computeStress",
                expected: nq,
                got: dam.len().min(eps_th.len()),
            });
        }
        // the split of the (public) allocator option, constants of initMaterial
        let split = self.split_constants();
        let mut out = vec![0.0; nq * n];
        for q in 0..nq {
            let g: &Matrix<DIM, DIM> = as_matrix(&grad_u.as_slice()[q * n..(q + 1) * n])?;
            let strain = grad_u_to_epsilon(&self.strain_input(g));
            let g_d = self.degradation(dam.as_slice()[q]);
            let (plus, minus) = split.sigma(&strain, eps_th.as_slice()[q]);
            let s: &mut Matrix<DIM, DIM> = as_matrix_mut(&mut out[q * n..(q + 1) * n])?;
            for j in 0..DIM {
                for i in 0..DIM {
                    s[j][i] = g_d * plus[j][i] + minus[j][i];
                }
            }
        }
        let mut sigma = self
            .elastic
            .base_mut()
            .sigma_field_mut()?
            .get_mut(ty, ghost)?;
        if sigma.as_slice().len() != out.len() {
            return Err(Error::DimensionMismatch {
                context: "MaterialPhaseField::computeStress: stress",
                expected: out.len(),
                got: sigma.as_slice().len(),
            });
        }
        sigma.as_mut_slice().copy_from_slice(&out);
        Ok(())
    }

    fn compute_tangent_moduli(
        &mut self,
        _ctx: &LawContext<'_>,
        ty: ElementType,
        ghost: GhostType,
        tangent: &mut Array<Real>,
    ) -> Result<()> {
        let n = DIM * DIM;
        let v = tangent_stiffness_voigt_size(DIM);
        let grad_u = self.elastic.base().grad_u.get(ty, ghost)?.clone();
        let dam = self.damage.damage.get(ty, ghost)?.clone();
        let nq = grad_u.len();
        check_tangent(tangent, nq, v)?;
        let mode = self.elastic.options.phasefield_tangent;
        let split = self.split_constants();
        let eps_th = self.elastic.epsilon_th.get(ty, ghost)?.clone();
        if dam.len() != nq || eps_th.len() != nq {
            return Err(Error::DimensionMismatch {
                context: "MaterialPhaseField::computeTangentModuli",
                expected: nq,
                got: dam.len().min(eps_th.len()),
            });
        }
        for (q, t) in tangent.as_mut_slice().chunks_exact_mut(v * v).enumerate() {
            let g: &Matrix<DIM, DIM> = as_matrix(&grad_u.as_slice()[q * n..(q + 1) * n])?;
            let strain = grad_u_to_epsilon(&self.strain_input(g));
            let g_d = self.degradation(dam.as_slice()[q]);
            let positive = match mode {
                PhaseFieldTangent::Upstream => trace_dyn(&strain) > 0.,
                PhaseFieldTangent::Consistent => {
                    let th = eps_th.as_slice()[q];
                    let shifted: Matrix<DIM, DIM> = std::array::from_fn(|j| {
                        std::array::from_fn(|i| strain[j][i] - th * id(i, j))
                    });
                    trace_dyn(&shifted) > 0.
                }
            };
            t.fill(0.0);
            split.tangent_coefficients::<DIM>(g_d, positive, mode, t)?;
        }
        Ok(())
    }

    fn compute_potential_energy(&mut self, _ctx: &LawContext<'_>, ty: ElementType) -> Result<()> {
        let g = GhostType::NotGhost;
        let n = DIM * DIM;
        let grad_u = self.elastic.base().grad_u.get(ty, g)?.clone();
        let sigma = self.elastic.base().sigma_field()?.get(ty, g)?.clone();
        let eps_th = self.elastic.epsilon_th.get(ty, g)?.clone();
        let nq = grad_u.len();
        if sigma.len() != nq || eps_th.len() != nq {
            return Err(Error::DimensionMismatch {
                context: "MaterialPhaseField::computePotentialEnergy",
                expected: nq,
                got: sigma.len().min(eps_th.len()),
            });
        }
        let mut density = Vec::with_capacity(nq);
        for q in 0..nq {
            let gu: &Matrix<DIM, DIM> = as_matrix(&grad_u.as_slice()[q * n..(q + 1) * n])?;
            let s: &Matrix<DIM, DIM> = as_matrix(&sigma.as_slice()[q * n..(q + 1) * n])?;
            density.push(self.point_energy(&self.strain_input(gu), s, eps_th.as_slice()[q]));
        }
        let mut epot = self.elastic.base_mut().potential_energy.get_mut(ty, g)?;
        if epot.len() != nq {
            return Err(Error::DimensionMismatch {
                context: "MaterialPhaseField::computePotentialEnergy: potential_energy",
                expected: nq,
                got: epot.len(),
            });
        }
        epot.as_mut_slice().copy_from_slice(&density);
        Ok(())
    }

    fn potential_energy_by_element(
        &self,
        ctx: &LawContext<'_>,
        element: Element,
    ) -> Result<Vec<Real>> {
        let ty = element.ty();
        let g = GhostType::NotGhost;
        let nq = ctx.services.nb_integration_points(ty, g);
        let n = DIM * DIM;
        let grad_u = self.elastic.base().grad_u.get(ty, g)?;
        let sigma = self.elastic.base().sigma_field()?.get(ty, g)?;
        let eps_th = self.elastic.epsilon_th.get(ty, g)?;
        let e = element.idx();
        let (Some(gu), Some(s), Some(th)) = (
            grad_u.as_slice().get(e * nq * n..(e + 1) * nq * n),
            sigma.as_slice().get(e * nq * n..(e + 1) * nq * n),
            eps_th.as_slice().get(e * nq..(e + 1) * nq),
        ) else {
            return Err(Error::InvalidIndex {
                what: "law-local element",
                value: i64::from(element.index()),
            });
        };
        gu.chunks_exact(n)
            .zip(s.chunks_exact(n))
            .zip(th)
            .map(|((gu, s), &th)| {
                Ok(self.point_energy(&self.strain_input(as_matrix(gu)?), as_matrix(s)?, th))
            })
            .collect()
    }

    /// Upstream `MaterialDamage::updateEnergies(type)` (the shared
    /// [`DamageFields::update_energies`]; reversible thermal work of the
    /// shifted-strain energy).
    fn update_energies(&mut self, ctx: &LawContext<'_>, ty: ElementType) -> Result<()> {
        self.compute_potential_energy(ctx, ty)?;
        let options = self.elastic.options;
        let thermal = if thermal_work_enabled(&options) {
            Some(shifted_strain_thermal_work::<DIM>(
                self.elastic.base(),
                &self.elastic.epsilon_th,
                ty,
            )?)
        } else {
            None
        };
        let work = DamageWork::new(&options, thermal.as_deref());
        self.damage
            .update_energies::<DIM>(self.elastic.base(), ty, work)
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

    /// Upstream `MaterialDamage::hasStiffnessMatrixChanged`: always.
    fn has_stiffness_matrix_changed(&self) -> bool {
        true
    }

    fn has_mass_matrix_changed(&self) -> bool {
        self.degrade_mass
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

    /// `"dissipated"` (`MaterialDamage::getEnergy`), otherwise the
    /// parent's.
    fn energy_of(&mut self, ctx: &LawContext<'_>, energy_id: &str) -> Result<Real> {
        if energy_id == "dissipated" {
            self.dissipated_energy_total(ctx)
        } else {
            self.potential_or_parent(ctx, energy_id)
        }
    }

    fn energy_of_element(
        &mut self,
        ctx: &LawContext<'_>,
        energy_id: &str,
        element: Element,
    ) -> Result<Real> {
        match energy_id {
            "potential" => self.potential_energy_of_element(ctx, element),
            // upstream MaterialDamage has no per-element dissipated energy
            // (Material::getEnergy(id, element) returns 0)
            _ => Ok(0.0),
        }
    }

    /// Upstream `getRho(rhos, element)`: `ρ`, and with `degrade_mass`
    /// `min(ρ, ρ·((1 − d_q)² + η))` per quadrature point of the law-local
    /// `element`.
    ///
    /// # Panics
    /// If the damage field is exported to a foreign runtime and no native
    /// section of it is active (protocol violation, `SharedArray::borrow`).
    fn rho_on_quadrature_points(&self, element: Element, out: &mut [Real]) {
        let rho = self.base().rho;
        out.fill(rho);
        if !self.degrade_mass {
            return;
        }
        // an absent block keeps ρ; a protocol violation panics instead of
        // silently not degrading the mass
        let Some(d) = self
            .damage
            .damage
            .values()
            .as_map()
            .get(element.ty(), element.ghost_type())
        else {
            return;
        };
        let d = d.borrow();
        let nq = out.len();
        let Some(dq) = d
            .as_slice()
            .get(element.idx() * nq..(element.idx() + 1) * nq)
        else {
            return;
        };
        for (r, &d) in out.iter_mut().zip(dq) {
            *r *= (1. - d) * (1. - d) + self.eta;
        }
        for r in out.iter_mut() {
            *r = if *r < rho { *r } else { rho };
        }
    }

    /// Upstream `MaterialPhaseField::getNbData/packData/unpackData`
    /// (`material_phasefield_inline_impl.hh:138-186`): with `degrade_mass`,
    /// `_smm_density` carries `damage` then `grad_u` of every quadrature
    /// point, so that the ghost elements' lumped mass uses their owner's
    /// damage (MPI-036).
    fn exchanged_internals(&self, tag: akantu_core::enums::SynchronizationTag) -> Vec<&str> {
        if tag == akantu_core::enums::SynchronizationTag::SmmDensity && self.degrade_mass {
            vec!["damage", "grad_u"]
        } else {
            Vec::new()
        }
    }
}

impl<const DIM: usize> MaterialPhaseField<DIM> {
    /// `"potential"` recomputes and integrates (`Material::getEnergy`);
    /// other ids are 0.
    fn potential_or_parent(&mut self, ctx: &LawContext<'_>, energy_id: &str) -> Result<Real> {
        if energy_id == "potential" {
            self.potential_energy(ctx)
        } else {
            Ok(0.0)
        }
    }
}

/// Register `phasefield` (upstream `instantiateMaterialPhaseField`; option
/// `no_split` (default) or `volumetric_deviatoric`).
pub fn register(factory: &mut MaterialFactory) -> Result<()> {
    factory.register(
        "phasefield",
        Arc::new(|spec: &LawSpec, services: &dyn ModelServices| {
            let mesh = Some(services.mesh());
            akantu_core::with_dim!(spec.spatial_dimension, D => {
                Box::new(MaterialPhaseField::<D>::new(spec, mesh)?) as Box<dyn Material>
            })
        }),
    )
}
