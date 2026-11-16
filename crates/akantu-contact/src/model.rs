// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/model/contact_mechanics/contact_mechanics_model.hh,
// contact_mechanics_model.cc, resolution.cc (assembleInternalForces,
// assembleLocalToGlobalArray, assembleStiffnessMatrix,
// assembleLocalToGlobalMatrix).

//! The contact mechanics model (CONT-001…005, CONT-018).
//!
//! Upstream `ContactMechanicsModel` is a `Model` that registers **no**
//! DOFs: it is always used through a coupler sharing the solid's DOF
//! manager. In the port it is a **physics part** (`docs/ARCHITECTURE.md`,
//! `Monolithic<P>`): it owns no `ModelSolver`; it implements
//! [`SolverCallback`] (residual = contact force on the `displacement`
//! family, `K` = contact stiffness) and exposes what the solid–contact
//! couplers call: [`ContactMechanicsModel::positions_mut`],
//! [`ContactMechanicsModel::search`],
//! [`ContactMechanicsModel::assemble_internal_forces`],
//! [`ContactMechanicsModel::internal_force`],
//! [`ContactMechanicsModel::assemble_stiffness_matrix`],
//! [`ContactMechanicsModel::before_solve_step`] /
//! [`ContactMechanicsModel::after_solve_step`],
//! [`ContactMechanicsModel::default_solver_id`],
//! [`ContactMechanicsModel::default_solver_options_for`] and
//! [`ContactMechanicsModel::on_cohesive_elements_added`].
//!
//! The model works on the main mesh of its `SharedMesh` or on its facet
//! mesh ([`MeshPart::Facets`], solid–cohesive–contact).
//!
//! Assembly (UD-002): by default contact forces are
//! **accumulated** (`+=`), deformable masters receive the weight of
//! [`DeformableMasterWeighting`] on force and stiffness alike, and the
//! stiffness goes through the DOF manager's equation numbering (symmetric
//! blocks added once; unsymmetric friction blocks — added only when
//! friction contributes — promote `K`).
//! [`ContactUpstreamCompat::assembly`] reproduces upstream (assignment,
//! ½ on forces only, node-indexed `add(i, j)`).
//!
//! **Serial only** (CONT-019 is a mission extension owned by the parallel
//! work): on a distributed mesh every entry point returns
//! `Error::Unsupported` (upstream silently misses cross-partition contacts).

use std::collections::BTreeMap;

use akantu_core::enums::{
    AnalysisMethod, ContactState, IntegrationSchemeType, MatrixType, ModelType,
    NonLinearSolverType, TimeStepSolverType,
};
use akantu_core::{
    Array, Element, ElementType, Error, GhostType, GlobalIdx, Id, Idx, Real, Result, SharedArray,
    try_to_usize,
};
use akantu_io::dumper::{DumpBuffer, Dumpable, FieldDescriptor, FieldResolver, FieldSource};
use akantu_mesh::{Mesh, MeshPart, SharedMesh};
use akantu_model::boundary_condition::{DirichletFunctor, apply_dirichlet};
use akantu_model::model::ModelBase;
use akantu_parser::{Parser, ParserSection, ParserType, SearchScope};
use akantu_solver::{DofManager, ModelSolverOptions, SolutionType, SolverCallback};

use crate::detector::{ContactDetector, SearchOutput};
use crate::eigen_ops::{dot, norm};
use crate::element::ContactElement;
use crate::fields::{ContactFields, ContactUpstreamCompat, DeformableMasterWeighting};
use crate::geometry::{
    MeshView, NormalOrientation, ProjectionMetric, ProjectionValidity, outside_direction_with_nodes,
};
use crate::resolution::{
    MAX_ELEMENT_DOFS, Resolution, ResolutionContext, ResolutionFactory, Scratch,
};
use crate::selector::SurfaceSelector;
use akantu_core::array_shared::ArrayRef;

/// Name of the FE engine on the elements of the model's dimension.
pub const FE_ENGINE: &str = "ContactMechanicsModel";
/// Name of the FE engine on the facets.
pub const FE_ENGINE_FACETS: &str = "ContactFacetsFEEngine";
/// The DOF family the contact force is assembled into.
pub const DISPLACEMENT: &str = "displacement";

/// Contact mechanics model.
#[derive(Debug)]
pub struct ContactMechanicsModel {
    /// Model base (mesh handle, id, dimension, method, FE engines, parser).
    pub base: ModelBase,
    part: MeshPart,
    detector: ContactDetector,
    resolutions: Vec<Box<dyn Resolution>>,
    resolution_names: BTreeMap<String, usize>,
    are_resolutions_instantiated: bool,
    /// Nodal fields.
    pub fields: ContactFields,
    contact_elements: Vec<ContactElement>,
    compat: ContactUpstreamCompat,
    /// Force/stiffness weight of deformable masters (corrected assembly).
    pub deformable_master_weighting: DeformableMasterWeighting,
    /// Resolution types available to this model.
    pub factory: ResolutionFactory,
    /// The model's dumpers (upstream: registered on the mesh, `dump.rs`).
    pub(crate) dumpable: Dumpable,
}

/// The detection mesh of `part` and its parent.
fn view_of<'a>(guard: &'a Mesh, part: MeshPart) -> Result<MeshView<'a>> {
    match part {
        MeshPart::Main => Ok(MeshView::new(guard)),
        MeshPart::Facets => {
            let f = guard
                .mesh_facets()
                .ok_or_else(|| Error::invalid("contact model on the facet mesh: no facet mesh"))?;
            Ok(MeshView::facets(f, guard))
        }
    }
}

impl ContactMechanicsModel {
    /// Upstream `ContactMechanicsModel(mesh, dim, id)` on the main mesh,
    /// with the input-file snapshot `parser` (upstream: the static parser;
    /// its first `contact_detector` section is mandatory).
    ///
    /// # Errors
    /// Invalid dimension, no `contact_detector` section (upstream
    /// dereferences an end iterator), FE engine precomputation errors.
    pub fn with_parser(
        mesh: SharedMesh,
        spatial_dimension: Option<usize>,
        id: &str,
        parser: Parser,
    ) -> Result<Self> {
        Self::on_part(mesh, MeshPart::Main, spatial_dimension, id, parser)
    }

    /// Contact model on a given part of the mesh domain (`Facets` for the
    /// solid–cohesive–contact coupler, upstream
    /// `ContactMechanicsModel(mesh.getMeshFacets(), …)`).
    pub fn on_part(
        mesh: SharedMesh,
        part: MeshPart,
        spatial_dimension: Option<usize>,
        id: &str,
        parser: Parser,
    ) -> Result<Self> {
        let mut base = ModelBase::new(
            mesh.clone(),
            ModelType::ContactMechanicsModel,
            spatial_dimension,
            id,
        )?;
        base.parser = parser;
        let dim = base.spatial_dimension;
        if !(2..=3).contains(&dim) {
            return Err(Error::Unsupported(format!(
                "contact mechanics in dimension {dim} (2 and 3 are supported)"
            )));
        }
        let guard = mesh.read()?;
        let view = view_of(&guard, part)?;
        base.fems.register(FE_ENGINE, view.mesh, dim)?;
        base.fems.register(FE_ENGINE_FACETS, view.mesh, dim - 1)?;
        let detector = ContactDetector::with_positions(
            dim,
            &view.mesh.nodes(),
            &base.parser,
            format!("{id}:contact_detector"),
        )?;
        drop(guard);
        let dumpable = crate::dump::new_dumpable(id, dim)?;
        Ok(Self {
            base,
            part,
            detector,
            resolutions: Vec::new(),
            resolution_names: BTreeMap::new(),
            are_resolutions_instantiated: false,
            fields: ContactFields::new(dim),
            contact_elements: Vec::new(),
            compat: ContactUpstreamCompat::default(),
            deformable_master_weighting: DeformableMasterWeighting::default(),
            factory: ResolutionFactory::default(),
            dumpable,
        })
    }

    /// Model id.
    #[must_use]
    pub fn id(&self) -> &str {
        &self.base.id
    }

    /// Spatial dimension.
    #[must_use]
    pub fn spatial_dimension(&self) -> usize {
        self.base.spatial_dimension
    }

    /// Mesh part the model works on.
    #[must_use]
    pub fn part(&self) -> MeshPart {
        self.part
    }

    /// Upstream-compatibility switches.
    #[must_use]
    pub fn upstream_compat(&self) -> ContactUpstreamCompat {
        self.compat
    }

    /// Set the upstream-compatibility switches (also the detector's
    /// projection metric, UD-363, normal orientation, UD-371, and
    /// projection validity, UD-372).
    pub fn set_upstream_compat(&mut self, compat: ContactUpstreamCompat) {
        self.compat = compat;
        self.detector.projection_metric = if compat.projection_metric {
            ProjectionMetric::Upstream
        } else {
            ProjectionMetric::Surface
        };
        self.detector.normal_orientation = if compat.normal_orientation {
            NormalOrientation::Reference
        } else {
            NormalOrientation::Current
        };
        self.detector.projection_validity = if compat.projection_validity {
            ProjectionValidity::UpstreamBox
        } else {
            ProjectionValidity::Element
        };
    }

    fn check_serial(&self, mesh: &Mesh) -> Result<()> {
        if mesh.is_distributed() && mesh.communicator().size() > 1 {
            return Err(Error::Unsupported(
                "contact mechanics on a distributed mesh (CONT-019: parallel contact is a mission \
                 extension not provided by the serial port)"
                    .into(),
            ));
        }
        Ok(())
    }

    // ------------------------------------------------------------ initFull

    /// Upstream `initFull(options)`: analysis method, shape functions of
    /// the FE engines (`initModel`), the default boundary engine, the nodal
    /// fields (`initSolver`) and — if an input file was parsed — the
    /// resolutions (`contact_resolution` sub-sections of the model section,
    /// then of the root; at least one is required).
    pub fn init_full(&mut self, method: AnalysisMethod) -> Result<()> {
        self.base.method = method;
        let mesh = self.base.mesh.clone();
        let guard = mesh.read()?;
        let view = view_of(&guard, self.part)?;
        self.check_serial(view.mesh)?;
        for name in [FE_ENGINE, FE_ENGINE_FACETS] {
            let fe = self.base.fems.get_mut(name)?;
            fe.init_shape_functions(view.mesh, GhostType::NotGhost)?;
            fe.init_shape_functions(view.mesh, GhostType::Ghost)?;
        }
        self.base.fems.init_boundary(view.mesh);
        self.fields.allocate(view.mesh.nb_nodes())?;
        drop(guard);
        if !self.base.parser.last_parsed_file.is_empty() {
            self.instantiate_resolutions()?;
        }
        Ok(())
    }

    /// Upstream `instantiateResolutions`.
    pub fn instantiate_resolutions(&mut self) -> Result<()> {
        let mut sections: Vec<ParserSection> = Vec::new();
        if let Some(s) = self.base.parser_section() {
            sections.extend(s.sub_sections_of(ParserType::ContactResolution).cloned());
        }
        sections.extend(
            self.base
                .parser
                .root
                .sub_sections_of(ParserType::ContactResolution)
                .cloned(),
        );
        for s in &sections {
            self.register_new_resolution_from_section(s)?;
        }
        if self.resolutions.is_empty() {
            return Err(Error::invalid(format!(
                "No contact resolutions where instantiated for the model{}",
                self.base.id
            )));
        }
        self.are_resolutions_instantiated = true;
        Ok(())
    }

    /// `true` once the resolutions were instantiated from the input file.
    #[must_use]
    pub fn are_resolutions_instantiated(&self) -> bool {
        self.are_resolutions_instantiated
    }

    /// Upstream `registerNewResolution(section)`: type = section name,
    /// option = section option, `name` parameter mandatory, then the section
    /// is parsed into the new resolution.
    pub fn register_new_resolution_from_section(&mut self, section: &ParserSection) -> Result<()> {
        let name = section
            .get_parameter("name", SearchScope::Current)
            .map_err(|_| {
                Error::invalid(format!(
                    "A contact resolution of type '{}' in the input file has been defined without a name!",
                    section.name
                ))
            })?
            .to_word()
            .to_string_lossy()
            .into_owned();
        let permissive = self.base.parser.is_permissive();
        let idx = self.register_new_resolution_index(&name, &section.name, &section.option)?;
        let r = &mut self.resolutions[idx];
        if let Err(e) = r.parse(section, permissive) {
            self.resolutions.pop();
            self.resolution_names.remove(&name);
            return Err(e);
        }
        Ok(())
    }

    fn register_new_resolution_index(&mut self, name: &str, ty: &str, opt: &str) -> Result<usize> {
        if self.resolution_names.contains_key(name) {
            return Err(Error::AlreadyExists {
                kind: "contact resolution",
                id: format!(
                    "{name} (A resolution with this name has already been registered. Please use unique names for resolutions)"
                ),
            });
        }
        let count = self.resolutions.len();
        let id = format!("{}:{count}:{ty}", self.base.id);
        let r = self
            .factory
            .allocate(ty, self.base.spatial_dimension, opt, &id)?;
        self.resolutions.push(r);
        self.resolution_names.insert(name.to_owned(), count);
        Ok(count)
    }

    /// Upstream public `registerNewResolution(name, type, option)`: an
    /// empty resolution of type `ty` named `name`.
    pub fn register_new_resolution(
        &mut self,
        name: &str,
        ty: &str,
        opt: &str,
    ) -> Result<&mut dyn Resolution> {
        let idx = self.register_new_resolution_index(name, ty, opt)?;
        Ok(self.resolutions[idx].as_mut())
    }

    /// The resolutions, in registration order.
    #[must_use]
    pub fn resolutions(&self) -> &[Box<dyn Resolution>] {
        &self.resolutions
    }

    /// A resolution by name.
    pub fn resolution_mut(&mut self, name: &str) -> Result<&mut dyn Resolution> {
        let i = *self
            .resolution_names
            .get(name)
            .ok_or_else(|| Error::not_found("contact resolution", name))?;
        Ok(self.resolutions[i].as_mut())
    }

    // ------------------------------------------------------------ solver ids

    /// Upstream `getDefaultSolverID(method)`: `explicit_lumped` /
    /// `explicit` / `static` / `implicit`; `_explicit_contact` and
    /// `_implicit_contact` give `("unknown", not_defined)`.
    #[must_use]
    pub fn default_solver_id(method: AnalysisMethod) -> (Id, TimeStepSolverType) {
        match method {
            AnalysisMethod::ExplicitLumpedMass => {
                ("explicit_lumped".into(), TimeStepSolverType::DynamicLumped)
            }
            AnalysisMethod::ExplicitConsistentMass => {
                ("explicit".into(), TimeStepSolverType::Dynamic)
            }
            AnalysisMethod::Static => ("static".into(), TimeStepSolverType::Static),
            AnalysisMethod::ImplicitDynamic => ("implicit".into(), TimeStepSolverType::Dynamic),
            _ => ("unknown".into(), TimeStepSolverType::NotDefined),
        }
    }

    /// Upstream `getDefaultSolverOptions(type)`: `_dynamic` and
    /// `_dynamic_lumped` → lumped non-linear solver, central difference on
    /// accelerations; `_static` → `newton_raphson_contact`, pseudo-time;
    /// other types are an error.
    pub fn default_solver_options_for(ty: TimeStepSolverType) -> Result<ModelSolverOptions> {
        let mut o = ModelSolverOptions::default();
        match ty {
            TimeStepSolverType::Dynamic | TimeStepSolverType::DynamicLumped => {
                o.non_linear_solver_type = Some(NonLinearSolverType::Lumped);
                o.integration_scheme_type.insert(
                    DISPLACEMENT.into(),
                    IntegrationSchemeType::CentralDifference,
                );
                o.solution_type
                    .insert(DISPLACEMENT.into(), SolutionType::Acceleration);
            }
            TimeStepSolverType::Static => {
                o.non_linear_solver_type = Some(NonLinearSolverType::NewtonRaphsonContact);
                o.integration_scheme_type
                    .insert(DISPLACEMENT.into(), IntegrationSchemeType::PseudoTime);
                o.solution_type
                    .insert(DISPLACEMENT.into(), SolutionType::NotDefined);
            }
            other => {
                return Err(Error::invalid(format!(
                    "{other} is not a valid time step solver type"
                )));
            }
        }
        Ok(o)
    }

    // ------------------------------------------------------------ accessors

    /// The contact detector.
    #[must_use]
    pub fn contact_detector(&self) -> &ContactDetector {
        &self.detector
    }

    /// The contact detector, mutable (surface selector, parameters).
    pub fn contact_detector_mut(&mut self) -> &mut ContactDetector {
        &mut self.detector
    }

    /// Set the surface selector (upstream
    /// `getContactDetector().setSurfaceSelector`). Its part must be the
    /// model's. On a mesh without consumers (a contact model alone on its
    /// mesh) the selector's surface is mirrored as the mesh group
    /// `contact_surface` of its part, as upstream's selector constructors
    /// create it ([`crate::selector`]); inside a coupler
    /// (the solid model is a consumer of the mesh) the coupler's
    /// `set_surface_selector` writes it. A contact model built **beside** a
    /// separately built model of the same mesh (e.g. a `SolidMechanicsModel`
    /// and a `ContactMechanicsModel` on one mesh without a coupler: the
    /// mesh has a consumer the contact model cannot pass to the group-only
    /// mutation) writes **no** group; a later lookup of `contact_surface`
    /// fails with "not found" (upstream's selector
    /// creates the group in every case). Set the selector through a coupler,
    /// or before the other model is built.
    ///
    /// # Errors
    /// A selector of another mesh part; the group-only mesh mutation
    /// (nothing changed then: the selector is not set).
    pub fn set_surface_selector(&mut self, selector: Box<dyn SurfaceSelector>) -> Result<()> {
        if selector.part() != self.part {
            return Err(Error::invalid(format!(
                "surface selector on {:?} for a contact model on {:?}",
                selector.part(),
                self.part
            )));
        }
        if self.base.mesh.consumer_names()?.is_empty() {
            self.mirror_surface(selector.contact_surface(), &mut [])?;
        }
        self.detector.set_surface_selector(selector);
        Ok(())
    }

    /// Write the surface selector's surface as the mesh group
    /// `contact_surface` of its part through a
    /// group-only `SharedMesh::mutate` (no mesh event, no topology change):
    /// `handlers` holds one handler per registered consumer of the mesh
    /// (none for a mesh without consumers; the couplers pass their solid
    /// model). Nothing is written without a selector.
    ///
    /// # Errors
    /// The mesh mutation's errors (missing consumers among `handlers`, a
    /// mesh being read, UD-125).
    pub fn write_contact_surface_group(
        &self,
        handlers: &mut [(
            &akantu_mesh::ConsumerToken,
            &mut dyn akantu_mesh::MeshEventHandler,
        )],
    ) -> Result<()> {
        match self.detector.surface_selector() {
            Some(s) => self.mirror_surface(s.contact_surface(), handlers),
            None => Ok(()),
        }
    }

    fn mirror_surface(
        &self,
        surface: &crate::selector::ContactSurface,
        handlers: &mut [(
            &akantu_mesh::ConsumerToken,
            &mut dyn akantu_mesh::MeshEventHandler,
        )],
    ) -> Result<()> {
        let part = self.part;
        let dimension = self.base.spatial_dimension.saturating_sub(1);
        self.base.mesh.mutate(
            handlers,
            |tx| {
                crate::selector::write_contact_surface_group(
                    tx.groups_mut(part)?,
                    surface,
                    dimension,
                )
            },
            None,
        )?;
        Ok(())
    }

    /// Current positions (upstream `getPositions`).
    #[must_use]
    pub fn positions(&self) -> &Array<Real> {
        self.detector.positions()
    }

    /// Mutable current positions (the coupler copies the solid's current
    /// positions here before each search).
    pub fn positions_mut(&mut self) -> &mut Array<Real> {
        self.detector.positions_mut()
    }

    /// Contact elements of the last search.
    #[must_use]
    pub fn contact_elements(&self) -> &[ContactElement] {
        &self.contact_elements
    }

    /// Contact force (upstream `getInternalForce`; the guard of the
    /// exportable array, see [`ContactFields`]).
    ///
    /// # Panics
    /// While the array is exported to a foreign runtime that is not in a
    /// native section (a protocol violation of the binding, as
    /// `SharedArray::borrow`).
    #[must_use]
    pub fn internal_force(&self) -> ArrayRef<'_, Real> {
        self.fields.internal_force.borrow()
    }

    /// Gaps (positive = penetration).
    ///
    /// # Panics
    /// As [`ContactMechanicsModel::internal_force`].
    #[must_use]
    pub fn gaps(&self) -> ArrayRef<'_, Real> {
        self.fields.gaps.borrow()
    }

    /// Normals.
    ///
    /// # Panics
    /// As [`ContactMechanicsModel::internal_force`].
    #[must_use]
    pub fn normals(&self) -> ArrayRef<'_, Real> {
        self.fields.normals.borrow()
    }

    /// Nodal areas.
    ///
    /// # Panics
    /// As [`ContactMechanicsModel::internal_force`].
    #[must_use]
    pub fn nodal_area(&self) -> ArrayRef<'_, Real> {
        self.fields.nodal_area.borrow()
    }

    /// Tangential tractions.
    #[must_use]
    pub fn tangential_tractions(&self) -> &Array<Real> {
        &self.fields.tangential_tractions
    }

    /// Contact states.
    #[must_use]
    pub fn contact_state(&self) -> &Array<ContactState> {
        &self.fields.contact_state
    }

    /// Upstream `getMatrixType`: `K` is symmetric.
    #[must_use]
    pub fn model_matrix_type(matrix_id: &str) -> MatrixType {
        if matrix_id == "K" {
            MatrixType::Symmetric
        } else {
            MatrixType::NotDefined
        }
    }

    /// Upstream `onElementsAdded` forwarded to the surface selector (the
    /// cohesive selectors add the facets of new cohesive elements; the
    /// solid–cohesive–contact coupler calls this after an insertion).
    pub fn on_cohesive_elements_added(&mut self, elements: &[Element]) -> Result<()> {
        let mesh = self.base.mesh.clone();
        let guard = mesh.read()?;
        let facets = guard
            .mesh_facets()
            .ok_or_else(|| Error::invalid("on_cohesive_elements_added: no facet mesh"))?;
        match self.detector.surface_selector_mut() {
            Some(s) => s.on_elements_added(facets, elements),
            None => Err(Error::invalid("ContactDetector: no surface selector set")),
        }
    }

    /// Dirichlet boundary condition on the model's own `displacement`
    /// (upstream `applyBC` through `initBC(displacement, increment,
    /// external_force)`). Upstream binds the **one-component**
    /// `blocked_dofs` to the `dim`-component displacement, so functors
    /// write flags out of bounds (**UD-367**); the port refuses unless the
    /// blocked array was given `dim` components by the caller.
    pub fn apply_dirichlet_bc(
        &mut self,
        functor: &dyn DirichletFunctor,
        group: &str,
    ) -> Result<()> {
        if self.fields.blocked_dofs.nb_component() != self.fields.displacement.nb_component() {
            return Err(Error::Unsupported(
                "Dirichlet boundary condition on the contact model: upstream binds a 1-component \
                 blocked_dofs to the dim-component displacement (UD-367)"
                    .into(),
            ));
        }
        let mesh = self.base.mesh.clone();
        let guard = mesh.read()?;
        let view = view_of(&guard, self.part)?;
        let g = view.mesh.groups().element_group(group)?;
        // the contact model's arrays are plain (not exportable): lend their
        // storage to the exportable-array interface without copying
        let mut u = SharedArray::new(std::mem::take(&mut self.fields.displacement));
        let mut b = SharedArray::new(std::mem::take(&mut self.fields.blocked_dofs));
        let r = apply_dirichlet(functor, view.mesh, g, &mut u, &mut b);
        self.fields.displacement = u.into_inner()?;
        self.fields.blocked_dofs = b.into_inner()?;
        r.map_err(|e| akantu_model::boundary_condition::group_error(group, &e))
    }

    // ------------------------------------------------------------ search

    /// Upstream `savePreviousState`: copies the current projections,
    /// tangents, tangential tractions and master elements into the
    /// `previous_*` history (the friction history of the next force
    /// evaluation). [`ContactMechanicsModel::search`] calls it first, as
    /// upstream.
    ///
    /// **Port extension:** public here (upstream's `savePreviousState` is
    /// protected) so that the static / implicit couplers can commit the
    /// history **once per step** (UD-374 corrected default: at the end of a
    /// converged solve step) and search with
    /// [`ContactMechanicsModel::search_keeping_history`] inside the Newton
    /// loop. Under those couplers the `previous_*` fields are the converged
    /// state of the last converged step, not the previous Newton iterate.
    ///
    /// # Errors
    /// A contact element whose slave node is out of range.
    pub fn save_previous_state(&mut self) -> Result<()> {
        let f = &mut self.fields;
        f.previous_projections = f.projections.clone();
        f.previous_projections.set_id("previous_projections");
        f.previous_tangents = f.tangents.clone();
        f.previous_tangents.set_id("previous_tangents");
        f.previous_tangential_tractions = f.tangential_tractions.clone();
        f.previous_tangential_tractions
            .set_id("previous_tangential_tractions");
        f.previous_master_elements.clear();
        f.previous_master_elements
            .try_resize(f.projections.len(), Element::NULL)?;
        f.previous_master_elements.fill(Element::NULL);
        for e in &self.contact_elements {
            *f.previous_master_elements
                .get_tuple_mut(try_to_usize(e.slave)?)?
                .first_mut()
                .ok_or_else(|| Error::invalid("previous_master_elements: no component"))? =
                e.master;
        }
        Ok(())
    }

    /// Upstream `search()`: save the previous state, clear the contact
    /// elements, resize and zero the per-search fields, detect, flip the
    /// gap sign (positive = penetration) and compute the nodal areas when
    /// there is contact.
    ///
    /// # Errors
    /// See [`ContactMechanicsModel::search_keeping_history`].
    pub fn search(&mut self) -> Result<()> {
        self.save_previous_state()?;
        self.search_keeping_history()
    }

    /// [`ContactMechanicsModel::search`] without the leading
    /// [`ContactMechanicsModel::save_previous_state`]: the `previous_*`
    /// history keeps its values. Its arrays are only **extended** to the
    /// current number of nodes when nodes were added since the history was
    /// saved (cohesive insertion), with empty rows (zero projections,
    /// tangents and tractions, `Element::NULL` master: no history, as for a
    /// first contact), so that no force evaluation reads past them.
    ///
    /// **Port extension** (no upstream counterpart; UD-374): used by the
    /// static / implicit couplers, which commit the history once per step.
    /// `search()` is exactly `save_previous_state()` followed by this
    /// function.
    ///
    /// # Errors
    /// No surface selector, a distributed mesh, shape mismatches, detection
    /// and nodal-area errors.
    pub fn search_keeping_history(&mut self) -> Result<()> {
        self.contact_elements.clear();
        let mesh = self.base.mesh.clone();
        let guard = mesh.read()?;
        let view = view_of(&guard, self.part)?;
        self.check_serial(view.mesh)?;
        let nb = view.mesh.nb_nodes();
        let f = &mut self.fields;
        // the exportable fields are refilled in place (views stay attached)
        for a in [
            &mut f.gaps,
            &mut f.normals,
            &mut f.nodal_area,
            &mut f.external_force,
        ] {
            let mut a = a.write()?;
            a.try_resize(nb, 0.0)?;
            a.fill(0.0);
        }
        for a in [
            &mut f.tangents,
            &mut f.projections,
            &mut f.tangential_tractions,
        ] {
            a.try_resize(nb, 0.0)?;
            a.fill(0.0);
        }
        f.contact_state.try_resize(nb, ContactState::NoContact)?;
        f.contact_state.fill(ContactState::NoContact);
        // UD-850 (B): upstream never resizes `blocked_dofs` after
        // a cohesive insertion, so its dumps then write fewer values than
        // points (invalid VTK); the port extends it with `false` rows (the
        // array is never read by a computation)
        if f.blocked_dofs.len() < nb {
            f.blocked_dofs.try_resize(nb, false)?;
        }
        // UD-374: rows for the nodes added since the
        // history was saved; existing rows are never touched
        for a in [
            &mut f.previous_projections,
            &mut f.previous_tangents,
            &mut f.previous_tangential_tractions,
        ] {
            if a.len() < nb {
                a.try_resize(nb, 0.0)?;
            }
        }
        if f.previous_master_elements.len() < nb {
            f.previous_master_elements.try_resize(nb, Element::NULL)?;
        }
        let mut out = SearchOutput {
            gaps: f.gaps.write()?,
            normals: f.normals.write()?,
            tangents: &mut f.tangents,
            projections: &mut f.projections,
        };
        self.detector
            .search(&view, &mut self.contact_elements, &mut out)?;
        drop(out);
        for g in self.fields.gaps.write()?.as_mut_slice() {
            *g *= -1.;
        }
        if !self.contact_elements.is_empty() {
            self.compute_nodal_areas_in(&view)?;
        }
        Ok(())
    }

    /// Upstream `computeNodalAreas()`.
    pub fn compute_nodal_areas(&mut self) -> Result<()> {
        let mesh = self.base.mesh.clone();
        let guard = mesh.read()?;
        let view = view_of(&guard, self.part)?;
        self.compute_nodal_areas_in(&view)
    }

    /// Nodal area = `‖Σ_facets ∫ Nᵀ n_out‖` over the contact surface, with
    /// the integration weights of the **current** positions and the normals
    /// of the mesh coordinates (upstream re-initialises the boundary FE
    /// engine on the positions; `computeNormalsOnIntegrationPoints` uses the
    /// mesh nodes).
    fn compute_nodal_areas_in(&mut self, view: &MeshView<'_>) -> Result<()> {
        let mesh = view.mesh;
        let dim = self.base.spatial_dimension;
        let nb = mesh.nb_nodes();
        let f = &mut self.fields;
        for a in [&mut f.nodal_area, &mut f.external_force] {
            let mut a = a.write()?;
            a.try_resize(nb, 0.0)?;
            a.fill(0.0);
        }
        self.base.fems.boundary(FE_ENGINE, mesh)?;
        let boundary_name = format!("{FE_ENGINE}:boundary");
        let fem = self.base.fems.get_mut(&boundary_name)?;
        let positions = self.detector.positions();
        fem.init_shape_functions_with_nodes(mesh, positions.view(), GhostType::NotGhost)?;
        fem.init_shape_functions_with_nodes(mesh, positions.view(), GhostType::Ghost)?;
        fem.compute_normals_on_integration_points(mesh, GhostType::NotGhost)?;
        fem.compute_normals_on_integration_points(mesh, GhostType::Ghost)?;
        let fem = self.base.fems.get(&boundary_name)?;
        let surface = self
            .detector
            .surface_selector()
            .ok_or_else(|| Error::invalid("ContactDetector: no surface selector set"))?
            .contact_surface();
        let ghost = GhostType::NotGhost;
        let mut dual_by_shapes = Array::new(1);
        let mut integrated = Array::new(1);
        for ty in surface.element_types(ghost) {
            if ty.spatial_dimension() != dim - 1 {
                continue;
            }
            let ids = surface.elements_of(ty, ghost);
            let nq = fem.nb_integration_points(ty, ghost)?;
            let nn = ty.nb_nodes_per_element();
            let normals = fem.normals_on_integration_points(ty, ghost)?;
            let mut dual = Array::try_zeros(ids.len() * nq, dim)?;
            // the reference nodes, read once (outside_direction's
            // barycentres take them per call)
            let mesh_nodes = view.mesh.nodes();
            let parent_nodes = view.parent.map(Mesh::nodes);
            for (pos, &el) in ids.iter().enumerate() {
                let mut dir = [0.0; 3];
                outside_direction_with_nodes(
                    view,
                    mesh_nodes.as_slice(),
                    parent_nodes.as_ref().map(|n| n.as_slice()),
                    Element::new(ty, el, ghost),
                    &mut dir,
                )?;
                let e = try_to_usize(el)?;
                for q in 0..nq {
                    let n = normals.get_tuple(e * nq + q)?;
                    let ddot = dot(&dir[..dim], &n[..dim]);
                    let out = dual.tuple_mut(pos * nq + q);
                    // Matrix<Real>::Identity(dim, dim) * normal: GEMV with a
                    // non-direct-access lhs, dest += n_k · I.col(k)
                    for (i, o) in out.iter_mut().enumerate() {
                        let mut v = 0.0;
                        for (k, &nk) in n.iter().enumerate().take(dim) {
                            v += nk * if i == k { 1.0 } else { 0.0 };
                        }
                        *o = v;
                    }
                    if ddot < 0.0 {
                        for o in out.iter_mut() {
                            *o *= -1.0;
                        }
                    }
                }
            }
            fem.compute_ntb(dual.view(), &mut dual_by_shapes, ty, ghost, Some(ids))?;
            fem.integrate(
                dual_by_shapes.view(),
                &mut integrated,
                dim * nn,
                ty,
                ghost,
                Some(ids),
            )?;
            // assembleElementalArrayLocalArray(…, scale = 1)
            let conn = mesh.connectivity(ty, ghost)?;
            let mut ext_g = self.fields.external_force.write()?;
            let ext = ext_g.as_mut_slice();
            for (pos, &el) in ids.iter().enumerate() {
                let nodes = conn.get_tuple(try_to_usize(el)?)?;
                let vals = integrated.tuple(pos);
                for (k, &node) in nodes.iter().enumerate() {
                    let u = try_to_usize(node)?;
                    for d in 0..dim {
                        ext[u * dim + d] += 1. * vals[k * dim + d];
                    }
                }
            }
        }
        let fields = &mut self.fields;
        {
            let mut area = fields.nodal_area.write()?;
            let ext = fields.external_force.read()?;
            for (a, fv) in area
                .as_mut_slice()
                .iter_mut()
                .zip(ext.as_slice().chunks_exact(dim))
            {
                *a = norm(fv);
            }
        }
        // upstream `external_force->clear()` (resize to 0: a view taken
        // before is retired, stale but safe, UD-085)
        fields.external_force.write()?.clear();
        Ok(())
    }

    // ------------------------------------------------------------ forces

    fn context<'a>(&'a mut self, view: &MeshView<'a>) -> ResolutionContext<'a> {
        ResolutionContext {
            mesh: view.mesh,
            positions: self.detector.positions(),
            dim: self.base.spatial_dimension,
            fields: &mut self.fields,
            compat: self.compat,
        }
    }

    /// Upstream `assembleInternalForces()`: zeroed `internal_force`,
    /// `normal_force`, `tangential_force`, then every resolution assembles
    /// its normal and tangential forces over the contact elements and sets
    /// `internal_force = tangential + normal`.
    pub fn assemble_internal_forces(&mut self) -> Result<()> {
        let mesh = self.base.mesh.clone();
        let guard = mesh.read()?;
        let view = view_of(&guard, self.part)?;
        self.check_serial(view.mesh)?;
        let nb = view.mesh.nb_nodes();
        // upstream `clear()` then `resize(nb_nodes, 0.)`: zeros, refilled in
        // place here (an exported view stays attached, as upstream's buffer
        // is reused)
        for a in [
            &mut self.fields.internal_force,
            &mut self.fields.normal_force,
            &mut self.fields.tangential_force,
        ] {
            let mut a = a.write()?;
            a.try_resize(nb, 0.0)?;
            a.fill(0.0);
        }
        let resolutions = std::mem::take(&mut self.resolutions);
        let elements = std::mem::take(&mut self.contact_elements);
        let compat = self.compat;
        let weighting = self.deformable_master_weighting;
        let result = (|| -> Result<()> {
            let mut ctx = self.context(&view);
            for r in &resolutions {
                assemble_resolution_forces(r.as_ref(), &mut ctx, &elements, compat, weighting)?;
            }
            Ok(())
        })();
        self.resolutions = resolutions;
        self.contact_elements = elements;
        result
    }

    // ------------------------------------------------------------ stiffness

    /// Upstream `assembleStiffnessMatrix()`: creates `K` (symmetric) if
    /// needed, then adds every resolution's normal and tangential moduli of
    /// every contact element.
    pub fn assemble_stiffness_matrix(&mut self, dofs: &mut DofManager) -> Result<()> {
        if !dofs.has_matrix("K") {
            dofs.new_matrix("K", Self::model_matrix_type("K"))?;
        }
        let mesh = self.base.mesh.clone();
        let guard = mesh.read()?;
        let view = view_of(&guard, self.part)?;
        self.check_serial(view.mesh)?;
        let resolutions = std::mem::take(&mut self.resolutions);
        let elements = std::mem::take(&mut self.contact_elements);
        let compat = self.compat;
        let weighting = self.deformable_master_weighting;
        let dim = self.base.spatial_dimension;
        let result = (|| -> Result<()> {
            let ctx = self.context(&view);
            let mut scratch = Scratch::new();
            let mut local = vec![0.0; MAX_ELEMENT_DOFS * MAX_ELEMENT_DOFS];
            let mut coupling_nodes = [0 as Idx; crate::geometry::MAX_FACET_NODES];
            let mut coupling = vec![0.0; MAX_ELEMENT_DOFS * dim * crate::geometry::MAX_FACET_NODES];
            for r in &resolutions {
                for el in &elements {
                    let nd = el.nb_nodes() * dim;
                    let k = &mut local[..nd * nd];
                    k.fill(0.0);
                    r.compute_normal_moduli(&ctx, el, k, &mut scratch)?;
                    let sym = normal_moduli_symmetric(r.as_ref(), compat);
                    assemble_local_matrix(
                        dofs,
                        view.mesh,
                        r.as_ref(),
                        el,
                        k,
                        dim,
                        compat,
                        weighting,
                        sym,
                    )?;
                    let k = &mut local[..nd * nd];
                    k.fill(0.0);
                    let n_coupled = r.compute_tangential_moduli_with_coupling(
                        &ctx,
                        el,
                        k,
                        &mut scratch,
                        &mut coupling_nodes,
                        &mut coupling,
                    )?;
                    // The consistent friction tangent's
                    // columns of the previous master facet's other nodes
                    // (deformable master; corrected assembly only)
                    if n_coupled > 0 && !compat.assembly && r.is_master_deformable() {
                        assemble_coupling_block(
                            dofs,
                            view.mesh,
                            el,
                            &coupling_nodes[..n_coupled],
                            &coupling[..nd * dim * n_coupled],
                            dim,
                            weighting,
                        )?;
                    }
                    let k = &mut local[..nd * nd];
                    // The corrected assembly adds the (unsymmetric)
                    // friction block only when friction contributes, so a
                    // frictionless K keeps symmetric storage; upstream adds
                    // the all-zero block (`add(i, j, 0)`, same entries as the
                    // normal block) and the compat assembly keeps doing so.
                    if !compat.assembly && k.iter().all(|&v| v == 0.0) {
                        continue;
                    }
                    assemble_local_matrix(
                        dofs,
                        view.mesh,
                        r.as_ref(),
                        el,
                        k,
                        dim,
                        compat,
                        weighting,
                        false,
                    )?;
                }
            }
            Ok(())
        })();
        self.resolutions = resolutions;
        self.contact_elements = elements;
        result
    }

    /// Local normal and tangential forces (`nd` values each, upstream
    /// `computeNormalForce` / `computeTangentialForce`) of `element` for the
    /// resolution `resolution` on the current fields (the tangential force
    /// updates the slave's tangential traction and state, as upstream).
    /// Diagnostic API (finite-difference checks, Python inspection).
    pub fn local_forces(
        &mut self,
        resolution: usize,
        element: &ContactElement,
    ) -> Result<(Vec<Real>, Vec<Real>)> {
        self.with_resolution_context(resolution, |r, ctx| {
            let nd = element.nb_nodes() * ctx.dim;
            let (mut fnv, mut ftv) = (vec![0.0; nd], vec![0.0; nd]);
            r.compute_normal_force(ctx, element, &mut fnv)?;
            r.compute_tangential_force(ctx, element, &mut ftv)?;
            Ok((fnv, ftv))
        })
    }

    /// Local normal and tangential moduli (`nd × nd`, column-major) of
    /// `element` for the resolution `resolution` (diagnostic API).
    pub fn local_moduli(
        &mut self,
        resolution: usize,
        element: &ContactElement,
    ) -> Result<(Vec<Real>, Vec<Real>)> {
        self.with_resolution_context(resolution, |r, ctx| {
            let nd = element.nb_nodes() * ctx.dim;
            let (mut kn, mut kt) = (vec![0.0; nd * nd], vec![0.0; nd * nd]);
            let mut scratch = Scratch::new();
            r.compute_normal_moduli(ctx, element, &mut kn, &mut scratch)?;
            r.compute_tangential_moduli(ctx, element, &mut kt, &mut scratch)?;
            Ok((kn, kt))
        })
    }

    fn with_resolution_context<R>(
        &mut self,
        resolution: usize,
        f: impl FnOnce(&dyn Resolution, &mut ResolutionContext<'_>) -> Result<R>,
    ) -> Result<R> {
        let mesh = self.base.mesh.clone();
        let guard = mesh.read()?;
        let view = view_of(&guard, self.part)?;
        let resolutions = std::mem::take(&mut self.resolutions);
        let result = match resolutions.get(resolution) {
            None => Err(Error::OutOfRange {
                what: "contact resolution",
                index: resolution,
                len: resolutions.len(),
            }),
            Some(r) => {
                let mut ctx = self.context(&view);
                f(r.as_ref(), &mut ctx)
            }
        };
        self.resolutions = resolutions;
        result
    }

    /// Resolutions' `beforeSolveStep`.
    pub fn before_solve_step(&mut self) -> Result<()> {
        for r in &mut self.resolutions {
            r.before_solve_step()?;
        }
        Ok(())
    }

    /// Resolutions' `afterSolveStep`.
    pub fn after_solve_step(&mut self, converged: bool) -> Result<()> {
        for r in &mut self.resolutions {
            r.after_solve_step(converged)?;
        }
        Ok(())
    }

    // ------------------------------------------------------------ dumps

    /// Real nodal dump fields (upstream `createNodalFieldReal`):
    /// `contact_force`, `normal_force`, `tangential_force`, `normals`,
    /// `tangents`, `gaps`, `areas`, `tangential_traction` (the model's
    /// dumper API: `dump.rs`).
    ///
    /// # Panics
    /// As [`ContactMechanicsModel::internal_force`].
    #[must_use]
    #[track_caller]
    pub fn nodal_field_real(&self, name: &str) -> Option<crate::dump::RealField<'_>> {
        self.fields.dump_real_borrowed(name)
    }

    /// Integer nodal dump field `contact_state` (enum → 0/1/2).
    #[must_use]
    pub fn nodal_field_int(&self, name: &str) -> Option<Vec<akantu_core::Int>> {
        self.fields.dump_int(name)
    }

    /// Boolean nodal dump field `blocked_dofs`.
    #[must_use]
    pub fn nodal_field_bool(&self, name: &str) -> Option<&Array<bool>> {
        self.fields.dump_bool(name)
    }
}

/// Whether the normal moduli of `r` are symmetric by construction (the
/// consistent tangent, UD-368; upstream's linear expression; upstream's
/// quadratic one only with the corrected UD-008 dyad).
fn normal_moduli_symmetric(r: &dyn Resolution, compat: ContactUpstreamCompat) -> bool {
    match r.resolution_type() {
        // consistent tangent: symmetric; upstream's linear expression too
        "penalty_linear" => true,
        "penalty_quadratic" => !compat.normal_moduli || !compat.quadratic_rotational_dyad,
        _ => false,
    }
}

/// Connectivity of a contact element: `[slave, master nodes…]`.
fn contact_connectivity(mesh: &Mesh, el: &ContactElement, out: &mut [Idx; 9]) -> Result<usize> {
    let conn = mesh.element_connectivity(el.master)?;
    if conn.len() + 1 > out.len() {
        return Err(Error::UnsupportedElementType {
            ty: el.master.ty(),
            context: "contact element connectivity",
        });
    }
    out[0] = el.slave;
    out[1..=conn.len()].copy_from_slice(&conn);
    Ok(conn.len() + 1)
}

/// Upstream `Resolution::assembleInternalForces()` for one resolution.
fn assemble_resolution_forces(
    r: &dyn Resolution,
    ctx: &mut ResolutionContext<'_>,
    elements: &[ContactElement],
    compat: ContactUpstreamCompat,
    weighting: DeformableMasterWeighting,
) -> Result<()> {
    let dim = ctx.dim;
    let mut fnv = [0.0; MAX_ELEMENT_DOFS];
    let mut ftv = [0.0; MAX_ELEMENT_DOFS];
    let mut conn = [0 as Idx; 9];
    for el in elements {
        let nd = el.nb_nodes() * dim;
        r.compute_normal_force(ctx, el, &mut fnv[..nd])?;
        r.compute_tangential_force(ctx, el, &mut ftv[..nd])?;
        let nbn = contact_connectivity(ctx.mesh, el, &mut conn)?;
        let deformable = r.is_master_deformable();
        let nb = if deformable { nbn } else { 1 };
        for (local, global) in [
            (&fnv, &mut ctx.fields.normal_force),
            (&ftv, &mut ctx.fields.tangential_force),
        ] {
            let mut global = global.write()?;
            for i in 0..nb {
                let g = global.get_tuple_mut(try_to_usize(conn[i])?)?;
                let l = &local[i * dim..(i + 1) * dim];
                if compat.assembly {
                    let alpha = if deformable { 0.5 } else { 1. };
                    for (gv, &lv) in g.iter_mut().zip(l) {
                        *gv = alpha * lv;
                    }
                } else {
                    let w = if deformable { weighting.weight() } else { 1. };
                    for (gv, &lv) in g.iter_mut().zip(l) {
                        *gv += w * lv;
                    }
                }
            }
        }
    }
    let f = &mut *ctx.fields;
    let mut fi = f.internal_force.write()?;
    let ft = f.tangential_force.read()?;
    let fnn = f.normal_force.read()?;
    for ((fc, ft), fnn) in fi
        .as_mut_slice()
        .iter_mut()
        .zip(ft.as_slice())
        .zip(fnn.as_slice())
    {
        *fc = ft + fnn;
    }
    Ok(())
}

/// Upstream `assembleLocalToGlobalMatrix` (compat) or the DOF-numbered,
/// weighted, symmetry-aware assembly (default).
#[allow(clippy::too_many_arguments)] // mirrors the upstream helper + switches
fn assemble_local_matrix(
    dofs: &mut DofManager,
    mesh: &Mesh,
    r: &dyn Resolution,
    el: &ContactElement,
    local: &[Real],
    dim: usize,
    compat: ContactUpstreamCompat,
    weighting: DeformableMasterWeighting,
    symmetric: bool,
) -> Result<()> {
    let mut conn = [0 as Idx; 9];
    let nbn = contact_connectivity(mesh, el, &mut conn)?;
    let nd = nbn * dim;
    let deformable = r.is_master_deformable();
    let nb = if deformable { nbn } else { 1 };
    let total = nb * dim;
    if compat.assembly {
        let mut eqs = [0 as GlobalIdx; MAX_ELEMENT_DOFS];
        for i in 0..nbn {
            for j in 0..dim {
                eqs[i * dim + j] = GlobalIdx::from(conn[i]) * dim as GlobalIdx + j as GlobalIdx;
            }
        }
        let k = dofs.matrix_mut("K")?;
        for i in 0..total {
            for j in 0..total {
                k.add(eqs[i], eqs[j], local[i + j * nd])?;
            }
        }
        return Ok(());
    }
    let w = if deformable { weighting.weight() } else { 1. };
    let leq = dofs.local_equation_numbers(DISPLACEMENT)?;
    let mut rows = [0 as GlobalIdx; MAX_ELEMENT_DOFS];
    for i in 0..nb {
        for j in 0..dim {
            let ldof = try_to_usize(conn[i])? * dim + j;
            let l = *leq.get(ldof).ok_or(Error::OutOfRange {
                what: "displacement DOF",
                index: ldof,
                len: leq.len(),
            })?;
            rows[i * dim + j] = dofs
                .local_to_global_equation_number(try_to_usize(l)?)
                .ok_or_else(|| Error::invalid("contact stiffness on a pure-ghost DOF"))?;
        }
    }
    let mut block_buf = [0.0; MAX_ELEMENT_DOFS * MAX_ELEMENT_DOFS];
    let block = &mut block_buf[..total * total];
    for j in 0..total {
        for i in 0..total {
            block[i + j * total] = w * local[i + j * nd];
        }
    }
    let ty = if symmetric {
        MatrixType::Symmetric
    } else {
        MatrixType::Unsymmetric
    };
    let backend = dofs.backend_mut()?;
    backend.add_block_to_profile("K", &rows[..total], &rows[..total])?;
    backend.add_values("K", &rows[..total], &rows[..total], block, ty)
}

/// Add the previous-facet coupling columns of a contact element's
/// consistent friction tangent: rows = the element's
/// DOFs (slave and master facet, deformable master), columns = the DOFs of
/// `nodes`; `block` holds one `nd × dim` column-major block per node; the
/// two-pass weight applies as to the element block. Unsymmetric.
///
/// Performance note (backlog): the function copies the whole
/// local-equation-number array (`leq.to_vec()`) and allocates two `Vec`s per
/// coupled contact element, i.e. `O(n_dof)` per such element and assembly;
/// it only runs for frictional contacts that changed facet on a deformable
/// master.
fn assemble_coupling_block(
    dofs: &mut DofManager,
    mesh: &Mesh,
    el: &ContactElement,
    nodes: &[Idx],
    block: &[Real],
    dim: usize,
    weighting: DeformableMasterWeighting,
) -> Result<()> {
    let mut conn = [0 as Idx; 9];
    let nbn = contact_connectivity(mesh, el, &mut conn)?;
    let nd = nbn * dim;
    let w = weighting.weight();
    let leq = dofs.local_equation_numbers(DISPLACEMENT)?.to_vec();
    let global = |node: Idx, j: usize| -> Result<GlobalIdx> {
        let ldof = try_to_usize(node)? * dim + j;
        let l = *leq.get(ldof).ok_or(Error::OutOfRange {
            what: "displacement DOF",
            index: ldof,
            len: leq.len(),
        })?;
        dofs.local_to_global_equation_number(try_to_usize(l)?)
            .ok_or_else(|| Error::invalid("contact stiffness on a pure-ghost DOF"))
    };
    let mut rows = [0 as GlobalIdx; MAX_ELEMENT_DOFS];
    for i in 0..nbn {
        for j in 0..dim {
            rows[i * dim + j] = global(conn[i], j)?;
        }
    }
    let nc = nodes.len() * dim;
    let mut cols = Vec::with_capacity(nc);
    for &node in nodes {
        for j in 0..dim {
            cols.push(global(node, j)?);
        }
    }
    let values: Vec<Real> = block[..nd * nc].iter().map(|v| w * v).collect();
    let backend = dofs.backend_mut()?;
    backend.add_block_to_profile("K", &rows[..nd], &cols)?;
    backend.add_values("K", &rows[..nd], &cols, &values, MatrixType::Unsymmetric)
}

impl SolverCallback for ContactMechanicsModel {
    fn matrix_type(&self, matrix_id: &str) -> MatrixType {
        Self::model_matrix_type(matrix_id)
    }

    fn assemble_matrix(&mut self, dofs: &mut DofManager, matrix_id: &str) -> Result<()> {
        if matrix_id == "K" {
            self.assemble_stiffness_matrix(dofs)?;
        }
        Ok(())
    }

    fn assemble_lumped_matrix(&mut self, _dofs: &mut DofManager, _matrix_id: &str) -> Result<()> {
        Err(Error::NotImplemented(
            "ContactMechanicsModel::assembleLumpedMatrix (AKANTU_TO_IMPLEMENT upstream)",
        ))
    }

    fn assemble_residual(&mut self, dofs: &mut DofManager) -> Result<()> {
        self.assemble_internal_forces()?;
        let f = self.fields.internal_force.read()?;
        dofs.assemble_to_residual(DISPLACEMENT, f.view(), 1.)
    }

    fn before_solve_step(&mut self, _dofs: &mut DofManager) -> Result<()> {
        ContactMechanicsModel::before_solve_step(self)
    }

    fn after_solve_step(&mut self, _dofs: &mut DofManager, converged: bool) -> Result<()> {
        ContactMechanicsModel::after_solve_step(self, converged)
    }

    fn init_solver(&mut self, _dofs: &mut DofManager, _ty: TimeStepSolverType) -> Result<()> {
        let mesh = self.base.mesh.clone();
        let guard = mesh.read()?;
        let view = view_of(&guard, self.part)?;
        self.fields.allocate(view.mesh.nb_nodes())
    }

    fn default_solver_options(&self, ty: TimeStepSolverType) -> ModelSolverOptions {
        Self::default_solver_options_for(ty).unwrap_or_default()
    }
}

impl FieldResolver for ContactMechanicsModel {
    fn resolve(
        &self,
        _mesh: &Mesh,
        field: &FieldDescriptor,
        ty: Option<ElementType>,
        _ghost: GhostType,
    ) -> Result<DumpBuffer> {
        let name = match (&field.source, ty) {
            (FieldSource::ModelNodal(n), None) => n.as_str(),
            (FieldSource::Blocked { .. }, None) => "blocked_dofs",
            _ => {
                return Err(Error::not_found(
                    "contact mechanics dump field",
                    field.name.clone(),
                ));
            }
        };
        if let Some(a) = self.nodal_field_real(name) {
            return Ok(DumpBuffer::Real {
                nb_component: a.nb_component(),
                values: a.as_slice().to_vec(),
            });
        }
        if let Some(v) = self.nodal_field_int(name) {
            return Ok(DumpBuffer::Int {
                nb_component: 1,
                values: v,
            });
        }
        if let Some(b) = self.nodal_field_bool(name) {
            return Ok(DumpBuffer::Bool {
                nb_component: b.nb_component(),
                values: b.as_slice().to_vec(),
            });
        }
        Err(Error::not_found("contact mechanics dump field", name))
    }
}
