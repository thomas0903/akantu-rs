// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/model/common/constitutive_laws/
// constitutive_laws_handler_tmpl.hh (instantiateConstitutiveLaws: manager
// creation; initConstitutiveLaws → NonLocalManager::initialize;
// insertIntegrationPointsInNeighborhoods, updateLocalInternal,
// updateNonLocalInternal, initializeNonLocal),
// constitutive_law_non_local_interface_tmpl.hh
// (insertIntegrationPointsInNeighborhoods, updateNonLocalInternals,
// registerNeighborhood), src/model/solid_mechanics/solid_mechanics_model.cc
// (computeNonLocalContribution; the non-local step of
// assembleInternalForces).

//! The solid model's side of the non-local toolbox: the
//! [`NonLocalManagerCallback`] over the model's laws, and the creation and
//! initialisation of the manager.

use akantu_core::element_type_map::TypeFilter;
use akantu_core::{ElementKind, ElementTypeMapArray, Error, GhostType, Idx, Real, Result};
use akantu_mesh::Mesh;
use akantu_model::ConstitutiveLawsHandler;
use akantu_model::constitutive_law::{InternalFieldMut, InternalFieldRef, LawContext};
use akantu_model::non_local::{NonLocalManager, NonLocalManagerCallback, QuadPoint};

use crate::material::Material;

/// The laws of a solid model seen by the non-local manager (upstream
/// `ConstitutiveLawsHandler` + `SolidMechanicsModel` as
/// `NonLocalManagerCallback`).
pub(crate) struct SmmNonLocalCallback<'a, 'c> {
    pub laws: &'a mut ConstitutiveLawsHandler<dyn Material>,
    pub ctx: &'a LawContext<'c>,
}

fn idx(v: usize) -> Result<Idx> {
    Idx::try_from(v).map_err(|_| Error::invalid("index does not fit an Idx"))
}

impl NonLocalManagerCallback for SmmNonLocalCallback<'_, '_> {
    /// The model's executor: the weighted averages split over its threads.
    fn exec(&self) -> &akantu_core::exec::Exec {
        self.ctx.services.exec()
    }

    /// Upstream `ConstitutiveLawsHandler::insertIntegrationPointsInNeighborhoods`
    /// and `ConstitutiveLawNonLocal::insertIntegrationPointsInNeighborhoods`:
    /// for every non-local law (creation order): register its neighbourhood
    /// (`(name, name)`) and variables
    /// ([`NonLocalManager::register_neighborhood_variable`], UD-558), then
    /// insert, for every regular type
    /// of the law's dimension with elements in its filter, every element of
    /// the filter (filter order) and every quadrature point (`global_num =
    /// element·nb_quad + q`) at its coordinates.
    fn insert_integration_points_in_neighborhoods(
        &mut self,
        ghost: GhostType,
        manager: &mut NonLocalManager,
    ) -> Result<()> {
        let services = self.ctx.services;
        for law in self.laws.laws() {
            let Some(nl) = law.as_non_local() else {
                continue;
            };
            let name = nl.neighborhood_name();
            manager.register_neighborhood(&name, &nl.weight_function_id())?;
            for v in nl.non_local_variables() {
                // upstream registerNonLocalVariables(): manager + neighbourhood;
                // UD-558: recorded per neighbourhood (corrected default) and
                // in upstream's shared map (compat)
                manager.register_neighborhood_variable(
                    &name,
                    &v.local,
                    &v.non_local,
                    v.nb_component,
                )?;
            }
            let dim = law.base().spatial_dimension;
            let filter_types: Vec<_> = law
                .element_filter()
                .element_types(TypeFilter {
                    spatial_dimension: Some(dim),
                    ghost_type: ghost,
                    kind: Some(ElementKind::Regular),
                })
                .collect();
            for ty in filter_types {
                let filter = law.element_filter().get(ty, ghost)?;
                if filter.is_empty() {
                    continue;
                }
                let nq = services.nb_integration_points(ty, ghost);
                let mut coords = akantu_core::Array::new(services.spatial_dimension());
                services.integration_point_coordinates(
                    ty,
                    ghost,
                    filter.as_slice(),
                    &mut coords,
                )?;
                let nq_i = idx(nq)?;
                for (k, &el) in filter.as_slice().iter().enumerate() {
                    for q in 0..nq {
                        let p = QuadPoint::new(ty, ghost, el, idx(q)?, nq_i);
                        manager.insert_integration_point(&name, p, coords.tuple(k * nq + q))?;
                    }
                }
            }
        }
        Ok(())
    }

    /// Upstream `updateLocalInternal`: `flattenInternal` of every law that
    /// has a `Real` internal `field` (upstream `isInternal<Real>(name,
    /// _ek_regular)`).
    fn update_local_internal(
        &self,
        field: &str,
        ghost: GhostType,
        flat: &mut ElementTypeMapArray<Real>,
    ) -> Result<()> {
        let services = self.ctx.services;
        let mesh = services.mesh();
        for law in self.laws.laws() {
            let mut result = Ok(());
            law.for_each_internal(&mut |name, f| {
                if name != field || result.is_err() {
                    return;
                }
                if let InternalFieldRef::Real(f) = f {
                    result = f.flatten(
                        law.element_filter(),
                        ghost,
                        &|ty| mesh.nb_element(ty, ghost),
                        &|ty| services.nb_integration_points(ty, ghost),
                        0.,
                        flat,
                    );
                }
            });
            result?;
        }
        Ok(())
    }

    /// Upstream `updateNonLocalInternal` →
    /// `ConstitutiveLawNonLocal::updateNonLocalInternals`: copy the averages
    /// into the internal `field` of every non-local law that has it; with
    /// `strict` (UD-561 compat), a non-local law without it is an error, as
    /// upstream's `getInternal`.
    fn update_non_local_internal(
        &mut self,
        field: &str,
        ghost: GhostType,
        flat: &ElementTypeMapArray<Real>,
        strict: bool,
    ) -> Result<()> {
        let services = self.ctx.services;
        for law in self.laws.laws_mut() {
            if law.as_non_local().is_none() {
                continue;
            }
            let filter = law.element_filter().clone();
            let mut found = false;
            law.for_each_internal_mut(&mut |name, f| {
                if name != field {
                    return Ok(());
                }
                if let InternalFieldMut::Real(f) = f {
                    found = true;
                    f.inflate(
                        &filter,
                        ghost,
                        &|ty| services.nb_integration_points(ty, ghost),
                        flat,
                    )?;
                }
                Ok(())
            })?;
            if !found && strict {
                return Err(Error::not_found(
                    "internal",
                    format!(
                        "The constitutive law {}({}) does not contain an internal {field}",
                        law.name(),
                        law.law_id()
                    ),
                ));
            }
        }
        Ok(())
    }

    /// Upstream `SolidMechanicsModel::computeNonLocalContribution`: every
    /// non-local law's `computeNonLocalStresses(ghost_type)`, in creation
    /// order.
    fn compute_non_local_contribution(&mut self, ghost: GhostType) -> Result<()> {
        for law in self.laws.laws_mut() {
            if let Some(nl) = law.as_non_local_mut() {
                nl.compute_non_local_stresses(self.ctx, ghost)?;
            }
        }
        Ok(())
    }
}

/// `true` if a law of the handler is non-local (upstream
/// `dynamic_cast<ConstitutiveLawNonLocalInterface *>` in
/// `instantiateConstitutiveLaws`).
pub(crate) fn has_non_local_law(laws: &ConstitutiveLawsHandler<dyn Material>) -> bool {
    laws.laws().iter().any(|l| l.as_non_local().is_some())
}

/// D-097 (1): the summation route `KeyOrder` is refused on a distributed
/// mesh (its key needs global element ids, which a distributed mesh does
/// not carry, D-062 (1)). Called in the recoverable selection phase of the
/// distributed `init_materials` (before the selection
/// is committed, so the model can be initialised again with another
/// route) and, as a defensive duplicate, at the start of
/// [`SolidMechanicsModel::init_non_local_distributed`]. Local: the inputs
/// (the laws, the options) are the same on every rank of a model built
/// identically; the caller agrees the result.
pub(crate) fn check_distributed_route(physics: &super::SolidMechanicsPhysics) -> Result<()> {
    if has_non_local_law(&physics.materials)
        && physics.non_local_options.order == SummationOrder::KeyOrder
    {
        return Err(Error::Unsupported(
            "non-local summation route KeyOrder on a distributed mesh: its key needs global \
             element ids, which a distributed mesh does not carry (D-062 (1)); use \
             UpstreamHalfPairs"
                .into(),
        ));
    }
    Ok(())
}

/// Refuse a rebuild of the neighbourhoods on a distributed mesh
/// ([`SolidMechanicsModel::reinitialize_non_local`]): upstream has no
/// rebuild, and a second pass of the grid synchronisers would add ghost
/// layers to a mesh that already holds the first ones (the
/// distributed manager is built once, by `init_full`).
pub(crate) fn check_serial(mesh: &Mesh) -> Result<()> {
    if mesh.synchronizers().element.is_some() {
        return Err(Error::Unsupported(
            "rebuilding the non-local neighbourhoods on a distributed mesh \
             (reinitialize_non_local): the grid synchronisers are built once by init_full \
             (upstream has no rebuild); rebuild the model instead"
                .into(),
        ));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Distributed meshes (MPI-048…050; ported from upstream
// non_local_manager.cc: initialize, createNeighborhoodSynchronizers,
// cleanupExtraGhostElements, onElementsRemoved; non_local_neighborhood_base.cc:
// createGridSynchronizer, getRelevantGhostElements, cleanupExtraGhostElements;
// constitutive_laws_handler_tmpl.hh: onElementsAdded → assignConstitutiveLawToElements
// → non_local_manager->synchronize(_constitutive_law_id), initializeNonLocal)
// ---------------------------------------------------------------------------

use std::collections::{BTreeMap, BTreeSet};

use akantu_comm::phase::{Phases, Severity};
use akantu_comm::{Communicator, Synchronizer};
use akantu_core::enums::SynchronizationTag;
use akantu_core::{Element, ElementType};
use akantu_mesh::grid_synchronizer::{
    GridCells, GridSynchronizerOptions, create_grid_synchronizer_shared,
};
use akantu_mesh::{DistributedHook, MeshEvent, MeshEventHandler, MeshPart, MeshTransaction};
use akantu_model::constitutive_law::LawPhase;
use akantu_model::non_local::{
    EmptyGridCells, GridGhosts, NeighborhoodGridCells, SummationOrder, agree_neighborhood_names_in,
};

use crate::SolidMechanicsModel;

/// Collective scope helper: run `f` in a fresh scope named `name` and
/// settle it (agreed outcome on every rank).
fn in_scope<R>(
    comm: &Communicator,
    name: &str,
    f: impl FnOnce(&mut Phases) -> Result<R>,
) -> Result<R> {
    let mut ph = Phases::new(comm, name);
    let r = f(&mut ph);
    let r = ph.agreed(&format!("{name}:done"), r, Severity::Recoverable);
    ph.settle(r)
}

/// The cleanup's distributed hook: the element synchronisers that hold the
/// removed ghost elements follow the removal (upstream
/// `ElementSynchronizer::onElementsRemoved`: `filterScheme` without the
/// removed elements — collective, the owners drop them from their send
/// schemes — then `renumberElements`), the mesh's own synchroniser in the
/// transaction, the grid synchronisers staged until the commit.
struct CleanupHook<'g> {
    grids: &'g BTreeMap<String, Synchronizer<Element>>,
    staged: Option<BTreeMap<String, Synchronizer<Element>>>,
}

fn follow_removal(
    sync: &mut Synchronizer<Element>,
    ph: &mut Phases,
    removed: &BTreeSet<Element>,
    numbering: Option<&ElementTypeMapArray<Idx>>,
) -> Result<()> {
    sync.filter_scheme_in(ph, |e| !removed.contains(e))?;
    if let Some(n) = numbering {
        akantu_mesh::synchronizers::renumber_elements(sync, n)?;
    }
    Ok(())
}

/// The elemental mesh data of the grid synchronisers' ghost elements are
/// short (upstream leaves them so, `akantu_mesh::grid_synchronizer`); the
/// removal renumbers one row per element: pad every ghost block of the
/// mesh's dimension with empty rows first (inside the cleanup's mutation).
fn pad_ghost_data(tx: &mut MeshTransaction<'_>) -> Result<()> {
    let dim = tx.mesh().spatial_dimension();
    let blocks: Vec<(ElementType, usize)> = tx
        .mesh()
        .element_types(TypeFilter {
            spatial_dimension: Some(dim),
            ghost_type: GhostType::Ghost,
            kind: None,
        })
        .map(|ty| (ty, tx.mesh().nb_element(ty, GhostType::Ghost)))
        .collect();
    let data = tx.data_mut(MeshPart::Main)?;
    for (ty, n) in blocks {
        data.extend_elemental_rows(ty, GhostType::Ghost, n, ty.nb_nodes_per_element())?;
    }
    Ok(())
}

impl DistributedHook for CleanupHook<'_> {
    fn prepare(&mut self, _mesh: &Mesh, _events: &[(MeshPart, MeshEvent)]) -> Result<()> {
        Ok(())
    }

    fn apply(
        &mut self,
        tx: &mut MeshTransaction<'_>,
        events: &[(MeshPart, MeshEvent)],
        ph: &mut Phases,
    ) -> Result<()> {
        let mut removed = BTreeSet::new();
        let mut numbering = None;
        for (part, ev) in events {
            if let (
                MeshPart::Main,
                MeshEvent::ElementsRemoved {
                    elements,
                    new_numbering,
                    ..
                },
            ) = (part, ev)
            {
                removed.extend(elements.iter().copied());
                numbering = Some(new_numbering.clone());
            }
        }
        if let Some(es) = tx.synchronizers_mut(MeshPart::Main)?.element.as_mut() {
            follow_removal(es, ph, &removed, numbering.as_ref())?;
        }
        let mut staged = self.grids.clone();
        for s in staged.values_mut() {
            follow_removal(s, ph, &removed, numbering.as_ref())?;
        }
        self.staged = Some(staged);
        Ok(())
    }
}

impl SolidMechanicsModel {
    /// The non-local manager of `init_non_local`: a fresh one (sections,
    /// solid weight functions, options, grid centre) or the existing one
    /// rebuilt (options re-applied, grids re-centred).
    pub(crate) fn take_or_new_non_local_manager(
        &mut self,
        mesh: &Mesh,
    ) -> Result<akantu_model::non_local::NonLocalManager> {
        let p = &mut self.physics;
        Ok(match p.non_local.take() {
            Some(mut m) => {
                m.set_options(p.non_local_options);
                m.recenter_grids(mesh)?;
                m
            }
            None => {
                let mut m = akantu_model::non_local::NonLocalManager::new(
                    &format!("{}:non_local_manager", p.base.id),
                    p.base.spatial_dimension,
                );
                m.read_sections(&p.base.parser.root, p.base.parser.is_permissive());
                #[cfg(feature = "damage-non-local")]
                crate::materials::non_local::register_weight_functions(m.factory_mut());
                m.set_options(p.non_local_options);
                m.set_grid_center_from(mesh)?;
                m
            }
        })
    }

    /// Upstream `NonLocalManager::initialize()` on a distributed mesh
    /// (MPI-048…050), after every law's `initMaterial`:
    ///
    /// 1. the owned points (`insertIntegrationPointsInNeighborhoods(_not_ghost)`);
    /// 2. the global neighbourhood names (`allGather` of every rank's
    ///    names; upstream `global_neighborhoods`);
    /// 3. per global name, in name order, a grid synchroniser built by a
    ///    mesh mutation (new pure-ghost nodes and ghost elements dispatched
    ///    to the model: FE engines, DOF manager, handler maps; a rank without
    ///    the neighbourhood takes part with upstream's dummy grid) and the
    ///    owners' laws of the new ghost elements (`_constitutive_law_id` over
    ///    the new synchroniser; upstream sequence with
    ///    [`super::SmmUpstreamCompat::non_local_ghost_laws_after_insertion`],
    ///    UD-1102), appended to the laws' ghost filters in unpack order,
    ///    internals resized;
    /// 4. the ghost points and the pair lists;
    /// 5. the cleanup: the ghost elements in no pair are erased (the
    ///    distribution's ghost layer kept by default, UD-1101; one erasure
    ///    with the union of the relevant elements, UD-1100), the mesh's
    ///    element synchroniser and the grid synchronisers follow
    ///    (`filterScheme`, renumbering), the neighbourhoods renumber their
    ///    ghost points;
    /// 6. `initializeNonLocal`: `_constitutive_law_id` over every grid
    ///    synchroniser;
    /// 7. the point layout, integration weights and weights, with the
    ///    `_mnl_weight` exchange.
    ///
    /// Every step is collective; a rank-specific failure is agreed on every
    /// rank (the caller makes it fatal: the mesh has changed, rebuild the
    /// model). The summation route `KeyOrder` (it needs global element ids,
    /// D-062 (1)) is refused before, recoverably, by the selection phase of
    /// `init_materials` ([`check_distributed_route`]).
    pub(crate) fn init_non_local_distributed(&mut self) -> Result<()> {
        let mesh_h = self.physics.base.mesh.clone();
        let (comm, dim) = {
            let m = mesh_h.read()?;
            (m.communicator().clone(), m.spatial_dimension())
        };
        let compat = self.physics.upstream_compat;
        self.physics.non_local_ghosts = None;
        let (mut manager_opt, mut first_err) = {
            let m = mesh_h.read()?;
            match self.take_or_new_non_local_manager(&m) {
                Ok(mg) => (Some(mg), None),
                Err(e) => (None, Some(e)),
            }
        };
        // 1–2. owned points, global names (and the refusals), agreed
        let names = in_scope(&comm, "smm:non_local:names", |ph| {
            let begin = (|| {
                if let Some(e) = first_err.take() {
                    return Err(e);
                }
                let manager = manager_opt
                    .as_mut()
                    .ok_or_else(|| Error::invalid("non-local manager missing"))?;
                // refused earlier, recoverably, by `init_materials_in`'s
                // selection phase; kept as a defensive duplicate
                check_distributed_route(&self.physics)?;
                let m = mesh_h.read()?;
                let p = &mut self.physics;
                let (laws, services) = p.split(&m, &self.solver.dof_manager);
                let ctx = LawContext {
                    phase: LawPhase::Init,
                    services: &services,
                };
                let mut cb = SmmNonLocalCallback { laws, ctx: &ctx };
                manager.begin_initialize(&mut cb)?;
                Ok(manager
                    .neighborhoods()
                    .keys()
                    .cloned()
                    .collect::<BTreeSet<_>>())
            })();
            let local = ph.agreed("owned_points", begin, Severity::Recoverable)?;
            let names = agree_neighborhood_names_in(ph, &local)?;
            Ok(names)
        })?;
        let mut manager = manager_opt.ok_or_else(|| Error::invalid("non-local manager missing"))?;
        // the distribution's ghost layer (UD-1101: kept by the default cleanup)
        let original: BTreeMap<ElementType, usize> = {
            let m = mesh_h.read()?;
            m.element_types(TypeFilter {
                spatial_dimension: Some(dim),
                ghost_type: GhostType::Ghost,
                kind: Some(ElementKind::Regular),
            })
            .map(|ty| (ty, m.nb_element(ty, GhostType::Ghost)))
            .collect()
        };
        // 3. grid synchronisers and the laws of their ghost elements
        let mut ghosts = GridGhosts::new(names.clone());
        let names: Vec<String> = names.into_iter().collect();
        for (k, name) in names.iter().enumerate() {
            let sync = {
                // a global name without a local neighbourhood: upstream's
                // dummy grid (unreachable from this model, which registers
                // every law's neighbourhood on every rank)
                let empty = EmptyGridCells::new(dim);
                let local = manager
                    .neighborhoods()
                    .get(name)
                    .map(NeighborhoodGridCells::new);
                let cells: &dyn GridCells = match &local {
                    Some(c) => c,
                    None => &empty,
                };
                let id = format!("{}:{name}:grid_synchronizer", manager.id());
                let (token, mut parts) = self.split_for_mutation();
                let handler: &mut dyn MeshEventHandler = &mut parts;
                create_grid_synchronizer_shared(
                    &mesh_h,
                    &mut [(token, handler)],
                    cells,
                    id,
                    GridSynchronizerOptions::default(),
                )?
                .0
            };
            ghosts.insert_synchronizer(name, sync);
            // UD-1102: upstream exchanges over the synchronisers built
            // before this one (`onElementsAdded` of this one's elements);
            // the default over this one
            let over: &[String] = if compat.non_local_ghost_laws_after_insertion {
                &names[..k]
            } else {
                std::slice::from_ref(name)
            };
            self.exchange_ghost_laws(&comm, &ghosts, over)?;
        }
        // 4. ghost points and pair lists
        in_scope(&comm, "smm:non_local:pairs", |_ph| {
            let m = mesh_h.read()?;
            let p = &mut self.physics;
            let (laws, services) = p.split(&m, &self.solver.dof_manager);
            let ctx = LawContext {
                phase: LawPhase::Init,
                services: &services,
            };
            let mut cb = SmmNonLocalCallback { laws, ctx: &ctx };
            manager.insert_ghost_points(&mut cb)?;
            manager.update_pair_lists()
        })?;
        // 5. cleanup of the ghost elements in no pair
        self.cleanup_extra_ghost_elements(&mut manager, &mut ghosts, &original)?;
        // 6. upstream initializeNonLocal
        self.exchange_ghost_laws(&comm, &ghosts, &names)?;
        // 7. layout, integration weights, weights
        let nb_quad = {
            let m = mesh_h.read()?;
            let fe = self.physics.base.fems.get("")?;
            let mut map = BTreeMap::new();
            for g in GhostType::ALL {
                for ty in m.element_types(TypeFilter {
                    spatial_dimension: Some(dim),
                    ghost_type: g,
                    kind: Some(ElementKind::Regular),
                }) {
                    map.insert(ty, fe.nb_integration_points(ty, g)?);
                }
            }
            map
        };
        ghosts.set_nb_quad(nb_quad);
        in_scope(&comm, "smm:non_local:weights", |ph| {
            let m = mesh_h.read()?;
            let p = &mut self.physics;
            // the point sources of the serial path (`init_non_local`): the
            // regular FE engine and the other element kinds' (IGFEM-009)
            let extra = p.extra_point_sources.clone();
            let (laws, services) = p.split(&m, &self.solver.dof_manager);
            let ctx = LawContext {
                phase: LawPhase::Init,
                services: &services,
            };
            let fe = services.fems.get("")?;
            let regular = akantu_model::non_local::FePointSource(fe);
            let mut sources: Vec<&dyn akantu_model::non_local::NonLocalPointSource> =
                vec![&regular];
            sources.extend(
                extra
                    .iter()
                    .map(|s| s.as_ref() as &dyn akantu_model::non_local::NonLocalPointSource),
            );
            let mut cb = SmmNonLocalCallback { laws, ctx: &ctx };
            let mut ex = ghosts.exchange_in(ph);
            manager.finish_initialize(&m, &sources, &mut cb, &mut ex)
        })?;
        self.physics.non_local = Some(manager);
        self.physics.non_local_ghosts = Some(ghosts);
        Ok(())
    }

    /// `_constitutive_law_id` over the grid synchronisers `names` (in that
    /// order; upstream `NonLocalManager::synchronize`): a ghost element
    /// without a law takes its owner's, appended to the law's ghost filter
    /// in unpack order (rank order, scheme order: UD-232); then every
    /// initialised law resizes its internals (upstream
    /// `ConstitutiveLaw::onElementsAdded` → `resizeInternals` after the
    /// assignment).
    fn exchange_ghost_laws(
        &mut self,
        comm: &Communicator,
        ghosts: &GridGhosts,
        names: &[String],
    ) -> Result<()> {
        let mesh_h = self.physics.base.mesh.clone();
        in_scope(comm, "smm:non_local:ghost_laws", |ph| {
            for name in names {
                let sync = ghosts.synchronizer(name).ok_or_else(|| {
                    Error::invalid(format!("no grid synchroniser for the neighbourhood {name}"))
                })?;
                let mut acc = super::parallel::LawIdAccessor {
                    materials: &mut self.physics.materials,
                };
                sync.synchronize_in(ph, &mut acc, SynchronizationTag::ConstitutiveLawId)?;
            }
            let m = mesh_h.read()?;
            let p = &mut self.physics;
            let (laws, services) = p.split(&m, &self.solver.dof_manager);
            let ctx = LawContext {
                phase: LawPhase::Init,
                services: &services,
            };
            for law in laws.laws_mut() {
                if law.base().is_init() {
                    super::resize_law_internals(law.as_mut(), &ctx)?;
                }
            }
            Ok(())
        })
    }

    /// Upstream `NonLocalManager::cleanupExtraGhostElements` (see
    /// [`SolidMechanicsModel::init_non_local_distributed`] step 5).
    ///
    /// Default: the ghost elements holding no point of any pair are erased
    /// once (UD-1100), and only those the grid synchronisers added, each
    /// with its ghost counter brought to 0 (UD-1101).
    ///
    /// Compat (`non_local_cleanup_erases_distribution_ghosts`): upstream's
    /// erasure, once per neighbourhood in name order, of every ghost element
    /// of the mesh as it is then whose `(type, index)` is not in the set of
    /// relevant elements computed **before** the first erasure (counter
    /// decremented once, removed at 0, `Mesh::eraseElements`): from the
    /// second neighbourhood on it compares renumbered elements with stale
    /// ones (UD-1100). Where that removes a point of a pair, upstream's
    /// assertion fails: an error here.
    fn cleanup_extra_ghost_elements(
        &mut self,
        manager: &mut akantu_model::non_local::NonLocalManager,
        ghosts: &mut GridGhosts,
        original: &BTreeMap<ElementType, usize>,
    ) -> Result<()> {
        let compat = self.physics.upstream_compat;
        let relevant = manager.relevant_ghost_elements();
        let passes = if compat.non_local_cleanup_erases_distribution_ghosts {
            manager.neighborhoods().len().max(1)
        } else {
            1
        };
        for _ in 0..passes {
            let to_erase = {
                let mesh_h = self.physics.base.mesh.clone();
                let m = mesh_h.read()?;
                let dim = m.spatial_dimension();
                let mut out = Vec::new();
                for ty in m.element_types(TypeFilter {
                    spatial_dimension: Some(dim),
                    ghost_type: GhostType::Ghost,
                    kind: None,
                }) {
                    let counters = m.ghosts_counters().get(ty, GhostType::Ghost).ok();
                    let protected = original.get(&ty).copied().unwrap_or(0);
                    for g in 0..m.nb_element(ty, GhostType::Ghost) {
                        let e = Element::new(ty, akantu_core::try_idx(g)?, GhostType::Ghost);
                        if relevant.contains(&e) {
                            continue;
                        }
                        if compat.non_local_cleanup_erases_distribution_ghosts {
                            out.push(e);
                        } else if g >= protected {
                            let c = counters
                                .and_then(|a| a.as_slice().get(g).copied())
                                .unwrap_or(1)
                                .max(1);
                            for _ in 0..c {
                                out.push(e);
                            }
                        }
                    }
                }
                out
            };
            self.erase_ghost_elements(manager, ghosts, &to_erase)?;
        }
        Ok(())
    }

    /// One `Mesh::eraseElements` of the cleanup as a mesh mutation (the
    /// model's consumers follow the `ElementsRemoved` event), the element
    /// synchronisers following in the distributed hook, the neighbourhoods
    /// renumbering their ghost points.
    fn erase_ghost_elements(
        &mut self,
        manager: &mut akantu_model::non_local::NonLocalManager,
        ghosts: &mut GridGhosts,
        to_erase: &[Element],
    ) -> Result<()> {
        let mesh_h = self.physics.base.mesh.clone();
        let mut hook = CleanupHook {
            grids: ghosts.synchronizers(),
            staged: None,
        };
        let committed = {
            let (token, mut parts) = self.split_for_mutation();
            let handler: &mut dyn MeshEventHandler = &mut parts;
            mesh_h.mutate(
                &mut [(token, handler)],
                |tx| {
                    pad_ghost_data(tx)?;
                    tx.erase_elements(MeshPart::Main, to_erase)
                },
                Some(&mut hook),
            )?
        };
        if let Some(staged) = hook.staged {
            *ghosts.synchronizers_mut() = staged;
        }
        let numbering = committed
            .events
            .iter()
            .find_map(|(part, ev)| match (part, ev) {
                (MeshPart::Main, MeshEvent::ElementsRemoved { new_numbering, .. }) => {
                    Some(new_numbering.clone())
                }
                _ => None,
            });
        // every rank renumbers (an empty local numbering changes nothing);
        // a failure (upstream's assertion, UD-1100) is agreed on every rank
        let comm = mesh_h.read()?.communicator().clone();
        in_scope(&comm, "smm:non_local:renumber", |_ph| {
            if let Some(n) = numbering {
                let fe = self.physics.base.fems.get("")?;
                manager
                    .renumber_ghost_points(&n, &|ty| fe.nb_integration_points(ty, GhostType::Ghost))
                    .map_err(|e| {
                        Error::invalid(format!(
                            "{e} (upstream aborts here: its cleanup with two or more \
                             neighbourhoods erases with a stale element set, UD-1100; the \
                             default cleanup does not)"
                        ))
                    })?;
            }
            Ok(())
        })
    }
}

// ---------------------------------------------------------------------------
// NeighborhoodMaxCriterion on the model (MPI-050; upstream
// neighborhood_max_criterion.cc: the constructor, initNeighborhood,
// createGridSynchronizer, cleanupExtraGhostElements, findMaxQuads with
// model.updateDataForNonLocalCriterion)
// ---------------------------------------------------------------------------

use akantu_model::non_local::max_criterion::NeighborhoodMaxCriterion;
use akantu_parser::ParserSection;

impl SolidMechanicsModel {
    /// Upstream `NeighborhoodMaxCriterion(model, quad_coordinates,
    /// criterion_id, id)` + `initNeighborhood()` on the model's mesh, with
    /// the `neighborhood` `section` (radius). On a mesh distributed over
    /// more than one rank (MPI-050, the intended behaviour: upstream's class
    /// cannot be instantiated, UD-563): the grid synchroniser of the owned
    /// points (a mesh mutation, the model's consumers follow), every ghost
    /// point, the pairs, the removal of the ghost elements the synchroniser
    /// added that hold no second point of a ghost pair (counters brought to
    /// 0; the distribution's ghost layer is kept, as upstream's
    /// `nb_ghost_protected`), and the layout of both ghost types. Collective
    /// there; a failure is agreed on every rank.
    ///
    /// The ghost elements the criterion's grid synchroniser adds get **no
    /// constitutive law** (their law index stays −1, unlike the non-local
    /// manager's, which exchanges `_constitutive_law_id`): the criterion
    /// reads only its own ghost slots, written by the `_nh_criterion`
    /// exchange, and its synchroniser is not one of the manager's, as
    /// upstream intends. Those elements stay in the
    /// mesh (without a law) until the model is rebuilt.
    ///
    /// # Errors
    /// Parse, FE, mutation or communication errors; a model whose
    /// non-local manager lives on a distributed mesh (the elements the
    /// criterion's synchroniser adds and removes would invalidate it, and
    /// the distributed manager is not rebuilt: `Error::Unsupported`).
    pub fn new_neighborhood_max_criterion(
        &mut self,
        criterion_id: &str,
        id: &str,
        section: &ParserSection,
    ) -> Result<NeighborhoodMaxCriterion> {
        let mesh_h = self.physics.base.mesh.clone();
        let permissive = self.physics.base.parser.is_permissive();
        let (comm, distributed) = {
            let m = mesh_h.read()?;
            let c = m.communicator().clone();
            let d = c.size() > 1 && m.synchronizers().element.is_some();
            (c, d)
        };
        let owned = (|| {
            if distributed && self.physics.non_local_ghosts.is_some() {
                return Err(Error::Unsupported(
                    "a neighbourhood max criterion on a distributed mesh whose model holds a \
                     distributed non-local manager: its grid synchroniser would add and remove \
                     ghost elements under the manager"
                        .into(),
                ));
            }
            let m = mesh_h.read()?;
            let fe = self.physics.base.fems.get("")?;
            let original: BTreeMap<ElementType, usize> = m
                .element_types(TypeFilter {
                    spatial_dimension: Some(m.spatial_dimension()),
                    ghost_type: GhostType::Ghost,
                    kind: Some(ElementKind::Regular),
                })
                .map(|ty| (ty, m.nb_element(ty, GhostType::Ghost)))
                .collect();
            let c =
                NeighborhoodMaxCriterion::new_owned(&m, fe, criterion_id, id, section, permissive)?;
            Ok((c, original))
        })();
        if !distributed {
            let (mut c, _) = owned?;
            let m = mesh_h.read()?;
            let fe = self.physics.base.fems.get("")?;
            c.update_pairs()?;
            c.finish(&m, fe)?;
            return Ok(c);
        }
        let (mut c, original) = in_scope(&comm, "smm:max_criterion:owned", |_| owned)?;
        let sync = {
            let cells = c.grid_cells();
            let (token, mut parts) = self.split_for_mutation();
            let handler: &mut dyn MeshEventHandler = &mut parts;
            create_grid_synchronizer_shared(
                &mesh_h,
                &mut [(token, handler)],
                &cells,
                format!("{id}:grid_synchronizer"),
                GridSynchronizerOptions::default(),
            )?
            .0
        };
        c.set_synchronizer(sync);
        in_scope(&comm, "smm:max_criterion:pairs", |_| {
            let m = mesh_h.read()?;
            let fe = self.physics.base.fems.get("")?;
            c.insert_ghost_quads(&m, fe)?;
            c.update_pairs()
        })?;
        // the cleanup of the ghost elements the synchroniser added
        let relevant = c.relevant_ghost_elements();
        let to_erase = {
            let m = mesh_h.read()?;
            let mut out = Vec::new();
            for ty in m.element_types(TypeFilter {
                spatial_dimension: Some(m.spatial_dimension()),
                ghost_type: GhostType::Ghost,
                kind: None,
            }) {
                let counters = m.ghosts_counters().get(ty, GhostType::Ghost).ok();
                let protected = original.get(&ty).copied().unwrap_or(0);
                for g in protected..m.nb_element(ty, GhostType::Ghost) {
                    let e = Element::new(ty, akantu_core::try_idx(g)?, GhostType::Ghost);
                    if relevant.contains(&e) {
                        continue;
                    }
                    let n = counters
                        .and_then(|a| a.as_slice().get(g).copied())
                        .unwrap_or(1)
                        .max(1);
                    for _ in 0..n {
                        out.push(e);
                    }
                }
            }
            out
        };
        let mut grids = BTreeMap::new();
        if let Some(s) = c.synchronizer() {
            grids.insert("max".to_owned(), s.clone());
        }
        let mut hook = CleanupHook {
            grids: &grids,
            staged: None,
        };
        let committed = {
            let (token, mut parts) = self.split_for_mutation();
            let handler: &mut dyn MeshEventHandler = &mut parts;
            mesh_h.mutate(
                &mut [(token, handler)],
                |tx| {
                    pad_ghost_data(tx)?;
                    tx.erase_elements(MeshPart::Main, &to_erase)
                },
                Some(&mut hook),
            )?
        };
        if let Some(mut staged) = hook.staged
            && let (Some(s), Some(new)) = (c.synchronizer_mut(), staged.remove("max"))
        {
            *s = new;
        }
        let numbering = committed
            .events
            .iter()
            .find_map(|(part, ev)| match (part, ev) {
                (MeshPart::Main, MeshEvent::ElementsRemoved { new_numbering, .. }) => {
                    Some(new_numbering.clone())
                }
                _ => None,
            });
        in_scope(&comm, "smm:max_criterion:finish", |_| {
            let m = mesh_h.read()?;
            let fe = self.physics.base.fems.get("")?;
            if let Some(n) = numbering {
                c.renumber_ghost_points(&n, &|ty| fe.nb_integration_points(ty, GhostType::Ghost))?;
            }
            c.finish(&m, fe)
        })?;
        Ok(c)
    }

    /// Upstream `NeighborhoodMaxCriterion::findMaxQuads` (evident intent,
    /// UD-563): the criterion internal of every law flattened over both
    /// ghost types (upstream `updateDataForNonLocalCriterion`), on a
    /// distributed mesh the `_nh_criterion` exchange at the ghost slots
    /// (collective), then the owned points maximal in their neighbourhood.
    ///
    /// # Errors
    /// Flattening, layout or communication errors.
    pub fn find_max_quads(
        &mut self,
        criterion: &NeighborhoodMaxCriterion,
    ) -> Result<Vec<QuadPoint>> {
        let mesh_h = self.physics.base.mesh.clone();
        let (values, comm) = {
            let m = mesh_h.read()?;
            let p = &mut self.physics;
            let (laws, services) = p.split(&m, &self.solver.dof_manager);
            let ctx = LawContext {
                phase: LawPhase::Init,
                services: &services,
            };
            let cb = SmmNonLocalCallback { laws, ctx: &ctx };
            let mut flat = ElementTypeMapArray::new(criterion.criterion_id().to_owned());
            for g in GhostType::ALL {
                cb.update_local_internal(criterion.criterion_id(), g, &mut flat)?;
            }
            let layout = criterion.layout();
            let mut v = vec![0.; layout.len()];
            for (ty, g, off, n) in layout.blocks() {
                if let Ok(a) = flat.get(ty, g) {
                    let k = n.min(a.len());
                    v[off..off + k].copy_from_slice(&a.as_slice()[..k]);
                }
            }
            (v, m.communicator().clone())
        };
        let mut values = values;
        if criterion.synchronizer().is_some() {
            in_scope(&comm, "smm:max_criterion:find", |ph| {
                criterion.find_max_quads_in(ph, &mut values)
            })
        } else {
            criterion.find_max_quads(&values)
        }
    }
}
