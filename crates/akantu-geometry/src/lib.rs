// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/geometry/*, src/common/aka_grid_dynamic.hh, src/common/aka_bbox.hh.

//! # akantu-geometry
//!
//! Geometric queries on meshes (upstream `src/geometry`, package `CGAL`;
//! parity rows GEOM-001…009) and the dynamic spatial grid
//! (`SpatialGrid`, upstream `aka_grid_dynamic.hh`) used by contact detection
//! and non-local neighbourhoods.
//!
//! Licensing (D-042). This crate is LGPL-3.0-or-later. It contains the
//! Akantu-derived code and ports of CGAL's *LGPL* packages (Kernel_23,
//! Cartesian_kernel, Intersections_3, Number_types): the inexact
//! `Simple_cartesian<double>` kernel ([`kernel`], [`intersections`]), the
//! exact number types of upstream's spherical kernel ([`number`]), the
//! mesh geometry factory ([`factory`]), the segment intersector
//! ([`segment_intersector`]) over a pluggable search structure ([`index`])
//! with the port's interval tiling of the pieces of a query ([`tiling`],
//! UD-599/UD-601)
//! and clean-room models of the libstdc++ algorithms whose unspecified
//! behaviour upstream observes ([`cxx`], D-030). CGAL's *GPL* packages
//! (AABB tree, spherical/circular kernel) and everything built on them
//! (bit-identical AABB traversal order, the sphere intersector) live in the
//! opt-in crate `akantu-geometry-cgal` (GPL-3.0-or-later, umbrella feature
//! `cgal`); this crate never depends on it. The crate has no Cargo
//! features.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod cxx;
pub mod factory;
pub mod helpers;
pub mod index;
pub mod intersection;
pub mod intersections;
pub mod kernel;
pub mod number;
pub mod segment_intersector;
pub mod tiling;

pub use factory::{MeshGeomFactory, TetrahedronPrimitives, TrianglePrimitive};
pub use index::{LinearScan, TriangleIndex};
pub use segment_intersector::{
    MeshSegmentIntersector, PieceAssembly, SegmentIntersectorOptions, SegmentOrder,
};
pub use tiling::TilingReport;

use akantu_core::{ElementType, Error, GhostType, Result};
use akantu_mesh::{Mesh, MeshAccessor};

/// Dynamic spatial grid (upstream `SpatialGrid<T>`): implemented in
/// [`akantu_core::spatial_grid`] (also needed by `akantu-mesh` and
/// `akantu-model`), re-exported here.
pub use akantu_core::spatial_grid::{CellId, GridMeshArrays, SpatialGrid};

/// Upstream `SpatialGrid::saveAsMesh(mesh)`: append one segment /
/// quadrangle / hexahedron per non-empty cell (cell order, `2^d` new nodes
/// each, no node sharing) to `mesh` and the running cell number to the
/// `Int` mesh data `"tag_1"` of that element type (created with one value
/// per existing element, `0`, as upstream `getDataPointer`).
///
/// # Errors
/// [`Error::DimensionMismatch`] if the mesh and grid dimensions differ;
/// the errors of [`SpatialGrid::mesh_arrays`]; a `"tag_1"` dataset of
/// another type. The mesh is unchanged on error (all checks precede the
/// first modification).
pub fn save_grid_as_mesh<T>(grid: &SpatialGrid<T>, mesh: &mut Mesh) -> Result<()> {
    let dim = grid.dimension();
    if mesh.spatial_dimension() != dim {
        return Err(Error::DimensionMismatch {
            context: "save_grid_as_mesh",
            expected: mesh.spatial_dimension(),
            got: dim,
        });
    }
    let arrays = grid.mesh_arrays(mesh.nb_nodes())?;
    let ty: ElementType = arrays.element_type;
    let ghost = GhostType::NotGhost;
    let nb_before = mesh.connectivity(ty, ghost).map_or(0, |c| c.len());
    if mesh.data().has_elemental("tag_1") {
        // type check before any modification
        mesh.data().ints("tag_1")?;
    }
    let mut acc = MeshAccessor::new(mesh);
    for x in arrays.nodes.chunks_exact(dim) {
        acc.push_node(x)?;
    }
    let nnodes = ty.nb_nodes_per_element();
    {
        let mut conn = acc.connectivity_mut(ty, ghost);
        for e in arrays.connectivity.chunks_exact(nnodes) {
            conn.try_push_tuple(e)?;
        }
    }
    let tags = acc.mesh_mut().data_mut().ints_mut_or_insert("tag_1")?;
    let mut block = tags.alloc(nb_before, 1, ty, ghost, 0)?;
    for &t in &arrays.tags {
        block.try_push_tuple(&[t])?;
    }
    Ok(())
}
