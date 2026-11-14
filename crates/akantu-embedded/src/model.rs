// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/model/solid_mechanics/
// solid_mechanics_model_embedded_interface/embedded_interface_model.hh,
// embedded_interface_model.cc; src/model/model_options.hh
// (EmbeddedInterfaceModelOptions).

//! [`EmbeddedInterfaceModel`] (EMB-001) and
//! [`InterfaceMeshDataMaterialSelector`] (EMB-004).
//!
//! # Structure
//!
//! `EmbeddedInterfaceModel { smm, … }`: `smm` is the solid mechanics model
//! of the background mesh (upstream subclassing); bulk and reinforcement
//! laws share its law handler, numbering and factory, so the residual and
//! the stiffness are assembled in law order exactly as upstream's material
//! loop, and `material_by_name("reinforcement")`, energies and dumps see the
//! reinforcement laws. The reinforcement laws assemble on the background
//! nodes through the law hooks `assemble_internal_forces_custom` /
//! `elemental_stiffness_custom` of `akantu-solid` (no change to the solid
//! model). The interface mesh and its FE engine
//! (`"EmbeddedInterfaceFEEngine"`) are an immutable [`InterfaceData`] shared
//! with the laws; the engine is not in the solid model's registry, so the
//! background mesh's element events never reach it.
//!
//! # Initialisation (upstream `initFullImpl`)
//!
//! 1. with `has_intersections` (default), the interface mesh is computed
//!    ([`crate::EmbeddedInterfaceIntersector`]) and its FE engine
//!    precomputed (upstream: `constructData`, then `initModel` computes the
//!    interface shapes);
//! 2. the solid model's method/solver and law instantiation;
//! 3. upstream `assignMaterialToElements`: every interface segment goes to
//!    the law named by its `physical_names` (interface selector, EMB-004),
//!    then the bulk elements are assigned by `physical_names` of the
//!    background mesh (`MeshDataMaterialSelector<std::string>`, set by the
//!    constructor) and every law is initialised.
//!
//! # Limits
//!
//! Serial only (upstream ignores ghost types; on more than one rank the
//! model refuses with `Error::Unsupported`); the background mesh must not
//! change after `init_full` (upstream rebuilds neither the interface nor the
//! filters; the port checks the mesh revision before every solve).
//!
//! # Upstream defects (D-003)
//!
//! UD-073 (options macro), UD-587 (does not compile), UD-595 (interface
//! elements without a matching law fall back to law 0 upstream; refused by
//! default, [`EmbeddedUpstreamCompat`]), UD-596 (a non-reinforcement law
//! selected for an interface segment, or a reinforcement law for a bulk
//! element, is refused), UD-597 (dump-field errors swallowed upstream),
//! UD-598 (the reinforcement dumper flattens internals over the background
//! mesh upstream), UD-600 (a second `initFull` appends the intersections
//! again upstream; refused).

use std::sync::Arc;

use akantu_core::enums::{AnalysisMethod, ModelType};
use akantu_core::{
    Element, ElementKind, ElementType, Error, GhostType, Idx, Real, Result,
    SharedElementTypeMapArray, TypeFilter,
};
use akantu_fe::FeEngine;
use akantu_fe::shape::ShapeFamily;
use akantu_mesh::{Mesh, SharedMesh};
use akantu_model::ModelOptions;
use akantu_model::constitutive_law::{
    ConstitutiveLawSelector, ElementDataSelector, SelectorContext,
};
use akantu_parser::Parser;
use akantu_solid::{Material, SolidMechanicsModel};

use crate::dump::ReinforcementDumper;
use crate::intersector::{EmbeddedInterfaceIntersector, PHYSICAL_NAMES, empty_interface_mesh};
use crate::material::{INTERFACE_FE_ENGINE, InterfaceData, InterfaceSlot, is_reinforcement};

/// Default model id (upstream `"embedded_interface_model"`).
pub const DEFAULT_ID: &str = "embedded_interface_model";

/// Options of [`EmbeddedInterfaceModel::init_full`] (upstream
/// `EmbeddedInterfaceModelOptions`; named argument `_init_intersections`,
/// whose declaration is broken upstream, UD-073).
#[derive(Clone, Debug)]
pub struct EmbeddedInterfaceModelOptions {
    /// Solid-mechanics options (default method `_explicit_lumped_mass`).
    pub base: ModelOptions,
    /// Compute the intersections (upstream `has_intersections`, default
    /// `true`); without them the interface mesh stays empty.
    pub has_intersections: bool,
}

impl Default for EmbeddedInterfaceModelOptions {
    fn default() -> Self {
        Self {
            base: ModelOptions::default(),
            has_intersections: true,
        }
    }
}

impl EmbeddedInterfaceModelOptions {
    /// Options with the given analysis method (upstream
    /// `EmbeddedInterfaceModelOptions(method)`).
    #[must_use]
    pub fn new(analysis_method: AnalysisMethod) -> Self {
        Self {
            base: ModelOptions::new(analysis_method),
            has_intersections: true,
        }
    }
}

/// Upstream-compatibility switches of the embedded model (D-003 category B
/// defects; the default `false` is the corrected behaviour).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EmbeddedUpstreamCompat {
    /// **UD-595**: upstream's interface selector has no fallback selector,
    /// so a segment whose name matches no law gets law **0**
    /// (`ConstitutiveLawSelector` fallback value). Default: refused with an
    /// error naming the segment and its name. With the switch the segment
    /// goes to law 0 — which must still be a reinforcement law (UD-596).
    pub interface_selector_falls_back_to_first_law: bool,
    /// **UD-597**: upstream `addDumpGroupFieldToDumper` swallows every
    /// exception of the solid model's version for the other dumpers
    /// (`catch (...) {}`). Default: errors are returned.
    pub ignore_dump_field_errors: bool,
}

/// Upstream `InterfaceMeshDataMaterialSelector<T>` (EMB-004): an
/// `ElementDataMaterialSelector` on the element data `name` of the
/// **interface** mesh (`physical_names` by default): a law name for string
/// data, a law index for integer data; missing data or an unknown name
/// defer (`None`). The embedded model evaluates it with
/// [`SelectorContext::mesh`] = the interface mesh.
#[derive(Clone, Debug)]
pub struct InterfaceMeshDataMaterialSelector(ElementDataSelector);

impl InterfaceMeshDataMaterialSelector {
    /// Selector on the interface-mesh element data `name`.
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self(ElementDataSelector::new(name))
    }

    /// Name of the element data read.
    #[must_use]
    pub fn data_name(&self) -> &str {
        &self.0.name
    }
}

impl ConstitutiveLawSelector for InterfaceMeshDataMaterialSelector {
    fn select(&self, ctx: &SelectorContext<'_>, element: Element) -> Result<Option<usize>> {
        self.0.select(ctx, element)
    }
}

/// Lagrange FE engine of element dimension 1 on the interface mesh
/// (upstream `registerFEEngineObject<MyFEEngineType>(
/// "EmbeddedInterfaceFEEngine", *interface_mesh, 1)` + `initShapeFunctions`
/// of both ghost types in `initModel`).
fn interface_fe_engine(mesh: &Mesh, model_id: &str) -> Result<FeEngine> {
    FeEngine::with_precomputation(
        mesh,
        Some(1),
        ElementKind::Regular,
        ShapeFamily::Lagrange,
        format!("{model_id}:fem:{INTERFACE_FE_ENGINE}"),
    )
}

/// Solid mechanics model with embedded 1-D reinforcements (upstream
/// `EmbeddedInterfaceModel`, see the module documentation).
#[derive(Debug)]
pub struct EmbeddedInterfaceModel {
    /// The solid mechanics model of the background mesh (bulk and
    /// reinforcement laws).
    pub smm: SolidMechanicsModel,
    /// Intersector options (default: corrected LGPL ordering, D-042).
    pub intersector: EmbeddedInterfaceIntersector,
    /// Upstream-defect switches (default: corrected).
    pub compat: EmbeddedUpstreamCompat,
    primitive_mesh: Mesh,
    interface_selector: Box<dyn ConstitutiveLawSelector>,
    slot: InterfaceSlot,
    interface: Arc<InterfaceData>,
    dumper: ReinforcementDumper,
    background_revision: Option<u64>,
    interface_assigned: bool,
}

impl EmbeddedInterfaceModel {
    /// Upstream `EmbeddedInterfaceModel(mesh, primitive_mesh, dim, id)` with
    /// an empty parser (see [`EmbeddedInterfaceModel::with_parser`]).
    ///
    /// # Errors
    /// See [`EmbeddedInterfaceModel::with_parser`].
    pub fn new(
        mesh: SharedMesh,
        primitive_mesh: Mesh,
        spatial_dimension: Option<usize>,
        id: &str,
    ) -> Result<Self> {
        Self::with_parser(
            mesh,
            primitive_mesh,
            spatial_dimension,
            id,
            Parser::default(),
        )
    }

    /// Upstream constructor: the solid model of `mesh` (model type
    /// `embedded_model`, so a `model embedded_model [ … ]` section configures
    /// solvers and laws), bulk laws chosen by the background
    /// `physical_names`, an empty interface mesh with its FE engine, the
    /// `reinforcement` allocator in the model's factory, and the
    /// `reinforcement` dumper on the interface mesh. `primitive_mesh` holds
    /// the reinforcement `_segment_2` elements with `physical_names`.
    ///
    /// # Errors
    /// Solid-model construction errors, a distributed mesh
    /// (`Error::Unsupported`).
    pub fn with_parser(
        mesh: SharedMesh,
        primitive_mesh: Mesh,
        spatial_dimension: Option<usize>,
        id: &str,
        parser: Parser,
    ) -> Result<Self> {
        if mesh.read()?.communicator().size() > 1 {
            return Err(Error::Unsupported(
                "EmbeddedInterfaceModel on more than one rank (upstream ignores ghost types; \
                 the parallel embedded model is not implemented)"
                    .into(),
            ));
        }
        let mut smm = SolidMechanicsModel::with_parser_and_type(
            mesh,
            spatial_dimension,
            id,
            parser,
            ModelType::EmbeddedModel,
        )?;
        smm.set_material_selector(Box::new(ElementDataSelector::new(PHYSICAL_NAMES)));
        let slot = InterfaceSlot::default();
        crate::material::register(&mut smm.physics.factory, slot.clone())?;
        let dim = smm.spatial_dimension();
        let imesh = empty_interface_mesh(dim)?;
        let fe = interface_fe_engine(&imesh, id)?;
        let interface = Arc::new(InterfaceData { mesh: imesh, fe });
        slot.set(interface.clone())?;
        let dumper = ReinforcementDumper::new(id)?;
        Ok(Self {
            smm,
            intersector: EmbeddedInterfaceIntersector::default(),
            compat: EmbeddedUpstreamCompat::default(),
            primitive_mesh,
            interface_selector: Box::new(InterfaceMeshDataMaterialSelector::new(PHYSICAL_NAMES)),
            slot,
            interface,
            dumper,
            background_revision: None,
            interface_assigned: false,
        })
    }

    /// Upstream `initFull(options)` (module documentation).
    ///
    /// # Errors
    /// A second call, intersection errors, law errors (an interface segment
    /// without a reinforcement law, UD-595/596), solver errors.
    pub fn init_full(&mut self, options: &EmbeddedInterfaceModelOptions) -> Result<()> {
        if self.background_revision.is_some() {
            return Err(Error::invalid(
                "EmbeddedInterfaceModel::init_full called twice (upstream would append the \
                 intersections a second time)",
            ));
        }
        if options.has_intersections {
            let imesh = {
                let mesh_h = self.smm.physics.base.mesh.clone();
                let mesh = mesh_h.read()?;
                self.intersector
                    .construct_data(&mesh, &self.primitive_mesh)?
            };
            let fe = interface_fe_engine(&imesh, self.smm.id())?;
            let data = Arc::new(InterfaceData { mesh: imesh, fe });
            self.slot.set(data.clone())?;
            self.interface = data;
        }
        self.smm.init_method_and_solver(&options.base)?;
        if let Some(instantiated) = self.smm.instantiate_materials()? {
            let pre = instantiated.and_then(|()| self.assign_interface_materials());
            self.smm.init_materials_after(pre)?;
        }
        self.background_revision = Some(self.smm.physics.base.mesh.read()?.revision());
        Ok(())
    }

    /// Upstream `initFull(_analysis_method = method)`.
    ///
    /// # Errors
    /// See [`EmbeddedInterfaceModel::init_full`].
    pub fn init_full_with(&mut self, method: AnalysisMethod) -> Result<()> {
        self.init_full(&EmbeddedInterfaceModelOptions::new(method))
    }

    /// Upstream `initMaterials()` for laws registered without an input file
    /// (`register_new_material`): interface then bulk assignment and the
    /// initialisation of every law.
    ///
    /// # Errors
    /// See [`EmbeddedInterfaceModel::init_full`].
    pub fn init_materials(&mut self) -> Result<()> {
        let pre = self.assign_interface_materials();
        self.smm.init_materials_after(pre)?;
        if self.background_revision.is_none() {
            self.background_revision = Some(self.smm.physics.base.mesh.read()?.revision());
        }
        Ok(())
    }

    /// The interface part of upstream `assignMaterialToElements`: every
    /// not-ghost interface element (dimension 1, mesh order) is added to the
    /// law chosen by the interface selector. Selection first, then the
    /// additions (no partial assignment on error).
    fn assign_interface_materials(&mut self) -> Result<()> {
        if self.interface_assigned {
            return Err(Error::invalid(
                "the interface elements are already assigned to their laws (init_full or \
                 init_materials ran before; upstream would add them a second time)",
            ));
        }
        let iface = self.interface.clone();
        let materials = &mut self.smm.physics.materials;
        let elements: Vec<Element> = iface
            .mesh
            .elements(TypeFilter {
                spatial_dimension: Some(1),
                ghost_type: GhostType::NotGhost,
                kind: Some(ElementKind::Regular),
            })
            .collect();
        let no_index = SharedElementTypeMapArray::<Idx>::new("interface_law_index");
        let mut chosen = Vec::with_capacity(elements.len());
        {
            let by_name = |n: &str| materials.law_index_of(n).ok();
            let ctx = SelectorContext {
                mesh: &iface.mesh,
                law_index: &no_index,
                law_by_name: &by_name,
            };
            for &e in &elements {
                let i = match self.interface_selector.select(&ctx, e)? {
                    Some(i) => i,
                    None if self.compat.interface_selector_falls_back_to_first_law => 0,
                    None => {
                        let name = iface
                            .mesh
                            .data()
                            .strings(PHYSICAL_NAMES)
                            .ok()
                            .and_then(|m| m.element(e).ok())
                            .and_then(|v| v.first().cloned())
                            .unwrap_or_default();
                        return Err(Error::invalid(format!(
                            "interface element {e} (physical name '{name}') matches no \
                             reinforcement law (UD-595: upstream falls back to law 0)"
                        )));
                    }
                };
                let law = materials.law(i)?;
                if !is_reinforcement(law) {
                    return Err(Error::invalid(format!(
                        "interface element {e} selected law {i} ('{}', type '{}') which is not a \
                         reinforcement law (UD-596)",
                        law.base().name,
                        law.law_type()
                    )));
                }
                chosen.push(i);
            }
        }
        for law in materials.laws_mut() {
            if is_reinforcement(law.as_ref()) {
                law.base_mut()
                    .initialize_element_filter(&iface.mesh, ElementKind::Regular)?;
            }
        }
        for (&e, &i) in elements.iter().zip(&chosen) {
            materials.law_mut(i)?.add_element(e)?;
        }
        self.interface_assigned = true;
        Ok(())
    }

    /// Replace the interface selector (default:
    /// [`InterfaceMeshDataMaterialSelector`] on `physical_names`); it is
    /// evaluated with the interface mesh in its context.
    pub fn set_interface_material_selector(&mut self, selector: Box<dyn ConstitutiveLawSelector>) {
        self.interface_selector = selector;
    }

    fn check_background_unchanged(&self) -> Result<()> {
        if let Some(rev) = self.background_revision
            && self.smm.physics.base.mesh.read()?.revision() != rev
        {
            return Err(Error::Unsupported(
                "the background mesh changed after EmbeddedInterfaceModel::init_full (the \
                 interface mesh and the reinforcement filters are not rebuilt, as upstream)"
                    .into(),
            ));
        }
        Ok(())
    }

    /// Solve one step (upstream `solveStep(solver_id)`).
    ///
    /// # Errors
    /// A changed background mesh, solver errors.
    pub fn solve_step(&mut self, solver_id: Option<&str>) -> Result<()> {
        self.check_background_unchanged()?;
        self.smm.solve_step(solver_id)
    }

    /// Upstream `assembleInternalForces()`.
    ///
    /// # Errors
    /// A changed background mesh, assembly errors.
    pub fn assemble_internal_forces(&mut self) -> Result<()> {
        self.check_background_unchanged()?;
        self.smm.assemble_internal_forces()
    }

    /// Upstream `assembleStiffnessMatrix(need_to_reassemble)`.
    ///
    /// # Errors
    /// A changed background mesh, assembly errors.
    pub fn assemble_stiffness_matrix(&mut self, need_to_reassemble: bool) -> Result<()> {
        self.check_background_unchanged()?;
        self.smm.assemble_stiffness_matrix(need_to_reassemble)
    }

    /// Upstream `getEnergy(id)` (sum over every law, reinforcements
    /// included).
    ///
    /// # Errors
    /// Energy errors.
    pub fn energy(&mut self, energy_id: &str) -> Result<Real> {
        self.smm.energy(energy_id)
    }

    /// The interface data (mesh and FE engine; empty before `init_full` or
    /// without intersections).
    #[must_use]
    pub fn interface(&self) -> &Arc<InterfaceData> {
        &self.interface
    }

    /// Upstream `getInterfaceMesh()`.
    #[must_use]
    pub fn interface_mesh(&self) -> &Mesh {
        &self.interface.mesh
    }

    /// The reinforcement (primitive) mesh.
    #[must_use]
    pub fn primitive_mesh(&self) -> &Mesh {
        &self.primitive_mesh
    }

    /// Upstream `getInterfaceAssociatedElements(type, ghost)`: the
    /// background element of every interface element of `(ty, ghost)`.
    ///
    /// # Errors
    /// No such block.
    pub fn interface_associated_elements(
        &self,
        ty: ElementType,
        ghost: GhostType,
    ) -> Result<&[Element]> {
        Ok(self
            .interface
            .mesh
            .data()
            .element_map(crate::intersector::ASSOCIATED_ELEMENT)?
            .get(ty, ghost)?
            .as_slice())
    }

    /// Upstream `getFEEngine(name)`: `"EmbeddedInterfaceFEEngine"` is the
    /// interface engine, any other name the solid model's.
    ///
    /// # Errors
    /// Unknown engine.
    pub fn fe_engine(&self, name: &str) -> Result<&FeEngine> {
        if name == INTERFACE_FE_ENGINE {
            Ok(&self.interface.fe)
        } else {
            self.smm.physics.base.fems.get(name)
        }
    }

    /// The reinforcement dumper registry.
    #[must_use]
    pub fn reinforcement_dumper(&self) -> &ReinforcementDumper {
        &self.dumper
    }

    /// Mutable reinforcement dumper registry.
    pub fn reinforcement_dumper_mut(&mut self) -> &mut ReinforcementDumper {
        &mut self.dumper
    }

    /// Upstream `addDumpGroupFieldToDumper(dumper, field, group, kind,
    /// padding)` override: the `reinforcement` dumper gets 1-D elemental
    /// fields of the interface mesh; other dumpers the solid model's fields
    /// (errors returned unless [`EmbeddedUpstreamCompat::ignore_dump_field_errors`],
    /// UD-597).
    ///
    /// # Errors
    /// Unknown dumper or field.
    pub fn add_dump_group_field_to_dumper(
        &mut self,
        dumper: &str,
        name: &str,
        group: &str,
        padding: bool,
    ) -> Result<()> {
        if dumper == crate::dump::REINFORCEMENT_DUMPER {
            let materials = &self.smm.physics.materials;
            return self.dumper.add_field(name, group, padding, materials);
        }
        let r = self
            .smm
            .add_dump_group_field_to_dumper(dumper, name, group, padding);
        match r {
            Err(_) if self.compat.ignore_dump_field_errors => Ok(()),
            r => r,
        }
    }

    /// Upstream `addDumpFieldToDumper(dumper, field)`.
    ///
    /// # Errors
    /// See [`EmbeddedInterfaceModel::add_dump_group_field_to_dumper`].
    pub fn add_dump_field_to_dumper(&mut self, dumper: &str, name: &str) -> Result<()> {
        self.add_dump_group_field_to_dumper(dumper, name, "all", false)
    }

    /// Upstream `addDumpFieldVectorToDumper(dumper, field)`.
    ///
    /// # Errors
    /// See [`EmbeddedInterfaceModel::add_dump_group_field_to_dumper`].
    pub fn add_dump_field_vector_to_dumper(&mut self, dumper: &str, name: &str) -> Result<()> {
        self.add_dump_group_field_to_dumper(dumper, name, "all", true)
    }

    /// Upstream `addDumpFieldTensorToDumper(dumper, field)`.
    ///
    /// # Errors
    /// See [`EmbeddedInterfaceModel::add_dump_group_field_to_dumper`].
    pub fn add_dump_field_tensor_to_dumper(&mut self, dumper: &str, name: &str) -> Result<()> {
        self.add_dump_group_field_to_dumper(dumper, name, "all", true)
    }

    /// Upstream `setBaseNameToDumper(dumper, name)`.
    ///
    /// # Errors
    /// Unknown dumper.
    pub fn set_base_name_to_dumper(&mut self, dumper: &str, name: &str) -> Result<()> {
        if dumper == crate::dump::REINFORCEMENT_DUMPER {
            self.dumper.dumpable.set_base_name_to_dumper(dumper, name)
        } else {
            self.smm.set_base_name_to_dumper(dumper, name)
        }
    }

    /// Upstream `setDirectoryToDumper(dumper, directory)`.
    ///
    /// # Errors
    /// Unknown dumper.
    pub fn set_directory_to_dumper(&mut self, dumper: &str, dir: &std::path::Path) -> Result<()> {
        if dumper == crate::dump::REINFORCEMENT_DUMPER {
            self.dumper.dumpable.set_directory_to_dumper(dumper, dir)
        } else {
            self.smm.set_directory_to_dumper(dumper, dir)
        }
    }

    /// Upstream `dump()`: the default dumper (the solid model's).
    ///
    /// # Errors
    /// Dump errors.
    pub fn dump(&mut self, time: Option<Real>, step: Option<usize>) -> Result<()> {
        self.smm.dump(time, step)
    }

    /// Upstream `dump(dumper_name)`: `reinforcement` writes the interface
    /// mesh with its 1-D fields, any other name a dumper of the solid model.
    ///
    /// # Errors
    /// Unknown dumper, dump errors.
    pub fn dump_named(
        &mut self,
        dumper: &str,
        time: Option<Real>,
        step: Option<usize>,
    ) -> Result<()> {
        if dumper == crate::dump::REINFORCEMENT_DUMPER {
            let materials = &self.smm.physics.materials;
            self.dumper.dump(&self.interface, materials, time, step)
        } else {
            self.smm.dump_named(dumper, time, step)
        }
    }

    /// Number of laws whose filter holds interface segments.
    #[must_use]
    pub fn nb_reinforcement_materials(&self) -> usize {
        self.smm
            .physics
            .materials
            .laws()
            .iter()
            .filter(|l| is_reinforcement(l.as_ref()))
            .count()
    }

    /// The reinforcement law `name`.
    ///
    /// # Errors
    /// Unknown name or not a reinforcement law.
    pub fn reinforcement(&self, name: &str) -> Result<&dyn Material> {
        let law = self.smm.material_by_name(name)?;
        if is_reinforcement(law) {
            Ok(law)
        } else {
            Err(Error::invalid(format!(
                "law '{name}' is not a reinforcement"
            )))
        }
    }
}
