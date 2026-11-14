// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/model/solid_mechanics/materials/.

//! Built-in materials, one sub-module per upstream
//! file family:
//!
//! | Sub-module | Upstream directory / files |
//! |---|---|
//! | [`thermal`] | `material_thermal*` (thermal strain/stress pre-step of `elastic`) |
//! | [`elastic`] | `material_elastic*`, `plane_stress_toolbox*` |
//! | [`anisotropic`] | `material_elastic_linear_anisotropic*` |
//! | [`orthotropic`] | `material_elastic_orthotropic*` |
//! | [`damage`] | `material_damage/material_damage*` (`MaterialDamage` base: damage internals, tangent scaling, dissipated energy) |
//! | [`marigo`] | `material_damage/material_marigo*` |
//! | [`mazars`] | `material_damage/material_mazars*`, `material_von_mises_mazars*` (`mazars`, `plastic_mazars`) |
//! | [`plastic`] | `material_plastic/material_plastic*`, `material_linear_isotropic_hardening*` |
//! | [`drucker_prager`] | `material_plastic/material_drucker_prager*` |
//! | [`neohookean`] | `material_finite_deformation/material_neohookean*` |
//! | [`sls_deviatoric`] | `material_viscoelastic/material_standard_linear_solid_deviatoric*` |
//! | [`maxwell`] | `material_viscoelastic/material_viscoelastic_maxwell*` |
//! | [`anisotropic_damage`] | `material_damage/material_anisotropic_damage*` |
//! | [`phasefield`] | `material_damage/material_phasefield*`, `phase_field/energy_split*`, `energy_splits/*` |
//! | [`non_local`] | `material_non_local.hh`, `material_damage/material_*_non_local*`, `weight_functions/*` (`mazars_non_local`, `plastic_mazars_non_local`, `marigo_non_local`; `damage_wf`, `remove_wf`; keywords registered with the feature `damage-non-local`) |
//!
//! Implementations follow `docs/ARCHITECTURE.md`: a `struct
//! MaterialX<const DIM: usize>` with typed internal fields, a static
//! parameter table with upstream names, kernels written per quadrature point
//! with upstream operation order (decision D-008).

pub mod anisotropic;
pub mod elastic;
pub mod orthotropic;
pub mod thermal;
// --- local inelastic laws ---
pub mod damage;
pub mod drucker_prager;
pub mod marigo;
pub mod mazars;
pub mod neohookean;
pub mod plastic;
// --- end of the local inelastic laws ---
// Viscoelastic, anisotropic damage, phase field.
pub mod anisotropic_damage;
pub mod maxwell;
pub mod phasefield;
pub mod sls_deviatoric;
// Non-local laws and weight functions.
pub mod non_local;

/// One parameter descriptor of a `Real` member reached through `$($f).+`
/// (upstream `registerParam(name, member, default, access, description)`).
#[macro_export]
#[doc(hidden)]
macro_rules! real_param {
    ($S:ty, $name:literal, $access:expr, $desc:literal, $($f:ident).+) => {
        ::akantu_parser::param::ParamDescriptor::<$S>::new(
            $name,
            ::akantu_parser::param::ParamKind::Real,
            $access,
            $desc,
            |s| ::akantu_parser::param::ParamValue::Real(s.$($f).+),
            Some(|s, v| {
                s.$($f).+ = v.as_real()?;
                Ok(())
            }),
        )
    };
}

/// A read-only `Real` parameter (`_pat_readable`).
#[macro_export]
#[doc(hidden)]
macro_rules! readable_real_param {
    ($S:ty, $name:literal, $desc:literal, $($f:ident).+) => {
        ::akantu_parser::param::ParamDescriptor::<$S>::new(
            $name,
            ::akantu_parser::param::ParamKind::Real,
            ::akantu_parser::param::ParameterAccess::READABLE,
            $desc,
            |s| ::akantu_parser::param::ParamValue::Real(s.$($f).+),
            None,
        )
    };
}

// --- parameters inherited from `MaterialElastic` --------------------------

/// The parameter descriptors that a law deriving from `MaterialElastic<dim>`
/// inherits (upstream registration order: `PlaneStressToolbox` —
/// `Plane_Stress`, 2-D only —, `MaterialThermal` — `E`, `nu`, `alpha`,
/// `delta_T` —, `MaterialElastic` — `lambda`, `mu`, `kapa`), for a law type
/// `$S` whose embedded `MaterialElastic<DIM>` is reached through the field
/// path `$($p).+`, followed by the law's own descriptors `$extra`.
///
/// `elastic_parent_params!(Self, [elastic], [extra…])` expands to a
/// `&'static [ParamDescriptor<Self>]` **without** `Plane_Stress`;
/// `elastic_parent_params!(plane_stress Self, [elastic], [extra…])` puts
/// `Plane_Stress` first (2-D laws).
#[macro_export]
#[doc(hidden)]
macro_rules! elastic_parent_params {
    (plane_stress $S:ty, [$($p:ident).+], [$($extra:expr),* $(,)?]) => {
        $crate::elastic_parent_params!(@build $S, [$($p).+], [
            ::akantu_parser::param::ParamDescriptor::<$S>::new(
                "Plane_Stress",
                ::akantu_parser::param::ParamKind::Bool,
                ::akantu_parser::param::ParameterAccess::PARSMOD,
                "Is plane stress",
                |s| ::akantu_parser::param::ParamValue::Bool(s.$($p).+.plane_stress),
                Some(|s, v| {
                    s.$($p).+.plane_stress = v.as_bool()?;
                    Ok(())
                }),
            )
        ], [$($extra),*])
    };
    ($S:ty, [$($p:ident).+], [$($extra:expr),* $(,)?]) => {
        $crate::elastic_parent_params!(@build $S, [$($p).+], [], [$($extra),*])
    };
    (@build $S:ty, [$($p:ident).+], [$($first:expr),*], [$($extra:expr),*]) => {
        &[
            $($first,)*
            $crate::real_param!($S, "E", ::akantu_parser::param::ParameterAccess::PARSMOD, "Young's modulus", $($p).+.thermal.e),
            $crate::real_param!($S, "nu", ::akantu_parser::param::ParameterAccess::PARSMOD, "Poisson's ratio", $($p).+.thermal.nu),
            $crate::real_param!($S, "alpha", ::akantu_parser::param::ParameterAccess::PARSMOD, "Thermal expansion coefficient", $($p).+.thermal.alpha),
            ::akantu_parser::param::ParamDescriptor::<$S>::new(
                "delta_T",
                ::akantu_parser::param::ParamKind::Real,
                ::akantu_parser::param::ParameterAccess::PARSMOD,
                "Uniform temperature field",
                |s| ::akantu_parser::param::ParamValue::Real(s.$($p).+.delta_t.parameter_value()),
                Some(|s, v| {
                    s.$($p).+.delta_t.set_default_value(v.as_real()?);
                    Ok(())
                }),
            ),
            ::akantu_parser::param::ParamDescriptor::<$S>::new(
                "lambda",
                ::akantu_parser::param::ParamKind::Real,
                ::akantu_parser::param::ParameterAccess::READABLE,
                "First Lamé coefficient",
                |s| ::akantu_parser::param::ParamValue::Real(s.$($p).+.lambda()),
                None,
            ),
            ::akantu_parser::param::ParamDescriptor::<$S>::new(
                "mu",
                ::akantu_parser::param::ParamKind::Real,
                ::akantu_parser::param::ParameterAccess::READABLE,
                "Second Lamé coefficient",
                |s| ::akantu_parser::param::ParamValue::Real(s.$($p).+.mu()),
                None,
            ),
            ::akantu_parser::param::ParamDescriptor::<$S>::new(
                "kapa",
                ::akantu_parser::param::ParamKind::Real,
                ::akantu_parser::param::ParameterAccess::READABLE,
                "Bulk coefficient",
                |s| ::akantu_parser::param::ParamValue::Real(s.$($p).+.kappa()),
                None,
            ),
            $($extra,)*
        ]
    };
}

/// A parsable random-internal-field parameter (upstream
/// `registerParam(name, random_internal_field, access, description)`):
/// the value is a `RandomParameter` (a plain number is `base + uniform [0,
/// 0]`) handed to `setRandomDistribution` (redraws existing values); the
/// getter returns the base value (upstream `operator Real()`).
#[macro_export]
#[doc(hidden)]
macro_rules! random_field_param {
    ($S:ty, $name:literal, $access:expr, $desc:literal, $($f:ident).+) => {
        ::akantu_parser::param::ParamDescriptor::<$S>::new(
            $name,
            ::akantu_parser::param::ParamKind::Random,
            $access,
            $desc,
            |s| ::akantu_parser::param::ParamValue::Real(s.$($f).+.parameter_value()),
            Some(|s, v| {
                let p = match v {
                    ::akantu_parser::param::ParamValue::Random(r) => r.to_core(),
                    other => ::akantu_core::random::RandomParameter::from_parsed(
                        other.as_real()?,
                        ::akantu_core::random::RandomDistributionType::NotDefined,
                        &[],
                    ),
                };
                s.$($f).+.set_random_distribution(p);
                Ok(())
            }),
        )
    };
}

/// A `bool` parameter reached through `$($f).+`.
#[macro_export]
#[doc(hidden)]
macro_rules! bool_param {
    ($S:ty, $name:literal, $access:expr, $desc:literal, $($f:ident).+) => {
        ::akantu_parser::param::ParamDescriptor::<$S>::new(
            $name,
            ::akantu_parser::param::ParamKind::Bool,
            $access,
            $desc,
            |s| ::akantu_parser::param::ParamValue::Bool(s.$($f).+),
            Some(|s, v| {
                s.$($f).+ = v.as_bool()?;
                Ok(())
            }),
        )
    };
}
// --- end of the parameters inherited from `MaterialElastic` ---
