// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): extra_packages/extra-materials/src/
// material_plastic/material_viscoplastic.hh, material_viscoplastic.cc,
// material_viscoplastic_inline_impl.hh (and the MaterialPlastic parent).

//! `visco_plastic` (XMAT-012): explicit Perzyna-type viscoplasticity
//! (upstream `MaterialViscoPlastic<dim> : MaterialPlastic<dim>`).
//!
//! Per quadrature point (`computeStressOnQuad`, upstream order), from the
//! stress `σ_in` and hardening `r` the flow is read from (UD-584):
//!
//! 1. `‖σ‖ = √(σ_in:σ_in)`; `f = ts·ε̇₀·‖σ‖^{m−1} / (σ_y + r)^m` (`m` =
//!    `rate`, glibc `pow`); `Δε_p = f·σ_in`; `‖Δε_p‖ = √(Δε_p:Δε_p)`;
//! 2. `r ← r + h·‖Δε_p‖`;
//! 3. `MaterialPlastic::computeStressAndInelasticStrainOnQuad`:
//!    `σ = σ_prev + C:(Δ∇u − Δε_p)`, `ε_p = ε_p,prev + Δε_p`
//!    (`Δ∇u = ∇u − ∇u_prev`; no thermal term: the law never runs the
//!    thermal pre-step, as upstream). Upstream first adds the same terms to
//!    the current stress and strain, then overwrites them (dead code).
//!
//! Tangent (upstream `computeTangentModuliOnQuad`): `δ_ik δ_jl 2μ + δ_ij
//! δ_kl λ`, minus `μ` on the shear diagonal — the isotropic elastic
//! tangent. Energies: `MaterialPlastic` (`ψ = ½σ:(∇u − ε_p)`, plastic work
//! after converged steps, `getEnergy("plastic")`). Parameters: `h`,
//! `sigma_y` (`MaterialPlastic`, 0), `rate`, `edot0`, `ts` (0, parsable-
//! modifiable). `ts` is a parameter (not the model's time step).
//!
//! # Upstream defects
//!
//! * **UD-583** (B): the previous-stress and previous-inelastic-strain
//!   iterators are never advanced, so every point builds `σ = σ_prev[0] +
//!   C:(Δ∇u − Δε_p)` and `ε_p = ε_p,prev[0] + Δε_p` from the first point's
//!   committed state;
//!   [`ExtraLawOptions::viscoplastic_previous_state_per_point`] (default)
//!   reads each point's own.
//! * **UD-583** (B, second part): at `σ = 0` with `m < 1` (the default `m = 0`)
//!   `‖σ‖^{m−1} = ∞` and `Δε_p = ∞·0 = NaN` (first step); and `σ_in`, `r`
//!   are the **current** values (last evaluation), so repeated evaluations
//!   of a step accumulate hardening and flow.
//!   [`ExtraLawOptions::viscoplastic_committed_flow`] (default): `Δε_p = 0`
//!   when `‖σ‖ = 0`, and `σ_in`, `r` from the state committed at the last
//!   `savePreviousState` (identical to upstream for one evaluation per
//!   step, the explicit scheme).
//! * **UD-783** (C, preserved): the flow is along the full stress
//!   (`Δε_p = f·σ`, not its deviator), so a hydrostatic stress produces
//!   plastic volume change.

use akantu_core::glibc_libm::pow;
use akantu_core::tensor::{Matrix, MatrixExt, as_matrix};
use akantu_core::voigt::voigt_table;
use akantu_core::{Array, Element, ElementType, ElementTypeMapArray, GhostType, Idx, Real, Result};
use akantu_model::constitutive_law::{
    ConstitutiveLaw, InternalFieldMut, InternalFieldRef, LawContext, LawSpec,
};
use akantu_parser::param::{ParamDescriptor, ParameterAccess};
use akantu_solid::Material;
use akantu_solid::material::{LawOptions, MaterialBase, MaterialFactory, MaterialParams};
use akantu_solid::material_tensor::tangent_stiffness_voigt_size;
use akantu_solid::materials::elastic::MaterialElastic;
use akantu_solid::materials::plastic::{PlasticFields, stress_and_inelastic_strain_on_quad};

use crate::kernels::{check_lengths, small_strain_only};
use crate::options::ExtraLawOptions;

const NG: GhostType = GhostType::NotGhost;

/// `visco_plastic` (upstream `MaterialViscoPlastic<DIM>`).
#[derive(Clone, Debug)]
pub struct MaterialViscoPlastic<const DIM: usize> {
    /// The elastic parent.
    pub elastic: MaterialElastic<DIM>,
    /// `iso_hardening`, `inelastic_strain` (history), `plastic_energy`,
    /// `d_plastic_energy`.
    pub plastic: PlasticFields,
    /// `h`: hardening modulus.
    pub h: Real,
    /// `sigma_y`: yield stress.
    pub sigma_y: Real,
    /// `rate`: rate sensitivity `m`.
    pub rate: Real,
    /// `edot0`: reference strain rate.
    pub edot0: Real,
    /// `ts`: time step.
    pub ts: Real,
    /// Extra-materials defect switches (UD-583).
    pub extra_options: ExtraLawOptions,
}

impl<const DIM: usize> MaterialViscoPlastic<DIM> {
    /// New law (upstream constructor chain).
    ///
    /// # Errors
    /// A spatial dimension other than `DIM`.
    pub fn new(spec: &LawSpec, mesh: Option<&akantu_mesh::Mesh>) -> Result<Self> {
        let mut elastic = MaterialElastic::<DIM>::new(spec, mesh)?;
        elastic.base_mut().law_type = "visco_plastic".into();
        Ok(Self {
            elastic,
            plastic: PlasticFields::new(DIM)?,
            h: 0.,
            sigma_y: 0.,
            rate: 0.,
            edot0: 0.,
            ts: 0.,
            extra_options: ExtraLawOptions::default(),
        })
    }

    /// Steps 1–2 of the module docs: `(Δε_p, r_new)` from the flow stress
    /// `sigma_in` and the hardening `r`.
    #[must_use]
    pub fn flow(&self, sigma_in: &Matrix<DIM, DIM>, r: Real) -> (Matrix<DIM, DIM>, Real) {
        let s = sigma_in.double_dot(sigma_in);
        let sigma_mag = s.sqrt();
        let committed = self.extra_options.viscoplastic_committed_flow;
        let delta = if committed && sigma_mag == 0. {
            // UD-583: the limit of f·σ at σ = 0
            [[0.; DIM]; DIM]
        } else {
            let factor = self.ts * self.edot0 * pow(sigma_mag, self.rate - 1.)
                / pow(self.sigma_y + r, self.rate);
            sigma_in.scale(factor)
        };
        let dep_mag = delta.double_dot(&delta).sqrt();
        (delta, r + self.h * dep_mag)
    }

    /// Upstream `computeTangentModuliOnQuad`.
    pub fn tangent_on_quad(&self, out: &mut [Real]) -> Result<()> {
        let v = tangent_stiffness_voigt_size(DIM);
        let table = voigt_table(DIM)?;
        let (lambda, mu) = (self.elastic.lambda(), self.elastic.mu());
        let b = |x: bool| if x { 1. } else { 0. };
        for m in 0..v {
            let (i, j) = table.vec(m);
            for n in 0..v {
                let (k, l) = table.vec(n);
                let mut t = b(i == k) * b(j == l) * 2. * mu + b(i == j) * b(k == l) * lambda;
                t -= b(m == n) * b(m >= DIM) * mu;
                out[m + n * v] = t;
            }
        }
        Ok(())
    }

    fn densities(&self, ty: ElementType, points: std::ops::Range<usize>) -> Result<Vec<Real>> {
        let n = DIM * DIM;
        let grad_u = self.elastic.base().grad_u.get(ty, NG)?;
        let sigma = self.elastic.base().stress.get(ty, NG)?;
        let inel = self.plastic.inelastic_strain.get(ty, NG)?;
        let nq = grad_u.len();
        check_lengths(
            "MaterialPlastic::computePotentialEnergy",
            nq,
            &[sigma.len(), inel.len()],
        )?;
        if points.end > nq {
            return Err(akantu_core::Error::DimensionMismatch {
                context: "MaterialViscoPlastic: points",
                expected: nq,
                got: points.end,
            });
        }
        points
            .map(|q| {
                let r = q * n..(q + 1) * n;
                let g: &Matrix<DIM, DIM> = as_matrix(&grad_u.as_slice()[r.clone()])?;
                let s: &Matrix<DIM, DIM> = as_matrix(&sigma.as_slice()[r.clone()])?;
                let e: &Matrix<DIM, DIM> = as_matrix(&inel.as_slice()[r])?;
                Ok(MaterialElastic::<DIM>::energy_on_quad(s, &g.sub(e)))
            })
            .collect()
    }
}

/// Parameter tables (elastic, then `MaterialPlastic`'s `h`, `sigma_y`,
/// then the law's).
impl<const DIM: usize> MaterialViscoPlastic<DIM> {
    const PARAMS: &'static [ParamDescriptor<Self>] = akantu_solid::elastic_parent_params!(
        Self,
        [elastic],
        [
            akantu_solid::real_param!(Self, "h", ParameterAccess::PARSMOD, "Hardening  modulus", h),
            akantu_solid::real_param!(
                Self,
                "sigma_y",
                ParameterAccess::PARSMOD,
                "Yield stress",
                sigma_y
            ),
            akantu_solid::real_param!(
                Self,
                "rate",
                ParameterAccess::PARSMOD,
                "Rate sensitivity component",
                rate
            ),
            akantu_solid::real_param!(
                Self,
                "edot0",
                ParameterAccess::PARSMOD,
                "Reference strain rate",
                edot0
            ),
            akantu_solid::real_param!(Self, "ts", ParameterAccess::PARSMOD, "Time Step", ts),
        ]
    );
    const PARAMS_2D: &'static [ParamDescriptor<Self>] = akantu_solid::elastic_parent_params!(plane_stress Self, [elastic], [
        akantu_solid::real_param!(Self, "h", ParameterAccess::PARSMOD, "Hardening  modulus", h),
        akantu_solid::real_param!(Self, "sigma_y", ParameterAccess::PARSMOD, "Yield stress", sigma_y),
        akantu_solid::real_param!(Self, "rate", ParameterAccess::PARSMOD, "Rate sensitivity component", rate),
        akantu_solid::real_param!(Self, "edot0", ParameterAccess::PARSMOD, "Reference strain rate", edot0),
        akantu_solid::real_param!(Self, "ts", ParameterAccess::PARSMOD, "Time Step", ts),
    ]);
}

impl<const DIM: usize> MaterialParams for MaterialViscoPlastic<DIM> {
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

akantu_solid::impl_material_registry!(MaterialViscoPlastic<DIM>, const DIM);

impl<const DIM: usize> ConstitutiveLaw for MaterialViscoPlastic<DIM> {
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

impl<const DIM: usize> Material for MaterialViscoPlastic<DIM> {
    fn base(&self) -> &MaterialBase {
        self.elastic.base()
    }
    fn base_mut(&mut self) -> &mut MaterialBase {
        self.elastic.base_mut()
    }

    /// Upstream `MaterialElastic::initMaterial` (through `MaterialPlastic`).
    fn init_material(&mut self, ctx: &LawContext<'_>) -> Result<()> {
        self.elastic.base_mut().init_history()?;
        let blocks = self.elastic.base().blocks(ctx);
        self.resize_internals(&blocks)?;
        MaterialParams::update_params(self)?;
        self.elastic.base_mut().set_init();
        if DIM == 1 {
            self.elastic.thermal.nu = 0.;
        }
        MaterialParams::update_params(self)
    }

    fn compute_stress(
        &mut self,
        _ctx: &LawContext<'_>,
        ty: ElementType,
        ghost: GhostType,
    ) -> Result<()> {
        small_strain_only("visco_plastic", self.elastic.base().finite_deformation)?;
        let n = DIM * DIM;
        let per_point = self.extra_options.viscoplastic_previous_state_per_point;
        let committed = self.extra_options.viscoplastic_committed_flow;
        let constants = self.elastic.constants();
        let base = self.elastic.base();
        let grad_u = base.grad_u.get(ty, ghost)?.clone();
        let prev_grad_u = base.grad_u.previous(ty, ghost)?.clone();
        let mut sigma = base.stress.get(ty, ghost)?.clone();
        let prev_sigma = base.stress.previous(ty, ghost)?.clone();
        let mut inel = self.plastic.inelastic_strain.get(ty, ghost)?.clone();
        let prev_inel = self.plastic.inelastic_strain.previous(ty, ghost)?.clone();
        let mut iso = self.plastic.iso_hardening.get(ty, ghost)?.clone();
        let prev_iso = self.plastic.iso_hardening.previous(ty, ghost)?.clone();
        let nq = grad_u.len();
        check_lengths(
            "MaterialViscoPlastic::computeStress",
            nq,
            &[
                prev_grad_u.len(),
                sigma.len(),
                prev_sigma.len(),
                inel.len(),
                prev_inel.len(),
                iso.len(),
                prev_iso.len(),
            ],
        )?;
        for q in 0..nq {
            let r = q * n..(q + 1) * n;
            // UD-583: upstream reads the first point's previous stress and
            // previous inelastic strain
            let rp = if per_point { r.clone() } else { 0..n };
            let g: &Matrix<DIM, DIM> = as_matrix(&grad_u.as_slice()[r.clone()])?;
            let pg: &Matrix<DIM, DIM> = as_matrix(&prev_grad_u.as_slice()[r.clone()])?;
            let ps: Matrix<DIM, DIM> = *as_matrix(&prev_sigma.as_slice()[rp.clone()])?;
            let pe: &Matrix<DIM, DIM> = as_matrix(&prev_inel.as_slice()[rp])?;
            let (sigma_in, r_in) = if committed {
                // UD-583: the committed state of the point
                (
                    *as_matrix::<DIM, DIM>(&prev_sigma.as_slice()[r.clone()])?,
                    prev_iso.as_slice()[q],
                )
            } else {
                (
                    *as_matrix::<DIM, DIM>(&sigma.as_slice()[r.clone()])?,
                    iso.as_slice()[q],
                )
            };
            let (delta, r_new) = self.flow(&sigma_in, r_in);
            iso.as_mut_slice()[q] = r_new;
            let grad_delta_u = g.sub(pg);
            let (s_new, e_new) =
                stress_and_inelastic_strain_on_quad(&constants, &grad_delta_u, &delta, &ps, pe, 0.);
            sigma.as_mut_slice()[r.clone()].copy_from_slice(s_new.as_flattened());
            inel.as_mut_slice()[r].copy_from_slice(e_new.as_flattened());
        }
        self.elastic
            .base_mut()
            .stress
            .get_mut(ty, ghost)?
            .copy_from(&sigma)?;
        self.plastic
            .inelastic_strain
            .get_mut(ty, ghost)?
            .copy_from(&inel)?;
        self.plastic
            .iso_hardening
            .get_mut(ty, ghost)?
            .copy_from(&iso)?;
        Ok(())
    }

    fn compute_tangent_moduli(
        &mut self,
        _ctx: &LawContext<'_>,
        ty: ElementType,
        ghost: GhostType,
        tangent: &mut Array<Real>,
    ) -> Result<()> {
        small_strain_only("visco_plastic", self.elastic.base().finite_deformation)?;
        let nq = self.elastic.base().grad_u.get(ty, ghost)?.len();
        let v = tangent_stiffness_voigt_size(DIM);
        check_lengths(
            "MaterialViscoPlastic::computeTangentModuli",
            nq,
            &[tangent.len()],
        )?;
        check_lengths(
            "MaterialViscoPlastic::computeTangentModuli: components",
            v * v,
            &[tangent.nb_component()],
        )?;
        for t in tangent.as_mut_slice().chunks_exact_mut(v * v) {
            self.tangent_on_quad(t)?;
        }
        Ok(())
    }

    /// Upstream `MaterialPlastic::computePotentialEnergy`: `½σ:(∇u − ε_p)`.
    fn compute_potential_energy(&mut self, _ctx: &LawContext<'_>, ty: ElementType) -> Result<()> {
        let nq = self.elastic.base().grad_u.get(ty, NG)?.len();
        let density = self.densities(ty, 0..nq)?;
        let mut epot = self.elastic.base_mut().potential_energy.get_mut(ty, NG)?;
        check_lengths("MaterialViscoPlastic: potential_energy", nq, &[epot.len()])?;
        epot.as_mut_slice().copy_from_slice(&density);
        Ok(())
    }

    fn potential_energy_by_element(
        &self,
        ctx: &LawContext<'_>,
        element: Element,
    ) -> Result<Vec<Real>> {
        let points =
            akantu_solid::materials::damage::element_points(self.elastic.base(), ctx, element)?;
        self.densities(element.ty(), points)
    }

    fn energy_of(&mut self, ctx: &LawContext<'_>, energy_id: &str) -> Result<Real> {
        match energy_id {
            "plastic" => self.plastic.plastic_energy(self.elastic.base(), ctx),
            "potential" => self.potential_energy(ctx),
            _ => Ok(0.0),
        }
    }

    /// Upstream `MaterialPlastic::updateEnergies` (plastic work).
    fn update_energies(&mut self, ctx: &LawContext<'_>, ty: ElementType) -> Result<()> {
        let exec = akantu_solid::material::points_exec(self, ctx);
        self.plastic
            .update_energies::<DIM>(exec, self.elastic.base(), ty)
    }

    fn push_wave_speed(&self, element: Element) -> Result<Real> {
        self.elastic.push_wave_speed(element)
    }

    fn shear_wave_speed(&self, element: Element) -> Result<Real> {
        self.elastic.shear_wave_speed(element)
    }

    fn tangent_type(&self) -> akantu_core::enums::MatrixType {
        akantu_core::enums::MatrixType::Symmetric
    }

    /// Upstream: `computeTangentModuli` never records the assembly, so the
    /// inherited `MaterialElastic` flag keeps reporting a change.
    fn has_stiffness_matrix_changed(&self) -> bool {
        true
    }

    fn options(&self) -> Option<&LawOptions> {
        Some(&self.elastic.options)
    }

    fn options_mut(&mut self) -> Option<&mut LawOptions> {
        Some(&mut self.elastic.options)
    }

    fn as_any(&self) -> Option<&dyn std::any::Any> {
        Some(self)
    }

    fn as_any_mut(&mut self) -> Option<&mut dyn std::any::Any> {
        Some(self)
    }
}

/// Register `visco_plastic` (upstream `INSTANTIATE_MATERIAL(visco_plastic,
/// MaterialViscoPlastic)`).
///
/// # Errors
/// A duplicate registration.
pub fn register(factory: &mut MaterialFactory, options: ExtraLawOptions) -> Result<()> {
    crate::damage_law::register_law!(factory, "visco_plastic", MaterialViscoPlastic, options)
}
