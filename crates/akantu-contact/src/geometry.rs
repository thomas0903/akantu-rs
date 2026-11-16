// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/model/contact_mechanics/geometry_utils.hh,
// geometry_utils_inline_impl.hh, geometry_utils.cc (empty),
// src/common/aka_math_tmpl.hh (Math::normal).

//! Geometric utilities of contact detection (upstream `GeometryUtils`,
//! CONT-010, CONT-012).
//!
//! Conventions: node coordinates of a facet are a column-major `dim × nn`
//! slice (upstream `mesh.extractNodalValuesFromElement`), natural
//! coordinates have `sd = dim − 1` entries, a covariant basis is the
//! column-major `dim × sd` matrix of **unit** tangents (upstream normalises
//! the columns of `X ∂N/∂ξ`). Every expression reproduces the evaluation
//! order of the upstream Eigen expression (`crate::eigen_ops`).
//!
//! Deviations (registered upstream defects):
//! * **UD-363** — the Newton iteration of [`natural_projection`] uses the
//!   surface metric `a = G Gᵀ` (`G = ∂N/∂ξ · Xᵀ`, `sd × dim`); upstream
//!   passes `G` to `covariantMetricTensor`, which computes `Gᵀ G` (`dim ×
//!   dim`) and reads its top-left block (only the x/y components). On a 2-D
//!   master segment inclined by more than 45° the iteration does not
//!   converge (vertical segments give NaN). [`ProjectionMetric::Upstream`]
//!   reproduces it.
//! * **UD-371** — upstream orients a facet normal outward with the
//!   barycentres of the facet and of its neighbour element on the
//!   **reference** mesh nodes while the normal comes from the current
//!   tangents, so a rigid rotation of more than 90° turns every normal
//!   inward and the detection loses every contact. The corrected default
//!   ([`NormalOrientation::Current`]) evaluates the same barycentres on the
//!   current positions; [`NormalOrientation::Reference`] reproduces
//!   upstream. Both give the same sign whenever the reference rule is right
//!   (the flip is exact), so results are then bitwise identical.
//! * **UD-372** — upstream `isValidProjection` accepts natural coordinates
//!   in the box `[−1 − tol, 1 + tol]^{d−1}` for every facet type; for
//!   triangle facets (reference triangle `ξ, η ≥ 0`, `ξ + η ≤ 1`) it accepts
//!   points outside the facet, which a neighbouring coplanar (tie) or
//!   flatter facet then wins with extrapolated shape functions. The
//!   corrected default ([`ProjectionValidity::Element`]) prefers a facet
//!   that contains the projection (barycentric coordinates `≥ −tol`) and
//!   falls back to upstream's box only when no facet around the master node
//!   contains it (contacts upstream finds are kept);
//!   [`ProjectionValidity::UpstreamBox`] reproduces upstream.
//! * **UD-366** — upstream `contravariantBasis` multiplies `(TᵀT)⁻¹ · T`
//!   (inner dimensions `sd` and `dim` do not match: undefined behaviour in
//!   release builds; Python-only API); [`contravariant_basis`] returns the
//!   contravariant vectors `T (TᵀT)⁻¹`.

use akantu_core::eigen_order::{Reduction, sum_from_zero};
use akantu_core::{
    Array, Element, ElementKind, ElementType, Error, GhostType, Real, Result, try_to_usize,
};
use akantu_fe::with_element_class;
use akantu_fe::{ElementClass, LagrangeElement};
use akantu_mesh::Mesh;
use akantu_mesh::utils::ELEMENT_TO_SUBELEMENT;

use crate::eigen_ops::{
    SMALL_MAX, cross_normalized, dot, gemv, inverse_gram, matmul, norm, normalize, small_inverse,
};
use crate::selector::ContactSurface;

/// Maximal number of nodes of a facet in the contact buffers
/// (`_quadrangle_8`). It sizes workspaces only: **quadratic master facets
/// are unsupported**, as upstream (**UD-373**, category C) — the natural
/// projection needs `∂²N/∂ξ²`, implemented only for `_segment_2`,
/// `_triangle_3` and `_quadrangle_4`, so a search on `_segment_3`,
/// `_triangle_6`, `_quadrangle_8` contact facets returns
/// `Error::NotImplemented` ("computeD2NDS2 for this interpolation";
/// upstream throws `computeD2NDS2 : not implemented yet !`).
pub const MAX_FACET_NODES: usize = 8;

/// Metric used by the Newton iteration of the natural projection.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ProjectionMetric {
    /// Corrected (default): the surface metric `a = G Gᵀ`.
    #[default]
    Surface,
    /// Upstream (**UD-363** compatibility): the top-left `sd × sd` block of
    /// `Gᵀ G`.
    Upstream,
}

/// The mesh contact detection works on, and its parent when it is a mesh
/// of facets (cohesive selectors; upstream `mesh.isMeshFacets()` /
/// `getMeshParent()`).
#[derive(Clone, Copy, Debug)]
pub struct MeshView<'a> {
    /// Detection mesh (regular mesh or facet mesh).
    pub mesh: &'a Mesh,
    /// Parent mesh when `mesh` is a mesh of facets.
    pub parent: Option<&'a Mesh>,
}

impl<'a> MeshView<'a> {
    /// View of a regular mesh.
    #[must_use]
    pub fn new(mesh: &'a Mesh) -> Self {
        Self { mesh, parent: None }
    }

    /// View of a facet mesh `facets` whose parent is `parent`.
    #[must_use]
    pub fn facets(facets: &'a Mesh, parent: &'a Mesh) -> Self {
        Self {
            mesh: facets,
            parent: Some(parent),
        }
    }

    /// Spatial dimension.
    #[must_use]
    pub fn dim(&self) -> usize {
        self.mesh.spatial_dimension()
    }

    /// `true` for a facet mesh (upstream `isMeshFacets`).
    #[must_use]
    pub fn is_mesh_facets(&self) -> bool {
        self.parent.is_some()
    }

    /// Row of `element_to_subelement` of a facet (upstream
    /// `getElementToSubelement()(element)`).
    pub fn element_to_subelement(&self, element: Element) -> Result<&'a [Element]> {
        let e = element
            .try_idx()
            .ok_or_else(|| Error::invalid("element_to_subelement of ElementNull"))?;
        let map = self.mesh.data().element_lists(ELEMENT_TO_SUBELEMENT)?;
        let block = map.get(element.ty(), element.ghost_type())?;
        let row = block.get_tuple(e)?;
        row.first()
            .map(Vec::as_slice)
            .ok_or_else(|| Error::Mesh(format!("element_to_subelement of {element} has no entry")))
    }
}

/// Check that `ty` can be a master facet in dimension `dim` (regular kind,
/// natural dimension `dim − 1`, at most [`MAX_FACET_NODES`] nodes,
/// `dim ∈ {2, 3}`).
pub fn check_facet_type(ty: ElementType, dim: usize) -> Result<()> {
    if !(2..=3).contains(&dim) {
        return Err(Error::Unsupported(format!(
            "contact mechanics in dimension {dim} (2 and 3 are supported)"
        )));
    }
    if ty.kind() != ElementKind::Regular
        || ty.natural_space_dimension() + 1 != dim
        || ty.nb_nodes_per_element() > MAX_FACET_NODES
    {
        return Err(Error::UnsupportedElementType {
            ty,
            context: "contact master facet",
        });
    }
    Ok(())
}

/// Shape functions `N(ξ)` of `ty` (upstream `ElementClassHelper::getN`).
pub fn shapes_at(ty: ElementType, xi: &[Real], n: &mut [Real]) -> Result<()> {
    with_element_class!(ty, E => {
        if xi.len() != E::NATURAL_DIM || n.len() != E::NB_SHAPES {
            return Err(Error::DimensionMismatch {
                context: "shapes_at",
                expected: E::NB_SHAPES,
                got: n.len(),
            });
        }
        E::compute_shapes(xi, n);
        Ok(())
    }, else Err(Error::UnsupportedElementType { ty, context: "shapes_at" }))
}

/// `∂N/∂ξ` of `ty`, column-major `sd × nn` (upstream `getDNDS`).
pub fn dnds_at(ty: ElementType, xi: &[Real], d: &mut [Real]) -> Result<()> {
    with_element_class!(ty, E => {
        if xi.len() != E::NATURAL_DIM || d.len() != E::NATURAL_DIM * E::NB_SHAPES {
            return Err(Error::DimensionMismatch {
                context: "dnds_at",
                expected: E::NATURAL_DIM * E::NB_SHAPES,
                got: d.len(),
            });
        }
        E::compute_dnds(xi, d);
        Ok(())
    }, else Err(Error::UnsupportedElementType { ty, context: "dnds_at" }))
}

/// `∂²N/∂ξ²` of `ty`, column-major `sd² × nn` (upstream `getD2NDS2`;
/// `Error::NotImplemented` where upstream has `AKANTU_TO_IMPLEMENT`).
pub fn d2nds2_at(ty: ElementType, xi: &[Real], d2: &mut [Real]) -> Result<()> {
    with_element_class!(ty, E => {
        if xi.len() != E::NATURAL_DIM {
            return Err(Error::DimensionMismatch {
                context: "d2nds2_at",
                expected: E::NATURAL_DIM,
                got: xi.len(),
            });
        }
        E::compute_d2nds2(xi, d2)
    }, else Err(Error::UnsupportedElementType { ty, context: "d2nds2_at" }))
}

/// Upstream `isBoundaryElement`: a facet with a single regular neighbour,
/// or with fewer regular neighbours than entries (cohesive interfaces of a
/// facet mesh).
pub fn is_boundary_element(view: &MeshView<'_>, element: Element) -> Result<bool> {
    let row = view.element_to_subelement(element)?;
    if row.len() == 1 && row[0].kind() == ElementKind::Regular {
        return Ok(true);
    }
    let nb_regular = row
        .iter()
        .filter(|e| !e.is_null() && e.kind() == ElementKind::Regular)
        .count();
    Ok(nb_regular < row.len())
}

/// Upstream `isValidProjection`: every natural coordinate in
/// `[−1 − tol, 1 + tol]` (**UD-372** compatibility for triangle facets).
#[must_use]
pub fn is_valid_projection(xi: &[Real], extension_tolerance: Real) -> bool {
    xi.iter()
        .filter(|&&x| x >= -1.0 - extension_tolerance && x <= 1.0 + extension_tolerance)
        .count()
        == xi.len()
}

/// Rule accepting the projection of a slave on the facets around its
/// closest master node (UD-372).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ProjectionValidity {
    /// Corrected (default): among the accepted candidates, a projection
    /// that lies in its facet's reference element extended by the tolerance
    /// (`[−1 − tol, 1 + tol]` per coordinate for segments and quadrangles —
    /// upstream's box —, barycentric coordinates `ξ, η, 1 − ξ − η ≥ −tol`
    /// for triangles) is preferred; only when no facet around the master
    /// node contains the projection does upstream's box rule decide, so no
    /// contact that upstream finds is lost. Identical to upstream for
    /// segments and quadrangles.
    #[default]
    Element,
    /// Upstream (**UD-372** compatibility): the box `[−1 − tol, 1 + tol]`
    /// for every facet type ([`is_valid_projection`]).
    UpstreamBox,
}

/// Whether the natural projection `xi` lies in a facet of type `ty`
/// extended by `extension_tolerance`, under `rule` (NaN coordinates are
/// never valid).
#[must_use]
pub fn is_valid_projection_on(
    ty: ElementType,
    xi: &[Real],
    extension_tolerance: Real,
    rule: ProjectionValidity,
) -> bool {
    match (rule, ty) {
        (ProjectionValidity::Element, ElementType::Triangle3 | ElementType::Triangle6) => {
            xi.len() == 2
                && xi[0] >= -extension_tolerance
                && xi[1] >= -extension_tolerance
                && 1.0 - xi[0] - xi[1] >= -extension_tolerance
        }
        _ => is_valid_projection(xi, extension_tolerance),
    }
}

/// Configuration whose barycentres orient a facet normal outward (UD-371).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum NormalOrientation {
    /// Corrected (default): barycentres of the facet and of its neighbour
    /// element on the **current** positions (the configuration the normal
    /// is computed on).
    #[default]
    Current,
    /// Upstream (**UD-371** compatibility): barycentres on the reference
    /// mesh nodes ([`outside_direction`]).
    Reference,
}

/// The configuration an outward orientation is evaluated on.
#[derive(Clone, Copy, Debug)]
pub enum OutwardFrame<'a> {
    /// Current positions of the detection mesh's nodes (shared with the
    /// parent mesh for a facet mesh).
    Current(&'a Array<Real>),
    /// As [`OutwardFrame::Current`], with the directions memoised per
    /// facet in `cache` (the positions must not change while the cache's
    /// generation lasts, see [`OutwardCache::next_generation`]).
    CurrentCached(&'a Array<Real>, &'a std::cell::RefCell<OutwardCache>),
    /// Reference mesh nodes (upstream).
    Reference,
}

/// Memo of the outward directions ([`outside_direction_current`]) of
/// facets for one set of positions: a contact search projects every slave
/// on the facets around its master node, so a facet is oriented many times
/// on the same positions. The stored direction is the computed one, so a
/// cached orientation is identical to an uncached one.
#[derive(Clone, Debug, Default)]
pub struct OutwardCache {
    generation: u32,
    /// Per (type, ghost type): the generation each facet was stored in and
    /// its direction.
    tables: Vec<OutwardTable>,
}

/// One (type, ghost type) of an [`OutwardCache`]: stamps and directions.
type OutwardTable = (ElementType, GhostType, Vec<u32>, Vec<[Real; 3]>);

impl OutwardCache {
    /// Forget every stored direction (new positions).
    pub fn next_generation(&mut self) {
        self.generation = self.generation.wrapping_add(1);
        if self.generation == 0 {
            // wrapped: clear the stamps so that none matches
            for t in &mut self.tables {
                t.2.fill(0);
            }
            self.generation = 1;
        }
    }

    fn get(&self, element: Element) -> Option<[Real; 3]> {
        let (_, _, stamps, dirs) = self
            .tables
            .iter()
            .find(|t| t.0 == element.ty() && t.1 == element.ghost_type())?;
        let i = element.idx();
        (self.generation != 0 && stamps.get(i) == Some(&self.generation)).then(|| dirs[i])
    }

    fn put(&mut self, element: Element, dir: [Real; 3]) {
        if self.generation == 0 {
            self.generation = 1;
        }
        let pos = match self
            .tables
            .iter()
            .position(|t| t.0 == element.ty() && t.1 == element.ghost_type())
        {
            Some(p) => p,
            None => {
                self.tables
                    .push((element.ty(), element.ghost_type(), Vec::new(), Vec::new()));
                self.tables.len() - 1
            }
        };
        let (_, _, stamps, dirs) = &mut self.tables[pos];
        let i = element.idx();
        if stamps.len() <= i {
            stamps.resize(i + 1, 0);
            dirs.resize(i + 1, [0.0; 3]);
        }
        stamps[i] = self.generation;
        dirs[i] = dir;
    }
}

impl<'a> OutwardFrame<'a> {
    /// The frame of `orientation` (current: `positions`).
    #[must_use]
    pub fn of(orientation: NormalOrientation, positions: &'a Array<Real>) -> Self {
        match orientation {
            NormalOrientation::Current => Self::Current(positions),
            NormalOrientation::Reference => Self::Reference,
        }
    }
}

/// Barycentre of `element` of `mesh` on `positions` (the arithmetic of
/// `Mesh::barycenter_into`: sum in connectivity order, then division).
fn barycenter_on(
    mesh: &Mesh,
    positions: &Array<Real>,
    element: Element,
    out: &mut [Real],
) -> Result<()> {
    if positions.len() != mesh.nb_nodes() || positions.nb_component() != out.len() {
        return Err(Error::DimensionMismatch {
            context: "outward orientation: positions of the mesh nodes",
            expected: mesh.nb_nodes(),
            got: positions.len(),
        });
    }
    let conn = mesh.element_connectivity(element)?;
    out.fill(0.0);
    for &n in &conn {
        for (xi, &c) in out.iter_mut().zip(positions.get_tuple(try_to_usize(n)?)?) {
            *xi += c;
        }
    }
    #[allow(clippy::cast_precision_loss)] // node counts are tiny
    let inv = conn.len() as Real;
    for xi in out {
        *xi /= inv;
    }
    Ok(())
}

/// Corrected `outsideDirection` (**UD-371**): barycentre of the facet minus
/// barycentre of its first neighbour (in the parent mesh for a facet mesh,
/// whose nodes it shares), both on the current `positions`.
pub fn outside_direction_current(
    view: &MeshView<'_>,
    positions: &Array<Real>,
    element: Element,
    out: &mut [Real],
) -> Result<()> {
    let dim = view.dim();
    let row = view.element_to_subelement(element)?;
    let first = row
        .first()
        .copied()
        .filter(|e| !e.is_null())
        .ok_or_else(|| Error::Mesh(format!("facet {element} has no neighbour element")))?;
    let mut inside = [0.0; 3];
    match view.parent {
        Some(p) => barycenter_on(p, positions, first, &mut inside[..dim])?,
        None => barycenter_on(view.mesh, positions, first, &mut inside[..dim])?,
    }
    barycenter_on(view.mesh, positions, element, &mut out[..dim])?;
    for (o, i) in out[..dim].iter_mut().zip(&inside) {
        *o -= i;
    }
    Ok(())
}

/// Upstream `outsideDirection`: barycenter of the facet minus barycenter of
/// its first neighbour (in the parent mesh for a facet mesh), on the mesh
/// node coordinates (**UD-371** compatibility for the detection; the
/// nodal-area flip keeps it because its normals are computed on the mesh
/// nodes too). Written into `out[..dim]`.
pub fn outside_direction(view: &MeshView<'_>, element: Element, out: &mut [Real]) -> Result<()> {
    let dim = view.dim();
    let row = view.element_to_subelement(element)?;
    let first = row
        .first()
        .copied()
        .filter(|e| !e.is_null())
        .ok_or_else(|| Error::Mesh(format!("facet {element} has no neighbour element")))?;
    let mut inside = [0.0; 3];
    match view.parent {
        Some(p) => p.barycenter_into(first, &mut inside[..dim])?,
        None => view.mesh.barycenter_into(first, &mut inside[..dim])?,
    }
    view.mesh.barycenter_into(element, &mut out[..dim])?;
    for (o, i) in out[..dim].iter_mut().zip(&inside) {
        *o -= i;
    }
    Ok(())
}

/// Flip `normal` (and `tangents`) when it points into the neighbour
/// element (`outsideDirection · n < 0`), the direction evaluated on
/// `frame` (UD-371).
fn orient_outward(
    view: &MeshView<'_>,
    element: Element,
    normal: &mut [Real],
    tangents: Option<&mut [Real]>,
    frame: OutwardFrame<'_>,
) -> Result<()> {
    let dim = view.dim();
    let mut dir = [0.0; 3];
    match frame {
        OutwardFrame::Current(p) => outside_direction_current(view, p, element, &mut dir)?,
        OutwardFrame::CurrentCached(p, cache) => {
            let known = cache.borrow().get(element);
            if let Some(d) = known {
                dir = d;
            } else {
                outside_direction_current(view, p, element, &mut dir)?;
                cache.borrow_mut().put(element, dir);
            }
        }
        OutwardFrame::Reference => outside_direction(view, element, &mut dir)?,
    }
    if dot(&dir[..dim], &normal[..dim]) < 0.0 {
        if let Some(t) = tangents {
            for v in t.iter_mut() {
                *v *= -1.0;
            }
        }
        for v in normal[..dim].iter_mut() {
            *v *= -1.0;
        }
    }
    Ok(())
}

/// Upstream `GeometryUtils::normal(mesh, coords, element, outward)`: normal
/// of a flat facet from its first nodes (2-D `Math::normal(x₁ − x₀)`, 3-D
/// `Math::normal(x₁ − x₀, x₂ − x₀)`), flipped outward when `outward` names
/// the frame of the orientation (UD-371).
pub fn normal_from_coords(
    view: &MeshView<'_>,
    coords: &[Real],
    element: Element,
    outward: Option<OutwardFrame<'_>>,
) -> Result<[Real; 3]> {
    let dim = view.dim();
    let mut n = [0.0; 3];
    match dim {
        2 => {
            let v = [coords[2] - coords[0], coords[3] - coords[1]];
            n[..2].copy_from_slice(&akantu_fe::geometry::normal_2d(&v));
        }
        3 => {
            let v1 = [
                coords[3] - coords[0],
                coords[4] - coords[1],
                coords[5] - coords[2],
            ];
            let v2 = [
                coords[6] - coords[0],
                coords[7] - coords[1],
                coords[8] - coords[2],
            ];
            n = akantu_fe::geometry::normal_3d(&v1, &v2);
        }
        _ => {
            return Err(Error::Unsupported(format!(
                "GeometryUtils::normal in dimension {dim}"
            )));
        }
    }
    if let Some(frame) = outward {
        orient_outward(view, element, &mut n, None, frame)?;
    }
    Ok(n)
}

/// Upstream `GeometryUtils::normal(mesh, element, tangents, outward)`:
/// 2-D `(−t_y, t_x)`, 3-D `normalized(t₁ × t₂)`; when `outward` names a
/// frame (UD-371), both the normal and the tangents are flipped if the
/// normal points inward.
pub fn normal_from_tangents(
    view: &MeshView<'_>,
    element: Element,
    tangents: &mut [Real],
    outward: Option<OutwardFrame<'_>>,
) -> Result<[Real; 3]> {
    let dim = view.dim();
    let mut n = [0.0; 3];
    match dim {
        2 => {
            n[0] = -tangents[1];
            n[1] = tangents[0];
        }
        3 => n = cross_normalized(&tangents[0..3], &tangents[3..6]),
        _ => {
            return Err(Error::Unsupported(format!(
                "GeometryUtils::normal in dimension {dim}"
            )));
        }
    }
    if let Some(frame) = outward {
        orient_outward(
            view,
            element,
            &mut n,
            Some(&mut tangents[..dim * (dim - 1)]),
            frame,
        )?;
    }
    Ok(n)
}

/// Upstream `covariantBasis(coords, element, ξ)`: the columns of
/// `X · (∂N/∂ξ)ᵀ`, each normalised (`dim × sd` into `out`).
pub fn covariant_basis(
    coords: &[Real],
    dim: usize,
    ty: ElementType,
    xi: &[Real],
    out: &mut [Real],
) -> Result<()> {
    let sd = dim - 1;
    let nn = ty.nb_nodes_per_element();
    let mut dn = [0.0; 2 * MAX_FACET_NODES];
    dnds_at(ty, xi, &mut dn[..sd * nn])?;
    matmul(
        coords,
        dim,
        nn,
        false,
        &dn[..sd * nn],
        sd,
        nn,
        true,
        &mut out[..dim * sd],
    )?;
    for c in 0..sd {
        normalize(&mut out[c * dim..(c + 1) * dim]);
    }
    Ok(())
}

/// Upstream `covariantBasis(coords, element, normal, ξ)`: as
/// [`covariant_basis`], then the second tangent (3-D) or — in 2-D, where
/// upstream flips `tangents(1)` too — the column 1 is negated when the
/// expected normal (`e_z × t` in 2-D, `normalized(t₁ × t₂)` in 3-D) is
/// opposite to `normal`. In 2-D upstream's `tangents(1) *= -1` addresses a
/// column that does not exist (`dim × 1` basis): the port flips nothing
/// there and reports it in the return value (`true` = a flip was requested).
pub fn covariant_basis_with_normal(
    coords: &[Real],
    dim: usize,
    ty: ElementType,
    normal: &[Real],
    xi: &[Real],
    out: &mut [Real],
) -> Result<bool> {
    covariant_basis(coords, dim, ty, xi, out)?;
    let expected = match dim {
        // e_z × (t_x, t_y, 0) on fixed-size Vector<Real, 3>
        2 => akantu_fe::eigen::cross(&[0.0, 0.0, 1.0], &[out[0], out[1], 0.0]),
        3 => cross_normalized(&out[0..3], &out[3..6]),
        _ => {
            return Err(Error::NotImplemented(
                "covariantBasis in dimension other than 2 or 3",
            ));
        }
    };
    let ddot = dot(&normal[..dim], &expected[..dim]);
    if ddot < 0.0 {
        if dim == 3 {
            for v in out[3..6].iter_mut() {
                *v *= -1.0;
            }
        }
        return Ok(true);
    }
    Ok(false)
}

/// Upstream `curvature(coords, element, ξ)`: `X · (∂²N/∂ξ²)ᵀ` (`dim × sd²`).
pub fn curvature(
    coords: &[Real],
    dim: usize,
    ty: ElementType,
    xi: &[Real],
    out: &mut [Real],
) -> Result<()> {
    let sd = dim - 1;
    let nn = ty.nb_nodes_per_element();
    let mut d2 = [0.0; 4 * MAX_FACET_NODES];
    d2nds2_at(ty, xi, &mut d2[..sd * sd * nn])?;
    matmul(
        coords,
        dim,
        nn,
        false,
        &d2[..sd * sd * nn],
        sd * sd,
        nn,
        true,
        &mut out[..dim * sd * sd],
    )
}

/// Upstream `covariantCurvatureTensor`: `H_αβ = (X ∂²_αβN) · n` (`sd × sd`).
pub fn covariant_curvature_tensor(
    coords: &[Real],
    dim: usize,
    ty: ElementType,
    xi: &[Real],
    normal: &[Real],
    out: &mut [Real],
) -> Result<()> {
    let sd = dim - 1;
    let mut c = [0.0; 12];
    curvature(coords, dim, ty, xi, &mut c[..dim * sd * sd])?;
    for i in 0..sd * sd {
        // H(alpha, beta) with i = alpha·sd + beta, stored column-major
        let (alpha, beta) = (i / sd, i % sd);
        out[alpha + beta * sd] = dot(&c[i * dim..(i + 1) * dim], &normal[..dim]);
    }
    Ok(())
}

/// Upstream `covariantMetricTensor(T)` = `Tᵀ T` of a `dim × sd` basis.
pub fn covariant_metric_tensor(t: &[Real], dim: usize, sd: usize, out: &mut [Real]) -> Result<()> {
    crate::eigen_ops::gram(t, dim, sd, out)
}

/// Upstream `contravariantMetricTensor(T)` = `(Tᵀ T)⁻¹` (`sd ≤ 2`).
pub fn contravariant_metric_tensor(
    t: &[Real],
    dim: usize,
    sd: usize,
    out: &mut [Real],
) -> Result<()> {
    if sd > SMALL_MAX {
        return Err(Error::invalid(
            "contravariant metric of more than 2 tangents",
        ));
    }
    inverse_gram(t, dim, sd, out)
}

/// Contravariant basis `T (TᵀT)⁻¹` (`dim × sd`; **UD-366**: upstream
/// computes the ill-formed `(TᵀT)⁻¹ T`).
pub fn contravariant_basis(t: &[Real], dim: usize, sd: usize, out: &mut [Real]) -> Result<()> {
    let mut inv = [0.0; SMALL_MAX * SMALL_MAX];
    contravariant_metric_tensor(t, dim, sd, &mut inv[..sd * sd])?;
    matmul(
        t,
        dim,
        sd,
        false,
        &inv[..sd * sd],
        sd,
        sd,
        false,
        &mut out[..dim * sd],
    )
}

/// Upstream `realProjection(coords, element, ξ)` = `X · N(ξ)`.
pub fn real_projection(
    coords: &[Real],
    dim: usize,
    ty: ElementType,
    xi: &[Real],
    out: &mut [Real],
) -> Result<()> {
    let nn = ty.nb_nodes_per_element();
    if coords.len() < dim * nn {
        return Err(Error::DimensionMismatch {
            context: "real_projection: coordinates",
            expected: dim * nn,
            got: coords.len(),
        });
    }
    let mut n = [0.0; MAX_FACET_NODES];
    shapes_at(ty, xi, &mut n[..nn])?;
    gemv(coords, dim, nn, &n[..nn], &mut out[..dim]);
    Ok(())
}

/// Upstream `realProjection(coords, slave, normal)`: projection of `slave`
/// on the plane through the first node with normal `normal`.
pub fn real_projection_on_plane(
    coords: &[Real],
    slave: &[Real],
    normal: &[Real],
    out: &mut [Real],
) {
    let dim = slave.len();
    let mut d = [0.0; 3];
    for i in 0..dim {
        d[i] = slave[i] - coords[i];
    }
    let alpha = dot(&d[..dim], &normal[..dim]);
    for i in 0..dim {
        out[i] = slave[i] - alpha * normal[i];
    }
}

/// Result of [`natural_projection`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NaturalProjection {
    /// Natural coordinates (first `sd` entries).
    pub xi: [Real; 2],
    /// Physical point `X · N(ξ)` (first `dim` entries).
    pub master: [Real; 3],
    /// Newton iterations performed.
    pub iterations: usize,
    /// Final residual `‖f‖`.
    pub error: Real,
}

/// Upstream `naturalProjection`: Newton iterations from `ξ = 0` on
/// `f_α = −2 ∂_αx · (s − x(ξ))`, `J_αβ = 2 (a_αβ − ∂²_αβx · (s − x))`,
/// `ξ += −J⁻¹ f`, while `tolerance < ‖f‖` and fewer than `max_iterations`
/// iterations (no convergence report, as upstream). `metric` selects the
/// corrected surface metric or upstream's (UD-363).
#[allow(clippy::too_many_arguments)] // mirrors the upstream signature + compat switch
pub fn natural_projection(
    coords: &[Real],
    dim: usize,
    ty: ElementType,
    slave: &[Real],
    max_iterations: i64,
    tolerance: Real,
    metric: ProjectionMetric,
) -> Result<NaturalProjection> {
    check_facet_type(ty, dim)?;
    let sd = dim - 1;
    let nn = ty.nb_nodes_per_element();
    if coords.len() < dim * nn || slave.len() < dim {
        return Err(Error::DimensionMismatch {
            context: "natural_projection: coordinates",
            expected: dim * nn,
            got: coords.len(),
        });
    }
    let mut xi = [0.0; 2];
    let mut master = [0.0; 3];
    let mut grad = [0.0; 6];
    let mut f = [0.0; 2];
    let mut n = [0.0; MAX_FACET_NODES];
    let mut dn = [0.0; 2 * MAX_FACET_NODES];

    let mut update_f = |xi: &[Real],
                        master: &mut [Real; 3],
                        grad: &mut [Real; 6],
                        f: &mut [Real; 2]|
     -> Result<Real> {
        shapes_at(ty, xi, &mut n[..nn])?;
        gemv(coords, dim, nn, &n[..nn], &mut master[..dim]);
        dnds_at(ty, xi, &mut dn[..sd * nn])?;
        matmul(
            &dn[..sd * nn],
            sd,
            nn,
            false,
            coords,
            dim,
            nn,
            true,
            &mut grad[..sd * dim],
        )?;
        for alpha in 0..sd {
            let d =
                Reduction::Sequential.sum(dim, &|i| grad[alpha + i * sd] * (slave[i] - master[i]));
            f[alpha] = -2. * d;
        }
        Ok(norm(&f[..sd]))
    };

    let mut error = update_f(&xi[..sd], &mut master, &mut grad, &mut f)?;
    let mut iterations: i64 = 0;
    let mut d2 = [0.0; 4 * MAX_FACET_NODES];
    while tolerance < error && iterations < max_iterations {
        // a: metric used by the Newton Jacobian
        let mut a = [0.0; 9];
        let a_rows = match metric {
            ProjectionMetric::Upstream => {
                matmul(
                    &grad[..sd * dim],
                    sd,
                    dim,
                    true,
                    &grad[..sd * dim],
                    sd,
                    dim,
                    false,
                    &mut a[..dim * dim],
                )?;
                dim
            }
            ProjectionMetric::Surface => {
                matmul(
                    &grad[..sd * dim],
                    sd,
                    dim,
                    false,
                    &grad[..sd * dim],
                    sd,
                    dim,
                    true,
                    &mut a[..sd * sd],
                )?;
                sd
            }
        };
        d2nds2_at(ty, &xi[..sd], &mut d2[..sd * sd * nn])?;
        let mut jac = [0.0; SMALL_MAX * SMALL_MAX];
        for alpha in 0..sd {
            for beta in 0..sd {
                let index = alpha * sd + beta;
                let mut col = [0.0; MAX_FACET_NODES];
                for (k, c) in col.iter_mut().enumerate().take(nn) {
                    *c = d2[index + k * sd * sd];
                }
                let mut dab = [0.0; 3];
                gemv(coords, dim, nn, &col[..nn], &mut dab[..dim]);
                let dd = Reduction::Sequential.sum(dim, &|i| dab[i] * (slave[i] - master[i]));
                jac[alpha + beta * sd] = 2. * (a[alpha + beta * a_rows] - dd);
            }
        }
        let mut jinv = [0.0; SMALL_MAX * SMALL_MAX];
        small_inverse(sd, &jac[..sd * sd], &mut jinv[..sd * sd]);
        let mut dxi = [0.0; 2];
        for (i, d) in dxi.iter_mut().enumerate().take(sd) {
            *d = 0.0;
            *d += sum_from_zero(sd, &|l| -jinv[i + l * sd] * f[l]);
        }
        for i in 0..sd {
            xi[i] += dxi[i];
        }
        error = update_f(&xi[..sd], &mut master, &mut grad, &mut f)?;
        iterations += 1;
    }
    Ok(NaturalProjection {
        xi,
        master,
        iterations: usize::try_from(iterations).unwrap_or(0),
        error,
    })
}

/// Accepted projection of [`orthogonal_projection`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OrthogonalProjection {
    /// Master facet.
    pub element: Element,
    /// Signed gap (`−|s − x(ξ)|`, upstream convention before the model's
    /// sign flip).
    pub gap: Real,
    /// Natural coordinates (first `sd`).
    pub xi: [Real; 2],
    /// Outward unit normal (first `dim`).
    pub normal: [Real; 3],
    /// Unit covariant tangents, column-major `dim × sd` (first `dim·sd`).
    pub tangents: [Real; 6],
}

/// Parameters of the orthogonal projection (upstream detector parameters).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ProjectionParameters {
    /// Newton iterations (upstream `max_iterations`).
    pub max_iterations: i64,
    /// Newton tolerance on `‖f‖` (upstream `projection_tolerance`).
    pub projection_tolerance: Real,
    /// Admissible extension of the reference element (upstream
    /// `extension_tolerance`).
    pub extension_tolerance: Real,
    /// Newton metric (UD-363).
    pub metric: ProjectionMetric,
    /// Configuration of the outward orientation of the normals (UD-371).
    pub orientation: NormalOrientation,
    /// Rule preferring projections inside their facet (UD-372).
    pub validity: ProjectionValidity,
}

/// Upstream `orthogonalProjection`: over the facets of `elements` that
/// belong to `surface` (`contact_surface`, not-ghost lists as upstream),
/// project `slave` (natural projection on the facet's coordinates taken
/// from `positions`), build the unit tangents and the outward normal
/// (oriented on `params.orientation`, UD-371), and accept the facet if
/// `(s − x)/|s − x| · n < 0` (penetration), `|s − x|` is not larger than
/// the best so far (`<=`: the last equal candidate wins) and the
/// projection lies in the box `[−1 − tol, 1 + tol]^{d−1}`; with
/// [`ProjectionValidity::Element`] (UD-372) the best candidate whose
/// projection lies in its facet wins over that, when there is one.
pub fn orthogonal_projection(
    view: &MeshView<'_>,
    positions: &Array<Real>,
    slave: &[Real],
    elements: &[Element],
    surface: &ContactSurface,
    params: &ProjectionParameters,
) -> Result<Option<OrthogonalProjection>> {
    let frame = OutwardFrame::of(params.orientation, positions);
    orthogonal_projection_in(view, positions, slave, elements, surface, params, frame)
}

/// [`orthogonal_projection`] with the normals oriented on `frame` (which
/// must be the frame of `params.orientation` on `positions`, possibly
/// cached: [`OutwardFrame::CurrentCached`]).
///
/// # Errors
/// As [`orthogonal_projection`].
pub fn orthogonal_projection_in(
    view: &MeshView<'_>,
    positions: &Array<Real>,
    slave: &[Real],
    elements: &[Element],
    surface: &ContactSurface,
    params: &ProjectionParameters,
    frame: OutwardFrame<'_>,
) -> Result<Option<OrthogonalProjection>> {
    let dim = view.dim();
    let sd = dim - 1;
    // upstream's candidate (box rule) and, for the corrected rule, the best
    // candidate whose projection lies in its facet (preferred, UD-372)
    let mut found: Option<OrthogonalProjection> = None;
    let mut min_gap = Real::MAX;
    let mut found_inside: Option<OrthogonalProjection> = None;
    let mut min_gap_inside = Real::MAX;
    let mut coords = [0.0; 3 * MAX_FACET_NODES];
    for &element in elements {
        if !surface.contains(element.ty(), GhostType::NotGhost, element.index()) {
            continue;
        }
        check_facet_type(element.ty(), dim)?;
        let conn = view.mesh.element_connectivity(element)?;
        let nn = conn.len();
        for (k, &node) in conn.iter().enumerate() {
            coords[k * dim..(k + 1) * dim]
                .copy_from_slice(positions.get_tuple(try_to_usize(node)?)?);
        }
        let coords = &coords[..dim * nn];
        let np = natural_projection(
            coords,
            dim,
            element.ty(),
            slave,
            params.max_iterations,
            params.projection_tolerance,
            params.metric,
        )?;
        let mut tangents = [0.0; 6];
        covariant_basis(coords, dim, element.ty(), &np.xi[..sd], &mut tangents)?;
        let normal = normal_from_tangents(view, element, &mut tangents, Some(frame))?;
        let mut master_to_slave = [0.0; 3];
        for i in 0..dim {
            master_to_slave[i] = slave[i] - np.master[i];
        }
        let temp_gap = norm(&master_to_slave[..dim]);
        if temp_gap != 0.0 {
            for v in master_to_slave[..dim].iter_mut() {
                *v /= temp_gap;
            }
        }
        let product = dot(&master_to_slave[..dim], &normal[..dim]);
        let candidate = OrthogonalProjection {
            element,
            gap: -temp_gap,
            xi: np.xi,
            normal,
            tangents,
        };
        if product < 0.0
            && temp_gap <= min_gap
            && is_valid_projection(&np.xi[..sd], params.extension_tolerance)
        {
            min_gap = temp_gap;
            found = Some(candidate);
        }
        if params.validity == ProjectionValidity::Element
            && product < 0.0
            && temp_gap <= min_gap_inside
            && is_valid_projection_on(
                element.ty(),
                &np.xi[..sd],
                params.extension_tolerance,
                ProjectionValidity::Element,
            )
        {
            min_gap_inside = temp_gap;
            found_inside = Some(candidate);
        }
    }
    Ok(match params.validity {
        ProjectionValidity::Element => found_inside.or(found),
        ProjectionValidity::UpstreamBox => found,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A flat linear segment: one Newton step gives the exact foot of the
    /// perpendicular in both metrics; horizontal: identical bits.
    #[test]
    fn natural_projection_on_segments() {
        let coords = [0.0, 0.0, 2.0, 0.0]; // horizontal, x ∈ [0, 2]
        let s = [1.5, -0.25];
        for m in [ProjectionMetric::Surface, ProjectionMetric::Upstream] {
            let p =
                natural_projection(&coords, 2, ElementType::Segment2, &s, 100, 1e-10, m).unwrap();
            assert_eq!(p.xi[0], 0.5);
            assert_eq!(&p.master[..2], &[1.5, 0.0]);
            assert_eq!(p.iterations, 1);
        }
        // inclined 30°: the corrected metric converges in one step
        let (c, sn) = (30f64.to_radians().cos(), 30f64.to_radians().sin());
        let coords = [0.0, 0.0, 2.0 * c, 2.0 * sn];
        let s = [1.2 * c - 0.1 * sn, 1.2 * sn + 0.1 * c];
        let p = natural_projection(
            &coords,
            2,
            ElementType::Segment2,
            &s,
            100,
            1e-10,
            ProjectionMetric::Surface,
        )
        .unwrap();
        assert!((p.xi[0] - 0.2).abs() < 1e-14, "{}", p.xi[0]);
        assert!(p.iterations <= 2);
        // the upstream metric converges linearly (factor tan²30° = 1/3)
        let u = natural_projection(
            &coords,
            2,
            ElementType::Segment2,
            &s,
            100,
            1e-10,
            ProjectionMetric::Upstream,
        )
        .unwrap();
        assert!((u.xi[0] - 0.2).abs() < 1e-9);
        assert!(u.iterations > 5, "{}", u.iterations);
        // inclined 60°: upstream diverges (factor tan²60° = 3), stops at max_iterations
        let (c, sn) = (60f64.to_radians().cos(), 60f64.to_radians().sin());
        let coords = [0.0, 0.0, 2.0 * c, 2.0 * sn];
        let s = [1.2 * c - 0.1 * sn, 1.2 * sn + 0.1 * c];
        let u = natural_projection(
            &coords,
            2,
            ElementType::Segment2,
            &s,
            20,
            1e-10,
            ProjectionMetric::Upstream,
        )
        .unwrap();
        assert!(u.iterations == 20 || u.error.is_nan());
        let p = natural_projection(
            &coords,
            2,
            ElementType::Segment2,
            &s,
            20,
            1e-10,
            ProjectionMetric::Surface,
        )
        .unwrap();
        assert!((p.xi[0] - 0.2).abs() < 1e-14);
        // vertical: upstream Jacobian is 0 → ξ = ±inf, then NaN residual
        let coords = [0.0, 0.0, 0.0, 2.0];
        let u = natural_projection(
            &coords,
            2,
            ElementType::Segment2,
            &[0.1, 1.2],
            20,
            1e-10,
            ProjectionMetric::Upstream,
        )
        .unwrap();
        assert!(!u.xi[0].is_finite());
        assert!(!is_valid_projection(&u.xi[..1], 1e-5));
    }

    #[test]
    fn natural_projection_on_bilinear_quad() {
        // warped quadrangle in 3-D; project a point and check orthogonality
        let coords = [0.0, 0.0, 0.0, 2.0, 0.0, 0.1, 2.0, 2.0, -0.1, 0.0, 2.0, 0.2];
        let s = [0.7, 1.1, 0.5];
        let p = natural_projection(
            &coords,
            3,
            ElementType::Quadrangle4,
            &s,
            100,
            1e-12,
            ProjectionMetric::Surface,
        )
        .unwrap();
        assert!(p.error <= 1e-12);
        let mut t = [0.0; 6];
        covariant_basis(&coords, 3, ElementType::Quadrangle4, &p.xi[..2], &mut t).unwrap();
        let d: Vec<Real> = (0..3).map(|i| s[i] - p.master[i]).collect();
        assert!(dot(&d, &t[0..3]).abs() < 1e-11);
        assert!(dot(&d, &t[3..6]).abs() < 1e-11);
        // tangents are unit vectors
        assert!((norm(&t[0..3]) - 1.0).abs() < 1e-15);
        assert!((norm(&t[3..6]) - 1.0).abs() < 1e-15);
    }

    #[test]
    fn metric_tensors_and_contravariant_basis() {
        let t = [1.0, 0.0, 0.0, 1.0, 1.0, 0.0]; // dim 3, sd 2
        let mut g = [0.0; 4];
        covariant_metric_tensor(&t, 3, 2, &mut g).unwrap();
        assert_eq!(g, [1.0, 1.0, 1.0, 2.0]);
        let mut gi = [0.0; 4];
        contravariant_metric_tensor(&t, 3, 2, &mut gi).unwrap();
        assert_eq!(gi, [2.0, -1.0, -1.0, 1.0]);
        let mut c = [0.0; 6];
        contravariant_basis(&t, 3, 2, &mut c).unwrap();
        // tᵅ · t_β = δᵅ_β
        for a in 0..2 {
            for b in 0..2 {
                let d = dot(&c[a * 3..a * 3 + 3], &t[b * 3..b * 3 + 3]);
                assert_eq!(d, if a == b { 1.0 } else { 0.0 });
            }
        }
    }

    #[test]
    fn triangle_projection_validity_ud372() {
        let t = ElementType::Triangle3;
        let (e, u) = (ProjectionValidity::Element, ProjectionValidity::UpstreamBox);
        // inside, on the edges (within the tolerance)
        for xi in [[0.2, 0.3], [0.0, 0.0], [0.5, 0.5], [1.0 + 5e-6, -5e-6]] {
            assert!(is_valid_projection_on(t, &xi, 1e-5, e), "{xi:?}");
            assert!(is_valid_projection_on(t, &xi, 1e-5, u), "{xi:?}");
        }
        // outside the triangle but inside upstream's box
        for xi in [[0.9, 0.9], [-0.5, 0.2], [0.3, -0.01], [0.6, 0.4 + 1e-4]] {
            assert!(!is_valid_projection_on(t, &xi, 1e-5, e), "{xi:?}");
            assert!(is_valid_projection_on(t, &xi, 1e-5, u), "{xi:?}");
        }
        assert!(!is_valid_projection_on(t, &[Real::NAN, 0.1], 1e-5, e));
        // segments and quadrangles: the box for both rules
        for ty in [ElementType::Segment2, ElementType::Quadrangle4] {
            let xi: &[Real] = if ty == ElementType::Segment2 {
                &[0.9]
            } else {
                &[0.9, -0.9]
            };
            assert!(is_valid_projection_on(ty, xi, 0.0, e));
            let out: &[Real] = if ty == ElementType::Segment2 {
                &[1.1]
            } else {
                &[0.9, -1.1]
            };
            assert!(!is_valid_projection_on(ty, out, 0.0, e));
        }
    }

    #[test]
    fn projection_validity() {
        assert!(is_valid_projection(&[1.0 + 1e-6], 1e-5));
        assert!(!is_valid_projection(&[1.0 + 1e-4], 1e-5));
        assert!(!is_valid_projection(&[Real::NAN], 1e-5));
        assert!(is_valid_projection(&[0.0, -1.0], 0.0));
    }
}
