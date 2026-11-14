// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/model/structural_mechanics/* (the
// structural element kernels and their FE engine live in
// akantu-fe/src/structural).

//! # akantu-structural
//!
//! The structural mechanics model (upstream `StructuralMechanicsModel`,
//! ported from `src/model/structural_mechanics`; the element kernels are in
//! `akantu-fe`): Bernoulli beams (2-D / 3-D) and discrete Kirchhoff triangles
//! with displacement and rotation DOFs per node, structural materials,
//! stiffness / mass assembly with the element rotations, generalised
//! stresses and internal forces, static, implicit dynamic and explicit
//! analyses as upstream configures them, energies, line loads and dumps.
//! Upstream's package is off by default; the facade's feature `structural`
//! is on by default (no native dependency). The crate has no Cargo features.
//! The model runs on one process: it refuses a distributed mesh, because
//! upstream's distributed residual is wrong at interface nodes (UD-902).
//!
//! * [`StructuralMechanicsModel`]: the model ([`model`]);
//! * [`StructuralMaterial`]: material parameters ([`material`]);
//! * [`StructuralModelUpstreamCompat`]: switches for category-B defects (D-003) (default
//!   corrected, `UPSTREAM` for oracle replays), including the rotary
//!   inertia of the lumped beam mass ([`LumpedRotaryInertia`], D-059).

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod material;
pub mod model;

pub use material::StructuralMaterial;
pub use model::{
    DEFAULT_ID, DOF_ID, FE_ENGINE_NAME, LumpedRotaryInertia, ROTARY_BLOCK_DEFINITENESS,
    StructuralDumpers, StructuralMechanicsModel, StructuralMechanicsPhysics,
    StructuralModelUpstreamCompat,
};
