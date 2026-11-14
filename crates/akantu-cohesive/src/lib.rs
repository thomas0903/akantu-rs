// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/model/solid_mechanics/solid_mechanics_model_cohesive/*, src/mesh_utils/cohesive_element_inserter*.

//! # akantu-cohesive
//!
//! `SolidMechanicsModelCohesive` (intrinsic and extrinsic), cohesive
//! materials (`cohesive_linear`, `cohesive_bilinear`, `cohesive_exponential`,
//! `cohesive_linear_fatigue`, `cohesive_linear_friction`,
//! `cohesive_linear_uncoupled`), the cohesive element inserter and the
//! fragment manager, ported from upstream's
//! `src/model/solid_mechanics/solid_mechanics_model_cohesive` and
//! `src/mesh_utils/cohesive_element_inserter*`. Cargo features, off by
//! default: `mpi` and `scotch` (test builds under real MPI and with Scotch).
//!
//! Mesh mutation (D-019). Extrinsic
//! insertion is one `SharedMesh::mutate` transaction driven by the model (or
//! the coupler / Python simulation owning every consumer of the mesh): the
//! inserter runs inside the topology stage on a journaled
//! `MeshTransaction` (doubled facet nodes, reconnected elements, cohesive
//! elements in the mesh and facet mesh, rolled back on failure), and the
//! domain then dispatches facet events, `ElementsAdded` (the cohesive
//! material is chosen by the model's selector) and `NodesAdded` (with
//! `old_nodes`) to every registered consumer in priority order. On a
//! distributed mesh, insertion decisions on shared facets are agreed with
//! the facet synchronizer before the transaction (upstream
//! `cohesive_element_inserter_parallel.cc`) and the distributed hook
//! (upstream `CohesiveMeshGlobalDataUpdater`) updates node flags,
//! synchronizers and global ids.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod dump;
pub mod fragment;
pub mod inserter;
pub mod laws;
pub mod material;
pub mod model;
pub mod par_scenarios;
pub mod parallel;
pub mod selector;
pub mod vecops;

pub use dump::{COHESIVE_DUMPER, FACETS_DUMPER};
pub use fragment::FragmentManager;
pub use inserter::{CohesiveElementInserter, InsertionResult};
pub use material::{CohesiveUpstreamCompat, MaterialCohesive, register_cohesive_materials};
pub use model::{CohesiveExtension, InsertionCheck, SolidMechanicsModelCohesive};
