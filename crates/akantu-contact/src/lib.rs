// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/model/contact_mechanics/* (model,
// contact_detector*, surface_selector*, geometry_utils*, resolution*,
// resolutions/*, contact_element.hh).

//! # akantu-contact
//!
//! The contact mechanics model (upstream `ContactMechanicsModel`, parity
//! rows CONT-001…018), ported from upstream's `src/model/contact_mechanics`.
//! The crate has no Cargo features. Contents:
//!
//! * [`selector`]: surface selectors (physical names, cohesive facets,
//!   both) and the owned `contact_surface`;
//! * [`detector`]: global (bounding boxes + the shared spatial grid
//!   `akantu_core::spatial_grid::SpatialGrid`, D-048) and local (closest
//!   master node) search, orthogonal projection, self-contact filter;
//! * [`geometry`]: natural projection (Newton), covariant bases, normals,
//!   metric tensors (upstream `GeometryUtils`);
//! * [`resolution`]: the resolution interface and factory, penalty linear
//!   and quadratic with Coulomb friction (forces, normal/stick/slip moduli);
//! * [`model`]: the model: search pipeline, nodal areas, force and
//!   stiffness assembly, solver callback, dump fields.
//!
//! Numerics follow upstream's Eigen evaluation order (`eigen_ops`); the
//! upstream defects UD-001, UD-002, UD-008, UD-362…369, UD-371 and UD-372
//! are handled as documented on the affected items (switches in
//! [`fields::ContactUpstreamCompat`]; `ContactUpstreamCompat::all()`
//! reproduces upstream wherever it is defined). Defaults: consistent normal
//! and friction tangents (UD-368, UD-369), normals oriented on the current
//! configuration (UD-371), triangle-aware projection validity (UD-372).
//! The model runs on one process: it refuses a distributed mesh, as upstream
//! has no distributed contact (approved exception CONT-019).
//!
//! Limits shared with upstream: only linear master facets (`_segment_2`,
//! `_triangle_3`, `_quadrangle_4`) are supported; quadratic facets make
//! the search return `Error::NotImplemented` (UD-373); the friction history
//! is saved by every [`ContactMechanicsModel::search`] (an implicit coupler
//! that searches inside its Newton loop redefines the friction residual at
//! every iteration, UD-374, a decision of the couplers). The selectors own
//! their `contact_surface` instead of adding a group to the mesh; the
//! observable differences are listed in [`selector`].
//!
//! The solid–contact couplers are in `akantu-couplers`; the interface
//! they need is listed in [`model`].

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod detector;
pub mod dump;
mod eigen_ops;
pub mod element;
pub mod fields;
pub mod geometry;
pub mod model;
pub mod resolution;
pub mod selector;

pub use detector::{ContactDetector, GridSpacing, SearchOutput};
pub use element::{ContactElement, ContactState, DetectionType, contact_state_code};
pub use fields::{ContactFields, ContactUpstreamCompat, DeformableMasterWeighting};
pub use geometry::{MeshView, NormalOrientation, ProjectionMetric, ProjectionValidity};
pub use model::ContactMechanicsModel;
pub use resolution::{
    PenaltyLaw, Resolution, ResolutionContext, ResolutionFactory, ResolutionPenalty,
};
pub use selector::{
    AllSurfaceSelector, CohesiveSurfaceSelector, ContactSurface, PhysicalSurfaceSelector,
    SurfaceSelector,
};
