// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/geometry/mesh_geom_abstract.hh,
// mesh_geom_factory.hh, mesh_geom_factory_tmpl.hh (Cartesian primitives),
// aabb_primitives/triangle.hh, aabb_primitives/tetrahedron.hh,
// aabb_primitives/aabb_primitive.{hh,cc} (Triangle_primitive), and the
// Triangle/Cartesian case of tree_type_helper.hh.

//! Mesh geometry factory for Cartesian triangle primitives (GEOM-002,
//! GEOM-003, GEOM-004).
//!
//! Upstream `MeshGeomFactory<dim, type, Triangle<K>, K>::constructData`
//! turns every element of one type into triangle primitives tagged with the
//! element index and builds a CGAL AABB tree over them. Here the primitive
//! list is built by [`MeshGeomFactory`] and the search structure is any
//! [`crate::index::TriangleIndex`]: the LGPL [`crate::index::LinearScan`] or
//! the CGAL-compatible AABB tree of `akantu-geometry-cgal` (which reproduces
//! upstream's traversal order).
//!
//! Primitive construction (`AddPrimitiveHelper`):
//! * triangles (`_triangle_3`, `_triangle_6`, `_igfem_triangle_4/5`): one
//!   triangle `(x, y, 0)` of the first three nodes;
//! * `_tetrahedron_4`: upstream maps it to the *triangle* helper
//!   (`GeometricalTypeHelper<_tetrahedron_4>::type = _triangle`,
//!   mesh_geom_factory_tmpl.hh:100-102), so a tetrahedron yields the single
//!   triangle `(x, y, 0)` of its first three nodes (UD-407). The default
//!   builds the four faces `abc, bcd, cda, dab` of the intended
//!   `AddPrimitiveHelper<_tetrahedron, Cartesian>`;
//!   [`TetrahedronPrimitives::UpstreamTriangle`] reproduces upstream.

use akantu_core::{Element, ElementType, Error, GhostType, Real, Result, UInt, try_to_usize};
use akantu_mesh::Mesh;

use crate::kernel::{Bbox3, Point3, Triangle3};

/// Akantu `Triangle<cgal::Cartesian>` with its element id, as stored in
/// the AABB tree (`Triangle_primitive`: `id()`, `datum()`,
/// `reference_point() = vertex(0)`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TrianglePrimitive {
    /// Element index (`meshId`, upstream `UInt`).
    pub id: UInt,
    /// The triangle (`datum()`).
    pub triangle: Triangle3,
}

impl TrianglePrimitive {
    /// `reference_point()` = `vertex(0)`.
    #[inline]
    #[must_use]
    pub fn reference_point(&self) -> Point3 {
        self.triangle.v[0]
    }

    /// Bounding box of the datum.
    #[inline]
    #[must_use]
    pub fn bbox(&self) -> Bbox3 {
        self.triangle.bbox()
    }
}

/// How `_tetrahedron_4` elements become triangle primitives (UD-407).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TetrahedronPrimitives {
    /// The four faces `(a,b,c), (b,c,d), (c,d,a), (d,a,b)` (intended
    /// `AddPrimitiveHelper<_tetrahedron, Cartesian>`; default).
    #[default]
    Faces,
    /// Upstream behaviour: one triangle `(x, y, 0)` of the first three
    /// nodes (z ignored).
    UpstreamTriangle,
}

/// Element types upstream's factory supports with Cartesian primitives
/// (`GeometricalTypeHelper` specialisations).
#[must_use]
pub fn is_supported_type(ty: ElementType) -> bool {
    matches!(
        ty,
        ElementType::Triangle3
            | ElementType::Triangle6
            | ElementType::IgfemTriangle4
            | ElementType::IgfemTriangle5
            | ElementType::Tetrahedron4
    )
}

/// `AddPrimitiveHelper::addPrimitive`: append the primitives of one
/// element. `coords` holds the element's nodes, `dim` coordinates each
/// (node after node, upstream's column-major `dim × nb_nodes` matrix).
///
/// # Errors
/// Unsupported type or too few coordinates.
pub fn add_primitives(
    ty: ElementType,
    coords: &[Real],
    dim: usize,
    id: UInt,
    tetrahedra: TetrahedronPrimitives,
    out: &mut Vec<TrianglePrimitive>,
) -> Result<()> {
    if !is_supported_type(ty) {
        return Err(Error::UnsupportedElementType {
            ty,
            context: "MeshGeomFactory (Cartesian primitives)",
        });
    }
    let nb_nodes = ty.nb_nodes_per_element();
    if dim < 2 || coords.len() < dim * nb_nodes {
        return Err(Error::DimensionMismatch {
            context: "MeshGeomFactory::addPrimitive",
            expected: dim.max(2) * nb_nodes,
            got: coords.len(),
        });
    }
    let c = |r: usize, n: usize| coords[n * dim + r];
    let flat = |n: usize| Point3::new(c(0, n), c(1, n), 0.0);
    if ty == ElementType::Tetrahedron4 && tetrahedra == TetrahedronPrimitives::Faces {
        if dim < 3 {
            return Err(Error::DimensionMismatch {
                context: "MeshGeomFactory::addPrimitive (tetrahedron)",
                expected: 3,
                got: dim,
            });
        }
        let p = |n: usize| Point3::new(c(0, n), c(1, n), c(2, n));
        let (a, b, cc, d) = (p(0), p(1), p(2), p(3));
        for t in [
            Triangle3::new(a, b, cc),
            Triangle3::new(b, cc, d),
            Triangle3::new(cc, d, a),
            Triangle3::new(d, a, b),
        ] {
            out.push(TrianglePrimitive { id, triangle: t });
        }
    } else {
        out.push(TrianglePrimitive {
            id,
            triangle: Triangle3::new(flat(0), flat(1), flat(2)),
        });
    }
    Ok(())
}

/// Upstream `MeshGeomFactory<dim, type, Triangle<K>, K>` (the primitive
/// list; the search structure is built from it with
/// [`crate::index::TriangleIndex::build`]).
#[derive(Clone, Debug)]
pub struct MeshGeomFactory {
    ty: ElementType,
    tetrahedra: TetrahedronPrimitives,
    primitives: Vec<TrianglePrimitive>,
}

impl MeshGeomFactory {
    /// Factory for elements of type `ty`.
    ///
    /// # Errors
    /// Unsupported type.
    pub fn new(ty: ElementType, tetrahedra: TetrahedronPrimitives) -> Result<Self> {
        if !is_supported_type(ty) {
            return Err(Error::UnsupportedElementType {
                ty,
                context: "MeshGeomFactory (Cartesian primitives)",
            });
        }
        Ok(Self {
            ty,
            tetrahedra,
            primitives: Vec::new(),
        })
    }

    /// Element type.
    #[must_use]
    pub fn element_type(&self) -> ElementType {
        self.ty
    }

    /// Tetrahedron policy.
    #[must_use]
    pub fn tetrahedra(&self) -> TetrahedronPrimitives {
        self.tetrahedra
    }

    /// `constructData(ghost_type)`: rebuild the primitive list from the
    /// elements of `(type, ghost_type)` in element order.
    ///
    /// # Errors
    /// The mesh's spatial dimension differs from the type's (upstream's
    /// fixed-size `Matrix<Real, dim, nb_nodes>` assignment), or invalid
    /// connectivity.
    pub fn construct_data(&mut self, mesh: &Mesh, ghost: GhostType) -> Result<()> {
        self.primitives.clear();
        let dim = self.ty.spatial_dimension();
        if mesh.spatial_dimension() != dim {
            return Err(Error::DimensionMismatch {
                context: "MeshGeomFactory::constructData (mesh dimension vs element type)",
                expected: dim,
                got: mesh.spatial_dimension(),
            });
        }
        let n = mesh.nb_element(self.ty, ghost);
        self.primitives.reserve(
            n * if self.ty == ElementType::Tetrahedron4 {
                4
            } else {
                1
            },
        );
        let nodes = mesh.nodes();
        for e in 0..n {
            let el = Element::try_new(self.ty, e, ghost)?;
            let coords = mesh.extract_nodal_values_from_element(&nodes, el)?;
            let id = UInt::try_from(e)
                .map_err(|_| Error::InvalidArgument("element index exceeds UInt".into()))?;
            add_primitives(
                self.ty,
                &coords,
                dim,
                id,
                self.tetrahedra,
                &mut self.primitives,
            )?;
        }
        Ok(())
    }

    /// `getPrimitiveList()`.
    #[must_use]
    pub fn primitive_list(&self) -> &[TrianglePrimitive] {
        &self.primitives
    }

    /// The primitives of element `id` built from its current nodes (used by
    /// the tetrahedron branch of the segment intersector).
    ///
    /// # Errors
    /// Invalid element.
    pub fn element_primitives(
        &self,
        mesh: &Mesh,
        id: usize,
    ) -> Result<(Vec<Real>, Vec<TrianglePrimitive>)> {
        let el = Element::try_new(self.ty, id, GhostType::NotGhost)?;
        let coords = mesh.extract_nodal_values_from_element(&mesh.nodes(), el)?;
        let mut out = Vec::with_capacity(4);
        let uid = UInt::try_from(id)
            .map_err(|_| Error::InvalidArgument("element index exceeds UInt".into()))?;
        add_primitives(
            self.ty,
            &coords,
            self.ty.spatial_dimension(),
            uid,
            self.tetrahedra,
            &mut out,
        )?;
        Ok((coords, out))
    }
}

/// Element index of a primitive id as `usize`.
///
/// # Errors
/// Never for ids produced by the factory.
pub fn primitive_element(id: UInt) -> Result<usize> {
    try_to_usize(
        akantu_core::Idx::try_from(id)
            .map_err(|_| Error::InvalidArgument("primitive id".into()))?,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tetrahedron_policies() {
        let coords = [0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0];
        let mut faces = Vec::new();
        add_primitives(
            ElementType::Tetrahedron4,
            &coords,
            3,
            7,
            TetrahedronPrimitives::Faces,
            &mut faces,
        )
        .unwrap();
        assert_eq!(faces.len(), 4);
        assert!(faces.iter().all(|p| p.id == 7));
        assert_eq!(faces[1].triangle.v[2], Point3::new(0.0, 0.0, 1.0));
        let mut upstream = Vec::new();
        add_primitives(
            ElementType::Tetrahedron4,
            &coords,
            3,
            7,
            TetrahedronPrimitives::UpstreamTriangle,
            &mut upstream,
        )
        .unwrap();
        assert_eq!(upstream.len(), 1);
        assert_eq!(upstream[0].triangle.v[1], Point3::new(1.0, 0.0, 0.0));
        assert!(
            add_primitives(
                ElementType::Quadrangle4,
                &coords,
                2,
                0,
                TetrahedronPrimitives::Faces,
                &mut upstream
            )
            .is_err()
        );
    }
}
