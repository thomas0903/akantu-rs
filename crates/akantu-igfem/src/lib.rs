// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): extra_packages/igfem/src/*.

//! # akantu-igfem
//!
//! Interface-enriched generalized FEM (IGFEM, Soghrati–Aragón–Duarte), port
//! of upstream's `extra_packages/igfem`, which is no longer maintained
//! (parity rows IGFEM-000…010, ELEM-024…026, FE-038). Cargo feature: `cgal`
//! (off by default; see the licensing paragraph). Contents:
//!
//! * [`element`]: IGFEM element classes `_igfem_segment_3`,
//!   `_igfem_triangle_4`, `_igfem_triangle_5` (tables in `akantu-core`) and
//!   upstream's `IGFEMHelper` queries;
//! * [`engine::IgfemFeEngine`]: the IGFEM FE engine (`ShapeLagrange<_ek_igfem>`,
//!   `IntegratorGauss<_ek_igfem>`);
//! * [`enrichment`]: the interface geometry (`IGFEMEnrichment`; spheres with
//!   the feature `cgal`) and, with `cgal`, `growing_gel`: the spherical
//!   growing-gel mesh generation (`MeshIgfemSphericalGrowingGel`);
//!
//! The upstream package does not compile at the reference commit: there is
//! no C++ oracle, and the port is verified against the `.verified` outputs
//! of its last working version, upstream's test criteria and independent
//! checks (decision D-007, `docs/NUMERICAL_VALIDATION.md`, IGFEM).
//!
//! Licensing. This crate is LGPL-3.0-or-later. The sphere–mesh
//! intersection that the growing-gel mesh generation needs is a port of
//! CGAL's GPL spherical kernel and lives in the GPL crate
//! `akantu-geometry-cgal` (decision D-042); the parts of this crate that
//! use it are compiled only with this crate's feature `cgal` (enabled by
//! the facade feature `igfem`, which implies `cgal`; both OFF by default).

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod context;
pub mod dump;
pub mod element;
pub mod engine;
pub mod enrichment;
pub mod events;
#[cfg(feature = "cgal")]
pub mod growing_gel;
pub mod material;
pub mod material_damage;
pub mod model;
pub mod non_local;
pub mod transfer;

pub use engine::{IgfemBlock, IgfemFeEngine};
pub use model::SolidMechanicsModelIgfem;
