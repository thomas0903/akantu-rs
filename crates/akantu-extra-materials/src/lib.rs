// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): extra_packages/extra-materials/package.cmake,
// src/material_extra_includes.hh, src/material_non_local_extra_includes.hh
// and the materials of extra_packages/extra-materials/src/*.

//! # akantu-extra-materials
//!
//! The materials of upstream's `extra-materials` package
//! (`extra_packages/extra-materials`, parity rows XMAT-000…015), registered into a solid [`MaterialFactory`] by
//! [`register_extra_materials`]. Cargo features: `damage-non-local` (the
//! non-local variants, upstream package `extra_materials_non_local`), and
//! `mpi` and `scotch` (test builds under real MPI and with Scotch), all off
//! by default.
//!
//! The package does not compile at the reference commit (decision D-007):
//! there is no oracle. The port follows the code (its
//! semantics are unchanged since v4.0.1, the last release in which it
//! built; later commits are renames to the Akantu 5 API) and its history,
//! and every ambiguity is an upstream-defect entry (UD-572…586,
//! UD-780…786) with a switch in [`ExtraLawOptions`] where a corrected
//! default differs from the code as written. The switches are chosen at
//! registration ([`register_extra_materials_with`]), so that they are in
//! effect when the input file is parsed.
//!
//! | Keyword | Module | Row |
//! |---|---|---|
//! | `damage_linear` | [`damage_linear`] | XMAT-004 |
//! | `brittle` | [`brittle`] | XMAT-005 |
//! | `damage_iterative` | [`damage_iterative`] | XMAT-001 |
//! | `iterative_stiffness_reduction` | [`iterative_stiffness_reduction`] | XMAT-003 |
//! | `vreepeerlings` (legacy list only; never registered upstream) | [`vreepeerlings`] | XMAT-007 |
//! | `orthotropic_damage_iterative` (base: XMAT-009) | [`orthotropic_damage`] | XMAT-010 |
//! | `damage_iterative_non_local`, `brittle_non_local`, `vreepeerlings_non_local`, `damage_orthotropic_iterative_non_local` (registered by `register_extra_non_local_materials`, crate feature `damage-non-local`; driven by the solid model's non-local manager) | [`non_local`] | XMAT-002/006/008/011 |
//! | `material_FE2` (registered by [`register_material_fe2`] with the input file of its RVEs) | [`fe2`] | XMAT-014 |
//! | `SolidMechanicsModelRVE`, `GelMaterialSelector` | [`rve`] | XMAT-015 |
//! | `ve_stiffness_prop` | [`stiffness_proportional`] | XMAT-013 |
//! | `visco_plastic` | [`viscoplastic`] | XMAT-012 |
//!
//! [`MaterialFactory`]: akantu_solid::MaterialFactory

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod brittle;
pub mod c_rand;
pub mod damage_iterative;
mod damage_law;
pub mod damage_linear;
pub mod fe2;
pub mod iterative_stiffness_reduction;
pub mod kernels;
pub mod non_local;
pub mod options;
pub mod orthotropic_damage;
pub mod rve;
pub mod stiffness_proportional;
pub mod viscoplastic;
pub mod vreepeerlings;

pub use damage_iterative::{IterativeDamage, MaterialDamageIterative};
pub use fe2::{Fe2Config, MaterialFE2, register_material_fe2};
pub use iterative_stiffness_reduction::MaterialIterativeStiffnessReduction;
#[cfg(feature = "damage-non-local")]
pub use non_local::{register_extra_non_local_materials, register_extra_non_local_materials_with};
pub use options::{ExtraLawOptions, HasExtraOptions};
pub use rve::{GelMaterialSelector, SolidMechanicsModelRve};

/// The [`IterativeDamage`] interface of `law` when it is one of the
/// sequentially linear damage laws of this crate (upstream
/// `dynamic_cast<MaterialDamageIterative<dim> &>(model.getMaterial(i))`).
pub fn as_iterative_damage_mut(
    law: &mut dyn akantu_solid::Material,
) -> Option<&mut dyn IterativeDamage> {
    macro_rules! try_as {
        ($any:ident, $($t:ty),+) => {
            $(
                if $any.is::<$t>() {
                    return $any.downcast_mut::<$t>().map(|m| m as &mut dyn IterativeDamage);
                }
            )+
        };
    }
    let any = law.as_any_mut()?;
    try_as!(
        any,
        MaterialDamageIterative<1>,
        MaterialDamageIterative<2>,
        MaterialDamageIterative<3>,
        MaterialIterativeStiffnessReduction<1>,
        MaterialIterativeStiffnessReduction<2>,
        MaterialIterativeStiffnessReduction<3>,
        orthotropic_damage::MaterialOrthotropicDamageIterative<1>,
        orthotropic_damage::MaterialOrthotropicDamageIterative<2>,
        orthotropic_damage::MaterialOrthotropicDamageIterative<3>,
        non_local::MaterialDamageIterativeNonLocal<1>,
        non_local::MaterialDamageIterativeNonLocal<2>,
        non_local::MaterialDamageIterativeNonLocal<3>,
        non_local::MaterialOrthotropicDamageIterativeNonLocal<1>,
        non_local::MaterialOrthotropicDamageIterativeNonLocal<2>,
        non_local::MaterialOrthotropicDamageIterativeNonLocal<3>
    );
    None
}

/// Read-only [`as_iterative_damage_mut`].
pub fn as_iterative_damage(law: &dyn akantu_solid::Material) -> Option<&dyn IterativeDamage> {
    macro_rules! try_as {
        ($any:ident, $($t:ty),+) => {
            $(
                if let Some(m) = $any.downcast_ref::<$t>() {
                    return Some(m as &dyn IterativeDamage);
                }
            )+
        };
    }
    let any = law.as_any()?;
    try_as!(
        any,
        MaterialDamageIterative<1>,
        MaterialDamageIterative<2>,
        MaterialDamageIterative<3>,
        MaterialIterativeStiffnessReduction<1>,
        MaterialIterativeStiffnessReduction<2>,
        MaterialIterativeStiffnessReduction<3>,
        orthotropic_damage::MaterialOrthotropicDamageIterative<1>,
        orthotropic_damage::MaterialOrthotropicDamageIterative<2>,
        orthotropic_damage::MaterialOrthotropicDamageIterative<3>,
        non_local::MaterialDamageIterativeNonLocal<1>,
        non_local::MaterialDamageIterativeNonLocal<2>,
        non_local::MaterialDamageIterativeNonLocal<3>,
        non_local::MaterialOrthotropicDamageIterativeNonLocal<1>,
        non_local::MaterialOrthotropicDamageIterativeNonLocal<2>,
        non_local::MaterialOrthotropicDamageIterativeNonLocal<3>
    );
    None
}

/// Implement [`HasExtraOptions`] for laws holding the switches in a field
/// (`$ty => field`) or delegating to an inner law (`$ty => inner.`).
macro_rules! has_extra_options {
    (field $($ty:ty),+ $(,)?) => {
        $(
            impl<const DIM: usize> HasExtraOptions for $ty {
                fn extra_options(&self) -> &ExtraLawOptions {
                    &self.extra_options
                }
                fn set_extra_options(&mut self, options: ExtraLawOptions) {
                    self.extra_options = options;
                }
            }
        )+
    };
    (delegate $($ty:ty => $inner:ident),+ $(,)?) => {
        $(
            impl<const DIM: usize> HasExtraOptions for $ty {
                fn extra_options(&self) -> &ExtraLawOptions {
                    self.$inner.extra_options()
                }
                fn set_extra_options(&mut self, options: ExtraLawOptions) {
                    self.$inner.set_extra_options(options);
                }
            }
        )+
    };
}

has_extra_options!(field
    damage_linear::MaterialDamageLinear<DIM>,
    brittle::MaterialBrittle<DIM>,
    MaterialDamageIterative<DIM>,
    orthotropic_damage::MaterialOrthotropicDamageIterative<DIM>,
    viscoplastic::MaterialViscoPlastic<DIM>,
    MaterialFE2<DIM>,
);

has_extra_options!(delegate
    MaterialIterativeStiffnessReduction<DIM> => parent,
    non_local::MaterialDamageIterativeNonLocal<DIM> => local,
    non_local::MaterialBrittleNonLocal<DIM> => local,
    non_local::MaterialVreePeerlingsNonLocal<DIM> => local,
    non_local::MaterialOrthotropicDamageIterativeNonLocal<DIM> => local,
);

/// The [`HasExtraOptions`] interface of `law` when it is one of the laws
/// of this crate that carry [`ExtraLawOptions`].
pub fn extra_options_mut(law: &mut dyn akantu_solid::Material) -> Option<&mut dyn HasExtraOptions> {
    macro_rules! try_as {
        ($any:ident, $($t:ident),+) => {
            $(
                if $any.is::<$t<1>>() {
                    return $any.downcast_mut::<$t<1>>().map(|m| m as &mut dyn HasExtraOptions);
                }
                if $any.is::<$t<2>>() {
                    return $any.downcast_mut::<$t<2>>().map(|m| m as &mut dyn HasExtraOptions);
                }
                if $any.is::<$t<3>>() {
                    return $any.downcast_mut::<$t<3>>().map(|m| m as &mut dyn HasExtraOptions);
                }
            )+
        };
    }
    use brittle::MaterialBrittle;
    use damage_linear::MaterialDamageLinear;
    use non_local::{
        MaterialBrittleNonLocal, MaterialDamageIterativeNonLocal,
        MaterialOrthotropicDamageIterativeNonLocal, MaterialVreePeerlingsNonLocal,
    };
    use orthotropic_damage::MaterialOrthotropicDamageIterative;
    use viscoplastic::MaterialViscoPlastic;
    use vreepeerlings::MaterialVreePeerlings;
    let any = law.as_any_mut()?;
    try_as!(
        any,
        MaterialDamageLinear,
        MaterialBrittle,
        MaterialDamageIterative,
        MaterialIterativeStiffnessReduction,
        MaterialVreePeerlings,
        MaterialOrthotropicDamageIterative,
        MaterialViscoPlastic,
        MaterialFE2,
        MaterialDamageIterativeNonLocal,
        MaterialBrittleNonLocal,
        MaterialVreePeerlingsNonLocal,
        MaterialOrthotropicDamageIterativeNonLocal
    );
    None
}

/// Register the extra materials (upstream `INSTANTIATE_MATERIAL` in the
/// extra package) with the corrected defaults
/// ([`ExtraLawOptions::default`]).
///
/// # Errors
/// A keyword already registered in `factory`.
pub fn register_extra_materials(
    factory: &mut akantu_solid::MaterialFactory,
) -> akantu_core::Result<()> {
    register_extra_materials_with(factory, ExtraLawOptions::default())
}

/// Register the extra materials with the upstream-defect switches
/// `options`: every law the factory creates carries them from its
/// construction on, i.e. before the input file is parsed (UD-578 changes
/// the binding of the key `Crate`).
///
/// # Errors
/// A keyword already registered in `factory`.
pub fn register_extra_materials_with(
    factory: &mut akantu_solid::MaterialFactory,
    options: ExtraLawOptions,
) -> akantu_core::Result<()> {
    damage_linear::register(factory, options)?;
    brittle::register(factory, options)?;
    damage_iterative::register(factory, options)?;
    iterative_stiffness_reduction::register(factory, options)?;
    vreepeerlings::register(factory, options)?;
    orthotropic_damage::register(factory, options)?;
    stiffness_proportional::register(factory)?;
    viscoplastic::register(factory, options)?;
    Ok(())
}
