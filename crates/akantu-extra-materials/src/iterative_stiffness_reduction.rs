// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): extra_packages/extra-materials/src/
// material_damage/material_iterative_stiffness_reduction.hh,
// material_iterative_stiffness_reduction.cc.

//! `iterative_stiffness_reduction` (XMAT-003): the regularised
//! sequentially linear saw-tooth softening of Rots & Invernizzi (2004,
//! section 4.2), derived from `damage_iterative` (upstream
//! `MaterialIterativeStiffnessReduction<dim> : MaterialDamageIterative<dim>`).
//!
//! * `initMaterial`: the parent's, then per point (both ghost types, every
//!   filter type) the ultimate strain `ε_u = (2·G_f)/(Sc·h)` (`h` =
//!   `crack_band_width`) and the softening tangent `D = Sc/(ε_u − Sc/E)`;
//! * stress, equivalent stress and maximum: the parent's;
//! * `updateDamage()`: as the parent's selection, but the `k`-th reduction
//!   sets `d = 1 − 1/a^k` (`a` = `reduction_constant`) and the strength
//!   `Sc = ε_u (1 − d) E D / ((1 − d) E + D)`, the last one (`k =
//!   max_reductions`) `d = max_damage` (strength unchanged).
//!
//! Parameters (after the parent's): `Gf`, `crack_band_width` (both
//! parsable-modifiable, no default: 0), `reduction_constant` (2).
//! Internals (after the parent's): `ultimate_strain`, `tangent`.
//!
//! `a^k` is `std::pow(double, unsigned)` upstream (promoted to `pow(double,
//! double)`), here glibc's `pow` ported bit for bit
//! ([`akantu_core::glibc_libm::pow`]); for the default `a = 2` every power
//! is exact.

use std::sync::Arc;

use akantu_core::enums::MatrixType;
use akantu_core::{
    Array, Element, ElementType, ElementTypeMapArray, Error, GhostType, Idx, Real, Result, UInt,
};
use akantu_model::InternalField;
use akantu_model::constitutive_law::{
    ConstitutiveLaw, InternalFieldMut, InternalFieldRef, LawContext, LawReducer, LawSpec,
    ModelServices,
};
use akantu_parser::param::{ParamDescriptor, ParameterAccess};
use akantu_solid::material::{
    DerivedMaterialParams, LawOptions, Material, MaterialBase, MaterialFactory,
};

use crate::damage_iterative::{
    IterativeDamage, MaterialDamageIterative, PointUpdate, agreed_damage_update,
};
use crate::kernels::check_lengths;
use crate::options::ExtraLawOptions;

const NG: GhostType = GhostType::NotGhost;

/// `iterative_stiffness_reduction` (upstream
/// `MaterialIterativeStiffnessReduction<DIM>`).
#[derive(Clone, Debug)]
pub struct MaterialIterativeStiffnessReduction<const DIM: usize> {
    /// The `damage_iterative` parent.
    pub parent: MaterialDamageIterative<DIM>,
    /// Ultimate strain `ε_u` per point (`ultimate_strain`).
    pub eps_u: InternalField<Real>,
    /// Softening tangent `D` per point (`tangent`).
    pub d: InternalField<Real>,
    /// Fracture energy `Gf`.
    pub gf: Real,
    /// `crack_band_width`.
    pub crack_band_width: Real,
    /// `reduction_constant` (`a`).
    pub reduction_constant: Real,
}

impl<const DIM: usize> MaterialIterativeStiffnessReduction<DIM> {
    /// New law (upstream constructor chain).
    ///
    /// # Errors
    /// A spatial dimension other than `DIM`.
    pub fn new(spec: &LawSpec, mesh: Option<&akantu_mesh::Mesh>) -> Result<Self> {
        Ok(Self {
            parent: MaterialDamageIterative::with_type(
                spec,
                mesh,
                "iterative_stiffness_reduction",
            )?,
            eps_u: InternalField::new("ultimate_strain", 1, 0.0)?,
            d: InternalField::new("tangent", 1, 0.0)?,
            gf: 0.,
            crack_band_width: 0.,
            reduction_constant: 2.,
        })
    }

    /// The per-point initialisation of upstream `initMaterial`: `ε_u` and
    /// `D` from `Sc`, `Gf`, `crack_band_width` and `E`.
    ///
    /// # Errors
    /// Missing blocks or size mismatches.
    pub fn init_ultimate_strain(&mut self) -> Result<()> {
        let e = self.parent.elastic.thermal.e;
        for ghost in GhostType::ALL {
            for ty in self.parent.elastic.base().filter_types(ghost) {
                let sc = self.parent.sc.get(ty, ghost)?;
                let mut eps_u = self.eps_u.get_mut(ty, ghost)?;
                let mut d = self.d.get_mut(ty, ghost)?;
                check_lengths(
                    "MaterialIterativeStiffnessReduction::initMaterial",
                    d.len(),
                    &[sc.len(), eps_u.len()],
                )?;
                for ((&s, eu), dd) in sc
                    .as_slice()
                    .iter()
                    .zip(eps_u.as_mut_slice())
                    .zip(d.as_mut_slice())
                {
                    *eu = (2. * self.gf) / (s * self.crack_band_width);
                    *dd = s / ((*eu) - (s / e));
                }
            }
        }
        Ok(())
    }

    /// Upstream `updateDamage()`: staged locally, agreed with the count
    /// reduction, then applied (`agreed_damage_update`).
    ///
    /// # Errors
    /// Missing blocks or communication failures; on more than one rank a
    /// failure on any rank is returned on every rank and changes nothing.
    pub fn update_damage_reduction(&mut self, ctx: &LawContext<'_>) -> Result<UInt> {
        let staged = self.stage_reduction();
        let reduce = self.parent.reduces_count(ctx);
        let parent = &mut self.parent;
        agreed_damage_update(
            ctx.services.mesh().communicator(),
            "iterative_stiffness_reduction:update_damage",
            reduce,
            staged,
            |changes| parent.apply_sawtooth(&changes),
        )
    }

    /// The local part of `updateDamage`: the new `(damage, reduction count,
    /// Sc)` of every damaged point, nothing modified.
    fn stage_reduction(&self) -> Result<(Vec<PointUpdate>, UInt)> {
        let p = &self.parent;
        let mut changes = Vec::new();
        let norm_max = p.norm_max_equivalent_stress();
        if norm_max >= 1. {
            let e = p.elastic.thermal.e;
            let threshold = (1. - p.dam_tolerance) * norm_max;
            for ty in p.elastic.base().filter_types(NG) {
                let eq = p.equivalent_stress.get(ty, NG)?;
                let eps_u = self.eps_u.get(ty, NG)?;
                let dd = self.d.get(ty, NG)?;
                let dam = p.damage.damage.get(ty, NG)?;
                let red = p.reduction_step.get(ty, NG)?;
                let sc = p.sc.get(ty, NG)?;
                check_lengths(
                    "MaterialIterativeStiffnessReduction::updateDamage",
                    eq.len(),
                    &[eps_u.len(), dd.len(), dam.len(), red.len(), sc.len()],
                )?;
                for q in 0..eq.len() {
                    if eq.as_slice()[q] >= threshold {
                        let r = red.as_slice()[q];
                        if r == p.max_reductions {
                            continue;
                        }
                        let r = r + 1;
                        let (d, new_sc) = if r == p.max_reductions {
                            (p.max_damage, None)
                        } else {
                            let d = 1.
                                - (1.
                                    / akantu_core::glibc_libm::pow(
                                        self.reduction_constant,
                                        Real::from(r),
                                    ));
                            let (eu, tan) = (eps_u.as_slice()[q], dd.as_slice()[q]);
                            (d, Some(eu * (1. - d) * e * tan / ((1. - d) * e + tan)))
                        };
                        changes.push(PointUpdate {
                            ty,
                            q,
                            damage: d,
                            reductions: r,
                            sc: new_sc,
                        });
                    }
                }
            }
        }
        let n = UInt::try_from(changes.len())
            .map_err(|_| Error::invalid("updateDamage: damaged-point count overflows UInt"))?;
        Ok((changes, n))
    }

    const PARAMS: &'static [ParamDescriptor<Self>] = &[
        akantu_solid::real_param!(Self, "Gf", ParameterAccess::PARSMOD, "fracture energy", gf),
        akantu_solid::real_param!(
            Self,
            "crack_band_width",
            ParameterAccess::PARSMOD,
            "crack_band_width",
            crack_band_width
        ),
        akantu_solid::real_param!(
            Self,
            "reduction_constant",
            ParameterAccess::PARSMOD,
            "reduction constant",
            reduction_constant
        ),
    ];
}

impl<const DIM: usize> DerivedMaterialParams for MaterialIterativeStiffnessReduction<DIM> {
    type Parent = MaterialDamageIterative<DIM>;
    fn parent(&self) -> &Self::Parent {
        &self.parent
    }
    fn parent_mut(&mut self) -> &mut Self::Parent {
        &mut self.parent
    }
    fn derived_params(&self) -> &'static [ParamDescriptor<Self>] {
        Self::PARAMS
    }
    fn update_derived(&mut self) -> Result<()> {
        akantu_parser::ParameterRegistry::update_internal_parameters(&mut self.parent)
    }
    fn derived_id(&self) -> &str {
        &self.parent.elastic.base().id
    }
}

akantu_solid::impl_derived_material_registry!(MaterialIterativeStiffnessReduction<DIM>, const DIM);

impl<const DIM: usize> ConstitutiveLaw for MaterialIterativeStiffnessReduction<DIM> {
    fn name(&self) -> &str {
        self.parent.name()
    }
    fn law_id(&self) -> &str {
        self.parent.law_id()
    }
    fn law_type(&self) -> &str {
        self.parent.law_type()
    }
    fn element_filter(&self) -> &ElementTypeMapArray<Idx> {
        self.parent.element_filter()
    }
    fn element_filter_mut(&mut self) -> &mut ElementTypeMapArray<Idx> {
        self.parent.element_filter_mut()
    }
    fn for_each_internal(&self, f: &mut dyn FnMut(&str, InternalFieldRef<'_>)) {
        self.parent.visit(f);
        f("ultimate_strain", InternalFieldRef::Real(&self.eps_u));
        f("tangent", InternalFieldRef::Real(&self.d));
    }
    fn for_each_internal_mut(
        &mut self,
        f: &mut dyn FnMut(&str, InternalFieldMut<'_>) -> Result<()>,
    ) -> Result<()> {
        self.parent.visit_mut(f)?;
        f("ultimate_strain", InternalFieldMut::Real(&mut self.eps_u))?;
        f("tangent", InternalFieldMut::Real(&mut self.d))
    }
}

impl<const DIM: usize> Material for MaterialIterativeStiffnessReduction<DIM> {
    fn base(&self) -> &MaterialBase {
        self.parent.base()
    }
    fn base_mut(&mut self) -> &mut MaterialBase {
        self.parent.base_mut()
    }

    /// Upstream `initMaterial`: the parent's (every internal of the law
    /// resized in register-id order), then `ε_u` and `D`.
    fn init_material(&mut self, ctx: &LawContext<'_>) -> Result<()> {
        MaterialDamageIterative::init_like_elastic(self, ctx, |s| &mut s.parent)?;
        self.init_ultimate_strain()
    }

    fn compute_stress(
        &mut self,
        ctx: &LawContext<'_>,
        ty: ElementType,
        ghost: GhostType,
    ) -> Result<()> {
        self.parent.compute_stress(ctx, ty, ghost)
    }

    fn compute_all_stresses(&mut self, ctx: &LawContext<'_>, ghost: GhostType) -> Result<()> {
        self.parent.compute_all_stresses(ctx, ghost)
    }

    fn reduce_after_stresses(
        &mut self,
        ghost: GhostType,
        reducer: &mut dyn LawReducer,
    ) -> Result<()> {
        self.parent.reduce_after_stresses(ghost, reducer)
    }

    fn compute_tangent_moduli(
        &mut self,
        ctx: &LawContext<'_>,
        ty: ElementType,
        ghost: GhostType,
        tangent: &mut Array<Real>,
    ) -> Result<()> {
        self.parent.compute_tangent_moduli(ctx, ty, ghost, tangent)
    }

    fn compute_potential_energy(&mut self, ctx: &LawContext<'_>, ty: ElementType) -> Result<()> {
        self.parent.compute_potential_energy(ctx, ty)
    }

    fn energy_of(&mut self, ctx: &LawContext<'_>, energy_id: &str) -> Result<Real> {
        self.parent.energy_of(ctx, energy_id)
    }

    fn potential_energy_by_element(
        &self,
        ctx: &LawContext<'_>,
        element: Element,
    ) -> Result<Vec<Real>> {
        self.parent.potential_energy_by_element(ctx, element)
    }

    fn update_energies(&mut self, ctx: &LawContext<'_>, ty: ElementType) -> Result<()> {
        self.parent.update_energies(ctx, ty)
    }

    fn update_energies_after_damage(
        &mut self,
        ctx: &LawContext<'_>,
        ty: ElementType,
    ) -> Result<()> {
        self.parent.update_energies_after_damage(ctx, ty)
    }

    fn push_wave_speed(&self, element: Element) -> Result<Real> {
        self.parent.push_wave_speed(element)
    }

    fn shear_wave_speed(&self, element: Element) -> Result<Real> {
        self.parent.shear_wave_speed(element)
    }

    fn tangent_type(&self) -> MatrixType {
        MatrixType::Symmetric
    }

    fn has_stiffness_matrix_changed(&self) -> bool {
        true
    }

    fn options(&self) -> Option<&LawOptions> {
        self.parent.options()
    }

    fn options_mut(&mut self) -> Option<&mut LawOptions> {
        self.parent.options_mut()
    }

    fn compute_all_cauchy_stresses(&mut self, ghost: GhostType) -> Result<()> {
        self.parent.compute_all_cauchy_stresses(ghost)
    }

    fn as_any(&self) -> Option<&dyn std::any::Any> {
        Some(self)
    }

    fn as_any_mut(&mut self) -> Option<&mut dyn std::any::Any> {
        Some(self)
    }
}

impl<const DIM: usize> IterativeDamage for MaterialIterativeStiffnessReduction<DIM> {
    fn norm_max_equivalent_stress(&self) -> Real {
        self.parent.norm_max_equivalent_stress()
    }

    fn update_damage(&mut self, ctx: &LawContext<'_>) -> Result<UInt> {
        self.update_damage_reduction(ctx)
    }

    fn set_rve_mode(&mut self, in_rve: bool) {
        self.parent.set_rve_mode(in_rve);
    }
}

/// Register `iterative_stiffness_reduction` (upstream
/// `INSTANTIATE_MATERIAL(iterative_stiffness_reduction, …)`).
///
/// # Errors
/// A duplicate registration.
pub fn register(factory: &mut MaterialFactory, options: ExtraLawOptions) -> Result<()> {
    factory.register(
        "iterative_stiffness_reduction",
        Arc::new(move |spec: &LawSpec, services: &dyn ModelServices| {
            let mesh = Some(services.mesh());
            akantu_core::with_dim!(spec.spatial_dimension, D => {
                let mut law = MaterialIterativeStiffnessReduction::<D>::new(spec, mesh)?;
                crate::options::HasExtraOptions::set_extra_options(&mut law, options);
                Box::new(law) as Box<dyn Material>
            })
        }),
    )
}
