// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/model/model.hh, model.cc,
// model_inline_impl.hh, model_options.hh, src/model/common/model_solver.cc
// (getParserSection), src/mesh/mesh_periodic.cc
// (makeConsistentForPeriodicity; its distributed part through
// akantu_mesh::periodic_synchronizer).

//! The model base (upstream `Model`, MODEL-001…004, 007…011).
//!
//! Upstream `Model` is an abstract class mixing the model solver, the FE
//! engine registry, the dumper convenience API and the reference to the
//! static parser. In the port a physics model is `{ physics, solver, token }`
//! (D-021): the physics part embeds a [`ModelBase`] (mesh handle, id,
//! spatial dimension, analysis method, FE engines, parser snapshot, nodal
//! field helpers), implements [`akantu_solver::SolverCallback`], and the
//! [`Model`] trait ties both halves together. The pieces of upstream's
//! `initFull` that are common to every model are free functions over these
//! halves ([`init_new_solver`]).
//!
//! # Parser
//!
//! Upstream models hold a reference to the process-global static parser and
//! read it at construction (`initDOFManager`) and in `initFull`. Library code
//! of the port takes the parser explicitly (decision D-023): a model owns a
//! [`Parser`] snapshot ([`ModelBase::parser`], empty by default; the facade
//! `akantu::runtime` holds the global one).

use std::collections::BTreeMap;

use akantu_comm::phase::{Phases, Severity};
use akantu_core::array_shared::{ArrayMut, SharedArray};
use akantu_core::enums::{AnalysisMethod, ModelType, TimeStepSolverType};
use akantu_core::{Array, Element, ElementKind, Error, GhostType, Id, Idx, Real, Result};
use akantu_fe::FeEngine;
use akantu_fe::shape::ShapeFamily;
use akantu_mesh::{ConsumerToken, Mesh, MeshEventHandler, SharedMesh};
use akantu_parser::{Parser, ParserSection, ParserType, SearchScope};
use akantu_solver::{ModelSolver, ModelSolverOptions, SolverCallback};

/// Options of `init_full` (upstream `ModelOptions` = `SolidMechanicsModelOptions`
/// = `HeatTransferModelOptions` …; model-specific option structs embed it).
#[derive(Clone, Debug)]
pub struct ModelOptions {
    /// Analysis method (upstream named argument `_analysis_method`).
    pub analysis_method: AnalysisMethod,
    /// Solver options (upstream `_solver_options`): kinds left `None` are
    /// filled from the model defaults ([`init_new_solver`]).
    pub solver_options: ModelSolverOptions,
}

impl Default for ModelOptions {
    /// Upstream default: `_explicit_lumped_mass`, default solver options.
    fn default() -> Self {
        Self {
            analysis_method: AnalysisMethod::ExplicitLumpedMass,
            solver_options: ModelSolverOptions::default(),
        }
    }
}

impl ModelOptions {
    /// Options with the given analysis method and default solver options
    /// (upstream `initFull(_analysis_method = method)`).
    #[must_use]
    pub fn new(analysis_method: AnalysisMethod) -> Self {
        Self {
            analysis_method,
            solver_options: ModelSolverOptions::default(),
        }
    }

    /// Replace the solver options (upstream `_solver_options = …`).
    #[must_use]
    pub fn with_solver_options(mut self, solver_options: ModelSolverOptions) -> Self {
        self.solver_options = solver_options;
        self
    }
}

/// Behaviour common to all physics models.
///
/// Implementors keep their physics state and their [`ModelSolver`] in
/// separate fields (see `akantu-solver` docs), hold the mesh through a
/// [`SharedMesh`] handle and are **one** registered mesh consumer reacting
/// at several priority levels (FE engines 9, DOF manager 20, own fields and
/// laws 94, non-local manager 100) — topology changes reach them only
/// through `SharedMesh::mutate`.
pub trait Model: MeshEventHandler {
    /// Model kind.
    fn model_type(&self) -> ModelType;

    /// Mesh handle.
    fn mesh(&self) -> &SharedMesh;

    /// Registration of this model as a mesh consumer (checked with
    /// `SharedMesh::check_current` at the start of `solve_step`).
    fn mesh_token(&self) -> &ConsumerToken;

    /// Solver (DOF manager + time-step solvers).
    fn model_solver(&self) -> &ModelSolver;

    /// Mutable solver.
    fn model_solver_mut(&mut self) -> &mut ModelSolver;

    /// Full initialisation (upstream `initFull(options)`): reads the model's
    /// input-file sections, creates FE engines, DOFs, constitutive laws and
    /// the default solver for `options.analysis_method`.
    fn init_full(&mut self, options: &ModelOptions) -> Result<()>;

    /// Solve one step with the default (or named) solver (upstream
    /// `solveStep`).
    fn solve_step(&mut self, solver_id: Option<&str>) -> Result<()>;

    /// Stable time step for explicit schemes (upstream
    /// `getStableTimeStep`).
    fn stable_time_step(&mut self) -> Result<Real>;

    /// Dump registered fields (upstream `dump()`); the model is the
    /// `FieldResolver` of its dumpers.
    fn dump(&mut self, time: Option<Real>, step: Option<usize>) -> Result<()>;
}

/// Named FE engines of a model (upstream `Model::fems`, `default_fem`,
/// `registerFEEngineObject`, `getFEEngine`, `getFEEngineClassBoundary`,
/// MODEL-001/008).
#[derive(Debug, Default)]
pub struct FeEngineRegistry {
    model_id: Id,
    fems: BTreeMap<Id, FeEngine>,
    default_fem: Option<Id>,
    /// The executor of the engines (D-123), given to every engine,
    /// including those registered later.
    exec: akantu_core::exec::Exec,
}

impl FeEngineRegistry {
    /// Empty registry of the model `model_id`.
    #[must_use]
    pub fn new(model_id: impl Into<Id>) -> Self {
        Self {
            model_id: model_id.into(),
            fems: BTreeMap::new(),
            default_fem: None,
            exec: akantu_core::exec::Exec::serial(),
        }
    }

    /// Set the executor of every engine, present and future (D-123).
    pub fn set_exec(&mut self, exec: &akantu_core::exec::Exec) {
        for fe in self.fems.values_mut() {
            fe.set_exec(exec.clone());
        }
        self.exec = exec.clone();
    }

    /// The executor of the engines.
    #[must_use]
    pub fn exec(&self) -> &akantu_core::exec::Exec {
        &self.exec
    }

    /// Upstream `registerFEEngineObject<FEEngineTemplate<IntegratorGauss,
    /// ShapeLagrange>>(name, mesh, dim)` (`do_not_precompute = false`):
    /// a Lagrange engine on the regular elements of dimension
    /// `element_dimension`, id `<model id>:fem:<name>`, shapes precomputed for
    /// both ghost types. The first registered engine is the default.
    ///
    /// # Errors
    /// A name already registered (upstream `AKANTU_EXCEPTION`) or the
    /// precomputation error.
    pub fn register(&mut self, name: &str, mesh: &Mesh, element_dimension: usize) -> Result<()> {
        self.register_with(name, mesh, element_dimension, ElementKind::Regular)
    }

    /// [`FeEngineRegistry::register`] for an explicit element kind
    /// (cohesive models register a cohesive engine).
    pub fn register_with(
        &mut self,
        name: &str,
        mesh: &Mesh,
        element_dimension: usize,
        kind: ElementKind,
    ) -> Result<()> {
        if self.fems.contains_key(name) {
            return Err(Error::AlreadyExists {
                kind: "FEEngine object",
                id: name.into(),
            });
        }
        let mut fe = FeEngine::with_precomputation(
            mesh,
            Some(element_dimension),
            kind,
            ShapeFamily::Lagrange,
            format!("{}:fem:{name}", self.model_id),
        )?;
        fe.set_exec(self.exec.clone());
        if self.fems.is_empty() {
            self.default_fem = Some(name.into());
        }
        self.fems.insert(name.into(), fe);
        Ok(())
    }

    /// Upstream `unRegisterFEEngineObject(name)`: the default moves to the
    /// first remaining engine (lexicographic) when it was removed.
    pub fn unregister(&mut self, name: &str) -> Result<FeEngine> {
        let fe = self
            .fems
            .remove(name)
            .ok_or_else(|| Error::not_found("FEEngine object", name))?;
        if self.default_fem.as_deref() == Some(name) {
            self.default_fem = self.fems.keys().next().cloned();
        }
        Ok(fe)
    }

    fn resolve<'a>(&'a self, name: &'a str) -> Result<&'a str> {
        if name.is_empty() {
            self.default_fem
                .as_deref()
                .ok_or_else(|| Error::not_found("FEEngine", "(default: none registered)"))
        } else {
            Ok(name)
        }
    }

    /// Name of the default engine.
    #[must_use]
    pub fn default_name(&self) -> Option<&str> {
        self.default_fem.as_deref()
    }

    /// Upstream `getFEEngine(name)` (empty name = default).
    pub fn get(&self, name: &str) -> Result<&FeEngine> {
        let n = self.resolve(name)?;
        self.fems
            .get(n)
            .ok_or_else(|| Error::not_found("FEEngine", format!("{n} (not registered)")))
    }

    /// Upstream `Model::getNbIntegrationPoints(elements, fem_id)`
    /// (`model_inline_impl.hh:149-157`): the sum, over `elements`, of the
    /// engine's integration points per element of each element's (type,
    /// ghost type); an empty `fem_id` is the default engine (MODEL-010).
    /// An element whose type the engine has no
    /// rule for (e.g. `Element::NULL`) is an error.
    pub fn nb_integration_points(&self, elements: &[Element], fem_id: &str) -> Result<usize> {
        let fe = self.get(fem_id)?;
        let mut nb_quad: usize = 0;
        for el in elements {
            let n = fe.nb_integration_points(el.ty(), el.ghost_type())?;
            nb_quad = nb_quad
                .checked_add(n)
                .ok_or_else(|| Error::invalid("number of integration points overflows"))?;
        }
        Ok(nb_quad)
    }

    /// Mutable engine (mesh events, orders).
    pub fn get_mut(&mut self, name: &str) -> Result<&mut FeEngine> {
        let n = self.resolve(name)?.to_owned();
        self.fems
            .get_mut(&n)
            .ok_or_else(|| Error::not_found("FEEngine", format!("{n} (not registered)")))
    }

    /// Upstream `hasFEEngineBoundary(name)`.
    #[must_use]
    pub fn has_boundary(&self, name: &str) -> bool {
        match self.resolve(name) {
            Ok(n) => self.fems.contains_key(&format!("{n}:boundary")),
            Err(_) => false,
        }
    }

    /// Upstream `getFEEngineClassBoundary<FEEngineClass>(name)`: the engine
    /// `<name>:boundary` on the elements of dimension `element_dimension − 1`
    /// of the engine `name`, created on first use with normals computed at
    /// the integration points of both ghost types.
    pub fn boundary(&mut self, name: &str, mesh: &Mesh) -> Result<&FeEngine> {
        let n = self.resolve(name)?.to_owned();
        let boundary_name = format!("{n}:boundary");
        if !self.fems.contains_key(&boundary_name) {
            let parent = self.get(&n)?;
            let dim = parent.element_dimension();
            if dim == 0 {
                return Err(Error::invalid(format!(
                    "FEEngine {n} handles points: it has no boundary"
                )));
            }
            let kind = parent.kind();
            let mut fe = FeEngine::with_precomputation(
                mesh,
                Some(dim - 1),
                kind,
                ShapeFamily::Lagrange,
                format!("{}:fem:{boundary_name}", self.model_id),
            )?;
            fe.compute_normals_on_integration_points(mesh, GhostType::NotGhost)?;
            fe.compute_normals_on_integration_points(mesh, GhostType::Ghost)?;
            fe.set_exec(self.exec.clone());
            self.fems.insert(boundary_name.clone(), fe);
        }
        self.fems
            .get(&boundary_name)
            .ok_or_else(|| Error::not_found("FEEngine boundary", boundary_name))
    }

    /// Upstream `getFEEngineBoundary(name)` without creation (error when it
    /// was never created).
    pub fn existing_boundary(&self, name: &str) -> Result<&FeEngine> {
        let n = self.resolve(name)?;
        let b = format!("{n}:boundary");
        self.fems
            .get(&b)
            .ok_or_else(|| Error::not_found("FEEngine boundary", format!("{n} (not registered)")))
    }

    /// Upstream `initFEEngineBoundary()`: (re)initialise the shapes and the
    /// normals of the default boundary engine, creating it if needed;
    /// failures are swallowed as upstream (`catch (debug::Exception &)`).
    pub fn init_boundary(&mut self, mesh: &Mesh) {
        let _ignored = self.boundary("", mesh);
    }

    /// Registered engine names (lexicographic).
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.fems.keys().map(String::as_str)
    }

    /// Every engine, mutable (mesh events at level `FE_ENGINE`).
    pub fn iter_mut(&mut self) -> impl Iterator<Item = (&str, &mut FeEngine)> {
        self.fems.iter_mut().map(|(k, v)| (k.as_str(), v))
    }

    /// The capability check of every engine
    /// ([`MeshEventHandler::check_mesh_event`]), for a model that
    /// dispatches mesh events to its engines at level `FE_ENGINE` and must
    /// forward the check to them.
    ///
    /// # Errors
    /// The first engine's refusal (engine-name order).
    pub fn check_mesh_event(&self, mesh: &Mesh, event: &akantu_mesh::MeshEvent) -> Result<()> {
        for fe in self.fems.values() {
            fe.check_mesh_event(mesh, event)?;
        }
        Ok(())
    }
}

/// State common to every model (upstream `Model` members: `mesh`,
/// `spatial_dimension`, `id`, `method`, `fems`, `parser`, MODEL-001).
#[derive(Debug)]
pub struct ModelBase {
    /// Model id (upstream `id`, default per model, e.g.
    /// `solid_mechanics_model`).
    pub id: Id,
    /// Model kind.
    pub model_type: ModelType,
    /// Mesh handle.
    pub mesh: SharedMesh,
    /// Spatial dimension (upstream: the mesh dimension when `_all_dimensions`
    /// is given).
    pub spatial_dimension: usize,
    /// Analysis method of the last `initFull` / `initNewSolver` (upstream
    /// `method`, default `_explicit_lumped_mass`).
    pub method: AnalysisMethod,
    /// FE engines.
    pub fems: FeEngineRegistry,
    /// Input-file snapshot (upstream reference to the static parser).
    pub parser: Parser,
}

impl ModelBase {
    /// Upstream `Model(mesh, type, dim, id)`: `dim = None` takes the mesh
    /// dimension.
    ///
    /// # Errors
    /// A spatial dimension outside 1…3 or larger than the mesh's, or an
    /// unusable mesh domain.
    pub fn new(
        mesh: SharedMesh,
        model_type: ModelType,
        spatial_dimension: Option<usize>,
        id: impl Into<Id>,
    ) -> Result<Self> {
        let id = id.into();
        let mesh_dim = mesh.read()?.spatial_dimension();
        let dim = spatial_dimension.unwrap_or(mesh_dim);
        akantu_core::dim::check_dim(dim)?;
        if dim > mesh_dim {
            return Err(Error::invalid(format!(
                "model spatial dimension {dim} exceeds the mesh dimension {mesh_dim}"
            )));
        }
        Ok(Self {
            fems: FeEngineRegistry::new(id.clone()),
            id,
            model_type,
            mesh,
            spatial_dimension: dim,
            method: AnalysisMethod::ExplicitLumpedMass,
            parser: Parser::default(),
        })
    }

    /// Upstream `ModelSolver::getParserSection()`: the `model <type> [ … ]`
    /// sub-section of the parser root whose type is this model's type and
    /// whose `name` (default: the model id) is the model id; `None` when
    /// there is none.
    #[must_use]
    pub fn parser_section(&self) -> Option<&ParserSection> {
        self.parser
            .root
            .sub_sections_of(ParserType::Model)
            .find(|s| {
                let ty_ok = s.name.parse::<ModelType>().ok() == Some(self.model_type);
                let name = match s.get_parameter("name", SearchScope::Current) {
                    Ok(p) => p.to_word().to_string_lossy().into_owned(),
                    Err(_) => self.id.clone(),
                };
                ty_ok && name == self.id
            })
    }

    /// [`ModelBase::parser_section`] with upstream's error: the `model`
    /// sub-sections are visited in order and each one's type name is
    /// converted with `getOptionToType<ModelType>` (`model_solver.cc:58-74`,
    /// an exact `operator>>`), so a section whose name is not a model type
    /// met before the matching one is an error ("The value \<name> is not a
    /// valid ModelType valid values are …"), as upstream.
    /// [`ModelBase::parser_section`] skips such sections.
    pub fn try_parser_section(&self) -> Result<Option<&ParserSection>> {
        for s in self.parser.root.sub_sections_of(ParserType::Model) {
            let ty: ModelType = s.name.parse()?;
            let name = match s.get_parameter("name", SearchScope::Current) {
                Ok(p) => p.to_word().to_string_lossy().into_owned(),
                Err(_) => self.id.clone(),
            };
            if ty == self.model_type && name == self.id {
                return Ok(Some(s));
            }
        }
        Ok(None)
    }

    /// The section laws are instantiated from (upstream `initFullImpl` of
    /// the constitutive-laws handler: the model section, or the parser root
    /// when the file has none); `None` when no file was parsed (upstream
    /// `getLastParsedFile().empty()`).
    #[must_use]
    pub fn laws_section(&self) -> Option<&ParserSection> {
        if self.parser.last_parsed_file.is_empty() {
            return None;
        }
        Some(self.parser_section().unwrap_or(&self.parser.root))
    }

    /// [`ModelBase::laws_section`] with upstream's error for a `model`
    /// section whose name is not a model type
    /// (`constitutive_laws_handler.hh:102` calls `getParserSection`; see
    /// [`ModelBase::try_parser_section`]).
    pub fn try_laws_section(&self) -> Result<Option<&ParserSection>> {
        if self.parser.last_parsed_file.is_empty() {
            return Ok(None);
        }
        Ok(Some(
            self.try_parser_section()?.unwrap_or(&self.parser.root),
        ))
    }

    /// Upstream `Model::getNbIntegrationPoints(elements, fem_id)` on this
    /// model's FE engines (see [`FeEngineRegistry::nb_integration_points`]).
    pub fn nb_integration_points(&self, elements: &[Element], fem_id: &str) -> Result<usize> {
        self.fems.nb_integration_points(elements, fem_id)
    }

    /// `true` for the explicit analysis methods (upstream
    /// `isDefaultSolverExplicit`).
    #[must_use]
    pub fn is_default_solver_explicit(&self) -> bool {
        matches!(
            self.method,
            AnalysisMethod::ExplicitLumpedMass | AnalysisMethod::ExplicitConsistentMass
        )
    }
}

/// Upstream `allocNodalField(array, nb_component, name)`: allocate the array
/// once (`nb_nodes × nb_component`, filled with `T::default()`); an existing
/// array is kept untouched (MODEL-010).
pub fn alloc_nodal_field<T: Clone + Default>(
    slot: &mut Option<Array<T>>,
    nb_nodes: usize,
    nb_component: usize,
) -> Result<&mut Array<T>> {
    if slot.is_none() {
        let mut a = Array::new(nb_component);
        a.try_resize(nb_nodes, T::default())?;
        *slot = Some(a);
    }
    slot.as_mut()
        .ok_or_else(|| Error::invalid("nodal field allocation failed"))
}

/// [`alloc_nodal_field`] for an exportable nodal field (D-047): allocate
/// the slot once, then return its exclusive guard.
pub fn alloc_nodal_shared_field<T: Clone + Default>(
    slot: &mut Option<SharedArray<T>>,
    nb_nodes: usize,
    nb_component: usize,
) -> Result<ArrayMut<'_, T>> {
    if slot.is_none() {
        let mut a = Array::new(nb_component);
        a.try_resize(nb_nodes, T::default())?;
        *slot = Some(SharedArray::new(a));
    }
    slot.as_mut()
        .ok_or_else(|| Error::invalid("nodal field allocation failed"))?
        .write()
}

/// Upstream `Mesh::removeNodesFromArray(array, new_numbering)`
/// (`mesh_inline_impl.hh:126-145`) on a model's nodal array, in place:
/// row `i` moves to `new_numbering[i]`, rows with `-1` are dropped, the
/// array keeps the surviving rows (SMM-015, node removal). Checked: the
/// array must have one row per entry of the numbering, and the numbering
/// must be a dense injective map onto `0..kept`; on error the array is
/// unchanged.
///
/// # Errors
/// `DimensionMismatch` for an array of another length, `InvalidArgument`
/// for an invalid numbering.
pub fn compact_nodal_rows<T: Copy + Default>(
    a: &mut Array<T>,
    new_numbering: &[Idx],
) -> Result<()> {
    let plan = node_compaction_plan(a.len(), new_numbering)?;
    apply_node_compaction(a, &plan);
    Ok(())
}

/// The validated target row of every row (`None`: removed) and the kept
/// count, for [`compact_nodal_rows`] (callers that compact several arrays
/// validate once, then apply).
///
/// # Errors
/// See [`compact_nodal_rows`].
pub fn node_compaction_plan(
    nb_rows: usize,
    new_numbering: &[Idx],
) -> Result<(Vec<Option<usize>>, usize)> {
    if nb_rows != new_numbering.len() {
        return Err(Error::DimensionMismatch {
            context: "node removal: nodal array rows vs removal numbering",
            expected: new_numbering.len(),
            got: nb_rows,
        });
    }
    // the mesh's validator (one copy)
    akantu_mesh::removal::removal_numbering_map(new_numbering, "nodal arrays")
}

/// Apply a plan of [`node_compaction_plan`] to an array of the planned
/// length (the array shrinks or keeps its length).
pub fn apply_node_compaction<T: Copy + Default>(
    a: &mut Array<T>,
    plan: &(Vec<Option<usize>>, usize),
) {
    let nbc = a.nb_component();
    move_rows(a.as_mut_slice(), nbc, &plan.0);
    a.truncate(plan.1);
}

/// [`apply_node_compaction`] through the guard of an exportable array
/// (D-047).
pub fn apply_node_compaction_guard<T: Copy + Default>(
    a: &mut ArrayMut<'_, T>,
    plan: &(Vec<Option<usize>>, usize),
) {
    let nbc = a.nb_component();
    move_rows(a.as_mut_slice(), nbc, &plan.0);
    a.truncate(plan.1);
}

/// Row `i` of `data` (`nbc` values) moved to row `map[i]`.
fn move_rows<T: Copy>(data: &mut [T], nbc: usize, map: &[Option<usize>]) {
    let src: Vec<T> = data.to_vec();
    for (i, m) in map.iter().enumerate() {
        if let Some(j) = *m {
            data[j * nbc..(j + 1) * nbc].copy_from_slice(&src[i * nbc..(i + 1) * nbc]);
        }
    }
}

/// Upstream `Model::initNewSolver(method, solver_options)` (MODEL-004): the
/// default solver id and type of `method` (model specific, upstream virtual
/// `getDefaultSolverID`) is `default_id`; an unset time-step solver type is
/// taken from it. When no solver of that id exists, the model's default
/// options for the type (`getDefaultSolverOptions`) **override** the user's
/// integration schemes and solution types (`ModelSolverOptions::update`,
/// UD-009 category C: preserved), the solver is created
/// (`getNewSolver` → `init_solver`) and every default scheme missing on it
/// is set. The solver becomes the default solver. The caller updates the
/// model's `method` (upstream sets it here).
pub fn init_new_solver(
    solver: &mut ModelSolver,
    physics: &mut dyn SolverCallback,
    default_id: (Id, TimeStepSolverType),
    mut solver_options: ModelSolverOptions,
) -> Result<()> {
    let (solver_name, tss_type) = default_id;
    if solver_options
        .time_step_solver_type
        .is_none_or(|t| t == TimeStepSolverType::NotDefined)
    {
        solver_options.time_step_solver_type = Some(tss_type);
    }
    if !solver.has_solver(&solver_name) {
        let tss = solver_options
            .time_step_solver_type
            .unwrap_or(TimeStepSolverType::NotDefined);
        let options = physics.default_solver_options(tss);
        solver_options.update(&options);
        solver.get_new_solver(&solver_name, &solver_options, physics)?;
        for (dof_id, ty) in &options.integration_scheme_type {
            if !solver.has_integration_scheme(&solver_name, dof_id)? {
                let st = options
                    .solution_type
                    .get(dof_id)
                    .copied()
                    .unwrap_or(akantu_solver::SolutionType::NotDefined);
                solver.set_integration_scheme(&solver_name, dof_id, *ty, st, physics)?;
            }
        }
    }
    solver.set_default_solver(&solver_name)
}

/// Reduction applied by [`make_consistent_for_periodicity`] (upstream
/// `ReduceOperation`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReduceOperation {
    /// Copy the master value to its slaves (`_id`).
    Id,
    /// Sum slaves into the master, then copy back (`_sum`).
    Sum,
    /// Logical or (`_or`, boolean arrays).
    Or,
}

impl From<ReduceOperation> for akantu_core::enums::ReduceOperation {
    fn from(op: ReduceOperation) -> Self {
        match op {
            ReduceOperation::Id => Self::Id,
            ReduceOperation::Sum => Self::Sum,
            ReduceOperation::Or => Self::Or,
        }
    }
}

/// Upstream `Mesh::makeConsistentForPeriodicity(array, op)` for serial
/// periodic meshes (the periodic node synchronizer belongs to the mesh
/// crate): a
/// no-op on non-periodic meshes; otherwise `Sum`/`Or` first reduce every
/// slave into its master, then (all operations) the master's value is copied
/// to its slaves.
///
/// Reduction order: upstream accumulates the slaves of a master in the
/// iteration order of `Mesh::periodic_master_slave`, an
/// `std::unordered_multimap`, whose equivalent keys iterate in **reverse
/// insertion order** with the oracle's standard library (UD-654,
/// established black-box, D-030: `g++ 14.2.0-19`; 178 container patterns
/// including rehash and erase, and 67 adversarial oracle cases;
/// oracle cases `model/smm/periodic_static_3d_2` and
/// `fixtures/mesh_utils/periodic_order`). [`Mesh::periodic_slaves`] keeps
/// upstream's insertion order, so the slaves are reduced from the last to
/// the first ([`Mesh::periodic_slaves_upstream_order`]); with several slaves
/// per master (multi-periodic meshes) the order changes the sum's rounding.
///
/// On a periodic mesh distributed over several ranks the call is
/// collective and runs upstream's distributed algorithm
/// (`mesh_periodic.cc:445-490`, [`akantu_mesh::periodic_synchronizer`]:
/// local reduction, ghost → owner reduction and owner → ghost copy of the
/// masters through the periodic node synchronizer, then master → slaves);
/// `reduce` is then the operation's own (`Sum`: addition, `Or`: logical
/// or). This entry point opens its own collective scope: inside a running
/// collective operation (a solver callback, a model method wrapped in
/// `DofManager::collective`) use
/// [`make_consistent_for_periodicity_values_in`] with that operation's
/// [`Phases`] (an independent scope is refused there, ARCHITECTURE).
pub fn make_consistent_for_periodicity<T: PeriodicConsistent>(
    mesh: &Mesh,
    array: &mut Array<T>,
    op: ReduceOperation,
    reduce: impl Fn(T, T) -> T,
) -> Result<()> {
    let (len, nbc) = (array.len(), array.nb_component());
    make_consistent_for_periodicity_values(mesh, array.as_mut_slice(), len, nbc, op, reduce)
}

/// The value types of the nodal arrays `makeConsistentForPeriodicity` makes
/// consistent on a distributed periodic mesh (upstream instantiates it for
/// `Real` and `bool`).
pub trait PeriodicConsistent: Copy + sealed::Sealed {
    /// The distributed algorithm as phases of `phases`.
    ///
    /// # Errors
    /// See [`akantu_mesh::periodic_synchronizer::make_consistent_for_periodicity_real_distributed_in`].
    fn distributed_in(
        mesh: &Mesh,
        phases: &mut Phases,
        values: &mut [Self],
        nbc: usize,
        op: ReduceOperation,
    ) -> Result<()>;
}

mod sealed {
    pub trait Sealed {}
    impl Sealed for akantu_core::Real {}
    impl Sealed for bool {}
}

impl PeriodicConsistent for Real {
    fn distributed_in(
        mesh: &Mesh,
        phases: &mut Phases,
        values: &mut [Self],
        nbc: usize,
        op: ReduceOperation,
    ) -> Result<()> {
        akantu_mesh::periodic_synchronizer::make_consistent_for_periodicity_real_distributed_in(
            mesh,
            phases,
            values,
            nbc,
            op.into(),
        )
    }
}

impl PeriodicConsistent for bool {
    fn distributed_in(
        mesh: &Mesh,
        phases: &mut Phases,
        values: &mut [Self],
        nbc: usize,
        op: ReduceOperation,
    ) -> Result<()> {
        akantu_mesh::periodic_synchronizer::make_consistent_for_periodicity_bool_distributed_in(
            mesh,
            phases,
            values,
            nbc,
            op.into(),
        )
    }
}

/// Whether `makeConsistentForPeriodicity` on `mesh` communicates (a
/// periodic mesh distributed over more than one rank).
#[must_use]
pub fn periodicity_is_distributed(mesh: &Mesh) -> bool {
    mesh.is_periodic() && mesh.is_distributed() && mesh.communicator().size() > 1
}

/// [`make_consistent_for_periodicity`] on the values of a nodal array of
/// `len` tuples of `nbc` components (tuple-major), e.g. the guard of an
/// exportable array (`ArrayMut::as_mut_slice`). Own collective scope on a
/// distributed periodic mesh (see [`make_consistent_for_periodicity`]).
pub fn make_consistent_for_periodicity_values<T: PeriodicConsistent>(
    mesh: &Mesh,
    a: &mut [T],
    len: usize,
    nbc: usize,
    op: ReduceOperation,
    reduce: impl Fn(T, T) -> T,
) -> Result<()> {
    if periodicity_is_distributed(mesh) {
        // the local checks are agreed inside the scope: a wrong array on
        // one rank is refused on every rank, never a rank returning while
        // the others wait in the exchange
        let mut ph = Phases::new(mesh.communicator(), format!("periodic:{}", mesh.id()));
        let check = check_periodic_len(mesh, len).and_then(|()| {
            akantu_mesh::periodic_synchronizer::check_distributed_periodic_values(
                mesh,
                a.len(),
                nbc,
            )
        });
        let r = ph
            .agree("periodic:prepare", check, Severity::Recoverable)
            .and_then(|()| T::distributed_in(mesh, &mut ph, a, nbc, op));
        return ph.settle(r);
    }
    make_consistent_for_periodicity_serial(mesh, a, len, nbc, op, reduce)
}

/// [`make_consistent_for_periodicity_values`] inside a running collective
/// operation: on a distributed periodic mesh the exchanges are phases of
/// `phases` (the operation's scope, e.g. `&mut *dofs.phases()?`); `phases`
/// is not used otherwise (pass `None` when the mesh does not communicate,
/// [`periodicity_is_distributed`]).
///
/// # Errors
/// As [`make_consistent_for_periodicity_values`]; on a distributed periodic
/// mesh without `phases`, [`Error::Communication`].
pub fn make_consistent_for_periodicity_values_in<T: PeriodicConsistent>(
    mesh: &Mesh,
    phases: Option<&mut Phases>,
    a: &mut [T],
    len: usize,
    nbc: usize,
    op: ReduceOperation,
    reduce: impl Fn(T, T) -> T,
) -> Result<()> {
    if periodicity_is_distributed(mesh) {
        check_periodic_len(mesh, len)?;
        let ph = phases.ok_or_else(|| {
            Error::Communication(
                "makeConsistentForPeriodicity on a distributed periodic mesh inside a collective \
                 operation needs the operation's scope"
                    .into(),
            )
        })?;
        return T::distributed_in(mesh, ph, a, nbc, op);
    }
    make_consistent_for_periodicity_serial(mesh, a, len, nbc, op, reduce)
}

fn check_periodic_len(mesh: &Mesh, len: usize) -> Result<()> {
    if len != mesh.nb_nodes() {
        return Err(Error::DimensionMismatch {
            context: "makeConsistentForPeriodicity: nodal array",
            expected: mesh.nb_nodes(),
            got: len,
        });
    }
    Ok(())
}

/// The serial algorithm (one rank, or a mesh that is not distributed).
fn make_consistent_for_periodicity_serial<T: Copy>(
    mesh: &Mesh,
    a: &mut [T],
    len: usize,
    nbc: usize,
    op: ReduceOperation,
    reduce: impl Fn(T, T) -> T,
) -> Result<()> {
    if !mesh.is_periodic() {
        return Ok(());
    }
    check_periodic_len(mesh, len)?;
    for master in 0..mesh.nb_nodes() {
        let m = akantu_core::try_idx(master)?;
        let slaves = mesh.periodic_slaves(m);
        if slaves.is_empty() {
            continue;
        }
        if op != ReduceOperation::Id {
            for s in mesh.periodic_slaves_upstream_order(m) {
                let s = akantu_core::try_to_usize(s)?;
                for c in 0..nbc {
                    a[master * nbc + c] = reduce(a[master * nbc + c], a[s * nbc + c]);
                }
            }
        }
        for &s in slaves {
            let s = akantu_core::try_to_usize(s)?;
            for c in 0..nbc {
                a[s * nbc + c] = a[master * nbc + c];
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use akantu_comm::Communicator;
    use akantu_core::ElementType;
    use akantu_mesh::MeshAccessor;
    use akantu_solver::DofManager;

    fn square() -> SharedMesh {
        let mut mesh = Mesh::new(2, "square").unwrap();
        {
            let mut acc = MeshAccessor::new(&mut mesh);
            for p in [[0., 0.], [1., 0.], [1., 1.], [0., 1.]] {
                acc.push_node(&p).unwrap();
            }
            let mut c = acc.connectivity_mut(ElementType::Triangle3, GhostType::NotGhost);
            c.push_tuple(&[0, 1, 2]);
            c.push_tuple(&[0, 2, 3]);
            drop(c);
            let mut s = acc.connectivity_mut(ElementType::Segment2, GhostType::NotGhost);
            s.push_tuple(&[0, 1]);
        }
        SharedMesh::new(mesh)
    }

    #[test]
    fn default_options() {
        assert_eq!(
            ModelOptions::default().analysis_method,
            AnalysisMethod::ExplicitLumpedMass
        );
    }

    #[test]
    fn base_dimension_and_parser_section() {
        let mesh = square();
        assert!(
            ModelBase::new(mesh.clone(), ModelType::SolidMechanicsModel, Some(3), "m").is_err()
        );
        let mut b = ModelBase::new(mesh, ModelType::SolidMechanicsModel, None, "smm").unwrap();
        assert_eq!(b.spatial_dimension, 2);
        assert!(b.laws_section().is_none());
        b.parser = Parser::parse_str(
            "model solid_mechanics_model [\n name = other\n a = 1\n]\nmodel solid_mechanics_model [\n name = smm\n b = 2\n]\n",
            "x.dat",
        )
        .unwrap();
        let s = b.parser_section().unwrap();
        assert!(s.parameter("b").is_some());
        assert!(
            b.try_parser_section()
                .unwrap()
                .unwrap()
                .parameter("b")
                .is_some()
        );
        b.id = "absent".into();
        assert!(b.parser_section().is_none());
        assert!(b.try_parser_section().unwrap().is_none());
        // no model section: laws come from the root
        assert!(b.laws_section().unwrap().parameter("b").is_none());
    }

    /// Upstream `getParserSection` converts every visited section name with
    /// `getOptionToType<ModelType>`: a name that is not a model type, met
    /// before the matching section, throws.
    #[test]
    fn invalid_model_type_name_is_an_error_as_upstream() {
        let mesh = square();
        let mut b = ModelBase::new(mesh, ModelType::SolidMechanicsModel, None, "smm").unwrap();
        b.parser = Parser::parse_str(
            "model solid_mechanic_model [\n b = 1\n]\nmodel solid_mechanics_model [\n b = 2\n]\n",
            "typo.dat",
        )
        .unwrap();
        b.parser.last_parsed_file = "typo.dat".into();
        let Err(Error::InvalidArgument(m)) = b.try_parser_section() else {
            panic!("a model section with an invalid type name must be an error");
        };
        assert!(
            m.starts_with(
                "The value solid_mechanic_model is not a valid ModelType valid values are \"model\", "
            ),
            "{m}"
        );
        assert!(b.try_laws_section().is_err());
        // the unchecked lookup skips it
        assert!(b.parser_section().unwrap().parameter("b").is_some());
        // after the matching section, an invalid name is never visited
        b.parser = Parser::parse_str(
            "model solid_mechanics_model [\n b = 2\n]\nmodel nonsense [\n]\n",
            "late.dat",
        )
        .unwrap();
        assert!(b.try_parser_section().unwrap().is_some());
        // a `_`-prefixed class-enum name is not a model type (exact parsing)
        b.parser = Parser::parse_str("model _solid_mechanics_model [\n]\n", "u.dat").unwrap();
        assert!(b.try_parser_section().is_err());
    }

    #[test]
    fn fe_registry_default_and_boundary() {
        let mesh = square();
        let m = mesh.read().unwrap();
        let mut r = FeEngineRegistry::new("smm");
        r.register("A", &m, 2).unwrap();
        assert!(r.register("A", &m, 2).is_err());
        assert_eq!(r.default_name(), Some("A"));
        assert_eq!(r.get("").unwrap().id(), "smm:fem:A");
        assert!(!r.has_boundary(""));
        assert!(r.existing_boundary("").is_err());
        let b = r.boundary("", &m).unwrap();
        assert_eq!(b.element_dimension(), 1);
        assert_eq!(
            b.normals_on_integration_points(ElementType::Segment2, GhostType::NotGhost)
                .unwrap()
                .len(),
            1
        );
        assert!(r.has_boundary("A"));
        r.unregister("A").unwrap();
        assert_eq!(r.default_name(), Some("A:boundary"));
    }

    /// Upstream `Model::getNbIntegrationPoints(elements, fem_id)`: sum of
    /// the per-element counts of each element's (type, ghost type) on the
    /// named (or default) engine.
    #[test]
    fn nb_integration_points_of_an_element_list() {
        let mesh = square();
        let mut b =
            ModelBase::new(mesh.clone(), ModelType::SolidMechanicsModel, None, "smm").unwrap();
        {
            let m = mesh.read().unwrap();
            b.fems.register("A", &m, 2).unwrap();
            b.fems.register("B", &m, 1).unwrap();
        }
        let tri = |e| Element::new(ElementType::Triangle3, e, GhostType::NotGhost);
        let seg = Element::new(ElementType::Segment2, 0, GhostType::NotGhost);
        let fe = b.fems.get("A").unwrap();
        let per_tri = fe
            .nb_integration_points(ElementType::Triangle3, GhostType::NotGhost)
            .unwrap();
        // default rule of _triangle_3 (order 1): one point
        assert_eq!(per_tri, 1);
        assert_eq!(b.nb_integration_points(&[], "").unwrap(), 0);
        assert_eq!(
            b.nb_integration_points(&[tri(0), tri(1), tri(0)], "")
                .unwrap(),
            3 * per_tri
        );
        assert_eq!(b.nb_integration_points(&[tri(1)], "A").unwrap(), per_tri);
        // engine B (elements of dimension 1): _segment_2, one point
        let per_seg = b
            .fems
            .get("B")
            .unwrap()
            .nb_integration_points(ElementType::Segment2, GhostType::NotGhost)
            .unwrap();
        assert_eq!(
            b.nb_integration_points(&[seg, seg], "B").unwrap(),
            2 * per_seg
        );
        // ghost elements use the (type, ghost) rule of the engine
        let ghost = Element::new(ElementType::Triangle3, 0, GhostType::Ghost);
        assert_eq!(
            b.nb_integration_points(&[ghost], "A").unwrap(),
            fe.nb_integration_points(ElementType::Triangle3, GhostType::Ghost)
                .unwrap()
        );
        assert!(b.nb_integration_points(&[tri(0)], "absent").is_err());
        assert!(b.nb_integration_points(&[Element::NULL], "").is_err());
    }

    /// MODEL-010: the element-list count is a count of
    /// integration **points**, not of elements: `_quadrangle_4` has the
    /// 2 × 2 Gauss rule (4 points) and `_triangle_6` the 3-point rule of
    /// order 2 (upstream `GaussIntegrationNbPoints<_git_segment, 2>` squared and
    /// `<_git_triangle, 2> = 3`), so a mixed list adds the per-type counts.
    #[test]
    fn nb_integration_points_of_an_element_list_multi_point_rules() {
        let mut mesh = Mesh::new(2, "mixed").unwrap();
        {
            let mut acc = MeshAccessor::new(&mut mesh);
            // two quadrangles and two second-order triangles
            for p in [
                [0., 0.],
                [1., 0.],
                [1., 1.],
                [0., 1.],
                [2., 0.],
                [2., 1.],
                [3., 0.],
                [4., 0.],
                [3., 1.],
                [3.5, 0.],
                [3.5, 0.5],
                [3., 0.5],
                [5., 0.],
                [5., 1.],
                [6., 0.],
                [5.5, 0.],
                [5.5, 0.5],
                [5., 0.5],
            ] {
                acc.push_node(&p).unwrap();
            }
            let mut q = acc.connectivity_mut(ElementType::Quadrangle4, GhostType::NotGhost);
            q.push_tuple(&[0, 1, 2, 3]);
            q.push_tuple(&[1, 4, 5, 2]);
            drop(q);
            let mut t = acc.connectivity_mut(ElementType::Triangle6, GhostType::NotGhost);
            t.push_tuple(&[6, 7, 8, 9, 10, 11]);
            t.push_tuple(&[12, 14, 13, 15, 16, 17]);
        }
        let mesh = SharedMesh::new(mesh);
        let mut b =
            ModelBase::new(mesh.clone(), ModelType::SolidMechanicsModel, None, "smm").unwrap();
        b.fems.register("A", &mesh.read().unwrap(), 2).unwrap();
        let q = |e| Element::new(ElementType::Quadrangle4, e, GhostType::NotGhost);
        let t = |e| Element::new(ElementType::Triangle6, e, GhostType::NotGhost);
        let fe = b.fems.get("A").unwrap();
        // the rules themselves, as upstream's tables
        assert_eq!(
            fe.nb_integration_points(ElementType::Quadrangle4, GhostType::NotGhost)
                .unwrap(),
            4
        );
        assert_eq!(
            fe.nb_integration_points(ElementType::Triangle6, GhostType::NotGhost)
                .unwrap(),
            3
        );
        assert_eq!(b.nb_integration_points(&[q(0)], "").unwrap(), 4);
        assert_eq!(b.nb_integration_points(&[t(1)], "A").unwrap(), 3);
        // 3 quadrangle entries (repeats count) + 4 triangle entries
        let list = [q(0), q(1), t(0), q(0), t(1), t(1), t(1)];
        assert_eq!(b.nb_integration_points(&list, "").unwrap(), 3 * 4 + 4 * 3);
        assert_ne!(b.nb_integration_points(&list, "").unwrap(), list.len());
    }

    #[test]
    fn alloc_nodal_field_allocates_once() {
        let mut slot: Option<Array<Real>> = None;
        alloc_nodal_field(&mut slot, 3, 2).unwrap().as_mut_slice()[0] = 5.0;
        assert_eq!(alloc_nodal_field(&mut slot, 7, 2).unwrap().len(), 3);
        assert_eq!(slot.unwrap().as_slice()[0], 5.0);
    }

    struct Defaults;
    impl SolverCallback for Defaults {
        fn default_solver_options(&self, _: TimeStepSolverType) -> ModelSolverOptions {
            let mut o = ModelSolverOptions {
                non_linear_solver_type: Some(akantu_core::enums::NonLinearSolverType::Linear),
                ..ModelSolverOptions::default()
            };
            o.integration_scheme_type.insert(
                "u".into(),
                akantu_core::enums::IntegrationSchemeType::PseudoTime,
            );
            o
        }
    }

    #[test]
    fn init_new_solver_creates_default_solver_once() {
        let mesh = square();
        let m = mesh.read().unwrap();
        let mut dm = DofManager::new("dm", Communicator::serial());
        dm.register_nodal_dofs(&m, "u", 2).unwrap();
        let mut solver = ModelSolver::new(dm);
        init_new_solver(
            &mut solver,
            &mut Defaults,
            ("static".into(), TimeStepSolverType::Static),
            ModelSolverOptions::default(),
        )
        .unwrap();
        assert!(solver.has_solver("static"));
        assert!(solver.has_integration_scheme("static", "u").unwrap());
        // second call: kept, only the default changes
        init_new_solver(
            &mut solver,
            &mut Defaults,
            ("static".into(), TimeStepSolverType::Static),
            ModelSolverOptions::default(),
        )
        .unwrap();
        assert_eq!(solver.solver(None).unwrap().id, "static");
    }
}
