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

    /// The [`PointInterpolation`] of `(ty, ghost)` for fields of `nbc`
    /// components, when the block's elements have one quadrature point and
    /// both products of [`FeEngine::interpolate_elemental_field`] take the
    /// coefficient-based route or the depth-1 `gebp` route (`ProductForm`);
    /// `None` otherwise (no block, other sizes).
    ///
    /// # Errors
    /// Stored matrices inconsistent with the block's slots.
    pub fn point_interpolation(
        &self,
        ty: ElementType,
        ghost: GhostType,
        nbc: usize,
    ) -> Result<Option<PointInterpolation<'_>>> {
        let Some(b) = self.block(ty, ghost) else {
            return Ok(None);
        };
        if b.nq != 1 {
            return Ok(None);
        }
        let Some(kernel) = CoeffInterpolation::new(1, b.nip, nbc) else {
            return Ok(None);
        };
        let (inv, itp) = self.matrices(b)?;
        Ok(Some(PointInterpolation {
            inv,
            itp,
            nip: b.nip,
            block: b,
            kernel,
        }))
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

/// The interpolation of one `(type, ghost)` block whose elements have **one**
/// quadrature point and whose two products are evaluated in a form the
/// batch kernel reproduces
/// ([`ElementalFieldInterpolation::point_interpolation`]; e.g. the linear
/// tetrahedron and triangle): the value at interpolation point `i` of the
/// element in slot `s` is `(inv(s)·f)·itp(s, i)` per component, with the
/// sign rule of each product's route for a zero (`ProductForm`): the
/// operations [`FeEngine::interpolate_elemental_field`] performs for that
/// block (`CoeffInterpolation`). A caller that needs
/// the values of a few points of each element in another order (the cohesive
/// model's facet stresses) computes them here without the per-element output
/// array.
#[derive(Clone, Copy, Debug)]
pub struct PointInterpolation<'a> {
    inv: &'a [Real],
    itp: &'a [Real],
    nip: usize,
    block: &'a InterpolationBlock,
    kernel: CoeffInterpolation,
}

impl PointInterpolation<'_> {
    /// Interpolation points per element.
    #[must_use]
    pub fn nb_interpolation_points(&self) -> usize {
        self.nip
    }

    /// Number of slots (initialised elements).
    #[must_use]
    pub fn nb_slots(&self) -> usize {
        self.block.elements.len()
    }

    /// The slot [`FeEngine::interpolate_elemental_field`] uses for each
    /// element id of `ids` (a filter in its order): the identity when `ids`
    /// is the initialisation filter, otherwise the first slot of the id.
    ///
    /// # Errors
    /// An id that was not part of the initialisation (the error of
    /// `interpolate_elemental_field`).
    pub fn slots(&self, ids: &[usize]) -> Result<Vec<usize>> {
        self.block.slots(ids)
    }

    /// `out[c] = (inv(slot)·field[c])·itp(slot, point)` for every component
    /// `c < out.len()` (`field` at least as long; each product with its
    /// route's form, see the type's documentation): the value
    /// [`FeEngine::interpolate_elemental_field`] writes at interpolation
    /// point `point` of an element in slot `slot` whose quadrature-point
    /// tuple is `field`. Equal to [`PointInterpolation::coefficients`]
    /// followed by [`PointInterpolation::value_from_coefficients`].
    ///
    /// # Panics
    /// `slot ≥ nb_slots()`, `point ≥ nb_interpolation_points()` or `field`
    /// shorter than `out`.
    #[inline]
    pub fn value(&self, slot: usize, point: usize, field: &[Real], out: &mut [Real]) {
        assert!(point < self.nip, "PointInterpolation::value: point {point}");
        let a = self.inv[slot];
        let m = self.itp[slot * self.nip + point];
        let field = &field[..out.len()];
        let (f1, f2) = (self.kernel.first, self.kernel.second);
        for (x, &f) in out.iter_mut().zip(field) {
            *x = f2.one(f1.one(a * f) * m);
        }
    }

    /// The first product of an element in slot `slot` (its coefficients,
    /// shared by all its interpolation points): `coeff[c] = inv(slot)·field[c]`
    /// for `c < coeff.len()`, with the first product's form.
    ///
    /// # Panics
    /// `slot ≥ nb_slots()` or `field` shorter than `coeff`.
    #[inline]
    pub fn coefficients(&self, slot: usize, field: &[Real], coeff: &mut [Real]) {
        let a = self.inv[slot];
        let f1 = self.kernel.first;
        let field = &field[..coeff.len()];
        for (c, &f) in coeff.iter_mut().zip(field) {
            *c = f1.one(a * f);
        }
    }

    /// The second product at interpolation point `point` of the element in
    /// slot `slot`, from its [`PointInterpolation::coefficients`]:
    /// `out[c] = coeff[c]·itp(slot, point)` for `c < out.len()`, with the
    /// second product's form.
    ///
    /// # Panics
    /// `slot ≥ nb_slots()`, `point ≥ nb_interpolation_points()` or `coeff`
    /// shorter than `out`.
    #[inline]
    pub fn value_from_coefficients(
        &self,
        coeff: &[Real],
        slot: usize,
        point: usize,
        out: &mut [Real],
    ) {
        assert!(
            point < self.nip,
            "PointInterpolation::value_from_coefficients: point {point}"
        );
        let m = self.itp[slot * self.nip + point];
        let f2 = self.kernel.second;
        let coeff = &coeff[..out.len()];
        for (x, &c) in out.iter_mut().zip(coeff) {
            *x = f2.one(c * m);
        }
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

/// Operand descriptions of the two per-element products of
/// [`FeEngine::interpolate_elemental_field`] (upstream
/// `interpolateElementalFieldFromIntegrationPoints`): `coefficients =
/// inv_quad_coord · fieldᵀ` and `result = coefficientsᵀ · itp_coordᵀ`, all
/// dynamic Eigen matrices.
fn interpolation_dims(nq: usize, nip: usize, nbc: usize) -> [(Dims, Dims); 2] {
    [
        (Dims::dynamic(nq, nq), Dims::dynamic(nbc, nq).t()),
        (Dims::dynamic(nq, nbc).t(), Dims::dynamic(nip, nq).t()),
    ]
}

/// How Eigen evaluates one of the products of [`interpolation_dims`], for
/// the routes the batch kernels reproduce (`akantu_core::eigen_gebp`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ProductForm {
    /// `Route::Coeff`: every coefficient is the reduction of its depth
    /// terms in the given order (one term: the term itself).
    Coeff(akantu_core::eigen_gebp::Reduction),
    /// `Route::Gebp` of depth 1 (`gebp_into`): the destination is zeroed,
    /// the accumulators start at `+0` (no peeled pair of terms below the
    /// packet depth), so every coefficient is `+0 + (+0 + t)` — the term
    /// with a negative zero turned positive.
    GebpDepth1,
}

impl ProductForm {
    fn of(route: akantu_core::eigen_gebp::Route, depth: usize) -> Option<Self> {
        use akantu_core::eigen_gebp::{GEBP_PK, Route};
        match route {
            Route::Coeff(order) => Some(Self::Coeff(order)),
            Route::Gebp { kc } if depth == 1 && kc >= 1 && GEBP_PK >= 2 => Some(Self::GebpDepth1),
            _ => None,
        }
    }

    /// The coefficient of a depth-1 product whose single term is `t`.
    #[inline]
    fn one(self, t: Real) -> Real {
        match self {
            Self::Coeff(_) => t,
            // c = (0 + 0) + t accumulated, then r = 0 + c
            Self::GebpDepth1 => 0.0 + ((0.0 + 0.0) + t),
        }
    }
}

/// Batch form of the two products of [`interpolation_dims`]: the routes are
/// selected once per block with [`eigen::product_route`] on the same operand
/// descriptions, and when both are forms this kernel reproduces
/// ([`ProductForm`]: coefficient-based products, and the depth-1 `gebp`
/// product of one-point elements, e.g. the linear tetrahedron's stresses at
/// its facets' three points each: `nq = 1`, `nip = 12`, `nbc = 9`), every
/// coefficient is computed with the same operations in the same order as
/// [`eigen::product_slices`], without the per-product route selection,
/// shape checks and closures. Bitwise equal to the general path (property
/// test below).
#[derive(Clone, Copy, Debug)]
struct CoeffInterpolation {
    nq: usize,
    nip: usize,
    nbc: usize,
    first: ProductForm,
    second: ProductForm,
}

impl CoeffInterpolation {
    /// `None` unless both products take a route of [`ProductForm`] (the
    /// `gebp` form only for one quadrature point; for more, both must be
    /// coefficient-based).
    fn new(nq: usize, nip: usize, nbc: usize) -> Option<Self> {
        let [(a1, b1), (a2, b2)] = interpolation_dims(nq, nip, nbc);
        let first = ProductForm::of(eigen::product_route(a1, b1), nq)?;
        let second = ProductForm::of(eigen::product_route(a2, b2), nq)?;
        Some(Self {
            nq,
            nip,
            nbc,
            first,
            second,
        })
    }

    /// One element: `inv` (`nq × nq`, column-major), `field` (`nq` tuples of
    /// `nbc`), `itp` (`nip × nq`, column-major) → `coeff` (`nq × nbc`,
    /// column-major, scratch) and `out` (`nip` tuples of `nbc`). The slices
    /// have exactly these lengths (callers slice them per element).
    #[inline]
    fn apply(
        &self,
        inv: &[Real],
        field: &[Real],
        itp: &[Real],
        coeff: &mut [Real],
        out: &mut [Real],
    ) {
        let (nq, nip, nbc) = (self.nq, self.nip, self.nbc);
        if nq == 1 {
            // depth-1 products: one term per coefficient
            let (f1, f2) = (self.first, self.second);
            let a = inv[0];
            for (c, &f) in coeff[..nbc].iter_mut().zip(&field[..nbc]) {
                *c = f1.one(a * f);
            }
            for (j, r) in out[..nip * nbc].chunks_exact_mut(nbc).enumerate() {
                let m = itp[j];
                for (x, &c) in r.iter_mut().zip(&coeff[..nbc]) {
                    *x = f2.one(c * m);
                }
            }
            return;
        }
        let (ProductForm::Coeff(first), ProductForm::Coeff(second)) = (self.first, self.second)
        else {
            unreachable!("CoeffInterpolation::new admits gebp forms for nq = 1 only");
        };
        // coefficients(i, j) = Σ_l inv(i, l)·field(j, l)
        for j in 0..nbc {
            for i in 0..nq {
                coeff[i + j * nq] = first.sum(nq, &|l| inv[i + l * nq] * field[l * nbc + j]);
            }
        }
        // result(i, j) = Σ_l coefficients(l, i)·itp(j, l)
        for j in 0..nip {
            for i in 0..nbc {
                out[i + j * nbc] = second.sum(nq, &|l| coeff[i * nq + l] * itp[j + l * nip]);
            }
        }
    }
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
            // Threading candidate (independent outputs): each element writes
            // only its own `nip` tuples (duplicated filter ids write the same
            // tuples: chunks must then keep the filter order of those ids).
            if let Some(kernel) = CoeffInterpolation::new(nq, nip, nbc) {
                // the general loop below with both routes selected once
                // (same operations in the same order, `CoeffInterpolation`)
                for (pos, (&e, &slot)) in ids.iter().zip(&slots).enumerate() {
                    kernel.apply(
                        &inv[slot * nq * nq..(slot + 1) * nq * nq],
                        &fs[pos * nq * nbc..(pos + 1) * nq * nbc],
                        &itp[slot * nip * nq..(slot + 1) * nip * nq],
                        &mut coeff,
                        &mut o[e * nip * nbc..(e + 1) * nip * nbc],
                    );
                }
                continue;
            }
            for (pos, (&e, &slot)) in ids.iter().zip(&slots).enumerate() {
                let fe = &fs[pos * nq * nbc..(pos + 1) * nq * nbc];
                let ie = &inv[slot * nq * nq..(slot + 1) * nq * nq];
                let me = &itp[slot * nip * nq..(slot + 1) * nip * nq];
                let [(a1, b1), (a2, b2)] = interpolation_dims(nq, nip, nbc);
                // coefficients = inv_quad_coord (nq × nq) · fieldᵀ (nq × nbc)
                eigen::product_slices(a1, b1, ie, fe, &mut coeff)?;
                // result(el) = coefficientsᵀ (nbc × nq) · itp_coordᵀ (nq × nip)
                let r = &mut o[e * nip * nbc..(e + 1) * nip * nbc];
                eigen::product_slices(a2, b2, &coeff, me, r)?;
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

    /// Test values (xorshift64*): special values (signed zeros, infinities,
    /// default quiet NaNs of either sign, subnormals, extremes), arbitrary
    /// bit patterns (NaNs among them made default quiet NaNs) and finite
    /// values over a wide exponent range.
    fn draw(state: &mut u64) -> Real {
        const SPECIAL: [u64; 12] = [
            0x0000_0000_0000_0000,
            0x8000_0000_0000_0000,
            0x7ff0_0000_0000_0000,
            0xfff0_0000_0000_0000,
            0x7ff8_0000_0000_0000,
            0xfff8_0000_0000_0000,
            0x0000_0000_0000_0001,
            0x800f_ffff_ffff_ffff,
            0x0010_0000_0000_0000,
            0x7fef_ffff_ffff_ffff,
            0x3ff0_0000_0000_0000,
            0xbff0_0000_0000_0000,
        ];
        let mut next = || {
            let mut x = *state;
            x ^= x >> 12;
            x ^= x << 25;
            x ^= x >> 27;
            *state = x;
            x.wrapping_mul(0x2545_f491_4f6c_dd1d)
        };
        let r = next();
        let x = match r % 8 {
            0 => Real::from_bits(SPECIAL[(r >> 8) as usize % SPECIAL.len()]),
            1 => Real::from_bits(next()),
            _ => {
                let m = (next() >> 11) as Real / (1u64 << 53) as Real;
                let e = ((r >> 8) % 41) as i32 - 20;
                let s = if (r >> 40) & 1 == 1 { -1.0 } else { 1.0 };
                s * m * Real::powi(2.0, e)
            }
        };
        if x.is_nan() {
            Real::from_bits(0x7ff8_0000_0000_0000 | (x.to_bits() & (1 << 63)))
        } else {
            x
        }
    }

    /// Identical bits, or two default quiet NaNs differing only in the sign
    /// (which NaN an addition of two NaNs returns is unspecified; the rule
    /// of the `fast` kernels' tests, D-046/D-051).
    fn same(a: Real, b: Real) -> bool {
        let (x, y) = (a.to_bits(), b.to_bits());
        let nan = 0x7ff8_0000_0000_0000;
        x == y || (x & !(1 << 63) == nan && y & !(1 << 63) == nan)
    }

    /// `CoeffInterpolation` against the general per-product path
    /// (`eigen::product_slices` with the operand descriptions of the call
    /// site), bitwise, for every size whose routes are both
    /// coefficient-based, with one finite-only round per size in which a
    /// single NaN with a payload enters (its propagation is then
    /// determined).
    #[test]
    fn coeff_interpolation_is_bitwise_the_general_products() {
        let mut state = 0x9e37_79b9_7f4a_7c15_u64;
        let mut covered = 0;
        for nq in 1..=4 {
            for nip in 1..=12 {
                for nbc in [1, 2, 3, 4, 6, 9] {
                    let Some(k) = CoeffInterpolation::new(nq, nip, nbc) else {
                        continue;
                    };
                    covered += 1;
                    let [(a1, b1), (a2, b2)] = interpolation_dims(nq, nip, nbc);
                    for round in 0..40 {
                        let mut inv: Vec<Real> = (0..nq * nq).map(|_| draw(&mut state)).collect();
                        let mut field: Vec<Real> =
                            (0..nq * nbc).map(|_| draw(&mut state)).collect();
                        let mut itp: Vec<Real> = (0..nip * nq).map(|_| draw(&mut state)).collect();
                        if round == 0 {
                            // finite values and one payload NaN
                            for v in inv.iter_mut().chain(&mut field).chain(&mut itp) {
                                if !v.is_finite() {
                                    *v = 0.5;
                                }
                            }
                            let n = inv.len() + field.len() + itp.len();
                            let at = (state as usize) % n;
                            let nan = Real::from_bits(0xfff8_0000_0000_0abc);
                            match at {
                                i if i < inv.len() => inv[i] = nan,
                                i if i < inv.len() + field.len() => field[i - inv.len()] = nan,
                                i => itp[i - inv.len() - field.len()] = nan,
                            }
                        }
                        let mut c_ref = vec![0.0; nq * nbc];
                        let mut r_ref = vec![0.0; nip * nbc];
                        eigen::product_slices(a1, b1, &inv, &field, &mut c_ref).unwrap();
                        eigen::product_slices(a2, b2, &c_ref, &itp, &mut r_ref).unwrap();
                        let mut c = vec![0.0; nq * nbc];
                        let mut r = vec![1.0; nip * nbc];
                        k.apply(&inv, &field, &itp, &mut c, &mut r);
                        for (x, y) in c.iter().zip(&c_ref).chain(r.iter().zip(&r_ref)) {
                            let ok = if round == 0 {
                                x.to_bits() == y.to_bits()
                            } else {
                                same(*x, *y)
                            };
                            assert!(
                                ok,
                                "nq {nq} nip {nip} nbc {nbc} round {round}: {x:e} vs {y:e}"
                            );
                        }
                    }
                }
            }
        }
        // the linear tetrahedron's (1, 12, 9: second product gebp, depth 1)
        // and the linear triangle's (1, 6, 4) among them; second-order
        // elements with a gebp product are not
        let k = CoeffInterpolation::new(1, 12, 9).unwrap();
        assert_eq!(k.second, ProductForm::GebpDepth1);
        assert!(matches!(k.first, ProductForm::Coeff(_)));
        assert!(CoeffInterpolation::new(1, 6, 4).is_some());
        assert!(CoeffInterpolation::new(4, 12, 9).is_none());
        assert!(covered > 50, "{covered}");
    }

    /// `PointInterpolation::value` is bitwise the output of
    /// `interpolate_elemental_field` for linear tetrahedra (`nq = 1`), with
    /// the initialisation filter and with a reordered subset of it.
    #[test]
    fn point_interpolation_is_bitwise_the_interpolation() {
        const T4: ElementType = ElementType::Tetrahedron4;
        let mut mesh = Mesh::new(3, "tets").unwrap();
        {
            let mut acc = MeshAccessor::new(&mut mesh);
            for x in [
                [0.0, 0.0, 0.0],
                [1.0, 0.1, 0.0],
                [0.2, 1.0, 0.1],
                [0.1, 0.3, 1.2],
                [1.1, 1.0, 0.9],
                [0.9, -0.2, 1.0],
            ] {
                acc.push_node(&x).unwrap();
            }
            let mut c = acc.connectivity_mut(T4, G);
            c.push_tuple(&[0, 1, 2, 3]);
            c.push_tuple(&[1, 2, 3, 4]);
            c.push_tuple(&[1, 3, 5, 4]);
        }
        let fe = FeEngine::with_precomputation(
            &mesh,
            None,
            ElementKind::Regular,
            ShapeFamily::Lagrange,
            "fe",
        )
        .unwrap();
        // three points per facet (the facets' order-2 rule): gebp form
        let nip = 12;
        let nbc = 9;
        let mut state = 0x1234_5678_9abc_def1_u64;
        let mut points = ElementTypeMapArray::new("points");
        let p = points.alloc(3 * nip, 3, T4, G, 0.0).unwrap();
        for v in p.as_mut_slice() {
            *v = draw(&mut state).clamp(-4.0, 4.0);
            if v.is_nan() {
                *v = 0.25;
            }
        }
        let interp = fe
            .init_elemental_field_interpolation(&mesh, &points, None)
            .unwrap();
        let view = interp.point_interpolation(T4, G, nbc).unwrap().unwrap();
        assert_eq!(view.nb_interpolation_points(), nip);
        assert_eq!(view.nb_slots(), 3);
        for ids in [vec![0, 1, 2], vec![2, 0]] {
            let mut filter = ElementTypeMapArray::<Idx>::new("filter");
            let f = filter.alloc(ids.len(), 1, T4, G, 0).unwrap();
            for (x, &e) in f.as_mut_slice().iter_mut().zip(&ids) {
                *x = Idx::try_from(e).unwrap();
            }
            for _ in 0..20 {
                let mut field = ElementTypeMapArray::new("field");
                let fa = field.alloc(ids.len(), nbc, T4, G, 0.0).unwrap();
                for v in fa.as_mut_slice() {
                    *v = draw(&mut state);
                }
                let mut out = ElementTypeMapArray::new("out");
                fe.interpolate_elemental_field(&mesh, &interp, &field, &mut out, G, Some(&filter))
                    .unwrap();
                let out = out.get(T4, G).unwrap().as_slice();
                let fs = field.get(T4, G).unwrap().as_slice();
                let slots = view.slots(&ids).unwrap();
                let mut v = [0.0; 9];
                for (pos, (&e, &slot)) in ids.iter().zip(&slots).enumerate() {
                    for i in 0..nip {
                        view.value(slot, i, &fs[pos * nbc..(pos + 1) * nbc], &mut v);
                        let mut c = [0.0; 9];
                        let mut w = [0.0; 9];
                        view.coefficients(slot, &fs[pos * nbc..(pos + 1) * nbc], &mut c);
                        view.value_from_coefficients(&c, slot, i, &mut w);
                        assert!(
                            v.iter().zip(&w).all(|(x, y)| x.to_bits() == y.to_bits()),
                            "two-step form"
                        );
                        let r = &out[(e * nip + i) * nbc..(e * nip + i + 1) * nbc];
                        for (x, y) in v.iter().zip(r) {
                            assert!(
                                same(*x, *y),
                                "{ids:?} element {e} point {i}: {x:e} vs {y:e}"
                            );
                        }
                    }
                }
            }
        }
        // a second-order block has no point interpolation (nq > 1)
        assert!(interp.point_interpolation(T4, G, 400).unwrap().is_none());
        assert!(
            interp
                .point_interpolation(ElementType::Tetrahedron10, G, nbc)
                .unwrap()
                .is_none()
        );
    }

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
