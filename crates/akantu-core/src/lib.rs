// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/common/aka_common.hh, aka_config.hh.in,
// aka_element_classes_info.hh, aka_array.hh, aka_types.hh, aka_bbox.hh,
// aka_csr.hh, aka_grid_dynamic.hh, aka_circular_array.hh,
// src/mesh/element_type_map.hh, src/fe_engine/element.hh and the element
// property tables in src/fe_engine/{element_class*.hh,
// geometrical_element_property.hh, interpolation_element_tmpl.hh,
// cohesive_element.hh, element_class_structural.hh} and
// extra_packages/igfem/src/*.

//! # akantu-core
//!
//! The building blocks every other crate of the workspace uses: index and
//! scalar types, errors, element types, arrays, small tensors, enums,
//! random numbers and the version. It ports upstream's `src/common` and the
//! element tables of `src/fe_engine`. It depends on no other akantu crate,
//! and its definitions are the interface contract of the whole port
//! (`docs/ARCHITECTURE.md`): no other crate re-defines them. Cargo feature:
//! `export-all`, a test-only mode in which every shared array behaves as if
//! exported to a foreign runtime (never enabled by a product crate).
//!
//! | Module | Upstream counterpart |
//! |---|---|
//! | [`types`] | `Real`, `Int`, `UInt`, `Idx` in `aka_config.hh.in` |
//! | [`error`] | `aka_error.hh` (`AKANTU_EXCEPTION`, `debug::Exception`) |
//! | [`debug`] | `aka_error.{hh,cc}` debug output (`DebugLevel`, `debug::Debugger`, `AKANTU_DEBUG_*`, `printBacktrace`) |
//! | [`element_type`] | `aka_element_classes_info.hh`, element property tables |
//! | [`element`] | `fe_engine/element.hh` (`Element`, `ElementNull`) |
//! | [`node_flag`] | `NodeFlag` in `aka_common.hh` |
//! | [`array`](mod@array) | `aka_array.hh`, `aka_array_view.hh`, `make_view` |
//! | [`array_print`] | `Array<T>::printself` (`aka_array_tmpl.hh`: the text golden files compare) |
//! | [`array_shared`], [`array_storage`] | exportable arrays: NumPy/C view lifetime and access protocol (no upstream counterpart; replaces `py_aka_array.hh` raw pointers) |
//! | [`element_type_map`] | `mesh/element_type_map.hh` |
//! | [`element_type_map_shared`] | `ElementTypeMapArray<T>` with exportable blocks (the per-type fields foreign code may view) |
//! | [`tensor`] | `aka_types.hh` (Eigen `Vector`/`Matrix`/`Map`, column-major) |
//! | [`eigen_order`] | evaluation order of Eigen 3.4 reductions/products (bit parity) |
//! | [`eigen_gebp`] | Eigen 3.4 product routes, `gebp`/GEMV kernel order (MPL-2.0 translation) |
//! | [`eigen_unrolled`] | straight-line versions of the [`eigen_order`] reductions for hot loops (same bits) |
//! | [`voigt`] | `aka_voigthelper.hh` (`VoigtHelper<dim>`) |
//! | [`enums`] | solver/model enums of `aka_common.hh` |
//! | [`dim`] | `tuple_dispatch<AllSpatialDimensions>` |
//! | [`math`] | `aka_math.hh` (`Math::tolerance`, `are_float_equal`, `reduce`, `pow`, `NewtonRaphson`) |
//! | [`random`] | `aka_random_generator.hh` (bit-compatible with libstdc++/glibc, D-006) |
//! | [`glibc_libm`] | bit-exact ports of the oracle's glibc 2.41 `log`, `exp`, `pow`, `tan`, `sin`, `cos` (upstream calls `<cmath>`) |
//! | [`bbox`] | `aka_bbox.hh` (`BBox`) |
//! | [`csr`] | `aka_csr.hh` (`CSR<T>`, `DataCSR<T>`) |
//! | [`spatial_grid`] | `aka_grid_dynamic.hh` (`SpatialGrid<T>`) |
//! | [`circular_array`] | `aka_circular_array.hh` (`CircularArray<T>`) |
//! | [`strings`] | `aka_common_inline_impl.hh` (`to_lower`, `trim`, `printMemorySize`) |
//! | [`exec`] | none: precision mode, threads and SIMD level of the kernels (D-123) |
//! | [`version`] | `aka_config.hh.in` / `aka_config.cc.in` (`AKANTU_VERSION_*`, `getVersion`) |

// `unsafe` is denied everywhere except the thread pool of `exec::spin` and
// the SIMD kernels of `exec::simd` (each site listed in UNSAFE.md).
#![deny(unsafe_code)]
#![warn(missing_docs)]

pub mod array;
pub mod array_print;
pub mod array_shared;
pub mod array_storage;
pub mod bbox;
pub mod circular_array;
pub mod csr;
pub mod debug;
pub mod dim;
pub mod eigen_gebp;
pub mod eigen_order;
pub mod eigen_unrolled;
pub mod element;
pub mod element_type;
pub mod element_type_map;
pub mod element_type_map_shared;
pub mod enums;
pub mod error;
pub mod exec;
pub mod glibc_libm;
pub mod math;
pub mod node_flag;
pub mod random;
pub mod spatial_grid;
pub mod strings;
pub mod tensor;
pub mod types;
pub mod version;
pub mod voigt;

pub use array::Array;
pub use array_shared::{ForeignRuntime, SharedArray, SliceRef};
pub use bbox::BBox;
pub use csr::Csr;
pub use element::Element;
pub use element_type::{
    ElementKind, ElementType, GaussIntegrationType, GeometricalShapeType, GeometricalType,
    GhostType, InterpolationKind, InterpolationType,
};
pub use element_type_map::{ElementTypeMap, ElementTypeMapArray, TypeFilter};
pub use element_type_map_shared::SharedElementTypeMapArray;
pub use error::{Error, FatalKind, Result};
pub use node_flag::NodeFlag;
pub use spatial_grid::SpatialGrid;
pub use types::*;

/// Commonly used items, `use akantu_core::prelude::*;`.
pub mod prelude {
    pub use crate::array::Array;
    pub use crate::element::Element;
    pub use crate::element_type::{ElementKind, ElementType, GhostType};
    pub use crate::element_type_map::{ElementTypeMap, ElementTypeMapArray, TypeFilter};
    pub use crate::error::{Error, Result};
    pub use crate::node_flag::NodeFlag;
    pub use crate::types::*;
}
