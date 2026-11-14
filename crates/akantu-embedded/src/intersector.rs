// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/model/solid_mechanics/
// solid_mechanics_model_embedded_interface/embedded_interface_intersector.hh,
// embedded_interface_intersector.cc.

//! [`EmbeddedInterfaceIntersector`] (EMB-002): cuts the reinforcement
//! segments of the primitive mesh by the background mesh into the
//! `_segment_2` elements of the **interface mesh**.
//!
//! Upstream algorithm (`constructData`), kept step by step:
//!
//! 1. the primitive mesh must carry the elemental string data
//!    `physical_names` on `_segment_2` (upstream: an `AKANTU_ERROR`
//!    otherwise); a 1-D model is refused;
//! 2. every `_segment_2` of the primitive mesh becomes a 3-D segment
//!    (`createSegment`: `z = 0` in 2-D) appended to the list of its physical
//!    name; the lists are visited in the byte-wise order of the names
//!    (upstream `std::map<std::string, std::list<Segment_3>>`), each in
//!    primitive-element order;
//! 3. for every background element type of the model dimension (not ghost,
//!    regular kind, enum order) a `MeshSegmentIntersector` is built on the
//!    elements of that type (`_triangle_3`, `_triangle_6`, `_tetrahedron_4`;
//!    any other type throws `std::range_error` upstream — the port returns
//!    [`Error::UnsupportedElementType`]); for every name, the name is set and
//!    the segment list is intersected (`buildResultFromQueryList`), which
//!    appends `_segment_2` elements (two new nodes each) with the element
//!    data `associated_element` (the background element) and
//!    `physical_names` (the name) to the interface mesh.
//!
//! **Pieces of a bar (UD-599, UD-601).** A reinforcement lying along a
//! background edge or face is reported by every element sharing it, and a
//! bar through a mesh vertex (3-D, in particular at a small angle to a face)
//! gets near-duplicate pieces whose end points are ill-conditioned, or loses
//! a piece when the inexact face predicates miss the vertex.
//! By default the pieces of every reinforcement segment are
//! assembled by parameter interval along it
//! ([`akantu_geometry::PieceAssembly::Tiled`], [`akantu_geometry::tiling`]):
//! they cover the part of the segment inside the background mesh exactly
//! once (duplicates and slivers dropped, overlaps clipped, a lost piece
//! rebuilt by clipping the segment against the element containing it), so
//! the reinforcement length is never silently wrong.
//! [`EmbeddedInterfaceIntersector::remove_near_duplicates`] `= false` keeps
//! upstream's collected set (exact deduplication only).
//!
//! **Licensing (D-042).** The search structure is the LGPL
//! [`LinearScan`] of `akantu-geometry`: this crate never depends on the
//! GPL-3.0-or-later crate `akantu-geometry-cgal`. With the default
//! [`SegmentIntersectorOptions`] (`SegmentOrder::Lexicographic`, UD-408) the
//! interface elements of one query come in exact lexicographic order with
//! duplicates removed independently of the search structure; upstream's
//! order (`SegmentOrder::Upstream`) is only bit-identical with the CGAL AABB
//! tree, which is available to GPL builds only. The interface-mesh physics
//! (stiffness, residual, energies) does not depend on the element order
//! beyond floating-point summation order at shared background nodes.

use std::collections::BTreeMap;

use akantu_core::{Element, ElementKind, ElementType, Error, GhostType, Result, TypeFilter};
use akantu_geometry::kernel::{Point3, Segment3};
use akantu_geometry::{
    LinearScan, MeshSegmentIntersector, PieceAssembly, SegmentIntersectorOptions,
};
use akantu_mesh::{Mesh, MeshAccessor};

/// Name of the interface-mesh element data holding the background element
/// of every interface segment.
pub const ASSOCIATED_ELEMENT: &str = "associated_element";

/// Name of the element data holding the reinforcement (material) names.
pub const PHYSICAL_NAMES: &str = "physical_names";

/// Background element types upstream's `tuple_dispatch` accepts.
pub const SUPPORTED_BACKGROUND_TYPES: [ElementType; 3] = [
    ElementType::Triangle3,
    ElementType::Triangle6,
    ElementType::Tetrahedron4,
];

/// Upstream `EmbeddedInterfaceIntersector` (a `MeshGeomAbstract` on the
/// background mesh): owns the options of the segment intersector; the
/// interface mesh is the value returned by
/// [`EmbeddedInterfaceIntersector::construct_data`].
#[derive(Clone, Debug)]
pub struct EmbeddedInterfaceIntersector {
    /// Options of the segment intersectors (default: the corrected, LGPL
    /// ordering of UD-408 and the tetrahedron faces of UD-407). Their
    /// `assembly` is set from [`Self::remove_near_duplicates`].
    pub options: SegmentIntersectorOptions,
    /// **UD-599 / UD-601**: assemble the pieces of every reinforcement
    /// segment by parameter interval along it
    /// ([`PieceAssembly::Tiled`]): duplicates and near-duplicates (pieces
    /// whose interval is covered by another one, up to the parameter
    /// tolerance derived from `options.point_tolerance`) are dropped,
    /// overlaps clipped, slivers dropped, and a piece missing inside the
    /// background mesh is rebuilt. A reinforcement lying along a background
    /// edge or face is reported by every element sharing it, and the
    /// exact-only deduplication of upstream's `std::set` would count it
    /// several times (UD-599); end-point comparison (upstream's 2015
    /// `compareSegments`) fails for bars through a vertex at small angles
    /// and cannot restore a lost piece (UD-601). Default `true`; `false`
    /// keeps the segment intersector's collected set (upstream).
    /// `true` needs `options.tetrahedra = Faces` for tetrahedral meshes.
    pub remove_near_duplicates: bool,
}

impl Default for EmbeddedInterfaceIntersector {
    fn default() -> Self {
        Self {
            options: SegmentIntersectorOptions::default(),
            remove_near_duplicates: true,
        }
    }
}

/// Upstream constructor part of `EmbeddedInterfaceIntersector`: an empty
/// interface mesh `interface_mesh` of the background's spatial dimension
/// with `_segment_2` connectivities (both ghost types) and empty
/// `associated_element` / `physical_names` arrays on `(_segment_2,
/// _not_ghost)`.
///
/// # Errors
/// An invalid spatial dimension.
pub fn empty_interface_mesh(spatial_dimension: usize) -> Result<Mesh> {
    let mut mesh = Mesh::new(spatial_dimension, "interface_mesh")?;
    {
        let mut acc = MeshAccessor::new(&mut mesh);
        acc.connectivity_mut(ElementType::Segment2, GhostType::NotGhost);
        acc.connectivity_mut(ElementType::Segment2, GhostType::Ghost);
    }
    let data = mesh.data_mut();
    data.element_map_mut_or_insert(ASSOCIATED_ELEMENT)?.alloc(
        0,
        1,
        ElementType::Segment2,
        GhostType::NotGhost,
        Element::NULL,
    )?;
    data.strings_mut_or_insert(PHYSICAL_NAMES)?.alloc(
        0,
        1,
        ElementType::Segment2,
        GhostType::NotGhost,
        String::new(),
    )?;
    Ok(mesh)
}

impl EmbeddedInterfaceIntersector {
    /// Intersector with the given segment-intersector options.
    #[must_use]
    pub fn new(options: SegmentIntersectorOptions) -> Self {
        Self {
            options,
            remove_near_duplicates: true,
        }
    }

    /// Upstream `createSegment(connectivity)`: the 3-D segment of a
    /// primitive `_segment_2` (`z = 0` for a 2-D model). `dim` is the
    /// model's spatial dimension.
    ///
    /// # Errors
    /// A node index outside the primitive mesh, or a primitive mesh with
    /// fewer coordinates per node than `dim` (upstream reads out of bounds).
    pub fn create_segment(
        primitive_mesh: &Mesh,
        dim: usize,
        nodes: [usize; 2],
    ) -> Result<Segment3> {
        let coords = primitive_mesh.nodes();
        let nc = coords.nb_component();
        if nc < dim {
            return Err(Error::DimensionMismatch {
                context: "EmbeddedInterfaceIntersector: primitive mesh coordinates",
                expected: dim,
                got: nc,
            });
        }
        let point = |n: usize| -> Result<Point3> {
            let x = coords.get_tuple(n)?;
            Ok(match dim {
                2 => Point3::new(x[0], x[1], 0.),
                _ => Point3::new(x[0], x[1], x[2]),
            })
        };
        Ok(Segment3::new(point(nodes[0])?, point(nodes[1])?))
    }

    /// The reinforcement segments of `primitive_mesh` grouped by physical
    /// name (byte-wise name order, primitive-element order within a name;
    /// upstream `name_to_primitives`).
    ///
    /// # Errors
    /// Missing `physical_names` on `(_segment_2, _not_ghost)` (upstream:
    /// "You must define physical names to reinforcements in order to use the
    /// embedded model"), more names than segments, invalid connectivity.
    pub fn primitives_by_name(
        primitive_mesh: &Mesh,
        dim: usize,
    ) -> Result<BTreeMap<String, Vec<Segment3>>> {
        let missing = || {
            Error::invalid(
                "You must define physical names to reinforcements in order to use the \
                 embedded model",
            )
        };
        let names = primitive_mesh
            .data()
            .strings(PHYSICAL_NAMES)
            .map_err(|_| missing())?
            .get(ElementType::Segment2, GhostType::NotGhost)
            .map_err(|_| missing())?;
        let conn = primitive_mesh
            .connectivity(ElementType::Segment2, GhostType::NotGhost)
            .map_err(|_| missing())?;
        let mut out: BTreeMap<String, Vec<Segment3>> = BTreeMap::new();
        for (element, name) in names.as_slice().iter().enumerate() {
            // upstream indexes the connectivity with the position of the name
            let c = conn.get_tuple(element).map_err(|_| Error::OutOfRange {
                what: "primitive _segment_2 (physical_names longer than the connectivity)",
                index: element,
                len: conn.len(),
            })?;
            let n0 = akantu_core::try_to_usize(c[0])?;
            let n1 = akantu_core::try_to_usize(c[1])?;
            let segment = Self::create_segment(primitive_mesh, dim, [n0, n1])?;
            out.entry(name.clone()).or_default().push(segment);
        }
        Ok(out)
    }

    /// Upstream `constructData()`: the interface mesh of `primitive_mesh`
    /// cut by `mesh` (see the module documentation).
    ///
    /// # Errors
    /// A 1-D model ("No embedded model in 1D. Deactivate intersection
    /// initialization"), missing physical names, an unsupported background
    /// element type, geometry errors.
    pub fn construct_data(&self, mesh: &Mesh, primitive_mesh: &Mesh) -> Result<Mesh> {
        let dim = mesh.spatial_dimension();
        if dim == 1 {
            return Err(Error::invalid(
                "No embedded model in 1D. Deactivate intersection initialization",
            ));
        }
        let mut interface = empty_interface_mesh(dim)?;
        let name_to_primitives = Self::primitives_by_name(primitive_mesh, dim)?;
        let types: Vec<ElementType> = mesh
            .element_types(TypeFilter {
                spatial_dimension: Some(dim),
                ghost_type: GhostType::NotGhost,
                kind: Some(ElementKind::Regular),
            })
            .collect();
        for ty in types {
            if !SUPPORTED_BACKGROUND_TYPES.contains(&ty) {
                return Err(Error::UnsupportedElementType {
                    ty,
                    context: "EmbeddedInterfaceIntersector::constructData (background type)",
                });
            }
            let options = SegmentIntersectorOptions {
                assembly: if self.remove_near_duplicates {
                    PieceAssembly::Tiled
                } else {
                    PieceAssembly::Collected
                },
                ..self.options
            };
            let mut intersector = MeshSegmentIntersector::<LinearScan>::new(mesh, ty, options)?;
            for (name, segments) in &name_to_primitives {
                intersector.set_physical_name(name.clone());
                intersector.build_result_from_query_list(mesh, segments, &mut interface)?;
            }
        }
        Ok(interface)
    }
}
