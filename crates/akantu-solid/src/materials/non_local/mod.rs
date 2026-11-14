// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/model/solid_mechanics/materials/
// material_non_local.hh (MaterialNonLocalInterface, MaterialNonLocal),
// material_damage/material_damage_non_local.hh (MaterialDamageNonLocal),
// src/model/common/constitutive_laws/constitutive_law_non_local_interface
// {.hh,_tmpl.hh} (ConstitutiveLawNonLocalInterface).

//! Non-local solid laws (package `damage_non_local`, crate feature
//! `damage-non-local`, default on): MAT-012, MAT-014, MAT-016, MAT-027,
//! CL-007, and the damage-dependent weight functions NL-005…008.
//!
//! A non-local law implements [`MaterialNonLocal`] (upstream
//! `MaterialNonLocalInterface` + `ConstitutiveLawNonLocalInterface`) and
//! exposes it through [`crate::material::Material::as_non_local`]. The solid
//! model creates its `NonLocalManager` when at least one law is non-local,
//! inserts the laws' integration points (every element of the law's filter,
//! every point; `global_num = element·nb_quad + q`), averages the
//! registered variables and calls
//! [`MaterialNonLocal::compute_non_local_stresses`] after the local stress
//! pass (`SolidMechanicsModel::assembleInternalForces`).
//!
//! | Keyword | Type | Averaged variable |
//! |---|---|---|
//! | `mazars_non_local` | [`MaterialMazarsNonLocal`] on `elastic` | `Ehat` (default) or `damage` (`average_on_damage = true`) → `non_local_variable` |
//! | `plastic_mazars_non_local` | [`MaterialMazarsNonLocal`] on `plastic_linear_isotropic_hardening` | as above |
//! | `marigo_non_local` | [`MaterialMarigoNonLocal`] (UD-027 corrected default) | `Y` → `Y non local` |
//!
//! **UD-027** (category B): upstream registers the *local* `MaterialMarigo`
//! under `marigo_non_local`, so its non-local class is unreachable from
//! input files and the `non_local` section is ignored. The default here
//! registers the real non-local law; [`register_upstream_marigo_non_local`]
//! re-registers the keyword with upstream's local law (compat switch for
//! oracle replays).

pub mod marigo_non_local;
pub mod mazars_non_local;
pub mod weight_functions;

use std::sync::Arc;

use akantu_core::{GhostType, Result};
use akantu_model::constitutive_law::{LawContext, LawReducer, LawSpec, ModelServices};
use akantu_model::non_local::NonLocalVariable;

pub use marigo_non_local::MaterialMarigoNonLocal;
pub use mazars_non_local::{
    MaterialMazarsNonLocal, MaterialMazarsNonLocalElastic, MaterialVonMisesMazarsNonLocal,
};
pub use weight_functions::{
    DamagedWeightFunction, RemoveDamagedWeightFunction, RemoveDamagedWithDamageRateWeightFunction,
    StressBasedWeightFunction, register_weight_functions,
};

use crate::material::{Material, MaterialFactory};

/// The non-local part of a solid law (upstream
/// `MaterialNonLocalInterface`: `ConstitutiveLawNonLocalInterface` +
/// `computeNonLocalStresses`).
pub trait MaterialNonLocal {
    /// Upstream `getNeighborhoodName()`: the law's `name` by default.
    fn neighborhood_name(&self) -> String;

    /// Id of the `non_local <id> <type> [ … ]` section of the neighbourhood
    /// (upstream `registerNeighborhood()` registers `(name, name)`).
    fn weight_function_id(&self) -> String {
        self.neighborhood_name()
    }

    /// Upstream `registerNonLocalVariables()`: the `(local, non-local,
    /// components)` variables averaged in the law's neighbourhood.
    fn non_local_variables(&self) -> Vec<NonLocalVariable>;

    /// Upstream `computeNonLocalStresses(ghost_type)`, after the averages
    /// were copied into the law's non-local internals.
    ///
    /// # Errors
    /// Law errors.
    fn compute_non_local_stresses(&mut self, ctx: &LawContext<'_>, ghost: GhostType) -> Result<()>;

    /// The rank reductions the law **declares** for the end of its
    /// non-local stress pass (upstream: a raw `comm.allReduce` at the end of
    /// `computeNonLocalStresses`, e.g. `MaterialDamageIterativeNonLocal`,
    /// `material_damage_iterative_non_local.cc:27-41`), issued through the
    /// model's reducer bound to the running operation's scope, right after
    /// the non-local pass of every law (see
    /// [`akantu_model::constitutive_law::LawReducer`]). Default: none.
    ///
    /// # Errors
    /// Reducer errors (the non-local pass failed on some rank, transport).
    fn reduce_after_non_local_stresses(
        &mut self,
        _ghost: GhostType,
        _reducer: &mut dyn LawReducer,
    ) -> Result<()> {
        Ok(())
    }
}

/// Register `mazars_non_local`, `plastic_mazars_non_local` and
/// `marigo_non_local` (the real non-local law, UD-027 corrected default).
///
/// # Errors
/// Factory errors (duplicate keyword).
pub fn register(factory: &mut MaterialFactory) -> Result<()> {
    factory.register(
        "mazars_non_local",
        Arc::new(|spec: &LawSpec, services: &dyn ModelServices| {
            let mesh = Some(services.mesh());
            akantu_core::with_dim!(spec.spatial_dimension, D => {
                Box::new(MaterialMazarsNonLocalElastic::<D>::new(spec, mesh)?) as Box<dyn Material>
            })
        }),
    )?;
    factory.register(
        "plastic_mazars_non_local",
        Arc::new(|spec: &LawSpec, services: &dyn ModelServices| {
            let mesh = Some(services.mesh());
            akantu_core::with_dim!(spec.spatial_dimension, D => {
                Box::new(MaterialVonMisesMazarsNonLocal::<D>::new(spec, mesh)?) as Box<dyn Material>
            })
        }),
    )?;
    factory.register(
        "marigo_non_local",
        Arc::new(|spec: &LawSpec, services: &dyn ModelServices| {
            let mesh = Some(services.mesh());
            akantu_core::with_dim!(spec.spatial_dimension, D => {
                Box::new(MaterialMarigoNonLocal::<D>::new(spec, mesh)?) as Box<dyn Material>
            })
        }),
    )
}

/// UD-027 compat: make `marigo_non_local` allocate upstream's **local**
/// `MaterialMarigo` (law type `marigo`), as the reference does
/// (`material_marigo_non_local.cc:83-84`); a model whose only non-local
/// keyword is `marigo_non_local` then has no non-local manager, as
/// upstream. Replaces the keyword's registration in `factory`.
pub fn register_upstream_marigo_non_local(factory: &mut MaterialFactory) {
    factory.replace(
        "marigo_non_local",
        Arc::new(|spec: &LawSpec, services: &dyn ModelServices| {
            let mesh = Some(services.mesh());
            akantu_core::with_dim!(spec.spatial_dimension, D => {
                Box::new(crate::materials::marigo::MaterialMarigo::<D>::new(spec, mesh)?) as Box<dyn Material>
            })
        }),
    );
}

/// Forward the `Material` methods that the local damage laws override to
/// `self.$inner` (the non-local wrappers keep every local behaviour).
macro_rules! forward_local_material {
    ($inner:ident) => {
        fn base(&self) -> &$crate::material::MaterialBase {
            self.$inner.base()
        }
        fn base_mut(&mut self) -> &mut $crate::material::MaterialBase {
            self.$inner.base_mut()
        }
        fn compute_tangent_moduli(
            &mut self,
            ctx: &::akantu_model::constitutive_law::LawContext<'_>,
            ty: ::akantu_core::ElementType,
            ghost: ::akantu_core::GhostType,
            tangent: &mut ::akantu_core::Array<::akantu_core::Real>,
        ) -> ::akantu_core::Result<()> {
            self.$inner.compute_tangent_moduli(ctx, ty, ghost, tangent)
        }
        fn compute_potential_energy(
            &mut self,
            ctx: &::akantu_model::constitutive_law::LawContext<'_>,
            ty: ::akantu_core::ElementType,
        ) -> ::akantu_core::Result<()> {
            self.$inner.compute_potential_energy(ctx, ty)
        }
        fn energy_of(
            &mut self,
            ctx: &::akantu_model::constitutive_law::LawContext<'_>,
            energy_id: &str,
        ) -> ::akantu_core::Result<::akantu_core::Real> {
            self.$inner.energy_of(ctx, energy_id)
        }
        fn potential_energy_by_element(
            &self,
            ctx: &::akantu_model::constitutive_law::LawContext<'_>,
            element: ::akantu_core::Element,
        ) -> ::akantu_core::Result<Vec<::akantu_core::Real>> {
            self.$inner.potential_energy_by_element(ctx, element)
        }
        fn update_energies(
            &mut self,
            ctx: &::akantu_model::constitutive_law::LawContext<'_>,
            ty: ::akantu_core::ElementType,
        ) -> ::akantu_core::Result<()> {
            self.$inner.update_energies(ctx, ty)
        }
        fn push_wave_speed(
            &self,
            element: ::akantu_core::Element,
        ) -> ::akantu_core::Result<::akantu_core::Real> {
            self.$inner.push_wave_speed(element)
        }
        fn shear_wave_speed(
            &self,
            element: ::akantu_core::Element,
        ) -> ::akantu_core::Result<::akantu_core::Real> {
            self.$inner.shear_wave_speed(element)
        }
        fn tangent_type(&self) -> ::akantu_core::enums::MatrixType {
            self.$inner.tangent_type()
        }
        fn has_stiffness_matrix_changed(&self) -> bool {
            self.$inner.has_stiffness_matrix_changed()
        }
        fn options(&self) -> Option<&$crate::material::LawOptions> {
            self.$inner.options()
        }
        fn options_mut(&mut self) -> Option<&mut $crate::material::LawOptions> {
            self.$inner.options_mut()
        }
        fn compute_all_cauchy_stresses(
            &mut self,
            ghost: ::akantu_core::GhostType,
        ) -> ::akantu_core::Result<()> {
            self.$inner.compute_all_cauchy_stresses(ghost)
        }
        // upstream: the non-local law derives from the local one and
        // inherits its getNbData/packData/unpackData (Marigo `_clh_init_cl`)
        fn exchanged_internals(&self, tag: ::akantu_core::enums::SynchronizationTag) -> Vec<&str> {
            self.$inner.exchanged_internals(tag)
        }
    };
}
pub(crate) use forward_local_material;
