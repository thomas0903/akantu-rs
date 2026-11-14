// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/fe_engine/* (element_class*.hh,
// element_classes/*, gauss_integration*, integrator*.hh, shape_*.hh,
// fe_engine*.hh, integration_point.hh, element_type_conversion.hh,
// element_class_structural.hh, shape_structural*, fe_engine_template_tmpl_struct.hh).

//! # akantu-fe
//!
//! Element classes, quadrature, shape functions and the FE engine, generic
//! over the element type (decision D-013), ported from upstream's
//! `src/fe_engine`; it also holds the structural element kernels
//! ([`structural`]). The crate has no Cargo features.
//!
//! Dispatch strategy. Each element type is a zero-sized type (ZST)
//! implementing [`element_class::ElementClass`] (constants taken from the
//! `akantu-core` tables) and, for its interpolation family, a kernel trait
//! ([`element_class::LagrangeElement`], …). Batch operations select the ZST
//! once per `(type, ghost)` block with [`with_element_class!`] and then run a
//! fully monomorphised, allocation-free loop over elements and quadrature
//! points, the Rust analogue of upstream's `tuple_dispatch` +
//! `ElementClass<type>` templates. Per-element virtual calls never occur.
//!
//! Data layout (identical to upstream, so fixtures compare entry by entry):
//! * shapes: `Array<Real>` with `nb_element * nb_quad` tuples of
//!   `nb_nodes_per_interpolation_element` values;
//! * shape derivatives: tuples of `spatial_dimension * nb_nodes` values, a
//!   column-major `dim × nb_nodes` matrix per quadrature point;
//! * jacobians (× weights): tuples of 1 value;
//! * quadrature-point fields: element-major, quadrature-point-minor.
//!
//! The FE engine never stores a mesh reference: every method takes `&Mesh`.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod eigen;
pub mod element_class;
pub mod engine;
pub mod geometry;
pub mod integration_point;
pub mod kernels;
pub mod lagrange;
pub mod quadrature;
pub mod shape;
pub mod structural;

pub use element_class::{ElementClass, LagrangeElement};
pub use engine::FeEngine;
pub use integration_point::IntegrationPoint;
pub use quadrature::{QuadratureRule, gauss_rule};
