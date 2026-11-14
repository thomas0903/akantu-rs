// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/model/solid_mechanics/{material.hh,
// material.cc, material_inline_impl.hh, material_selector*.hh,
// solid_mechanics_model.hh, solid_mechanics_model.cc,
// solid_mechanics_model_io.cc, solid_mechanics_model_mass.cc,
// solid_mechanics_model_event_handler.hh, materials/}.

//! # akantu-solid
//!
//! [`SolidMechanicsModel`] and the solid constitutive laws ([`Material`]),
//! ported from upstream's `src/model/solid_mechanics` (the model, the
//! materials and the material selectors). Cargo features: `damage-non-local`
//! (on by default: the non-local damage laws and their weight functions,
//! upstream package `damage_non_local`), and, off by default, `mpi`,
//! `scotch`, `mumps` (test builds under real MPI, with Scotch or with MUMPS)
//! and `petsc` (the PETSc DOF manager for the model; a build with it is
//! distributed under GPL-3.0-or-later as a whole, D-076).
//!
//! Materials are monomorphised on the spatial dimension
//! (`MaterialElastic<const DIM: usize>`), boxed behind `dyn Material`, and
//! created by name through [`material::MaterialFactory`]
//! (`register_builtin_materials`). Python-defined materials implement the same
//! trait through `akantu-python`.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod derived;
pub mod foreign;
pub mod material;
pub mod material_tensor;
pub mod materials;
pub mod model;
pub mod stress_interpolation;

pub use derived::{DerivedLaw, DerivedLawState, DerivedMaterial, DerivedMaterialMethods};
pub use foreign::{
    ForeignLaw, ForeignLawState, ForeignMaterial, ForeignMaterialMethods, RuntimeLawState,
};
pub use material::{Material, MaterialBase, MaterialFactory};
pub use model::{EventHandlerId, SmmUpstreamCompat, SolidMechanicsModel};
