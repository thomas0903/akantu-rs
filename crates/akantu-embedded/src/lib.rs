// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/model/solid_mechanics/solid_mechanics_model_embedded_interface/*, src/model/solid_mechanics/materials/material_embedded/*.

//! # akantu-embedded
//!
//! The embedded-interface model (parity rows EMB-001…005): 1-D
//! reinforcement bars embedded in a solid matrix without conforming meshes,
//! ported from upstream's
//! `src/model/solid_mechanics/solid_mechanics_model_embedded_interface` and
//! `materials/material_embedded`. The crate has no Cargo features.
//!
//! * [`EmbeddedInterfaceIntersector`] (EMB-002) cuts the reinforcement
//!   segments of a primitive mesh by the background mesh into the
//!   `_segment_2` elements of the interface mesh, each associated with the
//!   background element it lies in;
//! * [`MaterialReinforcement`] (`reinforcement elastic`, EMB-003) is the
//!   1-D elastic law of those segments, assembled on the background nodes
//!   (axial strain from the background gradient, directing cosines,
//!   cross-sectional area, uniform pre-stress);
//! * [`EmbeddedInterfaceModel`] (EMB-001) is the solid mechanics model of
//!   the background mesh holding both bulk and reinforcement laws, with the
//!   interface selector [`InterfaceMeshDataMaterialSelector`] (EMB-004) and
//!   the `reinforcement` dumper (EMB-005).
//!
//! Licensing (D-042). Upstream needs CGAL; this crate uses
//! the LGPL `akantu-geometry` segment intersector with its `LinearScan`
//! search structure and never depends on the GPL-3.0-or-later crate
//! `akantu-geometry-cgal` (guarded by `tests/license_guard.rs`), so the
//! `embedded` feature stays in the default, LGPL build.
//!
//! Verification. Upstream's embedded model does not compile at the
//! reference commit (UD-587). The port follows the source
//! intent; it is compared with a patched oracle variant that makes upstream
//! run without changing a formula (D-060, `oracle/patches/embedded`,
//! `tests/oracle_fixtures.rs`) and validated by independent checks
//! (`docs/NUMERICAL_VALIDATION.md`, embedded interface model).

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod dump;
pub mod intersector;
pub mod material;
pub mod model;

pub use dump::{REINFORCEMENT_DUMPER, ReinforcementDumper};
pub use intersector::{ASSOCIATED_ELEMENT, EmbeddedInterfaceIntersector, PHYSICAL_NAMES};
pub use material::{
    INTERFACE_FE_ENGINE, InterfaceData, InterfaceSlot, MaterialReinforcement, is_reinforcement,
};
pub use model::{
    DEFAULT_ID, EmbeddedInterfaceModel, EmbeddedInterfaceModelOptions, EmbeddedUpstreamCompat,
    InterfaceMeshDataMaterialSelector,
};
