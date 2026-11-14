// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/mesh/*, src/mesh_utils/{mesh_utils*,
// mesh_partition*, global_ids_updater*}, src/synchronizer/{element_synchronizer,
// node_synchronizer, periodic_node_synchronizer, facet_synchronizer,
// *_info_per_processor}.

//! # akantu-mesh
//!
//! The mesh and everything that only needs topology (no shape functions),
//! ported from upstream's `src/mesh`, `src/mesh_utils` and the mesh
//! synchronizers of `src/synchronizer`. Cargo features: `mpi` (distributed
//! meshes under real MPI) and `scotch` (partitioning with Scotch; needs
//! `libscotch`). Contents:
//!
//! * [`Mesh`]: nodes, per-`(type, ghost)` connectivities, node flags and
//!   global ids, bounding boxes, facet mesh, periodicity, mesh data, groups.
//! * [`SharedMesh`]: the mesh domain (owner of mesh + facet mesh + consumer
//!   registry) shared by models, couplers, dumpers and Python (D-019).
//! * [`mutation`]: staged, collective, acknowledged topology changes
//!   (replaces upstream's `EventHandlerManager` with raw pointers).
//! * [`events`]: mesh-change events delivered by [`mutation`] in
//!   [`EventPriority`] order.
//! * [`group`], [`data`]: element/node groups and typed mesh data.
//! * [`accessor`]: raw mutation access for readers/builders (upstream
//!   `MeshAccessor`).
//! * [`utils`], [`distribution`], [`periodic`], [`synchronizers`]: topology
//!   algorithms, partitioning/distribution, periodicity, ghost schemes.
//! * [`removal`]: element/node removal with renumbering; [`clusters`]:
//!   connected components and boundary groups; [`iterators`]: element
//!   iteration helpers and filters.
//!
//! [`EventPriority`]: akantu_core::enums::EventPriority

#![cfg_attr(not(feature = "scotch"), forbid(unsafe_code))]
#![cfg_attr(feature = "scotch", deny(unsafe_code))]
#![warn(missing_docs)]

pub mod accessor;
pub mod clusters;
pub mod data;
pub mod distributed_removal;
pub mod distribution;
pub mod events;
pub mod exports;
pub mod facet_synchronizer;
pub mod global_data_updater;
pub mod global_ids_updater;
pub mod graphviz;
pub mod grid_synchronizer;
pub mod group;
pub mod iterators;
pub mod mesh;
pub mod mutation;
pub mod partition;
pub mod periodic;
pub mod periodic_synchronizer;
pub mod removal;
#[cfg(feature = "scotch")]
pub mod scotch;
#[cfg(feature = "scotch")]
pub mod scotch_reorder;
pub mod shared;
pub mod synchronizers;
pub mod type_map_filter;
pub mod type_map_init;
mod upstream_bbox;
pub mod utils;

pub use accessor::MeshAccessor;
pub use data::{MeshData, MeshDataTypeCode, MeshDataValues, NodalDataValues};
pub use events::{MeshEvent, MeshEventHandler, MeshEvents};
pub use exports::{ExportSlot, MeshExports};
pub use group::{ElementGroup, GroupManager, NodeGroup};
pub use mesh::{BBox, Mesh};
pub use mutation::{CommittedMutation, DistributedHook, MeshTransaction};
pub use shared::{ConsumerId, ConsumerToken, MeshPart, SharedMesh};
