// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/model/diffusion_model/diffusion_law.hh,
// diffusion_law.cc; src/model/common/constitutive_laws/{constitutive_law.hh,
// constitutive_law_tmpl.hh, internal_field.hh, internal_field_tmpl.hh}
// (element filter initialisation, internal-field release counters).

//! Diffusion laws (DIFF-008): the [`DiffusionLaw`] trait, the state every
//! law shares ([`DiffusionLawBase`]: element filter, the internals `∇u`,
//! `D∇u`, `diffusivity` and their release counters) and the law factory.
//!
//! # Internals and release counters
//!
//! Upstream caches the gradient and the diffusivity per `(type, ghost)` with
//! the release counter of the internal field: a block is recomputed unless
//! its release is not `-1` **and** equals the model's `diffusion_release`
//! (bumped by every predictor / corrector, `-1` before the first one). The
//! counters follow upstream's `InternalField` life cycle: `0` after the
//! registration (`reset`), `+1` per `resize` (law initialisation, elements
//! added). The cache is observable — a temperature changed by the user
//! without a predictor / corrector is not seen by the next residual
//! assembly — and is reproduced.

use std::collections::BTreeMap;

use akantu_core::{
    Array, Element, ElementKind, ElementType, ElementTypeMapArray, Error, GhostType, Idx, Real,
    Result, TypeFilter,
};
use akantu_fe::FeEngine;
use akantu_mesh::Mesh;
use akantu_model::constitutive_law::{
    ConstitutiveLaw, ConstitutiveLawFactory, InternalFieldMut, InternalFieldRef, LawContext,
    ModelServices,
};
use akantu_model::internal_field::InternalField;
use akantu_parser::ByteString;

/// Name of the gradient internal (UTF-8 as upstream: `"∇u"`).
pub const GRAD_U: &str = "∇u";
/// Name of the flux internal (`"D∇u"`).
pub const D_GRAD_U: &str = "D∇u";
/// Name of the diffusivity internal (`dim × dim` per quadrature point,
/// column-major).
pub const DIFFUSIVITY: &str = "diffusivity";

/// Upstream-compatibility switches of the diffusion model (D-003 category B
/// defects found). The default (`false` everywhere) is the
/// corrected behaviour; differential replays against the unpatched oracle
/// set only the switch of the defect a case reproduces.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DiffusionUpstreamCompat {
    /// **UD-275**: upstream's element data accessor forwards the model tag
    /// `_diffusion` to every law, whose accessor raises
    /// `AKANTU_ERROR("Unknown ghost synchronization tag")` for every tag but
    /// `_diffusion_gradient` (`diffusion_law.cc:110-163`): a distributed
    /// model fails at its first residual assembly (confirmed on the oracle,
    /// `oracle/results/diffusion-parallel-strict.txt`). Default: a law
    /// contributes no data for tags it does not own; `true` reproduces the
    /// error.
    pub laws_reject_model_tags: bool,
    /// **UD-276**: `DiffusionLaw::assembleInternalFlow` and
    /// `assembleDiffusivityMatrix` `return` (instead of `continue`) on the
    /// first element type whose law filter is empty
    /// (`diffusion_law.cc:51-53, 82-84`), so the law's later types are
    /// never assembled (e.g. a law owning only the quadrangles of a mixed
    /// triangle / quadrangle mesh). Default: skip only the empty type.
    pub empty_filter_returns: bool,
    /// **UD-280**: `HeatDiffusion::getThermalEnergy(element)` receives the
    /// law-local element (`DiffusionModel::getEnergy(id, element)`) and uses
    /// its index as a **mesh** element index for the interpolation and the
    /// integration (`heat_diffusion.cc:100-122`): another element's energy
    /// when the law-local and mesh numberings differ (several laws).
    /// Default: the mesh element.
    pub element_energy_uses_law_local_index: bool,
    /// **UD-281**: `assembleLumpedMatrix("M")` is gated by the flag of the
    /// **consistent** capacity (`need_to_reassemble_capacity`,
    /// `diffusion_model.cc:105-110`): the lumped capacity is rebuilt at
    /// every request until a consistent capacity was assembled, and never
    /// afterwards (an explicit solve after `assembleCapacityMatrix` then
    /// fails without a lumped `M`). Default: the lumped capacity has its own
    /// flag (assembled once, again after mesh changes).
    pub lumped_capacity_gated_by_consistent_flag: bool,
    /// **UD-811** (latent B): `getStableTimeStep` takes
    /// the smallest inradius over **every** element type of the model
    /// dimension (`mesh.elementTypes(dim, _not_ghost)`: kind
    /// `_ek_not_defined`, `diffusion_model.cc:243`), and
    /// `GeometricalElement::getInradius` returns `0` for every geometry
    /// without a specialisation (`element_class.hh:116-119`: cohesive,
    /// structural): `Δt = 0` once such an element exists. Upstream (and the
    /// port) refuse those elements at `initFull` and on `ElementsAdded`, so
    /// only an element added without an event (raw mesh access) reaches it.
    /// Default: the regular elements only; `true` reproduces upstream.
    pub stable_time_step_includes_all_kinds: bool,
    /// **UD-553** (decision of 2026-11-11, category B, as for the
    /// solid model, MPI-053): upstream's `synchronize(tag)` for a
    /// tag without a registered synchronizer is a silent no-op. Default:
    /// `DiffusionModel::synchronize` refuses such a tag on a distributed
    /// mesh; `true` does nothing, as upstream.
    pub unregistered_synchronization_tag_is_a_noop: bool,
}

/// What a diffusion law reads during one phase: the model services (mesh,
/// FE engines, nodal fields), the diffusion field `u` (temperature) and
/// the model's release counter.
#[derive(Clone, Copy)]
pub struct DiffusionLawContext<'a> {
    /// Phase and model services (ARCHITECTURE).
    pub law: LawContext<'a>,
    /// The diffusion field (`u`, nodal, one component).
    pub diffusion: &'a Array<Real>,
    /// The model's `diffusion_release`.
    pub release: i64,
    /// Upstream-compatibility switches of the model.
    pub compat: DiffusionUpstreamCompat,
}

impl std::fmt::Debug for DiffusionLawContext<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DiffusionLawContext")
            .field("phase", &self.law.phase)
            .field("release", &self.release)
            .finish_non_exhaustive()
    }
}

impl DiffusionLawContext<'_> {
    /// The model's default FE engine.
    pub fn fe(&self) -> Result<&FeEngine> {
        self.law.services.fe_engine("")
    }

    /// The mesh.
    #[must_use]
    pub fn mesh(&self) -> &Mesh {
        self.law.services.mesh()
    }
}

/// State shared by every diffusion law (upstream `DiffusionLaw` members and
/// the `ConstitutiveLaw` base).
#[derive(Debug)]
pub struct DiffusionLawBase {
    /// User name (`name = steel`).
    pub name: String,
    /// Exact bytes of the name (upstream `std::string`).
    pub name_bytes: ByteString,
    /// Law id (`<model id>:<index>:<type>`).
    pub id: String,
    /// Registered type (`heat_diffusion`).
    pub law_type: String,
    /// Spatial dimension of the model.
    pub spatial_dimension: usize,
    /// Elements of the law per `(type, ghost)` (law-local → mesh index).
    pub element_filter: ElementTypeMapArray<Idx>,
    /// `∇u` per quadrature point (`dim` components).
    pub grad_u: InternalField<Real>,
    /// `D∇u` per quadrature point (`dim` components).
    pub d_grad_u: InternalField<Real>,
    /// Diffusivity per quadrature point (`dim × dim`, column-major).
    pub diffusivity: InternalField<Real>,
    /// Release counter of `∇u` per `(type, ghost)`.
    pub grad_u_release: BTreeMap<(ElementType, GhostType), i64>,
    /// Release counter of `diffusivity` per `(type, ghost)`.
    pub diffusivity_release: BTreeMap<(ElementType, GhostType), i64>,
    /// `true` after the law's initialisation (upstream `is_init`).
    pub is_init: bool,
}

/// Key used to sort `(type, ghost)` blocks in upstream order.
fn block_key(ty: ElementType, ghost: GhostType) -> (ElementType, GhostType) {
    (ty, ghost)
}

impl DiffusionLawBase {
    /// Upstream `ConstitutiveLaw(handler, id, dim, _ek_regular)` +
    /// `DiffusionLaw(model, id)`: the element filter holds an empty block
    /// for every regular element type of the model dimension of the mesh,
    /// both ghost types (`getElementFilter().initialize(mesh,
    /// _spatial_dimension = dim, _element_kind = _ek_regular)`), and the
    /// internals `∇u` (`dim`), `D∇u` (`dim`), `diffusivity` (`dim²`) are
    /// registered with default value `0` (release `0`, see the module
    /// documentation).
    pub fn new(id: &str, law_type: &str, mesh: &Mesh, spatial_dimension: usize) -> Result<Self> {
        let mut element_filter = ElementTypeMapArray::new(format!("{id}:element_filter"));
        let mut grad_u_release = BTreeMap::new();
        let mut diffusivity_release = BTreeMap::new();
        for ghost in GhostType::ALL {
            let types: Vec<ElementType> = mesh
                .element_types(TypeFilter {
                    spatial_dimension: Some(spatial_dimension),
                    ghost_type: ghost,
                    kind: Some(ElementKind::Regular),
                })
                .collect();
            for ty in types {
                element_filter.alloc(0, 1, ty, ghost, 0)?;
                grad_u_release.insert(block_key(ty, ghost), 0);
                diffusivity_release.insert(block_key(ty, ghost), 0);
            }
        }
        let d = spatial_dimension;
        Ok(Self {
            name: String::new(),
            name_bytes: ByteString::default(),
            id: id.to_owned(),
            law_type: law_type.to_owned(),
            spatial_dimension: d,
            element_filter,
            grad_u: InternalField::new(GRAD_U, d, 0.0)?,
            d_grad_u: InternalField::new(D_GRAD_U, d, 0.0)?,
            diffusivity: InternalField::new(DIFFUSIVITY, d * d, 0.0)?,
            grad_u_release,
            diffusivity_release,
            is_init: false,
        })
    }

    /// Set the user name (text and bytes).
    pub fn set_name(&mut self, name: impl Into<ByteString>) {
        let bytes: ByteString = name.into();
        self.name = String::from_utf8_lossy(bytes.as_bytes()).into_owned();
        self.name_bytes = bytes;
    }

    /// Element types of the filter of the law dimension for one ghost type,
    /// **including empty blocks**, in enum order (upstream
    /// `getElementFilter().elementTypes(dim, ghost)`).
    #[must_use]
    pub fn filter_types(&self, ghost: GhostType) -> Vec<ElementType> {
        self.element_filter
            .element_types(TypeFilter {
                spatial_dimension: Some(self.spatial_dimension),
                ghost_type: ghost,
                kind: None,
            })
            .collect()
    }

    /// Element filter of `(ty, ghost)` (empty if the block does not exist).
    #[must_use]
    pub fn filter(&self, ty: ElementType, ghost: GhostType) -> &[Idx] {
        self.element_filter
            .get(ty, ghost)
            .map(Array::as_slice)
            .unwrap_or(&[])
    }

    /// `(type, ghost, nb_quadrature_points)` of every filter block (upstream
    /// `resizeInternals` sizes).
    pub fn blocks(&self, services: &dyn ModelServices) -> Vec<(ElementType, GhostType, usize)> {
        self.element_filter
            .as_map()
            .iter_all()
            .map(|(ty, g, f)| (ty, g, f.len() * services.nb_integration_points(ty, g)))
            .collect()
    }

    /// Upstream `resizeInternals()`: the three internals sized to the filter
    /// (new entries `0`), release `+1` for every block of the filter.
    pub fn resize_internals(&mut self, services: &dyn ModelServices) -> Result<()> {
        let blocks = self.blocks(services);
        // register-id (byte-wise) order: "D∇u", "diffusivity", "∇u"
        self.d_grad_u.resize_blocks(&blocks)?;
        self.diffusivity.resize_blocks(&blocks)?;
        self.grad_u.resize_blocks(&blocks)?;
        for &(ty, g, _) in &blocks {
            *self.grad_u_release.entry(block_key(ty, g)).or_insert(0) += 1;
            *self
                .diffusivity_release
                .entry(block_key(ty, g))
                .or_insert(0) += 1;
        }
        Ok(())
    }

    /// Release of `∇u` for `(ty, ghost)` (`0` for a block never seen,
    /// upstream `ElementTypeMap<Int>` default).
    #[must_use]
    pub fn grad_u_release(&self, ty: ElementType, ghost: GhostType) -> i64 {
        self.grad_u_release
            .get(&block_key(ty, ghost))
            .copied()
            .unwrap_or(0)
    }

    /// Release of `diffusivity` for `(ty, ghost)`.
    #[must_use]
    pub fn diffusivity_release(&self, ty: ElementType, ghost: GhostType) -> i64 {
        self.diffusivity_release
            .get(&block_key(ty, ghost))
            .copied()
            .unwrap_or(0)
    }

    /// Upstream `DiffusionLaw::computeGradU(type, ghost)`: unless cached for
    /// the model's release, `∇u` of the filter's elements
    /// (`gradientOnIntegrationPoints(u, ∇u, 1, type, ghost, filter)`).
    pub fn compute_grad_u(
        &mut self,
        ctx: &DiffusionLawContext<'_>,
        ty: ElementType,
        ghost: GhostType,
    ) -> Result<()> {
        let rel = self.grad_u_release(ty, ghost);
        if rel != -1 && rel == ctx.release {
            return Ok(());
        }
        let fe = ctx.fe()?;
        let filter = self
            .element_filter
            .get(ty, ghost)
            .map(Array::as_slice)
            .unwrap_or(&[]);
        let sized = match fe.nb_integration_points(ty, ghost) {
            Ok(nq) => {
                self.grad_u.exists(ty, ghost)
                    && self.grad_u.get(ty, ghost)?.len() == filter.len() * nq
            }
            Err(_) => false,
        };
        if sized {
            // the block already has the gradient's shape: computed in place
            // (the same values; no temporary array and copy per call)
            let mut block = self.grad_u.get_mut(ty, ghost)?;
            fe.gradient_on_integration_points_view(
                ctx.mesh(),
                ctx.diffusion.view(),
                block.view_mut(),
                1,
                ty,
                ghost,
                Some(filter),
            )?;
        } else {
            let mut out = Array::new(self.spatial_dimension);
            fe.gradient_on_integration_points(
                ctx.mesh(),
                ctx.diffusion.view(),
                &mut out,
                1,
                ty,
                ghost,
                Some(filter),
            )?;
            store_block(&mut self.grad_u, ty, ghost, out)?;
        }
        self.grad_u_release
            .insert(block_key(ty, ghost), ctx.release);
        Ok(())
    }

    /// Visit the three internals in registration order (`∇u`, `D∇u`,
    /// `diffusivity`).
    pub fn for_each_internal(&self, f: &mut dyn FnMut(&str, InternalFieldRef<'_>)) {
        f(GRAD_U, InternalFieldRef::Real(&self.grad_u));
        f(D_GRAD_U, InternalFieldRef::Real(&self.d_grad_u));
        f(DIFFUSIVITY, InternalFieldRef::Real(&self.diffusivity));
    }

    /// Mutable visit of the three internals.
    pub fn for_each_internal_mut(
        &mut self,
        f: &mut dyn FnMut(&str, InternalFieldMut<'_>) -> Result<()>,
    ) -> Result<()> {
        f(GRAD_U, InternalFieldMut::Real(&mut self.grad_u))?;
        f(D_GRAD_U, InternalFieldMut::Real(&mut self.d_grad_u))?;
        f(DIFFUSIVITY, InternalFieldMut::Real(&mut self.diffusivity))
    }

    /// A real internal by its upstream name.
    pub fn internal(&self, name: &str) -> Result<&InternalField<Real>> {
        match name {
            GRAD_U => Ok(&self.grad_u),
            D_GRAD_U => Ok(&self.d_grad_u),
            DIFFUSIVITY => Ok(&self.diffusivity),
            _ => Err(Error::not_found(
                "internal",
                format!(
                    "{name} (the constitutive law {} ({}) does not contain an internal {name})",
                    self.name, self.id
                ),
            )),
        }
    }

    /// A real internal by its upstream name, mutable.
    pub fn internal_mut(&mut self, name: &str) -> Result<&mut InternalField<Real>> {
        match name {
            GRAD_U => Ok(&mut self.grad_u),
            D_GRAD_U => Ok(&mut self.d_grad_u),
            DIFFUSIVITY => Ok(&mut self.diffusivity),
            _ => Err(Error::not_found("internal", name)),
        }
    }
}

/// Replace the values of the block `(ty, ghost)` of `field` by `values`
/// (the block is (re)sized to `values`, as upstream's output arrays are
/// resized by the FE kernels; the release bookkeeping is the caller's).
pub(crate) fn store_block(
    field: &mut InternalField<Real>,
    ty: ElementType,
    ghost: GhostType,
    values: Array<Real>,
) -> Result<()> {
    field.resize(ty, ghost, values.len())?;
    let mut block = field.get_mut(ty, ghost)?;
    block.as_mut_slice().copy_from_slice(values.as_slice());
    Ok(())
}

/// A diffusion constitutive law (upstream `DiffusionLaw`).
///
/// Batch methods operate on one `(type, ghost)` block of the law's elements;
/// the model drives them in upstream order (see `DiffusionModel`).
pub trait DiffusionLaw: ConstitutiveLaw {
    /// Common state.
    fn base(&self) -> &DiffusionLawBase;

    /// Common state, mutable.
    fn base_mut(&mut self) -> &mut DiffusionLawBase;

    /// Upstream `initConstitutiveLaw()`: `resizeInternals()`, then
    /// `updateInternalParameters()`, then `is_init = true`.
    fn init_law(&mut self, services: &dyn ModelServices) -> Result<()> {
        self.base_mut().resize_internals(services)?;
        self.update_internal_parameters()?;
        self.base_mut().is_init = true;
        Ok(())
    }

    /// Upstream `computeDiffusivityOnQuadPoints(type, ghost)`.
    fn compute_diffusivity_on_quad_points(
        &mut self,
        ctx: &DiffusionLawContext<'_>,
        ty: ElementType,
        ghost: GhostType,
    ) -> Result<()>;

    /// Upstream `computeDiffusivityGradUOnQuadPoints(type, ghost)`
    /// (`D∇u` from `∇u`).
    fn compute_diffusivity_grad_u_on_quad_points(
        &mut self,
        ctx: &DiffusionLawContext<'_>,
        ty: ElementType,
        ghost: GhostType,
    ) -> Result<()>;

    /// Upstream `computeDiffusivityGradU(ghost)`: for every type of the
    /// filter (model dimension, empty blocks included), `computeGradU`
    /// then `computeDiffusivityGradUOnQuadPoints`.
    fn compute_diffusivity_grad_u(
        &mut self,
        ctx: &DiffusionLawContext<'_>,
        ghost: GhostType,
    ) -> Result<()> {
        for ty in self.base().filter_types(ghost) {
            self.base_mut().compute_grad_u(ctx, ty, ghost)?;
            self.compute_diffusivity_grad_u_on_quad_points(ctx, ty, ghost)?;
        }
        Ok(())
    }

    /// Upstream `getStableTimeStep(element_size)` (base law: `0`).
    fn stable_time_step(&self, element_size: Real) -> Result<Real> {
        let _ = element_size;
        Ok(0.0)
    }

    /// Upstream `getRho()` (base law: `1`): the capacity density used by the
    /// capacity matrices.
    fn rho(&self) -> Real {
        1.0
    }

    /// Upstream `getEnergy(id)` of the law (base: `0`).
    fn law_energy(&self, ctx: &DiffusionLawContext<'_>, energy_id: &str) -> Result<Real> {
        let _ = (ctx, energy_id);
        Ok(0.0)
    }

    /// Upstream `getEnergy(id, element)` of the law for the law-local
    /// element `local` of the mesh element `element` (base: `0`).
    fn law_energy_of_element(
        &self,
        ctx: &DiffusionLawContext<'_>,
        energy_id: &str,
        local: Element,
        element: Element,
    ) -> Result<Real> {
        let _ = (ctx, energy_id, local, element);
        Ok(0.0)
    }

    /// Bytes the law exchanges for `elements` (its own elements, in this
    /// order) and `tag` (upstream `DiffusionLaw::getNbData`): `∇u` of every
    /// quadrature point for `_diffusion_gradient`; other tags carry no law
    /// data (`Err` with `compat.laws_reject_model_tags`, UD-275).
    fn nb_data(
        &self,
        nb_quad: &dyn Fn(ElementType) -> usize,
        elements: &[(Element, usize)],
        tag: akantu_core::enums::SynchronizationTag,
        compat: DiffusionUpstreamCompat,
    ) -> Result<usize> {
        match tag {
            akantu_core::enums::SynchronizationTag::DiffusionGradient => {
                let d = self.base().spatial_dimension;
                Ok(elements
                    .iter()
                    .map(|(e, _)| nb_quad(e.ty()) * d * std::mem::size_of::<Real>())
                    .sum())
            }
            other if compat.laws_reject_model_tags => Err(unknown_tag(other)),
            _ => Ok(0),
        }
    }
}

/// Upstream `AKANTU_ERROR("Unknown ghost synchronization tag : " << tag)`.
pub(crate) fn unknown_tag(tag: akantu_core::enums::SynchronizationTag) -> Error {
    Error::invalid(format!("Unknown ghost synchronization tag : {tag}"))
}

/// Factory of diffusion laws (upstream `DiffusionFactory`).
pub type DiffusionLawFactory = ConstitutiveLawFactory<dyn DiffusionLaw>;

/// Kind of diffusion model a factory serves (upstream
/// `instantiateDiffusionLaw<Law, Model_>` checks the model type).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiffusionModelKind {
    /// A generic `DiffusionModel` (DOF name chosen by the user).
    Generic,
    /// A `HeatTransferModel` (DOF `temperature`).
    HeatTransfer,
}

/// Register the built-in laws (upstream static registration of
/// `heat_diffusion`, which works only with a `HeatTransferModel`: for a
/// generic model the allocator fails with upstream's message).
pub fn register_builtin_laws(
    factory: &mut DiffusionLawFactory,
    kind: DiffusionModelKind,
) -> Result<()> {
    match kind {
        DiffusionModelKind::HeatTransfer => factory.register(
            crate::heat_diffusion::HEAT_DIFFUSION,
            std::sync::Arc::new(|spec, services| {
                let law = crate::heat_diffusion::HeatDiffusion::new(spec, services)?;
                Ok(Box::new(law) as Box<dyn DiffusionLaw>)
            }),
        ),
        DiffusionModelKind::Generic => factory.register(
            crate::heat_diffusion::HEAT_DIFFUSION,
            std::sync::Arc::new(|_, _| {
                Err(Error::invalid(
                    "The diffusion law heat_diffusion works only with model of type \
                     akantu::HeatTransferModel",
                ))
            }),
        ),
    }
}
