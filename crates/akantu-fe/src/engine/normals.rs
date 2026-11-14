// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/fe_engine/fe_engine_template_tmpl.hh
// (computeNormalsOnIntegrationPoints, regular and _point_1
// specialisations), shape_cohesive_inline_impl.hh
// (ShapeLagrange<_ek_cohesive>::computeNormalsOnIntegrationPoints),
// element_class_tmpl.hh (computeNormalsOnNaturalCoordinates).

//! Normals on integration points (FE-008, FE-029, FE-033).
//!
//! * Facet elements (natural dimension = mesh dimension − 1):
//!   `computeNormalsOnNaturalCoordinates` with upstream's operand types
//!   (`Map<Matrix<Real, Dynamic, nn>>` node coordinates ⇒ fixed `dnds`,
//!   coefficient-based products): bit-exact.
//! * `_point_1` in 1-D: `±1` from the adjacent segment. **UD-043 fix**:
//!   upstream uses the element index as a node index (`coords(elem)`); the
//!   port uses the point's node (`coords(connectivity(elem))`).
//! * Cohesive elements: tangents from the natural derivatives of the
//!   mid-surface field, 2-D `n = (−t₁, t₀)/‖t‖` (opposite sign convention to
//!   regular facets), 3-D `(t₀ × t₁)/‖·‖`, 1-D `±1` from the barycentres of
//!   the segments adjacent to the two facets.

use akantu_core::array::ArrayView;
use akantu_core::{Array, Element, ElementKind, ElementType, Error, GhostType, Idx, Real, Result};
use akantu_mesh::Mesh;
use akantu_mesh::data::MeshDataValues;

use super::{Block, FeEngine, element_ids, prepare_out, resolve_filter};
use crate::eigen;
use crate::element_class::ElementClass;
use crate::kernels;
use crate::shape::CohesiveReduction;
use crate::with_element_class;

impl FeEngine {
    /// Normals at the quadrature points of the facet elements of this
    /// engine (upstream `computeNormalsOnIntegrationPoints(ghost_type)`),
    /// stored for [`FeEngine::normals_on_integration_points`]. Point
    /// elements take their adjacent segments from `mesh` (mesh data
    /// `element_to_subelement`); for a facet mesh use
    /// [`FeEngine::compute_normals_on_integration_points_with`].
    pub fn compute_normals_on_integration_points(
        &mut self,
        mesh: &Mesh,
        ghost: GhostType,
    ) -> Result<()> {
        self.compute_normals_on_integration_points_with(mesh, mesh.nodes().view(), mesh, ghost)
    }

    /// [`FeEngine::compute_normals_on_integration_points`] for an arbitrary
    /// nodal `field` (upstream overload with a field), and an explicit mesh
    /// holding the segments adjacent to point elements (upstream: the parent
    /// mesh when `mesh` is a mesh of facets).
    pub fn compute_normals_on_integration_points_with(
        &mut self,
        mesh: &Mesh,
        field: ArrayView<'_, Real>,
        segments_mesh: &Mesh,
        ghost: GhostType,
    ) -> Result<()> {
        if self.kind != ElementKind::Regular {
            return Err(Error::NotImplemented(
                "FEEngine::computeNormalsOnIntegrationPoints for non-regular kinds (AKANTU_TO_IMPLEMENT \
                 upstream; use compute_cohesive_normals)",
            ));
        }
        let types: Vec<ElementType> = self.element_types(mesh, ghost).collect();
        for ty in types {
            let mut normals = Array::new(mesh.spatial_dimension());
            self.compute_normals_for(mesh, field, segments_mesh, &mut normals, ty, ghost)?;
            self.normals.as_map_mut().replace(ty, ghost, normals);
        }
        Ok(())
    }

    /// Normals of the elements of one type into `out` (upstream
    /// `computeNormalsOnIntegrationPoints(field, normal, type, ghost)`):
    /// `nb_element·nq` tuples of `sdim`.
    pub fn compute_normals_for(
        &self,
        mesh: &Mesh,
        field: ArrayView<'_, Real>,
        segments_mesh: &Mesh,
        out: &mut Array<Real>,
        ty: ElementType,
        ghost: GhostType,
    ) -> Result<()> {
        self.check_type(ty, "compute_normals_on_integration_points")?;
        let sdim = mesh.spatial_dimension();
        let nq = self.nb_integration_points(ty, ghost)?;
        let block = Block::new(mesh, ty, ghost)?;
        if ty == ElementType::Point1 {
            return point_normals(mesh, segments_mesh, &block, nq, out);
        }
        if field.nb_component() != sdim || field.len() != mesh.nb_nodes() {
            return Err(Error::DimensionMismatch {
                context: "compute_normals_on_integration_points: field",
                expected: sdim,
                got: field.nb_component(),
            });
        }
        let rule = self.rule(ty, ghost)?;
        prepare_out(out, block.nb_element * nq, sdim)?;
        with_element_class!(ty, E => {
            let nd = E::NATURAL_DIM;
            let mut pts = vec![0.0; nd * nq];
            for q in 0..nq {
                pts[q * nd..(q + 1) * nd].copy_from_slice(rule.point(q, nd));
            }
            let mut x = vec![0.0; sdim * block.nnpe];
            let o = out.as_mut_slice();
            for e in 0..block.nb_element {
                block.gather(e, field.as_slice(), sdim, &mut x)?;
                kernels::normals_on_natural::<E>(&pts, &x, sdim, true, &mut o[e * nq * sdim..(e + 1) * nq * sdim])?;
            }
            Ok(())
        }, else Err(Error::UnsupportedElementType { ty, context: "compute_normals_on_integration_points" }))
    }

    /// Normals of cohesive elements from a nodal field (upstream
    /// `ShapeLagrange<_ek_cohesive>::computeNormalsOnIntegrationPoints<type,
    /// ReduceFunction>`, used by `MaterialCohesive::computeNormal` with the
    /// mean reduction on the positions): `filter.len()·nq` tuples of `sdim`.
    /// 1-D cohesive elements need the facet mesh of `mesh` with the
    /// `subelement_to_element` / `element_to_subelement` data. Deviation:
    /// upstream's 1-D branch writes at the element id instead of the
    /// filtered position when a filter is given; the port writes at the
    /// filtered position.
    #[allow(clippy::too_many_arguments)]
    pub fn compute_cohesive_normals(
        &self,
        mesh: &Mesh,
        u: ArrayView<'_, Real>,
        out: &mut Array<Real>,
        ty: ElementType,
        ghost: GhostType,
        reduction: CohesiveReduction,
        filter: Option<&[Idx]>,
    ) -> Result<()> {
        if self.kind != ElementKind::Cohesive {
            return Err(Error::UnsupportedElementType {
                ty,
                context: "compute_cohesive_normals (cohesive engines only)",
            });
        }
        let sdim = mesh.spatial_dimension();
        self.check_type(ty, "compute_cohesive_normals")?;
        if ty.natural_space_dimension() + 1 != sdim {
            return Err(Error::DimensionMismatch {
                context: "compute_cohesive_normals (mesh dimension must be the cohesive natural \
                          dimension + 1)",
                expected: ty.natural_space_dimension() + 1,
                got: sdim,
            });
        }
        let nq = self.nb_integration_points(ty, ghost)?;
        let block = Block::new(mesh, ty, ghost)?;
        let filter = resolve_filter(filter, block.nb_element)?;
        let ids = element_ids(filter.clone(), block.nb_element);
        prepare_out(out, ids.len() * nq, sdim)?;
        match sdim {
            3 | 2 => {
                let mut tangents = Array::new(sdim * (sdim - 1));
                let fidx: Option<Vec<Idx>> = filter
                    .as_ref()
                    .map(|f| f.iter().map(|&e| akantu_core::to_idx(e)).collect());
                self.variation_impl(
                    mesh,
                    u,
                    &mut tangents,
                    sdim,
                    ty,
                    ghost,
                    fidx.as_deref(),
                    Some(reduction),
                )?;
                let o = out.as_mut_slice();
                for (n, t) in o
                    .chunks_mut(sdim)
                    .zip(tangents.as_slice().chunks(sdim * (sdim - 1)))
                {
                    if sdim == 3 {
                        let t0 = [t[0], t[1], t[2]];
                        let t1 = [t[3], t[4], t[5]];
                        let mut c = eigen::cross(&t0, &t1);
                        eigen::normalize(&mut c, true);
                        n.copy_from_slice(&c);
                    } else {
                        let mut v = [-t[1], t[0]];
                        eigen::normalize(&mut v, true);
                        n.copy_from_slice(&v);
                    }
                }
                Ok(())
            }
            1 => cohesive_normals_1d(mesh, &block, &ids, nq, out),
            _ => Err(Error::Unsupported(format!(
                "cohesive normals in dimension {sdim}"
            ))),
        }
    }
}

/// Elemental `ElementList` / `Element` data of `mesh` by name.
fn element_lists<'a>(
    mesh: &'a Mesh,
    name: &str,
    ty: ElementType,
    ghost: GhostType,
) -> Result<&'a Array<Vec<Element>>> {
    match mesh.data().elemental(name)? {
        MeshDataValues::ElementList(m) => m.get(ty, ghost),
        _ => Err(Error::invalid(format!(
            "mesh data '{name}' is not an element list"
        ))),
    }
}

fn element_refs<'a>(
    mesh: &'a Mesh,
    name: &str,
    ty: ElementType,
    ghost: GhostType,
) -> Result<&'a Array<Element>> {
    match mesh.data().elemental(name)? {
        MeshDataValues::Element(m) => m.get(ty, ghost),
        _ => Err(Error::invalid(format!(
            "mesh data '{name}' is not an element map"
        ))),
    }
}

/// `_point_1` normals in 1-D (upstream specialisation, with the UD-043
/// fix): `±1` pointing out of the single adjacent segment, `1` for points
/// shared by two or more segments.
fn point_normals(
    mesh: &Mesh,
    segments_mesh: &Mesh,
    block: &Block<'_>,
    nq: usize,
    out: &mut Array<Real>,
) -> Result<()> {
    if mesh.spatial_dimension() != 1 {
        return Err(Error::DimensionMismatch {
            context: "point normals (mesh dimension must be 1)",
            expected: 1,
            got: mesh.spatial_dimension(),
        });
    }
    prepare_out(out, block.nb_element * nq, 1)?;
    if block.nb_element == 0 {
        return Ok(());
    }
    let segments = element_lists(mesh, "element_to_subelement", block.ty, block.ghost)?;
    let coords = block.nodes();
    let o = out.as_mut_slice();
    for e in 0..block.nb_element {
        let adj = segments
            .get_tuple(e)?
            .first()
            .ok_or_else(|| Error::invalid("missing element_to_subelement entry"))?;
        let mut normal_value = 1.0;
        match adj.len() {
            0 => {
                return Err(Error::Mesh(
                    "Impossible to compute a normal on a point connected to 0 segments".into(),
                ));
            }
            1 => {
                let point = block.node(e, 0)?;
                let seg = adj[0];
                let sc = segments_mesh.element_connectivity(seg)?;
                let &[n0, n1, ..] = sc.as_slice() else {
                    return Err(Error::Mesh(format!(
                        "point normals: adjacent element {seg:?} has fewer than two nodes"
                    )));
                };
                let other = if usize::try_from(n0).ok() == Some(point) {
                    n1
                } else {
                    n0
                };
                let other = akantu_core::try_to_usize(other)?;
                let x_other = *coords.get(other).ok_or(Error::OutOfRange {
                    what: "point normals: node of the adjacent segment",
                    index: other,
                    len: coords.len(),
                })?;
                // UD-043: upstream reads coords(elem); the port reads coords(point).
                let difference = coords[point] - x_other;
                normal_value = difference / difference.abs();
            }
            _ => {}
        }
        o[e * nq..(e + 1) * nq].fill(normal_value);
    }
    Ok(())
}

/// Barycentre with upstream `Math::barycenter` order (`+=` per node from
/// zero, then `/= nb_nodes`).
fn barycenter(mesh: &Mesh, el: Element, out: &mut [Real]) -> Result<()> {
    let conn = mesh.element_connectivity(el)?;
    let sdim = mesh.spatial_dimension();
    let got = out.len();
    let out = out.get_mut(..sdim).ok_or(Error::DimensionMismatch {
        context: "barycenter: output",
        expected: sdim,
        got,
    })?;
    out.fill(0.0);
    let x = mesh.nodes();
    let x = x.as_slice();
    for &n in &conn {
        let n = akantu_core::try_to_usize(n)?;
        let xn = x.get(n * sdim..(n + 1) * sdim).ok_or(Error::OutOfRange {
            what: "barycenter: connectivity node",
            index: n,
            len: mesh.nb_nodes(),
        })?;
        for (o, v) in out.iter_mut().zip(xn) {
            *o += v;
        }
    }
    #[allow(clippy::cast_precision_loss)] // node counts are tiny
    let c = conn.len() as Real;
    for v in out.iter_mut() {
        *v /= c;
    }
    Ok(())
}

fn cohesive_normals_1d(
    mesh: &Mesh,
    block: &Block<'_>,
    ids: &[usize],
    nq: usize,
    out: &mut Array<Real>,
) -> Result<()> {
    let facets_mesh = mesh
        .mesh_facets()
        .ok_or_else(|| Error::Mesh("1-D cohesive normals need the mesh of facets".into()))?;
    let facet_type = block
        .ty
        .facet_type(0)
        .ok_or(Error::UnsupportedElementType {
            ty: block.ty,
            context: "cohesive normals",
        })?;
    let facets = element_refs(facets_mesh, "subelement_to_element", block.ty, block.ghost)?;
    let segments = element_lists(
        facets_mesh,
        "element_to_subelement",
        facet_type,
        block.ghost,
    )?;
    let o = out.as_mut_slice();
    let mut b = [[0.0; 1]; 2];
    for (pos, &e) in ids.iter().enumerate() {
        let f = facets.get_tuple(e)?;
        if f.len() < 2 {
            return Err(Error::Mesh(format!(
                "1-D cohesive normals: subelement_to_element has {} facets per cohesive \
                 element, 2 needed",
                f.len()
            )));
        }
        for p in 0..2 {
            let facet = f[p];
            let fi = facet
                .try_idx()
                .ok_or_else(|| Error::Mesh("null facet of a cohesive element".into()))?;
            let segs = segments
                .get_tuple(fi)?
                .first()
                .ok_or_else(|| Error::invalid("missing element_to_subelement entry"))?;
            let seg = *segs
                .first()
                .ok_or_else(|| Error::Mesh("facet without adjacent segment".into()))?;
            barycenter(mesh, seg, &mut b[p])?;
        }
        let difference = b[0][0] - b[1][0];
        if difference == 0.0 {
            return Err(Error::Mesh(
                "Error in normal computation for cohesive elements".into(),
            ));
        }
        o[pos * nq..(pos + 1) * nq].fill(difference / difference.abs());
    }
    Ok(())
}
