// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): extra_packages/igfem/src/solid_mechanics_model_igfem{.hh,.cc,_inline_impl.hh}
// (SolidMechanicsModelIGFEM, DefaultMaterialIGFEMSelector). The parts that
// drive the spherical mesh generation are compiled only with the feature
// `cgal` (GPL-3.0-or-later crate akantu-geometry-cgal, decision D-042); this
// file itself is LGPL.

//! [`SolidMechanicsModelIgfem`] (IGFEM-004): a [`SolidMechanicsModel`] with
//! the IGFEM FE engine (`"IGFEMFEEngine"`), the IGFEM materials and the
//! spherical enrichment.
//!
//! # Structure
//!
//! `SolidMechanicsModelIgfem { smm, ctx, enrichment, … }`: the solid model
//! holds every law (regular and IGFEM, one numbering as upstream); the
//! IGFEM laws reach the IGFEM engine and the interface geometry through the
//! shared [`IgfemContext`]. The solver callback is the solid model's: the
//! IGFEM laws plug their residual and tangent into its material loop
//! through the law hooks (upstream `MaterialIGFEM` overrides). Mesh events
//! of an interface update go through [`IgfemEventHandler`]: the solid
//! model's handling, plus the full re-initialisation of the IGFEM engine at
//! the FE-engine level (upstream `onElementsAdded/Removed` →
//! `initShapeFunctions`), the `real_*` arrays and the old-element map at the
//! model level.
//!
//! # Moving interfaces
//!
//! `SolidMechanicsModelIgfem::update` and
//! `SolidMechanicsModelIgfem::move_interface` (feature `cgal`) re-mesh a live model in two
//! mesh mutations (D-095 rule 3; see `update`): node and element additions
//! with the erasure of the replaced elements, then the removal of the
//! previous enriched nodes (DOFs renumbered, nodal arrays compacted,
//! SMM-015).
//!
//! # Limitations (reported, not hidden)
//!
//! * IGFEM elements contribute no mass (upstream assembles the lumped mass
//!   with the regular engine only): static analyses only, as upstream's
//!   IGFEM tests.
//! * Serial only (UD-609; parallel IGFEM is an approved exception, D-085).

use std::collections::BTreeMap;
use std::sync::Arc;

use akantu_comm::phase::Phases;
use akantu_core::enums::EventPriority;
use akantu_core::tensor::MatrixX;
use akantu_core::{
    Array, Element, ElementKind, ElementType, Error, GhostType, Real, Result, TypeFilter,
};
use akantu_mesh::{Mesh, MeshEvent, MeshEventHandler, SharedMesh};
use akantu_model::ModelOptions;
use akantu_model::constitutive_law::{
    ConstitutiveLawSelector, LawIndexSelector, SelectorChain, SelectorContext,
};
use akantu_model::model::{apply_node_compaction, node_compaction_plan};
use akantu_parser::{ParamValue, Parser};
use akantu_solid::SolidMechanicsModel;
use akantu_solid::model::SmmEventParts;

use crate::context::IgfemContext;
use crate::element::{self, is_igfem};
use crate::engine::IgfemFeEngine;
use crate::material::{
    SubMaterialConstants, as_igfem_material, is_igfem_material, register_igfem_materials,
};

/// Default model id (upstream `"solid_mechanics_model_igfem"`).
pub const DEFAULT_ID: &str = "solid_mechanics_model_igfem";

/// Name of the IGFEM FE engine (upstream `"IGFEMFEEngine"`).
pub const IGFEM_FE_ENGINE: &str = "IGFEMFEEngine";

/// Upstream `DefaultMaterialIGFEMSelector` (first part): IGFEM elements
/// get `fallback_value_igfem` (the first IGFEM material); other elements
/// defer to the rest of the chain (upstream `DefaultMaterialSelector`).
#[derive(Clone, Copy, Debug)]
pub struct IgfemKindSelector(pub usize);

impl ConstitutiveLawSelector for IgfemKindSelector {
    fn select(&self, _ctx: &SelectorContext<'_>, element: Element) -> Result<Option<usize>> {
        Ok(is_igfem(element.ty()).then_some(self.0))
    }
}

/// Upstream `SolidMechanicsModelIGFEM`.
#[derive(Debug)]
pub struct SolidMechanicsModelIgfem {
    /// The solid mechanics model (regular and IGFEM laws).
    pub smm: SolidMechanicsModel,
    /// IGFEM engine and interface geometry, shared with the IGFEM laws.
    pub ctx: Arc<IgfemContext>,
    /// The spherical enrichment (upstream `IGFEMEnrichment` base).
    #[cfg(feature = "cgal")]
    pub enrichment: crate::enrichment::SphericalEnrichment,
    real_displacement: Option<Array<Real>>,
    real_force: Option<Array<Real>>,
    real_residual: Option<Array<Real>>,
    element_map: BTreeMap<Element, Element>,
    custom_selector: bool,
    igfem_fallback: usize,
}

impl SolidMechanicsModelIgfem {
    /// Upstream constructor: the solid model (input-file snapshot
    /// `parser`), the IGFEM FE engine on the IGFEM elements of the model
    /// dimension (precomputed for both ghost types), `igfem_elastic`
    /// registered in the model's material factory.
    ///
    /// # Errors
    /// See [`SolidMechanicsModel::with_parser`]; IGFEM precomputation errors.
    pub fn new(
        mesh: SharedMesh,
        spatial_dimension: Option<usize>,
        id: &str,
        parser: Parser,
    ) -> Result<Self> {
        let mut smm =
            SolidMechanicsModel::with_parser(mesh.clone(), spatial_dimension, id, parser)?;
        let dim = smm.spatial_dimension();
        let (engine, _enrichment_mesh_dim) = {
            let m = mesh.read()?;
            (
                IgfemFeEngine::with_precomputation(
                    &m,
                    Some(dim),
                    format!("{id}:fem:{IGFEM_FE_ENGINE}"),
                )?,
                m.spatial_dimension(),
            )
        };
        let ctx = Arc::new(IgfemContext::new(engine));
        register_igfem_materials(&mut smm.physics.factory, &ctx)?;
        // upstream constructor: the "igfem elements" dumper (IGFEM-010)
        crate::dump::register_igfem_dumper(&mut smm, id, dim)?;
        smm.dumpers.set_route(crate::dump::route);
        // upstream NonLocalManagerIGFEM (IGFEM-009): the IGFEM points in the
        // non-local layout and the model services
        smm.physics
            .extra_point_sources
            .push(Arc::new(crate::non_local::IgfemPointSource {
                ctx: ctx.clone(),
            }));
        #[cfg(feature = "cgal")]
        let enrichment = crate::enrichment::SphericalEnrichment::new(&*mesh.read()?)?;
        Ok(Self {
            smm,
            ctx,
            #[cfg(feature = "cgal")]
            enrichment,
            real_displacement: None,
            real_force: None,
            real_residual: None,
            element_map: BTreeMap::new(),
            custom_selector: false,
            igfem_fallback: 0,
        })
    }

    /// The shared mesh.
    #[must_use]
    pub fn mesh(&self) -> &SharedMesh {
        &self.smm.physics.base.mesh
    }

    /// Upstream `setMaterialSelector(selector)`: replaces the default
    /// IGFEM selector (the user selector with the default chain as
    /// fallback).
    pub fn set_material_selector(&mut self, selector: Box<dyn ConstitutiveLawSelector>) {
        self.custom_selector = true;
        self.smm.set_material_selector(selector);
    }

    /// Replace the whole selector chain (counts as a user selector).
    pub fn set_material_selector_chain(&mut self, chain: SelectorChain) {
        self.custom_selector = true;
        self.smm.set_material_selector_chain(chain);
    }

    /// Upstream `initFull(options)`: the enrichment (`initialize`:
    /// standard node count), the method and solver, the laws of the input
    /// file, the IGFEM selector fallback (first IGFEM law, upstream
    /// `initMaterials`), the sub-material constants of the IGFEM laws, the
    /// law assignment and `initMaterial`; finally the `real_*` arrays (zero).
    ///
    /// # Errors
    /// No IGFEM law, unknown or non-elastic sub-material, solid-model
    /// errors.
    pub fn init_full(&mut self, options: &ModelOptions) -> Result<()> {
        #[cfg(feature = "cgal")]
        {
            let mesh_h = self.mesh().clone();
            let m = mesh_h.read()?;
            self.enrichment.initialize(&m)?;
        }
        self.smm.init_method_and_solver(options)?;
        let instantiated = self.smm.instantiate_materials()?;
        let pre = match instantiated {
            Some(Err(e)) => Err(e),
            _ => self.prepare_igfem_laws(),
        };
        self.smm.init_materials_after(pre)?;
        let (nn, dim) = (self.mesh().read()?.nb_nodes(), self.smm.spatial_dimension());
        self.real_displacement = Some(Array::try_zeros(nn, dim)?);
        self.real_force = Some(Array::try_zeros(nn, dim)?);
        self.real_residual = Some(Array::try_zeros(nn, dim)?);
        Ok(())
    }

    /// First IGFEM law, selector, sub-material constants.
    fn prepare_igfem_laws(&mut self) -> Result<()> {
        let n = self.smm.nb_materials();
        let first = (0..n)
            .find(|&i| self.smm.material(i).is_ok_and(is_igfem_material))
            .ok_or_else(|| Error::invalid("No igfem materials in the material input file"))?;
        self.igfem_fallback = first;
        if !self.custom_selector {
            self.smm.set_material_selector_chain(SelectorChain {
                selectors: vec![
                    Box::new(IgfemKindSelector(first)),
                    Box::new(LawIndexSelector),
                ],
                fallback_value: 0,
            });
        }
        self.refresh_sub_material_constants()
    }

    /// Index of the first IGFEM law (upstream `fallback_value_igfem`).
    #[must_use]
    pub fn igfem_fallback(&self) -> usize {
        self.igfem_fallback
    }

    /// Copy the elastic constants of the sub-materials into every IGFEM
    /// law (upstream `updateElasticInternals` reads
    /// `dynamic_cast<MaterialElastic &>(model.getMaterial(name))`), and the
    /// damage parameters of the second sub-material into the IGFEM damage
    /// laws (upstream `initMaterial` of IGFEM-007 / 008).
    ///
    /// A sub-material must be an elastic law or a law built on it (upstream
    /// `MaterialElastic` and its subclasses, e.g. `damage_iterative`,
    /// `iterative_stiffness_reduction`): a law exposing the real parameters
    /// `lambda`, `mu` and `kapa` of `MaterialElastic` (`neohookean` exposes
    /// them too but is not a `MaterialElastic` upstream and is refused). The
    /// rule accepts a superset of upstream's laws, any law that registers
    /// the three names (UD-1210).
    ///
    /// # Errors
    /// Unknown sub-material, or one that is not elastic-based (upstream
    /// `std::bad_cast`); a damage parameter missing on the second
    /// sub-material.
    pub fn refresh_sub_material_constants(&mut self) -> Result<()> {
        let n = self.smm.nb_materials();
        let mut updates = Vec::new();
        for i in 0..n {
            let law = self.smm.material(i)?;
            let Some(igfem) = crate::material::as_igfem(law) else {
                continue;
            };
            let names = igfem.igfem_base().sub_material_names();
            let mut c = [SubMaterialConstants::default(); 2];
            let mut subs = [0usize; 2];
            for (k, name) in names.iter().enumerate() {
                let idx = self.smm.physics.materials.law_index_of(name).map_err(|_| {
                    Error::not_found(
                        "IGFEM sub-material",
                        format!("{name} (named by IGFEM law {})", law.base().name),
                    )
                })?;
                let sub = self.smm.material(idx)?;
                let real = |p: &str| -> Result<Real> {
                    match sub.get_param(p)? {
                        ParamValue::Real(v) => Ok(v),
                        _ => Err(Error::invalid(format!(
                            "{name}: parameter {p} is not a real"
                        ))),
                    }
                };
                let elastic_based = sub.law_type() != "neohookean"
                    && real("lambda").is_ok()
                    && real("mu").is_ok()
                    && real("kapa").is_ok();
                if !elastic_based {
                    return Err(Error::invalid(format!(
                        "IGFEM sub-material {name} is a {} law; the IGFEM laws need elastic(-based) \
                         sub-materials (upstream dynamic_cast<MaterialElastic>)",
                        sub.law_type()
                    )));
                }
                c[k] = SubMaterialConstants {
                    lambda: real("lambda")?,
                    mu: real("mu")?,
                    kappa: real("kapa")?,
                };
                subs[k] = idx;
            }
            updates.push((i, c, subs[1]));
        }
        for (i, c, damageable) in updates {
            // the parameters of the second sub-material, by name
            let params: BTreeMap<&'static str, ParamValue> = {
                let sub = self.smm.material(damageable)?;
                sub.param_names()
                    .into_iter()
                    .filter_map(|p| sub.get_param(p).ok().map(|v| (p, v)))
                    .collect()
            };
            let sub_name = self.smm.material(damageable)?.base().name.clone();
            let lookup = |p: &str| -> Result<ParamValue> {
                params.get(p).cloned().ok_or_else(|| {
                    Error::not_found(
                        "parameter of the IGFEM damage law's second sub-material",
                        format!("{p} of {sub_name}"),
                    )
                })
            };
            if let Some(m) = crate::material::as_igfem_mut(self.smm.material_mut(i)?) {
                m.igfem_base_mut().set_sub_material_constants(c);
                m.read_sub_material_parameters(&lookup)?;
            }
        }
        Ok(())
    }

    /// Upstream `real_displacement` (physical displacement at every node,
    /// see [`SolidMechanicsModelIgfem::compute_values_on_enriched_nodes`]).
    ///
    /// # Errors
    /// Not allocated (call `init_full`).
    pub fn real_displacement(&self) -> Result<&Array<Real>> {
        self.real_displacement
            .as_ref()
            .ok_or_else(|| Error::invalid("real_displacement is not allocated (init_full first)"))
    }

    /// Upstream `real_force`.
    ///
    /// # Errors
    /// Not allocated.
    pub fn real_force(&self) -> Result<&Array<Real>> {
        self.real_force
            .as_ref()
            .ok_or_else(|| Error::invalid("real_force is not allocated (init_full first)"))
    }

    /// Upstream `real_residual`.
    ///
    /// # Errors
    /// Not allocated.
    pub fn real_residual(&self) -> Result<&Array<Real>> {
        self.real_residual
            .as_ref()
            .ok_or_else(|| Error::invalid("real_residual is not allocated (init_full first)"))
    }

    /// The new → old element map of the last interface update (upstream
    /// `element_map`; old elements in the numbering before the update).
    #[must_use]
    pub fn element_map(&self) -> &BTreeMap<Element, Element> {
        &self.element_map
    }

    /// Upstream `computeValuesOnEnrichedNodes()`: `real_displacement` =
    /// displacement at the standard nodes and, at every enriched node, the
    /// IGFEM interpolation of the displacement at its coordinates (the
    /// last element of the ghost/type/element loop wins, as upstream).
    ///
    /// # Errors
    /// Not initialised, malformed mesh, inverse-map errors.
    pub fn compute_values_on_enriched_nodes(&mut self) -> Result<()> {
        let mesh_h = self.mesh().clone();
        let mesh = mesh_h.read()?;
        let dim = self.smm.spatial_dimension();
        let disp = self.smm.displacement()?.clone();
        let mut real = disp.clone();
        let engine = self.ctx.engine()?;
        for ghost in GhostType::ALL {
            let types: Vec<ElementType> = mesh
                .element_types(TypeFilter {
                    spatial_dimension: Some(dim),
                    ghost_type: ghost,
                    kind: Some(ElementKind::Igfem),
                })
                .collect();
            for ty in types {
                let np = element::nb_parent_nodes(ty)?;
                let ne = element::nb_enrichments(ty)?;
                for e in 0..mesh.nb_element(ty, ghost) {
                    let el = Element::try_new(ty, e, ghost)?;
                    let x = mesh.extract_nodal_values_from_element(&mesh.nodes(), el)?;
                    let u = mesh.extract_nodal_values_from_element(&disp, el)?;
                    let conn = mesh.element_connectivity(el)?.to_vec();
                    for i in 0..ne {
                        let xe = &x[(np + i) * dim..(np + i + 1) * dim];
                        let v = engine.interpolate(&mesh, xe, &u, el)?;
                        let node = akantu_core::try_to_usize(conn[np + i])?;
                        real.tuple_mut(node).copy_from_slice(&v);
                    }
                }
            }
        }
        self.real_displacement = Some(real);
        Ok(())
    }

    /// Upstream `getSubElementBarycenter(element, sub_element, type,
    /// barycenter, ghost)`.
    ///
    /// # Errors
    /// Not an IGFEM element, malformed mesh.
    pub fn sub_element_barycenter(&self, element: Element, s: usize) -> Result<Vec<Real>> {
        let mesh = self.mesh().read()?;
        let dim = mesh.spatial_dimension();
        let x = mesh.extract_nodal_values_from_element(&mesh.nodes(), element)?;
        let mut b = vec![0.0; dim];
        element::sub_element_barycenter(element.ty(), &x, dim, s, &mut b)?;
        Ok(b)
    }

    /// Upstream `SolidMechanicsModelIGFEM::applyEigenGradU(prescribed,
    /// material_name, ghost)`: every IGFEM law sets the eigen gradient of
    /// its points in the sub-material `material_name`; a regular law named
    /// `material_name` sets it everywhere.
    ///
    /// # Errors
    /// Wrong matrix size.
    pub fn apply_eigen_grad_u(
        &mut self,
        prescribed: &MatrixX,
        material_name: &str,
        ghost: GhostType,
    ) -> Result<()> {
        for i in 0..self.smm.nb_materials() {
            let law = self.smm.material_mut(i)?;
            if let Some(m) = as_igfem_material(law) {
                m.apply_eigen_grad_u_to_sub_material(prescribed, material_name, ghost)?;
            } else if law.base().name == material_name {
                law.base_mut().apply_eigen_grad_u(prescribed, ghost)?;
            }
        }
        Ok(())
    }

    /// Upstream `addDumpField(name)` (default dumper; `real_displacement`
    /// besides the solid model's fields).
    ///
    /// # Errors
    /// See [`SolidMechanicsModelIgfem::add_dump_group_field_to_dumper`].
    pub fn add_dump_field(&mut self, name: &str) -> Result<()> {
        let d = crate::dump::regular_descriptor(&self.smm, name, "all", false)?;
        self.smm.dumpers.dumpable.add_dump_field(d)
    }

    /// Upstream `addDumpFieldVector(name)` (default dumper, padding).
    ///
    /// # Errors
    /// See [`SolidMechanicsModelIgfem::add_dump_group_field_to_dumper`].
    pub fn add_dump_field_vector(&mut self, name: &str) -> Result<()> {
        let d = crate::dump::regular_descriptor(&self.smm, name, "all", true)?;
        self.smm.dumpers.dumpable.add_dump_field(d)
    }

    /// Upstream `addDumpFieldToDumper(dumper, name)`.
    ///
    /// # Errors
    /// See [`SolidMechanicsModelIgfem::add_dump_group_field_to_dumper`].
    pub fn add_dump_field_to_dumper(&mut self, dumper: &str, name: &str) -> Result<()> {
        self.add_dump_group_field_to_dumper(dumper, name, "all", false)
    }

    /// Upstream `addDumpFieldVectorToDumper(dumper, name)`.
    ///
    /// # Errors
    /// See [`SolidMechanicsModelIgfem::add_dump_group_field_to_dumper`].
    pub fn add_dump_field_vector_to_dumper(&mut self, dumper: &str, name: &str) -> Result<()> {
        self.add_dump_group_field_to_dumper(dumper, name, "all", true)
    }

    /// Upstream `SolidMechanicsModelIGFEM::addDumpGroupFieldToDumper`: on
    /// [`crate::dump::IGFEM_DUMPER`] the field with element kind
    /// `_ek_igfem` (IGFEM internals, `material_index` and `partitions` per
    /// sub-element, nodal fields, `real_displacement`; module docs of
    /// [`crate::dump`]); on any other dumper the solid model's field, or
    /// `real_displacement`.
    ///
    /// # Errors
    /// Unknown dumper or group, duplicate field, a name refused on the IGFEM
    /// dumper (UD-1208) or unknown to the solid model.
    pub fn add_dump_group_field_to_dumper(
        &mut self,
        dumper: &str,
        name: &str,
        group: &str,
        padding: bool,
    ) -> Result<()> {
        if dumper == crate::dump::IGFEM_DUMPER {
            let d = crate::dump::igfem_descriptor(&self.smm.physics, name, group, padding)?;
            return self
                .smm
                .dumpers
                .dumpable
                .add_dump_field_to_dumper(dumper, d);
        }
        if name == crate::dump::REAL_DISPLACEMENT {
            let d = crate::dump::regular_descriptor(&self.smm, name, group, padding)?;
            if group == "all" {
                return self
                    .smm
                    .dumpers
                    .dumpable
                    .add_dump_field_to_dumper(dumper, d);
            }
            return Err(Error::Unsupported(format!(
                "'{name}' on the element group '{group}': register it on a model dumper"
            )));
        }
        self.smm
            .add_dump_group_field_to_dumper(dumper, name, group, padding)
    }

    /// Upstream `dump()` (default dumper) with the IGFEM `onDump`
    /// (`computeValuesOnEnrichedNodes` first).
    ///
    /// # Errors
    /// See [`SolidMechanicsModelIgfem::dump_named`].
    pub fn dump(&mut self, time: Option<Real>, step: Option<usize>) -> Result<()> {
        self.dump_dumper(None, time, step)
    }

    /// Upstream `dump(dumper)` with the IGFEM `onDump`
    /// (`computeValuesOnEnrichedNodes`, then the solid model's): the IGFEM
    /// dumper writes the sub-element mesh ([`crate::dump`]), the others the
    /// solid model's mesh with `real_displacement` available.
    ///
    /// # Errors
    /// Unknown dumper, event-handler, mesh and resolver errors.
    pub fn dump_named(
        &mut self,
        dumper: &str,
        time: Option<Real>,
        step: Option<usize>,
    ) -> Result<()> {
        self.dump_dumper(Some(dumper), time, step)
    }

    fn dump_dumper(
        &mut self,
        dumper: Option<&str>,
        time: Option<Real>,
        step: Option<usize>,
    ) -> Result<()> {
        self.compute_values_on_enriched_nodes()?;
        let real = self.real_displacement.take().ok_or_else(|| {
            Error::invalid("real_displacement is not allocated (init_full first)")
        })?;
        let r = if dumper == Some(crate::dump::IGFEM_DUMPER) {
            crate::dump::dump_igfem_elements(&mut self.smm, Some(&real), time, step)
        } else {
            self.smm.dump_with_fields_of(
                &crate::dump::RealDisplacementResolver(&real),
                &crate::dump::is_real_displacement,
                dumper,
                time,
                step,
            )
        };
        self.real_displacement = Some(real);
        r
    }

    /// Volume (area) of the mesh: `∫ 1` over the regular elements (model
    /// FE engine) and the IGFEM elements (IGFEM engine) of the model
    /// dimension, not-ghost (upstream patch tests' volume check).
    ///
    /// # Errors
    /// FE errors.
    pub fn volume(&self) -> Result<Real> {
        let mesh = self.mesh().read()?;
        let dim = self.smm.spatial_dimension();
        let ghost = GhostType::NotGhost;
        let mut v = 0.0;
        let fe = self.smm.physics.base.fems.get("")?;
        let regular: Vec<ElementType> = mesh
            .element_types(TypeFilter {
                spatial_dimension: Some(dim),
                ghost_type: ghost,
                kind: Some(ElementKind::Regular),
            })
            .collect();
        for ty in regular {
            let n = mesh.nb_element(ty, ghost) * fe.nb_integration_points(ty, ghost)?;
            let ones = Array::filled(n, 1, 1.0);
            v += fe.integrate_scalar(ones.view(), ty, ghost, None)?;
        }
        let engine = self.ctx.engine()?;
        for ty in engine.element_types(&mesh, ghost).collect::<Vec<_>>() {
            let n = mesh.nb_element(ty, ghost) * engine.nb_integration_points(ty)?;
            let ones = Array::filled(n, 1, 1.0);
            v += engine.integrate_scalar(&mesh, ones.view(), ty, ghost, None)?;
        }
        Ok(v)
    }
}

#[cfg(feature = "cgal")]
impl SolidMechanicsModelIgfem {
    /// Upstream `registerGeometryObject(geometry, domain)` (the materials
    /// see the new geometry immediately).
    ///
    /// # Errors
    /// A domain registered twice.
    pub fn register_geometry_object(
        &mut self,
        geometry: Vec<akantu_geometry_cgal::SkSphere>,
        domain: &str,
    ) -> Result<()> {
        self.enrichment.register_geometry_object(geometry, domain)?;
        self.publish_geometry()
    }

    /// Upstream `unRegisterGeometryObject(domain)`.
    ///
    /// # Errors
    /// Unregistered domain.
    pub fn unregister_geometry_object(&mut self, domain: &str) -> Result<()> {
        self.enrichment.unregister_geometry_object(domain)?;
        self.publish_geometry()
    }

    fn publish_geometry(&self) -> Result<()> {
        self.ctx
            .set_geometry(Some(Arc::new(self.enrichment.geometry_snapshot())))
    }

    /// Upstream `isInside(point, domain)`.
    ///
    /// # Errors
    /// Unknown domain.
    pub fn is_inside(&self, point: &[Real], domain: &str) -> Result<bool> {
        use crate::enrichment::InterfaceGeometry;
        self.enrichment.is_inside(point, domain)
    }

    /// Upstream `update(domain)`: re-mesh the interface `domain` (`""`: the
    /// default) — the first call creates the IGFEM elements, later calls
    /// move them (the interface moved or its spheres changed).
    ///
    /// Upstream sends, in one `buildIGFEMMeshFromSpheres`: the new nodes,
    /// the changed / new / removed elements, then `removeAdditionalNodes`
    /// (the previous enriched nodes removed, the new ones renumbered). The
    /// node removal renumbers nodes the additions name, which one mutation
    /// cannot compose (D-095 rule 3), so the port runs **two mutations**:
    ///
    /// 1. node and element additions and the erasure of the replaced
    ///    elements (one `erase_elements` call before the additions; the old
    ///    → new element map travels with `ElementsAdded`, and the handler
    ///    captures the replaced elements' internals before the laws compact
    ///    them, for the transfer of IGFEM-007 / 008); then upstream's
    ///    `reassignMaterial`;
    /// 2. the node removal (`NodesRemoved`: DOFs renumbered by the DOF
    ///    manager, the solid model's and the `real_*` nodal arrays
    ///    compacted, the IGFEM engine re-initialised).
    ///
    /// The first pass records no removal (nothing to remove). If mutation 1
    /// is refused, nothing changed; if mutation 2 is refused after mutation
    /// 1 committed, the mesh keeps the previous enriched nodes as orphans
    /// (their DOFs hold no stiffness until then: the system is singular
    /// between the refusal and the next `update`), the generator's
    /// "removal owed" flag stays set (it is cleared only after mutation 2
    /// has committed), and the next `update` removes them first.
    ///
    /// # Errors
    /// Unknown domain, mesh-generation and event errors (refusals of either
    /// mutation are recoverable).
    pub fn update(&mut self, domain: &str) -> Result<crate::growing_gel::GrowingGelPass> {
        self.mutate_interface(domain, None)
    }

    /// Upstream `moveInterface(new_position, domain)`: new radius of the
    /// domain's spheres, then [`SolidMechanicsModelIgfem::update`].
    ///
    /// # Errors
    /// See [`SolidMechanicsModelIgfem::update`].
    pub fn move_interface(
        &mut self,
        new_position: Real,
        domain: &str,
    ) -> Result<crate::growing_gel::GrowingGelPass> {
        self.mutate_interface(domain, Some(new_position))
    }

    /// Replace the spheres of a registered domain (upstream edits the list
    /// it registered by reference between updates); the materials see the
    /// new geometry at once.
    ///
    /// # Errors
    /// Unregistered domain.
    pub fn set_geometry_object(
        &mut self,
        geometry: Vec<akantu_geometry_cgal::SkSphere>,
        domain: &str,
    ) -> Result<()> {
        self.enrichment.set_geometry_object(geometry, domain)?;
        self.publish_geometry()
    }

    fn mutate_interface(
        &mut self,
        domain: &str,
        new_radius: Option<Real>,
    ) -> Result<crate::growing_gel::GrowingGelPass> {
        // a node removal left over by an update whose second mutation failed
        if self.enrichment.intersector().has_pending_removal() {
            self.mutate_with_handler(|tx, e| e.remove_previous_enriched_nodes(tx))?;
            // cleared only now that the removal has committed
            self.enrichment.intersector_mut().clear_pending_removal();
        }
        if let Some(r) = new_radius {
            self.enrichment.set_radius(r, domain)?;
        }
        self.refresh_sub_material_constants()?;
        self.publish_geometry()?;
        // mutation 1: additions and the erasure of the replaced elements
        let r = self.mutate_with_handler(|tx, e| e.update_additions(tx, domain));
        let mut pass = match r {
            Ok(p) => p,
            Err(err) => {
                self.enrichment.intersector_mut().clear_pending_removal();
                return Err(err);
            }
        };
        // upstream onElementsAdded ends with reassignMaterial()
        if pass.new_elements > 0 {
            self.smm.reassign_material()?;
        }
        // mutation 2: the previous enriched nodes removed (none on the
        // first pass: no mutation, upstream's identity removal, UD-607)
        if self.enrichment.nb_enriched_nodes() > 0 {
            pass.removed_nodes =
                self.mutate_with_handler(|tx, e| e.remove_previous_enriched_nodes(tx))?;
            // cleared only now that the removal has committed
            self.enrichment.intersector_mut().clear_pending_removal();
        } else {
            self.enrichment.intersector_mut().clear_pending_removal();
        }
        // the element events invalidated the non-local neighbourhoods
        // (regular and IGFEM kinds, IGFEM-009): rebuild them
        if pass.new_elements > 0 && self.smm.is_non_local() {
            self.smm.reinitialize_non_local()?;
        }
        Ok(pass)
    }

    /// One mesh mutation coordinated by the IGFEM model's event handler,
    /// with the enrichment's topology change `change`.
    fn mutate_with_handler<R>(
        &mut self,
        change: impl FnOnce(
            &mut akantu_mesh::MeshTransaction<'_>,
            &mut crate::enrichment::SphericalEnrichment,
        ) -> Result<R>,
    ) -> Result<R> {
        let mesh_h = self.mesh().clone();
        let dim = self.smm.spatial_dimension();
        let Self {
            smm,
            ctx,
            enrichment,
            real_displacement,
            real_force,
            real_residual,
            element_map,
            ..
        } = self;
        let (token, parts) = smm.split_for_mutation();
        let mut handler = IgfemEventHandler::new(
            parts,
            ctx,
            [real_displacement, real_force, real_residual],
            element_map,
            dim,
        );
        let mut out = None;
        mesh_h.mutate(
            &mut [(token, &mut handler)],
            |tx| {
                out = Some(change(tx, enrichment)?);
                Ok(())
            },
            None,
        )?;
        // a mutation that recorded no event still commits a new revision:
        // the IGFEM engine follows it (its blocks are checked against the
        // revision of their precomputation)
        {
            let m = mesh_h.read()?;
            let mut e = ctx.engine_mut()?;
            if e.mesh_revision() != m.revision() {
                e.init_shape_functions(&m, GhostType::NotGhost)?;
                e.init_shape_functions(&m, GhostType::Ghost)?;
            }
        }
        out.ok_or_else(|| Error::invalid("IGFEM mesh mutation: the change produced no result"))
    }
}

/// The IGFEM model's mesh-event handler: the solid model's handling plus
/// upstream `SolidMechanicsModelIGFEM::onElementsAdded/Removed` (IGFEM
/// engine re-initialised, at the FE-engine level, before any law sees the
/// event), `onNodesAdded` (the `real_*` arrays follow the node count) and
/// `onNodesRemoved` (the `real_*` arrays compacted with the removal's
/// numbering, before the solid model compacts its own arrays).
pub struct IgfemEventHandler<'a, 'b> {
    smm: SmmEventParts<'b>,
    ctx: &'a IgfemContext,
    real: [&'a mut Option<Array<Real>>; 3],
    element_map: &'a mut BTreeMap<Element, Element>,
    dim: usize,
    /// Internals of the elements erased in this mutation, captured before
    /// the laws compact them (the transfer of IGFEM-007 / 008).
    snapshot: crate::transfer::TransferSnapshot,
}

impl std::fmt::Debug for IgfemEventHandler<'_, '_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IgfemEventHandler").finish_non_exhaustive()
    }
}

impl<'a, 'b> IgfemEventHandler<'a, 'b> {
    /// Handler of one mutation over the solid model's parts `smm` and the
    /// IGFEM model state.
    pub fn new(
        smm: SmmEventParts<'b>,
        ctx: &'a IgfemContext,
        real: [&'a mut Option<Array<Real>>; 3],
        element_map: &'a mut BTreeMap<Element, Element>,
        dim: usize,
    ) -> Self {
        Self {
            smm,
            ctx,
            real,
            element_map,
            dim,
            snapshot: crate::transfer::TransferSnapshot::default(),
        }
    }
}

impl IgfemEventHandler<'_, '_> {
    fn before(&mut self, mesh: &Mesh, event: &MeshEvent, level: EventPriority) -> Result<()> {
        if level != EventPriority::MODEL {
            return Ok(());
        }
        match event {
            MeshEvent::NodesAdded { .. } => {
                let n = mesh.nb_nodes();
                for a in self.real.iter_mut().filter_map(|a| a.as_mut()) {
                    if a.nb_component() == self.dim {
                        a.try_resize(n, 0.0)?;
                    }
                }
            }
            // upstream `SolidMechanicsModelIGFEM::onNodesRemoved`:
            // `mesh.removeNodesFromArray(real_*, new_numbering)`, then the
            // solid model (validated in `check_mesh_event`)
            MeshEvent::NodesRemoved { new_numbering, .. } => {
                let plans = self.real_compaction_plans(new_numbering)?;
                for (a, plan) in self.real.iter_mut().filter_map(|a| a.as_mut()).zip(&plans) {
                    apply_node_compaction(a, plan);
                }
            }
            MeshEvent::ElementsAdded {
                elements,
                extension: Some(ext),
                ..
            } => {
                if let Some(igfem) = ext.downcast_ref::<crate::events::NewIgfemElements>() {
                    self.element_map.clear();
                    for (new, old) in elements.iter().zip(&igfem.old_elements) {
                        self.element_map.insert(*new, *old);
                    }
                }
            }
            // the replaced elements' internals, before the laws compact
            // them (upstream reads them while handling NewIGFEMElementsEvent,
            // which it sends before RemovedElementsEvent)
            MeshEvent::ElementsRemoved { elements, .. } => {
                let materials = &self.smm.physics.materials;
                let names = crate::transfer::transferred_internals(materials);
                if !names.is_empty() {
                    let snap =
                        crate::transfer::TransferSnapshot::capture(materials, elements, &names)?;
                    self.snapshot.elements.extend(snap.elements);
                }
            }
            _ => {}
        }
        Ok(())
    }

    /// The compaction plans of the allocated `real_*` arrays for a node
    /// removal's numbering (checked: one row per entry, a dense injective
    /// map).
    #[allow(clippy::type_complexity)] // the plans of `node_compaction_plan`
    fn real_compaction_plans(
        &self,
        new_numbering: &[akantu_core::Idx],
    ) -> Result<Vec<(Vec<Option<usize>>, usize)>> {
        self.real
            .iter()
            .filter_map(|a| a.as_ref())
            .map(|a| node_compaction_plan(a.len(), new_numbering))
            .collect()
    }

    fn after(&mut self, mesh: &Mesh, event: &MeshEvent, level: EventPriority) -> Result<()> {
        // upstream MaterialIGFEMSawToothDamage::onElementsAdded: the
        // transfer, once the laws hold the new elements (sub-materials set)
        if level == EventPriority::MODEL {
            if let MeshEvent::ElementsAdded {
                elements,
                extension: Some(ext),
                ..
            } = event
                && ext.is::<crate::events::NewIgfemElements>()
            {
                crate::transfer::transfer_internals(
                    &mut self.smm.physics.materials,
                    &self.snapshot,
                    self.element_map,
                    elements,
                )?;
            }
            return Ok(());
        }
        if level != EventPriority::FE_ENGINE {
            return Ok(());
        }
        if matches!(
            event,
            MeshEvent::ElementsAdded { .. }
                | MeshEvent::ElementsRemoved { .. }
                | MeshEvent::NodesAdded { .. }
                | MeshEvent::NodesRemoved { .. }
        ) {
            let mut e = self.ctx.engine_mut()?;
            e.init_shape_functions(mesh, GhostType::NotGhost)?;
            e.init_shape_functions(mesh, GhostType::Ghost)?;
        }
        Ok(())
    }
}

impl MeshEventHandler for IgfemEventHandler<'_, '_> {
    fn event_priority(&self) -> EventPriority {
        EventPriority::MODEL
    }

    fn event_priorities(&self) -> Vec<EventPriority> {
        self.smm.event_priorities()
    }

    /// Forwarded to the solid model's parts like the dispatch:
    /// the check covers the FE engines and the DOF manager
    /// (which refuses, among others, a node removal preceded by a node
    /// addition in the same mutation, D-095 rule 1). The IGFEM additions
    /// validate a node removal's compaction of the `real_*` arrays here
    /// (applied at dispatch); the other IGFEM additions (the
    /// `real_*` arrays following added nodes, the element map, the
    /// transfer of internals, re-initialising the IGFEM engine) refuse
    /// nothing before doing the work.
    fn check_mesh_event(&self, mesh: &Mesh, event: &MeshEvent) -> Result<()> {
        self.smm.check_mesh_event(mesh, event)?;
        if let MeshEvent::NodesRemoved { new_numbering, .. } = event {
            self.real_compaction_plans(new_numbering)?;
        }
        Ok(())
    }

    fn on_mesh_event(&mut self, mesh: &Mesh, event: &MeshEvent) -> Result<()> {
        self.on_mesh_event_at(mesh, event, EventPriority::MODEL)
    }

    fn on_mesh_event_at(
        &mut self,
        mesh: &Mesh,
        event: &MeshEvent,
        level: EventPriority,
    ) -> Result<()> {
        self.before(mesh, event, level)?;
        self.smm.on_mesh_event_at(mesh, event, level)?;
        self.after(mesh, event, level)
    }

    fn on_mesh_event_phased(
        &mut self,
        mesh: &Mesh,
        event: &MeshEvent,
        level: EventPriority,
        phases: &mut Phases,
    ) -> Result<()> {
        self.before(mesh, event, level)?;
        self.smm.on_mesh_event_phased(mesh, event, level, phases)?;
        self.after(mesh, event, level)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use akantu_core::enums::AnalysisMethod;
    use akantu_core::{ElementType, GhostType, Idx};
    use akantu_mesh::{CommittedMutation, MeshAccessor, MeshPart, MeshTransaction};

    const DAT: &str = "material elastic [
  name = aggregate
  E = 10.
  nu = 0.3
]
material elastic [
  name = gel
  E = 1.
  nu = 0.25
]
material igfem_elastic [
  name = igfem
  name_sub_mat_1 = gel
  name_sub_mat_2 = aggregate
]
";

    /// Two triangles on the unit square (standard elements only; the input
    /// file still needs its `igfem_elastic` law), static.
    fn model() -> SolidMechanicsModelIgfem {
        let mut mesh = Mesh::new(2, "mesh").unwrap();
        {
            let mut a = MeshAccessor::new(&mut mesh);
            for x in [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]] {
                a.push_node(&x).unwrap();
            }
            let mut c = a.connectivity_mut(ElementType::Triangle3, GhostType::NotGhost);
            c.push_tuple(&[0, 1, 2]);
            c.push_tuple(&[0, 2, 3]);
        }
        mesh.compute_bounding_box().unwrap();
        let parser = Parser::parse_str(DAT, "material.dat").unwrap();
        let mut model =
            SolidMechanicsModelIgfem::new(SharedMesh::new(mesh), Some(2), DEFAULT_ID, parser)
                .unwrap();
        model
            .init_full(&ModelOptions::new(AnalysisMethod::Static))
            .unwrap();
        model
    }

    /// A mutation coordinated by the IGFEM model's event handler (as the
    /// growing-gel update does), with an arbitrary change.
    fn mutate_through_the_igfem_handler(
        model: &mut SolidMechanicsModelIgfem,
        change: impl FnOnce(&mut akantu_mesh::MeshTransaction<'_>) -> Result<()>,
    ) -> Result<CommittedMutation> {
        let mesh_h = model.smm.physics.base.mesh.clone();
        let dim = model.smm.spatial_dimension();
        let SolidMechanicsModelIgfem {
            smm,
            ctx,
            real_displacement,
            real_force,
            real_residual,
            element_map,
            ..
        } = model;
        let (token, parts) = smm.split_for_mutation();
        let mut handler = IgfemEventHandler::new(
            parts,
            ctx,
            [real_displacement, real_force, real_residual],
            element_map,
            dim,
        );
        mesh_h.mutate(&mut [(token, &mut handler)], change, None)
    }

    fn append_and_remove_a_node(tx: &mut MeshTransaction<'_>, nb_nodes: usize) -> Result<()> {
        let n = tx.push_node(MeshPart::Main, &[0.25, 0.25])?;
        tx.record(
            MeshPart::Main,
            MeshEvent::NodesAdded {
                nodes: vec![n],
                old_nodes: None,
                origin: "fu01".into(),
                extension: None,
            },
        );
        let mut numbering: Vec<Idx> = (0..nb_nodes).map(|i| Idx::try_from(i).unwrap()).collect();
        numbering.push(-1);
        tx.remove_nodes(MeshPart::Main, &numbering)
    }

    /// The IGFEM model's handler wraps the solid
    /// model's parts and forwards the capability check like it forwards the
    /// dispatch. A node appended and removed again in one transaction is
    /// refused **before** the commit (the DOF manager's check, D-095 rule
    /// 1: a removal after an addition): recoverable, the mesh and the model
    /// untouched, the domain usable afterwards.
    #[test]
    fn fu01_addition_then_removal_through_the_igfem_handler_is_refused_before_commit() {
        let mut model = model();
        let mesh = model.smm.physics.base.mesh.clone();
        let (nodes_before, nb_nodes, revision) = {
            let g = mesh.read().unwrap();
            (
                g.nodes().as_slice().to_vec(),
                g.nb_nodes(),
                mesh.revision().unwrap(),
            )
        };
        let u_before = model.smm.displacement().unwrap().as_slice().to_vec();
        let r = mutate_through_the_igfem_handler(&mut model, |tx| {
            append_and_remove_a_node(tx, nb_nodes)
        });
        assert!(r.as_ref().is_err_and(|e| e.fatal_kind().is_none()), "{r:?}");
        // nothing committed, nothing dispatched
        {
            let g = mesh.read().unwrap();
            assert_eq!(g.nodes().as_slice(), &nodes_before[..]);
        }
        assert_eq!(mesh.revision().unwrap(), revision);
        assert_eq!(model.smm.displacement().unwrap().as_slice(), &u_before[..]);
        assert_eq!(model.real_displacement().unwrap().len(), nb_nodes);
        // the domain stays usable through the same handler
        let again = mutate_through_the_igfem_handler(&mut model, |_| Ok(()));
        assert!(again.is_ok(), "{again:?}");
    }

    /// A node removal through the IGFEM handler (the
    /// first triangle erased, its orphaned node 1 removed by `purify` in a
    /// later mutation, nodes 2 and 3 moved down) compacts the `real_*`
    /// arrays with the removal's numbering (upstream
    /// `SolidMechanicsModelIGFEM::onNodesRemoved`), as the solid model
    /// compacts its own arrays.
    #[test]
    fn node_removal_through_the_igfem_handler_compacts_the_real_arrays() {
        let mut model = model();
        // distinct values per node in the real_* arrays
        for (k, a) in [
            &mut model.real_displacement,
            &mut model.real_force,
            &mut model.real_residual,
        ]
        .into_iter()
        .enumerate()
        {
            let a = a.as_mut().unwrap();
            for (i, v) in a.as_mut_slice().iter_mut().enumerate() {
                *v = (100 * k + i) as Real;
            }
        }
        mutate_through_the_igfem_handler(&mut model, |tx| {
            tx.erase_elements(
                MeshPart::Main,
                &[akantu_core::Element::new(
                    ElementType::Triangle3,
                    0,
                    GhostType::NotGhost,
                )],
            )
        })
        .unwrap();
        mutate_through_the_igfem_handler(&mut model, |tx| tx.purify(MeshPart::Main)).unwrap();
        let mesh = model.smm.physics.base.mesh.clone();
        let n = mesh.read().unwrap().nb_nodes();
        assert_eq!(n, 3);
        // purify numbers by first appearance in the remaining triangle
        // (0, 2, 3): 0 → 0, 1 removed, 2 → 1, 3 → 2
        for (k, a) in [
            model.real_displacement().unwrap(),
            model.real_force().unwrap(),
            model.real_residual().unwrap(),
        ]
        .into_iter()
        .enumerate()
        {
            assert_eq!(a.len(), 3);
            let expect: Vec<Real> = [0, 1, 4, 5, 6, 7]
                .iter()
                .map(|&i| (100 * k + i) as Real)
                .collect();
            assert_eq!(a.as_slice(), &expect[..]);
        }
        assert_eq!(model.smm.displacement().unwrap().len(), 3);
    }
}

#[cfg(all(test, feature = "cgal"))]
mod second_mutation_refusal;
