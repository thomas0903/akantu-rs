// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/model/solid_mechanics/materials/
// material_damage/material_marigo.hh, material_marigo.cc,
// material_marigo_inline_impl.hh (and the MaterialDamage / MaterialElastic
// parents it calls).

//! `marigo` (MAT-011): energy-release-rate isotropic damage on top of
//! `elastic` (upstream `MaterialMarigo<dim> : MaterialDamage<dim>`), all
//! dimensions, plane strain/stress.
//!
//! Per quadrature point (`computeStressOnQuad`, operand order kept, D-008),
//! with the default [`LawOptions`]:
//!
//! 0. the damage starts from its value committed at the last
//!    `savePreviousState` (UD-138), so an earlier trial of the same step
//!    leaves no trace;
//! 1. `σ = MaterialElastic::computeStressOnQuad(∇u, σ_th)` with the thermal
//!    stress refreshed first (UD-038);
//! 2. `Y = σ : ε(∇u) / 2` with `ε = ½(∇uᵀ + ∇u)` (halving-tree sum); with
//!    a thermal stress and [`ThermalEnergy::StoredElastic`], plus
//!    `½ σ_th (tr ∇u − m αΔT)` (the undamaged stored energy, UD-109 family);
//! 3. `damage_in_y`: `Y *= (1 − d)` with the damage of step 0; `Yc limit`:
//!    `Y = std::min(Y, Yc)`, `Yc = ε_c·E·ε_c/2`;
//! 4. local damage update (`computeDamageAndStressOnQuad`):
//!    `F = Y − Yd − Sd·d_prev`; if `F > 0`: `d = (Y − Yd)/Sd`;
//!    `d = std::max(d, d_prev)`; `d = std::min(d, 1)`; `σ *= 1 − d`.
//!
//! `Yd` is a random internal field (`Yd = 50 uniform [0, 10]` draws one
//! value per quadrature point at `initMaterial` from the global engine in
//! upstream order, D-006); `Sd` a plain parameter. Tangent, energies and
//! history: [`super::damage`]. With `finite_deformation` the law returns
//! [`Error::Unsupported`] (UD-142). [`LawOptions::upstream`] reproduces
//! upstream bit for bit.
//!
//! # Upstream defects
//!
//! * **UD-038** (category B): upstream never refreshes `sigma_th` (the
//!   kernel of step 1 uses its initial value 0 even with `delta_T`).
//!   [`LawOptions::damage_thermal_refresh`] (default) runs the thermal
//!   pre-step first.
//! * **UD-138** (category B): upstream starts from the current damage, so
//!   the damage of a high trial enters `damage_in_y` and survives an
//!   unloaded trial of the same step (`F ≤ 0` keeps it);
//!   [`LawOptions::committed_trial_state`].
//! * **UD-142** (category B): with `finite_deformation` upstream feeds the
//!   raw `∇u` (not the Green strain) to the elastic kernel and stores the
//!   result as the second Piola–Kirchhoff stress (not frame indifferent);
//!   [`FiniteStrainDamage::UpstreamLegacy`] reproduces it.
//! * **UD-027**: `marigo_non_local` is a non-local law; the local law
//!   honours `is_non_local` (step 4 skipped) like upstream.
//!
//! [`LawOptions::damage_thermal_refresh`]: crate::material::LawOptions::damage_thermal_refresh
//! [`LawOptions::committed_trial_state`]: crate::material::LawOptions::committed_trial_state
//! [`LawOptions::upstream`]: crate::material::LawOptions::upstream

use std::sync::Arc;

use akantu_core::enums::MatrixType;
use akantu_core::exec::{Tuples, TuplesRef};
use akantu_core::random::RandomParameter;
use akantu_core::tensor::{Matrix, MatrixExt, SquareMatrixExt, as_matrix, as_matrix_mut};
use akantu_core::{
    Array, Element, ElementType, ElementTypeMapArray, Error, GhostType, Idx, Real, Result,
};
use akantu_model::InternalField;
use akantu_model::constitutive_law::{
    ConstitutiveLaw, InternalFieldMut, InternalFieldRef, LawContext, LawSpec, ModelServices,
};
use akantu_parser::param::{ParamDescriptor, ParameterAccess};

use super::damage::{
    DamageFields, elastic_damage_energy_densities, elastic_damage_potential_energy, std_max,
    std_min,
};
use super::elastic::MaterialElastic;
use super::thermal::{ThermalEnergy, thermal_energy_correction};
use crate::material::{
    FiniteStrainDamage, LawOptions, Material, MaterialBase, MaterialFactory, MaterialParams,
};
use crate::material_tensor::{grad_u_to_epsilon, tangent_stiffness_voigt_size};

/// Marigo damage law (upstream `MaterialMarigo<DIM>`).
#[derive(Clone, Debug)]
pub struct MaterialMarigo<const DIM: usize> {
    /// The elastic parent (parameters, thermal state, base internals,
    /// options).
    pub elastic: MaterialElastic<DIM>,
    /// `damage`, `damage dissipated energy`, `integral of sigma`.
    pub damage: DamageFields,
    /// Energy release rate `Y` (internal).
    pub y: InternalField<Real>,
    /// Damage threshold `Yd` (random internal field).
    pub yd: InternalField<Real>,
    /// Damage softening `Sd`.
    pub sd: Real,
    /// Critical strain `epsilon_c`.
    pub epsilon_c: Real,
    /// `damage_in_y`: use `(1 − d)·Y`.
    pub damage_in_y: bool,
    yc: Real,
    yc_limit: bool,
}

impl<const DIM: usize> MaterialMarigo<DIM> {
    /// New law (upstream constructor chain).
    pub fn new(spec: &LawSpec, mesh: Option<&akantu_mesh::Mesh>) -> Result<Self> {
        let mut elastic = MaterialElastic::<DIM>::new(spec, mesh)?;
        elastic.base_mut().law_type = "marigo".into();
        Ok(Self {
            elastic,
            damage: DamageFields::new()?,
            y: InternalField::new("Y", 1, 0.0)?,
            yd: InternalField::new_random("Yd", 1, RandomParameter::default())?,
            sd: 5000.,
            epsilon_c: 0.,
            damage_in_y: false,
            yc: 0.,
            yc_limit: false,
        })
    }

    /// `Yc = ε_c·E·ε_c/2`.
    #[must_use]
    pub fn yc(&self) -> Real {
        self.yc
    }

    /// Upstream `Yc limit` (`|ε_c| > ε_machine`).
    #[must_use]
    pub fn yc_limit(&self) -> bool {
        self.yc_limit
    }

    /// Upstream `MaterialMarigo::updateInternalParameters`.
    fn update(&mut self) -> Result<()> {
        MaterialParams::update_params(&mut self.elastic)?;
        self.yc = self.epsilon_c * self.elastic.thermal.e * self.epsilon_c / 2.;
        self.yc_limit = self.epsilon_c.abs() > Real::EPSILON;
        Ok(())
    }

    /// Steps 2–3 of the module docs: the energy release rate of one point
    /// from the elastic stress `sigma` of `grad_u`, the current damage and
    /// the thermal state (`sigma_th`, `alpha_delta_t = αΔT`).
    #[must_use]
    pub fn energy_release_rate(
        &self,
        sigma: &Matrix<DIM, DIM>,
        grad_u: &Matrix<DIM, DIM>,
        damage: Real,
        sigma_th: Real,
        alpha_delta_t: Real,
    ) -> Real {
        let mut y = sigma.double_dot(&grad_u_to_epsilon(grad_u)) / 2.;
        if self.elastic.options.thermal_energy == ThermalEnergy::StoredElastic && sigma_th != 0. {
            y += thermal_energy_correction::<DIM>(
                self.elastic.plane_stress,
                sigma_th,
                grad_u.trace(),
                alpha_delta_t,
            );
        }
        if self.damage_in_y {
            y *= 1. - damage;
        }
        if self.yc_limit {
            y = std_min(y, self.yc);
        }
        y
    }

    /// Upstream `computeDamageAndStressOnQuad`: returns the new damage and
    /// scales `sigma` by `1 − d`.
    #[must_use]
    pub fn damage_and_stress_on_quad(
        &self,
        sigma: &mut Matrix<DIM, DIM>,
        damage: Real,
        previous_damage: Real,
        y: Real,
        yd: Real,
    ) -> Real {
        let mut dam = damage;
        let fd = y - yd - self.sd * previous_damage;
        if fd > 0. {
            dam = (y - yd) / self.sd;
        }
        dam = std_max(dam, previous_damage);
        dam = std_min(dam, 1.);
        let f = 1. - dam;
        for col in sigma.iter_mut() {
            for s in col.iter_mut() {
                *s *= f;
            }
        }
        dam
    }
}

impl<const DIM: usize> MaterialParams for MaterialMarigo<DIM> {
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
        self.update()
    }
}

impl<const DIM: usize> MaterialMarigo<DIM> {
    const PARAMS: &'static [ParamDescriptor<Self>] = crate::elastic_parent_params!(
        Self,
        [elastic],
        [
            crate::real_param!(Self, "Sd", ParameterAccess::PARSMOD, "Damage softening", sd),
            crate::real_param!(
                Self,
                "epsilon_c",
                ParameterAccess::PARSABLE,
                "Critical strain",
                epsilon_c
            ),
            crate::bool_param!(
                Self,
                "Yc limit",
                ParameterAccess::INTERNAL,
                "As the material a critical Y",
                yc_limit
            ),
            crate::bool_param!(
                Self,
                "damage_in_y",
                ParameterAccess::PARSABLE,
                "Use threshold (1-D)Y",
                damage_in_y
            ),
            crate::random_field_param!(
                Self,
                "Yd",
                ParameterAccess::PARSABLE,
                "Damaging energy threshold",
                yd
            ),
        ]
    );
    const PARAMS_2D: &'static [ParamDescriptor<Self>] = crate::elastic_parent_params!(plane_stress Self, [elastic], [
        crate::real_param!(Self, "Sd", ParameterAccess::PARSMOD, "Damage softening", sd),
        crate::real_param!(Self, "epsilon_c", ParameterAccess::PARSABLE, "Critical strain", epsilon_c),
        crate::bool_param!(Self, "Yc limit", ParameterAccess::INTERNAL, "As the material a critical Y", yc_limit),
        crate::bool_param!(Self, "damage_in_y", ParameterAccess::PARSABLE, "Use threshold (1-D)Y", damage_in_y),
        crate::random_field_param!(Self, "Yd", ParameterAccess::PARSABLE, "Damaging energy threshold", yd),
    ]);
}

crate::impl_material_registry!(MaterialMarigo<DIM>, const DIM);

impl<const DIM: usize> ConstitutiveLaw for MaterialMarigo<DIM> {
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
        f("Y", InternalFieldRef::Real(&self.y));
        f("Yd", InternalFieldRef::Real(&self.yd));
    }
    fn for_each_internal_mut(
        &mut self,
        f: &mut dyn FnMut(&str, InternalFieldMut<'_>) -> Result<()>,
    ) -> Result<()> {
        self.elastic.for_each_internal_mut(f)?;
        self.damage.for_each_mut(f)?;
        f("Y", InternalFieldMut::Real(&mut self.y))?;
        f("Yd", InternalFieldMut::Real(&mut self.yd))
    }
}

impl<const DIM: usize> Material for MaterialMarigo<DIM> {
    fn base(&self) -> &MaterialBase {
        self.elastic.base()
    }
    fn base_mut(&mut self) -> &mut MaterialBase {
        self.elastic.base_mut()
    }

    /// Upstream `MaterialElastic::initMaterial` (not overridden by
    /// `MaterialDamage`/`MaterialMarigo`): history, `resizeInternals` (all
    /// internals in register-id order: the draw order of `Yd`),
    /// `updateInternalParameters`, then `ν = 0` in 1-D and the update again.
    fn init_material(&mut self, ctx: &LawContext<'_>) -> Result<()> {
        self.elastic.base_mut().init_history()?;
        let blocks = self.elastic.base().blocks(ctx);
        self.resize_internals(&blocks)?;
        self.update()?;
        self.elastic.base_mut().set_init();
        if DIM == 1 {
            self.elastic.thermal.nu = 0.;
        }
        self.update()
    }

    fn compute_stress(
        &mut self,
        ctx: &LawContext<'_>,
        ty: ElementType,
        ghost: GhostType,
    ) -> Result<()> {
        if self.elastic.base().finite_deformation
            && self.elastic.options.finite_strain_damage == FiniteStrainDamage::Unsupported
        {
            return Err(Error::Unsupported(
                "marigo with finite_deformation: no reviewed objective finite-strain damage \
                 formulation (UD-142); FiniteStrainDamage::UpstreamLegacy \
                 reproduces upstream"
                    .into(),
            ));
        }
        if self.elastic.options.damage_thermal_refresh {
            let exec = crate::material::points_exec(self, ctx);
            self.elastic.compute_thermal_with(exec, ty, ghost)?;
        }
        let committed = self.elastic.options.committed_trial_state;
        let constants = self.elastic.constants();
        let alpha = self.elastic.thermal.alpha;
        let non_local = self.elastic.base().is_non_local;
        let n = DIM * DIM;
        let sigma_th = self.elastic.sigma_th.get(ty, ghost)?.clone();
        let delta_t = self.elastic.delta_t.get(ty, ghost)?.clone();
        let grad_u = self.elastic.base().grad_u.get(ty, ghost)?.clone();
        let yd = self.yd.get(ty, ghost)?.clone();
        let mut sigma = self.elastic.base().sigma_field()?.get(ty, ghost)?.clone();
        let (mut dam, prev_dam) = {
            let (c, p) = self.damage.damage.current_and_previous_mut(ty, ghost)?;
            (c.clone(), p.clone())
        };
        let mut y = self.y.get(ty, ghost)?.clone();
        let nq = grad_u.len();
        if [
            sigma.len(),
            sigma_th.len(),
            delta_t.len(),
            yd.len(),
            dam.len(),
            prev_dam.len(),
            y.len(),
        ]
        .iter()
        .any(|&l| l != nq)
        {
            return Err(Error::DimensionMismatch {
                context: "MaterialMarigo::computeStress",
                expected: nq,
                got: sigma.len().min(dam.len()).min(yd.len()),
            });
        }
        if grad_u.nb_component() != n || sigma.nb_component() != n {
            return Err(Error::DimensionMismatch {
                context: "MaterialMarigo::computeStress: components",
                expected: n,
                got: sigma.nb_component(),
            });
        }
        // every point from its own values (the copies above): parts on the
        // model's threads (D-123)
        let items = (
            (
                TuplesRef::new(grad_u.as_slice(), n),
                TuplesRef::new(sigma_th.as_slice(), 1),
                TuplesRef::new(delta_t.as_slice(), 1),
                TuplesRef::new(yd.as_slice(), 1),
                TuplesRef::new(prev_dam.as_slice(), 1),
            ),
            Tuples::new(sigma.as_mut_slice(), n),
            Tuples::new(dam.as_mut_slice(), 1),
            Tuples::new(y.as_mut_slice(), 1),
        );
        let this = &*self;
        crate::material::points_exec(this, ctx).try_for_each(
            items,
            1024,
            |_, ((g, th, dt, yd, pd), mut sigma, mut dam, mut y)| {
                let points = g
                    .as_slice()
                    .chunks_exact(n)
                    .zip(sigma.as_mut_slice().chunks_exact_mut(n))
                    .zip(th.as_slice().iter().zip(dt.as_slice()))
                    .zip(yd.as_slice().iter().zip(pd.as_slice()))
                    .zip(dam.as_mut_slice().iter_mut().zip(y.as_mut_slice()));
                for ((((g, s), (&th, &dt)), (&yd, &pd)), (d, y)) in points {
                    let g: &Matrix<DIM, DIM> = as_matrix(g)?;
                    let s: &mut Matrix<DIM, DIM> = as_matrix_mut(s)?;
                    constants.stress_on_quad(g, th, s);
                    // UD-138: the damage of an earlier trial of the step is
                    // not kept (threshold branch, `damage_in_y`)
                    if committed {
                        *d = pd;
                    }
                    let yq = this.energy_release_rate(s, g, *d, th, dt * alpha);
                    *y = yq;
                    if !non_local {
                        *d = this.damage_and_stress_on_quad(s, *d, pd, yq, yd);
                    }
                }
                Ok(())
            },
        )?;
        self.elastic
            .base_mut()
            .sigma_field_mut()?
            .get_mut(ty, ghost)?
            .copy_from(&sigma)?;
        self.damage.damage.get_mut(ty, ghost)?.copy_from(&dam)?;
        self.y.get_mut(ty, ghost)?.copy_from(&y)?;
        Ok(())
    }

    /// The stress, energy release rate and damage of a point read only that
    /// point's gradient, thermal stress, temperature change, threshold and
    /// damage (copies of the blocks, written back after the loop) and the
    /// law's constants; the thermal refresh runs per block before the loop.
    /// The non-local variant runs the same local loop (`Y` only) and
    /// averages afterwards, in its own phase.
    fn independent_points(&self) -> bool {
        true
    }

    fn compute_tangent_moduli(
        &mut self,
        _ctx: &LawContext<'_>,
        ty: ElementType,
        ghost: GhostType,
        tangent: &mut Array<Real>,
    ) -> Result<()> {
        let el = &self.elastic;
        self.damage.tangent(
            ty,
            ghost,
            tangent_stiffness_voigt_size(DIM),
            tangent,
            &|t| el.tangent_on_quad(t),
        )
    }

    fn compute_potential_energy(&mut self, _ctx: &LawContext<'_>, ty: ElementType) -> Result<()> {
        elastic_damage_potential_energy(&mut self.elastic, &self.damage.damage, ty)
    }

    fn energy_of(&mut self, ctx: &LawContext<'_>, energy_id: &str) -> Result<Real> {
        match energy_id {
            "dissipated" => self.damage.dissipated_energy(self.elastic.base(), ctx),
            "potential" => self.potential_energy(ctx),
            _ => Ok(0.0),
        }
    }

    /// The law's density at the element's points (upstream inherits
    /// `MaterialElastic::computePotentialEnergyByElement`, `½σ:∇u`; equal
    /// without a thermal stress).
    fn potential_energy_by_element(
        &self,
        ctx: &LawContext<'_>,
        element: Element,
    ) -> Result<Vec<Real>> {
        let points = super::damage::element_points(self.elastic.base(), ctx, element)?;
        elastic_damage_energy_densities(&self.elastic, &self.damage.damage, element.ty(), points)
    }

    /// Upstream `MaterialDamage::updateEnergies`: `MaterialElastic` adds
    /// nothing, then `computePotentialEnergy` and the dissipated energy.
    fn update_energies(&mut self, ctx: &LawContext<'_>, ty: ElementType) -> Result<()> {
        self.compute_potential_energy(ctx, ty)?;
        let options = self.elastic.options;
        let thermal = if super::damage::thermal_work_enabled(&options) {
            Some(super::damage::elastic_family_thermal_work(
                &self.elastic,
                &self.damage.damage,
                None,
                ty,
            )?)
        } else {
            None
        };
        let work = super::damage::DamageWork::new(&options, thermal.as_deref());
        let exec = crate::material::points_exec(self, ctx);
        self.damage
            .update_energies::<DIM>(exec, self.elastic.base(), ty, work)
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

    /// Upstream `MaterialDamage::hasStiffnessMatrixChanged`: always `true`.
    fn has_stiffness_matrix_changed(&self) -> bool {
        true
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

    /// Upstream `MaterialMarigo::getNbData/packData/unpackData`
    /// (`material_marigo_inline_impl.hh:82-118`): `_clh_init_cl` carries the
    /// random threshold `Yd` of every quadrature point, so that ghost
    /// elements hold their owner's draw (MPI-041; upstream registers no
    /// synchronizer for the tag, UD-900).
    fn exchanged_internals(&self, tag: akantu_core::enums::SynchronizationTag) -> Vec<&str> {
        if tag == akantu_core::enums::SynchronizationTag::ClhInitCl {
            vec!["Yd"]
        } else {
            Vec::new()
        }
    }
}

/// Register `marigo` (upstream `instantiateMaterial<MaterialMarigo>("marigo")`).
pub fn register(factory: &mut MaterialFactory) -> Result<()> {
    factory.register(
        "marigo",
        Arc::new(|spec: &LawSpec, services: &dyn ModelServices| {
            let mesh = Some(services.mesh());
            akantu_core::with_dim!(spec.spatial_dimension, D => {
                Box::new(MaterialMarigo::<D>::new(spec, mesh)?) as Box<dyn Material>
            })
        }),
    )
}
