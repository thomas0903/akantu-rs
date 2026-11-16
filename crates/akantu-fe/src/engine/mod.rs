// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/fe_engine/fe_engine.hh, fe_engine.cc,
// fe_engine_inline_impl.hh, fe_engine_template.hh, fe_engine_template_tmpl.hh,
// fe_engine_template_cohesive.cc, integrator_gauss.hh,
// integrator_gauss_inline_impl.hh, integrator.hh, shape_functions*.hh,
// shape_lagrange*.hh, shape_lagrange_base*, shape_cohesive*.hh.

//! The FE engine (upstream `FEEngine` / `FEEngineTemplate<IntegratorGauss,
//! ShapeLagrange, kind>`), FE-013…039.
//!
//! Interface contract for models and materials. Conventions:
//! * `filter: Option<&[Idx]>` restricts a batch operation to a subset of the
//!   `(type, ghost)` block (upstream `filter_elements`, `empty_filter` =
//!   `None`); outputs are then sized by the filter length;
//! * outputs are resized (and given the right number of components) by the
//!   callee;
//! * nodal inputs are views (`ArrayView<Real>`), so DOF-manager arrays,
//!   numpy buffers and temporaries can be passed without copies;
//! * the engine never stores a mesh reference: every method takes `&Mesh`
//!   (it reads node coordinates, connectivities and spatial dimension only).
//!
//! **Evaluation order.** Every product reproduces the Eigen operand types of
//! the upstream call site ([`crate::eigen`]) and is evaluated by
//! `akantu_core::eigen_gebp`, including the products Eigen sends to its
//! blocked GEMM kernel (`docs/NUMERICAL_VALIDATION.md`; parity
//! summary in ARCHITECTURE.md, "Extension points").

mod assembly;
mod elemental;
mod fast;
mod normals;
mod ops;
mod queries;

pub use assembly::{FieldFunctor, LumpingScheme, diagonal_scaling_factors};
pub use elemental::{ElementalFieldInterpolation, PointInterpolation};
pub use queries::{compute_shapes_at_natural, element_inradius_of};

use akantu_core::array_shared::ArrayRef;
use akantu_core::enums::EventPriority;
use akantu_core::tensor::MatrixX;
use akantu_core::{
    Array, ElementKind, ElementType, ElementTypeMap, ElementTypeMapArray, Error, GhostType, Id,
    Idx, Real, Result, TypeFilter,
};
use akantu_mesh::{Mesh, MeshEvent, MeshEventHandler};

use crate::eigen::Dims;
use crate::element_class::{ElementClass, LagrangeElement};
use crate::kernels;
use crate::quadrature::{QuadratureRule, gauss_rule};
use crate::shape::{IntegrationOrders, PrecomputedShapes, ShapeFamily};
use crate::with_element_class;

/// Finite-element engine for the elements of one dimension and kind of a
/// mesh.
#[derive(Debug)]
pub struct FeEngine {
    id: Id,
    element_dimension: usize,
    kind: ElementKind,
    family: ShapeFamily,
    orders: IntegrationOrders,
    /// Quadrature rule per `(type, ghost)` (upstream
    /// `IntegratorGauss::quadrature_points` + `ShapeFunctions::integration_points`).
    rules: ElementTypeMap<QuadratureRule>,
    precomputed: PrecomputedShapes,
    /// Upstream `normals_on_integration_points`.
    normals: ElementTypeMapArray<Real>,
    /// Mesh revision the precomputed data corresponds to.
    mesh_revision: u64,
    /// How the batch kernels run (threads, precision mode, SIMD level;
    /// D-123): serial until the owning model sets its own.
    exec: akantu_core::exec::Exec,
}

// ---------------------------------------------------------------------------
// Geometry access helpers
// ---------------------------------------------------------------------------

/// Node coordinates and connectivity of one `(type, ghost)` block (holds
/// the guards of the mesh's exportable arrays for the operation, ARCHITECTURE.md, "Shared storage and its aliasing rules").
pub(crate) struct Block<'m> {
    pub(crate) ty: ElementType,
    pub(crate) ghost: GhostType,
    nodes: ArrayRef<'m, Real>,
    /// Coordinates used instead of the mesh nodes (see
    /// [`Block::override_nodes`]).
    nodes_override: Option<&'m [Real]>,
    pub(crate) nb_nodes: usize,
    pub(crate) sdim: usize,
    conn: Option<ArrayRef<'m, Idx>>,
    pub(crate) nnpe: usize,
    pub(crate) nb_element: usize,
}

impl<'m> Block<'m> {
    /// Geometry of the `(ty, ghost)` block of `mesh`, validated before any
    /// numerical work: the node coordinates hold
    /// `spatial_dimension` values per node and the connectivity
    /// `nb_nodes_per_element(ty)` values per element. Malformed storage
    /// (e.g. replaced through [`akantu_mesh::MeshAccessor`]) is an error,
    /// never an out-of-bounds access; node ids are checked per access by
    /// [`Block::node`].
    pub(crate) fn new(mesh: &'m Mesh, ty: ElementType, ghost: GhostType) -> Result<Self> {
        let sdim = mesh.spatial_dimension();
        let nodes = mesh.nodes();
        if nodes.nb_component() != sdim {
            return Err(Error::DimensionMismatch {
                context: "FE engine: mesh node coordinates (components = spatial dimension)",
                expected: sdim,
                got: nodes.nb_component(),
            });
        }
        let nnpe = ty.nb_nodes_per_element();
        let (conn, nb_element): (Option<ArrayRef<'m, Idx>>, usize) = match mesh
            .connectivity(ty, ghost)
        {
            Ok(c) => {
                if c.nb_component() != nnpe {
                    return Err(Error::DimensionMismatch {
                        context: "FE engine: mesh connectivity (components = nodes per element)",
                        expected: nnpe,
                        got: c.nb_component(),
                    });
                }
                let n = c.len();
                (Some(c), n)
            }
            Err(_) => (None, 0),
        };
        let conn_len = conn.as_ref().map_or(0, |c| c.as_slice().len());
        if conn_len != nb_element * nnpe || nodes.as_slice().len() != nodes.len() * sdim {
            return Err(Error::Mesh(format!(
                "FE engine: inconsistent storage of the {} ({}) block",
                ty.name(),
                ghost.name()
            )));
        }
        let nb_nodes = nodes.len();
        Ok(Self {
            ty,
            ghost,
            nodes,
            nodes_override: None,
            nb_nodes,
            sdim,
            conn,
            nnpe,
            nb_element,
        })
    }

    /// Node coordinates (`nb_nodes × sdim`): the override if one is set.
    pub(crate) fn nodes(&self) -> &[Real] {
        self.nodes_override.unwrap_or_else(|| self.nodes.as_slice())
    }

    /// Use `nodes` (`nb_nodes × sdim`, e.g. current positions) instead of
    /// the mesh node coordinates for every geometric computation of this
    /// block (the mesh's guard stays held: the override is borrowed, not
    /// written into the exportable mesh array). Replaces the pre-migration
    /// `block.nodes = …` of engines that integrate on deformed coordinates
    /// (contact).
    pub(crate) fn override_nodes(&mut self, nodes: &'m [Real]) -> Result<()> {
        if nodes.len() != self.nb_nodes * self.sdim {
            return Err(Error::DimensionMismatch {
                context: "FE engine: node override (nb_nodes × spatial dimension)",
                expected: self.nb_nodes * self.sdim,
                got: nodes.len(),
            });
        }
        self.nodes_override = Some(nodes);
        Ok(())
    }

    /// Connectivity values (`nb_element × nnpe`).
    pub(crate) fn conn(&self) -> &[Idx] {
        self.conn.as_ref().map_or(&[], |c| c.as_slice())
    }

    /// Node `n` of element `e`, validated (checked access: an element or
    /// local node beyond the block is an error).
    #[inline]
    pub(crate) fn node(&self, e: usize, n: usize) -> Result<usize> {
        // errors built only on failure
        let Some(&v) = e
            .checked_mul(self.nnpe)
            .and_then(|i| i.checked_add(n))
            .filter(|_| n < self.nnpe)
            .and_then(|i| self.conn().get(i))
        else {
            return Err(Error::OutOfRange {
                what: "connectivity entry",
                index: e,
                len: self.nb_element,
            });
        };
        let u = usize::try_from(v).map_err(|_| Error::InvalidIndex {
            what: "connectivity node",
            value: i64::from(v),
        })?;
        if u >= self.nb_nodes {
            return Err(Error::OutOfRange {
                what: "connectivity node",
                index: u,
                len: self.nb_nodes,
            });
        }
        Ok(u)
    }

    /// Gather the values of `field` (`nb_dof` per node) at the nodes of
    /// element `e` into `out` (node-major = column-major `nb_dof × nnpe`;
    /// upstream `extractNodalToElementField`).
    #[inline]
    pub(crate) fn gather(
        &self,
        e: usize,
        field: &[Real],
        nb_dof: usize,
        out: &mut [Real],
    ) -> Result<()> {
        for n in 0..self.nnpe {
            let node = self.node(e, n)?;
            let Some(src) = field.get(node * nb_dof..(node + 1) * nb_dof) else {
                return Err(Error::OutOfRange {
                    what: "nodal field node",
                    index: node,
                    len: field.len() / nb_dof.max(1),
                });
            };
            let got = out.len();
            let Some(dst) = out.get_mut(n * nb_dof..(n + 1) * nb_dof) else {
                return Err(Error::DimensionMismatch {
                    context: "gather: element buffer",
                    expected: self.nnpe * nb_dof,
                    got,
                });
            };
            dst.copy_from_slice(src);
        }
        Ok(())
    }

    /// Node coordinates of element `e` (column-major `sdim × nnpe`).
    #[inline]
    pub(crate) fn coords(&self, e: usize, out: &mut [Real]) -> Result<()> {
        self.gather(e, self.nodes(), self.sdim, out)
    }
}

/// Validated element filter (upstream `filter_elements`): `None` means all
/// `nb_element` elements.
pub(crate) fn resolve_filter(
    filter: Option<&[Idx]>,
    nb_element: usize,
) -> Result<Option<Vec<usize>>> {
    filter
        .map(|f| {
            f.iter()
                .map(|&e| {
                    let u = usize::try_from(e).map_err(|_| Error::InvalidIndex {
                        what: "element filter",
                        value: i64::from(e),
                    })?;
                    if u >= nb_element {
                        return Err(Error::OutOfRange {
                            what: "element filter",
                            index: u,
                            len: nb_element,
                        });
                    }
                    Ok(u)
                })
                .collect()
        })
        .transpose()
}

/// Element ids of a resolved filter (or `0..nb_element`); takes the
/// resolved filter by value (no copy).
pub(crate) fn element_ids(filter: Option<Vec<usize>>, nb_element: usize) -> Vec<usize> {
    match filter {
        Some(f) => f,
        None => (0..nb_element).collect(),
    }
}

/// Resize `out` to `len` tuples of `nb_component` values (re-created if the
/// number of components differs, keeping its id).
pub(crate) fn prepare_out(out: &mut Array<Real>, len: usize, nb_component: usize) -> Result<()> {
    if out.nb_component() != nb_component {
        *out = Array::with_id(nb_component, out.id().to_owned());
    }
    out.try_resize(len, 0.0)
}

/// Check that a precomputed per-quadrature-point array still matches the
/// mesh block it is applied to (`nb_element · nq` tuples): a mesh changed
/// after `init_shape_functions` without forwarding its events must give an
/// error, never an out-of-bounds access.
pub(crate) fn check_precomputed(
    context: &'static str,
    len: usize,
    nb_element: usize,
    nq: usize,
) -> Result<()> {
    if len != nb_element * nq {
        return Err(Error::DimensionMismatch {
            context,
            expected: nb_element * nq,
            got: len,
        });
    }
    Ok(())
}

/// Check the shape of an input array.
pub(crate) fn check_input(
    context: &'static str,
    len: usize,
    nbc: usize,
    exp_len: usize,
    exp_nbc: usize,
) -> Result<()> {
    if nbc != exp_nbc {
        return Err(Error::DimensionMismatch {
            context,
            expected: exp_nbc,
            got: nbc,
        });
    }
    if len != exp_len {
        return Err(Error::DimensionMismatch {
            context,
            expected: exp_len,
            got: len,
        });
    }
    Ok(())
}

impl FeEngine {
    /// New engine on the elements of `element_dimension` (default: mesh
    /// dimension) and `kind` of `mesh` (upstream constructor with
    /// `do_not_precompute = true`; call [`FeEngine::init_shape_functions`]
    /// for both ghost types, or use [`FeEngine::with_precomputation`]).
    pub fn new(
        mesh: &Mesh,
        element_dimension: Option<usize>,
        kind: ElementKind,
        family: ShapeFamily,
        id: impl Into<Id>,
    ) -> Result<Self> {
        let id = id.into();
        if family == ShapeFamily::Structural || kind == ElementKind::Structural {
            return Err(Error::NotImplemented(
                "structural shape functions in FeEngine: structural elements use \
                 akantu_fe::structural::StructuralFeEngine",
            ));
        }
        if !matches!(kind, ElementKind::Regular | ElementKind::Cohesive) {
            return Err(Error::Unsupported(format!(
                "FE engine kind {kind:?} (regular and cohesive kinds are supported)"
            )));
        }
        Ok(Self {
            precomputed: PrecomputedShapes {
                shapes: ElementTypeMapArray::new(format!("{id}:shapes")),
                shapes_derivatives: ElementTypeMapArray::new(format!("{id}:shapes_derivatives")),
                jacobians: ElementTypeMapArray::new(format!("{id}:jacobians")),
            },
            normals: ElementTypeMapArray::new(format!("{id}:normals_on_quad_points")),
            id,
            element_dimension: element_dimension.unwrap_or(mesh.spatial_dimension()),
            kind,
            family,
            orders: IntegrationOrders::default(),
            rules: ElementTypeMap::new(),
            mesh_revision: mesh.revision(),
            exec: akantu_core::exec::Exec::serial(),
        })
    }

    /// Upstream constructor (`do_not_precompute = false`): build and
    /// precompute shapes, derivatives and Jacobians for both ghost types.
    pub fn with_precomputation(
        mesh: &Mesh,
        element_dimension: Option<usize>,
        kind: ElementKind,
        family: ShapeFamily,
        id: impl Into<Id>,
    ) -> Result<Self> {
        let mut fe = Self::new(mesh, element_dimension, kind, family, id)?;
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

    /// Kind of the handled elements.
    #[must_use]
    pub fn kind(&self) -> ElementKind {
        self.kind
    }

    /// Shape family.
    #[must_use]
    pub fn family(&self) -> ShapeFamily {
        self.family
    }

    /// Mesh revision of the precomputed data.
    #[must_use]
    pub fn mesh_revision(&self) -> u64 {
        self.mesh_revision
    }

    /// How the batch kernels (gradient, `BᵀD`, integration) run (D-123).
    #[must_use]
    pub fn exec(&self) -> &akantu_core::exec::Exec {
        &self.exec
    }

    /// Set how the batch kernels run (the owning model's executor). In
    /// [`akantu_core::exec::Precision::Bitwise`] every result is the same
    /// for every thread count and SIMD level.
    pub fn set_exec(&mut self, exec: akantu_core::exec::Exec) {
        self.exec = exec;
    }

    /// Configured integration orders.
    #[must_use]
    pub fn integration_orders(&self) -> &IntegrationOrders {
        &self.orders
    }

    /// Configure integration orders (must be called before
    /// `init_shape_functions`; upstream integration-order functor, e.g.
    /// [`IntegrationOrders::facets_cohesive`] for the facet engine of the
    /// cohesive model).
    pub fn set_integration_orders(&mut self, orders: IntegrationOrders) -> Result<()> {
        for &(ty, o) in &orders.per_type {
            gauss_rule(ty, o)?;
        }
        if self.rules.iter_all().next().is_some() {
            return Err(Error::invalid(
                "set_integration_orders must be called before init_shape_functions",
            ));
        }
        self.orders = orders;
        Ok(())
    }

    /// Element types of `mesh` handled by this engine (upstream
    /// `mesh.elementTypes(element_dimension, ghost_type, kind)`), enum order.
    pub fn element_types<'m>(
        &self,
        mesh: &'m Mesh,
        ghost: GhostType,
    ) -> impl Iterator<Item = ElementType> + 'm {
        mesh.element_types(TypeFilter {
            spatial_dimension: Some(self.element_dimension),
            ghost_type: ghost,
            kind: Some(self.kind),
        })
    }

    /// Accept only types of the engine's kind.
    fn check_type(&self, ty: ElementType, context: &'static str) -> Result<()> {
        if ty.kind() != self.kind {
            return Err(Error::UnsupportedElementType { ty, context });
        }
        Ok(())
    }

    /// Quadrature rule of `(ty, ghost)`.
    pub(crate) fn rule(&self, ty: ElementType, ghost: GhostType) -> Result<&QuadratureRule> {
        self.rules.get(ty, ghost).ok_or_else(|| {
            Error::invalid(format!(
                "no integration points for {} ({}); call init_shape_functions first",
                ty.name(),
                ghost.name()
            ))
        })
    }

    /// Precompute shapes, derivatives and Jacobians (upstream
    /// `initShapeFunctions(ghost_type)`): for every handled type, the
    /// quadrature points of the configured order, `det J · w` per point
    /// (error if any raw `det J < 0`, upstream `IntegratorGaussException`;
    /// nothing is stored for the type in that case), shapes per element and
    /// point, and shape derivatives: physical `J⁻¹·dN/dξ` for regular types
    /// whose natural dimension equals the mesh dimension, natural `dN/dξ`
    /// for cohesive types.
    pub fn init_shape_functions(&mut self, mesh: &Mesh, ghost: GhostType) -> Result<()> {
        let types: Vec<ElementType> = self.element_types(mesh, ghost).collect();
        for ty in types {
            let order = self.orders.order(ty);
            let rule = gauss_rule(ty, order)?;
            let block = Block::new(mesh, ty, ghost)?;
            let kind = self.kind;
            let computed = with_element_class!(ty, E => {
                init_type::<E>(&block, &rule, kind, mesh.spatial_dimension())
            }, else Err(Error::UnsupportedElementType { ty, context: "init_shape_functions" }))?;
            let (shapes, dshapes, jacobians) = computed;
            self.store(ty, ghost, shapes, dshapes, jacobians);
            self.rules.replace(ty, ghost, rule);
        }
        self.mesh_revision = mesh.revision();
        Ok(())
    }

    /// Upstream `initShapeFunctions(nodes, ghost_type)`: as
    /// [`FeEngine::init_shape_functions`], with the node coordinates
    /// `nodes` (`nb_nodes × spatial_dimension`, e.g. current positions)
    /// instead of the mesh's (connectivities still from `mesh`). Used by the
    /// contact model's nodal areas (`contact_mechanics_model.cc:417-418`).
    /// Added.
    ///
    /// Like upstream, the engine's cache stamp (`mesh_revision`) is set to
    /// the mesh's revision although the jacobians were computed on `nodes`,
    /// not on the mesh nodes: the engine then claims to
    /// match the mesh. Harmless for the contact model's private boundary
    /// engine, which re-initialises it before every use; an engine shared
    /// with mesh-coordinate computations must be re-initialised with
    /// [`FeEngine::init_shape_functions`] before such use.
    pub fn init_shape_functions_with_nodes(
        &mut self,
        mesh: &Mesh,
        nodes: akantu_core::array::ArrayView<'_, Real>,
        ghost: GhostType,
    ) -> Result<()> {
        if nodes.nb_component() != mesh.spatial_dimension() || nodes.len() != mesh.nb_nodes() {
            return Err(Error::DimensionMismatch {
                context: "init_shape_functions_with_nodes: nodes (nb_nodes × spatial_dimension)",
                expected: mesh.nb_nodes() * mesh.spatial_dimension(),
                got: nodes.len() * nodes.nb_component(),
            });
        }
        let types: Vec<ElementType> = self.element_types(mesh, ghost).collect();
        for ty in types {
            let order = self.orders.order(ty);
            let rule = gauss_rule(ty, order)?;
            let mut block = Block::new(mesh, ty, ghost)?;
            block.override_nodes(nodes.as_slice())?;
            let kind = self.kind;
            let computed = with_element_class!(ty, E => {
                init_type::<E>(&block, &rule, kind, mesh.spatial_dimension())
            }, else Err(Error::UnsupportedElementType { ty, context: "init_shape_functions_with_nodes" }))?;
            let (shapes, dshapes, jacobians) = computed;
            self.store(ty, ghost, shapes, dshapes, jacobians);
            self.rules.replace(ty, ghost, rule);
        }
        self.mesh_revision = mesh.revision();
        Ok(())
    }

    fn store(
        &mut self,
        ty: ElementType,
        ghost: GhostType,
        shapes: Array<Real>,
        dshapes: Option<Array<Real>>,
        jac: Array<Real>,
    ) {
        let suffix = if ghost == GhostType::Ghost {
            ":ghost"
        } else {
            ""
        };
        let named = |mut a: Array<Real>, base: &str| {
            a.set_id(format!("{}:{ty}{suffix}", base));
            a
        };
        let p = &mut self.precomputed;
        let sid = p.shapes.id().to_owned();
        let did = p.shapes_derivatives.id().to_owned();
        let jid = p.jacobians.id().to_owned();
        p.shapes
            .as_map_mut()
            .replace(ty, ghost, named(shapes, &sid));
        match dshapes {
            Some(d) => {
                p.shapes_derivatives
                    .as_map_mut()
                    .replace(ty, ghost, named(d, &did));
            }
            None => {
                p.shapes_derivatives.as_map_mut().remove(ty, ghost);
            }
        }
        p.jacobians
            .as_map_mut()
            .replace(ty, ghost, named(jac, &jid));
    }

    /// Quadrature points per element (upstream `getNbIntegrationPoints`:
    /// the stored rule, or the configured order's rule if not initialised).
    pub fn nb_integration_points(&self, ty: ElementType, ghost: GhostType) -> Result<usize> {
        match self.rules.get(ty, ghost) {
            Some(r) => Ok(r.nb_points()),
            None => Ok(gauss_rule(ty, self.orders.order(ty))?.nb_points()),
        }
    }

    /// Quadrature points in natural coordinates (upstream
    /// `getIntegrationPoints`), column-major `max(1, nd) × nq`.
    pub fn integration_points(&self, ty: ElementType, ghost: GhostType) -> Result<&MatrixX> {
        Ok(&self.rule(ty, ghost)?.points)
    }

    /// Quadrature rule (points and weights) of `(ty, ghost)`.
    pub fn quadrature_rule(&self, ty: ElementType, ghost: GhostType) -> Result<&QuadratureRule> {
        self.rule(ty, ghost)
    }

    /// Precomputed shapes (upstream `getShapes`): `nb_element·nq` tuples of
    /// `nb_shapes`.
    pub fn shapes(&self, ty: ElementType, ghost: GhostType) -> Result<&Array<Real>> {
        self.precomputed.shapes.get(ty, ghost)
    }

    /// Precomputed shape derivatives (upstream `getShapesDerivatives`):
    /// `nb_element·nq` tuples of `dim·nb_shapes` (column-major
    /// `dim × nb_shapes`; physical for regular, natural for cohesive types).
    pub fn shapes_derivatives(&self, ty: ElementType, ghost: GhostType) -> Result<&Array<Real>> {
        self.precomputed.shapes_derivatives.get(ty, ghost)
    }

    /// Precomputed `det J · w` per quadrature point (upstream
    /// `IntegratorGauss::getJacobians`).
    pub fn jacobians(&self, ty: ElementType, ghost: GhostType) -> Result<&Array<Real>> {
        self.precomputed.jacobians.get(ty, ghost)
    }

    /// All precomputed data.
    #[must_use]
    pub fn precomputed(&self) -> &PrecomputedShapes {
        &self.precomputed
    }

    /// Normals at quadrature points computed by
    /// [`FeEngine::compute_normals_on_integration_points`] (upstream
    /// `getNormalsOnIntegrationPoints`).
    pub fn normals_on_integration_points(
        &self,
        ty: ElementType,
        ghost: GhostType,
    ) -> Result<&Array<Real>> {
        self.normals.get(ty, ghost)
    }
}

/// Shapes, optional shape derivatives and `det J · w` of one block.
type TypeData = (Array<Real>, Option<Array<Real>>, Array<Real>);

/// Precompute one `(type, ghost)` block: shapes, optional derivatives,
/// `det J · w`.
fn init_type<E: LagrangeElement>(
    block: &Block<'_>,
    rule: &QuadratureRule,
    kind: ElementKind,
    mesh_dim: usize,
) -> Result<TypeData> {
    let nq = rule.nb_points();
    let ne = block.nb_element;
    let ids: Vec<usize> = (0..ne).collect();
    // integrator: raw jacobians, check, weights
    let mut jac = Array::try_zeros(ne * nq, 1)?;
    compute_jacobians::<E>(block, rule, kind, &ids, &ids, jac.as_mut_slice())?;
    for (i, &j) in jac.as_slice().iter().enumerate() {
        if j < 0.0 {
            return Err(Error::Numerical(format!(
                "Negative jacobian computed, possible problem in the element node ordering \
                 (Quadrature Point {}:{}:{}:{})",
                i % nq,
                i / nq,
                block.ty.name(),
                block.ghost.name()
            )));
        }
    }
    multiply_by_weights(jac.as_mut_slice(), &rule.weights, &ids);
    // shapes
    let mut shapes = Array::try_zeros(ne * nq, E::NB_SHAPES)?;
    compute_shapes::<E>(rule, &ids, shapes.as_mut_slice());
    // derivatives
    let dshapes = if kind == ElementKind::Cohesive {
        let mut d = Array::try_zeros(ne * nq, E::NATURAL_DIM * E::NB_SHAPES)?;
        compute_natural_derivatives::<E>(rule, &ids, d.as_mut_slice());
        Some(d)
    } else if E::NATURAL_DIM == mesh_dim {
        let mut d = Array::try_zeros(ne * nq, E::NATURAL_DIM * E::NB_SHAPES)?;
        compute_physical_derivatives::<E>(block, rule, &ids, d.as_mut_slice())?;
        Some(d)
    } else {
        None
    };
    Ok((shapes, dshapes, jac))
}

/// Raw `det J` of the elements `ids` written at the slots `slots`
/// (upstream `computeJacobiansOnIntegrationPoints`; cohesive types use the
/// mid-surface, `_cohesive_1d_2` gets `1`). Operand types: `dnds` fixed
/// `Matrix<Real, nd, nn>`, node coordinates dynamic, `J` dynamic.
pub(crate) fn compute_jacobians<E: LagrangeElement>(
    block: &Block<'_>,
    rule: &QuadratureRule,
    kind: ElementKind,
    ids: &[usize],
    slots: &[usize],
    out: &mut [Real],
) -> Result<()> {
    let nq = rule.nb_points();
    let nd = E::NATURAL_DIM;
    let nn = E::NB_SHAPES;
    let sdim = block.sdim;
    let mut xe = vec![0.0; sdim * block.nnpe];
    let mut xm = vec![0.0; sdim * nn];
    let mut dnds = vec![0.0; nd * nn];
    let mut j = vec![0.0; nd * sdim];
    for (&e, &slot) in ids.iter().zip(slots) {
        block.coords(e, &mut xe)?;
        let x: &[Real] = if kind == ElementKind::Cohesive {
            let half = block.nnpe / 2;
            for n in 0..half {
                for i in 0..sdim {
                    xm[i + n * sdim] = (xe[i + n * sdim] + xe[i + (n + half) * sdim]) / 2.;
                }
            }
            &xm
        } else {
            &xe
        };
        let jv = &mut out[slot * nq..(slot + 1) * nq];
        if E::TYPE == ElementType::Cohesive1d2 {
            jv[0] = 1.0;
            continue;
        }
        for (q, jq) in jv.iter_mut().enumerate() {
            E::compute_dnds(rule.point(q, nd), &mut dnds);
            kernels::jmat(
                &dnds,
                Dims::fixed(nd, nn),
                x,
                Dims::dynamic(sdim, nn),
                &mut j,
            )?;
            *jq = kernels::jacobian(&j, nd, sdim)?;
        }
    }
    Ok(())
}

/// `J.array() *= weights.array()` for the slots `slots`.
pub(crate) fn multiply_by_weights(jac: &mut [Real], weights: &[Real], slots: &[usize]) {
    let nq = weights.len();
    for &s in slots {
        for (j, w) in jac[s * nq..(s + 1) * nq].iter_mut().zip(weights) {
            *j *= w;
        }
    }
}

/// Shapes at the rule points for the slots `slots` (upstream
/// `computeShapesOnIntegrationPoints`).
pub(crate) fn compute_shapes<E: LagrangeElement>(
    rule: &QuadratureRule,
    slots: &[usize],
    out: &mut [Real],
) {
    let nq = rule.nb_points();
    let nn = E::NB_SHAPES;
    let nd = E::NATURAL_DIM;
    let mut n = vec![0.0; nn * nq];
    for q in 0..nq {
        E::compute_shapes(rule.point(q, nd), &mut n[q * nn..(q + 1) * nn]);
    }
    for &s in slots {
        out[s * nq * nn..(s + 1) * nq * nn].copy_from_slice(&n);
    }
}

/// Natural derivatives (cohesive shape derivatives).
pub(crate) fn compute_natural_derivatives<E: LagrangeElement>(
    rule: &QuadratureRule,
    slots: &[usize],
    out: &mut [Real],
) {
    let nq = rule.nb_points();
    let sz = E::NATURAL_DIM * E::NB_SHAPES;
    let mut d = vec![0.0; sz * nq];
    for q in 0..nq {
        E::compute_dnds(rule.point(q, E::NATURAL_DIM), &mut d[q * sz..(q + 1) * sz]);
    }
    for &s in slots {
        out[s * nq * sz..(s + 1) * nq * sz].copy_from_slice(&d);
    }
}

/// Physical derivatives `J⁻¹ · dnds` (upstream
/// `computeShapeDerivativesOnCPointsByElement`: dynamic `Tensor3` slices for
/// `dnds` and `J`, dynamic node coordinates), elements `ids` into `ids`
/// slots (the precomputed array is indexed by element).
pub(crate) fn compute_physical_derivatives<E: LagrangeElement>(
    block: &Block<'_>,
    rule: &QuadratureRule,
    ids: &[usize],
    out: &mut [Real],
) -> Result<()> {
    let nq = rule.nb_points();
    let nd = E::NATURAL_DIM;
    let nn = E::NB_SHAPES;
    let sdim = block.sdim;
    let sz = nd * nn;
    let mut x = vec![0.0; sdim * nn];
    let mut dnds = vec![0.0; sz * nq];
    for q in 0..nq {
        E::compute_dnds(rule.point(q, nd), &mut dnds[q * sz..(q + 1) * sz]);
    }
    let mut j = vec![0.0; nd * sdim];
    for &e in ids {
        block.coords(e, &mut x)?;
        for q in 0..nq {
            let d = &dnds[q * sz..(q + 1) * sz];
            kernels::jmat(
                d,
                Dims::dynamic(nd, nn),
                &x,
                Dims::dynamic(sdim, nn),
                &mut j,
            )?;
            let o = (e * nq + q) * sz;
            kernels::shape_derivatives(&j, d, Dims::dynamic(nd, nn), &mut out[o..o + sz])?;
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Mesh events
// ---------------------------------------------------------------------------

impl FeEngine {
    /// New elements (upstream `FEEngineTemplate::onElementsAdded`): compute
    /// the quadrature points (if missing), `det J · w` (no sign check, as
    /// upstream), shapes and derivatives of the new elements only; the
    /// per-type arrays are resized to the mesh's element count.
    pub fn on_elements_added(
        &mut self,
        mesh: &Mesh,
        elements: &[akantu_core::Element],
    ) -> Result<()> {
        let mut groups: Vec<(ElementType, GhostType, Vec<usize>)> = Vec::new();
        for el in elements {
            let (ty, g) = (el.ty(), el.ghost_type());
            if ty.spatial_dimension() != self.element_dimension || ty.kind() != self.kind {
                continue;
            }
            let idx = el
                .try_idx()
                .ok_or_else(|| Error::invalid("null element in ElementsAdded"))?;
            match groups.iter_mut().find(|(t, gg, _)| *t == ty && *gg == g) {
                Some((_, _, v)) => v.push(idx),
                None => groups.push((ty, g, vec![idx])),
            }
        }
        for (ty, ghost, ids) in groups {
            if !self.rules.exists(ty, ghost) {
                let rule = gauss_rule(ty, self.orders.order(ty))?;
                self.rules.replace(ty, ghost, rule);
            }
            let rule = self.rule(ty, ghost)?.clone();
            let block = Block::new(mesh, ty, ghost)?;
            let ne = block.nb_element;
            for &e in &ids {
                if e >= ne {
                    return Err(Error::OutOfRange {
                        what: "added element",
                        index: e,
                        len: ne,
                    });
                }
            }
            let nq = rule.nb_points();
            let kind = self.kind;
            let mesh_dim = mesh.spatial_dimension();
            let p = &mut self.precomputed;
            let ensure = |m: &mut ElementTypeMapArray<Real>, nbc: usize| -> Result<()> {
                m.alloc(ne * nq, nbc, ty, ghost, 0.0)?;
                Ok(())
            };
            ensure(&mut p.jacobians, 1)?;
            with_element_class!(ty, E => {
                let jac = p.jacobians.get_mut(ty, ghost)?;
                compute_jacobians::<E>(&block, &rule, kind, &ids, &ids, jac.as_mut_slice())?;
                multiply_by_weights(jac.as_mut_slice(), &rule.weights, &ids);
                ensure(&mut p.shapes, E::NB_SHAPES)?;
                compute_shapes::<E>(&rule, &ids, p.shapes.get_mut(ty, ghost)?.as_mut_slice());
                let sz = E::NATURAL_DIM * E::NB_SHAPES;
                if kind == ElementKind::Cohesive {
                    ensure(&mut p.shapes_derivatives, sz)?;
                    compute_natural_derivatives::<E>(&rule, &ids, p.shapes_derivatives.get_mut(ty, ghost)?.as_mut_slice());
                } else if E::NATURAL_DIM == mesh_dim {
                    ensure(&mut p.shapes_derivatives, sz)?;
                    compute_physical_derivatives::<E>(&block, &rule, &ids, p.shapes_derivatives.get_mut(ty, ghost)?.as_mut_slice())?;
                }
                Ok::<(), Error>(())
            }, else Err(Error::UnsupportedElementType { ty, context: "on_elements_added" }))?;
        }
        self.mesh_revision = mesh.revision();
        Ok(())
    }

    /// Removed elements (upstream `onElementsRemoved`): renumber the
    /// per-element blocks of every precomputed array
    /// (`new_numbering[old] = new` or `-1`; an empty numbering leaves the
    /// type unchanged, as upstream).
    ///
    /// The numbering is validated for every cached array before anything is
    /// modified (`Error` otherwise, the engine is left unchanged):
    /// it has one component, its length is exactly the number of
    /// cached elements of the `(type, ghost)` block — every cache holds
    /// `nb_element · nq` tuples with `nq` the engine's own quadrature-point
    /// count of the block, never a count derived from the numbering — and
    /// each kept element maps to a distinct new index below the number of
    /// kept elements. The renumbered caches keep `kept · nq` tuples (UD-103:
    /// upstream truncates them to `kept` entries).
    pub fn on_elements_removed(&mut self, new_numbering: &ElementTypeMapArray<Idx>) -> Result<()> {
        let mut plan = Vec::new();
        for (ty, ghost) in renumbered_types(new_numbering) {
            let numbering = new_numbering.get(ty, ghost)?;
            let p = &self.precomputed;
            let caches = [
                &p.shapes,
                &p.shapes_derivatives,
                &p.jacobians,
                &self.normals,
            ];
            if !caches.iter().any(|m| m.exists(ty, ghost)) || numbering.is_empty() {
                continue;
            }
            if numbering.nb_component() != 1 {
                return Err(Error::DimensionMismatch {
                    context: "on_elements_removed: numbering components",
                    expected: 1,
                    got: numbering.nb_component(),
                });
            }
            let num = numbering.as_slice();
            let nq = self.nb_integration_points(ty, ghost)?;
            for m in caches {
                if let Ok(arr) = m.get(ty, ghost) {
                    // `nq ≥ 1` for every rule; a cache that is not a whole
                    // number of blocks is inconsistent engine state.
                    if !arr.len().is_multiple_of(nq) {
                        return Err(Error::DimensionMismatch {
                            context: "on_elements_removed: cached array is not a whole number \
                                      of element blocks",
                            expected: nq,
                            got: arr.len(),
                        });
                    }
                    if arr.len() / nq != num.len() {
                        return Err(Error::DimensionMismatch {
                            context: "on_elements_removed: numbering length (the cached \
                                      number of elements of the type)",
                            expected: arr.len() / nq,
                            got: num.len(),
                        });
                    }
                }
            }
            let kept = kept_elements(num)?;
            plan.push((ty, ghost, nq, kept));
        }
        let p = &mut self.precomputed;
        for map in [
            &mut p.shapes,
            &mut p.shapes_derivatives,
            &mut p.jacobians,
            &mut self.normals,
        ] {
            for &(ty, ghost, nq, kept) in &plan {
                let num = new_numbering.get(ty, ghost)?.as_slice();
                if let Ok(arr) = map.get_mut(ty, ghost) {
                    renumber_blocks(arr, num, nq, kept);
                }
            }
        }
        Ok(())
    }
}

/// `(type, ghost)` blocks of a renumbering map.
fn renumbered_types(numbering: &ElementTypeMapArray<Idx>) -> Vec<(ElementType, GhostType)> {
    let mut out = Vec::new();
    for ghost in GhostType::ALL {
        out.extend(
            numbering
                .element_types(TypeFilter {
                    spatial_dimension: None,
                    ghost_type: ghost,
                    kind: None,
                })
                .map(|ty| (ty, ghost)),
        );
    }
    out
}

/// Validate the new indices of a renumbering `num` (`num[old] = new` or
/// `-1`, upstream `new_numbering`): every kept element maps to a distinct
/// index below the number of kept elements. Returns that number.
fn kept_elements(num: &[Idx]) -> Result<usize> {
    let kept = num.iter().filter(|&&n| n >= 0).count();
    let mut seen = vec![false; kept];
    for &n in num {
        if n < 0 {
            if n != -1 {
                return Err(Error::InvalidIndex {
                    what: "new element numbering (removed elements are -1)",
                    value: i64::from(n),
                });
            }
            continue;
        }
        let u = usize::try_from(n).map_err(|_| Error::InvalidIndex {
            what: "new element numbering",
            value: i64::from(n),
        })?;
        match seen.get_mut(u) {
            Some(s) if !*s => *s = true,
            Some(_) => {
                return Err(Error::invalid(format!(
                    "on_elements_removed: new element index {u} assigned twice"
                )));
            }
            None => {
                return Err(Error::OutOfRange {
                    what: "new element numbering",
                    index: u,
                    len: kept,
                });
            }
        }
    }
    Ok(kept)
}

/// Upstream `ElementTypeMapArray::onElementsRemoved` for per-element blocks
/// of `per` tuples, numbering validated by [`kept_elements`] against
/// `arr.len() == num.len() · per` (the array keeps `kept · per` tuples,
/// UD-103).
fn renumber_blocks(arr: &mut Array<Real>, num: &[Idx], per: usize, kept: usize) {
    debug_assert_eq!(arr.len(), num.len() * per);
    let block = per * arr.nb_component();
    let src = arr.as_slice().to_vec();
    let dst = arr.as_mut_slice();
    for (e, &ne) in num.iter().enumerate() {
        if let Ok(ne) = usize::try_from(ne) {
            dst[ne * block..(ne + 1) * block].copy_from_slice(&src[e * block..(e + 1) * block]);
        }
    }
    arr.truncate(kept * per);
}

impl MeshEventHandler for FeEngine {
    fn event_priority(&self) -> EventPriority {
        EventPriority::FE_ENGINE
    }

    /// Accepts every event: the engine has no
    /// capability limit. Its dispatch errors are internal-consistency
    /// errors that a validated transaction cannot produce (null element,
    /// element index or numbering length out of range); the quadrature rule
    /// of a new type comes from orders validated when they were configured
    /// (`set_integration_orders`) or from the type's default degree, which
    /// every Gauss family of the engine's kind provides. Upstream
    /// `FEEngineTemplate::onElementsAdded/Removed` never refuse either.
    fn check_mesh_event(&self, _mesh: &Mesh, _event: &MeshEvent) -> Result<()> {
        Ok(())
    }

    /// New elements: compute their shapes/jacobians; removed: renumber the
    /// precomputed arrays; changed: nothing (upstream
    /// `FEEngineTemplate::onElementsAdded/Removed/Changed`).
    fn on_mesh_event(&mut self, mesh: &Mesh, event: &MeshEvent) -> Result<()> {
        match event {
            MeshEvent::ElementsAdded { elements, .. } => self.on_elements_added(mesh, elements)?,
            MeshEvent::ElementsRemoved { new_numbering, .. } => {
                self.on_elements_removed(new_numbering)?
            }
            _ => {}
        }
        self.mesh_revision = mesh.revision();
        Ok(())
    }
}
