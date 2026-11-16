// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/fe_engine/shape_functions.cc
// (initElementalFieldInterpolationFromIntegrationPoints),
// shape_functions_inline_impl.hh (buildInterpolationMatrix,
// BuildElementalFieldInterpolationMatrix, interpolateElementalFieldFromIntegrationPoints),
// fe_engine_template_tmpl.hh (initElementalFieldInterpolationFromIntegrationPoints).

//! Interpolation of elemental (quadrature-point) fields at arbitrary
//! physical points (FE-025; upstream `Material::interpolateStress`).
//!
//! Per element, a polynomial is fitted through the quadrature-point values
//! in **physical** coordinates — basis `[1]` (order 1), `[1, x, y, z]`
//! (order 2), `[1, x, y, xy]` (quadrangles) — by inverting the fixed
//! `nq × nq` matrix of the basis at the quadrature points (Eigen closed-form
//! inverse, `nq ≤ 4`), then evaluated at the interpolation points.
//!
//! Supported types are those for which upstream's matrices are well formed:
//! `_point_1`, `_segment_2/3`, `_triangle_3/6`, `_quadrangle_4/8`,
//! `_tetrahedron_4/10`. Upstream reads out of bounds (`_hexahedron_8`,
//! `_pentahedron_15`), inverts uninitialised memory (`_pentahedron_6`) or
//! throws `AKANTU_TO_IMPLEMENT` (`_hexahedron_20`) for the others; the port
//! returns `Error::NotImplemented` for them.
//!
//! Deviations (documented): the matrices are returned to the caller
//! ([`ElementalFieldInterpolation`], upstream stores them in the material),
//! and a missing element filter means "all elements" (upstream iterates
//! over an empty filter and computes nothing, UD-096).
//!
//! **Reuse contract.** An [`ElementalFieldInterpolation`] is bound to the
//! ordered element ids (per `(type, ghost)`) it was built for and to the
//! interpolation points of those elements at that time. Interpolation maps
//! every requested element id to the matrices built for that id, so a
//! different (subset, reordered) filter is valid; an id that was not part
//! of the initialisation is an error, never another element's matrices.
//! Upstream rebuilds the matrices whenever the material filter changes.
//! The object is also bound to the **mesh revision** it was built at
//! ([`ElementalFieldInterpolation::mesh_revision`]): element ids alone
//! cannot tell that an element was renumbered (same count) or that nodes
//! moved, and every such change bumps the revision (`MeshAccessor`,
//! committed `SharedMesh` mutations). Interpolating with an object built at
//! another revision than the supplied mesh's is an error, raised before any
//! output is written: rebuild it with
//! [`FeEngine::init_elemental_field_interpolation`] after a mesh change.

use akantu_core::tensor::{Matrix, SquareMatrixExt};
use akantu_core::{
    Array, ElementType, ElementTypeMapArray, Error, GhostType, Idx, Real, Result,
    SharedElementTypeMapArray, TypeFilter,
};
use akantu_mesh::Mesh;

use super::{FeEngine, element_ids, resolve_filter};
use crate::eigen::{self, Dims};

/// Matrices of the elemental-field interpolation (upstream
/// `interpolation_points_matrices` and `interpolation_inverse_coordinates`
/// of the material), per `(type, ghost)` and initialised element, bound to
/// the ids of those elements (see the module's reuse contract).
#[derive(Clone, Debug)]
pub struct ElementalFieldInterpolation {
    /// `nip × nq` basis values at the interpolation points (column-major),
    /// one tuple per initialised element (slot).
    interpolation_points_matrices: ElementTypeMapArray<Real>,
    /// `nq × nq` inverse of the basis matrix at the quadrature points
    /// (column-major), one tuple per initialised element (slot).
    quad_points_inv_matrices: ElementTypeMapArray<Real>,
    /// Slot layout per `(type, ghost)`.
    blocks: Vec<InterpolationBlock>,
    /// Mesh revision at initialisation.
    mesh_revision: u64,
}

/// Slot layout of one `(type, ghost)` block.
#[derive(Clone, Debug)]
struct InterpolationBlock {
    ty: ElementType,
    ghost: GhostType,
    /// Interpolation points per element.
    nip: usize,
    /// Quadrature points per element.
    nq: usize,
    /// Element id of every slot (the initialisation filter, in order).
    elements: Vec<usize>,
}

impl InterpolationBlock {
    /// Slot of every requested element id (`ids`): the identity when `ids`
    /// is the initialisation filter, otherwise looked up by id (first slot
    /// of a duplicated id; its matrices are identical). Ids absent from the
    /// initialisation give an error.
    fn slots(&self, ids: &[usize]) -> Result<Vec<usize>> {
        if ids == self.elements.as_slice() {
            return Ok((0..ids.len()).collect());
        }
        let len = self.elements.iter().max().map_or(0, |&m| m + 1);
        let mut slot_of = vec![usize::MAX; len];
        for (slot, &e) in self.elements.iter().enumerate().rev() {
            slot_of[e] = slot;
        }
        ids.iter()
            .map(|&e| match slot_of.get(e) {
                Some(&slot) if slot != usize::MAX => Ok(slot),
                _ => Err(Error::invalid(format!(
                    "interpolate_elemental_field: element {e} of {} ({}) was not part of the \
                     elemental interpolation initialisation (re-initialise for this filter)",
                    self.ty.name(),
                    self.ghost.name()
                ))),
            })
            .collect()
    }
}

impl ElementalFieldInterpolation {
    fn block(&self, ty: ElementType, ghost: GhostType) -> Option<&InterpolationBlock> {
        self.blocks.iter().find(|b| b.ty == ty && b.ghost == ghost)
    }

    /// Interpolation points per element of `(ty, ghost)`.
    #[must_use]
    pub fn nb_interpolation_points(&self, ty: ElementType, ghost: GhostType) -> Option<usize> {
        self.block(ty, ghost).map(|b| b.nip)
    }

    /// Element ids the matrices of `(ty, ghost)` were built for, in slot
    /// order (the initialisation filter, or all elements).
    #[must_use]
    pub fn elements(&self, ty: ElementType, ghost: GhostType) -> Option<&[usize]> {
        self.block(ty, ghost).map(|b| b.elements.as_slice())
    }

    /// `nip × nq` basis matrices at the interpolation points, one tuple per
    /// slot of [`ElementalFieldInterpolation::elements`].
    #[must_use]
    pub fn interpolation_points_matrices(&self) -> &ElementTypeMapArray<Real> {
        &self.interpolation_points_matrices
    }

    /// `nq × nq` inverse basis matrices at the quadrature points, one tuple
    /// per slot of [`ElementalFieldInterpolation::elements`].
    #[must_use]
    pub fn quad_points_inv_matrices(&self) -> &ElementTypeMapArray<Real> {
        &self.quad_points_inv_matrices
    }

    /// Topology revision of the mesh the matrices were built on.
    #[must_use]
    pub fn mesh_revision(&self) -> u64 {
        self.mesh_revision
    }

    /// Accept a newer revision of `mesh` without rebuilding, after a
    /// change that provably keeps every interpolated element's geometry
    /// and index: cohesive insertion only appends elements and
    /// reconnects bulk elements to **duplicated nodes with identical
    /// coordinates**, so the matrices built before are those a rebuild
    /// would produce, bit for bit (upstream builds them once in
    /// `initStressInterpolation` and reuses them after every insertion).
    /// The check is the caller's responsibility; this verifies only that
    /// every element block the matrices were built for still exists with
    /// at least as many elements.
    ///
    /// # Errors
    /// A built block whose element count shrank or whose element ids are
    /// out of range in `mesh`.
    pub fn accept_revision_unchanged_geometry(&mut self, mesh: &Mesh) -> Result<()> {
        for b in &self.blocks {
            let n = mesh.nb_element(b.ty, b.ghost);
            if b.elements.iter().any(|&e| e >= n) {
                return Err(Error::invalid(format!(
                    "elemental interpolation: {} ({}) has {n} elements, fewer than at \
                     initialisation; rebuild it",
                    b.ty.name(),
                    b.ghost.name()
                )));
            }
        }
        self.mesh_revision = mesh.revision();
        Ok(())
    }

    /// Stored matrices of a block, validated against its slot layout
    /// (`(inverse matrices, interpolation-point matrices)`).
    fn matrices(&self, b: &InterpolationBlock) -> Result<(&[Real], &[Real])> {
        let inv = self.quad_points_inv_matrices.get(b.ty, b.ghost)?;
        let itp = self.interpolation_points_matrices.get(b.ty, b.ghost)?;
        let slots = b.elements.len();
        super::check_input(
            "interpolate_elemental_field: stored inverse quadrature-point matrices",
            inv.len(),
            inv.nb_component(),
            slots,
            b.nq * b.nq,
        )?;
        super::check_input(
            "interpolate_elemental_field: stored interpolation-point matrices",
            itp.len(),
            itp.nb_component(),
            slots,
            b.nip * b.nq,
        )?;
        Ok((inv.as_slice(), itp.as_slice()))
    }
}

/// Basis of the fit for `ty` (upstream `BuildElementalFieldInterpolationMatrix`
/// with the default order = polynomial degree): writes row `i` of `m`
/// (`rows × nq`, column-major) for the point with physical coordinates `x`.
fn basis_row(ty: ElementType, x: &[Real], m: &mut [Real], rows: usize, i: usize, nq: usize) {
    match ty {
        ElementType::Quadrangle4 | ElementType::Quadrangle8 => {
            m[i] = 1.;
            m[i + rows] = x[0];
            m[i + 2 * rows] = x[1];
            m[i + 3 * rows] = x[0] * x[1];
        }
        _ => {
            m[i] = 1.;
            // order 2: coordMatrix(i, j) = coordinates(j - 1, i); order 1 (nq = 1): nothing else
            for j in 1..nq {
                m[i + j * rows] = x[j - 1];
            }
        }
    }
}

/// `true` if upstream's construction is well defined for `ty`.
fn supported(ty: ElementType) -> bool {
    matches!(
        ty,
        ElementType::Point1
            | ElementType::Segment2
            | ElementType::Segment3
            | ElementType::Triangle3
            | ElementType::Triangle6
            | ElementType::Quadrangle4
            | ElementType::Quadrangle8
            | ElementType::Tetrahedron4
            | ElementType::Tetrahedron10
    )
}

/// Fixed-size Eigen inverse of the `n × n` column-major matrix (closed forms
/// for `n ≤ 4`).
fn fixed_inverse(n: usize, m: &[Real], out: &mut [Real]) -> Result<()> {
    fn inv<const N: usize>(m: &[Real], out: &mut [Real]) {
        let mut a: Matrix<N, N> = [[0.0; N]; N];
        for (j, col) in a.iter_mut().enumerate() {
            col.copy_from_slice(&m[j * N..(j + 1) * N]);
        }
        let r = a.inverse_unchecked();
        for (j, col) in r.iter().enumerate() {
            out[j * N..(j + 1) * N].copy_from_slice(col);
        }
    }
    match n {
        1 => inv::<1>(m, out),
        2 => inv::<2>(m, out),
        3 => inv::<3>(m, out),
        4 => inv::<4>(m, out),
        _ => {
            return Err(Error::NotImplemented(
                "elemental field interpolation with more than 4 quadrature points",
            ));
        }
    }
    Ok(())
}

impl FeEngine {
    /// Precompute the interpolation of elemental fields from the integration
    /// points to `points` (upstream
    /// `initElementalFieldInterpolationFromIntegrationPoints`): `points`
    /// holds, per `(type, ghost)`, `nb_element · nip` physical points (all
    /// elements of the type); `filter` selects the elements (material
    /// filter). Requires the default integration order for every handled
    /// type (upstream uses `GaussIntegrationElement<type>` there).
    pub fn init_elemental_field_interpolation(
        &self,
        mesh: &Mesh,
        points: &ElementTypeMapArray<Real>,
        filter: Option<&ElementTypeMapArray<Idx>>,
    ) -> Result<ElementalFieldInterpolation> {
        let sdim = mesh.spatial_dimension();
        let mut res = ElementalFieldInterpolation {
            interpolation_points_matrices: ElementTypeMapArray::new(format!(
                "{}:interpolation_points_matrices",
                self.id
            )),
            quad_points_inv_matrices: ElementTypeMapArray::new(format!(
                "{}:quad_points_inv_matrices",
                self.id
            )),
            blocks: Vec::new(),
            mesh_revision: mesh.revision(),
        };
        for ghost in GhostType::ALL {
            let tf = TypeFilter {
                spatial_dimension: Some(sdim),
                ghost_type: ghost,
                kind: Some(akantu_core::ElementKind::Regular),
            };
            let types: Vec<ElementType> = match filter {
                Some(f) => f.element_types(tf).collect(),
                None => mesh.element_types(tf).collect(),
            };
            for ty in types {
                let nb_element = mesh.nb_element(ty, ghost);
                if nb_element == 0 {
                    continue;
                }
                if !supported(ty) {
                    return Err(Error::NotImplemented(
                        "elemental field interpolation for this element type (undefined or \
                         AKANTU_TO_IMPLEMENT upstream)",
                    ));
                }
                let nq = crate::quadrature::nb_quadrature_points(ty, ty.polynomial_degree())?;
                if self.nb_integration_points(ty, ghost)? != nq {
                    return Err(Error::Unsupported(format!(
                        "elemental field interpolation needs the default integration order of {}",
                        ty.name()
                    )));
                }
                let pts = points.get(ty, ghost)?;
                if pts.nb_component() != sdim {
                    return Err(Error::DimensionMismatch {
                        context: "init_elemental_field_interpolation: interpolation point \
                                  components",
                        expected: sdim,
                        got: pts.nb_component(),
                    });
                }
                if pts.len() % nb_element != 0 {
                    return Err(Error::DimensionMismatch {
                        context: "init_elemental_field_interpolation: interpolation points \
                                  (a multiple of the number of elements of the type)",
                        expected: nb_element,
                        got: pts.len(),
                    });
                }
                let nip = pts.len() / nb_element;
                let ef: Option<Vec<Idx>> = match filter {
                    Some(f) => Some(f.get(ty, ghost)?.as_slice().to_vec()),
                    None => None,
                };
                let ids = element_ids(resolve_filter(ef.as_deref(), nb_element)?, nb_element);
                // quadrature point coordinates of the filtered elements
                let mut qc = akantu_core::Array::new(sdim);
                self.integration_points_coordinates(mesh, &mut qc, ty, ghost, ef.as_deref())?;
                let mut invs = vec![0.0; ids.len() * nq * nq];
                let mut itps = vec![0.0; ids.len() * nip * nq];
                let mut qm = vec![0.0; nq * nq];
                for (pos, &e) in ids.iter().enumerate() {
                    for q in 0..nq {
                        let x = qc.tuple(pos * nq + q);
                        basis_row(ty, x, &mut qm, nq, q, nq);
                    }
                    fixed_inverse(nq, &qm, &mut invs[pos * nq * nq..(pos + 1) * nq * nq])?;
                    let m = &mut itps[pos * nip * nq..(pos + 1) * nip * nq];
                    for i in 0..nip {
                        let x = pts.tuple(e * nip + i);
                        basis_row(ty, x, m, nip, i, nq);
                    }
                }
                res.quad_points_inv_matrices
                    .alloc(ids.len(), nq * nq, ty, ghost, 0.0)?
                    .as_mut_slice()
                    .copy_from_slice(&invs);
                res.interpolation_points_matrices
                    .alloc(ids.len(), nip * nq, ty, ghost, 0.0)?
                    .as_mut_slice()
                    .copy_from_slice(&itps);
                res.blocks.push(InterpolationBlock {
                    ty,
                    ghost,
                    nip,
                    nq,
                    elements: ids,
                });
            }
        }
        Ok(res)
    }

    /// Interpolate an elemental field from the integration points to the
    /// points given at initialisation (upstream
    /// `interpolateElementalFieldFromIntegrationPoints`): `field` has, per
    /// type, `filter.len() · nq` tuples (a material internal, in filter
    /// order); `out` gets `nb_element · nip` tuples per type (allocated if
    /// missing), written for the filtered elements only. Each filtered
    /// element uses the matrices built for that element id at
    /// initialisation (any subset or order of the initialised elements);
    /// an element that was not initialised is an error, and so is
    /// an `interpolation` built at another mesh revision than `mesh`'s
    /// (nothing is written).
    pub fn interpolate_elemental_field(
        &self,
        mesh: &Mesh,
        interpolation: &ElementalFieldInterpolation,
        field: &ElementTypeMapArray<Real>,
        out: &mut ElementTypeMapArray<Real>,
        ghost: GhostType,
        filter: Option<&ElementTypeMapArray<Idx>>,
    ) -> Result<()> {
        self.interpolate_elemental_field_with(
            mesh,
            interpolation,
            |ty, g| field.get(ty, g),
            out,
            ghost,
            filter,
        )
    }

    /// [`FeEngine::interpolate_elemental_field`] of an exportable field
    /// (e.g. a law's `stress` internal, D-047): each `(type, ghost)` block
    /// of `field` is read through **one** guard, held while that block is
    /// interpolated, instead of a copy of the whole map
    /// (`SharedElementTypeMapArray::to_plain`) per call. Same values, same
    /// checks and errors (a missing block gives the same `NotFound`); only
    /// the blocks the interpolation reads are borrowed.
    pub fn interpolate_elemental_field_shared(
        &self,
        mesh: &Mesh,
        interpolation: &ElementalFieldInterpolation,
        field: &SharedElementTypeMapArray<Real>,
        out: &mut ElementTypeMapArray<Real>,
        ghost: GhostType,
        filter: Option<&ElementTypeMapArray<Idx>>,
    ) -> Result<()> {
        self.interpolate_elemental_field_with(
            mesh,
            interpolation,
            |ty, g| field.get(ty, g),
            out,
            ghost,
            filter,
        )
    }

    /// Body of the two entry points above: `field(ty, ghost)` gives the
    /// block of `(ty, ghost)` (a reference or a guard), taken once per type.
    fn interpolate_elemental_field_with<A>(
        &self,
        mesh: &Mesh,
        interpolation: &ElementalFieldInterpolation,
        field: impl Fn(ElementType, GhostType) -> Result<A>,
        out: &mut ElementTypeMapArray<Real>,
        ghost: GhostType,
        filter: Option<&ElementTypeMapArray<Idx>>,
    ) -> Result<()>
    where
        A: std::ops::Deref<Target = Array<Real>>,
    {
        if interpolation.mesh_revision != mesh.revision() {
            return Err(Error::invalid(format!(
                "interpolate_elemental_field: the elemental interpolation was built at mesh \
                 revision {} but the mesh is at revision {} (nodes or elements changed since); \
                 rebuild it with init_elemental_field_interpolation",
                interpolation.mesh_revision,
                mesh.revision()
            )));
        }
        let sdim = mesh.spatial_dimension();
        let tf = TypeFilter {
            spatial_dimension: Some(sdim),
            ghost_type: ghost,
            kind: Some(akantu_core::ElementKind::Regular),
        };
        let types: Vec<ElementType> = match filter {
            Some(f) => f.element_types(tf).collect(),
            None => mesh.element_types(tf).collect(),
        };
        for ty in types {
            let nb_element = mesh.nb_element(ty, ghost);
            if nb_element == 0 {
                continue;
            }
            let b = interpolation.block(ty, ghost).ok_or_else(|| {
                Error::invalid(format!(
                    "elemental interpolation not initialised for {} ({})",
                    ty.name(),
                    ghost.name()
                ))
            })?;
            let (nq, nip) = (b.nq, b.nip);
            let ef: Option<Vec<Idx>> = match filter {
                Some(f) => Some(f.get(ty, ghost)?.as_slice().to_vec()),
                None => None,
            };
            let ids = element_ids(resolve_filter(ef.as_deref(), nb_element)?, nb_element);
            let slots = b.slots(&ids)?;
            let (inv, itp) = interpolation.matrices(b)?;
            let f = field(ty, ghost)?;
            let nbc = f.nb_component();
            if f.len() != ids.len() * nq {
                return Err(Error::DimensionMismatch {
                    context: "interpolate_elemental_field: field",
                    expected: ids.len() * nq,
                    got: f.len(),
                });
            }
            if !out.exists(ty, ghost) {
                out.alloc(nb_element * nip, nbc, ty, ghost, 0.0)?;
            }
            let o = out.get_mut(ty, ghost)?;
            if o.nb_component() != nbc || o.len() < nb_element * nip {
                return Err(Error::DimensionMismatch {
                    context: "interpolate_elemental_field: result",
                    expected: nb_element * nip,
                    got: o.len(),
                });
            }
            let o = o.as_mut_slice();
            let fs = f.as_slice();
            let mut coeff = vec![0.0; nq * nbc];
            // coefficients = inv_quad_coord (nq × nq) · fieldᵀ (nq × nbc)
            let p_coeff = eigen::prepare(Dims::dynamic(nq, nq), Dims::dynamic(nbc, nq).t())?;
            // result(el) = coefficientsᵀ (nbc × nq) · itp_coordᵀ (nq × nip)
            let p_res = eigen::prepare(Dims::dynamic(nq, nbc).t(), Dims::dynamic(nip, nq).t())?;
            for (pos, (&e, &slot)) in ids.iter().zip(&slots).enumerate() {
                let fe = &fs[pos * nq * nbc..(pos + 1) * nq * nbc];
                let ie = &inv[slot * nq * nq..(slot + 1) * nq * nq];
                let me = &itp[slot * nip * nq..(slot + 1) * nip * nq];
                p_coeff.eval(ie, fe, &mut coeff)?;
                let r = &mut o[e * nip * nbc..(e + 1) * nip * nbc];
                p_res.eval(&coeff, me, r)?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shape::ShapeFamily;
    use akantu_core::ElementKind;
    use akantu_mesh::MeshAccessor;

    const G: GhostType = GhostType::NotGhost;
    const S3: ElementType = ElementType::Segment3;

    /// Malformed stored matrices (wrong slot or component counts)
    /// are detected before any slicing.
    #[test]
    fn malformed_matrix_storage_is_an_error() {
        let mut mesh = Mesh::new(1, "seg3").unwrap();
        {
            let mut acc = MeshAccessor::new(&mut mesh);
            for x in [0.0, 1.0, 0.5, 2.0, 4.0, 3.0] {
                acc.push_node(&[x]).unwrap();
            }
            let mut c = acc.connectivity_mut(S3, G);
            c.push_tuple(&[0, 1, 2]);
            c.push_tuple(&[3, 4, 5]);
        }
        let fe = FeEngine::with_precomputation(
            &mesh,
            None,
            ElementKind::Regular,
            ShapeFamily::Lagrange,
            "fe",
        )
        .unwrap();
        let mut points = ElementTypeMapArray::new("points");
        points.alloc(2, 1, S3, G, 0.5).unwrap();
        let good = fe
            .init_elemental_field_interpolation(&mesh, &points, None)
            .unwrap();
        let mut field = ElementTypeMapArray::new("field");
        field.alloc(4, 1, S3, G, 1.0).unwrap();
        let interpolate = |interp: &ElementalFieldInterpolation| {
            let mut out = ElementTypeMapArray::new("out");
            fe.interpolate_elemental_field(&mesh, interp, &field, &mut out, G, None)
        };
        interpolate(&good).unwrap();
        // one slot missing in the inverse matrices
        let mut bad = good.clone();
        bad.quad_points_inv_matrices
            .get_mut(S3, G)
            .unwrap()
            .truncate(1);
        assert!(matches!(
            interpolate(&bad),
            Err(Error::DimensionMismatch { .. })
        ));
        // wrong component count of the interpolation-point matrices
        let mut bad = good.clone();
        bad.interpolation_points_matrices.as_map_mut().remove(S3, G);
        bad.interpolation_points_matrices
            .alloc(2, 3, S3, G, 0.0)
            .unwrap();
        assert!(matches!(
            interpolate(&bad),
            Err(Error::DimensionMismatch { .. })
        ));
        // matrices missing altogether
        let mut bad = good.clone();
        bad.quad_points_inv_matrices.as_map_mut().remove(S3, G);
        assert!(interpolate(&bad).is_err());
        // slot layout claims an element that the matrices do not cover
        let mut bad = good;
        bad.blocks[0].elements.push(1);
        assert!(matches!(
            interpolate(&bad),
            Err(Error::DimensionMismatch { .. })
        ));
    }
}
