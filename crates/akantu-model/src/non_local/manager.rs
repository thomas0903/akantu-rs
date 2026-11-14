// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/model/common/non_local_toolbox/
// non_local_manager.{hh,cc}, non_local_manager_inline_impl.hh,
// non_local_manager_callback.hh (NonLocalManager: parsing of the
// `non_local` sections, registerNeighborhood, registerNonLocalVariable,
// registerWeightFunctionInternal, initialize, computeWeights,
// averageInternals, computeAllNonLocalContribution, mesh events).

//! The non-local manager (NL-001).

use std::collections::{BTreeMap, BTreeSet};

use akantu_core::element_type_map::TypeFilter;
use akantu_core::enums::EventPriority;
use akantu_core::{
    ElementKind, ElementType, ElementTypeMapArray, Error, GhostType, Int, Real, Result,
};
use akantu_fe::FeEngine;
use akantu_mesh::{Mesh, MeshEvent, MeshEventHandler};
use akantu_parser::{Parsable, ParserSection, ParserType};

use super::neighborhood::NonLocalNeighborhood;
use super::point::{PointIdentity, QuadPoint};
use super::weight::{WeightArithmetic, WeightFunctionFactory, WeightInternals};

/// Arithmetic route of the averaging (D-008).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SummationOrder {
    /// Upstream: half pair lists in grid order, two weights per pair
    /// normalised by the accumulated volumes, accumulation in pair order,
    /// owned pairs before ghost pairs (default, bit for bit upstream).
    #[default]
    UpstreamHalfPairs,
    /// Opt-in partition-independent route: one key-ordered
    /// `Σ W·J·v / Σ W·J` per owned point over its neighbours (itself,
    /// owned and ghost), identical for any partition of the mesh.
    KeyOrder,
}

/// What happens to a non-local law without an averaged variable when the
/// averages are copied back (upstream `updateNonLocalInternal` asks
/// **every** non-local law for the internal; UD-561).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MissingNonLocalInternal {
    /// Laws that do not own the variable are skipped (corrected default).
    #[default]
    Skip,
    /// Upstream: an error ("does not contain an internal …").
    Error,
}

/// How the non-local variables of the laws are registered (UD-558,
/// category B, decision D-062 (2)).
///
/// Upstream keeps **one** map per manager, keyed by the non-local name but
/// looked up by the local name (`registerNonLocalVariable`), so two laws
/// that average different local variables under the same non-local name
/// (two `mazars_non_local` laws, one with `average_on_damage = true`) leave
/// one entry: both laws average the variable of the law registered last.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum VariableRegistration {
    /// Corrected default: variables are keyed per neighbourhood (upstream
    /// names a law's neighbourhood after the law), so every neighbourhood
    /// averages the local variable its laws declared
    /// ([`NonLocalManager::register_neighborhood_variable`]). Two
    /// declarations of one non-local name with different local variables
    /// or component counts in **one** neighbourhood, or one non-local name
    /// with different component counts in two neighbourhoods, are refused
    /// (they cannot be represented by one averaged field).
    #[default]
    PerNeighborhood,
    /// Upstream (compat, bitwise to the oracle): one shared map per
    /// manager with upstream's lookup-by-local, insert-by-non-local rule;
    /// every neighbourhood averages the shared entry of each non-local
    /// name it registered (last registration wins, silently).
    UpstreamShared,
}

/// Which weight-function internals the distributed weight update exchanges
/// at the ghost slots (`_mnl_weight`; UD-1103, category B).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum WeightExchange {
    /// Corrected default: every internal of the neighbourhood's weight
    /// function (the ghost points' damage of `damage_wf` is their owner's).
    #[default]
    AllInternals,
    /// Upstream: only weight functions whose upstream class packs data
    /// ([`super::WeightFunction::upstream_packs_weight_data`]: `remove_wf`);
    /// `damage_wf` then weighs ghost points with the ghost elements' own,
    /// never computed, damage.
    Upstream,
}

/// Options of the manager (upstream-defect switches and routes).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NonLocalOptions {
    /// Summation route (D-008, D-062 (1)).
    pub order: SummationOrder,
    /// Point identity of the half lists and self pairs (UD-037).
    pub identity: PointIdentity,
    /// Radius tolerance of the pair criterion `d ≤ R + tolerance`
    /// (upstream `Math::getTolerance()`).
    pub tolerance: Real,
    /// Copy-back to laws without the variable (UD-561).
    pub missing_internal: MissingNonLocalInternal,
    /// Registration of the averaged variables (UD-558).
    pub registration: VariableRegistration,
    /// Arithmetic of the weight functions on their upstream-undefined
    /// domain (UD-557, `damage_wf`).
    pub weight_arithmetic: WeightArithmetic,
    /// Weight-function internals exchanged on distributed meshes (UD-1103).
    pub weight_exchange: WeightExchange,
}

impl Default for NonLocalOptions {
    fn default() -> Self {
        Self {
            order: SummationOrder::default(),
            identity: PointIdentity::default(),
            tolerance: akantu_core::math::DEFAULT_TOLERANCE,
            missing_internal: MissingNonLocalInternal::default(),
            registration: VariableRegistration::default(),
            weight_arithmetic: WeightArithmetic::default(),
            weight_exchange: WeightExchange::default(),
        }
    }
}

impl NonLocalOptions {
    /// Every upstream-defect switch of the manager set to reproduce the
    /// reference (UD-037 global-number identity, UD-561 error on a missing
    /// internal, UD-558 shared variable registration, UD-557 x86_64
    /// arithmetic of `damage_wf`'s exponent, UD-1103 weight exchange).
    #[must_use]
    pub fn upstream() -> Self {
        Self {
            identity: PointIdentity::UpstreamGlobalNum,
            missing_internal: MissingNonLocalInternal::Error,
            registration: VariableRegistration::UpstreamShared,
            weight_arithmetic: WeightArithmetic::UpstreamX86_64,
            weight_exchange: WeightExchange::Upstream,
            ..Self::default()
        }
    }
}

/// A source of integration points of one element kind for the manager's
/// per-point layout: the point count and the integration weights (Jacobian
/// times quadrature weight) of every block. The regular kind is the
/// model's FE engine ([`FePointSource`]); upstream's `NonLocalManagerIGFEM`
/// adds the IGFEM kind with the IGFEM FE engine (IGFEM-009, the IGFEM
/// crate's point source).
pub trait NonLocalPointSource: std::fmt::Debug {
    /// Element kind of the blocks this source describes.
    fn kind(&self) -> ElementKind;

    /// Integration points per element of `(ty, ghost)`.
    ///
    /// # Errors
    /// A type the source does not know.
    fn nb_integration_points(&self, ty: ElementType, ghost: GhostType) -> Result<usize>;

    /// Integration weights (`det J · w`) of every point of the block
    /// `(ty, ghost)` of `mesh`, element by element.
    ///
    /// # Errors
    /// No precomputed data for the block.
    fn jacobians(&self, mesh: &Mesh, ty: ElementType, ghost: GhostType) -> Result<Vec<Real>>;
}

/// The regular-kind point source of an FE engine.
#[derive(Clone, Copy, Debug)]
pub struct FePointSource<'a>(pub &'a FeEngine);

impl NonLocalPointSource for FePointSource<'_> {
    fn kind(&self) -> ElementKind {
        ElementKind::Regular
    }
    fn nb_integration_points(&self, ty: ElementType, ghost: GhostType) -> Result<usize> {
        self.0.nb_integration_points(ty, ghost)
    }
    fn jacobians(&self, _mesh: &Mesh, ty: ElementType, ghost: GhostType) -> Result<Vec<Real>> {
        Ok(self.0.jacobians(ty, ghost)?.as_slice().to_vec())
    }
}

/// Implemented by the model owning the non-local laws (upstream
/// `NonLocalManagerCallback`).
pub trait NonLocalManagerCallback {
    /// Upstream `insertIntegrationPointsInNeighborhoods(ghost_type)`: for
    /// each non-local law, register its neighbourhood and variables
    /// ([`NonLocalManager::register_neighborhood`],
    /// [`NonLocalManager::register_non_local_variable`],
    /// [`NonLocalNeighborhood::register_non_local_variable`]) and insert its
    /// integration points of `ghost` ([`NonLocalManager::insert_integration_point`]).
    ///
    /// # Errors
    /// Registration or insertion errors.
    fn insert_integration_points_in_neighborhoods(
        &mut self,
        ghost: GhostType,
        manager: &mut NonLocalManager,
    ) -> Result<()>;

    /// Upstream `initializeNonLocal()` (the solid model synchronises its
    /// law ids; nothing in serial).
    ///
    /// # Errors
    /// Implementation errors.
    fn initialize_non_local(&mut self) -> Result<()> {
        Ok(())
    }

    /// Upstream `updateLocalInternal(internal_flat, ghost_type, kind)`:
    /// flatten the internal `field` of every law that has it into `flat`
    /// (blocks `(type, ghost)` of `nb_element·nb_quad` tuples allocated on
    /// demand, values at `element·nb_quad + q`; upstream `flattenInternal`).
    ///
    /// # Errors
    /// Implementation errors.
    fn update_local_internal(
        &self,
        field: &str,
        ghost: GhostType,
        flat: &mut ElementTypeMapArray<Real>,
    ) -> Result<()>;

    /// Upstream `updateNonLocalInternal(internal_flat, ghost_type, kind)`:
    /// copy the averaged `field` back into the non-local laws' internal of
    /// that name. `strict` (UD-561 compat) requires every non-local law to
    /// own the internal.
    ///
    /// # Errors
    /// Implementation errors.
    fn update_non_local_internal(
        &mut self,
        field: &str,
        ghost: GhostType,
        flat: &ElementTypeMapArray<Real>,
        strict: bool,
    ) -> Result<()>;

    /// Upstream `computeNonLocalContribution(ghost_type)`: every non-local
    /// law computes its non-local stresses.
    ///
    /// # Errors
    /// Law errors.
    fn compute_non_local_contribution(&mut self, ghost: GhostType) -> Result<()>;
}

/// Synchronisation tags of the non-local toolbox (upstream
/// `SynchronizationTag::_mnl_for_average`, `_mnl_weight`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NonLocalTag {
    /// Flattened local variables of a neighbourhood (`_mnl_for_average`).
    ForAverage,
    /// Weight-function internals (`_mnl_weight`).
    Weight,
}

/// Ghost handling of the manager (upstream: one `GridSynchronizer` per
/// neighbourhood, dummy synchronisers for neighbourhoods unknown on a rank,
/// `cleanupExtraGhostElements`). The serial implementation is [`NoGhosts`];
/// the distributed one belongs to the parallel layer (see the module docs
/// for what it must provide).
pub trait NonLocalGhostExchange {
    /// `true` when ghost points exist (a distributed run).
    fn is_distributed(&self) -> bool {
        false
    }

    /// The global neighbourhood names the exchanges run over (upstream
    /// `global_neighborhoods`, agreed over every rank); `None`: the
    /// manager's own neighbourhoods (serial).
    fn global_names(&self) -> Option<&BTreeSet<String>> {
        None
    }

    /// The point layout of the initialisation, once built (the exchanges
    /// map elements to slots with it).
    ///
    /// # Errors
    /// Implementation errors.
    fn on_layout(&mut self, layout: &PointLayout) -> Result<()> {
        let _ = layout;
        Ok(())
    }

    /// Upstream `createNeighborhoodSynchronizers`: called once per
    /// initialisation after the owned points were inserted, with every
    /// local neighbourhood (the implementation agrees the global list of
    /// neighbourhood names and builds one grid synchroniser per name,
    /// which may add ghost elements to the mesh).
    ///
    /// # Errors
    /// Communication errors.
    fn create_synchronizers(
        &mut self,
        neighborhoods: &BTreeMap<String, NonLocalNeighborhood>,
    ) -> Result<()> {
        let _ = neighborhoods;
        Ok(())
    }

    /// Start sending the owned values of `fields` (per-slot arrays of the
    /// manager's layout, `nb_component` values per slot, in variable
    /// order) of `neighborhood` for `tag`.
    ///
    /// # Errors
    /// Communication errors.
    fn start(
        &mut self,
        neighborhood: &str,
        tag: NonLocalTag,
        fields: &[(&[Real], usize)],
    ) -> Result<()> {
        let _ = (neighborhood, tag, fields);
        Ok(())
    }

    /// Receive the ghost values of `fields` (filled in place at the ghost
    /// slots).
    ///
    /// # Errors
    /// Communication errors.
    fn wait(
        &mut self,
        neighborhood: &str,
        tag: NonLocalTag,
        fields: &mut [(&mut [Real], usize)],
    ) -> Result<()> {
        let _ = (neighborhood, tag, fields);
        Ok(())
    }
}

/// Serial ghost handling: no ghost points, nothing exchanged.
#[derive(Clone, Copy, Debug, Default)]
pub struct NoGhosts;

impl NonLocalGhostExchange for NoGhosts {}

/// A registered non-local variable (upstream `NonLocalVariable`).
#[derive(Clone, Debug, PartialEq)]
pub struct NonLocalVariable {
    /// Local internal averaged (upstream `local`).
    pub local: String,
    /// Non-local internal receiving the average (upstream `non_local`).
    pub non_local: String,
    /// Components per point.
    pub nb_component: usize,
}

/// Per-point layout: one slot per integration point of every `(type,
/// ghost)` block of the mesh (regular kind, model dimension), blocks in
/// `(ghost, type)` order, points at `global_num` inside a block.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PointLayout {
    blocks: BTreeMap<(u8, ElementType), (usize, usize)>,
    total: usize,
}

impl PointLayout {
    fn key(ty: ElementType, ghost: GhostType) -> (u8, ElementType) {
        (ghost as u8, ty)
    }

    /// Build from `(type, ghost, nb_points)`.
    #[must_use]
    pub fn new(blocks: impl IntoIterator<Item = (ElementType, GhostType, usize)>) -> Self {
        let mut l = Self::default();
        for (ty, g, n) in blocks {
            l.blocks.insert(Self::key(ty, g), (l.total, n));
            l.total += n;
        }
        l
    }

    /// Number of slots.
    #[must_use]
    pub fn len(&self) -> usize {
        self.total
    }

    /// `true` without slots.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.total == 0
    }

    /// Slot of point `global_num` of `(ty, ghost)`.
    #[must_use]
    pub fn slot(&self, ty: ElementType, ghost: GhostType, global_num: Int) -> Option<usize> {
        let &(off, n) = self.blocks.get(&Self::key(ty, ghost))?;
        let k = usize::try_from(global_num).ok()?;
        (k < n).then_some(off + k)
    }

    /// Blocks `(type, ghost, offset, nb_points)`.
    pub fn blocks(&self) -> impl Iterator<Item = (ElementType, GhostType, usize, usize)> + '_ {
        self.blocks.iter().map(|(&(g, ty), &(off, n))| {
            let ghost = if g == GhostType::Ghost as u8 {
                GhostType::Ghost
            } else {
                GhostType::NotGhost
            };
            (ty, ghost, off, n)
        })
    }

    /// Gather `flat` into a per-slot array of `nb_component` values; blocks
    /// missing from `flat` are zero and reported in the returned set.
    ///
    /// # Errors
    /// A block with another number of components or fewer tuples than the
    /// layout's.
    fn gather(
        &self,
        flat: &ElementTypeMapArray<Real>,
        nb_component: usize,
        name: &str,
    ) -> Result<(Vec<Real>, MissingBlocks)> {
        let mut out = vec![0.; self.total * nb_component];
        let mut missing = BTreeSet::new();
        for (ty, g, off, n) in self.blocks() {
            if !flat.exists(ty, g) {
                missing.insert(Self::key(ty, g));
                continue;
            }
            let a = flat.get(ty, g)?;
            if a.nb_component() != nb_component {
                return Err(Error::DimensionMismatch {
                    context: "non-local variable: components of the flattened internal",
                    expected: nb_component,
                    got: a.nb_component(),
                });
            }
            if a.len() < n {
                return Err(Error::invalid(format!(
                    "flattened internal {name} of {ty}:{g:?} has {} points, the mesh {n}",
                    a.len()
                )));
            }
            out[off * nb_component..(off + n) * nb_component]
                .copy_from_slice(&a.as_slice()[..n * nb_component]);
        }
        Ok((out, missing))
    }

    /// Scatter a per-slot array into a new map named `name` (every block).
    ///
    /// # Errors
    /// Allocation errors.
    fn scatter(
        &self,
        values: &[Real],
        nb_component: usize,
        name: &str,
    ) -> Result<ElementTypeMapArray<Real>> {
        let mut flat = ElementTypeMapArray::new(name);
        for (ty, g, off, n) in self.blocks() {
            let a = flat.alloc(n, nb_component, ty, g, 0.)?;
            a.as_mut_slice()
                .copy_from_slice(&values[off * nb_component..(off + n) * nb_component]);
        }
        Ok(flat)
    }
}

/// Blocks `(ghost as u8, type)` absent from a flattened field.
type MissingBlocks = BTreeSet<(u8, ElementType)>;

#[derive(Clone, Debug, PartialEq, Eq)]
enum State {
    Uninitialized,
    Valid,
    Invalidated(String),
}

/// The non-local manager (upstream `NonLocalManager`, mesh-event priority
/// 100).
#[derive(Debug)]
pub struct NonLocalManager {
    id: String,
    spatial_dimension: usize,
    sections: BTreeMap<String, ParserSection>,
    permissive: bool,
    factory: WeightFunctionFactory,
    neighborhoods: BTreeMap<String, NonLocalNeighborhood>,
    /// Upstream's shared map (`non_local_variables`, keyed by non-local
    /// name, filled with upstream's rule).
    variables: BTreeMap<String, NonLocalVariable>,
    /// Declarations per neighbourhood, by non-local name (UD-558 default).
    declared: BTreeMap<String, BTreeMap<String, NonLocalVariable>>,
    weight_internal_names: BTreeSet<String>,
    layout: PointLayout,
    jacobians: Vec<Real>,
    volumes: Vec<Real>,
    weight_internals: WeightInternals,
    averages: BTreeMap<String, Vec<Real>>,
    compute_stress_calls: Int,
    options: NonLocalOptions,
    grid_center: Vec<Real>,
    state: State,
    /// Element kinds of the layout (the point sources of the last
    /// initialisation): element events of these kinds invalidate the
    /// neighbourhoods.
    kinds: BTreeSet<ElementKind>,
    /// Names the exchanges run over (upstream `global_neighborhoods`).
    global_names: BTreeSet<String>,
    /// Steps of the last cycle (diagnostics, tests).
    pub trace: Vec<String>,
}

impl NonLocalManager {
    /// New manager `id` (upstream `<model id>:non_local_manager`) for a mesh
    /// of `spatial_dimension`, with `base_wf` registered and no section.
    #[must_use]
    pub fn new(id: &str, spatial_dimension: usize) -> Self {
        Self {
            id: id.to_owned(),
            spatial_dimension,
            sections: BTreeMap::new(),
            permissive: false,
            factory: WeightFunctionFactory::default(),
            neighborhoods: BTreeMap::new(),
            variables: BTreeMap::new(),
            declared: BTreeMap::new(),
            weight_internal_names: BTreeSet::new(),
            layout: PointLayout::default(),
            jacobians: Vec::new(),
            volumes: Vec::new(),
            weight_internals: WeightInternals::new(),
            averages: BTreeMap::new(),
            compute_stress_calls: 0,
            options: NonLocalOptions::default(),
            grid_center: vec![0.; spatial_dimension],
            state: State::Uninitialized,
            kinds: BTreeSet::from([ElementKind::Regular]),
            global_names: BTreeSet::new(),
            trace: Vec::new(),
        }
    }

    /// Manager id.
    #[must_use]
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Spatial dimension.
    #[must_use]
    pub fn spatial_dimension(&self) -> usize {
        self.spatial_dimension
    }

    /// Options.
    #[must_use]
    pub fn options(&self) -> NonLocalOptions {
        self.options
    }

    /// Set the options (route, upstream-defect switches); takes effect at
    /// the next [`NonLocalManager::initialize`].
    pub fn set_options(&mut self, options: NonLocalOptions) {
        self.options = options;
        for n in self.neighborhoods.values_mut() {
            n.weight_function_mut()
                .set_arithmetic(options.weight_arithmetic);
        }
        if self.state == State::Valid {
            self.state = State::Invalidated("options changed".into());
        }
    }

    /// The factory of weight-function types.
    pub fn factory_mut(&mut self) -> &mut WeightFunctionFactory {
        &mut self.factory
    }

    /// Upstream constructor part: store every `non_local <name> <type> [ … ]`
    /// sub-section of `root` by name (a later section of the same name
    /// replaces an earlier one, as upstream's map assignment).
    /// `permissive`: upstream `Parser::isPermissive()`.
    pub fn read_sections(&mut self, root: &ParserSection, permissive: bool) {
        for s in root.sub_sections_of(ParserType::NonLocal) {
            self.sections.insert(s.name.clone(), s.clone());
        }
        self.permissive = permissive;
    }

    /// Add one `non_local` section.
    ///
    /// # Errors
    /// A section of another type.
    pub fn add_section(&mut self, section: ParserSection) -> Result<()> {
        if section.section_type != ParserType::NonLocal {
            return Err(Error::invalid(format!(
                "non-local manager: section of type {} is not a non_local section",
                section.section_type
            )));
        }
        self.sections.insert(section.name.clone(), section);
        Ok(())
    }

    /// Centre of the grids of neighbourhoods registered from now on: the
    /// centre of the mesh bounding box (upstream `createGrid` reads
    /// `(upper + lower) / 2` when the neighbourhood is created).
    ///
    /// # Errors
    /// An empty bounding box (compute it first, upstream: NaN centre).
    pub fn set_grid_center_from(&mut self, mesh: &Mesh) -> Result<()> {
        let (lo, hi) = (mesh.lower_bounds(), mesh.upper_bounds());
        if lo.len() != self.spatial_dimension || hi.len() != self.spatial_dimension {
            return Err(Error::DimensionMismatch {
                context: "non-local manager: mesh bounding box",
                expected: self.spatial_dimension,
                got: lo.len(),
            });
        }
        if lo
            .iter()
            .zip(hi)
            .any(|(l, h)| l.partial_cmp(h).is_none_or(std::cmp::Ordering::is_gt))
        {
            return Err(Error::invalid(
                "non-local manager: the mesh bounding box is empty (compute it before creating neighbourhoods)",
            ));
        }
        self.grid_center = lo.iter().zip(hi).map(|(l, h)| (h + l) / 2.).collect();
        Ok(())
    }

    /// Re-centre the grids of every neighbourhood on the current mesh
    /// bounding box ([`NonLocalManager::set_grid_center_from`]; the grids
    /// are emptied, so call [`NonLocalManager::initialize`] next). A
    /// rebuild after a topology change then pairs the points as a manager
    /// created on the changed mesh would.
    ///
    /// # Errors
    /// An empty bounding box.
    pub fn recenter_grids(&mut self, mesh: &Mesh) -> Result<()> {
        self.set_grid_center_from(mesh)?;
        for n in self.neighborhoods.values_mut() {
            n.init_neighborhood(&self.grid_center)?;
        }
        if self.state == State::Valid {
            self.state = State::Invalidated("grids re-centred".into());
        }
        Ok(())
    }

    /// Upstream `registerNeighborhood(neighborhood, weight_func_id)`:
    /// create the neighbourhood from the `non_local <weight_func_id> <type>`
    /// section if it does not exist (weight function of the section's
    /// type, section parsed: `radius`, `weight_function weight_parameter`
    /// sub-section; grid created).
    ///
    /// # Errors
    /// No such section (upstream: debug assertion, then undefined), an
    /// unknown weight-function type, parse errors.
    pub fn register_neighborhood(
        &mut self,
        neighborhood: &str,
        weight_func_id: &str,
    ) -> Result<()> {
        if self.neighborhoods.contains_key(neighborhood) {
            return Ok(());
        }
        let section = self.sections.get(weight_func_id).ok_or_else(|| {
            Error::not_found(
                "non_local section",
                format!(
                    "No info found in the input file for the weight_function {weight_func_id} in the neighborhood {neighborhood}"
                ),
            )
        })?;
        let mut wf = self.factory.create(&section.option)?;
        wf.set_arithmetic(self.options.weight_arithmetic);
        for name in wf.internal_names() {
            self.weight_internal_names.insert(name);
        }
        let mut n = NonLocalNeighborhood::new(
            neighborhood,
            &format!("{}:neighborhood:{neighborhood}", self.id),
            self.spatial_dimension,
            wf,
        );
        n.parse_section(section, self.permissive)?;
        n.init_neighborhood(&self.grid_center)?;
        self.neighborhoods.insert(neighborhood.to_owned(), n);
        Ok(())
    }

    /// Upstream `registerNonLocalVariable(variable_name, nl_variable_name,
    /// nb_component)` into the manager's shared map: upstream looks the
    /// entry up by the **local** name but stores it under the non-local
    /// name, so a registration whose local and non-local names differ
    /// replaces an earlier one (UD-558, kept here as upstream's map).
    ///
    /// This registers no neighbourhood: a neighbourhood that registered the
    /// non-local name ([`NonLocalNeighborhood::register_non_local_variable`])
    /// without a declaration of its own averages this shared entry in both
    /// registration modes. Laws declare their variables with
    /// [`NonLocalManager::register_neighborhood_variable`].
    pub fn register_non_local_variable(
        &mut self,
        local: &str,
        non_local: &str,
        nb_component: usize,
    ) {
        if !self.variables.contains_key(local) {
            self.variables.insert(
                non_local.to_owned(),
                NonLocalVariable {
                    local: local.to_owned(),
                    non_local: non_local.to_owned(),
                    nb_component,
                },
            );
        }
    }

    /// A law's declaration (upstream `registerNonLocalVariables()` of a
    /// non-local law: `registerNonLocalVariable(local, non_local, n)` on the
    /// manager, then `getNeighborhood(name).registerNonLocalVariable
    /// (non_local)`): the neighbourhood averages `local` into `non_local`
    /// (`nb_component` values per point).
    ///
    /// The declaration feeds upstream's shared map with upstream's rule
    /// (used by [`VariableRegistration::UpstreamShared`]) and is recorded
    /// per neighbourhood (used by the corrected default
    /// [`VariableRegistration::PerNeighborhood`], UD-558).
    ///
    /// # Errors
    /// Unknown neighbourhood; under [`VariableRegistration::PerNeighborhood`]
    /// a declaration of `non_local` in the same neighbourhood with another
    /// local variable or component count, or `non_local` declared with
    /// another component count in another neighbourhood (one averaged field
    /// per non-local name cannot hold both).
    pub fn register_neighborhood_variable(
        &mut self,
        neighborhood: &str,
        local: &str,
        non_local: &str,
        nb_component: usize,
    ) -> Result<()> {
        if !self.neighborhoods.contains_key(neighborhood) {
            return Err(Error::not_found(
                "neighborhood",
                format!("The neighborhood {neighborhood} is not registered"),
            ));
        }
        let var = NonLocalVariable {
            local: local.to_owned(),
            non_local: non_local.to_owned(),
            nb_component,
        };
        if self.options.registration == VariableRegistration::PerNeighborhood {
            if let Some(prev) = self
                .declared
                .get(neighborhood)
                .and_then(|m| m.get(non_local))
                && *prev != var
            {
                return Err(Error::invalid(format!(
                    "non-local variable {non_local} of the neighborhood {neighborhood} is declared \
                     as the average of {} ({} components) and of {local} ({nb_component} \
                     components): one neighborhood averages one variable per non-local name \
                     (UD-558; NonLocalOptions::registration = UpstreamShared reproduces \
                     upstream's silent last-wins registration)",
                    prev.local, prev.nb_component
                )));
            }
            for (other, m) in &self.declared {
                if let Some(prev) = m.get(non_local)
                    && other != neighborhood
                    && prev.nb_component != nb_component
                {
                    return Err(Error::invalid(format!(
                        "non-local variable {non_local} has {} components in the neighborhood \
                         {other} and {nb_component} in the neighborhood {neighborhood} (UD-558)",
                        prev.nb_component
                    )));
                }
            }
        }
        self.register_non_local_variable(local, non_local, nb_component);
        self.declared
            .entry(neighborhood.to_owned())
            .or_default()
            .insert(non_local.to_owned(), var);
        self.neighborhood_mut(neighborhood)?
            .register_non_local_variable(non_local);
        Ok(())
    }

    /// Upstream's shared map of registered variables, by non-local name
    /// (what [`VariableRegistration::UpstreamShared`] averages).
    #[must_use]
    pub fn variables(&self) -> &BTreeMap<String, NonLocalVariable> {
        &self.variables
    }

    /// The declarations of `neighborhood`
    /// ([`NonLocalManager::register_neighborhood_variable`]), by non-local
    /// name.
    #[must_use]
    pub fn declared_variables(
        &self,
        neighborhood: &str,
    ) -> Option<&BTreeMap<String, NonLocalVariable>> {
        self.declared.get(neighborhood)
    }

    /// The variable `neighborhood` averages into `non_local` under the
    /// current registration mode: its own declaration
    /// ([`VariableRegistration::PerNeighborhood`]), else upstream's shared
    /// entry.
    #[must_use]
    pub fn resolved_variable(
        &self,
        neighborhood: &str,
        non_local: &str,
    ) -> Option<&NonLocalVariable> {
        if self.options.registration == VariableRegistration::PerNeighborhood
            && let Some(v) = self
                .declared
                .get(neighborhood)
                .and_then(|m| m.get(non_local))
        {
            return Some(v);
        }
        self.variables.get(non_local)
    }

    /// The averaging plan of a cycle: `(neighbourhood, non-local name,
    /// variable)` for every neighbourhood (by name) and every non-local
    /// name it registered (by name) that resolves to a variable (upstream
    /// `averageInternals` order: neighbourhoods, then the manager's
    /// variables by non-local name, skipping those the neighbourhood did
    /// not register).
    fn averaging_plan(&self) -> Vec<(String, String, NonLocalVariable)> {
        let mut plan = Vec::new();
        for (name, n) in &self.neighborhoods {
            for nl in n.non_local_variables() {
                if let Some(v) = self.resolved_variable(name, nl) {
                    plan.push((name.clone(), nl.clone(), v.clone()));
                }
            }
        }
        plan
    }

    /// Upstream `getNeighborhood(name)`.
    ///
    /// # Errors
    /// Unknown neighbourhood.
    pub fn neighborhood(&self, name: &str) -> Result<&NonLocalNeighborhood> {
        self.neighborhoods.get(name).ok_or_else(|| {
            Error::not_found(
                "neighborhood",
                format!("The neighborhood {name} is not registered"),
            )
        })
    }

    /// Mutable [`NonLocalManager::neighborhood`].
    ///
    /// # Errors
    /// Unknown neighbourhood.
    pub fn neighborhood_mut(&mut self, name: &str) -> Result<&mut NonLocalNeighborhood> {
        self.neighborhoods.get_mut(name).ok_or_else(|| {
            Error::not_found(
                "neighborhood",
                format!("The neighborhood {name} is not registered"),
            )
        })
    }

    /// All neighbourhoods, by name.
    #[must_use]
    pub fn neighborhoods(&self) -> &BTreeMap<String, NonLocalNeighborhood> {
        &self.neighborhoods
    }

    /// Insert point `q` at `coords` into `neighborhood` (upstream
    /// `getNeighborhood(name).insertIntegrationPoint(q, coords)`).
    ///
    /// # Errors
    /// Unknown neighbourhood, invalid coordinates.
    pub fn insert_integration_point(
        &mut self,
        neighborhood: &str,
        q: QuadPoint,
        coords: &[Real],
    ) -> Result<()> {
        self.neighborhood_mut(neighborhood)?
            .insert_integration_point(q, coords)
    }

    /// The per-point layout of the last initialisation.
    #[must_use]
    pub fn layout(&self) -> &PointLayout {
        &self.layout
    }

    /// Integration weights `det J · w` per slot (upstream `getJacobians`).
    #[must_use]
    pub fn jacobians(&self) -> &[Real] {
        &self.jacobians
    }

    /// Volumes `V(q)` per slot of the last weight computation (upstream
    /// `getVolumes`; zeroed at the start of every averaging, as upstream).
    #[must_use]
    pub fn volumes(&self) -> &[Real] {
        &self.volumes
    }

    /// Averaged values of variable `non_local` per slot (last cycle).
    #[must_use]
    pub fn averages(&self, non_local: &str) -> Option<&[Real]> {
        self.averages.get(non_local).map(Vec::as_slice)
    }

    /// Upstream `getNbStressCalls()`.
    #[must_use]
    pub fn nb_stress_calls(&self) -> Int {
        self.compute_stress_calls
    }

    /// `true` once the neighbourhoods are built and current.
    #[must_use]
    pub fn is_valid(&self) -> bool {
        self.state == State::Valid
    }

    /// Upstream `initialize()` (may be called again to rebuild after a
    /// topology change, rebuild contract): layout and jacobians of
    /// `fe`, owned points (callback, which also registers neighbourhoods and
    /// variables), ghost synchronisers and ghost points, pair lists in grid
    /// order, `initializeNonLocal`, non-local arrays, weights (always
    /// computed here, whatever the update rate: UD-560). On error the
    /// manager is left uninitialised (averaging refused).
    ///
    /// # Errors
    /// Callback, registration, grid or exchange errors; a point outside the
    /// mesh blocks.
    pub fn initialize(
        &mut self,
        mesh: &Mesh,
        fe: &FeEngine,
        cb: &mut dyn NonLocalManagerCallback,
        ex: &mut dyn NonLocalGhostExchange,
    ) -> Result<()> {
        self.initialize_with_sources(mesh, &[&FePointSource(fe)], cb, ex)
    }

    /// [`NonLocalManager::initialize`] with the integration points of
    /// several element kinds (one [`NonLocalPointSource`] per kind; upstream
    /// `NonLocalManagerIGFEM::init`: the layout, volumes and jacobians of
    /// the regular and the IGFEM kinds). Element events of every kind of
    /// `sources` invalidate the neighbourhoods afterwards.
    ///
    /// # Errors
    /// See [`NonLocalManager::initialize`]; two sources of one kind.
    pub fn initialize_with_sources(
        &mut self,
        mesh: &Mesh,
        sources: &[&dyn NonLocalPointSource],
        cb: &mut dyn NonLocalManagerCallback,
        ex: &mut dyn NonLocalGhostExchange,
    ) -> Result<()> {
        // refused before anything changes (as before the staged split)
        source_kinds(sources)?;
        self.begin_initialize(cb)?;
        ex.create_synchronizers(&self.neighborhoods)?;
        self.insert_ghost_points(cb)?;
        self.update_pair_lists()?;
        self.finish_initialize(mesh, sources, cb, ex)
    }

    /// First stage of [`NonLocalManager::initialize`] (upstream
    /// `initialize()` up to `createNeighborhoodSynchronizers`): the
    /// neighbourhoods are emptied and the owned points inserted (the callback
    /// also registers the neighbourhoods and variables). The distributed
    /// initialisation (the model owning the laws) then builds the grid
    /// synchronisers, a mesh mutation, before
    /// [`NonLocalManager::insert_ghost_points`].
    ///
    /// # Errors
    /// Callback, registration or grid errors (the manager stays
    /// uninitialised).
    pub fn begin_initialize(&mut self, cb: &mut dyn NonLocalManagerCallback) -> Result<()> {
        self.state = State::Uninitialized;
        self.trace.clear();
        for n in self.neighborhoods.values_mut() {
            n.clear_points()?;
        }
        cb.insert_integration_points_in_neighborhoods(GhostType::NotGhost, self)
    }

    /// Upstream `insertIntegrationPointsInNeighborhoods(_ghost)` (after the
    /// grid synchronisers).
    ///
    /// # Errors
    /// Callback or grid errors.
    pub fn insert_ghost_points(&mut self, cb: &mut dyn NonLocalManagerCallback) -> Result<()> {
        cb.insert_integration_points_in_neighborhoods(GhostType::Ghost, self)
    }

    /// Upstream `updatePairLists()`: the pair lists of every neighbourhood.
    ///
    /// # Errors
    /// Grid errors.
    pub fn update_pair_lists(&mut self) -> Result<()> {
        let (identity, tolerance) = (self.options.identity, self.options.tolerance);
        for n in self.neighborhoods.values_mut() {
            n.update_pair_list(identity, tolerance)?;
        }
        Ok(())
    }

    /// The ghost elements holding a point of a pair, over every
    /// neighbourhood (upstream `getRelevantGhostElements`, the set of
    /// `cleanupExtraGhostElements`).
    #[must_use]
    pub fn relevant_ghost_elements(&self) -> BTreeSet<akantu_core::Element> {
        super::ghosts::relevant_ghost_elements(self.neighborhoods.values())
    }

    /// Upstream `NeighborhoodBase::onElementsRemoved` for every
    /// neighbourhood after the cleanup's removal of ghost elements: the
    /// ghost points of the kept elements take their new element index and
    /// `global_num` (`nb_quad(type)` points per element); the points of
    /// removed elements leave the neighbourhood (upstream keeps them in its
    /// grid, used only by the dump helpers, and asserts that no point of a
    /// pair was removed: an error here). The pair lists keep their order.
    ///
    /// # Errors
    /// A removed element holding a point of a pair, a numbering shorter
    /// than an element index.
    pub fn renumber_ghost_points(
        &mut self,
        new_numbering: &ElementTypeMapArray<akantu_core::Idx>,
        nb_quad: &dyn Fn(ElementType) -> Result<usize>,
    ) -> Result<()> {
        for n in self.neighborhoods.values_mut() {
            n.renumber_ghost_points(new_numbering, nb_quad)?;
        }
        Ok(())
    }

    /// Last stage of [`NonLocalManager::initialize`] (upstream: after
    /// `cleanupExtraGhostElements`): `initializeNonLocal`, the point layout
    /// of the mesh as it is now (ghost elements added by the synchronisers
    /// and removed by the cleanup included), the slots of
    /// every point, the integration weights, the non-local arrays and the
    /// weights (always computed here, whatever the update rate: UD-560).
    /// The exchanges run over `ex`'s global names (or the neighbourhoods'
    /// own names). `sources`: one [`NonLocalPointSource`] per element kind
    /// of the layout ([`NonLocalManager::initialize_with_sources`]). On
    /// error the manager stays uninitialised.
    ///
    /// # Errors
    /// Callback or exchange errors; a point outside the mesh blocks; two
    /// sources of one kind.
    pub fn finish_initialize(
        &mut self,
        mesh: &Mesh,
        sources: &[&dyn NonLocalPointSource],
        cb: &mut dyn NonLocalManagerCallback,
        ex: &mut dyn NonLocalGhostExchange,
    ) -> Result<()> {
        self.kinds = source_kinds(sources)?;
        let dim = self.spatial_dimension;
        cb.initialize_non_local()?;
        let mut blocks = Vec::new();
        for g in GhostType::ALL {
            for src in sources {
                let filter = TypeFilter::new().dim(dim).ghost(g).kind(src.kind());
                for ty in mesh.element_types(filter) {
                    let nq = src.nb_integration_points(ty, g)?;
                    blocks.push((ty, g, mesh.nb_element(ty, g) * nq));
                }
            }
        }
        self.layout = PointLayout::new(blocks);
        let layout = &self.layout;
        for n in self.neighborhoods.values_mut() {
            let mut slots = Vec::with_capacity(n.points().len());
            for q in n.points() {
                slots.push(layout.slot(q.ty, q.ghost, q.global_num).ok_or_else(|| {
                    Error::invalid(format!(
                        "non-local neighborhood {}: point {q} is outside the mesh blocks",
                        n.id()
                    ))
                })?);
            }
            n.slots = slots;
        }
        ex.on_layout(&self.layout)?;
        self.global_names = match ex.global_names() {
            Some(g) => g.clone(),
            None => self.neighborhoods.keys().cloned().collect(),
        };
        let mut jac = vec![0.; self.layout.len()];
        let mut has_jac = BTreeSet::new();
        for (ty, g, off, n) in self.layout.blocks() {
            if n == 0 {
                continue;
            }
            let Some(src) = sources.iter().find(|s| s.kind() == ty.kind()) else {
                continue;
            };
            if let Ok(j) = src.jacobians(mesh, ty, g)
                && j.len() >= n
            {
                jac[off..off + n].copy_from_slice(&j[..n]);
                has_jac.insert((g as u8, ty));
            }
        }
        for n in self.neighborhoods.values() {
            if let Some(q) = n
                .points()
                .iter()
                .find(|q| !has_jac.contains(&(q.ghost as u8, q.ty)))
            {
                return Err(Error::invalid(format!(
                    "non-local manager: no integration weights for {}:{:?} (point {q})",
                    q.ty, q.ghost
                )));
            }
        }
        self.jacobians = jac;
        self.volumes = vec![0.; self.layout.len()];
        self.averages.clear();
        self.compute_weights(cb, ex, true)?;
        self.state = State::Valid;
        Ok(())
    }

    /// The neighbourhood names the exchanges of the last initialisation run
    /// over (upstream `global_neighborhoods`; serial: the neighbourhoods).
    #[must_use]
    pub fn global_names(&self) -> &BTreeSet<String> {
        &self.global_names
    }

    /// Upstream `updateWeightFunctionInternals` + `computeWeights`: refresh
    /// the weight-function internals, zero the volumes, then every
    /// neighbourhood whose update is due (`update_rate ≠ 0` and
    /// `compute_stress_calls % update_rate == 0`, or `force`) exchanges
    /// its weight data and recomputes its weights.
    fn compute_weights(
        &mut self,
        cb: &mut dyn NonLocalManagerCallback,
        ex: &mut dyn NonLocalGhostExchange,
        force: bool,
    ) -> Result<()> {
        let mut internals = WeightInternals::new();
        let mut missing_of = BTreeMap::new();
        for name in &self.weight_internal_names {
            let mut flat = ElementTypeMapArray::new(name.clone());
            for g in GhostType::ALL {
                cb.update_local_internal(name, g, &mut flat)?;
            }
            let (v, missing) = self.layout.gather(&flat, 1, name)?;
            internals.insert(name.clone(), v);
            missing_of.insert(name.clone(), missing);
        }
        self.volumes.iter_mut().for_each(|v| *v = 0.);
        let calls = self.compute_stress_calls;
        let identity = self.options.identity;
        let key_order = self.options.order == SummationOrder::KeyOrder;
        // upstream `computeWeights`: every global neighbourhood in name
        // order, a local one exchanges its weight-function data and
        // recomputes its weights when due, a dummy synchroniser exchanges
        // nothing. The port exchanges once per global name whatever is due
        // (the exchanges are collective scopes; a not-due exchange writes
        // ghost slots of freshly flattened internals nothing reads).
        let names: Vec<String> = self.global_names.iter().cloned().collect();
        for name in &names {
            let Some(n) = self.neighborhoods.get_mut(name) else {
                ex.start(name, NonLocalTag::Weight, &[])?;
                ex.wait(name, NonLocalTag::Weight, &mut [])?;
                continue;
            };
            let rate = n.weight_function().update_rate();
            let due = force || (rate != 0 && calls % rate == 0);
            let wf_internals = if self.options.weight_exchange == WeightExchange::Upstream
                && !n.weight_function().upstream_packs_weight_data()
            {
                // UD-1103 compat: upstream exchanges nothing for this weight
                // function (the missing-internal check below still applies)
                Vec::new()
            } else {
                n.weight_function().internal_names()
            };
            let checked_internals = n.weight_function().internal_names();
            if due {
                for internal in &checked_internals {
                    let missing = missing_of.get(internal).cloned().unwrap_or_default();
                    if let Some(q) = n
                        .points()
                        .iter()
                        .find(|q| missing.contains(&(q.ghost as u8, q.ty)))
                    {
                        return Err(Error::not_found(
                            "weight-function internal",
                            format!(
                                "{internal} is not defined for the point {q} of the neighborhood {name}"
                            ),
                        ));
                    }
                }
            }
            {
                let fields: Vec<(&[Real], usize)> = internals
                    .iter()
                    .filter(|(k, _)| wf_internals.contains(*k))
                    .map(|(_, v)| (v.as_slice(), 1))
                    .collect();
                ex.start(name, NonLocalTag::Weight, &fields)?;
            }
            {
                let mut fields: Vec<(&mut [Real], usize)> = internals
                    .iter_mut()
                    .filter(|(k, _)| wf_internals.contains(*k))
                    .map(|(_, v)| (v.as_mut_slice(), 1))
                    .collect();
                ex.wait(name, NonLocalTag::Weight, &mut fields)?;
            }
            if due {
                n.compute_weights(
                    identity,
                    key_order,
                    &self.jacobians,
                    &mut self.volumes,
                    &internals,
                )?;
            }
        }
        self.weight_internals = internals;
        Ok(())
    }

    /// Upstream `computeAllNonLocalContribution()`: flatten the local
    /// variables, start the ghost exchange, average the owned pairs (after
    /// the weight update), wait, average the ghost pairs, copy the averages
    /// back to the laws, non-local stresses, `++compute_stress_calls`.
    /// Refused unless the neighbourhoods are valid.
    ///
    /// # Errors
    /// Stale or missing neighbourhoods, callback and exchange errors, a
    /// local variable missing for a point of a neighbourhood averaging it
    /// (upstream: exception).
    pub fn compute_all_non_local_contribution(
        &mut self,
        cb: &mut dyn NonLocalManagerCallback,
        ex: &mut dyn NonLocalGhostExchange,
    ) -> Result<()> {
        match &self.state {
            State::Valid => {}
            State::Uninitialized => {
                return Err(Error::Mesh(
                    "non-local neighbourhoods were never built: call initialize".into(),
                ));
            }
            State::Invalidated(why) => {
                return Err(Error::Mesh(format!(
                    "non-local neighbourhoods are stale ({why}): call initialize before averaging"
                )));
            }
        }
        self.trace.clear();
        // 0. what every neighbourhood averages (UD-558: per neighbourhood by
        //    default, upstream's shared map in compat mode)
        let plan = self.averaging_plan();
        // components and copy-back set: every averaged non-local name and
        // every entry of upstream's map (upstream copies all of them back)
        let mut nb_components: BTreeMap<String, usize> = BTreeMap::new();
        for (name, nl, var) in &plan {
            if let Some(&n) = nb_components.get(nl)
                && n != var.nb_component
            {
                return Err(Error::invalid(format!(
                    "non-local variable {nl}: {n} components elsewhere, {} in the neighborhood {name}",
                    var.nb_component
                )));
            }
            nb_components.insert(nl.clone(), var.nb_component);
        }
        for (nl, var) in &self.variables {
            nb_components.entry(nl.clone()).or_insert(var.nb_component);
        }
        // 1. flattened local variables (owned and ghost blocks), one per
        //    distinct (local, components)
        let mut locals: BTreeMap<(String, usize), Vec<Real>> = BTreeMap::new();
        let mut missing_of: BTreeMap<(String, usize), MissingBlocks> = BTreeMap::new();
        let mut sources: Vec<(String, usize)> = plan
            .iter()
            .map(|(_, _, v)| (v.local.clone(), v.nb_component))
            .collect();
        sources.extend(
            self.variables
                .values()
                .map(|v| (v.local.clone(), v.nb_component)),
        );
        for key in sources {
            if locals.contains_key(&key) {
                continue;
            }
            let mut flat = ElementTypeMapArray::new(key.0.clone());
            for g in GhostType::ALL {
                cb.update_local_internal(&key.0, g, &mut flat)?;
            }
            let (v, missing) = self.layout.gather(&flat, key.1, &key.0)?;
            locals.insert(key.clone(), v);
            missing_of.insert(key, missing);
        }
        for (name, _, var) in &plan {
            let key = (var.local.clone(), var.nb_component);
            let missing = missing_of.get(&key).cloned().unwrap_or_default();
            let n = self.neighborhood(name)?;
            if let Some(q) = n
                .points()
                .iter()
                .find(|q| missing.contains(&(q.ghost as u8, q.ty)))
            {
                return Err(Error::not_found(
                    "internal",
                    format!(
                        "{} is not defined for the point {q} of the neighborhood {name}",
                        var.local
                    ),
                ));
            }
        }
        let mut accumulated: BTreeMap<String, Vec<Real>> = nb_components
            .iter()
            .map(|(nl, &n)| (nl.clone(), vec![0.; self.layout.len() * n]))
            .collect();
        self.trace.push("flatten".into());
        self.volumes.iter_mut().for_each(|v| *v = 0.);
        // 2. start the exchange of the averaged variables (every global
        //    neighbourhood; a dummy one exchanges nothing)
        let names: Vec<String> = self.global_names.iter().cloned().collect();
        for name in &names {
            let keys = Self::neighborhood_sources(&plan, name);
            let fields: Vec<(&[Real], usize)> = keys
                .iter()
                .filter_map(|k| Some((locals.get(k)?.as_slice(), k.1)))
                .collect();
            ex.start(name, NonLocalTag::ForAverage, &fields)?;
        }
        // 3. weights, then the owned pairs
        self.compute_weights(cb, ex, false)?;
        self.trace.push("weights".into());
        let order = self.options.order;
        if order == SummationOrder::UpstreamHalfPairs {
            self.average(&plan, &locals, &mut accumulated, GhostType::NotGhost);
        }
        // 4. wait for the ghost values, 5. ghost pairs
        for name in &names {
            let keys = Self::neighborhood_sources(&plan, name);
            let mut fields: Vec<(&mut [Real], usize)> = Vec::new();
            for (k, v) in &mut locals {
                if keys.contains(k) {
                    fields.push((v.as_mut_slice(), k.1));
                }
            }
            ex.wait(name, NonLocalTag::ForAverage, &mut fields)?;
        }
        match order {
            SummationOrder::UpstreamHalfPairs => {
                self.average(&plan, &locals, &mut accumulated, GhostType::Ghost);
            }
            SummationOrder::KeyOrder => {
                for (name, nl, var) in &plan {
                    let key = (var.local.clone(), var.nb_component);
                    if let (Ok(n), Some(l), Some(a)) = (
                        self.neighborhood(name),
                        locals.get(&key),
                        accumulated.get_mut(nl),
                    ) {
                        n.key_order_average(l, a, var.nb_component);
                    }
                }
            }
        }
        self.trace.push("average".into());
        // 6. copy back
        let strict = self.options.missing_internal == MissingNonLocalInternal::Error;
        for (nl, &n) in &nb_components {
            let values = accumulated.get(nl).map_or(&[][..], Vec::as_slice);
            let flat = self.layout.scatter(values, n, nl)?;
            for g in GhostType::ALL {
                cb.update_non_local_internal(nl, g, &flat, strict)?;
            }
        }
        self.averages = accumulated;
        self.trace.push("inflate".into());
        // 7. non-local stresses
        cb.compute_non_local_contribution(GhostType::NotGhost)?;
        self.trace.push("compute_non_local_contribution".into());
        self.compute_stress_calls += 1;
        Ok(())
    }

    /// Distinct `(local, components)` sources averaged by `neighborhood`,
    /// in plan order (what its ghost exchange carries).
    fn neighborhood_sources(
        plan: &[(String, String, NonLocalVariable)],
        neighborhood: &str,
    ) -> Vec<(String, usize)> {
        let mut keys: Vec<(String, usize)> = Vec::new();
        for (name, _, var) in plan {
            let key = (var.local.clone(), var.nb_component);
            if name == neighborhood && !keys.contains(&key) {
                keys.push(key);
            }
        }
        keys
    }

    /// Upstream `averageInternals(ghost_type)` (after the weight update):
    /// every neighbourhood (by name), every variable (by non-local name)
    /// registered in it, as resolved in the plan.
    fn average(
        &self,
        plan: &[(String, String, NonLocalVariable)],
        locals: &BTreeMap<(String, usize), Vec<Real>>,
        accumulated: &mut BTreeMap<String, Vec<Real>>,
        ghost: GhostType,
    ) {
        for (name, nl, var) in plan {
            let key = (var.local.clone(), var.nb_component);
            if let (Some(n), Some(l), Some(a)) = (
                self.neighborhoods.get(name),
                locals.get(&key),
                accumulated.get_mut(nl),
            ) {
                n.weighted_average(l, a, var.nb_component, ghost);
            }
        }
    }

    /// Weight-function internals of the last weight update, per slot.
    #[must_use]
    pub fn weight_internals(&self) -> &WeightInternals {
        &self.weight_internals
    }
}

impl MeshEventHandler for NonLocalManager {
    fn event_priority(&self) -> EventPriority {
        EventPriority::NON_LOCAL_MANAGER
    }

    /// Accepts every event: the dispatch only
    /// invalidates the neighbourhoods and never fails.
    fn check_mesh_event(&self, _mesh: &Mesh, _event: &MeshEvent) -> Result<()> {
        Ok(())
    }

    /// New, removed or changed (old or new ids) elements of the model's
    /// dimension (regular kind, and every other kind of the layout's point
    /// sources: the IGFEM kind of `NonLocalManagerIGFEM`, whose
    /// `onElementsAdded/Removed` resize its IGFEM per-point arrays)
    /// invalidate the neighbourhoods (points dropped at the next
    /// initialisation, weight functions and variables kept); averaging is
    /// refused until [`NonLocalManager::initialize`] rebuilt them
    /// (upstream only resizes its per-point arrays and renumbers ghost
    /// points, UD-562). Events on other element kinds or dimensions
    /// (facets, cohesive elements) leave the neighbourhoods valid, as
    /// upstream (their points are not in any neighbourhood).
    fn on_mesh_event(&mut self, _mesh: &Mesh, event: &MeshEvent) -> Result<()> {
        let dim = self.spatial_dimension;
        let kinds = &self.kinds;
        let relevant = |els: &[akantu_core::Element]| {
            els.iter()
                .any(|e| kinds.contains(&e.ty().kind()) && e.ty().spatial_dimension() == dim)
        };
        let why = match event {
            MeshEvent::ElementsAdded { elements, .. } if relevant(elements) => "elements added",
            MeshEvent::ElementsRemoved { elements, .. } if relevant(elements) => "elements removed",
            MeshEvent::ElementsChanged { old, new, .. } if relevant(old) || relevant(new) => {
                "elements changed"
            }
            _ => return Ok(()),
        };
        if self.state != State::Uninitialized {
            self.state = State::Invalidated(why.into());
        }
        Ok(())
    }
}

/// The element kinds of `sources`, refused when two sources share a kind.
fn source_kinds(sources: &[&dyn NonLocalPointSource]) -> Result<BTreeSet<ElementKind>> {
    let mut kinds = BTreeSet::new();
    for src in sources {
        if !kinds.insert(src.kind()) {
            return Err(Error::invalid(format!(
                "non-local manager: two point sources of the element kind {:?}",
                src.kind()
            )));
        }
    }
    Ok(kinds)
}
