// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/common/aka_common.hh/.cc (initialize,
// finalize, getStaticParser, getStaticArgumentParser, readInputFile),
// src/common/aka_extern.cc (global statics).

//! # akantu
//!
//! The facade of the akantu-rs workspace, the crate applications depend on.
//! It re-exports every subsystem as a module (`akantu::core`,
//! `akantu::mesh`, `akantu::solid`, …) and adds the process-global
//! [`runtime`] (upstream `akantu::initialize`, static parser, static
//! communicator, from `src/common/aka_common.cc`). Its Cargo features are
//! named after upstream's CMake packages. On by default: `solid`,
//! `cohesive`, `contact`, `diffusion`, `phase-field`, `couplers`,
//! `structural` and `embedded`. Off by default: `ntn-contact`,
//! `extra-materials`, `extra-materials-non-local`, `igfem` (implies
//! `cgal`), `cgal` (GPL-3.0-or-later: a build with it is distributed under
//! GPL-3.0-or-later as a whole), `mpi`, `scotch` and `python` (the example
//! that embeds CPython). The user manual (`docs/manual`) documents every
//! module; its Rust examples are doc-tests of this crate.
//!
//! ```
//! use akantu::core::ElementType;
//! assert_eq!(ElementType::Triangle3.nb_nodes_per_element(), 3);
//! ```

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub use akantu_comm as comm;
pub use akantu_core as core;
pub use akantu_fe as fe;
pub use akantu_geometry as geometry;
pub use akantu_io as io;
pub use akantu_linalg as linalg;
pub use akantu_mesh as mesh;
pub use akantu_model as model;
pub use akantu_parser as parser;
pub use akantu_solver as solver;

#[cfg(feature = "cohesive")]
pub use akantu_cohesive as cohesive;
#[cfg(feature = "contact")]
pub use akantu_contact as contact;
#[cfg(feature = "couplers")]
pub use akantu_couplers as couplers;
#[cfg(feature = "diffusion")]
pub use akantu_diffusion as diffusion;
#[cfg(feature = "embedded")]
pub use akantu_embedded as embedded;
#[cfg(feature = "extra-materials")]
pub use akantu_extra_materials as extra_materials;
#[cfg(feature = "cgal")]
/// GPL-3.0-or-later geometry (feature `cgal`, D-042): CGAL AABB tree and
/// spherical kernel; a build with this feature is GPL-3.0-or-later.
pub use akantu_geometry_cgal as geometry_cgal;
#[cfg(feature = "igfem")]
pub use akantu_igfem as igfem;
#[cfg(feature = "ntn-contact")]
pub use akantu_ntn_contact as ntn_contact;
#[cfg(feature = "phase-field")]
pub use akantu_phase_field as phase_field;
#[cfg(feature = "solid")]
pub use akantu_solid as solid;
#[cfg(feature = "structural")]
pub use akantu_structural as structural;

pub mod runtime;

#[cfg(doctest)]
mod manual;

/// Version of the port (follows the workspace version).
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Upstream version this port tracks (`git describe` of the reference).
pub const UPSTREAM_VERSION: &str = "v5.0.7-post1-554-g35a851780";

/// Commonly used items.
pub mod prelude {
    pub use akantu_core::prelude::*;
    pub use akantu_mesh::{Mesh, SharedMesh};
    pub use akantu_model::{Model, ModelOptions};
}
