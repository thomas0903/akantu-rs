// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/model/solid_mechanics/materials/
// material_embedded/material_reinforcement.hh,
// material_reinforcement_tmpl.hh; the `reinforcement` allocator of
// src/model/solid_mechanics/solid_mechanics_model_embedded_interface/
// embedded_interface_model.cc.

//! `reinforcement elastic` (EMB-003): upstream
//! `MaterialReinforcement<MaterialElastic<1>, dim>`, a 1-D constitutive law
//! on the `_segment_2` elements of the interface mesh whose kinematics and
//! forces live on the **background** element each segment lies in.
//!
//! Per interface quadrature point `q` of a segment `s` inside the background
//! element `e` (upstream formulas, Gomes & Awruch 2001):
//!
//! * directing cosines `C = (l², m², [n²,] [mn, ln,] lm)` of the unit
//!   tangent of `s` (`computeDirectingCosinesOnQuad`, Voigt order of
//!   [`akantu_core::voigt`]);
//! * `B`: shape derivatives of `e` at the real position of `q` (inverse map
//!   in `e`, `computeShapeDerivatives`), `B_v` its symmetric Voigt matrix;
//! * axial strain `ε_s = C · ε_voigt(∇u_e)` with engineering shears
//!   (`computeGradU`), stored as the law's 1×1 `grad_u`; the background
//!   gradient `∇u_e` itself goes to `gradu_embedded` (UD-593);
//! * stress `σ_s` of the 1-D elastic law (`σ = E·ε + σ_th`) plus the uniform
//!   pre-stress `pre_stress` (`addPrestress`);
//! * residual `f_int(e) −= A ∫_s B_vᵀ C σ_s ds` and stiffness
//!   `K(e) += A ∫_s B_vᵀ Cᵀ D C B_v ds` assembled on the nodes of `e`
//!   (`assembleInternalForcesInterface`, `assembleStiffnessMatrixInterface`,
//!   `_symmetric` blocks);
//! * potential energy `A ∫ ½ σ_s ε_s ds` (`getEnergy("potential")`).
//!
//! The products follow upstream's Eigen 3.4 evaluation (strict build,
//! `akantu_fe::eigen`): products left to right, a scalar factor of a
//! product applied where Eigen's evaluator applies it — in the stiffness
//! `Bvᵀ·Cᵀ·D·C·Bv·A` the tangent `D` multiplies the outer product
//! `(Bvᵀ·Cᵀ) ⊗ C`, not `Bvᵀ·Cᵀ`. The class does not
//! compile at the reference commit (UD-587); the evidence is the
//! patched oracle variant `oracle/patches/embedded` (D-060: bitwise on
//! upstream's tests and the oblique bar, `tests/oracle_fixtures.rs`) and
//! the independent checks of `tests/`.
//!
//! **Upstream defects** (D-003, `docs/UPSTREAM_DEFECTS.md`): the class does
//! not compile (UD-587) — `pre_stress` is registered both as a parsed
//! parameter and as an internal (UD-588: parsed value = default of the
//! internal, as v5.0.0), the law dimension is the model's (UD-589: here 1);
//! the background data are indexed by interface element id instead of
//! position (UD-590: here per position, any number of laws and background
//! types); `getEnergy` multiplies the running sum by the area inside the
//! type loop (UD-591, latent); finite deformation is refused (UD-592);
//! `gradu_embedded` is never written (UD-593); `eigen_grad_u` is ignored as
//! upstream (UD-594, documented).

use std::sync::{Arc, RwLock};

use akantu_core::array::ArrayViewMut;
use akantu_core::array_shared::ArrayRef;
use akantu_core::eigen_order as eo;
use akantu_core::enums::MatrixType;
use akantu_core::voigt::{voigt_size, voigt_table};
use akantu_core::{
    Array, Element, ElementKind, ElementType, ElementTypeMapArray, Error, GhostType, Idx, Real,
    Result, TypeFilter,
};
use akantu_fe::FeEngine;
use akantu_fe::eigen::{Dims, product_slices};
use akantu_mesh::Mesh;
use akantu_model::InternalField;
use akantu_model::constitutive_law::{
    ConstitutiveLaw, InternalFieldMut, InternalFieldRef, LawContext, LawSpec, ModelServices,
};
use akantu_parser::ParameterRegistry;
use akantu_parser::param::{ParamDescriptor, ParamKind, ParamValue, ParameterAccess};
use akantu_solid::material::{DerivedMaterialParams, LawOptions, MaterialFactory};
use akantu_solid::materials::elastic::MaterialElastic;
use akantu_solid::{Material, MaterialBase};
use akantu_solver::DofManager;

/// Name of the FE engine on the interface mesh (upstream
/// `"EmbeddedInterfaceFEEngine"`, element dimension 1).
pub const INTERFACE_FE_ENGINE: &str = "EmbeddedInterfaceFEEngine";

/// The interface mesh and its FE engine, shared by the embedded model and
/// its reinforcement laws once the intersections are computed (upstream:
/// `EmbeddedInterfaceIntersector::interface_mesh` and the model's
/// `"EmbeddedInterfaceFEEngine"`). Immutable after construction.
#[derive(Debug)]
pub struct InterfaceData {
    /// Interface mesh: `_segment_2` elements with the element data
    /// `associated_element` and `physical_names`.
    pub mesh: Mesh,
    /// Lagrange FE engine of element dimension 1 on [`InterfaceData::mesh`].
    pub fe: FeEngine,
}

/// Slot through which the reinforcement laws reach the interface data: set
/// by `EmbeddedInterfaceModel::init_full` before the laws are initialised
/// (the laws may be allocated earlier, e.g. by `register_new_material`).
#[derive(Clone, Debug, Default)]
pub struct InterfaceSlot(Arc<RwLock<Option<Arc<InterfaceData>>>>);

impl InterfaceSlot {
    /// The interface data, if set.
    ///
    /// # Errors
    /// A poisoned lock.
    pub fn get(&self) -> Result<Option<Arc<InterfaceData>>> {
        let g = self
            .0
            .read()
            .map_err(|_| Error::invalid("embedded interface slot lock poisoned"))?;
        Ok(g.clone())
    }

    /// Set the interface data.
    ///
    /// # Errors
    /// A poisoned lock.
    pub fn set(&self, data: Arc<InterfaceData>) -> Result<()> {
        let mut g = self
            .0
            .write()
            .map_err(|_| Error::invalid("embedded interface slot lock poisoned"))?;
        *g = Some(data);
        Ok(())
    }
}

/// Model services of the interface for the 1-D law of a reinforcement
/// (upstream: the law is constructed on the FE engine
/// `"EmbeddedInterfaceFEEngine"`, so its internals live on the quadrature
/// points of the interface mesh). Nodal fields stay those of the background
/// model; gradients of them on the interface mesh are refused.
struct InterfaceServices<'a> {
    inner: &'a dyn ModelServices,
    iface: &'a InterfaceData,
}

impl ModelServices for InterfaceServices<'_> {
    fn spatial_dimension(&self) -> usize {
        self.inner.spatial_dimension()
    }

    fn mesh(&self) -> &Mesh {
        &self.iface.mesh
    }

    fn time_step(&self) -> Real {
        self.inner.time_step()
    }

    fn nb_integration_points(&self, ty: ElementType, ghost: GhostType) -> usize {
        // The trait has no error channel. The interface engine knows only
        // `_segment_2`: another type has no integration
        // point on the interface mesh; an error for `_segment_2` would be an
        // internal inconsistency.
        let n = self.iface.fe.nb_integration_points(ty, ghost);
        debug_assert!(
            n.is_ok() || ty != ElementType::Segment2,
            "interface engine without _segment_2 integration points: {n:?}"
        );
        n.unwrap_or(0)
    }

    fn integration_point_coordinates(
        &self,
        ty: ElementType,
        ghost: GhostType,
        filter: &[Idx],
        out: &mut Array<Real>,
    ) -> Result<()> {
        let m = &self.iface.mesh;
        self.iface.fe.interpolate_on_integration_points(
            m,
            m.nodes().view(),
            out,
            m.spatial_dimension(),
            ty,
            ghost,
            Some(filter),
        )
    }

    fn nodal_field(&self, name: &str) -> Result<ArrayRef<'_, Real>> {
        self.inner.nodal_field(name)
    }

    fn fe_engine(&self, name: &str) -> Result<&FeEngine> {
        if name.is_empty() || name == INTERFACE_FE_ENGINE {
            Ok(&self.iface.fe)
        } else {
            self.inner.fe_engine(name)
        }
    }

    fn gradient_on_integration_points(
        &self,
        field: &str,
        _out: &mut Array<Real>,
        _nb_component: usize,
        _ty: ElementType,
        _ghost: GhostType,
        _filter: &[Idx],
    ) -> Result<()> {
        Err(Error::Unsupported(format!(
            "gradient of the nodal field '{field}' on the interface mesh: reinforcement \
             kinematics come from the background elements"
        )))
    }
}

/// The interface segments of one law lying in background elements of one
/// type (upstream `foreground_filter` / `background_filter` /
/// `shape_derivatives` of one `(interface type, background type)` pair).
#[derive(Clone, Debug)]
struct BackgroundGroup {
    /// Background element type.
    bg_type: ElementType,
    /// Nodes per background element.
    nn: usize,
    /// Positions of the segments in the law's filter (law-local indices).
    positions: Vec<usize>,
    /// Interface element ids of those segments.
    fg_elements: Vec<Idx>,
    /// Background element of each segment.
    bg_elements: Vec<Idx>,
    /// Background shape derivatives at the segments' quadrature points:
    /// `positions.len() · nq` blocks of `dim × nn` (column-major).
    shapesd: Vec<Real>,
}

/// Everything precomputed for one interface element type (not ghost).
#[derive(Clone, Debug)]
struct InterfaceType {
    ty: ElementType,
    /// Quadrature points per interface element.
    nq: usize,
    /// One group per background type of the model dimension (enum order;
    /// possibly empty, as upstream's per-type filters).
    groups: Vec<BackgroundGroup>,
}

/// Upstream `MaterialReinforcement<MaterialElastic<1>, DIM>` (module
/// documentation). `DIM` is the problem dimension (2 or 3); the law itself
/// is one-dimensional (1×1 `stress`, `grad_u`).
#[derive(Debug)]
pub struct MaterialReinforcement<const DIM: usize> {
    inner: MaterialElastic<1>,
    /// Cross-sectional area `A` (parameter `area`, default 1).
    pub area: Real,
    /// Background displacement gradient at the interface quadrature points
    /// (`dim × dim`; UD-593).
    pub gradu_embedded: InternalField<Real>,
    /// Pre-stress per quadrature point (internal `pre_stress`, default
    /// value = the parameter `pre_stress`, UD-588).
    pub pre_stress: InternalField<Real>,
    /// Directing cosines per quadrature point (Voigt size of `DIM`).
    pub directing_cosines: InternalField<Real>,
    slot: InterfaceSlot,
    interface: Option<Arc<InterfaceData>>,
    types: Vec<InterfaceType>,
    /// Revision of the background mesh the interface data were built on
    /// (checked by every evaluation of the law, so that
    /// calls through `EmbeddedInterfaceModel::smm` are guarded too).
    background_revision: Option<u64>,
}

impl<const DIM: usize> MaterialReinforcement<DIM> {
    const VOIGT: usize = voigt_size(DIM);

    /// New law (upstream constructor). `spec.spatial_dimension` is the
    /// problem dimension (`DIM`); the wrapped elastic law is 1-D with the
    /// same id.
    ///
    /// # Errors
    /// A dimension other than 2 or 3, or `spec.spatial_dimension != DIM`.
    pub fn new(spec: &LawSpec, slot: InterfaceSlot) -> Result<Self> {
        if !(DIM == 2 || DIM == 3) {
            return Err(Error::invalid("Dimension 1 is invalid for reinforcements"));
        }
        if spec.spatial_dimension != DIM {
            return Err(Error::DimensionMismatch {
                context: "MaterialReinforcement",
                expected: DIM,
                got: spec.spatial_dimension,
            });
        }
        let spec1 = LawSpec {
            spatial_dimension: 1,
            option: spec.option.clone(),
            id: spec.id.clone(),
        };
        let mut inner = MaterialElastic::<1>::new(&spec1, None)?;
        inner.base_mut().law_type = "reinforcement".into();
        Ok(Self {
            inner,
            area: 1.0,
            gradu_embedded: InternalField::new("gradu_embedded", DIM * DIM, 0.0)?,
            pre_stress: InternalField::new("pre_stress", 1, 0.0)?,
            directing_cosines: InternalField::new("directing_cosines", Self::VOIGT, 0.0)?,
            slot,
            interface: None,
            types: Vec::new(),
            background_revision: None,
        })
    }

    /// Refuse a background mesh that changed since the law was initialised
    /// (upstream rebuilds neither the interface mesh nor the background
    /// filters and would read stale data).
    fn check_background(&self, mesh: &Mesh) -> Result<()> {
        match self.background_revision {
            Some(rev) if rev != mesh.revision() => Err(Error::Unsupported(
                "the background mesh changed after the reinforcement law was initialised (the \
                 interface mesh and the background filters are not rebuilt, as upstream)"
                    .into(),
            )),
            _ => Ok(()),
        }
    }

    /// The wrapped 1-D elastic law (upstream base class `Mat`).
    #[must_use]
    pub fn elastic(&self) -> &MaterialElastic<1> {
        &self.inner
    }

    /// The interface data, once the law is initialised.
    ///
    /// # Errors
    /// The law was not initialised.
    pub fn interface(&self) -> Result<&Arc<InterfaceData>> {
        self.interface.as_ref().ok_or_else(|| {
            Error::invalid("reinforcement law not initialised (EmbeddedInterfaceModel::init_full)")
        })
    }

    /// Background elements `(type, element)` of the law's segments of
    /// `ty`, in law-local order (upstream `background_filter` merged).
    ///
    /// # Errors
    /// The law was not initialised.
    pub fn background_elements(&self, ty: ElementType) -> Result<Vec<Element>> {
        let n = self
            .inner
            .base()
            .element_filter
            .get(ty, GhostType::NotGhost)
            .map_or(0, Array::len);
        let mut out = vec![Element::NULL; n];
        let it = self
            .types
            .iter()
            .find(|t| t.ty == ty)
            .ok_or_else(|| Error::not_found("reinforcement interface type", ty.to_string()))?;
        for g in &it.groups {
            for (&p, &bg) in g.positions.iter().zip(&g.bg_elements) {
                out[p] = Element::new(g.bg_type, bg, GhostType::NotGhost);
            }
        }
        Ok(out)
    }

    fn interface_ctx<'a>(
        services: &'a InterfaceServices<'a>,
        phase: akantu_model::constitutive_law::LawPhase,
    ) -> LawContext<'a> {
        LawContext { phase, services }
    }

    /// The not-ghost filter of `ty` as element ids.
    fn filter_of(&self, ty: ElementType) -> Vec<Idx> {
        self.inner
            .base()
            .element_filter
            .get(ty, GhostType::NotGhost)
            .map(|a| a.as_slice().to_vec())
            .unwrap_or_default()
    }

    /// Checks of the filter before initialisation: only interface segments
    /// (a bulk element in a reinforcement law means a selector picked it —
    /// upstream would add it silently and fail later, UD-596).
    fn check_filter(&self, iface: &InterfaceData) -> Result<()> {
        for (ty, ghost, a) in self.inner.base().element_filter.as_map().iter_all() {
            if a.is_empty() {
                continue;
            }
            if ty.spatial_dimension() != 1 || ty.kind() != ElementKind::Regular {
                return Err(Error::invalid(format!(
                    "reinforcement law '{}' was given {} {ty} element(s) of the background mesh: \
                     reinforcement laws only take interface segments (check the physical names \
                     and the material selector)",
                    self.inner.base().name,
                    a.len()
                )));
            }
            if ghost == GhostType::Ghost {
                return Err(Error::Unsupported(
                    "ghost interface elements (the embedded model is serial)".into(),
                ));
            }
            let ne = iface.mesh.nb_element(ty, ghost);
            for &e in a.as_slice() {
                let u = akantu_core::try_to_usize(e)?;
                if u >= ne {
                    return Err(Error::OutOfRange {
                        what: "interface element of a reinforcement law",
                        index: u,
                        len: ne,
                    });
                }
            }
        }
        Ok(())
    }

    /// Upstream `initFilters` + `allocBackgroundShapeDerivatives` +
    /// `initBackgroundShapeDerivatives` (per position, UD-590).
    fn init_background(&mut self, ctx: &LawContext<'_>, iface: &InterfaceData) -> Result<()> {
        let mesh = ctx.services.mesh();
        self.background_revision = Some(mesh.revision());
        let main_fe = ctx.services.fe_engine("")?;
        let bg_types: Vec<ElementType> = mesh
            .element_types(TypeFilter {
                spatial_dimension: Some(DIM),
                ghost_type: GhostType::NotGhost,
                kind: Some(ElementKind::Regular),
            })
            .collect();
        let assoc = iface
            .mesh
            .data()
            .element_map(crate::intersector::ASSOCIATED_ELEMENT)?;
        let mut types = Vec::new();
        for ty in self.inner.base().filter_types(GhostType::NotGhost) {
            let filter = self.filter_of(ty);
            let nq = iface.fe.nb_integration_points(ty, GhostType::NotGhost)?;
            let elements = assoc.get(ty, GhostType::NotGhost)?;
            // quadrature-point positions of the whole filter (upstream
            // interpolates with the law's filter)
            let mut quad_pos = Array::new(DIM);
            iface.fe.interpolate_on_integration_points(
                &iface.mesh,
                iface.mesh.nodes().view(),
                &mut quad_pos,
                DIM,
                ty,
                GhostType::NotGhost,
                Some(&filter),
            )?;
            let mut groups = Vec::with_capacity(bg_types.len());
            for &bg_type in &bg_types {
                let nn = bg_type.nb_nodes_per_interpolation_element();
                let mut g = BackgroundGroup {
                    bg_type,
                    nn,
                    positions: Vec::new(),
                    fg_elements: Vec::new(),
                    bg_elements: Vec::new(),
                    shapesd: Vec::new(),
                };
                // upstream filterInterfaceBackgroundElements
                for (p, &e) in filter.iter().enumerate() {
                    let el = elements.get_tuple(akantu_core::try_to_usize(e)?)?[0];
                    if el.ty() == bg_type {
                        if el.ghost_type() != GhostType::NotGhost {
                            return Err(Error::Unsupported(
                                "interface segment associated with a ghost element".into(),
                            ));
                        }
                        g.positions.push(p);
                        g.fg_elements.push(e);
                        g.bg_elements.push(el.index());
                    }
                }
                // upstream computeBackgroundShapeDerivatives
                let block = DIM * nn;
                g.shapesd = vec![0.0; g.positions.len() * nq * block];
                for (k, (&p, &bg)) in g.positions.iter().zip(&g.bg_elements).enumerate() {
                    let element = Element::new(bg_type, bg, GhostType::NotGhost);
                    for q in 0..nq {
                        let x = quad_pos.get_tuple(p * nq + q)?;
                        let out = &mut g.shapesd[(k * nq + q) * block..(k * nq + q + 1) * block];
                        main_fe.compute_shape_derivatives_at(mesh, x, element, out)?;
                    }
                }
                groups.push(g);
            }
            types.push(InterfaceType { ty, nq, groups });
        }
        self.types = types;
        Ok(())
    }

    /// Upstream `initDirectingCosines` / `computeDirectingCosines` /
    /// `computeDirectingCosinesOnQuad`.
    fn init_directing_cosines(&mut self, iface: &InterfaceData) -> Result<()> {
        let nodes = iface.mesh.nodes();
        for it in &self.types {
            let ty = it.ty;
            let nnpe = ty.nb_nodes_per_element();
            if nnpe != 2 {
                return Err(Error::UnsupportedElementType {
                    ty,
                    context: "reinforcement directing cosines (higher order reinforcement \
                              elements not implemented)",
                });
            }
            let conn = iface.mesh.connectivity(ty, GhostType::NotGhost)?;
            let filter = self.filter_of(ty);
            let mut dc = self.directing_cosines.get_mut(ty, GhostType::NotGhost)?;
            let v = Self::VOIGT;
            for (p, &e) in filter.iter().enumerate() {
                let c = conn.get_tuple(akantu_core::try_to_usize(e)?)?;
                let x0 = nodes.get_tuple(akantu_core::try_to_usize(c[0])?)?;
                let x1 = nodes.get_tuple(akantu_core::try_to_usize(c[1])?)?;
                let mut delta = [0.0; 3];
                for i in 0..DIM {
                    delta[i] = x1[i] - x0[i];
                }
                // `delta.dot(delta)` of a dynamic vector: sequential
                let sq_length =
                    eo::sum(DIM, eo::PRODUCT_TERM_COST, false, &|i| delta[i] * delta[i]);
                let mut cosines = [0.0; 6];
                if DIM == 2 {
                    cosines[0] = delta[0] * delta[0]; // l^2
                    cosines[1] = delta[1] * delta[1]; // m^2
                    cosines[2] = delta[0] * delta[1]; // lm
                } else {
                    cosines[0] = delta[0] * delta[0]; // l^2
                    cosines[1] = delta[1] * delta[1]; // m^2
                    cosines[2] = delta[2] * delta[2]; // n^2
                    cosines[3] = delta[1] * delta[2]; // mn
                    cosines[4] = delta[0] * delta[2]; // ln
                    cosines[5] = delta[0] * delta[1]; // lm
                }
                for x in &mut cosines[..v] {
                    *x /= sq_length;
                }
                // every quadrature point of the element gets the same row
                for q in 0..it.nq {
                    dc.get_tuple_mut(p * it.nq + q)?
                        .copy_from_slice(&cosines[..v]);
                }
            }
        }
        Ok(())
    }

    /// Upstream `computeGradU(interface_type)`: axial strain into the law's
    /// `grad_u` (per position, UD-590) and the background gradient into
    /// `gradu_embedded` (UD-593).
    fn compute_grad_u(&mut self, ctx: &LawContext<'_>, it_index: usize) -> Result<()> {
        let mesh = ctx.services.mesh();
        let u = ctx.services.nodal_field("displacement")?;
        let it = &self.types[it_index];
        let ty = it.ty;
        let nq = it.nq;
        let table = voigt_table(DIM)?;
        let v = Self::VOIGT;
        let dc = self.directing_cosines.get(ty, GhostType::NotGhost)?;
        let mut ge = self.gradu_embedded.get_mut(ty, GhostType::NotGhost)?;
        let mut gu = self
            .inner
            .base_mut()
            .grad_u
            .get_mut(ty, GhostType::NotGhost)?;
        let mut u_el = Vec::new();
        let mut du = [0.0; 9];
        for g in &it.groups {
            if g.positions.is_empty() {
                continue;
            }
            let nn = g.nn;
            let conn = mesh.connectivity(g.bg_type, GhostType::NotGhost)?;
            u_el.resize(DIM * nn, 0.0);
            for (k, (&p, &bg)) in g.positions.iter().zip(&g.bg_elements).enumerate() {
                // FEEngine::extractNodalToElementField: node-major
                let c = conn.get_tuple(akantu_core::try_to_usize(bg)?)?;
                for (a, &node) in c.iter().take(nn).enumerate() {
                    let un = u.get_tuple(akantu_core::try_to_usize(node)?)?;
                    u_el[a * DIM..(a + 1) * DIM].copy_from_slice(&un[..DIM]);
                }
                for q in 0..nq {
                    let block = DIM * nn;
                    let b = &g.shapesd[(k * nq + q) * block..(k * nq + q + 1) * block];
                    // concrete_du = u * B.transpose()
                    product_slices(
                        Dims::dynamic(DIM, nn),
                        Dims::dynamic(DIM, nn).t(),
                        &u_el,
                        b,
                        &mut du[..DIM * DIM],
                    )?;
                    // epsilon = 0.5 (du + duᵀ); evoigt with factors
                    let mut evoigt = [0.0; 6];
                    for (big_i, e) in evoigt.iter_mut().enumerate().take(v) {
                        let (i, j) = table.vec(big_i);
                        *e = table.factor(big_i) * (0.5 * (du[i + j * DIM] + du[j + i * DIM]));
                    }
                    let c = dc.get_tuple(p * nq + q)?;
                    // C.dot(evoigt): dynamic lhs, sequential
                    let strain = eo::sum(v, eo::PRODUCT_TERM_COST, false, &|i| c[i] * evoigt[i]);
                    gu.get_tuple_mut(p * nq + q)?[0] = strain;
                    ge.get_tuple_mut(p * nq + q)?
                        .copy_from_slice(&du[..DIM * DIM]);
                }
            }
        }
        Ok(())
    }

    /// Upstream `addPrestress(type)`: `σ += σ_pre` per quadrature point.
    fn add_prestress(&mut self, ty: ElementType) -> Result<()> {
        let sp = self.pre_stress.get(ty, GhostType::NotGhost)?;
        let mut stress = self
            .inner
            .base_mut()
            .stress
            .get_mut(ty, GhostType::NotGhost)?;
        if sp.len() != stress.len() {
            return Err(Error::DimensionMismatch {
                context: "MaterialReinforcement::addPrestress",
                expected: stress.len(),
                got: sp.len(),
            });
        }
        for (s, p) in stress.as_mut_slice().iter_mut().zip(sp.as_slice()) {
            *s += *p;
        }
        Ok(())
    }

    /// Upstream `transferBMatrixToSymVoigtBMatrix(B, Bvoigt, nn)`:
    /// column-major `voigt × dim·nn`.
    fn b_voigt(b: &[Real], nn: usize, bv: &mut Vec<Real>) {
        let v = Self::VOIGT;
        bv.clear();
        bv.resize(v * DIM * nn, 0.0);
        let at = |i: usize, j: usize| i + j * v;
        for i in 0..DIM {
            for n in 0..nn {
                bv[at(i, i + n * DIM)] = b[i + n * DIM];
            }
        }
        if DIM == 2 {
            for n in 0..nn {
                bv[at(2, 1 + n * 2)] = b[n * 2];
                bv[at(2, n * 2)] = b[1 + n * 2];
            }
        } else {
            for n in 0..nn {
                let dndx = b[n * 3];
                let dndy = b[1 + n * 3];
                let dndz = b[2 + n * 3];
                bv[at(3, 1 + n * 3)] = dndz;
                bv[at(3, 2 + n * 3)] = dndy;
                bv[at(4, n * 3)] = dndz;
                bv[at(4, 2 + n * 3)] = dndx;
                bv[at(5, n * 3)] = dndy;
                bv[at(5, 1 + n * 3)] = dndx;
            }
        }
    }

    /// Residual of the law (upstream `assembleInternalForcesInterface` for
    /// every background type), into `f_int`.
    fn assemble_internal_forces_impl(
        &self,
        mesh: &Mesh,
        mut f_int: ArrayViewMut<'_, Real>,
    ) -> Result<()> {
        let iface = self.interface()?;
        let v = Self::VOIGT;
        let mut bv = Vec::new();
        let mut t = Vec::new();
        for it in &self.types {
            let ty = it.ty;
            let nq = it.nq;
            let stress = self.inner.base().stress.get(ty, GhostType::NotGhost)?;
            let dc = self.directing_cosines.get(ty, GhostType::NotGhost)?;
            for g in &it.groups {
                if g.positions.is_empty() {
                    continue;
                }
                let back_dof = DIM * g.nn;
                let block = DIM * g.nn;
                let mut integrant = Array::zeros(g.positions.len() * nq, back_dof);
                t.resize(back_dof, 0.0);
                for (k, &p) in g.positions.iter().enumerate() {
                    for q in 0..nq {
                        let b = &g.shapesd[(k * nq + q) * block..(k * nq + q + 1) * block];
                        Self::b_voigt(b, g.nn, &mut bv);
                        let c = dc.get_tuple(p * nq + q)?;
                        let sigma = stress.get_tuple(p * nq + q)?[0];
                        // BtCt_sigma = Bvoigt.transpose() * C * sigma * area
                        product_slices(
                            Dims::dynamic(v, back_dof).t(),
                            Dims::new(v, 1, false, true),
                            &bv,
                            c,
                            &mut t,
                        )?;
                        let row = integrant.get_tuple_mut(k * nq + q)?;
                        for (r, x) in row.iter_mut().zip(&t) {
                            *r = *x * sigma * self.area;
                        }
                    }
                }
                let mut residual = Array::new(back_dof);
                iface.fe.integrate(
                    integrant.view(),
                    &mut residual,
                    back_dof,
                    ty,
                    GhostType::NotGhost,
                    Some(&g.fg_elements),
                )?;
                DofManager::assemble_elemental_array_local_view(
                    mesh,
                    residual.view(),
                    f_int.reborrow(),
                    g.bg_type,
                    GhostType::NotGhost,
                    -1.,
                    Some(&g.bg_elements),
                )?;
            }
        }
        Ok(())
    }

    /// Elemental stiffness blocks of the law (upstream
    /// `assembleStiffnessMatrixInterface` for every background type).
    #[allow(clippy::type_complexity)] // mirrors Material::elemental_stiffness_custom
    fn elemental_stiffness_impl(
        &mut self,
        ctx: &LawContext<'_>,
    ) -> Result<Vec<(ElementType, Vec<Idx>, Array<Real>, MatrixType)>> {
        let iface = self.interface()?.clone();
        let services = InterfaceServices {
            inner: ctx.services,
            iface: &iface,
        };
        let ictx = Self::interface_ctx(&services, ctx.phase);
        let v = Self::VOIGT;
        let mut blocks = Vec::new();
        let mut bv = Vec::new();
        let (mut t1, mut t3, mut t4) = (Vec::new(), Vec::new(), Vec::new());
        for it_index in 0..self.types.len() {
            let (ty, nq) = (self.types[it_index].ty, self.types[it_index].nq);
            let nb_quad = self.inner.base().grad_u.get(ty, GhostType::NotGhost)?.len();
            let mut tangent = Array::zeros(nb_quad, 1);
            self.inner
                .compute_tangent_moduli(&ictx, ty, GhostType::NotGhost, &mut tangent)?;
            let it = &self.types[it_index];
            let dc = self.directing_cosines.get(ty, GhostType::NotGhost)?;
            for g in &it.groups {
                if g.positions.is_empty() {
                    continue;
                }
                let back_dof = DIM * g.nn;
                let block = DIM * g.nn;
                let n2 = back_dof * back_dof;
                let mut integrant = Array::zeros(g.positions.len() * nq, n2);
                t1.resize(back_dof, 0.0);
                t3.resize(back_dof * v, 0.0);
                t4.resize(n2, 0.0);
                for (k, &p) in g.positions.iter().enumerate() {
                    for q in 0..nq {
                        let b = &g.shapesd[(k * nq + q) * block..(k * nq + q + 1) * block];
                        Self::b_voigt(b, g.nn, &mut bv);
                        let c = dc.get_tuple(p * nq + q)?;
                        let d = tangent.get_tuple(p * nq + q)?[0];
                        // Bvoigt.transpose() * C.transpose() * D * C * Bvoigt * area
                        // (C is a 1 × voigt view) as Eigen 3.4 evaluates it: the
                        // scalar D of `(Bvᵀ·Cᵀ)·D` is extracted and applied after
                        // the outer product with C (bitwise
                        // with the patched oracle on an oblique bar), then
                        // `·Bvoigt`, then `·area`.
                        product_slices(
                            Dims::dynamic(v, back_dof).t(),
                            Dims::dynamic(1, v).t(),
                            &bv,
                            c,
                            &mut t1,
                        )?;
                        product_slices(
                            Dims::dynamic(back_dof, 1),
                            Dims::dynamic(1, v),
                            &t1,
                            c,
                            &mut t3,
                        )?;
                        for x in &mut t3 {
                            *x *= d;
                        }
                        product_slices(
                            Dims::dynamic(back_dof, v),
                            Dims::dynamic(v, back_dof),
                            &t3,
                            &bv,
                            &mut t4,
                        )?;
                        let row = integrant.get_tuple_mut(k * nq + q)?;
                        for (r, x) in row.iter_mut().zip(&t4) {
                            *r = *x * self.area;
                        }
                    }
                }
                let mut k_interface = Array::new(n2);
                iface.fe.integrate(
                    integrant.view(),
                    &mut k_interface,
                    n2,
                    ty,
                    GhostType::NotGhost,
                    Some(&g.fg_elements),
                )?;
                blocks.push((
                    g.bg_type,
                    g.bg_elements.clone(),
                    k_interface,
                    MatrixType::Symmetric,
                ));
            }
        }
        Ok(blocks)
    }
}

impl<const DIM: usize> MaterialReinforcement<DIM> {
    /// Parameters registered by the reinforcement after those of the 1-D
    /// elastic law (upstream constructor: `area`, `pre_stress`).
    const PARAMS: &'static [ParamDescriptor<Self>] = &[
        ParamDescriptor::new(
            "area",
            ParamKind::Real,
            ParameterAccess::PARSMOD,
            "Reinforcement cross-sectional area",
            |s| ParamValue::Real(s.area),
            Some(|s, v| {
                s.area = v.as_real()?;
                Ok(())
            }),
        ),
        ParamDescriptor::new(
            "pre_stress",
            ParamKind::Real,
            ParameterAccess::PARSMOD,
            "Uniform pre-stress",
            |s| ParamValue::Real(s.pre_stress.parameter_value()),
            Some(|s, v| {
                s.pre_stress.set_default_value(v.as_real()?);
                Ok(())
            }),
        ),
    ];
}

impl<const DIM: usize> DerivedMaterialParams for MaterialReinforcement<DIM> {
    type Parent = MaterialElastic<1>;
    fn parent(&self) -> &Self::Parent {
        &self.inner
    }
    fn parent_mut(&mut self) -> &mut Self::Parent {
        &mut self.inner
    }
    fn derived_params(&self) -> &'static [ParamDescriptor<Self>] {
        Self::PARAMS
    }
    fn update_derived(&mut self) -> Result<()> {
        self.inner.update_internal_parameters()
    }
    fn derived_id(&self) -> &str {
        &self.inner.base().id
    }
}

akantu_solid::impl_derived_material_registry!(MaterialReinforcement<DIM>, const DIM);

impl<const DIM: usize> ConstitutiveLaw for MaterialReinforcement<DIM> {
    fn name(&self) -> &str {
        &self.inner.base().name
    }
    fn law_id(&self) -> &str {
        &self.inner.base().id
    }
    fn law_type(&self) -> &str {
        &self.inner.base().law_type
    }
    fn element_filter(&self) -> &ElementTypeMapArray<Idx> {
        &self.inner.base().element_filter
    }
    fn element_filter_mut(&mut self) -> &mut ElementTypeMapArray<Idx> {
        &mut self.inner.base_mut().element_filter
    }
    fn for_each_internal(&self, f: &mut dyn FnMut(&str, InternalFieldRef<'_>)) {
        self.inner.for_each_internal(f);
        f(
            "gradu_embedded",
            InternalFieldRef::Real(&self.gradu_embedded),
        );
        f("pre_stress", InternalFieldRef::Real(&self.pre_stress));
        f(
            "directing_cosines",
            InternalFieldRef::Real(&self.directing_cosines),
        );
    }
    fn for_each_internal_mut(
        &mut self,
        f: &mut dyn FnMut(&str, InternalFieldMut<'_>) -> Result<()>,
    ) -> Result<()> {
        self.inner.for_each_internal_mut(f)?;
        f(
            "gradu_embedded",
            InternalFieldMut::Real(&mut self.gradu_embedded),
        )?;
        f("pre_stress", InternalFieldMut::Real(&mut self.pre_stress))?;
        f(
            "directing_cosines",
            InternalFieldMut::Real(&mut self.directing_cosines),
        )
    }
}

impl<const DIM: usize> Material for MaterialReinforcement<DIM> {
    fn base(&self) -> &MaterialBase {
        self.inner.base()
    }
    fn base_mut(&mut self) -> &mut MaterialBase {
        self.inner.base_mut()
    }

    /// Upstream `initMaterial`: the 1-D law's `initMaterial` on the
    /// interface quadrature points, then `gradu_embedded` / `pre_stress`
    /// (default = the parameter), the background filters and shape
    /// derivatives, and the directing cosines.
    fn init_material(&mut self, ctx: &LawContext<'_>) -> Result<()> {
        let iface = self.slot.get()?.ok_or_else(|| {
            Error::invalid(
                "reinforcement law initialised without an interface mesh: use \
                 EmbeddedInterfaceModel (init_full builds the interface first)",
            )
        })?;
        if self.inner.base().finite_deformation {
            // UD-592
            return Err(Error::Unsupported(
                "finite_deformation in a reinforcement law (the axial strain is a small-strain \
                 projection of the background gradient)"
                    .into(),
            ));
        }
        if ctx.services.mesh().communicator().size() > 1 {
            return Err(Error::Unsupported(
                "reinforcement laws on a distributed mesh (the embedded model is serial)".into(),
            ));
        }
        self.check_filter(&iface)?;
        // the upstream filter is initialised on the interface mesh (both
        // ghost types), so every block exists even without elements
        self.inner
            .base_mut()
            .initialize_element_filter(&iface.mesh, ElementKind::Regular)?;
        let blocks = {
            let services = InterfaceServices {
                inner: ctx.services,
                iface: &iface,
            };
            let ictx = Self::interface_ctx(&services, ctx.phase);
            self.inner.init_material(&ictx)?;
            self.inner.base().blocks(&ictx)
        };
        self.gradu_embedded.resize_blocks(&blocks)?;
        self.pre_stress.resize_blocks(&blocks)?;
        self.directing_cosines.resize_blocks(&blocks)?;
        self.init_background(ctx, &iface)?;
        self.init_directing_cosines(&iface)?;
        self.interface = Some(iface);
        Ok(())
    }

    fn compute_stress(
        &mut self,
        ctx: &LawContext<'_>,
        ty: ElementType,
        ghost: GhostType,
    ) -> Result<()> {
        let iface = self.interface()?.clone();
        let services = InterfaceServices {
            inner: ctx.services,
            iface: &iface,
        };
        let ictx = Self::interface_ctx(&services, ctx.phase);
        self.inner.compute_stress(&ictx, ty, ghost)
    }

    fn compute_tangent_moduli(
        &mut self,
        ctx: &LawContext<'_>,
        ty: ElementType,
        ghost: GhostType,
        tangent: &mut Array<Real>,
    ) -> Result<()> {
        let iface = self.interface()?.clone();
        let services = InterfaceServices {
            inner: ctx.services,
            iface: &iface,
        };
        let ictx = Self::interface_ctx(&services, ctx.phase);
        self.inner.compute_tangent_moduli(&ictx, ty, ghost, tangent)
    }

    fn compute_potential_energy(&mut self, ctx: &LawContext<'_>, ty: ElementType) -> Result<()> {
        let iface = self.interface()?.clone();
        let services = InterfaceServices {
            inner: ctx.services,
            iface: &iface,
        };
        let ictx = Self::interface_ctx(&services, ctx.phase);
        self.inner.compute_potential_energy(&ictx, ty)
    }

    fn push_wave_speed(&self, element: Element) -> Result<Real> {
        self.inner.push_wave_speed(element)
    }

    fn shear_wave_speed(&self, element: Element) -> Result<Real> {
        self.inner.shear_wave_speed(element)
    }

    fn tangent_type(&self) -> MatrixType {
        MatrixType::Symmetric
    }

    fn has_stiffness_matrix_changed(&self) -> bool {
        self.inner.has_stiffness_matrix_changed()
    }

    fn mark_stiffness_changed(&mut self) {
        self.inner.mark_stiffness_changed();
    }

    /// Upstream `computeAllStresses(ghost)`: for every interface type,
    /// `computeGradU`, the 1-D law's `computeStress`, `addPrestress`. The
    /// law's `eigen_grad_u` is not subtracted (UD-594, as upstream). There
    /// are no ghost interface elements.
    fn compute_all_stresses(&mut self, ctx: &LawContext<'_>, ghost: GhostType) -> Result<()> {
        if ghost == GhostType::Ghost {
            return Ok(());
        }
        self.check_background(ctx.services.mesh())?;
        let iface = self.interface()?.clone();
        for it_index in 0..self.types.len() {
            let ty = self.types[it_index].ty;
            self.compute_grad_u(ctx, it_index)?;
            {
                let services = InterfaceServices {
                    inner: ctx.services,
                    iface: &iface,
                };
                let ictx = Self::interface_ctx(&services, ctx.phase);
                self.inner.compute_stress(&ictx, ty, GhostType::NotGhost)?;
            }
            self.add_prestress(ty)?;
        }
        Ok(())
    }

    /// Upstream `getEnergy("potential")`: `A ∫ w ds` per interface type
    /// (UD-591: upstream multiplies the running sum by `A` inside the type
    /// loop; identical with the single interface type `_segment_2`).
    fn potential_energy(&mut self, ctx: &LawContext<'_>) -> Result<Real> {
        let iface = self.interface()?.clone();
        let services = InterfaceServices {
            inner: ctx.services,
            iface: &iface,
        };
        let ictx = Self::interface_ctx(&services, ctx.phase);
        let mut epot = 0.;
        for ty in self.inner.base().filter_types(GhostType::NotGhost) {
            self.inner.compute_potential_energy(&ictx, ty)?;
            let filter = self.filter_of(ty);
            let w = self
                .inner
                .base()
                .potential_energy
                .get(ty, GhostType::NotGhost)?;
            let integral =
                iface
                    .fe
                    .integrate_scalar(w.view(), ty, GhostType::NotGhost, Some(&filter))?;
            epot += integral * self.area;
        }
        Ok(epot)
    }

    fn options(&self) -> Option<&LawOptions> {
        self.inner.options()
    }

    fn options_mut(&mut self) -> Option<&mut LawOptions> {
        self.inner.options_mut()
    }

    fn as_any(&self) -> Option<&dyn std::any::Any> {
        Some(self)
    }

    fn as_any_mut(&mut self) -> Option<&mut dyn std::any::Any> {
        Some(self)
    }

    /// Upstream `assembleInternalForces(ghost)`: the residual on the
    /// background nodes (nothing for ghost types: no ghost segments).
    fn assemble_internal_forces_custom(
        &self,
        _fems: &akantu_model::model::FeEngineRegistry,
        mesh: &Mesh,
        ghost: GhostType,
        f_int: ArrayViewMut<'_, Real>,
    ) -> Option<Result<()>> {
        if ghost == GhostType::Ghost {
            return Some(Ok(()));
        }
        Some(
            self.check_background(mesh)
                .and_then(|()| self.assemble_internal_forces_impl(mesh, f_int)),
        )
    }

    /// Upstream `assembleStiffnessMatrix(ghost)`: `_symmetric` blocks on
    /// the background elements.
    fn elemental_stiffness_custom(
        &mut self,
        ctx: &LawContext<'_>,
        ghost: GhostType,
    ) -> Option<Result<Vec<(ElementType, Vec<Idx>, Array<Real>, MatrixType)>>> {
        if ghost == GhostType::Ghost {
            return Some(Ok(Vec::new()));
        }
        Some(
            self.check_background(ctx.services.mesh())
                .and_then(|()| self.elemental_stiffness_impl(ctx)),
        )
    }
}

/// `true` if `law` is a reinforcement law.
#[must_use]
pub fn is_reinforcement(law: &dyn Material) -> bool {
    law.as_any()
        .is_some_and(|a| a.is::<MaterialReinforcement<2>>() || a.is::<MaterialReinforcement<3>>())
}

/// Register the `reinforcement` allocator (upstream: registered by the
/// `EmbeddedInterfaceModel` constructor in the global material factory;
/// option `elastic` → `MaterialReinforcement<MaterialElastic<1>, dim>`).
/// The laws reach the interface through `slot`.
///
/// # Errors
/// `reinforcement` already registered in `factory`.
pub fn register(factory: &mut MaterialFactory, slot: InterfaceSlot) -> Result<()> {
    factory.register(
        "reinforcement",
        Arc::new(move |spec: &LawSpec, _services: &dyn ModelServices| {
            if spec.option != "elastic" {
                // upstream message, verbatim (no space after "type")
                return Err(Error::invalid(format!(
                    "Reinforcement type{} is not recognized",
                    spec.option
                )));
            }
            match spec.spatial_dimension {
                2 => Ok(
                    Box::new(MaterialReinforcement::<2>::new(spec, slot.clone())?)
                        as Box<dyn Material>,
                ),
                3 => Ok(
                    Box::new(MaterialReinforcement::<3>::new(spec, slot.clone())?)
                        as Box<dyn Material>,
                ),
                _ => Err(Error::invalid("Dimension 1 is invalid for reinforcements")),
            }
        }),
    )
}
