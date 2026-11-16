// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/model/solid_mechanics/materials/
// material_damage/material_damage.hh, material_damage_tmpl.hh (the
// `MaterialDamage<dim>` parent shared by the extra damage laws).

//! The `MaterialDamage<dim>` plumbing shared by `damage_linear`, `brittle`
//! and `vreepeerlings`: [`impl_damage_law!`] implements
//! `ParameterRegistry`, `ConstitutiveLaw` and `Material` for a law struct
//! with the fields `elastic: MaterialElastic<DIM>` and `damage:
//! DamageFields` and the inherent methods
//!
//! * `visit_own(&self, f)` / `visit_own_mut(&mut self, f)`: the law's own
//!   internals after the damage ones (registration order);
//! * `stress_impl(&mut self, ctx, ty, ghost) -> Result<()>`: upstream
//!   `computeStress(type, ghost)`;
//! * `after_init(&mut self) -> Result<()>`: the law's part of
//!   `initMaterial` after `MaterialDamage::initMaterial`;
//! * `own_params(&self) -> &'static [ParamDescriptor<Self>]` (through
//!   `MaterialParams`, implemented by the law).
//!
//! Tangent `(1 − d)·C`, energies and history are upstream's
//! `MaterialDamage` (`akantu_solid::materials::damage`).

/// See the module documentation.
macro_rules! impl_damage_law {
    ($ty:ident) => {
        akantu_solid::impl_material_registry!($ty<DIM>, const DIM);

        impl<const DIM: usize> akantu_model::constitutive_law::ConstitutiveLaw for $ty<DIM> {
            fn name(&self) -> &str {
                self.elastic.name()
            }
            fn law_id(&self) -> &str {
                self.elastic.law_id()
            }
            fn law_type(&self) -> &str {
                self.elastic.law_type()
            }
            fn element_filter(&self) -> &akantu_core::ElementTypeMapArray<akantu_core::Idx> {
                self.elastic.element_filter()
            }
            fn element_filter_mut(
                &mut self,
            ) -> &mut akantu_core::ElementTypeMapArray<akantu_core::Idx> {
                self.elastic.element_filter_mut()
            }
            fn for_each_internal(
                &self,
                f: &mut dyn FnMut(&str, akantu_model::constitutive_law::InternalFieldRef<'_>),
            ) {
                self.elastic.for_each_internal(f);
                self.damage.for_each(f);
                self.visit_own(f);
            }
            fn for_each_internal_mut(
                &mut self,
                f: &mut dyn FnMut(
                    &str,
                    akantu_model::constitutive_law::InternalFieldMut<'_>,
                ) -> akantu_core::Result<()>,
            ) -> akantu_core::Result<()> {
                self.elastic.for_each_internal_mut(f)?;
                self.damage.for_each_mut(f)?;
                self.visit_own_mut(f)
            }
        }

        impl<const DIM: usize> akantu_solid::Material for $ty<DIM> {
            fn base(&self) -> &akantu_solid::MaterialBase {
                self.elastic.base()
            }
            fn base_mut(&mut self) -> &mut akantu_solid::MaterialBase {
                self.elastic.base_mut()
            }

            /// Upstream `MaterialElastic::initMaterial` (through
            /// `MaterialDamage`), then the law's own part.
            fn init_material(
                &mut self,
                ctx: &akantu_model::constitutive_law::LawContext<'_>,
            ) -> akantu_core::Result<()> {
                use akantu_model::constitutive_law::ConstitutiveLaw as _;
                use akantu_solid::material::MaterialParams as _;
                self.elastic.base_mut().init_history()?;
                let blocks = self.elastic.base().blocks(ctx);
                self.resize_internals(&blocks)?;
                self.update_params()?;
                self.elastic.base_mut().set_init();
                if DIM == 1 {
                    self.elastic.thermal.nu = 0.;
                }
                self.update_params()?;
                self.after_init()
            }

            fn compute_stress(
                &mut self,
                ctx: &akantu_model::constitutive_law::LawContext<'_>,
                ty: akantu_core::ElementType,
                ghost: akantu_core::GhostType,
            ) -> akantu_core::Result<()> {
                crate::kernels::small_strain_only(
                    &self.elastic.base().law_type,
                    self.elastic.base().finite_deformation,
                )?;
                self.stress_impl(ctx, ty, ghost)
            }

            fn independent_points(&self) -> bool {
                self.points_are_independent()
            }

            fn compute_tangent_moduli(
                &mut self,
                _ctx: &akantu_model::constitutive_law::LawContext<'_>,
                ty: akantu_core::ElementType,
                ghost: akantu_core::GhostType,
                tangent: &mut akantu_core::Array<akantu_core::Real>,
            ) -> akantu_core::Result<()> {
                crate::kernels::small_strain_only(
                    &self.elastic.base().law_type,
                    self.elastic.base().finite_deformation,
                )?;
                let el = &self.elastic;
                self.damage.tangent(
                    ty,
                    ghost,
                    akantu_solid::material_tensor::tangent_stiffness_voigt_size(DIM),
                    tangent,
                    &|t| el.tangent_on_quad(t),
                )
            }

            fn compute_potential_energy(
                &mut self,
                _ctx: &akantu_model::constitutive_law::LawContext<'_>,
                ty: akantu_core::ElementType,
            ) -> akantu_core::Result<()> {
                akantu_solid::materials::damage::elastic_damage_potential_energy(
                    &mut self.elastic,
                    &self.damage.damage,
                    ty,
                )
            }

            fn energy_of(
                &mut self,
                ctx: &akantu_model::constitutive_law::LawContext<'_>,
                energy_id: &str,
            ) -> akantu_core::Result<akantu_core::Real> {
                match energy_id {
                    "dissipated" => self.damage.dissipated_energy(self.elastic.base(), ctx),
                    "potential" => self.potential_energy(ctx),
                    _ => Ok(0.0),
                }
            }

            fn potential_energy_by_element(
                &self,
                ctx: &akantu_model::constitutive_law::LawContext<'_>,
                element: akantu_core::Element,
            ) -> akantu_core::Result<Vec<akantu_core::Real>> {
                let points = akantu_solid::materials::damage::element_points(
                    self.elastic.base(),
                    ctx,
                    element,
                )?;
                akantu_solid::materials::damage::elastic_damage_energy_densities(
                    &self.elastic,
                    &self.damage.damage,
                    element.ty(),
                    points,
                )
            }

            /// Upstream `MaterialDamage::updateEnergies`.
            fn update_energies(
                &mut self,
                ctx: &akantu_model::constitutive_law::LawContext<'_>,
                ty: akantu_core::ElementType,
            ) -> akantu_core::Result<()> {
                use akantu_solid::materials::damage as dmg;
                dmg::elastic_damage_potential_energy(&mut self.elastic, &self.damage.damage, ty)?;
                let options = self.elastic.options;
                let thermal = if dmg::thermal_work_enabled(&options) {
                    Some(dmg::elastic_family_thermal_work(
                        &self.elastic,
                        &self.damage.damage,
                        None,
                        ty,
                    )?)
                } else {
                    None
                };
                let work = dmg::DamageWork::new(&options, thermal.as_deref());
                let exec = akantu_solid::material::points_exec(self, ctx);
                self.damage
                    .update_energies::<DIM>(exec, self.elastic.base(), ty, work)
            }

            fn push_wave_speed(
                &self,
                element: akantu_core::Element,
            ) -> akantu_core::Result<akantu_core::Real> {
                self.elastic.push_wave_speed(element)
            }

            fn shear_wave_speed(
                &self,
                element: akantu_core::Element,
            ) -> akantu_core::Result<akantu_core::Real> {
                self.elastic.shear_wave_speed(element)
            }

            fn tangent_type(&self) -> akantu_core::enums::MatrixType {
                akantu_core::enums::MatrixType::Symmetric
            }

            /// Upstream `MaterialDamage::hasStiffnessMatrixChanged`.
            fn has_stiffness_matrix_changed(&self) -> bool {
                true
            }

            fn options(&self) -> Option<&akantu_solid::material::LawOptions> {
                Some(&self.elastic.options)
            }

            fn options_mut(&mut self) -> Option<&mut akantu_solid::material::LawOptions> {
                Some(&mut self.elastic.options)
            }

            fn compute_all_cauchy_stresses(
                &mut self,
                ghost: akantu_core::GhostType,
            ) -> akantu_core::Result<()> {
                self.elastic.compute_all_cauchy_stresses(ghost)
            }

            fn as_any(&self) -> Option<&dyn std::any::Any> {
                Some(self)
            }

            fn as_any_mut(&mut self) -> Option<&mut dyn std::any::Any> {
                Some(self)
            }
        }
    };
}
pub(crate) use impl_damage_law;

/// Register a dimension-generic law under `keyword` (with the
/// [`crate::ExtraLawOptions`] `$options` set at construction when given).
macro_rules! register_law {
    ($factory:expr, $keyword:literal, $ty:ident) => {
        $factory.register(
            $keyword,
            std::sync::Arc::new(
                |spec: &akantu_model::constitutive_law::LawSpec,
                 services: &dyn akantu_model::constitutive_law::ModelServices| {
                    let mesh = Some(services.mesh());
                    akantu_core::with_dim!(spec.spatial_dimension, D => {
                        Box::new($ty::<D>::new(spec, mesh)?) as Box<dyn akantu_solid::Material>
                    })
                },
            ),
        )
    };
    ($factory:expr, $keyword:literal, $ty:ident, $options:expr) => {{
        let options: $crate::options::ExtraLawOptions = $options;
        $factory.register(
            $keyword,
            std::sync::Arc::new(
                move |spec: &akantu_model::constitutive_law::LawSpec,
                      services: &dyn akantu_model::constitutive_law::ModelServices| {
                    let mesh = Some(services.mesh());
                    akantu_core::with_dim!(spec.spatial_dimension, D => {
                        let mut law = $ty::<D>::new(spec, mesh)?;
                        $crate::options::HasExtraOptions::set_extra_options(&mut law, options);
                        Box::new(law) as Box<dyn akantu_solid::Material>
                    })
                },
            ),
        )
    }};
}
pub(crate) use register_law;
