// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/model/solid_mechanics/
// solid_mechanics_model_cohesive/solid_mechanics_model_cohesive{.hh,.cc,
// _inline_impl.hh} (serial parts; the distributed parts of
// solid_mechanics_model_cohesive_parallel.cc are in the parallel layer).

//! [`SolidMechanicsModelCohesive`] (COH-001…007, 009, 013): a
//! [`SolidMechanicsModel`] whose law handler also holds cohesive laws, plus
//! the facet machinery of dynamic (extrinsic) insertion.
//!
//! # Structure
//!
//! `SolidMechanicsModelCohesive { smm, ext }`: `smm` is the solid model
//! (bulk and cohesive laws share its numbering, upstream subclassing); the
//! [`CohesiveExtension`] owns the inserter, the facet FE engine
//! (`FacetsFEEngine`, order of the cohesive types), `facet_stress`,
//! `facet_material`, the facet tangents and the bulk laws' stress
//! interpolation to the facet quadrature points. The model's solver
//! callback is the SMM physics wrapped by [`CohesiveCallback`], which
//! computes the cohesive tractions before every residual assembly (upstream
//! `SolidMechanicsModelCohesive::assembleInternalForces`), refreshes the
//! current positions before the tangent and computes Cauchy stresses after
//! converged finite-deformation steps.
//!
//! # Insertion (performance design)
//!
//! `check_cohesive_stress` = stress interpolation (precomputed matrices,
//! reused buffers; no copy of nodes, connectivities, shapes or quadrature
//! points), the laws' checks over their facet filters (facets flagged in
//! `check_facets` only), and one `SharedMesh::mutate` transaction when a
//! facet must open (row-journaled facet adjacency; no node→element map is
//! kept or rebuilt). Parallel hooks: shared-facet agreement and the
//! distributed hook belong to the parallel layer; on more than one rank
//! the model refuses with `Error::Unsupported`.

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use akantu_comm::phase::Phases;
use akantu_core::enums::{EventPriority, MatrixType, TimeStepSolverType};
use akantu_core::error::if_present;
use akantu_core::{
    Array, Element, ElementKind, ElementType, ElementTypeMapArray, Error, GhostType, Idx, Real,
    Result, TypeFilter,
};
use akantu_fe::FeEngine;
use akantu_fe::engine::ElementalFieldInterpolation;
use akantu_fe::shape::{IntegrationOrders, ShapeFamily};
use akantu_mesh::utils::{ELEMENT_TO_SUBELEMENT, SUBELEMENT_TO_ELEMENT};
use akantu_mesh::{Mesh, MeshEvent, MeshEventHandler, SharedMesh};
use akantu_model::ModelOptions;
use akantu_model::constitutive_law::{
    ConstitutiveLawSelector, LawIndexSelector, LawPhase, SelectorChain, SelectorContext,
};
use akantu_model::constitutive_laws_handler::ConstitutiveLawsHandler;
use akantu_parser::{Parser, ParserType};
use akantu_solid::model::{DOF_ID, SmmEventParts, SolidMechanicsPhysics};
use akantu_solid::{Material, SolidMechanicsModel};
use akantu_solver::{DofManager, ModelSolverOptions, SolverCallback};

use crate::inserter::{CohesiveElementInserter, InsertionResult};
use crate::laws::{CohesiveLaw, InsertionInputs};
use crate::material::{
    COHESIVE_FE_ENGINE, MaterialCohesive, nb_quad_facet, register_cohesive_materials,
};
use crate::selector::DefaultMaterialCohesiveSelector;
use crate::vecops::tangents_of;

/// Default model id (upstream `"solid_mechanics_model_cohesive"`).
pub const DEFAULT_ID: &str = "solid_mechanics_model_cohesive";

/// Name of the facet FE engine (upstream `"FacetsFEEngine"`).
pub const FACETS_FE_ENGINE: &str = "FacetsFEEngine";

/// Timings of the insertion check (per call of
/// [`SolidMechanicsModelCohesive::check_cohesive_stress`]).
#[derive(Clone, Debug, Default)]
pub struct InsertionStats {
    /// Number of calls.
    pub calls: usize,
    /// Total time spent in the stress interpolation (s).
    pub interpolation: f64,
    /// Total time spent in the laws' checks (s).
    pub checks: f64,
    /// Total time spent in insertion transactions (s).
    pub insertion: f64,
    /// Inserted cohesive elements.
    pub inserted: usize,
}

/// The cohesive part of the model state (upstream members of
/// `SolidMechanicsModelCohesive`).
#[derive(Debug)]
pub struct CohesiveExtension {
    /// Cohesive element inserter.
    pub inserter: CohesiveElementInserter,
    /// `FacetsFEEngine` (facet mesh, order of the cohesive types). Boxed:
    /// the engine is 20.9 kB inline, a third of the model, which every
    /// unoptimised constructor of the model and of the couplers copies on
    /// the stack.
    pub facet_fe: Box<FeEngine>,
    /// Dynamic insertion.
    pub is_extrinsic: bool,
    /// Stresses of both sides at the facet quadrature points
    /// (`2·dim²` components).
    pub facet_stress: ElementTypeMapArray<Real>,
    /// Material to use on each facet.
    pub facet_material: ElementTypeMapArray<Idx>,
    /// Facet tangents (`dim·(dim − 1)` components).
    pub tangents: ElementTypeMapArray<Real>,
    stress_interpolation: BTreeMap<usize, ElementalFieldInterpolation>,
    by_elem_buffer: ElementTypeMapArray<Real>,
    /// Copy plans of [`SolidMechanicsModelCohesive::interpolate_stress`]
    /// per bulk law (performance).
    stress_plans: BTreeMap<usize, StressCopyPlan>,
    selector_state: Arc<std::sync::RwLock<crate::selector::CohesiveSelectorState>>,
    custom_selector: bool,
    dim: usize,
    /// Insertion-check timings.
    pub stats: InsertionStats,
    /// Upstream `Math::getTolerance()` (a process global upstream).
    pub tolerance: Real,
    /// Upstream-defect switches (default: corrected behaviour). Set before
    /// `init_full`: the model-level switches are read by the model and the
    /// set is merged into every cohesive law at initialisation
    /// ([`crate::CohesiveUpstreamCompat`]).
    pub compat: crate::material::CohesiveUpstreamCompat,
    /// The symmetry policy of the DOF manager's matrices (UD-034; set by
    /// [`SolidMechanicsModelCohesive::set_symmetry_policy`]).
    pub symmetry_policy: akantu_linalg::SymmetryPolicy,
    /// The inserter's insertion flags while the runtime laws check the
    /// insertion (between [`SolidMechanicsModelCohesive::check_cohesive_stress_prepare`]
    /// and its finish or abort): **moved** into exportable storage, so that
    /// a binding hands the checks zero-copy writable views (D-047). `None`
    /// otherwise (the flags are the inserter's). Boxed: the map is 4.3 kB
    /// inline, which every unoptimised constructor of the model and of the
    /// couplers copies on the stack.
    pub staged_insertion_facets: Option<Box<akantu_core::SharedElementTypeMapArray<bool>>>,
    /// The open insertion check (sequence number of its
    /// [`InsertionCheck`]) and the last number given
    /// (only the open check finishes or aborts; a stale one changes
    /// nothing).
    insertion_check: (Option<u64>, u64),
    /// Synchronizers of a distributed mesh (upstream `cohesive_synchronizer`,
    /// `facet_stress_synchronizer`; [`crate::parallel`]); `None` when the
    /// mesh is not distributed.
    pub parallel: Option<crate::parallel::CohesiveSynchronizers>,
}

/// Where [`SolidMechanicsModelCohesive::interpolate_stress`] copies the
/// interpolated stresses of one bulk law: for each (bulk type, facet type,
/// facet ghost) block, the source offset in the by-element buffer and the
/// destination offset in `facet_stress` of the first quadrature point of
/// every (element, local facet) pair, in upstream's loop order (filter
/// order, then local facet), then ordered by destination
/// ([`sort_by_destination`]). Precomputed so that the per-step copy reads
/// neither the facet adjacency (`element_to_subelement` lists) nor the
/// type maps and writes sequentially (performance); rebuilt
/// when the mesh revision or the law's filter changes. Blocks of different
/// facet types write to different arrays, so grouping by facet type keeps
/// every value.
#[derive(Clone, Debug, Default, PartialEq)]
struct StressCopyPlan {
    revision: u64,
    /// Per bulk type: filter length and an order-sensitive digest.
    filter_key: Vec<(ElementType, usize, u64)>,
    blocks: Vec<StressCopyBlock>,
    /// The fused form of every block ([`build_fused`]), once built for
    /// these blocks (`None` for a block without it).
    fused: Option<Vec<Option<Vec<FusedPair>>>>,
}

#[derive(Clone, Debug, PartialEq)]
struct StressCopyBlock {
    bulk: ElementType,
    facet: ElementType,
    facet_ghost: GhostType,
    /// Quadrature points per facet.
    nq: usize,
    /// Interpolation points per bulk element (the by-element buffer's).
    nip: usize,
    /// `(source, destination)` offsets of the first quadrature point,
    /// ordered by destination.
    pairs: Vec<(usize, usize)>,
    /// Exclusive bounds the offsets need (checked before copying).
    src_end: usize,
    dst_end: usize,
}

/// One `(source, destination)` pair of a [`StressCopyBlock`] in the fused
/// form of [`SolidMechanicsModelCohesive::interpolate_stress`]: instead of
/// interpolating every element into the by-element buffer and copying the
/// buffer's tuples, the value of the pair's interpolation points is
/// computed directly into `facet_stress` from the element's stress tuple
/// ([`akantu_fe::PointInterpolation::value`], the operations the
/// interpolation performs for that element and point). The element's tuple
/// and slot are those the interpolation would have left in the buffer: the
/// **last** position of the element in the law's filter (later positions
/// overwrite earlier ones) and the interpolation slot of that position.
#[derive(Clone, Copy, Debug, PartialEq)]
struct FusedPair {
    /// Offset of the element's stress tuple in the law's stress block.
    field: u32,
    /// Interpolation slot of the element.
    slot: u32,
    /// Interpolation point of the facet's first quadrature point.
    point: u32,
    /// Destination offset (the pair's).
    dst: u32,
}

/// The fused form of every block of `plan` for the law filter `filter`
/// and its stress interpolation `interp`: `None` for a block whose bulk
/// type has no [`akantu_fe::PointInterpolation`] (more than one quadrature
/// point, other routes), whose filter does not match the interpolation, or
/// whose offsets do not fit `u32`; such a block makes the law take the
/// general path.
fn build_fused(
    plan: &StressCopyPlan,
    filter: &ElementTypeMapArray<Idx>,
    interp: &ElementalFieldInterpolation,
    stress_size: usize,
) -> Vec<Option<Vec<FusedPair>>> {
    plan.blocks
        .iter()
        .map(|block| fuse_block(block, filter, interp, stress_size))
        .collect()
}

fn fuse_block(
    block: &StressCopyBlock,
    filter: &ElementTypeMapArray<Idx>,
    interp: &ElementalFieldInterpolation,
    stress_size: usize,
) -> Option<Vec<FusedPair>> {
    let view = interp
        .point_interpolation(block.bulk, GhostType::NotGhost, stress_size)
        .ok()??;
    let nip = view.nb_interpolation_points();
    if nip != block.nip || stress_size == 0 || block.nq == 0 {
        return None;
    }
    let ids: Vec<usize> = filter
        .get(block.bulk, GhostType::NotGhost)
        .ok()?
        .as_slice()
        .iter()
        .map(|&e| usize::try_from(e).ok())
        .collect::<Option<_>>()?;
    let slots = view.slots(&ids).ok()?;
    let mut last = vec![usize::MAX; ids.iter().max().map_or(0, |&m| m + 1)];
    for (pos, &e) in ids.iter().enumerate() {
        last[e] = pos;
    }
    let u = |x: usize| u32::try_from(x).ok();
    let mut out = Vec::with_capacity(block.pairs.len());
    for &(src, dst) in &block.pairs {
        if src % stress_size != 0 {
            return None;
        }
        let p = src / stress_size;
        let (element, point) = (p / nip, p % nip);
        let pos = *last.get(element)?;
        if pos == usize::MAX || point + block.nq > nip {
            return None;
        }
        out.push(FusedPair {
            field: u(pos.checked_mul(stress_size)?)?,
            slot: u(slots[pos])?,
            point: u(point)?,
            dst: u(dst)?,
        });
    }
    Some(out)
}

/// Length and digest of a law's not-ghost filter per type (plan key).
fn filter_key(
    filter: &ElementTypeMapArray<Idx>,
    types: &[ElementType],
) -> Result<Vec<(ElementType, usize, u64)>> {
    let mut key = Vec::with_capacity(types.len());
    for &ty in types {
        let elems = filter.get(ty, GhostType::NotGhost)?.as_slice();
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        for &e in elems {
            h = (h ^ u64::from(e.cast_unsigned())).wrapping_mul(0x0100_0000_01b3);
        }
        key.push((ty, elems.len(), h));
    }
    Ok(key)
}

/// Copy `nq` stress tuples of `n` values: source tuples consecutive,
/// destination tuples `2·n` apart (the two sides interleave).
#[inline]
fn copy_points<const N: usize>(by: &[Real], rs: &mut [Real], src: usize, dst: usize, nq: usize) {
    for q in 0..nq {
        let (s, d) = (src + q * N, dst + 2 * q * N);
        rs[d..d + N].copy_from_slice(&by[s..s + N]);
    }
}

#[inline]
fn copy_points_dyn(by: &[Real], rs: &mut [Real], src: usize, dst: usize, nq: usize, n: usize) {
    for q in 0..nq {
        let (s, d) = (src + q * n, dst + 2 * q * n);
        rs[d..d + n].copy_from_slice(&by[s..s + n]);
    }
}

/// Build the [`StressCopyPlan`] of one bulk law (upstream
/// `interpolateStress` loop: for every element of the filter and every
/// facet of it, the side is 1 unless the element is the facet's first
/// neighbour in `element_to_subelement`).
fn build_stress_plan(
    m: &Mesh,
    fm: &Mesh,
    filter: &ElementTypeMapArray<Idx>,
    types: &[ElementType],
    by_elem_buffer: &ElementTypeMapArray<Real>,
    stress_size: usize,
) -> Result<StressCopyPlan> {
    let s2e = fm.data().element_map(SUBELEMENT_TO_ELEMENT)?;
    let e2s = fm.data().element_lists(ELEMENT_TO_SUBELEMENT)?;
    let mut plan = StressCopyPlan::default();
    for &ty in types {
        let elems = filter.get(ty, GhostType::NotGhost)?.as_slice();
        if elems.is_empty() {
            continue;
        }
        let ne_full = m.nb_element(ty, GhostType::NotGhost);
        let by_len = by_elem_buffer.get(ty, GhostType::NotGhost)?.len();
        let nip = by_len / ne_full.max(1);
        let f2e = s2e.get(ty, GhostType::NotGhost)?;
        let nfe = f2e.nb_component();
        let nq = nip / nfe.max(1);
        let first_block = plan.blocks.len();
        // last facet-type array looked up (one type per bulk type normally)
        let mut cached: Option<(ElementType, GhostType, &akantu_core::Array<Vec<Element>>)> = None;
        for &ge in elems {
            let ge_u = akantu_core::try_to_usize(ge)?;
            let element = Element::new(ty, ge, GhostType::NotGhost);
            let row = f2e.get_tuple(ge_u)?;
            for (f, facet) in row.iter().enumerate() {
                let lists = match cached {
                    Some((t, g, a)) if t == facet.ty() && g == facet.ghost_type() => a,
                    _ => {
                        let a = e2s.get(facet.ty(), facet.ghost_type())?;
                        cached = Some((facet.ty(), facet.ghost_type(), a));
                        a
                    }
                };
                let fi = facet.try_idx().ok_or_else(|| {
                    Error::invalid("interpolateStress: a bulk element without facet")
                })?;
                let neighbours = lists.get_tuple(fi)?;
                let second = usize::from(neighbours[0].first() != Some(&element));
                let src = (ge_u * nip + f * nq) * stress_size;
                let dst = (fi * nq * 2 + second) * stress_size;
                let block = match plan.blocks[first_block..]
                    .iter()
                    .position(|b| b.facet == facet.ty() && b.facet_ghost == facet.ghost_type())
                {
                    Some(k) => &mut plan.blocks[first_block + k],
                    None => {
                        plan.blocks.push(StressCopyBlock {
                            bulk: ty,
                            facet: facet.ty(),
                            facet_ghost: facet.ghost_type(),
                            nq,
                            nip,
                            pairs: Vec::with_capacity(elems.len() * nfe),
                            src_end: 0,
                            dst_end: 0,
                        });
                        let k = plan.blocks.len() - 1;
                        &mut plan.blocks[k]
                    }
                };
                block.pairs.push((src, dst));
                if nq > 0 {
                    block.src_end = block.src_end.max(src + nq * stress_size);
                    block.dst_end = block.dst_end.max(dst + (2 * (nq - 1) + 1) * stress_size);
                }
            }
        }
    }
    for block in &mut plan.blocks {
        sort_by_destination(&mut block.pairs, stress_size);
    }
    Ok(plan)
}

/// The fused form of [`SolidMechanicsModelCohesive::interpolate_stress`]
/// for one bulk law ([`FusedPair`]): every destination of the plan written
/// from the law's stress tuple of its element, in the plan's order (the
/// values the interpolation and the copy write, bit for bit). `Ok(false)`,
/// nothing written, when a block has no fused form or the law's stress,
/// filter or `facet_stress` do not have the sizes the plan was built for:
/// the general path then runs (and reports what it reports).
fn interpolate_fused(
    plan: &StressCopyPlan,
    stress: &akantu_core::SharedElementTypeMapArray<Real>,
    filter: &ElementTypeMapArray<Idx>,
    interp: &ElementalFieldInterpolation,
    facet_stress: &mut ElementTypeMapArray<Real>,
    stress_size: usize,
) -> Result<bool> {
    let Some(fused) = &plan.fused else {
        return Ok(false);
    };
    if fused.len() != plan.blocks.len() || fused.iter().any(Option::is_none) {
        return Ok(false);
    }
    let g = GhostType::NotGhost;
    for block in &plan.blocks {
        let Ok(f) = stress.get(block.bulk, g) else {
            return Ok(false);
        };
        let n = filter.get(block.bulk, g).map_or(usize::MAX, Array::len);
        if f.nb_component() != stress_size || f.len() != n {
            return Ok(false);
        }
        match facet_stress.get(block.facet, block.facet_ghost) {
            Ok(a) if a.as_slice().len() >= block.dst_end => {}
            _ => return Ok(false),
        }
        if interp
            .point_interpolation(block.bulk, g, stress_size)?
            .is_none()
        {
            return Ok(false);
        }
    }
    for (block, pairs) in plan.blocks.iter().zip(fused) {
        let (Some(pairs), Some(view)) = (
            pairs,
            interp.point_interpolation(block.bulk, g, stress_size)?,
        ) else {
            return Err(Error::invalid(
                "interpolateStress: fused plan changed while in use",
            ));
        };
        let f = stress.get(block.bulk, g)?;
        let fs = f.as_slice();
        let rs = facet_stress
            .get_mut(block.facet, block.facet_ghost)?
            .as_mut_slice();
        let nq = block.nq;
        for p in pairs {
            let src = &fs[p.field as usize..p.field as usize + stress_size];
            for q in 0..nq {
                let d = p.dst as usize + 2 * q * stress_size;
                view.value(
                    p.slot as usize,
                    p.point as usize + q,
                    src,
                    &mut rs[d..d + stress_size],
                );
            }
        }
    }
    Ok(true)
}

/// Rebuild `plan` when it was never built, the bulk mesh revision changed
/// (an insertion, any committed mesh transaction or set-up operation) or the
/// law's filter changed (length or order-sensitive digest per type); returns
/// whether it was rebuilt. Pinned by `perf_tests`.
fn update_stress_plan(
    plan: &mut StressCopyPlan,
    m: &Mesh,
    fm: &Mesh,
    filter: &ElementTypeMapArray<Idx>,
    types: &[ElementType],
    by_elem_buffer: &ElementTypeMapArray<Real>,
    stress_size: usize,
) -> Result<bool> {
    let key = filter_key(filter, types)?;
    if !plan.blocks.is_empty() && plan.revision == m.revision() && plan.filter_key == key {
        return Ok(false);
    }
    *plan = build_stress_plan(m, fm, filter, types, by_elem_buffer, stress_size)?;
    plan.revision = m.revision();
    plan.filter_key = key;
    Ok(true)
}

/// Order the copies by destination (sequential writes into `facet_stress`,
/// gathered reads: the scattered writes in element order were the
/// memory-bound part of `interpolateStress`). A destination
/// offset fixes the facet and the side, so pairs with equal destinations
/// write the same tuples and overwrite each other: only the last one in
/// upstream's order is kept (the stable sort keeps equal destinations in
/// that order), which leaves exactly the values upstream's loop leaves.
/// Pairs with different destinations write disjoint tuples.
///
/// Linear time when every destination is a multiple of `unit` (the stress
/// size: the plan's destinations are tuple offsets) and the destinations
/// span at most a few slots per pair: a table of the last pair written to
/// each destination, read in destination order — the same pairs in the
/// same order as the stable sort and deduplication of the general case.
fn sort_by_destination(pairs: &mut Vec<(usize, usize)>, unit: usize) {
    let max = pairs.iter().map(|&(_, d)| d).max().unwrap_or(0);
    let slots = max / unit.max(1) + 1;
    if unit > 0 && pairs.iter().all(|&(_, d)| d % unit == 0) && slots <= 4 * pairs.len() + 1024 {
        let mut last = vec![usize::MAX; slots];
        for (k, &(_, d)) in pairs.iter().enumerate() {
            last[d / unit] = k;
        }
        *pairs = last
            .iter()
            .filter(|&&k| k != usize::MAX)
            .map(|&k| pairs[k])
            .collect();
        return;
    }
    pairs.sort_by_key(|&(_, dst)| dst);
    let mut out: Vec<(usize, usize)> = Vec::with_capacity(pairs.len());
    for &(src, dst) in pairs.iter() {
        match out.last_mut() {
            Some(last) if last.1 == dst => *last = (src, dst),
            _ => out.push((src, dst)),
        }
    }
    *pairs = out;
}

/// Upstream `facet_stress.initialize(FacetsFEEngine, 2·dim², dim − 1)`:
/// one tuple per facet quadrature point (existing values kept).
fn resize_facet_stress(fs: &mut ElementTypeMapArray<Real>, fm: &Mesh, dim: usize) -> Result<()> {
    for ghost in GhostType::ALL {
        for ty in fm.element_types(TypeFilter {
            spatial_dimension: Some(dim - 1),
            ghost_type: ghost,
            kind: Some(ElementKind::Regular),
        }) {
            let n = fm.nb_element(ty, ghost) * nb_quad_facet(ty)?;
            fs.alloc(n, 2 * dim * dim, ty, ghost, 0.0)?;
        }
    }
    Ok(())
}

/// The cohesive model's mesh-event handler: the solid model's, then
/// upstream's `SolidMechanicsModelCohesive::onElementsAdded` (facet stress
/// resized) and `onNodesAdded` (values of the doubled nodes copied; by
/// default also the recorded Neumann loads moved to the copies their
/// boundary elements use, D-105 / UD-1280). The
/// solid model's parts are shared (`RefCell`) with the insertion
/// transaction's change, which stages the law selection of the new cohesive
/// elements before commit; the change runs before any
/// dispatch, so the two never borrow at the same time.
struct CohesiveEventHandler<'a, 'b> {
    smm: &'a RefCell<SmmEventParts<'b>>,
    facet_stress: &'a mut ElementTypeMapArray<Real>,
    is_extrinsic: bool,
    dim: usize,
    /// Laws of the new ghost cohesive elements received by the distributed
    /// hook, handed to the solid model's `ElementsAdded`
    /// dispatch.
    ghost_laws: Option<crate::parallel::GhostLaws>,
    /// D-105 / UD-1280: after a doubling, move the recorded Neumann loads to
    /// the copies their boundary elements use (`false`: the compat switch
    /// `doubled_node_keeps_external_force`, upstream's placement).
    move_external_force: bool,
}

/// The shared solid-model parts, mutably (an error instead of a panic if
/// they were borrowed: a re-entrant dispatch).
fn parts_mut<'r, 'b>(
    smm: &'r RefCell<SmmEventParts<'b>>,
) -> Result<std::cell::RefMut<'r, SmmEventParts<'b>>> {
    smm.try_borrow_mut()
        .map_err(|_| Error::invalid("cohesive mesh-event handler re-entered"))
}

impl CohesiveEventHandler<'_, '_> {
    /// Before the solid model's `ElementsAdded` at the model level: the
    /// ghost laws received from the owners (upstream's
    /// `_constitutive_law_id` unpack position).
    fn stage_ghost_laws(&mut self, event: &MeshEvent, level: EventPriority) -> Result<()> {
        if level != EventPriority::MODEL || !matches!(event, MeshEvent::ElementsAdded { .. }) {
            return Ok(());
        }
        if let Some(g) = &self.ghost_laws {
            let list = std::mem::take(
                &mut *g
                    .lock()
                    .map_err(|_| Error::invalid("staged ghost laws poisoned"))?,
            );
            parts_mut(self.smm)?.physics.staged_ghost_laws = list;
        }
        Ok(())
    }

    fn extra(&mut self, mesh: &Mesh, event: &MeshEvent, level: EventPriority) -> Result<()> {
        if level != EventPriority::MODEL {
            return Ok(());
        }
        match event {
            MeshEvent::ElementsAdded { .. } => {
                if self.is_extrinsic
                    && let Some(fm) = mesh.mesh_facets()
                {
                    resize_facet_stress(self.facet_stress, fm, self.dim)?;
                }
                Ok(())
            }
            MeshEvent::NodesAdded {
                nodes,
                old_nodes: Some(old),
                ..
            } => {
                let mut parts = parts_mut(self.smm)?;
                copy_doubled_nodes(&mut parts.solver.dof_manager, nodes, old, self.dim)?;
                // D-105 / UD-1280: the recorded Neumann load follows its
                // boundary elements (rank-local, every copy of the rank)
                if self.move_external_force {
                    parts
                        .physics
                        .move_doubled_node_external_force(mesh, nodes, old)?;
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }
}

/// Upstream `onNodesAdded` for a `CohesiveNewNodesEvent`: displacement,
/// blocked flags, velocity, acceleration, previous displacement, increment
/// and the DOF manager's solution of each new node copied from its old
/// node (the current positions are recomputed from `X + u`).
fn copy_doubled_nodes(dofs: &mut DofManager, nodes: &[Idx], old: &[Idx], dim: usize) -> Result<()> {
    let Ok(fam) = dofs.family_mut(DOF_ID) else {
        return Ok(());
    };
    fn copy<T: Copy>(s: &mut [T], nodes: &[Idx], old: &[Idx], dim: usize) -> Result<()> {
        for (&n, &o) in nodes.iter().zip(old) {
            let (n, o) = (akantu_core::try_to_usize(n)?, akantu_core::try_to_usize(o)?);
            if (n + 1) * dim > s.len() || (o + 1) * dim > s.len() {
                return Err(Error::OutOfRange {
                    what: "doubled node (nodal array not resized)",
                    index: n.max(o),
                    len: s.len() / dim.max(1),
                });
            }
            s.copy_within(o * dim..(o + 1) * dim, n * dim);
        }
        Ok(())
    }
    copy(fam.dofs.write()?.as_mut_slice(), nodes, old, dim)?;
    if let Some(b) = fam.blocked.as_mut() {
        copy(b.write()?.as_mut_slice(), nodes, old, dim)?;
    }
    for d in fam.derivatives.values_mut() {
        copy(d.write()?.as_mut_slice(), nodes, old, dim)?;
    }
    if let Some(p) = fam.previous.as_mut() {
        copy(p.write()?.as_mut_slice(), nodes, old, dim)?;
    }
    if let Some(i) = fam.increment.as_mut() {
        copy(i.write()?.as_mut_slice(), nodes, old, dim)?;
    }
    if fam.solution.len() == fam.dofs.len() {
        copy(fam.solution.as_mut_slice(), nodes, old, dim)?;
    }
    Ok(())
}

impl MeshEventHandler for CohesiveEventHandler<'_, '_> {
    fn event_priority(&self) -> EventPriority {
        EventPriority::MODEL
    }

    fn event_priorities(&self) -> Vec<EventPriority> {
        self.smm
            .try_borrow()
            .map_or_else(|_| vec![EventPriority::MODEL], |p| p.event_priorities())
    }

    fn on_mesh_event(&mut self, mesh: &Mesh, event: &MeshEvent) -> Result<()> {
        self.on_mesh_event_at(mesh, event, EventPriority::MODEL)
    }

    /// Forwarded to the solid model's parts like the dispatch;
    /// the cohesive additions (facet stress resize, copy of the
    /// doubled nodes' values) cannot refuse an event. A borrowed `RefCell`
    /// (re-entrant use) refuses the event, before the commit.
    fn check_mesh_event(&self, mesh: &Mesh, event: &MeshEvent) -> Result<()> {
        let parts = self
            .smm
            .try_borrow()
            .map_err(|_| Error::invalid("cohesive mesh-event handler re-entered"))?;
        parts.check_mesh_event(mesh, event)?;
        // the ghost laws staged by the distributed hook are
        // consumed by the dispatch: every refusal is stated here, before the
        // commit (D-068) — each a new ghost element of the event with an
        // existing law
        if let (Some(g), MeshEvent::ElementsAdded { elements, .. }) = (&self.ghost_laws, event) {
            let staged = g
                .lock()
                .map_err(|_| Error::invalid("staged ghost laws poisoned"))?;
            if staged.is_empty() {
                return Ok(());
            }
            let new: std::collections::HashSet<Element> = elements.iter().copied().collect();
            let nb_laws = parts.physics.materials.nb_laws();
            for &(e, law) in staged.iter() {
                if e.ghost_type() != GhostType::Ghost || !new.contains(&e) || law >= nb_laws {
                    return Err(Error::invalid(format!(
                        "staged ghost law {law} for {e}: not a new ghost element of the event \
                         or no such law ({nb_laws} laws)"
                    )));
                }
            }
        }
        Ok(())
    }

    fn on_mesh_event_at(
        &mut self,
        mesh: &Mesh,
        event: &MeshEvent,
        level: EventPriority,
    ) -> Result<()> {
        self.stage_ghost_laws(event, level)?;
        parts_mut(self.smm)?.on_mesh_event_at(mesh, event, level)?;
        self.extra(mesh, event, level)
    }

    fn on_mesh_event_phased(
        &mut self,
        mesh: &Mesh,
        event: &MeshEvent,
        level: EventPriority,
        phases: &mut Phases,
    ) -> Result<()> {
        self.stage_ghost_laws(event, level)?;
        parts_mut(self.smm)?.on_mesh_event_phased(mesh, event, level, phases)?;
        self.extra(mesh, event, level)
    }
}

/// Upstream `SolidMechanicsModelCohesive` (serial).
///
/// # Size
/// The type is large: about 63 kB (63,200 bytes) by value
/// (63,184 bytes were measured). Box it when several are alive
/// in one frame on a spawned thread in debug builds: with unoptimised frames
/// of that size, several models on a small thread stack (e.g. the 2 MiB of a
/// libtest worker thread) overflow the stack.
#[derive(Debug)]
pub struct SolidMechanicsModelCohesive {
    /// The solid mechanics model (bulk and cohesive laws).
    pub smm: SolidMechanicsModel,
    /// The cohesive part.
    pub ext: CohesiveExtension,
}

/// Downcast a law of the handler to a cohesive material.
fn as_cohesive(law: &mut dyn Material) -> Option<&mut MaterialCohesive> {
    law.as_any_mut()
        .and_then(|a| a.downcast_mut::<MaterialCohesive>())
}

fn is_cohesive(law: &dyn Material) -> bool {
    law.as_any().is_some_and(|a| a.is::<MaterialCohesive>())
}

/// Solver callback of the cohesive model: the solid physics plus the
/// cohesive overrides (see the module documentation).
#[derive(Debug)]
pub struct CohesiveCallback<'a> {
    /// The solid physics.
    pub physics: &'a mut SolidMechanicsPhysics,
    /// The cohesive synchronizer of a distributed mesh: the not-ghost
    /// tractions are sent to the ghost copies after they are computed
    /// (`_smm_stress`).
    pub cohesive_sync: Option<&'a akantu_comm::Synchronizer<Element>>,
    /// `before_solve_step` ran and `after_solve_step` has not run since
    /// (a step the solver reports converged with this
    /// still set skipped `after_solve_step`, UD-010 legacy hooks).
    open_step: bool,
}

impl<'a> CohesiveCallback<'a> {
    /// The cohesive callback of `physics`.
    pub fn new(physics: &'a mut SolidMechanicsPhysics) -> Self {
        Self {
            physics,
            cohesive_sync: None,
            open_step: false,
        }
    }

    /// The cohesive callback of `physics` on a distributed mesh with the
    /// cohesive synchronizer `sync`.
    pub fn with_synchronizer(
        physics: &'a mut SolidMechanicsPhysics,
        sync: Option<&'a akantu_comm::Synchronizer<Element>>,
    ) -> Self {
        Self {
            physics,
            cohesive_sync: sync,
            open_step: false,
        }
    }

    /// `true` while a solve step is open: `before_solve_step` ran, and
    /// `after_solve_step` did not run after it.
    #[must_use]
    pub fn step_open(&self) -> bool {
        self.open_step
    }
}

/// Commit the maximal openings of every cohesive law
/// ([`MaterialCohesive::commit_maxima`]).
fn commit_cohesive_maxima(physics: &mut SolidMechanicsPhysics) -> Result<()> {
    for law in physics.materials.laws_mut() {
        if let Some(m) = as_cohesive(law.as_mut()) {
            m.commit_maxima()?;
        }
    }
    Ok(())
}

/// Upstream `SolidMechanicsModelCohesive::assembleInternalForces` prologue:
/// every cohesive law computes its tractions on the not-ghost elements
/// (normals from the current positions, openings from the displacement).
///
/// # Errors
/// Unallocated fields, FE engine or law errors.
pub fn compute_cohesive_tractions(
    physics: &mut SolidMechanicsPhysics,
    dofs: &DofManager,
) -> Result<()> {
    if cohesive_laws_have_elements(physics, GhostType::NotGhost) {
        physics.refresh_current_position(dofs)?;
    }
    let explicit = physics.base.is_default_solver_explicit();
    physics.with_laws(dofs, LawPhase::Stress, |laws, ctx| {
        let fe = ctx.services.fe_engine(COHESIVE_FE_ENGINE)?;
        let mesh = ctx.services.mesh();
        for law in laws.laws_mut() {
            if let Some(m) = as_cohesive(law.as_mut()) {
                m.core.default_solver_explicit = explicit;
                if m.is_foreign() {
                    // a runtime law calls back into foreign code: the
                    // guards on the exported nodal arrays end first
                    {
                        let cp = ctx.services.nodal_field("current_position")?;
                        let u = ctx.services.nodal_field("displacement")?;
                        m.compute_kinematics(fe, mesh, &cp, &u, GhostType::NotGhost)?;
                    }
                    m.compute_foreign_tractions(ctx, GhostType::NotGhost)?;
                } else {
                    let cp = ctx.services.nodal_field("current_position")?;
                    let u = ctx.services.nodal_field("displacement")?;
                    m.compute_traction_in(Some(ctx), fe, mesh, &cp, &u, GhostType::NotGhost)?;
                }
            }
        }
        Ok(())
    })
}

/// Quadrature points per cohesive type of the mesh (`CohesiveFEEngine`).
fn cohesive_nb_quad(
    physics: &SolidMechanicsPhysics,
    mesh: &Mesh,
) -> Result<BTreeMap<ElementType, usize>> {
    let fe = physics.base.fems.get(COHESIVE_FE_ENGINE)?;
    let mut out = BTreeMap::new();
    for ghost in GhostType::ALL {
        for ty in mesh.element_types(TypeFilter {
            spatial_dimension: Some(physics.base.spatial_dimension),
            ghost_type: ghost,
            kind: Some(ElementKind::Cohesive),
        }) {
            if let std::collections::btree_map::Entry::Vacant(v) = out.entry(ty) {
                v.insert(fe.nb_integration_points(ty, GhostType::NotGhost)?);
            }
        }
    }
    Ok(out)
}

/// `_smm_stress` of the cohesive elements over the cohesive synchronizer
/// `sync` through the running operation's scope (upstream
/// `asynchronousSynchronize(_smm_stress)` also covers the cohesive
/// synchronizer: the ghost cohesive elements are assembled with their
/// owner's tractions and contact tractions).
///
/// # Errors
/// Exchange errors (agreed in the operation's scope).
pub fn exchange_cohesive_tractions(
    physics: &mut SolidMechanicsPhysics,
    dofs: &mut DofManager,
    sync: &akantu_comm::Synchronizer<Element>,
) -> Result<()> {
    let mesh_h = physics.base.mesh.clone();
    let nb_quad = {
        let m = mesh_h.read()?;
        cohesive_nb_quad(physics, &m)
    };
    let dim = physics.base.spatial_dimension;
    let (nb_quad, mut ph) = match (nb_quad, dofs.phases()) {
        (Ok(q), Ok(ph)) => (q, ph),
        (Err(e), _) | (_, Err(e)) => return Err(e),
    };
    let mut acc =
        crate::parallel::TractionAccessor::tractions(&mut physics.materials, &nb_quad, dim);
    sync.synchronize_in(
        &mut ph,
        &mut acc,
        akantu_core::enums::SynchronizationTag::SmmStress,
    )
}

/// Whether a cohesive law holds cohesive elements of `ghost`. Upstream
/// `MaterialCohesive::computeTraction` and `assembleStiffnessMatrix` call
/// `model->getCurrentPosition()` only for the cohesive types of their
/// filter with elements, so the cached current positions are refreshed
/// only then (the release bookkeeping matters: upstream `initSolver`
/// resets the positions to `X` without touching the release, UD-352 /
/// D-064). Independent of the `init_solver` rule.
fn cohesive_laws_have_elements(physics: &SolidMechanicsPhysics, ghost: GhostType) -> bool {
    physics.materials.laws().iter().any(|law| {
        law.as_any()
            .and_then(|a| a.downcast_ref::<MaterialCohesive>())
            .is_some_and(|m| m.has_elements(ghost))
    })
}

impl SolverCallback for CohesiveCallback<'_> {
    fn matrix_type(&self, matrix_id: &str) -> MatrixType {
        <SolidMechanicsPhysics as SolverCallback>::matrix_type(self.physics, matrix_id)
    }

    fn assemble_matrix(&mut self, dofs: &mut DofManager, matrix_id: &str) -> Result<()> {
        if matrix_id == "K" && cohesive_laws_have_elements(self.physics, GhostType::NotGhost) {
            // upstream `getCurrentPosition()` in `computeNormal` (types
            // with elements only)
            self.physics.refresh_current_position(dofs)?;
        }
        <SolidMechanicsPhysics as SolverCallback>::assemble_matrix(self.physics, dofs, matrix_id)
    }

    fn assemble_lumped_matrix(&mut self, dofs: &mut DofManager, matrix_id: &str) -> Result<()> {
        <SolidMechanicsPhysics as SolverCallback>::assemble_lumped_matrix(
            self.physics,
            dofs,
            matrix_id,
        )
    }

    fn assemble_residual(&mut self, dofs: &mut DofManager) -> Result<()> {
        compute_cohesive_tractions(self.physics, dofs)?;
        if let Some(sync) = self.cohesive_sync {
            exchange_cohesive_tractions(self.physics, dofs, sync)?;
        }
        <SolidMechanicsPhysics as SolverCallback>::assemble_residual(self.physics, dofs)
    }

    fn assemble_residual_part(&mut self, dofs: &mut DofManager, part: &str) -> Result<()> {
        if part == "internal" {
            compute_cohesive_tractions(self.physics, dofs)?;
            if let Some(sync) = self.cohesive_sync {
                exchange_cohesive_tractions(self.physics, dofs, sync)?;
            }
        }
        <SolidMechanicsPhysics as SolverCallback>::assemble_residual_part(self.physics, dofs, part)
    }

    fn can_split_residual(&self) -> bool {
        <SolidMechanicsPhysics as SolverCallback>::can_split_residual(self.physics)
    }

    fn predictor(&mut self, dofs: &mut DofManager) -> Result<()> {
        <SolidMechanicsPhysics as SolverCallback>::predictor(self.physics, dofs)
    }

    fn corrector(&mut self, dofs: &mut DofManager) -> Result<()> {
        <SolidMechanicsPhysics as SolverCallback>::corrector(self.physics, dofs)
    }

    fn before_solve_step(&mut self, dofs: &mut DofManager) -> Result<()> {
        self.open_step = true;
        <SolidMechanicsPhysics as SolverCallback>::before_solve_step(self.physics, dofs)
    }

    /// Upstream `SolidMechanicsModelCohesive::afterSolveStep`: Cauchy
    /// stresses of finite-deformation laws after a converged step (the
    /// insertion check reads them), then the solid model's.
    fn after_solve_step(&mut self, dofs: &mut DofManager, converged: bool) -> Result<()> {
        self.open_step = false;
        if converged {
            for law in self.physics.materials.laws_mut() {
                if law.base().finite_deformation {
                    law.compute_all_cauchy_stresses(GhostType::NotGhost)?;
                }
            }
        }
        <SolidMechanicsPhysics as SolverCallback>::after_solve_step(self.physics, dofs, converged)
    }

    fn restore_last_converged_step(&mut self, dofs: &mut DofManager) -> Result<()> {
        <SolidMechanicsPhysics as SolverCallback>::restore_last_converged_step(self.physics, dofs)
    }

    fn init_solver(&mut self, dofs: &mut DofManager, ty: TimeStepSolverType) -> Result<()> {
        <SolidMechanicsPhysics as SolverCallback>::init_solver(self.physics, dofs, ty)
    }

    fn default_solver_type(&self) -> TimeStepSolverType {
        <SolidMechanicsPhysics as SolverCallback>::default_solver_type(self.physics)
    }

    fn default_solver_options(&self, ty: TimeStepSolverType) -> ModelSolverOptions {
        <SolidMechanicsPhysics as SolverCallback>::default_solver_options(self.physics, ty)
    }
}

/// A prepared insertion check of a model with runtime cohesive laws
/// ([`SolidMechanicsModelCohesive::check_cohesive_stress_prepare`]).
#[derive(Debug)]
pub struct InsertionCheck {
    marks: Vec<Vec<(ElementType, [usize; 4])>>,
    flagged_before: Vec<(ElementType, GhostType, usize)>,
    revision: u64,
    /// Sequence number ([`CohesiveExtension::insertion_check`]).
    seq: u64,
    prepared: Vec<(usize, Vec<Element>)>,
    /// The runtime laws (indices) whose `checkInsertion` the caller runs
    /// before [`SolidMechanicsModelCohesive::check_cohesive_stress_finish`].
    pub foreign_laws: Vec<usize>,
    times: (Instant, Instant, Instant),
}

impl SolidMechanicsModelCohesive {
    /// Upstream constructor with an empty parser (see
    /// [`SolidMechanicsModelCohesive::with_parser`]).
    ///
    /// # Errors
    /// See [`SolidMechanicsModelCohesive::with_parser`].
    pub fn new(mesh: SharedMesh, spatial_dimension: Option<usize>, id: &str) -> Result<Self> {
        Self::with_parser(mesh, spatial_dimension, id, Parser::default())
    }

    /// Upstream constructor (COH-001): the facet mesh (`initMeshFacets`) and
    /// the empty cohesive connectivity blocks of the bulk types' facets
    /// (upstream `initModel`, done here before the solid model registers as
    /// a mesh consumer), the solid model, the cohesive laws in its factory,
    /// `CohesiveFEEngine`, the inserter, `FacetsFEEngine` and the default
    /// cohesive material selector.
    ///
    /// On a distributed mesh the facet mesh is built
    /// collectively (`init_mesh_facets` on a distributed mesh: the facet
    /// synchronizer, ghost facets in their owner's node order with the
    /// corrected default flip, UD-122 — build the facet mesh in set-up with
    /// `init_mesh_facets_with(…, FacetFlipMode::UpstreamInverse)` before the
    /// model to reproduce upstream) and the cohesive synchronizer is split
    /// off the mesh's element synchronizer (upstream constructor).
    ///
    /// A mesh of spatial dimension 1 distributed over more than one rank is
    /// refused (UD-1141, category C, as upstream, whose constructor throws in
    /// `initMeshFacets` → `flipFacets`): `Error::Unsupported` on every rank,
    /// the mesh unchanged.
    ///
    /// # Errors
    /// A mesh that already has consumers and no facet mesh (the facet mesh
    /// is set up before any consumer exists), a distributed 1-D mesh
    /// (`Error::Unsupported`, UD-1141), or the solid model's errors.
    pub fn with_parser(
        mesh: SharedMesh,
        spatial_dimension: Option<usize>,
        id: &str,
        parser: Parser,
    ) -> Result<Self> {
        let needs_setup = {
            let m = mesh.read()?;
            m.mesh_facets().is_none() || !cohesive_blocks_missing(&m).is_empty()
        };
        if needs_setup {
            mesh.setup(|m| {
                let ev = akantu_mesh::utils::init_mesh_facets(m)?;
                let missing = cohesive_blocks_missing(m);
                let mut acc = akantu_mesh::MeshAccessor::new(m);
                for (ty, ghost) in missing {
                    acc.connectivity_mut(ty, ghost);
                }
                Ok(ev)
            })
            .map_err(|e| match e {
                // UD-1141's refusal of a distributed 1-D facet mesh (the same
                // on every rank) keeps its own error
                Error::Unsupported(ref m) if m.contains("UD-1141") => e,
                e => Error::Mesh(format!(
                    "SolidMechanicsModelCohesive needs the facet mesh before the mesh has \
                     consumers (create the cohesive model first or call \
                     akantu_mesh::utils::init_mesh_facets in set-up): {e}"
                )),
            })?;
        }
        // upstream constructor: the cohesive synchronizer split off the
        // mesh's element synchronizer (cohesive elements move); the mesh's
        // synchronizer changes only if it held cohesive elements (set-up)
        let parallel = {
            let m = mesh.read()?;
            if m.is_distributed() {
                let mut es = m.synchronizers().element.clone().unwrap_or_else(|| {
                    akantu_comm::Synchronizer::new(
                        format!("{}:element_synchronizer", m.id()),
                        m.communicator().clone(),
                    )
                });
                let had_cohesive = es
                    .communications
                    .send
                    .values()
                    .chain(es.communications.recv.values())
                    .flatten()
                    .any(|e| e.kind() == ElementKind::Cohesive);
                let syncs = crate::parallel::CohesiveSynchronizers::split_from(&m, &mut es, id)?;
                Some((syncs, had_cohesive.then_some(es)))
            } else {
                None
            }
        };
        let parallel = match parallel {
            Some((syncs, Some(es))) => {
                mesh.setup(|m| {
                    akantu_mesh::MeshAccessor::new(m)
                        .synchronizers_mut()
                        .element = Some(es);
                    Ok(akantu_mesh::MeshEvents::none())
                })?;
                Some(syncs)
            }
            Some((syncs, None)) => Some(syncs),
            None => None,
        };
        let mut smm = SolidMechanicsModel::with_parser_and_type(
            mesh.clone(),
            spatial_dimension,
            id,
            parser,
            akantu_core::enums::ModelType::SolidMechanicsModelCohesive,
        )?;
        register_cohesive_materials(&mut smm.physics.factory)?;
        let dim = smm.spatial_dimension();
        crate::dump::register_cohesive_dumper(&mut smm, id, dim)?;
        // The solid model dumps the cohesive dumpers through the
        // cohesive resolver (upstream `Model::dump` = `mesh.dump`)
        smm.dumpers.set_route(crate::dump::route);
        let (inserter, facet_fe) = {
            let m = mesh.read()?;
            smm.physics.base.fems.register_with(
                COHESIVE_FE_ENGINE,
                &m,
                dim,
                ElementKind::Cohesive,
            )?;
            let inserter =
                CohesiveElementInserter::new(&m, format!("{id}:cohesive_element_inserter"))?;
            let fm = m
                .mesh_facets()
                .ok_or_else(|| Error::Mesh("no facet mesh".into()))?;
            let mut fe = FeEngine::new(
                fm,
                Some(dim - 1),
                ElementKind::Regular,
                ShapeFamily::Lagrange,
                format!("{id}:fem:{FACETS_FE_ENGINE}"),
            )?;
            fe.set_integration_orders(IntegrationOrders::facets_cohesive())?;
            fe.init_shape_functions(fm, GhostType::NotGhost)?;
            fe.init_shape_functions(fm, GhostType::Ghost)?;
            (inserter, fe)
        };
        let mut model = Self {
            smm,
            ext: CohesiveExtension {
                inserter,
                facet_fe: Box::new(facet_fe),
                is_extrinsic: false,
                facet_stress: ElementTypeMapArray::new(format!("{id}:facet_stress")),
                facet_material: ElementTypeMapArray::new(format!("{id}:facet_material")),
                tangents: ElementTypeMapArray::new(format!("{id}:tangents")),
                stress_interpolation: BTreeMap::new(),
                by_elem_buffer: ElementTypeMapArray::new("temporary_stress_by_facets"),
                stress_plans: BTreeMap::new(),
                selector_state: Arc::default(),
                custom_selector: false,
                dim,
                stats: InsertionStats::default(),
                tolerance: akantu_core::math::DEFAULT_TOLERANCE,
                compat: crate::material::CohesiveUpstreamCompat::default(),
                symmetry_policy: akantu_linalg::SymmetryPolicy::default(),
                staged_insertion_facets: None,
                insertion_check: (None, 0),
                parallel,
            },
        };
        model.refresh_default_selector(None);
        Ok(model)
    }

    /// Publish `facet_material` and the first cohesive material to the
    /// cohesive selectors and, unless the user installed a selector chain,
    /// install the default chain (upstream `DefaultMaterialCohesiveSelector`
    /// with the previous selector as fallback).
    fn refresh_default_selector(&mut self, fallback_cohesive: Option<usize>) {
        if let Ok(mut st) = self.ext.selector_state.write() {
            st.facet_material = self.ext.facet_material.clone();
            st.fallback_cohesive = fallback_cohesive;
        }
        if !self.ext.custom_selector {
            let sel = DefaultMaterialCohesiveSelector {
                state: Arc::clone(&self.ext.selector_state),
            };
            self.smm.set_material_selector_chain(SelectorChain {
                selectors: vec![Box::new(sel), Box::new(LawIndexSelector)],
                fallback_value: 0,
            });
        }
    }

    /// The default cohesive selector, sharing the model's state (always
    /// current; for user chains combining it with other selectors, e.g.
    /// [`crate::selector::MeshDataMaterialCohesiveSelector`]).
    #[must_use]
    pub fn default_cohesive_selector(&self) -> DefaultMaterialCohesiveSelector {
        DefaultMaterialCohesiveSelector {
            state: Arc::clone(&self.ext.selector_state),
        }
    }

    /// Replace the material selector chain (upstream `setMaterialSelector`;
    /// combine with [`SolidMechanicsModelCohesive::default_cohesive_selector`]
    /// as upstream's cohesive selectors do). The chain is kept by
    /// `init_full`.
    pub fn set_material_selector_chain(&mut self, chain: SelectorChain) {
        self.ext.custom_selector = true;
        self.smm.set_material_selector_chain(chain);
    }

    /// Upstream `getDefaultCohesiveMaterial`: the first cohesive law.
    #[must_use]
    pub fn default_cohesive_material(&self) -> Option<usize> {
        self.smm
            .physics
            .materials
            .laws()
            .iter()
            .position(|l| is_cohesive(l.as_ref()))
    }

    /// Upstream `initFull(options)` with `_is_extrinsic` (COH-002): the
    /// inserter's input section (`cohesive_inserter` in the model section;
    /// with extrinsic insertion `limitCheckFacets`), then the solid model's
    /// initialisation with the cohesive `initConstitutiveLaws`.
    ///
    /// # Errors
    /// Parsing, solver or law errors; insertion errors. A `model` section
    /// whose name is not a model type is an error before the inserter, the
    /// solver or any law is initialised (upstream `getParserSection`,
    /// `solid_mechanics_model_cohesive.cc:193`).
    pub fn init_full(&mut self, options: &ModelOptions, is_extrinsic: bool) -> Result<()> {
        self.refuse_during_insertion_check("initFull")?;
        self.ext.is_extrinsic = is_extrinsic;
        self.ext.inserter.set_is_extrinsic(is_extrinsic);
        // upstream initFullImpl: on a distributed mesh the facet-stress
        // synchronizer of extrinsic models (a copy of the facet
        // synchronizer, send and receive swapped)
        if is_extrinsic && self.ext.parallel.is_some() {
            let mesh_h = self.smm.physics.base.mesh.clone();
            let m = mesh_h.read()?;
            let fm = m
                .mesh_facets()
                .ok_or_else(|| Error::Mesh("no facet mesh".into()))?;
            let facets = fm.synchronizers().element.clone().unwrap_or_else(|| {
                akantu_comm::Synchronizer::new(
                    format!("{}:facet_synchronizer", fm.id()),
                    fm.communicator().clone(),
                )
            });
            let id = self.smm.physics.base.id.clone();
            if let Some(p) = self.ext.parallel.as_mut() {
                p.facet_stress = Some(crate::parallel::facet_stress_synchronizer(&facets, &id));
            }
        }
        if let Some(section) = self.smm.physics.base.try_parser_section()?.cloned() {
            let permissive = self.smm.physics.base.parser.is_permissive();
            if let Some(ins) = section.sub_sections_of(ParserType::CohesiveInserter).next() {
                let mesh_h = self.smm.physics.base.mesh.clone();
                let m = mesh_h.read()?;
                self.ext.inserter.parse_section(&m, ins, permissive)?;
            }
        }
        self.smm.init_method_and_solver(options)?;
        // the laws read the default solver's time step (upstream
        // `model.getTimeStep()`, live): synchronised here, after the solver
        // exists and before the laws are initialised and the intrinsic
        // insertion dispatches to them — upstream's order, as
        // `SolidMechanicsModel::init_full` does at its end
        // (after a plain `init_full` upstream and port both hold 0, the
        // `TimeStepSolver` default). No cohesive or bulk law reads the time
        // step during its initialisation today, so this position is not
        // observable (the sync moved to the end of
        // `init_full` is an equivalent mutant); it matters only for a law
        // that would
        self.smm.sync_law_time_step();
        let instantiated = self.smm.instantiate_materials()?;
        if instantiated.is_some() || self.smm.nb_materials() > 0 {
            // the instantiation result goes to the solid model's agreed
            // initialisation
            self.init_constitutive_laws_after(instantiated.unwrap_or(Ok(())))?;
        }
        Ok(())
    }

    /// Upstream `SolidMechanicsModelCohesive::initConstitutiveLaws`: the
    /// facet material of every not-ghost facet chosen by the selector (the
    /// facet is added to the law's facet filter when extrinsic; upstream
    /// skips a facet whose selected index is 0 — **UD-219**, reproduced only
    /// with `compat.skip_material_index_0`; by default a selected index 0 is
    /// skipped only when law 0 is not cohesive), the solid
    /// model's law initialisation, then automatic insertion (extrinsic) or
    /// the intrinsic insertion.
    ///
    /// # Errors
    /// A facet assigned to a non-cohesive law, law or insertion errors.
    pub fn init_constitutive_laws(&mut self) -> Result<()> {
        // re-assigning the facet laws and re-initialising the laws between
        // the two phases of a staged check would leave its prepared
        // parameters and insertion marks pending
        self.refuse_during_insertion_check("initConstitutiveLaws")?;
        self.init_constitutive_laws_after(Ok(()))
    }

    /// [`SolidMechanicsModelCohesive::init_constitutive_laws`] after a local
    /// step `pre` (`init_full`'s law instantiation). The facet-material
    /// assignment runs only after a successful `pre`; both go to
    /// [`SolidMechanicsModel::init_materials_after`] as one local step, so on
    /// a distributed mesh (an element synchronizer exists) a rank-specific
    /// failure of either is agreed — fatal: the assignment has changed the
    /// laws' facet filters — before any communication of the law
    /// initialisation (ARCHITECTURE).
    fn init_constitutive_laws_after(&mut self, pre: Result<()>) -> Result<()> {
        let default = self.default_cohesive_material();
        let pre = pre.and_then(|()| self.assign_facet_materials(default));
        self.refresh_default_selector(default);
        self.smm.init_materials_after(pre)?;
        if self.ext.is_extrinsic {
            self.init_automatic_insertion()
        } else {
            self.insert_intrinsic_elements().map(|_| ())
        }
    }

    /// The facet part of upstream `initConstitutiveLaws` (before the solid
    /// model's `initConstitutiveLaws`): each cohesive law's mode, tolerance
    /// and compat switches; `facet_material` from the selector; the law's
    /// facet filter when extrinsic.
    fn assign_facet_materials(&mut self, default: Option<usize>) -> Result<()> {
        let dim = self.ext.dim;
        let mesh_h = self.smm.physics.base.mesh.clone();
        {
            let m = mesh_h.read()?;
            let compat = self.ext.compat;
            for law in self.smm.physics.materials.laws_mut() {
                if let Some(c) = as_cohesive(law.as_mut()) {
                    c.set_extrinsic(&m, self.ext.is_extrinsic)?;
                    c.law.set_tolerance(self.ext.tolerance);
                    c.core.compat = c.core.compat.merged(compat);
                }
            }
            let fm = m
                .mesh_facets()
                .ok_or_else(|| Error::Mesh("no facet mesh".into()))?;
            let def_val = default.map_or(-1, |d| akantu_core::to_idx(d));
            for ghost in GhostType::ALL {
                for ty in fm.element_types(TypeFilter {
                    spatial_dimension: Some(dim - 1),
                    ghost_type: ghost,
                    kind: None,
                }) {
                    let n = fm.nb_element(ty, ghost);
                    self.ext.facet_material.alloc(n, 1, ty, ghost, def_val)?;
                }
            }
            self.refresh_default_selector(default);
            let facets: Vec<Element> = fm
                .elements(TypeFilter {
                    spatial_dimension: Some(dim - 1),
                    ghost_type: GhostType::NotGhost,
                    kind: Some(ElementKind::Regular),
                })
                .collect();
            for facet in facets {
                let mat = {
                    let materials = &self.smm.physics.materials;
                    let by_name = |n: &str| materials.law_index_of(n).ok();
                    let ctx = SelectorContext {
                        mesh: &m,
                        law_index: materials.law_index_map(),
                        law_by_name: &by_name,
                    };
                    materials.selector.select(&ctx, facet)?
                };
                // upstream: `if (not mat_index) return;` — index 0 skipped
                // (UD-219 compat); corrected: skipped only when index 0 is
                // the chain's fallback to a law that is not cohesive (no
                // cohesive material for this facet)
                if mat == 0
                    && (compat.skip_material_index_0
                        || !is_cohesive(self.smm.physics.materials.law(0)?))
                {
                    continue;
                }
                let law = self.smm.physics.materials.law_mut(mat)?;
                let c = as_cohesive(law).ok_or_else(|| {
                    Error::invalid(format!(
                        "facet {facet} selected material {mat}, which is not a cohesive material"
                    ))
                })?;
                self.ext.facet_material.element_mut(facet)?[0] = akantu_core::to_idx(mat);
                if self.ext.is_extrinsic {
                    c.add_facet(facet)?;
                }
            }
        }
        Ok(())
    }

    /// Upstream `insertIntrinsicElements()`: every allowed facet receives a
    /// cohesive element. Returns the number of new elements. A failure
    /// before the mesh changed (e.g. the UD-224 refusal of
    /// [`SolidMechanicsModelCohesive::insert_elements`]) restores the
    /// inserter's insertion flags, so a later extrinsic pass does not insert
    /// the facets flagged here.
    ///
    /// # Errors
    /// Insertion errors, including the UD-224 refusal.
    pub fn insert_intrinsic_elements(&mut self) -> Result<usize> {
        self.refuse_during_insertion_check("insertIntrinsicElements")?;
        let flagged_before = self.ext.inserter.insertion_facets().clone();
        let revision = self.smm.physics.base.mesh.revision()?;
        let r = self.flag_and_insert_intrinsic();
        if r.is_err() && self.smm.physics.base.mesh.revision()? == revision {
            *self.ext.inserter.insertion_facets_mut() = flagged_before;
        }
        r
    }

    /// The flagging and insertion of
    /// [`SolidMechanicsModelCohesive::insert_intrinsic_elements`].
    fn flag_and_insert_intrinsic(&mut self) -> Result<usize> {
        {
            let mesh_h = self.smm.physics.base.mesh.clone();
            let m = mesh_h.read()?;
            self.ext.inserter.upstream_zone_name_error =
                self.ext.compat.zones_unnamed_neighbour_error;
            self.ext.inserter.flag_intrinsic_facets(&m)?;
        }
        self.insert_elements()
    }

    /// Upstream `initAutomaticInsertion()`.
    fn init_automatic_insertion(&mut self) -> Result<()> {
        let mesh_h = self.smm.physics.base.mesh.clone();
        {
            let m = mesh_h.read()?;
            self.ext.inserter.upstream_zone_name_error =
                self.ext.compat.zones_unnamed_neighbour_error;
            self.ext.inserter.limit_check_facets(&m)?;
            self.update_facet_stress_synchronizer(&m)?;
            let fm = m
                .mesh_facets()
                .ok_or_else(|| Error::Mesh("no facet mesh".into()))?;
            resize_facet_stress(&mut self.ext.facet_stress, fm, self.ext.dim)?;
            self.compute_normals(&m)?;
        }
        self.init_stress_interpolation()
    }

    /// Upstream `updateAutomaticInsertion()`: recompute `check_facets`
    /// after a change of the inserter's limits. With `cohesive_zones` and a
    /// cohesive element in the mesh upstream throws (**UD-808**, B): by
    /// default the cohesive neighbour is outside every zone, the switch
    /// [`crate::CohesiveUpstreamCompat::zones_unnamed_neighbour_error`]
    /// reproduces upstream's error.
    ///
    /// # Errors
    /// Mesh errors; the UD-808 compat error.
    pub fn update_automatic_insertion(&mut self) -> Result<()> {
        let mesh_h = self.smm.physics.base.mesh.clone();
        let m = mesh_h.read()?;
        self.ext.inserter.upstream_zone_name_error = self.ext.compat.zones_unnamed_neighbour_error;
        self.ext.inserter.limit_check_facets(&m)?;
        self.update_facet_stress_synchronizer(&m)
    }

    /// Upstream `updateFacetStressSynchronizer()`: the facet-stress
    /// synchronizer keeps the checked facets between this rank and each
    /// scheme's rank (local; [`crate::parallel::filter_facet_stress_synchronizer`]).
    fn update_facet_stress_synchronizer(&mut self, m: &Mesh) -> Result<()> {
        let Some(p) = self.ext.parallel.as_mut() else {
            return Ok(());
        };
        let fm = m
            .mesh_facets()
            .ok_or_else(|| Error::Mesh("no facet mesh".into()))?;
        let check_map = self.ext.inserter.check_facets();
        let check = |f: Element| -> Result<bool> { Ok(check_map.element(f)?[0]) };
        crate::parallel::filter_facet_stress_synchronizer(p, fm, &check)
    }

    /// Upstream `computeNormals()`: facet normals at the facet quadrature
    /// points (reference positions) and their tangents.
    fn compute_normals(&mut self, m: &Mesh) -> Result<()> {
        let fm = m
            .mesh_facets()
            .ok_or_else(|| Error::Mesh("no facet mesh".into()))?;
        self.ext
            .facet_fe
            .compute_normals_on_integration_points(fm, GhostType::NotGhost)?;
        let dim = self.ext.dim;
        let tc = dim * (dim - 1);
        for ghost in GhostType::ALL {
            for ty in fm.element_types(TypeFilter {
                spatial_dimension: Some(dim - 1),
                ghost_type: ghost,
                kind: Some(ElementKind::Regular),
            }) {
                self.ext.tangents.alloc(0, tc, ty, ghost, 0.0)?;
            }
        }
        let types: Vec<ElementType> = fm
            .element_types(TypeFilter {
                spatial_dimension: Some(dim - 1),
                ghost_type: GhostType::NotGhost,
                kind: Some(ElementKind::Regular),
            })
            .collect();
        for ty in types {
            let normals = self
                .ext
                .facet_fe
                .normals_on_integration_points(ty, GhostType::NotGhost)?
                .as_slice()
                .to_vec();
            let n = normals.len() / dim;
            let t = self
                .ext
                .tangents
                .alloc(n, tc, ty, GhostType::NotGhost, 0.0)?;
            if tc == 0 {
                continue;
            }
            for (q, out) in t.as_mut_slice().chunks_exact_mut(tc).enumerate() {
                tangents_of(
                    &normals[q * dim..(q + 1) * dim],
                    dim,
                    out,
                    self.ext.tolerance,
                    self.ext.compat.tangent_rotation_2d_in_3d,
                );
            }
        }
        Ok(())
    }

    /// Upstream `initStressInterpolation()`: the facet quadrature points of
    /// every bulk element (facet-major, `NaN` for missing facets) and, for
    /// every bulk law, the elemental interpolation of its stresses there.
    fn init_stress_interpolation(&mut self) -> Result<()> {
        let mesh_h = self.smm.physics.base.mesh.clone();
        let m = mesh_h.read()?;
        let fm = m
            .mesh_facets()
            .ok_or_else(|| Error::Mesh("no facet mesh".into()))?;
        let dim = self.ext.dim;
        let mut quad_facets = ElementTypeMapArray::<Real>::new("quad_facets");
        for ghost in GhostType::ALL {
            for ty in fm.element_types(TypeFilter {
                spatial_dimension: Some(dim - 1),
                ghost_type: ghost,
                kind: Some(ElementKind::Regular),
            }) {
                let mut out = Array::new(dim);
                self.ext.facet_fe.interpolate_on_integration_points(
                    fm,
                    fm.nodes().view(),
                    &mut out,
                    dim,
                    ty,
                    ghost,
                    None,
                )?;
                quad_facets.as_map_mut().insert(ty, ghost, out)?;
            }
        }
        let s2e = fm.data().element_map(SUBELEMENT_TO_ELEMENT)?;
        let mut points = ElementTypeMapArray::<Real>::new("elements_quad_facets");
        for ghost in GhostType::ALL {
            for ty in m.element_types(TypeFilter {
                spatial_dimension: Some(dim),
                ghost_type: ghost,
                kind: Some(ElementKind::Regular),
            }) {
                let ne = m.nb_element(ty, ghost);
                if ne == 0 {
                    continue;
                }
                let f2e = s2e.get(ty, ghost)?;
                let nfe = f2e.nb_component();
                let facet_ty = ty.facet_type(0).ok_or(Error::UnsupportedElementType {
                    ty,
                    context: "initStressInterpolation (no facet type)",
                })?;
                let nq = nb_quad_facet(facet_ty)?;
                let mut el_q = Array::try_filled(ne * nfe * nq, dim, Real::NAN)?;
                {
                    let o = el_q.as_mut_slice();
                    for (e, row) in f2e.as_slice().chunks_exact(nfe).enumerate() {
                        for (f, gf) in row.iter().enumerate() {
                            if gf.is_null() {
                                continue;
                            }
                            let q = quad_facets.get(gf.ty(), gf.ghost_type())?.as_slice();
                            let start = gf.idx() * nq * dim;
                            let dst = (e * nfe + f) * nq * dim;
                            o[dst..dst + nq * dim].copy_from_slice(&q[start..start + nq * dim]);
                        }
                    }
                }
                points.as_map_mut().insert(ty, ghost, el_q)?;
            }
        }
        let fe = self.smm.physics.base.fems.get("")?;
        self.ext.stress_interpolation.clear();
        self.ext.stress_plans.clear();
        for (i, law) in self.smm.physics.materials.laws().iter().enumerate() {
            if is_cohesive(law.as_ref()) {
                continue;
            }
            let interp = fe.init_elemental_field_interpolation(
                &m,
                &points,
                Some(&law.base().element_filter),
            )?;
            self.ext.stress_interpolation.insert(i, interp);
        }
        Ok(())
    }

    /// Upstream `interpolateStress()`: every bulk law's stresses at the
    /// facet quadrature points, written into `facet_stress` (side 0 for the
    /// facet's first neighbour, side 1 for the second).
    ///
    /// # Errors
    /// Interpolation not initialised (implicit model), mesh errors.
    pub fn interpolate_stress(&mut self) -> Result<()> {
        let mesh_h = self.smm.physics.base.mesh.clone();
        let m = mesh_h.read()?;
        let fm = m
            .mesh_facets()
            .ok_or_else(|| Error::Mesh("no facet mesh".into()))?;
        let dim = self.ext.dim;
        let stress_size = dim * dim;
        let fe = self.smm.physics.base.fems.get("")?;
        let CohesiveExtension {
            stress_interpolation,
            by_elem_buffer,
            facet_stress,
            stress_plans,
            ..
        } = &mut self.ext;
        for (&i, interp) in stress_interpolation.iter_mut() {
            if interp.mesh_revision() != m.revision() {
                interp.accept_revision_unchanged_geometry(&m)?;
            }
            let law = self.smm.physics.materials.law(i)?;
            let filter = &law.base().element_filter;
            // fused form (performance): once the by-element buffer has its
            // blocks (first call), the plan is brought up to date first and
            // the facet values are computed directly from the stresses
            let types: Vec<ElementType> = law.base().filter_types(GhostType::NotGhost);
            if types
                .iter()
                .all(|&t| by_elem_buffer.exists(t, GhostType::NotGhost))
            {
                let plan = stress_plans.entry(i).or_default();
                update_stress_plan(plan, &m, fm, filter, &types, by_elem_buffer, stress_size)?;
                if plan.fused.is_none() {
                    plan.fused = Some(build_fused(plan, filter, interp, stress_size));
                }
                if interpolate_fused(
                    plan,
                    law.base().stress.values(),
                    filter,
                    interp,
                    facet_stress,
                    stress_size,
                )? {
                    continue;
                }
            }
            // one guard per stress block instead of a copy of the whole
            // stress map per check
            fe.interpolate_elemental_field_shared(
                &m,
                interp,
                law.base().stress.values(),
                by_elem_buffer,
                GhostType::NotGhost,
                Some(filter),
            )?;
            let plan = stress_plans.entry(i).or_default();
            update_stress_plan(plan, &m, fm, filter, &types, by_elem_buffer, stress_size)?;
            for block in &plan.blocks {
                let by = by_elem_buffer
                    .get(block.bulk, GhostType::NotGhost)?
                    .as_slice();
                let rs = facet_stress
                    .get_mut(block.facet, block.facet_ghost)?
                    .as_mut_slice();
                if by.len() < block.src_end || rs.len() < block.dst_end {
                    return Err(Error::DimensionMismatch {
                        context: "interpolateStress: facet stress or interpolated stress \
                                  shorter than the copy plan",
                        expected: block.dst_end,
                        got: rs.len(),
                    });
                }
                let nq = block.nq;
                match stress_size {
                    9 => {
                        for &(src, dst) in &block.pairs {
                            copy_points::<9>(by, rs, src, dst, nq);
                        }
                    }
                    4 => {
                        for &(src, dst) in &block.pairs {
                            copy_points::<4>(by, rs, src, dst, nq);
                        }
                    }
                    n => {
                        for &(src, dst) in &block.pairs {
                            copy_points_dyn(by, rs, src, dst, nq, n);
                        }
                    }
                }
            }
        }
        // upstream `synchronize(_smmc_facets_stress)`: the ghost side of
        // every checked facet on a rank boundary sends the stress of its
        // own element to the facet's owner
        if let Some(fss) = self
            .ext
            .parallel
            .as_ref()
            .and_then(|p| p.facet_stress.as_ref())
        {
            let mut acc = crate::parallel::FacetStressAccessor {
                facet_stress: &mut self.ext.facet_stress,
                mesh_facets: fm,
                dim,
            };
            fss.synchronize(
                &mut acc,
                akantu_core::enums::SynchronizationTag::SmmcFacetsStress,
            )?;
        }
        Ok(())
    }

    /// Upstream `checkCohesiveStress()` (COH-004): interpolate the stresses
    /// on the facets, let every cohesive law flag the facets where its
    /// criterion is met, insert the cohesive elements. Returns the number of
    /// new cohesive elements over every rank, ghost copies included (upstream
    /// `updateData`'s `allReduce(nb_new_elements)`; on one process the local
    /// count); see `CohesiveGlobalDataUpdater::global_new_elements`.
    ///
    /// # Errors
    /// An intrinsic model (upstream exception), interpolation, check or
    /// insertion errors.
    pub fn check_cohesive_stress(&mut self) -> Result<usize> {
        let check = self.check_cohesive_stress_prepare()?;
        if let Some(&i) = check.foreign_laws.first() {
            // the runtime laws' checks run with the model released (the
            // binding's phases, `check_cohesive_stress_prepare`)
            let name = self.smm.physics.materials.law(i)?.name().to_owned();
            self.abort_insertion_check(&check)?;
            return Err(Error::invalid(format!(
                "checkCohesiveStress: the runtime cohesive law '{name}' checks the insertion \
                 itself (check_cohesive_stress_prepare, its check, check_cohesive_stress_finish)"
            )));
        }
        self.check_cohesive_stress_finish(check)
    }

    /// First phase of [`SolidMechanicsModelCohesive::check_cohesive_stress`]
    /// for a model with runtime cohesive laws (D-078): the facet
    /// stresses (`interpolateStress`) and the insertion checks of the
    /// built-in laws. The caller then runs the `checkInsertion` of every law
    /// of [`InsertionCheck::foreign_laws`] — the Python binding with the
    /// model released, so the check reads the model's facet stresses,
    /// tangents, facet FE engine and inserter flags through its ordinary
    /// accessors and writes the insertion flags
    /// ([`crate::CohesiveElementInserter::insertion_facets_mut`]) — and
    /// finishes with [`SolidMechanicsModelCohesive::check_cohesive_stress_finish`].
    ///
    /// Upstream checks the laws in index order; here the built-in laws are
    /// checked before the runtime ones. Each law flags facets of its own
    /// facet filter only (the facet filters are disjoint), so the order does
    /// not change the flags.
    ///
    /// **Re-entry** (UD-985): while a prepared check's
    /// runtime laws run their checks (the insertion flags are staged), a
    /// second check and every mesh-changing entry point of the model
    /// (`insert_elements`, `insert_facets_only`, `insert_intrinsic_elements`,
    /// `init_full`, `init_constitutive_laws`) are
    /// refused with a recoverable error that changes nothing, so the
    /// running check keeps its flags and finishes normally. Upstream admits
    /// the nested `checkCohesiveStress()` (its inner call inserts what the
    /// outer check flagged so far); the port refuses it (port-only).
    ///
    /// # Errors
    /// An intrinsic model; a check in progress (re-entry); the built-in
    /// checks' errors (the flags and the laws' prepared parameters are
    /// restored).
    pub fn check_cohesive_stress_prepare(&mut self) -> Result<InsertionCheck> {
        self.refuse_during_insertion_check("checkCohesiveStress")?;
        if !self.ext.is_extrinsic {
            return Err(Error::invalid(
                "This function can only be used for extrinsic cohesive elements",
            ));
        }
        let t0 = Instant::now();
        self.interpolate_stress()?;
        let t1 = Instant::now();
        // The laws append the new elements' parameters
        // before the transaction; a failure before commit restores them and
        // the insertion flags (the next pass must not reuse stale values)
        let marks: Vec<Vec<(ElementType, [usize; 4])>> = self
            .smm
            .physics
            .materials
            .laws()
            .iter()
            .map(|law| {
                law.as_any()
                    .and_then(|a| a.downcast_ref::<MaterialCohesive>())
                    .map(|c| c.law.insertion_marks(&c.core))
                    .unwrap_or_default()
            })
            .collect();
        let flagged_before = self.flagged_insertion_facets();
        let revision = self.smm.physics.base.mesh.revision()?;
        let seq = self.ext.insertion_check.1 + 1;
        self.ext.insertion_check = (Some(seq), seq);
        let mut check = InsertionCheck {
            marks,
            flagged_before,
            revision,
            seq,
            prepared: Vec::new(),
            foreign_laws: Vec::new(),
            times: (t0, t1, t1),
        };
        match self.builtin_insertion_checks() {
            Ok((prepared, foreign)) => {
                check.prepared = prepared;
                check.foreign_laws = foreign;
                check.times.2 = Instant::now();
                if !check.foreign_laws.is_empty() {
                    self.stage_insertion_facets();
                }
                Ok(check)
            }
            Err(e) => {
                self.abort_insertion_check(&check)?;
                Err(e)
            }
        }
    }

    /// Last phase (see [`SolidMechanicsModelCohesive::check_cohesive_stress_prepare`]):
    /// the facets each runtime law flagged become its prepared facets (in
    /// its facet filter's order), then the insertion. Returns the number of
    /// new cohesive elements.
    ///
    /// The check must be the open one (a finished, aborted or superseded
    /// check is refused, nothing changed) and the mesh must be at the
    /// revision of the prepare phase: otherwise the
    /// check is abandoned — nothing is inserted; the flags and the built-in
    /// laws' prepared parameters are restored (for a check with runtime
    /// laws always: the model's own mesh-changing calls were refused during
    /// it) — and an error is returned.
    ///
    /// # Errors
    /// As [`SolidMechanicsModelCohesive::check_cohesive_stress`] (the flags
    /// and prepared parameters are restored when the mesh did not change);
    /// a mesh changed between the phases; a staged block that could not be
    /// read back (the check is abandoned as on a mesh change).
    pub fn check_cohesive_stress_finish(&mut self, check: InsertionCheck) -> Result<usize> {
        if self.ext.insertion_check.0 != Some(check.seq) {
            return Err(Error::invalid(
                "checkCohesiveStress: this insertion check is not the open one (finished, \
                 aborted, or another check was prepared since); nothing changed",
            ));
        }
        self.ext.insertion_check.0 = None;
        if let Err(e) = self.unstage_insertion_facets() {
            // every block is back in the inserter (one that could not be
            // read returned cleared): abandon the check, nothing inserted
            if self.restore_is_safe(&check)? {
                self.restore_insertion_state(&check);
            }
            return Err(e);
        }
        let now = self.smm.physics.base.mesh.revision()?;
        if now != check.revision {
            if self.restore_is_safe(&check)? {
                self.restore_insertion_state(&check);
            }
            return Err(Error::invalid(format!(
                "checkCohesiveStress: the mesh changed during the insertion check (revision {} \
                 at the check, {now} at the insertion); the check is abandoned, nothing inserted",
                check.revision
            )));
        }
        let r = self.finish_insertion(&check);
        if r.is_err() && self.smm.physics.base.mesh.revision()? == check.revision {
            self.restore_insertion_state(&check);
        }
        let (n, t3) = r?;
        let (t0, t1, t2) = check.times;
        let s = &mut self.ext.stats;
        s.calls += 1;
        s.interpolation += (t1 - t0).as_secs_f64();
        s.checks += (t2 - t1).as_secs_f64();
        s.insertion += (t3 - t2).as_secs_f64();
        Ok(n)
    }

    /// Abandon a prepared insertion check (flags and prepared parameters
    /// restored, nothing inserted). The staged flags always return to the
    /// inserter first, so the inserter is never left without its flag map.
    /// Only the open check is abandoned: a check that
    /// was finished, aborted or superseded by a later prepare changes
    /// nothing. The flags and prepared parameters are restored when that is
    /// safe (see `restore_is_safe`).
    ///
    /// # Errors
    /// A poisoned mesh domain; a staged block that could not be read back
    /// (exported, no native section of its runtime: it returned to the
    /// inserter cleared, the flags were restored).
    pub fn abort_insertion_check(&mut self, check: &InsertionCheck) -> Result<()> {
        if self.ext.insertion_check.0 != Some(check.seq) {
            // finished, aborted or superseded: nothing of it to abandon
            return Ok(());
        }
        self.ext.insertion_check.0 = None;
        // the flags are restored even when a block could not be read back
        // (it returned cleared)
        let unstaged = self.unstage_insertion_facets();
        if self.restore_is_safe(check)? {
            self.restore_insertion_state(check);
        }
        unstaged
    }

    /// Whether the flags and the laws' prepared parameters of `check` can
    /// be restored: the mesh is at the check's revision, or the check was
    /// staged (runtime laws) — then the model's mesh-changing calls were
    /// refused during it, so no insertion of this model consumed the
    /// prepared parameters (a different revision comes from outside the
    /// model). An unstaged check of a Rust caller that inserted between the
    /// phases keeps what that insertion made.
    fn restore_is_safe(&self, check: &InsertionCheck) -> Result<bool> {
        Ok(!check.foreign_laws.is_empty()
            || self.smm.physics.base.mesh.revision()? == check.revision)
    }

    /// Whether a prepared insertion check is running its runtime laws'
    /// checks (the insertion flags are staged).
    #[must_use]
    pub fn insertion_check_in_progress(&self) -> bool {
        self.ext.staged_insertion_facets.is_some()
    }

    /// The re-entry refusal (see
    /// [`SolidMechanicsModelCohesive::check_cohesive_stress_prepare`]).
    fn refuse_during_insertion_check(&self, what: &str) -> Result<()> {
        if self.insertion_check_in_progress() {
            return Err(Error::invalid(format!(
                "{what} inside a checkInsertion of the model's insertion check is refused \
                 (port-only, UD-985): the running checkCohesiveStress inserts the flagged \
                 facets when the checks return"
            )));
        }
        Ok(())
    }

    /// Move the inserter's insertion flags into exportable storage
    /// ([`CohesiveExtension::staged_insertion_facets`]). Cannot fail: each
    /// block moves into a fresh map whose keys are those of the inserter's
    /// map (`replace` into an empty slot), so the inserter is never left
    /// with part of its map.
    fn stage_insertion_facets(&mut self) {
        let mut plain = std::mem::take(self.ext.inserter.insertion_facets_mut());
        let mut shared = akantu_core::SharedElementTypeMapArray::new(plain.id().to_owned());
        let blocks: Vec<(ElementType, GhostType)> =
            plain.as_map().iter_all().map(|(t, g, _)| (t, g)).collect();
        for (ty, ghost) in blocks {
            if let Some(a) = plain.as_map_mut().remove(ty, ghost) {
                let previous =
                    shared
                        .as_map_mut()
                        .replace(ty, ghost, akantu_core::SharedArray::new(a));
                debug_assert!(previous.is_none(), "one block per (type, ghost)");
            }
        }
        self.ext.staged_insertion_facets = Some(Box::new(shared));
    }

    /// Move the staged insertion flags back into the inserter (exported
    /// blocks are retired: views taken during the checks become stale but
    /// safe, UD-085).
    ///
    /// **Every block returns to the inserter before an error is reported**:
    /// a block whose values cannot be read — it is
    /// exported and its runtime is not in a native section, which every
    /// binding entry opens, so this is unreachable through the bindings —
    /// returns **cleared** (no facet flagged) with its shape and id, its
    /// exported allocation retired, and the first such error is returned
    /// after the inserter has its whole map again. The callers then abandon
    /// the check (flags and prepared parameters restored).
    fn unstage_insertion_facets(&mut self) -> Result<()> {
        let Some(mut shared) = self.ext.staged_insertion_facets.take().map(|b| *b) else {
            return Ok(());
        };
        let id = shared.id().to_owned();
        let mut plain = ElementTypeMapArray::<bool>::new(id);
        let blocks: Vec<(ElementType, GhostType)> =
            shared.as_map().iter_all().map(|(t, g, _)| (t, g)).collect();
        let mut first_error = None;
        for (ty, ghost) in blocks {
            let Some(a) = shared.as_map_mut().remove(ty, ghost) else {
                continue;
            };
            // the shape and id are read without native access
            let (len, nbc, block_id) = (a.len(), a.nb_component(), a.id().to_owned());
            let array = a.into_inner().unwrap_or_else(|e| {
                first_error.get_or_insert(e);
                let mut cleared = Array::filled(len, nbc, false);
                cleared.set_id(block_id);
                cleared
            });
            let previous = plain.as_map_mut().replace(ty, ghost, array);
            debug_assert!(previous.is_none(), "one block per (type, ghost)");
        }
        *self.ext.inserter.insertion_facets_mut() = plain;
        first_error.map_or(Ok(()), Err)
    }

    /// The staged insertion flags of `(ty, ghost)` (exportable), while the
    /// runtime laws check the insertion.
    ///
    /// # Errors
    /// No check in progress; no block of that type.
    pub fn staged_insertion_facets_mut(
        &mut self,
        ty: ElementType,
        ghost: GhostType,
    ) -> Result<&mut akantu_core::SharedArray<bool>> {
        self.ext
            .staged_insertion_facets
            .as_mut()
            .ok_or_else(|| {
                Error::invalid(
                    "the insertion flags are writable only inside a checkInsertion of the model's \
                     insertion check",
                )
            })?
            .shared_mut(ty, ghost)
    }

    fn restore_insertion_state(&mut self, check: &InsertionCheck) {
        for (law, m) in self
            .smm
            .physics
            .materials
            .laws_mut()
            .iter_mut()
            .zip(&check.marks)
        {
            if let Some(MaterialCohesive { core, law, .. }) = as_cohesive(law.as_mut()) {
                law.restore_insertion_marks(core, m);
            }
        }
        let ins = self.ext.inserter.insertion_facets_mut();
        ins.fill(false);
        for &(ty, ghost, i) in &check.flagged_before {
            if let Ok(a) = ins.get_mut(ty, ghost)
                && let Some(f) = a.as_mut_slice().get_mut(i)
            {
                *f = true;
            }
        }
    }

    /// Facets flagged for insertion (normally none between passes).
    fn flagged_insertion_facets(&self) -> Vec<(ElementType, GhostType, usize)> {
        let mut out = Vec::new();
        for (ty, ghost, a) in self.ext.inserter.insertion_facets().as_map().iter_all() {
            for (i, &f) in a.as_slice().iter().enumerate() {
                if f {
                    out.push((ty, ghost, i));
                }
            }
        }
        out
    }

    /// The built-in laws' insertion checks of
    /// [`SolidMechanicsModelCohesive::check_cohesive_stress_prepare`]: the
    /// facets whose new elements' parameters each law prepared, and the
    /// indices of the runtime laws (checked by the caller; a runtime law
    /// without its own check refuses, upstream `AKANTU_TO_IMPLEMENT`).
    #[allow(clippy::type_complexity)] // (prepared per law, runtime law indices)
    fn builtin_insertion_checks(&mut self) -> Result<(Vec<(usize, Vec<Element>)>, Vec<usize>)> {
        let mut prepared: Vec<(usize, Vec<Element>)> = Vec::new();
        let mut foreign = Vec::new();
        let mesh_h = self.smm.physics.base.mesh.clone();
        let m = mesh_h.read()?;
        let fm = m
            .mesh_facets()
            .ok_or_else(|| Error::Mesh("no facet mesh".into()))?;
        let CohesiveExtension {
            inserter,
            facet_fe,
            facet_stress,
            tangents,
            ..
        } = &mut self.ext;
        let (check, insertion) = inserter.flags_mut();
        let mut inputs = InsertionInputs {
            mesh_facets: fm,
            check_facets: check,
            insertion_facets: insertion,
            facet_stress,
            tangents,
            facet_fe,
        };
        for (i, law) in self.smm.physics.materials.laws_mut().iter_mut().enumerate() {
            let Some(c) = as_cohesive(law.as_mut()) else {
                continue;
            };
            if let CohesiveLaw::Foreign(f) = &c.law {
                if !f.has_insertion_check() {
                    return Err(Error::NotImplemented(
                        "checkInsertion of a runtime cohesive law that does not define it \
                         (upstream MaterialCohesive::checkInsertion: AKANTU_TO_IMPLEMENT)",
                    ));
                }
                foreign.push(i);
                continue;
            }
            let MaterialCohesive { core, law, .. } = c;
            prepared.push((i, law.check_insertion_prepared(core, &mut inputs)?));
        }
        Ok((prepared, foreign))
    }

    /// The insertion of a finished check: the runtime laws' flagged facets
    /// become their prepared facets, then the insertion.
    fn finish_insertion(&mut self, check: &InsertionCheck) -> Result<(usize, Instant)> {
        let mut prepared = check.prepared.clone();
        for &i in &check.foreign_laws {
            let law = self.smm.physics.materials.law(i)?;
            let c = law
                .as_any()
                .and_then(|a| a.downcast_ref::<MaterialCohesive>())
                .ok_or_else(|| Error::invalid("internal error: runtime law index"))?;
            let ins = self.ext.inserter.insertion_facets();
            let mut facets = Vec::new();
            for (ty, ghost, filter) in c.core.facet_filter.as_map().iter_all() {
                let Ok(flags) = ins.get(ty, ghost) else {
                    continue;
                };
                let flags = flags.as_slice();
                for &f in filter.as_slice() {
                    let k = akantu_core::try_to_usize(f)?;
                    if flags.get(k).copied().unwrap_or(false) {
                        facets.push(Element::new(ty, f, ghost));
                    }
                }
            }
            prepared.push((i, facets));
        }
        prepared.sort_by_key(|(i, _)| *i);
        let n = self.insert_flagged_elements(Some(&prepared))?;
        Ok((n, Instant::now()))
    }

    /// The cohesive laws (index, name, type) that have no strength unless
    /// the extrinsic insertion check sets it (**UD-224**, see
    /// [`crate::laws::CohesiveLaw::strength_from_insertion_check_only`]).
    fn zero_strength_laws(&self) -> Vec<(usize, String, String)> {
        self.smm
            .physics
            .materials
            .laws()
            .iter()
            .enumerate()
            .filter_map(|(i, law)| {
                let c = law
                    .as_any()
                    .and_then(|a| a.downcast_ref::<MaterialCohesive>())?;
                c.law
                    .strength_from_insertion_check_only()
                    .then(|| (i, law.name().to_owned(), law.law_type().to_owned()))
            })
            .collect()
    }

    /// Upstream `CohesiveElementInserter::insertElements()` driven by the
    /// model (the model is the only consumer of the mesh): one
    /// `SharedMesh::mutate` transaction; afterwards the inserter's flags,
    /// the facet FE engine and the stress interpolation follow the new
    /// facets. Returns the number of new cohesive elements.
    ///
    /// **Planned law assignment**. Inside the
    /// transaction, after the topology change and before commit, the law of
    /// every new cohesive element is chosen exactly as the post-commit
    /// dispatch chooses it (the model's selector chain, on the staged mesh,
    /// in dispatch order, each decision visible to the next) and staged: the
    /// dispatch takes the staged decisions (a first selector answering for
    /// the new elements), so a second selector call cannot change them. The
    /// planned assignment is validated before commit — a refusal rolls the
    /// transaction back (on a distributed mesh the transaction's topology
    /// agreement covers it) and leaves the law-index map as it was:
    ///
    /// * a cohesive element assigned to a law that is not cohesive is
    ///   refused (**UD-246**, upstream adds it to the bulk law);
    /// * **UD-224**: outside the extrinsic insertion
    ///   check ([`SolidMechanicsModelCohesive::check_cohesive_stress`])
    ///   nothing sets the per-point strength `σc,eff` of the linear-family
    ///   laws without an elastic branch (`cohesive_linear`, `_fatigue`,
    ///   `_friction`, `_uncoupled`): an insertion that would give such a law
    ///   new elements is refused — the intrinsic insertion of `init_full(…,
    ///   false)` and [`SolidMechanicsModelCohesive::insert_intrinsic_elements`]
    ///   included, on intrinsic and extrinsic models alike, whatever the
    ///   facets' laws. `CohesiveUpstreamCompat::intrinsic_linear_zero_strength`
    ///   (set before `init_full`) reproduces upstream;
    /// * after the extrinsic insertion check, every law must receive exactly
    ///   the elements whose parameters its check prepared, in its check's
    ///   order (**UD-246**: otherwise the elements would read another
    ///   facet's or no prepared strength).
    ///
    /// # Errors
    /// A refused planned assignment (UD-224, UD-246; nothing changed);
    /// selector errors; transaction errors (the mesh is rolled back before
    /// commit; a consumer failure after commit poisons the mesh domain).
    pub fn insert_elements(&mut self) -> Result<usize> {
        self.refuse_during_insertion_check("insertElements")?;
        self.insert_flagged_elements(None)
    }

    /// [`SolidMechanicsModelCohesive::insert_elements`]; `prepared`: after
    /// the laws' insertion checks, the facets each law (by index) prepared,
    /// in its order of preparation.
    fn insert_flagged_elements(
        &mut self,
        prepared: Option<&[(usize, Vec<Element>)]>,
    ) -> Result<usize> {
        // distributed mesh: the insertion decision is collective
        // (owner -> ghost flags, global insertion order), on every rank,
        // also with nothing flagged locally; nothing flagged anywhere: no
        // transaction on any rank
        let order = if self.ext.parallel.is_some() {
            let mesh_h = self.smm.physics.base.mesh.clone();
            let m = mesh_h.read()?;
            let mut ph = Phases::new(m.communicator(), "cohesive:insertion_order");
            let r = self.ext.inserter.synchronize_insertion_in(&mut ph, &m);
            let o = ph.settle(r)?;
            if o.global_flagged == 0 {
                return Ok(0);
            }
            Some(o.facets)
        } else {
            // nothing flagged: no transaction, no revision bump
            // (upstream inserts nothing either)
            if !self
                .ext
                .inserter
                .insertion_facets()
                .as_map()
                .iter_all()
                .any(|(_, _, a)| a.as_slice().contains(&true))
            {
                return Ok(0);
            }
            None
        };
        let zero_strength = if prepared.is_some() || self.ext.compat.intrinsic_linear_zero_strength
        {
            Vec::new()
        } else {
            self.zero_strength_laws()
        };
        let staged: StagedLaws = Arc::default();
        self.smm
            .physics
            .materials
            .selector
            .selectors
            .insert(0, Box::new(StagedSelector(Arc::clone(&staged))));
        let revision = self.smm.physics.base.mesh.revision();
        let r = self.insert_staged(prepared, &zero_strength, &staged, order.as_deref());
        let selectors = &mut self.smm.physics.materials.selector.selectors;
        if !selectors.is_empty() {
            selectors.remove(0);
        }
        if r.is_err()
            && revision.is_ok()
            && self.smm.physics.base.mesh.revision().ok() == revision.ok()
        {
            // rolled back: the staged law indices of elements that do not
            // exist are dropped
            let mesh_h = self.smm.physics.base.mesh.clone();
            let m = mesh_h.read()?;
            let map = self.smm.physics.materials.law_index_map_mut();
            let blocks: Vec<(ElementType, GhostType)> =
                map.as_map().iter_all().map(|(t, g, _)| (t, g)).collect();
            for (ty, g) in blocks {
                let n = m.nb_element(ty, g);
                if let Some(mut a) = if_present(map.get_mut(ty, g))?
                    && a.len() > n
                {
                    a.truncate(n);
                }
            }
        }
        r
    }

    /// Upstream `getElementInserter().insertElements(true)` in a cohesive
    /// model (`CohesiveElementInserter::insertElements(only_double_facets =
    /// true)` → `CohesiveElementInserterHelper::insertFacetsOnly`, MUTIL-016,
    /// COH-005): the facets flagged in the inserter's `insertion_facets` and
    /// their nodes are doubled — the crack opens as free surfaces — but no
    /// cohesive element is created. One `SharedMesh::mutate` transaction:
    /// the facet events and the parent's `NodesAdded` event (with
    /// `old_nodes`) reach the model, which resizes its nodal arrays and
    /// copies the state of each old node to its new node (upstream
    /// `onNodesAdded`); no `ElementsAdded` event is sent, so no law receives
    /// an element and `facet_stress` / `facet_material` are not resized (as
    /// upstream, whose `onElementsAdded` is not called). Afterwards the
    /// inserter's flags are updated when a facet was doubled (upstream
    /// `updateInsertionFacets`) and the facet FE engine follows the new
    /// facets.
    ///
    /// Returns the topology result: `new_elements` is empty — upstream's
    /// return value, the number of new cohesive elements of
    /// `updateGlobalData`, is `new_elements.len()` = 0 — with the doubled
    /// nodes and facets.
    ///
    /// **UD-250 (A):** `facet_stress` is resized to the new facets (upstream
    /// is not, and a later `checkCohesiveStress` writes past it), so
    /// [`SolidMechanicsModelCohesive::check_cohesive_stress`] works after a
    /// facets-only insertion; the doubled facets are not checkable.
    ///
    /// **Distributed mesh** (PART-011; upstream
    /// `insertElements(true)` on a distributed mesh): collective on every
    /// rank, also on a rank without a flagged facet. The insertion decision
    /// is upstream's (`_ce_groups`: ghost facets take their owner's flag;
    /// the global insertion order through `_ce_insertion_order`, see
    /// [`crate::inserter::parallel`]); nothing flagged anywhere: nothing
    /// happens on any rank. The facets are doubled in that order and the
    /// transaction's distributed hook (upstream
    /// `CohesiveMeshGlobalDataUpdater::updateData`) gives the doubled nodes
    /// their old nodes' flags, global ids and owner ranks, prunes the
    /// facet-stress synchronizer (the doubled facets are no longer checked)
    /// and rebuilds the node synchronizer; the cohesive synchronizer is
    /// unchanged (no new cohesive element). Upstream's return value is the
    /// global number of new cohesive elements, 0 here as in serial.
    /// **UD-1043 (A):** an insertion that doubles a node shared with
    /// another rank (a slave copy on some rank) is refused on every rank and
    /// rolled back (upstream leaves the copy without global id and owner,
    /// then asserts in `NodeSynchronizer::onNodesAdded`).
    ///
    /// # Errors
    /// Malformed adjacency, transaction errors (the mesh is rolled back
    /// before commit); on a distributed mesh the agreed refusals of the
    /// insertion order (UD-815) and exchange errors.
    pub fn insert_facets_only(&mut self) -> Result<InsertionResult> {
        // the facet mesh would change and the inserter's flags are staged
        // (moved out) during a check
        self.refuse_during_insertion_check("insertElements(true)")?;
        // distributed mesh: the decision is collective (owner -> ghost flags,
        // global insertion order) on every rank, also with nothing flagged
        // locally; nothing flagged anywhere: no transaction on any rank
        let order = if self.ext.parallel.is_some() {
            let mesh_h = self.smm.physics.base.mesh.clone();
            let m = mesh_h.read()?;
            let mut ph = Phases::new(m.communicator(), "cohesive:facets_only_order");
            let r = self.ext.inserter.synchronize_insertion_in(&mut ph, &m);
            let o = ph.settle(r)?;
            if o.global_flagged == 0 {
                return Ok(InsertionResult::default());
            }
            Some(o.facets)
        } else {
            if !self
                .ext
                .inserter
                .insertion_facets()
                .as_map()
                .iter_all()
                .any(|(_, _, a)| a.as_slice().contains(&true))
            {
                return Ok(InsertionResult::default());
            }
            None
        };
        let mesh_h = self.smm.physics.base.mesh.clone();
        let (token, smm_parts) = self.smm.split_for_mutation();
        let parts = RefCell::new(smm_parts);
        let CohesiveExtension {
            inserter,
            facet_stress,
            is_extrinsic,
            dim,
            parallel,
            compat,
            ..
        } = &mut self.ext;
        let mut handler = CohesiveEventHandler {
            smm: &parts,
            facet_stress,
            is_extrinsic: *is_extrinsic,
            dim: *dim,
            ghost_laws: None,
            move_external_force: !compat.doubled_node_keeps_external_force,
        };
        let mut result: Option<InsertionResult> = None;
        let mut new_facets: Vec<Element> = Vec::new();
        let ins: &CohesiveElementInserter = inserter;
        // upstream `updateInsertionFacets` runs before `updateData` (when the
        // rank doubled a facet): a doubled facet is no longer checked
        let (check_map, ins_map) = (ins.check_facets(), ins.insertion_facets());
        let check_after = |f: Element| -> Result<bool> {
            let c = check_map.element(f).is_ok_and(|v| v[0]);
            let i = ins_map.element(f).is_ok_and(|v| v[0]);
            Ok(c && !(*is_extrinsic && i))
        };
        // no new cohesive element: no staged law and no law-id exchange (the
        // same on every rank)
        let mut hook = parallel.as_ref().map(|p| {
            crate::parallel::CohesiveGlobalDataUpdater::new(p, &check_after, Arc::default(), false)
        });
        mesh_h.mutate(
            &mut [(token, &mut handler)],
            |tx| {
                let r = ins.insert_elements_ordered_in(tx, true, order.as_deref())?;
                // UD-1043 (A): upstream renumbers the doubled nodes through the
                // cohesive synchronizer, which holds no element of a
                // facets-only insertion: a doubled node shared with another
                // rank (slave copy) keeps no global id nor owner and
                // upstream's `NodeSynchronizer::onNodesAdded` asserts ("does
                // not have a valid associated prank"); refused here, agreed
                // with the topology and rolled back on every rank
                if order.is_some()
                    && let Some(&(old, new)) = r
                        .doubled_nodes
                        .iter()
                        .find(|&&(_, new)| tx.mesh().node_flag(new).is_slave())
                {
                    return Err(Error::Unsupported(format!(
                        "insert_facets_only on a distributed mesh doubles node {old} (new node \
                         {new}), which another rank owns: upstream gives the doubled copies \
                         global ids through the cohesive synchronizer, which holds no element \
                         of a facets-only insertion, so the copy would have no global id nor \
                         owner (upstream asserts in NodeSynchronizer::onNodesAdded); refused \
                         (UPSTREAM_DEFECTS UD-1043). Facets whose doubled nodes belong to one \
                         rank are supported"
                    )));
                }
                for (part, ev) in tx.events() {
                    if *part == akantu_mesh::MeshPart::Facets
                        && let MeshEvent::ElementsAdded { elements, .. } = ev
                    {
                        new_facets.extend_from_slice(elements);
                    }
                }
                result = Some(r);
                Ok(())
            },
            hook.as_mut()
                .map(|h| h as &mut dyn akantu_mesh::DistributedHook),
        )?;
        // every rank committed: the staged synchronizers become the model's
        if let Some(h) = hook
            && h.committed
        {
            *parallel = Some(h.staged);
        }
        let r = result.unwrap_or_default();
        let m = mesh_h.read()?;
        if !r.doubled_facets.is_empty() {
            self.ext.inserter.update_insertion_facets(&m)?;
        }
        if let Some(fm) = m.mesh_facets()
            && !new_facets.is_empty()
        {
            self.ext.facet_fe.on_elements_added(fm, &new_facets)?;
            // UD-250 (A): upstream resizes `facet_stress` only on
            // `ElementsAdded`, so a later `checkCohesiveStress` writes the
            // doubled facets' stresses past it; the port keeps one entry per
            // facet (the doubled facets are not checkable)
            if self.ext.is_extrinsic {
                resize_facet_stress(&mut self.ext.facet_stress, fm, self.ext.dim)?;
            }
        }
        Ok(r)
    }

    /// The transaction of [`SolidMechanicsModelCohesive::insert_flagged_elements`]
    /// with the staged selection.
    fn insert_staged(
        &mut self,
        prepared: Option<&[(usize, Vec<Element>)]>,
        zero_strength: &[(usize, String, String)],
        staged: &StagedLaws,
        order: Option<&[Element]>,
    ) -> Result<usize> {
        let mesh_h = self.smm.physics.base.mesh.clone();
        let (token, smm_parts) = self.smm.split_for_mutation();
        let exchange_laws = smm_parts.physics.materials.nb_laws() > 0;
        let parts = RefCell::new(smm_parts);
        let CohesiveExtension {
            inserter,
            facet_stress,
            is_extrinsic,
            dim,
            parallel,
            compat,
            ..
        } = &mut self.ext;
        let ghost_laws: crate::parallel::GhostLaws = Arc::default();
        let mut handler = CohesiveEventHandler {
            smm: &parts,
            facet_stress,
            is_extrinsic: *is_extrinsic,
            dim: *dim,
            ghost_laws: parallel.is_some().then(|| Arc::clone(&ghost_laws)),
            move_external_force: !compat.doubled_node_keeps_external_force,
        };
        let mut result: Option<InsertionResult> = None;
        let mut new_facets: Vec<Element> = Vec::new();
        let ins: &CohesiveElementInserter = inserter;
        // upstream `updateInsertionFacets` runs before `updateData`: in the
        // hook an inserted facet is no longer checked (new facets: false)
        let (check_map, ins_map) = (ins.check_facets(), ins.insertion_facets());
        let check_after = |f: Element| -> Result<bool> {
            let c = check_map.element(f).is_ok_and(|v| v[0]);
            let i = ins_map.element(f).is_ok_and(|v| v[0]);
            Ok(c && !(*is_extrinsic && i))
        };
        let mut hook = parallel.as_ref().map(|p| {
            let mut h = crate::parallel::CohesiveGlobalDataUpdater::new(
                p,
                &check_after,
                Arc::clone(staged),
                exchange_laws,
            );
            h.ghost_laws = Arc::clone(&ghost_laws);
            h
        });
        mesh_h.mutate(
            &mut [(token, &mut handler)],
            |tx| {
                let r = ins.insert_elements_ordered_in(tx, false, order)?;
                // The planned law of every new element,
                // validated before commit (rolled back on refusal; on a
                // distributed mesh agreed by the transaction)
                {
                    let mut p = parts_mut(&parts)?;
                    let materials = &mut p.physics.materials;
                    let planned =
                        stage_cohesive_laws(materials, tx.mesh(), &r.new_elements, staged)?;
                    validate_planned_laws(materials, &planned, &r, prepared, zero_strength)?;
                }
                for (part, ev) in tx.events() {
                    if *part == akantu_mesh::MeshPart::Facets
                        && let MeshEvent::ElementsAdded { elements, .. } = ev
                    {
                        new_facets.extend_from_slice(elements);
                    }
                }
                result = Some(r);
                Ok(())
            },
            hook.as_mut()
                .map(|h| h as &mut dyn akantu_mesh::DistributedHook),
        )?;
        // every rank committed: the staged synchronizers become the model's
        let global = match hook {
            Some(h) if h.committed => {
                let n = h.global_new_elements;
                *parallel = Some(h.staged);
                Some(n)
            }
            _ => None,
        };
        let r = result.unwrap_or_default();
        let m = mesh_h.read()?;
        if !r.new_elements.is_empty() {
            self.ext.inserter.update_insertion_facets(&m)?;
        }
        if let Some(fm) = m.mesh_facets()
            && !new_facets.is_empty()
        {
            self.ext.facet_fe.on_elements_added(fm, &new_facets)?;
        }
        self.ext.stats.inserted += r.new_elements.len();
        // upstream returns the global count (`CohesiveMeshGlobalDataUpdater`:
        // the sum over the ranks of their new cohesive elements, ghost
        // copies included)
        Ok(global.map_or(r.new_elements.len(), |n| {
            usize::try_from(n).unwrap_or(usize::MAX)
        }))
    }

    /// Upstream `assembleInternalForces()` (COH-007): cohesive tractions,
    /// then the solid model's internal forces (cohesive laws assemble at
    /// their position in the material loop).
    ///
    /// Outside a solve step this is a **trial** evaluation
    /// (UD-241/UD-245 defaults): the laws read the history committed
    /// by the last converged step and commit nothing; only a converged solve
    /// step commits the maximal openings (or, for a hand-driven loop, the
    /// material hooks `before_solve_step` / `after_solve_step(true)`).
    ///
    /// Side effect of a trial: the internals without a
    /// committed baseline — `damage`, `tractions`, `opening`,
    /// `contact_tractions`, `contact_opening` and the current (uncommitted)
    /// maximal openings — hold the **trial** values until the next
    /// evaluation, also after a failed solve step. A dump, an energy query
    /// or a [`crate::FragmentManager`] pass taken then sees the trial state
    /// (e.g. the trial damage); evaluate at the converged displacement first
    /// for converged outputs. `cohesive_linear_friction` and
    /// `cohesive_linear_fatigue` keep upstream's per-evaluation internals: a
    /// standalone evaluation is a commit for them (UD-241, UD-245).
    ///
    /// # Errors
    /// See [`compute_cohesive_tractions`].
    pub fn assemble_internal_forces(&mut self) -> Result<()> {
        compute_cohesive_tractions(&mut self.smm.physics, &self.smm.solver.dof_manager)?;
        if let Some(sync) = self.ext.parallel.as_ref().map(|p| &p.cohesive) {
            let physics = &mut self.smm.physics;
            self.smm
                .solver
                .dof_manager
                .collective("cohesive:tractions", |dm| {
                    exchange_cohesive_tractions(physics, dm, sync)
                })?;
        }
        self.smm.assemble_internal_forces()
    }

    /// Exchange `tag` over the cohesive synchronizer (upstream
    /// `synchronize(tag)` restricted to the cohesive elements):
    /// `_smm_stress` (tractions and contact tractions) and `_smmc_damage`
    /// (damage, upstream `FragmentManager`'s exchange, MPI-039). Owners'
    /// values overwrite their ghost copies. No-op on a mesh that is not
    /// distributed.
    ///
    /// # Errors
    /// Another tag (`Error::Unsupported`), exchange errors.
    pub fn synchronize_cohesive(
        &mut self,
        tag: akantu_core::enums::SynchronizationTag,
    ) -> Result<()> {
        use akantu_core::enums::SynchronizationTag as T;
        let Some(p) = self.ext.parallel.as_ref() else {
            return Ok(());
        };
        let mesh_h = self.smm.physics.base.mesh.clone();
        let nb_quad = {
            let m = mesh_h.read()?;
            cohesive_nb_quad(&self.smm.physics, &m)?
        };
        let dim = self.ext.dim;
        let materials = &mut self.smm.physics.materials;
        let mut acc = match tag {
            T::SmmStress => crate::parallel::TractionAccessor::tractions(materials, &nb_quad, dim),
            T::SmmcDamage => crate::parallel::DamageAccessor::damage(materials, &nb_quad, dim),
            _ => {
                return Err(Error::Unsupported(format!(
                    "synchronize_cohesive: tag {tag:?} (the cohesive synchronizer exchanges \
                     _smm_stress and _smmc_damage)"
                )));
            }
        };
        p.cohesive.synchronize(&mut acc, tag)
    }

    /// Solve one step (upstream `solveStep(solver_id)`) with the cohesive
    /// callback.
    ///
    /// Committed maxima (UD-245 default): a converged
    /// step commits the maximal openings in `after_solve_step(true)`. Under
    /// the Newton solver's legacy hooks (UD-010 compat,
    /// `NonLinearSolverNewtonRaphson::set_legacy_hooks(true)`) a step whose
    /// initial residual already converges returns successfully without that
    /// hook; the model then commits the maxima here, on the return path both
    /// hook modes take (nothing else of `after_solve_step` runs, as
    /// upstream's early return).
    ///
    /// # Errors
    /// Stale mesh revision, solver errors.
    pub fn solve_step(&mut self, solver_id: Option<&str>) -> Result<()> {
        let sync = self.ext.parallel.as_ref().map(|p| &p.cohesive);
        self.smm.solve_step_with(solver_id, |physics, solver, id| {
            let mut cb = CohesiveCallback::with_synchronizer(physics, sync);
            solver.solve_step(&mut cb, id)?;
            if cb.step_open() {
                commit_cohesive_maxima(cb.physics)?;
            }
            Ok(())
        })
    }

    /// Upstream `setTimeStep(dt, solver_id)`: the solid model's time step
    /// (solver, default dumper) and the time step of the
    /// [`crate::COHESIVE_DUMPER`] (COH-013).
    ///
    /// # Errors
    /// Solver errors.
    pub fn set_time_step(&mut self, dt: Real, solver_id: Option<&str>) -> Result<()> {
        self.smm.set_time_step(dt, solver_id)?;
        // upstream: `mesh.getDumper("cohesive elements").setTimeStep(dt)`,
        // set explicitly (the solid model sets its default dumper only)
        self.smm
            .dumpers
            .dumpable
            .set_time_step_to_dumper(crate::dump::COHESIVE_DUMPER, dt)
    }

    /// Upstream `getStableTimeStep()` (bulk elements).
    ///
    /// # Errors
    /// See `SolidMechanicsModel::stable_time_step`.
    pub fn stable_time_step(&mut self) -> Result<Real> {
        self.smm.stable_time_step()
    }

    /// Upstream `getEnergy(id)`: `kinetic`, `potential`, `external work`,
    /// and the cohesive `dissipated`, `reversible`, `cohesive contact`
    /// (summed over the laws).
    ///
    /// # Errors
    /// See `SolidMechanicsModel::energy`.
    pub fn energy(&mut self, id: &str) -> Result<Real> {
        self.smm.energy(id)
    }

    /// The cohesive material named `name`.
    ///
    /// # Errors
    /// Unknown name or not a cohesive law.
    pub fn cohesive_material(&self, name: &str) -> Result<&MaterialCohesive> {
        let i = self.smm.material_index(name)?;
        self.smm
            .material(i)?
            .as_any()
            .and_then(|a| a.downcast_ref::<MaterialCohesive>())
            .ok_or_else(|| Error::invalid(format!("material {name} is not cohesive")))
    }

    /// Mutable [`SolidMechanicsModelCohesive::cohesive_material`].
    ///
    /// # Errors
    /// Unknown name or not a cohesive law.
    pub fn cohesive_material_mut(&mut self, name: &str) -> Result<&mut MaterialCohesive> {
        let i = self.smm.material_index(name)?;
        as_cohesive(self.smm.material_mut(i)?)
            .ok_or_else(|| Error::invalid(format!("material {name} is not cohesive")))
    }

    /// Select upstream's assembly of `_unsymmetric` cohesive tangent blocks
    /// into the symmetric `K` (**UD-034** compat:
    /// `SymmetryPolicy::UpstreamUpperSelection`, only the upper-triangle
    /// entries are kept) instead of the default promotion of `K` to
    /// unsymmetric storage. Must be called before the DOF families exist
    /// (before `init_full`): the DOF manager is recreated with the policy.
    ///
    /// # Errors
    /// DOF families already registered.
    pub fn set_symmetry_policy(&mut self, policy: akantu_linalg::SymmetryPolicy) -> Result<()> {
        let dm = &self.smm.solver.dof_manager;
        if !dm.dof_ids().is_empty() {
            return Err(Error::invalid(
                "set_symmetry_policy must be called before init_full (DOF families exist)",
            ));
        }
        // The policy belongs to the default (AIJ) back-end; recreating the
        // manager would silently drop another back-end chosen with
        // `dof_manager_type` / `init_dof_manager` (PETSc).
        let kind = dm.backend().kind();
        if kind != akantu_solver::BackendKind::Aij {
            return Err(Error::invalid(format!(
                "set_symmetry_policy applies to the default DOF manager (AIJ back-end); this \
                 model's DOF manager '{}' has the {kind:?} back-end",
                dm.id()
            )));
        }
        let id = dm.id().to_owned();
        let comm = dm.communicator().clone();
        self.smm.solver.dof_manager = DofManager::with_backend(
            id,
            comm,
            Box::new(akantu_solver::backend::AijBackend::with_policy(policy)),
        );
        self.ext.symmetry_policy = policy;
        Ok(())
    }

    /// The symmetry policy set by
    /// [`SolidMechanicsModelCohesive::set_symmetry_policy`] (default: the
    /// UD-034 promotion).
    #[must_use]
    pub fn symmetry_policy(&self) -> akantu_linalg::SymmetryPolicy {
        self.ext.symmetry_policy
    }

    /// Upstream `Math::setTolerance(tol)` for this model: the cohesive laws
    /// (penetration and damage tests, fictitious openings), the facet
    /// tangents and the inserter's bounding-box test. Call before
    /// `init_full` (upstream tests set it before reading the mesh).
    pub fn set_math_tolerance(&mut self, tol: Real) {
        self.ext.tolerance = tol;
        self.ext.inserter.tolerance = tol;
        for law in self.smm.physics.materials.laws_mut() {
            if let Some(c) = as_cohesive(law.as_mut()) {
                c.law.set_tolerance(tol);
            }
        }
    }

    /// Upstream `getIsExtrinsic()`.
    #[must_use]
    pub fn is_extrinsic(&self) -> bool {
        self.ext.is_extrinsic
    }

    /// One staged mutation of the model's mesh with the cohesive model as
    /// its consumer (the solid model's token and the cohesive event handler,
    /// as [`SolidMechanicsModelCohesive::insert_elements`] uses; the model
    /// must be the mesh's only consumer). E.g. a node removal after
    /// insertion (SMM-015): `MeshTransaction::{remove_nodes, purify}`
    /// renumber the DOFs (the doubled nodes' block included) and compact the
    /// solid model's nodal arrays; the cohesive state (facet stresses, laws,
    /// inserter) is element- or facet-indexed and does not change.
    /// `distributed`: the hook a distributed mesh needs (the removal hook
    /// refuses a mesh with a facet mesh: distributed cohesive removals are
    /// not supported).
    ///
    /// # Errors
    /// The mutation's error (rolled back on every rank before the commit,
    /// poisoned after it; `docs/ARCHITECTURE.md`).
    pub fn mutate_mesh(
        &mut self,
        change: impl FnOnce(&mut akantu_mesh::MeshTransaction<'_>) -> Result<()>,
        distributed: Option<&mut dyn akantu_mesh::DistributedHook>,
    ) -> Result<akantu_mesh::CommittedMutation> {
        let mesh_h = self.smm.physics.base.mesh.clone();
        let (token, parts) = self.smm.split_for_mutation();
        let parts = RefCell::new(parts);
        let mut handler = CohesiveEventHandler {
            smm: &parts,
            facet_stress: &mut self.ext.facet_stress,
            is_extrinsic: self.ext.is_extrinsic,
            dim: self.ext.dim,
            ghost_laws: None,
            move_external_force: !self.ext.compat.doubled_node_keeps_external_force,
        };
        mesh_h.mutate(&mut [(token, &mut handler)], change, distributed)
    }

    /// Use a selector chain whose first entry is `selector`, followed by the
    /// default cohesive selector and the law-index selector (kept by
    /// `init_full`).
    pub fn set_material_selector(&mut self, selector: Box<dyn ConstitutiveLawSelector>) {
        let def = self.default_cohesive_selector();
        self.set_material_selector_chain(SelectorChain {
            selectors: vec![selector, Box::new(def), Box::new(LawIndexSelector)],
            fallback_value: 0,
        });
    }
}

/// The law decisions staged for the new cohesive elements of an insertion
/// transaction.
type StagedLaws = Arc<Mutex<HashMap<Element, usize>>>;

/// First selector of the model's chain while an insertion transaction runs:
/// the staged (validated) law of each new cohesive element, so the
/// post-commit dispatch assigns exactly the planned laws; `None` (the rest
/// of the chain) for every other element.
#[derive(Debug)]
struct StagedSelector(StagedLaws);

impl ConstitutiveLawSelector for StagedSelector {
    fn select(&self, _: &SelectorContext<'_>, element: Element) -> Result<Option<usize>> {
        let staged = self
            .0
            .lock()
            .map_err(|_| Error::invalid("staged cohesive law selection poisoned"))?;
        Ok(staged.get(&element).copied())
    }
}

/// Choose the law of every new cohesive element as the solid model's
/// `ElementsAdded` dispatch does (`fit_handler_maps`, `select_laws`): the
/// law-index map grown to the staged mesh (new entries `-1`), the selector
/// chain asked for the event's elements of the mesh dimension (not ghost,
/// type order, then event order), each decision stored in the map before
/// the next selection — and in `staged`. Returns `(element, law)` in that
/// order.
///
/// Visibility: staging runs inside the topology closure
/// of the transaction, after the topology is complete but **before** the
/// mesh's local dependents are rebuilt (`update_local_dependents`): a
/// selector sees the new elements, their connectivities, the facet
/// adjacency and the element groups' element lists, but element-group
/// **node** lists still hold the pre-insertion state. The built-in
/// selectors (default cohesive, mesh-data, element-data, rules) do not read
/// node lists. Only not-ghost elements are staged; once insertion is
/// distributed the laws of ghost cohesive elements come from
/// the owner's law-id exchange, never from a local selector.
fn stage_cohesive_laws(
    materials: &mut ConstitutiveLawsHandler<dyn Material>,
    mesh: &Mesh,
    new_elements: &[Element],
    staged: &StagedLaws,
) -> Result<Vec<(Element, usize)>> {
    if materials.nb_laws() == 0 {
        // the dispatch assigns no law either
        return Ok(Vec::new());
    }
    let dim = mesh.spatial_dimension();
    let mut filter = ElementTypeMapArray::<Idx>::new("staged_element_filter");
    for e in new_elements {
        if e.ty().spatial_dimension() != dim {
            continue;
        }
        if !filter.exists(e.ty(), e.ghost_type()) {
            filter.alloc(0, 1, e.ty(), e.ghost_type(), 0)?;
        }
        filter
            .get_mut(e.ty(), e.ghost_type())?
            .try_push_tuple(&[e.index()])?;
    }
    let blocks: Vec<(ElementType, GhostType)> =
        filter.as_map().iter_all().map(|(t, g, _)| (t, g)).collect();
    for (ty, g) in blocks {
        let n = mesh.nb_element(ty, g);
        let map = materials.law_index_map_mut();
        if !map.exists(ty, g) {
            map.alloc(0, 1, ty, g, -1)?;
        }
        map.get_mut(ty, g)?.try_resize(n, -1)?;
    }
    let elements: Vec<Element> = filter
        .as_map()
        .iter(GhostType::NotGhost)
        .filter(|(ty, _)| ty.spatial_dimension() == dim)
        .flat_map(|(ty, a)| {
            a.as_slice()
                .iter()
                .map(move |&i| Element::new(ty, i, GhostType::NotGhost))
        })
        .collect();
    let mut planned = Vec::with_capacity(elements.len());
    for e in elements {
        let law = {
            let by_name = |n: &str| materials.law_index_of(n).ok();
            let ctx = SelectorContext {
                mesh,
                law_index: materials.law_index_map(),
                law_by_name: &by_name,
            };
            materials.selector.select(&ctx, e)?
        };
        if law >= materials.nb_laws() {
            return Err(Error::OutOfRange {
                what: "constitutive law (selector result for a new cohesive element)",
                index: law,
                len: materials.nb_laws(),
            });
        }
        materials
            .law_index_map_mut()
            .set_element(e, &[akantu_core::try_idx(law)?])?;
        staged
            .lock()
            .map_err(|_| Error::invalid("staged cohesive law selection poisoned"))?
            .insert(e, law);
        planned.push((e, law));
    }
    Ok(planned)
}

/// `'name' (type, n element(s))` of every law with a non-zero count.
fn law_counts(
    materials: &ConstitutiveLawsHandler<dyn Material>,
    counts: &BTreeMap<usize, usize>,
) -> String {
    counts
        .iter()
        .filter(|&(_, &n)| n > 0)
        .map(|(&l, n)| match materials.law(l) {
            Ok(law) => format!("'{}' ({}, {n} element(s))", law.name(), law.law_type()),
            Err(_) => format!("law {l} ({n} element(s))"),
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// Validation of the planned law assignment `planned` of an insertion (see
/// [`SolidMechanicsModelCohesive::insert_elements`]): every law cohesive
/// (UD-246); `zero_strength` (UD-224, insertions outside the check, empty
/// with the compat switch) receives nothing; after the laws' checks
/// (`prepared`: law index → prepared facets in order) every law receives
/// exactly the elements of its prepared facets, per cohesive type in the
/// same order (UD-246).
fn validate_planned_laws(
    materials: &ConstitutiveLawsHandler<dyn Material>,
    planned: &[(Element, usize)],
    inserted: &InsertionResult,
    prepared: Option<&[(usize, Vec<Element>)]>,
    zero_strength: &[(usize, String, String)],
) -> Result<()> {
    let mut not_cohesive: BTreeMap<usize, usize> = BTreeMap::new();
    for &(_, l) in planned {
        if !is_cohesive(materials.law(l)?) {
            *not_cohesive.entry(l).or_default() += 1;
        }
    }
    if !not_cohesive.is_empty() {
        return Err(Error::invalid(format!(
            "cohesive insertion: the material selection assigns new cohesive elements to the \
             non-cohesive law(s) {}; refused before any change (UPSTREAM_DEFECTS UD-246)",
            law_counts(materials, &not_cohesive)
        )));
    }
    let mut refused: BTreeMap<usize, usize> = BTreeMap::new();
    for &(_, l) in planned {
        if zero_strength.iter().any(|z| z.0 == l) {
            *refused.entry(l).or_default() += 1;
        }
    }
    if !refused.is_empty() {
        return Err(Error::invalid(format!(
            "cohesive insertion outside the extrinsic insertion check (intrinsic insertion or a \
             manual insert_elements) into the cohesive law(s) {}: these laws have no elastic \
             branch and their strength sigma_c_eff is set only by the extrinsic insertion check, \
             so the inserted interface would carry no traction (upstream behaviour, \
             UPSTREAM_DEFECTS UD-224). Use cohesive_bilinear (elastic branch up to delta_0) for \
             intrinsic insertion, or extrinsic insertion (init_full(.., true) and \
             check_cohesive_stress); CohesiveUpstreamCompat::intrinsic_linear_zero_strength \
             reproduces upstream",
            law_counts(materials, &refused)
        )));
    }
    let Some(prepared) = prepared else {
        return Ok(());
    };
    // original facet of every new element (one element per doubled facet)
    let facet_of: HashMap<Element, Element> = inserted
        .new_elements
        .iter()
        .zip(&inserted.doubled_facets)
        .map(|(&e, &(old, _))| (e, old))
        .collect();
    // law → cohesive type → facets, received (dispatch order) and prepared
    type PerType = BTreeMap<ElementType, Vec<Element>>;
    let mut received: BTreeMap<usize, PerType> = BTreeMap::new();
    for &(e, l) in planned {
        let facet = facet_of.get(&e).copied().ok_or_else(|| {
            Error::Mesh(format!("new cohesive element {e} without doubled facet"))
        })?;
        received
            .entry(l)
            .or_default()
            .entry(e.ty())
            .or_default()
            .push(facet);
    }
    let mut preparers: HashMap<Element, usize> = HashMap::new();
    let mut expected: BTreeMap<usize, PerType> = BTreeMap::new();
    for (l, facets) in prepared {
        for &f in facets {
            let ct = f
                .ty()
                .cohesive_type()
                .ok_or(Error::UnsupportedElementType {
                    ty: f.ty(),
                    context: "insertion check (no cohesive type)",
                })?;
            expected
                .entry(*l)
                .or_default()
                .entry(ct)
                .or_default()
                .push(f);
            preparers.insert(f, *l);
        }
    }
    let laws: std::collections::BTreeSet<usize> =
        received.keys().chain(expected.keys()).copied().collect();
    let empty = PerType::new();
    let mismatched: Vec<usize> = laws
        .into_iter()
        .filter(|l| received.get(l).unwrap_or(&empty) != expected.get(l).unwrap_or(&empty))
        .collect();
    if mismatched.is_empty() {
        return Ok(());
    }
    // the elements whose law did not prepare them, by receiving law, and
    // the laws that prepared them
    let mut foreign: BTreeMap<usize, usize> = BTreeMap::new();
    let mut by: BTreeMap<usize, usize> = BTreeMap::new();
    for &(e, l) in planned {
        let p = facet_of.get(&e).and_then(|f| preparers.get(f)).copied();
        if p != Some(l) {
            *foreign.entry(l).or_default() += 1;
            if let Some(p) = p {
                *by.entry(p).or_default() += 1;
            }
        }
    }
    let names = |ls: &[usize]| {
        ls.iter()
            .map(|&l| {
                materials
                    .law(l)
                    .map_or(format!("law {l}"), |x| format!("'{}'", x.name()))
            })
            .collect::<Vec<_>>()
            .join(", ")
    };
    Err(Error::invalid(format!(
        "cohesive insertion after the extrinsic insertion check: the material selection does not \
         give the new cohesive elements the law whose insertion check prepared their parameters \
         (sigma_c_eff, delta_c_eff, insertion stress) — elements without prepared parameters: {}; \
         prepared by: {}; laws affected: {}. A selector must give a cohesive element the law of \
         its facet (as the default cohesive selection does); refused before any change \
         (UPSTREAM_DEFECTS UD-246)",
        if foreign.is_empty() {
            "none".to_owned()
        } else {
            law_counts(materials, &foreign)
        },
        if by.is_empty() {
            "none".to_owned()
        } else {
            law_counts(materials, &by)
        },
        names(&mismatched)
    )))
}

/// Cohesive types (`(type, ghost)`) of the facets of the bulk types that
/// have no connectivity block yet (upstream `initModel` adds them empty).
fn cohesive_blocks_missing(m: &Mesh) -> Vec<(ElementType, GhostType)> {
    let dim = m.spatial_dimension();
    let mut out = Vec::new();
    for ghost in GhostType::ALL {
        for ty in m.element_types(TypeFilter {
            spatial_dimension: Some(dim),
            ghost_type: ghost,
            kind: Some(ElementKind::Regular),
        }) {
            if m.nb_element(ty, ghost) == 0 {
                continue;
            }
            if let Some(c) = ty.facet_type(0).and_then(ElementType::cohesive_type)
                && !m.connectivities().exists(c, ghost)
                && !out.contains(&(c, ghost))
            {
                out.push((c, ghost));
            }
        }
    }
    out
}

#[cfg(test)]
mod perf_tests {
    use super::*;
    use akantu_core::enums::AnalysisMethod;
    use akantu_mesh::MeshAccessor;
    use akantu_parser::Parser;

    #[test]
    fn sorting_by_destination_keeps_the_last_writer() {
        // (source, destination) in loop order; destination 9 written twice
        let pairs = vec![(0, 18), (9, 9), (18, 0), (27, 9), (36, 27)];
        // the table (unit 9), the sort (a unit that does not divide every
        // destination) and the sort of a sparse range give the same pairs
        for unit in [9, 1, 7, 0] {
            let mut p = pairs.clone();
            sort_by_destination(&mut p, unit);
            assert_eq!(p, vec![(18, 0), (27, 9), (0, 18), (36, 27)], "unit {unit}");
        }
        let mut sparse = vec![(1, 9_000_000), (2, 0), (3, 9_000_000)];
        sort_by_destination(&mut sparse, 9);
        assert_eq!(sparse, vec![(2, 0), (3, 9_000_000)]);
    }

    const DAT: &str = "model solid_mechanics_model_cohesive [
  material elastic [
    name = body
    rho = 1e3
    E   = 1e9
    nu  = 0.2
  ]
  material cohesive_linear [
    name = insertion
    beta = 1
    G_c = 10
    sigma_c = 1e6
    delta_c = 1e-4
  ]
]
";

    /// An extrinsic model on a strip of three unit quadrangles along x (two
    /// interior facets), stretched along x well beyond `sigma_c`.
    fn stretched_strip() -> (SolidMechanicsModelCohesive, SharedMesh) {
        let mut m = Mesh::new(2, "strip").unwrap();
        {
            let mut acc = MeshAccessor::new(&mut m);
            for i in 0..=3 {
                acc.push_node(&[Real::from(i), 0.]).unwrap();
                acc.push_node(&[Real::from(i), 1.]).unwrap();
            }
            let mut c = acc.connectivity_mut(ElementType::Quadrangle4, GhostType::NotGhost);
            for k in [0, 2, 4] {
                c.try_push_tuple(&[k, k + 2, k + 3, k + 1]).unwrap();
            }
        }
        let mesh = SharedMesh::new(m);
        let parser = Parser::parse_str(DAT, "material.dat").unwrap();
        let mut model =
            SolidMechanicsModelCohesive::with_parser(mesh.clone(), None, DEFAULT_ID, parser)
                .unwrap();
        model
            .init_full(&ModelOptions::new(AnalysisMethod::ExplicitLumpedMass), true)
            .unwrap();
        let nodes = mesh.read().unwrap().nodes().as_slice().to_vec();
        {
            let mut u_g = model.smm.displacement_mut().unwrap();
            let u = u_g.as_mut_slice();
            for (x, ui) in nodes.chunks(2).zip(u.chunks_mut(2)) {
                ui[0] = 0.1 * x[0]; // σxx ≈ 1e8 ≫ σc
            }
        }
        model.assemble_internal_forces().unwrap();
        (model, mesh)
    }

    /// Index of the (only) bulk law.
    fn bulk_law(model: &SolidMechanicsModelCohesive) -> usize {
        let laws = model.smm.physics.materials.laws();
        let bulk: Vec<usize> = (0..laws.len())
            .filter(|&i| !is_cohesive(laws[i].as_ref()))
            .collect();
        assert_eq!(bulk.len(), 1);
        bulk[0]
    }

    /// A plan built now from the current mesh and `filter`.
    fn fresh_plan(
        model: &SolidMechanicsModelCohesive,
        filter: &ElementTypeMapArray<Idx>,
    ) -> StressCopyPlan {
        let mesh = model.smm.physics.base.mesh.clone();
        let m = mesh.read().unwrap();
        let fm = m.mesh_facets().unwrap();
        let types: Vec<ElementType> = filter
            .as_map()
            .iter_all()
            .filter(|(_, g, a)| *g == GhostType::NotGhost && !a.as_slice().is_empty())
            .map(|(t, _, _)| t)
            .collect();
        let dim = model.ext.dim;
        build_stress_plan(&m, fm, filter, &types, &model.ext.by_elem_buffer, dim * dim).unwrap()
    }

    /// The copy plan follows an insertion: after the mesh revision changed,
    /// the cached plan equals a plan built from scratch, and differs from
    /// the plan before the insertion (the doubled facet moved one element's
    /// side). A plan never rebuilt after a revision change (a mutant that
    /// survives every oracle replay) fails here.
    #[test]
    fn copy_plan_is_rebuilt_after_an_insertion() {
        let (mut model, mesh) = stretched_strip();
        let i = bulk_law(&model);
        model.interpolate_stress().unwrap();
        let before = model.ext.stress_plans[&i].clone();
        assert_eq!(before.revision, mesh.read().unwrap().revision());
        // interpolates with the cached plan (same revision), then inserts
        assert_eq!(
            model.check_cohesive_stress().unwrap(),
            2,
            "both interior facets open"
        );
        assert_ne!(before.revision, mesh.read().unwrap().revision());
        model.interpolate_stress().unwrap();
        let cached = model.ext.stress_plans[&i].clone();
        let filter = model
            .smm
            .physics
            .materials
            .law(i)
            .unwrap()
            .base()
            .element_filter
            .clone();
        let fresh = fresh_plan(&model, &filter);
        assert_eq!(
            cached.blocks, fresh.blocks,
            "cached plan after the insertion"
        );
        assert_eq!(cached.revision, mesh.read().unwrap().revision());
        assert_ne!(
            before.blocks, fresh.blocks,
            "the insertion changed the plan"
        );
    }

    /// An extrinsic model on a fixture mesh of `test_cohesive` (the DAT
    /// above: every interior facet checked).
    fn fixture_model(file: &str, dim: usize) -> (SolidMechanicsModelCohesive, SharedMesh) {
        let path = akantu_validation::fixtures_root().join(format!(
            "meshes/gmsh-4.13.1/test/test_model/test_solid_mechanics_model/test_cohesive/{file}"
        ));
        let mut mesh = Mesh::new(dim, "mesh").unwrap();
        akantu_io::read_mesh(&mut mesh, &path, akantu_core::enums::MeshIoType::Gmsh).unwrap();
        let shared = SharedMesh::new(mesh);
        let parser = Parser::parse_str(DAT, "material.dat").unwrap();
        let mut model =
            SolidMechanicsModelCohesive::with_parser(shared.clone(), None, DEFAULT_ID, parser)
                .unwrap();
        model
            .init_full(&ModelOptions::new(AnalysisMethod::ExplicitLumpedMass), true)
            .unwrap();
        (model, shared)
    }

    /// Bits of every `facet_stress` value after `interpolate_stress` with
    /// every value set to a sentinel before; `general` forces the general
    /// path (interpolation into the by-element buffer, then the copy),
    /// otherwise the fused path must be taken.
    fn facet_stress_after(model: &mut SolidMechanicsModelCohesive, general: bool) -> Vec<u64> {
        model.ext.facet_stress.fill(-7.25);
        let i = bulk_law(model);
        if let Some(plan) = model.ext.stress_plans.get_mut(&i) {
            plan.fused = general.then(|| vec![None; plan.blocks.len()]);
        }
        model.interpolate_stress().unwrap();
        if !general {
            let fused = model.ext.stress_plans[&i].fused.as_ref().unwrap();
            assert!(!fused.is_empty() && fused.iter().all(Option::is_some));
        }
        model
            .ext
            .facet_stress
            .as_map()
            .iter_all()
            .flat_map(|(_, _, a)| a.as_slice().iter().map(|x| x.to_bits()))
            .collect()
    }

    /// The fused interpolation of the facet stresses (linear tetrahedra and
    /// triangles) writes bitwise the values of the general path, before and
    /// after insertions (plan and fused pairs rebuilt).
    #[test]
    fn fused_interpolation_is_bitwise_the_general_path() {
        for (file, dim) in [
            ("_cohesive_3d_6_tetrahedron_4.msh", 3),
            ("_cohesive_2d_4_triangle_3.msh", 2),
        ] {
            let (mut model, mesh) = fixture_model(file, dim);
            // first call: the general path allocates the by-element buffer
            model.interpolate_stress().unwrap();
            for round in 0..4 {
                let nodes = mesh.read().unwrap().nodes().as_slice().to_vec();
                {
                    // a non-uniform field: stresses differ between elements
                    let mut u_g = model.smm.displacement_mut().unwrap();
                    let u = u_g.as_mut_slice();
                    let scale = 2e-3 * Real::from(round + 1);
                    for (k, (x, ui)) in nodes.chunks(dim).zip(u.chunks_mut(dim)).enumerate() {
                        for d in 0..dim {
                            let wobble = Real::from(u8::try_from(k % 7).unwrap()) * 0.05;
                            ui[d] = scale * (x[d] * (1.0 + x[(d + 1) % dim]) + wobble);
                        }
                    }
                }
                model.assemble_internal_forces().unwrap();
                let general = facet_stress_after(&mut model, true);
                let fused = facet_stress_after(&mut model, false);
                assert_eq!(general, fused, "{file}, round {round}");
                let sentinel = (-7.25_f64).to_bits();
                assert!(
                    fused.iter().any(|&b| b != sentinel),
                    "{file}: values written"
                );
                model.check_cohesive_stress().unwrap();
            }
            assert!(
                model.ext.stats.inserted > 0,
                "{file}: insertions between the rounds"
            );
        }
    }

    /// The copy plan follows a change of the law's filter without a mesh
    /// revision (an element dropped): rebuilt and equal to a fresh plan; an
    /// unchanged revision and filter keep the cached plan.
    #[test]
    fn copy_plan_follows_the_law_filter_and_is_kept_otherwise() {
        let (mut model, mesh) = stretched_strip();
        model.interpolate_stress().unwrap();
        let i = bulk_law(&model);
        let mut plan = model.ext.stress_plans.remove(&i).unwrap();
        let m = mesh.read().unwrap();
        let fm = m.mesh_facets().unwrap();
        let ss = model.ext.dim * model.ext.dim;
        let mut filter = model
            .smm
            .physics
            .materials
            .law(i)
            .unwrap()
            .base()
            .element_filter
            .clone();
        let types = vec![ElementType::Quadrangle4];
        let by = &model.ext.by_elem_buffer;
        // same revision, same filter: kept
        assert!(!update_stress_plan(&mut plan, &m, fm, &filter, &types, by, ss).unwrap());
        // the last element leaves the law: rebuilt
        filter
            .get_mut(ElementType::Quadrangle4, GhostType::NotGhost)
            .unwrap()
            .truncate(2);
        assert!(update_stress_plan(&mut plan, &m, fm, &filter, &types, by, ss).unwrap());
        drop(m);
        let fresh = fresh_plan(&model, &filter);
        assert_eq!(plan.blocks, fresh.blocks);
        let pairs: usize = plan.blocks.iter().map(|b| b.pairs.len()).sum();
        assert_eq!(pairs, 2 * 4, "two quadrangles, four facets each");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use akantu_core::enums::{AnalysisMethod, MeshIoType};
    use akantu_mesh::{CommittedMutation, MeshPart, MeshTransaction};

    const DAT: &str = "model solid_mechanics_model_cohesive [
  cohesive_inserter [
    cohesive_surfaces = [insertion]
  ]
  material elastic [
    name = body
    rho = 1e3
    E   = 1e9
    nu  = 0.2
  ]
  material cohesive_linear [
    name = insertion
    beta = 1
    G_c = 10
    sigma_c = 1e6
  ]
]
";

    fn extrinsic_model() -> SolidMechanicsModelCohesive {
        let path = akantu_validation::fixtures_root().join(
            "meshes/gmsh-4.13.1/test/test_model/test_solid_mechanics_model/test_cohesive/\
             _cohesive_2d_4_triangle_3.msh",
        );
        let mut mesh = Mesh::new(2, "mesh").unwrap();
        akantu_io::read_mesh(&mut mesh, &path, MeshIoType::Gmsh).unwrap();
        let parser = Parser::parse_str(DAT, "material.dat").unwrap();
        let mut model = SolidMechanicsModelCohesive::with_parser(
            SharedMesh::new(mesh),
            None,
            "solid_mechanics_model_cohesive",
            parser,
        )
        .unwrap();
        model
            .init_full(&ModelOptions::new(AnalysisMethod::ExplicitLumpedMass), true)
            .unwrap();
        model
    }

    /// A mutation coordinated by the cohesive model's event handler (as
    /// [`SolidMechanicsModelCohesive::insert_flagged_elements`] does), with
    /// an arbitrary change.
    fn mutate_through_the_cohesive_handler(
        model: &mut SolidMechanicsModelCohesive,
        change: impl FnOnce(&mut MeshTransaction<'_>) -> Result<()>,
    ) -> Result<CommittedMutation> {
        let mesh_h = model.smm.physics.base.mesh.clone();
        let (token, parts) = model.smm.split_for_mutation();
        let parts = RefCell::new(parts);
        let mut handler = CohesiveEventHandler {
            smm: &parts,
            facet_stress: &mut model.ext.facet_stress,
            is_extrinsic: model.ext.is_extrinsic,
            dim: model.ext.dim,
            ghost_laws: None,
            move_external_force: !model.ext.compat.doubled_node_keeps_external_force,
        };
        mesh_h.mutate(&mut [(token, &mut handler)], change, None)
    }

    /// The cohesive model's handler forwards the dispatch
    /// methods to the solid model's parts and must forward the capability
    /// check too. Node removal is followed since SMM-015; a node appended
    /// and removed again in one transaction is still refused by the DOF
    /// manager's check (the addition names a node that does not exist in
    /// the final mesh) **before** the commit — recoverable, the mesh and the
    /// model untouched, the domain usable — instead of being dispatched and
    /// poisoning the domain (`Fatal { kind: Agreed }`, which is what the
    /// handler without the forwarded check returned). A renumbering then
    /// commits through the same handler.
    #[test]
    fn fu01_node_events_through_the_cohesive_handler_are_checked_before_commit() {
        let mut model = extrinsic_model();
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
        let r = mutate_through_the_cohesive_handler(&mut model, |tx| {
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
            let mut numbering: Vec<Idx> =
                (0..nb_nodes).map(|i| Idx::try_from(i).unwrap()).collect();
            numbering.push(-1);
            tx.remove_nodes(MeshPart::Main, &numbering)
        });
        let Err(e) = &r else {
            panic!("expected the DOF manager's refusal, got {r:?}");
        };
        assert!(matches!(e, Error::Unsupported(_)), "{e:?}");
        assert!(e.fatal_kind().is_none(), "recoverable: {e:?}");
        assert!(format!("{e}").contains("record the removal first"), "{e}");
        // nothing committed, nothing dispatched
        {
            let g = mesh.read().unwrap();
            assert_eq!(g.nodes().as_slice(), &nodes_before[..]);
        }
        assert_eq!(mesh.revision().unwrap(), revision);
        assert_eq!(model.smm.displacement().unwrap().as_slice(), &u_before[..]);
        // the domain stays usable through the same handler, and a
        // renumbering (SMM-015) commits through it
        let again = mutate_through_the_cohesive_handler(&mut model, |_| Ok(()));
        assert!(again.is_ok(), "{again:?}");
        let purified = model.mutate_mesh(|tx| tx.purify(MeshPart::Main), None);
        assert!(purified.is_ok(), "{purified:?}");
    }
}
