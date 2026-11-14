// SPDX-License-Identifier: GPL-3.0-or-later
// akantu-rs — GPL-3.0-or-later (D-042). Ported from Akantu (EPFL/LSMS):
// src/geometry/* (the parts that use CGAL's GPL packages). GPL-derived
// modules: `aabb` (CGAL AABB_tree), `spherical` (CGAL Circular_kernel_3 and
// Algebraic_kernel_for_spheres); `sphere_intersector` (Akantu code built on
// `spherical`). See README.md for the licence consequences.

//! # akantu-geometry-cgal (GPL-3.0-or-later)
//!
//! The part of the geometry port (parity rows GEOM-001…009, upstream
//! `src/geometry`) that derives from
//! CGAL's GPL-3.0-or-later packages (decision record D-042):
//!
//! * [`aabb::AabbTree`]: CGAL's AABB tree over triangle primitives, with
//!   upstream's build and traversal order; implements
//!   [`akantu_geometry::TriangleIndex`], so
//!   `MeshSegmentIntersector<AabbTree>` with `SegmentOrder::Upstream` is
//!   bit-identical to upstream's `MeshSegmentIntersector`.
//! * [`spherical`]: the exact spherical kernel subset (line arcs, spheres,
//!   circular-arc points) on `Quotient<MP_Float>`.
//! * [`sphere_intersector::MeshSphereIntersector`]: upstream's IGFEM
//!   sphere–mesh intersector.
//!
//! Opt-in: enabled in the facade crate by the feature `cgal` (off by
//! default). Any build including this crate is GPL-3.0-or-later as a whole.
//! The crate itself has no Cargo features.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod aabb;
pub mod sphere_intersector;
pub mod spherical;

pub use aabb::AabbTree;
pub use sphere_intersector::{
    MeshSphereIntersector, NodeNumbering, ProximityTest, SphereIntersectorOptions, TangentPolicy,
};
pub use spherical::{CircularArcPoint, LineArc, SkPoint, SkSphere};

/// Upstream-compatible segment intersector: CGAL AABB-tree order plus the
/// upstream options (`SegmentIntersectorOptions::upstream()`).
pub type UpstreamSegmentIntersector = akantu_geometry::MeshSegmentIntersector<AabbTree>;
