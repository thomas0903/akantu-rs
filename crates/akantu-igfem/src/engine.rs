// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): extra_packages/igfem/src/shape_igfem{.hh,.cc,_inline_impl.hh}
// (ShapeLagrange<_ek_igfem>), integrator_gauss_igfem{.hh,_inline_impl.hh}
// (IntegratorGauss<_ek_igfem>), fe_engine_template_tmpl_igfem.hh, and the
// generic parts of src/fe_engine/shape_functions_inline_impl.hh /
// fe_engine_template_tmpl.hh used by FEEngineTemplate<…, _ek_igfem>
// (interpolate/gradient of elemental fields, computeBtD, computeBtDB).

//! The IGFEM finite-element engine (IGFEM-002): upstream
//! `FEEngineTemplate<IntegratorGauss, ShapeLagrange, _ek_igfem>`.
//!
//! # Quadrature
//!
//! The integration points of an IGFEM element are the Gauss points of
//! sub-element 0 followed by those of sub-element 1, each with the default
//! rule of the sub-element type (`_triangle_3`: 1 point, `_quadrangle_4`:
//! 2 × 2, `_segment_2`: 1): `_igfem_triangle_4` has 2 points,
//! `_igfem_triangle_5` 5, `_igfem_segment_3` 2. The Jacobians are those of
//! the sub-elements (`det J_sub · w_sub`), so integrals are exact for
//! piecewise-polynomial integrands up to the sub-rule degree on each side
//! of the interface.
//!
//! # Precomputation (`initShapeFunctions`)
//!
//! Per element: each sub-element Gauss point is mapped to the physical
//! domain (sub-element interpolation of its node coordinates) and then to
//! the parent's reference element (parent `inverseMap`, Gauss–Newton
//! from `ξ = 0`, tolerance `1e-10`, 100 iterations); the shapes are the
//! parent shapes at that point followed by the sub-element shapes of the
//! enriched nodes ([`crate::element::assemble_shapes`]). The physical
//! derivatives are assembled the same way from the parent's and the
//! sub-element's `J⁻¹·dN/dξ`. The shapes are also computed **at the
//! enriched nodes** (parent shapes at the node, enrichment `δ_ij`), to
//! recover the physical values of a field there
//! ([`IgfemFeEngine::interpolate_at_enriched_nodes`]).
//!
//! Upstream defects (D-003, `docs/UPSTREAM_DEFECTS.md`):
//! * UD-603 (C, observationally identical): the parent derivatives of the
//!   sub-element-1 points are read at index `i` instead of `n₁ + i` (and
//!   the parent shapes are computed with `sub_element_type_1` instead of
//!   the parent type); both are bitwise immaterial for the three IGFEM
//!   types (linear parents with constant derivatives; `sub_element_type_1`
//!   is the parent type). The port uses the intended indices and types.
//! * UD-604 (A): `IntegratorGauss<_ek_igfem>::integrateOnIntegrationPoints`
//!   loops over `nb_element` values instead of `nb_element · nb_points`
//!   (only the first points are scaled, the rest of the output is
//!   uninitialised); the port scales every point, as the regular
//!   integrator. No upstream test exercises it.
//! * UD-605 (A): `computeShapeDerivatives(real_coords, …)` is
//!   `AKANTU_DEBUG_TO_IMPLEMENT` upstream; the port returns
//!   `Error::NotImplemented`.
//! * UD-606 (B, no compat): see [`IgfemFeEngine::contains`].
//! * UD-790 (C): upstream's `computeIntegrationPointsCoordinates` overload
//!   for an `ElementTypeMapArray` zeroes the not-ghost values while it
//!   extracts the ghost ones; it has no user and is not ported (only the
//!   per-type overload, which is correct).
//!
//! The engine keeps no mesh reference: every method takes `&Mesh`, and
//! every method reading precomputed data refuses a block whose mesh
//! revision or element count no longer matches the mesh (re-run
//! [`IgfemFeEngine::init_shape_functions`] after a mesh change, as the IGFEM
//! model does on every element event, upstream `onElementsAdded/Removed`).

use std::collections::BTreeMap;

use akantu_core::array::ArrayView;
use akantu_core::tensor::MatrixX;
use akantu_core::{
    Array, Element, ElementKind, ElementType, Error, GhostType, Id, Idx, Real, Result, TypeFilter,
    try_to_usize,
};
use akantu_fe::eigen::{self, Dims};
use akantu_fe::element_class::{INVERSE_MAP_MAX_ITERATIONS, INVERSE_MAP_TOLERANCE};
use akantu_fe::quadrature::{QuadratureRule, default_rule};
use akantu_fe::{LagrangeElement, kernels, with_element_class};
use akantu_mesh::Mesh;

use crate::element::{
    self, NB_SUB_ELEMENTS, assemble_shape_derivatives, assemble_shapes, check_len, nb_enrichments,
    nb_parent_nodes, parent_type, sub_element_coords, sub_element_enrichments, sub_element_type,
};

/// `Math::setTolerance(1e-14)` of upstream `computeShapes(real_coords, …)`
/// while it locates the sub-element containing a point.
pub const COMPUTE_SHAPES_TOLERANCE: Real = 1e-14;

/// Signature of `LagrangeElement::inverse_map` (real point, node
/// coordinates, natural coordinates, max iterations, tolerance).
type InverseMapFn = fn(&[Real], &[Real], &mut [Real], usize, Real) -> Result<()>;

/// Lagrange kernels of a regular element type (sub-elements and parents),
/// resolved once per block (function pointers to the monomorphised
/// `akantu-fe` kernels).
#[derive(Clone, Copy)]
struct Kernels {
    nd: usize,
    nn: usize,
    shapes: fn(&[Real], &mut [Real]),
    dnds: fn(&[Real], &mut [Real]),
    inverse_map: InverseMapFn,
    contains: fn(&[Real], Real) -> bool,
}

fn lagrange_kernels<E: LagrangeElement>() -> Kernels {
    Kernels {
        nd: E::NATURAL_DIM,
        nn: E::NB_SHAPES,
        shapes: E::compute_shapes,
        dnds: E::compute_dnds,
        inverse_map: E::inverse_map,
        contains: E::contains_with_tolerance,
    }
}

fn kernels_of(ty: ElementType) -> Result<Kernels> {
    if ty.kind() != ElementKind::Regular {
        return Err(Error::UnsupportedElementType {
            ty,
            context: "IGFEM sub-element / parent kernels (regular types only)",
        });
    }
    with_element_class!(ty, E => Ok(lagrange_kernels::<E>()), else Err(Error::UnsupportedElementType {
        ty,
        context: "IGFEM sub-element / parent kernels",
    }))
}

/// Kernels and rules of one IGFEM type.
#[derive(Clone)]
struct TypeKernels {
    ty: ElementType,
    parent: Kernels,
    sub: [Kernels; 2],
    rules: [QuadratureRule; 2],
    np: usize,
    nb_enr: usize,
    nn: usize,
    nd: usize,
}

impl TypeKernels {
    fn new(ty: ElementType) -> Result<Self> {
        element::igfem_info(ty)?;
        let parent = kernels_of(parent_type(ty)?)?;
        let s0 = sub_element_type(ty, 0)?;
        let s1 = sub_element_type(ty, 1)?;
        Ok(Self {
            ty,
            parent,
            sub: [kernels_of(s0)?, kernels_of(s1)?],
            rules: [default_rule(s0)?, default_rule(s1)?],
            np: nb_parent_nodes(ty)?,
            nb_enr: nb_enrichments(ty)?,
            nn: ty.nb_nodes_per_interpolation_element(),
            nd: ty.natural_space_dimension(),
        })
    }

    fn nq(&self, s: usize) -> usize {
        self.rules[s].nb_points()
    }

    fn nq_total(&self) -> usize {
        self.nq(0) + self.nq(1)
    }
}

/// Precomputed data of one `(type, ghost)` block (upstream
/// `IntegratorGauss<_ek_igfem>::jacobians`, `quadrature_points`,
/// `ShapeLagrange<_ek_igfem>::shapes`, `shapes_derivatives`,
/// `igfem_integration_points`, `shapes_at_enrichments`).
#[derive(Clone, Debug)]
pub struct IgfemBlock {
    /// Quadrature rules of the two sub-elements.
    pub sub_rules: [QuadratureRule; 2],
    /// All integration points in the sub-elements' natural coordinates,
    /// sub-element 0 first (upstream `quadrature_points(type)`), column-major
    /// `natural_dim × nq`.
    pub points: MatrixX,
    /// Elements of the block when it was computed.
    pub nb_element: usize,
    /// `det J_sub · w` per point (`nb_element · nq` × 1).
    pub jacobians: Array<Real>,
    /// Shapes per point (`nb_element · nq` × `nb_shapes`).
    pub shapes: Array<Real>,
    /// Physical shape derivatives per point (`nb_element · nq` ×
    /// `dim · nb_shapes`, column-major `dim × nb_shapes`), when the natural
    /// dimension equals the mesh dimension.
    pub shapes_derivatives: Option<Array<Real>>,
    /// Natural coordinates of the integration points in the parent element
    /// (`nb_element · nq` × `natural_dim`).
    pub igfem_integration_points: Array<Real>,
    /// Shapes at the enriched nodes (`nb_element · nb_enrichments` ×
    /// `nb_shapes`).
    pub shapes_at_enrichments: Array<Real>,
}

/// IGFEM finite-element engine (upstream `"IGFEMFEEngine"`).
#[derive(Clone, Debug)]
pub struct IgfemFeEngine {
    id: Id,
    element_dimension: usize,
    blocks: BTreeMap<(ElementType, GhostType), IgfemBlock>,
    mesh_revision: u64,
}

/// Coordinates (or any nodal field) of the nodes of element `e` of a
/// connectivity block, column-major `nb_comp × nnpe`.
fn gather(
    conn: &Array<Idx>,
    e: usize,
    field: &[Real],
    nb_comp: usize,
    nb_nodes: usize,
    out: &mut [Real],
) -> Result<()> {
    let nnpe = conn.nb_component();
    let row = conn.get_tuple(e)?;
    for (k, &n) in row.iter().enumerate() {
        let n = try_to_usize(n)?;
        if n >= nb_nodes {
            return Err(Error::OutOfRange {
                what: "node (IGFEM element connectivity)",
                index: n,
                len: nb_nodes,
            });
        }
        out[k * nb_comp..(k + 1) * nb_comp].copy_from_slice(&field[n * nb_comp..(n + 1) * nb_comp]);
    }
    debug_assert_eq!(out.len(), nnpe * nb_comp);
    Ok(())
}

fn resolve_filter(filter: Option<&[Idx]>, nb_element: usize) -> Result<Vec<usize>> {
    match filter {
        None => Ok((0..nb_element).collect()),
        Some(f) => f
            .iter()
            .map(|&e| {
                let e = try_to_usize(e)?;
                if e >= nb_element {
                    return Err(Error::OutOfRange {
                        what: "element (IGFEM filter)",
                        index: e,
                        len: nb_element,
                    });
                }
                Ok(e)
            })
            .collect(),
    }
}

fn prepare_out(out: &mut Array<Real>, len: usize, nb_component: usize) -> Result<()> {
    if out.nb_component() != nb_component {
        *out = Array::with_id(nb_component, out.id().to_owned());
    }
    out.try_resize(len, 0.0)?;
    out.fill(0.0);
    Ok(())
}

fn check_nodal(
    context: &'static str,
    field: &ArrayView<'_, Real>,
    nb_nodes: usize,
    nb_comp: usize,
) -> Result<()> {
    if field.nb_component() != nb_comp {
        return Err(Error::DimensionMismatch {
            context,
            expected: nb_comp,
            got: field.nb_component(),
        });
    }
    if field.len() != nb_nodes {
        return Err(Error::DimensionMismatch {
            context,
            expected: nb_nodes,
            got: field.len(),
        });
    }
    Ok(())
}

impl IgfemFeEngine {
    /// New engine on the IGFEM elements of dimension `element_dimension`
    /// (default: the mesh dimension); nothing is precomputed (call
    /// [`IgfemFeEngine::init_shape_functions`] for both ghost types, or use
    /// [`IgfemFeEngine::with_precomputation`]).
    #[must_use]
    pub fn new(mesh: &Mesh, element_dimension: Option<usize>, id: impl Into<Id>) -> Self {
        Self {
            id: id.into(),
            element_dimension: element_dimension.unwrap_or(mesh.spatial_dimension()),
            blocks: BTreeMap::new(),
            mesh_revision: mesh.revision(),
        }
    }

    /// [`IgfemFeEngine::new`] followed by the precomputation for both ghost
    /// types (upstream `FEEngineTemplate` constructor + `initShapeFunctions`).
    ///
    /// # Errors
    /// See [`IgfemFeEngine::init_shape_functions`].
    pub fn with_precomputation(
        mesh: &Mesh,
        element_dimension: Option<usize>,
        id: impl Into<Id>,
    ) -> Result<Self> {
        let mut fe = Self::new(mesh, element_dimension, id);
        fe.init_shape_functions(mesh, GhostType::NotGhost)?;
        fe.init_shape_functions(mesh, GhostType::Ghost)?;
        Ok(fe)
    }

    /// Identifier.
    #[must_use]
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Dimension of the handled elements.
    #[must_use]
    pub fn element_dimension(&self) -> usize {
        self.element_dimension
    }

    /// Mesh revision of the last precomputation.
    #[must_use]
    pub fn mesh_revision(&self) -> u64 {
        self.mesh_revision
    }

    /// IGFEM element types of `mesh` handled by the engine (upstream
    /// `mesh.elementTypes(element_dimension, ghost_type, _ek_igfem)`).
    pub fn element_types<'m>(
        &self,
        mesh: &'m Mesh,
        ghost: GhostType,
    ) -> impl Iterator<Item = ElementType> + 'm {
        mesh.element_types(TypeFilter {
            spatial_dimension: Some(self.element_dimension),
            ghost_type: ghost,
            kind: Some(ElementKind::Igfem),
        })
    }

    /// Integration points per element of `ty` (upstream
    /// `getNbIntegrationPoints`: the sum of the sub-elements' default rule
    /// sizes; no precomputation needed).
    ///
    /// # Errors
    /// Not an IGFEM type.
    pub fn nb_integration_points(&self, ty: ElementType) -> Result<usize> {
        Ok(TypeKernels::new(ty)?.nq_total())
    }

    /// Upstream `initShapeFunctions(ghost)` (with `initIntegrator`): for
    /// every IGFEM type of the mesh, the quadrature points, the Jacobians
    /// (error if one is negative, upstream `checkJacobians`), the shapes,
    /// the derivatives (natural dimension = mesh dimension) and the shapes
    /// at the enriched nodes. Blocks of types that no longer exist for this
    /// ghost type are dropped. Transactional per ghost type: on error the
    /// previous blocks of the ghost type are kept.
    ///
    /// # Errors
    /// Malformed mesh, negative Jacobian, non-converged inverse map.
    pub fn init_shape_functions(&mut self, mesh: &Mesh, ghost: GhostType) -> Result<()> {
        let types: Vec<ElementType> = self.element_types(mesh, ghost).collect();
        let mut computed = Vec::with_capacity(types.len());
        for ty in types {
            computed.push((ty, precompute(mesh, ty, ghost)?));
        }
        self.blocks.retain(|(_, g), _| *g != ghost);
        for (ty, b) in computed {
            self.blocks.insert((ty, ghost), b);
        }
        self.mesh_revision = mesh.revision();
        Ok(())
    }

    /// The precomputed block of `(ty, ghost)`, checked against the mesh
    /// (revision and element count).
    ///
    /// # Errors
    /// Missing or stale block.
    pub fn block(&self, mesh: &Mesh, ty: ElementType, ghost: GhostType) -> Result<&IgfemBlock> {
        let b = self.blocks.get(&(ty, ghost)).ok_or_else(|| {
            Error::invalid(format!(
                "IGFEM FE engine {}: no precomputed data for {} ({}); call init_shape_functions",
                self.id,
                ty.name(),
                ghost.name()
            ))
        })?;
        if mesh.revision() != self.mesh_revision || mesh.nb_element(ty, ghost) != b.nb_element {
            return Err(Error::invalid(format!(
                "IGFEM FE engine {}: precomputed data of {} ({}) is stale (mesh changed); \
                 call init_shape_functions",
                self.id,
                ty.name(),
                ghost.name()
            )));
        }
        Ok(b)
    }

    /// Precomputed block without the mesh check (for inspection).
    #[must_use]
    pub fn block_unchecked(&self, ty: ElementType, ghost: GhostType) -> Option<&IgfemBlock> {
        self.blocks.get(&(ty, ghost))
    }

    /// Upstream `getIntegrationPoints(type)`: natural coordinates (in the
    /// sub-elements) of the integration points, sub-element 0 first.
    ///
    /// # Errors
    /// Missing or stale block.
    pub fn integration_points(
        &self,
        mesh: &Mesh,
        ty: ElementType,
        ghost: GhostType,
    ) -> Result<&MatrixX> {
        Ok(&self.block(mesh, ty, ghost)?.points)
    }

    /// Upstream `getShapes`.
    ///
    /// # Errors
    /// Missing or stale block.
    pub fn shapes(&self, mesh: &Mesh, ty: ElementType, ghost: GhostType) -> Result<&Array<Real>> {
        Ok(&self.block(mesh, ty, ghost)?.shapes)
    }

    /// Upstream `getShapesDerivatives`.
    ///
    /// # Errors
    /// Missing or stale block, or no derivatives (natural dimension differs
    /// from the mesh dimension).
    pub fn shapes_derivatives(
        &self,
        mesh: &Mesh,
        ty: ElementType,
        ghost: GhostType,
    ) -> Result<&Array<Real>> {
        self.block(mesh, ty, ghost)?
            .shapes_derivatives
            .as_ref()
            .ok_or_else(|| {
                Error::invalid(format!(
                    "IGFEM FE engine: no shape derivatives for {} (natural dimension differs \
                     from the mesh dimension)",
                    ty.name()
                ))
            })
    }

    /// Upstream `getJacobians` (`det J_sub · w`).
    ///
    /// # Errors
    /// Missing or stale block.
    pub fn jacobians(
        &self,
        mesh: &Mesh,
        ty: ElementType,
        ghost: GhostType,
    ) -> Result<&Array<Real>> {
        Ok(&self.block(mesh, ty, ghost)?.jacobians)
    }

    /// Upstream `getShapesAtEnrichedNodes`.
    ///
    /// # Errors
    /// Missing or stale block.
    pub fn shapes_at_enrichments(
        &self,
        mesh: &Mesh,
        ty: ElementType,
        ghost: GhostType,
    ) -> Result<&Array<Real>> {
        Ok(&self.block(mesh, ty, ghost)?.shapes_at_enrichments)
    }

    /// Parent natural coordinates of the integration points (upstream
    /// `igfem_integration_points`).
    ///
    /// # Errors
    /// Missing or stale block.
    pub fn igfem_integration_points(
        &self,
        mesh: &Mesh,
        ty: ElementType,
        ghost: GhostType,
    ) -> Result<&Array<Real>> {
        Ok(&self.block(mesh, ty, ghost)?.igfem_integration_points)
    }

    /// Upstream `interpolateOnIntegrationPoints(u, uq, nb_dof, type, ghost,
    /// filter)` (`ShapeFunctions::interpolateElementalFieldOnIntegrationPoints`:
    /// `uq = u_e · N` per element). Output `ids·nq` tuples of `nb_dof`.
    ///
    /// # Errors
    /// Wrong nodal field size, filter out of range, missing/stale block.
    #[allow(clippy::too_many_arguments)] // mirrors the upstream signature
    pub fn interpolate_on_integration_points(
        &self,
        mesh: &Mesh,
        u: ArrayView<'_, Real>,
        out: &mut Array<Real>,
        nb_dof: usize,
        ty: ElementType,
        ghost: GhostType,
        filter: Option<&[Idx]>,
    ) -> Result<()> {
        check_nodal(
            "IGFEM interpolate_on_integration_points: nodal field",
            &u,
            mesh.nb_nodes(),
            nb_dof,
        )?;
        let b = self.block(mesh, ty, ghost)?;
        let conn = mesh.connectivity(ty, ghost)?;
        let ids = resolve_filter(filter, b.nb_element)?;
        let nq = b.points.cols();
        let nn = b.shapes.nb_component();
        prepare_out(out, ids.len() * nq, nb_dof)?;
        let mut ue = vec![0.0; nb_dof * nn];
        let s = b.shapes.as_slice();
        let o = out.as_mut_slice();
        for (pos, &e) in ids.iter().enumerate() {
            gather(&conn, e, u.as_slice(), nb_dof, mesh.nb_nodes(), &mut ue)?;
            eigen::product_slices(
                Dims::dynamic(nb_dof, nn),
                Dims::dynamic(nn, nq),
                &ue,
                &s[e * nq * nn..(e + 1) * nq * nn],
                &mut o[pos * nq * nb_dof..(pos + 1) * nq * nb_dof],
            )?;
        }
        Ok(())
    }

    /// Upstream `gradientOnIntegrationPoints` (`gradientElementalFieldOnIntegrationPoints`:
    /// `∇u = u_e · Bᵀ` per point). Output tuples of `nb_dof · dim`
    /// (column-major `nb_dof × dim`).
    ///
    /// # Errors
    /// Wrong nodal field size, filter out of range, missing/stale block, no
    /// derivatives.
    #[allow(clippy::too_many_arguments)] // mirrors the upstream signature
    pub fn gradient_on_integration_points(
        &self,
        mesh: &Mesh,
        u: ArrayView<'_, Real>,
        out: &mut Array<Real>,
        nb_dof: usize,
        ty: ElementType,
        ghost: GhostType,
        filter: Option<&[Idx]>,
    ) -> Result<()> {
        check_nodal(
            "IGFEM gradient_on_integration_points: nodal field",
            &u,
            mesh.nb_nodes(),
            nb_dof,
        )?;
        let b = self.block(mesh, ty, ghost)?;
        let dsh = self.shapes_derivatives(mesh, ty, ghost)?;
        let conn = mesh.connectivity(ty, ghost)?;
        let ids = resolve_filter(filter, b.nb_element)?;
        let nq = b.points.cols();
        let nn = ty.nb_nodes_per_interpolation_element();
        let nd = ty.natural_space_dimension();
        let bsz = nd * nn;
        prepare_out(out, ids.len() * nq, nb_dof * nd)?;
        let mut ue = vec![0.0; nb_dof * nn];
        let ud = Dims::new(nb_dof, nn, false, true);
        let bt = Dims::fixed(nd, nn).t();
        let (ds, o) = (dsh.as_slice(), out.as_mut_slice());
        for (pos, &e) in ids.iter().enumerate() {
            gather(&conn, e, u.as_slice(), nb_dof, mesh.nb_nodes(), &mut ue)?;
            for q in 0..nq {
                let t = pos * nq + q;
                eigen::product_slices(
                    ud,
                    bt,
                    &ue,
                    &ds[(e * nq + q) * bsz..(e * nq + q + 1) * bsz],
                    &mut o[t * nb_dof * nd..(t + 1) * nb_dof * nd],
                )?;
            }
        }
        Ok(())
    }

    /// Upstream `IntegratorGauss<_ek_igfem>::integrate(f, intf, nb_dof,
    /// ghost, filter)`: `∫ f = f_e · J_e` per element (one tuple of `nb_dof`
    /// per element of the filter).
    ///
    /// # Errors
    /// Wrong field size, filter out of range, missing/stale block.
    #[allow(clippy::too_many_arguments)] // mirrors the upstream signature
    pub fn integrate(
        &self,
        mesh: &Mesh,
        f: ArrayView<'_, Real>,
        out: &mut Array<Real>,
        nb_dof: usize,
        ty: ElementType,
        ghost: GhostType,
        filter: Option<&[Idx]>,
    ) -> Result<()> {
        let b = self.block(mesh, ty, ghost)?;
        let ids = resolve_filter(filter, b.nb_element)?;
        let nq = b.points.cols();
        if f.nb_component() != nb_dof || f.len() != ids.len() * nq {
            return Err(Error::DimensionMismatch {
                context: "IGFEM integrate: quadrature field (filter·nq tuples of nb_dof)",
                expected: ids.len() * nq * nb_dof,
                got: f.len() * f.nb_component(),
            });
        }
        prepare_out(out, ids.len(), nb_dof)?;
        let (js, fs, o) = (b.jacobians.as_slice(), f.as_slice(), out.as_mut_slice());
        for (pos, &e) in ids.iter().enumerate() {
            eigen::product_slices(
                Dims::dynamic(nb_dof, nq),
                Dims::new(nq, 1, false, true),
                &fs[pos * nq * nb_dof..(pos + 1) * nq * nb_dof],
                &js[e * nq..(e + 1) * nq],
                &mut o[pos * nb_dof..(pos + 1) * nb_dof],
            )?;
        }
        Ok(())
    }

    /// Upstream `Real integrate(f, type, ghost, filter)`: per-element
    /// integrals of a scalar quadrature field combined with the sorted
    /// pairwise sum (`akantu_core::math::reduce`; an empty result is `0`).
    ///
    /// # Errors
    /// See [`IgfemFeEngine::integrate`].
    pub fn integrate_scalar(
        &self,
        mesh: &Mesh,
        f: ArrayView<'_, Real>,
        ty: ElementType,
        ghost: GhostType,
        filter: Option<&[Idx]>,
    ) -> Result<Real> {
        let mut per_element = Array::new(1);
        self.integrate(mesh, f, &mut per_element, 1, ty, ghost, filter)?;
        Ok(akantu_core::math::reduce(per_element.as_mut_slice()))
    }

    /// Upstream `Real integrate(Vector f, type, index, ghost)`: the integral
    /// over element `index` of the values `f` at its `nq` points
    /// (sequential `Σ f_q · J_q` from `+0`).
    ///
    /// # Errors
    /// Wrong number of values, element out of range, missing/stale block.
    pub fn integrate_element(
        &self,
        mesh: &Mesh,
        f: &[Real],
        ty: ElementType,
        index: usize,
        ghost: GhostType,
    ) -> Result<Real> {
        let b = self.block(mesh, ty, ghost)?;
        let nq = b.points.cols();
        check_len("IGFEM integrate(element): values per point", nq, f.len())?;
        if index >= b.nb_element {
            return Err(Error::OutOfRange {
                what: "element (IGFEM integrate)",
                index,
                len: b.nb_element,
            });
        }
        let j = &b.jacobians.as_slice()[index * nq..(index + 1) * nq];
        let mut inte = 0.0;
        for (fq, jq) in f.iter().zip(j) {
            inte += *fq * *jq;
        }
        Ok(inte)
    }

    /// Upstream `integrateOnIntegrationPoints`: `f_q · J_q` per point
    /// (UD-604: every point, not only the first `nb_element`).
    ///
    /// # Errors
    /// Wrong field size, filter out of range, missing/stale block.
    #[allow(clippy::too_many_arguments)] // mirrors the upstream signature
    pub fn integrate_on_integration_points(
        &self,
        mesh: &Mesh,
        f: ArrayView<'_, Real>,
        out: &mut Array<Real>,
        nb_dof: usize,
        ty: ElementType,
        ghost: GhostType,
        filter: Option<&[Idx]>,
    ) -> Result<()> {
        let b = self.block(mesh, ty, ghost)?;
        let ids = resolve_filter(filter, b.nb_element)?;
        let nq = b.points.cols();
        if f.nb_component() != nb_dof || f.len() != ids.len() * nq {
            return Err(Error::DimensionMismatch {
                context: "IGFEM integrate_on_integration_points: quadrature field",
                expected: ids.len() * nq * nb_dof,
                got: f.len() * f.nb_component(),
            });
        }
        prepare_out(out, ids.len() * nq, nb_dof)?;
        let (js, fs, o) = (b.jacobians.as_slice(), f.as_slice(), out.as_mut_slice());
        for (pos, &e) in ids.iter().enumerate() {
            for q in 0..nq {
                let t = pos * nq + q;
                let j = js[e * nq + q];
                for d in 0..nb_dof {
                    o[t * nb_dof + d] = fs[t * nb_dof + d] * j;
                }
            }
        }
        Ok(())
    }

    /// Upstream `computeBtD`: `D · B` per point (`D` has `r · dim`
    /// components, column-major `r × dim`; output `r · nn`).
    ///
    /// # Errors
    /// Wrong sizes, missing/stale block, no derivatives.
    pub fn compute_btd(
        &self,
        mesh: &Mesh,
        d: ArrayView<'_, Real>,
        out: &mut Array<Real>,
        ty: ElementType,
        ghost: GhostType,
        filter: Option<&[Idx]>,
    ) -> Result<()> {
        let b = self.block(mesh, ty, ghost)?;
        let dsh = self.shapes_derivatives(mesh, ty, ghost)?;
        let ids = resolve_filter(filter, b.nb_element)?;
        let nq = b.points.cols();
        let nn = ty.nb_nodes_per_interpolation_element();
        let sdim = ty.natural_space_dimension();
        if sdim == 0 || d.nb_component() % sdim != 0 || d.len() != ids.len() * nq {
            return Err(Error::DimensionMismatch {
                context: "IGFEM compute_btd: D (filter·nq tuples of r·dim)",
                expected: ids.len() * nq * sdim,
                got: d.len() * d.nb_component(),
            });
        }
        let r = d.nb_component() / sdim;
        prepare_out(out, d.len(), r * nn)?;
        let (bs, ds, os) = (dsh.as_slice(), d.as_slice(), out.as_mut_slice());
        let bsz = sdim * nn;
        for (pos, &e) in ids.iter().enumerate() {
            for q in 0..nq {
                let t = pos * nq + q;
                eigen::product_slices(
                    Dims::dynamic(r, sdim),
                    Dims::dynamic(sdim, nn),
                    &ds[t * r * sdim..(t + 1) * r * sdim],
                    &bs[(e * nq + q) * bsz..(e * nq + q + 1) * bsz],
                    &mut os[t * r * nn..(t + 1) * r * nn],
                )?;
            }
        }
        Ok(())
    }

    /// Upstream `computeBtDB` (generic `FEEngineTemplate` version):
    /// `order_d == 4` (`D` a Voigt tangent, `B` the symmetric Voigt strain
    /// matrix, output `(dim·nn)²`) or `order_d == 2` (`D` is `dim × dim`,
    /// output `nn²`).
    ///
    /// # Errors
    /// Wrong sizes or order, missing/stale block, no derivatives.
    #[allow(clippy::too_many_arguments)] // mirrors the upstream signature
    pub fn compute_btdb(
        &self,
        mesh: &Mesh,
        d: ArrayView<'_, Real>,
        out: &mut Array<Real>,
        order_d: usize,
        ty: ElementType,
        ghost: GhostType,
        filter: Option<&[Idx]>,
    ) -> Result<()> {
        let b = self.block(mesh, ty, ghost)?;
        let dsh = self.shapes_derivatives(mesh, ty, ghost)?;
        let ids = resolve_filter(filter, b.nb_element)?;
        let nq = b.points.cols();
        let nn = ty.nb_nodes_per_interpolation_element();
        let dim = ty.natural_space_dimension();
        let bsz = dim * nn;
        let bs = dsh.as_slice();
        match order_d {
            4 => {
                let ts = akantu_core::voigt::voigt_table(dim)?.size;
                let n = dim * nn;
                if d.nb_component() != ts * ts || d.len() != ids.len() * nq {
                    return Err(Error::DimensionMismatch {
                        context: "IGFEM compute_btdb: D (Voigt tangent per point)",
                        expected: ids.len() * nq * ts * ts,
                        got: d.len() * d.nb_component(),
                    });
                }
                prepare_out(out, d.len(), n * n)?;
                let mut bfull = MatrixX::zeros(dim, nn);
                let mut bv = MatrixX::zeros(ts, n);
                let mut btd = vec![0.0; n * ts];
                let (ds, os) = (d.as_slice(), out.as_mut_slice());
                for (pos, &e) in ids.iter().enumerate() {
                    for q in 0..nq {
                        bfull
                            .as_mut_slice()
                            .copy_from_slice(&bs[(e * nq + q) * bsz..(e * nq + q + 1) * bsz]);
                        akantu_core::voigt::b_matrix_to_sym_voigt(dim, &bfull, &mut bv, nn)?;
                        let t = pos * nq + q;
                        let dq = &ds[t * ts * ts..(t + 1) * ts * ts];
                        let bm = bv.as_slice();
                        eigen::product_slices(
                            Dims::dynamic(ts, n).t(),
                            Dims::dynamic(ts, ts),
                            bm,
                            dq,
                            &mut btd,
                        )?;
                        eigen::product_slices(
                            Dims::dynamic(n, ts),
                            Dims::dynamic(ts, n),
                            &btd,
                            bm,
                            &mut os[t * n * n..(t + 1) * n * n],
                        )?;
                    }
                }
                Ok(())
            }
            2 => {
                if d.nb_component() != dim * dim || d.len() != ids.len() * nq {
                    return Err(Error::DimensionMismatch {
                        context: "IGFEM compute_btdb: D (dim × dim per point)",
                        expected: ids.len() * nq * dim * dim,
                        got: d.len() * d.nb_component(),
                    });
                }
                prepare_out(out, d.len(), nn * nn)?;
                let mut btd = vec![0.0; nn * dim];
                let (ds, os) = (d.as_slice(), out.as_mut_slice());
                for (pos, &e) in ids.iter().enumerate() {
                    for q in 0..nq {
                        let bq = &bs[(e * nq + q) * bsz..(e * nq + q + 1) * bsz];
                        let t = pos * nq + q;
                        let dq = &ds[t * dim * dim..(t + 1) * dim * dim];
                        eigen::product_slices(
                            Dims::dynamic(dim, nn).t(),
                            Dims::dynamic(dim, dim),
                            bq,
                            dq,
                            &mut btd,
                        )?;
                        eigen::product_slices(
                            Dims::dynamic(nn, dim),
                            Dims::dynamic(dim, nn),
                            &btd,
                            bq,
                            &mut os[t * nn * nn..(t + 1) * nn * nn],
                        )?;
                    }
                }
                Ok(())
            }
            _ => Err(Error::InvalidArgument(format!(
                "IGFEM compute_btdb: order_d must be 2 or 4, got {order_d}"
            ))),
        }
    }

    /// Upstream `fieldTimesShapes(field, field_times_shapes, ghost)`:
    /// `f_q · N_qᵀ` per point (output `nb_dof · nn`, column-major
    /// `nb_dof × nn`); `field` holds one tuple per point of every element.
    ///
    /// # Errors
    /// Wrong field size, missing/stale block.
    pub fn field_times_shapes(
        &self,
        mesh: &Mesh,
        field: ArrayView<'_, Real>,
        out: &mut Array<Real>,
        ty: ElementType,
        ghost: GhostType,
    ) -> Result<()> {
        let b = self.block(mesh, ty, ghost)?;
        let nn = b.shapes.nb_component();
        let nb_dof = field.nb_component();
        if field.len() != b.shapes.len() {
            return Err(Error::DimensionMismatch {
                context: "IGFEM field_times_shapes: one tuple per integration point",
                expected: b.shapes.len(),
                got: field.len(),
            });
        }
        prepare_out(out, field.len(), nb_dof * nn)?;
        let (fs, ss, os) = (field.as_slice(), b.shapes.as_slice(), out.as_mut_slice());
        for t in 0..field.len() {
            eigen::product_slices(
                Dims::dynamic(nb_dof, 1),
                Dims::dynamic(1, nn),
                &fs[t * nb_dof..(t + 1) * nb_dof],
                &ss[t * nn..(t + 1) * nn],
                &mut os[t * nb_dof * nn..(t + 1) * nb_dof * nn],
            )?;
        }
        Ok(())
    }

    /// Upstream `extractValuesAtStandardNodes(nodal, extracted, ghost)`:
    /// zero `extracted`, then copy the values at the **parent** nodes of
    /// every IGFEM element of `ghost` (enriched values stay zero).
    ///
    /// # Errors
    /// Component mismatch, malformed mesh.
    pub fn extract_values_at_standard_nodes(
        &self,
        mesh: &Mesh,
        nodal: ArrayView<'_, Real>,
        extracted: &mut Array<Real>,
        ghost: GhostType,
    ) -> Result<()> {
        let nb_comp = nodal.nb_component();
        if extracted.nb_component() != nb_comp {
            return Err(Error::DimensionMismatch {
                context: "IGFEM extractValuesAtStandardNodes: the arrays are not of the same size",
                expected: nb_comp,
                got: extracted.nb_component(),
            });
        }
        extracted.try_resize(nodal.len(), 0.0)?;
        extracted.fill(0.0);
        let types: Vec<ElementType> = mesh
            .element_types(TypeFilter {
                spatial_dimension: Some(mesh.spatial_dimension()),
                ghost_type: ghost,
                kind: Some(ElementKind::Igfem),
            })
            .collect();
        for ty in types {
            let np = nb_parent_nodes(ty)?;
            let conn = mesh.connectivity(ty, ghost)?;
            for row in conn.tuples() {
                for &n in &row[..np] {
                    let n = try_to_usize(n)?;
                    if n >= nodal.len() {
                        return Err(Error::OutOfRange {
                            what: "node (extractValuesAtStandardNodes)",
                            index: n,
                            len: nodal.len(),
                        });
                    }
                    extracted.tuple_mut(n).copy_from_slice(nodal.tuple(n));
                }
            }
        }
        Ok(())
    }

    /// Upstream `computeIntegrationPointsCoordinates(coords, type, ghost,
    /// filter)`: interpolation of the node coordinates with the enriched
    /// values set to zero (the position field has no kink: "there is no gap
    /// in the mesh").
    ///
    /// # Errors
    /// See [`IgfemFeEngine::interpolate_on_integration_points`].
    pub fn integration_points_coordinates(
        &self,
        mesh: &Mesh,
        out: &mut Array<Real>,
        ty: ElementType,
        ghost: GhostType,
        filter: Option<&[Idx]>,
    ) -> Result<()> {
        let sdim = mesh.spatial_dimension();
        let mut igfem_nodes = Array::new(sdim);
        self.extract_values_at_standard_nodes(mesh, mesh.nodes().view(), &mut igfem_nodes, ghost)?;
        self.interpolate_on_integration_points(
            mesh,
            igfem_nodes.view(),
            out,
            sdim,
            ty,
            ghost,
            filter,
        )
    }

    /// Upstream `inverseMap(real, element, natural, ghost)`: natural
    /// coordinates of `real` in the **parent** element.
    ///
    /// # Errors
    /// Not an IGFEM type, malformed mesh, non-converged inverse map.
    pub fn inverse_map(&self, mesh: &Mesh, real: &[Real], element: Element) -> Result<Vec<Real>> {
        let tk = TypeKernels::new(element.ty())?;
        let x = element_coords(mesh, element)?;
        let sdim = mesh.spatial_dimension();
        inverse_map_parent(&tk, &x, sdim, real)
    }

    /// Upstream `inverseMap(real, element, natural, sub_element, ghost)`:
    /// natural coordinates of `real` in sub-element `s`.
    ///
    /// # Errors
    /// Not an IGFEM type, `s ≥ 2`, malformed mesh, non-converged inverse map.
    pub fn inverse_map_sub(
        &self,
        mesh: &Mesh,
        real: &[Real],
        element: Element,
        s: usize,
    ) -> Result<Vec<Real>> {
        let tk = TypeKernels::new(element.ty())?;
        if s >= NB_SUB_ELEMENTS {
            return Err(Error::OutOfRange {
                what: "IGFEM sub-element",
                index: s,
                len: NB_SUB_ELEMENTS,
            });
        }
        let x = element_coords(mesh, element)?;
        inverse_map_sub(&tk, &x, mesh.spatial_dimension(), real, s)
    }

    /// Upstream `contains(real, element, ghost)`: whether the parent
    /// natural coordinates of `real` lie in the parent's reference element
    /// (triangle or segment test, default `Math::tolerance`). UD-606 (B, no
    /// compat): upstream's version calls `ElementClass<igfem type>::contains`,
    /// which does not compile at the reference commit (the IGFEM
    /// `GeometricalElement` specialisations declare no `contains`); the
    /// pre-2020 API it was written against would have applied the default
    /// `_gst_point` test `ξ₀ < DBL_EPSILON`. There is no reference behaviour
    /// to reproduce, so the port implements the evident intent only.
    ///
    /// # Errors
    /// See [`IgfemFeEngine::inverse_map`].
    pub fn contains(&self, mesh: &Mesh, real: &[Real], element: Element) -> Result<bool> {
        let xi = self.inverse_map(mesh, real, element)?;
        let shape = parent_type(element.ty())?.geometrical_type().info().shape;
        Ok(akantu_fe::geometry::contains(
            shape,
            &xi,
            akantu_core::math::DEFAULT_TOLERANCE,
        ))
    }

    /// Upstream `computeShapes(real, element, shapes, ghost)`: parent
    /// shapes at the parent natural coordinates of `real`, then the
    /// enrichments of the sub-element containing it (tested with
    /// `Math::tolerance = 1e-14`; sub-element 0 first).
    ///
    /// # Errors
    /// Not an IGFEM type, malformed mesh, non-converged inverse map, or a
    /// point contained in neither sub-element (upstream debug assertion
    /// "Physical point not contained in any element").
    pub fn compute_shapes_at(
        &self,
        mesh: &Mesh,
        real: &[Real],
        element: Element,
        shapes: &mut [Real],
    ) -> Result<()> {
        let tk = TypeKernels::new(element.ty())?;
        let x = element_coords(mesh, element)?;
        compute_shapes_at_point(&tk, &x, mesh.spatial_dimension(), real, shapes)
    }

    /// Upstream `computeShapeDerivatives(real_coords, …)`:
    /// `AKANTU_DEBUG_TO_IMPLEMENT` (UD-605).
    ///
    /// # Errors
    /// Always.
    pub fn compute_shape_derivatives_at(&self) -> Result<()> {
        Err(Error::NotImplemented(
            "ShapeLagrange<_ek_igfem>::computeShapeDerivatives at physical points \
             (AKANTU_DEBUG_TO_IMPLEMENT upstream)",
        ))
    }

    /// Upstream `interpolate(real, nodal_values, interpolated, element)`:
    /// `nodal_values · N(real)` (`nodal_values` column-major `rows × nn`).
    ///
    /// # Errors
    /// See [`IgfemFeEngine::compute_shapes_at`]; wrong sizes.
    pub fn interpolate(
        &self,
        mesh: &Mesh,
        real: &[Real],
        nodal_values: &[Real],
        element: Element,
    ) -> Result<Vec<Real>> {
        let nn = element.ty().nb_nodes_per_interpolation_element();
        if nn == 0 || !nodal_values.len().is_multiple_of(nn) {
            return Err(Error::DimensionMismatch {
                context: "IGFEM interpolate: nodal values (rows × nb_shapes)",
                expected: nn,
                got: nodal_values.len(),
            });
        }
        let rows = nodal_values.len() / nn;
        let mut n = vec![0.0; nn];
        self.compute_shapes_at(mesh, real, element, &mut n)?;
        let mut out = vec![0.0; rows];
        eigen::product_slices(
            Dims::dynamic(rows, nn),
            Dims::dynamic(nn, 1),
            nodal_values,
            &n,
            &mut out,
        )?;
        Ok(out)
    }

    /// Upstream `interpolateOnPhysicalPoint(real, element, field,
    /// interpolated, ghost)`: [`IgfemFeEngine::interpolate`] of the element
    /// values of the nodal `field`.
    ///
    /// # Errors
    /// See [`IgfemFeEngine::interpolate`].
    pub fn interpolate_on_physical_point(
        &self,
        mesh: &Mesh,
        real: &[Real],
        element: Element,
        field: &Array<Real>,
    ) -> Result<Vec<Real>> {
        let values = mesh.extract_nodal_values_from_element(field, element)?;
        self.interpolate(mesh, real, &values, element)
    }

    /// Upstream `interpolateAtEnrichedNodes<type>(src, dst, ghost)`: the
    /// physical value of `src` at every enriched node of the IGFEM elements
    /// of `(ty, ghost)`, written into `dst` at the node (precomputed shapes
    /// at the enriched nodes).
    ///
    /// # Errors
    /// Wrong sizes, missing/stale block.
    pub fn interpolate_at_enriched_nodes(
        &self,
        mesh: &Mesh,
        src: &Array<Real>,
        dst: &mut Array<Real>,
        ty: ElementType,
        ghost: GhostType,
    ) -> Result<()> {
        let b = self.block(mesh, ty, ghost)?;
        let nb_comp = src.nb_component();
        if dst.nb_component() != nb_comp || dst.len() != src.len() || src.len() != mesh.nb_nodes() {
            return Err(Error::DimensionMismatch {
                context: "IGFEM interpolateAtEnrichedNodes: src and dst nodal arrays",
                expected: mesh.nb_nodes() * nb_comp,
                got: dst.len() * dst.nb_component(),
            });
        }
        let conn = mesh.connectivity(ty, ghost)?;
        let nn = b.shapes_at_enrichments.nb_component();
        let np = nb_parent_nodes(ty)?;
        let ne = nb_enrichments(ty)?;
        let mut ue = vec![0.0; nb_comp * nn];
        let mut val = vec![0.0; nb_comp];
        let s = b.shapes_at_enrichments.as_slice();
        for e in 0..b.nb_element {
            gather(&conn, e, src.as_slice(), nb_comp, mesh.nb_nodes(), &mut ue)?;
            for i in 0..ne {
                eigen::product_slices(
                    Dims::dynamic(nb_comp, nn),
                    Dims::dynamic(nn, 1),
                    &ue,
                    &s[(e * ne + i) * nn..(e * ne + i + 1) * nn],
                    &mut val,
                )?;
                let node = try_to_usize(conn.tuple(e)[np + i])?;
                dst.tuple_mut(node).copy_from_slice(&val);
            }
        }
        Ok(())
    }

    /// Upstream `interpolateEnrichmentsAllTypes(src, dst, type, ghost)`.
    ///
    /// # Errors
    /// See [`IgfemFeEngine::interpolate_at_enriched_nodes`].
    pub fn interpolate_enrichments_all_types(
        &self,
        mesh: &Mesh,
        src: &Array<Real>,
        dst: &mut Array<Real>,
        ty: ElementType,
        ghost: GhostType,
    ) -> Result<()> {
        self.interpolate_at_enriched_nodes(mesh, src, dst, ty, ghost)
    }
}

/// Column-major `sdim × nnpe` node coordinates of `element`.
fn element_coords(mesh: &Mesh, element: Element) -> Result<Vec<Real>> {
    mesh.extract_nodal_values_from_element(&mesh.nodes(), element)
}

fn inverse_map_parent(
    tk: &TypeKernels,
    x: &[Real],
    sdim: usize,
    real: &[Real],
) -> Result<Vec<Real>> {
    check_len("IGFEM inverse_map: real coordinates", sdim, real.len())?;
    let mut parent = vec![0.0; sdim * tk.np];
    element::parent_coords(tk.ty, x, sdim, &mut parent)?;
    let mut xi = vec![0.0; tk.parent.nd];
    (tk.parent.inverse_map)(
        real,
        &parent,
        &mut xi,
        INVERSE_MAP_MAX_ITERATIONS,
        INVERSE_MAP_TOLERANCE,
    )?;
    Ok(xi)
}

fn inverse_map_sub(
    tk: &TypeKernels,
    x: &[Real],
    sdim: usize,
    real: &[Real],
    s: usize,
) -> Result<Vec<Real>> {
    check_len("IGFEM inverse_map: real coordinates", sdim, real.len())?;
    let k = tk.sub[s];
    let mut sub = vec![0.0; sdim * k.nn];
    sub_element_coords(tk.ty, x, sdim, s, &mut sub)?;
    let mut xi = vec![0.0; k.nd];
    (k.inverse_map)(
        real,
        &sub,
        &mut xi,
        INVERSE_MAP_MAX_ITERATIONS,
        INVERSE_MAP_TOLERANCE,
    )?;
    Ok(xi)
}

fn compute_shapes_at_point(
    tk: &TypeKernels,
    x: &[Real],
    sdim: usize,
    real: &[Real],
    shapes: &mut [Real],
) -> Result<()> {
    check_len("IGFEM computeShapes: shapes", tk.nn, shapes.len())?;
    let xi = inverse_map_parent(tk, x, sdim, real)?;
    let mut parent_shapes = vec![0.0; tk.parent.nn];
    (tk.parent.shapes)(&xi, &mut parent_shapes);
    for s in 0..NB_SUB_ELEMENTS {
        let xs = inverse_map_sub(tk, x, sdim, real, s)?;
        let k = tk.sub[s];
        if (k.contains)(&xs, COMPUTE_SHAPES_TOLERANCE) {
            let mut sub_shapes = vec![0.0; k.nn];
            (k.shapes)(&xs, &mut sub_shapes);
            return assemble_shapes(tk.ty, &parent_shapes, &sub_shapes, shapes, s);
        }
    }
    Err(Error::InvalidArgument(format!(
        "{}: physical point {real:?} not contained in any sub-element",
        tk.ty
    )))
}

/// Precompute one `(type, ghost)` block (upstream `initIntegrator` +
/// `ShapeLagrange<_ek_igfem>::initShapeFunctions`).
fn precompute(mesh: &Mesh, ty: ElementType, ghost: GhostType) -> Result<IgfemBlock> {
    let tk = TypeKernels::new(ty)?;
    let sdim = mesh.spatial_dimension();
    let conn = mesh.connectivity(ty, ghost)?;
    let ne = conn.len();
    let nnpe = ty.nb_nodes_per_element();
    if conn.nb_component() != nnpe {
        return Err(Error::DimensionMismatch {
            context: "IGFEM initShapeFunctions: connectivity",
            expected: nnpe,
            got: conn.nb_component(),
        });
    }
    if mesh.nodes().nb_component() != sdim {
        return Err(Error::DimensionMismatch {
            context: "IGFEM initShapeFunctions: node coordinates",
            expected: sdim,
            got: mesh.nodes().nb_component(),
        });
    }
    let nd = tk.nd;
    if sdim < nd {
        return Err(Error::DimensionMismatch {
            context: "IGFEM initShapeFunctions: mesh dimension below the element dimension",
            expected: nd,
            got: sdim,
        });
    }
    let (n1, n2) = (tk.nq(0), tk.nq(1));
    let nq = n1 + n2;
    let nn = tk.nn;
    // computeQuadraturePoints: sub-element 0 points, then sub-element 1
    let mut points = MatrixX::try_zeros(nd, nq)?;
    for (s, offset) in [(0, 0), (1, n1)] {
        for q in 0..tk.nq(s) {
            let p = tk.rules[s].point(q, nd);
            points.as_mut_slice()[(offset + q) * nd..(offset + q + 1) * nd].copy_from_slice(p);
        }
    }
    let mut jacobians = Array::try_zeros(ne * nq, 1)?;
    let mut shapes = Array::try_zeros(ne * nq, nn)?;
    let with_derivatives = nd == sdim;
    let mut dshapes = if with_derivatives {
        Some(Array::try_zeros(ne * nq, nd * nn)?)
    } else {
        None
    };
    let mut nat_parent = Array::try_zeros(ne * nq, nd)?;
    let mut at_enr = Array::try_zeros(ne * tk.nb_enr, nn)?;

    // sub-element shapes (and natural derivatives) at the rule points
    let sub_n: [Vec<Real>; 2] = [0, 1].map(|s| {
        let k = tk.sub[s];
        let mut v = vec![0.0; k.nn * tk.nq(s)];
        for q in 0..tk.nq(s) {
            (k.shapes)(tk.rules[s].point(q, nd), &mut v[q * k.nn..(q + 1) * k.nn]);
        }
        v
    });
    let sub_dnds: [Vec<Real>; 2] = [0, 1].map(|s| {
        let k = tk.sub[s];
        let mut v = vec![0.0; nd * k.nn * tk.nq(s)];
        for q in 0..tk.nq(s) {
            (k.dnds)(
                tk.rules[s].point(q, nd),
                &mut v[q * nd * k.nn..(q + 1) * nd * k.nn],
            );
        }
        v
    });

    let mut x = vec![0.0; sdim * nnpe];
    let mut parent = vec![0.0; sdim * tk.np];
    let mut sub_x = [
        vec![0.0; sdim * tk.sub[0].nn],
        vec![0.0; sdim * tk.sub[1].nn],
    ];
    let mut phys = vec![0.0; sdim * n1.max(n2)];
    let mut xi = vec![0.0; nd];
    let mut parent_n = vec![0.0; tk.np];
    let mut parent_dnds = vec![0.0; nd * tk.np];
    let mut jm = vec![0.0; nd * sdim];
    let mut b_parent = vec![0.0; nd * tk.np];
    let mut b_sub = vec![0.0; nd * tk.sub[0].nn.max(tk.sub[1].nn)];
    let mut all_n = vec![0.0; nn];
    let mut all_b = vec![0.0; nd * nn];
    let nodes_g = mesh.nodes();
    let nodes = nodes_g.as_slice();
    for e in 0..ne {
        gather(&conn, e, nodes, sdim, mesh.nb_nodes(), &mut x)?;
        element::parent_coords(ty, &x, sdim, &mut parent)?;
        for (s, sx) in sub_x.iter_mut().enumerate() {
            sub_element_coords(ty, &x, sdim, s, sx)?;
        }
        // --- integrator: det J_sub · w_sub, negative check ---------------
        for (s, offset) in [(0, 0), (1, n1)] {
            let k = tk.sub[s];
            for q in 0..tk.nq(s) {
                let d = &sub_dnds[s][q * nd * k.nn..(q + 1) * nd * k.nn];
                kernels::jmat(
                    d,
                    Dims::fixed(nd, k.nn),
                    &sub_x[s],
                    Dims::dynamic(sdim, k.nn),
                    &mut jm,
                )?;
                let j = kernels::jacobian(&jm, nd, sdim)?;
                let slot = e * nq + offset + q;
                jacobians.as_mut_slice()[slot] = j;
            }
        }
        for q in 0..nq {
            let j = jacobians.as_slice()[e * nq + q];
            if j < 0.0 {
                return Err(Error::Numerical(format!(
                    "Negative jacobian computed, possible problem in the element node ordering \
                     (Quadrature Point {q}:{e}:{}:{})",
                    ty.name(),
                    ghost.name()
                )));
            }
        }
        for (s, offset) in [(0, 0), (1, n1)] {
            for (q, w) in tk.rules[s].weights.iter().enumerate() {
                jacobians.as_mut_slice()[e * nq + offset + q] *= *w;
            }
        }
        // --- shapes: map sub-element points to the parent ----------------
        for (s, offset) in [(0, 0), (1, n1)] {
            let k = tk.sub[s];
            let nqs = tk.nq(s);
            // physical points = X_sub · N_sub (dynamic operands)
            eigen::product_slices(
                Dims::dynamic(sdim, k.nn),
                Dims::dynamic(k.nn, nqs),
                &sub_x[s],
                &sub_n[s],
                &mut phys[..sdim * nqs],
            )?;
            for q in 0..nqs {
                (tk.parent.inverse_map)(
                    &phys[q * sdim..(q + 1) * sdim],
                    &parent,
                    &mut xi,
                    INVERSE_MAP_MAX_ITERATIONS,
                    INVERSE_MAP_TOLERANCE,
                )?;
                (tk.parent.shapes)(&xi, &mut parent_n);
                assemble_shapes(
                    ty,
                    &parent_n,
                    &sub_n[s][q * k.nn..(q + 1) * k.nn],
                    &mut all_n,
                    s,
                )?;
                let t = e * nq + offset + q;
                shapes.tuple_mut(t).copy_from_slice(&all_n);
                nat_parent.tuple_mut(t).copy_from_slice(&xi);
            }
        }
        // --- physical derivatives ---------------------------------------
        if let Some(ds) = dshapes.as_mut() {
            for (s, offset) in [(0, 0), (1, n1)] {
                let k = tk.sub[s];
                for q in 0..tk.nq(s) {
                    // sub-element J⁻¹·dnds at its rule point (dynamic)
                    let d = &sub_dnds[s][q * nd * k.nn..(q + 1) * nd * k.nn];
                    kernels::jmat(
                        d,
                        Dims::dynamic(nd, k.nn),
                        &sub_x[s],
                        Dims::dynamic(sdim, k.nn),
                        &mut jm,
                    )?;
                    let bs = &mut b_sub[..nd * k.nn];
                    kernels::shape_derivatives(&jm, d, Dims::dynamic(nd, k.nn), bs)?;
                    // parent J⁻¹·dnds at the parent natural coordinates of
                    // the point (UD-603: index offset + q)
                    let t = e * nq + offset + q;
                    (tk.parent.dnds)(nat_parent.tuple(t), &mut parent_dnds);
                    kernels::jmat(
                        &parent_dnds,
                        Dims::dynamic(nd, tk.np),
                        &parent,
                        Dims::dynamic(sdim, tk.np),
                        &mut jm,
                    )?;
                    kernels::shape_derivatives(
                        &jm,
                        &parent_dnds,
                        Dims::dynamic(nd, tk.np),
                        &mut b_parent,
                    )?;
                    assemble_shape_derivatives(ty, nd, &b_parent, bs, &mut all_b, s)?;
                    ds.tuple_mut(t).copy_from_slice(&all_b);
                }
            }
        }
        // --- shapes at the enriched nodes --------------------------------
        let enr0 = sub_element_enrichments(ty, 0)?;
        let mut sub_unit = vec![0.0; tk.sub[0].nn];
        for i in 0..tk.nb_enr {
            let real = &x[(tk.np + i) * sdim..(tk.np + i + 1) * sdim];
            (tk.parent.inverse_map)(
                real,
                &parent,
                &mut xi,
                INVERSE_MAP_MAX_ITERATIONS,
                INVERSE_MAP_TOLERANCE,
            )?;
            (tk.parent.shapes)(&xi, &mut parent_n);
            sub_unit.fill(0.0);
            sub_unit[usize::from(enr0[i])] = 1.0;
            assemble_shapes(ty, &parent_n, &sub_unit, &mut all_n, 0)?;
            at_enr.tuple_mut(e * tk.nb_enr + i).copy_from_slice(&all_n);
        }
    }
    Ok(IgfemBlock {
        sub_rules: tk.rules.clone(),
        points,
        nb_element: ne,
        jacobians,
        shapes,
        shapes_derivatives: dshapes,
        igfem_integration_points: nat_parent,
        shapes_at_enrichments: at_enr,
    })
}
